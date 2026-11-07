//! An image as renders have it: channels of floats, grouped into layers (AOVs)
//! by their names, and how one is shown on screen.
//!
//! - **Channels** are named as in the file (`R`, `combineddiffuse.r`, `P.x`),
//!   one float per pixel, rows from the top.
//! - **Layers** are the channels sharing what is before the last dot:
//!   `combineddiffuse.r`, `.g`, `.b` are the layer `combineddiffuse`. The
//!   unprefixed `R`, `G`, `B`, `A` are the beauty, listed first; another
//!   unprefixed channel (`Z`) is a layer of its own.
//! - **Showing** one: its colour, one channel, its alpha or its luminance, made
//!   brighter or darker by an exposure (in stops), through sRGB or a plain
//!   gamma, as 8-bit RGBA.
//!
//! Pure std: no part of it knows a file format or a screen.

/// An image: its size, where it is in the frame, and its channels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    /// Where its pixels are in the frame (the data window's corner): an
    /// overscan render starts before 0.
    pub origin: (i32, i32),
    /// The frame (the display window): position and size.
    pub frame: (i32, i32, usize, usize),
    pub channels: Vec<Channel>,
}

/// One channel: its name, and a value per pixel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Channel {
    pub name: String,
    pub values: Vec<f32>,
}

/// A layer: channels sharing a prefix, and which of them is red, green, blue
/// and alpha (or x, y, z, w; u, v).
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// Its name: the prefix, `RGBA` for the beauty.
    pub name: String,
    /// Its channels, by index into `Image::channels`, in the file's order.
    pub channels: Vec<usize>,
    /// The channel that is each of red, green, blue, alpha.
    pub rgba: [Option<usize>; 4],
}

impl Layer {
    /// How many colour channels it has: 1 is shown grey.
    pub fn colours(&self) -> usize {
        self.rgba[..3].iter().filter(|c| c.is_some()).count()
    }
}

/// What of a layer is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Show {
    /// Its colour: red, green and blue (one channel grey, two as red and
    /// green).
    #[default]
    Colour,
    Red,
    Green,
    Blue,
    Alpha,
    /// Rec. 709 luminance of its colour.
    Luminance,
}

impl Show {
    pub const ALL: [Show; 6] = [
        Show::Colour,
        Show::Red,
        Show::Green,
        Show::Blue,
        Show::Alpha,
        Show::Luminance,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Show::Colour => "RGB",
            Show::Red => "R",
            Show::Green => "G",
            Show::Blue => "B",
            Show::Alpha => "A",
            Show::Luminance => "L",
        }
    }
}

/// How values become what is on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Stops: each one doubles the values.
    pub exposure: f32,
    /// A power after the transfer: 1 leaves it.
    pub gamma: f32,
    /// sRGB's transfer (renders are linear); off, values are shown as they are.
    pub srgb: bool,
}

impl Default for Look {
    fn default() -> Self {
        Look {
            exposure: 0.0,
            gamma: 1.0,
            srgb: true,
        }
    }
}

/// A display transform the program gives (OpenColorIO's): linear RGBA to what
/// the display is sent, in place. `id` tells one from another, so a change
/// makes the picture again.
#[derive(Clone)]
pub struct Transform {
    pub id: u64,
    pub apply: std::sync::Arc<dyn Fn(&mut [[f32; 4]]) + Send + Sync>,
}

impl std::fmt::Debug for Transform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Transform({})", self.id)
    }
}

impl Look {
    /// A value a display transform gave, as a byte: gamma, then clamped.
    pub fn display_byte(&self, v: f32) -> u8 {
        if !v.is_finite() {
            return 0;
        }
        let mut x = v.max(0.0);
        if (self.gamma - 1.0).abs() > 1e-6 && self.gamma > 0.0 {
            x = x.powf(1.0 / self.gamma);
        }
        (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    }

    /// A linear value as a byte on screen.
    pub fn byte(&self, v: f32) -> u8 {
        if !v.is_finite() {
            return 0;
        }
        let mut x = (v * self.exposure.exp2()).max(0.0);
        if self.srgb {
            x = if x <= 0.003_130_8 {
                12.92 * x
            } else {
                1.055 * x.powf(1.0 / 2.4) - 0.055
            };
        }
        if (self.gamma - 1.0).abs() > 1e-6 && self.gamma > 0.0 {
            x = x.powf(1.0 / self.gamma);
        }
        (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    }
}

/// Which colour a channel's suffix is: red, green, blue, alpha.
fn role(suffix: &str, prefixed: bool) -> Option<usize> {
    match suffix.to_ascii_lowercase().as_str() {
        "r" | "red" => Some(0),
        "g" | "green" => Some(1),
        "b" | "blue" => Some(2),
        "a" | "alpha" => Some(3),
        "x" | "u" if prefixed => Some(0),
        "y" | "v" if prefixed => Some(1),
        "z" if prefixed => Some(2),
        "w" if prefixed => Some(3),
        _ => None,
    }
}

impl Image {
    /// An image of one value everywhere in each named channel.
    pub fn filled(width: usize, height: usize, names: &[&str], value: f32) -> Self {
        Image {
            width,
            height,
            origin: (0, 0),
            frame: (0, 0, width, height),
            channels: names
                .iter()
                .map(|n| Channel {
                    name: n.to_string(),
                    values: vec![value; width * height],
                })
                .collect(),
        }
    }

    /// Its layers: the beauty first, then the others in the order their first
    /// channel comes.
    pub fn layers(&self) -> Vec<Layer> {
        let mut out: Vec<Layer> = vec![];
        let at = |name: String, out: &mut Vec<Layer>| -> usize {
            match out.iter().position(|l| l.name == name) {
                Some(k) => k,
                None => {
                    out.push(Layer {
                        name,
                        channels: vec![],
                        rgba: [None; 4],
                    });
                    out.len() - 1
                }
            }
        };
        for (i, c) in self.channels.iter().enumerate() {
            let (prefix, suffix) = match c.name.rfind('.') {
                Some(k) => (&c.name[..k], &c.name[k + 1..]),
                None => ("", c.name.as_str()),
            };
            let prefixed = !prefix.is_empty();
            let r = role(suffix, prefixed);
            // An unprefixed channel that is not R, G, B or A is a layer of its own.
            let name = if prefixed {
                prefix.to_string()
            } else if r.is_some() {
                "RGBA".to_string()
            } else {
                suffix.to_string()
            };
            let k = at(name, &mut out);
            let l = &mut out[k];
            l.channels.push(i);
            match r {
                Some(r) if l.rgba[r].is_none() => l.rgba[r] = Some(i),
                _ => {}
            }
        }
        // A layer with no channel named as a colour: its channels in order.
        for l in out.iter_mut() {
            if l.rgba.iter().all(|c| c.is_none()) {
                for (k, c) in l.channels.iter().take(4).enumerate() {
                    l.rgba[k] = Some(*c);
                }
            }
        }
        if let Some(k) = out.iter().position(|l| l.name == "RGBA") {
            let beauty = out.remove(k);
            out.insert(0, beauty);
        }
        out
    }

    /// A pixel's value in a channel, by index (row from the top).
    fn at(&self, channel: Option<usize>, p: usize) -> f32 {
        channel
            .and_then(|c| self.channels.get(c))
            .and_then(|c| c.values.get(p))
            .copied()
            .unwrap_or(0.0)
    }

    /// A layer as it is shown: 8-bit RGBA, rows from the top, opaque.
    pub fn display(&self, layer: &Layer, show: Show, look: &Look) -> Vec<u8> {
        self.display_through(layer, show, look, None)
    }

    /// As `display`, its colours through `transform` in place of sRGB's
    /// curve: the exposure before it, the gamma after. Alpha is shown as it
    /// is.
    pub fn display_through(&self, layer: &Layer, show: Show, look: &Look, transform: Option<&Transform>) -> Vec<u8> {
        let n = self.width * self.height;
        if let Some(t) = transform.filter(|_| show != Show::Alpha) {
            let gain = look.exposure.exp2();
            let mut px: Vec<[f32; 4]> = (0..n)
                .map(|p| {
                    let [r, g, b] = self.shown_rgb(layer, show, p);
                    [r * gain, g * gain, b * gain, 1.0]
                })
                .collect();
            // Through the transform on every core: a 4K frame is millions of
            // pixels, and a change of colour space or view should show at once.
            let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
            let chunk = px.len().div_ceil(threads).max(1 << 14);
            std::thread::scope(|s| {
                for part in px.chunks_mut(chunk) {
                    s.spawn(|| (t.apply)(part));
                }
            });
            return px
                .iter()
                .flat_map(|c| {
                    [
                        look.display_byte(c[0]),
                        look.display_byte(c[1]),
                        look.display_byte(c[2]),
                        255,
                    ]
                })
                .collect();
        }
        let mut out = vec![0u8; n * 4];
        for p in 0..n {
            let rgb = self.shown_rgb(layer, show, p);
            // Alpha is shown as it is: it is not light.
            let byte = |v: f32| {
                if show == Show::Alpha {
                    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
                } else {
                    look.byte(v)
                }
            };
            out[p * 4] = byte(rgb[0]);
            out[p * 4 + 1] = byte(rgb[1]);
            out[p * 4 + 2] = byte(rgb[2]);
            out[p * 4 + 3] = 255;
        }
        out
    }

    /// What a pixel shows of a layer, as RGB: its colour, one channel as
    /// grey, its alpha (1 without), or its luminance.
    fn shown_rgb(&self, layer: &Layer, show: Show, p: usize) -> [f32; 3] {
        let [r, g, b, a] = layer.rgba;
        let grey = layer.colours() == 1;
        let one = |v: f32| [v, v, v];
        match show {
            Show::Colour if grey => one(self.at(r.or(g).or(b), p)),
            Show::Colour => [self.at(r, p), self.at(g, p), self.at(b, p)],
            Show::Red => one(self.at(r, p)),
            Show::Green => one(self.at(g, p)),
            Show::Blue => one(self.at(b, p)),
            Show::Alpha => one(if a.is_some() { self.at(a, p) } else { 1.0 }),
            Show::Luminance => {
                let (x, y, z) = if grey {
                    let v = self.at(r, p);
                    (v, v, v)
                } else {
                    (self.at(r, p), self.at(g, p), self.at(b, p))
                };
                one(0.2126 * x + 0.7152 * y + 0.0722 * z)
            }
        }
    }

    /// Every channel of a layer at a pixel: name and value. Empty outside it.
    pub fn probe(&self, layer: &Layer, x: i64, y: i64) -> Vec<(String, f32)> {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return vec![];
        }
        let p = y as usize * self.width + x as usize;
        layer
            .channels
            .iter()
            .map(|c| (self.channels[*c].name.clone(), self.at(Some(*c), p)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A display transform replaces sRGB's curve: the exposure before it,
    /// the gamma after; alpha is shown as it is.
    #[test]
    fn a_display_transform_shows_the_colours() {
        let img = Image::filled(1, 1, &["R", "G", "B", "A"], 0.25);
        let l = &img.layers()[0];
        let halve = Transform {
            id: 1,
            apply: std::sync::Arc::new(|px: &mut [[f32; 4]]| {
                for p in px {
                    p[0] *= 0.5;
                    p[1] *= 0.5;
                    p[2] *= 0.5;
                }
            }),
        };
        let look = Look {
            exposure: 1.0,
            ..Look::default()
        };
        // 0.25, one stop up, halved: 0.25.
        assert_eq!(img.display_through(l, Show::Colour, &look, Some(&halve))[0], 64);
        assert_eq!(
            img.display_through(l, Show::Alpha, &look, Some(&halve))[0],
            64,
            "alpha as it is"
        );
    }

    #[test]
    fn channels_are_grouped_into_layers() {
        let names = [
            "A",
            "B",
            "G",
            "R",
            "Z",
            "combineddiffuse.b",
            "combineddiffuse.g",
            "combineddiffuse.r",
            "P.x",
            "P.y",
            "P.z",
            "uv.u",
            "uv.v",
            "depth.Z",
            "mask",
        ];
        let img = Image::filled(2, 2, &names, 0.0);
        let layers = img.layers();
        let shown: Vec<&str> = layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(shown, ["RGBA", "Z", "combineddiffuse", "P", "uv", "depth", "mask"]);
        assert_eq!(
            layers[0].rgba,
            [Some(3), Some(2), Some(1), Some(0)],
            "R G B A, whatever the file's order"
        );
        assert_eq!(layers[2].rgba, [Some(7), Some(6), Some(5), None]);
        assert_eq!(
            layers[3].rgba,
            [Some(8), Some(9), Some(10), None],
            "x y z are red, green, blue"
        );
        assert_eq!(layers[4].colours(), 2);
        assert_eq!(layers[5].colours(), 1, "depth.Z is one channel, shown grey");
        assert_eq!(layers[1].rgba[0], Some(4), "Z on its own: its one channel");
    }

    #[test]
    fn a_layer_is_shown_through_its_look() {
        let mut img = Image::filled(2, 1, &["R", "G", "B", "A"], 0.0);
        img.channels[0].values = vec![1.0, 0.18];
        img.channels[3].values = vec![0.5, 1.0];
        let l = &img.layers()[0];
        let lin = Look {
            srgb: false,
            ..Default::default()
        };
        let px = img.display(l, Show::Colour, &lin);
        assert_eq!(&px[..4], &[255, 0, 0, 255]);
        assert_eq!(px[4], 46, "0.18 linear, no transfer");
        assert_eq!(
            img.display(l, Show::Colour, &Look::default())[4],
            118,
            "0.18 through sRGB"
        );
        assert_eq!(
            img.display(
                l,
                Show::Colour,
                &Look {
                    exposure: 1.0,
                    srgb: false,
                    ..Default::default()
                }
            )[4],
            92,
            "a stop up doubles it"
        );
        assert_eq!(img.display(l, Show::Alpha, &Look::default())[0], 128, "alpha as it is");
        assert_eq!(img.display(l, Show::Red, &lin)[..3], [255, 255, 255]);
        assert_eq!(img.display(l, Show::Luminance, &lin)[0], 54, "0.2126 of red");
        assert_eq!(Look::default().byte(f32::NAN), 0);
        assert_eq!(
            img.probe(l, 1, 0),
            vec![
                ("R".to_string(), 0.18),
                ("G".into(), 0.0),
                ("B".into(), 0.0),
                ("A".into(), 1.0)
            ]
        );
        assert!(img.probe(l, 2, 0).is_empty());
    }

    #[test]
    fn one_channel_is_grey() {
        let mut img = Image::filled(1, 1, &["depth.Z"], 0.5);
        img.channels[0].values = vec![0.5];
        let l = &img.layers()[0];
        let px = img.display(
            l,
            Show::Colour,
            &Look {
                srgb: false,
                ..Default::default()
            },
        );
        assert_eq!(&px[..3], &[128, 128, 128]);
    }
}
