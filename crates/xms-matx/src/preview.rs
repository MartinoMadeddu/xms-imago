//! What the Viewer can show of a graph's material: the main values of its
//! surface shader, and the texture files behind them.
//!
//! The Viewer draws a material as a colour, a roughness, a metalness, an
//! opacity and an emission, each maybe from a texture. Here these are read from
//! the shader feeding the material's `surfaceshader`, by its own input names
//! (standard_surface, open_pbr_surface, UsdPreviewSurface, gltf_pbr). An input
//! fed by a graph shows the first texture file found up that graph; whatever
//! the graph does to it on the way is not shown.

use crate::graph::{Binding, Graph};
use crate::library::Library;
use crate::types::Value;

/// A value the Viewer shows, and the texture behind it, if one.
#[derive(Clone, Debug, PartialEq)]
pub struct Channel<T> {
    pub value: T,
    pub file: Option<String>,
}

/// The material as the Viewer shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    /// The shader read: `standard_surface`, ... None when nothing goes into the
    /// material, or the shader is not one of those read.
    pub shader: Option<String>,
    pub color: Channel<[f32; 3]>,
    pub roughness: Channel<f32>,
    pub metallic: Channel<f32>,
    pub opacity: Channel<f32>,
    pub emission: Channel<[f32; 3]>,
    pub clearcoat: f32,
    /// A normal map's file.
    pub normal: Option<String>,
}

impl Default for Preview {
    fn default() -> Self {
        Preview {
            shader: None,
            color: Channel {
                value: [0.18; 3],
                file: None,
            },
            roughness: Channel { value: 0.5, file: None },
            metallic: Channel { value: 0.0, file: None },
            opacity: Channel { value: 1.0, file: None },
            emission: Channel {
                value: [0.0; 3],
                file: None,
            },
            clearcoat: 0.0,
            normal: None,
        }
    }
}

/// Which inputs of a shader give what the Viewer shows, and the weights that
/// scale them.
struct Inputs {
    color: (&'static str, Option<&'static str>),
    roughness: &'static str,
    metallic: &'static str,
    opacity: &'static str,
    emission: (&'static str, Option<&'static str>),
    clearcoat: &'static str,
    normal: &'static str,
}

fn inputs_of(shader: &str) -> Option<Inputs> {
    Some(match shader {
        "standard_surface" => Inputs {
            color: ("base_color", Some("base")),
            roughness: "specular_roughness",
            metallic: "metalness",
            opacity: "opacity",
            emission: ("emission_color", Some("emission")),
            clearcoat: "coat",
            normal: "normal",
        },
        "open_pbr_surface" => Inputs {
            color: ("base_color", Some("base_weight")),
            roughness: "specular_roughness",
            metallic: "base_metalness",
            opacity: "geometry_opacity",
            emission: ("emission_color", None),
            clearcoat: "coat_weight",
            normal: "geometry_normal",
        },
        "UsdPreviewSurface" => Inputs {
            color: ("diffuseColor", None),
            roughness: "roughness",
            metallic: "metallic",
            opacity: "opacity",
            emission: ("emissiveColor", None),
            clearcoat: "clearcoat",
            normal: "normal",
        },
        "gltf_pbr" => Inputs {
            color: ("base_color", None),
            roughness: "roughness",
            metallic: "metallic",
            opacity: "alpha",
            emission: ("emissive", Some("emissive_strength")),
            clearcoat: "clearcoat",
            normal: "normal",
        },
        _ => return None,
    })
}

/// Nodes whose `file` is a texture.
const TEXTURES: [&str; 7] = [
    "image",
    "tiledimage",
    "hextiledimage",
    "UsdUVTexture",
    "gltf_image",
    "gltf_colorimage",
    "gltf_normalmap",
];

/// The first texture file up the graph from an input, if it is connected.
fn texture_up(graph: &Graph, lib: &Library, id: u32, input: &str) -> Option<String> {
    let (src, _) = graph.source(id, input)?;
    let mut seen = vec![];
    let mut todo = vec![src];
    while let Some(n) = todo.pop() {
        if seen.contains(&n) || seen.len() > 64 {
            continue;
        }
        seen.push(n);
        let node = graph.node(n)?;
        if TEXTURES.contains(&node.node.as_str()) {
            if let Some(Value::String(f)) = graph.value(lib, n, "file") {
                if !f.trim().is_empty() {
                    return Some(f.trim().to_string());
                }
            }
        }
        // Up the inputs, the first first.
        for (_, b) in node.inputs.iter().rev() {
            if let Binding::Connect { node, .. } = b {
                todo.push(*node);
            }
        }
    }
    None
}

fn float_of(v: Option<&Value>) -> Option<f32> {
    match v? {
        Value::Floats(f) if !f.is_empty() => Some(f.iter().sum::<f32>() / f.len() as f32),
        Value::Integer(i) => Some(*i as f32),
        _ => None,
    }
}

fn colour_of(v: Option<&Value>) -> Option<[f32; 3]> {
    match v? {
        Value::Floats(f) if f.len() >= 3 => Some([f[0], f[1], f[2]]),
        Value::Floats(f) if f.len() == 1 => Some([f[0]; 3]),
        _ => None,
    }
}

/// What the Viewer shows of the graph's material.
pub fn preview(graph: &Graph, lib: &Library) -> Preview {
    let mut p = Preview::default();
    let Some(id) = graph.surface_shader() else { return p };
    let Some(node) = graph.node(id) else { return p };
    let Some(names) = inputs_of(&node.node) else { return p };
    p.shader = Some(node.node.clone());
    let val = |input: &str| graph.value(lib, id, input);
    let tex = |input: &str| texture_up(graph, lib, id, input);
    let weight = |w: Option<&str>| w.and_then(|w| float_of(val(w))).unwrap_or(1.0);

    // A textured colour is white under its texture, as USD shows it.
    let file = tex(names.color.0);
    let w = weight(names.color.1);
    let c = if file.is_some() {
        [1.0; 3]
    } else {
        colour_of(val(names.color.0)).unwrap_or(p.color.value)
    };
    p.color = Channel {
        value: [c[0] * w, c[1] * w, c[2] * w],
        file,
    };

    for (slot, input, default) in [
        (&mut p.roughness, names.roughness, 0.5),
        (&mut p.metallic, names.metallic, 0.0),
        (&mut p.opacity, names.opacity, 1.0),
    ] {
        let file = tex(input);
        let value = if file.is_some() {
            default
        } else {
            float_of(val(input)).unwrap_or(default)
        };
        *slot = Channel { value, file };
    }

    let file = tex(names.emission.0);
    let e = colour_of(val(names.emission.0)).unwrap_or([0.0; 3]);
    // OpenPBR's emission is lit by its luminance: off at 0.
    let w = match node.node.as_str() {
        "open_pbr_surface" => {
            if float_of(val("emission_luminance")).unwrap_or(0.0) > 0.0 {
                1.0
            } else {
                0.0
            }
        }
        _ => weight(names.emission.1),
    };
    let e = if file.is_some() {
        [w; 3]
    } else {
        [e[0] * w, e[1] * w, e[2] * w]
    };
    p.emission = Channel { value: e, file };

    p.clearcoat = float_of(val(names.clearcoat)).unwrap_or(0.0);
    p.normal = tex(names.normal);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_standard_surface_shows_its_base() {
        let lib = Library::standard();
        let mut g = Graph::standard_surface(lib);
        let s = g.surface_shader().unwrap();
        let p = preview(&g, lib);
        assert_eq!(p.shader.as_deref(), Some("standard_surface"));
        // base 1 x base_color 0.8.
        assert_eq!(p.color.value, [0.8; 3]);
        assert_eq!(p.roughness.value, 0.2);
        assert_eq!(p.opacity.value, 1.0);

        g.set_value(s, "base", Value::Floats(vec![0.5]));
        g.set_value(s, "metalness", Value::Floats(vec![1.0]));
        let p = preview(&g, lib);
        assert_eq!(p.color.value, [0.4; 3]);
        assert_eq!(p.metallic.value, 1.0);
    }

    #[test]
    fn textures_are_found_up_the_graph() {
        let lib = Library::standard();
        let mut g = Graph::standard_surface(lib);
        let s = g.surface_shader().unwrap();
        let img = g.add(lib, "image", [0.0, 0.0]).unwrap();
        g.set_value(img, "file", Value::String("wood.png".into()));
        let mul = g.add(lib, "multiply", [0.0, 0.0]).unwrap();
        g.connect(lib, img, "out", mul, "in1").unwrap();
        g.connect(lib, mul, "out", s, "base_color").unwrap();
        let p = preview(&g, lib);
        assert_eq!(p.color.file.as_deref(), Some("wood.png"));
        assert_eq!(p.color.value, [1.0; 3]);
        assert_eq!(p.roughness.file, None);
    }

    #[test]
    fn other_shaders_and_none() {
        let lib = Library::standard();
        let mut g = Graph::new();
        assert_eq!(preview(&g, lib), Preview::default());
        let usd = g.add(lib, "UsdPreviewSurface", [0.0, 0.0]).unwrap();
        let m = g.add(lib, "surfacematerial", [0.0, 0.0]).unwrap();
        g.connect(lib, usd, "out", m, "surfaceshader").unwrap();
        g.set_value(usd, "roughness", Value::Floats(vec![0.9]));
        let p = preview(&g, lib);
        assert_eq!(p.shader.as_deref(), Some("UsdPreviewSurface"));
        assert_eq!(p.color.value, [0.18; 3]);
        assert_eq!(p.roughness.value, 0.9);
    }
}
