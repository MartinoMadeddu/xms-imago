//! What the Viewer draws, as plain data for the GPU: meshes with the
//! placements and colours of their copies, and lines. Built from what the
//! graph puts out, the same choices as under Bevy (`update_generated_meshes`).

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};

use crate::types::{EvalResult, MeshData, Prim, Purpose};
use crate::viewport::display::{wire_edges, DisplayMode};

/// Floats per vertex: position, normal, texture coordinates.
pub const VERTEX_FLOATS: usize = 8;

/// A mesh as the GPU draws it: position, normal and UV per vertex, triangles
/// (or, for edges, pairs).
pub struct Geometry {
    pub vertices: Vec<f32>,
    pub indices:  Vec<u32>,
}

/// Which geometry a mesh gives.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    /// Surfaces with hard edges where faces meet sharply, as under Bevy.
    Surface,
    /// Surfaces with their UVs: a vertex per triangle corner, smooth normals.
    Textured,
    /// Polygon edges, as lines.
    Edges,
    /// Curves and points, as lines: one GPU mesh drawn at every copy, like
    /// the edges, so a groom is held once however often it is placed.
    Strands,
}

impl Geometry {
    fn of(md: &MeshData, kind: Kind) -> Geometry {
        let mut vertices = vec![];
        let mut push = |p: &[f32; 3], n: &[f32; 3], uv: [f32; 2]| {
            vertices.extend_from_slice(p); vertices.extend_from_slice(n); vertices.extend_from_slice(&uv);
        };
        match kind {
            Kind::Strands => {
                let (points, pairs) = crate::strands::lines(md);
                for p in &points { push(p, &[0.0, 1.0, 0.0], [0.0; 2]); }
                Geometry { vertices, indices: pairs }
            }
            Kind::Edges => {
                for p in &md.vertices { push(p, &[0.0, 1.0, 0.0], [0.0; 2]); }
                Geometry { vertices, indices: wire_edges(md) }
            }
            Kind::Textured if md.uvs.len() == md.indices.len() && !md.uvs.is_empty() => {
                let normals = if md.normals.len() == md.vertices.len() { md.normals.clone() } else {
                    let mut with = md.clone();
                    with.compute_normals();
                    with.normals
                };
                for (k, i) in md.indices.iter().enumerate() {
                    let i = *i as usize;
                    // USD puts v = 0 at the bottom of the image, the GPU at the top.
                    let uv = md.uvs[k];
                    push(&md.vertices[i], normals.get(i).unwrap_or(&[0.0, 1.0, 0.0]), [uv[0], 1.0 - uv[1]]);
                }
                Geometry { vertices, indices: (0..md.indices.len() as u32).collect() }
            }
            _ => {
                let (pos, nrm, indices) = md.render_buffers();
                for (p, n) in pos.iter().zip(&nrm) { push(p, n, [0.0; 2]); }
                Geometry { vertices, indices }
            }
        }
    }
}

/// A material's textures, decoded.
pub struct Material {
    pub colour:   Option<(std::path::PathBuf, Arc<crate::viewport::textures::Decoded>)>,
    pub emissive: Option<(std::path::PathBuf, Arc<crate::viewport::textures::Decoded>)>,
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
    /// Its textures: an index into `Scene::materials`.
    pub material:  Option<usize>,
}

#[derive(Default)]
pub struct Scene {
    pub meshes:     Vec<MeshDraw>,
    pub materials:  Vec<Material>,
    /// x, y, z and a linear colour (r, g, b, a) per vertex, two vertices a line.
    pub lines:      Vec<f32>,
    /// Lines drawn over everything: the skeletons of clips.
    pub top:        Vec<f32>,
    /// The cameras of the scene, to look through: path, placement, lens.
    pub cameras:    Vec<(String, Mat4, crate::core::camera::Camera)>,
    /// Changes whenever the scene does: the GPU uploads again only then.
    pub generation: u64,
}

/// Geometry made from a `MeshData`, kept while the mesh lives, so a scene
/// rebuilt for a moved playhead or a reopened prim does not rebuild it.
#[derive(Default)]
pub struct GeometryCache(HashMap<(usize, Kind), (Arc<MeshData>, Arc<Geometry>)>);

/// The mesh Edit Poly is working on, made drawable once and kept while it
/// stays the same: picking polygons on a big model does not build it, or
/// send it to the GPU, again.
fn stage_mesh(mesh: &Arc<crate::core::poly::PolyMesh>) -> Arc<MeshData> {
    use std::sync::{Mutex, Weak};
    static LAST: Mutex<Option<(Weak<crate::core::poly::PolyMesh>, Arc<MeshData>)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((w, md)) = last.as_ref() {
        if std::ptr::eq(w.as_ptr(), Arc::as_ptr(mesh)) && w.strong_count() > 0 { return md.clone(); }
    }
    let md = Arc::new(mesh.to_mesh());
    *last = Some((Arc::downgrade(mesh), md.clone()));
    md
}

impl GeometryCache {
    fn get(&mut self, md: &Arc<MeshData>, kind: Kind, used: &mut Vec<(usize, Kind)>) -> Arc<Geometry> {
        let key = (Arc::as_ptr(md) as usize, kind);
        used.push(key);
        match self.0.get(&key) {
            Some((m, g)) if Arc::ptr_eq(m, md) => g.clone(),
            _ => {
                let g = Arc::new(Geometry::of(md, kind));
                self.0.insert(key, (md.clone(), g.clone()));
                g
            }
        }
    }
    fn keep(&mut self, used: &[(usize, Kind)]) {
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
    push_instance_with(out, m, colour, [if lit { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0]);
}

/// `flags`: lit, colour map, emissive map, cut out (1 or 0 each).
fn push_instance_with(out: &mut Vec<f32>, m: &Mat4, colour: [f32; 4], flags: [f32; 4]) {
    out.extend_from_slice(&m.to_cols_array());
    out.extend_from_slice(&colour);
    out.extend_from_slice(&flags);
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
    pub subnets:   &'a crate::ice::IceGraphStore,
    pub hierarchy: &'a crate::types::SceneHierarchy,
    pub time:      f64,
}


pub fn build(input: &Inputs, cache: &mut GeometryCache, generation: u64) -> Scene {
    let _t = crate::timings::span("viewer: build the scene to draw");
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
    let eval_subnet = |sid: crate::types::IceGraphId, mesh: &crate::core::geo::Geo, template: &[crate::ice::Reference]| {
        subnets.get(sid).map(|sg| sg.evaluate(mesh, template)).unwrap_or_else(|| mesh.clone())
    };

    // Skeletons at the playhead, over everything: bones from joint to
    // joint, a small tripod at each root, a ball at each joint.
    for clip in graph.display_clips() { skeleton(&mut s.top, &clip, input.time); }

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
        s.meshes.push(MeshDraw { geometry: Arc::new(Geometry::of(&md, Kind::Surface)), instances: inst, blend: false, edges: false, material: None });
    }

    // A selected Edit Poly node shows the mesh being edited, the selected
    // polygons tinted.
    let stage = crate::modelling::stage(graph, &eval_subnet);
    if let Some(hl) = stage.as_ref().and_then(crate::modelling::highlight_mesh) {
        let mut inst = vec![];
        push_instance(&mut inst, &Mat4::IDENTITY, [1.0, 0.05, 0.02, 0.45], false);
        s.meshes.push(MeshDraw { geometry: Arc::new(Geometry::of(&hl, Kind::Surface)), instances: inst, blend: true, edges: false, material: None });
    }

    let packed = if stage.is_none() {
        let _t = crate::timings::span("viewer: cook the graph");
        graph.evaluate_for_viewport_packed(&eval_subnet)
    } else { None };
    // A material viewed on its own: a sphere wearing it.
    let packed = match packed {
        Some(EvalResult::Material(m)) => Some(crate::core::material::preview(&m)),
        other => other,
    };
    let surface_grey = { let c = linear_f([0.6, 0.6, 0.6]); [c[0], c[1], c[2], 1.0] };
    let wire_ink = { let c = ink(crate::theme::c(225, 225, 225)); [c[0], c[1], c[2], 1.0] };
    let bg_lin = { let c = linear(bg); [c[0], c[1], c[2], 1.0] };

    match packed {
        Some(EvalResult::Named(prims)) => {
            // Cameras: kept to be looked through, drawn as a pyramid to their
            // gate, but for the one looked through.
            let looked = super::camera::looking_through();
            let cam_ink = { let c = ink(crate::theme::c(225, 215, 140)); [c[0], c[1], c[2], 1.0] };
            for p in prims.iter().filter(|p| !crate::core::properties::invisible(p)) {
                let (Some(c), Some(world)) = (&p.camera, p.place.matrices().first()) else { continue };
                s.cameras.push((p.path.clone(), *world, (**c).clone()));
                if looked.as_deref() == Some(p.path.as_str()) { continue; }
                for [a, b] in c.glyph(*world) {
                    s.lines.extend_from_slice(&a.to_array()); s.lines.extend_from_slice(&cam_ink);
                    s.lines.extend_from_slice(&b.to_array()); s.lines.extend_from_slice(&cam_ink);
                }
            }
            // Lights: their shapes, in their colour (see `Light::glyph`).
            for p in prims.iter().filter(|p| !crate::core::properties::invisible(p)) {
                let (Some(l), Some(world)) = (&p.light, p.place.matrices().first()) else { continue };
                let c = linear_f(l.tint());
                let col = [c[0], c[1], c[2], 1.0];
                for [a, b] in l.glyph(*world) {
                    s.lines.extend_from_slice(&a.to_array()); s.lines.extend_from_slice(&col);
                    s.lines.extend_from_slice(&b.to_array()); s.lines.extend_from_slice(&col);
                }
            }
            // Purposes the Scene Explorer's toggles leave out are not drawn.
            let prims: Vec<Prim> = prims.into_iter()
                // Its purpose, or the one it inherits from a prim above it.
                .filter(|p| { let purpose = crate::core::properties::purpose_of(p); purpose == Purpose::Default || input.hierarchy.draws_purpose(purpose) })
                // Set invisible, on itself or on a prim above it.
                .filter(|p| !crate::core::properties::invisible(p))
                // A point instancer is drawn by its prototypes, at its points.
                .filter(|p| p.instancing.is_none())
                // A material is worn, not drawn; a camera is drawn above; a render
                // prim is not drawn.
                .filter(|p| p.is_geometry())
                .collect();
            // Closed prims of a composed stage: the box around what is below.
            let prims = if prims.iter().any(|p| p.stage.is_some()) {
                let (shown, boxes) = {
                    let _t = crate::timings::span("viewer: boxes for closed prims");
                    crate::viewport::bounds::split(prims, input.hierarchy)
                };
                let (points, pairs) = crate::viewport::bounds::box_lines(&boxes);
                let c = ink(crate::theme::c(200, 200, 200));
                push_lines(&mut s.lines, &points, &pairs, &Mat4::IDENTITY, [c[0], c[1], c[2], 1.0]);
                shown
            } else { prims };
            // Every primitive drawn at each of its placements; copies of
            // one mesh share it. In Textured mode a primitive takes its
            // material: colour and opacity, colour and emissive textures,
            // cut out by the colour texture's alpha where it says so.
            #[derive(PartialEq)]
            struct Key { mesh: usize, kind: Kind, look: usize, blend: bool }
            let mut groups: Vec<(Key, Arc<MeshData>, Vec<f32>, Option<usize>)> = vec![];
            let mut looks: Vec<(usize, usize)> = vec![];
            for p in &prims {
                let identity = [Mat4::IDENTITY];
                let places: &[Mat4] = if p.place.is_placed() { p.place.matrices() } else { &identity };
                let look = p.look.as_deref().filter(|_| mode == DisplayMode::Textured);
                let mut add = |kind: Kind, colour: [f32; 4], flags: [f32; 4], look_key: usize, material: Option<usize>, blend: bool| {
                    let key = Key { mesh: Arc::as_ptr(&p.mesh) as usize, kind, look: look_key, blend };
                    let at = match groups.iter().position(|g| g.0 == key) {
                        Some(i) => i,
                        None => { groups.push((key, p.mesh.clone(), vec![], material)); groups.len() - 1 }
                    };
                    for m in places { push_instance_with(&mut groups[at].2, m, colour, flags); }
                };
                // Curves and points, as lines, at every copy.
                if crate::strands::has_strands(&p.mesh) {
                    let c = match look {
                        Some(l) => { let c = linear_f(l.color); [c[0], c[1], c[2], 1.0] }
                        None => { let c = ink(crate::theme::c(205, 205, 205)); [c[0], c[1], c[2], 1.0] }
                    };
                    add(Kind::Strands, c, [0.0; 4], 0, None, false);
                }
                if p.mesh.vertices.is_empty() { continue; }
                if matches!(mode, DisplayMode::HiddenLine | DisplayMode::Wireframe) {
                    add(Kind::Edges, wire_ink, [0.0; 4], 0, None, false);
                }
                match (mode, look) {
                    (DisplayMode::Wireframe, _) => {}
                    (DisplayMode::HiddenLine, _) => add(Kind::Surface, bg_lin, [0.0; 4], 0, None, false),
                    (_, Some(l)) => {
                        let look_key = l as *const crate::types::Look as usize;
                        let material = match looks.iter().find(|(k, _)| *k == look_key) {
                            Some((_, m)) => *m,
                            None => {
                                let map = |path: &Option<std::path::PathBuf>| path.as_ref()
                                    .and_then(|p| crate::viewport::textures::request(p).map(|d| (p.clone(), d)));
                                s.materials.push(Material { colour: map(&l.color_map), emissive: map(&l.emissive_map) });
                                looks.push((look_key, s.materials.len() - 1));
                                s.materials.len() - 1
                            }
                        };
                        let m = &s.materials[material];
                        let has_colour = m.colour.is_some();
                        let cut = l.cutout && m.colour.as_ref().is_some_and(|(_, d)| d.has_alpha);
                        let flags = [1.0, has_colour as u8 as f32, m.emissive.is_some() as u8 as f32, cut as u8 as f32];
                        let c = linear_f(l.color);
                        let kind = if has_colour || m.emissive.is_some() { Kind::Textured } else { Kind::Surface };
                        add(kind, [c[0], c[1], c[2], l.opacity], flags, look_key, Some(material), l.opacity < 0.999);
                    }
                    _ => add(Kind::Surface, surface_grey, [1.0, 0.0, 0.0, 0.0], 0, None, false),
                }
            }
            for (key, md, instances, material) in groups {
                let geometry = cache.get(&md, key.kind, &mut used);
                s.meshes.push(MeshDraw { geometry, instances, blend: key.blend, edges: matches!(key.kind, Kind::Edges | Kind::Strands), material });
            }
        }
        other => {
            // With a clip kept out of a collider, the collider is drawn too,
            // see-through so the character shows inside it.
            let collider = if stage.is_none() && matches!(other, Some(EvalResult::Anim(_))) { graph.display_collider() } else { None };
            let ghostly = collider.is_some();
            let shown: Option<Arc<MeshData>> = match (&stage, collider) {
                (Some(st), _) => Some(stage_mesh(&st.mesh)),
                (None, Some(c)) => Some(c),
                (None, None) => other.map(|r| Arc::new(r.into_mesh())),
            };
            if let Some(md) = shown {
                if crate::strands::has_strands(&md) {
                    let c = ink(crate::theme::c(205, 205, 205));
                    let mut inst = vec![];
                    push_instance(&mut inst, &Mat4::IDENTITY, [c[0], c[1], c[2], 1.0], false);
                    let geometry = cache.get(&md, Kind::Strands, &mut used);
                    s.meshes.push(MeshDraw { geometry, instances: inst, blend: false, edges: true, material: None });
                }
                if !md.vertices.is_empty() {
                    if matches!(mode, DisplayMode::HiddenLine | DisplayMode::Wireframe) {
                        let mut inst = vec![];
                        push_instance(&mut inst, &Mat4::IDENTITY, wire_ink, false);
                        let geometry = cache.get(&md, Kind::Edges, &mut used);
                        s.meshes.push(MeshDraw { geometry, instances: inst, blend: false, edges: true, material: None });
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
                        let geometry = cache.get(&md, Kind::Surface, &mut used);
                        s.meshes.push(MeshDraw { geometry, instances: inst, blend, edges: false, material: None });
                    }
                }
            }
        }
    }
    ghosts(&mut s, graph, &eval_subnet, input.time, cache, &mut used);
    cache.keep(&used);
    s
}

/// Ghosted operators: what each puts out drawn as a grey wireframe beside
/// the scene, the edges of its meshes at every copy, the bones of its clip
/// at the playhead. Hidden by what is in front, like the ground grid. The
/// operator with the view flag is drawn as the scene already.
fn ghosts(
    s:     &mut Scene,
    graph: &crate::node_graph::NodeGraphState,
    eval_subnet: &impl Fn(crate::types::IceGraphId, &crate::core::geo::Geo, &[crate::ice::Reference]) -> crate::core::geo::Geo,
    time:  f64,
    cache: &mut GeometryCache,
    used:  &mut Vec<(usize, Kind)>,
) {
    let grey = { let c = ink(crate::theme::c(170, 150, 165)); [c[0], c[1], c[2], 1.0] };
    for node in graph.nodes.iter().filter(|n| n.ghost && Some(n.id) != graph.view_flag) {
        let r = graph.eval_node_out(node.id, 0, &mut std::collections::HashMap::new(), eval_subnet);
        let mut draw = |md: &Arc<MeshData>, places: &[Mat4]| {
            if md.vertices.is_empty() { return; }
            let mut inst = vec![];
            for m in places { push_instance(&mut inst, m, grey, false); }
            let geometry = cache.get(md, Kind::Edges, used);
            s.meshes.push(MeshDraw { geometry, instances: inst, blend: false, edges: true, material: None });
        };
        match r {
            Some(EvalResult::Named(prims)) => for p in prims.iter().filter(|p| p.is_geometry()) {
                let identity = [Mat4::IDENTITY];
                draw(&p.mesh, if p.place.is_placed() { p.place.matrices() } else { &identity });
            },
            Some(EvalResult::Anim(clip)) => {
                if clip.joints.is_empty() { continue; }
                let pose = clip.world_pose(clip.index_at(time));
                for (j, jt) in clip.joints.iter().enumerate() {
                    if let Some(p) = jt.parent.filter(|p| clip.joints[*p].is_bone) {
                        s.lines.extend_from_slice(&pose[p].w_axis.truncate().to_array()); s.lines.extend_from_slice(&grey);
                        s.lines.extend_from_slice(&pose[j].w_axis.truncate().to_array()); s.lines.extend_from_slice(&grey);
                    }
                }
            }
            Some(EvalResult::Material(_)) | None => {}
            Some(other) => draw(&Arc::new(other.into_mesh()), &[Mat4::IDENTITY]),
        }
    }
}

/// A clip's skeleton at `time`, as lines, as under Bevy (`draw_clip_skeleton`).
fn skeleton(out: &mut Vec<f32>, clip: &crate::core::anim::AnimData, time: f64) {
    if clip.joints.is_empty() { return; }
    let pose = clip.world_pose(clip.index_at(time));
    let pos: Vec<Vec3> = pose.iter().map(|m| m.w_axis.truncate()).collect();
    // Marker size follows the skeleton, so centimetre and metre rigs both read.
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in &pos { lo = lo.min(*p); hi = hi.max(*p); }
    let r = ((hi - lo).max_element() * 0.012).max(1e-4);
    let col = |c: [f32; 3]| { let l = linear_f(c); [l[0], l[1], l[2], 1.0] };
    let (bone, joint) = (col([0.95, 0.82, 0.35]), col([0.95, 0.95, 0.95]));
    let mut seg = |a: Vec3, b: Vec3, c: [f32; 4]| {
        out.extend_from_slice(&a.to_array()); out.extend_from_slice(&c);
        out.extend_from_slice(&b.to_array()); out.extend_from_slice(&c);
    };
    for (j, jt) in clip.joints.iter().enumerate() {
        match jt.parent {
            // A helper above the bones (a character root at the origin) is
            // not a bone: no link is drawn from it.
            Some(p) => if clip.joints[p].is_bone { seg(pos[p], pos[j], bone); },
            None => {
                let m = pose[j];
                let l = r * 5.0;
                seg(pos[j], pos[j] + m.x_axis.truncate().normalize_or_zero() * l, col([1.0, 0.2, 0.2]));
                seg(pos[j], pos[j] + m.y_axis.truncate().normalize_or_zero() * l, col([0.2, 1.0, 0.2]));
                seg(pos[j], pos[j] + m.z_axis.truncate().normalize_or_zero() * l, col([0.3, 0.5, 1.0]));
            }
        }
        // A ball: three rings of eight.
        for (u, v) in [(Vec3::X, Vec3::Y), (Vec3::Y, Vec3::Z), (Vec3::Z, Vec3::X)] {
            let at = |k: usize| { let a = k as f32 / 8.0 * std::f32::consts::TAU; pos[j] + (u * a.cos() + v * a.sin()) * r };
            for k in 0..8 { seg(at(k), at(k + 1), joint); }
        }
    }
}
