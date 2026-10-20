//! Images read from files: OpenEXR with every layer and channel, and the
//! formats the `image` crate reads (PNG, JPEG, TIFF, TGA, HDR), as linear
//! RGBA.

use std::path::Path;

use crate::image::{Channel, Image};

/// An image file, read whole.
pub fn read(path: &Path) -> Result<Image, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "exr" { read_exr(path) } else { read_other(path, ext == "hdr") }
}

/// Every layer of an OpenEXR file, each channel named as the file names
/// it, `part.channel` for a named part. Laid out in the box holding every
/// part's data window.
fn read_exr(path: &Path) -> Result<Image, String> {
    use exr::prelude::*;
    let file = read_all_flat_layers_from_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let dw = file.attributes.display_window;
    // The box every part's pixels are in.
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for layer in file.layer_data.iter() {
        let p = layer.attributes.layer_position;
        x0 = x0.min(p.0);
        y0 = y0.min(p.1);
        x1 = x1.max(p.0 + layer.size.0 as i32);
        y1 = y1.max(p.1 + layer.size.1 as i32);
    }
    if x0 > x1 || y0 > y1 { return Err(format!("{}: no pixels", path.display())); }
    let (width, height) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let mut channels = vec![];
    for layer in file.layer_data.iter() {
        let prefix = layer.attributes.layer_name.as_ref().map(|t| t.to_string()).filter(|s| !s.is_empty());
        let (lw, lh) = (layer.size.0, layer.size.1);
        let (ox, oy) = ((layer.attributes.layer_position.0 - x0) as usize, (layer.attributes.layer_position.1 - y0) as usize);
        for ch in layer.channel_data.list.iter() {
            // Subsampled channels (chroma) are not read.
            if ch.sampling.0 != 1 || ch.sampling.1 != 1 { continue; }
            let samples: Vec<f32> = match &ch.sample_data {
                FlatSamples::F16(v) => v.iter().map(|x| x.to_f32()).collect(),
                FlatSamples::F32(v) => v.clone(),
                FlatSamples::U32(v) => v.iter().map(|x| *x as f32).collect(),
            };
            let mut values = vec![0.0f32; width * height];
            for y in 0..lh {
                let row = &samples[y * lw..(y + 1) * lw];
                let at = (y + oy) * width + ox;
                values[at..at + lw].copy_from_slice(row);
            }
            let name = ch.name.to_string();
            channels.push(Channel { name: match &prefix { Some(p) => format!("{p}.{name}"), None => name }, values });
        }
    }
    Ok(Image { width, height, origin: (x0, y0), frame: (dw.position.0, dw.position.1, dw.size.0, dw.size.1), channels })
}

/// Another format, as linear RGBA: 8- and 16-bit images are taken as sRGB,
/// HDR as linear.
fn read_other(path: &Path, linear: bool) -> Result<Image, String> {
    let img = ::image::open(path).map_err(|e| format!("{}: {e}", path.display()))?.to_rgba32f();
    let (width, height) = (img.width() as usize, img.height() as usize);
    let decode = |v: f32| if linear { v } else if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    let mut channels: Vec<Channel> = ["R", "G", "B", "A"].iter().map(|n| Channel { name: n.to_string(), values: Vec::with_capacity(width * height) }).collect();
    for p in img.pixels() {
        for k in 0..3 { channels[k].values.push(decode(p.0[k])); }
        channels[3].values.push(p.0[3]);
    }
    Ok(Image { width, height, origin: (0, 0), frame: (0, 0, width, height), channels })
}
