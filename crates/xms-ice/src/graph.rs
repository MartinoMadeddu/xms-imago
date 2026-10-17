//! The graph as the engine sees it. The editor's graph (positions, names,
//! selection) is turned into this before compiling.

use xms_geo::Context;

use super::types::Value;

/// Nodes by position in `nodes`; wires point at a node's position and output.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub op:     Op,
    /// One per input of `op`, in order.
    pub inputs: Vec<Input>,
}

/// What an input takes: a wire from another node's output, or its own value.
#[derive(Clone, Debug)]
pub enum Input {
    Wire { node: usize, output: usize },
    Value(Value),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// The graph's end. Its inputs are Execute ports, run in order; it runs
    /// over every element of `context`.
    Root { context: Context },
    /// An attribute of the geometry, by name (`P` is the points). `context`:
    /// where the attribute is read from; None, the graph's own context (or
    /// Object). From another context, each element reads the elements it
    /// touches: a corner its point or its face, a face its corners or
    /// points, a point the faces or corners around it; where it touches
    /// several, `reduce` makes them one value.
    GetData { name: String, context: Option<Context>, reduce: Reduce },
    /// Set an attribute: input 0 is the value. Its output is an Execute,
    /// to wire into the root. `context`: where the attribute is written;
    /// None, the graph's own. Into another context, each element there
    /// takes the values of the graph's elements it touches (a face its
    /// points, a point the faces around it), made one by `reduce`.
    SetData { name: String, context: Option<Context>, reduce: Reduce },
    /// A value given when the graph runs (time, a promoted parameter);
    /// `default` when none is given. Changing it does not recompile.
    Parameter { name: String, default: Value },
    Constant(Value),
    Math(Math),
}

/// How the values an element touches in another context become one: a
/// face reading its points' positions gets their average, by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Reduce {
    #[default]
    Average,
    Sum,
    Min,
    Max,
}

impl Reduce {
    pub const ALL: [Reduce; 4] = [Reduce::Average, Reduce::Sum, Reduce::Min, Reduce::Max];
}

/// Maths, on any type it makes sense for: `Add` adds floats or vectors,
/// `Multiply` also scales a vector by a float.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Math {
    Add,
    Subtract,
    Multiply,
    Divide,
    Min,
    Max,
    /// a, b, t.
    Lerp,
    Negate,
    Abs,
    Sin,
    Cos,
    Dot,
    Cross,
    Length,
    Normalize,
}

impl Math {
    /// Every operation, in the order the add menu lists them.
    pub const ALL: [Math; 15] = [
        Math::Add, Math::Subtract, Math::Multiply, Math::Divide, Math::Min, Math::Max, Math::Lerp,
        Math::Negate, Math::Abs, Math::Sin, Math::Cos,
        Math::Dot, Math::Cross, Math::Length, Math::Normalize,
    ];

    pub fn arity(self) -> usize {
        match self {
            Math::Lerp => 3,
            Math::Negate | Math::Abs | Math::Sin | Math::Cos | Math::Length | Math::Normalize => 1,
            _ => 2,
        }
    }
}

impl Op {
    /// How many inputs a node of this kind has. The root has as many as
    /// its node lists.
    pub fn arity(&self) -> Option<usize> {
        match self {
            Op::Root { .. } => None,
            Op::GetData { .. } | Op::Parameter { .. } | Op::Constant(_) => Some(0),
            Op::SetData { .. } => Some(1),
            Op::Math(m) => Some(m.arity()),
        }
    }
}

impl Op {
    /// Get Data of an attribute in the graph's own context.
    pub fn get(name: &str) -> Op { Op::GetData { name: name.into(), context: None, reduce: Reduce::Average } }

    /// Set Data of an attribute in the graph's own context.
    pub fn set(name: &str) -> Op { Op::SetData { name: name.into(), context: None, reduce: Reduce::Average } }
}

impl Graph {
    pub fn add(&mut self, op: Op, inputs: Vec<Input>) -> usize {
        self.nodes.push(Node { op, inputs });
        self.nodes.len() - 1
    }
}

/// A wire from output 0 of `node`.
pub fn wire(node: usize) -> Input { Input::Wire { node, output: 0 } }
