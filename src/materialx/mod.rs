//! The MaterialX operator: a material made from a graph of MaterialX nodes.
//!
//! The graph lives in the operator (`NodeType::MaterialX`), so saving, copying,
//! pasting and undo take it with the operator; it is saved as a MaterialX
//! document (`saved`). The nodes, their definitions and how connecting picks
//! them are `xms_matx`'s. The operator puts out a `Material` like the Material
//! operator's, named by the operator, holding its graph: Write USD writes the
//! graph as a MaterialX shading network, the Viewer shows what it can of it
//! (`xms_matx::preview`).
//!
//! The graph is opened in the Node Graph pane (double-click the operator, or
//! ↓): `ui` draws and edits it, `params` lays out the parameters of the node
//! selected in it. What the canvas shows (pan, zoom, which node is selected) is
//! view state, kept with the pane, not in the graph.

pub mod from_usd;
pub mod params;
pub mod ui;

pub use from_usd::from_usd;

use xms_matx::{Graph, Library};

use crate::core::material::PreviewSurface;
use crate::types::NodeId;

/// MaterialX's node library, read once.
pub fn lib() -> &'static Library {
    Library::standard()
}

/// A new graph: a `standard_surface` into a `surfacematerial`.
pub fn new_graph() -> Graph {
    Graph::standard_surface(lib())
}

/// A graph saved in a graph file: as the MaterialX document it is.
pub mod saved {
    use serde::{Deserialize, Deserializer, Serializer};
    use xms_matx::Graph;

    pub fn serialize<S: Serializer>(g: &Graph, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&xms_matx::document::write(g, super::lib()))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Graph, D::Error> {
        let text = String::deserialize(d)?;
        xms_matx::document::read(&text, super::lib()).map_err(serde::de::Error::custom)
    }
}

/// The graph with the variables in its strings put in (`$JOB/tex/wood.png`), as
/// USD is written: renderers read `$JOB` as it is. The graph itself when no
/// string has one.
pub fn with_variables_put_in(graph: &Graph) -> std::borrow::Cow<'_, Graph> {
    use xms_matx::{Binding, Value};
    let has = graph.nodes.iter().any(|n| {
        n.inputs
            .iter()
            .any(|(_, b)| matches!(b, Binding::Value(Value::String(s)) if crate::core::vars::has_vars(s)))
    });
    if !has {
        return std::borrow::Cow::Borrowed(graph);
    }
    let mut g = graph.clone();
    for n in &mut g.nodes {
        for (_, b) in &mut n.inputs {
            if let Binding::Value(Value::String(s)) = b {
                *s = crate::core::vars::expand(s, &[]);
            }
        }
    }
    std::borrow::Cow::Owned(g)
}

/// What the Viewer shows of a graph's material, as the Material operator's
/// surface: the values and texture files of its surface shader.
pub fn surface_of(graph: &Graph) -> PreviewSurface {
    let p = xms_matx::preview(graph, lib());
    let mut s = PreviewSurface {
        diffuse_color: p.color.value,
        emissive_color: p.emission.value,
        metallic: p.metallic.value,
        roughness: p.roughness.value,
        opacity: p.opacity.value,
        clearcoat: p.clearcoat,
        ..Default::default()
    };
    for (input, file) in [
        ("diffuseColor", p.color.file),
        ("emissiveColor", p.emission.file),
        ("metallic", p.metallic.file),
        ("roughness", p.roughness.file),
        ("opacity", p.opacity.file),
        ("normal", p.normal),
    ] {
        if let Some(f) = file {
            s.textures.insert(input.to_string(), f);
        }
    }
    s
}

/// The canvas of an open MaterialX graph: where it is looked at from, what is
/// selected and what is being done. Kept per operator, for the session.
#[derive(Clone, Debug)]
pub struct View {
    pub pan: egui::Vec2,
    pub zoom: f32,
    /// The node selected in the graph: its parameters are shown.
    pub selected: Option<u32>,
    /// Every node selected (a box, or Shift+click): moved and deleted together.
    pub selected_nodes: Vec<u32>,
    /// Where a box being dragged out to select nodes started, on the canvas.
    pub marquee: Option<egui::Pos2>,
    /// A port clicked to unplug: a click on the empty canvas takes its wires
    /// off (`graph_ports`).
    pub unplug: Option<crate::graph_ports::Picked<u32, String>>,
    /// The inputs whose wires were picked up off an output (Ctrl+drag): let go
    /// of on another output they go there, anywhere else they are gone.
    pub carrying: Vec<(u32, String)>,
    /// The wire selected: the node and input it goes into. Delete takes it off.
    pub selected_wire: Option<(u32, String)>,
    /// A wire being drawn from this node's output.
    pub wiring: Option<(u32, String)>,
    pub dragging: Option<u32>,
    pub drag_offset: egui::Vec2,
    /// A wire dropped on a node: the menu of the inputs that take it.
    pub drop: Option<Drop>,
    /// Why the last connection was refused, shown until the next one.
    pub message: String,
    /// Framed once when first opened.
    pub framed: bool,
    /// Nodes folded the other way from their default: a node with a few inputs
    /// shows them all unless toggled, one with many shows the connected ones
    /// only unless toggled.
    pub toggled: Vec<u32>,
}

/// A wire dropped on a node's body.
#[derive(Clone, Debug)]
pub struct Drop {
    pub from: (u32, String),
    pub to: u32,
    pub at: egui::Pos2,
}

impl Default for View {
    fn default() -> Self {
        View {
            pan: egui::vec2(60.0, 60.0),
            zoom: 1.0,
            selected: None,
            selected_nodes: vec![],
            marquee: None,
            unplug: None,
            carrying: vec![],
            selected_wire: None,
            wiring: None,
            dragging: None,
            drag_offset: egui::Vec2::ZERO,
            drop: None,
            message: String::new(),
            framed: false,
            toggled: vec![],
        }
    }
}

fn view_id(op: NodeId) -> egui::Id {
    egui::Id::new(("materialx_view", op.0))
}

/// The view of an operator's graph.
pub fn view(ctx: &egui::Context, op: NodeId) -> View {
    ctx.data(|d| d.get_temp(view_id(op))).unwrap_or_default()
}

pub fn set_view(ctx: &egui::Context, op: NodeId, v: View) {
    ctx.data_mut(|d| d.insert_temp(view_id(op), v));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_graph_is_saved_as_its_document() {
        let mut g = new_graph();
        let s = g.surface_shader().unwrap();
        g.set_value(s, "base_color", xms_matx::Value::Floats(vec![0.9, 0.1, 0.1]));
        let t = crate::types::NodeType::MaterialX { graph: g.clone() };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("standard_surface"), "{json}");
        let back: crate::types::NodeType = serde_json::from_str(&json).unwrap();
        let crate::types::NodeType::MaterialX { graph } = back else {
            panic!()
        };
        assert_eq!(graph, g);
    }

    #[test]
    fn the_viewer_shows_its_surface() {
        let mut g = new_graph();
        let s = g.surface_shader().unwrap();
        g.set_value(s, "base_color", xms_matx::Value::Floats(vec![0.5, 0.25, 1.0]));
        g.set_value(s, "specular_roughness", xms_matx::Value::Floats(vec![0.7]));
        let surface = surface_of(&g);
        assert_eq!(surface.diffuse_color, [0.5, 0.25, 1.0]);
        assert_eq!(surface.roughness, 0.7);
        assert!(surface.textures.is_empty());
    }
}
