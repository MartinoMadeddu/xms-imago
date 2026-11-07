//! Images written to files: OpenEXR with every channel as floats (ZIP
//! compressed), the frame and the pixels' place kept; PNG, JPEG, TIFF and TGA
//! as 8-bit, the beauty (`R`, `G`, `B`, `A`) clamped from 0 to 1 (JPEG without
//! alpha). Values are written as given: the colour space and straight alpha
//! of an 8-bit file are the caller's to make.

use std::path::Path;

use crate::image::Image;

/// Write `img` to `path`, the format by its extension.
pub fn write(img: &Image, path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "exr" {
        return write_exr(img, path);
    }
    let channel = |name: &str| img.channels.iter().find(|c| c.name == name).map(|c| &c.values);
    let (r, g, b, a) = (channel("R"), channel("G"), channel("B"), channel("A"));
    let byte = |c: Option<&Vec<f32>>, i: usize, or: f32| {
        let v = c.and_then(|c| c.get(i)).copied().unwrap_or(or);
        (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    };
    let rgba = ::image::RgbaImage::from_fn(img.width as u32, img.height as u32, |x, y| {
        let i = y as usize * img.width + x as usize;
        // One channel on its own is grey.
        let (g, b) = if g.is_none() && b.is_none() { (r, r) } else { (g, b) };
        ::image::Rgba([byte(r, i, 0.0), byte(g, i, 0.0), byte(b, i, 0.0), byte(a, i, 1.0)])
    });
    let out = ::image::DynamicImage::ImageRgba8(rgba);
    let out = if matches!(ext.as_str(), "jpg" | "jpeg") { ::image::DynamicImage::ImageRgb8(out.to_rgb8()) } else { out };
    out.save(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_exr(img: &Image, path: &Path) -> Result<(), String> {
    use exr::prelude::*;
    let channels: SmallVec<[AnyChannel<FlatSamples>; 4]> = img
        .channels
        .iter()
        .map(|c| AnyChannel::new(c.name.as_str(), FlatSamples::F32(c.values.clone())))
        .collect();
    let attributes = LayerAttributes {
        layer_position: Vec2(img.origin.0, img.origin.1),
        ..LayerAttributes::default()
    };
    let layer = Layer::new((img.width, img.height), attributes, Encoding::SMALL_LOSSLESS, AnyChannels::sort(channels));
    let mut out = exr::image::Image::from_layer(layer);
    let (fx, fy, fw, fh) = img.frame;
    if fw > 0 && fh > 0 {
        out.attributes.display_window = IntegerBounds::new((fx, fy), (fw, fh));
    }
    out.write().to_file(path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Channel;

    fn image() -> Image {
        let ch = |name: &str, v: f32| Channel { name: name.into(), values: vec![v; 6] };
        Image {
            width: 3,
            height: 2,
            origin: (0, 0),
            frame: (0, 0, 3, 2),
            channels: vec![ch("R", 0.25), ch("G", 0.5), ch("B", 2.0), ch("A", 1.0), ch("depth.Z", 7.5)],
        }
    }

    #[test]
    fn an_exr_keeps_every_channel_as_floats() {
        let path = std::env::temp_dir().join("xms_image_write_test.exr");
        write(&image(), &path).unwrap();
        let back = crate::read(&path).unwrap();
        assert_eq!((back.width, back.height), (3, 2));
        let z = back.channels.iter().find(|c| c.name == "depth.Z").unwrap();
        assert_eq!(z.values[5], 7.5);
        let b = back.channels.iter().find(|c| c.name == "B").unwrap();
        assert_eq!(b.values[0], 2.0, "above 1, kept");
    }

    #[test]
    fn a_png_is_the_beauty_in_bytes() {
        let path = std::env::temp_dir().join("xms_image_write_test.png");
        write(&image(), &path).unwrap();
        let back = crate::read(&path).unwrap();
        let r = back.channels.iter().find(|c| c.name == "R").unwrap();
        assert!((r.values[0] - 64.0 / 255.0).abs() < 1e-6);
        let b = back.channels.iter().find(|c| c.name == "B").unwrap();
        assert_eq!(b.values[0], 1.0, "clamped");
    }
}
