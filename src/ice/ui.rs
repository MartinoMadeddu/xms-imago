use super::{IceGraph, IceGraphNode};
use crate::types::{ConnectionId, IceGraphNodeType, NodeId};
use std::collections::HashSet;

// ICE look, named
// Modelled on Softimage ICE: nodes in solid colours by kind (green for data
// and maths, blue for getting and setting geometry, teal for the tree's in and
// out) with a two-line title (the node's own name in bold, its type beneath)
// and a collapse box; a row per port,
// outputs first then inputs; small dots on the node's edge coloured by data
// type, wires in the same colour; a light grey canvas with a fine grid. Every
// size and colour is here, so the look can be tuned in one place.

const TITLE_H: f32 = 32.0; // two lines: the node's name, then its type
const FOLDED_H: f32 = 20.0; // a collapsed node: its name only
const ROW_H: f32 = 15.0; // one port per row
const PAD_B: f32 = 4.0; // space under the last row
const MIN_W: f32 = 96.0;
const TITLE_CH: f32 = 7.4; // width of a title character, for sizing nodes
const LABEL_CH: f32 = 5.9; // width of a label character
const ICONS_W: f32 = 20.0; // collapse box, right of the title
const PORT_R: f32 = 3.2;
const SOCK_HIT: f32 = 16.0; // invisible hit area around a port
const ROUNDING: f32 = 5.0;
const TYPE_PT: f32 = 9.5; // the type line under the name
const TITLE_PT: f32 = 12.5;
const LABEL_PT: f32 = 10.5;
const GRID: f32 = 12.0; // canvas grid spacing
const WIRE_W: f32 = 1.6;

mod pal {
    #![allow(non_snake_case)]
    use egui::Color32;
    // ICE's own look, the same in both themes.
    // The canvas: light grey in the Light theme, dark in the dark ones, as
    // the scene graph's.
    pub fn BG() -> Color32 {
        crate::theme::canvas(170, 100)
    }
    pub fn GRID() -> Color32 {
        crate::theme::canvas(161, 92)
    }
    pub fn BREADCRUMB_BG() -> Color32 {
        crate::theme::c(78, 78, 82)
    }
    pub fn TEXT() -> Color32 {
        crate::theme::c(248, 248, 248)
    }
    pub fn TEXT_DIM() -> Color32 {
        crate::theme::c(210, 210, 210)
    }

    pub fn INK() -> Color32 {
        Color32::from_rgb(12, 12, 12)
    } // titles and labels
    pub fn INK_TYPE() -> Color32 {
        Color32::from_rgba_unmultiplied(12, 12, 12, 150)
    } // the type line
    pub fn DIVIDER() -> Color32 {
        Color32::from_black_alpha(80)
    } // the line under the title
    pub fn RIM() -> Color32 {
        Color32::from_rgb(38, 38, 38)
    } // the node's edge
    pub fn FRAME() -> Color32 {
        Color32::from_rgb(105, 105, 105)
    } // the grey ring around it
    pub fn SELECTED() -> Color32 {
        crate::theme::selection()
    } // the ring of a selected node
    pub fn PORT_RIM() -> Color32 {
        Color32::from_rgb(30, 30, 30)
    }

    // Node colours, by kind.
    pub fn GREEN() -> Color32 {
        Color32::from_rgb(112, 186, 154)
    } // constants, maths, random, arrays: the cyan-green of MaterialX's geometric nodes
    pub fn BLUE() -> Color32 {
        Color32::from_rgb(102, 150, 204)
    } // get / set geometry
    pub fn ORANGE() -> Color32 {
        Color32::from_rgb(192, 140, 92)
    } // what makes geometry: scattering, instancing
    pub fn PORTS() -> Color32 {
        Color32::from_rgb(52, 52, 52)
    } // Execution Ports: dark grey, as MaterialX's material
    pub fn COMPOUND() -> Color32 {
        Color32::from_rgb(162, 146, 204)
    } // a compound: lavender

    // Data types: the colours every graph of XMS gives them (`type_colours`).
    pub fn ERROR() -> Color32 {
        Color32::from_rgb(230, 40, 30)
    } // the node a graph broke at
    pub fn OTHER() -> Color32 {
        crate::type_colours::DataType::Other.colour()
    } // execute and the rest: dark grey
    pub fn ARRAY() -> Color32 {
        crate::type_colours::array()
    } // an array, whatever it holds: the closures' pink
}

/// Where everything of a node sits, relative to its top-left corner.
struct Layout {
    size: egui::Vec2,
    inputs: Vec<egui::Vec2>,
    outputs: Vec<egui::Vec2>,
}

/// The title: the node type's full name in PascalCase (`GetData`,
/// `CrossProduct`, `ExecutionPorts`).
fn title_text(node: &IceGraphNode) -> String {
    crate::types::subnet_node_label(&node.node_type)
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect()
}

/// A Get Data or Get Array Data reading from another prim: its Geometry input
/// wired.
fn from_geometry(node: &IceGraphNode) -> bool {
    matches!(
        node.node_type,
        IceGraphNodeType::GetData { .. } | IceGraphNodeType::GetArrayData { .. }
    ) && node.inputs.get(1).is_some_and(|i| i.connected_output.is_some())
}

/// A Get Data or Get Array Data reading at surface samples: its Samples input
/// wired.
fn sampled(node: &IceGraphNode) -> bool {
    matches!(
        node.node_type,
        IceGraphNodeType::GetData { .. } | IceGraphNodeType::GetArrayData { .. }
    ) && node.inputs.first().is_some_and(|i| i.connected_output.is_some())
}

/// A constant's value, or the attribute a Get or Set Data names, shown in a row
/// of its own under the title.
fn value_text(node: &IceGraphNode) -> Option<String> {
    use xms_ice::Value;
    let list = |v: &[f32]| format!("<{}>", v.iter().map(|x| short(*x)).collect::<Vec<_>>().join(", "));
    match &node.node_type {
        IceGraphNodeType::Constant { value } => Some(match value {
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => short(*f),
            Value::Vec2(v) => list(v),
            Value::Vec3(v) => list(v),
            Value::Vec4(v) | Value::Quat(v) => list(v),
            Value::Mat4(m) if *m == glam::Mat4::IDENTITY.to_cols_array() => "identity".into(),
            Value::Mat4(m) => format!("origin {}", list(&m[12..15])),
            Value::Sample(l) => format!("face {}", l.face),
        }),
        // Across contexts: where from, and how the values become one.
        // Wired to samples: read there.
        IceGraphNodeType::GetData { name, .. } if sampled(node) && from_geometry(node) => {
            Some(format!("{name} at samples, on geometry"))
        }
        IceGraphNodeType::GetArrayData { name, .. } if sampled(node) && from_geometry(node) => {
            Some(format!("{name}[] at samples, on geometry"))
        }
        IceGraphNodeType::GetData { name, .. } if sampled(node) => Some(format!("{name} at samples")),
        IceGraphNodeType::GetArrayData { name, .. } if sampled(node) => Some(format!("{name}[] at samples")),
        IceGraphNodeType::GetData { name, .. } if from_geometry(node) => Some(format!("geometry.{name}")),
        IceGraphNodeType::GetArrayData { name, .. } if from_geometry(node) => Some(format!("geometry.{name}[]")),
        IceGraphNodeType::GetData {
            name,
            context: Some(c),
            resample,
        } => Some(format!("{}.{name} ({})", context_word(*c), resample_word(*resample))),
        IceGraphNodeType::SetData {
            name,
            context: Some(c),
            resample,
        } => Some(format!("{}.{name} ({})", context_word(*c), resample_word(*resample))),
        IceGraphNodeType::GetData { name, .. } | IceGraphNodeType::SetData { name, .. } => Some(format!("self.{name}")),
        IceGraphNodeType::Root { context } => Some(format!("per {context:?}")),
        IceGraphNodeType::Random { seed } => Some(format!("seed {seed}")),
        IceGraphNodeType::Scatter { seed, weight } if weight.trim().is_empty() => Some(format!("seed {seed}")),
        IceGraphNodeType::Scatter { seed, weight } => Some(format!("seed {seed}, by {weight}")),
        IceGraphNodeType::ClosestPoint => None,
        IceGraphNodeType::GetArrayData { name, context: Some(c) } => Some(format!("{}.{name}[]", context_word(*c))),
        IceGraphNodeType::GetArrayData { name, context: None } => Some(format!("{name}[]")),
        IceGraphNodeType::RandomOrientation { seed } => Some(format!("seed {seed}")),
        IceGraphNodeType::GetReference { input, paths } => Some(match crate::core::locations::rows(paths).count() {
            0 => format!("Reference {input}: all"),
            1 => format!(
                "Reference {input}: {}",
                crate::core::locations::rows(paths).next().unwrap_or_default()
            ),
            n => format!("Reference {input}: {n} rows"),
        }),
        // The instancer's name, not its whole path: hierarchies get long.
        IceGraphNodeType::Instancer { path, seed, draw_mode } => {
            let name = path.trim().trim_end_matches('/').rsplit('/').next().unwrap_or_default();
            Some(match draw_mode {
                crate::types::DrawMode::Default => format!("{name}, seed {seed}"),
                m => format!("{name}, seed {seed}, {}", m.label().to_lowercase()),
            })
        }
        IceGraphNodeType::Compose { op } => Some(op.output().name().to_string()),
        IceGraphNodeType::Decompose { op } => Some(match op {
            xms_ice::Decompose::AxisAngle => "axis and angle".to_string(),
            op => op.input().name().to_string(),
        }),
        IceGraphNodeType::Compound {
            definition,
            editable: Some(_),
            ..
        } => Some(format!("{definition}, editable")),
        IceGraphNodeType::Compound { definition, .. } => Some(definition.clone()),
        IceGraphNodeType::CompoundInput { name, .. } => Some(format!("in: {name}")),
        IceGraphNodeType::CompoundOutput { name } => Some(format!("out: {name}")),
        IceGraphNodeType::Parameter { name, .. } => Some(format!("${{{name}}}")),
        IceGraphNodeType::Math { .. } | IceGraphNodeType::Array { .. } | IceGraphNodeType::Concat => None,
    }
}

/// A context as Get Data's line under the title names it.
fn context_word(c: crate::core::geo::Context) -> &'static str {
    use crate::core::geo::Context;
    match c {
        Context::Point => "point",
        Context::Face => "face",
        Context::Corner => "corner",
        Context::Object => "object",
    }
}

fn resample_word(r: xms_ice::Resample) -> &'static str {
    match r {
        xms_ice::Resample::Average => "average",
        xms_ice::Resample::Sum => "sum",
        xms_ice::Resample::Min => "min",
        xms_ice::Resample::Max => "max",
    }
}

/// A number as short as it reads: 1, 0.5, 0.125.
fn short(v: f32) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// Rows: the outputs, then the inputs. Collapsed, a node is its title row
/// alone, every port on its edge at mid-height.
fn layout(node: &IceGraphNode, collapsed: bool) -> Layout {
    let title = (node.name.chars().count() as f32 * TITLE_CH).max(title_text(node).chars().count() as f32 * LABEL_CH)
        + ICONS_W
        + 18.0;
    let label = |n: &str| n.chars().count() as f32 * LABEL_CH + 22.0;
    let value = value_text(node);
    let widest = node
        .outputs
        .iter()
        .map(|o| label(&o.name))
        .chain(node.inputs.iter().map(|i| label(&i.name)))
        .chain(value.as_deref().map(label))
        .fold(0.0, f32::max);
    let w = title.max(widest).max(MIN_W).round();
    if collapsed {
        let mid = FOLDED_H * 0.5;
        return Layout {
            size: egui::vec2(w, FOLDED_H),
            inputs: vec![egui::vec2(0.0, mid); node.inputs.len()],
            outputs: vec![egui::vec2(w, mid); node.outputs.len()],
        };
    }
    // A constant's value takes the first row.
    let first = value.is_some() as usize;
    let row_y = |r: usize| TITLE_H + (r as f32 + 0.5) * ROW_H;
    let outs = node.outputs.len();
    Layout {
        size: egui::vec2(w, TITLE_H + (first + outs + node.inputs.len()) as f32 * ROW_H + PAD_B),
        outputs: (0..outs).map(|r| egui::vec2(w, row_y(first + r))).collect(),
        inputs: (0..node.inputs.len())
            .map(|r| egui::vec2(0.0, row_y(first + outs + r)))
            .collect(),
    }
}

fn input_socket_pos(node: &IceGraphNode, idx: usize, collapsed: &HashSet<NodeId>) -> egui::Pos2 {
    node.position
        + layout(node, collapsed.contains(&node.id))
            .inputs
            .get(idx)
            .copied()
            .unwrap_or_default()
}

fn output_socket_pos(node: &IceGraphNode, idx: usize, collapsed: &HashSet<NodeId>) -> egui::Pos2 {
    node.position
        + layout(node, collapsed.contains(&node.id))
            .outputs
            .get(idx)
            .copied()
            .unwrap_or_default()
}

/// The text colours that read on a node's colour: black on light ones, white on
/// dark ones (Execution Ports). The name's, then the type line's.
fn inks(body: egui::Color32) -> (egui::Color32, egui::Color32) {
    let lum = 0.299 * body.r() as f32 + 0.587 * body.g() as f32 + 0.114 * body.b() as f32;
    if lum > 110.0 {
        (pal::INK(), pal::INK_TYPE())
    } else {
        (
            egui::Color32::from_rgb(240, 240, 240),
            egui::Color32::from_rgba_unmultiplied(240, 240, 240, 160),
        )
    }
}

/// A node's colour, by kind.
fn tint(t: &IceGraphNodeType) -> egui::Color32 {
    match t {
        IceGraphNodeType::Root { .. } => pal::PORTS(),
        IceGraphNodeType::GetData { .. } | IceGraphNodeType::SetData { .. } => pal::BLUE(),
        IceGraphNodeType::Constant { .. }
        | IceGraphNodeType::Math { .. }
        | IceGraphNodeType::Random { .. }
        | IceGraphNodeType::Array { .. }
        | IceGraphNodeType::RandomOrientation { .. }
        | IceGraphNodeType::Concat
        | IceGraphNodeType::Compose { .. }
        | IceGraphNodeType::Decompose { .. } => pal::GREEN(),
        IceGraphNodeType::GetReference { .. } | IceGraphNodeType::GetArrayData { .. } => pal::BLUE(),
        // The Geometry category: what makes or queries geometry.
        IceGraphNodeType::Scatter { .. } | IceGraphNodeType::Instancer { .. } | IceGraphNodeType::ClosestPoint => {
            pal::ORANGE()
        }
        IceGraphNodeType::Compound { .. } => pal::COMPOUND(),
        IceGraphNodeType::CompoundInput { .. } | IceGraphNodeType::CompoundOutput { .. } => pal::PORTS(),
        IceGraphNodeType::Parameter { .. } => pal::GREEN(),
    }
}

/// Which nodes are collapsed: view state, kept with the canvas, not saved.
fn collapsed_id() -> egui::Id {
    egui::Id::new("ice_collapsed_nodes")
}

// Breadcrumb

pub fn draw_breadcrumb(ui: &mut egui::Ui, subnet_name: &str) -> bool {
    let mut exit = false;
    ui.horizontal(|ui| {
        egui::Frame::NONE
            .fill(pal::BREADCRUMB_BG())
            .inner_margin(egui::vec2(8.0, 4.0))
            .show(ui, |ui| {
                if ui.link(egui::RichText::new("Root").color(pal::TEXT())).clicked() {
                    exit = true;
                }
                ui.label(egui::RichText::new(" › ").color(pal::TEXT_DIM()));
                ui.label(
                    egui::RichText::new(subnet_name)
                        .color(egui::Color32::from_rgb(180, 200, 230))
                        .strong(),
                );
            });
    });
    exit
}

/// The bar over an ICE graph: Root, the ICE operator, the compounds entered
/// (a click goes back out to one), ↑ Up while inside one, and Make Editable
/// while the one looked at is locked. Returns whether Root was clicked, and
/// what to tell.
pub fn draw_ice_bar(ui: &mut egui::Ui, graph: &mut IceGraph) -> (bool, Option<String>) {
    let (path, locked) = graph.entered();
    let (mut root, mut out_to, mut editable) = (false, None, false);
    ui.horizontal(|ui| {
        egui::Frame::NONE
            .fill(pal::BREADCRUMB_BG())
            .inner_margin(egui::vec2(8.0, 4.0))
            .show(ui, |ui| {
                if ui.link(egui::RichText::new("Root").color(pal::TEXT())).clicked() {
                    root = true;
                }
                let strong = |t: &str| egui::RichText::new(t).color(egui::Color32::from_rgb(180, 200, 230)).strong();
                ui.label(egui::RichText::new(" › ").color(pal::TEXT_DIM()));
                if path.is_empty() {
                    ui.label(strong(graph.name.as_str()));
                    return;
                }
                if ui.link(strong(graph.name.as_str())).clicked() {
                    out_to = Some(0);
                }
                for (k, name) in path.iter().enumerate() {
                    ui.label(egui::RichText::new(" › ").color(pal::TEXT_DIM()));
                    if k + 1 == path.len() {
                        ui.label(strong(name.as_str()));
                    } else if ui.link(strong(name.as_str())).clicked() {
                        out_to = Some(k + 1);
                    }
                }
            });
        if !path.is_empty() {
            if ui.button("↑ Up").on_hover_text("Back out of this compound (↑)").clicked() {
                out_to = Some(path.len() - 1);
            }
            if locked {
                ui.label(egui::RichText::new("🔒 Locked").color(pal::TEXT_DIM()));
                if ui
                    .button("Make Editable")
                    .on_hover_text("A copy of the definition for this instance alone, to change here; it no longer follows the library")
                    .clicked()
                {
                    editable = true;
                }
            }
        }
    });
    let mut told = None;
    if editable {
        told = super::compound::make_editable(graph);
    }
    // Back out to the graph `depth` compounds in (0: the operator's own); all
    // the way out when going to the scene network.
    let depth = if root { Some(0) } else { out_to };
    if let Some(depth) = depth {
        while graph.outer.len() > depth {
            told = super::compound::leave(graph).or(told);
        }
    }
    (root, told)
}

// Subnet node properties

/// The parameters of a node inside an ICE subnet, laid out like every other
/// node's (see `properties::form`). The parameters of the node selected in an
/// ICE graph. `references`: every prim on each of its operator's reference
/// inputs, by path, from input 1 (Get Reference picks from them); `selected`:
/// the prims selected in the Scene Explorer (a Get Reference row adds them).
/// `browser`: where an ICE compound's Save as Definition asks for a file.
pub fn draw_subnet_node_properties(
    ui: &mut egui::Ui,
    graph: &mut IceGraph,
    references: &[Vec<String>],
    selected: &[String],
    browser: &mut crate::file_browser::FileBrowser,
) {
    use crate::core::geo::Context;
    use crate::properties::form::group;
    // The geometry the graph last ran on, for Get Data's list: listed only
    // while the list is open.
    let last_input = graph.last_input.clone();
    // The context the graph runs over: Set Data's Resample is for writing
    // from it into another.
    let runs_over = graph
        .nodes
        .iter()
        .find_map(|n| match n.node_type {
            IceGraphNodeType::Root { context } => Some(context),
            _ => None,
        })
        .unwrap_or(Context::Point);
    row_menu(ui.ctx(), graph);
    let mut resync = false;
    // A compound's: go inside, or its ports again from the library.
    let (mut enter, mut back) = (false, false);
    let graph_id = graph.id;
    // Its settings promoted: drawn following their parameter.
    let promoted: Vec<String> = graph
        .selected_node
        .and_then(|s| graph.nodes.iter().find(|n| n.id == s))
        .map(|n| {
            graph
                .parameters
                .iter()
                .flat_map(|p| p.targets.iter().filter(|t| t.node == n.name).map(|t| t.path.clone()))
                .collect()
        })
        .unwrap_or_default();
    let whose = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (graph_id.0, graph.selected_node.map(|n| n.0), &promoted).hash(&mut h);
        h.finish()
    };
    let Some(node) = graph
        .selected_node
        .and_then(|s| graph.nodes.iter_mut().find(|n| n.id == s))
    else {
        ui.label("Select a node to see its parameters.");
        return;
    };
    let named = node.name.clone();
    let title = format!(
        "{}  {}",
        crate::types::subnet_node_icon(&node.node_type),
        crate::types::subnet_node_label(&node.node_type)
    );
    group(ui, &title, None, |f| {
        f.text(
            "Name",
            Some("The node's name: one word, a space typed becomes an underscore"),
            &mut node.name,
            "",
        );
    });
    if node.name.chars().any(char::is_whitespace) {
        node.name = crate::node_graph::node_name(&node.name);
    }
    // Its settings promoted follow it by name.
    if node.name != named {
        for t in graph.parameters.iter_mut().flat_map(|p| p.targets.iter_mut()) {
            if t.node == named {
                t.node = node.name.clone();
            }
        }
    }
    // Get Data and Get Array Data with samples wired read there.
    let at_samples = sampled(node);
    // A compound's promoted parameters.
    let compound_params = super::compound::definition(&node.node_type)
        .map(|d| d.parameters.clone())
        .unwrap_or_default();
    // Its settings' rows, right-clicked, have their menu (see
    // `properties::form::probing`); the node is put back after.
    let before = node.clone();
    crate::properties::form::probing(true, whose, promoted);
    match &mut node.node_type {
        IceGraphNodeType::Root { context } => group(ui, "Runs over", None, |f| {
            f.choice(
                "Context",
                Some("The elements every Execute port runs over"),
                context,
                &[
                    (Context::Point, "Point"),
                    (Context::Face, "Face"),
                    (Context::Corner, "Corner"),
                    (Context::Object, "Object"),
                ],
            );
        }),
        IceGraphNodeType::GetData {
            name,
            context,
            resample,
        } => {
            group(ui, "Attribute", None, |f| {
                f.text("Name", Some("P is the points, N the normals, uv the first texture coordinates; any other name is an attribute of the geometry"), name, "P");
                // Every attribute of the geometry the graph last ran on, as the
                // Primitive Inspector lists them: choosing one sets the name
                // and where it is read from. Listed when opened, not before.
                if last_input.lock().is_ok_and(|g| g.is_some()) {
                    f.row(
                        "Attributes",
                        Some("The attributes of the geometry coming in: pick one to read it"),
                        |ui| {
                            egui::ComboBox::from_id_salt("ice_get_data_attrs")
                                .width(ui.available_width() - 8.0)
                                .selected_text("Pick an attribute...")
                                .show_ui(ui, |ui| {
                                    for (c, n, kind) in &super::readable(&last_input) {
                                        if ui
                                            .selectable_label(*n == *name, format!("{n}   {} {kind}", context_word(*c)))
                                            .clicked()
                                        {
                                            *name = if n == "points" { "P".into() } else { n.clone() };
                                            *context = Some(*c);
                                        }
                                    }
                                });
                        },
                    );
                }
                if at_samples {
                    f.choice("Context", Some("Read at the samples wired in: a point's or a corner's attribute blended across the triangle each sample is in, a face's that face's, the object's its one value. Auto: the first that has it, point, corner, face, object"),
                             context, &[(None, "Auto"), (Some(Context::Point), "Point"), (Some(Context::Face), "Face"), (Some(Context::Corner), "Corner"), (Some(Context::Object), "Object")]);
                    return;
                }
                f.choice("Context", Some("Where the attribute is read from. Auto: the graph's own context, or Object. Another context is read across: a face reads its points, a corner its point or face, a point the faces or corners around it. Wire samples into Samples to read there instead; wire a Geometry (Get Reference) to read it from that prim, element by element (it has the same topology: a rest mesh and its animated copy)"),
                         context, &[(None, "Auto"), (Some(Context::Point), "Point"), (Some(Context::Face), "Face"), (Some(Context::Corner), "Corner"), (Some(Context::Object), "Object")]);
                if context.is_some_and(|c| c != Context::Object) {
                    f.choice("Resample", Some("Reading from another context resamples it: when an element touches several of the context read (a face's points, the faces around a point), how they become one value"),
                             resample, &[(xms_ice::Resample::Average, "Average"), (xms_ice::Resample::Sum, "Sum"), (xms_ice::Resample::Min, "Min"), (xms_ice::Resample::Max, "Max")]);
                }
            })
        }
        IceGraphNodeType::SetData {
            name,
            context,
            resample,
        } => group(ui, "Attribute", None, |f| {
            f.text(
                "Name",
                Some("P is the points, N the normals; any other name makes or sets an attribute"),
                name,
                "P",
            );
            f.choice("Context", Some("Where the attribute is written. Auto: the graph's own context. Into another, each element there takes the values of the graph's elements it touches: a face its points, a point the faces around it, the object all of them"),
                     context, &[(None, "Auto"), (Some(Context::Point), "Point"), (Some(Context::Face), "Face"), (Some(Context::Corner), "Corner"), (Some(Context::Object), "Object")]);
            if context.is_some_and(|c| c != runs_over) && runs_over != Context::Object {
                f.choice("Resample", Some("Writing into another context resamples: when an element written touches several of the graph's (a face's points, the faces around a point), how their values become one"),
                         resample, &[(xms_ice::Resample::Average, "Average"), (xms_ice::Resample::Sum, "Sum"), (xms_ice::Resample::Min, "Min"), (xms_ice::Resample::Max, "Max")]);
            }
        }),
        IceGraphNodeType::Constant { value } => group(ui, "Value", None, |f| {
            value_row(f, "Value", value);
        }),
        IceGraphNodeType::Random { seed } => group(ui, "Random", None, |f| {
            f.row(
                "Seed",
                Some("Another seed, other values; the same seed gives the same values every time"),
                |ui| crate::properties::form::number(ui, None, seed, |d| d.speed(1)),
            );
        }),
        IceGraphNodeType::Scatter { seed, weight } => group(ui, "Scatter", None, |f| {
            f.row(
                "Seed",
                Some("Another seed, other places; the same seed gives the same places every time"),
                |ui| crate::properties::form::number(ui, None, seed, |d| d.speed(1)),
            );
            f.text("Weight", Some("A per-face or per-point number scaling how many land where (0 none there, 2 twice as many); empty, evenly by area"), weight, "");
        }),
        IceGraphNodeType::GetArrayData { name, context } if at_samples => group(ui, "Attribute", None, |f| {
            f.text("Name", Some("What each sample reads of the face it is on, kept as a list: P its points' positions, ptIndices their numbers, uv its corners'; a face's own attribute, a list of one"), name, "P");
            f.choice("Context", Some("Which of the face's elements: its points, its corners, the face itself, the object. Auto: the first that has the attribute"),
                     context, &[(None, "Auto"), (Some(Context::Point), "Point"), (Some(Context::Face), "Face"), (Some(Context::Corner), "Corner"), (Some(Context::Object), "Object")]);
        }),
        IceGraphNodeType::GetArrayData { name, context } => group(ui, "Attribute", None, |f| {
            f.text("Name", Some("The attribute each element reads as a list, every value it touches kept: P from Point, a face's points' positions; ptIndices, a face's point numbers; faceIndices, a point's faces"), name, "ptIndices");
            f.choice("Context", Some("Where the values are read from. Auto: where the attribute is (ptIndices: Point, faceIndices: Face). A face reads its points or corners, a point the faces or corners around it, a corner its point or face (a list of one); in an Object graph, every one, as the graph's array"),
                     context, &[(None, "Auto"), (Some(Context::Point), "Point"), (Some(Context::Face), "Face"), (Some(Context::Corner), "Corner"), (Some(Context::Object), "Object")]);
        }),
        IceGraphNodeType::RandomOrientation { seed } => group(ui, "Random", None, |f| {
            f.row(
                "Seed",
                Some("Another seed, other turns; the same seed gives the same turns every time"),
                |ui| crate::properties::form::number(ui, None, seed, |d| d.speed(1)),
            );
        }),
        IceGraphNodeType::GetReference { input, paths } => {
            group(ui, "Reference", None, |f| {
                f.row(
                    "Input",
                    Some("Which of the operator's reference inputs: 1 is the first after the scene"),
                    |ui| {
                        crate::properties::form::number(ui, None, input, |d| d.range(1..=64).speed(0.05))
                    },
                );
            });
            // The prims on that input, to pick from, as in a Prims Location tab.
            let there = references
                .get((*input as usize).saturating_sub(1))
                .map(Vec::as_slice)
                .unwrap_or_default();
            crate::properties::ui::locations_tab(
                ui,
                paths,
                there,
                selected,
                crate::properties::ui::TabKind::Names,
                None,
            );
        }
        IceGraphNodeType::Instancer { path, seed, draw_mode } => group(ui, "Instancer", None, |f| {
            f.choice("Draw Mode", Some("How viewports draw the prototypes: as geometry, as their bounding boxes, or as axes at their origins. Written as model:drawMode on each prototype, so other USD viewers draw them so too; the Scene Explorer's right-click chooses for the Viewer alone"),
                     draw_mode, &[(crate::types::DrawMode::Default, "Geometry"), (crate::types::DrawMode::Bounds, "Bounding Box"), (crate::types::DrawMode::Origin, "Origin")]);
            f.text("Primitive Location", Some("Where the point instancer is made: a path (/root/instancer), or a name alone, beside the primitive the graph runs on (scatter on /set/ground is /set/scatter). Its prototypes go under it, in Prototypes"), path, "/root/instancer");
            f.row("Seed", Some("Which prototype each instance takes when Proto Indices is -1: another seed, other picks; the same seed, the same every time"), |ui| crate::properties::form::number(ui, None, seed, |d| d.speed(1)));
        }),
        IceGraphNodeType::Compose { op } => group(ui, "Compose", None, |f| {
            let choices: Vec<(xms_ice::Compose, &str)> =
                xms_ice::Compose::ALL.iter().map(|c| (*c, c.label())).collect();
            if f.choice("Makes", Some("What it makes, and from what: numbers into a vector, a vector and a number into a bigger one, numbers into a rotation"), op, &choices) {
                resync = true;
            }
        }),
        IceGraphNodeType::Decompose { op } => group(ui, "Decompose", None, |f| {
            let choices: Vec<(xms_ice::Decompose, &str)> =
                xms_ice::Decompose::ALL.iter().map(|d| (*d, d.label())).collect();
            if f.choice("From", Some("What it takes apart, and into what: a vector into its numbers, or into a smaller vector and a number; a rotation into its numbers, or its axis and angle (degrees)"), op, &choices) {
                resync = true;
            }
        }),
        IceGraphNodeType::Compound {
            definition,
            version,
            editable,
            values,
            ..
        } => {
            // `$OPNAME` in its definition's name: this compound's.
            let _operator = crate::core::vars::operator(&node.name);
            (enter, back) = crate::properties::ui::definition_rows(
                ui,
                definition,
                *version,
                editable,
                "its outputs give nothing",
                browser,
                crate::file_browser::BrowseTarget::SaveIceDefinition(graph_id, node.id),
            );
            crate::properties::ui::promoted_groups(ui, &compound_params, values);
        }
        IceGraphNodeType::CompoundInput { name, value } => group(
            ui,
            "Port",
            Some("Inside a compound: what comes into the instance's input port of this name. The compound's ports are named by these nodes, top to bottom"),
            |f| {
                crate::properties::form::unprobed(|| f.text("Name", Some("The input port's name on the compound"), name, "Value"));
                value_with_type(f, "Value", value, Some("What the port holds while nothing is wired to the instance's"));
            },
        ),
        IceGraphNodeType::CompoundOutput { name } => group(
            ui,
            "Port",
            Some("Inside a compound: what goes out of the instance's output port of this name. Wired from an Execute (Set Data), the port is an Execute"),
            |f| {
                crate::properties::form::unprobed(|| f.text("Name", Some("The output port's name on the compound"), name, "Value"));
            },
        ),
        IceGraphNodeType::Parameter { name, default } => group(
            ui,
            "Parameter",
            Some("A value its ICE operator gives, by name, in the operator's parameters: what a scene compound can promote"),
            |f| {
                crate::properties::form::unprobed(|| f.text("Name", Some("What the ICE operator's parameters call it"), name, "parameter"));
                value_with_type(f, "Default", default, Some("Its value while the operator gives none"));
            },
        ),
        IceGraphNodeType::Math { .. }
        | IceGraphNodeType::Array { .. }
        | IceGraphNodeType::Concat
        | IceGraphNodeType::ClosestPoint => {}
    }
    if let Some(probe) = crate::properties::form::probed() {
        let now = serde_json::to_value(&node.node_type).unwrap_or_default();
        let was = serde_json::to_value(&before.node_type).ok();
        // Not its definition's own rows (an editable compound's name).
        let path = crate::keys_ui::probed_path(&now, was.as_ref()).filter(|p| !p.starts_with("editable/"));
        *node = before;
        match probe {
            crate::properties::form::Probe::Clicked(_, at) => {
                if let Some(path) = path {
                    let m = RowMenu {
                        node: node.id,
                        what: Row::Setting(path),
                        at,
                    };
                    ui.data_mut(|d| d.insert_temp(row_menu_id(), m));
                }
            }
            crate::properties::form::Probe::Finding(row) => crate::properties::form::found(row, path),
        }
    }
    if enter || back {
        let id = node.id;
        if back {
            super::compound::refresh_ports(graph, id);
        }
        if enter {
            ui.data_mut(|d| d.insert_temp(enter_id(), id));
        }
        return;
    }
    // Compose's and Decompose's ports follow what they make or take apart:
    // made again, wires kept by place.
    if resync {
        let id = node.id;
        graph.resync_sockets(id);
        return;
    }
    // What each input holds while nothing is wired to it.
    let free: Vec<usize> = node
        .inputs
        .iter()
        .enumerate()
        .filter(|(_, i)| i.connected_output.is_none() && i.value.is_some())
        .map(|(k, _)| k)
        .collect();
    if !free.is_empty() {
        group(ui, "Inputs", Some("Used while nothing is wired in"), |f| {
            for k in free {
                let input = &mut node.inputs[k];
                let Some(value) = input.value.as_mut() else { continue };
                let top = f.ui.cursor().top();
                value_with_type(f, &input.name, value, None);
                // Right-clicked: its menu (Promote Input).
                let rows = egui::Rect::from_x_y_ranges(f.ui.max_rect().x_range(), top..=f.ui.cursor().top());
                let at = f.ui.input(|i| {
                    i.pointer
                        .secondary_clicked()
                        .then(|| i.pointer.interact_pos())
                        .flatten()
                });
                if let Some(at) = at.filter(|p| rows.contains(*p)) {
                    let m = RowMenu {
                        node: node.id,
                        what: Row::Input(k),
                        at,
                    };
                    f.ui.data_mut(|d| d.insert_temp(row_menu_id(), m));
                }
            }
        });
    }
}

/// A row's menu, opened by a right-click on it in the parameters: the node,
/// the input or the setting (by its path), where the pointer was.
#[derive(Clone)]
struct RowMenu {
    node: NodeId,
    what: Row,
    at: egui::Pos2,
}

#[derive(Clone)]
enum Row {
    Input(usize),
    Setting(String),
}

fn row_menu_id() -> egui::Id {
    egui::Id::new("ice_row_menu")
}

/// The menu of a row right-clicked in the parameters, while it is open: an
/// input's Promote Input, a setting's Promote or Remove Promotion. What it did
/// is told by the graph (`told_id`).
fn row_menu(ctx: &egui::Context, graph: &mut IceGraph) {
    let Some(m) = ctx.data(|d| d.get_temp::<RowMenu>(row_menu_id())) else {
        return;
    };
    let close = |ctx: &egui::Context| ctx.data_mut(|d| d.remove::<RowMenu>(row_menu_id()));
    let Some(node) = graph.nodes.iter().find(|n| n.id == m.node) else {
        close(ctx);
        return;
    };
    let title = match &m.what {
        Row::Input(k) => node.inputs.get(*k).map(|i| i.name.clone()),
        Row::Setting(path) => Some(format!("{} {}", node.name, crate::core::keys::label(path))),
    };
    let Some(title) = title else {
        close(ctx);
        return;
    };
    let promoted = match &m.what {
        Row::Setting(path) => graph
            .parameters
            .iter()
            .any(|p| p.targets.iter().any(|t| t.node == node.name && t.path == *path)),
        Row::Input(_) => false,
    };
    let node_name = node.name.clone();
    let editable = !locked(ctx, graph);
    let in_compound = !graph.outer.is_empty();
    let (mut promote, mut remove) = (false, false);
    let area = egui::Area::new(row_menu_id())
        .order(egui::Order::Foreground)
        .fixed_pos(m.at)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new(title).small().weak());
                match &m.what {
                    Row::Input(_) => promote = promote_button(ui, "Promote Input", editable, in_compound),
                    Row::Setting(_) if promoted => {
                        remove = ui
                            .add_enabled(editable, egui::Button::new("Remove Promotion"))
                            .on_hover_text("Its own value again, no longer the parameter's")
                            .clicked();
                    }
                    Row::Setting(_) => promote = promote_button(ui, "Promote", editable, in_compound),
                }
            });
        });
    let away = ctx.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer();
    let told = match (&m.what, promote, remove) {
        (Row::Input(k), true, _) => super::compound::promote_input(graph, m.node, *k),
        (Row::Setting(path), true, _) => {
            Some(super::compound::promote_setting(graph, m.node, path).unwrap_or_else(|e| e))
        }
        (Row::Setting(path), _, true) => {
            super::compound::remove_promotion(graph, &node_name, path);
            Some(format!(
                "{node_name} {} is its own again",
                crate::core::keys::label(path)
            ))
        }
        _ => None,
    };
    if let Some(told) = told {
        ctx.data_mut(|d| d.insert_temp(told_id(), told));
    }
    if promote || remove || away || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close(ctx);
    }
}

/// Promote Input's button: inside a compound, the input a port of it; else a
/// parameter of the ICE operator. Not where the graph cannot be changed.
fn promote_button(ui: &mut egui::Ui, label: &str, editable: bool, in_compound: bool) -> bool {
    let what = if in_compound {
        "A port of the compound, its value this input's: a Compound Input wired in"
    } else {
        "A parameter of the ICE operator, in its parameters, its value this input's: a Parameter node wired in"
    };
    ui.add_enabled(editable, egui::Button::new(label))
        .on_hover_text(what)
        .on_disabled_hover_text("Locked: it follows its definition. Make Editable to change it")
        .clicked()
}

/// A value's type (a choice; changed, the value made of the new type) and the
/// value.
fn value_with_type(f: &mut crate::properties::form::Rows, label: &str, value: &mut xms_ice::Value, hint: Option<&str>) {
    use xms_ice::Value;
    let mut kind = value.ty();
    if f.choice(
        &format!("{label} type"),
        hint,
        &mut kind,
        &[
            (xms_ice::Ty::Float, "Scalar"),
            (xms_ice::Ty::Int, "Integer"),
            (xms_ice::Ty::Vec2, "2D Vector"),
            (xms_ice::Ty::Vec3, "3D Vector"),
            (xms_ice::Ty::Vec4, "4D Vector"),
            (xms_ice::Ty::Quat, "Rotation"),
            (xms_ice::Ty::Mat4, "Matrix"),
            (xms_ice::Ty::Bool, "Boolean"),
        ],
    ) {
        *value = match kind {
            xms_ice::Ty::Int => Value::Int(0),
            xms_ice::Ty::Vec2 => value.convert(xms_ice::Ty::Vec2).unwrap_or(Value::Vec2([0.0; 2])),
            xms_ice::Ty::Vec3 => value.convert(xms_ice::Ty::Vec3).unwrap_or(Value::Vec3([0.0; 3])),
            xms_ice::Ty::Vec4 => value.convert(xms_ice::Ty::Vec4).unwrap_or(Value::Vec4([0.0; 4])),
            // No rotation: (0, 0, 0, 1).
            xms_ice::Ty::Quat => Value::Quat([0.0, 0.0, 0.0, 1.0]),
            xms_ice::Ty::Mat4 => super::identity(),
            xms_ice::Ty::Bool => Value::Bool(false),
            _ => value.convert(xms_ice::Ty::Float).unwrap_or(Value::Float(0.0)),
        };
    }
    value_row(f, label, value);
}

/// Where the graph broke the last time it ran, and why: the node in this
/// graph, when there is one (inside a compound looked at, the node inside it
/// that broke; outside it, the compound holding that node). The graph that
/// ran is the outermost, while a compound's inside is looked at.
fn broken(graph: &IceGraph) -> Option<(Option<NodeId>, String)> {
    let top = graph.outer.first().map_or(graph, |o| &*o.graph);
    let (_, message) = top.error.lock().ok()?.clone()?;
    let way = top.broke.lock().ok()?.clone();
    let entered: Vec<NodeId> = graph.outer.iter().map(|o| o.instance).collect();
    let here = way
        .starts_with(&entered)
        .then(|| way.get(entered.len()).copied())
        .flatten();
    Some((here, message))
}

fn scene_locked_id() -> egui::Id {
    egui::Id::new("ice_scene_locked")
}

/// Whether the scene graph is inside a locked compound, so its ICE graphs
/// cannot be changed: set once a frame.
pub fn set_scene_locked(ctx: &egui::Context, locked: bool) {
    ctx.data_mut(|d| d.insert_temp(scene_locked_id(), locked));
}

/// Whether the graph cannot be changed: inside a locked ICE compound, or in
/// a locked scene compound.
fn locked(ctx: &egui::Context, graph: &IceGraph) -> bool {
    ctx.data(|d| d.get_temp::<bool>(scene_locked_id())).unwrap_or(false) || graph.entered().1
}

/// What a node's menu has to tell, until the graph is drawn.
fn told_id() -> egui::Id {
    egui::Id::new("ice_told")
}

/// Where a compound to go inside of waits, until the graph is drawn.
fn enter_id() -> egui::Id {
    egui::Id::new("ice_compound_enter")
}

/// A row editing one value, by its type.
pub fn value_row(f: &mut crate::properties::form::Rows, label: &str, value: &mut xms_ice::Value) {
    use xms_ice::Value;
    match value {
        Value::Bool(b) => {
            f.toggle(label, None, b);
        }
        Value::Int(i) => {
            f.row(label, None, |ui| {
                crate::properties::form::number(ui, None, i, |d| d.speed(1))
            });
        }
        Value::Float(x) => {
            f.drag(label, None, x, 0.01, "");
        }
        Value::Vec2(v) => {
            f.uv(label, None, v, 0.01);
        }
        Value::Vec3(v) => {
            f.xyz(label, None, v, 0.01, "");
        }
        Value::Vec4(v) | Value::Quat(v) => {
            f.row(label, None, |ui| {
                for c in v.iter_mut() {
                    crate::properties::form::number(ui, None, c, |d| d.speed(0.01));
                }
            });
        }
        // Its axes and origin, a row each (its columns).
        Value::Mat4(m) => {
            for (k, row) in ["X Axis", "Y Axis", "Z Axis", "Origin"].iter().enumerate() {
                let mut v = [m[k * 4], m[k * 4 + 1], m[k * 4 + 2]];
                f.xyz(&format!("{label} {row}"), None, &mut v, 0.01, "");
                m[k * 4..k * 4 + 3].copy_from_slice(&v);
            }
        }
        // Made by Scatter on Surface, never typed in.
        Value::Sample(_) => {}
    }
}

// Main canvas

/// Draw an ICE tree and handle its editing and navigation, as the main network
/// does: Shift+drag or middle-drag pans, the wheel zooms toward the pointer, F
/// or A frames every node. A compound: double-click it, or select it and press
/// ↓, to go inside; ↑ comes back out. Returns whether the Up arrow asks to go
/// back up to the scene network, and what to tell.
pub fn draw_subnet_graph(ui: &mut egui::Ui, graph: &mut IceGraph) -> (bool, Option<String>) {
    let mut told = None;
    let (response, painter) = ui.allocate_painter(
        egui::Vec2::new(ui.available_width(), ui.available_height()),
        egui::Sense::click_and_drag(),
    );
    let canvas_rect = response.rect;
    // The types of the outputs, wired or not, for the ports and wires.
    graph.refresh_types();
    let mut collapsed: HashSet<NodeId> = ui.data(|d| d.get_temp(collapsed_id())).unwrap_or_default();
    let keys_free = response.hovered() && !ui.ctx().egui_wants_keyboard_input();

    // Up arrow: out of the compound looked at, else back up to the scene
    // network. Down arrow: inside the compound selected.
    if keys_free && ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
        if graph.outer.is_empty() {
            return (true, None);
        }
        return (false, super::compound::leave(graph));
    }
    let compound_selected = graph.selected_node.filter(|s| {
        graph
            .nodes
            .iter()
            .any(|n| n.id == *s && matches!(n.node_type, IceGraphNodeType::Compound { .. }))
    });
    if let Some(id) = compound_selected.filter(|_| keys_free && ui.input(|i| i.key_pressed(egui::Key::ArrowDown))) {
        ui.data_mut(|d| d.insert_temp(enter_id(), id));
    }

    // Delete or Backspace: the selected wire, else the selected nodes
    // Pointer anywhere over the canvas, nodes included, as in the main network.
    // Not with the pointer over a window above the graph (the file browser).
    let ptr_in_canvas = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| canvas_rect.contains(p) && ui.ctx().layer_id_at(p).is_none_or(|l| l == ui.layer_id()));
    if ptr_in_canvas
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
    {
        graph.delete_selected();
    }

    // Copy and paste (Ctrl+C, Ctrl+V; Cmd on macOS)
    // The selected nodes and the wires between them, pasted under the
    // pointer. egui gives Cmd+V as a Paste event (the clipboard's text),
    // not as a key.
    if ptr_in_canvas && !ui.ctx().egui_wants_keyboard_input() {
        let (copy, paste) = ui.input(|i| {
            let copy = i.events.iter().any(|e| matches!(e, egui::Event::Copy));
            let paste = i.events.iter().find_map(|e| match e {
                egui::Event::Paste(t) => Some(Some(t.clone())),
                egui::Event::Key {
                    key: egui::Key::V,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.command => Some(None),
                _ => None,
            });
            (copy, paste)
        });
        if copy {
            if let Some(text) = crate::graph_io::copy_ice(graph, &graph.selected_nodes) {
                ui.ctx().copy_text(text);
            }
        }
        if let Some(text) = paste {
            let at = ui.input(|i| i.pointer.hover_pos()).map(|p| {
                egui::pos2(
                    (p.x - canvas_rect.min.x - graph.pan_offset.x) / graph.zoom,
                    (p.y - canvas_rect.min.y - graph.pan_offset.y) / graph.zoom,
                )
            });
            crate::graph_io::paste_ice(graph, text.as_deref(), at);
        }
    }

    // Frame every node (F or A)
    if keys_free
        && ui.input(|i| i.key_pressed(egui::Key::F) || i.key_pressed(egui::Key::A))
        && canvas_rect.width() > 40.0
        && !graph.nodes.is_empty()
    {
        let mut bounds = egui::Rect::NOTHING;
        for node in &graph.nodes {
            let lay = layout(node, collapsed.contains(&node.id));
            bounds = bounds.union(egui::Rect::from_min_size(node.position, lay.size));
        }
        let room = canvas_rect.size() - egui::vec2(60.0, 60.0);
        graph.zoom = (room.x / bounds.width()).min(room.y / bounds.height()).clamp(0.3, 1.5);
        graph.pan_offset = canvas_rect.size() * 0.5 - bounds.center().to_vec2() * graph.zoom;
        ui.ctx().request_repaint();
    }

    // Zoom (wheel, toward the pointer)
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let cursor = ui.input(|i| i.pointer.hover_pos()).unwrap_or(canvas_rect.center());
            let (z0, pan0) = (graph.zoom, graph.pan_offset);
            let under = (cursor - canvas_rect.min - pan0) / z0; // canvas point under the pointer
            graph.zoom = (z0 * (1.0 + scroll * 0.002)).clamp(0.15, 4.0);
            graph.pan_offset = cursor - canvas_rect.min - under * graph.zoom;
        }
    }

    let pan = graph.pan_offset;
    let z = graph.zoom;
    let to_screen = |p: egui::Pos2| canvas_rect.min + p.to_vec2() * z + pan;
    let to_canvas = move |p: egui::Pos2| egui::Pos2::ZERO + (p - canvas_rect.min - pan) / z;

    // Light grey canvas with a fine grid that moves and scales with the view.
    painter.rect_filled(canvas_rect, 0.0, pal::BG());
    let step = GRID * z;
    if step >= 5.0 {
        let grid = egui::Stroke::new(1.0_f32, pal::GRID());
        let mut x = canvas_rect.min.x + pan.x.rem_euclid(step);
        while x < canvas_rect.max.x {
            painter.line_segment(
                [egui::pos2(x, canvas_rect.min.y), egui::pos2(x, canvas_rect.max.y)],
                grid,
            );
            x += step;
        }
        let mut y = canvas_rect.min.y + pan.y.rem_euclid(step);
        while y < canvas_rect.max.y {
            painter.line_segment(
                [egui::pos2(canvas_rect.min.x, y), egui::pos2(canvas_rect.max.x, y)],
                grid,
            );
            y += step;
        }
    }

    // Existing connections
    let mut hovered: Option<ConnectionId> = None;
    for conn in &graph.connections.clone() {
        if let (Some(fn_), Some(tn)) = (
            graph.nodes.iter().find(|n| n.id == conn.from_node),
            graph.nodes.iter().find(|n| n.id == conn.to_node),
        ) {
            let fp = to_screen(output_socket_pos(fn_, conn.from_output, &collapsed));
            let tp = to_screen(input_socket_pos(tn, conn.to_input, &collapsed));
            if let Some(ptr) = response.hover_pos() {
                if is_near_bezier_h(ptr, fp, tp, 10.0) {
                    hovered = Some(conn.id);
                }
            }
            let colour = out_colour(graph, fn_.id, conn.from_output);
            // Selected, or a wire of the port picked to unplug: a band in the
            // selection colour under it.
            let picked = graph
                .unplug
                .as_ref()
                .is_some_and(|p| p.holds(conn.from_node, &conn.from_output, conn.to_node, &conn.to_input));
            if graph.selected_wire == Some(conn.id) || picked {
                draw_selected_wire_h(&painter, fp, tp, z);
            }
            draw_wire_h(&painter, fp, tp, colour, hovered == Some(conn.id), z);
        }
    }
    // Click a wire: select it (Delete takes it off). Right-click: take it off.
    if let Some(cid) = hovered {
        if response.clicked() {
            graph.selected_wire = Some(cid);
            graph.selected_node = None;
            graph.selected_nodes.clear();
            graph.unplug = None;
        }
        if response.secondary_clicked() {
            graph.remove_connection(cid);
        }
    }

    // In-progress wire — use raw pointer pos, not response.hover_pos()
    if let Some((fid, fo)) = graph.connecting_from {
        if let Some(fn_) = graph.nodes.iter().find(|n| n.id == fid) {
            let fp = to_screen(output_socket_pos(fn_, fo, &collapsed));
            let ptr = ui.input(|i| i.pointer.hover_pos());
            let colour = out_colour(graph, fid, fo);
            if let Some(ptr) = ptr {
                draw_wire_h(&painter, fp, ptr, colour, false, z);
            }
        }
    }

    // Wires picked up off an output: from the pointer to their inputs.
    if let Some(ptr) = ui.input(|i| i.pointer.hover_pos()) {
        for (to, input) in &graph.carrying {
            if let Some(tn) = graph.nodes.iter().find(|n| n.id == *to) {
                draw_selected_wire_h(&painter, ptr, to_screen(input_socket_pos(tn, *input, &collapsed)), z);
            }
        }
    }

    // Cancel on Escape
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        graph.connecting_from = None;
        graph.unplug = None;
    }

    // Draw all nodes — sockets handle their own press/release via raw input
    let nodes_clone = graph.nodes.clone();
    for node in &nodes_clone {
        draw_subnet_node(ui, &painter, graph, node, &to_screen, &to_canvas, z, &mut collapsed);
    }
    // Where the graph broke, the last time it ran: a red ring on the node,
    // and why, along the top of the canvas.
    if let Some((here, message)) = broken(graph) {
        if let Some((id, node)) = here.and_then(|id| graph.nodes.iter().find(|n| n.id == id).map(|n| (id, n))) {
            let size = layout(node, collapsed.contains(&id)).size * z;
            let r = egui::Rect::from_min_size(to_screen(node.position), size).expand(3.0 * z);
            painter.rect_stroke(
                r,
                ROUNDING * z,
                egui::Stroke::new(2.0_f32, pal::ERROR()),
                egui::StrokeKind::Outside,
            );
        }
        painter.text(
            canvas_rect.left_top() + egui::vec2(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            message,
            egui::FontId::proportional(12.0),
            pal::ERROR(),
        );
    }
    ui.data_mut(|d| d.insert_temp(collapsed_id(), collapsed));
    // Locked: a dark layer, and a change put back.
    if locked(ui.ctx(), graph) {
        painter.rect_filled(canvas_rect, 0.0, egui::Color32::from_black_alpha(90));
        painter.text(
            canvas_rect.left_bottom() + egui::vec2(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            "Locked: it follows its definition. Make Editable to change it",
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
    }

    // Cancel wire only if mouse released and no socket consumed it
    // (drag_stopped on the background canvas means we missed all sockets)
    if graph.connecting_from.is_some() && response.drag_stopped() {
        graph.connecting_from = None;
    }
    // The button let go of after a wire was dragged, over no port that took
    // it (the ports, drawn above, take it first): it is gone. Wires picked
    // up and let go of over the canvas are so unplugged.
    if ui.input(|i| i.pointer.primary_released()) {
        if graph.wire_dragged {
            graph.connecting_from = None;
        }
        graph.wire_dragged = false;
        graph.carrying.clear();
    }

    // Pan with Shift+drag or MMB
    let is_mmb = ui.input(|i| i.pointer.middle_down());
    let panning = ui.input(|i| i.modifiers.shift) || is_mmb;
    if response.dragged() && panning {
        graph.pan_offset += response.drag_delta();
    }

    // Box select: drag on the empty canvas
    let busy = panning || graph.connecting_from.is_some() || graph.dragging_node.is_some();
    if response.drag_started_by(egui::PointerButton::Primary) && !busy {
        graph.marquee = response.interact_pointer_pos().map(to_canvas);
    }
    if let (Some(start), true) = (graph.marquee, response.dragged() && !busy) {
        if let Some(cur) = response.interact_pointer_pos().map(to_canvas) {
            let r = egui::Rect::from_two_pos(start, cur);
            let folded: HashSet<NodeId> = ui.data(|d| d.get_temp(collapsed_id())).unwrap_or_default();
            graph.selected_nodes = graph
                .nodes
                .iter()
                .filter(|n| {
                    r.intersects(egui::Rect::from_min_size(
                        n.position,
                        layout(n, folded.contains(&n.id)).size,
                    ))
                })
                .map(|n| n.id)
                .collect();
            graph.selected_node = graph.selected_nodes.first().copied();
            graph.selected_wire = None;
            crate::graph_ports::draw_marquee(
                &painter,
                egui::Rect::from_two_pos(to_screen(start), to_screen(cur)),
                true,
            );
        }
    }
    if response.drag_stopped() {
        graph.marquee = None;
    }

    // A click on the empty canvas: a picked port's wires come off, a wire
    // being drawn is let go of, nothing is selected.
    if response.clicked() && hovered.is_none() {
        if let Some(picked) = graph.unplug.take() {
            graph.unplug(&picked);
        }
        graph.connecting_from = None;
        graph.selected_node = None;
        graph.selected_nodes.clear();
        graph.selected_wire = None;
    }

    // Add a node: Tab or right-click on the canvas, as in every graph
    // A menu with a search line: ↓ and ↑ through what is found, Enter adds.
    let menu_id = egui::Id::new(("ice_add", graph.id.0));
    let mut menu_at: Option<egui::Pos2> = ui.data(|d| d.get_temp::<Option<egui::Pos2>>(menu_id)).flatten();
    let mut opened_now = false;
    if response.hovered()
        && hovered.is_none()
        && (response.secondary_clicked() || ui.input(|i| i.key_pressed(egui::Key::Tab)))
    {
        menu_at = ui.input(|i| i.pointer.hover_pos());
        opened_now = true;
    }
    if let Some(at) = menu_at {
        let mut close = false;
        let area = egui::Area::new(menu_id.with("area"))
            .fixed_pos(at)
            .order(egui::Order::Foreground)
            .constrain(true)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(190.0);
                    match add_menu(ui, opened_now) {
                        Some(Added::Node(t)) => {
                            let name = match &t {
                                IceGraphNodeType::Compound { definition, .. } => definition.clone(),
                                t => crate::types::subnet_node_label(t).to_string(),
                            };
                            let id = graph.add_node(name, t, to_canvas(at));
                            graph.selected_node = Some(id);
                            close = true;
                        }
                        Some(Added::MakeCompound) => {
                            let mut ids = graph.selected_nodes.clone();
                            ids.extend(graph.selected_node);
                            told = Some(match super::compound::make_compound(graph, &ids, "Compound") {
                                Ok(_) => "Made a compound: double-click it, or ↓, to go inside".into(),
                                Err(e) => e,
                            });
                            close = true;
                        }
                        None => {}
                    }
                });
            });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if !opened_now
            && ui.input(|i| i.pointer.any_click())
            && !area.response.contains_pointer()
            && !ui.ctx().any_popup_open()
        {
            close = true;
        }
        if close {
            menu_at = None;
        }
    }
    ui.data_mut(|d| d.insert_temp(menu_id, menu_at));
    if let Some(m) = ui.data(|d| d.get_temp::<String>(told_id())) {
        ui.data_mut(|d| d.remove::<String>(told_id()));
        told = Some(m);
    }
    // A compound double-clicked, or ↓: inside it.
    if let Some(id) = ui.data(|d| d.get_temp::<NodeId>(enter_id())) {
        ui.data_mut(|d| d.remove::<NodeId>(enter_id()));
        told = Some(super::compound::enter(graph, id).unwrap_or_else(|e| e));
    }
    if super::compound::keep_locked(graph) {
        told = Some("Locked: it follows its definition. Make Editable to change it here".into());
    }
    (false, told)
}

/// Every node the add menu offers: its category, the node as it is made, and a
/// line on what it does.
fn catalog() -> Vec<(&'static str, IceGraphNodeType, &'static str)> {
    use xms_ice::{Math, Value};
    let mut out = vec![
        ("Data Access", IceGraphNodeType::get("P"), "Read an attribute of the geometry: P (the points) or any other by name, in the graph's context or from another (a face reading its points). Wire surface samples into its Samples (Scatter on Surface's, Closest Point on Surface's) to read it there instead, blended across the surface, in the graph's space"),
        ("Data Access", IceGraphNodeType::set("P"), "Write an attribute; its Execute goes into the Execution Ports"),
        ("Constants", IceGraphNodeType::Constant { value: Value::Float(0.0) }, "A number"),
        ("Constants", IceGraphNodeType::Constant { value: Value::Int(0) }, "A whole number"),
        ("Constants", IceGraphNodeType::Constant { value: Value::Vec2([0.0; 2]) }, "Two numbers"),
        ("Constants", IceGraphNodeType::Constant { value: Value::Vec3([0.0; 3]) }, "Three numbers: a position, a direction, a colour"),
        ("Constants", IceGraphNodeType::Constant { value: Value::Vec4([0.0; 4]) }, "Four numbers"),
        ("Constants", IceGraphNodeType::Constant { value: Value::Bool(false) }, "True or false"),
        ("Constants", IceGraphNodeType::Constant { value: Value::Quat([0.0, 0.0, 0.0, 1.0]) }, "A rotation (i, j, k, real); none by default"),
        ("Constants", IceGraphNodeType::Constant { value: super::identity() }, "A matrix: three axes and an origin; the identity by default"),
        ("Random", IceGraphNodeType::Random { seed: 0 }, "A random value between Min and Max, one per element; with a Count, an array of that many. Wire a Seed (ptIndex, an id, a face's index) to say what each is drawn for"),
        ("Geometry", IceGraphNodeType::Scatter { seed: 0, weight: String::new() }, "Places on the surface, spread evenly by area (or by a weight attribute), in an Object graph: their positions, the normals there and the surface samples, each an array"),
        ("Data Access", IceGraphNodeType::GetArrayData { name: "ptIndices".into(), context: None }, "Read an attribute as a list per element, every value it touches kept: a face's point numbers (ptIndices), its points' positions (P from Point), a point's faces (faceIndices). Array Size, Select in Array, Sum and Average make one value of each; maths works item by item. Wire surface samples into its Samples to read, at each, the values of the face it is on (its points' P, ptIndices, its corners' uv)"),
        ("Geometry", IceGraphNodeType::ClosestPoint, "The nearest places on reference meshes (Geometry: Get Reference) to each Origin (P, per point), within a Radius (0: anywhere): surface samples to read with Get Data at Samples, and how far each is. For deformers: shrink-wrap, stick to a body, blend by distance"),
        ("Random", IceGraphNodeType::RandomOrientation { seed: 0 }, "Random rotations up along the normals given and turned at random: one per point or face (an orient attribute), or an array of them from Scatter's normals (an Instancer's orientations). Wire a Seed to say what each is drawn for"),
        ("Data Access", IceGraphNodeType::GetReference { input: 1, paths: vec![] }, "Prims on one of the operator's reference inputs, by path (an array): all, or those its rows name, picked from the scene wired there"),
        ("Geometry", IceGraphNodeType::Instancer { path: "/root/instancer".into(), seed: 0, draw_mode: crate::types::DrawMode::Default }, "A point instancer, as USD's: prototypes (paths) at positions, with proto indices, orientations and scales, at the Primitive Location you choose; an Execute"),
    ];
    out.push(("Math", IceGraphNodeType::Compose { op: xms_ice::Compose::Vec3 }, "A vector from its parts: numbers into a 2D, 3D or 4D vector, a vector and a number into a bigger one, numbers into a rotation. Choose in its parameters"));
    out.push(("Math", IceGraphNodeType::Decompose { op: xms_ice::Decompose::Vec3 }, "A vector taken apart, an output per part: into its numbers, or a smaller vector and a number; a rotation into its numbers, or its axis and angle. Choose in its parameters"));
    for op in xms_ice::ArrayOp::ALL {
        out.push(("Arrays", IceGraphNodeType::Array { op }, ""));
    }
    out.push(("Compounds", IceGraphNodeType::CompoundInput { name: "Value".into(), value: Value::Float(0.0) }, "Inside a compound: what comes into its input port of this name, its value while nothing is wired to the instance's"));
    out.push((
        "Compounds",
        IceGraphNodeType::CompoundOutput { name: "Value".into() },
        "Inside a compound: what goes out of its output port of this name",
    ));
    out.push(("Data Access", IceGraphNodeType::Parameter { name: "parameter".into(), default: Value::Float(0.0) }, "A value the ICE operator gives in its parameters, by name; its default until set. A scene compound can promote it"));
    out.push(("Arrays", IceGraphNodeType::Concat, "Arrays one after the other, all of one type (paths of two Get References, positions of two scatters); single values join as one element. Wiring the last input adds another"));
    out.push(("Matrix", IceGraphNodeType::Compose { op: xms_ice::Compose::Matrix }, "A matrix from three axes and an origin: a frame (tangent, normal, bitangent and a position). Its columns are the axes and the origin"));
    out.push((
        "Matrix",
        IceGraphNodeType::Decompose {
            op: xms_ice::Decompose::Matrix,
        },
        "A matrix taken apart into its three axes and its origin",
    ));
    for op in Math::ALL {
        let about = match op {
            Math::Distance => "How far apart two positions are",
            Math::Invert => "A matrix's inverse: from its frame back to where it is from",
            Math::Transpose => "A matrix's transpose: the inverse of a frame's turn (its axes unit and at right angles), cheaper than inverting",
            Math::TransformPoint => "A position through a matrix: turned, scaled and moved (into the frame's parent space)",
            Math::TransformVector => "A direction through a matrix: turned and scaled, not moved",
            Math::Multiply => "Multiply numbers or vectors; scale a vector; combine two rotations, or two matrices (the right one applies first)",
            _ => "",
        };
        let category = if matches!(
            op,
            Math::Invert | Math::Transpose | Math::TransformPoint | Math::TransformVector
        ) {
            "Matrix"
        } else {
            "Math"
        };
        out.push((category, IceGraphNodeType::Math { op }, about));
    }
    out
}

/// The add menu: a search line, then the nodes by category. The node chosen, if
/// one was.
/// What the add menu adds: a node, or the selected nodes made a compound.
#[derive(Clone)]
enum Added {
    Node(IceGraphNodeType),
    MakeCompound,
}

/// An entry of the add menu: its category, its label, a line on what it does.
struct Entry {
    category: String,
    label: String,
    hint: String,
    added: Added,
}

/// Every entry of the add menu: the nodes, Make Compound, and the library's
/// ICE compounds.
fn entries() -> Vec<Entry> {
    let mut out: Vec<Entry> = catalog()
        .into_iter()
        .map(|(category, t, hint)| Entry {
            category: category.into(),
            label: format!(
                "{}  {}",
                crate::types::subnet_node_icon(&t),
                crate::types::subnet_node_label(&t)
            ),
            hint: hint.into(),
            added: Added::Node(t),
        })
        .collect();
    out.push(Entry {
        category: "Compounds".into(),
        label: "◆  Make Compound".into(),
        hint: "The selected nodes as one compound, the wires crossing the selection its ports: double-click it, or ↓, to go inside".into(),
        added: Added::MakeCompound,
    });
    for f in crate::compounds::library().of_context(crate::compounds::Context::Ice) {
        let d = &f.definition;
        let names = |ports: &[crate::compounds::Port]| ports.iter().map(|p| p.name.clone()).collect();
        out.push(Entry {
            category: "Compounds".into(),
            label: format!("{}  {}", if d.icon.is_empty() { "◆" } else { d.icon.as_str() }, d.name),
            hint: d.help.clone(),
            added: Added::Node(IceGraphNodeType::Compound {
                definition: d.name.clone(),
                version: d.version,
                hash: d.hash(),
                inputs: names(&d.inputs),
                outputs: names(&d.outputs),
                values: Default::default(),
                editable: None,
            }),
        });
    }
    out
}

fn add_menu(ui: &mut egui::Ui, opened_now: bool) -> Option<Added> {
    let search_id = egui::Id::new("ice_add_search");
    let mut search: String = if opened_now {
        String::new()
    } else {
        ui.data(|d| d.get_temp(search_id)).unwrap_or_default()
    };
    let field = ui.add(
        egui::TextEdit::singleline(&mut search)
            .hint_text("Search nodes")
            .desired_width(180.0),
    );
    if opened_now {
        field.request_focus();
    }
    ui.data_mut(|d| d.insert_temp(search_id, search.clone()));
    ui.separator();
    let all = entries();
    let words = search.trim().to_lowercase();
    let mut chosen = None;
    if !words.is_empty() {
        let found: Vec<&Entry> = all
            .iter()
            .filter(|e| {
                let text = format!("{} {} {}", e.label, e.category, e.hint).to_lowercase();
                words.split_whitespace().all(|w| text.contains(w))
            })
            .collect();
        let (hi, moved, enter) =
            crate::menus::search_keys(ui, egui::Id::new("ice_add_pick"), &words, found.len(), opened_now);
        for (k, e) in found.iter().enumerate() {
            let mut r = ui.selectable_label(k == hi, format!("{}   {}", e.label, e.category));
            if !e.hint.is_empty() {
                r = r.on_hover_text(e.hint.as_str());
            }
            if moved && k == hi {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                chosen = Some(e.added.clone());
            }
        }
        if found.is_empty() {
            ui.label("No node by that name");
        }
        if enter {
            chosen = found.get(hi).map(|e| e.added.clone());
        }
        return chosen;
    }
    // Categories, each a submenu, as in the scene graph's menu.
    let mut categories: Vec<&str> = vec![];
    for e in &all {
        if !categories.contains(&e.category.as_str()) {
            categories.push(&e.category);
        }
    }
    for category in categories {
        crate::menus::menu(ui, category, |ui| {
            for e in all.iter().filter(|e| e.category == category) {
                let mut r = ui.button(e.label.as_str());
                if !e.hint.is_empty() {
                    r = r.on_hover_text(e.hint.as_str());
                }
                if r.clicked() {
                    chosen = Some(e.added.clone());
                    ui.close();
                }
            }
        });
    }
    chosen
}

/// A type's colour on ports and wires; Execute and what is not known yet are
/// dark grey.
fn ty_colour(t: Option<xms_ice::Ty>) -> egui::Color32 {
    use crate::type_colours::DataType;
    use xms_ice::Ty;
    match t {
        Some(Ty::Float) => DataType::Float,
        Some(Ty::Int) => DataType::Integer,
        Some(Ty::Bool) => DataType::Boolean,
        Some(Ty::Vec2 | Ty::Vec3) => DataType::Vector,
        // Four numbers, and a rotation (a quaternion).
        Some(Ty::Vec4 | Ty::Quat) => DataType::Vector4,
        Some(Ty::Mat4) => DataType::Matrix,
        Some(Ty::Sample | Ty::Token) => DataType::Other,
        None => DataType::Other,
    }
    .colour()
}

/// An output's colour: the type it gives, as the graph's last run found it; an
/// array's own colour whatever it holds (its port says what).
fn out_colour(graph: &IceGraph, node: NodeId, output: usize) -> egui::Color32 {
    if graph.output_is_array(node, output) {
        return pal::ARRAY();
    }
    ty_colour(graph.output_type(node, output))
}

/// What an output gives, for its port's tooltip: "3D vector", "array of surface
/// samples"…
fn out_text(graph: &IceGraph, node: NodeId, output: usize) -> String {
    let Some(ty) = graph.output_type(node, output) else {
        let execute = graph
            .nodes
            .iter()
            .find(|n| n.id == node)
            .and_then(|n| n.outputs.get(output))
            .is_some_and(|o| o.value_hint == "Execute");
        return if execute {
            "execute".into()
        } else {
            "not known yet (the graph has not run)".into()
        };
    };
    if graph.output_is_array(node, output) {
        format!("array of {}s", ty.name())
    } else {
        ty.name().into()
    }
}

/// What an input takes, for its port's tooltip: what is wired into it, or the
/// type of its own value.
fn in_text(graph: &IceGraph, input: &super::IceGraphInputSocket) -> String {
    if input.value_hint == "Execute" {
        return "execute".into();
    }
    match (input.connected_output, input.value) {
        (Some((from, out)), _) => out_text(graph, from, out),
        (None, Some(v)) => format!("{} (its own value)", v.ty().name()),
        (None, None) => "nothing wired".into(),
    }
}

/// An input's colour: what is wired into it, or the value it holds.
fn in_colour(graph: &IceGraph, input: &super::IceGraphInputSocket) -> egui::Color32 {
    if input.value_hint == "Execute" {
        return pal::OTHER();
    }
    match input.connected_output {
        Some((from, out)) => out_colour(graph, from, out),
        None => ty_colour(input.value.map(|v| v.ty())),
    }
}

// Single node

#[allow(clippy::too_many_arguments)]
fn draw_subnet_node(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    graph: &mut IceGraph,
    node: &IceGraphNode,
    to_screen: &impl Fn(egui::Pos2) -> egui::Pos2,
    to_canvas: &impl Fn(egui::Pos2) -> egui::Pos2,
    z: f32,
    collapsed: &mut HashSet<NodeId>,
) {
    let id = node.id;
    let folded = collapsed.contains(&id);
    let np = to_screen(node.position);
    let lay = layout(node, folded);
    let rect = egui::Rect::from_min_size(np, lay.size * z);
    let font = |pt: f32| egui::FontId::proportional(pt * z);
    let is_sel = graph.selected_node == Some(id) || graph.selected_nodes.contains(&id);
    let body = tint(&node.node_type);
    let (ink, ink_type) = inks(body);

    // A grey ring, the body, a dark edge; the selection blue around it when
    // selected, wide enough to see at any zoom.
    let w = crate::node_graph::ui::ring_width(z, is_sel);
    let ring = rect.expand(w);
    painter.rect_filled(
        ring,
        if ROUNDING > 0.0 { ROUNDING * z + w } else { 0.0 },
        if is_sel { pal::SELECTED() } else { pal::FRAME() },
    );
    painter.rect_filled(rect, ROUNDING * z, body);
    painter.rect_stroke(
        rect,
        ROUNDING * z,
        egui::Stroke::new(1.0_f32, pal::RIM()),
        egui::StrokeKind::Middle,
    );

    // Two-line title: the node's own name in bold (drawn twice, a hair
    // apart), its type in small dim text beneath. Collapsed, only the name.
    let name_y = if folded {
        np.y + FOLDED_H * 0.5 * z
    } else {
        np.y + 12.0 * z
    };
    for dx in [0.0, 0.6] {
        painter.text(
            egui::pos2(np.x + (7.0 + dx) * z, name_y),
            egui::Align2::LEFT_CENTER,
            &node.name,
            font(TITLE_PT),
            ink,
        );
    }
    if !folded {
        painter.text(
            egui::pos2(np.x + 7.0 * z, np.y + 25.0 * z),
            egui::Align2::LEFT_CENTER,
            title_text(node),
            font(TYPE_PT),
            ink_type,
        );
        // A thin line between the title and the ports, short of the edges.
        let y = np.y + TITLE_H * z - 0.5;
        painter.line_segment(
            [egui::pos2(rect.min.x + 4.0 * z, y), egui::pos2(rect.max.x - 4.0 * z, y)],
            egui::Stroke::new(1.0_f32, pal::DIVIDER()),
        );
    }

    // A constant's value, under the title.
    if let (false, Some(v)) = (folded, value_text(node)) {
        painter.text(
            egui::pos2(np.x + 7.0 * z, np.y + (TITLE_H + ROW_H * 0.5) * z),
            egui::Align2::LEFT_CENTER,
            v,
            egui::FontId::monospace(LABEL_PT * z),
            ink,
        );
    }

    // The collapse box, at the right of the title.
    let fold_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.max.x - 11.0 * z,
            np.y + (if folded { FOLDED_H * 0.5 } else { 12.0 }) * z,
        ),
        egui::vec2(11.0, 9.0) * z,
    );
    draw_fold_box(painter, fold_rect);

    // IMPORTANT: title bar allocated FIRST → lowest hit-test priority
    // The whole node, so a click or drag on its body selects or moves it
    // rather than starting a box on the canvas.
    let dr = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    let fold = ui
        .allocate_rect(fold_rect.expand(2.0 * z), egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if folded { "Expand" } else { "Collapse" });
    if fold.clicked() {
        if !collapsed.remove(&id) {
            collapsed.insert(id);
        }
    }

    // Output ports (right edge) — allocated before inputs, highest priority
    for (i, out) in node.outputs.iter().enumerate() {
        let sp = np + lay.outputs[i] * z;
        let hit = egui::Rect::from_center_size(sp, egui::Vec2::splat((SOCK_HIT * z).max(10.0)));
        let sr = ui
            .allocate_rect(hit, egui::Sense::click_and_drag())
            .on_hover_text(out_text(graph, id, i));
        let wiring = graph.connecting_from == Some((id, i));
        draw_port(painter, sp, out_colour(graph, id, i), sr.hovered() || wiring, z);
        if !folded {
            painter.text(
                egui::pos2(sp.x - 7.0 * z, sp.y),
                egui::Align2::RIGHT_CENTER,
                &out.name,
                font(LABEL_PT),
                ink,
            );
        }

        // Fire on the very first frame of press
        if sr.drag_started() || (sr.is_pointer_button_down_on() && graph.connecting_from.is_none()) {
            graph.connecting_from = Some((id, i));
        }
        if sr.drag_started_by(egui::PointerButton::Primary) {
            graph.wire_dragged = true;
            graph.unplug = None;
            // Ctrl+drag (Cmd on macOS): its wires picked up, to go to
            // another output or, let go of over the canvas, nowhere.
            if ui.input(|i| i.modifiers.command) {
                let off: Vec<(ConnectionId, NodeId, usize)> = graph
                    .connections
                    .iter()
                    .filter(|c| c.from_node == id && c.from_output == i)
                    .map(|c| (c.id, c.to_node, c.to_input))
                    .collect();
                if !off.is_empty() {
                    graph.connecting_from = None;
                    graph.carrying = off.iter().map(|(_, n, k)| (*n, *k)).collect();
                    for (cid, _, _) in off {
                        graph.remove_connection(cid);
                    }
                }
            }
        }
        // Wires picked up off another output, let go of here: they come
        // from this output now (not into its own node).
        if sr.contains_pointer() && ui.input(|s| s.pointer.primary_released()) && !graph.carrying.is_empty() {
            for (to, input) in std::mem::take(&mut graph.carrying) {
                if to != id {
                    graph.add_connection(id, i, to, input);
                }
            }
        }
        // Clicked, with wires out of it: picked, to unplug them with a click
        // on the canvas (or still to wire, with a click on an input).
        if sr.clicked() {
            let wired = graph
                .connections
                .iter()
                .any(|c| c.from_node == id && c.from_output == i);
            graph.unplug = wired.then_some(crate::graph_ports::Picked::Output(id, i));
        }
        if graph.unplug == Some(crate::graph_ports::Picked::Output(id, i)) {
            crate::graph_ports::draw_picked(painter, sp, PORT_R * z.max(0.6));
        }
    }

    // Input ports (left edge)
    for (i, inp) in node.inputs.iter().enumerate() {
        let sp = np + lay.inputs[i] * z;
        let hit = egui::Rect::from_center_size(sp, egui::Vec2::splat((SOCK_HIT * z).max(10.0)));
        let sr = ui
            .allocate_rect(hit, egui::Sense::click_and_drag())
            .on_hover_text(in_text(graph, inp));
        // Hovered while a wire is being drawn: a target.
        let target = sr.hovered() && graph.connecting_from.is_some();
        draw_port(painter, sp, in_colour(graph, inp), target, z);
        if !folded {
            painter.text(
                egui::pos2(sp.x + 7.0 * z, sp.y),
                egui::Align2::LEFT_CENTER,
                &inp.name,
                font(LABEL_PT),
                ink,
            );
        }

        // Complete wire: mouse released while this socket is hovered.
        // Use raw input so the canvas response can't steal the release.
        // Dragged with a wire in it: the wire picked up, its end following
        // the pointer, to go into another input or, let go of over the
        // canvas, nowhere.
        if sr.drag_started_by(egui::PointerButton::Primary) && graph.connecting_from.is_none() {
            if let Some(src) = inp.connected_output {
                let cid = graph
                    .connections
                    .iter()
                    .find(|c| c.to_node == id && c.to_input == i)
                    .map(|c| c.id);
                if let Some(cid) = cid {
                    graph.remove_connection(cid);
                }
                graph.connecting_from = Some(src);
                graph.wire_dragged = true;
                graph.unplug = None;
            }
        }
        if sr.contains_pointer() && ui.input(|s| s.pointer.primary_released()) {
            if let Some((fn_, fo)) = graph.connecting_from {
                if fn_ != id {
                    graph.add_connection(fn_, fo, id, i);
                }
                graph.connecting_from = None;
                graph.wire_dragged = false;
                graph.unplug = None;
            } else if sr.clicked() && inp.connected_output.is_some() {
                // Clicked, with a wire into it: picked, to unplug it with a
                // click on the canvas.
                graph.unplug = Some(crate::graph_ports::Picked::Input(id, i));
            }
        }
        if graph.unplug == Some(crate::graph_ports::Picked::Input(id, i)) {
            crate::graph_ports::draw_picked(painter, sp, PORT_R * z.max(0.6));
        }
    }

    // Title bar interactions (lowest priority — allocated first)
    // A click selects the node; Shift+click adds it to the selection.
    if dr.clicked() {
        graph.unplug = None;
        graph.selected_wire = None;
        if ui.input(|i| i.modifiers.shift) {
            if let Some(k) = graph.selected_nodes.iter().position(|n| *n == id) {
                graph.selected_nodes.remove(k);
            } else {
                // The node selected before joins too.
                if let Some(s) = graph.selected_node {
                    if !graph.selected_nodes.contains(&s) {
                        graph.selected_nodes.push(s);
                    }
                }
                graph.selected_nodes.push(id);
            }
        } else {
            graph.selected_nodes = vec![id];
        }
        graph.selected_node = Some(id);
    }
    // A compound: double-click to go inside (after the nodes are drawn).
    if dr.double_clicked() && matches!(node.node_type, IceGraphNodeType::Compound { .. }) {
        ui.data_mut(|d| d.insert_temp(enter_id(), id));
    }
    let terminal = matches!(node.node_type, IceGraphNodeType::Root { .. });
    if !terminal {
        // An input that holds a value can be promoted: a port of the compound
        // it is in, else a parameter of the ICE operator.
        let editable = !locked(ui.ctx(), graph);
        let in_compound = !graph.outer.is_empty();
        let promotable: Vec<(usize, String)> = node
            .inputs
            .iter()
            .enumerate()
            .filter(|(_, i)| i.connected_output.is_none() && i.value.is_some())
            .map(|(k, i)| (k, i.name.clone()))
            .collect();
        dr.context_menu(|ui| {
            if !promotable.is_empty() {
                if editable {
                    crate::menus::menu(ui, "Promote Input", |ui| {
                        for (k, name) in &promotable {
                            if promote_button(ui, name, true, in_compound) {
                                if let Some(told) = super::compound::promote_input(graph, id, *k) {
                                    ui.data_mut(|d| d.insert_temp(told_id(), told));
                                }
                                ui.close();
                            }
                        }
                    });
                } else {
                    promote_button(ui, "Promote Input", false, in_compound);
                }
                ui.separator();
            }
            if ui.button("Delete").clicked() {
                graph.remove_node(id);
                ui.close();
            }
        });
    }
    // Only drag the node if we are not mid-wire. A node of the selection
    // moves the whole selection; another is selected and moved alone.
    if dr.drag_started() && graph.connecting_from.is_none() {
        graph.dragging_node = Some(id);
        graph.drag_offset = dr.interact_pointer_pos().unwrap_or_default() - np;
        if !graph.selected_nodes.contains(&id) {
            graph.selected_nodes = vec![id];
        }
        graph.selected_node = Some(id);
        graph.selected_wire = None;
    }
    if dr.dragged() && graph.dragging_node == Some(id) {
        if let Some(ptr) = dr.interact_pointer_pos() {
            let delta = to_canvas(ptr - graph.drag_offset) - node.position;
            let moving = graph.selected_nodes.clone();
            for n in graph.nodes.iter_mut().filter(|n| moving.contains(&n.id) || n.id == id) {
                n.position += delta;
            }
        }
    }
    if dr.drag_stopped() {
        graph.dragging_node = None;
    }
}

/// The band under a selected wire: the same curve as `draw_wire_h`, wide, in
/// the selection colour.
pub(crate) fn draw_selected_wire_h(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, z: f32) {
    let off = (to.x - from.x).abs().max(60.0 * z) * 0.5;
    let (c1, c2) = (egui::pos2(from.x + off, from.y), egui::pos2(to.x - off, to.y));
    let pts: Vec<egui::Pos2> = (0..=24).map(|i| bezier(from, c1, c2, to, i as f32 / 24.0)).collect();
    painter.add(egui::Shape::line(
        pts,
        egui::Stroke::new(6.0 * z.clamp(0.7, 1.5), crate::theme::selection()),
    ));
}

/// A port: a small dot in its type's colour on the node's edge, larger under
/// the pointer.
pub(crate) fn draw_port(painter: &egui::Painter, at: egui::Pos2, colour: egui::Color32, hot: bool, z: f32) {
    let r = (if hot { PORT_R + 1.5 } else { PORT_R }) * z.max(0.6);
    painter.circle_filled(at, r + 1.0, pal::PORT_RIM());
    painter.circle_filled(at, r, colour);
}

/// The collapse box: a small grey box with three lines in it.
pub(crate) fn draw_fold_box(painter: &egui::Painter, r: egui::Rect) {
    painter.rect_filled(r, 1.5, egui::Color32::from_rgb(200, 200, 200));
    painter.rect_stroke(r, 1.5, egui::Stroke::new(1.0_f32, pal::RIM()), egui::StrokeKind::Middle);
    for k in 1..=3 {
        let y = r.min.y + r.height() * k as f32 / 4.0;
        painter.line_segment(
            [egui::pos2(r.min.x + 2.0, y), egui::pos2(r.max.x - 2.0, y)],
            egui::Stroke::new(1.0_f32, pal::RIM()),
        );
    }
}

// Wire drawing

/// A wire from an output to an input: a thin curve in the colour of the data it
/// carries.
pub fn draw_wire_h(
    painter: &egui::Painter,
    from: egui::Pos2,
    to: egui::Pos2,
    colour: egui::Color32,
    hovered: bool,
    z: f32,
) {
    let off = (to.x - from.x).abs().max(60.0 * z) * 0.5;
    let c1 = egui::pos2(from.x + off, from.y);
    let c2 = egui::pos2(to.x - off, to.y);
    let pts: Vec<egui::Pos2> = (0..=24).map(|i| bezier(from, c1, c2, to, i as f32 / 24.0)).collect();
    painter.add(egui::Shape::line(
        pts,
        egui::Stroke::new(
            (if hovered { WIRE_W + 1.4 } else { WIRE_W }) * z.clamp(0.7, 1.5),
            if hovered { egui::Color32::WHITE } else { colour },
        ),
    ));
}

pub(crate) fn is_near_bezier_h(p: egui::Pos2, from: egui::Pos2, to: egui::Pos2, thresh: f32) -> bool {
    let off = (to.x - from.x).abs().max(60.0) * 0.5;
    let c1 = egui::pos2(from.x + off, from.y);
    let c2 = egui::pos2(to.x - off, to.y);
    (0..=20).any(|i| {
        let q = bezier(from, c1, c2, to, i as f32 / 20.0);
        ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt() < thresh
    })
}

fn bezier(p0: egui::Pos2, p1: egui::Pos2, p2: egui::Pos2, p3: egui::Pos2, t: f32) -> egui::Pos2 {
    let mt = 1.0 - t;
    egui::pos2(
        mt * mt * mt * p0.x + 3.0 * mt * mt * t * p1.x + 3.0 * mt * t * t * p2.x + t * t * t * p3.x,
        mt * mt * mt * p0.y + 3.0 * mt * mt * t * p1.y + 3.0 * mt * t * t * p2.y + t * t * t * p3.y,
    )
}
