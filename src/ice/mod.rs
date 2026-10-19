//! ICE graphs: the graph inside an ICE operator, as the editor holds it
//! (nodes, wires, positions, values on unconnected inputs), and running it
//! on a primitive's geometry with the ICE engine (`xms_ice`).
//!
//! A graph ends in its **ICE Execution Ports** root (named ICEExectPorts):
//! its Execute ports run in order,
//! over every element of its context (points by default). Set Data nodes
//! wired into it write attributes; everything else is reached from them.

pub mod ui;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use xms_ice::{Input, Math, Node, Op, Value};

use crate::core::geo::{Context, Geo};
use crate::types::{ConnectionId, IceGraphId, IceGraphNodeType, NodeId};

#[derive(Default)]
pub struct GraphNavigation {
    pub current_subnet: Option<IceGraphId>,
    /// The MaterialX operator whose graph the Node Graph pane shows.
    pub materialx: Option<NodeId>,
}

#[derive(Clone)]
pub struct IceGraphInputSocket {
    pub name:             String,
    pub value_hint:       &'static str,
    pub connected_output: Option<(NodeId, usize)>,
    /// What the input takes while nothing is wired to it. `None` for
    /// Execute ports.
    pub value:            Option<Value>,
}

#[derive(Clone)]
pub struct IceGraphOutputSocket {
    pub name:       String,
    pub value_hint: &'static str,
}

#[derive(Clone)]
pub struct IceGraphNode {
    pub id:        NodeId,
    pub name:      String,
    pub node_type: IceGraphNodeType,
    pub position:  egui::Pos2,
    pub inputs:    Vec<IceGraphInputSocket>,
    pub outputs:   Vec<IceGraphOutputSocket>,
}

#[derive(Clone)]
pub struct IceGraphConnection {
    pub id:          ConnectionId,
    pub from_node:   NodeId,
    pub from_output: usize,
    pub to_node:     NodeId,
    pub to_input:    usize,
}

/// The name a new graph's root is given (its type: ICE Execution Ports).
/// A root still named as before, `ICE Tree`, is renamed when read.
pub const ROOT_NAME: &str = "ICEExectPorts";

#[derive(Clone)]
pub struct IceGraph {
    pub id:                 IceGraphId,
    pub name:               String,
    pub nodes:              Vec<IceGraphNode>,
    pub connections:        Vec<IceGraphConnection>,
    pub next_node_id:       usize,
    pub next_connection_id: usize,
    pub selected_node:      Option<NodeId>,
    pub dragging_node:      Option<NodeId>,
    pub drag_offset:        egui::Vec2,
    pub connecting_from:    Option<(NodeId, usize)>,
    pub pan_offset:         egui::Vec2,
    /// View scale of the tree: 1 is actual size.
    pub zoom:               f32,
    /// Why the graph did not compile the last time it ran, and at which
    /// node: shown on that node and above the canvas.
    pub error:              Arc<Mutex<Option<(NodeId, String)>>>,
    /// The type of each node's output on the geometry the graph last ran
    /// on (`refresh_types`), for the colours of its ports and wires.
    pub types:              Arc<Mutex<HashMap<NodeId, xms_ice::Ty>>>,
    /// The geometry the graph last ran on, sharing its columns (a copy costs
    /// next to nothing): the editor works out the types and Get Data's list
    /// from it when it draws, not every run.
    pub last_input:         Arc<Mutex<Option<Geo>>>,
}

/// The name a type goes by on ports and wires (and their colour).
pub fn type_hint(ty: xms_ice::Ty) -> &'static str {
    match ty {
        xms_ice::Ty::Bool => "Bool",
        xms_ice::Ty::Int => "Int",
        xms_ice::Ty::Float => "Float",
        xms_ice::Ty::Vec2 => "Vec2",
        xms_ice::Ty::Vec3 => "Vec3",
        xms_ice::Ty::Vec4 => "Vec4",
    }
}

impl IceGraph {
    pub fn new(id: IceGraphId, name: String) -> Self {
        let mut g = Self {
            id, name,
            nodes: vec![], connections: vec![],
            next_node_id: 0, next_connection_id: 0,
            selected_node: None, dragging_node: None,
            drag_offset: egui::Vec2::ZERO,
            connecting_from: None, pan_offset: egui::Vec2::ZERO, zoom: 1.0,
            error: Arc::default(),
            types: Arc::default(),
            last_input: Arc::default(),
        };
        g.add_node(ROOT_NAME.into(), IceGraphNodeType::Root { context: Context::Point }, egui::pos2(500.0, 200.0));
        g
    }

    /// The graph's root: the ICE Execution Ports node.
    pub fn root(&self) -> Option<NodeId> {
        self.nodes.iter().find(|n| matches!(n.node_type, IceGraphNodeType::Root { .. })).map(|n| n.id)
    }

    pub fn add_node(&mut self, name: String, t: IceGraphNodeType, pos: egui::Pos2) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        let (inputs, outputs) = Self::create_sockets(&t);
        self.nodes.push(IceGraphNode { id, name, node_type: t, position: pos, inputs, outputs });
        id
    }

    /// A node's ports. Unconnected inputs start with a value that leaves
    /// what comes in unchanged where it can: 0 to add, 1 to multiply.
    pub fn create_sockets(t: &IceGraphNodeType) -> (Vec<IceGraphInputSocket>, Vec<IceGraphOutputSocket>) {
        let input = |name: &str, value: f32| IceGraphInputSocket {
            name: name.into(), value_hint: "Any", connected_output: None, value: Some(Value::Float(value)),
        };
        let output = |name: &str, hint: &'static str| IceGraphOutputSocket { name: name.into(), value_hint: hint };
        match t {
            IceGraphNodeType::Root { .. } => (vec![execute_port(1)], vec![]),
            IceGraphNodeType::GetData { .. } => (vec![], vec![output("Value", "Any")]),
            IceGraphNodeType::SetData { .. } => (vec![input("Value", 0.0)], vec![output("Execute", "Execute")]),
            IceGraphNodeType::Constant { value } => (vec![], vec![output("Value", type_hint(value.ty()))]),
            IceGraphNodeType::Math { op } => {
                let inputs = match op {
                    Math::Multiply | Math::Divide => vec![input("A", 1.0), input("B", 1.0)],
                    Math::Lerp => vec![input("A", 0.0), input("B", 1.0), input("T", 0.5)],
                    op if op.arity() == 1 => vec![input("Value", 0.0)],
                    _ => vec![input("A", 0.0), input("B", 0.0)],
                };
                (inputs, vec![output("Result", "Any")])
            }
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
            // The root keeps one Execute port free, as Softimage's ICETree does.
            if matches!(n.node_type, IceGraphNodeType::Root { .. }) && n.inputs.iter().all(|i| i.connected_output.is_some()) {
                let next = n.inputs.len() + 1;
                n.inputs.push(execute_port(next));
            }
        }
        self.connections.push(IceGraphConnection {
            id,
            from_node: from, from_output: from_out,
            to_node:   to,   to_input:    to_in,
        });
    }

    /// Delete a node and the wires to and from it. The root stays: every
    /// ICE graph has one. Returns whether it was deleted.
    pub fn remove_node(&mut self, id: NodeId) -> bool {
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else { return false };
        if matches!(node.node_type, IceGraphNodeType::Root { .. }) { return false; }
        let wires: Vec<ConnectionId> = self.connections.iter()
            .filter(|c| c.from_node == id || c.to_node == id)
            .map(|c| c.id)
            .collect();
        for cid in wires { self.remove_connection(cid); }
        self.nodes.retain(|n| n.id != id);
        if self.selected_node == Some(id) { self.selected_node = None; }
        if self.dragging_node == Some(id) { self.dragging_node = None; }
        if self.connecting_from.is_some_and(|(n, _)| n == id) { self.connecting_from = None; }
        true
    }

    pub fn remove_connection(&mut self, cid: ConnectionId) {
        if let Some(c) = self.connections.iter().find(|c| c.id == cid) {
            let (tn, ti) = (c.to_node, c.to_input);
            if let Some(n) = self.nodes.iter_mut().find(|n| n.id == tn) {
                if let Some(inp) = n.inputs.get_mut(ti) { inp.connected_output = None; }
            }
        }
        self.connections.retain(|c| c.id != cid);
    }

    /// The graph as the engine takes it, and the editor's node at each of
    /// its positions (so an error can be shown on the right node).
    fn to_engine(&self) -> (xms_ice::Graph, Vec<NodeId>) {
        let ids: Vec<NodeId> = self.nodes.iter().map(|n| n.id).collect();
        let nodes = self.nodes.iter().map(|n| {
            let root = matches!(n.node_type, IceGraphNodeType::Root { .. });
            let op = match &n.node_type {
                IceGraphNodeType::Root { context } => Op::Root { context: *context },
                IceGraphNodeType::GetData { name, context, reduce } => Op::GetData { name: name.clone(), context: *context, reduce: *reduce },
                IceGraphNodeType::SetData { name, context, reduce } => Op::SetData { name: name.clone(), context: *context, reduce: *reduce },
                IceGraphNodeType::Constant { value } => Op::Constant(*value),
                IceGraphNodeType::Math { op } => Op::Math(*op),
            };
            let inputs = n.inputs.iter().filter_map(|i| {
                let wired = i.connected_output.and_then(|(from, output)| {
                    ids.iter().position(|id| *id == from).map(|node| Input::Wire { node, output })
                });
                match wired {
                    Some(w) => Some(w),
                    // The root's free Execute ports are not inputs to run.
                    None if root => None,
                    None => Some(Input::Value(i.value.unwrap_or(Value::Float(0.0)))),
                }
            }).collect();
            Node { op, inputs }
        }).collect();
        (xms_ice::Graph { nodes }, ids)
    }

    /// The nodes wired into the root, the root among them: the only ones
    /// a run computes.
    fn reaching_root(&self) -> HashSet<NodeId> {
        let mut found: HashSet<NodeId> = HashSet::new();
        let mut todo: Vec<NodeId> = self.root().into_iter().collect();
        while let Some(id) = todo.pop() {
            if !found.insert(id) { continue; }
            let Some(n) = self.nodes.iter().find(|n| n.id == id) else { continue };
            todo.extend(n.inputs.iter().filter_map(|i| i.connected_output.map(|(from, _)| from)));
        }
        found
    }

    /// What the graph computes: the nodes wired into the root, what each
    /// input takes and the wires between them. Not where nodes are drawn,
    /// their names or what is selected, nor a node wired to nothing yet:
    /// moving a node or putting a new one down does not run the graph again.
    pub fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let live = self.reaching_root();
        for n in self.nodes.iter().filter(|n| live.contains(&n.id)) {
            n.id.0.hash(&mut h);
            serde_json::to_string(&n.node_type).unwrap_or_default().hash(&mut h);
            for i in &n.inputs {
                i.connected_output.map(|(id, out)| (id.0, out)).hash(&mut h);
                serde_json::to_string(&i.value).unwrap_or_default().hash(&mut h);
            }
        }
        for c in self.connections.iter().filter(|c| live.contains(&c.to_node)) {
            (c.from_node.0, c.from_output, c.to_node.0, c.to_input).hash(&mut h);
        }
        h.finish()
    }

    /// Work out the type of every node's output, wired or not, on the
    /// geometry the graph last ran on. Cheap (no attribute is read), so the
    /// editor does it as it draws.
    pub fn refresh_types(&self) {
        let Some(input) = self.last_input.lock().ok().and_then(|g| g.clone()) else { return };
        let (graph, ids) = self.to_engine();
        let types: HashMap<NodeId, xms_ice::Ty> = xms_ice::output_types(&graph, &input).into_iter().enumerate()
            .filter_map(|(k, t)| Some((*ids.get(k)?, t?)))
            .collect();
        if let Ok(mut kept) = self.types.lock() { *kept = types; }
    }

    /// What Get Data can read on the geometry the graph last ran on: by
    /// context, each attribute's name and kind, stored ones then those made
    /// from the topology, as the Primitive Inspector lists them.
    pub fn readable(&self) -> Vec<(Context, String, String)> { readable(&self.last_input) }
}

/// What Get Data can read on the geometry kept in `last` (see
/// `IceGraph::readable`).
pub fn readable(last: &Mutex<Option<Geo>>) -> Vec<(Context, String, String)> {
    let Some(input) = last.lock().ok().and_then(|g| g.clone()) else { return vec![] };
    let mut out: Vec<(Context, String, String)> = Context::ALL.into_iter()
        .flat_map(|c| input.attrs(c).map(move |(n, a)| (c, n.to_string(), format!("{:?}", a.column.kind()))))
        .collect();
    for &(c, n, t) in xms_ice::derived::DERIVED {
        let stored = out.iter().any(|(oc, on, _)| *oc == c && on.as_str() == n);
        if !stored && xms_ice::derived::ty(&input, c, n).is_some() {
            out.push((c, n.to_string(), type_hint(t).to_string()));
        }
    }
    out
}

impl IceGraph {
    /// The type an output gives, when known: a constant's own, else what
    /// `refresh_types` found.
    pub fn output_type(&self, node: NodeId, output: usize) -> Option<xms_ice::Ty> {
        let n = self.nodes.iter().find(|n| n.id == node)?;
        match n.outputs.get(output)?.value_hint {
            "Execute" => None,
            _ => match &n.node_type {
                IceGraphNodeType::Constant { value } => Some(value.ty()),
                _ => self.types.lock().ok()?.get(&node).copied(),
            },
        }
    }

    /// Run the graph on a geometry, in its own space. What it does not set
    /// stays shared with `input`. A graph that does not compile gives
    /// `input` back, and says why on the node that broke it.
    pub fn evaluate(&self, input: &Geo, _template: Option<&Geo>) -> Geo {
        let (graph, ids) = self.to_engine();
        // Kept for the editor: its columns are shared, not copied.
        if let Ok(mut last) = self.last_input.lock() { *last = Some(input.clone()); }
        let result = xms_ice::compile(&graph, input).map(|plan| xms_ice::run(&plan, input, &HashMap::new()));
        let mut error = self.error.lock().unwrap_or_else(|e| e.into_inner());
        match result {
            Ok(out) => { *error = None; out }
            Err(e) => { *error = ids.get(e.node).map(|id| (*id, e.message)); input.clone() }
        }
    }
}

/// The root's Execute port number `n`.
pub(crate) fn execute_port(n: usize) -> IceGraphInputSocket {
    IceGraphInputSocket { name: format!("Port {n}"), value_hint: "Execute", connected_output: None, value: None }
}

#[derive(Default)]
pub struct IceGraphStore {
    pub subnets: HashMap<IceGraphId, IceGraph>,
    pub next_id: usize,
}

impl IceGraphStore {
    pub fn create_subnet(&mut self, name: String) -> IceGraphId {
        let id = IceGraphId(self.next_id);
        self.next_id += 1;
        self.subnets.insert(id, IceGraph::new(id, name));
        id
    }
    pub fn get(&self,         id: IceGraphId) -> Option<&IceGraph>     { self.subnets.get(&id)     }

    /// What every graph computes (`IceGraph::content_hash`), together.
    pub fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut ids: Vec<&IceGraphId> = self.subnets.keys().collect();
        ids.sort_by_key(|i| i.0);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for id in ids { (id.0, self.subnets[id].content_hash()).hash(&mut h); }
        h.finish()
    }
    pub fn get_mut(&mut self, id: IceGraphId) -> Option<&mut IceGraph> { self.subnets.get_mut(&id) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geo::{Attr, Column, Role, POINTS};

    /// Get P → Multiply (by its B, 2) → Set P → ICE Execution Ports.
    fn doubling() -> IceGraph {
        let mut g = IceGraph::new(IceGraphId(0), "double".into());
        let root = g.root().unwrap();
        let p = g.add_node("P".into(), IceGraphNodeType::get("P"), egui::pos2(0.0, 0.0));
        let mul = g.add_node("Multiply".into(), IceGraphNodeType::Math { op: Math::Multiply }, egui::pos2(0.0, 0.0));
        let set = g.add_node("Set P".into(), IceGraphNodeType::set("P"), egui::pos2(0.0, 0.0));
        g.nodes.iter_mut().find(|n| n.id == mul).unwrap().inputs[1].value = Some(Value::Float(2.0));
        g.add_connection(p, 0, mul, 0);
        g.add_connection(mul, 0, set, 0);
        g.add_connection(set, 0, root, 0);
        g
    }

    #[test]
    fn a_graph_runs_on_geo_and_shares_what_it_does_not_set() {
        let g = doubling();
        let mut geo = Geo::from_polygons(vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], vec![3], vec![0, 1, 2]);
        geo.set(Context::Face, "heat", Attr::new(Column::Float(Arc::new(vec![0.5])), Role::None));
        let out = g.evaluate(&geo, None);
        assert_eq!(out.points()[1], [0.0, 2.0, 0.0]);
        assert!(!out.attr(Context::Point, POINTS).unwrap().same(geo.attr(Context::Point, POINTS).unwrap()));
        assert!(out.attr(Context::Face, "heat").unwrap().same(geo.attr(Context::Face, "heat").unwrap()));
        assert!(out.topology.same(&geo.topology));
        assert!(g.error.lock().unwrap().is_none());
    }

    /// Moving a node, renaming it or selecting it does not change what the
    /// graph computes; a value does. The types of the outputs are known
    /// once it has run.
    #[test]
    fn the_hash_follows_what_the_graph_computes() {
        let mut g = doubling();
        let before = g.content_hash();
        g.nodes[1].position = egui::pos2(300.0, 300.0);
        g.nodes[1].name = "moved".into();
        g.selected_node = Some(g.nodes[1].id);
        assert_eq!(g.content_hash(), before);
        let mul = g.nodes.iter().position(|n| matches!(n.node_type, IceGraphNodeType::Math { .. })).unwrap();
        g.nodes[mul].inputs[1].value = Some(Value::Float(3.0));
        assert_ne!(g.content_hash(), before);
        let geo = Geo::from_points(vec![[1.0, 2.0, 3.0]]);
        g.evaluate(&geo, None);
        g.refresh_types();
        let mul_id = g.nodes[mul].id;
        assert_eq!(g.output_type(mul_id, 0), Some(xms_ice::Ty::Vec3));
    }

    /// A new attribute on the second port, from a constant, reaches the
    /// primitive's primvars (what the Primitive Inspector lists); written
    /// per face from a per-point graph, it is a uniform primvar.
    #[test]
    fn a_new_attribute_reaches_the_primvars() {
        use crate::core::geo_mesh::GeoMesh;
        let mut g = doubling();
        let root = g.root().unwrap();
        let k = g.add_node("Constant".into(), IceGraphNodeType::Constant { value: Value::Float(2.5) }, egui::pos2(0.0, 0.0));
        let set = g.add_node("Set".into(), IceGraphNodeType::set("test"), egui::pos2(0.0, 0.0));
        g.add_connection(k, 0, set, 0);
        g.add_connection(set, 0, root, 1);
        let geo = Geo::from_polygons(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], vec![3], vec![0, 1, 2]);
        let out = g.evaluate(&geo, None);
        assert!(g.error.lock().unwrap().is_none(), "{:?}", g.error.lock().unwrap());
        let mesh = out.to_mesh();
        let pv = mesh.primvars.iter().find(|p| p.name == "test").expect("a test primvar");
        assert_eq!(pv.interp, crate::types::PrimVarInterp::Vertex);
        assert_eq!(pv.values, vec![vec![2.5]; 3]);
        if let IceGraphNodeType::SetData { context, .. } = &mut g.nodes.iter_mut().find(|n| n.id == set).unwrap().node_type {
            *context = Some(Context::Face);
        }
        let mesh = g.evaluate(&geo, None).to_mesh();
        let pv = mesh.primvars.iter().find(|p| p.name == "test").expect("a test primvar");
        assert_eq!(pv.interp, crate::types::PrimVarInterp::Uniform);
        assert_eq!(pv.values, vec![vec![2.5]]);
    }

    /// Putting a node down, wired to nothing, does not run the graph again;
    /// its type is known all the same, and Get Data's list has what the
    /// topology gives too.
    #[test]
    fn a_node_wired_to_nothing_does_not_change_the_hash() {
        let mut g = doubling();
        let geo = Geo::from_polygons(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], vec![3], vec![0, 1, 2]);
        g.evaluate(&geo, None);
        let before = g.content_hash();
        let get = g.add_node("Get".into(), IceGraphNodeType::GetData { name: "area".into(), context: Some(Context::Face), reduce: xms_ice::Reduce::Average }, egui::pos2(0.0, 0.0));
        assert_eq!(g.content_hash(), before);
        g.refresh_types();
        assert_eq!(g.output_type(get, 0), Some(xms_ice::Ty::Float));
        let readable = g.readable();
        assert!(readable.iter().any(|(c, n, _)| *c == Context::Face && n == "firstCorner"), "{readable:?}");
        assert!(readable.iter().any(|(c, n, _)| *c == Context::Point && n == POINTS));
    }

    #[test]
    fn the_root_keeps_a_port_free_and_an_empty_graph_changes_nothing() {
        let g = doubling();
        let root = g.nodes.iter().find(|n| Some(n.id) == g.root()).unwrap();
        assert_eq!(root.inputs.len(), 2, "one wired, one free");
        let empty = IceGraph::new(IceGraphId(1), "empty".into());
        let geo = Geo::from_points(vec![[1.0, 2.0, 3.0]]);
        let out = empty.evaluate(&geo, None);
        assert!(out.attr(Context::Point, POINTS).unwrap().same(geo.attr(Context::Point, POINTS).unwrap()));
    }

    #[test]
    fn an_error_is_kept_on_the_node_that_broke_the_graph() {
        let mut g = IceGraph::new(IceGraphId(0), "missing".into());
        let root = g.root().unwrap();
        let get = g.add_node("Get".into(), IceGraphNodeType::get("nowhere"), egui::pos2(0.0, 0.0));
        let set = g.add_node("Set".into(), IceGraphNodeType::set("copy"), egui::pos2(0.0, 0.0));
        g.add_connection(get, 0, set, 0);
        g.add_connection(set, 0, root, 0);
        let geo = Geo::from_points(vec![[0.0; 3]]);
        let out = g.evaluate(&geo, None);
        assert!(out.attr(Context::Point, "copy").is_none(), "the input comes back");
        assert_eq!(g.error.lock().unwrap().as_ref().map(|(id, _)| *id), Some(get));
    }
}
