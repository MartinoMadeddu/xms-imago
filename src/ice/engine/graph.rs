//! The graph as the engine sees it. The editor's graph (positions, names,
//! selection) is turned into this before compiling.

use crate::core::geo::Context;

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
    /// An attribute of the geometry, by name (`P` is the points).
    GetData { name: String },
    /// Set an attribute: input 0 is the value. Its output is an Execute,
    /// to wire into the root.
    SetData { name: String },
    /// A value given when the graph runs (time, a promoted parameter);
    /// `default` when none is given. Changing it does not recompile.
    Parameter { name: String, default: Value },
    Constant(Value),
    Math(Math),
}

/// Maths, on any type it makes sense for: `Add` adds floats or vectors,
/// `Multiply` also scales a vector by a float.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

impl Graph {
    pub fn add(&mut self, op: Op, inputs: Vec<Input>) -> usize {
        self.nodes.push(Node { op, inputs });
        self.nodes.len() - 1
    }
}

/// A wire from output 0 of `node`.
pub fn wire(node: usize) -> Input { Input::Wire { node, output: 0 } }
