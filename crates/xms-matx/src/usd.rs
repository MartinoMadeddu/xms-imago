//! A graph's material as USD has it: a MaterialX shading network.
//!
//! The `surfacematerial` node is the USD `Material`; every node it reads is
//! a `Shader` under it, its `info:id` its MaterialX definition
//! (`ND_standard_surface_surfaceshader`), with the inputs set on it and its
//! outputs. The Material's `outputs:mtlx:surface` (and
//! `outputs:mtlx:displacement`) connect to the shaders feeding it. This is
//! the network usdMtlx makes from a `.mtlx` file, and what Hydra render
//! delegates read MaterialX from.
//!
//! The UsdPreviewSurface family (`UsdPreviewSurface`, `UsdUVTexture`,
//! `UsdPrimvarReader`, `UsdTransform2d`) is written as USD has it, as any
//! USD tool reads it: their own `info:id`s, their output names (`surface`,
//! `result`), tokens for their string inputs, and a material whose surface
//! is a UsdPreviewSurface connected by `outputs:surface`.
//!
//! The other way, `read_network` makes a graph of a USD material's network
//! as the scene layer reads it, MaterialX shaders and the UsdPreviewSurface
//! family alike.

use std::collections::HashMap;

use crate::graph::{Binding, Graph};
use crate::library::{Library, NodeDef};
use crate::types::{Type, Value};

/// Whether a node is of the UsdPreviewSurface family, written as USD has it.
fn usd_family(node: &str) -> bool {
    matches!(node, "UsdPreviewSurface" | "UsdUVTexture" | "UsdPrimvarReader" | "UsdTransform2d")
}

/// The `info:id` a node is written with: its definition, or, for the
/// UsdPreviewSurface family, USD's own name.
pub fn usd_id(def: &NodeDef) -> String {
    match def.node.as_str() {
        "UsdPreviewSurface" | "UsdUVTexture" | "UsdTransform2d" => def.node.clone(),
        "UsdPrimvarReader" => {
            let ty = match def.out_type() {
                Type::Float => "float",
                Type::Vector2 => "float2",
                Type::Vector3 => "float3",
                Type::Vector4 => "float4",
                Type::Integer => "int",
                Type::Matrix44 => "matrix",
                Type::Boolean => "bool",
                _ => "string",
            };
            format!("UsdPrimvarReader_{ty}")
        }
        _ => def.name.clone(),
    }
}

/// The definition an `info:id` names: a MaterialX definition, or a node of
/// the UsdPreviewSurface family by USD's name.
pub fn def_of_usd_id<'l>(lib: &'l Library, id: &str) -> Option<&'l NodeDef> {
    if let Some(d) = lib.def(id) {
        return Some(d);
    }
    let nd = match id {
        "UsdPreviewSurface" => "ND_UsdPreviewSurface_surfaceshader",
        "UsdUVTexture" => "ND_UsdUVTexture_23",
        "UsdTransform2d" => "ND_UsdTransform2d",
        "UsdPrimvarReader_float" => "ND_UsdPrimvarReader_float",
        "UsdPrimvarReader_float2" => "ND_UsdPrimvarReader_vector2",
        "UsdPrimvarReader_float3" | "UsdPrimvarReader_normal" | "UsdPrimvarReader_point" | "UsdPrimvarReader_vector" => {
            "ND_UsdPrimvarReader_vector3"
        }
        "UsdPrimvarReader_float4" => "ND_UsdPrimvarReader_vector4",
        "UsdPrimvarReader_int" => "ND_UsdPrimvarReader_integer",
        "UsdPrimvarReader_string" => "ND_UsdPrimvarReader_string",
        "UsdPrimvarReader_matrix" => "ND_UsdPrimvarReader_matrix44",
        _ => return None,
    };
    lib.def(nd)
}

/// An output's name in USD: the family's `surface` and `result` where
/// MaterialX says `out`.
fn usd_output<'a>(node: &str, output: &'a str) -> &'a str {
    match (node, output) {
        ("UsdPreviewSurface", "out") => "surface",
        ("UsdPrimvarReader" | "UsdTransform2d", "out") => "result",
        _ => output,
    }
}

/// A texture's wrap mode as USD names it: MaterialX's `periodic` is USD's `repeat`.
fn wrap_to_usd(w: &str) -> &str { if w == "periodic" { "repeat" } else { w } }

/// A texture's wrap mode as MaterialX names it.
fn wrap_from_usd(w: &str) -> &str { if w == "repeat" { "periodic" } else { w } }

/// An input's USD type: for the family, tokens for strings, `normal3f`
/// for a normal, `float4` for the texture's four-channel values.
fn input_type(node: &str, input: &str, t: Type) -> &'static str {
    if usd_family(node) {
        match (t, input) {
            (Type::String, "varname") => return "string",
            (Type::String, _) => return "token",
            (Type::Vector3, "normal") => return "normal3f",
            (Type::Color4, _) => return "float4",
            _ => {}
        }
    }
    usd_type(t)
}

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
            let node = graph.node(src)?;
            // A UsdPreviewSurface is a material's plain surface, as USD has it.
            let prefix = if usd_family(&node.node) { "" } else { "mtlx:" };
            lines.push(format!("token outputs:{prefix}{terminal}.connect = <{path}/{}.outputs:{}>", node.name, usd_output(&node.node, output)));
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
    let mut lines = vec![format!("uniform token info:id = \"{}\"", usd_id(def))];
    for (input, b) in &node.inputs {
        let Some(idef) = def.input(input) else { continue };
        let ty = input_type(&node.node, input, idef.ty);
        let uniform = if idef.uniform { "uniform " } else { "" };
        match b {
            Binding::Value(v) if !idef.ty.is_shading() => {
                let v = match (node.node.as_str(), input.as_str(), v) {
                    ("UsdUVTexture", "wrapS" | "wrapT", Value::String(w)) => Value::String(wrap_to_usd(w).to_string()),
                    _ => v.clone(),
                };
                lines.push(format!("{uniform}{ty} inputs:{input} = {}", usd_value(idef.ty, &v)));
            }
            Binding::Connect { node: src, output } => {
                let Some(src) = graph.node(*src) else { continue };
                lines.push(format!("{uniform}{ty} inputs:{input}.connect = <{path}/{}.outputs:{}>", src.name, usd_output(&src.node, output)));
            }
            _ => {}
        }
    }
    for o in &def.outputs {
        lines.push(format!("{} outputs:{}", usd_type(o.ty), usd_output(&node.node, &o.name)));
    }
    Some(Shader { name: node.name.clone(), lines })
}

/// A value of a USD shader input, as the scene layer reads it.
#[derive(Clone, Debug, PartialEq)]
pub enum UsdValue {
    Float(f32),
    Int(i32),
    Bool(bool),
    Floats(Vec<f32>),
    /// A string, token or asset path.
    Text(String),
    /// The output `output` of the shader at `path`.
    Connect { path: String, output: String },
}

/// A shader of a USD material: its path, its `info:id`, its inputs.
#[derive(Clone, Debug, PartialEq)]
pub struct UsdShader {
    pub path: String,
    pub id: String,
    pub inputs: Vec<(String, UsdValue)>,
}

/// A graph of a USD material's network: each shader a node, by its prim
/// name, its definition from its `info:id` (`def_of_usd_id`), its inputs
/// and connections as they are; a `surfacematerial` reading the shaders at
/// the material's terminals (`surface`, `displacement`: the shader's path
/// and output). Refused when a shader the material reads has an `info:id`
/// the library does not know. What does not fit (an input the definition
/// does not have, a value not of its type) is left out, and said in the
/// notes.
pub fn read_network(
    shaders: &[UsdShader],
    surface: Option<(&str, &str)>,
    displacement: Option<(&str, &str)>,
    lib: &Library,
) -> Result<(Graph, Vec<String>), String> {
    let mut g = Graph::new();
    let mut notes = vec![];
    let mut ids: HashMap<&str, u32> = HashMap::new();
    let mut defs: HashMap<u32, &NodeDef> = HashMap::new();
    for s in shaders {
        let Some(def) = def_of_usd_id(lib, &s.id) else {
            return Err(format!("{}: {} is not a MaterialX or UsdPreviewSurface shader", s.path, if s.id.is_empty() { "a shader with no info:id" } else { &s.id }));
        };
        let name = g.unique_name(s.path.rsplit('/').next().unwrap_or(&s.path));
        let id = g.push(name, def.node.clone(), def.name.clone(), [0.0, 0.0]);
        ids.insert(s.path.as_str(), id);
        defs.insert(id, def);
    }
    // MaterialX's output name for a USD one.
    let output_of = |id: u32, usd: &str| -> String {
        let def = defs[&id];
        if def.output(usd).is_some() { usd.to_string() } else { def.outputs.first().map(|o| o.name.clone()).unwrap_or_else(|| usd.to_string()) }
    };
    for s in shaders {
        let id = ids[s.path.as_str()];
        let def = defs[&id];
        for (input, v) in &s.inputs {
            let Some(idef) = def.input(input) else {
                notes.push(format!("{}: {} has no input {input}: left out", s.path, def.name));
                continue;
            };
            let value = match v {
                UsdValue::Connect { path, output } => match ids.get(path.as_str()) {
                    Some(&src) => {
                        g.bind(id, input, Binding::Connect { node: src, output: output_of(src, output) });
                        continue;
                    }
                    None => { notes.push(format!("{}.{input} reads {path}, not in the material: left out", s.path)); continue; }
                },
                UsdValue::Float(f) => match idef.ty {
                    Type::Integer => Some(Value::Integer(*f as i32)),
                    Type::Boolean => Some(Value::Boolean(*f != 0.0)),
                    t if t.floats() > 0 => Some(Value::Floats(vec![*f; t.floats()])),
                    _ => None,
                },
                UsdValue::Int(i) => match idef.ty {
                    Type::Integer => Some(Value::Integer(*i)),
                    Type::Boolean => Some(Value::Boolean(*i != 0)),
                    Type::Float => Some(Value::Floats(vec![*i as f32])),
                    _ => None,
                },
                UsdValue::Bool(b) => match idef.ty {
                    Type::Boolean => Some(Value::Boolean(*b)),
                    Type::Integer => Some(Value::Integer(*b as i32)),
                    _ => None,
                },
                // Colours read as three or four: the ones the type has.
                UsdValue::Floats(f) => match idef.ty.floats() {
                    0 => None,
                    n => Some(Value::Floats((0..n).map(|k| f.get(k).copied().unwrap_or(if k == 3 { 1.0 } else { 0.0 })).collect())),
                },
                UsdValue::Text(t) if def.node == "UsdUVTexture" && matches!(input.as_str(), "wrapS" | "wrapT") => {
                    Some(Value::String(wrap_from_usd(t).to_string()))
                }
                UsdValue::Text(t) => match idef.ty {
                    Type::String | Type::Filename | Type::Geomname => Some(Value::String(t.clone())),
                    _ => None,
                },
            };
            match value {
                Some(v) => g.set_value(id, input, v),
                None => notes.push(format!("{}.{input}: {v:?} is not a {}: left at its default", s.path, idef.ty)),
            }
        }
    }
    let material = g.add(lib, "surfacematerial", [0.0, 0.0]).ok_or("surfacematerial is not in the library")?;
    for (terminal, input) in [(surface, "surfaceshader"), (displacement, "displacementshader")] {
        let Some((path, output)) = terminal else { continue };
        match ids.get(path) {
            Some(&src) => g.bind(material, input, Binding::Connect { node: src, output: output_of(src, output) }),
            None => notes.push(format!("the material's {input} reads {path}, not in the material")),
        }
    }
    g.arrange();
    Ok((g, notes))
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

    /// The UsdPreviewSurface family is written as USD has it, and read back.
    #[test]
    fn a_preview_surface_is_written_as_usd_has_it() {
        let lib = Library::standard();
        let mut g = Graph::new();
        let s = g.add(lib, "UsdPreviewSurface", [0.0, 0.0]).unwrap();
        let m = g.add(lib, "surfacematerial", [0.0, 0.0]).unwrap();
        g.connect(lib, s, "out", m, "surfaceshader").unwrap();
        let tex = g.add(lib, "UsdUVTexture", [0.0, 0.0]).unwrap();
        g.set_value(tex, "file", Value::String("wood.png".into()));
        g.set_value(tex, "wrapS", Value::String("periodic".into()));
        let st = g.add(lib, "UsdPrimvarReader", [0.0, 0.0]).unwrap();
        g.connect(lib, st, "out", tex, "st").unwrap();
        g.set_value(st, "varname", Value::String("st".into()));
        g.connect(lib, tex, "rgb", s, "diffuseColor").unwrap();
        g.set_value(s, "roughness", Value::Floats(vec![0.4]));

        let (lines, shaders) = network(&g, lib, "/m").unwrap();
        assert_eq!(lines, ["token outputs:surface.connect = </m/UsdPreviewSurface1.outputs:surface>"]);
        let find = |n: &str| shaders.iter().find(|s| s.name == n).unwrap().lines.clone();
        let surface = find("UsdPreviewSurface1");
        assert_eq!(surface[0], "uniform token info:id = \"UsdPreviewSurface\"");
        assert!(surface.contains(&"color3f inputs:diffuseColor.connect = </m/UsdUVTexture1.outputs:rgb>".to_string()), "{surface:?}");
        assert!(surface.contains(&"token outputs:surface".to_string()), "{surface:?}");
        let texture = find("UsdUVTexture1");
        assert_eq!(texture[0], "uniform token info:id = \"UsdUVTexture\"");
        assert!(texture.iter().any(|l| l.ends_with("token inputs:wrapS = \"repeat\"")), "{texture:?}");
        assert!(texture.iter().any(|l| l.ends_with("inputs:st.connect = </m/UsdPrimvarReader1.outputs:result>")), "{texture:?}");
        let reader = find("UsdPrimvarReader1");
        assert_eq!(reader[0], "uniform token info:id = \"UsdPrimvarReader_float2\"");
        assert!(reader.iter().any(|l| l.ends_with("string inputs:varname = \"st\"")), "{reader:?}");

        // Read back as the scene layer would give it.
        let usd = vec![
            UsdShader { path: "/m/surface".into(), id: "UsdPreviewSurface".into(), inputs: vec![
                ("diffuseColor".into(), UsdValue::Connect { path: "/m/tex".into(), output: "rgb".into() }),
                ("roughness".into(), UsdValue::Float(0.4)),
                ("opacity".into(), UsdValue::Int(1)),
                ("nope".into(), UsdValue::Float(1.0)),
            ] },
            UsdShader { path: "/m/tex".into(), id: "UsdUVTexture".into(), inputs: vec![
                ("file".into(), UsdValue::Text("wood.png".into())),
                ("wrapT".into(), UsdValue::Text("repeat".into())),
                ("st".into(), UsdValue::Connect { path: "/m/st".into(), output: "result".into() }),
            ] },
            UsdShader { path: "/m/st".into(), id: "UsdPrimvarReader_float2".into(), inputs: vec![("varname".into(), UsdValue::Text("st".into()))] },
        ];
        let (back, notes) = read_network(&usd, Some(("/m/surface", "surface")), None, lib).unwrap();
        assert_eq!(notes.len(), 1, "{notes:?}");
        let s = back.surface_shader().unwrap();
        assert_eq!(back.node(s).unwrap().def, "ND_UsdPreviewSurface_surfaceshader");
        assert_eq!(back.value(lib, s, "opacity"), Some(&Value::Floats(vec![1.0])));
        let (tex, out) = back.source(s, "diffuseColor").unwrap();
        assert_eq!((back.node(tex).unwrap().def.as_str(), out), ("ND_UsdUVTexture_23", "rgb"));
        assert_eq!(back.value(lib, tex, "wrapT"), Some(&Value::String("periodic".into())));
        let (st, out) = back.source(tex, "st").unwrap();
        assert_eq!((back.node(st).unwrap().def.as_str(), out), ("ND_UsdPrimvarReader_vector2", "out"));
        assert!(back.nodes.iter().all(|n| n.inputs.iter().all(|(i, _)| back.wire_ok(lib, n.id, i))));
        // A MaterialX network reads by its definitions.
        let mx = vec![UsdShader { path: "/x/sr".into(), id: "ND_standard_surface_surfaceshader".into(),
            inputs: vec![("base_color".into(), UsdValue::Floats(vec![1.0, 0.0, 0.0]))] }];
        let (g, _) = read_network(&mx, Some(("/x/sr", "out")), None, lib).unwrap();
        assert_eq!(g.value(lib, g.surface_shader().unwrap(), "base_color"), Some(&Value::Floats(vec![1.0, 0.0, 0.0])));
        // An unknown shader is refused.
        let bad = vec![UsdShader { path: "/x/s".into(), id: "ArnoldStandard".into(), inputs: vec![] }];
        assert!(read_network(&bad, Some(("/x/s", "out")), None, lib).is_err());
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
