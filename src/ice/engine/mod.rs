//! The ICE engine: a graph of nodes compiled into a plan, and the plan run
//! over a geometry's attributes in parallel batches.
//!
//! Loosely modelled after Softimage ICE and Houdini VOPs: ICE's contexts and
//! per-element attribute arrays, VOPs' compile step.
//!
//! - `types`: the types values have (bool, int, float, vectors).
//! - `graph`: the graph as the engine sees it: nodes, wires, values on
//!   unconnected inputs, a root whose Execute inputs run in order.
//! - `compile`: graph to plan. Types and contexts are resolved and checked;
//!   only what reaches the root is compiled; what does not vary per element
//!   (constants, parameters, Object attributes) is computed once.
//! - `exec`: the plan run over a `Geo`. Only the attributes the graph reads
//!   are read, a batch at a time; only the attributes it sets get new
//!   columns; every other column stays shared with the input.

pub mod compile;
pub mod exec;
pub mod graph;
pub mod types;

pub use compile::{compile, CompileError, Plan};
pub use exec::run;
pub use graph::{Graph, Input, Math, Node, Op};
pub use types::{Ty, Value};

#[cfg(test)]
mod tests;
