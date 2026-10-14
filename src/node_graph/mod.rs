pub mod nodes;
pub mod ui;

use std::collections::HashMap;
use crate::types::{BodyView, ConnectionId, EvalResult, MeshData, NodeId, NodeType, IceGraphId};
use nodes::evaluate_node_type;

/// Results of one evaluation pass, per node output socket.
pub type EvalCache = HashMap<(NodeId, usize), Option<EvalResult>>;

/// What the graph has cooked, kept between evaluations, as Houdini keeps a
/// node's result until the node or something above it changes. Found by a
/// key made from the node's parameters and the keys of what is wired into
/// it (`NodeGraphState::cook_key`), so an output is cooked again only when
/// something it depends on changed: moving the playhead, selecting a node or
/// redrawing the window cooks nothing. Copies of the graph (undo steps, a
/// batch writing a folder) share it; results are shared, not copied.
#[derive(Clone, Default)]
pub struct Cooked(std::sync::Arc<std::sync::Mutex<CookedInner>>);

#[derive(Default)]
struct CookedInner {
    entries: HashMap<u64, (Option<EvalResult>, usize, u64)>,
    bytes:   usize,
    clock:   u64,
}

impl Cooked {
    /// Most results kept, and about how much memory they may hold.
    const MAX_ENTRIES: usize = 256;
    const MAX_BYTES:   usize = 3 << 30;

    fn get(&self, key: u64) -> Option<Option<EvalResult>> {
        let mut c = self.0.lock().ok()?;
        c.clock += 1;
        let now = c.clock;
        let e = c.entries.get_mut(&key)?;
        e.2 = now;
        Some(e.0.clone())
    }

    fn put(&self, key: u64, result: &Option<EvalResult>) {
        let Ok(mut c) = self.0.lock() else { return };
        let size = result.as_ref().map(approx_bytes).unwrap_or(0);
        c.clock += 1;
        let now = c.clock;
        if let Some(old) = c.entries.insert(key, (result.clone(), size, now)) { c.bytes -= old.1; }
        c.bytes += size;
        // The least recently used go first.
        while c.entries.len() > Self::MAX_ENTRIES || (c.bytes > Self::MAX_BYTES && c.entries.len() > 1) {
            let Some((&old, _)) = c.entries.iter().filter(|(k, _)| **k != key).min_by_key(|(_, e)| e.2) else { break };
            if let Some(e) = c.entries.remove(&old) { c.bytes -= e.1; }
        }
    }

    /// How many results are kept (for tests).
    #[cfg(test)]
    pub fn len(&self) -> usize { self.0.lock().map(|c| c.entries.len()).unwrap_or(0) }
}

/// About how much memory a result holds, for the cache's budget.
fn approx_bytes(r: &EvalResult) -> usize {
    let mesh = |m: &MeshData| m.vertices.len() * 12 + m.indices.len() * 4 + m.normals.len() * 12
        + m.uvs.len() * 8 + m.polys.len() * 32 + m.points.len() * 12;
    match r {
        EvalResult::Single(m) => mesh(m),
        EvalResult::Named(prims) => prims.iter().map(|p| mesh(&p.mesh)).sum::<usize>() + prims.len() * 256,
        EvalResult::Anim(a) => a.tracks.iter().map(|t| t.len()).sum::<usize>() * 48
            + a.skin.as_ref().map(|s| s.positions.len() * 64).unwrap_or(0),
    }
}

/// When a file was last changed, and its size: a file written again is
/// read again.
fn file_stamp(path: &str) -> Option<(std::time::SystemTime, u64)> {
    let m = std::fs::metadata(path.trim()).ok()?;
    Some((m.modified().ok()?, m.len()))
}

// ============================================================================
// GRAPH DATA STRUCTURES
// ============================================================================

#[derive(Clone)]
pub struct InputSocket {
    pub name:             String,
    pub connected_output: Option<(NodeId, usize)>,
}

#[derive(Clone)]
pub struct OutputSocket {
    pub name: String,
}

#[derive(Clone)]
pub struct GraphNode {
    pub id:        NodeId,
    pub name:      String,
    pub node_type: NodeType,
    pub position:  egui::Pos2,
    pub inputs:    Vec<InputSocket>,
    pub outputs:   Vec<OutputSocket>,
    /// A bypassed node hands its first input on unchanged.
    pub bypassed:  bool,
    /// Primitive Locations (the Prims Location tab): the primitives the
    /// operator works on. Empty: every primitive (`core::locations`).
    pub locations: Vec<String>,
}

#[derive(Clone)]
pub struct Connection {
    pub id:          ConnectionId,
    pub from_node:   NodeId,
    pub from_output: usize,
    pub to_node:     NodeId,
    pub to_input:    usize,
}

// ============================================================================
// NODE GRAPH STATE
// ============================================================================

#[derive(Clone)]
pub struct NodeGraphState {
    pub nodes:               Vec<GraphNode>,
    pub connections:         Vec<Connection>,
    pub next_node_id:        usize,
    pub next_connection_id:  usize,
    pub selected_node:       Option<NodeId>,
    pub dragging_node:       Option<NodeId>,
    pub drag_offset:         egui::Vec2,
    pub connecting_from:     Option<(NodeId, usize)>,
    pub pan_offset:          egui::Vec2,
    pub renaming_node:       Option<NodeId>,
    pub rename_buffer:       String,
    pub tab_menu_screen_pos: Option<egui::Pos2>,
    pub tab_menu_canvas_pos: Option<egui::Pos2>,
    pub zoom:                f32,
    pub selected_nodes:      Vec<NodeId>,
    pub marquee_start:       Option<egui::Pos2>,
    pub selected_connection: Option<ConnectionId>,
    pub graph_version:       u64,
    /// The operator whose output is the scene: what the Viewer draws, the
    /// Scene Explorer lists and the Primitive Inspector shows. One operator
    /// always has it while the graph has any (see `repair_view_flag`).
    pub view_flag:           Option<NodeId>,
    /// Which output of that operator is viewed, for operators with several
    /// (Load FBX: Clip, Geometry, Rest). Clicking the flag again moves on.
    pub view_output:         usize,
    /// Output socket the add-node menu was opened from: the new node goes
    /// under it, wired to it.
    pub menu_from:           Option<(NodeId, usize)>,
    /// Bring every node into view on the next frame.
    pub frame_request:       bool,
    /// Results kept between evaluations (see `Cooked`).
    pub cooked:              Cooked,
}

impl Default for NodeGraphState {
    fn default() -> Self {
        let mut s = Self {
            nodes: vec![], connections: vec![],
            next_node_id: 0, next_connection_id: 0,
            selected_node: None, dragging_node: None,
            drag_offset: egui::Vec2::ZERO,
            connecting_from: None, pan_offset: egui::Vec2::ZERO,
            renaming_node: None, rename_buffer: String::new(),
            tab_menu_screen_pos: None, tab_menu_canvas_pos: None,
            zoom: 1.0,
            selected_nodes: vec![],
            marquee_start:  None,
            selected_connection: None,
            graph_version: 0,
            view_flag: None,
            view_output: 0,
            menu_from: None,
            frame_request: true,
            cooked: Cooked::default(),
        };
        s
    }
}

/// Counts changes to what the graph evaluates to or shows. Systems that
/// cook the graph run when this changes, not on every frame.
#[derive(Default)]
pub struct GraphRevision(pub u64);

impl NodeGraphState {
    /// Hash of everything evaluation and display depend on: nodes with their
    /// parameters, wires, the view flag and the selection. Node positions,
    /// panning and drags in progress are left out.
    pub fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        struct W(std::collections::hash_map::DefaultHasher);
        impl std::io::Write for W {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> { self.0.write(buf); Ok(buf.len()) }
            fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
        }
        let mut w = W(Default::default());
        for n in &self.nodes {
            n.id.0.hash(&mut w.0);
            n.name.hash(&mut w.0);
            let _ = serde_json::to_writer(&mut w, &n.node_type);
            for i in &n.inputs { i.connected_output.map(|(id, out)| (id.0, out)).hash(&mut w.0); }
            n.outputs.len().hash(&mut w.0);
            n.bypassed.hash(&mut w.0);
            n.locations.hash(&mut w.0);
        }
        for c in &self.connections { (c.from_node.0, c.from_output, c.to_node.0, c.to_input).hash(&mut w.0); }
        self.view_flag.map(|n| n.0).hash(&mut w.0);
        self.view_output.hash(&mut w.0);
        self.selected_node.map(|n| n.0).hash(&mut w.0);
        for n in &self.selected_nodes { n.0.hash(&mut w.0); }
        w.0.finish()
    }

    // ✅ Increment version whenever graph changes
    fn mark_dirty(&mut self) {
        self.graph_version = self.graph_version.wrapping_add(1);
    }

    pub fn add_node(&mut self, name: String, node_type: NodeType, pos: egui::Pos2) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        let (inputs, outputs) = Self::create_sockets(&node_type);
        self.nodes.push(GraphNode { id, name, node_type, position: pos, inputs, outputs, bypassed: false, locations: vec![] });
        // The first operator of a graph is viewed: there is always one.
        if self.view_flag.is_none() { self.view_flag = Some(id); self.view_output = 0; }
        self.mark_dirty();
        id
    }

    pub fn create_sockets(t: &NodeType) -> (Vec<InputSocket>, Vec<OutputSocket>) {
        let i = |n: &str| InputSocket  { name: n.into(), connected_output: None };
        let o = |n: &str| OutputSocket { name: n.into() };
        match t {
            NodeType::CreateCube { .. }
            | NodeType::CreateSphere { .. }
            | NodeType::CreateGrid { .. }
            | NodeType::LoadFbxMesh { .. }
            | NodeType::LoadUsd { .. }      => (vec![], vec![o("Mesh")]),
            NodeType::LoadFbx { .. }        => (vec![], vec![o("Clip"), o("Geometry"), o("Rest")]),
            NodeType::PickPrims { .. } | NodeType::PrunePrims { .. } | NodeType::UnpackPrims | NodeType::WriteUsd { .. }
                => (vec![i("Prims")], vec![o("Prims")]),
            NodeType::Transform { .. }      => (vec![i("Input")], vec![o("Output")]),
            NodeType::Merge { .. }          => (vec![i("A"), i("B")], vec![o("Result")]),
            NodeType::Placeholder { inputs, outputs, .. } => (
                inputs.iter().map(|n| InputSocket { name: n.clone(), connected_output: None }).collect(),
                outputs.iter().map(|n| OutputSocket { name: n.clone() }).collect(),
            ),
            NodeType::Ice { .. }         => (vec![i("Geometry"), i("Template")], vec![o("Out")]),
            NodeType::TestClip { .. }       => (vec![], vec![o("Clip")]),
            NodeType::RenameJoints { .. }
            | NodeType::TrimClip { .. }
            | NodeType::Retime { .. }
            | NodeType::SetTimecode { .. }
            | NodeType::AutoTPose { .. }
            | NodeType::FixPose { .. }
            | NodeType::ProxySkin { .. }
            | NodeType::WriteFbx { .. }
            | NodeType::MirrorClip
            | NodeType::SmoothClip { .. }
            | NodeType::InPlace { .. }
            | NodeType::TransformClip { .. }
            | NodeType::LoopClip { .. }
            | NodeType::TimeWarp { .. }
            | NodeType::PruneJoints { .. }
            | NodeType::Calamari { .. }
            | NodeType::FloorClip { .. }    => (vec![i("Clip")], vec![o("Clip")]),
            NodeType::Ragdoll { .. }        => (vec![i("Clip"), i("Collider")], vec![o("Clip")]),
            NodeType::BlendClips { .. }     => (vec![i("First"), i("Next")], vec![o("Clip")]),
            NodeType::Retarget              => (vec![i("Motion"), i("Skeleton")], vec![o("Clip")]),
            NodeType::Characterize { .. }   => (vec![i("Clip")], vec![o("Clip")]),
            NodeType::Reach { .. }          => (vec![i("Clip"), i("Goal")], vec![o("Clip")]),
            NodeType::HumanLimits { .. }    => (vec![i("Clip"), i("Reference")], vec![o("Clip")]),
            NodeType::Capture { .. }        => (vec![i("Geometry"), i("Skeleton")], vec![o("Geometry")]),
            NodeType::Skin                  => (vec![i("Geometry"), i("Rest"), i("Clip")], vec![o("Clip")]),
            NodeType::UvUnwrap { .. }
            | NodeType::UvTransform { .. }
            | NodeType::UvEdit { .. }       => (vec![i("Mesh")], vec![o("Mesh")]),
            NodeType::LoadFbxDir { .. }     => (vec![], vec![o("Clip")]),
            NodeType::EditPoly { .. }       => (vec![i("Mesh")], vec![o("Mesh")]),
            NodeType::SplitSkeleton { picks } => (
                vec![i("Clip")],
                (0..picks.len()).map(|n| OutputSocket { name: format!("Char {}", n + 1) }).collect(),
            ),
        }
    }

    pub fn add_connection(&mut self, from: NodeId, from_out: usize, to: NodeId, to_in: usize) {
        self.connections.retain(|c| !(c.to_node == to && c.to_input == to_in));
        let id = ConnectionId(self.next_connection_id);
        self.next_connection_id += 1;
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == to) {
            if let Some(inp) = n.inputs.get_mut(to_in) {
                inp.connected_output = Some((from, from_out));
            }
        }
        self.connections.push(Connection {
            id,
            from_node: from, from_output: from_out,
            to_node: to,     to_input: to_in,
        });
        self.mark_dirty();
    }

    pub fn remove_connection(&mut self, cid: ConnectionId) {
        if let Some(c) = self.connections.iter().find(|c| c.id == cid) {
            let (tn, ti) = (c.to_node, c.to_input);
            if let Some(n) = self.nodes.iter_mut().find(|n| n.id == tn) {
                if let Some(inp) = n.inputs.get_mut(ti) { inp.connected_output = None; }
            }
        }
        self.connections.retain(|c| c.id != cid);
        self.mark_dirty();
    }
    
    pub fn delete_selected(&mut self) {
        // Delete selected connection first (if any), nodes take priority if both set
        if !self.selected_nodes.is_empty() {
            let to_delete: Vec<NodeId> = self.selected_nodes.drain(..).collect();
            // The viewed operator going: the view moves to what came into it.
            let next_view = self.view_flag.filter(|v| to_delete.contains(v))
                .and_then(|v| self.nodes.iter().find(|n| n.id == v))
                .and_then(|n| n.inputs.iter().find_map(|i| i.connected_output))
                .map(|(src, _)| src)
                .filter(|src| !to_delete.contains(src));
            for nid in &to_delete {
                // Remove all connections involving this node
                let conns_to_remove: Vec<ConnectionId> = self.connections.iter()
                    .filter(|c| c.from_node == *nid || c.to_node == *nid)
                    .map(|c| c.id)
                    .collect();
                for cid in conns_to_remove { self.remove_connection(cid); }
                self.nodes.retain(|n| n.id != *nid);
            }
            if self.selected_node.map(|id| to_delete.contains(&id)).unwrap_or(false) {
                self.selected_node = None;
            }
            self.repair_view_flag(next_view);
            self.selected_connection = None;
        } else if let Some(cid) = self.selected_connection.take() {
            self.remove_connection(cid);
        }
        self.mark_dirty();
    }

    // ============================================================================
    // 👁️ NEW - VIEW FLAG SYSTEM
    // ============================================================================

    /// Put the view flag on an operator. On the operator that has it
    /// already, the view moves on to its next output, for operators with
    /// several (Load FBX: Clip, Geometry, Rest). The flag never goes off:
    /// one operator is always viewed, as Houdini's display flag.
    pub fn set_view_flag(&mut self, node_id: NodeId) {
        let outputs = self.nodes.iter().find(|n| n.id == node_id).map(|n| n.outputs.len()).unwrap_or(0);
        if self.view_flag == Some(node_id) {
            if outputs > 1 { self.view_output = (self.view_output + 1) % outputs; }
        } else {
            self.view_flag = Some(node_id);
            self.view_output = 0;
        }
    }

    /// Keep one operator viewed while the graph has any. When the viewed one
    /// is gone (deleted, or never set in a file), the view goes to `prefer`
    /// if it is there, else to the last operator nothing is wired from: the
    /// end of the last chain.
    pub fn repair_view_flag(&mut self, prefer: Option<NodeId>) {
        let exists = |id: NodeId| self.nodes.iter().any(|n| n.id == id);
        if self.view_flag.is_some_and(exists) { return; }
        let end = self.nodes.iter().rev()
            .find(|n| !self.connections.iter().any(|c| c.from_node == n.id))
            .or(self.nodes.last())
            .map(|n| n.id);
        let next = prefer.filter(|p| exists(*p)).or(end);
        self.view_flag = next;
        self.view_output = 0;
    }

    /// The output of the viewed operator that is viewed, within the outputs
    /// it has.
    pub fn viewed_output(&self) -> usize {
        let n = self.view_flag.and_then(|id| self.nodes.iter().find(|n| n.id == id)).map(|n| n.outputs.len()).unwrap_or(1);
        if self.view_output < n { self.view_output } else { 0 }
    }

    /// Lay the graph out top to bottom: each node on the row
    /// above the first node that uses it. Rows are centred on
    /// one another and a chain of single inputs comes out as a straight line.
    pub fn auto_layout(&mut self) {
        const DX: f32 = 205.0;
        const DY: f32 = 100.0;
        let n = self.nodes.len();
        if n == 0 { return; }
        let index: HashMap<NodeId, usize> = self.nodes.iter().enumerate().map(|(i, node)| (node.id, i)).collect();
        let mut parents: Vec<Vec<usize>> = vec![vec![]; n];
        let mut children: Vec<Vec<usize>> = vec![vec![]; n];
        for (i, node) in self.nodes.iter().enumerate() {
            for input in &node.inputs {
                if let Some(src) = input.connected_output.and_then(|(id, _)| index.get(&id).copied()) {
                    if src != i && !parents[i].contains(&src) { parents[i].push(src); children[src].push(i); }
                }
            }
        }
        // Row: as low as it can go, straight above the first node that
        // uses it, so a branch sits beside the chain it joins instead of
        // every source crowding the top row. The pass count bounds a loop.
        let mut above = vec![0usize; n];   // rows between a node and the bottom
        for _ in 0..n {
            let mut moved = false;
            for i in 0..n {
                let want = children[i].iter().map(|c| above[*c] + 1).max().unwrap_or(0);
                if want > above[i] && want <= n { above[i] = want; moved = true; }
            }
            if !moved { break; }
        }
        let depth = above.iter().copied().max().unwrap_or(0);
        let row: Vec<usize> = above.iter().map(|a| depth - a).collect();
        let rows = row.iter().copied().max().unwrap_or(0) + 1;
        let mut by_row: Vec<Vec<usize>> = vec![vec![]; rows];
        // Start from the order the nodes already have, left to right.
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|a, b| self.nodes[*a].position.x.total_cmp(&self.nodes[*b].position.x).then(a.cmp(b)));
        for i in order { by_row[row[i]].push(i); }

        let mut x = vec![0.0f32; n];
        for members in &by_row {
            for (k, i) in members.iter().enumerate() { x[*i] = (k as f32 - (members.len() as f32 - 1.0) * 0.5) * DX; }
        }
        // Pull each node towards the middle of its neighbours on the row
        // above, then below, keeping nodes of a row apart.
        let settle = |members: &mut Vec<usize>, x: &mut Vec<f32>, toward: &Vec<Vec<usize>>| {
            let want: HashMap<usize, f32> = members.iter().map(|i| {
                let near = &toward[*i];
                (*i, if near.is_empty() { x[*i] } else { near.iter().map(|j| x[*j]).sum::<f32>() / near.len() as f32 })
            }).collect();
            members.sort_by(|a, b| want[a].total_cmp(&want[b]).then(a.cmp(b)));
            let mut placed: Vec<f32> = vec![];
            for i in members.iter() {
                let at = placed.last().map(|p| want[i].max(p + DX)).unwrap_or(want[i]);
                placed.push(at);
            }
            // Spread evenly about where the row wanted to be.
            let shift = members.iter().zip(&placed).map(|(i, p)| want[i] - p).sum::<f32>() / members.len().max(1) as f32;
            for (i, p) in members.iter().zip(&placed) { x[*i] = p + shift; }
        };
        for _ in 0..3 {
            for r in 1..rows { settle(&mut by_row[r], &mut x, &parents); }
            for r in (0..rows.saturating_sub(1)).rev() { settle(&mut by_row[r], &mut x, &children); }
        }
        for r in 1..rows { settle(&mut by_row[r], &mut x, &parents); }
        let left = x.iter().copied().fold(f32::MAX, f32::min);
        for (i, node) in self.nodes.iter_mut().enumerate() {
            node.position = egui::pos2((x[i] - left + 40.0).round(), row[i] as f32 * DY + 20.0);
        }
        self.frame_request = true;
    }

    /// Paths of the packed primitives on an output socket, with whether each
    /// is picked. Kept until the graph changes: the properties panel asks
    /// every frame.
    pub fn eval_packed(
        &self, id: NodeId, output: usize,
        eval_subnet: &impl Fn(IceGraphId, &crate::core::geo::Geo, Option<&crate::core::geo::Geo>) -> crate::core::geo::Geo,
    ) -> Option<Vec<(String, bool)>> {
        type Kept = (u64, usize, usize, Option<Vec<(String, bool)>>);
        static LAST: std::sync::Mutex<Option<Kept>> = std::sync::Mutex::new(None);
        let key = self.content_hash();
        if let Ok(last) = LAST.lock() {
            if let Some((k, i, o, value)) = &*last { if *k == key && *i == id.0 && *o == output { return value.clone(); } }
        }
        let value = match self.eval_node_out(id, output, &mut HashMap::new(), eval_subnet) {
            Some(EvalResult::Named(prims)) => Some(prims.iter().map(|p| (p.path.clone(), p.picked)).collect()),
            _ => None,
        };
        if let Ok(mut last) = LAST.lock() { *last = Some((key, id.0, output, value.clone())); }
        value
    }

    pub fn toggle_bypass(&mut self, node_id: NodeId) {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == node_id) {
            n.bypassed = !n.bypassed;
        }
        self.mark_dirty();
    }

    pub fn has_view_flag(&self, node_id: NodeId) -> bool {
        self.view_flag == Some(node_id)
    }

    /// The operator the Viewer shows: the one with the view flag.
    pub fn get_viewport_node(&self) -> Option<NodeId> { self.view_flag }

    /// What the Viewer shows, before anything is merged: packed primitives
    /// keep their materials. The viewed output of the viewed operator.
    pub fn evaluate_for_viewport_packed(
        &self,
        eval_subnet: &impl Fn(IceGraphId, &crate::core::geo::Geo, Option<&crate::core::geo::Geo>) -> crate::core::geo::Geo,
    ) -> Option<EvalResult> {
        let id = self.view_flag?;
        self.eval_node_out(id, self.viewed_output(), &mut HashMap::new(), eval_subnet)
    }

    /// The same, as one mesh.
    pub fn evaluate_for_viewport(
        &self,
        eval_subnet: &impl Fn(IceGraphId, &crate::core::geo::Geo, Option<&crate::core::geo::Geo>) -> crate::core::geo::Geo,
    ) -> Option<MeshData> {
        self.evaluate_for_viewport_packed(eval_subnet).map(|r| r.into_mesh())
    }

    /// The operator the Viewer shows.
    pub fn display_source(&self) -> Option<NodeId> { self.view_flag }

    /// Clip produced by one node, if it produces animation. Clip operators
    /// never go through subnets, so no subnet evaluator is needed.
    pub fn eval_anim(&self, id: NodeId) -> Option<std::sync::Arc<crate::core::anim::AnimData>> {
        self.eval_anim_out(id, 0)
    }

    /// Clip on one output socket of a node.
    pub fn eval_anim_out(&self, id: NodeId, output: usize) -> Option<std::sync::Arc<crate::core::anim::AnimData>> {
        let passthrough = |_: IceGraphId, mesh: &crate::core::geo::Geo, _: Option<&crate::core::geo::Geo>| mesh.clone();
        let mut cache = HashMap::new();
        match self.eval_anim_node(id, output, &mut cache, &passthrough)? {
            EvalResult::Anim(a) => Some(a),
            _ => None,
        }
    }

    /// Like `eval_node`, but stops as soon as a branch is known not to be
    /// animation, so asking a mesh node for its clip does not cook the mesh.
    fn eval_anim_node(
        &self,
        id:          NodeId,
        output:      usize,
        cache:       &mut EvalCache,
        eval_subnet: &impl Fn(IceGraphId, &crate::core::geo::Geo, Option<&crate::core::geo::Geo>) -> crate::core::geo::Geo,
    ) -> Option<EvalResult> {
        let node = self.nodes.iter().find(|n| n.id == id)?;
        match &node.node_type {
            _ if node.bypassed => {
                let (src, out) = node.inputs.first()?.connected_output?;
                self.eval_anim_node(src, out, cache, eval_subnet)
            }
            t if t.is_anim() => self.eval_node_out(id, output, cache, eval_subnet),
            NodeType::Placeholder { .. } => {
                let (src, out) = node.inputs.get(output)?.connected_output?;
                self.eval_anim_node(src, out, cache, eval_subnet)
            }
            // Transform passes a clip on when it is given one.
            NodeType::Transform { .. } => match self.eval_node_out(id, output, cache, eval_subnet) {
                Some(r @ EvalResult::Anim(_)) => Some(r),
                _ => None,
            },
            _ => None,
        }
    }

    // Recursive bottom-up evaluator — follows the full chain.
    pub fn eval_node(
        &self,
        id:          NodeId,
        cache:       &mut EvalCache,
        eval_subnet: &impl Fn(IceGraphId, &crate::core::geo::Geo, Option<&crate::core::geo::Geo>) -> crate::core::geo::Geo,
    ) -> Option<EvalResult> {
        self.eval_node_out(id, 0, cache, eval_subnet)
    }

    /// The key an output's result is kept under: the node's parameters,
    /// whether it is bypassed, which output, and the keys of what is wired
    /// into it. Files read are keyed by when they were last written; Body
    /// Collide by the solves finished so far. None for what cannot be keyed
    /// (an ICE node, whose graph is kept elsewhere, and what comes after one).
    pub fn cook_key(&self, id: NodeId, output: usize) -> Option<u64> {
        self.cook_key_depth(id, output, 0)
    }

    fn cook_key_depth(&self, id: NodeId, output: usize, depth: usize) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        if depth > 256 { return None; }
        let node = self.nodes.iter().find(|n| n.id == id)?;
        if matches!(node.node_type, NodeType::Ice { .. }) { return None; }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        match &node.node_type {
            // Edit Poly's result is its operations on its input. The selection
            // being built, and which operation is open, change only what the
            // Viewer shows: picking polygons cooks nothing again.
            NodeType::EditPoly { ops, .. } => ("EditPoly", serde_json::to_string(ops).ok()?).hash(&mut h),
            other => serde_json::to_string(other).ok()?.hash(&mut h),
        }
        (node.bypassed, output).hash(&mut h);
        node.locations.hash(&mut h);
        match &node.node_type {
            NodeType::LoadFbx { path, .. } | NodeType::LoadFbxMesh { path } | NodeType::LoadUsd { path } => file_stamp(path).hash(&mut h),
            NodeType::LoadFbxDir { dir, index, .. } => {
                let file = crate::fbx_loader::list_fbx(dir).get(*index as usize).cloned();
                file.as_ref().and_then(|f| file_stamp(&f.to_string_lossy())).hash(&mut h);
                file.hash(&mut h);
            }
            NodeType::Ragdoll { .. } => crate::ragdoll::generation().hash(&mut h),
            _ => {}
        }
        for input in &node.inputs {
            match input.connected_output {
                Some((src, out)) => self.cook_key_depth(src, out, depth + 1)?.hash(&mut h),
                None => 0u8.hash(&mut h),
            }
        }
        Some(h.finish())
    }

    /// Evaluate one output socket of a node. Each input is taken from the
    /// output socket it is wired to.
    pub fn eval_node_out(
        &self,
        id:          NodeId,
        output:      usize,
        cache:       &mut EvalCache,
        eval_subnet: &impl Fn(IceGraphId, &crate::core::geo::Geo, Option<&crate::core::geo::Geo>) -> crate::core::geo::Geo,
    ) -> Option<EvalResult> {
        if let Some(cached) = cache.get(&(id, output)) { return cached.clone(); }
        // Cooked before, and nothing it depends on changed since.
        let key = self.cook_key(id, output);
        if let Some(kept) = key.and_then(|k| self.cooked.get(k)) {
            cache.insert((id, output), kept.clone());
            return kept;
        }
        let result = self.cook(id, output, cache, eval_subnet);
        if let Some(k) = key { self.cooked.put(k, &result); }
        result
    }

    fn cook(
        &self,
        id:          NodeId,
        output:      usize,
        cache:       &mut EvalCache,
        eval_subnet: &impl Fn(IceGraphId, &crate::core::geo::Geo, Option<&crate::core::geo::Geo>) -> crate::core::geo::Geo,
    ) -> Option<EvalResult> {
        let node = self.nodes.iter().find(|n| n.id == id)?;

        // A placeholder hands each input to the output of the same number.
        if matches!(node.node_type, NodeType::Placeholder { .. }) {
            let result = node.inputs.get(output).and_then(|s| s.connected_output)
                .and_then(|(src, out)| self.eval_node_out(src, out, cache, eval_subnet));
            cache.insert((id, output), result.clone());
            return result;
        }

        let inputs: Vec<EvalResult> = node.inputs.iter()
            .filter_map(|s| s.connected_output
                .and_then(|(src, out)| self.eval_node_out(src, out, cache, eval_subnet)))
            .collect();

        let result = if node.bypassed {
            node.inputs.first()
                .and_then(|s| s.connected_output)
                .and_then(|(src, out)| self.eval_node_out(src, out, cache, eval_subnet))
        } else if crate::core::locations::takes_locations(&node.node_type) && crate::core::locations::is_set(&node.locations) {
            // The operator works on the primitives its Prims Location tab
            // lists; what comes after it sees the picks that came in.
            use crate::core::locations;
            let first = inputs.first()?;
            match &node.node_type {
                NodeType::PrunePrims { keep, .. } => Some(locations::prune(first, &node.locations, *keep)),
                t => {
                    let mut targeted = inputs.clone();
                    targeted[0] = locations::target(first, &node.locations);
                    evaluate_node_type(t, &targeted, eval_subnet, output).map(|r| locations::restore_picks(r, first))
                }
            }
        } else {
            evaluate_node_type(&node.node_type, &inputs, eval_subnet, output)
        };
        cache.insert((id, output), result.clone());
        result
    }

    // ── The scene ───────────────────────────────────────────────────────────
    /// The scene at the view flag: the operator it is read from, its name,
    /// and what its viewed output puts out. The Viewer draws it, the Scene Explorer lists it, the Primitive
    /// Inspector shows it.
    pub fn evaluate_scene(
        &self,
        eval_subnet: &impl Fn(IceGraphId, &crate::core::geo::Geo, Option<&crate::core::geo::Geo>) -> crate::core::geo::Geo,
    ) -> Option<(NodeId, String, EvalResult)> {
        let id = self.display_source()?;
        let name = self.nodes.iter().find(|n| n.id == id)?.name.clone();
        let result = self.evaluate_for_viewport_packed(eval_subnet)?;
        Some((id, name, result))
    }

    /// Clips the viewport should draw: the viewed node's clip, or one per
    /// output when the viewed node is a Split.
    pub fn display_clips(&self) -> Vec<std::sync::Arc<crate::core::anim::AnimData>> {
        let Some(id) = self.get_viewport_node() else { return vec![] };
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else { return vec![] };
        let clips: Vec<_> = match &node.node_type {
            NodeType::SplitSkeleton { picks } =>
                (0..picks.len()).filter_map(|o| self.eval_anim_out(id, o)).collect(),
            _ => self.eval_anim_out(id, self.viewed_output()).into_iter().collect(),
        };
        // Body Collide can show the bodies it collides instead of the skin.
        let bodies = self.body_collide_node().and_then(|n| match &n.node_type {
            NodeType::Ragdoll { settings, view, .. } => Some((*view, settings.detail)), _ => None,
        });
        match bodies {
            Some((view @ (BodyView::Pieces | BodyView::Hulls), detail)) => {
                clips.into_iter().map(|c| {
                    let key = format!("bodies:{view:?}:{detail}");
                    crate::core::anim::memo(&key, Some(&c), || Some(crate::ragdoll::calamari(&c, view == BodyView::Hulls, detail))).unwrap_or(c)
                }).collect()
            }
            _ => clips,
        }
    }

    /// The Body Collide node the viewed clip comes through, if any: the
    /// viewed node itself or the nearest one upstream along clips.
    fn body_collide_node(&self) -> Option<&GraphNode> {
        let mut id = self.display_source()?;
        let mut node = self.nodes.iter().find(|n| n.id == id)?;
        for _ in 0..64 {
            if !node.node_type.passes_clips() { return None; }
            if matches!(node.node_type, NodeType::Ragdoll { .. }) && !node.bypassed { return Some(node); }
            id = node.inputs.first()?.connected_output?.0;
            node = self.nodes.iter().find(|n| n.id == id)?;
        }
        None
    }

    /// What the viewport draws of the character: set on that Body Collide node.
    pub fn body_view(&self) -> Option<BodyView> {
        match &self.body_collide_node()?.node_type { NodeType::Ragdoll { view, .. } => Some(*view), _ => None }
    }

    /// The mesh the viewed clip was kept out of, to draw with it: the
    /// collider of the Ragdoll node that is viewed, or of the nearest one
    /// upstream of the viewed node.
    pub fn display_collider(&self) -> Option<std::sync::Arc<MeshData>> {
        let node = self.body_collide_node()?;
        let (src, out) = node.inputs.get(1)?.connected_output?;
        let passthrough = |_: IceGraphId, mesh: &crate::core::geo::Geo, _: Option<&crate::core::geo::Geo>| mesh.clone();
        self.eval_node_out(src, out, &mut HashMap::new(), &passthrough).map(|r| r.shared_mesh())
    }

    /// The clip and the collider that reach a Ragdoll node.
    pub fn ragdoll_inputs(&self, id: NodeId) -> (Option<std::sync::Arc<crate::core::anim::AnimData>>, Option<std::sync::Arc<MeshData>>) {
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else { return (None, None) };
        let passthrough = |_: IceGraphId, mesh: &crate::core::geo::Geo, _: Option<&crate::core::geo::Geo>| mesh.clone();
        let mut cache = HashMap::new();
        let input = |k: usize, cache: &mut EvalCache| node.inputs.get(k).and_then(|s| s.connected_output)
            .and_then(|(src, out)| self.eval_node_out(src, out, cache, &passthrough));
        let clip = match input(0, &mut cache) { Some(EvalResult::Anim(a)) => Some(a), _ => None };
        let collider = match input(1, &mut cache) { Some(EvalResult::Anim(_)) | None => None, Some(r) => Some(r.shared_mesh()) };
        (clip, collider)
    }

    /// Make a node's sockets match its type again after its parameters
    /// changed the socket count, and drop wires to sockets that are gone.
    pub fn sync_sockets(&mut self, id: NodeId) {
        let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) else { return };
        let (inputs, outputs) = Self::create_sockets(&node.node_type);
        let same = |a: &[String], b: &[String]| a == b;
        let names_in: Vec<String> = inputs.iter().map(|s| s.name.clone()).collect();
        let had_in: Vec<String> = node.inputs.iter().map(|s| s.name.clone()).collect();
        let names_out: Vec<String> = outputs.iter().map(|s| s.name.clone()).collect();
        let had_out: Vec<String> = node.outputs.iter().map(|s| s.name.clone()).collect();
        if same(&names_in, &had_in) && same(&names_out, &had_out) { return; }
        // Names follow; wires stay on the sockets that are still there.
        let (count_in, count_out) = (inputs.len(), outputs.len());
        let kept: Vec<Option<(NodeId, usize)>> = node.inputs.iter().map(|s| s.connected_output).collect();
        node.inputs = inputs;
        for (k, s) in node.inputs.iter_mut().enumerate() { s.connected_output = kept.get(k).copied().flatten(); }
        node.outputs = outputs;
        let dead: Vec<ConnectionId> = self.connections.iter()
            .filter(|c| (c.from_node == id && c.from_output >= count_out) || (c.to_node == id && c.to_input >= count_in))
            .map(|c| c.id).collect();
        for cid in dead { self.remove_connection(cid); }
        self.mark_dirty();
    }
}
#[cfg(test)]
mod cook_tests {
    use super::*;
    use std::sync::Arc;

    fn pass(_: IceGraphId, m: &crate::core::geo::Geo, _: Option<&crate::core::geo::Geo>) -> crate::core::geo::Geo { m.clone() }

    /// A result is cooked once and handed out again until the node or
    /// something above it changes; what does not depend on the change is
    /// not cooked again.
    #[test]
    fn results_are_kept_until_what_they_depend_on_changes() {
        let mut g = NodeGraphState::default();
        let p = |y: f32| egui::pos2(0.0, y);
        let walk = g.add_node("Walk".into(), NodeType::TestClip { seconds: 0.7, fps_num: 30, fps_den: 1 }, p(0.0));
        let place = g.add_node("Place".into(), NodeType::Transform { translation: glam::Vec3::ZERO, rotation: glam::Vec3::ZERO, scale: glam::Vec3::ONE }, p(100.0));
        let cube = g.add_node("Cube".into(), NodeType::CreateCube { size: 1.0, location: String::new() }, p(200.0));
        g.add_connection(walk, 0, place, 0);

        let a = g.eval_anim(place).unwrap();
        let cube_a = g.eval_node(cube, &mut Default::default(), &pass).unwrap();
        // Asked again: the same result, not a new one.
        assert!(Arc::ptr_eq(&a, &g.eval_anim(place).unwrap()));
        assert!(g.cooked.len() >= 3);

        // A parameter changed: that node is cooked again, its input is not.
        let walk_before = g.eval_anim(walk).unwrap();
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == place) {
            if let NodeType::Transform { translation, .. } = &mut n.node_type { translation.x = 1.0; }
        }
        let b = g.eval_anim(place).unwrap();
        assert!(!Arc::ptr_eq(&a, &b));
        assert!(Arc::ptr_eq(&walk_before, &g.eval_anim(walk).unwrap()));
        // Changing it back finds the first result again.
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == place) {
            if let NodeType::Transform { translation, .. } = &mut n.node_type { translation.x = 0.0; }
        }
        assert!(Arc::ptr_eq(&a, &g.eval_anim(place).unwrap()));
        // Bypassing is a change too.
        g.nodes.iter_mut().find(|n| n.id == place).unwrap().bypassed = true;
        assert!(Arc::ptr_eq(&walk_before, &g.eval_anim(place).unwrap()));
        // The unrelated branch was never cooked again.
        let cube_b = g.eval_node(cube, &mut Default::default(), &pass).unwrap();
        assert_eq!(cube_a.prim_paths(), cube_b.prim_paths());
        // A copy of the graph (an undo step, a batch) finds the same results.
        let copy = g.clone();
        assert!(Arc::ptr_eq(&walk_before, &copy.eval_anim(walk).unwrap()));
    }

    /// Picking polygons in Edit Poly changes what the Viewer shows, not the
    /// node's result: its key stays. A new operation changes it.
    #[test]
    fn edit_poly_is_keyed_by_its_operations() {
        let mut g = NodeGraphState::default();
        let cube = g.add_node("Cube".into(), NodeType::CreateCube { size: 1.0, location: String::new() }, egui::pos2(0.0, 0.0));
        let ep = g.add_node("EditPoly".into(), NodeType::EditPoly { ops: vec![], pending: Default::default(), edit: None, auto_collapse: false }, egui::pos2(0.0, 100.0));
        g.add_connection(cube, 0, ep, 0);
        let key = g.cook_key(ep, 0).unwrap();
        let set = |g: &mut NodeGraphState, f: &dyn Fn(&mut Vec<crate::core::poly::PolyOp>, &mut crate::core::poly::PolySelection)| {
            if let Some(n) = g.nodes.iter_mut().find(|n| n.id == ep) {
                if let NodeType::EditPoly { ops, pending, .. } = &mut n.node_type { f(ops, pending); }
            }
        };
        set(&mut g, &|_, pending| pending.polys.push(2));
        assert_eq!(g.cook_key(ep, 0), Some(key));
        set(&mut g, &|ops, pending| ops.push(crate::core::poly::PolyOp::new(pending.clone(), crate::core::poly::PolyOpKind::Delete)));
        assert_ne!(g.cook_key(ep, 0), Some(key));
    }

    /// A file written again is read again; a solve finishing changes what
    /// Body Collide puts out.
    #[test]
    fn files_and_solves_are_part_of_the_key() {
        let dir = std::env::temp_dir().join("xms_cook_key_test");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("take.fbx");
        let clip = crate::core::anim::create_test_clip(0.5, crate::core::anim::FrameRate::new(30, 1));
        crate::fbx_writer::write_fbx(&file, &clip, true, false).unwrap();
        let mut g = NodeGraphState::default();
        let load = g.add_node("Load".into(), NodeType::LoadFbx { path: file.to_string_lossy().to_string(), take: 0 }, egui::pos2(0.0, 0.0));
        let body = g.add_node("Body".into(), NodeType::Ragdoll { settings: Default::default(), view: Default::default(), limits: Default::default() }, egui::pos2(0.0, 100.0));
        g.add_connection(load, 0, body, 0);
        let (k_load, k_body) = (g.cook_key(load, 0).unwrap(), g.cook_key(body, 0).unwrap());
        // Written again, longer: another key.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let longer = crate::core::anim::create_test_clip(0.8, crate::core::anim::FrameRate::new(30, 1));
        crate::fbx_writer::write_fbx(&file, &longer, true, false).unwrap();
        assert_ne!(g.cook_key(load, 0), Some(k_load));
        let k_body2 = g.cook_key(body, 0).unwrap();
        assert_ne!(k_body2, k_body);
        // A solve finished or was cleared somewhere: Body Collide is asked again.
        crate::ragdoll::clear(0x5eed_0000_0000_0001);
        assert_ne!(g.cook_key(body, 0), Some(k_body2));
    }
}
