//! A graph's material as USD has it: a MaterialX shading network.
//!
//! The `surfacematerial` node is the USD `Material`; every node it reads is
//! a `Shader` under it, its `info:id` its MaterialX definition
//! (`ND_standard_surface_surfaceshader`), with the inputs set on it and its
//! outputs. The Material's `outputs:mtlx:surface` (and
//! `outputs:mtlx:displacement`) connect to the shaders feeding it. This is
//! the network usdMtlx makes from a `.mtlx` file, and what Hydra render
//! delegates read MaterialX from.

use crate::graph::{Binding, Graph};
use crate::library::Library;
use crate::types::{Type, Value};

/// A shader prim of the network: its name under the material, and its
/// attribute lines.
#[derive(Clone, Debug, PartialEq)]
pub struct Shader {
    pub name: String,
    pub lines: Vec<String>,
}

/// The USD type of a MaterialX type.
pub fn usd_type(t: Type) -> &'static str {
    match t {
        Type::Boolean => "bool",
        Type::Integer => "int",
        Type::Float => "float",
        Type::Color3 => "color3f",
        Type::Color4 => "color4f",
        Type::Vector2 => "float2",
        Type::Vector3 => "float3",
        Type::Vector4 => "float4",
        Type::Matrix33 => "matrix3d",
        Type::Matrix44 => "matrix4d",
        Type::String | Type::Geomname => "string",
        Type::Filename => "asset",
        Type::IntegerArray => "int[]",
        Type::FloatArray => "float[]",
        Type::Color3Array => "color3f[]",
        Type::Color4Array => "color4f[]",
        Type::Vector2Array => "float2[]",
        Type::Vector3Array => "float3[]",
        Type::Vector4Array => "float4[]",
        Type::StringArray | Type::GeomnameArray => "string[]",
        // Closures and shaders are connections only.
        _ => "token",
    }
}

/// A value as USD writes it, for a MaterialX type.
pub fn usd_value(t: Type, v: &Value) -> String {
    let n = |x: f32| if x.is_finite() { format!("{x}") } else { "0".into() };
    let tuple = |v: &[f32]| format!("({})", v.iter().map(|x| n(*x)).collect::<Vec<_>>().join(", "));
    let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    match (t, v) {
        (_, Value::Boolean(b)) => (if *b { "1" } else { "0" }).into(),
        (_, Value::Integer(i)) => i.to_string(),
        (Type::Float, Value::Floats(f)) => f.first().map(|x| n(*x)).unwrap_or_else(|| "0".into()),
        (Type::Matrix33 | Type::Matrix44, Value::Floats(f)) => {
            let k = if t == Type::Matrix33 { 3 } else { 4 };
            format!("({})", f.chunks(k).map(tuple).collect::<Vec<_>>().join(", "))
        }
        (_, Value::Floats(f)) => tuple(f),
        (Type::Filename, Value::String(s)) => format!("@{s}@"),
        (_, Value::String(s)) => quote(s),
        (_, Value::IntegerArray(v)) => format!("[{}]", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ")),
        (_, Value::FloatArray(f)) => {
            let k = match t {
                Type::Color3Array | Type::Vector3Array => 3,
                Type::Color4Array | Type::Vector4Array => 4,
                Type::Vector2Array => 2,
                _ => 1,
            };
            if k == 1 {
                format!("[{}]", f.iter().map(|x| n(*x)).collect::<Vec<_>>().join(", "))
            } else {
                format!("[{}]", f.chunks(k).map(tuple).collect::<Vec<_>>().join(", "))
            }
        }
        (_, Value::StringArray(v)) => format!("[{}]", v.iter().map(|s| quote(s)).collect::<Vec<_>>().join(", ")),
    }
}

/// The material of `graph` at the USD path `path`: the Material's own
/// lines, and its shaders. None when the graph has no `surfacematerial`
/// or nothing goes into it.
pub fn network(graph: &Graph, lib: &Library, path: &str) -> Option<(Vec<String>, Vec<Shader>)> {
    let material = graph.material()?;
    let mut lines = vec![];
    let mut feeding = vec![];
    for (input, terminal) in [("surfaceshader", "surface"), ("displacementshader", "displacement"), ("volumeshader", "volume")] {
        if let Some((src, output)) = graph.source(material, input) {
            let name = &graph.node(src)?.name;
            lines.push(format!("token outputs:mtlx:{terminal}.connect = <{path}/{name}.outputs:{output}>"));
            feeding.push(src);
        }
    }
    if feeding.is_empty() {
        return None;
    }
    // Every node the material reads, sources first, each once.
    let mut order: Vec<u32> = vec![];
    for f in feeding {
        for id in graph.upstream(f) {
            if !order.contains(&id) {
                order.push(id);
            }
        }
    }
    let shaders = order.into_iter().filter_map(|id| shader(graph, lib, id, path)).collect();
    Some((lines, shaders))
}

fn shader(graph: &Graph, lib: &Library, id: u32, path: &str) -> Option<Shader> {
    let node = graph.node(id)?;
    let def = lib.def(&node.def)?;
    let mut lines = vec![format!("uniform token info:id = \"{}\"", def.name)];
    for (input, b) in &node.inputs {
        let Some(idef) = def.input(input) else { continue };
        let ty = usd_type(idef.ty);
        let uniform = if idef.uniform { "uniform " } else { "" };
        match b {
            Binding::Value(v) if !idef.ty.is_shading() => {
                lines.push(format!("{uniform}{ty} inputs:{input} = {}", usd_value(idef.ty, v)));
            }
            Binding::Connect { node: src, output } => {
                let Some(src) = graph.node(*src) else { continue };
                lines.push(format!("{uniform}{ty} inputs:{input}.connect = <{path}/{}.outputs:{output}>", src.name));
            }
            _ => {}
        }
    }
    for o in &def.outputs {
        lines.push(format!("{} outputs:{}", usd_type(o.ty), o.name));
    }
    Some(Shader { name: node.name.clone(), lines })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_material_is_a_materialx_network() {
        let lib = Library::standard();
        let mut g = Graph::standard_surface(lib);
        let s = g.surface_shader().unwrap();
        let img = g.add(lib, "image", [0.0, 0.0]).unwrap();
        g.set_value(img, "file", Value::String("tex/wood.png".into()));
        g.connect(lib, img, "out", s, "base_color").unwrap();
        g.set_value(s, "specular_roughness", Value::Floats(vec![0.35]));
        // A node the material does not read is not written.
        g.add(lib, "noise3d", [0.0, 0.0]).unwrap();

        let (lines, shaders) = network(&g, lib, "/materials/wood").unwrap();
        assert_eq!(lines, ["token outputs:mtlx:surface.connect = </materials/wood/standard_surface1.outputs:out>"]);
        let names: Vec<_> = shaders.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["image1", "standard_surface1"]);
        let image = &shaders[0].lines;
        assert_eq!(image[0], "uniform token info:id = \"ND_image_color3\"");
        assert!(image.contains(&"uniform asset inputs:file = @tex/wood.png@".to_string()), "{image:?}");
        assert!(image.contains(&"color3f outputs:out".to_string()));
        let surface = &shaders[1].lines;
        assert_eq!(surface[0], "uniform token info:id = \"ND_standard_surface_surfaceshader\"");
        assert!(surface.contains(&"color3f inputs:base_color.connect = </materials/wood/image1.outputs:out>".to_string()), "{surface:?}");
        assert!(surface.contains(&"float inputs:specular_roughness = 0.35".to_string()));
        assert!(surface.contains(&"token outputs:out".to_string()));
    }

    #[test]
    fn nothing_into_the_material_is_no_network() {
        let lib = Library::standard();
        let mut g = Graph::new();
        assert!(network(&g, lib, "/m").is_none());
        g.add(lib, "surfacematerial", [0.0, 0.0]).unwrap();
        assert!(network(&g, lib, "/m").is_none());
    }

    #[test]
    fn values_are_written_as_usd_writes_them() {
        assert_eq!(usd_value(Type::Color3, &Value::Floats(vec![0.5, 1.0, 0.0])), "(0.5, 1, 0)");
        assert_eq!(usd_value(Type::Boolean, &Value::Boolean(true)), "1");
        assert_eq!(usd_value(Type::String, &Value::String("a \"b\"".into())), "\"a \\\"b\\\"\"");
        assert_eq!(usd_value(Type::Matrix33, &Value::Floats(vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0])),
                   "((1, 0, 0), (0, 1, 0), (0, 0, 1))");
        assert_eq!(usd_value(Type::Vector2Array, &Value::FloatArray(vec![0.0, 1.0, 2.0, 3.0])), "[(0, 1), (2, 3)]");
    }
}
