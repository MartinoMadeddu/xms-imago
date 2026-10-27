//! The Properties pane: the parameters of the selected node.
//!
//! Every node is laid out the same way (see `form`): a first group with the
//! node's type and name, then its parameters in titled groups of aligned rows,
//! then what came in and what goes out. The same words mean the same thing
//! everywhere: Translate, Rotate and Scale for every transform, Path for every
//! file, In and Out for what passes through.

use crate::batch::{self, BatchState};
use crate::core::anim::{AnimData, FrameRate, PoseEdit, RATE_PRESETS};
use crate::core::manip::Tool;
use crate::core::poly::{
    collapse_all, collapse_op, push_op, restore_run, ExtrudeMode, PolyMesh, PolyOp, PolyOpKind, PolySelection,
    SelSource, SubLevel,
};
use crate::file_browser::{BrowseMode, BrowseTarget, FileBrowser};
use crate::ice::ui::draw_subnet_node_properties;
use crate::ice::{GraphNavigation, IceGraphStore};
use crate::node_graph::NodeGraphState;
use crate::scene_explorer::OperatorStack;
use crate::types::{node_type_icon, node_type_label, BodyView, MergeMode, NodeId, NodeType, RetimeMode, SplitPick};
use glam::Vec3;
use glam::{EulerRot, Quat};
use std::sync::Arc;

use super::form::{self, group, group_with, ink, FieldExt, Rows, Status};

/// Clips around the selected node and the playhead, so animation nodes can show
/// what they receive and set parameters from the current frame.
pub struct AnimContext {
    /// Playhead on the timecode axis, seconds.
    pub time: f64,
    pub output: Option<Arc<AnimData>>,
    pub input: Option<Arc<AnimData>>,
}

/// Things a node's parameters can ask the application to do.
pub enum PanelAction {
    /// Run these Write FBX nodes, for the current file or the whole folder.
    Write { targets: Vec<NodeId>, all_files: bool },
    /// Run this Write USD node.
    WriteUsd(NodeId),
    /// Run this Render Command: write the layer above it first if it says so,
    /// then start its command lines.
    RunCommand(NodeId),
}

/// Services the panel can reach beyond the graph.
pub struct PanelIo<'a> {
    pub browser: &'a mut FileBrowser,
    pub batch: &'a BatchState,
    pub action: Option<PanelAction>,
    /// Mesh entering the selected node, when it is an Edit Poly node.
    pub poly_input: Option<crate::modelling::InputMesh>,
    /// Viewport tool of the Edit Poly node.
    pub tool: &'a mut Tool,
    /// The rows selected in the Scene Explorer: what "+ Add Location" adds.
    pub scene_paths: Vec<String>,
    /// Operators to add to the graph after the panel: a name and a type (a
    /// MaterialX operator made from a stage's material).
    pub add: Vec<(String, NodeType)>,
    /// The selected ICE operator's Parameter nodes: name and default, a name
    /// once.
    pub ice_params: Vec<(String, xms_ice::Value)>,
}

pub fn draw_properties_panel(
    ui: &mut egui::Ui,
    graph: &mut NodeGraphState,
    scene: &OperatorStack,
    subnets: &mut IceGraphStore,
    nav: &GraphNavigation,
    anim: &AnimContext,
    io: &mut PanelIo,
) {
    egui::Frame::NONE
        .fill(ink::PANEL_BG())
        .inner_margin(6.0)
        .show(ui, |ui| {
            if let Some(sid) = nav.current_subnet {
                // Every prim on each reference input of the operator the
                // ICE graph is in, from input 1: what Get Reference picks from.
                let pass =
                    |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
                let references: Vec<Vec<String>> = graph
                    .nodes
                    .iter()
                    .find(|n| matches!(n.node_type, NodeType::Ice { id, .. } if id == sid))
                    .map(|n| {
                        n.inputs
                            .iter()
                            .skip(1)
                            .map(|i| {
                                i.connected_output
                                    .and_then(|(src, out)| graph.eval_scene_paths(src, out, &pass))
                                    .unwrap_or_default()
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if let Some(sg) = subnets.get_mut(sid) {
                    draw_subnet_node_properties(ui, sg, &references, &io.scene_paths, io.browser);
                    return;
                }
            }
            // A MaterialX graph open: the node selected in it.
            if let Some(op) = nav.materialx {
                let found = graph
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == op)
                    .and_then(|n| match &mut n.node_type {
                        NodeType::MaterialX { graph } => Some((n.name.clone(), graph)),
                        _ => None,
                    });
                if let Some((name, g)) = found {
                    crate::materialx::params::inner(ui, op, &name, g, io);
                    return;
                }
            }
            // An ICE operator: its graph's Parameter nodes.
            io.ice_params = graph
                .selected_node
                .and_then(|id| graph.nodes.iter().find(|n| n.id == id))
                .and_then(|n| match n.node_type {
                    NodeType::Ice { id, .. } => subnets.get(id),
                    _ => None,
                })
                .map(|sg| crate::ice::compound::parameters_of(&sg.composed()))
                .unwrap_or_default();
            draw_properties(ui, graph, scene, anim, io);
        });
}

const FBX_EXTS: &[&str] = &["fbx"];

/// What an FBX file holds, as Load FBX reads it.
fn fbx_contents(ui: &mut egui::Ui, info: &crate::fbx_loader::FbxInfo) {
    let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    if !info.meshes.is_empty() {
        group(
            ui,
            "Geometry",
            Some("Second output: one packed primitive per mesh node"),
            |f| {
                for m in info.meshes.iter().take(12) {
                    let mut what = format!(
                        "{}, {}",
                        plural(m.points, "point", "points"),
                        plural(m.polygons, "polygon", "polygons")
                    );
                    if m.skinned {
                        what.push_str(", skinned");
                    }
                    if m.uv_sets > 0 {
                        what.push_str(", UVs");
                    }
                    if !m.shapes.is_empty() {
                        what.push_str(&format!(", {}", plural(m.shapes.len(), "blend shape", "blend shapes")));
                    }
                    f.value(m.path.rsplit('/').next().unwrap_or(&m.path), what);
                }
                if info.meshes.len() > 12 {
                    f.value("", format!("and {} more", info.meshes.len() - 12));
                }
                if info.bones > 0 {
                    f.value(
                        "Bind pose",
                        if info.bind_pose {
                            "in the file: Rest stands in it"
                        } else {
                            "not in the file: Rest is the first frame"
                        },
                    );
                }
            },
        );
    }
    if !info.materials.is_empty() {
        group(ui, "Materials", None, |f| {
            for m in info.materials.iter().take(12) {
                let tex = if m.textures.is_empty() {
                    String::new()
                } else {
                    format!(
                        ", {}",
                        m.textures
                            .iter()
                            .map(|(what, file)| format!("{what}: {}", file.rsplit(['/', '\\']).next().unwrap_or(file)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                f.value(
                    &m.name,
                    format!("colour {:.2} {:.2} {:.2}{tex}", m.color[0], m.color[1], m.color[2]),
                );
            }
        });
    }
    if !info.cameras.is_empty() {
        group(ui, "Cameras", None, |f| {
            for c in &info.cameras {
                let lens = if c.perspective {
                    format!("{:.0} mm on {:.1} × {:.1} mm", c.focal_mm, c.film_mm[0], c.film_mm[1])
                } else {
                    "orthographic".into()
                };
                f.value(
                    &c.name,
                    format!(
                        "{lens}, clip {:.2} to {:.0} m, at {:.2} {:.2} {:.2}{}",
                        c.near,
                        c.far,
                        c.position.x,
                        c.position.y,
                        c.position.z,
                        if c.animated { ", animated" } else { "" }
                    ),
                );
            }
        });
    }
    if !info.lights.is_empty() {
        group(ui, "Lights", None, |f| {
            for l in &info.lights {
                f.value(
                    &l.name,
                    format!(
                        "{}, colour {:.2} {:.2} {:.2}, intensity {:.2}, at {:.2} {:.2} {:.2}",
                        l.kind,
                        l.color[0],
                        l.color[1],
                        l.color[2],
                        l.intensity,
                        l.position.x,
                        l.position.y,
                        l.position.z
                    ),
                );
            }
        });
    }
    if !info.not_read.is_empty() {
        group(ui, "Not read in full", None, |f| {
            for n in &info.not_read {
                f.status(Status::Info, n.clone());
            }
        });
    }
}

/// The human pass's settings, as Body Collide shows them.
fn limits_group(ui: &mut egui::Ui, limits: &mut crate::core::human_ik::Limits, hint: &str) {
    group(ui, "Human limits", Some(hint), |f| {
        f.toggle("On", None, &mut limits.on);
        f.enabled(limits.on, |f| {
            f.slider(
                "Swivel",
                Some("Furthest an elbow or a knee may point away from where it points in the reference"),
                &mut limits.swivel,
                0.0..=90.0,
                "°",
            );
            f.slider(
                "Wrist and ankle",
                Some("Furthest a hand or a foot may turn away from the reference"),
                &mut limits.give,
                0.0..=90.0,
                "°",
            );
        });
    });
}

/// What a Reach or Human Limits node did.
fn reach_report(ui: &mut egui::Ui, anim: &AnimContext, reach: bool) {
    let Some(r) = anim.output.as_ref().and_then(crate::core::reach::report_for) else {
        return;
    };
    group(ui, "Result", None, |f| {
        if reach {
            f.value("Limbs", format!("{}", r.limbs));
            f.value(
                "Furthest from the goal",
                format!(
                    "{:.1} cm before, {:.1} cm after",
                    r.miss_before * 100.0,
                    r.miss_after * 100.0
                ),
            );
        }
        let h = r.human;
        f.value(
            "Bent the wrong way",
            format!("{} frames before, {} after", h.flips_before, h.flips_after),
        );
        f.value(
            "Wrist or ankle past its range",
            format!("{} frames before, {} after", h.ends_before, h.ends_after),
        );
    });
}

pub fn draw_properties(
    ui: &mut egui::Ui,
    graph: &mut NodeGraphState,
    _scene: &OperatorStack,
    anim: &AnimContext,
    io: &mut PanelIo,
) {
    let Some(sel_id) = graph.selected_node else {
        ui.colored_label(ink::DIM(), "Select a node to see its parameters.");
        return;
    };
    // `$OPNAME` in what its panel shows (a path's → line, Found) is its
    // name; `{name}` what comes down the wires to it.
    let _operator = graph.operator(sel_id);
    let from_above = graph.from_above(sel_id);
    // The same, for the panel: name, value, the operator it is from.
    let named = |a: &crate::node_graph::from_above::FromAbove| {
        (
            a.name.clone(),
            a.value.clone(),
            graph
                .nodes
                .iter()
                .find(|n| n.id == a.from)
                .map(|n| n.name.clone())
                .unwrap_or_default(),
        )
    };
    let above_rows: Vec<(String, String, String)> = from_above.iter().map(named).collect();
    // What it publishes itself, for those below.
    let own_rows: Vec<(String, String, String)> = graph
        .going_out(sel_id)
        .iter()
        .filter(|a| a.from == sel_id)
        .map(named)
        .collect();

    // Facts about the graph the parameter widgets need, gathered before the
    // node is borrowed for editing.
    let all_writers = batch::write_nodes(graph);
    let batch_files = batch::file_count(graph, &batch::upstream_folder_loaders(graph, &[sel_id]));
    let mut resync = false;
    // Tabs: Parameters; Prims Location, for operators that work on
    // primitives (which they work on); Graph Variables, for every operator
    // (the `{name}` that come down the wires to it).
    let takes = graph
        .nodes
        .iter()
        .find(|n| n.id == sel_id)
        .is_some_and(|n| crate::core::locations::takes_locations(&n.node_type));
    let tab_id = egui::Id::new(("parameters_tab_shown", sel_id.0));
    let mut tab = ui.data(|d| d.get_temp::<Tab>(tab_id)).unwrap_or(Tab::Parameters);
    if tab == Tab::Locations && !takes {
        tab = Tab::Parameters;
    }
    let on_locations = tab == Tab::Locations;
    // Packed primitives coming into the node: their paths, and which are picked.
    // Asked only for the nodes that list them: it cooks what is upstream.
    let packed_in: Vec<(String, bool)> = graph
        .nodes
        .iter()
        .find(|n| n.id == sel_id)
        .filter(|n| {
            on_locations
                || matches!(
                    n.node_type,
                    NodeType::PickPrims { .. }
                        | NodeType::PrunePrims { .. }
                        | NodeType::UnpackPrims
                        | NodeType::SetAttribute { .. }
                )
        })
        .and_then(|n| n.inputs.first())
        .and_then(|i| i.connected_output)
        .and_then(|(src, out)| {
            let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
            graph.eval_packed(src, out, &pass)
        })
        .unwrap_or_default();
    let prim_paths: Vec<String> = packed_in.iter().map(|p| p.0.clone()).collect();
    // An operator whose locations name prims (Set Attribute) can name any
    // prim of the scene coming in: groups, cameras and lights too.
    let names = graph
        .nodes
        .iter()
        .find(|n| n.id == sel_id)
        .is_some_and(|n| crate::core::locations::names_prims(&n.node_type));
    let scene_in: Vec<String> = if on_locations && names {
        graph
            .nodes
            .iter()
            .find(|n| n.id == sel_id)
            .and_then(|n| n.inputs.first())
            .and_then(|i| i.connected_output)
            .and_then(|(src, out)| {
                let pass =
                    |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
                graph.eval_scene_paths(src, out, &pass)
            })
            .unwrap_or_default()
    } else {
        vec![]
    };
    // What reaches a Material Binding: the material on each input, and the
    // materials of the scene coming in.
    let (binding_ports, scene_materials) = match graph.nodes.iter().find(|n| n.id == sel_id).map(|n| &n.node_type) {
        Some(NodeType::MaterialBinding { .. }) => {
            let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
            graph.binding_inputs(sel_id, &pass)
        }
        _ => (vec![], vec![]),
    };
    let port_names: Vec<Option<String>> = binding_ports
        .iter()
        .map(|m| m.as_ref().map(|m| m.name.clone()))
        .collect();
    // What reaches a render operator: the scene's cameras, products and
    // RenderSettings prim.
    let render_in = match graph.nodes.iter().find(|n| n.id == sel_id).map(|n| &n.node_type) {
        Some(NodeType::RenderCommons { .. } | NodeType::RenderSettings { .. } | NodeType::RenderVariables { .. }) => {
            let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
            graph.eval_render_context(sel_id, &pass)
        }
        _ => Default::default(),
    };
    // What a Render Command renders: the Write USD above it, its name and file;
    // and the scene's frames.
    let command_in = match graph.nodes.iter().find(|n| n.id == sel_id).map(|n| &n.node_type) {
        Some(NodeType::RenderCommand { .. }) => super::render::CommandIn {
            // `{usd}` as it comes down the wires, and who gave it.
            writer: above_rows
                .iter()
                .find(|a| a.0 == "usd")
                .map(|a| (a.2.clone(), a.1.clone())),
            scene: (graph.scene.start, graph.scene.end),
        },
        _ => Default::default(),
    };
    // What reaches a Body Collide node: its clip and its collider.
    let collide_in = match graph.nodes.iter().find(|n| n.id == sel_id).map(|n| &n.node_type) {
        Some(NodeType::Ragdoll { .. }) => graph.ragdoll_inputs(sel_id),
        _ => (None, None),
    };

    // Time: what a Local Animation keys, and where each input of a Merge
    // Animation lands.
    let time_info: Vec<(String, String)> = match graph.nodes.iter().find(|n| n.id == sel_id) {
        Some(n) if matches!(n.node_type, NodeType::LocalAnimation { .. }) => {
            let src = n
                .inputs
                .first()
                .and_then(|i| i.connected_output)
                .and_then(|(s, _)| graph.nodes.iter().find(|x| x.id == s));
            let keyed = n.channels.len();
            let local = graph.node_frame(n);
            vec![
                (
                    "Keys".into(),
                    match src {
                        Some(x) => format!(
                            "{} ({}): {keyed} number{} keyed",
                            x.name,
                            crate::types::node_type_label(&x.node_type),
                            if keyed == 1 { "" } else { "s" }
                        ),
                        None => "Wire an operator into it: its parameters are keyed here".into(),
                    },
                ),
                ("Local frame".into(), format!("{local}")),
            ]
        }
        Some(n) if matches!(n.node_type, NodeType::MergeAnimation { .. }) => graph
            .merge_shifts(n)
            .iter()
            .enumerate()
            .filter_map(|(k, (w, r, s))| {
                let (src, _) = (*w)?;
                let name = graph
                    .nodes
                    .iter()
                    .find(|x| x.id == src)
                    .map(|x| x.name.clone())
                    .unwrap_or_default();
                let port = if k < 26 {
                    ((b'A' + k as u8) as char).to_string()
                } else {
                    (k + 1).to_string()
                };
                Some((
                    port,
                    match r {
                        Some(r) => {
                            let p = s.place(*r);
                            format!("{name}: frames {:.0} to {:.0}, on {:.0} to {:.0}", r.0, r.1, p.0, p.1)
                        }
                        None => format!("{name}: not in time"),
                    },
                ))
            })
            .collect(),
        _ => vec![],
    };
    let range_now = graph.time_range(sel_id);

    let Some(node) = graph.nodes.iter_mut().find(|n| n.id == sel_id) else {
        return;
    };

    // The node itself
    let title = format!(
        "{}  {}",
        node_type_icon(&node.node_type),
        node_type_label(&node.node_type)
    );
    group(ui, &title, None, |f| {
        f.text(
            "Name",
            Some("The operator's name in the graph: one word, a space typed becomes an underscore"),
            &mut node.name,
            "",
        );
    });
    if node.name.chars().any(char::is_whitespace) {
        node.name = crate::node_graph::node_name(&node.name);
    }

    {
        // Parameters, Prims Location for those that take it, Graph Variables.
        let set = crate::core::locations::rows(&node.locations).count();
        // Material Binding's rows are its bindings: a location and a material each.
        let tab_name = if matches!(node.node_type, NodeType::MaterialBinding { .. }) {
            "Bindings"
        } else {
            "Prims Location"
        };
        let located_label = if set == 0 {
            tab_name.to_string()
        } else {
            format!("{tab_name} ({set})")
        };
        let n = above_rows.len();
        let vars_label = if n == 0 {
            "Graph Variables".to_string()
        } else {
            format!("Graph Variables ({n})")
        };
        let mut shown: Vec<(Tab, &str, Option<&str>)> = vec![(Tab::Parameters, "Parameters", None)];
        if takes {
            shown.push((
                Tab::Locations,
                located_label.as_str(),
                Some("The primitives this operator works on. Drop prims from the Scene Explorer here"),
            ));
        }
        shown.push((
            Tab::Variables,
            vars_label.as_str(),
            Some("The {name} variables that come down the wires to this operator, read in its fields"),
        ));
        let labels: Vec<(&str, Option<&str>)> = shown.iter().map(|(_, l, h)| (*l, *h)).collect();
        let mut open = shown.iter().position(|(t, _, _)| *t == tab).unwrap_or(0);
        let responses = form::tabs(ui, ("parameters", sel_id.0), &labels, &mut open);
        tab = shown[open].0;
        // A prim dragged from the Scene Explorer and dropped on the tab.
        if let Some(k) = shown.iter().position(|(t, _, _)| *t == Tab::Locations) {
            if let Some(p) = responses[k].dnd_release_payload::<crate::types::DraggedPrims>() {
                for path in &p.0 {
                    add_location(&mut node.locations, path);
                }
                tab = Tab::Locations;
            }
        }
    }
    ui.data_mut(|d| d.insert_temp(tab_id, tab));
    if tab == Tab::Variables {
        graph_variables_tab(ui, &above_rows, &own_rows);
        return;
    }
    if takes {
        if tab == Tab::Locations {
            let kind = match node.node_type {
                NodeType::PrunePrims { .. } => TabKind::Prune,
                NodeType::MaterialBinding { .. } => TabKind::Binds,
                _ if names => TabKind::Names,
                _ => TabKind::Takes,
            };
            let paths = if names { &scene_in } else { &prim_paths };
            let binds = match &mut node.node_type {
                NodeType::MaterialBinding { bindings, .. } => Some(BindColumn {
                    to: bindings,
                    inputs: &port_names,
                    scene: &scene_materials,
                }),
                _ => None,
            };
            locations_tab(ui, &mut node.locations, paths, &io.scene_paths, kind, binds);
            return;
        }
    }
    let located = crate::core::locations::is_set(&node.locations);
    let mut enter_compound = false;
    let mut back_to_library = false;

    match &mut node.node_type {
        // Create
        NodeType::CreateCube { size, location } => {
            location_group(ui, location, "/cube");
            group(ui, "Shape", None, |f| { f.slider("Size", None, size, 0.1..=5.0, " m"); });
        }
        NodeType::CreateSphere { radius, segments, location } => {
            location_group(ui, location, "/sphere");
            group(ui, "Shape", None, |f| {
                f.slider("Radius", None, radius, 0.1..=3.0, " m");
                f.slider_u32("Segments", None, segments, 4..=64, "");
            });
        }
        NodeType::CreateGrid { rows, cols, size, location } => {
            location_group(ui, location, "/grid");
            group(ui, "Shape", None, |f| {
                f.slider_u32("Rows", None, rows, 1..=100, "");
                f.slider_u32("Columns", None, cols, 1..=100, "");
                f.slider("Size", None, size, 0.1..=20.0, " m");
            });
        }
        NodeType::Camera { location, translate, rotate, camera } => camera_rows(ui, location, translate, rotate, camera),
        NodeType::Light { location, translate, rotate, light } => super::light::light_rows(ui, location, translate, rotate, light, io, sel_id),
        NodeType::TestClip { seconds, fps_num, fps_den } => group(ui, "Clip", Some("A procedural walk on the spot, starting at 01:00:00:00"), |f| {
            f.slider("Length", None, seconds, 0.5..=60.0, " s");
            rate_row(f, fps_num, fps_den);
        }),

        // Files
        NodeType::LoadUsd { path, mode, location, prim, convert } => {
            group(ui, "File", Some("A USD file (.usda, .usdc, .usdz), read as primitives. Variables are put in: $GDIR/usd/set.usda"), |f| {
                path_row(f, path, "/path/to/file.usdz", io, sel_id, BrowseMode::File, "Open USD", &["usda", "usdc", "usdz", "usd"]);
                path_status(f, path);
            });
            let read = crate::core::vars::expand(path.trim(), &[]);
            load_mode_rows(ui, &read, mode, location, prim, *convert);
            group(ui, "Units and axis", None, |f| {
                f.toggle("Y-up metres", Some("Turn the stage to Y-up and scale it to metres by its upAxis and metersPerUnit, in its placement (the geometry stays as authored). Off: as authored, as USD composes it and renderers read it. Changing it reads the stage again"), convert);
            });
            if !read.is_empty() { usd_summary(ui, &read, *convert, io); }
        }
        NodeType::WriteUsd { path } => {
            group(ui, "File", Some("A .usda layer with what the scene graph changed in its USD stages: it sublayers each stage's file and holds only opinions over it. Opened on its own it gives what Imago shows"), |f| {
                path_row(f, path, "/path/to/edits.usda", io, sel_id, BrowseMode::Save, "Write USD layer", &["usda"]);
            });
            group(ui, "Write", None, |f| {
                f.buttons("Override layer", Some("Moved prims get their own transform, changed geometry its changed attributes, pruned prims are deactivated, prims made in Imago are added. Values at the current frame"), |ui| {
                    if ui.add_enabled(!path.trim().is_empty(), egui::Button::new("Write")).clicked() {
                        io.action = Some(PanelAction::WriteUsd(sel_id));
                    }
                });
                match crate::usd_write::last(sel_id) {
                    None => {}
                    Some(Err(e)) => f.status(Status::Bad, e),
                    Some(Ok(r)) => {
                        let arcs = if r.referenced > 0 { format!(", {} referenced", r.referenced) } else { String::new() };
                        f.status(Status::Ok, format!("Written: {} changed, {} deactivated, {} added{arcs}", r.changed, r.deactivated, r.added));
                        for s in r.skipped.iter().take(8) { f.status(Status::Info, format!("Not written: {s}")); }
                        if r.skipped.len() > 8 { f.status(Status::Info, format!("… and {} more not written", r.skipped.len() - 8)); }
                    }
                }
            });
        }
        NodeType::LoadFbx { path, take } => {
            group(ui, "File", Some("Everything the file holds. Clip: the skeleton and one take, baked per frame, with the mesh skinned to it. Geometry: every mesh as a packed primitive, placed where the file has it, with normals, texture coordinates, colours, material and skin weights by joint name. Rest: the skeleton in the pose its skin was bound in, one frame. Read Y up, in metres; written back in the file's own axes and unit"), |f| {
                path_row(f, path, "/path/to/take.fbx", io, sel_id, BrowseMode::File, "Open FBX", FBX_EXTS);
                take_rows(f, crate::fbx_loader::load_fbx_cached(path, *take), take);
            });
            if let Ok(scene) = crate::fbx_loader::load_scene_cached(path) { fbx_contents(ui, &scene.info); }
            clip_summary(ui, anim, false);
        }
        NodeType::LoadFbxDir { dir, index, take } => {
            let files = crate::fbx_loader::list_fbx(dir);
            group(ui, "Folder", Some("One FBX of the folder at a time. Write FBX can run over all of them"), |f| {
                path_row(f, dir, "/path/to/folder", io, sel_id, BrowseMode::Folder, "Choose FBX folder", &["fbx"]);
                if files.is_empty() {
                    f.status(if dir.is_empty() { Status::Info } else { Status::Bad }, if dir.is_empty() { "No folder set" } else { "No .fbx files in this folder" });
                    return;
                }
                let last = files.len() as u32 - 1;
                *index = (*index).min(last);
                let name = |i: usize| files[i].file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                f.row("File", None, |ui| {
                    ui.horizontal(|ui| {
                        if ui.add_enabled(*index > 0, egui::Button::new("◀")).clicked() { *index -= 1; }
                        if ui.add_enabled(*index < last, egui::Button::new("▶")).clicked() { *index += 1; }
                        egui::ComboBox::from_id_salt("fbx_dir_file")
                            .width(ui.available_width() - 8.0)
                            .selected_text(format!("{} of {}  {}", *index + 1, files.len(), name(*index as usize)))
                            .show_ui(ui, |ui| {
                                for i in 0..files.len() { ui.selectable_value(index, i as u32, format!("{}  {}", i + 1, name(i))); }
                            });
                    });
                });
                take_rows(f, crate::fbx_loader::load_fbx_cached(&files[*index as usize].to_string_lossy(), *take), take);
            });
            clip_summary(ui, anim, false);
        }
        NodeType::LoadFbxMesh { path } => group(ui, "File", Some("Every mesh of an FBX file, where it stands, as primitives. A set for Body Collide to keep a character out of"), |f| {
            path_row(f, path, "/path/to/set.fbx", io, sel_id, BrowseMode::File, "Open FBX", &["fbx"]);
            match crate::fbx_loader::load_meshes_cached(path) {
                Ok(meshes) => {
                    let tris: usize = meshes.iter().map(|m| m.1.indices.len() / 3).sum();
                    f.status(Status::Ok, format!("{} mesh{}, {} triangles", meshes.len(), if meshes.len() == 1 { "" } else { "es" }, tris));
                }
                Err(e) if path.is_empty() => { let _ = e; f.status(Status::Info, "No file set"); }
                Err(e) => f.status(Status::Bad, e),
            }
        }),
        NodeType::WriteFbx { path, mesh } => {
            group(ui, "File", Some("Its own variables in the path: {dir} source folder, {file} source file, {char} character, {take} take name; and the built-ins ($GDIR, $GNAME, $F…)"), |f| {
                path_row(f, path, crate::types::DEFAULT_WRITE_PATH, io, sel_id, BrowseMode::Folder, "Choose output folder", &["fbx"]);
                f.toggle("Mesh", Some("Also write the skinned mesh, its skin and bind pose. Off: the skeleton and its motion only, for importing onto a skeleton the engine already has"), mesh);
                match &anim.output {
                    Some(c) => {
                        f.value("Writes", crate::fbx_writer::resolve_path(path, c).display().to_string());
                        f.value("Content", format!("{} joints, {}{}", c.joints.len(),
                            if c.frames > 1 { format!("{} frames", c.frames) } else { "one pose".into() },
                            match (&c.skin, *mesh) { (Some(s), true) => format!(", mesh of {} vertices", s.positions.len()), _ => String::new() }));
                        f.value("Space", space_line(c));
                    }
                    None => f.status(Status::Info, "No clip connected"),
                }
            });
            let running = io.batch.0.lock().unwrap().running;
            group(ui, "Write", None, |f| {
                f.enabled(!running, |f| {
                    f.buttons("This node", None, |ui| {
                        if ui.button("This file").clicked() { io.action = Some(PanelAction::Write { targets: vec![sel_id], all_files: false }); }
                        if ui.add_enabled(batch_files > 0, egui::Button::new(format!("Whole folder ({batch_files})"))).clicked() {
                            io.action = Some(PanelAction::Write { targets: vec![sel_id], all_files: true });
                        }
                    });
                    f.buttons(&format!("All {} Write nodes", all_writers.len()), None, |ui| {
                        if ui.button("This file").clicked() { io.action = Some(PanelAction::Write { targets: all_writers.clone(), all_files: false }); }
                        if ui.add_enabled(batch_files > 0, egui::Button::new(format!("Whole folder ({batch_files})"))).clicked() {
                            io.action = Some(PanelAction::Write { targets: all_writers.clone(), all_files: true });
                        }
                    });
                });
            });
            batch_log(ui, io.batch);
        }

        // Modify
        NodeType::Transform { translation, rotation, scale } => {
            let clip = anim.input.is_some();
            let mut reset = false;
            group_with(ui, "Transform",
                Some("Moves whatever comes in: a mesh, primitives (the picked ones, or all of them), or a clip. A clip moves by its top joints, so its skinned mesh moves with it once. Scale, then rotate X, Y, Z, then translate"),
                |ui| { if ui.small_button("Reset").clicked() { reset = true; } },
                |f| {
                    let mut t = translation.to_array();
                    if f.xyz("Translate", None, &mut t, 0.01, "") { *translation = Vec3::from_array(t); }
                    let mut deg = [rotation.x.to_degrees(), rotation.y.to_degrees(), rotation.z.to_degrees()];
                    if f.xyz("Rotate", None, &mut deg, 0.5, "°") { *rotation = Vec3::new(deg[0].to_radians(), deg[1].to_radians(), deg[2].to_radians()); }
                    if clip {
                        let mut s = scale.x;
                        if f.drag("Scale", Some("A skeleton scales the same on every axis"), &mut s, 0.01, "").changed() { *scale = Vec3::splat(s); }
                    } else {
                        let mut s = scale.to_array();
                        if f.xyz("Scale", None, &mut s, 0.01, "") { *scale = Vec3::from_array(s); }
                    }
                });
            if reset { *translation = Vec3::ZERO; *rotation = Vec3::ZERO; *scale = Vec3::ONE; }
            if clip { clip_summary(ui, anim, true); }
        }
        NodeType::LocalAnimation { start, end, .. } => group(ui, "Local Animation",
            Some("Keys on the parameters of the operator wired into it, on frames of its own: local frame 0 falls on Start. Key them in the Keys group below, or with S"),
            |f| {
                let (mut a, mut b) = (*start as i32, *end as i32);
                if f.row("Start", Some("The scene frame its local frame 0 falls on"), |ui| form::number(ui, None, &mut a, |d| d.speed(0.25))).changed() { *start = a as i64; }
                if f.row("End", Some("The scene frame it ends on: held before Start and after End"), |ui| form::number(ui, None, &mut b, |d| d.speed(0.25))).changed() { *end = (b as i64).max(*start); }
                for (k, v) in &time_info { f.value(k, v.clone()); }
                if let Some(r) = range_now { f.value("Range", format!("{:.0} to {:.0}, {:.0} frames", r.0, r.1, r.1 - r.0 + 1.0)); }
            }),
        NodeType::MergeAnimation { align, match_rate, inputs } => group(ui, "Merge Animation",
            Some("Its inputs as one, as Merge does, each first moved or stretched in time. Wiring the last input adds another"),
            |f| {
                if f.drag_u32("Inputs", Some("How many inputs. Fewer drops the wires on the ones taken away"), inputs, 2..=64).changed() { resync = true; }
                let options: Vec<(crate::types::AnimAlign, &str)> = crate::types::AnimAlign::ALL.iter().map(|a| (*a, a.label())).collect();
                f.choice("Align", Some("First, Last and Middle line every input up with input A; Stretch and Shrink fit every input over the longest or shortest range"), align, &options);
                f.toggle("Match rate", Some("Clips coming in are resampled to the scene's rate first"), match_rate);
                for (k, v) in &time_info { f.value(k, v.clone()); }
                if let Some(r) = range_now { f.value("Range", format!("{:.0} to {:.0}", r.0, r.1)); }
            }),
        NodeType::Merge { mode, inputs } => group(ui, "Merge", Some("Its inputs, in order: on the same path the later is the stronger. Wiring the last input adds another"), |f| {
            if f.drag_u32("Inputs", Some("How many inputs. Fewer drops the wires on the ones taken away"), inputs, 2..=64).changed() { resync = true; }
            f.choice("Mode", Some("Auto goes by the first input: primitives are merged as primitives, a plain mesh or a clip becomes one mesh. Primitives and Flatten force one or the other"),
                mode, &[(MergeMode::Auto, "Auto"), (MergeMode::Primitives, "Primitives"), (MergeMode::Flatten, "Flatten")]);
            match mode {
                MergeMode::Auto => f.status(Status::Info, "Primitives when A brings primitives, else one mesh"),
                MergeMode::Primitives => f.status(Status::Info, "The inputs side by side; on the same path, the later is the stronger"),
                MergeMode::Flatten => f.status(Status::Info, "All the inputs as one mesh"),
            }
        }),
        NodeType::Ice { params, .. } => {
            group(ui, "ICE graph", None, |f| { f.status(Status::Info, "Double-click the node, or select it and press ↓, to open its ICE graph"); });
            if !io.ice_params.is_empty() {
                group(ui, "Parameters", Some("Its graph's Parameter nodes, by name: the value it gives each, the node's default until set"), |f| {
                    // Each kept on the operator (`ice::compound::keep_params`).
                    for (name, _) in &io.ice_params {
                        if let Some(v) = params.get_mut(name) {
                            crate::ice::ui::value_row(f, name, v);
                        }
                    }
                });
            }
        }

        // Primitives
        NodeType::PickPrims { pattern } => group(ui, "Pick", Some("The nodes after this one work on the picked primitives only and pass the others through: Edit Poly, Transform and the UV nodes all do"), |f| {
            pattern_rows(f, "pick_prims", "Paths", pattern, &prim_paths, false);
            if prim_paths.is_empty() { f.status(Status::Info, "No primitives coming in"); }
        }),
        NodeType::PrunePrims { pattern, keep } => group(ui, "Prune", Some("The primitives to prune are listed in the Prims Location tab"), |f| {
            f.choice("Matches", Some("Remove the primitives the Prims Location tab lists, or keep only those"), keep, &[(false, "Remove"), (true, "Keep only")]);
            // Graphs from before the tab pruned by a pattern of their own,
            // still used while the tab is empty.
            if !pattern.trim().is_empty() {
                pattern_rows(f, "prune_prims", "Older pattern", pattern, &prim_paths, false);
                f.status(Status::Info, "From an older graph: used while the Prims Location tab is empty");
            } else {
                f.status(Status::Info, "Which primitives: the Prims Location tab. Empty, nothing is pruned");
            }
            if prim_paths.is_empty() { f.status(Status::Info, "No primitives coming in"); }
        }),
        NodeType::SetAttribute { rows } => set_attribute_rows(ui, rows, located, prim_paths.len()),
        NodeType::Material { surface } => material_rows(ui, surface, &node.name, io, sel_id),
        NodeType::MaterialX { graph } => crate::materialx::params::operator(ui, sel_id, &node.name, graph, io),
        NodeType::MaterialBinding { library, inputs, .. } => {
            resync |= binding_rows(ui, library, inputs, &binding_ports, crate::core::locations::rows(&node.locations).count());
        }
        // Render
        NodeType::RenderCommons { scene } => super::render::scene_settings_rows(ui, scene, &render_in),
        NodeType::RenderSettings { renderer } => super::render::renderer_rows(ui, renderer, &render_in, sel_id),
        NodeType::RenderVariables { vars } => super::render::variables_rows(ui, vars, &render_in),
        NodeType::RenderCommand { command } => super::render::command_rows(ui, command, sel_id, &node.name, &command_in, io),
        NodeType::UnpackPrims => group(ui, "Unpack", Some("Nodes that need one mesh do this on their own. Unpack makes it explicit, or drops a pick"), |f| {
            f.value("In", format!("{} primitives", prim_paths.len()));
            f.value("Out", "One mesh");
        }),

        // UV
        NodeType::UvUnwrap { method, angle, margin, axis, tiles } => {
            use crate::core::uv::UvMethod;
            group(ui, "Unwrap", Some("The UV Editor pane shows the layout"), |f| {
                f.choice("Method", Some("Conformal: charts by normal angle, each flattened with least squares conformal maps and packed at equal density"),
                    method, &[(UvMethod::Conformal, "Conformal"), (UvMethod::Box, "Box"), (UvMethod::Planar, "Planar")]);
                match method {
                    UvMethod::Conformal => { f.slider("Chart angle", Some("Smaller: more charts, less distortion"), angle, 5.0..=89.0, "°"); }
                    UvMethod::Planar => { f.choice("Along", None, axis, &[(0, "X"), (1, "Y"), (2, "Z")]); }
                    UvMethod::Box => {}
                }
                f.slider("Margin", Some("Gap between charts, in UV space"), margin, 0.0..=0.1, "");
                f.drag_u32("UDIM tiles", Some("The separate pieces of the mesh spread over this many tiles. Pieces close together share one; the most surface goes to 1001"), tiles, 1..=100);
                if *tiles > 1 { f.value("", format!("1001 to {}", 1000 + *tiles)); }
            });
        }
        NodeType::UvTransform { offset, rotate, scale } => group(ui, "Transform", None, |f| {
            f.uv("Translate", None, offset, 0.005);
            f.drag("Rotate", None, rotate, 0.5, "°");
            f.uv("Scale", None, scale, 0.005);
        }),
        NodeType::UvEdit { edits } => {
            group(ui, "Islands", Some("In the UV Editor pane: click an island to select it, drag to move it"), |f| {
                if edits.is_empty() { f.status(Status::Info, "No island edited yet"); }
                else { f.value("Edited", format!("{} island{}", edits.len(), if edits.len() == 1 { "" } else { "s" })); }
            });
            let mut remove = None;
            for (i, e) in edits.iter_mut().enumerate() {
                let mut turn = false; let (mut flip_u, mut flip_v) = (false, false);
                group_with(ui, &format!("Island {}", e.island), None, |ui| {
                    if ui.small_button("🗑").on_hover_text("Remove this edit").clicked() { remove = Some(i); }
                    if ui.small_button("Flip V").clicked() { flip_v = true; }
                    if ui.small_button("Flip U").clicked() { flip_u = true; }
                    if ui.small_button("⟲ 90°").clicked() { turn = true; }
                }, |f| {
                    f.uv("Translate", None, &mut e.offset, 0.002);
                    f.drag("Rotate", None, &mut e.rotate, 0.5, "°");
                    f.uv("Scale", None, &mut e.scale, 0.005);
                });
                if turn { e.rotate = (e.rotate + 90.0) % 360.0; }
                if flip_u { e.scale[0] = -e.scale[0]; }
                if flip_v { e.scale[1] = -e.scale[1]; }
            }
            if let Some(i) = remove { edits.remove(i); }
        }

        // Animation
        NodeType::RenameJoints { find, replace, strip_namespace, prefix } => {
            let names: Vec<String> = anim.input.as_ref().map(|c| c.joints.iter().map(|j| j.name.clone()).collect()).unwrap_or_default();
            group(ui, "Rename", Some("Find is a regular expression, case ignored. Replace may use $1, $2 for its groups"), |f| {
                f.toggle("Strip namespace", Some("Remove the text up to the last ':'"), strip_namespace);
                pattern_rows(f, "rename_find", "Find", find, &names, true);
                f.text("Replace", None, replace, "");
                f.text("Prefix", None, prefix, "");
            });
            if let (Some(i), Some(o)) = (&anim.input, &anim.output) {
                group(ui, "Result", None, |f| f.wide(|ui| {
                    egui::ScrollArea::vertical().id_salt("rename_preview").max_height(220.0).show(ui, |ui| {
                        for (a, b) in i.joints.iter().zip(o.joints.iter()) {
                            let col = if a.name == b.name { ink::DIM() } else { ink::VALUE() };
                            ui.colored_label(col, format!("{}  >  {}", a.name, b.name));
                        }
                    });
                }));
            }
        }
        NodeType::TrimClip { head, tail } => {
            group(ui, "Trim", Some("Frames cut from the start and from the end of the clip"), |f| {
                let max = anim.input.as_ref().map(|i| i.frames.saturating_sub(1) as u32).unwrap_or(100_000);
                f.drag_u32("Cut at start", None, head, 0..=max);
                f.drag_u32("Cut at end", None, tail, 0..=max);
                if let Some(i) = &anim.input {
                    let playhead = i.frame_at(anim.time).clamp(i.start_frame, i.end_frame());
                    f.buttons("Playhead", None, |ui| {
                        if ui.button("Start here").clicked() { *head = (playhead - i.start_frame) as u32; *tail = (*tail).min(max - *head); }
                        if ui.button("End here").clicked() { *tail = (i.end_frame() - playhead) as u32; *head = (*head).min(max - *tail); }
                        if ui.button("Reset").clicked() { *head = 0; *tail = 0; }
                    });
                }
            });
            clip_summary(ui, anim, true);
        }
        NodeType::Retime { fps_num, fps_den, mode } => {
            group(ui, "Retime", None, |f| {
                rate_row(f, fps_num, fps_den);
                f.choice("Mode", Some("Resample keeps the duration and makes new frames. Reinterpret keeps the frames and changes the speed"),
                    mode, &[(RetimeMode::Resample, "Resample"), (RetimeMode::Reinterpret, "Reinterpret")]);
            });
            clip_summary(ui, anim, true);
        }
        NodeType::SetTimecode { hours, minutes, seconds, frames, drop_frame } => {
            group(ui, "Timecode", None, |f| {
                let tb = anim.input.as_ref().map(|i| i.rate.timebase() as u32).unwrap_or(120);
                f.row("Start", Some("Hours : minutes : seconds : frames"), |ui| {
                    ui.horizontal(|ui| {
                        form::number(ui, None, hours, |d| d.range(0..=23));
                        ui.label(":");
                        form::number(ui, None, minutes, |d| d.range(0..=59));
                        ui.label(":");
                        form::number(ui, None, seconds, |d| d.range(0..=59));
                        ui.label(":");
                        form::number(ui, None, frames, |d| d.range(0..=tb.saturating_sub(1)));
                    });
                });
                let can_drop = anim.input.as_ref().map(|i| i.rate.supports_drop_frame()).unwrap_or(true);
                f.enabled(can_drop, |f| { f.toggle("Drop frame", Some("29.97 and 59.94 only"), drop_frame); });
            });
            clip_summary(ui, anim, true);
        }
        NodeType::TimeWarp { speed, reverse } => {
            group(ui, "Time warp", None, |f| {
                f.drag("Speed", None, speed, 0.01, " ×");
                *speed = speed.clamp(0.05, 20.0);
                f.toggle("Reverse", None, reverse);
            });
            clip_summary(ui, anim, true);
        }
        NodeType::BlendClips { blend, align } => {
            group(ui, "Blend", Some("The first input, then the second. Joints are matched by name"), |f| {
                f.slider_u32("Blend", Some("Frames over which one clip becomes the other"), blend, 0..=240, "");
                f.toggle("Align", Some("Start the second clip where the first one ends"), align);
            });
            clip_summary(ui, anim, true);
        }
        NodeType::LoopClip { blend } => {
            group(ui, "Loop", Some("The end of the clip eased into the pose of its first frame. Travel is kept"), |f| {
                f.slider_u32("Blend", Some("Frames over which the end is eased"), blend, 0..=240, "");
            });
            clip_summary(ui, anim, true);
        }
        NodeType::MirrorClip => {
            group(ui, "Mirror", Some("Joints pair up by name: Left / Right, L_ / R_, _L / _R"), |f| {
                f.status(Status::Info, "Left and right swap, the motion is reflected");
            });
            clip_summary(ui, anim, true);
        }
        NodeType::SmoothClip { radius, amount, translations } => {
            group(ui, "Smooth", None, |f| {
                f.slider_u32("Radius", Some("Frames on each side"), radius, 0..=30, "");
                f.slider("Amount", None, amount, 0.0..=1.0, "");
                f.toggle("Positions too", None, translations);
            });
            clip_summary(ui, anim, true);
        }
        NodeType::InPlace { keep_height, to_root } => {
            group(ui, "In place", None, |f| {
                f.toggle("Keep height", Some("Keep the up and down motion"), keep_height);
                f.toggle("Root motion", Some("The joint above the hips carries the travel, the hips stay under it"), to_root);
            });
            clip_summary(ui, anim, true);
        }
        NodeType::FloorClip { height } => {
            group(ui, "Floor", None, |f| {
                f.drag("Height", None, height, 0.005, " m");
                if let Some(i) = &anim.input { f.value("Lowest now", format!("{:.3} m", i.lowest_point())); }
            });
            clip_summary(ui, anim, true);
        }
        NodeType::PruneJoints { words } => {
            let names: Vec<String> = anim.input.as_ref().map(|c| c.joints.iter().map(|j| j.name.clone()).collect()).unwrap_or_default();
            group(ui, "Prune", Some("Each matching joint goes, with everything below it"), |f| {
                pattern_rows(f, "prune_words", "Joints", words, &names, false);
                if let (Some(i), Some(o)) = (&anim.input, &anim.output) { f.value("Joints", format!("{} in, {} out", i.joints.len(), o.joints.len())); }
            });
        }
        NodeType::TransformClip { .. } | NodeType::Calamari { .. } => group(ui, "Older node", None, |f| {
            f.status(Status::Info, "Open the graph again to bring this node up to date");
        }),

        // Mocap
        NodeType::SplitSkeleton { picks } => {
            let input = anim.input.clone();
            let chars: Vec<(usize, String)> = input.as_ref()
                .map(|c| c.character_roots().into_iter().map(|r| (r, c.character_name(r))).collect())
                .unwrap_or_default();
            let (mut detect, mut add) = (false, false);
            let mut removed = false;
            group_with(ui, "Outputs", Some("One output per character"), |ui| {
                if ui.add_enabled(!chars.is_empty(), egui::Button::new("Detect").small())
                    .on_hover_text("One output per character found in the incoming clip").clicked() { detect = true; }
                if ui.small_button("+").on_hover_text("Add an output").clicked() { add = true; }
            }, |f| {
                let mut remove = None;
                for (n, pick) in picks.iter_mut().enumerate() {
                    f.row(&format!("Output {}", n + 1), None, |ui| {
                        ui.horizontal(|ui| {
                            let text = match &*pick {
                                SplitPick::Character(i) => match chars.get(*i as usize) {
                                    Some((_, name)) => format!("Character {}: {name}", i + 1),
                                    None            => format!("Character {} (not in this file)", i + 1),
                                },
                                SplitPick::Joint(name) => format!("Joint {name}"),
                            };
                            egui::ComboBox::from_id_salt(("split_pick", n))
                                .width(ui.available_width() - 30.0)
                                .selected_text(text)
                                .show_ui(ui, |ui| {
                                    ui.weak("Characters, by position");
                                    for (i, (_, name)) in chars.iter().enumerate() {
                                        ui.selectable_value(pick, SplitPick::Character(i as u32), format!("Character {}: {name}", i + 1));
                                    }
                                    if let Some(c) = &input {
                                        ui.separator();
                                        ui.weak("Any joint, by name");
                                        for j in &c.joints { ui.selectable_value(pick, SplitPick::Joint(j.name.clone()), &j.name); }
                                    }
                                });
                            if ui.small_button("🗑").on_hover_text("Remove this output").clicked() { remove = Some(n); }
                        });
                    });
                }
                if let Some(n) = remove { picks.remove(n); removed = true; }
            });
            if detect { *picks = (0..chars.len() as u32).map(SplitPick::Character).collect(); }
            if add { picks.push(SplitPick::Character(picks.len() as u32)); }
            resync |= detect || add || removed;
            group(ui, "Characters in", None, |f| match &input {
                None => f.status(Status::Info, "No clip connected"),
                Some(_) if chars.is_empty() => f.status(Status::Info, "None found in the incoming clip"),
                Some(c) => for (i, (root, name)) in chars.iter().enumerate() {
                    f.value(&format!("{}", i + 1), format!("{name}: {} joints from \"{}\"", c.subtree(*root).len(), c.joints[*root].name));
                },
            });
        }
        NodeType::Retarget => {
            group(ui, "Retarget", Some("The motion of the first input on the skeleton of the second. Joints are matched by the part of the body they are (from a Characterize node, or read from the names), the rest by name, ignoring namespaces and the prefix each skeleton shares. The skeletons may rest in different poses: bones are lined up at rest first"), |f| {
                match (&anim.input, &anim.output) {
                    (Some(i), Some(o)) => {
                        let (hi, ho) = (crate::core::human::Human::of(i), crate::core::human::Human::of(o));
                        f.value("Motion", format!("{} joints, {}", i.joints.len(), hi.convention));
                        f.value("Result", format!("{} joints, {}", o.joints.len(), ho.convention));
                    }
                    _ => f.status(Status::Info, "Connect a clip to both inputs"),
                }
            });
            clip_summary(ui, anim, true);
        }
        NodeType::Placeholder { inputs, outputs, description } => {
            group(ui, "Placeholder", Some("A node that does not exist yet. Each input goes to the output of the same number unchanged. Save the graph to show how a new node should work: its name, its inputs and outputs and the description travel in the file"), |f| {
                f.status(Status::Info, "Input 1 goes to output 1 unchanged, input 2 to output 2, and so on");
            });
            for (title, list, base) in [("Inputs", inputs, "In"), ("Outputs", outputs, "Out")] {
                group(ui, title, None, |f| {
                    let mut remove = None;
                    for (k, name) in list.iter_mut().enumerate() {
                        f.row(&format!("{}", k + 1), None, |ui| {
                            ui.horizontal(|ui| {
                                if crate::properties::form::typed(ui, &mut *name, |ui, t| ui.field(egui::TextEdit::singleline(t).desired_width(ui.available_width() - 30.0))).changed() { resync = true; }
                                if ui.small_button("🗑").on_hover_text("Remove").clicked() { remove = Some(k); }
                            });
                        });
                    }
                    if let Some(k) = remove { list.remove(k); resync = true; }
                    f.row("", None, |ui| {
                        if ui.button(format!("+ {}", if base == "In" { "Input" } else { "Output" })).clicked() {
                            list.push(format!("{base}{}", list.len() + 1));
                            resync = true;
                        }
                    });
                });
            }
            group(ui, "Description of node functionality for LLM", Some("What the node should do, for whoever builds it: what comes in on each input, what goes out on each output, the parameters it needs"), |f| {
                f.wide(|ui| {
                    let w = ui.available_width();
                    crate::properties::form::typed(ui, &mut *description, |ui, t| ui.field(egui::TextEdit::multiline(t).desired_rows(10).desired_width(w)
                        .hint_text("What this node should do with its inputs, what each output gives, the parameters it needs")));
                });
            });
        }
        NodeType::Reach { settings } => {
            group(ui, "Reach", Some("Hands and feet of the first input go where the second input (the motion before the retarget) has them against its hips, scaled by the ratio of the hip heights. Each arm and leg is solved with two bones; the hand and the foot keep their turn. Then the Body Collide node's human pass"), |f| {
                f.toggle("Feet", None, &mut settings.feet);
                f.toggle("Hands", None, &mut settings.hands);
                f.slider("Weight", Some("How much of the way to the goal"), &mut settings.weight, 0.0..=1.0, "");
            });
            limits_group(ui, &mut settings.limits, "Elbows and knees bend about their hinge, the way the input bends them, never past what a human joint does. Wrists and ankles stay within their range. Twist joints take their share");
            reach_report(ui, anim, true);
            clip_summary(ui, anim, true);
        }
        NodeType::HumanLimits { limits } => {
            group(ui, "Inputs", Some("The first input put back within human limits, against the second: the same clip before something moved it (a retarget, Fix Pose, a blend). Both need the same skeleton"), |f| {
                if anim.input.is_none() { f.status(Status::Info, "Connect the moved clip to the first input, the clip before it to the second"); }
            });
            limits_group(ui, limits, "The Body Collide node's human pass on its own. Elbows and knees bend about their hinge, the way the reference bends them, never past what a human joint does. Wrists and ankles stay within their range. Twist joints take their share. A limit never makes a frame worse than the reference");
            reach_report(ui, anim, false);
            clip_summary(ui, anim, true);
        }
        NodeType::Capture { falloff } => {
            group(ui, "Capture", Some("Skin weights for every point of the geometry, by how near it is to each bone of the skeleton in its first frame: the four nearest, the nearest weighing most. Bones above the hips weigh nothing. The weights are point attributes, by joint name: skel:joint0 to skel:joint3 and skel:jointWeights"), |f| {
                f.slider("Falloff", Some("How fast a further bone's weight drops: higher gives a harder skin"), falloff, 1.0..=12.0, "");
                f.status(Status::Info, "Wire the geometry first, then the skeleton in the pose the mesh is in (Load FBX's Rest)");
            });
        }
        NodeType::Skin => {
            group(ui, "Skin", Some("The animation (third input) with the skin of the geometry (first input). The geometry's weights go by joint name, and bind where the rest skeleton (second input) has those joints. The same weights fit any animation whose joints have those names"), |f| {
                match anim.output.as_ref().and_then(crate::core::skin::report_for) {
                    Some(r) => {
                        f.value("Points", format!("{}", r.points));
                        f.value("Primitives", format!("{} weighted{}", r.weighted, if r.unweighted > 0 { format!(", {} without weights left out", r.unweighted) } else { String::new() }));
                        f.value("Joints", format!("{} found in the animation", r.matched));
                        if !r.missing.is_empty() {
                            f.status(Status::Bad, format!("{} joints not in the animation: {}{}", r.missing.len(), r.missing.iter().take(6).cloned().collect::<Vec<_>>().join(", "), if r.missing.len() > 6 { ", …" } else { "" }));
                        }
                        if r.not_in_rest > 0 { f.status(Status::Info, format!("{} joints of the animation are not in the rest skeleton: they bind where the animation rests", r.not_in_rest)); }
                    }
                    None => f.status(Status::Info, "Connect geometry with weights, the rest skeleton and an animation"),
                }
            });
            clip_summary(ui, anim, false);
        }
        NodeType::AutoTPose { set_hip_height, hip_height } => {
            group(ui, "Pose", Some("Rotations zeroed, the root at the origin, the hips centred. One frame, no animation"), |f| {
                let rest = anim.input.as_ref().and_then(|c| c.hip_joint().map(|h| (c.joints[h].name.clone(), c.joints[h].rest.translation.y * 100.0)));
                if let Some((name, h)) = &rest { f.value("Hips", format!("{name}, {h:.2} cm in the file")); }
                f.toggle("Set hip height", None, set_hip_height);
                f.row("Hip height", None, |ui| {
                    ui.add_enabled_ui(*set_hip_height, |ui| {
                        ui.horizontal(|ui| {
                            form::number(ui, None, hip_height, |d| d.speed(0.1).range(0.0..=300.0).suffix(" cm"));
                            if let Some((_, h)) = &rest { if ui.small_button("From file").clicked() { *hip_height = *h; } }
                        });
                    });
                });
            });
        }
        NodeType::FixPose { edits } => {
            let joints: Vec<String> = anim.input.as_ref().map(|c| c.joints.iter().map(|j| j.name.clone()).collect()).unwrap_or_default();
            let mut add = false;
            group_with(ui, "Corrections", Some("Added on every frame. Rotation in degrees, in the parent's space"), |ui| {
                if ui.small_button("+").on_hover_text("Add a correction").clicked() { add = true; }
            }, |f| { if edits.is_empty() { f.status(Status::Info, "None yet"); } });
            let mut remove = None;
            for (n, e) in edits.iter_mut().enumerate() {
                group_with(ui, &format!("Correction {}", n + 1), None, |ui| {
                    if ui.small_button("🗑").on_hover_text("Remove this correction").clicked() { remove = Some(n); }
                }, |f| {
                    pattern_rows(f, &format!("fix_joint{n}"), "Joints", &mut e.joint, &joints, false);
                    f.xyz("Rotate", None, &mut e.rotation, 0.5, "°");
                    f.xyz("Translate", None, &mut e.translation, 0.1, " cm");
                });
            }
            if let Some(n) = remove { edits.remove(n); }
            if add { edits.push(PoseEdit { joint: String::new(), rotation: [0.0; 3], translation: [0.0; 3] }); }
        }
        NodeType::ProxySkin { thickness } => group(ui, "Proxy", Some("A sphere per joint and a cylinder per bone, each bound to one bone, built in the pose of the first frame"), |f| {
            f.slider("Thickness", None, thickness, 0.25..=4.0, "");
            if let Some(skin) = anim.output.as_ref().and_then(|c| c.skin.as_ref()) {
                f.value("Mesh", format!("{} vertices, {} faces", skin.positions.len(), skin.faces.len()));
            }
        }),
        NodeType::Characterize { picks } => characterize(ui, picks, &anim.input),
        NodeType::Ragdoll { settings, view, limits } => body_collide(ui, settings, view, limits, &collide_in),

        // Modelling
        NodeType::EditPoly { ops, pending, edit, auto_collapse } => edit_poly(ui, ops, pending, edit, auto_collapse, io),

        // Organise
        NodeType::PassThrough => group(ui, "Pass Through", Some("Hands what comes in on to what follows, unchanged: for laying out a big graph. Rename it to say what passes through it"), |f| {
            f.status(Status::Info, "Nothing to set: what comes in goes out");
        }),
        NodeType::SetVariable { vars } => set_variable_rows(ui, vars),
        NodeType::Compound { .. } => {
            let (enter, back) = compound_rows(ui, &mut node.node_type, sel_id, io);
            enter_compound = enter;
            back_to_library = back;
        }
        NodeType::CompoundInput { name } => group(ui, "Port", Some("Inside a compound: what comes into the instance's input port of this name. The compound's ports are named by these operators when you leave it"), |f| {
            resync |= f.text("Name", Some("The input port's name on the compound"), name, "Scene").changed();
        }),
        NodeType::CompoundOutput { name } => group(ui, "Port", Some("Inside a compound: what goes out of the instance's output port of this name"), |f| {
            resync |= f.text("Name", Some("The output port's name on the compound"), name, "Scene").changed();
        }),
    }
    if enter_compound {
        graph.inside.request = Some(crate::node_graph::compound::Request::Enter(sel_id));
    }
    // Back to the library's definition: its ports, version and hash.
    if back_to_library {
        graph.refresh_compounds();
    }
    // Not for a row probed: its number is a stand-in for the moment (Merge's
    // 7,777,777 inputs would be made), put back after the panel.
    if resync && !form::probe_put() {
        graph.sync_sockets(sel_id);
    }
}

// Variables down the wires

/// Set Variable: its rows, a name and a value each, for every operator below it
/// as `{name}`.
fn set_variable_rows(ui: &mut egui::Ui, vars: &mut Vec<crate::core::vars::Variable>) {
    group(ui, "Variables", Some("Written {name} in the fields of every operator below this one, along the wires: the nearest wins. A value can hold variables: $ built-ins, the rows before it, and {name} from above"), |f| {
        let mut remove = None;
        let names: Vec<String> = vars.iter().map(|v| v.name.trim().to_lowercase()).collect();
        // A row reads the rows before it: their names and values.
        let mut earlier: Vec<(String, String)> = vec![];
        for (k, v) in vars.iter_mut().enumerate() {
            f.row(&format!("Variable {}", k + 1), None, |ui| {
                ui.horizontal(|ui| {
                    let h = ui.spacing().interact_size.y;
                    let w = ((ui.available_width() - 30.0 - ui.spacing().item_spacing.x) / 2.0).max(40.0);
                    crate::properties::form::typed(ui, &mut v.name, |ui, t| ui.field_sized([w, h], egui::TextEdit::singleline(t).hint_text("name").font(egui::TextStyle::Monospace)));
                    crate::properties::form::typed(ui, &mut v.value, |ui, t| {
    let r = ui.field_sized([w, h], egui::TextEdit::singleline(&mut *t).hint_text("value"));
    crate::variables::complete(ui, &r, t, &[]);
    r
});
                    if ui.small_button("🗑").on_hover_text("Remove this variable").clicked() { remove = Some(k); }
                });
            });
            let name = v.name.trim();
            if !name.is_empty() {
                if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    f.status(Status::Bad, "letters, digits and _ only");
                } else if names.iter().take(k).any(|n| *n == name.to_lowercase()) {
                    f.status(Status::Bad, "a variable above has this name: the first is used");
                } else {
                    let before: Vec<(&str, String)> = earlier.iter().map(|(n, v)| (n.as_str(), v.clone())).collect();
                    let shown = crate::core::vars::expand(v.value.trim(), &before);
                    f.status(Status::Info, format!("{{{}}} → {shown}", name.to_lowercase()));
                    earlier.push((name.to_lowercase(), shown));
                }
            }
        }
        if let Some(k) = remove { vars.remove(k); }
        f.buttons("Add", None, |ui| {
            if ui.button("+ Variable").clicked() {
                let n = (1..).map(|i| format!("var{i}")).find(|n| !names.contains(n)).unwrap_or_default();
                vars.push(crate::core::vars::Variable::new(&n, ""));
            }
        });
    });
}

/// The panel's tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Parameters,
    Locations,
    Variables,
}

/// The Graph Variables tab: the `{name}` that come down the wires to the
/// operator shown (name, value, the operator it is from), nearest first; then
/// those it gives the operators below. Empty: nothing.
fn graph_variables_tab(ui: &mut egui::Ui, above: &[(String, String, String)], own: &[(String, String, String)]) {
    let rows = |f: &mut Rows, list: &[(String, String, String)]| {
        for (name, value, from) in list {
            let shown = if value.is_empty() {
                "(empty)".to_string()
            } else {
                value.clone()
            };
            let hint = format!("From {from}");
            f.row(&format!("{{{name}}}"), Some(hint.as_str()), |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(format!("{shown}   ← {from}")).color(ink::VALUE())).truncate(),
                );
            });
        }
    };
    if !above.is_empty() {
        group(ui, "From Above", Some("Variables the operators above publish, read as {name} in this operator's fields: Write USD's {usd}, Render Settings' {delegate}, a Set Variable's own. The nearest wins"), |f| rows(f, above));
    }
    if !own.is_empty() {
        group(
            ui,
            "Given Below",
            Some("What this operator publishes for every operator below it, over any of the same name from above"),
            |f| rows(f, own),
        );
    }
}

// Compounds

/// A compound instance's rows, in the scene graph or an ICE graph: where its
/// definition is found (or its own, editable), Go Inside, Make Editable, Save
/// as Definition… (to `save`), Revert to Latest Definition, and an editable
/// one's name, category, icon, help and author. `missing`: what the instance
/// does while its definition cannot be found. Returns whether it asked to go
/// inside, and whether it went back to the library's definition.
pub fn definition_rows(
    ui: &mut egui::Ui,
    definition: &mut String,
    version: u32,
    editable: &mut Option<Box<crate::compounds::Definition>>,
    missing: &str,
    browser: &mut FileBrowser,
    save: BrowseTarget,
) -> (bool, bool) {
    let mut enter = false;
    let mut back = false;
    let library = crate::compounds::library();
    let found = library.get(definition).cloned();
    let editable_now = editable.is_some();
    group(
        ui,
        "Compound",
        Some("A piece of graph as one operator: double-click it, or Go Inside, to see the graph inside"),
        |f| {
            match (&*editable, &found) {
                (Some(_), _) => f.status(
                    Status::Info,
                    "Editable: its own copy of the definition, kept in this graph",
                ),
                (None, Some(fd)) => f.status(
                    Status::Ok,
                    format!(
                        "{} v{}, {} ({})",
                        fd.definition.name,
                        fd.definition.version,
                        fd.path.display(),
                        fd.source.label()
                    ),
                ),
                (None, None) => f.status(
                    Status::Bad,
                    format!("{definition} was not found in the library: {missing}. Its wires and values are kept"),
                ),
            }
            if !editable_now && found.as_ref().is_some_and(|fd| fd.definition.version != version) {
                f.status(Status::Info, format!("Saved with v{version}"));
            }
            f.buttons("", None, |ui| {
            if ui.button("Go Inside").on_hover_text("The graph inside it (double-click the operator, or ↓)").clicked() {
                enter = true;
            }
            if !editable_now
                && found.is_some()
                && ui
                    .button("Make Editable")
                    .on_hover_text("A copy of the definition for this instance alone, to change inside it; it no longer follows the library")
                    .clicked()
            {
                *editable = found.as_ref().map(|fd| Box::new(fd.definition.clone()));
            }
            if editable_now
                && ui
                    .button("Save as Definition…")
                    .on_hover_text("Write its definition as an .icd file in a library folder: this instance and others then follow it")
                    .clicked()
            {
                let name = editable.as_ref().map_or(String::from("Compound.icd"), |d| d.file_name());
                let user = crate::compounds::folders()
                    .into_iter()
                    .find(|(_, s)| *s == crate::compounds::Source::User)
                    .map(|(d, _)| d);
                if let Some(dir) = user {
                    let _ = std::fs::create_dir_all(&dir);
                    browser.set_dir(&dir);
                }
                browser.open(save, BrowseMode::Save, "Save as Definition", &["icd"], &name);
            }
            // Editable, with a definition of its name in the library: back to
            // it, locked, its own copy dropped.
            if editable_now
                && found.is_some()
                && ui
                    .button("Revert to Latest Definition")
                    .on_hover_text("Drop this instance's own copy: it follows the library's definition of its name again, locked")
                    .clicked()
            {
                *editable = None;
                back = true;
            }
        });
        },
    );
    if let Some(d) = editable.as_deref_mut() {
        group(
            ui,
            "Definition",
            Some("What the library and the add menu call it"),
            |f| {
                if f.text("Name", Some("Instances find it by this name"), &mut d.name, "Compound")
                    .changed()
                {
                    *definition = d.name.clone();
                }
                f.text(
                    "Category",
                    Some("Its group in the add menu's Compounds"),
                    &mut d.category,
                    "",
                );
                f.text("Icon", Some("Shown before its name in the add menu"), &mut d.icon, "◆");
                f.text("Help", Some("What it does: the add menu's tooltip"), &mut d.help, "");
                f.text("Author", None, &mut d.author, "");
                f.value("Version", d.version.to_string());
            },
        );
    }
    (enter, back)
}

/// A compound instance's parameters: its definition's rows
/// (`definition_rows`), and its promoted parameters. Returns whether it asked
/// to go inside, and whether it went back to the library's definition.
fn compound_rows(ui: &mut egui::Ui, t: &mut NodeType, id: NodeId, io: &mut PanelIo) -> (bool, bool) {
    let NodeType::Compound {
        definition,
        version,
        editable,
        ..
    } = &mut *t
    else {
        return (false, false);
    };
    let (enter, back) = definition_rows(
        ui,
        definition,
        *version,
        editable,
        "what comes into its first input goes on",
        io.browser,
        BrowseTarget::SaveDefinition(id),
    );
    // Its promoted parameters, by group.
    let params = crate::node_graph::compound::with_definition(&*t, |d| d.parameters.clone()).unwrap_or_default();
    let NodeType::Compound { values, .. } = t else {
        return (enter, back);
    };
    let mut groups: Vec<String> = vec![];
    for p in &params {
        if !groups.contains(&p.group) {
            groups.push(p.group.clone());
        }
    }
    for g in groups {
        let title = if g.is_empty() {
            "Parameters".to_string()
        } else {
            g.clone()
        };
        group(ui, &title, None, |f| {
            for p in params.iter().filter(|p| p.group == g) {
                promoted_row(f, p, values);
            }
        });
    }
    (enter, back)
}

/// One promoted parameter's row: its value, the instance's, else the default.
/// A value set back to the default is the default again, not a value of its
/// own.
fn promoted_row(
    f: &mut Rows,
    p: &crate::compounds::Parameter,
    values: &mut std::collections::BTreeMap<String, serde_json::Value>,
) {
    use crate::compounds::ParameterType as T;
    let label = if p.label.is_empty() {
        p.name.as_str()
    } else {
        p.label.as_str()
    };
    let hint = (!p.help.is_empty()).then_some(p.help.as_str());
    let now = values.get(&p.name).cloned().unwrap_or_else(|| p.default.clone());
    let changed: Option<serde_json::Value> = match p.kind {
        T::Float => {
            let mut x = now.as_f64().unwrap_or(0.0) as f32;
            let r = match p.range {
                Some([lo, hi]) => f.slider(label, hint, &mut x, lo as f32..=hi as f32, ""),
                None => f.drag(label, hint, &mut x, 0.01, ""),
            };
            r.changed().then(|| serde_json::json!(x))
        }
        T::Integer => {
            let mut x = now.as_i64().unwrap_or(0) as f32;
            let r = f.drag(label, hint, &mut x, 1.0, "");
            r.changed().then(|| serde_json::json!(x.round() as i64))
        }
        T::Boolean => {
            let mut b = now.as_bool().unwrap_or(false);
            f.toggle(label, hint, &mut b).changed().then(|| serde_json::json!(b))
        }
        T::String => {
            let mut s = now.as_str().unwrap_or_default().to_string();
            f.text_vars(label, hint, &mut s, "", false)
                .changed()
                .then(|| serde_json::json!(s))
        }
        T::Vector2 | T::Vector3 | T::Vector4 | T::Colour => {
            let n = match p.kind {
                T::Vector2 => 2,
                T::Vector4 => 4,
                _ => 3,
            };
            let mut v: Vec<f64> = match &now {
                serde_json::Value::Array(a) => a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect(),
                x => vec![x.as_f64().unwrap_or(0.0)],
            };
            v.resize(n, 0.0);
            let mut changed = false;
            f.row(label, hint, |ui| {
                ui.horizontal(|ui| {
                    for x in v.iter_mut() {
                        changed |= form::number(ui, None, x, |d| d.speed(0.01)).changed();
                    }
                });
            });
            changed.then(|| serde_json::json!(v))
        }
        T::Ramp => {
            f.value(label, "a ramp: its row comes with Ramp parameters");
            None
        }
    };
    if let Some(v) = changed {
        if v == p.default {
            values.remove(&p.name);
        } else {
            values.insert(p.name.clone(), v);
        }
    }
}

// Characterize

fn characterize(ui: &mut egui::Ui, picks: &mut crate::core::human::Picks, input: &Option<Arc<AnimData>>) {
    use crate::core::human::{Human, Slot};
    let Some(clip) = input else {
        group(ui, "Skeleton", None, |f| f.status(Status::Info, "Connect a clip"));
        return;
    };
    let h = Human::detect(&clip.joints, picks);
    let mut clear = false;
    group_with(ui, "Skeleton", Some("Each part is read from the joint names and the hierarchy. Pick a joint to set a part by hand; Auto goes back to the names. The spine is what lies between the hips and the chest, the neck what lies between the chest and the head"), |ui| {
        if !picks.is_empty() && ui.small_button("Auto all").on_hover_text("Forget every pick").clicked() { clear = true; }
    }, |f| {
        f.value("Convention", h.convention);
        f.value("Found", format!("{} of 20 parts{}", h.found(), if picks.is_empty() { String::new() } else { format!(", {} set by hand", h.picked.len()) }));
        if h.usable() { f.status(Status::Ok, "Body Collide and Retarget can use it"); }
        else { f.status(Status::Bad, "Hips and at least one arm or leg are needed"); }
    });
    if clear {
        picks.clear();
    }
    let bones: Vec<&str> = clip
        .joints
        .iter()
        .filter(|j| j.is_bone)
        .map(|j| j.name.as_str())
        .collect();
    let name = |j: Option<usize>| j.map(|j| clip.joints[j].name.clone());
    let chain = |v: &[usize]| match v {
        [] => "None".to_string(),
        [a] => clip.joints[*a].name.clone(),
        [a, .., b] => format!("{} to {} ({})", clip.joints[*a].name, clip.joints[*b].name, v.len()),
    };
    let twists = |v: &[crate::core::human::Twist]| {
        if v.is_empty() {
            "None".to_string()
        } else {
            v.iter()
                .map(|t| {
                    format!(
                        "{} ({})",
                        clip.joints[t.joint].name,
                        if t.in_line { "in line" } else { "beside" }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let mut set: Option<(Slot, Option<String>)> = None;
    let mut slot_row = |f: &mut Rows, s: Slot| {
        let picked = picks.iter().rev().find(|p| p.0 == s).map(|p| p.1.clone());
        let shown = match (&picked, name(h.get(s))) {
            (Some(p), _) if p.is_empty() => "None".to_string(),
            (_, Some(n)) => n,
            (_, None) => "None".to_string(),
        };
        let id = f.ui.id().with(("slot", s));
        f.row(s.label(), None, |ui| {
            let text = egui::RichText::new(&shown).color(if picked.is_some() { ink::VALUE() } else { ink::DIM() });
            egui::ComboBox::from_id_salt(id)
                .width(ui.available_width() - 8.0)
                .selected_text(text)
                .height(320.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(picked.is_none(), "Auto, from the names").clicked() {
                        set = Some((s, None));
                    }
                    if ui.selectable_label(picked.as_deref() == Some(""), "None").clicked() {
                        set = Some((s, Some(String::new())));
                    }
                    ui.separator();
                    for b in &bones {
                        if ui.selectable_label(picked.as_deref() == Some(*b), *b).clicked() {
                            set = Some((s, Some(b.to_string())));
                        }
                    }
                });
        });
    };
    group(ui, "Trunk", None, |f| {
        slot_row(f, Slot::Hips);
        f.value("Spine", chain(&h.spine));
        slot_row(f, Slot::Chest);
        slot_row(f, Slot::Neck);
        f.value("Neck chain", chain(&h.neck));
        slot_row(f, Slot::Head);
    });
    for (k, title) in ["Left arm", "Right arm", "Left leg", "Right leg"].iter().enumerate() {
        let limb = &h.limbs[k];
        let side = if limb.left {
            crate::core::human::Side::Left
        } else {
            crate::core::human::Side::Right
        };
        use crate::core::human::Part;
        let parts: &[Part] = if limb.arm {
            &[Part::Clavicle, Part::UpperArm, Part::LowerArm, Part::Hand]
        } else {
            &[Part::Thigh, Part::Calf, Part::Foot, Part::Toe]
        };
        group(
            ui,
            title,
            Some("Twist joints are found by name: in line, between two main joints, or beside the limb"),
            |f| {
                for p in parts {
                    slot_row(f, Slot::of(*p, side).unwrap());
                }
                f.value(
                    if limb.arm { "Arm twist" } else { "Thigh twist" },
                    twists(&limb.upper_twist),
                );
                f.value(
                    if limb.arm { "Forearm twist" } else { "Calf twist" },
                    twists(&limb.mid_twist),
                );
            },
        );
    }
    if let Some((s, v)) = set {
        picks.retain(|p| p.0 != s);
        if let Some(v) = v {
            picks.push((s, v));
        }
    }
}

// Body Collide

fn body_collide(
    ui: &mut egui::Ui,
    settings: &mut crate::ragdoll::Settings,
    view: &mut BodyView,
    limits: &mut crate::core::human_ik::Limits,
    inputs: &(Option<Arc<AnimData>>, Option<Arc<crate::types::MeshData>>),
) {
    let (clip, collider) = inputs;
    group(ui, "Inputs", Some("The character follows the capture and is kept out of the collider and out of itself. Joints give way, each as far as that joint may. No bone changes length"), |f| {
        match clip {
            None => f.status(Status::Info, "Connect a clip with a human skeleton to the first input"),
            Some(c) => {
                f.value("Clip", format!("{} frames, {}", c.frames, match &c.skin {
                    Some(s) => format!("skin of {} vertices", s.positions.len()), None => "no skin: capsules stand in".into() }));
                let h = crate::core::human::Human::of(c);
                f.value("Skeleton", format!("{}, {} of 20 parts", h.convention, h.found()));
                if !h.usable() { f.status(Status::Bad, "Hips and limbs not found: put a Characterize node before this one"); }
            }
        }
        match collider {
            Some(m) => f.value("Collider", format!("{} triangles", m.indices.len() / 3)),
            None => f.value("Collider", "None: the character collides with itself only"),
        }
    });
    group(
        ui,
        "Display",
        Some("What the Viewer draws of the character. The node puts out the clip with its skin either way"),
        |f| {
            f.choice(
                "Show",
                Some("Pieces: the skin cut into one rigid piece per body. Hulls: the shapes the solver collides"),
                view,
                &[
                    (BodyView::Skin, "Skin"),
                    (BodyView::Pieces, "Pieces"),
                    (BodyView::Hulls, "Hulls"),
                ],
            );
        },
    );
    group(ui, "Contact", None, |f| {
        f.slider(
            "Margin",
            Some("Soft margin kept between the character and a surface"),
            &mut settings.margin,
            0.0..=5.0,
            " cm",
        );
        f.slider("Friction", None, &mut settings.friction, 0.0..=2.0, "");
        f.slider(
            "Release speed",
            Some("How fast a limb may be pushed out of a surface"),
            &mut settings.release,
            5.0..=300.0,
            " cm/s",
        );
        f.toggle(
            "Self collision",
            Some("Keep the character out of itself"),
            &mut settings.self_collision,
        );
        f.row("Self overlap", Some("How far its parts may overlap"), |ui| {
            ui.add_enabled_ui(settings.self_collision, |ui| {
                form::slider_look(ui);
                form::slider(ui, &mut settings.self_slack, |v| {
                    egui::Slider::new(v, 0.0..=10.0).suffix(" cm")
                });
            });
        });
        f.choice(
            "Hull detail",
            Some("How closely the convex hulls follow the skin. Also what Display: Hulls shows"),
            &mut settings.detail,
            &[(0, "Coarse"), (1, "Medium"), (2, "Fine")],
        );
    });
    group(
        ui,
        "Capture",
        Some("How closely the character follows the capture"),
        |f| {
            f.slider("Stiffness", None, &mut settings.stiffness, 0.25..=3.0, "");
            f.slider(
                "Limb release",
                Some("A limb further than this from the capture is let go"),
                &mut settings.limb_limit,
                5.0..=100.0,
                " cm",
            );
            f.slider_u32(
                "Smoothing",
                Some("Frames on each side over which the result is evened out"),
                &mut settings.smooth,
                0..=8,
                "",
            );
        },
    );
    group(ui, "Trunk", Some("Hips and spine sunk in a seat are lifted onto it, up to Max lift. Deeper than that, they are lifted that far and left in by the rest"), |f| {
        f.slider("Max lift", None, &mut settings.ghost_depth, 1.0..=40.0, " cm");
        f.slider("Rest depth", Some("How far the trunk may rest in a surface"), &mut settings.sink, 0.0..=15.0, " cm");
    });
    group(ui, "Pass through", Some("Where the capture takes the trunk through a surface, as when an actor walks through a door that was not there on the day, the character follows the capture with collisions off, and is caught again after"), |f| {
        f.slider_u32("Fade out", Some("Frames"), &mut settings.fade_out, 1..=60, "");
        f.slider_u32("Fade in", Some("Frames"), &mut settings.fade_in, 1..=60, "");
    });
    group(ui, "Human limits", Some("The solved clip goes through a human IK pass. Elbows and knees bend about their hinge, the way the capture bends them, never past 150 degrees, pointing near where the capture points them. Wrists and ankles stay within their range: flexion either way, much less from side to side. Twist joints, in line or beside the limb, take their share. Changing these does not solve again"), |f| {
        f.toggle("On", None, &mut limits.on);
        f.enabled(limits.on, |f| {
            f.slider("Swivel", Some("Furthest an elbow or a knee may point away from where it points in the capture"), &mut limits.swivel, 0.0..=90.0, "°");
            f.slider("Wrist and ankle", Some("Furthest a hand or a foot may turn away from its capture"), &mut limits.give, 0.0..=90.0, "°");
        });
        if let (Some(c), true) = (clip, limits.on) {
            let key = crate::ragdoll::key(c, collider.as_ref(), settings);
            if let Some((k, r)) = *crate::ragdoll::last_report().lock().unwrap() {
                if k == key {
                    f.value("Elbows, knees", format!("bent the wrong way in {} of {} frames, {} after", r.flips_before, r.frames, r.flips_after));
                    f.value("Wrists, ankles", format!("past their range in {} frames, {} after", r.ends_before, r.ends_after));
                }
            }
        }
    });
    group(ui, "Solve", None, |f| {
        let Some(c) = clip else {
            f.status(Status::Info, "Nothing to solve");
            return;
        };
        let key = crate::ragdoll::key(c, collider.as_ref(), settings);
        let job = crate::ragdoll::job(key).or_else(|| {
            crate::ragdoll::solved(key);
            crate::ragdoll::job(key)
        });
        let start = |c: &Arc<AnimData>| crate::ragdoll::start(c.clone(), collider.clone(), *settings);
        let Some(job) = job else {
            f.status(
                Status::Info,
                "Not solved for these inputs and settings: the clip passes through as it came",
            );
            if f.buttons("", None, |ui| ui.button("Solve").clicked()) {
                start(c);
            }
            return;
        };
        let notes = job.notes.lock().unwrap().clone();
        if !notes.is_empty() {
            f.status(Status::Info, notes);
        }
        let state = job.state.lock().unwrap();
        match &*state {
            crate::ragdoll::State::Running => {
                let done = job.done.load(std::sync::atomic::Ordering::Relaxed);
                let secs = job.started.elapsed().as_secs_f32();
                let rate = done as f32 / secs.max(0.01);
                let left = if rate > 0.0 {
                    (job.total.saturating_sub(done)) as f32 / rate
                } else {
                    0.0
                };
                f.row("Progress", None, |ui| {
                    ui.add(
                        egui::ProgressBar::new(done as f32 / job.total.max(1) as f32)
                            .text(format!("{done} of {}, {left:.0} s left", job.total)),
                    )
                });
                drop(state);
                if f.buttons("", None, |ui| ui.button("Stop").clicked()) {
                    crate::ragdoll::cancel(key);
                }
                f.ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
            crate::ragdoll::State::Done(s) => {
                let r = s.report.clone();
                f.status(Status::Ok, format!("Solved {} frames in {:.1} s", s.frames, s.seconds));
                drop(state);
                f.value("In contact", format!("{} frames", r.contact_frames));
                f.value(
                    "Furthest",
                    format!("{:.1} cm from the capture", r.max_deviation * 100.0),
                );
                f.value(
                    "Deepest",
                    format!("{:.1} cm left in the collider", r.max_residual * 100.0),
                );
                if r.ghost_ranges.is_empty() {
                    f.value("Passed through", "Never: collisions were on throughout");
                } else {
                    f.value(
                        "Passed through",
                        format!(
                            "{} frames in {} stretch{}",
                            r.ghost_frames,
                            r.ghost_ranges.len(),
                            if r.ghost_ranges.len() == 1 { "" } else { "es" }
                        ),
                    );
                    f.row("", None, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("collide_ghosts")
                            .max_height(110.0)
                            .show(ui, |ui| {
                                for (a, b) in &r.ghost_ranges {
                                    ui.colored_label(
                                        ink::DIM(),
                                        egui::RichText::new(format!(
                                            "{} to {}  (frames {} to {})",
                                            c.timecode(c.start_frame + *a as i64),
                                            c.timecode(c.start_frame + *b as i64),
                                            a,
                                            b
                                        ))
                                        .monospace()
                                        .small(),
                                    );
                                }
                            });
                    });
                }
                f.buttons("", None, |ui| {
                    if ui.button("Solve again").clicked() {
                        start(c);
                    }
                    if ui
                        .button("Forget")
                        .on_hover_text("Remove the solved result, from memory and from disk")
                        .clicked()
                    {
                        crate::ragdoll::clear(key);
                    }
                });
            }
            crate::ragdoll::State::Failed(e) => {
                let e = e.clone();
                drop(state);
                f.status(Status::Bad, e);
                if f.buttons("", None, |ui| ui.button("Solve").clicked()) {
                    start(c);
                }
            }
            crate::ragdoll::State::Cancelled => {
                drop(state);
                f.status(Status::Info, "Stopped: the clip passes through as it came");
                if f.buttons("", None, |ui| ui.button("Solve").clicked()) {
                    start(c);
                }
            }
        }
    });
}

// Edit Poly

fn edit_poly(
    ui: &mut egui::Ui,
    ops: &mut Vec<PolyOp>,
    pending: &mut PolySelection,
    edit: &mut Option<usize>,
    auto_collapse: &mut bool,
    io: &mut PanelIo,
) {
    if edit.map(|i| i >= ops.len() || ops[i].collapsed).unwrap_or(false) {
        *edit = None;
    }
    let input = io.poly_input.clone();
    // Mesh the selection under edit applies to, and the final mesh.
    let stage = input
        .as_ref()
        .map(|m| crate::core::poly::eval_cached_seeded(&m.mesh, m.seed, ops, edit.unwrap_or(ops.len())));
    let end = input
        .as_ref()
        .map(|m| crate::core::poly::eval_cached_seeded(&m.mesh, m.seed, ops, ops.len()));

    let sel_title = match *edit {
        Some(i) => format!("Selection of #{} {}", i + 1, ops[i].kind.label()),
        None => "Selection".into(),
    };
    let mut done = false;
    group_with(
        ui,
        &sel_title,
        Some("The selection the next operation uses, built in the Viewer or by rule"),
        |ui| {
            if edit.is_some()
                && ui
                    .small_button("Done")
                    .on_hover_text("Back to the selection for the next operation")
                    .clicked()
            {
                done = true;
            }
        },
        |f| {
            if edit.is_some() {
                f.status(Status::Info, "The Viewer shows the mesh before this operation");
            }
            let sel = match *edit {
                Some(i) if i < ops.len() => &mut ops[i].selection,
                _ => &mut *pending,
            };
            selection_rows(f, sel, stage.as_deref());
            if input.is_none() {
                f.status(Status::Info, "No mesh connected");
            }
        },
    );
    if done {
        *edit = None;
    }

    group(
        ui,
        "Viewer tool",
        Some("Drag a handle in the Viewer: each drag is stored as a Transform operation"),
        |f| {
            let mut t = *io.tool;
            f.choice(
                "Tool",
                Some("Keys Q, W, E, R"),
                &mut t,
                &[
                    (Tool::Select, "Select"),
                    (Tool::Move, "Move"),
                    (Tool::Rotate, "Rotate"),
                    (Tool::Scale, "Scale"),
                ],
            );
            *io.tool = t;
        },
    );

    group(ui, "Add operation", Some("Each uses the selection above"), |f| {
        let base = pending.level.base();
        let (v, e, p) = (
            base == SubLevel::Vertex,
            base == SubLevel::Edge,
            base == SubLevel::Polygon,
        );
        let mut add: Option<PolyOpKind> = None;
        let mut line = |f: &mut Rows, title: &str, items: &[(&str, bool, PolyOpKind)]| {
            f.buttons(title, None, |ui| {
                for (label, on, kind) in items {
                    if ui.add_enabled(*on, egui::Button::new(*label)).clicked() {
                        add = Some(kind.clone());
                    }
                }
            });
        };
        line(
            f,
            "Polygon",
            &[
                (
                    "Extrude",
                    true,
                    PolyOpKind::Extrude {
                        height: 0.25,
                        mode: ExtrudeMode::Group,
                    },
                ),
                (
                    "Bevel",
                    true,
                    PolyOpKind::Bevel {
                        height: 0.25,
                        outline: -0.1,
                        mode: ExtrudeMode::Group,
                    },
                ),
                (
                    "Inset",
                    true,
                    PolyOpKind::Inset {
                        amount: 0.1,
                        by_polygon: false,
                    },
                ),
                ("Outline", true, PolyOpKind::Outline { amount: 0.05 }),
                (
                    "Hinge",
                    true,
                    PolyOpKind::Hinge {
                        angle: 30.0,
                        segments: 4,
                        edge: 0,
                    },
                ),
                ("Bridge", p || e, PolyOpKind::Bridge),
                ("Flip", true, PolyOpKind::Flip),
                ("Detach", true, PolyOpKind::Detach),
                ("Tessellate", true, PolyOpKind::Tessellate),
                ("Triangulate", true, PolyOpKind::Triangulate),
            ],
        );
        line(
            f,
            "Edge",
            &[
                (
                    "Extrude",
                    e,
                    PolyOpKind::ExtrudeEdge {
                        height: 0.0,
                        width: 0.25,
                    },
                ),
                ("Chamfer", true, PolyOpKind::Chamfer { amount: 0.05 }),
                ("Connect", e || v, PolyOpKind::Connect { segments: 1 }),
                ("Remove", e || v, PolyOpKind::Remove { clean: true }),
                ("Cap", true, PolyOpKind::Cap),
                ("Turn", e, PolyOpKind::Turn),
            ],
        );
        line(
            f,
            "Vertex",
            &[
                (
                    "Extrude",
                    true,
                    PolyOpKind::ExtrudeVertex {
                        height: 0.2,
                        width: 0.1,
                    },
                ),
                ("Weld", true, PolyOpKind::Weld { threshold: 0.01 }),
                ("Collapse", true, PolyOpKind::Collapse),
                ("Break", true, PolyOpKind::Break),
            ],
        );
        line(
            f,
            "Any",
            &[
                ("Transform", true, PolyOpKind::identity_transform()),
                ("Delete", true, PolyOpKind::Delete),
                ("Make planar", true, PolyOpKind::MakePlanar { axis: None }),
                (
                    "Relax",
                    true,
                    PolyOpKind::Relax {
                        amount: 0.5,
                        iterations: 1,
                        hold_border: true,
                    },
                ),
                ("Slice", true, PolyOpKind::Slice { axis: 1, offset: 0.0 }),
                ("Insert vertex", true, PolyOpKind::InsertVertex { segments: 1 }),
            ],
        );
        line(
            f,
            "Whole mesh",
            &[("Subdivide", true, PolyOpKind::Subdivide { iterations: 1 })],
        );
        if let Some(kind) = add {
            let selection = pending.clone();
            let level = pending.level;
            if let Some(m) = &end {
                let before = m.polys.len();
                let op = PolyOp::new(selection.clone(), kind.clone());
                *pending = match &kind {
                    // The same polygons stay selected, as in Edit Poly.
                    PolyOpKind::Extrude { .. } | PolyOpKind::Bevel { .. } | PolyOpKind::Inset { .. } => {
                        let mask = selection.poly_mask(m);
                        PolySelection {
                            level,
                            ..PolySelection::picked_polys(
                                (0..mask.len() as u32).filter(|p| mask[*p as usize]).collect(),
                            )
                        }
                    }
                    k if k.keeps_indices() => selection.clone(),
                    // The new caps.
                    PolyOpKind::Cap => {
                        let mut after = (**m).clone();
                        op.apply(&mut after);
                        PolySelection::picked_polys((before as u32..after.polys.len() as u32).collect())
                    }
                    _ => PolySelection {
                        level,
                        ..Default::default()
                    },
                };
            }
            push_op(ops, *auto_collapse, PolyOp::new(selection, kind));
            *edit = None;
        }
    });

    let live = ops.iter().filter(|op| !op.collapsed).count();
    let mut collapse_everything = false;
    group_with(
        ui,
        "Operations",
        None,
        |ui| {
            if ui
                .add_enabled(live > 0, egui::Button::new("Collapse all").small())
                .clicked()
            {
                collapse_everything = true;
            }
        },
        |f| {
            f.choice("Mode", Some("Keep live: operations stay editable until collapsed. Auto-collapse: adding one collapses those before it"),
            auto_collapse, &[(false, "Keep live"), (true, "Auto-collapse")]);
            f.value("Count", format!("{} live, {} collapsed", live, ops.len() - live));
            if let Some(m) = &end {
                f.value(
                    "Result",
                    format!("{} vertices, {} polygons", m.verts.len(), m.polys.len()),
                );
            }
        },
    );
    if collapse_everything {
        collapse_all(ops);
        *edit = None;
    }

    enum ListEdit {
        Up(usize),
        Down(usize),
        Delete(usize),
        Collapse(usize),
        Restore(usize),
    }
    let mut change = None;
    let count = ops.len();
    let mut i = 0;
    while i < count {
        if ops[i].collapsed {
            let mut j = i;
            while j + 1 < count && ops[j + 1].collapsed {
                j += 1;
            }
            let n = j - i + 1;
            let title = if n == 1 {
                format!("#{} {}  (collapsed)", i + 1, ops[i].kind.label())
            } else {
                format!("#{} to #{}  ({n} collapsed)", i + 1, j + 1)
            };
            let names = ops[i..=j].iter().map(|o| o.kind.label()).collect::<Vec<_>>().join(", ");
            group_with(
                ui,
                &title,
                Some(&names),
                |ui| {
                    if ui
                        .small_button("Restore")
                        .on_hover_text("Make editable again")
                        .clicked()
                    {
                        change = Some(ListEdit::Restore(i));
                    }
                },
                |_| {},
            );
            i = j + 1;
            continue;
        }
        let title = format!("#{} {}", i + 1, ops[i].kind.label());
        let op = &mut ops[i];
        group_with(
            ui,
            &title,
            None,
            |ui| {
                if ui.small_button("🗑").on_hover_text("Remove").clicked() {
                    change = Some(ListEdit::Delete(i));
                }
                if ui.small_button("✔").on_hover_text("Collapse").clicked() {
                    change = Some(ListEdit::Collapse(i));
                }
                if ui
                    .add_enabled(i + 1 < count, egui::Button::new("⏷").small())
                    .on_hover_text("Later")
                    .clicked()
                {
                    change = Some(ListEdit::Down(i));
                }
                if ui
                    .add_enabled(i > 0, egui::Button::new("⏶").small())
                    .on_hover_text("Earlier")
                    .clicked()
                {
                    change = Some(ListEdit::Up(i));
                }
                if ui
                    .selectable_label(*edit == Some(i), "🎯")
                    .on_hover_text("Edit this operation's selection")
                    .clicked()
                {
                    *edit = if *edit == Some(i) { None } else { Some(i) };
                }
                form::check(ui, &mut op.enabled).on_hover_text("On");
            },
            |f| op_rows(f, i, &mut op.kind),
        );
        i += 1;
    }
    match change {
        Some(ListEdit::Up(i)) => {
            ops.swap(i, i - 1);
            *edit = None;
        }
        Some(ListEdit::Down(i)) => {
            ops.swap(i, i + 1);
            *edit = None;
        }
        Some(ListEdit::Delete(i)) => {
            ops.remove(i);
            *edit = None;
        }
        Some(ListEdit::Collapse(i)) => {
            collapse_op(ops, i);
            *edit = None;
        }
        Some(ListEdit::Restore(i)) => {
            restore_run(ops, i);
        }
        None => {}
    }
}

/// The parameters of one Edit Poly operation.
fn op_rows(f: &mut Rows, i: usize, kind: &mut PolyOpKind) {
    let modes = [
        (ExtrudeMode::Group, "Group"),
        (ExtrudeMode::LocalNormal, "Local normal"),
        (ExtrudeMode::ByPolygon, "By polygon"),
    ];
    let mode_row = |f: &mut Rows, mode: &mut ExtrudeMode| {
        let id = f.ui.id().with(("poly_mode", i));
        f.row("Mode", None, |ui| form::choice_widget(ui, id, mode, &modes));
    };
    match kind {
        PolyOpKind::Extrude { height, mode } => {
            f.drag("Height", None, height, 0.005, "");
            mode_row(f, mode);
        }
        PolyOpKind::Bevel { height, outline, mode } => {
            f.drag("Height", None, height, 0.005, "");
            f.drag("Outline", None, outline, 0.005, "");
            mode_row(f, mode);
        }
        PolyOpKind::Inset { amount, by_polygon } => {
            f.drag("Amount", None, amount, 0.005, "");
            f.toggle("By polygon", None, by_polygon);
        }
        PolyOpKind::Transform {
            translate,
            rotate,
            scale,
            falloff,
        } => {
            f.xyz("Translate", None, translate, 0.01, "");
            // Shown and edited as XYZ angles in degrees.
            let q = Quat::from_array(*rotate).normalize();
            let (x, y, z) = q.to_euler(EulerRot::XYZ);
            let mut deg = [x.to_degrees(), y.to_degrees(), z.to_degrees()];
            if f.xyz("Rotate", None, &mut deg, 0.5, "°") {
                *rotate = Quat::from_euler(
                    EulerRot::XYZ,
                    deg[0].to_radians(),
                    deg[1].to_radians(),
                    deg[2].to_radians(),
                )
                .to_array();
            }
            f.xyz("Scale", None, scale, 0.01, "");
            f.drag(
                "Soft falloff",
                Some("Soft selection: vertices within this distance of the selection follow part of the way. 0 is off"),
                falloff,
                0.01,
                "",
            );
            *falloff = falloff.max(0.0);
        }
        PolyOpKind::Chamfer { amount } | PolyOpKind::Outline { amount } => {
            f.drag("Amount", None, amount, 0.005, "");
        }
        PolyOpKind::ExtrudeVertex { height, width } | PolyOpKind::ExtrudeEdge { height, width } => {
            f.drag("Height", None, height, 0.005, "");
            f.drag("Width", None, width, 0.005, "");
        }
        PolyOpKind::Hinge { angle, segments, edge } => {
            f.drag("Angle", None, angle, 0.5, "°");
            *angle = angle.clamp(-360.0, 360.0);
            f.drag_u32("Segments", None, segments, 1..=64);
            f.drag_u32(
                "Hinge edge",
                Some("Which edge of the selection's outline is the hinge"),
                edge,
                0..=9999,
            );
        }
        PolyOpKind::Slice { axis, offset } => {
            f.choice("Across", None, axis, &[(0, "X"), (1, "Y"), (2, "Z")]);
            f.drag("At", None, offset, 0.005, "");
        }
        PolyOpKind::InsertVertex { segments } => {
            f.drag_u32("Per edge", None, segments, 1..=64);
        }
        PolyOpKind::Remove { clean } => {
            f.toggle("Clean up", Some("Also remove the vertices left over"), clean);
        }
        PolyOpKind::Weld { threshold } => {
            f.drag("Threshold", None, threshold, 0.001, "");
        }
        PolyOpKind::Connect { segments } => {
            f.drag_u32("Segments", None, segments, 1..=64);
        }
        PolyOpKind::MakePlanar { axis } => {
            f.choice(
                "Plane",
                None,
                axis,
                &[(None, "Best fit"), (Some(0), "X"), (Some(1), "Y"), (Some(2), "Z")],
            );
        }
        PolyOpKind::Relax {
            amount,
            iterations,
            hold_border,
        } => {
            f.drag("Amount", None, amount, 0.005, "");
            f.drag_u32("Iterations", None, iterations, 1..=200);
            f.toggle("Hold borders", Some("Open borders stay where they are"), hold_border);
        }
        PolyOpKind::Subdivide { iterations } => {
            f.drag_u32("Iterations", None, iterations, 1..=4);
        }
        PolyOpKind::Delete
        | PolyOpKind::Collapse
        | PolyOpKind::Cap
        | PolyOpKind::Bridge
        | PolyOpKind::Detach
        | PolyOpKind::Break
        | PolyOpKind::Flip
        | PolyOpKind::Tessellate
        | PolyOpKind::Triangulate
        | PolyOpKind::Turn => {}
    }
}

/// Sub-object level, where the selection comes from, and its modifiers.
fn selection_rows(f: &mut Rows, sel: &mut PolySelection, mesh: Option<&PolyMesh>) {
    f.choice(
        "Level",
        None,
        &mut sel.level,
        &[
            (SubLevel::Vertex, "Vertex"),
            (SubLevel::Edge, "Edge"),
            (SubLevel::Border, "Border"),
            (SubLevel::Polygon, "Polygon"),
            (SubLevel::Element, "Element"),
        ],
    );
    #[derive(Clone, Copy, PartialEq)]
    enum Src {
        Picked,
        All,
        Normal,
        Box,
    }
    let mut src = match sel.source {
        SelSource::Picked => Src::Picked,
        SelSource::All => Src::All,
        SelSource::ByNormal { .. } => Src::Normal,
        SelSource::InBox { .. } => Src::Box,
    };
    if f.choice(
        "Source",
        Some("Picked: click or drag a box in the Viewer, Ctrl adds, Shift removes"),
        &mut src,
        &[
            (Src::Picked, "Picked"),
            (Src::All, "All"),
            (Src::Normal, "By normal"),
            (Src::Box, "In box"),
        ],
    ) {
        sel.source = match src {
            Src::Picked => SelSource::Picked,
            Src::All => SelSource::All,
            Src::Normal => SelSource::ByNormal {
                dir: [0.0, 1.0, 0.0],
                angle: 45.0,
            },
            Src::Box => {
                // Start with a box around the whole mesh.
                let (mut lo, mut hi) = ([-1.0f32; 3], [1.0f32; 3]);
                if let Some(m) = mesh.filter(|m| !m.verts.is_empty()) {
                    lo = [f32::MAX; 3];
                    hi = [f32::MIN; 3];
                    for v in &m.verts {
                        for a in 0..3 {
                            lo[a] = lo[a].min(v[a]);
                            hi[a] = hi[a].max(v[a]);
                        }
                    }
                }
                SelSource::InBox { min: lo, max: hi }
            }
        };
    }
    match &mut sel.source {
        SelSource::ByNormal { dir, angle } => {
            f.choice(
                "Facing",
                None,
                dir,
                &[
                    ([1.0, 0.0, 0.0], "+X"),
                    ([-1.0, 0.0, 0.0], "-X"),
                    ([0.0, 1.0, 0.0], "+Y"),
                    ([0.0, -1.0, 0.0], "-Y"),
                    ([0.0, 0.0, 1.0], "+Z"),
                    ([0.0, 0.0, -1.0], "-Z"),
                ],
            );
            f.drag("Within", None, angle, 0.5, "°");
            *angle = angle.clamp(0.0, 180.0);
        }
        SelSource::InBox { min, max } => {
            f.xyz("Box min", None, min, 0.01, "");
            f.xyz("Box max", None, max, 0.01, "");
        }
        SelSource::Picked | SelSource::All => {}
    }
    f.buttons("Modify", None, |ui| {
        if ui.button("Shrink").clicked() {
            sel.grow -= 1;
        }
        if ui.button("Grow").clicked() {
            sel.grow += 1;
        }
        if sel.grow != 0 {
            ui.colored_label(ink::DIM(), format!("{:+}", sel.grow));
        }
        if ui.button("Clear").clicked() {
            let level = sel.level;
            *sel = PolySelection {
                level,
                ..Default::default()
            };
        }
    });
    f.toggle("Invert", None, &mut sel.invert);
    if let (Some(m), SubLevel::Edge) = (mesh, sel.level) {
        f.buttons("Extend", None, |ui| {
            if ui
                .button("Loop")
                .on_hover_text("Extend the selected edges along their loops")
                .clicked()
            {
                sel.expand_edges(m, false);
            }
            if ui
                .button("Ring")
                .on_hover_text("Extend the selected edges across their rings")
                .clicked()
            {
                sel.expand_edges(m, true);
            }
        });
    }
    if let Some(m) = mesh {
        let what = match sel.level.base() {
            SubLevel::Vertex => "vertices",
            SubLevel::Polygon => "polygons",
            _ => "edges",
        };
        let polys = sel.poly_mask(m).iter().filter(|s| **s).count();
        let text = if sel.level.base() == SubLevel::Polygon {
            format!("{polys} polygons")
        } else {
            format!("{} {what}, on {polys} polygons", sel.count(m))
        };
        f.value("Selected", text);
    }
}

// Shared rows

/// A Material node: the name it goes into the scene by, the surface's inputs,
/// and the texture files feeding them. An input fed by a texture is greyed out:
/// its value comes from the texture.
fn material_rows(
    ui: &mut egui::Ui,
    s: &mut crate::core::material::PreviewSurface,
    name: &str,
    io: &mut PanelIo,
    node: NodeId,
) {
    use crate::core::material::{prim_name, TEXTURED};
    group(ui, "Material", Some("Material Binding puts it in the scene under its Material Library Location (/materials by default), by this name. Rename the node to rename it"), |f| {
        f.value("Prim name", prim_name(name));
    });
    let fed = |input: &str| s.texture(input).is_some();
    let (diffuse, emissive, metallic, roughness, opacity) = (
        fed("diffuseColor"),
        fed("emissiveColor"),
        fed("metallic"),
        fed("roughness"),
        fed("opacity"),
    );
    group(
        ui,
        "Surface",
        Some("A UsdPreviewSurface. Values in linear colour, as USD has them"),
        |f| {
            f.enabled(!diffuse, |f| {
                f.row("Diffuse", None, |ui| colour_fields(ui, &mut s.diffuse_color));
            });
            f.enabled(!metallic, |f| {
                f.slider("Metallic", None, &mut s.metallic, 0.0..=1.0, "");
            });
            f.enabled(!roughness, |f| {
                f.slider("Roughness", None, &mut s.roughness, 0.0..=1.0, "");
            });
            f.enabled(!opacity, |f| {
                f.slider("Opacity", None, &mut s.opacity, 0.0..=1.0, "");
            });
            f.slider(
                "Opacity threshold",
                Some("Below this opacity the surface is cut out. 0: see-through instead"),
                &mut s.opacity_threshold,
                0.0..=1.0,
                "",
            );
            f.slider("IOR", Some("Index of refraction"), &mut s.ior, 1.0..=3.0, "");
            f.slider("Clearcoat", None, &mut s.clearcoat, 0.0..=1.0, "");
            f.slider("Coat roughness", None, &mut s.clearcoat_roughness, 0.0..=1.0, "");
            f.enabled(!emissive, |f| {
                f.row("Emissive", None, |ui| colour_fields(ui, &mut s.emissive_color));
            });
        },
    );
    group(ui, "Textures", Some("Image files feeding the inputs, read through the `st` texture coordinates. Colours are read as sRGB, the rest as they are. An opacity file that is the diffuse file reads its alpha"), |f| {
        for (input, label) in TEXTURED {
            let mut file = s.textures.get(input).cloned().unwrap_or_default();
            let mut clear = false;
            f.row(label, None, |ui| {
                ui.horizontal(|ui| {
                    let w = ui.available_width() - if file.is_empty() { 34.0 } else { 62.0 };
                    crate::properties::form::typed(ui, &mut file, |ui, t| {
    let r = ui.field(egui::TextEdit::singleline(&mut *t).hint_text("/path/to/texture.png").desired_width(w.max(40.0)));
    crate::variables::complete(ui, &r, t, &[]);
    r
});
                    if ui.button("📂").on_hover_text("Browse").clicked() {
                        io.browser.open(BrowseTarget::Texture(node, input), BrowseMode::File, &format!("Texture for {label}"), &["png", "jpg", "jpeg", "exr", "tif", "tiff", "tga", "hdr"], "");
                    }
                    if !file.is_empty() && ui.button("✕").on_hover_text("No texture").clicked() { clear = true; }
                });
            });
            if clear || file.trim().is_empty() { s.textures.remove(input); } else { s.textures.insert(input.to_string(), file.clone()); }
            if !clear && !file.trim().is_empty() && !crate::core::sequences::texture_exists(&file) {
                f.status(Status::Bad, format!("{label}: file not found"));
            }
        }
    });
}

/// A colour: its swatch, which opens a picker, and its three values.
fn colour_fields(ui: &mut egui::Ui, c: &mut [f32; 3]) {
    ui.horizontal(|ui| {
        ui.color_edit_button_rgb(c);
        let gap = ui.spacing().item_spacing.x;
        let w = ((ui.available_width() - 2.0 * gap) / 3.0).max(30.0);
        let h = ui.spacing().interact_size.y;
        for x in c.iter_mut() {
            form::number(ui, Some([w, h]), x, |d| {
                d.speed(0.01).range(0.0..=f32::MAX).max_decimals(3)
            });
        }
    });
}

/// A row's Material column: the input of its number by default, another input
/// with a material wired in, or Location: a material already in the scene, by
/// its path, typed on a line of its own under the row.
fn bind_choice(ui: &mut egui::Ui, row: usize, col: &mut BindColumn) {
    use crate::core::material::BindTo;
    let input = |n: usize| match col.inputs.get(n.wrapping_sub(1)) {
        Some(Some(name)) => format!("{n}: {name}"),
        _ => format!("{n}: none"),
    };
    let current = col.to.get(row).cloned().unwrap_or_default();
    let text = match &current {
        BindTo::Same => input(row + 1),
        BindTo::Input(n) => input(*n as usize),
        BindTo::Path(_) => "Location".to_string(),
    };
    let mut picked = None;
    egui::ComboBox::from_id_salt(("bind_to", row)).width(96.0).selected_text(text).show_ui(ui, |ui| {
        // The materials wired into this operator, by input.
        for (k, name) in col.inputs.iter().enumerate() {
            let n = k + 1;
            if name.is_none() && n != row + 1 { continue; }
            let choice = if n == row + 1 { BindTo::Same } else { BindTo::Input(n as u32) };
            let label = if n == row + 1 { format!("{} (this row's)", input(n)) } else { input(n) };
            if ui.selectable_label(current == choice, label).clicked() { picked = Some(choice); }
        }
        ui.separator();
        let at = matches!(current, BindTo::Path(_));
        if ui.selectable_label(at, "Location").on_hover_text("A material already in the scene, by its path: typed, picked or dropped from the Scene Explorer on the line under the row").clicked() && !at {
            picked = Some(BindTo::Path(String::new()));
        }
    }).response.on_hover_text("The material this row binds: one wired into this operator, or Location, a material already in the scene");
    if let Some(choice) = picked {
        while col.to.len() <= row {
            col.to.push(BindTo::Same);
        }
        col.to[row] = choice;
    }
}

/// Under a row bound by Location: the material's path, typed, picked from the
/// materials of the scene coming in, or dropped from the Scene Explorer.
fn material_location_row(f: &mut Rows, row: usize, col: &mut BindColumn) {
    use crate::core::material::BindTo;
    let scene = col.scene;
    let Some(BindTo::Path(path)) = col.to.get_mut(row) else {
        return;
    };
    f.row("", Some("The material's path in the scene"), |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("↳ 🎨").color(ink::DIM()));
            let w = (ui.available_width() - 52.0).max(60.0);
            let field = crate::properties::form::typed(ui, &mut *path, |ui, t| {
                ui.field(
                    egui::TextEdit::singleline(t)
                        .desired_width(w)
                        .hint_text("/materials/name")
                        .font(egui::TextStyle::Monospace)
                        .background_color(location_bg()),
                )
            });
            if let Some(p) = field.dnd_release_payload::<crate::types::DraggedPrims>() {
                if let Some(first) = p.0.first() {
                    *path = first.clone();
                }
            }
            ui.add_enabled_ui(!scene.is_empty(), |ui| {
                crate::menus::menu(ui, "Pick", |ui| {
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for m in scene {
                            if ui.selectable_label(path.trim() == m, m).clicked() {
                                *path = m.clone();
                                ui.close();
                            }
                        }
                    });
                })
                .response
                .on_hover_text("A material of the scene coming in");
            });
        });
    });
    let p = path.trim();
    if p.is_empty() {
        f.status(Status::Info, "No material location: the row binds nothing");
    } else if !scene.iter().any(|m| m == p) {
        f.status(Status::Bad, format!("{p}: no material there in the scene coming in"));
    }
}

/// Material Binding's parameters: where the materials go, and what is on each
/// input. True when the number of inputs changed.
fn binding_rows(
    ui: &mut egui::Ui,
    library: &mut String,
    inputs: &mut u32,
    ports: &[Option<std::sync::Arc<crate::core::material::Material>>],
    rows: usize,
) -> bool {
    use crate::core::material::{library_path, placed};
    group(
        ui,
        "Material Library",
        Some("Where the materials wired into this operator go in the scene, each by its operator's name"),
        |f| {
            f.row(
                "Location",
                Some("A USD path. Its parents are Scopes. Variables are put in"),
                |ui| {
                    crate::properties::form::typed(ui, &mut *library, |ui, t| {
                        let r = ui.field(
                            egui::TextEdit::singleline(&mut *t)
                                .hint_text("/materials")
                                .desired_width(ui.available_width())
                                .font(egui::TextStyle::Monospace)
                                .background_color(location_bg()),
                        );
                        crate::variables::complete(ui, &r, t, &[]);
                        r
                    });
                },
            );
            let path = library_path(library);
            if path != library.trim() {
                f.status(Status::Info, format!("Used as {path}"));
            }
        },
    );
    let mut changed = false;
    group(
        ui,
        "Materials",
        Some("One input per material, on the node's left side. Wiring the last one adds another"),
        |f| {
            let places = placed(library, ports);
            for (k, m) in ports.iter().enumerate() {
                let label = format!("Input {}", k + 1);
                match (m, places.get(k).cloned().flatten()) {
                    (None, _) => f.value(&label, "No material"),
                    (Some(_), Some(path)) => f.value(&label, path),
                    (Some(m), None) => f.status(
                        Status::Bad,
                        format!(
                            "{label}: an earlier input's material is also named {}. That one is used",
                            m.name
                        ),
                    ),
                }
            }
            changed = f
                .drag_u32(
                    "Inputs",
                    Some("How many material inputs. Fewer drops the wires on the ones taken away"),
                    inputs,
                    1..=64,
                )
                .changed();
        },
    );
    group(ui, "Bindings", Some("The rows are on the Bindings tab"), |f| {
        if rows == 0 {
            f.status(Status::Info, "No rows yet: nothing is bound. On the Bindings tab, row 1 binds the material on input 1, row 2 the one on input 2…");
        } else {
            f.status(
                Status::Info,
                format!("{rows} rows on the Bindings tab. On a primitive two rows take, the later wins"),
            );
        }
    });
    changed
}

/// How Load USD brings its file in: sublayered, or one prim of it referenced or
/// payloaded at a Primitive Location.
fn load_mode_rows(
    ui: &mut egui::Ui,
    path: &str,
    mode: &mut crate::usd_scene::LoadMode,
    location: &mut String,
    prim: &mut String,
    convert: bool,
) {
    use crate::usd_scene::LoadMode;
    let hint = "Sublayer: the whole stage at its own paths; Write USD sublayers the file. Reference: one prim of the file (its default prim) at the Primitive Location, its paths rebased there; Write USD writes a reference on that prim. Payload: as Reference, written as a payload, which other programs load on demand";
    group(ui, "Mode", Some(hint), |f| {
        f.choice(
            "Mode",
            None,
            mode,
            &[
                (LoadMode::Sublayer, "Sublayer"),
                (LoadMode::Reference, "Reference"),
                (LoadMode::Payload, "Payload"),
            ],
        );
        if *mode == LoadMode::Sublayer {
            return;
        }
        f.row(
            "Primitive Location",
            Some("Where the file's prim goes in the scene. Its parents are Xforms. Variables are put in: /set/$OPNAME"),
            |ui| {
                crate::properties::form::typed(ui, &mut *location, |ui, t| {
                    let r = ui.field(
                        egui::TextEdit::singleline(&mut *t)
                            .hint_text("/root")
                            .desired_width(ui.available_width())
                            .font(egui::TextStyle::Monospace)
                            .background_color(location_bg()),
                    );
                    crate::variables::complete(ui, &r, t, &[]);
                    r
                });
            },
        );
        let at = crate::types::prim_location(location, &crate::types::reference_location());
        if at != location.trim() {
            f.status(Status::Info, format!("Used as {at}"));
        }
        let scene = if path.trim().is_empty() {
            None
        } else {
            crate::usd_scene::load_cached(path.trim(), convert).ok()
        };
        let roots: Vec<String> = scene
            .as_ref()
            .map(|s| {
                s.tree
                    .nodes
                    .iter()
                    .filter(|n| n.depth == 1)
                    .map(|n| n.path.clone())
                    .collect()
            })
            .unwrap_or_default();
        f.row(
            "Prim",
            Some("The prim of the file it brings in, with everything below it. Empty: the file's default prim"),
            |ui| {
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - 48.0).max(60.0);
                    crate::properties::form::typed(ui, &mut *prim, |ui, t| {
                        ui.field(
                            egui::TextEdit::singleline(t)
                                .hint_text("default prim")
                                .desired_width(w)
                                .font(egui::TextStyle::Monospace)
                                .background_color(location_bg()),
                        )
                    });
                    ui.add_enabled_ui(!roots.is_empty(), |ui| {
                        crate::menus::menu(ui, "Pick", |ui| {
                            if ui.selectable_label(prim.trim().is_empty(), "Default prim").clicked() {
                                prim.clear();
                                ui.close();
                            }
                            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                                for r in &roots {
                                    if ui.selectable_label(prim.trim() == r, r).clicked() {
                                        *prim = r.clone();
                                        ui.close();
                                    }
                                }
                            });
                        })
                        .response
                        .on_hover_text("The file's root prims");
                    });
                });
            },
        );
        if let Some(scene) = &scene {
            match crate::usd_scene::referenced_prim(scene, prim) {
                Some(p) => {
                    let named = crate::types::prim_location(prim, "");
                    if !named.is_empty() && named != p {
                        f.status(
                            Status::Bad,
                            format!("The file has no {named}: its default prim is used"),
                        );
                    } else if named.is_empty() && scene.default_prim.is_none() {
                        f.status(
                            Status::Info,
                            "The file names no default prim: its first root prim is used",
                        );
                    }
                    let word = if *mode == LoadMode::Payload {
                        "Payloads"
                    } else {
                        "References"
                    };
                    f.status(Status::Ok, format!("{word} {p} at {at}"));
                }
                None => f.status(Status::Bad, "The file has no prims to bring in"),
            }
        }
    });
}

/// The Camera operator: where it is, where it looks, its lens and gate, and a
/// button to look through it.
fn camera_rows(
    ui: &mut egui::Ui,
    location: &mut String,
    translate: &mut [f32; 3],
    rotate: &mut [f32; 3],
    c: &mut crate::core::camera::Camera,
) {
    use crate::core::camera::Projection;
    let path = crate::types::prim_location(location, &crate::types::camera_location());
    group(
        ui,
        "Primitive",
        Some("Where the camera is in the scene. Its parents are Xforms"),
        |f| {
            f.text_vars(
                "Primitive Location",
                Some("A USD path, such as /camera or /shot/cam_main. Variables are put in: /shot/$OPNAME"),
                location,
                "/camera",
                false,
            );
            if path != location.trim() {
                f.status(Status::Info, format!("Used as {path}"));
            }
        },
    );
    group(
        ui,
        "Transform",
        Some("It looks down its -Z axis, with +Y up. A Transform after it moves it too"),
        |f| {
            f.xyz("Translate", None, translate, 0.05, "");
            f.xyz("Rotate", Some("Degrees, X then Y then Z"), rotate, 0.5, "°");
        },
    );
    group(ui, "Lens", Some("As USD has them: focal length and gate in tenths of a scene unit, which is millimetres in a centimetre scene. The field of view comes from their ratio"), |f| {
        f.choice("Projection", None, &mut c.projection, &[(Projection::Perspective, "Perspective"), (Projection::Orthographic, "Orthographic")]);
        f.slider("Focal length", None, &mut c.focal_length, 4.0..=300.0, "");
        f.row("Filmback Presets", Some("Cameras' filmbacks (gates), width by height in millimetres"), |ui| {
            let current = crate::core::camera::filmback_name(c.horizontal_aperture, c.vertical_aperture).unwrap_or("Custom");
            egui::ComboBox::from_id_salt("filmback_presets").width(ui.available_width() - 8.0).selected_text(current).show_ui(ui, |ui| {
                for (name, w, h) in crate::core::camera::FILMBACKS {
                    if ui.selectable_label(current == *name, format!("{name}   {w} × {h} mm")).clicked() {
                        c.horizontal_aperture = *w;
                        c.vertical_aperture = *h;
                    }
                }
            });
        });
        f.row("Filmback", Some("Horizontal and vertical aperture: the gate"), |ui| {
            ui.horizontal(|ui| {
                let h = ui.spacing().interact_size.y;
                let w = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(30.0);
                form::number(ui, Some([w, h]), &mut c.horizontal_aperture, |d| d.speed(0.05).range(0.01..=1000.0).prefix("H "));
                form::number(ui, Some([w, h]), &mut c.vertical_aperture, |d| d.speed(0.05).range(0.01..=1000.0).prefix("V "));
            });
        });
        f.row("Filmback offset", Some("Shift of the gate, horizontal and vertical. Written, not shown in the Viewer yet"), |ui| {
            ui.horizontal(|ui| {
                let h = ui.spacing().interact_size.y;
                let w = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(30.0);
                form::number(ui, Some([w, h]), &mut c.horizontal_aperture_offset, |d| d.speed(0.05).prefix("H "));
                form::number(ui, Some([w, h]), &mut c.vertical_aperture_offset, |d| d.speed(0.05).prefix("V "));
            });
        });
        f.row("Clipping", Some("Near and far, in scene units"), |ui| {
            ui.horizontal(|ui| {
                let h = ui.spacing().interact_size.y;
                let w = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(30.0);
                form::number(ui, Some([w, h]), &mut c.clipping_range[0], |d| d.speed(0.01).range(0.0001..=f32::MAX).prefix("near "));
                form::number(ui, Some([w, h]), &mut c.clipping_range[1], |d| d.speed(10.0).range(0.001..=f32::MAX).prefix("far "));
            });
        });
        f.drag("f-stop", Some("0: no depth of field. Written for renderers, not shown in the Viewer"), &mut c.f_stop, 0.05, "");
        f.drag("Focus distance", Some("In scene units. Written for renderers; leaving the camera in the Viewer keeps this far a point to orbit about"), &mut c.focus_distance, 0.05, "");
        if c.projection == Projection::Perspective {
            f.value("Field of view", format!("{:.1}° wide, {:.1}° high", 2.0 * (c.horizontal_aperture * 0.5 / c.focal_length.max(1e-6)).atan().to_degrees(),
                c.fov(c.gate_aspect()).to_degrees()));
        }
    });
    group(ui, "View", None, |f| {
        let looking = crate::viewer::camera::looking_through().as_deref() == Some(path.as_str());
        f.buttons(
            "Viewer",
            Some("The Viewer's camera menu does the same, for every camera of the scene"),
            |ui| {
                if looking {
                    if ui.button("Back to Perspective").clicked() {
                        crate::viewer::camera::look_through(None);
                    }
                } else if ui.button("Look through").clicked() {
                    crate::viewer::camera::look_through(Some(path.clone()));
                }
            },
        );
        if looking {
            f.status(
                Status::Ok,
                "The Viewer looks through it. Navigating leaves it, where it was",
            );
        }
    });
}

/// Path field with a button that opens the file browser.
fn path_row(
    f: &mut Rows,
    value: &mut String,
    hint: &str,
    io: &mut PanelIo,
    node: NodeId,
    mode: BrowseMode,
    title: &str,
    exts: &[&str],
) {
    f.row(
        "Path",
        Some("Variables are put in: $JOB/assets/set.usda. Type $ for the list"),
        |ui| {
            ui.horizontal(|ui| {
                let w = ui.available_width() - 34.0;
                crate::properties::form::typed(ui, &mut *value, |ui, t| {
                    let r = ui.field(
                        egui::TextEdit::singleline(&mut *t)
                            .hint_text(hint)
                            .desired_width(w.max(40.0)),
                    );
                    crate::variables::complete(ui, &r, t, &[]);
                    r
                });
                if ui.button("📂").on_hover_text("Browse").clicked() {
                    io.browser.open(BrowseTarget::Node(node), mode, title, exts, "");
                }
            });
        },
    );
    // What it is with its variables put in, when that differs.
    let path = crate::core::vars::expand(value.trim(), &[]);
    if !path.is_empty() && path != value.trim() {
        f.status(Status::Info, format!("→ {path}"));
    }
}

/// Loaded or not, and the take when the file has several.
fn take_rows(f: &mut Rows, loaded: Result<crate::fbx_loader::LoadedFbx, String>, take: &mut u32) {
    match loaded {
        Ok(loaded) => {
            f.status(Status::Ok, "Loaded");
            if loaded.takes.len() > 1 {
                let options: Vec<(u32, &str)> = loaded
                    .takes
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (i as u32, n.as_str()))
                    .collect();
                f.choice("Take", None, take, &options);
            }
        }
        Err(e) => f.status(Status::Bad, e),
    }
}

fn rate_row(f: &mut Rows, num: &mut u32, den: &mut u32) {
    let mut cur = FrameRate::new(*num, *den);
    let labels: Vec<String> = RATE_PRESETS.iter().map(|(name, _)| format!("{name} fps")).collect();
    let options: Vec<(FrameRate, &str)> = RATE_PRESETS
        .iter()
        .zip(&labels)
        .map(|((_, r), l)| (*r, l.as_str()))
        .collect();
    if f.choice("Rate", None, &mut cur, &options) {
        *num = cur.num;
        *den = cur.den;
    }
}

/// What comes in and what goes out of a clip node.
fn clip_summary(ui: &mut egui::Ui, anim: &AnimContext, with_input: bool) {
    let line = |c: &AnimData| {
        format!(
            "{} to {}, {} frames at {} fps{}",
            c.timecode(c.start_frame),
            c.timecode(c.end_frame()),
            c.frames,
            c.rate.label(),
            if c.drop_frame { " DF" } else { "" }
        )
    };
    group(ui, "Clip", None, |f| {
        if with_input {
            match &anim.input {
                Some(i) => f.value("In", line(i)),
                None => f.status(Status::Info, "No clip connected"),
            }
        }
        if let Some(o) = &anim.output {
            f.value("Out", line(o));
        }
    });
}

/// Progress and results of the last write.
fn batch_log(ui: &mut egui::Ui, batch: &BatchState) {
    let p = batch.0.lock().unwrap();
    if p.log.is_empty() && !p.running {
        return;
    }
    group(ui, "Log", None, |f| {
        f.wide(|ui| {
            if p.running {
                ui.add(
                    egui::ProgressBar::new(p.done as f32 / p.total.max(1) as f32)
                        .text(format!("Writing {} of {}", p.done, p.total)),
                );
            }
            egui::ScrollArea::vertical()
                .id_salt("batch_log")
                .max_height(260.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &p.log {
                        let col = if line.starts_with("FAILED") {
                            ink::BAD()
                        } else {
                            ink::LABEL()
                        };
                        ui.colored_label(col, egui::RichText::new(line).small());
                    }
                });
        })
    });
}

/// Under a file to read (after `path_row`, which shows what it becomes):
/// whether it is set, and there.
fn path_status(f: &mut Rows, typed: &str) {
    let path = crate::core::vars::expand(typed.trim(), &[]);
    if path.is_empty() {
        f.status(Status::Info, "No file set");
    } else if std::path::Path::new(&path).exists() {
        f.status(Status::Ok, "Found");
    } else {
        f.status(Status::Bad, "Not found");
    }
}

/// A unit's usual name, for a stage's metersPerUnit.
fn unit_name(metres: f64) -> &'static str {
    match metres {
        m if (m - 0.01).abs() < 1e-9 => "centimetres",
        m if (m - 0.001).abs() < 1e-9 => "millimetres",
        m if (m - 0.0254).abs() < 1e-9 => "inches",
        m if (m - 0.3048).abs() < 1e-9 => "feet",
        m if (m - 1000.0).abs() < 1e-6 => "kilometres",
        _ => "another unit",
    }
}

/// What a USD file holds, from the cached stage (`convert`: read turned to Y-up
/// and metres).
fn usd_summary(ui: &mut egui::Ui, path: &str, convert: bool, io: &mut PanelIo) {
    // A big stage is read in the background: its boxes first, then the
    // whole of it. Say how far it has come.
    let reading = crate::usd_scene::reading(path, convert);
    let scene = match crate::usd_scene::load_cached(path, convert) {
        Ok(s) => s,
        Err(e) if reading.is_some() => {
            group(ui, "Stage", None, |f| f.status(Status::Info, e));
            return;
        }
        Err(e) => {
            group(ui, "Stage", None, |f| {
                f.status(Status::Bad, format!("Could not read the file: {e}"))
            });
            return;
        }
    };
    if let Some(r) = reading {
        group(ui, "Stage", None, |f| f.status(Status::Info, r));
    }
    let leaf = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();
    group(ui, "Stage", None, |f| {
        f.value(
            "Meshes",
            format!("{} primitives, {} triangles", scene.meshes.len(), scene.triangles()),
        );
        // As the stage says, and what Imago does with it.
        let kept = if scene.converted {
            "turned to Y-up"
        } else {
            "kept as authored"
        };
        f.value(
            "Up axis",
            if scene.up_axis == "Y" {
                "Y".to_string()
            } else {
                format!("{}, {kept}", scene.up_axis)
            },
        );
        let unit = match scene.meters_per_unit {
            None => "Not stated (USD's fallback is centimetres): kept as authored".to_string(),
            Some(u) if (u - 1.0).abs() < 1e-9 => "Metres".to_string(),
            Some(u) if scene.converted => format!("{u} m per unit ({}), scaled to metres", unit_name(u)),
            Some(u) => format!("{u} m per unit ({}), kept as authored", unit_name(u)),
        };
        f.value("Unit", unit);
        if !scene.converted && (scene.up_axis != "Y" || scene.meters_per_unit.is_some_and(|u| (u - 1.0).abs() > 1e-9)) {
            f.status(
                Status::Info,
                "Not Y-up metres: other stages may not match it. Turn on Y-up metres, or move and scale it yourself",
            );
        }
        if let Some((start, end, rate)) = scene.time {
            f.value("Time", format!("{start} to {end} at {rate} per second"));
        }
        let kinds: Vec<String> = scene.prim_counts.iter().map(|(k, n)| format!("{n} {k}")).collect();
        f.value("Prims", kinds.join(", "));
    });
    let list = |ui: &mut egui::Ui, title: String, rows: Vec<(String, String)>| {
        if rows.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(title).default_open(false).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(ui.id().with("usd_rows"))
                .max_height(180.0)
                .show(ui, |ui| {
                    for (name, detail) in rows {
                        ui.colored_label(ink::LABEL(), name);
                        if !detail.is_empty() {
                            ui.colored_label(ink::DIM(), egui::RichText::new(detail).small());
                        }
                    }
                });
        });
    };
    // Materials: what each is, and a MaterialX operator made from its network.
    if !scene.materials.is_empty() {
        egui::CollapsingHeader::new(format!("Materials ({})", scene.materials.len())).default_open(false).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt(ui.id().with("usd_materials")).max_height(240.0).show(ui, |ui| {
                for m in &scene.materials {
                    let mut parts = vec![];
                    if let Some(c) = m.diffuse { parts.push(format!("colour {:.2} {:.2} {:.2}", c[0], c[1], c[2])); }
                    if let Some(v) = m.roughness { parts.push(format!("roughness {v:.2}")); }
                    if let Some(v) = m.metallic { parts.push(format!("metallic {v:.2}")); }
                    if let Some(v) = m.opacity { parts.push(format!("opacity {v:.2}")); }
                    for (role, file) in &m.textures { parts.push(format!("{role}: {file}")); }
                    ui.horizontal(|ui| {
                        ui.colored_label(ink::LABEL(), leaf(&m.path));
                        if let Some(g) = &m.graph {
                            if ui.small_button("→ MaterialX").on_hover_text(format!(
                                "Add a MaterialX operator with this material's network ({} nodes), named after it. Bound by a Material Binding whose library is {}, it takes the place of this one",
                                g.nodes.len(), m.path.rsplit_once('/').map(|(p, _)| if p.is_empty() { "/" } else { p }).unwrap_or("/"))).clicked()
                            {
                                io.add.push((leaf(&m.path), NodeType::MaterialX { graph: (**g).clone() }));
                            }
                        }
                    });
                    if !parts.is_empty() { ui.colored_label(ink::DIM(), egui::RichText::new(parts.join("\n")).small()); }
                    for n in m.graph_notes.iter().take(3) {
                        ui.colored_label(ink::DIM(), egui::RichText::new(format!("MaterialX: {n}")).small());
                    }
                }
            });
        });
    }
    list(
        ui,
        format!("Cameras ({})", scene.cameras.len()),
        scene
            .cameras
            .iter()
            .map(|c| {
                let (k, at) = (&c.camera, c.position());
                (
                    leaf(&c.path),
                    format!(
                        "{} {} mm, aperture {} x {}, clip {} to {}, at {:.2} {:.2} {:.2}",
                        k.projection.token(),
                        k.focal_length,
                        k.horizontal_aperture,
                        k.vertical_aperture,
                        k.clipping_range[0],
                        k.clipping_range[1],
                        at[0],
                        at[1],
                        at[2]
                    ),
                )
            })
            .collect(),
    );
    list(
        ui,
        format!("Skeletons ({})", scene.skeletons.len()),
        scene
            .skeletons
            .iter()
            .map(|s| (leaf(&s.path), format!("{} joints", s.joints.len())))
            .collect(),
    );
    list(
        ui,
        format!("Lights ({})", scene.lights.len()),
        scene
            .lights
            .iter()
            .map(|l| {
                let at = l
                    .place
                    .matrices()
                    .first()
                    .map(|m| m.w_axis.truncate().to_array())
                    .unwrap_or([0.0; 3]);
                (
                    leaf(&l.path),
                    format!(
                        "{}, intensity {}, at {:.2} {:.2} {:.2}",
                        l.light.kind.usd(),
                        l.light.float("inputs:intensity"),
                        at[0],
                        at[1],
                        at[2]
                    ),
                )
            })
            .collect(),
    );
    if !scene.notes.is_empty() {
        ui.add_space(form::GAP);
        group(ui, "Not read in full", None, |f| {
            for n in &scene.notes {
                f.status(Status::Info, n.clone());
            }
        });
    }
}

/// A name pattern: comma-separated regular expressions (see `core::pattern`),
/// with a picker of the names coming in and a match count. With `single`, the
/// field holds one expression and picking replaces it. The background of a
/// Primitive Location field: a grey of its own, between the panel and the other
/// fields, in light and dark schemes alike.
fn location_bg() -> egui::Color32 {
    ink::LOCATION()
}

/// Add a path to a list of Primitive Locations, unless it is there already.
fn add_location(list: &mut Vec<String>, path: &str) {
    let path = path.trim();
    if path.is_empty() {
        list.push(String::new());
        return;
    }
    if !list.iter().any(|l| l.trim() == path) {
        list.push(path.to_string());
    }
}

/// Set Attribute's rows: each a context, a name (the common ones offered), a
/// type and a value. `located`: the Prims Location tab names prims.
fn set_attribute_rows(
    ui: &mut egui::Ui,
    rows: &mut Vec<crate::core::set_attribute::SetRow>,
    located: bool,
    coming_in: usize,
) {
    use crate::core::properties::{preset, SetContext, ValueType, OBJECT_PRESETS, PROPERTY_PRESETS};
    use crate::core::set_attribute::SetRow;
    let hint = "One value per prim, on the prims the Prims Location tab names: a group, a camera or a light as well as a mesh. Without locations, on every primitive coming in (or those an earlier Pick Primitives chose). Values per point, face or corner are ICE's";
    group(ui, "Set Attribute", Some(hint), |f| {
        if located {
            f.status(Status::Info, "On the prims the Prims Location tab names");
        } else if coming_in == 0 {
            f.status(Status::Info, "No primitives coming in");
        } else {
            f.status(Status::Info, format!("No Prims Location: on the {coming_in} primitives coming in (or the picked ones). Groups, cameras and lights need a location"));
        }
        if rows.is_empty() {
            f.status(Status::Info, "No rows: nothing is set");
        }
    });
    let context_hint = "Object: a constant primvar (primvars:name), data for rendering that shaders read and the prims below a group inherit. Property: the prim's own attribute or metadata (visibility, purpose, kind, active…), not seen by shaders, not inherited";
    let mut remove = None;
    let mut up = None;
    for (k, row) in rows.iter_mut().enumerate() {
        let title = format!(
            "{}  {}",
            k + 1,
            if row.name.trim().is_empty() {
                "(no name)"
            } else {
                row.name.trim()
            }
        );
        group_with(
            ui,
            &title,
            Some("Where two rows set the same name, the later one wins"),
            |ui| {
                if ui.small_button("🗑").on_hover_text("Remove this row").clicked() {
                    remove = Some(k);
                }
                if k > 0 && ui.small_button("⏶").on_hover_text("Move up").clicked() {
                    up = Some(k);
                }
            },
            |f| {
                let was = row.context;
                f.choice(
                    "Context",
                    Some(context_hint),
                    &mut row.context,
                    &[(SetContext::Object, "Object"), (SetContext::Property, "Property")],
                );
                // An Object value is a column of a Geo: only the types it holds.
                if row.context != was
                    && row.context == SetContext::Object
                    && !ValueType::OBJECT.contains(&row.value.kind())
                {
                    row.value = ValueType::Float.convert(&row.value);
                }
                let offered = match row.context {
                    SetContext::Object => OBJECT_PRESETS,
                    SetContext::Property => PROPERTY_PRESETS,
                };
                let name_hint = match row.context {
                SetContext::Object => "Written as primvars:name. Pick a common one, or type any name",
                SetContext::Property => "The attribute or metadata to set. Pick a common one, or type any name (userProperties:assetId, focalLength)",
            };
                f.row("Name", Some(name_hint), |ui| {
                    ui.horizontal(|ui| {
                        let w = (ui.available_width() - 48.0).max(60.0);
                        crate::properties::form::typed(ui, &mut row.name, |ui, t| {
                            ui.field(
                                egui::TextEdit::singleline(t)
                                    .desired_width(w)
                                    .hint_text("name")
                                    .font(egui::TextStyle::Monospace),
                            )
                        });
                        crate::menus::menu(ui, "Pick", |ui| {
                            for p in offered {
                                if ui
                                    .selectable_label(row.name.trim() == p.name, p.name)
                                    .on_hover_text(p.hint)
                                    .clicked()
                                {
                                    *row = SetRow::from_preset(row.context, p);
                                }
                            }
                        });
                    });
                });
                let known = preset(row.context, &row.name);
                match known {
                    // A common name has its type.
                    Some(p) => {
                        if row.value.kind() != p.ty {
                            row.value = p.ty.convert(&row.value);
                        }
                        f.value("Type", p.ty.usd());
                    }
                    None => {
                        let types: &[ValueType] = match row.context {
                            SetContext::Object => &ValueType::OBJECT,
                            SetContext::Property => &ValueType::ALL,
                        };
                        let options: Vec<(ValueType, &str)> = types.iter().map(|t| (*t, t.usd())).collect();
                        let mut ty = row.value.kind();
                        if f.choice("Type", None, &mut ty, &options) {
                            row.value = ty.convert(&row.value);
                        }
                    }
                }
                value_row(f, &mut row.value, known.map(|p| p.tokens).unwrap_or(&[]));
                if row.name.trim().is_empty() {
                    f.status(Status::Info, "No name: this row sets nothing");
                } else if let Some(p) = known {
                    f.status(Status::Info, p.hint);
                }
            },
        );
    }
    if let Some(k) = remove {
        rows.remove(k);
    }
    if let Some(k) = up {
        rows.swap(k, k - 1);
    }
    group(ui, "Rows", None, |f| {
        f.buttons("Add", Some("Object: a primvar, displayColor to start. Property: an attribute or metadata, visibility to start. Either can be renamed"), |ui| {
            if ui.button("+ Object").clicked() { rows.push(SetRow::from_preset(SetContext::Object, &OBJECT_PRESETS[0])); }
            if ui.button("+ Property").clicked() { rows.push(SetRow::from_preset(SetContext::Property, &PROPERTY_PRESETS[0])); }
        });
    });
}

/// A value's field, for its type. Tokens with fixed values are a choice.
fn value_row(f: &mut Rows, v: &mut crate::core::properties::Value, tokens: &[&str]) {
    use crate::core::properties::Value;
    let fields = |ui: &mut egui::Ui, v: &mut [f32]| {
        ui.horizontal(|ui| {
            let gap = ui.spacing().item_spacing.x;
            let n = v.len() as f32;
            let w = ((ui.available_width() - (n - 1.0) * gap) / n).max(30.0);
            let h = ui.spacing().interact_size.y;
            for x in v.iter_mut() {
                form::number(ui, Some([w, h]), x, |d| d.speed(0.01).max_decimals(4));
            }
        });
    };
    match v {
        Value::Bool(b) => {
            f.toggle("Value", None, b);
        }
        Value::Int(i) => {
            f.row("Value", None, |ui| {
                let h = ui.spacing().interact_size.y;
                form::number(ui, Some([ui.available_width(), h]), i, |d| d.speed(0.1));
            });
        }
        Value::Float(x) => {
            f.drag("Value", None, x, 0.01, "");
        }
        Value::Double(x) => {
            f.row("Value", None, |ui| {
                let h = ui.spacing().interact_size.y;
                form::number(ui, Some([ui.available_width(), h]), x, |d| d.speed(0.01));
            });
        }
        Value::Token(s) if !tokens.is_empty() => {
            let options: Vec<(String, &str)> = tokens.iter().map(|t| (t.to_string(), *t)).collect();
            f.choice("Value", None, s, &options);
        }
        Value::String(s) | Value::Token(s) => {
            f.text_vars("Value", Some("Variables are put in: $OPNAME, $GNAME…"), s, "", false);
        }
        Value::Asset(s) => {
            f.text_vars(
                "Value",
                Some("A file path, written as @path@, its variables put in: $JOB/tex/sky.exr"),
                s,
                "/path/to/file",
                false,
            );
        }
        Value::Float2(a) => {
            f.row("Value", None, |ui| fields(ui, a));
        }
        Value::Float3(a) => {
            f.row("Value", None, |ui| fields(ui, a));
        }
        Value::Float4(a) => {
            f.row("Value", None, |ui| fields(ui, a));
        }
        Value::Color3f(c) => {
            f.row("Value", None, |ui| {
                ui.horizontal(|ui| {
                    ui.color_edit_button_rgb(c);
                    fields(ui, c);
                });
            });
        }
    }
}

/// What an operator's Prims Location tab does with its paths.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabKind {
    /// The operator works on what a path matches and everything below it.
    Takes,
    /// As Takes, for removing: empty prunes nothing.
    Prune,
    /// A path names that prim alone, of any kind (Set Attribute).
    Names,
    /// As Takes, each row with the material it binds: empty binds nothing.
    Binds,
}

/// Material Binding's Material column: what each row binds, the material on
/// each input (its name), and the materials of the scene coming in.
pub(crate) struct BindColumn<'a> {
    to: &'a mut Vec<crate::core::material::BindTo>,
    inputs: &'a [Option<String>],
    scene: &'a [String],
}

/// The Prims Location tab: the Primitive Locations an operator works on, one
/// per row, each with how many of the primitives coming in it matches.
pub(crate) fn locations_tab(
    ui: &mut egui::Ui,
    list: &mut Vec<String>,
    paths: &[String],
    selected: &[String],
    kind: TabKind,
    mut binds: Option<BindColumn>,
) {
    use crate::core::locations;
    let names = kind == TabKind::Names;
    let prune = kind == TabKind::Prune;
    let binding = kind == TabKind::Binds;
    let hits = |row: &str, p: &str| {
        if names {
            locations::names(row, p)
        } else {
            locations::matches(row, p)
        }
    };
    let hint = if binding {
        "Each row binds a material to the primitive at its path and every one below it: the material input of the row's number, unless another input is chosen in its Material column, or Location, a material already in the scene by its path. On a primitive two rows take, the later row wins"
    } else if names {
        "A path names that prim alone: a group, a camera or a light as well as a mesh. * stands for any run of characters in a name, ? for one, ** for any number of levels: /set/* names the children of /set, /set/** /set and everything below it"
    } else {
        "A path takes that primitive and everything below it. * stands for any run of characters in a name, ? for one, ** for any number of levels: /set/**/lamp*"
    };
    group(
        ui,
        if binding { "Bindings" } else { "Primitive Locations" },
        Some(hint),
        |f| {
            let mut remove = None;
            for (k, row) in list.iter_mut().enumerate() {
                f.row(&format!("{}", k + 1), None, |ui| {
                    ui.horizontal(|ui| {
                        let w = (ui.available_width() - if binds.is_some() { 180.0 } else { 74.0 }).max(60.0);
                        // A path, not text: grey, in the code font, so a location
                        // is told apart from the other text fields at a glance.
                        crate::properties::form::typed(ui, &mut *row, |ui, t| {
                            ui.field(
                                egui::TextEdit::singleline(t)
                                    .desired_width(w)
                                    .hint_text("/path/to/prim")
                                    .font(egui::TextStyle::Monospace)
                                    .background_color(location_bg()),
                            )
                        });
                        // How many of the primitives coming in this row matches.
                        if !row.trim().is_empty() && !paths.is_empty() {
                            let n = paths.iter().filter(|p| hits(row, p)).count();
                            let text = egui::RichText::new(format!("{n}")).monospace();
                            let label = if n == 0 {
                                ui.colored_label(ink::BAD(), text)
                            } else {
                                ui.label(text)
                            };
                            label.on_hover_text(if n == 0 {
                                "Matches no primitive coming in".to_string()
                            } else {
                                format!("Matches {n} of the primitives coming in")
                            });
                        }
                        if let Some(col) = binds.as_mut() {
                            bind_choice(ui, k, col);
                        }
                        if ui.small_button("🗑").on_hover_text("Remove").clicked() {
                            remove = Some(k);
                        }
                    });
                });
                // Bound to a material of the scene by its path: the path, under the row.
                if let Some(col) = binds.as_mut() {
                    material_location_row(f, k, col);
                }
            }
            if let Some(k) = remove {
                list.remove(k);
                // Its material goes with it.
                if let Some(col) = binds.as_mut() {
                    if k < col.to.len() {
                        col.to.remove(k);
                    }
                }
            }
            f.row("", None, |ui| {
            ui.horizontal(|ui| {
                let tip = match selected {
                    [] => "Add a row. Select prims in the Scene Explorer first to add their paths, or drag prims from it and drop them here".to_string(),
                    [one] => format!("Add {one}, the prim selected in the Scene Explorer. Prims dragged from the Scene Explorer can be dropped here"),
                    many => format!("Add the {} prims selected in the Scene Explorer. Prims dragged from the Scene Explorer can be dropped here", many.len()),
                };
                let add = ui.button("+ Add Location").on_hover_text(tip);
                if add.clicked() {
                    if selected.is_empty() { add_location(list, ""); }
                    for p in selected { add_location(list, p); }
                }
                if let Some(p) = add.dnd_release_payload::<crate::types::DraggedPrims>() { for path in &p.0 { add_location(list, path); } }
                ui.add_enabled_ui(!paths.is_empty(), |ui| {
                    crate::menus::menu(ui, "Pick", |ui| {
                        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                            for p in paths {
                                let listed = list.iter().any(|l| l.trim() == p);
                                if ui.selectable_label(listed, p).clicked() && !listed { add_location(list, p); }
                            }
                        });
                    }).response.on_hover_text("Add a path from the primitives coming in");
                });
            });
        });
            let set = locations::is_set(list);
            let taken = paths
                .iter()
                .filter(|p| list.iter().any(|row| !row.trim().is_empty() && hits(row, p)))
                .count();
            match (set, prune) {
                (false, _) if binding => f.status(
                    Status::Info,
                    "Empty: nothing is bound. Add a row per material: row 1 binds the material on input 1",
                ),
                (false, false) => f.status(
                    Status::Info,
                    "Empty: every primitive coming in (or those an earlier Pick Primitives chose)",
                ),
                (false, true) => f.status(Status::Info, "Empty: nothing is pruned"),
                (true, _) if paths.is_empty() => f.status(Status::Info, "No primitives coming in"),
                (true, _) if names => f.status(
                    Status::Info,
                    format!("Names {taken} of the {} prims of the scene coming in", paths.len()),
                ),
                (true, _) => f.status(
                    Status::Info,
                    format!("Takes {taken} of the {} primitives coming in", paths.len()),
                ),
            }
        },
    );
}

fn pattern_rows(f: &mut Rows, id: &str, label: &str, text: &mut String, names: &[String], single: bool) {
    use crate::core::pattern;
    let hint = if single {
        "A regular expression, case ignored"
    } else {
        "Comma-separated regular expressions, case ignored. A plain word matches names containing it"
    };
    f.row(label, Some(hint), |ui| {
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 48.0).max(60.0);
            crate::properties::form::typed(ui, &mut *text, |ui, t| {
                ui.field(egui::TextEdit::singleline(t).desired_width(w).hint_text("regex, regex"))
            });
            ui.add_enabled_ui(!names.is_empty(), |ui| {
                crate::menus::menu(ui, "Pick", |ui| {
                    let filter_id = ui.id().with(id).with("filter");
                    let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
                    ui.field(
                        egui::TextEdit::singleline(&mut filter)
                            .desired_width(200.0)
                            .hint_text("filter the list"),
                    );
                    let shown = pattern::NamePattern::new(&filter);
                    let current = pattern::NamePattern::new(text);
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for n in names {
                            if !shown.is_empty() && !shown.matches(n) {
                                continue;
                            }
                            if single {
                                if ui.selectable_label(false, n).clicked() {
                                    *text = regex::escape(n);
                                    ui.close();
                                }
                            } else if ui.selectable_label(current.matches(n), n).clicked() {
                                pattern::toggle(text, n);
                            }
                        }
                    });
                    if !single && !filter.trim().is_empty() && ui.button("Use the filter as a pattern").clicked() {
                        let mut list: Vec<String> = pattern::parts(text).into_iter().map(String::from).collect();
                        list.extend(pattern::parts(&filter).into_iter().map(String::from));
                        *text = list.join(", ");
                        filter.clear();
                    }
                    ui.data_mut(|d| d.insert_temp(filter_id, filter));
                })
                .response
                .on_hover_text("Pick from the names coming in");
            });
        });
    });
    if let Some(e) = pattern::error(text) {
        f.status(Status::Bad, e);
    } else if !names.is_empty() && !text.trim().is_empty() {
        let p = pattern::NamePattern::new(text);
        let n = names.iter().filter(|x| p.matches(x)).count();
        f.status(Status::Info, format!("Matches {n} of {}", names.len()));
    }
}

/// The axes and unit a clip is written in.
fn space_line(c: &AnimData) -> String {
    let axis = |(a, sgn): (i32, i32)| {
        format!(
            "{}{}",
            if sgn < 0 { "-" } else { "" },
            ["X", "Y", "Z"][a.clamp(0, 2) as usize]
        )
    };
    let unit = |cm: f64| match cm {
        x if (x - 1.0).abs() < 1e-9 => "cm".to_string(),
        x if (x - 100.0).abs() < 1e-9 => "m".into(),
        x if (x - 0.1).abs() < 1e-9 => "mm".into(),
        x if (x - 2.54).abs() < 1e-9 => "inches".into(),
        x => format!("{x} cm units"),
    };
    match c.space.as_deref() {
        Some(sp) => format!("{} up, {}, as the source file", axis(sp.up), unit(sp.unit_cm)),
        None => "Y up, cm (made in this program)".into(),
    }
}

/// The Primitive Location of a shape: where in the scene its primitive is.
/// Parents on the path are plain Xforms. What is typed is cleaned into a USD
/// path, and the panel says so when that changes it.
fn location_group(ui: &mut egui::Ui, location: &mut String, fallback: &str) {
    group(
        ui,
        "Primitive",
        Some("Where the shape is in the scene. Its parents are Xforms"),
        |f| {
            f.text_vars(
                "Primitive Location",
                Some("A USD path, such as /cube or /set/props/box1. Variables are put in: /set/$OPNAME"),
                location,
                fallback,
                false,
            );
            let path = crate::types::prim_location(location, fallback);
            if path != location.trim() {
                f.status(Status::Info, format!("Used as {path}"));
            }
        },
    );
}
