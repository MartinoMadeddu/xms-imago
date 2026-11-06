//! Image files for the Load Image operator: which file a frame reads (a path
//! with variables and `####`), reading it into the scene's colour space
//! (ACEScg in Imago's config) from the space it is in, as a Nuke Read node
//! does, and keeping it a while, so scrubbing back over frames already seen is
//! quick.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use xms_image::Image;

/// The file frame `frame` reads: `typed` with its variables put in (`$F4`
/// the frame cooked) and each run of `#` the frame padded to that many digits
/// (`plate.####.exr` is `plate.0012.exr` at frame 12).
pub fn path_at(typed: &str, frame: i64) -> String {
    let expanded = crate::core::vars::expand(typed.trim(), &[("F", frame.to_string())]);
    let mut out = String::with_capacity(expanded.len());
    let mut chars = expanded.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '#' {
            out.push(c);
            continue;
        }
        let mut width = 1;
        while chars.next_if_eq(&'#').is_some() {
            width += 1;
        }
        if frame < 0 {
            out.push_str(&format!("-{:0width$}", -frame));
        } else {
            out.push_str(&format!("{frame:0width$}"));
        }
    }
    out
}

/// What is kept: the newest read last, up to `KEEP_BYTES` of pixels; by file,
/// its time, and the space it was read from.
type Kept = Vec<((String, Option<SystemTime>, String), Arc<Image>)>;
static KEPT: Mutex<Kept> = Mutex::new(Vec::new());
const KEEP_BYTES: usize = 1 << 30;

/// The image at `path`, its pixels in colour space `space` brought into the
/// scene's (empty: already in it): read from the file, or as kept when the
/// file has not changed since. Every layer of an EXR; only the colour (`R`,
/// `G`, `B`) is converted, never data layers (`P`, `N`, depth).
pub fn read(path: &str, space: &str) -> Result<Arc<Image>, String> {
    let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let key = (path.to_string(), stamp, space.to_string());
    if let Ok(kept) = KEPT.lock() {
        if let Some((_, img)) = kept.iter().find(|(k, _)| *k == key) {
            return Ok(img.clone());
        }
    }
    if !Path::new(path).is_file() {
        return Err(format!("{path} not found"));
    }
    let mut img = xms_image::read(Path::new(path))?;
    if !space.is_empty() {
        to_scene(&mut img, space)?;
    }
    let img = Arc::new(img);
    if let Ok(mut kept) = KEPT.lock() {
        kept.retain(|(k, _)| (&k.0, &k.2) != (&key.0, &key.2));
        kept.push((key, img.clone()));
        let bytes = |i: &Image| i.channels.iter().map(|c| c.values.len() * 4).sum::<usize>();
        while kept.len() > 1 && kept.iter().map(|(_, i)| bytes(i)).sum::<usize>() > KEEP_BYTES {
            kept.remove(0);
        }
    }
    Ok(img)
}

/// The colour of `img` (its `R`, `G`, `B`, with `A`) from `space` into the
/// scene's.
fn to_scene(img: &mut Image, space: &str) -> Result<(), String> {
    let cpu = crate::colour::Colour::get().convert(space, "")?;
    let at = |name: &str| img.channels.iter().position(|c| c.name == name);
    let (Some(r), Some(g), Some(b)) = (at("R"), at("G"), at("B")) else { return Ok(()) };
    let a = at("A");
    let n = img.width * img.height;
    let mut px: Vec<[f32; 4]> = (0..n)
        .map(|i| {
            let alpha = a.map_or(1.0, |a| img.channels[a].values[i]);
            [img.channels[r].values[i], img.channels[g].values[i], img.channels[b].values[i], alpha]
        })
        .collect();
    cpu.apply(&mut px);
    for (i, p) in px.iter().enumerate() {
        img.channels[r].values[i] = p[0];
        img.channels[g].values[i] = p[1];
        img.channels[b].values[i] = p[2];
    }
    Ok(())
}

/// The colour space an image is in: the one chosen, else by the file: an EXR
/// or HDR in the rendering space (empty), anything else as the config's sRGB
/// texture space.
pub fn colour_space(chosen: &str, path: &str) -> String {
    if !chosen.is_empty() {
        return chosen.to_string();
    }
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if matches!(ext.as_str(), "exr" | "hdr") {
        return String::new();
    }
    let spaces = crate::colour::Colour::get().spaces();
    let srgb = |s: &&String| s.contains("sRGB") && !s.contains("Display");
    spaces.iter().find(|s| srgb(s) && (s.contains("Texture") || s.contains("Encoded")))
        .or_else(|| spaces.iter().find(srgb))
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sequence_reads_the_frame_cooked() {
        assert_eq!(path_at("/p/plate.####.exr", 12), "/p/plate.0012.exr");
        assert_eq!(path_at("/p/plate.#.exr", 7), "/p/plate.7.exr");
        assert_eq!(path_at("/p/plate.$F4.exr", 12), "/p/plate.0012.exr");
        assert_eq!(path_at("/p/plate.####.exr", -3), "/p/plate.-0003.exr");
        assert_eq!(path_at(" /p/still.png ", 12), "/p/still.png");
    }

    #[test]
    fn an_image_is_read_and_kept() {
        let dir = std::env::temp_dir().join("xms_image_file_test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("grey.0001.png");
        image::RgbaImage::from_pixel(4, 2, image::Rgba([128, 128, 128, 255])).save(&file).unwrap();
        let path = path_at(&dir.join("grey.####.png").to_string_lossy(), 1);
        // As stored: 128 of 255, not decoded.
        let a = read(&path, "").unwrap();
        assert_eq!((a.width, a.height), (4, 2));
        assert!((a.channels[0].values[0] - 128.0 / 255.0).abs() < 1e-6);
        assert!(Arc::ptr_eq(&a, &read(&path, "").unwrap()), "kept, not read again");
        assert!(read(&path_at(&dir.join("grey.####.png").to_string_lossy(), 2), "").is_err());
        // From sRGB into the scene's linear space: mid-grey's code value 128
        // is about 0.22 linear, the same in any primaries for a grey.
        let srgb = colour_space("", &path);
        if !srgb.is_empty() {
            let lin = read(&path, &srgb).unwrap();
            let v = lin.channels[0].values[0];
            assert!((v - 0.2158).abs() < 0.01, "{srgb}: {v}");
            assert!((lin.channels[3].values[0] - 1.0).abs() < 1e-6, "alpha untouched");
        }
    }
}
