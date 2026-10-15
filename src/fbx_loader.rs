//! FBX import through ufbx. Reads the node hierarchy and bakes one take to
//! dense per-frame local transforms. A mesh bound to the skeleton comes
//! with it, with its weights. Meshes on their own are read by `load_meshes`.
//!
//! ufbx converts the file to right-handed Y-up, metres, and folds pre/post
//! rotations and rotation orders into the local transform it evaluates.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use glam::{Mat4, Quat, Vec3};
use crate::core::xform::Transform;

use crate::core::anim::{AnimData, FileSpace, FrameRate, Joint, SkinMesh, Track};
use crate::types::MeshData;

#[derive(Clone)]
pub struct LoadedFbx {
    pub anim:  Arc<AnimData>,
    /// Names of every take (anim stack) in the file.
    pub takes: Arc<Vec<String>>,
}

fn to_transform(t: &ufbx::Transform) -> Transform {
    Transform {
        translation: Vec3::new(t.translation.x as f32, t.translation.y as f32, t.translation.z as f32),
        rotation:    Quat::from_xyzw(
            t.rotation.x as f32, t.rotation.y as f32, t.rotation.z as f32, t.rotation.w as f32,
        ).normalize(),
        scale:       Vec3::new(t.scale.x as f32, t.scale.y as f32, t.scale.z as f32),
    }
}

fn to_mat4(m: &ufbx::Matrix) -> Mat4 {
    Mat4::from_cols_array(&[
        m.m00 as f32, m.m10 as f32, m.m20 as f32, 0.0,
        m.m01 as f32, m.m11 as f32, m.m21 as f32, 0.0,
        m.m02 as f32, m.m12 as f32, m.m22 as f32, 0.0,
        m.m03 as f32, m.m13 as f32, m.m23 as f32, 1.0,
    ])
}

/// Triangles and quads of a mesh as vertex indices. Larger polygons are cut into a fan.
fn mesh_faces(mesh: &ufbx::Mesh, mut face: impl FnMut(&[u32])) {
    for f in mesh.faces.iter() {
        let (b, n) = (f.index_begin as usize, f.num_indices as usize);
        if n < 3 || b + n > mesh.vertex_indices.len() { continue; }
        let all: &[u32] = &mesh.vertex_indices;
        let idx = &all[b..b + n];
        if n <= 4 { face(idx); } else { for k in 1..n - 1 { face(&[idx[0], idx[k], idx[k + 1]]); } }
    }
}

/// Whether a matrix can be inverted. Its determinant is not a measure: a
/// skeleton carrying a unit conversion (0.0001 on each axis) has one of 1e-12.
fn invertible(m: &Mat4) -> bool {
    let d = m.determinant();
    d.is_finite() && d != 0.0 && m.inverse().is_finite()
}

/// World matrix of every joint in the pose the skin was bound in. The
/// file's bind pose where it lists the joint; a joint it does not list sits
/// under its parent as it does at rest; with no bind pose at all, joints rest
/// where the file has them. Clusters then set the joints they bind.
fn bind_pose(scene: &ufbx::Scene, order: &[usize]) -> Vec<Mat4> {
    let index: HashMap<usize, usize> = order.iter().enumerate().map(|(j, i)| (*i, j)).collect();
    let mut bind: Vec<Option<Mat4>> = vec![None; order.len()];
    for (j, i) in order.iter().enumerate() {
        let node = &scene.nodes[*i];
        let Some(pose) = node.bind_pose.as_ref() else { continue };
        if !pose.is_bind_pose { continue; }
        if let Some(b) = pose.bone_poses.iter().find(|b| b.bone_node.element.typed_id == node.element.typed_id) {
            let m = to_mat4(&b.bone_to_world);
            if invertible(&m) { bind[j] = Some(m); }
        }
    }
    if bind.iter().all(|b| b.is_none()) {
        return order.iter().map(|i| to_mat4(&scene.nodes[*i].node_to_world)).collect();
    }
    let mut out: Vec<Mat4> = Vec::with_capacity(order.len());
    for (j, i) in order.iter().enumerate() {
        let node = &scene.nodes[*i];
        let parent = node.parent.as_deref().and_then(|p| index.get(&(p.element.typed_id as usize))).copied().filter(|p| *p < j);
        out.push(match (bind[j], parent) {
            (Some(m), _) => m,
            (None, Some(p)) => out[p] * to_mat4(&node.node_to_parent),
            (None, None) => to_mat4(&node.node_to_world),
        });
    }
    out
}

/// The meshes bound to the skeleton, as one, with up to four weights per vertex.
fn read_skin(scene: &ufbx::Scene, joint_of: &HashMap<usize, usize>, order: &[usize]) -> Option<SkinMesh> {
    let mut skin = SkinMesh::default();
    skin.bind = bind_pose(scene, order);
    let mut bound = vec![false; order.len()];
    for mesh in scene.meshes.iter() {
        let Some(deformer) = mesh.skin_deformers.iter().next() else { continue };
        let Some(node) = mesh.element.instances.iter().next() else { continue };
        if deformer.vertices.len() < mesh.num_vertices { continue; }
        // Mesh space to the world, as the mesh stands in the file.
        let to_world = to_mat4(&node.geometry_to_world);
        // Joint of each cluster. Its bind matrix follows from where the mesh
        // is and how the cluster sees the mesh.
        let cluster_joint: Vec<Option<usize>> = deformer.clusters.iter().map(|c| {
            let j = *joint_of.get(&(c.bone_node.as_ref()?.element.typed_id as usize))?;
            if !bound[j] {
                let to_bone = to_mat4(&c.geometry_to_bone);
                if invertible(&to_bone) { skin.bind[j] = to_world * to_bone.inverse(); bound[j] = true; }
            }
            Some(j)
        }).collect();
        let base = skin.positions.len() as u32;
        for (v, p) in mesh.vertices.iter().enumerate() {
            skin.positions.push(to_world.transform_point3(Vec3::new(p.x as f32, p.y as f32, p.z as f32)));
            let sv = &deformer.vertices[v];
            let mut top = [(0u32, 0.0f32); 4];
            for k in sv.weight_begin as usize..(sv.weight_begin + sv.num_weights) as usize {
                let Some(w) = deformer.weights.get(k) else { break };
                let Some(Some(j)) = cluster_joint.get(w.cluster_index as usize) else { continue };
                let entry = (*j as u32, w.weight as f32);
                // Keep the four heaviest, heaviest first.
                let mut at = 4;
                for s in (0..4).rev() { if entry.1 > top[s].1 { at = s; } }
                if at < 4 { for s in (at + 1..4).rev() { top[s] = top[s - 1]; } top[at] = entry; }
            }
            let sum: f32 = top.iter().map(|t| t.1).sum();
            if sum > 1e-8 { for t in top.iter_mut() { t.1 /= sum; } } else { top[0] = (0, 1.0); }
            skin.joint.push(top[0].0);
            skin.weights.push(top);
        }
        mesh_faces(mesh, |idx| {
            let i = |k: usize| base + idx[k];
            skin.faces.push(if idx.len() == 3 { [i(0), i(1), i(2), i(2)] } else { [i(0), i(1), i(2), i(3)] });
        });
    }
    if skin.positions.is_empty() { return None; }
    // Smooth normals from the faces.
    let mut normals = vec![Vec3::ZERO; skin.positions.len()];
    for f in &skin.faces {
        let p = |k: usize| skin.positions[f[k] as usize];
        let n = (p(1) - p(0)).cross(p(2) - p(0)) + if SkinMesh::is_tri(f) { Vec3::ZERO } else { (p(2) - p(0)).cross(p(3) - p(0)) };
        for k in 0..4 { normals[f[k] as usize] += n; }
    }
    skin.normals = normals.into_iter().map(|n| n.normalize_or(Vec3::Y)).collect();
    Some(skin)
}

/// Every mesh of a file, in the world, as triangles: name of its node and
/// the mesh. For models with no skeleton, such as a set to collide with.
pub fn load_meshes(path: &str) -> Result<Vec<(String, Arc<MeshData>)>, String> {
    let opts = ufbx::LoadOpts {
        ignore_embedded:    true,
        ignore_animation:   true,
        target_axes:        ufbx::CoordinateAxes::right_handed_y_up(),
        target_unit_meters: 1.0,
        space_conversion:   ufbx::SpaceConversion::AdjustTransforms,
        ..Default::default()
    };
    let scene = ufbx::load_file(path, opts)
        .map_err(|e| format!("{}", e.description.as_ref() as &str))?;
    let mut out = vec![];
    for mesh in scene.meshes.iter() {
        for node in mesh.element.instances.iter() {
            let to_world = to_mat4(&node.geometry_to_world);
            let vertices: Vec<[f32; 3]> = mesh.vertices.iter()
                .map(|p| to_world.transform_point3(Vec3::new(p.x as f32, p.y as f32, p.z as f32)).to_array()).collect();
            let mut indices: Vec<u32> = Vec::with_capacity(mesh.num_triangles * 3);
            // A mirrored node turns its faces inside out: turn them back.
            let flip = to_world.determinant() < 0.0;
            mesh_faces(mesh, |idx| {
                for k in 1..idx.len() - 1 {
                    if flip { indices.extend([idx[0], idx[k + 1], idx[k]]); } else { indices.extend([idx[0], idx[k], idx[k + 1]]); }
                }
            });
            if indices.is_empty() { continue; }
            let mut data = MeshData::from_triangles(vertices, indices);
            data.face_count = mesh.num_faces;
            data.compute_normals();
            data.primvars.clear();
            out.push((node.element.name.to_string(), Arc::new(data)));
        }
    }
    if out.is_empty() { return Err("no meshes in file".into()); }
    Ok(out)
}

type MeshEntry = (Option<SystemTime>, Result<Vec<(String, Arc<MeshData>)>, String>);
static MESHES: OnceLock<Mutex<HashMap<String, MeshEntry>>> = OnceLock::new();

/// `load_meshes`, kept until the file changes.
pub fn load_meshes_cached(path: &str) -> Result<Vec<(String, Arc<MeshData>)>, String> {
    if path.is_empty() { return Err("no file set".into()); }
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let cache = MESHES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some((t, r)) = cache.lock().unwrap().get(path) {
        if *t == mtime { return r.clone(); }
    }
    let result = if mtime.is_none() { Err("file not found".to_string()) } else { load_meshes(path) };
    let mut cache = cache.lock().unwrap();
    if cache.len() > 3 { cache.clear(); }
    cache.insert(path.to_string(), (mtime, result.clone()));
    result
}

/// Axes, unit and joint kinds as the file has them, from its raw
/// GlobalSettings: the loader's own conversion does not change those.
fn file_space(scene: &ufbx::Scene, order: &[usize]) -> FileSpace {
    let props = &scene.settings.props;
    let int = |name: &str, default: i32| props.props.iter().find(|p| p.name.as_ref() as &str == name)
        .map(|p| p.value_int as i32).unwrap_or(default);
    let real = |name: &str, default: f64| props.props.iter().find(|p| p.name.as_ref() as &str == name)
        .map(|p| p.value_vec4.x as f64).unwrap_or(default);
    let mut space = FileSpace::new(
        (int("UpAxis", 1), int("UpAxisSign", 1)),
        (int("FrontAxis", 2), int("FrontAxisSign", 1)),
        (int("CoordAxis", 0), int("CoordAxisSign", 1)),
        real("UnitScaleFactor", 1.0),
    );
    space.root_joints = order.iter().map(|i| &scene.nodes[*i])
        .filter(|n| n.bone.as_ref().map_or(false, |b| b.is_root))
        .map(|n| n.element.name.to_string()).collect();
    space.mesh_name = scene.meshes.iter().find(|m| !m.skin_deformers.is_empty())
        .and_then(|m| m.element.instances.first().map(|n| n.element.name.to_string()))
        .unwrap_or_default();
    space
}

/// Ask `load_fbx` for this take to read the skeleton and skin with no
/// animation: quick, for the rest pose of a long take.
pub const NO_TAKE: u32 = u32::MAX;

pub fn load_fbx(path: &str, take: u32) -> Result<LoadedFbx, String> {
    let opts = ufbx::LoadOpts {
        ignore_embedded:    true,
        ignore_animation:   take == NO_TAKE,
        target_axes:        ufbx::CoordinateAxes::right_handed_y_up(),
        target_unit_meters: 1.0,
        space_conversion:   ufbx::SpaceConversion::AdjustTransforms,
        ..Default::default()
    };
    let scene = ufbx::load_file(path, opts)
        .map_err(|e| format!("{}", e.description.as_ref() as &str))?;

    // ── Which nodes become joints ────────────────────────────────────────────
    // Bones and every ancestor of a bone. A file with no bones at all (marker
    // or null-only capture) keeps every node. The scene root is never a joint.
    let n = scene.nodes.len();
    let any_bone = scene.nodes.iter().any(|nd| nd.bone.is_some());
    let mut keep = vec![!any_bone; n];
    if any_bone {
        for nd in scene.nodes.iter().filter(|nd| nd.bone.is_some()) {
            let mut cur: Option<&ufbx::Node> = Some(nd);
            while let Some(c) = cur {
                let i = c.element.typed_id as usize;
                if i >= n || keep[i] { break; }
                keep[i] = true;
                cur = c.parent.as_deref();
            }
        }
    }
    for nd in scene.nodes.iter() {
        if nd.is_root || nd.is_geometry_transform_helper || nd.is_scale_helper {
            let i = nd.element.typed_id as usize;
            if i < n { keep[i] = false; }
        }
    }

    // Hierarchy order: each joint is followed by its whole subtree.
    let mut order: Vec<usize> = vec![];
    let mut stack: Vec<&ufbx::Node> = vec![&scene.root_node];
    while let Some(nd) = stack.pop() {
        let i = nd.element.typed_id as usize;
        if i < n && keep[i] { order.push(i); }
        for c in nd.children.iter().rev() { stack.push(c); }
    }
    let joint_of: HashMap<usize, usize> =
        order.iter().enumerate().map(|(j, i)| (*i, j)).collect();

    let joints: Vec<Joint> = order.iter().map(|i| {
        let nd = &scene.nodes[*i];
        let mut parent = None;
        let mut cur = nd.parent.as_deref();
        while let Some(p) = cur {
            if let Some(j) = joint_of.get(&(p.element.typed_id as usize)) { parent = Some(*j); break; }
            cur = p.parent.as_deref();
        }
        // Rotation the rig shows when its animated rotation is zero: the
        // rest rotation with the node's own Euler rotation taken back out.
        // Exact when the node has no post-rotation.
        let rest  = to_transform(&nd.local_transform);
        let euler = ufbx::euler_to_quat(nd.euler_rotation, nd.rotation_order);
        let euler = Quat::from_xyzw(euler.x as f32, euler.y as f32, euler.z as f32, euler.w as f32);
        let zero  = (rest.rotation * euler.inverse()).normalize();
        Joint {
            name:     nd.element.name.to_string(),
            parent,
            rest,
            is_bone:  nd.bone.is_some(),
            zero_rot: if zero.angle_between(Quat::IDENTITY) < 1e-4 { Quat::IDENTITY } else { zero },
        }
    }).collect();

    if joints.is_empty() {
        return Err("no transform nodes in file".into());
    }

    let skin = read_skin(&scene, &joint_of, &order).map(Arc::new);
    let space = Some(Arc::new(file_space(&scene, &order)));

    let p    = std::path::Path::new(path);
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let dir  = p.parent().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();

    // ── Rate ─────────────────────────────────────────────────────────────────
    let rate = FrameRate::from_fps(scene.settings.frames_per_second);
    let drop_frame = matches!(
        scene.settings.time_mode,
        ufbx::TimeMode::NtscDropFrame | ufbx::TimeMode::E30FpsDrop
    ) && rate.supports_drop_frame();
    let fps = rate.fps();

    // ── Take ─────────────────────────────────────────────────────────────────
    let takes: Vec<String> = scene.anim_stacks.iter()
        .map(|s| s.element.name.to_string())
        .collect();

    let anim = if scene.anim_stacks.is_empty() {
        AnimData {
            name:        "(no animation)".into(),
            tracks:      Arc::new(vec![Vec::new(); joints.len()]),
            joints,
            rate,
            drop_frame,
            start_frame: 0,
            frames:      1,
            source:      stem.clone(),
            source_dir:  dir.clone(),
            subject:     String::new(),
            skin:        skin.clone(),
            space:       space.clone(),
            human:       None,
        }
    } else {
        let stack  = &scene.anim_stacks[(take as usize).min(scene.anim_stacks.len() - 1)];
        let begin  = stack.time_begin;
        let end    = stack.time_end.max(begin);
        let frames = ((end - begin) * fps).round() as usize + 1;

        let tracks: Vec<Track> = order.iter().map(|i| {
            let nd = &scene.nodes[*i];
            (0..frames).map(|f| {
                let t = begin + f as f64 / fps;
                to_transform(&ufbx::evaluate_transform(&stack.anim, nd, t))
            }).collect()
        }).collect();

        AnimData {
            name:        stack.element.name.to_string(),
            joints,
            rate,
            drop_frame,
            start_frame: (begin * fps).round() as i64,
            frames,
            tracks:      Arc::new(tracks),
            source:      stem.clone(),
            source_dir:  dir.clone(),
            subject:     String::new(),
            skin:        skin.clone(),
            space:       space.clone(),
            human:       None,
        }
    };

    Ok(LoadedFbx { anim: Arc::new(anim), takes: Arc::new(takes) })
}

// ── Everything else in the file ─────────────────────────────────────────────

/// Names of the per-point skin attributes: up to four joints by name (a
/// token per point, indexed, so each name is held once) and their weights.
/// Weights go by joint name, not by index: they hold for any skeleton with
/// those names (see the Skin node).
pub const SKIN_JOINTS: [&str; 4] = ["skel:joint0", "skel:joint1", "skel:joint2", "skel:joint3"];
pub const SKIN_WEIGHTS: &str = "skel:jointWeights";

/// A camera of the file, as it stands at rest.
#[derive(Clone, Debug, PartialEq)]
pub struct CameraInfo {
    pub name: String,
    pub perspective: bool,
    pub focal_mm: f32,
    /// Film back, millimetres.
    pub film_mm: [f32; 2],
    pub near: f32,
    pub far: f32,
    pub position: Vec3,
    pub animated: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightInfo {
    pub name: String,
    pub kind: &'static str,
    pub color: [f32; 3],
    pub intensity: f32,
    pub position: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialInfo {
    pub name: String,
    pub color: [f32; 3],
    /// Texture files the material uses, by what they feed.
    pub textures: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshInfo {
    pub path: String,
    pub points: usize,
    pub polygons: usize,
    pub skinned: bool,
    pub uv_sets: usize,
    /// Blend shape channels: read and listed, not applied.
    pub shapes: Vec<String>,
}

/// What a file holds, for the Load FBX node's properties.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FbxInfo {
    pub takes: Vec<(String, f64)>,
    pub meshes: Vec<MeshInfo>,
    pub materials: Vec<MaterialInfo>,
    pub cameras: Vec<CameraInfo>,
    pub lights: Vec<LightInfo>,
    pub bones: usize,
    /// Joints in the bind pose, and joints a skin weights.
    pub bind_pose: bool,
    /// What the file has that is not read in full.
    pub not_read: Vec<String>,
}

/// The geometry of a file and what it holds.
#[derive(Clone)]
pub struct FbxScene {
    /// One packed primitive per mesh node, in its own space, placed where
    /// the file has it at rest, with normals, texture coordinates, colours,
    /// its material and its skin weights.
    pub prims: Arc<Vec<crate::types::Prim>>,
    pub info: Arc<FbxInfo>,
}

fn safe_name(s: &str) -> String {
    let out: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if out.is_empty() { "node".into() } else if out.as_bytes()[0].is_ascii_digit() { format!("_{out}") } else { out }
}

/// The node's place in the hierarchy, as a path.
fn node_path(node: &ufbx::Node) -> String {
    let mut names = vec![];
    let mut cur = Some(node);
    while let Some(n) = cur {
        if n.is_root { break; }
        names.push(safe_name(&n.element.name));
        cur = n.parent.as_deref();
    }
    names.reverse();
    format!("/{}", names.join("/"))
}

fn texture_path(t: &ufbx::Texture, dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let abs = std::path::PathBuf::from(t.absolute_filename.as_ref() as &str);
    if !abs.as_os_str().is_empty() && abs.is_file() { return Some(abs); }
    let rel = dir.join(t.relative_filename.as_ref() as &str);
    if !(t.relative_filename.as_ref() as &str).is_empty() && rel.is_file() { return Some(rel); }
    let file = dir.join(std::path::Path::new(t.filename.as_ref() as &str).file_name()?);
    file.is_file().then_some(file)
}

fn look_of(m: &ufbx::Material, dir: &std::path::Path) -> crate::types::Look {
    let pick = |a: &ufbx::MaterialMap, b: &ufbx::MaterialMap| if a.has_value { a.value_vec4 } else { b.value_vec4 };
    let c = pick(&m.pbr.base_color, &m.fbx.diffuse_color);
    let real = |a: &ufbx::MaterialMap, d: f32| if a.has_value { a.value_vec4.x as f32 } else { d };
    let map = |a: &ufbx::MaterialMap, b: &ufbx::MaterialMap| a.texture.as_ref().or(b.texture.as_ref()).and_then(|t| texture_path(t, dir));
    crate::types::Look {
        name: m.element.name.to_string(),
        color: [c.x as f32, c.y as f32, c.z as f32],
        roughness: real(&m.pbr.roughness, 0.5),
        metallic: real(&m.pbr.metalness, 0.0),
        opacity: real(&m.pbr.opacity, 1.0).clamp(0.0, 1.0),
        color_map: map(&m.pbr.base_color, &m.fbx.diffuse_color),
        emissive_map: map(&m.pbr.emission_color, &m.fbx.emission_color),
        cutout: false,
    }
}

/// A mesh node as a packed primitive, with its skin weights by joint name.
fn mesh_prim(mesh: &ufbx::Mesh, node: &ufbx::Node, dir: &std::path::Path) -> Option<crate::types::Prim> {
    use crate::core::geo::{Attr, Column, Context, Geo, Role, Topology, NORMALS, ST};
    let points: Vec<[f32; 3]> = mesh.vertices.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect();
    // Polygons, and the file's index of each of their corners.
    let mut counts = vec![];
    let mut indices = vec![];
    let mut corners: Vec<usize> = vec![];
    for f in mesh.faces.iter() {
        let (b, n) = (f.index_begin as usize, f.num_indices as usize);
        if n < 3 || b + n > mesh.vertex_indices.len() { continue; }
        counts.push(n as u32);
        for c in b..b + n { indices.push(mesh.vertex_indices[c]); corners.push(c); }
    }
    if counts.is_empty() { return None; }
    let to_world = to_mat4(&node.geometry_to_world);
    let mut geo = Geo::from_points(points);
    geo.topology = Topology::mesh(counts, indices);
    // A mirrored node turns the winding round.
    if let Topology::Mesh { left_handed, .. } = &mut geo.topology { *left_handed = to_world.determinant() < 0.0; }
    let corner_index = |ix: &[u32]| -> Option<Vec<u32>> { corners.iter().map(|c| ix.get(*c).copied()).collect() };
    if mesh.vertex_normal.exists && !mesh.generated_normals {
        let vals: Vec<[f32; 3]> = mesh.vertex_normal.values.iter().map(|v| [v.x as f32, v.y as f32, v.z as f32]).collect();
        if let Some(ix) = corner_index(&mesh.vertex_normal.indices) {
            geo.set(Context::Corner, NORMALS, Attr::indexed(Column::Vec3(Arc::new(vals)), ix, Role::Normal));
        }
    }
    if mesh.vertex_uv.exists {
        let vals: Vec<[f32; 2]> = mesh.vertex_uv.values.iter().map(|v| [v.x as f32, v.y as f32]).collect();
        if let Some(ix) = corner_index(&mesh.vertex_uv.indices) {
            geo.set(Context::Corner, ST, Attr::indexed(Column::Vec2(Arc::new(vals)), ix, Role::TexCoord));
        }
    }
    if mesh.vertex_color.exists {
        let vals: Vec<[f32; 3]> = mesh.vertex_color.values.iter().map(|v| [v.x as f32, v.y as f32, v.z as f32]).collect();
        if let Some(ix) = corner_index(&mesh.vertex_color.indices) {
            geo.set(Context::Corner, "displayColor", Attr::indexed(Column::Vec3(Arc::new(vals)), ix, Role::Color));
        }
    }
    // Skin weights: the four heaviest joints of each point, by name.
    if let Some(skin) = mesh.skin_deformers.iter().next() {
        if skin.vertices.len() >= mesh.num_vertices {
            let names: Vec<Arc<str>> = skin.clusters.iter()
                .map(|c| Arc::from(c.bone_node.as_ref().map(|n| n.element.name.to_string()).unwrap_or_default().as_str())).collect();
            let mut joint_ix: [Vec<u32>; 4] = Default::default();
            let mut weights: Vec<[f32; 4]> = Vec::with_capacity(mesh.num_vertices);
            for v in 0..mesh.num_vertices {
                let sv = &skin.vertices[v];
                let mut top = [(0u32, 0.0f32); 4];
                for k in sv.weight_begin as usize..(sv.weight_begin + sv.num_weights) as usize {
                    let Some(w) = skin.weights.get(k) else { break };
                    let entry = (w.cluster_index, w.weight as f32);
                    let mut at = 4;
                    for s in (0..4).rev() { if entry.1 > top[s].1 { at = s; } }
                    if at < 4 { for s in (at + 1..4).rev() { top[s] = top[s - 1]; } top[at] = entry; }
                }
                let sum: f32 = top.iter().map(|t| t.1).sum();
                let norm = if sum > 1e-8 { 1.0 / sum } else { 0.0 };
                for s in 0..4 { joint_ix[s].push(top[s].0.min(names.len().saturating_sub(1) as u32)); }
                weights.push([top[0].1 * norm, top[1].1 * norm, top[2].1 * norm, top[3].1 * norm]);
            }
            if !names.is_empty() {
                let column = Column::Token(Arc::new(names));
                for (s, ix) in joint_ix.into_iter().enumerate() {
                    geo.set(Context::Point, SKIN_JOINTS[s], Attr { column: column.clone(), indices: Some(Arc::new(ix)), role: Role::None });
                }
                geo.set(Context::Point, SKIN_WEIGHTS, Attr::new(Column::Vec4(Arc::new(weights)), Role::None));
            }
        }
    }
    let material = mesh.materials.iter().next();
    let geo = Arc::new(geo);
    Some(crate::types::Prim {
        path: node_path(node),
        mesh: Arc::new(geo.to_mesh()),
        picked: false,
        material: material.map(|m| m.element.name.to_string()),
        look: material.map(|m| Arc::new(look_of(m, dir))),
        stage: None,
        geo: Some(geo),
        source: None,
        place: crate::types::Placement::One(to_world),
        purpose: Default::default(),
        props: Default::default(),
        above: Default::default(),
        instancing: None,
        instanced_by: None,
    })
}

fn kind_of(t: ufbx::LightType) -> &'static str {
    match t {
        ufbx::LightType::Point => "point",
        ufbx::LightType::Directional => "directional",
        ufbx::LightType::Spot => "spot",
        ufbx::LightType::Area => "area",
        ufbx::LightType::Volume => "volume",
    }
}

/// Read a file's meshes, materials, cameras and lights.
pub fn load_scene(path: &str) -> Result<FbxScene, String> {
    let opts = ufbx::LoadOpts {
        ignore_embedded:    true,
        target_axes:        ufbx::CoordinateAxes::right_handed_y_up(),
        target_unit_meters: 1.0,
        space_conversion:   ufbx::SpaceConversion::AdjustTransforms,
        ..Default::default()
    };
    let scene = ufbx::load_file(path, opts).map_err(|e| format!("{}", e.description.as_ref() as &str))?;
    let dir = std::path::Path::new(path).parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
    let mut info = FbxInfo::default();
    let mut prims = vec![];
    let mut seen = std::collections::HashMap::<String, usize>::new();
    for node in scene.nodes.iter() {
        let Some(mesh) = node.mesh.as_ref() else { continue };
        let Some(mut prim) = mesh_prim(mesh, node, &dir) else { continue };
        // Two nodes of one name get paths of their own.
        let k = seen.entry(prim.path.clone()).or_insert(0);
        *k += 1;
        if *k > 1 { prim.path = format!("{}_{}", prim.path, *k); }
        let shapes: Vec<String> = mesh.blend_deformers.iter().flat_map(|b| b.channels.iter().map(|c| c.element.name.to_string())).collect();
        info.meshes.push(MeshInfo {
            path: prim.path.clone(), points: mesh.num_vertices, polygons: mesh.num_faces,
            skinned: !mesh.skin_deformers.is_empty(), uv_sets: mesh.uv_sets.len(), shapes,
        });
        prims.push(prim);
    }
    let anim = scene.anim_stacks.first().map(|s| &s.anim);
    let moves = |node: &ufbx::Node| anim.map_or(false, |a| {
        let (t0, t1) = scene.anim_stacks.first().map(|s| (s.time_begin, s.time_end)).unwrap_or((0.0, 0.0));
        let (a0, a1) = (ufbx::evaluate_transform(a, node, t0), ufbx::evaluate_transform(a, node, t1));
        let d = |x: &ufbx::Vec3, y: &ufbx::Vec3| (x.x - y.x).abs() + (x.y - y.y).abs() + (x.z - y.z).abs();
        d(&a0.translation, &a1.translation) > 1e-6 || (a0.rotation.x - a1.rotation.x).abs() + (a0.rotation.w - a1.rotation.w).abs() > 1e-6
    });
    for node in scene.nodes.iter() {
        let at = to_mat4(&node.node_to_world).w_axis.truncate();
        if let Some(c) = node.camera.as_ref() {
            info.cameras.push(CameraInfo {
                name: node.element.name.to_string(),
                perspective: matches!(c.projection_mode, ufbx::ProjectionMode::Perspective),
                focal_mm: c.focal_length_mm as f32,
                film_mm: [(c.film_size_inch.x * 25.4) as f32, (c.film_size_inch.y * 25.4) as f32],
                near: c.near_plane as f32, far: c.far_plane as f32,
                position: at, animated: moves(node),
            });
        }
        if let Some(l) = node.light.as_ref() {
            info.lights.push(LightInfo { name: node.element.name.to_string(), kind: kind_of(l.type_),
                color: [l.color.x as f32, l.color.y as f32, l.color.z as f32], intensity: l.intensity as f32, position: at });
        }
    }
    for m in scene.materials.iter() {
        let look = look_of(m, &dir);
        let mut textures = vec![];
        for t in m.textures.iter() {
            let file = t.texture.filename.to_string();
            if !file.is_empty() { textures.push((t.material_prop.to_string(), file)); }
        }
        info.materials.push(MaterialInfo { name: m.element.name.to_string(), color: look.color, textures });
    }
    info.takes = scene.anim_stacks.iter().map(|s| (s.element.name.to_string(), (s.time_end - s.time_begin).max(0.0))).collect();
    info.bones = scene.bones.len();
    info.bind_pose = scene.poses.iter().any(|p| p.is_bind_pose);
    // What is listed but not used, so the panel can say so.
    let shapes: usize = info.meshes.iter().map(|m| m.shapes.len()).sum();
    if shapes > 0 { info.not_read.push(format!("{shapes} blend shape channels: listed, not applied")); }
    if !info.cameras.is_empty() { info.not_read.push("Cameras: listed, not drawn or looked through".into()); }
    if !info.lights.is_empty() { info.not_read.push("Lights: listed, not drawn or lit with".into()); }
    if !scene.nurbs_curves.is_empty() || !scene.nurbs_surfaces.is_empty() {
        info.not_read.push(format!("NURBS: {} curves, {} surfaces, not read", scene.nurbs_curves.len(), scene.nurbs_surfaces.len()));
    }
    if scene.meshes.iter().any(|m| m.uv_sets.len() > 1) { info.not_read.push("UV sets after the first: not read".into()); }
    if scene.meshes.iter().any(|m| m.materials.len() > 1) { info.not_read.push("Meshes with several materials: shown with the first".into()); }
    if !scene.cache_deformers.is_empty() { info.not_read.push(format!("{} geometry caches: not read", scene.cache_deformers.len())); }
    if scene.anim_stacks.iter().any(|s| s.layers.len() > 1) { info.not_read.push("Animation layers: baked together".into()); }
    Ok(FbxScene { prims: Arc::new(prims), info: Arc::new(info) })
}

type SceneEntry = (Option<SystemTime>, Result<FbxScene, String>);
static SCENES: OnceLock<Mutex<HashMap<String, SceneEntry>>> = OnceLock::new();

/// `load_scene`, kept until the file changes.
pub fn load_scene_cached(path: &str) -> Result<FbxScene, String> {
    if path.is_empty() { return Err("no file set".into()); }
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let cache = SCENES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some((t, r)) = cache.lock().unwrap().get(path) {
        if *t == mtime { return r.clone(); }
    }
    let result = if mtime.is_none() { Err("file not found".to_string()) } else { load_scene(path) };
    let mut cache = cache.lock().unwrap();
    if cache.len() > 3 { cache.clear(); }
    cache.insert(path.to_string(), (mtime, result.clone()));
    result
}

// ── Cache ────────────────────────────────────────────────────────────────────
// Keyed by path + take, invalidated when the file's modification time changes.

type CacheKey   = (String, u32);
type CacheEntry = (Option<SystemTime>, Result<LoadedFbx, String>);

static CACHE: OnceLock<Mutex<HashMap<CacheKey, CacheEntry>>> = OnceLock::new();

pub fn load_fbx_cached(path: &str, take: u32) -> Result<LoadedFbx, String> {
    if path.is_empty() { return Err("no file set".into()); }
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let key   = (path.to_string(), take);
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Some((t, r)) = cache.lock().unwrap().get(&key) {
        if *t == mtime { return r.clone(); }
    }

    let result = if mtime.is_none() {
        Err("file not found".to_string())
    } else {
        let r = load_fbx(path, take);
        if let Err(e) = &r { eprintln!("[LoadFbx] failed to load '{path}': {e}"); }
        r
    };

    let mut cache = cache.lock().unwrap();
    if cache.len() > 3 { cache.clear(); }   // clips are large
    cache.insert(key, (mtime, result.clone()));
    result
}

/// FBX files directly inside a folder, sorted by name.
pub fn list_fbx(dir: &str) -> Vec<std::path::PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let mut files: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| p.extension().map(|x| x.to_string_lossy().eq_ignore_ascii_case("fbx")).unwrap_or(false))
        .collect();
    files.sort();
    files
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal ASCII FBX: Z-up, centimetres, 30 fps, two bones, one take that
    /// starts at 01:00:00:00 and moves the root 30 cm along X over one second.
    const FBX: &str = r#"; FBX 7.4.0 project file
FBXHeaderExtension:  {
	FBXHeaderVersion: 1003
	FBXVersion: 7400
}
GlobalSettings:  {
	Version: 1000
	Properties70:  {
		P: "UpAxis", "int", "Integer", "",2
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",1
		P: "FrontAxisSign", "int", "Integer", "",-1
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",1
		P: "TimeMode", "enum", "", "",6
	}
}
Objects:  {
	NodeAttribute: 11, "NodeAttribute::", "LimbNode" {
		TypeFlags: "Skeleton"
	}
	NodeAttribute: 12, "NodeAttribute::", "LimbNode" {
		TypeFlags: "Skeleton"
	}
	Model: 1, "Model::Hips", "LimbNode" {
		Version: 232
		Properties70:  {
			P: "Lcl Translation", "Lcl Translation", "", "A",0,0,100
		}
	}
	Model: 2, "Model::rig:Spine", "LimbNode" {
		Version: 232
		Properties70:  {
			P: "Lcl Translation", "Lcl Translation", "", "A",0,0,20
		}
	}
	AnimationStack: 100, "AnimStack::Take 001", "" {
		Properties70:  {
			P: "LocalStart", "KTime", "Time", "",166270168800000
			P: "LocalStop", "KTime", "Time", "",166316354958000
			P: "ReferenceStart", "KTime", "Time", "",166270168800000
			P: "ReferenceStop", "KTime", "Time", "",166316354958000
		}
	}
	AnimationLayer: 101, "AnimLayer::BaseLayer", "" {
	}
	AnimationCurveNode: 200, "AnimCurveNode::T", "" {
		Properties70:  {
			P: "d|X", "Number", "", "A",0
			P: "d|Y", "Number", "", "A",0
			P: "d|Z", "Number", "", "A",100
		}
	}
	AnimationCurve: 300, "AnimCurve::", "" {
		Default: 0
		KeyVer: 4008
		KeyTime: *2 {
			a: 166270168800000,166316354958000
		}
		KeyValueFloat: *2 {
			a: 0,30
		}
		KeyAttrFlags: *1 {
			a: 4
		}
		KeyAttrDataFloat: *4 {
			a: 0,0,0,0
		}
		KeyAttrRefCount: *1 {
			a: 2
		}
	}
}
Connections:  {
	C: "OO",1,0
	C: "OO",2,1
	C: "OO",11,1
	C: "OO",12,2
	C: "OO",101,100
	C: "OO",200,101
	C: "OP",200,1, "Lcl Translation"
	C: "OP",300,200, "d|X"
}
"#;

    #[test]
    fn loads_skeleton_take_and_timecode() {
        let path = std::env::temp_dir().join("xms_fbx_loader_test.fbx");
        std::fs::write(&path, FBX).unwrap();
        let loaded = load_fbx(path.to_str().unwrap(), 0).unwrap();
        let a = &loaded.anim;

        assert_eq!(*loaded.takes, vec!["Take 001".to_string()]);
        assert_eq!(a.joints.len(), 2);
        assert_eq!(a.joints[0].name, "Hips");
        assert_eq!(a.joints[1].name, "rig:Spine");
        assert_eq!(a.joints[1].parent, Some(0));

        assert_eq!(a.rate, FrameRate::new(30, 1));
        assert_eq!(a.frames, 31);
        assert_eq!(a.timecode(a.start_frame).to_string(), "01:00:00:00");
        assert_eq!(a.timecode(a.end_frame()).to_string(), "01:00:01:00");

        // Z-up centimetres in the file, Y-up metres here.
        let close = |a: Vec3, b: Vec3| (a - b).length() < 1e-4;
        let first = a.world_pose(0);
        let last  = a.world_pose(30);
        let mid   = a.world_pose(15);
        assert!(close(first[0].w_axis.truncate(), Vec3::new(0.0, 1.0, 0.0)), "{:?}", first[0].w_axis);
        assert!(close(first[1].w_axis.truncate(), Vec3::new(0.0, 1.2, 0.0)), "{:?}", first[1].w_axis);
        assert!(close(mid[0].w_axis.truncate(),   Vec3::new(0.15, 1.0, 0.0)), "{:?}", mid[0].w_axis);
        assert!(close(last[1].w_axis.truncate(),  Vec3::new(0.3, 1.2, 0.0)), "{:?}", last[1].w_axis);
    }

    /// The studio example: two meshes with their materials and UVs, a
    /// camera that dollies in, a point light.
    #[test]
    fn reads_meshes_materials_cameras_and_lights() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/fbx/studio.fbx");
        let s = load_scene(path).unwrap();
        assert_eq!(s.prims.len(), 2);
        let crate_ = s.prims.iter().find(|p| p.path == "/Crate").unwrap();
        let g = crate_.geo.as_ref().unwrap();
        g.validate().unwrap();
        assert_eq!((g.point_count(), g.topology.primitive_count()), (8, 6));
        assert!(g.attr(crate::core::geo::Context::Corner, crate::core::geo::ST).is_some());
        assert_eq!(crate_.material.as_deref(), Some("Wood"));
        assert!((crate_.look.as_ref().unwrap().color[0] - 0.62).abs() < 1e-3);
        // Placed where the file has it, the points left as written.
        let crate::types::Placement::One(m) = crate_.place else { panic!() };
        assert!((m.w_axis.truncate() - Vec3::new(0.8, 0.0, -0.4)).length() < 1e-4, "{:?}", m.w_axis);
        assert!(g.bounds().unwrap().1[1] > 0.99);
        let i = &s.info;
        assert_eq!(i.materials.len(), 2);
        assert_eq!(i.cameras.len(), 1);
        let c = &i.cameras[0];
        assert_eq!(c.name, "ShotCam");
        assert!(c.perspective && (c.focal_mm - 35.0).abs() < 1e-3 && c.animated);
        assert!((c.film_mm[0] - 36.0).abs() < 0.1, "{:?}", c.film_mm);
        assert_eq!(i.lights.len(), 1);
        assert_eq!(i.lights[0].kind, "point");
        assert!((i.lights[0].position - Vec3::new(2.0, 3.0, 2.0)).length() < 1e-4);
        assert_eq!(i.takes.len(), 1);
        assert!((i.takes[0].1 - 2.0).abs() < 1e-6);
        assert!(i.not_read.iter().any(|n| n.starts_with("Cameras")));
    }

    /// A skeleton in centimetres scaled down to metres carries 0.0001 on each
    /// axis: its determinant is 1e-12, and it inverts all the same.
    #[test]
    fn a_skeleton_in_small_units_still_inverts() {
        let m = Mat4::from_scale(Vec3::splat(1e-4)) * Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        assert!(m.determinant().abs() <= 1e-12);
        assert!(invertible(&m));
        assert!(!invertible(&Mat4::ZERO));
    }

    /// Simon's case: the example take's actor, named "Skeleton 001_…", put on
    /// the 342-joint character of the Body Collide take. Before, the actor's
    /// parts were not read and the character was taken in its first frame
    /// (sitting in the car); its feet ended up half a metre off the floor.
    #[test]
    fn the_example_take_retargets_onto_the_character_in_its_bind_pose() {
        let (Some(take), Some(who)) = (crate::examples::existing(crate::examples::TAKE_FBX), crate::examples::existing(crate::examples::CHARACTER_FBX)) else { return };
        let take = load_fbx(&take, 0).unwrap().anim;
        let actor = take.split(take.character_roots()[0]);
        let character = load_fbx(&who, NO_TAKE).unwrap().anim.bind_clip();
        use crate::core::human::{Human, Slot};
        assert_eq!(Human::of(&actor).found(), 20);
        let (out, _) = actor.retargeted(&character);
        let h = Human::of(&out);
        let ha = Human::of(&actor);
        for f in [0usize, 200, 500] {
            let (w, wa) = (out.world_pose(f), actor.world_pose(f));
            for s in [Slot::LeftFoot, Slot::RightFoot] {
                let y = w[h.get(s).unwrap()].w_axis.y;
                assert!(y > 0.0 && y < 0.2, "frame {f} {s:?} at {y}");
            }
            // The character stands where the actor does.
            let (a, b) = (w[h.get(Slot::Hips).unwrap()].w_axis.truncate(), wa[ha.get(Slot::Hips).unwrap()].w_axis.truncate());
            assert!((a.x - b.x).abs() < 0.15 && (a.z - b.z).abs() < 0.2, "frame {f}: {a} against {b}");
        }
        // Reach then puts the feet where the actor's are, scaled.
        let (_, r) = crate::core::reach::reach(&out, &actor, &Default::default());
        assert!(r.miss_after < 0.01, "{r:?}");
    }

    #[test]
    fn missing_file_is_an_error() {
        assert!(load_fbx_cached("/nonexistent/take.fbx", 0).is_err());
        assert!(load_fbx_cached("", 0).is_err());
    }
}
