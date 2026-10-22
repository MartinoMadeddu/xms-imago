//! ICE graphs: the graph inside an ICE operator, as the editor holds it
//! (nodes, wires, positions, values on unconnected inputs), and running it
//! on a primitive's geometry with the ICE engine (`xms_ice`).
//!
//! A graph ends in its **Execution Ports** root (named ExecutionPorts):
//! its Execute ports run in order,
//! over every element of its context (points by default). Set Data nodes
//! wired into it write attributes; everything else is reached from them.

pub mod ui;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use xms_ice::{Input, Math, Node, Op, Value};

use crate::core::geo::{Context, Geo};
use crate::types::{ConnectionId, IceGraphId, IceGraphNodeType, NodeId};

/// What one of the ICE operator's reference inputs brings into its graph:
/// every prim of the scene wired there, by path (Get Reference lists them).
pub type Reference = Vec<String>;

thread_local! {
    /// What the Instancers of the last graph run on this thread made. The
    /// evaluator's type gives back geometry only; the ICE operator, which
    /// runs a graph and then turns its instances into prims, takes them
    /// from here (`take_instances`).
    static INSTANCES: std::cell::RefCell<Vec<(xms_ice::Instances, crate::types::DrawMode)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The instances the last graph run on this thread made, taken: empty until
/// another runs.
/// Each with the draw mode its Instancer gives its prototypes.
pub fn take_instances() -> Vec<(xms_ice::Instances, crate::types::DrawMode)> {
    INSTANCES.with(|i| std::mem::take(&mut *i.borrow_mut()))
}

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

/// The name a new graph's root is given (its type: Execution Ports).
/// A root still named as before, `ICE Tree`, is renamed when read.
pub const ROOT_NAME: &str = "ExecutionPorts";

#[derive(Clone)]
pub struct IceGraph {
    pub id:                 IceGraphId,
    pub name:               String,
    pub nodes:              Vec<IceGraphNode>,
    pub connections:        Vec<IceGraphConnection>,
    pub next_node_id:       usize,
    pub next_connection_id: usize,
    pub selected_node:      Option<NodeId>,
    /// Every node selected (a box, or Shift+click): moved and deleted
    /// together. `selected_node` is the one whose parameters are shown.
    pub selected_nodes:     Vec<NodeId>,
    /// The wire selected: Delete takes it off.
    pub selected_wire:      Option<ConnectionId>,
    /// Where a box being dragged out to select nodes started, on the canvas.
    pub marquee:            Option<egui::Pos2>,
    /// A port clicked to unplug: a click on the empty canvas takes its
    /// wires off (`graph_ports`).
    pub unplug:             Option<crate::graph_ports::Picked<NodeId, usize>>,
    /// The inputs whose wires were picked up off an output (Ctrl+drag): let
    /// go of on another output they go there, anywhere else they are gone.
    pub carrying:           Vec<(NodeId, usize)>,
    /// The wire being drawn was dragged, not clicked: let go of over no
    /// input, it is gone.
    pub wire_dragged:       bool,
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
    /// The nodes whose output is an array (`refresh_types`): drawn and
    /// named apart from single values.
    pub arrays:             Arc<Mutex<std::collections::HashSet<NodeId>>>,
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
        xms_ice::Ty::Quat => "Quat",
        xms_ice::Ty::Sample => "Sample",
        xms_ice::Ty::Token => "Token",
    }
}

impl IceGraph {
    pub fn new(id: IceGraphId, name: String) -> Self {
        let mut g = Self {
            id, name,
            nodes: vec![], connections: vec![],
            next_node_id: 0, next_connection_id: 0,
            selected_node: None, selected_nodes: vec![], selected_wire: None, marquee: None, unplug: None,
            carrying: vec![], wire_dragged: false,
            dragging_node: None,
            drag_offset: egui::Vec2::ZERO,
            connecting_from: None, pan_offset: egui::Vec2::ZERO, zoom: 1.0,
            error: Arc::default(),
            types: Arc::default(),
            arrays: Arc::default(),
            last_input: Arc::default(),
        };
        g.add_node(ROOT_NAME.into(), IceGraphNodeType::Root { context: Context::Point }, egui::pos2(500.0, 200.0));
        g
    }

    /// The graph's root: the Execution Ports node.
    pub fn root(&self) -> Option<NodeId> {
        self.nodes.iter().find(|n| matches!(n.node_type, IceGraphNodeType::Root { .. })).map(|n| n.id)
    }

    pub fn add_node(&mut self, name: String, t: IceGraphNodeType, pos: egui::Pos2) -> NodeId {
        let name = crate::node_graph::node_name(&name);
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
                    Math::Rotate => vec![IceGraphInputSocket { value: Some(Value::Quat([0.0, 0.0, 0.0, 1.0])), ..input("Rotation", 0.0) },
                                         IceGraphInputSocket { value: Some(Value::Vec3([1.0, 0.0, 0.0])), ..input("Vector", 0.0) }],
                    Math::AxisAngle => vec![IceGraphInputSocket { value: Some(Value::Vec3([0.0, 1.0, 0.0])), ..input("Axis", 0.0) }, input("Angle", 0.0)],
                    Math::Euler => vec![IceGraphInputSocket { value: Some(Value::Vec3([0.0; 3])), ..input("Angles", 0.0) }],
                    op if op.arity() == 1 => vec![input("Value", 0.0)],
                    _ => vec![input("A", 0.0), input("B", 0.0)],
                };
                (inputs, vec![output("Result", "Any")])
            }
            // A count of 0: one value per element. The Seed, unwired: each
            // element's own number.
            IceGraphNodeType::Random { .. } => (
                vec![input("Min", 0.0), input("Max", 1.0), IceGraphInputSocket { value: Some(Value::Int(0)), ..input("Count", 0.0) },
                     IceGraphInputSocket { value: None, ..input("Seed", 0.0) }],
                vec![output("Value", "Any")],
            ),
            IceGraphNodeType::Concat => (vec![array_port(1), array_port(2)], vec![output("Array", "Any")]),
            // Its parts, by what it makes: numbers 0 (a rotation's real
            // part 1: none), vectors zero.
            IceGraphNodeType::Compose { op } => (op.inputs().iter().map(|&(name, ty)| {
                let value = match ty {
                    xms_ice::Ty::Vec2 => Value::Vec2([0.0; 2]),
                    xms_ice::Ty::Vec3 => Value::Vec3([0.0; 3]),
                    _ if name == "Real" => Value::Float(1.0),
                    _ => Value::Float(0.0),
                };
                IceGraphInputSocket { value: Some(value), ..input(name, 0.0) }
            }).collect(), vec![output("Vector", "Any")]),
            // What it takes apart, zero (no rotation) when not wired; an
            // output per part.
            IceGraphNodeType::Decompose { op } => {
                let (name, value) = match op.input() {
                    xms_ice::Ty::Vec2 => ("Vector", Value::Vec2([0.0; 2])),
                    xms_ice::Ty::Vec4 => ("Vector", Value::Vec4([0.0; 4])),
                    xms_ice::Ty::Quat => ("Rotation", Value::Quat([0.0, 0.0, 0.0, 1.0])),
                    _ => ("Vector", Value::Vec3([0.0; 3])),
                };
                (vec![IceGraphInputSocket { value: Some(value), ..input(name, 0.0) }],
                 op.outputs().iter().map(|(n, _)| output(*n, "Any")).collect())
            }
            // Outputs in `xms_ice::SCATTER_*`'s order.
            IceGraphNodeType::Scatter { .. } => (
                vec![IceGraphInputSocket { value: Some(Value::Int(1000)), ..input("Count", 0.0) }],
                vec![output("Positions", "Any"), output("Normals", "Any"), output("Surface Samples", "Any")],
            ),
            IceGraphNodeType::GetDataAt { .. } => (vec![IceGraphInputSocket { value: None, ..input("Samples", 0.0) }], vec![output("Value", "Any")]),
            // Normals +Y (none) and a random turn about it, when not wired;
            // a count of 0, one per element (or per normal of an array).
            IceGraphNodeType::RandomOrientation { .. } => (vec![
                IceGraphInputSocket { value: Some(Value::Vec3([0.0, 1.0, 0.0])), ..input("Normals", 0.0) },
                IceGraphInputSocket { value: Some(Value::Vec3([0.0, 360.0, 0.0])), ..input("Random Rotation", 0.0) },
                IceGraphInputSocket { value: Some(Value::Int(0)), ..input("Count", 0.0) },
                IceGraphInputSocket { value: None, ..input("Seed", 0.0) },
            ], vec![output("Orientations", "Any")]),
            IceGraphNodeType::GetReference { .. } => (vec![], vec![output("Paths", "Any")]),
            IceGraphNodeType::GetArrayData { .. } => (vec![], vec![output("Array", "Any")]),
            // A PointInstancer's attributes; one orientation and scale for
            // all when not wired, a prototype picked at random (-1).
            IceGraphNodeType::Instancer { .. } => (vec![
                IceGraphInputSocket { value: None, ..input("Positions", 0.0) },
                IceGraphInputSocket { value: None, ..input("Prototypes", 0.0) },
                IceGraphInputSocket { value: Some(Value::Int(-1)), ..input("Proto Indices", 0.0) },
                IceGraphInputSocket { value: Some(Value::Quat([0.0, 0.0, 0.0, 1.0])), ..input("Orientations", 0.0) },
                input("Scales", 1.0),
            ], vec![output("Execute", "Execute")]),
            IceGraphNodeType::Array { op } => {
                let mut inputs = vec![IceGraphInputSocket { value: None, ..input("Array", 0.0) }];
                if *op == xms_ice::ArrayOp::Element { inputs.push(IceGraphInputSocket { value: Some(Value::Int(0)), ..input("Index", 0.0) }); }
                (inputs, vec![output("Result", "Any")])
            }
        }
    }

    /// A node's ports made again for what it now is (Compose changed what
    /// it makes, Decompose what it takes apart): the wires into inputs and
    /// out of outputs still there kept, the others taken off.
    pub fn resync_sockets(&mut self, id: NodeId) {
        let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) else { return };
        let (mut inputs, outputs) = Self::create_sockets(&n.node_type);
        for (new, old) in inputs.iter_mut().zip(&n.inputs) { new.connected_output = old.connected_output; }
        let (ins, outs) = (inputs.len(), outputs.len());
        n.inputs = inputs;
        n.outputs = outputs;
        self.connections.retain(|c| (c.to_node != id || c.to_input < ins) && (c.from_node != id || c.from_output < outs));
        // Inputs of other nodes wired to an output that is gone: free.
        for other in self.nodes.iter_mut() {
            for input in other.inputs.iter_mut() {
                if input.connected_output.is_some_and(|(from, k)| from == id && k >= outs) { input.connected_output = None; }
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
            // The root keeps one Execute port free, as Softimage's ICETree
            // does; Concatenate Arrays one input.
            if n.inputs.iter().all(|i| i.connected_output.is_some()) {
                let next = n.inputs.len() + 1;
                match n.node_type {
                    IceGraphNodeType::Root { .. } => n.inputs.push(execute_port(next)),
                    IceGraphNodeType::Concat => n.inputs.push(array_port(next)),
                    _ => {}
                }
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
        self.selected_nodes.retain(|n| *n != id);
        if self.dragging_node == Some(id) { self.dragging_node = None; }
        if self.connecting_from.is_some_and(|(n, _)| n == id) { self.connecting_from = None; }
        true
    }

    /// Delete what is selected: the wire, else every node selected (not the
    /// root, which every graph keeps).
    pub fn delete_selected(&mut self) {
        if let Some(cid) = self.selected_wire.take() {
            self.remove_connection(cid);
            return;
        }
        let mut ids = std::mem::take(&mut self.selected_nodes);
        ids.extend(self.selected_node);
        for id in ids { self.remove_node(id); }
    }

    /// Take off every wire of a picked port (`graph_ports::Picked`).
    pub fn unplug(&mut self, picked: &crate::graph_ports::Picked<NodeId, usize>) {
        let off: Vec<ConnectionId> = self.connections.iter()
            .filter(|c| picked.holds(c.from_node, &c.from_output, c.to_node, &c.to_input))
            .map(|c| c.id).collect();
        for cid in off { self.remove_connection(cid); }
    }

    pub fn remove_connection(&mut self, cid: ConnectionId) {
        if let Some(c) = self.connections.iter().find(|c| c.id == cid) {
            let (tn, ti) = (c.to_node, c.to_input);
            if let Some(n) = self.nodes.iter_mut().find(|n| n.id == tn) {
                if let Some(inp) = n.inputs.get_mut(ti) { inp.connected_output = None; }
            }
        }
        self.connections.retain(|c| c.id != cid);
        if self.selected_wire == Some(cid) { self.selected_wire = None; }
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
                IceGraphNodeType::Random { seed } => Op::Random { seed: *seed },
                IceGraphNodeType::Array { op } => Op::Array(*op),
                IceGraphNodeType::Compose { op } => Op::Compose(*op),
                IceGraphNodeType::Decompose { op } => Op::Decompose(*op),
                IceGraphNodeType::Scatter { seed, weight } => Op::Scatter { seed: *seed, weight: weight.clone() },
                IceGraphNodeType::GetDataAt { name } => Op::GetDataAt { name: name.clone() },
                IceGraphNodeType::GetArrayData { name, context } => Op::GetArrayData { name: name.clone(), context: *context },
                IceGraphNodeType::RandomOrientation { seed } => Op::RandomOrientation { seed: *seed },
                IceGraphNodeType::Concat => Op::Concat,
                // Rows as they are: `evaluate` turns them into the paths they
                // name on the reference input.
                IceGraphNodeType::GetReference { input, paths } => Op::GetReference {
                    input: *input as usize,
                    paths: crate::core::locations::is_set(paths).then(|| paths.clone()),
                },
                IceGraphNodeType::Instancer { path, seed, .. } => Op::Instancer { path: path.clone(), seed: *seed },
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
        let arrays = xms_ice::output_arrays(&graph).into_iter().enumerate()
            .filter_map(|(k, a)| a.then(|| ids.get(k).copied()).flatten())
            .collect();
        if let Ok(mut kept) = self.arrays.lock() { *kept = arrays; }
    }

    /// Whether an output gives an array, as `refresh_types` found.
    pub fn output_is_array(&self, node: NodeId, output: usize) -> bool {
        let Some(n) = self.nodes.iter().find(|n| n.id == node) else { return false };
        n.outputs.get(output).is_some_and(|o| o.value_hint != "Execute")
            && self.arrays.lock().is_ok_and(|a| a.contains(&node))
    }

    /// What Get Data can read on the geometry the graph last ran on: by
    /// context, each attribute's name and kind, stored ones then those made
    /// from the topology, as the Primitive Inspector lists them.
    #[cfg(test)]
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
                // Positions and normals, then the samples.
                IceGraphNodeType::Scatter { .. } => Some(if output == xms_ice::SCATTER_SAMPLES { xms_ice::Ty::Sample } else { xms_ice::Ty::Vec3 }),
                // Each part its own type, whatever comes in.
                IceGraphNodeType::Decompose { op } => op.outputs().get(output).map(|o| o.1),
                _ => self.types.lock().ok()?.get(&node).copied(),
            },
        }
    }

    /// Run the graph on a geometry, in its own space. What it does not set
    /// stays shared with `input`. A graph that does not compile gives
    /// `input` back, and says why on the node that broke it.
    pub fn evaluate(&self, input: &Geo, references: &[Reference]) -> Geo {
        let (mut graph, ids) = self.to_engine();
        // Get Reference's rows, as Primitive Locations name prims: the paths
        // on its input each row names, row by row, each once.
        for node in &mut graph.nodes {
            if let Op::GetReference { input, paths: Some(rows) } = &mut node.op {
                let there = references.get(*input - 1).map(Vec::as_slice).unwrap_or_default();
                let mut named: Vec<String> = vec![];
                for row in crate::core::locations::rows(rows) {
                    for p in there.iter().filter(|p| crate::core::locations::names(row, p)) {
                        if !named.contains(p) { named.push(p.clone()); }
                    }
                }
                *rows = named;
            }
        }
        // Kept for the editor: its columns are shared, not copied.
        if let Ok(mut last) = self.last_input.lock() { *last = Some(input.clone()); }
        let result = xms_ice::compile(&graph, input).map(|plan| {
            let ran = xms_ice::run(&plan, input, &HashMap::new(), references);
            // Each with its Instancer's draw mode, found by its path.
            let made = ran.instances.into_iter().map(|i| {
                let mode = self.nodes.iter().find_map(|n| match &n.node_type {
                    IceGraphNodeType::Instancer { path, draw_mode, .. } if path.trim() == i.path => Some(*draw_mode),
                    _ => None,
                }).unwrap_or_default();
                (i, mode)
            }).collect();
            INSTANCES.with(|i| *i.borrow_mut() = made);
            ran.geo
        });
        let mut error = self.error.lock().unwrap_or_else(|e| e.into_inner());
        match result {
            Ok(out) => { *error = None; out }
            Err(e) => { *error = ids.get(e.node).map(|id| (*id, e.message)); input.clone() }
        }
    }
}

/// The root's Execute port number `n`.
/// Concatenate Arrays' input `n` (from 1).
pub(crate) fn array_port(n: usize) -> IceGraphInputSocket {
    IceGraphInputSocket { name: format!("Array {n}"), value_hint: "Any", connected_output: None, value: None }
}

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

    /// Get P → Multiply (by its B, 2) → Set P → Execution Ports.
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
        let out = g.evaluate(&geo, &[]);
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
        g.evaluate(&geo, &[]);
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
        let out = g.evaluate(&geo, &[]);
        assert!(g.error.lock().unwrap().is_none(), "{:?}", g.error.lock().unwrap());
        let mesh = out.to_mesh();
        let pv = mesh.primvars.iter().find(|p| p.name == "test").expect("a test primvar");
        assert_eq!(pv.interp, crate::types::PrimVarInterp::Vertex);
        assert_eq!(pv.values, vec![vec![2.5]; 3]);
        if let IceGraphNodeType::SetData { context, .. } = &mut g.nodes.iter_mut().find(|n| n.id == set).unwrap().node_type {
            *context = Some(Context::Face);
        }
        let mesh = g.evaluate(&geo, &[]).to_mesh();
        let pv = mesh.primvars.iter().find(|p| p.name == "test").expect("a test primvar");
        assert_eq!(pv.interp, crate::types::PrimVarInterp::Uniform);
        assert_eq!(pv.values, vec![vec![2.5]]);
    }

    /// A picked port's wires come off: an input's one wire, an output's
    /// every wire. Deleting the selection takes the selected wire first,
    /// else every selected node but the root.
    #[test]
    fn unplugging_and_deleting_the_selection() {
        use crate::graph_ports::Picked;
        let mut g = doubling();
        let root = g.root().unwrap();
        let p = g.nodes.iter().find(|n| matches!(n.node_type, IceGraphNodeType::GetData { .. })).unwrap().id;
        let mul = g.nodes.iter().find(|n| matches!(n.node_type, IceGraphNodeType::Math { .. })).unwrap().id;
        // P feeds both of Multiply's inputs: unplugging P's output takes both.
        g.add_connection(p, 0, mul, 1);
        g.unplug(&Picked::Output(p, 0));
        assert!(g.connections.iter().all(|c| c.from_node != p));
        assert!(g.nodes.iter().find(|n| n.id == mul).unwrap().inputs.iter().all(|i| i.connected_output.is_none()));
        // An input: its wire only.
        let set = g.nodes.iter().find(|n| matches!(n.node_type, IceGraphNodeType::SetData { .. })).unwrap().id;
        g.unplug(&Picked::Input(set, 0));
        assert!(g.connections.iter().all(|c| c.to_node != set));
        assert!(g.connections.iter().any(|c| c.to_node == root), "Set's Execute still goes into the root");
        // The selected wire goes first, then the nodes; the root stays.
        let wire = g.connections[0].id;
        g.selected_wire = Some(wire);
        g.selected_nodes = vec![set, root];
        g.delete_selected();
        assert!(g.connections.iter().all(|c| c.id != wire));
        assert!(g.nodes.iter().any(|n| n.id == set), "the wire only");
        g.delete_selected();
        assert!(g.nodes.iter().all(|n| n.id != set));
        assert!(g.root().is_some());
        assert!(g.selected_nodes.is_empty());
    }

    /// Putting a node down, wired to nothing, does not run the graph again;
    /// its type is known all the same, and Get Data's list has what the
    /// topology gives too.
    #[test]
    fn a_node_wired_to_nothing_does_not_change_the_hash() {
        let mut g = doubling();
        let geo = Geo::from_polygons(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], vec![3], vec![0, 1, 2]);
        g.evaluate(&geo, &[]);
        let before = g.content_hash();
        let get = g.add_node("Get".into(), IceGraphNodeType::GetData { name: "area".into(), context: Some(Context::Face), reduce: xms_ice::Reduce::Average }, egui::pos2(0.0, 0.0));
        assert_eq!(g.content_hash(), before);
        g.refresh_types();
        assert_eq!(g.output_type(get, 0), Some(xms_ice::Ty::Float));
        let readable = g.readable();
        assert!(readable.iter().any(|(c, n, _)| *c == Context::Face && n == "firstCorner"), "{readable:?}");
        assert!(readable.iter().any(|(c, n, _)| *c == Context::Point && n == POINTS));
    }

    /// Concatenate Arrays keeps an input free, as the root does: two Get
    /// References joined, then a third input to join another.
    #[test]
    fn concatenate_arrays_keeps_an_input_free_and_joins_paths() {
        let mut g = IceGraph::new(IceGraphId(0), "join".into());
        let root = g.root().unwrap();
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == root) { n.node_type = IceGraphNodeType::Root { context: Context::Object }; }
        let a = g.add_node("A".into(), IceGraphNodeType::GetReference { input: 1, paths: vec![] }, egui::pos2(0.0, 0.0));
        let b = g.add_node("B".into(), IceGraphNodeType::GetReference { input: 2, paths: vec![] }, egui::pos2(0.0, 0.0));
        let join = g.add_node("Join".into(), IceGraphNodeType::Concat, egui::pos2(0.0, 0.0));
        g.add_connection(a, 0, join, 0);
        g.add_connection(b, 0, join, 1);
        assert_eq!(g.nodes.iter().find(|n| n.id == join).unwrap().inputs.len(), 3, "one free");
        let size = g.add_node("Size".into(), IceGraphNodeType::Array { op: xms_ice::ArrayOp::Size }, egui::pos2(0.0, 0.0));
        let set = g.add_node("Set".into(), IceGraphNodeType::set("n"), egui::pos2(0.0, 0.0));
        g.add_connection(join, 0, size, 0);
        g.add_connection(size, 0, set, 0);
        g.add_connection(set, 0, root, 0);
        let refs = vec![vec!["/a".to_string()], vec!["/b".to_string(), "/c".into()]];
        let out = g.evaluate(&Geo::from_points(vec![[0.0; 3]]), &refs);
        assert!(g.error.lock().unwrap().is_none(), "{:?}", g.error.lock().unwrap());
        assert_eq!(out.attr(Context::Object, "n").unwrap().column.ints().unwrap(), &[3]);
    }

    #[test]
    fn the_root_keeps_a_port_free_and_an_empty_graph_changes_nothing() {
        let g = doubling();
        let root = g.nodes.iter().find(|n| Some(n.id) == g.root()).unwrap();
        assert_eq!(root.inputs.len(), 2, "one wired, one free");
        let empty = IceGraph::new(IceGraphId(1), "empty".into());
        let geo = Geo::from_points(vec![[1.0, 2.0, 3.0]]);
        let out = empty.evaluate(&geo, &[]);
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
        let out = g.evaluate(&geo, &[]);
        assert!(out.attr(Context::Point, "copy").is_none(), "the input comes back");
        assert_eq!(g.error.lock().unwrap().as_ref().map(|(id, _)| *id), Some(get));
    }

    #[test]
    fn a_graph_reads_the_paths_on_its_reference_inputs() {
        let mut g = IceGraph::new(IceGraphId(0), "refs".into());
        let root = g.root().unwrap();
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == root) { n.node_type = IceGraphNodeType::Root { context: Context::Object }; }
        let paths = g.add_node("Pines".into(), IceGraphNodeType::GetReference { input: 1, paths: vec!["**/pine*".into()] }, egui::pos2(0.0, 0.0));
        let size = g.add_node("Size".into(), IceGraphNodeType::Array { op: xms_ice::ArrayOp::Size }, egui::pos2(0.0, 0.0));
        let set = g.add_node("Set n".into(), IceGraphNodeType::set("n"), egui::pos2(0.0, 0.0));
        g.add_connection(paths, 0, size, 0);
        g.add_connection(size, 0, set, 0);
        g.add_connection(set, 0, root, 0);
        let geo = Geo::from_points(vec![[0.0; 3]]);
        let refs = vec![vec!["/trees/pine".to_string(), "/trees/oak".into(), "/more/pine_2".into()]];
        let out = g.evaluate(&geo, &refs);
        assert_eq!(out.attr(Context::Object, "n").unwrap().column.ints().unwrap(), &[2]);
        // Its output is an array of paths, and says so.
        g.refresh_types();
        assert!(g.output_is_array(paths, 0) && !g.output_is_array(size, 0));
        assert_eq!(g.output_type(paths, 0), Some(xms_ice::Ty::Token));
    }

    #[test]
    fn the_ice_operator_scatters_a_reference_on_its_scene_as_instances() {
        use crate::node_graph::NodeGraphState;
        use crate::types::{EvalResult, NodeType, Placement};
        // ICE: Scatter on Surface (12) and Get Reference (all) into an
        // Instancer, per Object.
        let mut subnets = IceGraphStore::default();
        let sid = subnets.create_subnet("Scatter".into());
        {
            let g = subnets.get_mut(sid).unwrap();
            let root = g.root().unwrap();
            if let Some(n) = g.nodes.iter_mut().find(|n| n.id == root) { n.node_type = IceGraphNodeType::Root { context: Context::Object }; }
            let at = g.add_node("Scatter".into(), IceGraphNodeType::Scatter { seed: 1, weight: String::new() }, egui::pos2(0.0, 0.0));
            if let Some(n) = g.nodes.iter_mut().find(|n| n.id == at) { n.inputs[0].value = Some(Value::Int(12)); }
            let protos = g.add_node("Get".into(), IceGraphNodeType::GetReference { input: 1, paths: vec![] }, egui::pos2(0.0, 0.0));
            let inst = g.add_node("Instancer".into(), IceGraphNodeType::Instancer { path: "scatter".into(), seed: 0, draw_mode: Default::default() }, egui::pos2(0.0, 0.0));
            g.add_connection(at, 0, inst, 0);
            g.add_connection(protos, 0, inst, 1);
            g.add_connection(inst, 0, root, 0);
        }
        // The scene: a cube; its reference: another cube, the prototype.
        let mut graph = NodeGraphState::default();
        let ground = graph.add_node("Ground".into(), NodeType::cube(4.0), egui::pos2(0.0, 0.0));
        let proto = graph.add_node("Proto".into(), NodeType::cube(0.2), egui::pos2(0.0, 0.0));
        let ice = graph.add_node("ICE".into(), NodeType::Ice { id: sid, name: "Scatter".into(), references: 1 }, egui::pos2(0.0, 0.0));
        graph.add_connection(ground, 0, ice, 0);
        graph.add_connection(proto, 0, ice, 1);
        let eval = |sid: IceGraphId, mesh: &Geo, refs: &[Reference]| subnets.get(sid).map(|sg| sg.evaluate(mesh, refs)).unwrap_or_else(|| mesh.clone());
        let Some(EvalResult::Named(prims)) = graph.eval_node_out(ice, 0, &mut HashMap::new(), &eval) else { panic!("prims come out") };
        // The scene's cube; beside it a point instancer of 12 points, and the
        // prototype's cube under it, drawn at each of them.
        let ground_path = prims[0].path.clone();
        let parent = ground_path.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default();
        let inst = prims.iter().find(|p| p.instancing.is_some()).expect("a point instancer");
        assert_eq!(inst.path, format!("{parent}/scatter"), "beside the primitive, not under it");
        assert_eq!(inst.geo.as_ref().unwrap().points().len(), 12);
        let protos: Vec<_> = prims.iter().filter(|p| p.instanced_by.is_some()).collect();
        assert_eq!(protos.len(), 1, "{:?}", prims.iter().map(|p| &p.path).collect::<Vec<_>>());
        assert!(protos[0].path.starts_with(&format!("{}/Prototypes/", inst.path)), "{}", protos[0].path);
        assert!(matches!(&protos[0].place, Placement::Many(m) if m.len() == 12));
        assert_eq!(inst.instancing.as_ref().unwrap().len(), 1);
    }
}
