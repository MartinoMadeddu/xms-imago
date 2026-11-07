//! Image operators' work: merging, colour correcting, shuffling channels,
//! cropping, resizing, transforming, blurring, premultiplying, constants, and
//! the images an operator works on.
//!
//! - **The working space.** An operator works on images in the rendering
//!   space (ACEScg), alpha premultiplied, as Nuke and Gaffer do. An image in
//!   another space (an 8-bit file, sRGB) is brought there as it comes in
//!   (`working`), once, and taken as having straight alpha, so it is
//!   premultiplied then; what an operator gives is in the rendering space.
//! - **Windows.** An image's pixels (its data window, `origin` and size) and
//!   its frame (the display window) are kept apart: operators work in frame
//!   coordinates, so images of different sizes and places line up.
//! - **The beauty.** Colour operators work on the beauty (`R`, `G`, `B`, `A`)
//!   unless told other layers; channel operators on any channel.

use std::sync::{Arc, Mutex, Weak};

pub use xms_image::Image;
use xms_image::Channel;

/// How Image Merge lays A over B (premultiplied).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MergeOp {
    #[default]
    Over,
    Under,
    Plus,
    Minus,
    Multiply,
    Screen,
    Max,
    Min,
    Difference,
    In,
    Out,
    Atop,
    Mask,
    Stencil,
}

impl MergeOp {
    pub const ALL: [MergeOp; 14] = [
        MergeOp::Over, MergeOp::Under, MergeOp::Plus, MergeOp::Minus, MergeOp::Multiply, MergeOp::Screen, MergeOp::Max,
        MergeOp::Min, MergeOp::Difference, MergeOp::In, MergeOp::Out, MergeOp::Atop, MergeOp::Mask, MergeOp::Stencil,
    ];

    /// A channel of the result from A's and B's, with A's and B's alphas.
    fn apply(self, a: f32, b: f32, aa: f32, ba: f32) -> f32 {
        match self {
            MergeOp::Over => a + b * (1.0 - aa),
            MergeOp::Under => b + a * (1.0 - ba),
            MergeOp::Plus => a + b,
            MergeOp::Minus => a - b,
            MergeOp::Multiply => a * b,
            MergeOp::Screen => a + b - a * b,
            MergeOp::Max => a.max(b),
            MergeOp::Min => a.min(b),
            MergeOp::Difference => (a - b).abs(),
            MergeOp::In => a * ba,
            MergeOp::Out => a * (1.0 - ba),
            MergeOp::Atop => a * ba + b * (1.0 - aa),
            MergeOp::Mask => b * aa,
            MergeOp::Stencil => b * (1.0 - aa),
        }
    }
}

/// Which pixels Image Merge gives: of every input, or of B's or A's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Bounds {
    #[default]
    Union,
    B,
    A,
}

/// How pixels are taken between others when resizing or transforming.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Filter {
    /// The nearest pixel; when shrinking, the average of those it covers.
    Box,
    Bilinear,
    /// Catmull-Rom: sharp, as Nuke's Cubic.
    #[default]
    Cubic,
}

/// How Reformat fits an image to its new size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Fit {
    /// All of it inside, its shape kept.
    #[default]
    Fit,
    /// The new size filled, its shape kept, what is beyond cut.
    Fill,
    /// Stretched to the new size.
    Distort,
}

/// The beauty's channels: red, green, blue, alpha.
fn beauty(img: &Image) -> [Option<usize>; 4] {
    img.layers().into_iter().find(|l| l.name == "RGBA").map(|l| l.rgba).unwrap_or([None; 4])
}

/// The channel named `name`, made (zeros) when the image has none.
fn channel_mut<'a>(img: &'a mut Image, name: &str) -> &'a mut Vec<f32> {
    let n = img.width * img.height;
    let k = match img.channels.iter().position(|c| c.name == name) {
        Some(k) => k,
        None => {
            img.channels.push(Channel { name: name.to_string(), values: vec![0.0; n] });
            img.channels.len() - 1
        }
    };
    &mut img.channels[k].values
}

/// A channel's value at frame position `(x, y)` (a pixel's corner), 0 outside
/// the image's pixels.
fn at(img: &Image, ch: usize, x: i32, y: i32) -> f32 {
    let (px, py) = (x - img.origin.0, y - img.origin.1);
    if px < 0 || py < 0 || px as usize >= img.width || py as usize >= img.height {
        return 0.0;
    }
    img.channels[ch].values[py as usize * img.width + px as usize]
}

/// An image whose pixels cover `(x, y, w, h)` in frame coordinates, its frame
/// `frame`, each channel made by `value(channel name, x, y)`.
fn made(names: &[String], rect: (i32, i32, usize, usize), frame: (i32, i32, usize, usize), value: impl Fn(&str, i32, i32) -> f32 + Sync) -> Image {
    let (x0, y0, w, h) = rect;
    let channels = names.iter().map(|name| {
        let mut values = vec![0.0; w * h];
        rows(&mut values, w, |y, row| {
            for (x, v) in row.iter_mut().enumerate() {
                *v = value(name, x0 + x as i32, y0 + y as i32);
            }
        });
        Channel { name: name.clone(), values }
    }).collect();
    Image { width: w, height: h, origin: (x0, y0), frame, channels }
}

/// `f(row index, row)` over the rows of `values` (`width` wide), on every core.
fn rows(values: &mut [f32], width: usize, f: impl Fn(usize, &mut [f32]) + Sync) {
    if width == 0 || values.is_empty() {
        return;
    }
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let per = values.len().div_ceil(width).div_ceil(threads).max(16) * width;
    std::thread::scope(|s| {
        for (k, part) in values.chunks_mut(per).enumerate() {
            let f = &f;
            s.spawn(move || {
                for (j, row) in part.chunks_mut(width).enumerate() {
                    f(k * per / width + j, row);
                }
            });
        }
    });
}

/// What an operator works on: `img`, its colour in `space`, brought into the
/// rendering space and premultiplied (an image in another space, an 8-bit
/// file, is taken as having straight alpha). Already there (`space` empty, or
/// Raw): `img` itself. Kept a while, so a graph cooked again converts nothing.
pub fn working(img: &Arc<Image>, space: &str) -> Result<Arc<Image>, String> {
    if space.is_empty() || space.eq_ignore_ascii_case("raw") || space.ends_with("- Raw") {
        return Ok(img.clone());
    }
    type Kept = Vec<((usize, String), Weak<Image>, Arc<Image>)>;
    static KEPT: Mutex<Kept> = Mutex::new(Vec::new());
    let key = (Arc::as_ptr(img) as usize, space.to_string());
    if let Ok(kept) = KEPT.lock() {
        if let Some((_, _, out)) = kept.iter().find(|(k, w, _)| *k == key && w.upgrade().is_some_and(|i| Arc::ptr_eq(&i, img))) {
            return Ok(out.clone());
        }
    }
    let mut out = (**img).clone();
    if let [Some(r), Some(g), Some(b), a] = beauty(&out) {
        let cpu = crate::colour::Colour::get().convert(space, "")?;
        let n = out.width * out.height;
        let alpha = a.map(|a| out.channels[a].values.clone());
        let mut px: Vec<[f32; 4]> = (0..n).map(|i| [out.channels[r].values[i], out.channels[g].values[i], out.channels[b].values[i], 1.0]).collect();
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        std::thread::scope(|s| {
            for part in px.chunks_mut(n.div_ceil(threads).max(1 << 14)) {
                let cpu = &cpu;
                s.spawn(move || cpu.apply(part));
            }
        });
        for (i, p) in px.iter().enumerate() {
            let k = alpha.as_ref().map_or(1.0, |a| a[i]);
            out.channels[r].values[i] = p[0] * k;
            out.channels[g].values[i] = p[1] * k;
            out.channels[b].values[i] = p[2] * k;
        }
    }
    let out = Arc::new(out);
    if let Ok(mut kept) = KEPT.lock() {
        kept.retain(|(_, w, _)| w.strong_count() > 0);
        if kept.len() >= 8 {
            kept.remove(0);
        }
        kept.push((key, Arc::downgrade(img), out.clone()));
    }
    Ok(out)
}

/// The image in the rendering space as another space has it (to write a file
/// in `space`): its colour converted, and alpha made straight again for a
/// file that keeps it so (`straight`).
pub fn to_space(img: &Image, space: &str, straight: bool) -> Result<Image, String> {
    let mut out = img.clone();
    let [Some(r), Some(g), Some(b), a] = beauty(&out) else { return Ok(out) };
    let n = out.width * out.height;
    let alpha = a.map(|a| out.channels[a].values.clone());
    let mut px: Vec<[f32; 4]> = (0..n)
        .map(|i| {
            let k = match (&alpha, straight) {
                (Some(a), true) if a[i] > 0.0 => 1.0 / a[i],
                _ => 1.0,
            };
            [out.channels[r].values[i] * k, out.channels[g].values[i] * k, out.channels[b].values[i] * k, 1.0]
        })
        .collect();
    if !space.is_empty() {
        crate::colour::Colour::get().convert("", space)?.apply(&mut px);
    }
    for (i, p) in px.iter().enumerate() {
        out.channels[r].values[i] = p[0];
        out.channels[g].values[i] = p[1];
        out.channels[b].values[i] = p[2];
    }
    Ok(out)
}

/// B, then each A laid over what is there by `op` (`images`: B first), mixed
/// with what was there by `mix`. The beauty is merged; other channels are
/// B's. The frame is B's.
pub fn merge(images: &[Arc<Image>], op: MergeOp, mix: f32, bounds: Bounds) -> Option<Image> {
    let (b, rest) = images.split_first()?;
    let mut out = (**b).clone();
    for a in rest {
        let rect = |i: &Image| (i.origin.0, i.origin.1, i.origin.0 + i.width as i32, i.origin.1 + i.height as i32);
        let (ra, rb) = (rect(a), rect(&out));
        let (x0, y0, x1, y1) = match bounds {
            Bounds::Union => (ra.0.min(rb.0), ra.1.min(rb.1), ra.2.max(rb.2), ra.3.max(rb.3)),
            Bounds::B => rb,
            Bounds::A => ra,
        };
        let names: Vec<String> = out.channels.iter().map(|c| c.name.clone())
            .chain(["R", "G", "B", "A"].iter().map(|s| s.to_string()))
            .fold(vec![], |mut v, n| { if !v.contains(&n) { v.push(n); } v });
        let (ba, aa) = (beauty(&out), beauty(a));
        let find = |img: &Image, name: &str| img.channels.iter().position(|c| c.name.as_str() == name);
        let under = &out;
        out = made(&names, (x0, y0, (x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize), out.frame, |name, x, y| {
            let bv = find(under, name).map_or(0.0, |c| at(under, c, x, y));
            let role = ["R", "G", "B", "A"].iter().position(|r| *r == name);
            let Some(role) = role else { return bv };
            let av = aa[role].map_or(0.0, |c| at(a, c, x, y));
            let alpha_a = aa[3].map_or(1.0, |c| at(a, c, x, y));
            let alpha_b = ba[3].map_or(1.0, |c| at(under, c, x, y));
            let merged = op.apply(av, bv, alpha_a, alpha_b);
            bv + (merged - bv) * mix
        });
    }
    Some(out)
}

/// Colour Correct's settings, applied in this order.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Grade {
    /// Stops: each one doubles.
    pub exposure: f32,
    /// Around `pivot`: above 1 more, below 1 less.
    pub contrast: f32,
    pub pivot: f32,
    /// 0 grey, 1 as it is.
    pub saturation: f32,
    pub gamma: f32,
    pub gain: [f32; 3],
    pub offset: [f32; 3],
    pub clamp_black: bool,
    pub clamp_white: bool,
}

impl Default for Grade {
    fn default() -> Self {
        Grade { exposure: 0.0, contrast: 1.0, pivot: 0.18, saturation: 1.0, gamma: 1.0, gain: [1.0; 3], offset: [0.0; 3], clamp_black: false, clamp_white: false }
    }
}

impl Grade {
    /// One colour through it.
    fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        // ACEScg's luminance.
        const LUMA: [f32; 3] = [0.272_228_7, 0.674_081_8, 0.053_689_5];
        let k = self.exposure.exp2();
        let mut c = c.map(|v| v * k);
        if (self.contrast - 1.0).abs() > 1e-6 && self.pivot > 0.0 {
            c = c.map(|v| if v > 0.0 { (v / self.pivot).powf(self.contrast) * self.pivot } else { v });
        }
        let l = c[0] * LUMA[0] + c[1] * LUMA[1] + c[2] * LUMA[2];
        c = c.map(|v| l + (v - l) * self.saturation);
        if (self.gamma - 1.0).abs() > 1e-6 && self.gamma > 0.0 {
            c = c.map(|v| if v > 0.0 { v.powf(1.0 / self.gamma) } else { v });
        }
        let mut out = [0.0; 3];
        for i in 0..3 {
            let mut v = c[i] * self.gain[i] + self.offset[i];
            if self.clamp_black { v = v.max(0.0); }
            if self.clamp_white { v = v.min(1.0); }
            out[i] = v;
        }
        out
    }
}

/// The image's colour graded: the beauty, and the layers whose names match
/// `layers` (`*`, `diffuse*`; empty, the beauty only).
pub fn colour_correct(img: &Image, grade: &Grade, layers: &str) -> Image {
    let mut out = img.clone();
    for l in img.layers().into_iter().filter(|l| l.name == "RGBA" || (!layers.trim().is_empty() && glob(layers.trim(), &l.name))) {
        let [Some(r), Some(g), Some(b), _] = l.rgba else { continue };
        for i in 0..out.width * out.height {
            let c = grade.apply([out.channels[r].values[i], out.channels[g].values[i], out.channels[b].values[i]]);
            out.channels[r].values[i] = c[0];
            out.channels[g].values[i] = c[1];
            out.channels[b].values[i] = c[2];
        }
    }
    out
}

/// Whether `name` matches `pattern`: `*` any run of characters, `?` one;
/// several patterns apart by spaces.
pub fn glob(pattern: &str, name: &str) -> bool {
    fn one(p: &[u8], t: &[u8]) -> bool {
        match (p.first(), t.first()) {
            (None, None) => true,
            (Some(b'*'), _) => one(&p[1..], t) || (!t.is_empty() && one(p, &t[1..])),
            (Some(b'?'), Some(_)) => one(&p[1..], &t[1..]),
            (Some(a), Some(b)) if a == b => one(&p[1..], &t[1..]),
            _ => false,
        }
    }
    pattern.split_whitespace().any(|p| one(p.as_bytes(), name.as_bytes()))
}

/// Shuffle: each of red, green, blue, alpha taken from a channel of the image
/// (`R`, `diffuse.r`), of the second (`2:R`), or a constant (`0`, `1`); an
/// empty source leaves the channel as it is. Written into the beauty, or into
/// the layer `to` (`to.r`…). Every other channel passes through.
pub fn shuffle(img: &Image, second: Option<&Image>, sources: &[String; 4], to: &str) -> Image {
    let mut out = img.clone();
    let names: [String; 4] = if to.trim().is_empty() {
        ["R", "G", "B", "A"].map(String::from)
    } else {
        ["r", "g", "b", "a"].map(|c| format!("{}.{c}", to.trim()))
    };
    let n = img.width * img.height;
    // Every source read before any is written: R from G and G from R swap.
    let taken: Vec<Option<Vec<f32>>> = sources.iter().map(|s| {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        if let Ok(v) = s.parse::<f32>() {
            return Some(vec![v; n]);
        }
        let (from, name) = match s.strip_prefix("2:") {
            Some(name) => (second?, name),
            None => (img, s),
        };
        let c = from.channels.iter().position(|c| c.name == name)?;
        Some((0..n).map(|i| at(from, c, img.origin.0 + (i % img.width.max(1)) as i32, img.origin.1 + (i / img.width.max(1)) as i32)).collect())
    }).collect();
    for (name, values) in names.iter().zip(taken) {
        if let Some(v) = values {
            *channel_mut(&mut out, name) = v;
        }
    }
    out
}

/// The channels matching `pattern` deleted, or (`keep`) only those kept.
pub fn delete_channels(img: &Image, pattern: &str, keep: bool) -> Image {
    let mut out = img.clone();
    out.channels.retain(|c| glob(pattern, &c.name) == keep);
    out
}

/// A flat colour, `w` by `h`.
pub fn constant(colour: [f32; 4], w: usize, h: usize) -> Image {
    let n = w * h;
    let channels = ["R", "G", "B", "A"].iter().zip(colour).map(|(name, v)| Channel { name: name.to_string(), values: vec![v; n] }).collect();
    Image { width: w, height: h, origin: (0, 0), frame: (0, 0, w, h), channels }
}

/// The pixels inside a box of the frame (`x`, `y` its corner, from the top
/// left), the rest dropped; with `frame`, the frame made the box too.
pub fn crop(img: &Image, b: (i32, i32, usize, usize), frame: bool) -> Image {
    let names: Vec<String> = img.channels.iter().map(|c| c.name.clone()).collect();
    let x0 = b.0.max(img.origin.0);
    let y0 = b.1.max(img.origin.1);
    let x1 = (b.0 + b.2 as i32).min(img.origin.0 + img.width as i32);
    let y1 = (b.1 + b.3 as i32).min(img.origin.1 + img.height as i32);
    let rect = (x0, y0, (x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
    let index: Vec<(String, usize)> = names.iter().cloned().zip(0..).collect();
    made(&names, rect, if frame { b } else { img.frame }, |name, x, y| {
        index.iter().find(|(n, _)| n.as_str() == name).map_or(0.0, |(_, c)| at(img, *c, x, y))
    })
}

/// A channel's value between pixels, at frame position `(x, y)` (pixel
/// centres at .5), by `filter`; `scale` the footprint when shrinking (box).
fn sample(img: &Image, ch: usize, x: f32, y: f32, filter: Filter, scale: (f32, f32)) -> f32 {
    let get = |i: i32, j: i32| at(img, ch, i, j);
    match filter {
        Filter::Box => {
            // The pixels a destination pixel covers, averaged.
            let (rx, ry) = ((1.0 / scale.0).max(1.0) * 0.5, (1.0 / scale.1).max(1.0) * 0.5);
            let (i0, i1) = ((x - rx).floor() as i32, ((x + rx).ceil() as i32 - 1).max((x - rx).floor() as i32));
            let (j0, j1) = ((y - ry).floor() as i32, ((y + ry).ceil() as i32 - 1).max((y - ry).floor() as i32));
            let mut sum = 0.0;
            for j in j0..=j1 {
                for i in i0..=i1 {
                    sum += get(i, j);
                }
            }
            sum / ((i1 - i0 + 1) * (j1 - j0 + 1)) as f32
        }
        Filter::Bilinear => {
            let (fx, fy) = (x - 0.5, y - 0.5);
            let (i, j) = (fx.floor() as i32, fy.floor() as i32);
            let (tx, ty) = (fx - i as f32, fy - j as f32);
            let top = get(i, j) * (1.0 - tx) + get(i + 1, j) * tx;
            let bottom = get(i, j + 1) * (1.0 - tx) + get(i + 1, j + 1) * tx;
            top * (1.0 - ty) + bottom * ty
        }
        Filter::Cubic => {
            let w = |t: f32| {
                let t = t.abs();
                if t < 1.0 { 1.5 * t * t * t - 2.5 * t * t + 1.0 } else if t < 2.0 { -0.5 * t * t * t + 2.5 * t * t - 4.0 * t + 2.0 } else { 0.0 }
            };
            let (fx, fy) = (x - 0.5, y - 0.5);
            let (i, j) = (fx.floor() as i32, fy.floor() as i32);
            let mut sum = 0.0;
            for dj in -1..=2 {
                let wy = w(fy - (j + dj) as f32);
                for di in -1..=2 {
                    sum += get(i + di, j + dj) * w(fx - (i + di) as f32) * wy;
                }
            }
            sum
        }
    }
}

/// Every channel of a new image covering `frame`, each pixel taken from `img`
/// where `to_source` puts its centre.
fn resampled(img: &Image, frame: (i32, i32, usize, usize), filter: Filter, scale: (f32, f32), to_source: impl Fn(f32, f32) -> (f32, f32) + Sync) -> Image {
    let names: Vec<String> = img.channels.iter().map(|c| c.name.clone()).collect();
    let index: Vec<(String, usize)> = names.iter().cloned().zip(0..).collect();
    made(&names, frame, frame, |name, x, y| {
        let Some((_, c)) = index.iter().find(|(n, _)| n.as_str() == name) else { return 0.0 };
        let (sx, sy) = to_source(x as f32 + 0.5, y as f32 + 0.5);
        sample(img, *c, sx, sy, filter, scale)
    })
}

/// The image's frame made `w` by `h`, the picture fitted, filled or stretched.
pub fn reformat(img: &Image, w: usize, h: usize, fit: Fit, filter: Filter) -> Image {
    let (fx, fy, fw, fh) = img.frame;
    let (fw, fh) = (fw.max(1) as f32, fh.max(1) as f32);
    let (sx, sy) = match fit {
        Fit::Distort => (w as f32 / fw, h as f32 / fh),
        Fit::Fit => { let s = (w as f32 / fw).min(h as f32 / fh); (s, s) }
        Fit::Fill => { let s = (w as f32 / fw).max(h as f32 / fh); (s, s) }
    };
    // Centred in the new frame.
    let (ox, oy) = ((w as f32 - fw * sx) * 0.5, (h as f32 - fh * sy) * 0.5);
    resampled(img, (0, 0, w, h), filter, (sx, sy), |x, y| ((x - ox) / sx + fx as f32, (y - oy) / sy + fy as f32))
}

/// Moved, turned (degrees, anticlockwise on screen) and scaled about `pivot`
/// (frame pixels); the frame kept.
pub fn transform(img: &Image, translate: [f32; 2], rotate: f32, scale: [f32; 2], pivot: [f32; 2], filter: Filter) -> Image {
    let (s, c) = (-rotate.to_radians()).sin_cos();
    let (kx, ky) = (if scale[0].abs() > 1e-6 { scale[0] } else { 1e-6 }, if scale[1].abs() > 1e-6 { scale[1] } else { 1e-6 });
    // Each destination pixel back to where it came from: the move undone,
    // then the turn, then the scale.
    resampled(img, img.frame, filter, (kx.abs(), ky.abs()), |x, y| {
        let (dx, dy) = (x - translate[0] - pivot[0], y + translate[1] - pivot[1]);
        let (rx, ry) = (dx * c + dy * s, -dx * s + dy * c);
        (rx / kx + pivot[0], ry / ky + pivot[1])
    })
}

/// Blurred by `size` pixels across and down: Gaussian (size the width that
/// holds nearly all of it) or box. Every channel.
pub fn blur(img: &Image, size: [f32; 2], gaussian: bool) -> Image {
    let kernel = |size: f32| -> Vec<f32> {
        let r = (size * 0.5).ceil().max(0.0) as i32;
        if r == 0 {
            return vec![1.0];
        }
        let sigma = (size / 6.0).max(1e-3);
        let w: Vec<f32> = (-r..=r).map(|i| if gaussian { (-(i * i) as f32 / (2.0 * sigma * sigma)).exp() } else { 1.0 }).collect();
        let sum: f32 = w.iter().sum();
        w.iter().map(|v| v / sum).collect()
    };
    let (kx, ky) = (kernel(size[0]), kernel(size[1]));
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    for ch in out.channels.iter_mut() {
        let src = ch.values.clone();
        let mut across = vec![0.0; w * h];
        let rx = (kx.len() / 2) as i32;
        rows(&mut across, w, |y, row| {
            for (x, v) in row.iter_mut().enumerate() {
                *v = kx.iter().enumerate().map(|(k, wk)| {
                    let i = (x as i32 + k as i32 - rx).clamp(0, w as i32 - 1) as usize;
                    src[y * w + i] * wk
                }).sum();
            }
        });
        let ry = (ky.len() / 2) as i32;
        rows(&mut ch.values, w, |y, row| {
            for (x, v) in row.iter_mut().enumerate() {
                *v = ky.iter().enumerate().map(|(k, wk)| {
                    let j = (y as i32 + k as i32 - ry).clamp(0, h as i32 - 1) as usize;
                    across[j * w + x] * wk
                }).sum();
            }
        });
    }
    out
}

/// The beauty's colour multiplied by its alpha, or (`undo`) divided by it.
pub fn premultiply(img: &Image, undo: bool) -> Image {
    let mut out = img.clone();
    let [Some(r), Some(g), Some(b), Some(a)] = beauty(&out) else { return out };
    let alpha = out.channels[a].values.clone();
    for c in [r, g, b] {
        for (v, k) in out.channels[c].values.iter_mut().zip(&alpha) {
            if !undo {
                *v *= k;
            } else if *k > 0.0 {
                *v /= k;
            }
        }
    }
    out
}

/// What the last write of each Write Image node did.
static LAST: Mutex<Vec<(crate::types::NodeId, Result<String, String>)>> = Mutex::new(Vec::new());

pub fn remember(node: crate::types::NodeId, result: Result<String, String>) {
    if let Ok(mut last) = LAST.lock() {
        last.retain(|(n, _)| *n != node);
        last.push((node, result));
    }
}

pub fn last(node: crate::types::NodeId) -> Option<Result<String, String>> {
    LAST.lock().ok()?.iter().find(|(n, _)| *n == node).map(|(_, r)| r.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(w: usize, h: usize, c: [f32; 4]) -> Arc<Image> {
        Arc::new(constant(c, w, h))
    }

    fn pixel(img: &Image, name: &str, x: usize, y: usize) -> f32 {
        img.channels.iter().find(|c| c.name == name).unwrap().values[y * img.width + x]
    }

    #[test]
    fn merging_lays_a_over_b() {
        let b = rgba(2, 2, [0.0, 0.0, 1.0, 1.0]);
        let a = rgba(2, 2, [0.5, 0.0, 0.0, 0.5]);
        let out = merge(&[b.clone(), a.clone()], MergeOp::Over, 1.0, Bounds::Union).unwrap();
        assert_eq!((pixel(&out, "R", 0, 0), pixel(&out, "B", 0, 0), pixel(&out, "A", 0, 0)), (0.5, 0.5, 1.0));
        let plus = merge(&[b.clone(), a.clone()], MergeOp::Plus, 1.0, Bounds::Union).unwrap();
        assert_eq!(pixel(&plus, "B", 1, 1), 1.0);
        let half = merge(&[b, a], MergeOp::Over, 0.5, Bounds::Union).unwrap();
        assert_eq!(pixel(&half, "R", 0, 0), 0.25, "mixed halfway back to B");
    }

    #[test]
    fn merging_lines_up_images_by_their_frames() {
        let b = rgba(4, 1, [0.0, 0.0, 0.0, 1.0]);
        let mut a = constant([1.0, 1.0, 1.0, 1.0], 1, 1);
        a.origin = (2, 0);
        let out = merge(&[b, Arc::new(a)], MergeOp::Over, 1.0, Bounds::B).unwrap();
        assert_eq!((0..4).map(|x| pixel(&out, "R", x, 0)).collect::<Vec<_>>(), vec![0.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn a_grade_in_its_order() {
        let g = Grade { exposure: 1.0, ..Default::default() };
        assert_eq!(g.apply([0.25, 0.5, 1.0]), [0.5, 1.0, 2.0]);
        let grey = Grade { saturation: 0.0, ..Default::default() }.apply([1.0, 0.0, 0.0]);
        assert!((grey[0] - grey[1]).abs() < 1e-6 && (grey[1] - grey[2]).abs() < 1e-6);
        let pivot = Grade { contrast: 2.0, ..Default::default() }.apply([0.18, 0.36, 0.0]);
        assert!((pivot[0] - 0.18).abs() < 1e-6 && (pivot[1] - 0.72).abs() < 1e-5, "the pivot stays");
        let clamped = Grade { offset: [-1.0; 3], clamp_black: true, ..Default::default() }.apply([0.5; 3]);
        assert_eq!(clamped, [0.0; 3]);
    }

    #[test]
    fn shuffling_swaps_takes_from_the_second_and_writes_layers() {
        let a = constant([0.1, 0.2, 0.3, 1.0], 1, 1);
        let b = constant([0.9, 0.8, 0.7, 0.5], 1, 1);
        let s = shuffle(&a, Some(&b), &["G".into(), "R".into(), "2:B".into(), "0".into()], "");
        assert_eq!([pixel(&s, "R", 0, 0), pixel(&s, "G", 0, 0), pixel(&s, "B", 0, 0), pixel(&s, "A", 0, 0)], [0.2, 0.1, 0.7, 0.0]);
        let l = shuffle(&a, None, &["R".into(), "G".into(), "B".into(), String::new()], "copy");
        assert_eq!(pixel(&l, "copy.g", 0, 0), 0.2);
        assert_eq!(pixel(&l, "R", 0, 0), 0.1, "the beauty passes through");
        let kept = delete_channels(&l, "copy.*", true);
        assert_eq!(kept.channels.len(), 3);
    }

    #[test]
    fn cropping_reformatting_and_blurring() {
        let img = constant([1.0; 4], 4, 2);
        let c = crop(&img, (1, 0, 2, 2), true);
        assert_eq!((c.width, c.height, c.origin, c.frame), (2, 2, (1, 0), (1, 0, 2, 2)));
        let r = reformat(&img, 8, 4, Fit::Distort, Filter::Box);
        assert_eq!((r.width, r.height), (8, 4));
        assert!((pixel(&r, "R", 3, 1) - 1.0).abs() < 1e-5, "a flat colour stays flat");
        let small = reformat(&img, 2, 1, Fit::Fit, Filter::Box);
        assert!((pixel(&small, "R", 0, 0) - 1.0).abs() < 1e-5);
        let b = blur(&img, [3.0, 3.0], true);
        assert!((pixel(&b, "R", 1, 1) - 1.0).abs() < 1e-5, "flat stays flat, edges held");
    }

    #[test]
    fn a_turn_and_a_move() {
        let mut img = constant([0.0; 4], 3, 3);
        img.channels[0].values[1 * 3 + 2] = 1.0; // the pixel right of the centre
        // A quarter turn anticlockwise about the centre: it goes above it.
        let t = transform(&img, [0.0, 0.0], 90.0, [1.0, 1.0], [1.5, 1.5], Filter::Box);
        assert!((pixel(&t, "R", 1, 0) - 1.0).abs() < 1e-5, "{:?}", t.channels[0].values);
        // One pixel right.
        let m = transform(&img, [1.0, 0.0], 0.0, [1.0, 1.0], [0.0, 0.0], Filter::Box);
        assert!((pixel(&m, "R", 2, 1)).abs() < 1e-5 && (pixel(&img, "R", 2, 1) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn premultiplying_and_back() {
        let img = constant([0.5, 0.5, 0.5, 0.5], 1, 1);
        let p = premultiply(&img, false);
        assert_eq!(pixel(&p, "R", 0, 0), 0.25);
        assert_eq!(pixel(&premultiply(&p, true), "R", 0, 0), 0.5);
        assert!(glob("diffuse* spec?", "specZ") && !glob("diffuse*", "spec"));
    }
}
