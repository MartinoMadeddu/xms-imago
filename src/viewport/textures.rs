//! Texture files for the viewport.
//!
//! Decoding a set of large images takes seconds, so it happens on
//! background threads. The viewport asks for a file; until it is ready the
//! surface shows its plain colour. `generation` moves each time a file
//! finishes, which is the viewport's cue to rebuild its materials.
//!
//! PNG, JPEG, TIFF and OpenEXR are read. A float image (EXR) holds linear
//! light, and is encoded as sRGB as the texture is (an 8-bit image already
//! is). A `<UDIM>` path is its tiles laid out side by side in one image,
//! tile 1001 at the bottom left, as the tiles lie in UV space: `tiles` says
//! how many across and up, and the viewport scales the UVs to match.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Largest side kept. Larger images are reduced: the viewport is a preview.
pub const MAX_SIDE: u32 = 2048;

/// A decoded image with its chain of half-size copies, ready for the GPU.
#[derive(Debug)]
pub struct Decoded {
    pub width:  u32,
    pub height: u32,
    /// How many sizes `pixels` holds, the full size first.
    pub levels: u32,
    /// RGBA, eight bits a channel, every level one after the other.
    pub pixels: Vec<u8>,
    /// True when some pixel is not fully opaque.
    pub has_alpha: bool,
    /// UDIM tiles across and up the image: [1, 1] for an ordinary texture.
    pub tiles: [u32; 2],
}

enum State { Loading, Ready(Arc<Decoded>), Failed }

static CACHE: Mutex<Option<HashMap<PathBuf, State>>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Counts finished files and changes of display mode.
pub fn generation() -> u64 { GENERATION.load(Ordering::Relaxed) }

/// Have the viewport rebuild what it shows.
pub fn touch() { GENERATION.fetch_add(1, Ordering::Relaxed); }

/// Files still being decoded.
pub fn pending() -> usize {
    CACHE.lock().ok().and_then(|c| c.as_ref().map(|m| m.values().filter(|s| matches!(s, State::Loading)).count())).unwrap_or(0)
}

/// Half-size copy of an RGBA image, each pixel the mean of the four it covers.
fn halve(src: &[u8], w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let (x0, y0) = ((x * 2).min(w - 1), (y * 2).min(h - 1));
            let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
            for c in 0..4 {
                let at = |px: u32, py: u32| src[((py * w + px) * 4 + c) as usize] as u32;
                out[((y * nw + x) * 4 + c) as usize] = ((at(x0, y0) + at(x1, y0) + at(x0, y1) + at(x1, y1) + 2) / 4) as u8;
            }
        }
    }
    (out, nw, nh)
}

/// An RGBA image with its smaller copies down to one pixel.
pub fn with_levels(pixels: Vec<u8>, width: u32, height: u32) -> Decoded {
    let has_alpha = pixels.chunks_exact(4).any(|p| p[3] < 250);
    let mut all = pixels;
    let (mut start, mut w, mut h, mut levels) = (0usize, width, height, 1u32);
    while w > 1 || h > 1 {
        let (next, nw, nh) = halve(&all[start..], w, h);
        start = all.len();
        all.extend_from_slice(&next);
        (w, h) = (nw, nh);
        levels += 1;
    }
    Decoded { width, height, levels, pixels: all, has_alpha, tiles: [1, 1] }
}

/// Linear light to an sRGB byte.
fn srgb(x: f32) -> u8 {
    let x = if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 };
    let s = if x <= 0.003_130_8 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
}

/// An image as eight-bit sRGB RGBA: a float one (EXR) encoded from linear
/// light, an eight-bit one as it is.
fn to_srgb8(img: image::DynamicImage) -> image::RgbaImage {
    use image::DynamicImage as D;
    match img {
        D::ImageRgb32F(_) | D::ImageRgba32F(_) => {
            let f = img.into_rgba32f();
            let (w, h) = f.dimensions();
            let px: Vec<u8> = f.into_raw().chunks_exact(4)
                .flat_map(|p| [srgb(p[0]), srgb(p[1]), srgb(p[2]), (p[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8])
                .collect();
            image::RgbaImage::from_raw(w, h, px).unwrap_or_default()
        }
        other => other.into_rgba8(),
    }
}

fn read(path: &Path) -> Option<image::DynamicImage> {
    let bytes = std::fs::read(path).ok()?;
    image::load_from_memory(&bytes).ok()
}

fn decode(path: &Path) -> Option<Decoded> {
    if crate::core::sequences::is_udim(path) { return decode_udim(path); }
    let mut img = read(path)?;
    if img.width().max(img.height()) > MAX_SIDE {
        img = img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle);
    }
    let rgba = to_srgb8(img);
    let (w, h) = rgba.dimensions();
    Some(with_levels(rgba.into_raw(), w, h))
}

/// Largest side of the image a set of UDIM tiles is laid out in.
pub const UDIM_SIDE: u32 = 4096;

/// A `<UDIM>` texture: every tile found, each made square at one size, laid
/// out as the tiles lie in UV space, 1001 at the bottom left. A tile not
/// there is mid grey. `None` when no tile is found.
fn decode_udim(path: &Path) -> Option<Decoded> {
    use crate::core::sequences::{tile_place, udim_tiles};
    let tiles = udim_tiles(path);
    if tiles.is_empty() { return None; }
    let cols = tiles.iter().map(|(n, _)| tile_place(*n).0).max().unwrap_or(0) + 1;
    let rows = tiles.iter().map(|(n, _)| tile_place(*n).1).max().unwrap_or(0) + 1;
    let side = (UDIM_SIDE / cols.max(rows)).clamp(32, 1024);
    let mut atlas = image::RgbaImage::from_pixel(cols * side, rows * side, image::Rgba([128, 128, 128, 255]));
    let mut any = false;
    for (n, file) in &tiles {
        let Some(img) = read(file) else { continue };
        let tile = to_srgb8(img.resize_exact(side, side, image::imageops::FilterType::Triangle));
        let (c, r) = tile_place(*n);
        // Row 0 of the image is the top: the highest row of tiles.
        image::imageops::replace(&mut atlas, &tile, (c * side) as i64, ((rows - 1 - r) * side) as i64);
        any = true;
    }
    if !any { return None; }
    let (w, h) = atlas.dimensions();
    let mut d = with_levels(atlas.into_raw(), w, h);
    d.tiles = [cols, rows];
    Some(d)
}

/// The decoded file, if it is ready. The first call for a file starts its
/// decoding and returns nothing.
pub fn request(path: &Path) -> Option<Arc<Decoded>> {
    let mut guard = CACHE.lock().ok()?;
    let cache = guard.get_or_insert_with(HashMap::new);
    match cache.get(path) {
        Some(State::Ready(d)) => return Some(d.clone()),
        Some(_) => return None,
        None => {}
    }
    cache.insert(path.to_path_buf(), State::Loading);
    let path = path.to_path_buf();
    std::thread::spawn(move || {
        let state = match decode(&path) { Some(d) => State::Ready(Arc::new(d)), None => State::Failed };
        if let Ok(mut guard) = CACHE.lock() { guard.get_or_insert_with(HashMap::new).insert(path, state); }
        GENERATION.fetch_add(1, Ordering::Relaxed);
    });
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_run_down_to_one_pixel() {
        // A 4 x 2 image: left half white, right half black, opaque.
        let mut px = vec![];
        for _ in 0..2 { for x in 0..4 { let v = if x < 2 { 255 } else { 0 }; px.extend([v, v, v, 255]); } }
        let d = with_levels(px, 4, 2);
        assert_eq!(d.levels, 3);
        assert_eq!(d.pixels.len(), (4 * 2 + 2 * 1 + 1) * 4);
        assert!(!d.has_alpha);
        // The second level is a white pixel and a black one; the last their mean.
        assert_eq!(&d.pixels[32..40], &[255, 255, 255, 255, 0, 0, 0, 255]);
        assert_eq!(d.pixels[40], 128);
    }

    /// A `<UDIM>` path is its tiles in one image, laid out as in UV space:
    /// 1001 at the bottom left, 1002 to its right, 1011 above it.
    #[test]
    fn udim_tiles_are_laid_out_as_in_uv_space() {
        let dir = std::env::temp_dir().join("xms_udim_texture_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (n, c) in [(1001, [255, 0, 0]), (1002, [0, 255, 0]), (1011, [0, 0, 255])] {
            image::RgbaImage::from_pixel(8, 8, image::Rgba([c[0], c[1], c[2], 255])).save(dir.join(format!("t.{n}.png"))).unwrap();
        }
        let d = decode(&dir.join("t.<UDIM>.png")).expect("tiles found");
        assert_eq!(d.tiles, [2, 2]);
        let (w, h) = (d.width, d.height);
        assert_eq!(w, h);
        let at = |x: u32, y: u32| { let k = ((y * w + x) * 4) as usize; [d.pixels[k], d.pixels[k + 1], d.pixels[k + 2]] };
        let (q, b) = (w / 4, h - h / 4);
        assert_eq!(at(q, b), [255, 0, 0], "1001: bottom left");
        assert_eq!(at(w - q, b), [0, 255, 0], "1002: bottom right");
        assert_eq!(at(q, h / 4), [0, 0, 255], "1011: top left");
        assert_eq!(at(w - q, h / 4), [128, 128, 128], "1012 is not there: grey");
        assert!(decode(&dir.join("none.<UDIM>.png")).is_none());
        // Linear light, encoded as sRGB: half is about 188.
        assert_eq!((srgb(0.0), srgb(0.5), srgb(1.0), srgb(4.0)), (0, 188, 255, 255));
    }

    #[test]
    fn a_file_is_decoded_in_the_background_and_then_kept() {
        let dir = std::env::temp_dir().join("xms_texture_test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("dot.png");
        let img = image::RgbaImage::from_fn(8, 8, |x, _| image::Rgba([x as u8 * 30, 0, 0, if x == 0 { 0 } else { 255 }]));
        img.save(&file).unwrap();
        let before = generation();
        assert!(request(&file).is_none(), "the first call only starts the work");
        let mut got = None;
        for _ in 0..200 { got = request(&file); if got.is_some() { break; } std::thread::sleep(std::time::Duration::from_millis(10)); }
        let d = got.expect("decoded");
        assert_eq!((d.width, d.height, d.levels), (8, 8, 4));
        assert!(d.has_alpha && generation() > before);
        // A file that is not an image fails quietly and is not retried forever.
        let bad = dir.join("bad.png");
        std::fs::write(&bad, b"not an image").unwrap();
        for _ in 0..100 { if request(&bad).is_none() && pending() == 0 { break; } std::thread::sleep(std::time::Duration::from_millis(10)); }
        assert!(request(&bad).is_none());
    }
}
