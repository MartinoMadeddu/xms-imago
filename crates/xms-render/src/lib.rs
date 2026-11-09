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
    /// The render's own view, in place of the layer's camera: the Viewer's.
    View(View),
}

/// A view to render through, the Viewer's: its camera's placement in the
/// layer's space (rows, translation last, looking down -Z), its horizontal
/// field of view in degrees, and the image's size in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub matrix: [[f32; 4]; 4],
    pub fov: f32,
    pub size: [u32; 2],
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
    /// Start rendering what is loaded, or again after edits: `progressive`,
    /// in passes from few samples up (IPR); else once, each bucket finished at
    /// the samples the Render Settings give.
    fn render(&mut self, progressive: bool) -> Result<(), String>;
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

/// One of a render's images (an AOV): its name, how many of red, green, blue
/// and alpha it has (one channel is held grey), its pixels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layer {
    pub name: String,
    pub channels: usize,
    pub frame: Frame,
}

/// Part of a layer: `width` by `height` pixels from `(x, y)`, rows from the
/// top, in a frame `frame` big.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Region {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub frame: (usize, usize),
    pub rgba: Vec<[f32; 4]>,
}

/// The images a renderer fills bucket by bucket as it renders, in memory, one
/// per AOV: read by what shows them (the Image Viewer, the Viewer), from any
/// thread, when the generation says they changed; or only what changed of
/// one, as a region.
#[derive(Default)]
pub struct Pixels {
    buffer: Mutex<Buffer>,
    generation: AtomicU64,
}

#[derive(Default)]
struct Buffer {
    size: (usize, usize),
    /// Where the frame starts in the render's coordinates.
    origin: (i32, i32),
    layers: Vec<Held>,
    /// The buckets being rendered, by the render thread rendering each: `x,
    /// y, width, height` in the frame.
    rendering: Vec<(u16, [usize; 4])>,
}

/// How many read only what changed (`Pixels::take`), each its own: the Image
/// Viewer (0), the Viewer (1).
pub const READERS: usize = 2;

struct Held {
    layer: Layer,
    /// What changed since each reader last took it: `x0, y0, x1, y1`, `x1`
    /// and `y1` past it.
    changed: [Option<[usize; 4]>; READERS],
}

impl Held {
    fn changed(&mut self, x0: usize, y0: usize, x1: usize, y1: usize) {
        for c in &mut self.changed {
            *c = Some(match *c {
                Some([a, b, c, d]) => [a.min(x0), b.min(y0), c.max(x1), d.max(y1)],
                None => [x0, y0, x1, y1],
            });
        }
    }
}

impl Pixels {
    /// Changes each time a bucket comes in.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// The layers as they are, the beauty (`RGBA`) first.
    pub fn layers(&self) -> Vec<Layer> {
        let mut out: Vec<Layer> = self
            .buffer
            .lock()
            .map(|b| b.layers.iter().map(|h| h.layer.clone()).collect())
            .unwrap_or_default();
        out.sort_by_key(|l| l.name != "RGBA");
        out
    }

    /// The beauty as it is (the first layer, else).
    pub fn frame(&self) -> Frame {
        self.layers().into_iter().next().map(|l| l.frame).unwrap_or_default()
    }

    /// What changed of the layer `name` since `reader` last took it (all of
    /// it, `all`), and nothing changed since for that reader: as a texture is
    /// kept up to date.
    pub fn take(&self, name: &str, reader: usize, all: bool) -> Option<Region> {
        let mut b = self.buffer.lock().ok()?;
        let held = b.layers.iter_mut().find(|h| h.layer.name == name)?;
        let changed = held.changed.get_mut(reader)?.take();
        let f = &held.layer.frame;
        let (w, h) = (f.width, f.height);
        let [x0, y0, x1, y1] = if all { Some([0, 0, w, h]) } else { changed }?;
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let mut rgba = Vec::with_capacity((x1 - x0) * (y1 - y0));
        for row in y0..y1 {
            rgba.extend_from_slice(&f.rgba[row * w + x0..row * w + x1]);
        }
        Some(Region {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
            frame: (w, h),
            rgba,
        })
    }

    /// A render starts: its frame, from `(x, y)`, `width` by `height`. What
    /// is there stays, to be rendered over, unless the size changed.
    pub fn open(&self, x: i32, y: i32, width: usize, height: usize) {
        if let Ok(mut b) = self.buffer.lock() {
            if b.size != (width, height) {
                b.size = (width, height);
                b.layers.clear();
            }
            b.origin = (x, y);
            b.rendering.clear();
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    /// A render thread starts a bucket: `width` by `height` from `(x, y)` in
    /// the render's coordinates (its last one done, if it did not say).
    pub fn bucket_started(&self, thread: u16, (x, y, width, height): (i32, i32, usize, usize)) {
        if let Ok(mut b) = self.buffer.lock() {
            let (x0, y0) = b.origin;
            let at = [(x - x0).max(0) as usize, (y - y0).max(0) as usize, width, height];
            b.rendering.retain(|(t, _)| *t != thread);
            b.rendering.push((thread, at));
        }
    }

    /// A render thread's bucket done.
    pub fn bucket_done(&self, thread: u16) {
        if let Ok(mut b) = self.buffer.lock() {
            b.rendering.retain(|(t, _)| *t != thread);
        }
    }

    /// The render ended or was stopped: no bucket is being rendered.
    pub fn closed(&self) {
        if let Ok(mut b) = self.buffer.lock() {
            b.rendering.clear();
        }
    }

    /// The buckets being rendered now: `x, y, width, height` in the frame.
    pub fn rendering(&self) -> Vec<[usize; 4]> {
        self.buffer
            .lock()
            .map(|b| b.rendering.iter().map(|(_, at)| *at).collect())
            .unwrap_or_default()
    }

    /// A bucket of the layer `name` (`channels` of red, green, blue, alpha)
    /// done: `width` by `height` from `(x, y)` in the frame's coordinates,
    /// `pixel(i)` its `i`th pixel, row by row.
    pub fn bucket(
        &self,
        name: &str,
        channels: usize,
        (x, y, width, height): (i32, i32, usize, usize),
        pixel: impl Fn(usize) -> [f32; 4],
    ) {
        if let Ok(mut b) = self.buffer.lock() {
            let ((x0, y0), (w, h)) = (b.origin, b.size);
            let held = match b.layers.iter().position(|h| h.layer.name == name) {
                Some(k) => &mut b.layers[k],
                None => {
                    b.layers.push(Held {
                        layer: Layer {
                            name: name.into(),
                            channels,
                            frame: Frame {
                                width: w,
                                height: h,
                                rgba: vec![[0.0; 4]; w * h],
                            },
                        },
                        changed: [Some([0, 0, w, h]); READERS],
                    });
                    b.layers.last_mut().expect("just pushed")
                }
            };
            let (w, h) = (w as i32, h as i32);
            // The bucket, in the frame.
            let (fx0, fy0) = ((x - x0).clamp(0, w), (y - y0).clamp(0, h));
            let (fx1, fy1) = (
                (x - x0 + width as i32).clamp(0, w),
                (y - y0 + height as i32).clamp(0, h),
            );
            for fy in fy0..fy1 {
                for fx in fx0..fx1 {
                    let i = (fy - (y - y0)) as usize * width + (fx - (x - x0)) as usize;
                    held.layer.frame.rgba[(fy * w + fx) as usize] = pixel(i);
                }
            }
            if fx1 > fx0 && fy1 > fy0 {
                held.changed(fx0 as usize, fy0 as usize, fx1 as usize, fy1 as usize);
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
        pixels.bucket("RGBA", 4, (0, 0, 2, 2), |i| [i as f32, 0.0, 0.0, 1.0]);
        assert!(pixels.generation() > g);
        let f = pixels.frame();
        assert_eq!((f.width, f.height), (4, 3));
        assert_eq!(f.rgba[4 + 1], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(f.rgba[4 + 2], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(f.rgba[8 + 2], [3.0, 0.0, 0.0, 1.0]);
        // Out of the frame: left out.
        pixels.bucket("RGBA", 4, (2, 1, 2, 2), |_| [9.0; 4]);
        assert_eq!(pixels.frame().rgba[8 + 3], [9.0; 4]);
        // The next pass renders over it; another size starts again.
        pixels.open(-1, -1, 4, 3);
        assert_eq!(pixels.frame().rgba[8 + 3], [9.0; 4]);
        pixels.open(0, 0, 2, 2);
        assert!(pixels.layers().is_empty());
    }

    #[test]
    fn each_aov_is_a_layer_the_beauty_first() {
        let pixels = Pixels::default();
        pixels.open(0, 0, 2, 1);
        pixels.bucket("Z", 1, (0, 0, 2, 1), |i| [i as f32; 4]);
        pixels.bucket("RGBA", 4, (0, 0, 1, 1), |_| [1.0; 4]);
        let layers = pixels.layers();
        assert_eq!(
            layers.iter().map(|l| (l.name.as_str(), l.channels)).collect::<Vec<_>>(),
            [("RGBA", 4), ("Z", 1)]
        );
        assert_eq!(layers[1].frame.rgba[1], [1.0; 4]);
    }

    /// The buckets being rendered: one a thread, in the frame, gone when done
    /// or when the render ends.
    #[test]
    fn the_buckets_rendering_are_known() {
        let p = Pixels::default();
        p.open(10, 20, 64, 64);
        p.bucket_started(0, (10, 20, 16, 16));
        p.bucket_started(1, (26, 20, 16, 16));
        assert_eq!(p.rendering(), [[0, 0, 16, 16], [16, 0, 16, 16]]);
        // A thread's next bucket replaces its last.
        p.bucket_started(0, (10, 36, 16, 16));
        assert_eq!(p.rendering(), [[16, 0, 16, 16], [0, 16, 16, 16]]);
        p.bucket_done(1);
        assert_eq!(p.rendering(), [[0, 16, 16, 16]]);
        p.closed();
        assert!(p.rendering().is_empty());
        p.bucket_started(3, (10, 20, 8, 8));
        p.open(10, 20, 64, 64);
        assert!(p.rendering().is_empty(), "a new render starts with none");
    }

    #[test]
    fn only_what_changed_is_taken() {
        let pixels = Pixels::default();
        pixels.open(0, 0, 8, 4);
        pixels.bucket("RGBA", 4, (2, 1, 2, 2), |_| [1.0; 4]);
        // A new layer: all of it.
        assert_eq!(pixels.take("RGBA", 0, false).map(|r| (r.width, r.height)), Some((8, 4)));
        assert_eq!(pixels.take("RGBA", 0, false), None);
        pixels.bucket("RGBA", 4, (2, 1, 2, 2), |_| [1.0; 4]);
        pixels.bucket("RGBA", 4, (5, 0, 1, 1), |_| [2.0; 4]);
        let r = pixels.take("RGBA", 0, false).unwrap();
        // The two buckets' bounds, as rows.
        assert_eq!((r.x, r.y, r.width, r.height, r.frame), (2, 0, 4, 3, (8, 4)));
        assert_eq!(r.rgba[3], [2.0; 4]);
        assert_eq!(r.rgba[4], [1.0; 4]);
        assert_eq!(pixels.take("RGBA", 0, false), None);
        assert_eq!(pixels.take("RGBA", 0, true).map(|r| r.rgba.len()), Some(32));
        assert_eq!(pixels.take("diffuse", 0, true), None);
        // Each reader its own: the Viewer has not taken any.
        assert_eq!(pixels.take("RGBA", 1, false).map(|r| (r.width, r.height)), Some((8, 4)));
    }

    #[test]
    fn frame_tokens_are_put_in() {
        assert_eq!(with_frame("render/beauty.$F4.exr", 1.0), "render/beauty.0001.exr");
        assert_eq!(with_frame("a.$F.exr", 12.0), "a.12.exr");
        assert_eq!(with_frame("plain.exr", 3.0), "plain.exr");
    }
}
