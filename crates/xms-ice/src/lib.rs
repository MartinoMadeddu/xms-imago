//! The ICE engine: a graph of nodes compiled into a plan, and the plan run over
//! a geometry's attributes (`xms_geo::Geo`) in parallel batches.
//!
//! Its own crate, depending on the geometry model and nothing of the app: no
//! interface, no node editor. The app turns its ICE graphs into `graph::Graph`
//! and runs the plan.
//!
//! Loosely modelled after Softimage ICE and Houdini VOPs: ICE's contexts and
//! per-element attribute arrays, VOPs' compile step.
//!
//! - `types`: the types values have (bool, int, float, vectors, rotations). A
//!   value is one per element, or (in a graph's one-value part, an Object
//!   graph's for one) an array: arrays come from Random with a count, and maths
//!   works on them element by element.
//! - `graph`: the graph as the engine sees it: nodes, wires, values on
//!   unconnected inputs, a root whose Execute inputs run in order.
//! - `compile`: graph to plan. Types and contexts are resolved and checked;
//!   only what reaches the root is compiled; what does not vary per element
//!   (constants, parameters, Object attributes) is computed once.
//! - `exec`: the plan run over a `Geo`. Only the attributes the graph reads are
//!   read, a batch at a time; only the attributes it sets get new columns;
//!   every other column stays shared with the input.
//! - `derived`: what the topology gives without being stored (a face's corners,
//!   first corner, normal and area; a corner's point, face and place in it), as
//!   the Primitive Inspector shows it.
//! - `surface`: the reference inputs' meshes a graph queries (Closest Point on
//!   Surface), taken into the graph's space, and read at samples.
//! - `spatial`: a BVH (rray's builder) and the closest point on a triangle.

pub mod compile;
pub mod derived;
pub mod exec;
pub mod graph;
pub mod spatial;
pub mod surface;
pub mod types;

pub use compile::{compile, compile_with, output_arrays, output_types, CompileError, Plan};
pub use exec::{run, run_in, Instances, Ran, BATCH};
pub use graph::{
    ArrayOp, Compose, Decompose, Graph, Input, Math, Node, Op, Resample, CLOSEST_DISTANCE, CLOSEST_PARAM_UVS,
    CLOSEST_POLYGONS, SCATTER_NORMALS, SCATTER_PARAM_UVS, SCATTER_POLYGONS, SCATTER_POSITIONS,
};
pub use surface::{RefGeo, RefMesh, Reference};
pub use types::{Loc, Ty, Value};

#[cfg(test)]
mod tests;
