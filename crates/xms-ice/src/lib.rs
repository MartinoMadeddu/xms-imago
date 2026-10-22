//! The ICE engine: a graph of nodes compiled into a plan, and the plan run
//! over a geometry's attributes (`xms_geo::Geo`) in parallel batches.
//!
//! Its own crate, depending on the geometry model and nothing of the app:
//! no interface, no node editor. The app turns its ICE graphs into
//! `graph::Graph` and runs the plan.
//!
//! Loosely modelled after Softimage ICE and Houdini VOPs: ICE's contexts and
//! per-element attribute arrays, VOPs' compile step.
//!
//! - `types`: the types values have (bool, int, float, vectors, rotations).
//!   A value is one per element, or (in a graph's one-value part, an Object
//!   graph's for one) an array: arrays come from Random with a count, and
//!   maths works on them element by element.
//! - `graph`: the graph as the engine sees it: nodes, wires, values on
//!   unconnected inputs, a root whose Execute inputs run in order.
//! - `compile`: graph to plan. Types and contexts are resolved and checked;
//!   only what reaches the root is compiled; what does not vary per element
//!   (constants, parameters, Object attributes) is computed once.
//! - `exec`: the plan run over a `Geo`. Only the attributes the graph reads
//!   are read, a batch at a time; only the attributes it sets get new
//!   columns; every other column stays shared with the input.
//! - `derived`: what the topology gives without being stored (a face's
//!   corners, first corner, normal and area; a corner's point, face and
//!   place in it), as the Primitive Inspector shows it.

pub mod compile;
pub mod derived;
pub mod exec;
pub mod graph;
pub mod types;

pub use compile::{compile, output_arrays, output_types, CompileError, Plan};
pub use exec::{run, Instances, Ran, BATCH};
pub use graph::{ArrayOp, Graph, Input, Math, Node, Op, Reduce};
pub use types::{Loc, Ty, Value};

#[cfg(test)]
mod tests;
