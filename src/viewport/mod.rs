//! Pieces the Viewer's 3D view uses: navigation styles, display modes,
//! bounding boxes, background texture decoding. The drawing itself is in
//! `crate::viewer`.

pub mod nav;
pub mod textures;
pub mod display;
pub mod bounds;
