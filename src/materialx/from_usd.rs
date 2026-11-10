//! A USD material's network as a MaterialX graph: the scene layer's shaders
//! handed to `xms_matx::usd::read_network`.

use xms_matx::usd::{read_network, UsdShader, UsdValue};
use xms_matx::Graph;
use xms_scene as rs;

/// The graph of a material read from a stage, and what of it was left out.
/// Refused when it has no surface and no volume, or a shader that is not a
/// MaterialX, UsdPreviewSurface or Arnold one (of Imago's `ai_` nodes).
pub fn from_usd(m: &rs::Material) -> Result<(Graph, Vec<String>), String> {
    if m.surface.is_none() && m.volume.is_none() {
        return Err("no surface or volume shader".into());
    }
    let shaders: Vec<UsdShader> = m
        .nodes
        .iter()
        .map(|n| UsdShader {
            path: n.path.as_str().to_string(),
            id: n.id.clone(),
            inputs: n.inputs.iter().map(|i| (i.name.clone(), value(&i.value))).collect(),
        })
        .collect();
    read_network(
        &shaders,
        terminal(&m.surface),
        terminal(&m.displacement),
        terminal(&m.volume),
        super::lib(),
    )
}

fn value(v: &rs::ShaderValue) -> UsdValue {
    match v {
        rs::ShaderValue::Float(f) => UsdValue::Float(*f),
        rs::ShaderValue::Int(i) => UsdValue::Int(*i),
        rs::ShaderValue::Bool(b) => UsdValue::Bool(*b),
        rs::ShaderValue::Vec2(v) => UsdValue::Floats(v.to_vec()),
        rs::ShaderValue::Vec3(v) => UsdValue::Floats(v.to_vec()),
        rs::ShaderValue::Vec4(v) => UsdValue::Floats(v.to_vec()),
        rs::ShaderValue::String(s) | rs::ShaderValue::Asset(s) => UsdValue::Text(s.clone()),
        rs::ShaderValue::Connection { node, output } => UsdValue::Connect {
            path: node.as_str().to_string(),
            output: output.clone(),
        },
    }
}

/// A material's terminal as the reader takes it: the shader's path and output.
fn terminal(t: &Option<(rs::Path, String)>) -> Option<(&str, &str)> {
    t.as_ref().map(|(p, o)| (p.as_str(), o.as_str()))
}
