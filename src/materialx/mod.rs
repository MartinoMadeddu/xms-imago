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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};

use xms_matx::{Graph, Library};

use crate::core::material::PreviewSurface;
use crate::types::NodeId;

/// The node library: MaterialX's, the shaders each renderer's definition
/// ships (`ai_standard_volume`), and, once renderer shaders are turned on
/// (`renderer_shaders`), each renderer's SDK's own definitions. Made at the
/// first call, made again once when the SDKs' are first loaded; a library
/// stays for the program's life, so graphs read with it keep their
/// definitions, and turning renderer shaders off only takes the SDKs' out of
/// the add menu (`offered`).
pub fn lib() -> &'static Library {
    if let Some(l) = *LIB.read().unwrap_or_else(|e| e.into_inner()) {
        return l;
    }
    let mut w = LIB.write().unwrap_or_else(|e| e.into_inner());
    *w.get_or_insert_with(|| {
        let (lib, notes) = with_renderers(Library::standard(), Files::Shipped);
        note(notes);
        let lib: &'static Library = Box::leak(Box::new(lib));
        lib
    })
}

static LIB: RwLock<Option<&'static Library>> = RwLock::new(None);
/// Renderer shaders on: the SDKs' definitions offered, loaded once.
static SDK_ON: AtomicBool = AtomicBool::new(false);
static SDK_LOADED: AtomicBool = AtomicBool::new(false);
/// The SDKs' files read, by their paths.
static SDK_FILES: Mutex<Vec<String>> = Mutex::new(vec![]);
static NOTES: Mutex<Vec<String>> = Mutex::new(vec![]);

/// The environment variable that turns renderer shaders on for every graph
/// (`1`), whatever the graph says: for a wrapper launching Imago.
pub const RENDERER_SHADERS: &str = "XMS_RENDERER_SHADERS";

/// Whether the environment turns renderer shaders on: read once, at launch.
pub fn renderer_shaders_from_env() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var(RENDERER_SHADERS).is_ok_and(|v| v.trim() == "1"))
}

/// Renderer shaders on or off: on, each renderer's SDK's definitions are read
/// (the first time) and offered. Cheap to call every frame.
pub fn renderer_shaders(on: bool) {
    SDK_ON.store(on, Ordering::Relaxed);
    if !on || SDK_LOADED.swap(true, Ordering::SeqCst) {
        return;
    }
    let (lib, notes) = with_renderers(lib(), Files::Sdk);
    note(notes);
    let lib: &'static Library = Box::leak(Box::new(lib));
    *LIB.write().unwrap_or_else(|e| e.into_inner()) = Some(lib);
}

/// Whether a node is listed in the add menu: not a renderer's hidden one, and
/// not an SDK's while renderer shaders are off.
pub fn offered(def: &xms_matx::NodeDef) -> bool {
    if def.hidden {
        return false;
    }
    let sdk = SDK_FILES.lock().is_ok_and(|f| f.contains(&def.file));
    !sdk || SDK_ON.load(Ordering::Relaxed)
}

/// What loading the renderers' shaders said: each file read, each
/// definition left out.
pub fn shader_notes() -> Vec<String> {
    NOTES.lock().map(|n| n.clone()).unwrap_or_default()
}

fn note(notes: Vec<String>) {
    if let Ok(mut n) = NOTES.lock() {
        n.extend(notes);
    }
}

/// Which of a renderer's shader files.
#[derive(Clone, Copy)]
enum Files {
    Shipped,
    Sdk,
}

/// `base` with the shaders of every renderer definition that has some: those
/// it ships, or its SDK's file.
fn with_renderers(base: &Library, files: Files) -> (Library, Vec<String>) {
    let mut lib = base.clone();
    let mut notes = vec![];
    for d in crate::core::renderers::definitions() {
        let Some(s) = &d.shaders else { continue };
        let renderer = xms_matx::Renderer {
            prefix: s.prefix.clone(),
            context: s.context.clone(),
            label: d.label.clone(),
        };
        let file = match files {
            Files::Shipped => crate::core::renderers::shipped_shaders(&d),
            Files::Sdk => {
                let path = crate::core::vars::expand(s.sdk.trim(), &[]);
                match std::fs::read_to_string(&path) {
                    Ok(text) => Some((path, text)),
                    Err(e) if !path.is_empty() => {
                        notes.push(format!("{}: {path}: {e}: its shaders are those Imago ships", d.label));
                        None
                    }
                    Err(_) => None,
                }
            }
        };
        let Some((path, text)) = file else { continue };
        match lib.with_renderer(&renderer, &path, &text) {
            Ok((with, left_out)) => {
                let count = with.defs().iter().filter(|x| x.file == path).count();
                notes.push(format!("{}: {count} shader definitions from {path}", d.label));
                notes.extend(left_out);
                if matches!(files, Files::Sdk) {
                    if let Ok(mut f) = SDK_FILES.lock() {
                        f.push(path);
                    }
                }
                lib = with;
            }
            Err(e) => notes.push(format!("{}: {e}", d.label)),
        }
    }
    (lib, notes)
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
    /// The wire a node dragged with Shift is over (the node and input it goes
    /// into): let go of, the node goes into it (`Graph::inserted`).
    pub drop_on_wire: Option<(u32, String)>,
    /// The node being dragged was taken out of its wires (Shift held).
    pub pulled_out: bool,
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
            drop_on_wire: None,
            pulled_out: false,
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

    /// The renderers' shipped shaders are in the library from the start:
    /// Arnold's volume, named apart, written as Arnold's; an SDK's are offered
    /// only while renderer shaders are on.
    #[test]
    fn the_shipped_renderer_shaders_are_there() {
        let vol = lib()
            .default_def("ai_standard_volume")
            .expect("Arnold's shipped volume");
        let t = vol.target.as_ref().unwrap();
        assert_eq!((t.context.as_str(), t.shader.as_str()), ("arnold", "standard_volume"));
        assert_eq!(vol.group, "Arnold/Volume");
        assert!(offered(vol), "shipped: offered with renderer shaders off");
        let sdk = xms_matx::NodeDef {
            file: "/sdk/nodedefs.mtlx".into(),
            ..vol.clone()
        };
        SDK_FILES.lock().unwrap().push(sdk.file.clone());
        assert!(!offered(&sdk));
        assert!(lib().default_def("standard_surface").unwrap().target.is_none());
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
