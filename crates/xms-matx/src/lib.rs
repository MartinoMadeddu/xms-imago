//! MaterialX for XMS | Imago.
//!
//! MaterialX's own node libraries (stdlib, pbrlib, the shading models of
//! bxdf, cmlib, nprlib and lights), vendored in `libraries/` and compiled
//! into the program, read into typed node definitions: each node's inputs
//! and outputs with their types, defaults, labels, limits and folders, as
//! the libraries define them. The MaterialX operator's graph is made of
//! these nodes, by their MaterialX names, so what it writes (USD's MaterialX
//! shading networks, `.mtlx` documents) reads back in any MaterialX tool.
//!
//! * [`Library::standard`]: the vendored libraries, read once.
//! * [`Library::resolve`]: which definition of a node (`ND_add_color3`) a
//!   node in a graph is, from the types connected to it.
//! * [`Graph`]: a graph of these nodes, their values and connections;
//!   connecting picks the definitions (`Graph::connect`).
//! * [`document`]: a graph as a `.mtlx` document, and back: how a graph is
//!   saved, and written out.
//! * [`usd`]: a graph's material as a USD MaterialX shading network.
//! * [`preview`]: what the Viewer shows of a graph's material.
//! * [`xml`]: the small XML reader the documents are read with.
//!
//! Dependency-free, like `xms-scene`; no interface code.

pub mod document;
pub mod graph;
pub mod library;
pub mod preview;
pub mod types;
pub mod usd;
pub mod xml;

pub use graph::{Binding, Graph, Node};
pub use library::{Input, Library, LoadError, NodeDef, Output, Ui, FILES, MATERIALX_VERSION};
pub use preview::{preview, Preview};
pub use types::{Type, Value};

#[cfg(test)]
mod tests;
