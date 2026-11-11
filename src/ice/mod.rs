//! ICE graphs: the graph inside an ICE operator, as the editor holds it (nodes,
//! wires, positions, values on unconnected inputs), and running it on a
//! primitive's geometry with the ICE engine (`xms_ice`).
//!
//! A graph ends in its **Execution Ports** root (named ExecutionPorts):
//! its Execute ports run in order,
//! over every element of its context (points by default). Set Data nodes
//! wired into it write attributes; everything else is reached from them.

pub mod compound;
pub mod ui;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use xms_ice::{Input, Math, Node, Op, Value};

use crate::core::geo::{Context, Geo};
use crate::types::{ConnectionId, IceGraphId, IceGraphNodeType, NodeId};

/// What one of the ICE operator's reference inputs brings into its graph: every
/// prim of the scene wired there, by path (Get Reference lists them), and the
/// meshes among them, where they are in the world (Closest Point on Surface
/// searches them).
pub type Reference = xms_ice::Reference;

thread_local! {
    /// Where the primitive the next graph runs on is in the world: the
    /// reference meshes are taken into its space. Set by the ICE operator
    /// before each primitive (`set_space`), taken by `evaluate`.
    static SPACE: std::cell::Cell<glam::Mat4> = const { std::cell::Cell::new(glam::Mat4::IDENTITY) };
    /// What the Instancers of the last graph run on this thread made. The
    /// evaluator's type gives back geometry only; the ICE operator, which runs
    /// a graph and then turns its instances into prims, takes them from here
    /// (`take_instances`).
    static INSTANCES: std::cell::RefCell<Vec<(xms_ice::Instances, crate::types::DrawMode)>> = const { std::cell::RefCell::new(Vec::new()) };
    /// The values the ICE operator sets on its graph's Parameter nodes, by
    /// name (`set_params`), taken by `evaluate` as `SPACE` is.
    static PARAMS: std::cell::RefCell<std::collections::BTreeMap<String, serde_json::Value>> = std::cell::RefCell::new(std::collections::BTreeMap::new());
    /// The key each graph last ran with on this thread (`IceGraph::cook_key`),
    /// by its id: the scene graph checks it is the one it kept the result
    /// under (`ran`).
    static RAN: std::cell::RefCell<HashMap<IceGraphId, u64>> = std::cell::RefCell::new(HashMap::new());
}

/// The key graph `id` last ran with on this thread, taken.
pub fn ran(id: IceGraphId) -> Option<u64> {
    RAN.with(|r| r.borrow_mut().remove(&id))
}

/// The next graph run on this thread takes these values for its parameters
/// (`NodeType::Ice::params`): its Parameter nodes', and its node settings
/// promoted (`IceGraph::parameters`).
pub fn set_params(params: std::collections::BTreeMap<String, serde_json::Value>) {
    PARAMS.with(|p| *p.borrow_mut() = params);
}

/// The instances the last graph run on this thread made, taken: empty until
/// another runs.
/// Each with the draw mode its Instancer gives its prototypes.
pub fn take_instances() -> Vec<(xms_ice::Instances, crate::types::DrawMode)> {
    INSTANCES.with(|i| std::mem::take(&mut *i.borrow_mut()))
}

/// The next graph run on this thread runs on a primitive at `world`.
pub fn set_space(world: glam::Mat4) {
    SPACE.with(|s| s.set(world));
}

#[derive(Default)]
pub struct GraphNavigation {
    pub current_subnet: Option<IceGraphId>,
    /// The MaterialX operator whose graph the Node Graph pane shows.
    pub materialx: Option<NodeId>,
}

#[derive(Clone)]
pub struct IceGraphInputSocket {
    pub name: String,
    pub value_hint: &'static str,
    pub connected_output: Option<(NodeId, usize)>,
    /// What the input takes while nothing is wired to it. `None` for Execute
    /// ports.
    pub value: Option<Value>,
}

#[derive(Clone)]
pub struct IceGraphOutputSocket {
    pub name: String,
    pub value_hint: &'static str,
}

#[derive(Clone)]
pub struct IceGraphNode {
    pub id: NodeId,
    pub name: String,
    pub node_type: IceGraphNodeType,
    pub position: egui::Pos2,
    pub inputs: Vec<IceGraphInputSocket>,
    pub outputs: Vec<IceGraphOutputSocket>,
}

#[derive(Clone)]
pub struct IceGraphConnection {
    pub id: ConnectionId,
    pub from_node: NodeId,
    pub from_output: usize,
    pub to_node: NodeId,
    pub to_input: usize,
}

/// The name a new graph's root is given (its type: Execution Ports). A root
/// still named as before, `ICE Tree`, is renamed when read.
pub const ROOT_NAME: &str = "ExecutionPorts";

#[derive(Clone)]
pub struct IceGraph {
    pub id: IceGraphId,
    pub name: String,
    pub nodes: Vec<IceGraphNode>,
    pub connections: Vec<IceGraphConnection>,
    pub next_node_id: usize,
    pub next_connection_id: usize,
    pub selected_node: Option<NodeId>,
    /// Every node selected (a box, or Shift+click): moved and deleted together.
    /// `selected_node` is the one whose parameters are shown.
    pub selected_nodes: Vec<NodeId>,
    /// The wire selected: Delete takes it off.
    pub selected_wire: Option<ConnectionId>,
    /// Where a box being dragged out to select nodes started, on the canvas.
    pub marquee: Option<egui::Pos2>,
    /// A port clicked to unplug: a click on the empty canvas takes its wires
    /// off (`graph_ports`).
    pub unplug: Option<crate::graph_ports::Picked<NodeId, usize>>,
    /// The inputs whose wires were picked up off an output (Ctrl+drag): let go
    /// of on another output they go there, anywhere else they are gone.
    pub carrying: Vec<(NodeId, usize)>,
    /// The wire being drawn was dragged, not clicked: let go of over no input,
    /// it is gone.
    pub wire_dragged: bool,
    pub dragging_node: Option<NodeId>,
    pub drag_offset: egui::Vec2,
    pub connecting_from: Option<(NodeId, usize)>,
    pub pan_offset: egui::Vec2,
    /// View scale of the tree: 1 is actual size.
    pub zoom: f32,
    /// Why the graph did not compile the last time it ran, and at which node:
    /// shown on that node and above the canvas.
    pub error: Arc<Mutex<Option<(NodeId, String)>>>,
    /// The way to that node when it is inside a compound: the compound, then
    /// the node inside it, and so on (see `compound::Flat::trail`); empty
    /// when the graph ran.
    pub broke: Arc<Mutex<Vec<NodeId>>>,
    /// The type of each node's output on the geometry the graph last ran on
    /// (`refresh_types`), for the colours of its ports and wires.
    pub types: Arc<Mutex<HashMap<NodeId, xms_ice::Ty>>>,
    /// The nodes whose output is an array (`refresh_types`): drawn and named
    /// apart from single values.
    pub arrays: Arc<Mutex<std::collections::HashSet<NodeId>>>,
    /// The geometry the graph last ran on, sharing its columns (a copy costs
    /// next to nothing): the editor works out the types and Get Data's list
    /// from it when it draws, not every run.
    pub last_input: Arc<Mutex<Option<Geo>>>,
    /// The type of each output of its ICE compounds, and whether it is an
    /// array (`refresh_types`).
    pub ports: Arc<Mutex<HashMap<(NodeId, usize), (Option<xms_ice::Ty>, bool)>>>,
    /// While an ICE compound's inside is looked at (this graph): the graphs
    /// its instance is in, outermost first (see `compound`).
    pub outer: Vec<compound::Outer>,
    /// Its nodes' settings promoted (a seed, a path): the ICE operator's
    /// parameters, or, inside an ICE compound, the compound's.
    pub parameters: Vec<crate::compounds::Parameter>,
}

/// No transform: the identity matrix, as a value.
pub fn identity() -> Value {
    Value::Mat4(glam::Mat4::IDENTITY.to_cols_array())
}

/// A new Snippet node's text.
pub const SNIPPET: &str =
    "snippet lift (float amount = 0.1 [[ string label = \"Amount\" ]])\n{\n    @P.y += amount;\n}\n";

/// A snippet's ports, from its signature: an input per parameter (its default
/// as its value; a geometry none, for Get Reference's paths), an output per
/// `output` parameter, then the Execute. Text that does not parse: just the
/// Execute.
pub fn snippet_sockets(text: &str) -> (Vec<IceGraphInputSocket>, Vec<IceGraphOutputSocket>) {
    let ports = xms_ice::lang::parse(text)
        .map(|s| xms_ice::lang::ports(&s))
        .unwrap_or_default();
    let inputs = ports
        .iter()
        .filter(|p| !p.output)
        .map(|p| IceGraphInputSocket {
            name: p.name.clone(),
            value_hint: if p.ty == xms_ice::Ty::Token { "Token" } else { "Any" },
            connected_output: None,
            value: p.default.or((p.ty != xms_ice::Ty::Token).then_some(Value::Float(0.0))),
        })
        .collect();
    let mut outputs: Vec<IceGraphOutputSocket> = ports
        .iter()
        .filter(|p| p.output)
        .map(|p| IceGraphOutputSocket {
            name: p.name.clone(),
            value_hint: type_hint(p.ty),
        })
        .collect();
    outputs.push(IceGraphOutputSocket {
        name: "Execute".into(),
        value_hint: "Execute",
    });
    (inputs, outputs)
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
        xms_ice::Ty::Mat4 => "Mat4",
        xms_ice::Ty::Sample => "Sample",
        xms_ice::Ty::Token => "Token",
    }
}

impl IceGraph {
    pub fn new(id: IceGraphId, name: String) -> Self {
        let mut g = Self {
            id,
            name,
            nodes: vec![],
            connections: vec![],
            next_node_id: 0,
            next_connection_id: 0,
            selected_node: None,
            selected_nodes: vec![],
            selected_wire: None,
            marquee: None,
            unplug: None,
            carrying: vec![],
            wire_dragged: false,
            dragging_node: None,
            drag_offset: egui::Vec2::ZERO,
            connecting_from: None,
            pan_offset: egui::Vec2::ZERO,
            zoom: 1.0,
            error: Arc::default(),
            broke: Arc::default(),
            types: Arc::default(),
            arrays: Arc::default(),
            last_input: Arc::default(),
            ports: Arc::default(),
            outer: vec![],
            parameters: vec![],
        };
        g.add_node(
            ROOT_NAME.into(),
            IceGraphNodeType::Root {
                context: Context::Point,
            },
            egui::pos2(500.0, 200.0),
        );
        g
    }

    /// The graph's root: the Execution Ports node.
    pub fn root(&self) -> Option<NodeId> {
        self.nodes
            .iter()
            .find(|n| matches!(n.node_type, IceGraphNodeType::Root { .. }))
            .map(|n| n.id)
    }

    /// Add a node named `name`, or `name2`, `name3`… when a node has it:
    /// its settings are promoted by its name, which has to be its own.
    pub fn add_node(&mut self, name: String, t: IceGraphNodeType, pos: egui::Pos2) -> NodeId {
        let base = crate::node_graph::node_name(&name);
        let name = (1..)
            .map(|k| if k == 1 { base.clone() } else { format!("{base}{k}") })
            .find(|n| !self.nodes.iter().any(|m| m.name == *n))
            .unwrap_or_default();
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        let (inputs, outputs) = Self::create_sockets(&t);
        self.nodes.push(IceGraphNode {
            id,
            name,
            node_type: t,
            position: pos,
            inputs,
            outputs,
        });
        id
    }

    /// A node's ports. Unconnected inputs start with a value that leaves what
    /// comes in unchanged where it can: 0 to add, 1 to multiply.
    pub fn create_sockets(t: &IceGraphNodeType) -> (Vec<IceGraphInputSocket>, Vec<IceGraphOutputSocket>) {
        let input = |name: &str, value: f32| IceGraphInputSocket {
            name: name.into(),
            value_hint: "Any",
            connected_output: None,
            value: Some(Value::Float(value)),
        };
        let output = |name: &str, hint: &'static str| IceGraphOutputSocket {
            name: name.into(),
            value_hint: hint,
        };
        match t {
            IceGraphNodeType::Root { .. } => (vec![execute_port(1)], vec![]),
            // Polygon and Param UV, unwired: the geometry's own elements.
            // Geometry, unwired: the geometry the graph runs on (or, with a
            // Polygon, the surface it was found on).
            IceGraphNodeType::GetData { .. } => (read_inputs(), vec![output("Value", "Any")]),
            IceGraphNodeType::SetData { .. } => (vec![input("Value", 0.0)], vec![output("Execute", "Execute")]),
            IceGraphNodeType::Constant { value } => (vec![], vec![output("Value", type_hint(value.ty()))]),
            IceGraphNodeType::Math { op } => {
                let inputs = match op {
                    Math::Multiply | Math::Divide => vec![input("A", 1.0), input("B", 1.0)],
                    Math::Lerp => vec![input("A", 0.0), input("B", 1.0), input("T", 0.5)],
                    Math::Rotate => vec![
                        IceGraphInputSocket {
                            value: Some(Value::Quat([0.0, 0.0, 0.0, 1.0])),
                            ..input("Rotation", 0.0)
                        },
                        IceGraphInputSocket {
                            value: Some(Value::Vec3([1.0, 0.0, 0.0])),
                            ..input("Vector", 0.0)
                        },
                    ],
                    Math::AxisAngle => vec![
                        IceGraphInputSocket {
                            value: Some(Value::Vec3([0.0, 1.0, 0.0])),
                            ..input("Axis", 0.0)
                        },
                        input("Angle", 0.0),
                    ],
                    Math::Euler => vec![IceGraphInputSocket {
                        value: Some(Value::Vec3([0.0; 3])),
                        ..input("Angles", 0.0)
                    }],
                    // A matrix, none (the identity) when not wired.
                    Math::Invert | Math::Transpose => vec![IceGraphInputSocket {
                        value: Some(identity()),
                        ..input("Matrix", 0.0)
                    }],
                    Math::TransformPoint => vec![
                        IceGraphInputSocket {
                            value: Some(identity()),
                            ..input("Matrix", 0.0)
                        },
                        IceGraphInputSocket {
                            value: Some(Value::Vec3([0.0; 3])),
                            ..input("Position", 0.0)
                        },
                    ],
                    Math::TransformVector => vec![
                        IceGraphInputSocket {
                            value: Some(identity()),
                            ..input("Matrix", 0.0)
                        },
                        IceGraphInputSocket {
                            value: Some(Value::Vec3([0.0, 1.0, 0.0])),
                            ..input("Vector", 0.0)
                        },
                    ],
                    Math::Distance => vec![
                        IceGraphInputSocket {
                            value: Some(Value::Vec3([0.0; 3])),
                            ..input("A", 0.0)
                        },
                        IceGraphInputSocket {
                            value: Some(Value::Vec3([0.0; 3])),
                            ..input("B", 0.0)
                        },
                    ],
                    Math::And | Math::Or => vec![
                        IceGraphInputSocket {
                            value: Some(Value::Bool(false)),
                            ..input("A", 0.0)
                        },
                        IceGraphInputSocket {
                            value: Some(Value::Bool(false)),
                            ..input("B", 0.0)
                        },
                    ],
                    Math::Not => vec![IceGraphInputSocket {
                        value: Some(Value::Bool(false)),
                        ..input("Value", 0.0)
                    }],
                    Math::Select => vec![
                        IceGraphInputSocket {
                            value: Some(Value::Bool(true)),
                            ..input("Condition", 0.0)
                        },
                        input("If True", 1.0),
                        input("If False", 0.0),
                    ],
                    Math::Pow => vec![input("Base", 2.0), input("Exponent", 2.0)],
                    Math::Mod => vec![input("A", 0.0), input("B", 1.0)],
                    Math::Atan2 => vec![input("Y", 0.0), input("X", 1.0)],
                    op if op.arity() == 1 => vec![input("Value", 0.0)],
                    _ => vec![input("A", 0.0), input("B", 0.0)],
                };
                (inputs, vec![output("Result", "Any")])
            }
            // A count of 0: one value per element. The Seed, unwired: each
            // element's own number.
            IceGraphNodeType::Random { .. } => (
                vec![
                    input("Min", 0.0),
                    input("Max", 1.0),
                    IceGraphInputSocket {
                        value: Some(Value::Int(0)),
                        ..input("Count", 0.0)
                    },
                    IceGraphInputSocket {
                        value: None,
                        ..input("Seed", 0.0)
                    },
                ],
                vec![output("Value", "Any")],
            ),
            IceGraphNodeType::Concat => (vec![array_port(1), array_port(2)], vec![output("Array", "Any")]),
            // Its parts, by what it makes: numbers 0 (a rotation's real
            // part 1: none), vectors zero.
            IceGraphNodeType::Compose { op } => (
                op.inputs()
                    .iter()
                    .map(|&(name, ty)| {
                        let value = match ty {
                            // A matrix's axes, unwired: the world's.
                            xms_ice::Ty::Vec3 if name == "X Axis" => Value::Vec3([1.0, 0.0, 0.0]),
                            xms_ice::Ty::Vec3 if name == "Y Axis" => Value::Vec3([0.0, 1.0, 0.0]),
                            xms_ice::Ty::Vec3 if name == "Z Axis" => Value::Vec3([0.0, 0.0, 1.0]),
                            xms_ice::Ty::Vec2 => Value::Vec2([0.0; 2]),
                            xms_ice::Ty::Vec3 => Value::Vec3([0.0; 3]),
                            _ if name == "Real" => Value::Float(1.0),
                            _ => Value::Float(0.0),
                        };
                        IceGraphInputSocket {
                            value: Some(value),
                            ..input(name, 0.0)
                        }
                    })
                    .collect(),
                vec![output("Vector", "Any")],
            ),
            // What it takes apart, zero (no rotation) when not wired; an
            // output per part.
            IceGraphNodeType::Decompose { op } => {
                let (name, value) = match op.input() {
                    xms_ice::Ty::Vec2 => ("Vector", Value::Vec2([0.0; 2])),
                    xms_ice::Ty::Vec4 => ("Vector", Value::Vec4([0.0; 4])),
                    xms_ice::Ty::Quat => ("Rotation", Value::Quat([0.0, 0.0, 0.0, 1.0])),
                    xms_ice::Ty::Mat4 => ("Matrix", identity()),
                    _ => ("Vector", Value::Vec3([0.0; 3])),
                };
                (
                    vec![IceGraphInputSocket {
                        value: Some(value),
                        ..input(name, 0.0)
                    }],
                    op.outputs().iter().map(|(n, _)| output(*n, "Any")).collect(),
                )
            }
            // Outputs in `xms_ice::SCATTER_*`'s order.
            IceGraphNodeType::Scatter { .. } => (
                vec![IceGraphInputSocket {
                    value: Some(Value::Int(1000)),
                    ..input("Count", 0.0)
                }],
                vec![
                    output("Positions", "Any"),
                    output("Normals", "Any"),
                    output("Face Index", "Any"),
                    output("Param UV", "Any"),
                ],
            ),
            // Outputs in `xms_ice::CLOSEST_*`'s order. The Origin, the
            // points', is wired; a radius of 0: anywhere.
            IceGraphNodeType::ClosestPoint => (
                vec![
                    IceGraphInputSocket {
                        value: None,
                        ..input("Geometry", 0.0)
                    },
                    IceGraphInputSocket {
                        value: Some(Value::Vec3([0.0; 3])),
                        ..input("Origin", 0.0)
                    },
                    input("Radius", 0.0),
                ],
                vec![
                    output("Face Index", "Any"),
                    output("Distance", "Any"),
                    output("Param UV", "Any"),
                ],
            ),
            // Outputs in `xms_ice::NEAR_*`'s order. The Geometry unwired: the
            // graph's own points; a radius of 0: anywhere; a count of 0: all
            // within the radius.
            IceGraphNodeType::NearPoints => (
                vec![
                    IceGraphInputSocket {
                        value: None,
                        ..input("Geometry", 0.0)
                    },
                    IceGraphInputSocket {
                        value: Some(Value::Vec3([0.0; 3])),
                        ..input("Origin", 0.0)
                    },
                    input("Radius", 0.0),
                    IceGraphInputSocket {
                        value: Some(Value::Int(1)),
                        ..input("Count", 0.0)
                    },
                ],
                vec![output("Points", "Any"), output("Distances", "Any")],
            ),
            // Normals +Y (none) and a random turn about it, when not wired;
            // a count of 0, one per element (or per normal of an array).
            IceGraphNodeType::RandomOrientation { .. } => (
                vec![
                    IceGraphInputSocket {
                        value: Some(Value::Vec3([0.0, 1.0, 0.0])),
                        ..input("Normals", 0.0)
                    },
                    IceGraphInputSocket {
                        value: Some(Value::Vec3([0.0, 360.0, 0.0])),
                        ..input("Random Rotation", 0.0)
                    },
                    IceGraphInputSocket {
                        value: Some(Value::Int(0)),
                        ..input("Count", 0.0)
                    },
                    IceGraphInputSocket {
                        value: None,
                        ..input("Seed", 0.0)
                    },
                ],
                vec![output("Orientations", "Any")],
            ),
            IceGraphNodeType::GetReference { .. } => (vec![], vec![output("Paths", "Any")]),
            IceGraphNodeType::GetArrayData { .. } => (read_inputs(), vec![output("Array", "Any")]),
            // A PointInstancer's attributes; one orientation and scale for
            // all when not wired, a prototype picked at random (-1).
            IceGraphNodeType::Instancer { .. } => (
                vec![
                    IceGraphInputSocket {
                        value: None,
                        ..input("Positions", 0.0)
                    },
                    IceGraphInputSocket {
                        value: None,
                        ..input("Prototypes", 0.0)
                    },
                    IceGraphInputSocket {
                        value: Some(Value::Int(-1)),
                        ..input("Proto Indices", 0.0)
                    },
                    IceGraphInputSocket {
                        value: Some(Value::Quat([0.0, 0.0, 0.0, 1.0])),
                        ..input("Orientations", 0.0)
                    },
                    input("Scales", 1.0),
                ],
                vec![output("Execute", "Execute")],
            ),
            IceGraphNodeType::Compound { .. } => compound::sockets(t),
            IceGraphNodeType::Snippet { text, .. } => snippet_sockets(text),
            IceGraphNodeType::CompoundInput { .. } | IceGraphNodeType::Parameter { .. } => {
                (vec![], vec![output("Value", "Any")])
            }
            // Execute or a value: whatever feeds it.
            IceGraphNodeType::CompoundOutput { .. } => (
                vec![IceGraphInputSocket {
                    value: None,
                    ..input("Value", 0.0)
                }],
                vec![],
            ),
            IceGraphNodeType::Array { op } => {
                let mut inputs = vec![IceGraphInputSocket {
                    value: None,
                    ..input("Array", 0.0)
                }];
                if *op == xms_ice::ArrayOp::Element {
                    inputs.push(IceGraphInputSocket {
                        value: Some(Value::Int(0)),
                        ..input("Index", 0.0)
                    });
                }
                (inputs, vec![output("Result", "Any")])
            }
        }
    }

    /// A node's ports made again for what it now is (Compose changed what it
    /// makes, Decompose what it takes apart): the wires into inputs and out of
    /// outputs still there kept, the others taken off.
    pub fn resync_sockets(&mut self, id: NodeId) {
        let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) else {
            return;
        };
        if matches!(n.node_type, IceGraphNodeType::Snippet { .. }) {
            return self.resync_by_name(id);
        }
        let (mut inputs, outputs) = Self::create_sockets(&n.node_type);
        for (new, old) in inputs.iter_mut().zip(&n.inputs) {
            new.connected_output = old.connected_output;
        }
        let (ins, outs) = (inputs.len(), outputs.len());
        n.inputs = inputs;
        n.outputs = outputs;
        self.connections
            .retain(|c| (c.to_node != id || c.to_input < ins) && (c.from_node != id || c.from_output < outs));
        // Inputs of other nodes wired to an output that is gone: free.
        for other in self.nodes.iter_mut() {
            for input in other.inputs.iter_mut() {
                if input.connected_output.is_some_and(|(from, k)| from == id && k >= outs) {
                    input.connected_output = None;
                }
            }
        }
    }

    /// A snippet's ports made again from its signature: wires and values kept
    /// by the parameter's name, wherever it now is; the Execute stays last.
    fn resync_by_name(&mut self, id: NodeId) {
        let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) else {
            return;
        };
        let (mut inputs, outputs) = Self::create_sockets(&n.node_type);
        let input_at: Vec<Option<usize>> = n
            .inputs
            .iter()
            .map(|old| inputs.iter().position(|i| i.name == old.name))
            .collect();
        let output_at: Vec<Option<usize>> = n
            .outputs
            .iter()
            .map(|old| outputs.iter().position(|o| o.name == old.name))
            .collect();
        for (old, at) in n.inputs.iter().zip(&input_at) {
            if let Some(new) = at.and_then(|k| inputs.get_mut(k)) {
                new.connected_output = old.connected_output;
                // A value of the type the port now takes; the new default
                // otherwise.
                if old.value.map(|v| v.ty()) == new.value.map(|v| v.ty()) {
                    new.value = old.value;
                }
            }
        }
        n.inputs = inputs;
        n.outputs = outputs;
        self.connections.retain_mut(|c| {
            if c.to_node == id {
                match input_at.get(c.to_input).copied().flatten() {
                    Some(k) => c.to_input = k,
                    None => return false,
                }
            }
            if c.from_node == id {
                match output_at.get(c.from_output).copied().flatten() {
                    Some(k) => c.from_output = k,
                    None => return false,
                }
            }
            true
        });
        for other in self.nodes.iter_mut() {
            for input in other.inputs.iter_mut() {
                if let Some((from, k)) = input.connected_output.filter(|(from, _)| *from == id) {
                    input.connected_output = output_at.get(k).copied().flatten().map(|k| (from, k));
                }
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
            from_node: from,
            from_output: from_out,
            to_node: to,
            to_input: to_in,
        });
    }

    /// Delete a node and the wires to and from it. The root stays: every ICE
    /// graph has one. Returns whether it was deleted.
    pub fn remove_node(&mut self, id: NodeId) -> bool {
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            return false;
        };
        if matches!(node.node_type, IceGraphNodeType::Root { .. }) {
            return false;
        }
        let wires: Vec<ConnectionId> = self
            .connections
            .iter()
            .filter(|c| c.from_node == id || c.to_node == id)
            .map(|c| c.id)
            .collect();
        for cid in wires {
            self.remove_connection(cid);
        }
        self.nodes.retain(|n| n.id != id);
        if self.selected_node == Some(id) {
            self.selected_node = None;
        }
        self.selected_nodes.retain(|n| *n != id);
        if self.dragging_node == Some(id) {
            self.dragging_node = None;
        }
        if self.connecting_from.is_some_and(|(n, _)| n == id) {
            self.connecting_from = None;
        }
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
        for id in ids {
            self.remove_node(id);
        }
    }

    /// Take off every wire of a picked port (`graph_ports::Picked`).
    pub fn unplug(&mut self, picked: &crate::graph_ports::Picked<NodeId, usize>) {
        let off: Vec<ConnectionId> = self
            .connections
            .iter()
            .filter(|c| picked.holds(c.from_node, &c.from_output, c.to_node, &c.to_input))
            .map(|c| c.id)
            .collect();
        for cid in off {
            self.remove_connection(cid);
        }
    }

    pub fn remove_connection(&mut self, cid: ConnectionId) {
        if let Some(c) = self.connections.iter().find(|c| c.id == cid) {
            let (tn, ti) = (c.to_node, c.to_input);
            if let Some(n) = self.nodes.iter_mut().find(|n| n.id == tn) {
                if let Some(inp) = n.inputs.get_mut(ti) {
                    inp.connected_output = None;
                }
            }
        }
        self.connections.retain(|c| c.id != cid);
        if self.selected_wire == Some(cid) {
            self.selected_wire = None;
        }
    }

    /// The graph, its compounds flattened (`compound::flatten`), as the engine
    /// takes it, and the node at each of its positions (so an error can be
    /// shown on the right node).
    fn engine(&self) -> (xms_ice::Graph, Vec<NodeId>) {
        let ids: Vec<NodeId> = self.nodes.iter().map(|n| n.id).collect();
        let nodes = self
            .nodes
            .iter()
            .map(|n| {
                let root = matches!(n.node_type, IceGraphNodeType::Root { .. });
                // Get Data and Get Array Data read where `read_at` says, from
                // another prim when their Geometry is wired. The engine takes
                // [Index, Param UV, Geometry], [Index, Param UV], [Index,
                // Geometry], [Index] or [Geometry]: an unwired input is not
                // one to run, a Param UV given as a value is.
                let reads = matches!(
                    n.node_type,
                    IceGraphNodeType::GetData { .. } | IceGraphNodeType::GetArrayData { .. }
                );
                let at = read_at(n);
                let sampled = at == ReadAt::Place;
                let op = match &n.node_type {
                    IceGraphNodeType::Root { context } => Op::Root { context: *context },
                    IceGraphNodeType::GetData { name, context, .. } if sampled => Op::GetDataAt {
                        name: name.clone(),
                        context: *context,
                    },
                    IceGraphNodeType::GetData { name, context, .. } if at == ReadAt::Index => Op::GetDataAtIndex {
                        name: name.clone(),
                        context: *context,
                    },
                    IceGraphNodeType::GetArrayData { name, context } if at == ReadAt::Index => {
                        Op::GetArrayDataAtIndex {
                            name: name.clone(),
                            context: *context,
                        }
                    }
                    IceGraphNodeType::GetArrayData { name, context } if sampled => Op::GetArrayDataAt {
                        name: name.clone(),
                        context: *context,
                    },
                    IceGraphNodeType::GetData {
                        name,
                        context,
                        resample,
                    } => Op::GetData {
                        name: name.clone(),
                        context: *context,
                        resample: *resample,
                    },
                    IceGraphNodeType::SetData {
                        name,
                        context,
                        resample,
                    } => Op::SetData {
                        name: name.clone(),
                        context: *context,
                        resample: *resample,
                    },
                    IceGraphNodeType::Constant { value } => Op::Constant(*value),
                    IceGraphNodeType::Math { op } => Op::Math(*op),
                    IceGraphNodeType::Random { seed } => Op::Random { seed: *seed },
                    IceGraphNodeType::Array { op } => Op::Array(*op),
                    IceGraphNodeType::Compose { op } => Op::Compose(*op),
                    IceGraphNodeType::Decompose { op } => Op::Decompose(*op),
                    IceGraphNodeType::Scatter { seed, weight } => Op::Scatter {
                        seed: *seed,
                        weight: weight.clone(),
                    },
                    IceGraphNodeType::ClosestPoint => Op::ClosestPoint,
                    IceGraphNodeType::NearPoints => Op::NearPoints,
                    IceGraphNodeType::GetArrayData { name, context } => Op::GetArrayData {
                        name: name.clone(),
                        context: *context,
                    },
                    IceGraphNodeType::RandomOrientation { seed } => Op::RandomOrientation { seed: *seed },
                    IceGraphNodeType::Concat => Op::Concat,
                    // Rows as they are: `evaluate` turns them into the paths they
                    // name on the reference input.
                    IceGraphNodeType::GetReference { input, paths } => Op::GetReference {
                        input: *input as usize,
                        paths: crate::core::locations::is_set(paths).then(|| paths.clone()),
                    },
                    IceGraphNodeType::Instancer { path, seed, .. } => Op::Instancer {
                        path: path.clone(),
                        seed: *seed,
                    },
                    IceGraphNodeType::Parameter { name, default } => Op::Parameter {
                        name: name.clone(),
                        default: *default,
                    },
                    // Lowered into nodes of its own by `expand_snippets`.
                    IceGraphNodeType::Snippet { .. } => Op::Constant(Value::Float(0.0)),
                    // Gone once flattened.
                    IceGraphNodeType::CompoundInput { value, .. } => Op::Constant(*value),
                    IceGraphNodeType::Compound { .. } | IceGraphNodeType::CompoundOutput { .. } => {
                        Op::Constant(Value::Float(0.0))
                    }
                };
                let inputs = n
                    .inputs
                    .iter()
                    .enumerate()
                    .filter_map(|(k, i)| {
                        let wired = i.connected_output.and_then(|(from, output)| {
                            ids.iter()
                                .position(|id| *id == from)
                                .map(|node| Input::Wire { node, output })
                        });
                        match wired {
                            Some(w) => Some(w),
                            None if sampled && k == READ_PARAM_UV => i.value.map(Input::Value),
                            // The root's free Execute ports are not inputs to run,
                            // nor a read's unwired ones.
                            None if root || reads => None,
                            None => Some(Input::Value(i.value.unwrap_or(Value::Float(0.0)))),
                        }
                    })
                    .collect();
                Node { op, inputs }
            })
            .collect();
        (xms_ice::Graph { nodes }, ids)
    }

    /// Each Snippet node of `graph` (the engine's, from `engine`) lowered into
    /// nodes of its own, appended (their ids in `ids` the snippet's), and what
    /// was wired to it wired to what it became: an output to its value, the
    /// Execute to its Set Data nodes, each an Execute port of the root. `geo`:
    /// the geometry it runs on, for the types of what it reads. Where each new
    /// node comes from in its text, for the engine's errors; a snippet that
    /// does not lower, its node and why.
    fn expand_snippets(
        &self,
        graph: &mut xms_ice::Graph,
        ids: &mut Vec<NodeId>,
        geo: Option<&Geo>,
    ) -> Result<Vec<(usize, xms_ice::lang::Pos)>, (NodeId, String)> {
        let context = self
            .nodes
            .iter()
            .find_map(|n| match n.node_type {
                IceGraphNodeType::Root { context } => Some(context),
                _ => None,
            })
            .unwrap_or(Context::Point);
        let mut at = vec![];
        for (k, n) in self.nodes.iter().enumerate() {
            let IceGraphNodeType::Snippet { text, run_over } = &n.node_type else {
                continue;
            };
            let broke = |e: xms_ice::lang::Error| (n.id, e.to_string());
            let snippet = xms_ice::lang::parse(text).map_err(broke)?;
            let runs_over = run_over.or(xms_ice::lang::pinned(&snippet)).unwrap_or(context);
            let inputs: Vec<(String, Input)> = n
                .inputs
                .iter()
                .zip(graph.nodes[k].inputs.iter())
                .map(|(socket, input)| (socket.name.clone(), input.clone()))
                .collect();
            let first = graph.nodes.len();
            let lowered = xms_ice::lang::lower(&snippet, runs_over, &inputs, geo, graph).map_err(broke)?;
            ids.extend(std::iter::repeat(n.id).take(graph.nodes.len() - first));
            let execute = n.outputs.len().saturating_sub(1);
            for node in graph.nodes.iter_mut() {
                let root = matches!(node.op, Op::Root { .. });
                let inputs = std::mem::take(&mut node.inputs);
                for input in inputs {
                    match input {
                        Input::Wire { node: from, output } if from == k => {
                            if output == execute {
                                if root {
                                    node.inputs
                                        .extend(lowered.sets.iter().map(|s| xms_ice::graph::wire(*s)));
                                }
                            } else if let Some((_, v)) = lowered.outputs.get(output) {
                                node.inputs.push(v.clone());
                            }
                        }
                        other => node.inputs.push(other),
                    }
                }
            }
            at.extend(lowered.at);
        }
        Ok(at)
    }

    /// The nodes wired into the root, the root among them: the only ones a run
    /// computes.
    fn reaching_root(&self) -> HashSet<NodeId> {
        let mut found: HashSet<NodeId> = HashSet::new();
        let mut todo: Vec<NodeId> = self.root().into_iter().collect();
        while let Some(id) = todo.pop() {
            if !found.insert(id) {
                continue;
            }
            let Some(n) = self.nodes.iter().find(|n| n.id == id) else {
                continue;
            };
            todo.extend(n.inputs.iter().filter_map(|i| i.connected_output.map(|(from, _)| from)));
        }
        found
    }

    /// What the graph computes: the nodes wired into the root, what each input
    /// takes and the wires between them. Not where nodes are drawn, their names
    /// or what is selected, nor a node wired to nothing yet: moving a node or
    /// putting a new one down does not run the graph again.
    /// While an ICE compound's inside is looked at, the graph composed with it
    /// (`composed`).
    pub fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        if !self.outer.is_empty() {
            return self.composed().content_hash();
        }
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
        serde_json::to_string(&self.parameters).unwrap_or_default().hash(&mut h);
        h.finish()
    }

    /// What a run of it gives depends on, its input and parameters aside: what
    /// it computes (`content_hash`) and the definitions of the ICE compounds in
    /// it (one following the library changes with it).
    pub fn cook_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.content_hash().hash(&mut h);
        for n in &self.nodes {
            if matches!(n.node_type, IceGraphNodeType::Compound { .. }) {
                compound::definition(&n.node_type).map(|d| d.hash()).ok().hash(&mut h);
            }
        }
        h.finish()
    }

    /// Work out the type of every node's output, wired or not, on the geometry
    /// the graph last ran on. Cheap (no attribute is read), so the editor does
    /// it as it draws.
    pub fn refresh_types(&self) {
        let Some(input) = self.last_input.lock().ok().and_then(|g| g.clone()) else {
            return;
        };
        let Ok(flat) = compound::flatten(self) else {
            return;
        };
        let (mut graph, mut ids) = flat.graph.engine();
        // A snippet that does not lower: its error is the run's to show.
        let _ = flat.graph.expand_snippets(&mut graph, &mut ids, Some(&input));
        let types: HashMap<NodeId, xms_ice::Ty> = xms_ice::output_types(&graph, &input)
            .into_iter()
            .enumerate()
            .filter_map(|(k, t)| Some((*ids.get(k)?, t?)))
            .collect();
        let arrays: HashSet<NodeId> = xms_ice::output_arrays(&graph)
            .into_iter()
            .enumerate()
            .filter_map(|(k, a)| a.then(|| ids.get(k).copied()).flatten())
            .collect();
        // Each compound's outputs: what gives them, inside.
        let ports = flat
            .ports
            .iter()
            .map(|(&port, &(node, out))| {
                (
                    port,
                    (flat.graph.output_type_in(node, out, &types), arrays.contains(&node)),
                )
            })
            .collect();
        if let Ok(mut kept) = self.ports.lock() {
            *kept = ports;
        }
        if let Ok(mut kept) = self.types.lock() {
            *kept = types;
        }
        if let Ok(mut kept) = self.arrays.lock() {
            *kept = arrays;
        }
    }

    /// Whether an output gives an array, as `refresh_types` found.
    pub fn output_is_array(&self, node: NodeId, output: usize) -> bool {
        let Some(n) = self.nodes.iter().find(|n| n.id == node) else {
            return false;
        };
        if matches!(n.node_type, IceGraphNodeType::Compound { .. }) {
            return self
                .ports
                .lock()
                .is_ok_and(|p| p.get(&(node, output)).is_some_and(|t| t.1));
        }
        n.outputs.get(output).is_some_and(|o| o.value_hint != "Execute")
            && self.arrays.lock().is_ok_and(|a| a.contains(&node))
    }

    /// What Get Data can read on the geometry the graph last ran on: by
    /// context, each attribute's name and kind, stored ones then those made
    /// from the topology, as the Primitive Inspector lists them.
    #[cfg(test)]
    pub fn readable(&self) -> Vec<(Context, String, String)> {
        readable(&self.last_input)
    }
}

/// What Get Data can read on the geometry kept in `last` (see
/// `IceGraph::readable`).
pub fn readable(last: &Mutex<Option<Geo>>) -> Vec<(Context, String, String)> {
    let Some(input) = last.lock().ok().and_then(|g| g.clone()) else {
        return vec![];
    };
    let mut out: Vec<(Context, String, String)> = Context::ALL
        .into_iter()
        .flat_map(|c| {
            input
                .attrs(c)
                .map(move |(n, a)| (c, n.to_string(), format!("{:?}", a.column.kind())))
        })
        .collect();
    for c in Context::ALL {
        for i in xms_geo::intrinsics::of_geo(&input, c).filter(|i| i.ice && i.gathers.is_none()) {
            out.push((c, i.name.to_string(), format!("{:?}", i.kind)));
        }
    }
    out
}

impl IceGraph {
    /// The type an output gives, when known: a constant's own, else what
    /// `refresh_types` found.
    pub fn output_type(&self, node: NodeId, output: usize) -> Option<xms_ice::Ty> {
        let n = self.nodes.iter().find(|n| n.id == node)?;
        if matches!(n.node_type, IceGraphNodeType::Compound { .. }) {
            return self.ports.lock().ok()?.get(&(node, output))?.0;
        }
        let types = self.types.lock().ok()?;
        self.output_type_in(node, output, &types)
    }

    /// The type an output gives, a constant's own, else what `types` says.
    fn output_type_in(&self, node: NodeId, output: usize, types: &HashMap<NodeId, xms_ice::Ty>) -> Option<xms_ice::Ty> {
        let n = self.nodes.iter().find(|n| n.id == node)?;
        match n.outputs.get(output)?.value_hint {
            "Execute" => None,
            _ => match &n.node_type {
                IceGraphNodeType::Constant { value } => Some(value.ty()),
                // Positions and normals, then the Polygons and their Param UVs.
                IceGraphNodeType::Scatter { .. } => Some(match output {
                    xms_ice::SCATTER_POLYGONS => xms_ice::Ty::Int,
                    xms_ice::SCATTER_PARAM_UVS => xms_ice::Ty::Vec2,
                    _ => xms_ice::Ty::Vec3,
                }),
                // The Polygons, the distances, the Param UVs.
                IceGraphNodeType::ClosestPoint => Some(match output {
                    xms_ice::CLOSEST_DISTANCE => xms_ice::Ty::Float,
                    xms_ice::CLOSEST_PARAM_UVS => xms_ice::Ty::Vec2,
                    _ => xms_ice::Ty::Int,
                }),
                // The points' numbers, their distances.
                IceGraphNodeType::NearPoints => Some(match output {
                    xms_ice::NEAR_DISTANCES => xms_ice::Ty::Float,
                    _ => xms_ice::Ty::Int,
                }),
                // Each part its own type, whatever comes in.
                IceGraphNodeType::Decompose { op } => op.outputs().get(output).map(|o| o.1),
                IceGraphNodeType::Parameter { default, .. } => Some(default.ty()),
                // As its signature says.
                IceGraphNodeType::Snippet { text, .. } => {
                    let s = xms_ice::lang::parse(text).ok()?;
                    let outputs: Vec<_> = xms_ice::lang::ports(&s).into_iter().filter(|p| p.output).collect();
                    outputs.get(output).map(|p| p.ty)
                }
                _ => types.get(&node).copied(),
            },
        }
    }

    /// Run the graph on a geometry, in its own space. What it does not set
    /// stays shared with `input`. A graph that does not compile gives `input`
    /// back, and says why on the node that broke it.
    pub fn evaluate(&self, input: &Geo, references: &[Reference]) -> Geo {
        let key = self.cook_key();
        RAN.with(|r| r.borrow_mut().insert(self.id, key));
        // While an ICE compound's inside is looked at: the graph it is in,
        // with it.
        let composed = self.composed();
        let params = PARAMS.with(|p| std::mem::take(&mut *p.borrow_mut()));
        // Its node settings promoted, as the operator sets them.
        let set = compound::with_values(&composed, &composed.parameters, &params);
        let me: &IceGraph = &set;
        // Its Parameter nodes' values.
        let params: HashMap<String, Value> = params
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), serde_json::from_value(v.clone()).ok()?)))
            .collect();
        let space = SPACE.with(|s| s.replace(glam::Mat4::IDENTITY));
        // Kept for the editor: its columns are shared, not copied.
        if let Ok(mut last) = me.last_input.lock() {
            *last = Some(input.clone());
        }
        let broke = |way: Vec<NodeId>, message: String| {
            *me.error.lock().unwrap_or_else(|e| e.into_inner()) = way.first().map(|id| (*id, message));
            *me.broke.lock().unwrap_or_else(|e| e.into_inner()) = way;
        };
        let flat = match compound::flatten(me) {
            Ok(f) => f,
            Err((way, message)) => {
                broke(way, message);
                return input.clone();
            }
        };
        let (mut graph, mut ids) = flat.graph.engine();
        let at = match flat.graph.expand_snippets(&mut graph, &mut ids, Some(input)) {
            Ok(at) => at,
            Err((id, message)) => {
                broke(flat.trail(id), message);
                return input.clone();
            }
        };
        // Get Reference's rows, as Primitive Locations name prims: the paths
        // on its input each row names, row by row, each once.
        for node in &mut graph.nodes {
            if let Op::GetReference {
                input,
                paths: Some(rows),
            } = &mut node.op
            {
                let there = references
                    .get(*input - 1)
                    .map(|r| r.paths.as_slice())
                    .unwrap_or_default();
                let mut named: Vec<String> = vec![];
                for row in crate::core::locations::rows(rows) {
                    for p in there.iter().filter(|p| crate::core::locations::names(row, p)) {
                        if !named.contains(p) {
                            named.push(p.clone());
                        }
                    }
                }
                *rows = named;
            }
        }
        let result = xms_ice::compile_with(&graph, input, references).map(|plan| {
            let ran = xms_ice::run_in(&plan, input, space, &params, references);
            // Each with its Instancer's draw mode, found by its path.
            let made = ran
                .instances
                .into_iter()
                .map(|i| {
                    let mode = flat
                        .graph
                        .nodes
                        .iter()
                        .find_map(|n| match &n.node_type {
                            IceGraphNodeType::Instancer { path, draw_mode, .. } if path.trim() == i.path => {
                                Some(*draw_mode)
                            }
                            _ => None,
                        })
                        .unwrap_or_default();
                    (i, mode)
                })
                .collect();
            INSTANCES.with(|i| *i.borrow_mut() = made);
            ran.geo
        });
        match result {
            Ok(out) => {
                broke(vec![], String::new());
                out
            }
            Err(e) => {
                // In a snippet: where in its text.
                let message = match at.iter().find(|(n, _)| *n == e.node) {
                    Some((_, p)) => format!("line {}, col {}: {}", p.line, p.col, e.message),
                    None => e.message,
                };
                broke(ids.get(e.node).map(|id| flat.trail(*id)).unwrap_or_default(), message);
                input.clone()
            }
        }
    }
}

/// What Get Data and Get Array Data read that intrinsics were named before they
/// were `intrinsic:…`.
const OLD_INTRINSIC_NAMES: [&str; 11] = [
    "ptIndex",
    "curveu",
    "faceIndex",
    "corners",
    "firstCorner",
    "localCornerIndex",
    "area",
    "triP",
    "bary",
    "ptIndices",
    "faceIndices",
];

/// Saved nodes (`nodes`, as JSON, compounds in them too) reading intrinsics by
/// their names from before `intrinsic:…`, named as they are now: a face's `N`
/// read per face too. Not a name the graph sets itself (Set Data), which it
/// then reads.
pub fn upgrade_intrinsic_names(nodes: &mut [serde_json::Value]) {
    use serde_json::Value as J;
    fn walk(v: &mut J, f: &mut dyn FnMut(&str, &mut serde_json::Map<String, J>)) {
        match v {
            J::Object(o) => {
                for (k, x) in o.iter_mut() {
                    if let J::Object(fields) = x {
                        f(k, fields);
                    }
                }
                o.values_mut().for_each(|x| walk(x, f));
            }
            J::Array(a) => a.iter_mut().for_each(|x| walk(x, f)),
            _ => {}
        }
    }
    let mut set: Vec<String> = vec![];
    for n in nodes.iter_mut() {
        walk(n, &mut |kind: &str, fields: &mut serde_json::Map<String, J>| {
            if let (true, Some(J::String(name))) = (kind == "SetData", fields.get("name")) {
                set.push(name.trim().to_string());
            }
        });
    }
    for n in nodes.iter_mut() {
        walk(n, &mut |kind: &str, fields: &mut serde_json::Map<String, J>| {
            if !matches!(kind, "GetData" | "GetDataAt" | "GetArrayData" | "GetArrayDataAt") {
                return;
            }
            let per_face = fields.get("context").and_then(|c| c.as_str()) == Some("Face");
            if let Some(J::String(name)) = fields.get_mut("name") {
                let old = name.trim();
                let was = OLD_INTRINSIC_NAMES.contains(&old) || (old == "N" && per_face && kind.starts_with("GetData"));
                if was && !set.iter().any(|s| s == old) {
                    *name = format!("{}{old}", xms_geo::intrinsics::PREFIX);
                }
            }
        });
    }
}

/// Get Data's and Get Array Data's inputs, by number.
pub const READ_INDEX: usize = 0;
pub const READ_PARAM_UV: usize = 1;
pub const READ_GEOMETRY: usize = 2;

/// Where a Get Data or Get Array Data reads (see `read_at`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadAt {
    /// At its own elements: no Index wired (and not a read).
    Own,
    /// At a place on a face: an Index (a Polygon) and its Param UV, wired or
    /// given as a value, as Houdini's `primuv()`.
    Place,
    /// At the elements the Index numbers (a point's, a face's: its Context
    /// says which), as Houdini's `point()` and `prim()`.
    Index,
}

/// Where a Get Data or Get Array Data reads: at an Index wired with a Param
/// UV, a place; an Index alone, the elements it numbers; else its own.
pub fn read_at(n: &IceGraphNode) -> ReadAt {
    let reads = matches!(
        n.node_type,
        IceGraphNodeType::GetData { .. } | IceGraphNodeType::GetArrayData { .. }
    );
    let wired = |k: usize| n.inputs.get(k).is_some_and(|i| i.connected_output.is_some());
    let uv = n
        .inputs
        .get(READ_PARAM_UV)
        .is_some_and(|i| i.connected_output.is_some() || i.value.is_some());
    match (reads && wired(READ_INDEX), uv) {
        (false, _) => ReadAt::Own,
        (true, true) => ReadAt::Place,
        (true, false) => ReadAt::Index,
    }
}

/// Get Data's and Get Array Data's inputs: an Index to read at (element
/// numbers, or a Polygon with its Param UV: Closest Point on Surface's,
/// Scatter on Surface's, or stored), and a Geometry (Get Reference's paths) to
/// read on. None holds a value.
fn read_inputs() -> Vec<IceGraphInputSocket> {
    ["Index", "Param UV", "Geometry"]
        .into_iter()
        .map(|name| IceGraphInputSocket {
            name: name.into(),
            value_hint: "Any",
            connected_output: None,
            value: None,
        })
        .collect()
}

/// The root's Execute port number `n`. Concatenate Arrays' input `n` (from 1).
pub(crate) fn array_port(n: usize) -> IceGraphInputSocket {
    IceGraphInputSocket {
        name: format!("Array {n}"),
        value_hint: "Any",
        connected_output: None,
        value: None,
    }
}

pub(crate) fn execute_port(n: usize) -> IceGraphInputSocket {
    IceGraphInputSocket {
        name: format!("Port {n}"),
        value_hint: "Execute",
        connected_output: None,
        value: None,
    }
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
    pub fn get(&self, id: IceGraphId) -> Option<&IceGraph> {
        self.subnets.get(&id)
    }

    /// Each graph's key (`IceGraph::cook_key`), by its id.
    pub fn cook_keys(&self) -> HashMap<IceGraphId, u64> {
        self.subnets.iter().map(|(id, g)| (*id, g.cook_key())).collect()
    }

    /// What every graph computes (`IceGraph::content_hash`), together.
    pub fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut ids: Vec<&IceGraphId> = self.subnets.keys().collect();
        ids.sort_by_key(|i| i.0);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for id in ids {
            (id.0, self.subnets[id].content_hash()).hash(&mut h);
        }
        h.finish()
    }
    pub fn get_mut(&mut self, id: IceGraphId) -> Option<&mut IceGraph> {
        self.subnets.get_mut(&id)
    }
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
        let mul = g.add_node(
            "Multiply".into(),
            IceGraphNodeType::Math { op: Math::Multiply },
            egui::pos2(0.0, 0.0),
        );
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
        let mut geo = Geo::from_polygons(
            vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            vec![3],
            vec![0, 1, 2],
        );
        geo.set(
            Context::Face,
            "heat",
            Attr::new(Column::Float(Arc::new(vec![0.5])), Role::None),
        );
        let out = g.evaluate(&geo, &[]);
        assert_eq!(out.points()[1], [0.0, 2.0, 0.0]);
        assert!(!out
            .attr(Context::Point, POINTS)
            .unwrap()
            .same(geo.attr(Context::Point, POINTS).unwrap()));
        assert!(out
            .attr(Context::Face, "heat")
            .unwrap()
            .same(geo.attr(Context::Face, "heat").unwrap()));
        assert!(out.topology.same(&geo.topology));
        assert!(g.error.lock().unwrap().is_none());
    }

    /// A Snippet node: its parameters ports (a wire into one taking its
    /// place), its output a value for other nodes, its Execute the attributes
    /// it sets. Its text changed, its wires stay by name.
    #[test]
    fn a_snippet_runs_as_nodes_and_keeps_its_wires_by_name() {
        let mut g = IceGraph::new(IceGraphId(0), "snippet".into());
        let root = g.root().unwrap();
        let text = "snippet s (float k = 2, output float half = 0) { half = k / 2; @P *= k; }";
        let snippet = g.add_node(
            "s".into(),
            IceGraphNodeType::Snippet {
                text: text.into(),
                run_over: None,
            },
            egui::pos2(0.0, 0.0),
        );
        let three = g.add_node(
            "Three".into(),
            IceGraphNodeType::Constant {
                value: Value::Float(3.0),
            },
            egui::pos2(0.0, 0.0),
        );
        let set = g.add_node("Set half".into(), IceGraphNodeType::set("half"), egui::pos2(0.0, 0.0));
        g.add_connection(three, 0, snippet, 0);
        g.add_connection(snippet, 0, set, 0);
        g.add_connection(snippet, 1, root, 0);
        g.add_connection(set, 0, root, 1);
        let geo = Geo::from_points(vec![[1.0, 2.0, 3.0]]);
        let out = g.evaluate(&geo, &[]);
        assert!(g.error.lock().unwrap().is_none(), "{:?}", g.error.lock().unwrap());
        assert_eq!(out.points()[0], [3.0, 6.0, 9.0]);
        let Column::Float(half) = &out.attr(Context::Point, "half").unwrap().column else {
            panic!("a float")
        };
        assert_eq!(half[0], 1.5);

        // A parameter before k: the constant's wire follows k.
        let n = g.nodes.iter_mut().find(|n| n.id == snippet).unwrap();
        n.node_type = IceGraphNodeType::Snippet {
            text: text.replace("(float k", "(float extra = 0, float k"),
            run_over: None,
        };
        g.resync_sockets(snippet);
        let n = g.nodes.iter().find(|n| n.id == snippet).unwrap();
        assert_eq!(n.inputs[1].name, "k");
        assert_eq!(n.inputs[1].connected_output, Some((three, 0)));
        assert!(n.inputs[0].connected_output.is_none());
        assert_eq!(g.evaluate(&geo, &[]).points()[0], [3.0, 6.0, 9.0]);

        // Text that does not lower: the snippet node says why, and where.
        let n = g.nodes.iter_mut().find(|n| n.id == snippet).unwrap();
        n.node_type = IceGraphNodeType::Snippet {
            text: "snippet s () {\n  @P += @nothing;\n}".into(),
            run_over: None,
        };
        g.resync_sockets(snippet);
        g.evaluate(&geo, &[]);
        let error = g.error.lock().unwrap().clone().unwrap();
        assert_eq!(error.0, snippet);
        assert!(error.1.starts_with("line 2"), "{}", error.1);
    }

    /// Moving a node, renaming it or selecting it does not change what the
    /// graph computes; a value does. The types of the outputs are known once it
    /// has run.
    #[test]
    fn the_hash_follows_what_the_graph_computes() {
        let mut g = doubling();
        let before = g.content_hash();
        g.nodes[1].position = egui::pos2(300.0, 300.0);
        g.nodes[1].name = "moved".into();
        g.selected_node = Some(g.nodes[1].id);
        assert_eq!(g.content_hash(), before);
        let mul = g
            .nodes
            .iter()
            .position(|n| matches!(n.node_type, IceGraphNodeType::Math { .. }))
            .unwrap();
        g.nodes[mul].inputs[1].value = Some(Value::Float(3.0));
        assert_ne!(g.content_hash(), before);
        let geo = Geo::from_points(vec![[1.0, 2.0, 3.0]]);
        g.evaluate(&geo, &[]);
        g.refresh_types();
        let mul_id = g.nodes[mul].id;
        assert_eq!(g.output_type(mul_id, 0), Some(xms_ice::Ty::Vec3));
    }

    /// A new attribute on the second port, from a constant, reaches the
    /// primitive's primvars (what the Primitive Inspector lists); written per
    /// face from a per-point graph, it is a uniform primvar.
    #[test]
    fn a_new_attribute_reaches_the_primvars() {
        use crate::core::geo_mesh::GeoMesh;
        let mut g = doubling();
        let root = g.root().unwrap();
        let k = g.add_node(
            "Constant".into(),
            IceGraphNodeType::Constant {
                value: Value::Float(2.5),
            },
            egui::pos2(0.0, 0.0),
        );
        let set = g.add_node("Set".into(), IceGraphNodeType::set("test"), egui::pos2(0.0, 0.0));
        g.add_connection(k, 0, set, 0);
        g.add_connection(set, 0, root, 1);
        let geo = Geo::from_polygons(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![3],
            vec![0, 1, 2],
        );
        let out = g.evaluate(&geo, &[]);
        assert!(g.error.lock().unwrap().is_none(), "{:?}", g.error.lock().unwrap());
        let mesh = out.to_mesh();
        let pv = mesh.primvars.iter().find(|p| p.name == "test").expect("a test primvar");
        assert_eq!(pv.interp, crate::types::PrimVarInterp::Vertex);
        assert_eq!(pv.values, vec![vec![2.5]; 3]);
        if let IceGraphNodeType::SetData { context, .. } =
            &mut g.nodes.iter_mut().find(|n| n.id == set).unwrap().node_type
        {
            *context = Some(Context::Face);
        }
        let mesh = g.evaluate(&geo, &[]).to_mesh();
        let pv = mesh.primvars.iter().find(|p| p.name == "test").expect("a test primvar");
        assert_eq!(pv.interp, crate::types::PrimVarInterp::Uniform);
        assert_eq!(pv.values, vec![vec![2.5]]);
    }

    /// Graphs saved before intrinsics were `intrinsic:…` read them by their
    /// new names, in compounds too; a face's N read per face is the
    /// intrinsic, a point's N is not; a name the graph sets itself stays.
    #[test]
    fn old_intrinsic_names_read_as_the_new_ones() {
        let mut nodes: Vec<serde_json::Value> = vec![
            serde_json::json!({"GetData": {"name": "corners", "context": "Face", "resample": "Average"}}),
            serde_json::json!({"GetArrayData": {"name": "ptIndices", "context": null}}),
            serde_json::json!({"GetData": {"name": "N", "context": "Face", "resample": "Average"}}),
            serde_json::json!({"GetData": {"name": "N", "context": null, "resample": "Average"}}),
            serde_json::json!({"GetData": {"name": "area", "context": null, "resample": "Average"}}),
            serde_json::json!({"SetData": {"name": "area", "context": null, "resample": "Average"}}),
            serde_json::json!({"Compound": {"editable": {"graph": {"nodes": [{"node_type": {"GetArrayData": {"name": "triP", "context": null}}}]}}}}),
        ];
        upgrade_intrinsic_names(&mut nodes);
        let name = |v: &serde_json::Value| v.as_object().unwrap().values().next().unwrap()["name"].clone();
        assert_eq!(name(&nodes[0]), "intrinsic:corners");
        assert_eq!(name(&nodes[1]), "intrinsic:ptIndices");
        assert_eq!(name(&nodes[2]), "intrinsic:N");
        assert_eq!(name(&nodes[3]), "N");
        assert_eq!(name(&nodes[4]), "area", "the graph sets an area of its own");
        assert!(nodes[6].to_string().contains("intrinsic:triP"));
        let read: IceGraphNodeType = serde_json::from_value(nodes[0].clone()).unwrap();
        assert!(matches!(read, IceGraphNodeType::GetData { ref name, .. } if name == "intrinsic:corners"));
    }

    /// A picked port's wires come off: an input's one wire, an output's every
    /// wire. Deleting the selection takes the selected wire first, else every
    /// selected node but the root.
    #[test]
    fn unplugging_and_deleting_the_selection() {
        use crate::graph_ports::Picked;
        let mut g = doubling();
        let root = g.root().unwrap();
        let p = g
            .nodes
            .iter()
            .find(|n| matches!(n.node_type, IceGraphNodeType::GetData { .. }))
            .unwrap()
            .id;
        let mul = g
            .nodes
            .iter()
            .find(|n| matches!(n.node_type, IceGraphNodeType::Math { .. }))
            .unwrap()
            .id;
        // P feeds both of Multiply's inputs: unplugging P's output takes both.
        g.add_connection(p, 0, mul, 1);
        g.unplug(&Picked::Output(p, 0));
        assert!(g.connections.iter().all(|c| c.from_node != p));
        assert!(g
            .nodes
            .iter()
            .find(|n| n.id == mul)
            .unwrap()
            .inputs
            .iter()
            .all(|i| i.connected_output.is_none()));
        // An input: its wire only.
        let set = g
            .nodes
            .iter()
            .find(|n| matches!(n.node_type, IceGraphNodeType::SetData { .. }))
            .unwrap()
            .id;
        g.unplug(&Picked::Input(set, 0));
        assert!(g.connections.iter().all(|c| c.to_node != set));
        assert!(
            g.connections.iter().any(|c| c.to_node == root),
            "Set's Execute still goes into the root"
        );
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

    /// Putting a node down, wired to nothing, does not run the graph again; its
    /// type is known all the same, and Get Data's list has what the topology
    /// gives too.
    #[test]
    fn a_node_wired_to_nothing_does_not_change_the_hash() {
        let mut g = doubling();
        let geo = Geo::from_polygons(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![3],
            vec![0, 1, 2],
        );
        g.evaluate(&geo, &[]);
        let before = g.content_hash();
        let get = g.add_node(
            "Get".into(),
            IceGraphNodeType::GetData {
                name: "intrinsic:area".into(),
                context: Some(Context::Face),
                resample: xms_ice::Resample::Average,
            },
            egui::pos2(0.0, 0.0),
        );
        assert_eq!(g.content_hash(), before);
        g.refresh_types();
        assert_eq!(g.output_type(get, 0), Some(xms_ice::Ty::Float));
        let readable = g.readable();
        assert!(
            readable
                .iter()
                .any(|(c, n, _)| *c == Context::Face && n == "intrinsic:firstCorner"),
            "{readable:?}"
        );
        assert!(readable.iter().any(|(c, n, _)| *c == Context::Point && n == POINTS));
    }

    /// Concatenate Arrays keeps an input free, as the root does: two Get
    /// References joined, then a third input to join another.
    #[test]
    fn concatenate_arrays_keeps_an_input_free_and_joins_paths() {
        let mut g = IceGraph::new(IceGraphId(0), "join".into());
        let root = g.root().unwrap();
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == root) {
            n.node_type = IceGraphNodeType::Root {
                context: Context::Object,
            };
        }
        let a = g.add_node(
            "A".into(),
            IceGraphNodeType::GetReference {
                input: 1,
                paths: vec![],
            },
            egui::pos2(0.0, 0.0),
        );
        let b = g.add_node(
            "B".into(),
            IceGraphNodeType::GetReference {
                input: 2,
                paths: vec![],
            },
            egui::pos2(0.0, 0.0),
        );
        let join = g.add_node("Join".into(), IceGraphNodeType::Concat, egui::pos2(0.0, 0.0));
        g.add_connection(a, 0, join, 0);
        g.add_connection(b, 0, join, 1);
        assert_eq!(
            g.nodes.iter().find(|n| n.id == join).unwrap().inputs.len(),
            3,
            "one free"
        );
        let size = g.add_node(
            "Size".into(),
            IceGraphNodeType::Array {
                op: xms_ice::ArrayOp::Size,
            },
            egui::pos2(0.0, 0.0),
        );
        let set = g.add_node("Set".into(), IceGraphNodeType::set("n"), egui::pos2(0.0, 0.0));
        g.add_connection(join, 0, size, 0);
        g.add_connection(size, 0, set, 0);
        g.add_connection(set, 0, root, 0);
        let refs: Vec<Reference> = [vec!["/a".to_string()], vec!["/b".to_string(), "/c".into()]]
            .into_iter()
            .map(|paths| Reference { paths, meshes: vec![] })
            .collect();
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
        assert!(out
            .attr(Context::Point, POINTS)
            .unwrap()
            .same(geo.attr(Context::Point, POINTS).unwrap()));
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
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == root) {
            n.node_type = IceGraphNodeType::Root {
                context: Context::Object,
            };
        }
        let paths = g.add_node(
            "Pines".into(),
            IceGraphNodeType::GetReference {
                input: 1,
                paths: vec!["**/pine*".into()],
            },
            egui::pos2(0.0, 0.0),
        );
        let size = g.add_node(
            "Size".into(),
            IceGraphNodeType::Array {
                op: xms_ice::ArrayOp::Size,
            },
            egui::pos2(0.0, 0.0),
        );
        let set = g.add_node("Set n".into(), IceGraphNodeType::set("n"), egui::pos2(0.0, 0.0));
        g.add_connection(paths, 0, size, 0);
        g.add_connection(size, 0, set, 0);
        g.add_connection(set, 0, root, 0);
        let geo = Geo::from_points(vec![[0.0; 3]]);
        let refs = vec![Reference {
            paths: vec!["/trees/pine".to_string(), "/trees/oak".into(), "/more/pine_2".into()],
            meshes: vec![],
        }];
        let out = g.evaluate(&geo, &refs);
        assert_eq!(out.attr(Context::Object, "n").unwrap().column.ints().unwrap(), &[2]);
        // Its output is an array of paths, and says so.
        g.refresh_types();
        assert!(g.output_is_array(paths, 0) && !g.output_is_array(size, 0));
        assert_eq!(g.output_type(paths, 0), Some(xms_ice::Ty::Token));
    }

    /// A shrink-wrap: per point, Closest Point on Surface on the reference
    /// input's mesh, then P there. The reference is placed in the world and so
    /// is the primitive: the points land on the reference where the Viewer
    /// shows it, in the primitive's own space.
    #[test]
    fn the_ice_operator_wraps_points_onto_a_placed_reference_mesh() {
        use crate::node_graph::NodeGraphState;
        use crate::types::{EvalResult, NodeType};
        let mut subnets = IceGraphStore::default();
        let sid = subnets.create_subnet("Wrap".into());
        {
            let g = subnets.get_mut(sid).unwrap();
            let root = g.root().unwrap();
            let refs = g.add_node(
                "Get".into(),
                IceGraphNodeType::GetReference {
                    input: 1,
                    paths: vec![],
                },
                egui::pos2(0.0, 0.0),
            );
            let p = g.add_node("P".into(), IceGraphNodeType::get("P"), egui::pos2(0.0, 0.0));
            let near = g.add_node("Near".into(), IceGraphNodeType::ClosestPoint, egui::pos2(0.0, 0.0));
            // Get Data with its Polygon and Param UV wired: P read where each landed.
            let at = g.add_node("At".into(), IceGraphNodeType::get("P"), egui::pos2(0.0, 0.0));
            let set = g.add_node("Set".into(), IceGraphNodeType::set("P"), egui::pos2(0.0, 0.0));
            g.add_connection(refs, 0, near, 0);
            g.add_connection(p, 0, near, 1);
            g.add_connection(near, xms_ice::CLOSEST_POLYGONS, at, READ_INDEX);
            g.add_connection(near, xms_ice::CLOSEST_PARAM_UVS, at, READ_PARAM_UV);
            g.add_connection(at, 0, set, 0);
            g.add_connection(set, 0, root, 0);
        }
        // A small cube, moved up 3; the reference: a big cube, two metres a
        // side, moved over 10 along X.
        let mut graph = NodeGraphState::default();
        let small = graph.add_node("Small".into(), NodeType::cube(0.5), egui::pos2(0.0, 0.0));
        let up = graph.add_node(
            "Up".into(),
            NodeType::Transform {
                translation: glam::Vec3::new(0.0, 3.0, 0.0),
                rotation: glam::Vec3::ZERO,
                scale: glam::Vec3::ONE,
            },
            egui::pos2(0.0, 0.0),
        );
        let big = graph.add_node("Big".into(), NodeType::cube(2.0), egui::pos2(0.0, 0.0));
        let over = graph.add_node(
            "Over".into(),
            NodeType::Transform {
                translation: glam::Vec3::new(10.0, 0.0, 0.0),
                rotation: glam::Vec3::ZERO,
                scale: glam::Vec3::ONE,
            },
            egui::pos2(0.0, 0.0),
        );
        let ice = graph.add_node(
            "ICE".into(),
            NodeType::Ice {
                id: sid,
                name: "Wrap".into(),
                references: 1,
                params: Default::default(),
            },
            egui::pos2(0.0, 0.0),
        );
        graph.add_connection(small, 0, up, 0);
        graph.add_connection(big, 0, over, 0);
        graph.add_connection(up, 0, ice, 0);
        graph.add_connection(over, 0, ice, 1);
        let eval = |sid: IceGraphId, mesh: &Geo, refs: &[Reference]| {
            subnets
                .get(sid)
                .map(|sg| sg.evaluate(mesh, refs))
                .unwrap_or_else(|| mesh.clone())
        };
        let Some(EvalResult::Named(prims)) = graph.eval_node_out(ice, 0, &mut HashMap::new(), &eval) else {
            panic!("prims come out")
        };
        assert!(
            subnets.get(sid).unwrap().error.lock().unwrap().is_none(),
            "{:?}",
            subnets.get(sid).unwrap().error.lock().unwrap()
        );
        let p = &prims[0];
        let world = p.place.matrices().first().copied().unwrap_or(glam::Mat4::IDENTITY);
        // Every point, in the world, on the big cube: on its faces at x = 9.
        for q in p.geo.as_ref().unwrap().points() {
            let w = world.transform_point3(glam::Vec3::from_array(*q));
            assert!((w.x - 9.0).abs() < 1e-4, "{w}: on the big cube's near face");
            assert!(w.y.abs() <= 1.0 + 1e-4 && w.z.abs() <= 1.0 + 1e-4, "{w}");
        }
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
            if let Some(n) = g.nodes.iter_mut().find(|n| n.id == root) {
                n.node_type = IceGraphNodeType::Root {
                    context: Context::Object,
                };
            }
            let at = g.add_node(
                "Scatter".into(),
                IceGraphNodeType::Scatter {
                    seed: 1,
                    weight: String::new(),
                },
                egui::pos2(0.0, 0.0),
            );
            if let Some(n) = g.nodes.iter_mut().find(|n| n.id == at) {
                n.inputs[0].value = Some(Value::Int(12));
            }
            let protos = g.add_node(
                "Get".into(),
                IceGraphNodeType::GetReference {
                    input: 1,
                    paths: vec![],
                },
                egui::pos2(0.0, 0.0),
            );
            let inst = g.add_node(
                "Instancer".into(),
                IceGraphNodeType::Instancer {
                    path: "scatter".into(),
                    seed: 0,
                    draw_mode: Default::default(),
                },
                egui::pos2(0.0, 0.0),
            );
            g.add_connection(at, 0, inst, 0);
            g.add_connection(protos, 0, inst, 1);
            g.add_connection(inst, 0, root, 0);
        }
        // The scene: a cube; its reference: another cube, the prototype.
        let mut graph = NodeGraphState::default();
        let ground = graph.add_node("Ground".into(), NodeType::cube(4.0), egui::pos2(0.0, 0.0));
        let proto = graph.add_node("Proto".into(), NodeType::cube(0.2), egui::pos2(0.0, 0.0));
        let ice = graph.add_node(
            "ICE".into(),
            NodeType::Ice {
                id: sid,
                name: "Scatter".into(),
                references: 1,
                params: Default::default(),
            },
            egui::pos2(0.0, 0.0),
        );
        graph.add_connection(ground, 0, ice, 0);
        graph.add_connection(proto, 0, ice, 1);
        let eval = |sid: IceGraphId, mesh: &Geo, refs: &[Reference]| {
            subnets
                .get(sid)
                .map(|sg| sg.evaluate(mesh, refs))
                .unwrap_or_else(|| mesh.clone())
        };
        let Some(EvalResult::Named(prims)) = graph.eval_node_out(ice, 0, &mut HashMap::new(), &eval) else {
            panic!("prims come out")
        };
        // The scene's cube; beside it a point instancer of 12 points, and the
        // prototype's cube under it, drawn at each of them.
        let ground_path = prims[0].path.clone();
        let parent = ground_path
            .rsplit_once('/')
            .map(|(p, _)| p.to_string())
            .unwrap_or_default();
        let inst = prims
            .iter()
            .find(|p| p.instancing.is_some())
            .expect("a point instancer");
        assert_eq!(
            inst.path,
            format!("{parent}/scatter"),
            "beside the primitive, not under it"
        );
        assert_eq!(inst.geo.as_ref().unwrap().points().len(), 12);
        let protos: Vec<_> = prims.iter().filter(|p| p.instanced_by.is_some()).collect();
        assert_eq!(
            protos.len(),
            1,
            "{:?}",
            prims.iter().map(|p| &p.path).collect::<Vec<_>>()
        );
        assert!(
            protos[0].path.starts_with(&format!("{}/Prototypes/", inst.path)),
            "{}",
            protos[0].path
        );
        assert!(matches!(&protos[0].place, Placement::Many(m) if m.len() == 12));
        assert_eq!(inst.instancing.as_ref().unwrap().len(), 1);
    }
}
