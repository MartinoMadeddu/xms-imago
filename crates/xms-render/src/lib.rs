//! Rendering inside Imago: one interface for every renderer that reads USD
//! itself (`docs/arnold-native.md`).
//!
//! A [`Renderer`] loads a layer Write USD wrote, renders it progressively, and
//! takes edits by prim path while it renders, when it can apply that kind of
//! change live; anything else is a reload of the layer, written again. [`Live`]
//! runs one on a thread of its own, deciding between an edit and a reload, and
//! reports its progress as [`Event`]s.
//!
//! Backends: [`arnold`], `libai` from the user's SDK opened at run time.

pub mod arnold;
mod live;

pub use live::Live;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// A value of an attribute, as USD has it.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i32),
    Int2([i32; 2]),
    Float(f32),
    Color([f32; 3]),
    Token(String),
}

/// What changed on a prim since the layer was loaded.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// Its world matrix in the layer's space, rows as USD writes them
    /// (translation last). For a prim with shapes below it, an edit per shape.
    Transform([[f32; 4]; 4]),
    /// A light's or a shader's attributes, by their USD names (`inputs:intensity`).
    Attributes(Vec<(String, Value)>),
    /// The Render Settings' attributes, by their USD names (`resolution`,
    /// `arnold:global:AA_samples`).
    Settings(Vec<(String, Value)>),
    /// Shown or hidden.
    Visible(bool),
    /// A mesh's points, as many as it had (animation, a deformer).
    Points(Arc<Vec<[f32; 3]>>),
    /// Anything else (a prim added or removed, topology, a new material
    /// network, another frame): the layer is loaded again.
    Structure,
}

/// A change to the prim at `path` (the Render Settings' for `Settings`).
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub path: String,
    pub change: Change,
}

/// What a renderer did, as it happens.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// The layer is loaded, in that many seconds. Its images: the files each
    /// pass's image is in, whole, once its `Pass` comes.
    Loaded { seconds: f64, images: Vec<PathBuf> },
    /// A pass is done: its images are whole, to be read.
    Pass { index: u32, total: u32 },
    /// The last pass is done.
    Finished,
    /// Edits applied live, without a reload.
    Edited(usize),
    /// Something it could not do, and why.
    Failed(String),
}

/// A renderer that reads USD itself. Each call is made from one thread, the
/// one [`Live`] runs it on.
pub trait Renderer {
    /// Load the layer at `frame`, in place of what was loaded: the files each
    /// pass's images will be in, whole.
    fn load(&mut self, layer: &Path, frame: f64) -> Result<Vec<PathBuf>, String>;
    /// Start rendering what is loaded, progressively, or again after edits.
    fn render(&mut self) -> Result<(), String>;
    /// Whether this kind of change can be applied live at all.
    fn live(&self, change: &Change) -> bool;
    /// Apply live edits. `Ok(false)`: one could not be (no node at its path,
    /// an attribute it does not map), so the layer is to be loaded again.
    fn edit(&mut self, edits: &[Edit]) -> Result<bool, String>;
    /// What happened since it was last asked.
    fn poll(&mut self) -> Vec<Event>;
    /// Stop rendering; what is loaded stays.
    fn stop(&mut self);
}

/// An image as it renders: RGBA floats, rows from the top, the frame's size.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<[f32; 4]>,
}

/// The image a renderer fills bucket by bucket as it renders, in memory: read
/// by what shows it (the Image Viewer, the Viewer), from any thread, when its
/// generation says it changed.
#[derive(Default)]
pub struct Pixels {
    buffer: Mutex<(Frame, (i32, i32))>,
    generation: AtomicU64,
}

impl Pixels {
    /// Changes each time a bucket comes in.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// The image as it is.
    pub fn frame(&self) -> Frame {
        self.buffer.lock().map(|b| b.0.clone()).unwrap_or_default()
    }

    /// A render starts: its frame, from `(x, y)`, `width` by `height`. What
    /// is there stays, to be rendered over, unless the size changed.
    pub fn open(&self, x: i32, y: i32, width: usize, height: usize) {
        if let Ok(mut b) = self.buffer.lock() {
            if (b.0.width, b.0.height) != (width, height) {
                b.0 = Frame {
                    width,
                    height,
                    rgba: vec![[0.0; 4]; width * height],
                };
            }
            b.1 = (x, y);
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    /// A bucket done: `width` by `height` from `(x, y)` in the frame's
    /// coordinates, `pixel(i)` its `i`th pixel, row by row.
    pub fn bucket(&self, x: i32, y: i32, width: usize, height: usize, pixel: impl Fn(usize) -> [f32; 4]) {
        if let Ok(mut b) = self.buffer.lock() {
            let ((x0, y0), w, h) = (b.1, b.0.width as i32, b.0.height as i32);
            for row in 0..height {
                let fy = y + row as i32 - y0;
                if !(0..h).contains(&fy) {
                    continue;
                }
                for col in 0..width {
                    let fx = x + col as i32 - x0;
                    if (0..w).contains(&fx) {
                        b.0.rgba[(fy * w + fx) as usize] = pixel(row * width + col);
                    }
                }
            }
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

/// `file` with the frame's tokens put in, as husk does and Arnold does not:
/// `$F` the frame, `$F4` padded to 4 digits.
pub fn with_frame(file: &str, frame: f64) -> String {
    let frame = frame.round() as i64;
    let mut out = String::new();
    let mut rest = file;
    while let Some(k) = rest.find("$F") {
        out.push_str(&rest[..k]);
        rest = &rest[k + 2..];
        let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        let pad: usize = rest[..digits].parse().unwrap_or(0);
        out.push_str(&format!("{frame:0pad$}"));
        rest = &rest[digits..];
    }
    out + rest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_fill_the_frame_where_they_are() {
        let pixels = Pixels::default();
        // An overscan's frame starts before 0.
        pixels.open(-1, -1, 4, 3);
        let g = pixels.generation();
        pixels.bucket(0, 0, 2, 2, |i| [i as f32, 0.0, 0.0, 1.0]);
        assert!(pixels.generation() > g);
        let f = pixels.frame();
        assert_eq!((f.width, f.height), (4, 3));
        assert_eq!(f.rgba[4 + 1], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(f.rgba[4 + 2], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(f.rgba[8 + 2], [3.0, 0.0, 0.0, 1.0]);
        // Out of the frame: left out.
        pixels.bucket(2, 1, 2, 2, |_| [9.0; 4]);
        assert_eq!(pixels.frame().rgba[8 + 3], [9.0; 4]);
        // The next pass renders over it; another size starts again.
        pixels.open(-1, -1, 4, 3);
        assert_eq!(pixels.frame().rgba[8 + 3], [9.0; 4]);
        pixels.open(0, 0, 2, 2);
        assert_eq!(pixels.frame().rgba, vec![[0.0; 4]; 4]);
    }

    #[test]
    fn frame_tokens_are_put_in() {
        assert_eq!(with_frame("render/beauty.$F4.exr", 1.0), "render/beauty.0001.exr");
        assert_eq!(with_frame("a.$F.exr", 12.0), "a.12.exr");
        assert_eq!(with_frame("plain.exr", 3.0), "plain.exr");
    }
}
