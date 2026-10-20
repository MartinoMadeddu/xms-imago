//! XMS | Imago's images: what a render makes, and a view of it.
//!
//! - `image`: an image as channels of floats, grouped into layers (AOVs)
//!   by their names, and how a layer is shown (exposure, gamma, sRGB).
//!   Pure std.
//! - `read`: images read from files: every layer and channel of an
//!   OpenEXR file, and PNG, JPEG, TIFF, TGA, HDR as linear RGBA.
//! - `view`: an egui view of one: layer and channel menus, exposure and
//!   gamma, zoom and pan, the values under the pointer.
//!
//! Nothing here knows Imago's scene: a renderer, a file or anything else
//! gives it an `Image`. The Image Viewer pane is built on it.

pub mod image;
pub mod read;
pub mod view;

pub use crate::image::{Channel, Image, Layer, Look, Show};
pub use read::read;
pub use view::ImageView;
