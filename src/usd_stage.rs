//! Composed USD stages, read through `xms-usd`.
//!
//! `xms-usd` opens the stage with `openusd`, which resolves composition
//! (sublayers, references, payloads, inherits, specializes, variants, in text
//! and binary layers), and translates it into a scene layer (`xms-scene`): raw,
//! USD-shaped prims keyed by path. This module turns that scene layer into the
//! `UsdScene` the rest of the program already knows: meshes as packed
//! primitives, materials, cameras, lights, and the stage hierarchy for the
//! scene explorer.
//!
//! Geometry stays in its own space, as a `Geo` with its topology and every
//! primvar (normals, UVs with their indices and interpolation, displayColor,
//! custom ones). Each packed primitive is placed by its world transform, which
//! includes the stage's up-axis and unit correction (also kept on the stage
//! tree as `root`, so writing can take it off again). Instanced geometry is
//! shared: a native instance's mesh, and a point instancer's prototype mesh, is
//! made once and placed by every copy. A native instance gives one packed
//! primitive per mesh under the instance's own path; a point instancer gives
//! one per prototype mesh, holding all its points.

use crate::core::geo_mesh::GeoMesh;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use glam::{DMat4, DQuat, DVec3, Mat4};
use xms_scene as rs;

use crate::core::geo::{self, Attr, Column, Context, Geo, Kind, Role, Subdiv, Topology};
use crate::types::{MeshData, Placement, Purpose};
use crate::usd_scene::{StageNode, StageTree, UsdCamera, UsdLight, UsdMaterial, UsdMesh, UsdScene, UsdSkeleton};

/// Read a composed stage, as authored. `layer` is the root layer (a `.usdz` is
/// unpacked beforehand, so its textures are files on disk).
#[cfg(test)]
pub fn read(layer: &Path) -> Result<UsdScene, String> {
    read_phased(layer, false, |_| {})
}

/// Read a composed stage in two passes over one open stage: `boxes` is given
/// the stage as boxes first (each mesh, curves or points as the box of its
/// extent, no materials or authored values), then the whole of it is read and
/// returned. `convert`: turned to Y-up and metres (see `scene_of`).
pub fn read_phased(layer: &Path, convert: bool, boxes: impl FnOnce(UsdScene)) -> Result<UsdScene, String> {
    let start = std::time::Instant::now();
    let mut first = 0.0;
    let translated = xms_usd::translate_phased(layer, Default::default(), |b| {
        let _t = crate::timings::span("usd: the stage as boxes, for the Viewer");
        first = b.seconds;
        boxes(scene_of(b, layer, true, convert));
    })?;
    if crate::timings::on() {
        // The stage is opened once (openusd reads and composes the layers),
        // translated as boxes, then in full (xms-usd), which says where its
        // time went.
        let total = start.elapsed().as_secs_f64();
        eprintln!(
            "[xms] usd: open the stage (openusd: read and compose the layers) and make the boxes: {:.0} ms",
            (total - translated.seconds) * 1e3
        );
        eprintln!("[xms] usd: translate the boxes (xms-usd): {:.0} ms", first * 1e3);
        eprintln!(
            "[xms] usd: translate the whole stage (xms-usd): {:.0} ms",
            translated.seconds * 1e3
        );
        for w in translated
            .warnings
            .iter()
            .filter(|w| w.starts_with("Scene-layer translation:"))
        {
            eprintln!("[xms]     {w}");
        }
    }
    Ok(scene_of(translated, layer, false, convert))
}

/// A translated stage as the program holds it: the hierarchy, and every mesh,
/// curves and points with its geometry and placements. `boxes`: the translation
/// was the first, boxes-only pass. `convert`: the stage turned to Y-up and
/// scaled to metres by its `upAxis` and `metersPerUnit`; else as authored, as
/// USD composes it and renderers read it.
fn scene_of(translated: xms_usd::Translated, layer: &Path, boxes: bool, convert: bool) -> UsdScene {
    let _walk = crate::timings::span(if boxes {
        "usd: boxes to geometry"
    } else {
        "usd: prims to geometry and mesh views"
    });
    let sc = &translated.scene;
    let info = &sc.info;

    let mut scene = UsdScene {
        up_axis: if info.z_up { "Z".into() } else { "Y".into() },
        meters_per_unit: info.meters_per_unit,
        default_prim: info.default_prim.clone(),
        // The scene layer does not carry timeCodesPerSecond yet.
        time: info.time_range.map(|(start, end)| (start, end, 24.0)),
        converted: convert,
        ..Default::default()
    };

    // When asked: to Y-up, and to metres when the stage says what its unit
    // is. The geometry stays as authored; this is in the placements.
    let mut root = DMat4::IDENTITY;
    if convert && info.z_up {
        root = DMat4::from_rotation_x(-std::f64::consts::FRAC_PI_2);
    }
    if let Some(unit) = info.meters_per_unit.filter(|_| convert) {
        if unit > 0.0 && (unit - 1.0).abs() > 1e-9 {
            root *= DMat4::from_scale(DVec3::splat(unit));
        }
    }

    let t = sc.time().time;
    let mut walk = Walk {
        sc,
        t,
        root,
        scene: &mut scene,
        nodes: vec![],
        counts: Counts::default(),
        jobs: vec![],
        job_of: HashMap::new(),
        pending: vec![],
        listing: 0,
    };
    for child in sc.children(&rs::Path::root()) {
        // Native-instance prototypes are reached through their instances.
        if child.name().starts_with("__Prototype") {
            continue;
        }
        walk.prim(
            child,
            child.as_str().to_string(),
            root,
            None,
            Purpose::Default,
            1,
            false,
            false,
        );
    }
    let Walk {
        nodes,
        mut counts,
        jobs,
        pending,
        ..
    } = walk;

    // The geometry of every mesh, curves and points, once per source prim,
    // made on every core: each is independent of the others.
    let made: Vec<Option<(Arc<Geo>, Arc<MeshData>)>> = {
        use rayon::prelude::*;
        let _t = crate::timings::span(format!("usd: geometry of {} prims, in parallel", jobs.len()));
        jobs.par_iter()
            .map(|src| sc.get(src).and_then(|p| geometry(&p.kind, t)).map(shared))
            .collect()
    };
    let mut invalid: HashSet<String> = HashSet::new();
    for (mesh, job) in pending {
        match &made[job] {
            Some((geo, view)) => {
                scene.meshes[mesh].geo = Some(geo.clone());
                scene.meshes[mesh].mesh = view.clone();
            }
            None => {
                invalid.insert(scene.meshes[mesh].path.clone());
            }
        }
    }
    if !invalid.is_empty() {
        counts.invalid += invalid.len();
        scene.meshes.retain(|m| !invalid.contains(&m.path));
        for i in &mut scene.instancers {
            i.members.retain(|(path, ..)| !invalid.contains(path));
        }
    }
    let nodes_index: HashMap<String, usize> = nodes.iter().enumerate().map(|(i, n)| (n.path.clone(), i)).collect();
    // Drawn, but not where the hierarchy has a prim: the Scene Explorer
    // would list them a second time, by their paths. Said, to be fixed.
    let unplaced: Vec<&str> = scene
        .meshes
        .iter()
        .map(|m| m.path.as_str())
        .chain(scene.instancers.iter().map(|i| i.path.as_str()))
        .filter(|p| !nodes_index.contains_key(*p))
        .collect();
    if let Some(first) = unplaced.first() {
        scene.notes.push(format!(
            "{} drawn prims are not where the hierarchy lists them (first: {first})",
            unplaced.len()
        ));
    }
    scene.tree = Arc::new(StageTree {
        nodes,
        root: root.as_mat4(),
        source: layer.to_path_buf(),
        up_axis: scene.up_axis.clone(),
        meters_per_unit: scene.meters_per_unit,
        opinions: Default::default(),
        index: nodes_index,
        arc: None,
    });
    scene.notes = counts.notes();
    if boxes {
        scene
            .notes
            .insert(0, "Shown as boxes while the geometry is read".into());
        scene.reading = true;
        return scene;
    }
    // The translator's own notes, minus timings and empty reports.
    scene.notes.extend(
        translated.warnings.into_iter().filter(|w| {
            !w.starts_with("Scene-layer translation:") && !w.starts_with("Prototypes in the scene layer: 0 ")
        }),
    );
    scene
}

/// What the stage holds that is not turned into packed primitives yet.
#[derive(Default)]
struct Counts {
    nested: usize,
    invalid: usize,
    gprims: usize,
    subsets: usize,
    hidden: usize,
}

impl Counts {
    fn notes(&self) -> Vec<String> {
        let mut notes = vec![];
        let mut say = |n: usize, text: &str| {
            if n > 0 {
                notes.push(format!("{n} {text}"))
            }
        };
        say(
            self.invalid,
            "meshes, curves or points have inconsistent topology or primvars and are left out",
        );
        say(
            self.nested,
            "point instancers nested more than four deep in other point instancers' prototypes are not expanded",
        );
        say(
            self.gprims,
            "implicit shapes (sphere, cube, cylinder…) are not read yet",
        );
        say(
            self.subsets,
            "meshes have face subsets (per-face materials): the mesh's own material is used",
        );
        say(self.hidden, "invisible meshes are listed but not drawn");
        notes
    }
}

struct Walk<'a> {
    sc: &'a rs::Scene,
    t: f64,
    root: DMat4,
    scene: &'a mut UsdScene,
    nodes: Vec<StageNode>,
    counts: Counts,
    /// The prims whose geometry is to be made, each once (by its path in the
    /// scene layer, so an instanced mesh is shared by every copy), after the
    /// walk and in parallel.
    jobs: Vec<rs::Path>,
    job_of: HashMap<String, usize>,
    /// Which of `scene.meshes` takes which job's geometry.
    pending: Vec<(usize, usize)>,
    /// Inside a point instancer: prims are listed, its prototypes are drawn
    /// through the instancer instead.
    listing: usize,
}

impl Walk<'_> {
    /// One prim and what is below it. `src` is where the scene layer holds it
    /// (inside a prototype, for an instance proxy); `shown` is its path as the
    /// user sees it. `under_instance`: inside an expanded instance, where
    /// materials and cameras are not collected again.
    #[allow(clippy::too_many_arguments)]
    fn prim(
        &mut self,
        src: &rs::Path,
        shown: String,
        parent: DMat4,
        binding: Option<String>,
        purpose: Purpose,
        depth: usize,
        hidden: bool,
        under_instance: bool,
    ) {
        // The scene outlives the walk: borrow it apart from `self`.
        let sc = self.sc;
        let Some(prim) = sc.get(src) else { return };
        let t = self.t;
        let type_name = prim.type_name.clone();
        let key = if type_name.is_empty() {
            "(untyped)".to_string()
        } else {
            type_name.clone()
        };
        *self.scene.prim_counts.entry(key).or_default() += 1;

        let hidden = hidden || !prim.visible_at(t);
        let purpose = purpose_of(sc, src, &prim.purpose, purpose);
        let name = shown.rsplit('/').next().unwrap_or("").to_string();
        let local = mat(&prim.local_xform_at(t));
        self.nodes.push(StageNode {
            path: shown.clone(),
            name,
            type_name: type_name.clone(),
            depth,
            hidden,
            local,
            reset_xform: prim.reset_xform_stack,
            proxy: under_instance,
            prototype: self.listing > 0,
            authored: authored(prim),
        });

        let world = if prim.reset_xform_stack {
            self.root * local
        } else {
            parent * local
        };
        let binding = prim
            .material_binding
            .as_ref()
            .map(|p| p.as_str().to_string())
            .or(binding);

        match &prim.kind {
            rs::PrimKind::Material(m) => {
                // Its shaders are held inside it, not as prims of their own.
                if !m.nodes.is_empty() {
                    *self.scene.prim_counts.entry("Shader".into()).or_default() += m.nodes.len();
                }
                if !under_instance {
                    self.scene.materials.push(material(src.as_str(), m));
                }
                return;
            }
            rs::PrimKind::Instance { prototype } => {
                for child in sc.children(prototype) {
                    let child_shown = format!("{shown}/{}", child.name());
                    self.prim(
                        child,
                        child_shown,
                        world,
                        binding.clone(),
                        purpose,
                        depth + 1,
                        hidden,
                        true,
                    );
                }
                return;
            }
            rs::PrimKind::Mesh(_) | rs::PrimKind::Curves(_) | rs::PrimKind::Points(_) => {
                if let rs::PrimKind::Mesh(m) = &prim.kind {
                    if !m.subsets.is_empty() && !hidden && self.listing == 0 {
                        self.counts.subsets += 1;
                    }
                }
                if self.listing > 0 {
                    // A point instancer's prototype: drawn through the instancer.
                } else if hidden {
                    self.counts.hidden += 1;
                } else {
                    // In its own space, placed by its world transform (root
                    // correction included). Under an instance it is shared with
                    // every other instance of the same prototype.
                    self.later(src);
                    self.scene.meshes.push(UsdMesh {
                        path: shown.clone(),
                        mesh: Arc::default(),
                        geo: None,
                        material: binding.clone(),
                        place: Placement::One(world.as_mat4()),
                        purpose,
                    });
                }
            }
            rs::PrimKind::Camera(c) if !under_instance => self.scene.cameras.push(UsdCamera {
                path: shown.clone(),
                camera: crate::core::camera::Camera {
                    projection: if c.projection_orthographic {
                        crate::core::camera::Projection::Orthographic
                    } else {
                        crate::core::camera::Projection::Perspective
                    },
                    focal_length: c.focal_length.at(t).unwrap_or(50.0) as f32,
                    horizontal_aperture: c.horizontal_aperture as f32,
                    vertical_aperture: c.vertical_aperture as f32,
                    horizontal_aperture_offset: c.horizontal_aperture_offset as f32,
                    vertical_aperture_offset: c.vertical_aperture_offset as f32,
                    clipping_range: [c.clipping_range[0] as f32, c.clipping_range[1] as f32],
                    f_stop: c.f_stop.at(t).unwrap_or(0.0) as f32,
                    focus_distance: c.focus_distance.at(t).unwrap_or(0.0) as f32,
                },
                place: Placement::One(world.as_mat4()),
                purpose,
            }),
            rs::PrimKind::Light(l) if !under_instance => self.scene.lights.push(UsdLight {
                path: shown.clone(),
                light: light_of(l, &type_name, t),
                place: Placement::One(world.as_mat4()),
                purpose,
            }),
            rs::PrimKind::Instancer(i) => {
                // Nested in another instancer's prototype: drawn through it
                // (`instancer_at`), at each of its copies.
                if self.listing == 0 && !hidden {
                    self.point_instancer(src, &shown, i, world, binding.clone(), purpose);
                }
                // Its prototypes usually sit below it: listed, not drawn as they are.
                self.listing += 1;
                for child in sc.children(src) {
                    let child_shown = format!("{shown}/{}", child.name());
                    self.prim(
                        child,
                        child_shown,
                        world,
                        binding.clone(),
                        purpose,
                        depth + 1,
                        hidden,
                        under_instance,
                    );
                }
                self.listing -= 1;
                return;
            }
            rs::PrimKind::Gprim(_) => self.counts.gprims += 1,
            _ => {
                if type_name == "Skeleton" && !under_instance {
                    self.scene.skeletons.push(UsdSkeleton {
                        path: shown.clone(),
                        joints: vec![],
                    });
                }
            }
        }
        for child in sc.children(src) {
            let child_shown = format!("{shown}/{}", child.name());
            self.prim(
                child,
                child_shown,
                world,
                binding.clone(),
                purpose,
                depth + 1,
                hidden,
                under_instance,
            );
        }
    }

    /// The geometry of `src` for the mesh about to be pushed onto
    /// `scene.meshes`: made after the walk, once per source prim, so every copy
    /// of an instanced mesh shares it.
    fn later(&mut self, src: &rs::Path) {
        let job = match self.job_of.get(src.as_str()) {
            Some(&j) => j,
            None => {
                self.jobs.push(src.clone());
                self.job_of.insert(src.as_str().to_string(), self.jobs.len() - 1);
                self.jobs.len() - 1
            }
        };
        self.pending.push((self.scene.meshes.len(), job));
    }

    /// A point instancer: one packed primitive per mesh of each prototype,
    /// placed at every point that uses that prototype.
    fn point_instancer(
        &mut self,
        src: &rs::Path,
        shown: &str,
        i: &rs::Instancer,
        world: DMat4,
        binding: Option<String>,
        purpose: Purpose,
    ) {
        self.instancer_at(src, shown, i, &[world], None, binding, purpose, 0);
    }

    /// A point instancer placed at `at`: once for one in the hierarchy, once
    /// per copy for one nested in another instancer's prototype (`by`: that
    /// instancer, the prototype's index, this one's transform inside it). Its
    /// prototypes' meshes are placed at every one of its points in every one of
    /// its copies; an instancer in one of its prototypes is nested in it, the
    /// same way, down to four deep.
    #[allow(clippy::too_many_arguments)]
    fn instancer_at(
        &mut self,
        src: &rs::Path,
        shown: &str,
        i: &rs::Instancer,
        at: &[DMat4],
        by: Option<(String, u32, Mat4)>,
        binding: Option<String>,
        purpose: Purpose,
        depth: usize,
    ) {
        let (sc, t) = (self.sc, self.t);
        let positions = i.positions.at(t).unwrap_or_default();
        let scales = i.scales.as_ref().and_then(|s| s.at(t)).unwrap_or_default();
        let orients = i.orientations.as_ref().and_then(|o| o.at(t)).unwrap_or_default();
        let invisible: HashSet<i64> = i.invisible_ids.iter().copied().collect();
        // The instancer itself: one point per instance, its arrays as attributes.
        let place = match (&by, at) {
            (None, [one]) => Placement::One(one.as_mat4()),
            _ => Placement::Many(at.iter().map(|m| m.as_mat4()).collect()),
        };
        let mut instancer = self.instancer_prim(src, shown, i, place, purpose, &positions, &orients, &scales);
        instancer.by = by;
        // Each point in the instancer's space, by prototype: scaled, then
        // turned, then moved.
        let mut points: Vec<Vec<DMat4>> = vec![vec![]; i.prototypes.len()];
        for (k, &proto) in i.proto_indices.iter().enumerate() {
            let id = i.ids.as_ref().and_then(|ids| ids.get(k).copied()).unwrap_or(k as i64);
            if invisible.contains(&id) {
                continue;
            }
            let Some(list) = points.get_mut(proto as usize) else {
                continue;
            };
            let p = positions.get(k).copied().unwrap_or([0.0; 3]);
            let s = scales.get(k).copied().unwrap_or([1.0; 3]);
            // Stored (i, j, k, real).
            let q = orients
                .get(k)
                .map(|q| DQuat::from_xyzw(q[0] as f64, q[1] as f64, q[2] as f64, q[3] as f64).normalize())
                .unwrap_or(DQuat::IDENTITY);
            list.push(DMat4::from_scale_rotation_translation(
                DVec3::new(s[0] as f64, s[1] as f64, s[2] as f64),
                q,
                DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64),
            ));
        }
        // Every copy of the instancer, then every point in it: where a
        // prototype is drawn.
        let copies = |local: &[DMat4], inside: DMat4| -> Vec<DMat4> {
            at.iter()
                .flat_map(|a| local.iter().map(move |p| *a * *p * inside))
                .collect()
        };
        for (index, (proto, local)) in i.prototypes.iter().zip(&points).enumerate() {
            if local.is_empty() {
                continue;
            }
            // Where the hierarchy lists it.
            let proto_shown = shown_path(src.as_str(), shown, proto.as_str());
            let mut found = vec![];
            let mut nested = vec![];
            self.gather(
                proto,
                proto_shown,
                DMat4::IDENTITY,
                binding.clone(),
                purpose,
                &mut found,
                &mut nested,
            );
            for (mesh_src, mesh_shown, inside, material, purpose) in found {
                let Some(kind) = sc.get(&mesh_src).map(|p| &p.kind) else {
                    continue;
                };
                if let rs::PrimKind::Mesh(m) = kind {
                    if !m.subsets.is_empty() {
                        self.counts.subsets += 1;
                    }
                }
                let place: Arc<[Mat4]> = copies(local, inside).iter().map(|m| m.as_mat4()).collect();
                instancer
                    .members
                    .push((mesh_shown.clone(), index as u32, inside.as_mat4()));
                self.later(&mesh_src);
                self.scene.meshes.push(UsdMesh {
                    path: mesh_shown,
                    mesh: Arc::default(),
                    geo: None,
                    material,
                    place: Placement::Many(place),
                    purpose,
                });
            }
            for (inner_src, inner_shown, inside, material, purpose) in nested {
                let Some(rs::PrimKind::Instancer(inner)) = sc.get(&inner_src).map(|p| &p.kind) else {
                    continue;
                };
                if depth >= 3 {
                    self.counts.nested += 1;
                    continue;
                }
                let inner_at = copies(local, inside);
                self.instancer_at(
                    &inner_src,
                    &inner_shown,
                    inner,
                    &inner_at,
                    Some((shown.to_string(), index as u32, inside.as_mat4())),
                    material,
                    purpose,
                    depth + 1,
                );
            }
        }
        self.scene.instancers.push(instancer);
    }

    /// A point instancer as a points primitive, in its own space: `points` from
    /// its positions, and `orient`, `scale`, `protoIndex` and `id` per point
    /// where it has them. Its prototypes' paths are kept by index.
    #[allow(clippy::too_many_arguments)]
    fn instancer_prim(
        &self,
        src: &rs::Path,
        shown: &str,
        i: &rs::Instancer,
        place: Placement,
        purpose: Purpose,
        positions: &[[f32; 3]],
        orients: &[[f32; 4]],
        scales: &[[f32; 3]],
    ) -> crate::usd_scene::UsdInstancer {
        use crate::core::geo::{Attr, Column, Context, Geo, Role};
        let n = i.proto_indices.len().max(positions.len());
        let mut points = positions.to_vec();
        points.resize(n, [0.0; 3]);
        let mut geo = Geo::from_points(points);
        if !orients.is_empty() {
            let mut q = orients.to_vec();
            q.resize(n, [0.0, 0.0, 0.0, 1.0]);
            geo.set(
                Context::Point,
                "orient",
                Attr::new(Column::Quat(Arc::new(q)), Role::None),
            );
        }
        if !scales.is_empty() {
            let mut s = scales.to_vec();
            s.resize(n, [1.0; 3]);
            geo.set(
                Context::Point,
                "scale",
                Attr::new(Column::Vec3(Arc::new(s)), Role::None),
            );
        }
        let mut index: Vec<i32> = i.proto_indices.iter().map(|&k| k as i32).collect();
        index.resize(n, 0);
        geo.set(
            Context::Point,
            "protoIndex",
            Attr::new(Column::Int(Arc::new(index)), Role::None),
        );
        if let Some(ids) = &i.ids {
            let mut id: Vec<i32> = ids.iter().map(|&k| k as i32).collect();
            id.resize(n, -1);
            geo.set(Context::Point, "id", Attr::new(Column::Int(Arc::new(id)), Role::None));
        }
        let prototypes: Vec<String> = i
            .prototypes
            .iter()
            .map(|proto| shown_path(src.as_str(), shown, proto.as_str()))
            .collect();
        let mesh = Arc::new(geo.to_mesh());
        crate::usd_scene::UsdInstancer {
            path: shown.to_string(),
            geo: Arc::new(geo),
            mesh,
            place,
            purpose,
            prototypes: Arc::new(prototypes),
            members: vec![],
            hidden: Arc::new(i.invisible_ids.clone()),
            by: None,
        }
    }

    /// The meshes of a prototype, with their transform inside it (its own
    /// root's included), their path as listed, and their material; and, in
    /// `nested`, the point instancers in it, the same way.
    #[allow(clippy::type_complexity)]
    fn gather(
        &mut self,
        src: &rs::Path,
        shown: String,
        parent: DMat4,
        binding: Option<String>,
        purpose: Purpose,
        out: &mut Vec<(rs::Path, String, DMat4, Option<String>, Purpose)>,
        nested: &mut Vec<(rs::Path, String, DMat4, Option<String>, Purpose)>,
    ) {
        let (sc, t) = (self.sc, self.t);
        let Some(prim) = sc.get(src) else { return };
        if !prim.visible_at(t) {
            return;
        }
        let purpose = purpose_of(sc, src, &prim.purpose, purpose);
        let local = mat(&prim.local_xform_at(t));
        let inside = if prim.reset_xform_stack { local } else { parent * local };
        let binding = prim
            .material_binding
            .as_ref()
            .map(|p| p.as_str().to_string())
            .or(binding);
        let below: &[rs::Path] = match &prim.kind {
            rs::PrimKind::Mesh(_) | rs::PrimKind::Curves(_) | rs::PrimKind::Points(_) => {
                out.push((src.clone(), shown.clone(), inside, binding.clone(), purpose));
                sc.children(src)
            }
            rs::PrimKind::Instance { prototype } => sc.children(prototype),
            // Its own prototypes are below it: reached through it, not as they are.
            rs::PrimKind::Instancer(_) => {
                nested.push((src.clone(), shown.clone(), inside, binding.clone(), purpose));
                return;
            }
            rs::PrimKind::Material(_) => return,
            _ => sc.children(src),
        };
        for c in below {
            self.gather(
                c,
                format!("{shown}/{}", c.name()),
                inside,
                binding.clone(),
                purpose,
                out,
                nested,
            );
        }
    }
}

/// Where the hierarchy lists a prim the scene layer holds at `target`, seen
/// from a prim it holds at `src` and lists at `shown`: the walk lists a native
/// instance's prims under the instance (`/island/isCoral5/geometry`) though the
/// scene layer holds them under its prototype (`/__Prototype_22/geometry`).
/// What `src` and `target` share is where both are; the rest of `target` goes
/// below the same place in `shown`. A target sharing nothing with `src` is at
/// its own path.
fn shown_path(src: &str, shown: &str, target: &str) -> String {
    let segs = |p: &str| {
        p.split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let (s, sh, t) = (segs(src), segs(shown), segs(target));
    let common = s.iter().zip(&t).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return target.to_string();
    }
    let up = s.len() - common;
    if up > sh.len() {
        return target.to_string();
    }
    let mut out = sh[..sh.len() - up].to_vec();
    out.extend(t[common..].iter().cloned());
    format!("/{}", out.join("/"))
}

/// What a prim authors beyond what the scene layer's fields carry, as Imago
/// values: tokens and strings as tokens, assets as assets, numbers and small
/// vectors as they are.
fn authored(prim: &rs::Prim) -> Vec<(String, crate::core::properties::Value)> {
    use crate::core::properties::Value;
    prim.authored
        .iter()
        .map(|(name, v)| {
            (
                name.clone(),
                match v {
                    rs::ShaderValue::Float(x) => Value::Float(*x),
                    rs::ShaderValue::Int(i) => Value::Int(*i),
                    rs::ShaderValue::Bool(b) => Value::Bool(*b),
                    rs::ShaderValue::Vec2(a) => Value::Float2(*a),
                    rs::ShaderValue::Vec3(a) => Value::Float3(*a),
                    rs::ShaderValue::Vec4(a) => Value::Float4(*a),
                    rs::ShaderValue::String(s) => Value::Token(s.clone()),
                    rs::ShaderValue::Asset(s) => Value::Asset(s.clone()),
                    rs::ShaderValue::Connection { node, output } => {
                        Value::String(format!("{}.{output}", node.as_str()))
                    }
                },
            )
        })
        .collect()
}

/// A prim's purpose: its own when authored, else its parent's. Render geometry
/// notes whether its asset also carries proxy geometry: a sibling of the prim
/// where render was set that is itself set to proxy (the usual `render` /
/// `proxy` pair under one asset).
fn purpose_of(sc: &rs::Scene, src: &rs::Path, own: &rs::Purpose, inherited: Purpose) -> Purpose {
    match own {
        rs::Purpose::Render => Purpose::Render {
            has_proxy: src.parent().is_some_and(|parent| {
                sc.children(&parent)
                    .iter()
                    .any(|c| sc.get(c).is_some_and(|p| matches!(p.purpose, rs::Purpose::Proxy)))
            }),
        },
        rs::Purpose::Proxy => Purpose::Proxy,
        rs::Purpose::Guide => Purpose::Guide,
        _ => inherited,
    }
}

/// The scene layer's row-vector matrix as a column-vector one: USD's rows are
/// the columns here, so the numbers are taken as they come.
fn mat(m: &rs::Mat4d) -> DMat4 {
    DMat4::from_cols_array_2d(&m.0)
}

// Geometry

/// A mesh, curves or points prim as geometry in its own space: topology and
/// every primvar, as USD has them. Placed in the scene by its packed
/// primitive's `Placement`, not by moving its points.
fn geometry(kind: &rs::PrimKind, t: f64) -> Option<Geo> {
    let geo = match kind {
        rs::PrimKind::Mesh(m) => mesh_geo(m, t)?,
        rs::PrimKind::Curves(c) => curves_geo(c, t)?,
        rs::PrimKind::Points(p) => points_geo(p, t)?,
        _ => return None,
    };
    geo.validate().ok()?;
    Some(geo)
}

/// The geometry and its `MeshData` view, made once per prim (or prototype).
fn shared(geo: Geo) -> (Arc<Geo>, Arc<MeshData>) {
    // Lean: the attributes stay in the geometry, and the Primitive
    // Inspector makes their rows when a geometry tab is opened, instead of
    // every primvar of every prim being expanded (a row of numbers per
    // element) as the stage loads.
    let view = Arc::new(geo.to_mesh_lean());
    (Arc::new(geo), view)
}

fn mesh_geo(m: &rs::Mesh, t: f64) -> Option<Geo> {
    let points = m.points.at(t)?;
    if points.len() < 3 || m.face_vertex_indices.is_empty() {
        return None;
    }
    let mut g = Geo::from_polygons(points, m.face_vertex_counts.clone(), m.face_vertex_indices.clone());
    if let Topology::Mesh {
        left_handed, subdiv, ..
    } = &mut g.topology
    {
        *left_handed = m.left_handed;
        *subdiv = match m.subdivision.scheme.as_deref() {
            Some("none") => Subdiv::None,
            Some("loop") => Subdiv::Loop,
            Some("bilinear") => Subdiv::Bilinear,
            _ => Subdiv::CatmullClark,
        };
    }
    if let Some(n) = &m.normals {
        add_primvar(&mut g, n, t);
    }
    for pv in &m.primvars {
        add_primvar(&mut g, pv, t);
    }
    Some(g)
}

fn curves_geo(c: &rs::Curves, t: f64) -> Option<Geo> {
    use crate::types::{CurveBasis, CurveWrap};
    let points = c.points.at(t)?;
    let total: usize = c.curve_vertex_counts.iter().map(|&n| n as usize).sum();
    if points.is_empty() || c.curve_vertex_counts.is_empty() || total > points.len() {
        return None;
    }
    let basis = if c.curve_type == "linear" {
        CurveBasis::Linear
    } else {
        match c.basis.as_str() {
            "bspline" => CurveBasis::Bspline,
            "catmullRom" => CurveBasis::CatmullRom,
            _ => CurveBasis::Bezier,
        }
    };
    let wrap = match c.wrap.as_str() {
        "periodic" => CurveWrap::Periodic,
        "pinned" => CurveWrap::Pinned,
        _ => CurveWrap::Nonperiodic,
    };
    let mut g = Geo::from_points(points);
    g.topology = Topology::curves(c.curve_vertex_counts.clone(), basis, wrap);
    for pv in c.widths.iter().chain(&c.normals).chain(&c.primvars) {
        add_primvar(&mut g, pv, t);
    }
    Some(g)
}

fn points_geo(p: &rs::Points, t: f64) -> Option<Geo> {
    let points = p.points.at(t)?;
    if points.is_empty() {
        return None;
    }
    let mut g = Geo::from_points(points);
    for pv in p.widths.iter().chain(&p.primvars) {
        add_primvar(&mut g, pv, t);
    }
    Some(g)
}

/// A primvar as an attribute, in the context of its interpolation. Without one,
/// the context is the one whose element count it matches (corners first, as
/// faceVarying UVs are the common unauthored case). Primvars that match no
/// context, such as curve widths per segment end (`varying` on curves), are
/// left out: they stay in the file, unedited.
fn add_primvar(g: &mut Geo, pv: &rs::Primvar, t: f64) {
    let Some(values) = pv.values.at(t) else { return };
    let column = match values {
        rs::PrimvarValues::Float(v) => Column::Float(Arc::new(v)),
        rs::PrimvarValues::Float2(v) => Column::Vec2(Arc::new(v)),
        rs::PrimvarValues::Float3(v) => Column::Vec3(Arc::new(v)),
        rs::PrimvarValues::Float4(v) => Column::Vec4(Arc::new(v)),
        #[allow(unreachable_patterns)]
        _ => return,
    };
    let elements = pv.indices.as_ref().map_or(column.len(), |i| i.len());
    let by_count = || {
        [Context::Corner, Context::Point, Context::Face, Context::Object]
            .into_iter()
            .find(|&c| g.count(c) == elements && elements > 0)
    };
    let ctx = match pv.interpolation {
        Some(rs::Interpolation::Constant) => Some(Context::Object),
        Some(rs::Interpolation::Uniform) => Some(Context::Face),
        Some(rs::Interpolation::Vertex) => Some(Context::Point),
        Some(rs::Interpolation::FaceVarying) => Some(Context::Corner),
        // Per point for meshes and points; per segment end for curves.
        Some(rs::Interpolation::Varying) => Some(Context::Point).filter(|&c| g.count(c) == elements),
        None => by_count(),
    };
    let Some(ctx) = ctx.filter(|&c| g.count(c) == elements) else {
        return;
    };
    let role = match (pv.name.as_str(), column.kind()) {
        (geo::NORMALS, _) => Role::Normal,
        ("displayColor", _) => Role::Color,
        (_, Kind::Vec2) if geo::UV_NAMES.contains(&pv.name.as_str()) => Role::TexCoord,
        _ => Role::None,
    };
    let attr = match &pv.indices {
        Some(ix) => Attr::indexed(column, ix.clone(), role),
        None => Attr::new(column, role),
    };
    g.set(ctx, &pv.name, attr);
}

// Materials

/// A material as the viewport and the properties panel know it: the surface
/// shader's colour, roughness, metallic and opacity, and the texture behind
/// each of those inputs. OpenPBR and Standard Surface inputs are given the
/// names of the UsdPreviewSurface inputs they stand for.
fn material(path: &str, m: &rs::Material) -> UsdMaterial {
    let mut out = UsdMaterial {
        path: path.to_string(),
        ..Default::default()
    };
    match crate::materialx::from_usd(m) {
        Ok((g, notes)) => {
            out.graph = Some(Arc::new(g));
            out.graph_notes = notes;
        }
        Err(why) => out.graph_notes = vec![why],
    }
    let node = |p: &rs::Path| m.nodes.iter().find(|n| n.path == *p);
    let Some(shader) = m.surface.as_ref().and_then(|(p, _)| node(p)) else {
        return out;
    };
    let roles: &[(&str, &str)] = if shader.id.contains("open_pbr_surface") {
        &[
            ("base_color", "diffuseColor"),
            ("specular_roughness", "roughness"),
            ("base_metalness", "metallic"),
            ("geometry_opacity", "opacity"),
            ("emission_color", "emissiveColor"),
        ]
    } else if shader.id.contains("standard_surface") {
        &[
            ("base_color", "diffuseColor"),
            ("specular_roughness", "roughness"),
            ("metalness", "metallic"),
            ("opacity", "opacity"),
            ("emission_color", "emissiveColor"),
        ]
    } else {
        &[
            ("diffuseColor", "diffuseColor"),
            ("roughness", "roughness"),
            ("metallic", "metallic"),
            ("opacity", "opacity"),
            ("emissiveColor", "emissiveColor"),
        ]
    };
    for (input, role) in roles {
        let Some(i) = shader.inputs.iter().find(|i| i.name == *input) else {
            continue;
        };
        match &i.value {
            rs::ShaderValue::Connection { node: src, .. } => {
                if let Some(file) = node(src).and_then(texture_file) {
                    out.textures.push((role.to_string(), file));
                }
            }
            v => match *role {
                "diffuseColor" => out.diffuse = color(v),
                "roughness" => out.roughness = number(v),
                "metallic" => out.metallic = number(v),
                // Standard Surface's opacity is a colour.
                "opacity" => out.opacity = number(v).or_else(|| color(v).map(|c| (c[0] + c[1] + c[2]) / 3.0)),
                _ => {}
            },
        }
    }
    out
}

/// The file a texture node reads (UsdUVTexture, MaterialX image nodes).
fn texture_file(n: &rs::ShaderNode) -> Option<String> {
    n.inputs.iter().find(|i| i.name == "file").and_then(|i| match &i.value {
        rs::ShaderValue::Asset(s) | rs::ShaderValue::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    })
}

fn number(v: &rs::ShaderValue) -> Option<f32> {
    match v {
        rs::ShaderValue::Float(f) => Some(*f),
        rs::ShaderValue::Int(i) => Some(*i as f32),
        _ => None,
    }
}

fn color(v: &rs::ShaderValue) -> Option<[f32; 3]> {
    match v {
        rs::ShaderValue::Vec3(c) => Some(*c),
        rs::ShaderValue::Vec4(c) => Some([c[0], c[1], c[2]]),
        rs::ShaderValue::Float(f) => Some([*f; 3]),
        _ => None,
    }
}

/// A light of the scene layer as Imago holds it: its type, and the values the
/// file sets, at time `t`. Values the schema has as fallback are not kept.
fn light_of(l: &rs::Light, type_name: &str, t: f64) -> crate::core::light::Light {
    use crate::core::light::{Light, LightType};
    use crate::core::properties::Value;
    let kind = LightType::from_usd(type_name).unwrap_or(match &l.kind {
        rs::LightKind::Distant { .. } => LightType::Distant,
        rs::LightKind::Sphere { .. } => LightType::Sphere,
        rs::LightKind::Rect { .. } => LightType::Rect,
        rs::LightKind::Disk { .. } => LightType::Disk,
        rs::LightKind::Cylinder { .. } => LightType::Cylinder,
        rs::LightKind::Dome { .. } => LightType::Dome,
    });
    let mut out = Light::new(kind);
    if let Some(v) = l.intensity.at(t) {
        out.set("inputs:intensity", Value::Float(v));
    }
    if let Some(v) = l.exposure.at(t) {
        out.set("inputs:exposure", Value::Float(v));
    }
    if let Some(c) = l.color.first() {
        out.set("inputs:color", Value::Color3f(*c));
    }
    out.set("inputs:normalize", Value::Bool(l.normalize));
    out.set("inputs:enableColorTemperature", Value::Bool(l.enable_color_temperature));
    out.set("inputs:colorTemperature", Value::Float(l.color_temperature));
    let asset = |s: &Option<String>| s.clone().filter(|s| !s.is_empty()).map(Value::Asset);
    match &l.kind {
        rs::LightKind::Distant { angle } => out.set("inputs:angle", Value::Float(*angle)),
        rs::LightKind::Sphere { radius, treat_as_point } => {
            out.set("inputs:radius", Value::Float(*radius));
            out.set("treatAsPoint", Value::Bool(*treat_as_point));
        }
        rs::LightKind::Rect { width, height, texture } => {
            out.set("inputs:width", Value::Float(*width));
            out.set("inputs:height", Value::Float(*height));
            if let Some(v) = asset(texture) {
                out.set("inputs:texture:file", v);
            }
        }
        rs::LightKind::Disk { radius } => out.set("inputs:radius", Value::Float(*radius)),
        rs::LightKind::Cylinder {
            radius,
            length,
            treat_as_line,
        } => {
            out.set("inputs:radius", Value::Float(*radius));
            out.set("inputs:length", Value::Float(*length));
            out.set("treatAsLine", Value::Bool(*treat_as_line));
        }
        rs::LightKind::Dome {
            texture,
            format,
            pole_axis,
        } => {
            if let Some(v) = asset(texture) {
                out.set("inputs:texture:file", v);
            }
            if let Some(f) = format {
                out.set("inputs:texture:format", Value::Token(f.clone()));
            }
            if let Some(p) = pole_axis {
                out.set("poleAxis", Value::Token(p.clone()));
            }
        }
    }
    // Shaping, which the definitions do not list yet: kept, listed, written back.
    if let Some(a) = l.cone_angle {
        out.values.insert("inputs:shaping:cone:angle".into(), Value::Float(a));
    }
    if let Some(s) = l.cone_softness {
        out.values
            .insert("inputs:shaping:cone:softness".into(), Value::Float(s));
    }
    if let Some(f) = l.focus {
        out.values.insert("inputs:shaping:focus".into(), Value::Float(f));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::shown_path;

    #[test]
    fn prototype_paths_are_listed_where_the_hierarchy_has_them() {
        // Below the instancer.
        assert_eq!(shown_path("/a/inst", "/a/inst", "/a/inst/Protos/x"), "/a/inst/Protos/x");
        // Inside a native instance: the scene layer's prototype path, listed under the instance.
        assert_eq!(
            shown_path(
                "/__Prototype_22/geometry/xgCabbage",
                "/island/isCoral5/geometry/xgCabbage",
                "/__Prototype_22/geometry/xgCabbage/Prototypes/leaf"
            ),
            "/island/isCoral5/geometry/xgCabbage/Prototypes/leaf"
        );
        // Beside the instancer, in the same prototype.
        assert_eq!(
            shown_path(
                "/__Prototype_22/geometry/xgCabbage",
                "/island/isCoral5/geometry/xgCabbage",
                "/__Prototype_22/geometry/Protos/leaf"
            ),
            "/island/isCoral5/geometry/Protos/leaf"
        );
        // Elsewhere in the file: at its own path.
        assert_eq!(
            shown_path(
                "/__Prototype_22/geometry/xgCabbage",
                "/island/isCoral5/geometry/xgCabbage",
                "/library/leaf"
            ),
            "/library/leaf"
        );
    }
}
