//! The Viewer's side of the Edit Poly node: what the Viewer shows while one
//! is selected, and the state of its tools. Picking, the manipulators and
//! the overlay are in `crate::viewer::edit`.
//!
//! While an Edit Poly node is selected the viewport shows that node, whatever
//! the view flag says: the mesh after all its operations, or the mesh
//! entering the operation whose selection is being edited.

use glam::{Quat, Vec2, Vec3};

use crate::core::manip::{self, Handle, Tool};
use crate::core::poly::{PickView, PolyMesh, PolyOpKind, PolySelection};
use crate::node_graph::NodeGraphState;
use crate::types::{MeshData, NodeId, NodeType, IceGraphId};

/// Active viewport tool of the Edit Poly node, and the manipulator handle
/// under the cursor or being dragged.
#[derive(Default)]
pub struct PolyTool {
    pub tool: Tool,
    pub hot:  Option<Handle>,
}

/// The mesh and selection the viewport is editing.
pub struct Stage {
    pub node:      NodeId,
    pub mesh:      std::sync::Arc<PolyMesh>,
    pub selection: PolySelection,
    /// True while the selection of an existing operation is being edited.
    pub editing:   bool,
}

impl Stage {
    /// Where the manipulator sits and how long its handles are.
    pub fn gizmo(&self, tool: Tool, view: &PickView) -> Option<(Vec3, f32)> {
        if tool == Tool::Select || self.editing { return None; }
        let c = self.selection.centre(&self.mesh)?;
        Some((c, manip::gizmo_size(view, c)))
    }
}

/// The mesh entering an Edit Poly node, shared, with its fingerprint.
#[derive(Clone)]
pub struct InputMesh {
    pub mesh: std::sync::Arc<PolyMesh>,
    /// Fingerprint of the mesh: the cook key of what is wired in, so a big
    /// mesh is not hashed on every frame.
    pub seed: u64,
}

/// Mesh entering a node's first input. The same mesh is handed out again,
/// without being rebuilt, until what is wired in changes: the Viewer, the
/// panel and picking ask for it on every frame.
pub fn input_mesh(
    graph:       &NodeGraphState,
    id:          NodeId,
    eval_subnet: &impl Fn(IceGraphId, &crate::core::geo::Geo, Option<&crate::core::geo::Geo>) -> crate::core::geo::Geo,
) -> Option<InputMesh> {
    use std::sync::{Arc, Mutex};
    static KEPT: Mutex<Vec<InputMesh>> = Mutex::new(Vec::new());
    let node = graph.nodes.iter().find(|n| n.id == id)?;
    let (src, out) = node.inputs.first()?.connected_output?;
    // What is wired in, and which of it the node's Prims Location tab takes.
    let key = graph.cook_key(src, out).map(|k| {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (k, &node.locations).hash(&mut h);
        h.finish()
    });
    if let Some(k) = key {
        if let Some(m) = KEPT.lock().ok()?.iter().find(|m| m.seed == k) { return Some(m.clone()); }
    }
    let mut cache = std::collections::HashMap::new();
    // On packed primitives with some picked, the tools see only those: the
    // ones its Prims Location tab lists, or else what came in picked.
    let input = graph.eval_node_out(src, out, &mut cache, eval_subnet)?;
    let mesh = crate::core::locations::target(&input, &node.locations).work_mesh();
    if mesh.vertices.is_empty() { return None; }
    let mesh = Arc::new(PolyMesh::from_mesh(&mesh));
    let seed = key.unwrap_or_else(|| crate::core::poly::mesh_hash(&mesh));
    let input = InputMesh { mesh, seed };
    if key.is_some() {
        let mut kept = KEPT.lock().ok()?;
        if kept.len() >= 2 { kept.remove(0); }
        kept.push(input.clone());
    }
    Some(input)
}

/// Stage of the selected node, if it is an Edit Poly node with a mesh.
pub fn stage(
    graph:       &NodeGraphState,
    eval_subnet: &impl Fn(IceGraphId, &crate::core::geo::Geo, Option<&crate::core::geo::Geo>) -> crate::core::geo::Geo,
) -> Option<Stage> {
    let id   = graph.selected_node?;
    let node = graph.nodes.iter().find(|n| n.id == id)?;
    let NodeType::EditPoly { ops, pending, edit, .. } = &node.node_type else { return None };
    let input = input_mesh(graph, id, eval_subnet)?;
    let edit  = edit.filter(|i| *i < ops.len());
    Some(Stage {
        node: id,
        mesh: crate::core::poly::eval_cached_seeded(&input.mesh, input.seed, ops, edit.unwrap_or(ops.len())),
        editing: edit.is_some(),
        selection: match edit {
            Some(i) => ops[i].selection.clone(),
            None    => pending.clone(),
        },
    })
}

/// Selected polygons as a mesh lifted slightly off the surface, for the
/// translucent highlight.
pub fn highlight_mesh(stage: &Stage) -> Option<MeshData> {
    let mask = stage.selection.poly_mask(&stage.mesh);
    if !mask.iter().any(|m| *m) { return None; }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in &stage.mesh.verts { lo = lo.min(*v); hi = hi.max(*v); }
    let lift = (hi - lo).max_element().max(1e-3) * 0.002;
    let mut verts = vec![];
    let mut polys = vec![];
    for (p, poly) in stage.mesh.polys.iter().enumerate() {
        if !mask[p] { continue; }
        let n = stage.mesh.normal(p);
        let base = verts.len() as u32;
        verts.extend(poly.iter().map(|v| (stage.mesh.verts[*v as usize] + n * lift).to_array()));
        polys.push((base..base + poly.len() as u32).collect());
    }
    Some(MeshData::from_polys(verts, polys))
}

/// What the left mouse button is doing in the viewport.
#[derive(Default)]
pub enum Drag {
    #[default]
    Idle,
    Marquee(Vec2),
    Handle {
        handle: Handle,
        start:  Vec2,
        pivot:  Vec3,
        size:   f32,
        /// Transform operation the drag writes to, and what it held at the start.
        op:     usize,
        base:   (Vec3, Quat, Vec3),
    },
}

pub(crate) fn transform_of(kind: &PolyOpKind) -> Option<(Vec3, Quat, Vec3)> {
    match kind {
        PolyOpKind::Transform { translate, rotate, scale, .. } =>
            Some((Vec3::from_array(*translate), Quat::from_array(*rotate).normalize(), Vec3::from_array(*scale))),
        _ => None,
    }
}
