//! The Parameters pane for the MaterialX operator: the operator itself, and the
//! node selected in its graph.
//!
//! A node's parameters are its definition's inputs, laid out from what the
//! library says of them: grouped by their folder (`uifolder`), labelled by
//! their `uiname`, a slider where they have limits, a colour swatch for
//! colours, a list for their choices, a file field for file names. An input
//! that is connected shows where from, and can be taken off. An input set on
//! the node can be put back to its default (↺). Advanced inputs show when asked
//! for.

use xms_matx::{Binding, Graph, Input, Type, Value};

use super::lib;
use crate::file_browser::{BrowseMode, BrowseTarget};
use crate::properties::form::{group, group_with, FieldExt, Status};
use crate::properties::ui::PanelIo;
use crate::types::NodeId;

/// The operator, selected in the scene graph: what it makes, how to open its
/// graph, and its graph read from or written to a `.mtlx` file.
pub fn operator(ui: &mut egui::Ui, op: NodeId, name: &str, graph: &Graph, io: &mut PanelIo) {
    group(ui, "Material", Some("Material Binding puts it in the scene under its Material Library Location (/materials by default), by this name. Rename the node to rename it"), |f| {
        f.value("Prim name", crate::core::material::prim_name(name));
        f.buttons("MaterialX file", None, |ui| {
            if ui.button("Load .mtlx...").on_hover_text("Replace the graph with a material of a MaterialX file: the one of this operator's name, else the first").clicked() {
                io.browser.open(BrowseTarget::MatxLoad(op), BrowseMode::File, "Load a MaterialX material", &["mtlx"], "");
            }
            if ui.button("Save .mtlx...").on_hover_text("Write the graph as a MaterialX document").clicked() {
                io.browser.open(BrowseTarget::MatxSave(op), BrowseMode::Save, "Save as MaterialX", &["mtlx"], &format!("{}.mtlx", crate::core::material::prim_name(name)));
            }
        });
    });
    summary(ui, graph);
    group(ui, "Graph", None, |f| {
        f.value("Nodes", graph.nodes.len().to_string());
        f.status(
            Status::Info,
            "Double-click the node, or select it and press ↓, to open its MaterialX graph",
        );
    });
}

/// What the material is made of, and what the Viewer shows of it.
fn summary(ui: &mut egui::Ui, graph: &Graph) {
    let lib = lib();
    group(ui, "Shader", Some("The surface shader feeding the graph's surfacematerial: what Write USD writes, with every node it reads, as a MaterialX network"), |f| {
        match (graph.material(), graph.surface_shader()) {
            (None, _) => f.status(Status::Bad, "No surfacematerial node: the graph puts out no material"),
            (Some(_), None) => f.status(Status::Bad, "Nothing goes into the surfacematerial's surfaceshader"),
            (Some(_), Some(s)) => {
                let node = graph.node(s);
                f.value("Surface", node.map(|n| format!("{} ({})", n.name, n.node)).unwrap_or_default());
                f.value("Definition", node.map(|n| n.def.clone()).unwrap_or_default());
                let p = xms_matx::preview(graph, lib);
                match p.shader {
                    Some(_) => {
                        let tex: Vec<&str> = [&p.color.file, &p.roughness.file, &p.metallic.file, &p.opacity.file, &p.emission.file, &p.normal]
                            .into_iter().flatten().map(|f| f.rsplit(['/', '\\']).next().unwrap_or(f.as_str())).collect();
                        let what = if tex.is_empty() { "its values".to_string() } else { format!("its values and {}", tex.join(", ")) };
                        f.status(Status::Info, format!("The Viewer shows {what}"));
                    }
                    None => f.status(Status::Info, "The Viewer shows a grey: it reads standard_surface, open_pbr_surface, UsdPreviewSurface and gltf_pbr"),
                }
            }
        }
        let wrong = graph.nodes.iter().flat_map(|n| n.inputs.iter().map(move |(i, _)| (n.id, i.clone())))
            .filter(|(id, i)| !graph.wire_ok(lib, *id, i)).count();
        if wrong > 0 { f.status(Status::Bad, format!("{wrong} wire(s) between types that do not join: drawn red")); }
        // What a renderer would fail on: Imago does not render such a material.
        let problems = xms_matx::check::problems(graph, lib, None);
        for p in problems.iter().take(6) {
            f.status(Status::Bad, format!("A renderer would fail: {p}"));
        }
        if problems.len() > 6 { f.status(Status::Bad, format!("and {} more", problems.len() - 6)); }
    });
}

fn advanced_id(op: NodeId) -> egui::Id {
    egui::Id::new(("materialx_advanced", op.0))
}

/// The parameters while the operator's graph is open: the selected node's, or
/// the operator's when none is.
pub fn inner(ui: &mut egui::Ui, op: NodeId, op_name: &str, graph: &mut Graph, io: &mut PanelIo) {
    let lib = lib();
    let view = super::view(ui.ctx(), op);
    let Some(id) = view.selected.filter(|s| graph.node(*s).is_some()) else {
        operator(ui, op, op_name, graph, io);
        group(ui, "Node", None, |f| {
            f.status(Status::Info, "Select a node in the graph to see its parameters")
        });
        return;
    };
    let Some(def) = graph.def(lib, id) else {
        let name = graph.node(id).map(|n| n.def.clone()).unwrap_or_default();
        let kept = graph.missing.iter().any(|m| m.name == name);
        group(ui, "Node", None, |f| {
            f.status(
                Status::Bad,
                match kept {
                    // A renderer's shader from its SDK, the SDK's not loaded.
                    true => format!("{name}: its definition is not loaded (Settings > Renderers > Shaders from the SDK, or its renderer not installed here): its values and wires are kept, and written as they were"),
                    false => format!("{name} is not in the MaterialX library"),
                },
            )
        });
        return;
    };
    let node = graph.node(id).cloned().unwrap();

    // The node
    let title = match &def.ui_name {
        Some(n) => format!("{}  ({n})", node.node),
        None => node.node.clone(),
    };
    let mut name = node.name.clone();
    group(ui, &title, def.doc.as_deref(), |f| {
        if f.text(
            "Name",
            Some("Letters, digits and _; unique in the graph"),
            &mut name,
            "",
        )
        .changed()
        {
            name = graph.rename(id, &name);
        }
        f.value("Definition", def.name.clone());
        if let Some(v) = &def.version {
            f.value("Version", v.clone());
        }
    });

    // A `ramp`: its keys as a ramp, in place of their inputs.
    let keys = xms_matx::ramp::read(graph, lib, id);
    if let Some(k) = &keys {
        ramp_group(ui, op, id, k, graph);
    }
    // Arnold's ramp_rgb and ramp_float: their arrays as a ramp.
    let arnold = xms_matx::ramp::read_arnold(graph, lib, id);
    if let Some(k) = &arnold {
        arnold_ramp_group(ui, op, id, k, graph);
    }

    // Its inputs, by folder
    let mut advanced = ui.data(|d| d.get_temp::<bool>(advanced_id(op))).unwrap_or(false);
    let has_advanced = def.inputs.iter().any(|i| i.ui.advanced && i.ui.visible);
    let mut folders: Vec<String> = vec![];
    for i in &def.inputs {
        let folder = i.ui.folder.clone().unwrap_or_else(|| "Inputs".into());
        if !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    for (k, folder) in folders.iter().enumerate() {
        let inputs: Vec<(usize, &Input)> = def
            .inputs
            .iter()
            .enumerate()
            .filter(|(_, i)| i.ui.folder.as_deref().unwrap_or("Inputs") == folder)
            .filter(|(_, i)| i.ui.visible && (advanced || !i.ui.advanced || node.binding(&i.name).is_some()))
            .filter(|(_, i)| keys.is_none() || !xms_matx::ramp::is_key_input(&i.name))
            .filter(|(_, i)| arnold.is_none() || !xms_matx::ramp::is_arnold_key_input(&i.name))
            .collect();
        if inputs.is_empty() {
            continue;
        }
        let body = |f: &mut crate::properties::form::Rows| {
            for (index, input) in &inputs {
                input_row(f, op, id, *index, input, graph, io);
            }
        };
        // The advanced switch on the first group's title.
        if k == 0 && has_advanced {
            group_with(
                ui,
                folder,
                None,
                |ui| {
                    ui.checkbox(&mut advanced, "Advanced")
                        .on_hover_text("Show the inputs the library marks advanced");
                },
                body,
            );
        } else {
            group(ui, folder, None, body);
        }
    }
    ui.data_mut(|d| d.insert_temp(advanced_id(op), advanced));
    // What the node puts out.
    group(ui, "Outputs", None, |f| {
        for o in &def.outputs {
            f.value(&o.name, o.ty.name());
        }
    });
}

/// A `ramp` node's keys in the ramp widget: its colours along the bar, its
/// interpolation (Linear, Smooth, Step, as MaterialX has them), two to ten
/// keys. Shown as **Float** instead, a curve: each key's value its grey (R, G
/// and B the value, alpha 1), for a ramp read as a float (its `out` through an
/// `extract` or `convert`). MaterialX has no float `ramp`; `ramplr`,
/// `ramptb` and `ramp4` are two or four values blended, edited as they are.
fn ramp_group(ui: &mut egui::Ui, op: NodeId, id: u32, keys: &xms_matx::ramp::Keys, graph: &mut Graph) {
    use crate::ramp::{colour_ramp, float_ramp, Interp, Key, Limits, Ramp};
    let interp = match keys.interpolation {
        0 => Interp::Linear,
        2 => Interp::Step,
        _ => Interp::Smooth,
    };
    let limits = Limits {
        interps: &[Interp::Linear, Interp::Smooth, Interp::Step],
        min_keys: xms_matx::ramp::MIN_KEYS,
        max_keys: xms_matx::ramp::MAX_KEYS,
        alpha: true,
    };
    let widget = egui::Id::new(("matx_ramp", op.0, id));
    // Float or colour, as last chosen for it: colour to start with.
    let mut as_float = ui.data(|d| d.get_temp::<bool>(widget.with("float"))).unwrap_or(false);
    let mut out: Option<Vec<(f32, [f32; 4])>> = None;
    let mut new_interp = interp;
    group(
        ui,
        "Ramp",
        Some("Its keys along the ramp: click for a key, drag one to move it, right-click it to take it away; each key listed under it with its place and value. Two to ten keys, as MaterialX's ramp takes. Colour: linear colours, shown as sRGB. Float: a curve, each key's value its grey, for a ramp read as a float"),
        |f| {
            let ui = &mut *f.ui;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut as_float, false, "Colour")
                    .on_hover_text("Its keys as colours along a bar");
                ui.selectable_value(&mut as_float, true, "Float")
                    .on_hover_text("Its keys as values on a curve: each colour's grey (the mean of R, G and B), written back as R, G and B with alpha 1");
            });
            if as_float {
                let mut ramp = Ramp {
                    keys: keys
                        .keys
                        .iter()
                        .map(|&(pos, c)| Key { pos, value: (c[0] + c[1] + c[2]) / 3.0 })
                        .collect(),
                    interp,
                };
                if float_ramp(ui, widget.with("f"), &mut ramp, (0.0, 1.0), &limits) {
                    out = Some(ramp.keys.iter().map(|k| (k.pos, [k.value, k.value, k.value, 1.0])).collect());
                    new_interp = ramp.interp;
                }
            } else {
                let mut ramp = Ramp {
                    keys: keys.keys.iter().map(|&(pos, value)| Key { pos, value }).collect(),
                    interp,
                };
                if colour_ramp(ui, widget, &mut ramp, &limits) {
                    out = Some(ramp.keys.iter().map(|k| (k.pos, k.value)).collect());
                    new_interp = ramp.interp;
                }
            }
        },
    );
    ui.data_mut(|d| d.insert_temp(widget.with("float"), as_float));
    if let Some(keys) = out {
        let interpolation = match new_interp {
            Interp::Linear => 0,
            Interp::Step => 2,
            _ => 1,
        };
        xms_matx::ramp::write(graph, id, &xms_matx::ramp::Keys { keys, interpolation });
    }
}

/// Arnold's interpolations by their numbers: 0 constant, 1 linear, 2
/// Catmull-Rom, 3 monotone cubic.
const ARNOLD_INTERPS: [(i32, crate::ramp::Interp); 4] = [
    (0, crate::ramp::Interp::Step),
    (1, crate::ramp::Interp::Linear),
    (2, crate::ramp::Interp::Cubic),
    (3, crate::ramp::Interp::MonotoneCubic),
];

/// Arnold's `ramp_rgb` (a colour bar, RGB) or `ramp_float` (a curve) in the
/// ramp widget, in place of its position, colour or value and interpolation
/// arrays: as many keys as wanted, one interpolation for them all (Step,
/// Linear, Cubic: Catmull-Rom, Monotone Cubic). Keys of differing
/// interpolations, as read, keep theirs until one is chosen.
fn arnold_ramp_group(ui: &mut egui::Ui, op: NodeId, id: u32, keys: &xms_matx::ramp::ArnoldKeys, graph: &mut Graph) {
    use crate::ramp::{colour_ramp, float_ramp, Interp, Key, Limits, Ramp};
    let shown = keys
        .interpolation
        .and_then(|i| ARNOLD_INTERPS.iter().find(|(n, _)| *n == i))
        .map(|(_, t)| *t)
        .unwrap_or(Interp::Cubic);
    let limits = Limits {
        interps: &[Interp::Step, Interp::Linear, Interp::Cubic, Interp::MonotoneCubic],
        min_keys: 1,
        max_keys: 256,
        alpha: false,
    };
    let widget = egui::Id::new(("arnold_ramp", op.0, id));
    let mut out: Option<(Vec<(f32, [f32; 4])>, Interp)> = None;
    group(
        ui,
        "Ramp",
        Some("Its keys along the ramp: click for a key, drag one to move it, right-click it to take it away; each key listed under it with its place and value. Written as Arnold's position, color (or value) and interpolation arrays, one interpolation for every key"),
        |f| {
            if keys.interpolation.is_none() {
                f.status(
                    Status::Info,
                    "Its keys' interpolations differ: kept until one is chosen",
                );
            }
            let ui = &mut *f.ui;
            if keys.colour {
                let mut ramp = Ramp {
                    keys: keys.keys.iter().map(|&(pos, value)| Key { pos, value }).collect(),
                    interp: shown,
                };
                if colour_ramp(ui, widget, &mut ramp, &limits) {
                    out = Some((ramp.keys.iter().map(|k| (k.pos, k.value)).collect(), ramp.interp));
                }
            } else {
                let mut ramp = Ramp {
                    keys: keys.keys.iter().map(|&(pos, c)| Key { pos, value: c[0] }).collect(),
                    interp: shown,
                };
                if float_ramp(ui, widget, &mut ramp, (0.0, 1.0), &limits) {
                    out = Some((
                        ramp.keys.iter().map(|k| (k.pos, [k.value, k.value, k.value, 1.0])).collect(),
                        ramp.interp,
                    ));
                }
            }
        },
    );
    if let Some((k, interp)) = out {
        // The interpolation chosen, or, untouched where the keys differ, theirs.
        let interpolation = if keys.interpolation.is_none() && interp == shown {
            None
        } else {
            ARNOLD_INTERPS.iter().find(|(_, t)| *t == interp).map(|(n, _)| *n)
        };
        let new = xms_matx::ramp::ArnoldKeys {
            colour: keys.colour,
            keys: k,
            interpolation,
        };
        xms_matx::ramp::write_arnold(graph, lib(), id, &new);
    }
}

/// One input: where it is connected from, or its value.
fn input_row(
    f: &mut crate::properties::form::Rows,
    op: NodeId,
    id: u32,
    index: usize,
    input: &Input,
    graph: &mut Graph,
    io: &mut PanelIo,
) {
    let lib = lib();
    let label = input.label().to_string();
    let hint = input
        .doc
        .clone()
        .unwrap_or_else(|| format!("{} ({})", input.name, input.ty));
    // Connected: from where, and a way to take it off.
    if let Some((src, output)) = graph.source(id, &input.name) {
        let from = graph.node(src).map(|n| n.name.clone()).unwrap_or_default();
        let ok = graph.wire_ok(lib, id, &input.name);
        let mut off = false;
        f.row(&label, Some(hint.as_str()), |ui| {
            ui.horizontal(|ui| {
                let text = if output == "out" {
                    format!("← {from}")
                } else {
                    format!("← {from}.{output}")
                };
                let text = egui::RichText::new(text).monospace();
                ui.label(if ok {
                    text
                } else {
                    text.color(crate::properties::form::ink::BAD())
                });
                if ui.small_button("✕").on_hover_text("Take the connection off").clicked() {
                    off = true;
                }
            });
        });
        if off {
            graph.disconnect(id, &input.name);
        }
        return;
    }
    if input.ty.is_shading() {
        f.row(&label, Some(hint.as_str()), |ui| {
            ui.label(egui::RichText::new("not connected").italics())
        });
        return;
    }
    let set = matches!(
        graph.node(id).and_then(|n| n.binding(&input.name)),
        Some(Binding::Value(_))
    );
    let Some(mut v) = graph.value(lib, id, &input.name).cloned().or_else(|| empty(input.ty)) else {
        f.row(&label, Some(hint.as_str()), |ui| ui.label(input.ty.name()));
        return;
    };
    let mut reset = false;
    let mut browse = false;
    let changed = f.row(&label, Some(hint.as_str()), |ui| {
        ui.horizontal(|ui| {
            // Room for the reset button.
            let room = if set { 24.0 } else { 0.0 };
            let changed = ui
                .allocate_ui(
                    egui::vec2((ui.available_width() - room).max(40.0), ui.spacing().interact_size.y),
                    |ui| value_widget(ui, input, &mut v, &mut browse),
                )
                .inner;
            if set && ui.small_button("↺").on_hover_text("Back to the default").clicked() {
                reset = true;
            }
            changed
        })
        .inner
    });
    if browse {
        io.browser.open(
            BrowseTarget::MatxFile(op, id, index as u16),
            BrowseMode::File,
            &format!("File for {label}"),
            &["png", "jpg", "jpeg", "exr", "tif", "tiff", "tga", "hdr", "tx"],
            "",
        );
    }
    if reset {
        graph.reset(id, &input.name);
    } else if changed {
        graph.set_value(id, &input.name, v);
    }
    // A texture's colour space, under its file: MaterialX's and USD's own
    // nodes (a renderer's image node has its own input for it).
    let own = graph.def(lib, id).is_some_and(|d| d.target.is_none());
    if input.ty == Type::Filename && own {
        space_row(f, id, &input.name, graph);
    }
}

/// The colour spaces offered first: MaterialX's names, which OpenColorIO
/// configs know as aliases (Imago's, ACES's).
const SPACES: [(&str, &str); 5] = [
    ("srgb_texture", "sRGB texture: 8-bit colour maps"),
    ("lin_rec709", "Linear Rec.709: linear colour, USD's default"),
    ("acescg", "ACEScg: linear, Imago's rendering space"),
    ("lin_displayp3", "Linear P3-D65"),
    ("raw", "Raw: data, not converted (roughness, normals, masks)"),
];

/// The Colour Space of an input: Auto (none given: the graph's, else the
/// renderer's rule, 8-bit sRGB and float linear), one of MaterialX's, or any
/// of the OCIO config's.
fn space_row(f: &mut crate::properties::form::Rows, id: u32, input: &str, graph: &mut Graph) {
    let lib = lib();
    let given = graph.node(id).and_then(|n| n.space(input)).map(str::to_string);
    let from_graph = match &given {
        None => graph.space_of(lib, id, input),
        Some(_) => None,
    };
    let auto = match &from_graph {
        Some(s) => format!("Auto ({s}, the document's)"),
        None => "Auto".to_string(),
    };
    let shown = given.clone().unwrap_or_else(|| auto.clone());
    let mut pick: Option<Option<String>> = None;
    f.row(
        "Colour Space",
        Some("What the texture's colours are in, converted from to the rendering space by the renderer and the Viewer. Auto: the renderer's rule (8-bit files sRGB, float files linear). Raw for data: roughness, normals, masks. Written as the file input's colorSpace"),
        |ui| {
            egui::ComboBox::from_id_salt(("matx_space", id, input))
                .selected_text(shown.clone())
                .width(ui.available_width() - 8.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(given.is_none(), auto.as_str()).clicked() {
                        pick = Some(None);
                    }
                    for (name, what) in SPACES {
                        let r = ui.selectable_label(given.as_deref() == Some(name), name).on_hover_text(what);
                        if r.clicked() {
                            pick = Some(Some(name.to_string()));
                        }
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("The OCIO config's").weak());
                    for name in crate::colour::Colour::get().spaces() {
                        if ui.selectable_label(given.as_deref() == Some(name.as_str()), name.as_str()).clicked() {
                            pick = Some(Some(name));
                        }
                    }
                    // One not in either, as read: kept, shown.
                    if let Some(g) = &given {
                        let known = SPACES.iter().any(|(n, _)| n == g)
                            || crate::colour::Colour::get().spaces().iter().any(|n| n == g);
                        if !known {
                            ui.separator();
                            let _ = ui.selectable_label(true, g.as_str()).on_hover_text("As read: not a name of the config");
                        }
                    }
                })
                .response
        },
    );
    if let Some(p) = pick {
        if let Some(n) = graph.node_mut(id) {
            n.set_space(input, p.as_deref());
        }
    }
}

/// The value an input with no default starts from when edited.
fn empty(ty: Type) -> Option<Value> {
    match ty {
        Type::Boolean => Some(Value::Boolean(false)),
        Type::Integer => Some(Value::Integer(0)),
        t if t.floats() > 0 => Some(Value::Floats(vec![0.0; t.floats()])),
        Type::String | Type::Filename | Type::Geomname => Some(Value::String(String::new())),
        _ => None,
    }
}

/// The widget for a value of the input's type. True when it changed.
fn value_widget(ui: &mut egui::Ui, input: &Input, v: &mut Value, browse: &mut bool) -> bool {
    let limit = |x: &Option<Value>| x.as_ref().and_then(|v| v.as_float());
    match (input.ty, v) {
        (Type::Boolean, Value::Boolean(b)) => crate::properties::form::check(ui, b).changed(),
        // Choices of an integer: by name.
        (Type::Integer, Value::Integer(i)) if !input.choices.is_empty() => {
            let values: Vec<i32> = input
                .choice_values
                .as_deref()
                .map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect())
                .unwrap_or_else(|| (0..input.choices.len() as i32).collect());
            let current = values
                .iter()
                .position(|x| x == i)
                .and_then(|k| input.choices.get(k))
                .cloned()
                .unwrap_or_else(|| i.to_string());
            let mut changed = false;
            egui::ComboBox::from_id_salt(("matx_int", input.name.as_str()))
                .selected_text(current)
                .width(ui.available_width() - 8.0)
                .show_ui(ui, |ui| {
                    for (k, name) in input.choices.iter().enumerate() {
                        if let Some(x) = values.get(k) {
                            if ui.selectable_label(*i == *x, name).clicked() {
                                *i = *x;
                                changed = true;
                            }
                        }
                    }
                });
            changed
        }
        (Type::Integer, Value::Integer(i)) => {
            let mut d = egui::DragValue::new(i).update_while_editing(false).speed(0.1);
            if let (Some(lo), Some(hi)) = (limit(&input.ui.min), limit(&input.ui.max)) {
                d = d.range(lo as i32..=hi as i32);
            }
            let h = ui.spacing().interact_size.y;
            ui.field_sized([ui.available_width(), h], d).changed()
        }
        (Type::Float, Value::Floats(f)) if f.len() == 1 => {
            let lo = limit(&input.ui.soft_min).or(limit(&input.ui.min));
            let hi = limit(&input.ui.soft_max).or(limit(&input.ui.max));
            match (lo, hi) {
                (Some(lo), Some(hi)) if hi > lo => {
                    crate::properties::form::slider_look(ui);
                    // Past the soft limits by typing, within the hard ones.
                    let clamp = (
                        limit(&input.ui.min).unwrap_or(f32::MIN),
                        limit(&input.ui.max).unwrap_or(f32::MAX),
                    );
                    let r = ui.add(
                        egui::Slider::new(&mut f[0], lo..=hi)
                            .update_while_editing(false)
                            .max_decimals(3)
                            .clamping(egui::SliderClamping::Never),
                    );
                    f[0] = f[0].clamp(clamp.0, clamp.1);
                    r.changed()
                }
                _ => {
                    let h = ui.spacing().interact_size.y;
                    ui.field_sized(
                        [ui.available_width(), h],
                        egui::DragValue::new(&mut f[0])
                            .update_while_editing(false)
                            .speed(0.01)
                            .max_decimals(4),
                    )
                    .changed()
                }
            }
        }
        (t, Value::Floats(f)) if t.is_color() => {
            let mut changed = false;
            ui.horizontal(|ui| {
                if f.len() == 3 {
                    let mut c = [f[0], f[1], f[2]];
                    if ui.color_edit_button_rgb(&mut c).changed() {
                        f.copy_from_slice(&c);
                        changed = true;
                    }
                } else if f.len() == 4 {
                    let mut c = [f[0], f[1], f[2], f[3]];
                    if ui.color_edit_button_rgba_unmultiplied(&mut c).changed() {
                        f.copy_from_slice(&c);
                        changed = true;
                    }
                }
                changed |= floats(ui, f, 0.01);
            });
            changed
        }
        (Type::Vector2 | Type::Vector3 | Type::Vector4, Value::Floats(f)) => floats(ui, f, 0.01),
        // Choices of a string: a list.
        (Type::String, Value::String(s)) if !input.choices.is_empty() => {
            let mut changed = false;
            egui::ComboBox::from_id_salt(("matx_str", input.name.as_str()))
                .selected_text(s.clone())
                .width(ui.available_width() - 8.0)
                .show_ui(ui, |ui| {
                    for c in &input.choices {
                        if ui.selectable_label(*s == *c, c).clicked() {
                            *s = c.clone();
                            changed = true;
                        }
                    }
                });
            changed
        }
        (Type::Filename, Value::String(s)) => {
            let mut changed = false;
            ui.horizontal(|ui| {
                let w = ui.available_width() - 34.0;
                changed = crate::properties::form::typed(ui, s, |ui, t| {
                    let r = ui.field(
                        egui::TextEdit::singleline(&mut *t)
                            .hint_text("/path/to/texture.png")
                            .desired_width(w.max(40.0)),
                    );
                    crate::variables::complete(ui, &r, t, &[]);
                    r
                })
                .changed();
                if ui.button("📂").on_hover_text("Browse").clicked() {
                    *browse = true;
                }
            });
            if !s.trim().is_empty() && !crate::core::sequences::texture_exists(s) {
                ui.label(
                    egui::RichText::new("file not found")
                        .small()
                        .color(crate::properties::form::ink::BAD()),
                );
            }
            changed
        }
        (_, Value::String(s)) => crate::properties::form::typed(ui, s, |ui, t| {
            ui.field(egui::TextEdit::singleline(t).desired_width(ui.available_width()))
        })
        .changed(),
        // Matrices and arrays: as text, read back when it reads as the type.
        (t, v) => {
            let mut text = v.to_text();
            let r = crate::properties::form::typed(ui, &mut text, |ui, t| {
                ui.field(egui::TextEdit::singleline(t).desired_width(ui.available_width()))
            });
            let mut changed = false;
            if r.changed() {
                if let Some(parsed) = Value::parse(t, &text) {
                    *v = parsed;
                    changed = true;
                }
            }
            changed
        }
    }
}

/// Floats side by side, filling the width.
fn floats(ui: &mut egui::Ui, f: &mut [f32], speed: f64) -> bool {
    let n = f.len().max(1) as f32;
    let gap = ui.spacing().item_spacing.x;
    let w = ((ui.available_width() - (n - 1.0) * gap) / n).max(30.0);
    let h = ui.spacing().interact_size.y;
    let mut changed = false;
    ui.horizontal(|ui| {
        for x in f.iter_mut() {
            changed |= ui
                .field_sized(
                    [w, h],
                    egui::DragValue::new(x)
                        .update_while_editing(false)
                        .speed(speed)
                        .max_decimals(3),
                )
                .changed();
        }
    });
    changed
}
