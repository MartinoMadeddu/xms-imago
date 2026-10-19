//! Core data structures for geometry representation.
//! 
//! This module provides the foundation for attribute-based geometry processing.
//! All types here are pure data containers with no procedural logic.

pub mod anim;
pub mod poly;
pub mod poly_ops;
pub mod anim_tools;
pub mod human;
pub mod human_ik;
/// Hands and feet sent where another clip has them; the human pass on its own.
pub mod reach;
/// Skin weights kept on the geometry by joint name; Capture and Skin.
pub mod skin;
pub mod pattern;
/// Primitive Locations: the paths an operator works on.
pub mod locations;
/// Properties: the values a prim holds beyond its geometry, and how they
/// are inherited below it.
pub mod properties;
/// Set Attribute: one value per primitive, set by the scene graph.
pub mod set_attribute;
/// Point instancers: their prototypes' copies made from their points.
pub mod instancing;
/// Materials: a UsdPreviewSurface, as the Material operator makes it.
pub mod material;
/// Cameras: a UsdGeomCamera's values, looked through by the Viewer.
pub mod camera;
pub mod uv;
pub mod sequences;
pub mod manip;
/// Keys on parameters: animation curves.
pub mod keys;
pub mod xform;
/// The geometry model of packed primitives, the nodes and ICE: typed
/// attribute columns per context.
pub use xms_geo as geo;
/// `Geo` to and from `MeshData`.
pub mod geo_mesh;
