//! What the Viewer draws, as plain data for the GPU: meshes with the
//! placements and colours of their copies, and lines. Built from what the
//! graph puts out, the same choices as under Bevy (`update_generated_meshes`).

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};

use crate::types::{EvalResult, MeshData, NamedMesh, Purpose};
use crate::viewport::display::{wire_edges, DisplayMode};

/// A mesh as the GPU draws it: position and normal per vertex, triangles.
pub struct Geometry {
    /// x, y, z, nx, ny, nz per vertex.
    pub vertices: Vec<f32>,
    pub indices:  Vec<u32>,
}

impl Geometry {
    /// A mesh's polygon edges, drawn as lines with the mesh pipeline's
    /// vertex layout (the normals unused).
    fn edges_of(md: &MeshData) -> Geometry {
        let mut vertices = Vec::with_capacity(md.vertices.len() * 6);
        for p in &md.vertices { vertices.extend_from_slice(p); vertices.extend_from_slice(&[0.0, 1.0, 0.0]); }
        Geometry { vertices, indices: wire_edges(md) }
    }

    fn of(md: &MeshData) -> Geometry {
        // Hard edges where faces meet sharply, as under Bevy.
        let (pos, nrm, indices) = md.render_buffers();
        let mut vertices = Vec::with_capacity(pos.len() * 6);
        for (p, n) in pos.iter().zip(&nrm) { vertices.extend_from_slice(p); vertices.extend_from_slice(n); }
        Geometry { vertices, indices }
    }
}

/// One mesh and its copies. `instances`: per copy, a column-major matrix
/// (16 floats), a linear colour (4) and flags (4: x = lit).
pub struct MeshDraw {
    pub geometry:  Arc<Geometry>,
    pub instances: Vec<f32>,
    /// Drawn see-through, after everything else.
    pub blend:     bool,
    /// The mesh's polygon edges, as lines.
    pub edges:     bool,
}

#[derive(Default)]
pub struct Scene {
    pub meshes:     Vec<MeshDraw>,
    /// x, y, z and a linear colour (r, g, b, a) per vertex, two vertices a line.
    pub lines:      Vec<f32>,
    /// Changes whenever the scene does: the GPU uploads again only then.
    pub generation: u64,
}

/// Geometry made from a `MeshData`, kept while the mesh lives, so a scene
/// rebuilt for a moved playhead or a reopened prim does not rebuild it.
#[derive(Default)]
pub struct GeometryCache(HashMap<(usize, bool), (Arc<MeshData>, Arc<Geometry>)>);

impl GeometryCache {
    /// The surfaces of a mesh, or with `edges` its polygon edges.
    fn get(&mut self, md: &Arc<MeshData>, edges: bool, used: &mut Vec<(usize, bool)>) -> Arc<Geometry> {
        let key = (Arc::as_ptr(md) as usize, edges);
        used.push(key);
        match self.0.get(&key) {
            Some((m, g)) if Arc::ptr_eq(m, md) => g.clone(),
            _ => {
                let g = Arc::new(if edges { Geometry::edges_of(md) } else { Geometry::of(md) });
                self.0.insert(key, (md.clone(), g.clone()));
                g
            }
        }
    }
    fn keep(&mut self, used: &[(usize, bool)]) {
        let used: std::collections::HashSet<_> = used.iter().copied().collect();
        self.0.retain(|k, _| used.contains(k));
    }
}

/// An 8-bit sRGB colour as linear light.
pub fn linear(c: [u8; 3]) -> [f32; 3] {
    let f = |v: u8| { let v = v as f32 / 255.0; if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) } };
    [f(c[0]), f(c[1]), f(c[2])]
}

/// A 0..1 sRGB colour as linear light.
pub fn linear_f(c: [f32; 3]) -> [f32; 3] {
    let f = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    [f(c[0]), f(c[1]), f(c[2])]
}

fn ink(c: egui::Color32) -> [f32; 3] { linear([c.r(), c.g(), c.b()]) }

fn push_instance(out: &mut Vec<f32>, m: &Mat4, colour: [f32; 4], lit: bool) {
    out.extend_from_slice(&m.to_cols_array());
    out.extend_from_slice(&colour);
    out.extend_from_slice(&[if lit { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0]);
}

fn push_lines(out: &mut Vec<f32>, points: &[[f32; 3]], pairs: &[u32], at: &Mat4, colour: [f32; 4]) {
    for &i in pairs {
        let p = at.transform_point3(Vec3::from_array(points[i as usize]));
        out.extend_from_slice(&[p.x, p.y, p.z]);
        out.extend_from_slice(&colour);
    }
}

/// What the Viewer reads to build its scene.
pub struct Inputs<'a> {
    pub graph:     &'a crate::node_graph::NodeGraphState,
    pub subnets:   &'a crate::ice::SubnetStore,
    pub hierarchy: &'a crate::types::SceneHierarchy,
    pub time:      f64,
}

/// Curves and points of a primitive are drawn at this many of its copies
/// at most (they are lines made on the CPU, not instanced).
const STRAND_COPIES: usize = 64;

pub fn build(input: &Inputs, cache: &mut GeometryCache, generation: u64) -> Scene {
    let mode = crate::viewport::display::mode();
    let bg = crate::theme::viewport_bg();
    let mut s = Scene { generation, ..Default::default() };
    let mut used = vec![];

    // The ground grid, faint over the background, and the axes.
    let grid = {
        let (g, b) = (linear([89, 89, 89]), linear(bg));
        [b[0] * 0.4 + g[0] * 0.6, b[1] * 0.4 + g[1] * 0.6, b[2] * 0.4 + g[2] * 0.6, 1.0]
    };
    for i in 0..=20 {
        let p = i as f32 - 10.0;
        for (a, b) in [([p, 0.0, -10.0], [p, 0.0, 10.0]), ([-10.0, 0.0, p], [10.0, 0.0, p])] {
            s.lines.extend_from_slice(&a); s.lines.extend_from_slice(&grid);
            s.lines.extend_from_slice(&b); s.lines.extend_from_slice(&grid);
        }
    }
    for (dir, c) in [([1.0, 0.0, 0.0], [1.0, 0.0, 0.0, 1.0]), ([0.0, 1.0, 0.0], [0.0, 1.0, 0.0, 1.0]), ([0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0])] {
        s.lines.extend_from_slice(&[0.0, 0.0, 0.0]); s.lines.extend_from_slice(&c);
        s.lines.extend_from_slice(&dir); s.lines.extend_from_slice(&c);
    }

    let graph = input.graph;
    let subnets = input.subnets;
    let eval_subnet = |sid: crate::types::SubnetId, mesh: &crate::core::geo::Geo, template: Option<&crate::core::geo::Geo>| {
        subnets.get(sid).map(|sg| sg.evaluate(mesh, template)).unwrap_or_else(|| mesh.clone())
    };

    // Meshes bound to a skeleton, posed at the playhead.
    let skin_colour = { let c = linear_f([0.55, 0.62, 0.7]); [c[0], c[1], c[2], 1.0] };
    for clip in graph.display_clips() {
        let Some(skin) = &clip.skin else { continue };
        let (pos, nrm) = skin.deformed(&clip.world_pose(clip.index_at(input.time)));
        let mut md = MeshData {
            vertices: pos.iter().map(|p| p.to_array()).collect(),
            normals:  nrm.iter().map(|n| n.to_array()).collect(),
            ..Default::default()
        };
        for f in &skin.faces {
            md.indices.extend([f[0], f[1], f[2]]);
            if !crate::core::anim::SkinMesh::is_tri(f) { md.indices.extend([f[0], f[2], f[3]]); }
        }
        let mut inst = vec![];
        push_instance(&mut inst, &Mat4::IDENTITY, skin_colour, true);
        s.meshes.push(MeshDraw { geometry: Arc::new(Geometry::of(&md)), instances: inst, blend: false, edges: false });
    }

    // A selected Edit Poly node shows the mesh being edited, the selected
    // polygons tinted.
    let stage = crate::modelling::stage(graph, &eval_subnet);
    if let Some(hl) = stage.as_ref().and_then(crate::modelling::highlight_mesh) {
        let mut inst = vec![];
        push_instance(&mut inst, &Mat4::IDENTITY, [1.0, 0.05, 0.02, 0.45], false);
        s.meshes.push(MeshDraw { geometry: Arc::new(Geometry::of(&hl)), instances: inst, blend: true, edges: false });
    }

    let packed = if stage.is_none() { graph.evaluate_for_viewport_packed(&eval_subnet) } else { None };
    let surface_grey = { let c = linear_f([0.6, 0.6, 0.6]); [c[0], c[1], c[2], 1.0] };
    let wire_ink = { let c = ink(crate::theme::c(225, 225, 225)); [c[0], c[1], c[2], 1.0] };
    let bg_lin = { let c = linear(bg); [c[0], c[1], c[2], 1.0] };

    match packed {
        Some(EvalResult::Named(prims)) => {
            // Purposes the Scene Explorer's toggles leave out are not drawn.
            let prims: Vec<NamedMesh> = prims.into_iter()
                .filter(|p| p.purpose == Purpose::Default || input.hierarchy.draws_purpose(p.purpose))
                .collect();
            // Closed prims of a composed stage: the box around what is below.
            let prims = if prims.iter().any(|p| p.stage.is_some()) {
                let (shown, boxes) = crate::viewport::bounds::split(prims, input.hierarchy);
                let (points, pairs) = crate::viewport::bounds::box_lines(&boxes);
                let c = ink(crate::theme::c(200, 200, 200));
                push_lines(&mut s.lines, &points, &pairs, &Mat4::IDENTITY, [c[0], c[1], c[2], 1.0]);
                shown
            } else { prims };
            // Every primitive drawn at each of its placements; copies of
            // one mesh share it.
            let mut by_mesh: Vec<(Arc<MeshData>, bool, Vec<f32>)> = vec![];
            for p in &prims {
                let colour = match &p.look {
                    Some(l) if mode == DisplayMode::Textured => { let c = linear_f(l.color); [c[0], c[1], c[2], 1.0] }
                    _ => surface_grey,
                };
                let (colour, lit) = match mode {
                    DisplayMode::HiddenLine => (bg_lin, false),
                    _ => (colour, true),
                };
                let identity = [Mat4::IDENTITY];
                let places: &[Mat4] = if p.place.is_placed() { p.place.matrices() } else { &identity };
                // Curves and points, as lines.
                if crate::strands::has_strands(&p.mesh) {
                    let (points, pairs) = crate::strands::lines(&p.mesh);
                    let c = match &p.look {
                        Some(l) if mode == DisplayMode::Textured => { let c = linear_f(l.color); [c[0], c[1], c[2], 1.0] }
                        _ => { let c = ink(crate::theme::c(205, 205, 205)); [c[0], c[1], c[2], 1.0] }
                    };
                    for m in places.iter().take(STRAND_COPIES) { push_lines(&mut s.lines, &points, &pairs, m, c); }
                }
                if p.mesh.vertices.is_empty() { continue; }
                // Surfaces, and edges in the line modes: each mesh once,
                // drawn at every copy, edges included.
                let mut add = |edges: bool, colour: [f32; 4], lit: bool| {
                    let key = (Arc::as_ptr(&p.mesh) as usize, edges);
                    let at = match by_mesh.iter().position(|(m, e, _)| (Arc::as_ptr(m) as usize, *e) == key) {
                        Some(i) => i,
                        None => { by_mesh.push((p.mesh.clone(), edges, vec![])); by_mesh.len() - 1 }
                    };
                    for m in places { push_instance(&mut by_mesh[at].2, m, colour, lit); }
                };
                if matches!(mode, DisplayMode::HiddenLine | DisplayMode::Wireframe) { add(true, wire_ink, false); }
                if mode != DisplayMode::Wireframe { add(false, colour, lit); }
            }
            for (md, edges, instances) in by_mesh {
                let geometry = cache.get(&md, edges, &mut used);
                s.meshes.push(MeshDraw { geometry, instances, blend: false, edges });
            }
        }
        other => {
            // With a clip kept out of a collider, the collider is drawn too,
            // see-through so the character shows inside it.
            let collider = if stage.is_none() && matches!(other, Some(EvalResult::Anim(_))) { graph.display_collider() } else { None };
            let ghostly = collider.is_some();
            let shown: Option<Arc<MeshData>> = match (&stage, collider) {
                (Some(st), _) => Some(Arc::new(st.mesh.to_mesh())),
                (None, Some(c)) => Some(c),
                (None, None) => other.map(|r| Arc::new(r.into_mesh())),
            };
            if let Some(md) = shown {
                if crate::strands::has_strands(&md) {
                    let (points, pairs) = crate::strands::lines(&md);
                    let c = ink(crate::theme::c(205, 205, 205));
                    push_lines(&mut s.lines, &points, &pairs, &Mat4::IDENTITY, [c[0], c[1], c[2], 1.0]);
                }
                if !md.vertices.is_empty() {
                    if matches!(mode, DisplayMode::HiddenLine | DisplayMode::Wireframe) {
                        let mut inst = vec![];
                        push_instance(&mut inst, &Mat4::IDENTITY, wire_ink, false);
                        let geometry = cache.get(&md, true, &mut used);
                        s.meshes.push(MeshDraw { geometry, instances: inst, blend: false, edges: true });
                    }
                    let surface = match mode {
                        DisplayMode::Wireframe => None,
                        DisplayMode::HiddenLine => Some((bg_lin, false, false)),
                        _ if ghostly => Some(({ let c = linear_f([0.62, 0.66, 0.72]); [c[0], c[1], c[2], 0.32] }, true, true)),
                        _ => Some((surface_grey, true, false)),
                    };
                    if let Some((colour, lit, blend)) = surface {
                        let mut inst = vec![];
                        push_instance(&mut inst, &Mat4::IDENTITY, colour, lit);
                        let geometry = cache.get(&md, false, &mut used);
                        s.meshes.push(MeshDraw { geometry, instances: inst, blend, edges: false });
                    }
                }
            }
        }
    }
    cache.keep(&used);
    s
}
