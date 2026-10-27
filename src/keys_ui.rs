//! Keying in the interface: the Keys group under a node's parameters, and S to
//! key the selected node at the playhead.
//!
//! The Keys group lists every number of the selected operator that can be
//! keyed, with its value at the playhead and a diamond: red and filled on a
//! key, red and hollow when the number is keyed elsewhere, grey when it is not
//! keyed. Clicking the diamond sets a key there, or takes it away.

use crate::core::keys;
use crate::node_graph::keying::kind_of;
use crate::node_graph::NodeGraphState;
use crate::types::NodeId;

/// Red of keys, as on the timeline and in the Curve Editor.
pub fn key_red() -> egui::Color32 {
    crate::theme::c(215, 60, 50)
}

/// A diamond button: filled, hollow, or grey.
fn diamond(ui: &mut egui::Ui, on_key: bool, keyed: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
    let c = rect.center();
    let r = if resp.hovered() { 5.5 } else { 4.6 };
    let pts = vec![
        c + egui::vec2(0.0, -r),
        c + egui::vec2(r, 0.0),
        c + egui::vec2(0.0, r),
        c + egui::vec2(-r, 0.0),
    ];
    let red = key_red();
    let grey = ui.visuals().weak_text_color();
    let (fill, edge) = match (on_key, keyed) {
        (true, _) => (red, red),
        (false, true) => (egui::Color32::TRANSPARENT, red),
        _ => (egui::Color32::TRANSPARENT, grey),
    };
    ui.painter()
        .add(egui::Shape::convex_polygon(pts, fill, egui::Stroke::new(1.4, edge)));
    resp
}

/// What a row of the Keys group asked for, from its right-click menu.
enum RowAsk {
    CopyReference(String),
    PasteReference(String),
    RemoveReference(String),
    Promote(String, f64),
    RemovePromotion(String),
}

/// The number Copy as Reference copied: the operator, its name (to know it
/// is still the one copied from), the number's path.
#[derive(Clone)]
struct Copied {
    node: NodeId,
    name: String,
    path: String,
}

fn copied_id() -> egui::Id {
    egui::Id::new("reference_copied")
}

/// The Keys group of the selected operator, under its parameters: every
/// number that can be keyed, its diamond, its value at the playhead. A
/// number's right-click menu makes it follow another (Copy as Reference on
/// the source, Paste as Reference on it) and, inside an editable compound,
/// promotes it to the compound's parameters. Returns what to tell.
pub fn channel_box(ui: &mut egui::Ui, graph: &mut NodeGraphState, id: NodeId) -> Option<String> {
    let node = graph.nodes.iter().find(|n| n.id == id)?;
    let values = graph.keyable_values(id);
    if values.is_empty() {
        return None;
    }
    let kind = kind_of(&node.node_type);
    let node_name = node.name.clone();
    // A Local Animation's keys are on its own frames.
    let frame = graph.node_frame(node).round();
    let animated = !node.channels.is_empty() || !node.links.is_empty();
    // Each number: path, value, keyed, keyed here, what it follows, what it
    // is promoted as.
    let promoting = !graph.inside.path.is_empty() && !graph.inside.locked;
    let rows: Vec<(String, f64, bool, bool, Option<String>, Option<String>)> = values
        .into_iter()
        .map(|(p, v)| {
            let ch = node.channels.iter().find(|c| c.path == p);
            let keyed = ch.is_some();
            let on = ch.and_then(|c| c.key_at(frame)).is_some();
            let follows = node.links.iter().find(|l| l.path == p).map(|l| {
                let source = graph
                    .nodes
                    .iter()
                    .find(|n| n.id == l.node)
                    .map_or("(gone)".to_string(), |n| n.name.clone());
                format!("{source} {}", keys::label(&l.from))
            });
            let promoted = graph
                .inside
                .parameters
                .iter()
                .find(|q| q.targets.iter().any(|t| t.node == node_name && t.path == p))
                .map(|q| {
                    if q.label.is_empty() {
                        q.name.clone()
                    } else {
                        q.label.clone()
                    }
                });
            (p, v, keyed, on, follows, promoted)
        })
        .collect();
    let copied: Option<Copied> = ui.data(|d| d.get_temp(copied_id()));
    let mut toggle: Option<(String, bool)> = None;
    let mut ask: Option<RowAsk> = None;
    let (mut key_all, mut delete_here) = (false, false);
    ui.add_space(6.0);
    egui::CollapsingHeader::new(egui::RichText::new("Keys").strong())
        .id_salt(("keys", id.0))
        .default_open(animated)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button("Key all")
                    .on_hover_text(
                        "Key every number below at the playhead (S over the Viewer, the graph or the timeline)",
                    )
                    .clicked()
                {
                    key_all = true;
                }
                if ui
                    .add_enabled(rows.iter().any(|r| r.3), egui::Button::new("Delete keys here"))
                    .on_hover_text("Take away this operator's keys on this frame")
                    .clicked()
                {
                    delete_here = true;
                }
                ui.label(egui::RichText::new(format!("frame {frame}")).weak());
            });
            egui::Grid::new(("keys_grid", id.0))
                .num_columns(3)
                .spacing([6.0, 2.0])
                .show(ui, |ui| {
                    for (p, v, keyed, on, follows, promoted) in &rows {
                        if let Some(source) = follows {
                            ui.add_enabled(false, egui::Label::new("🔗"))
                                .on_disabled_hover_text(format!("Follows {source}: Remove Reference to key it"));
                        } else {
                            let tip = match (on, keyed) {
                                (true, _) => "A key here: click to take it away",
                                (false, true) => "Keyed on other frames: click to key it here",
                                _ => "Not keyed: click to key it here",
                            };
                            if diamond(ui, *on, *keyed).on_hover_text(tip).clicked() {
                                toggle = Some((p.clone(), *on));
                            }
                        }
                        let text = match promoted {
                            Some(_) => format!("◆ {}", keys::label(p)),
                            None => keys::label(p).to_string(),
                        };
                        let label = egui::RichText::new(text);
                        let label = match (keyed, follows.is_some()) {
                            (true, _) => label.color(key_red()),
                            (_, true) => label
                                .color(egui::Color32::WHITE)
                                .background_color(crate::properties::form::follows_orange()),
                            _ => label,
                        };
                        let mut hint = "Right-click: Copy as Reference, Paste as Reference".to_string();
                        if let Some(source) = follows {
                            hint = format!("Follows {source}. {hint}");
                        }
                        if let Some(q) = promoted {
                            hint = format!("Promoted as {q}. {hint}");
                        }
                        let r = ui
                            .add(egui::Label::new(label).sense(egui::Sense::click()))
                            .on_hover_text(hint);
                        r.context_menu(|ui| {
                            let state = NumberState {
                                keyed: *keyed,
                                follows: follows.is_some(),
                                promoted: promoted.is_some(),
                            };
                            if let Some(a) = number_menu(ui, id, p, *v, &state, copied.as_ref(), promoting) {
                                ask = Some(a);
                            }
                        });
                        let shown = v * keys::display_scale(&kind, p);
                        ui.label(egui::RichText::new(format!("{shown:.3}")).monospace().weak());
                        ui.end_row();
                    }
                });
        });
    if let Some((p, on)) = toggle {
        if on {
            graph.remove_key(id, &p, frame);
        } else {
            graph.set_key(id, &p, frame);
        }
    }
    if key_all {
        graph.key_all(id);
    }
    if delete_here {
        graph.delete_keys_at(id, frame);
    }
    ask.and_then(|a| carry_out(ui.ctx(), graph, id, a))
}

/// What a number's menu needs to know of it.
struct NumberState {
    keyed: bool,
    follows: bool,
    promoted: bool,
}

/// The items of a number's right-click menu (in the Keys group, or on its row
/// in the parameters): references, and, inside an editable compound,
/// promotion. Returns what was chosen.
fn number_menu(
    ui: &mut egui::Ui,
    id: NodeId,
    path: &str,
    value: f64,
    state: &NumberState,
    copied: Option<&Copied>,
    promoting: bool,
) -> Option<RowAsk> {
    let mut ask = None;
    if ui.button("Copy as Reference").clicked() {
        ask = Some(RowAsk::CopyReference(path.to_string()));
        ui.close();
    }
    let paste = copied.filter(|c| !(c.node == id && c.path == path));
    let paste_label = match paste {
        Some(c) => format!("Paste as Reference: {} {}", c.name, keys::label(&c.path)),
        None => "Paste as Reference".to_string(),
    };
    if ui
        .add_enabled(paste.is_some() && !state.keyed, egui::Button::new(paste_label))
        .on_disabled_hover_text(if state.keyed {
            "Keyed: take its keys away to make it follow another"
        } else {
            "Copy as Reference on another number first"
        })
        .clicked()
    {
        ask = Some(RowAsk::PasteReference(path.to_string()));
        ui.close();
    }
    if state.follows && ui.button("Remove Reference").clicked() {
        ask = Some(RowAsk::RemoveReference(path.to_string()));
        ui.close();
    }
    if promoting {
        ui.separator();
        if ui
            .button("Promote…")
            .on_hover_text("Put this number on the compound's parameters: its instances set it")
            .clicked()
        {
            ask = Some(RowAsk::Promote(path.to_string(), value));
            ui.close();
        }
        if state.promoted && ui.button("Remove Promotion").clicked() {
            ask = Some(RowAsk::RemovePromotion(path.to_string()));
            ui.close();
        }
    }
    ask
}

/// Carry out what a number's menu asked for, on operator `id`. Returns what to
/// tell.
fn carry_out(ctx: &egui::Context, graph: &mut NodeGraphState, id: NodeId, ask: RowAsk) -> Option<String> {
    let node_name = graph.nodes.iter().find(|n| n.id == id)?.name.clone();
    let copied: Option<Copied> = ctx.data(|d| d.get_temp(copied_id()));
    match ask {
        RowAsk::CopyReference(p) => {
            let c = Copied {
                node: id,
                name: node_name.clone(),
                path: p.clone(),
            };
            ctx.data_mut(|d| d.insert_temp(copied_id(), c));
            Some(format!("Copied {node_name} {} as a reference", keys::label(&p)))
        }
        RowAsk::PasteReference(p) => copied.map(|c| {
            // The operator copied from, as long as it is still there.
            if !graph.nodes.iter().any(|n| n.id == c.node && n.name == c.name) {
                return format!("{} is not in this graph: copy the reference again", c.name);
            }
            match graph.link(id, &p, c.node, &c.path) {
                Ok(()) => format!(
                    "{node_name} {} follows {} {}",
                    keys::label(&p),
                    c.name,
                    keys::label(&c.path)
                ),
                Err(e) => e,
            }
        }),
        RowAsk::RemoveReference(p) => {
            graph.unlink(id, &p);
            Some(format!("{node_name} {} is its own again", keys::label(&p)))
        }
        RowAsk::Promote(p, v) => {
            promote_open(ctx, &node_name, &p, v, graph);
            None
        }
        RowAsk::RemovePromotion(p) => {
            remove_promotion(graph, &node_name, &p);
            Some(format!("{node_name} {} is no longer promoted", keys::label(&p)))
        }
    }
}

/// A number's menu opened from its row in the parameters: the operator, the
/// number's path and value, where the pointer was.
#[derive(Clone)]
struct RowMenu {
    node: NodeId,
    path: String,
    value: f64,
    at: egui::Pos2,
}

fn row_menu_id() -> egui::Id {
    egui::Id::new("parameter_row_menu")
}

/// Open the menu of number `path` of operator `node`, right-clicked on its row
/// in the parameters (found by `properties::form`'s probe).
pub fn open_row_menu(ctx: &egui::Context, node: NodeId, path: String, value: f64, at: egui::Pos2) {
    ctx.data_mut(|d| d.insert_temp(row_menu_id(), RowMenu { node, path, value, at }));
}

/// The path of what a probed row (see `form::probing`) put the probe in place
/// of, in parameters `now` (as JSON): the number that is `form::PROBE`, the
/// text that is `form::PROBE_TEXT`, else (a row showing its number another
/// way, angles in degrees) the number that changed from `before`.
pub fn probed_path(now: &serde_json::Value, before: Option<&serde_json::Value>) -> Option<String> {
    let probe = crate::properties::form::PROBE;
    let numbers = keys::keyable(now);
    if let Some((p, _)) = numbers.iter().find(|(_, x)| (*x - probe).abs() < 0.5) {
        return Some(p.clone());
    }
    if let Some(p) = text_at(now, crate::properties::form::PROBE_TEXT) {
        return Some(p);
    }
    let was: Vec<(String, f64)> = before.map(keys::keyable).unwrap_or_default();
    numbers
        .iter()
        .find(|(p, x)| {
            was.iter()
                .any(|(q, y)| q == p && (x - y).abs() > 1e-9 * (1.0 + y.abs()))
        })
        .map(|(p, _)| p.clone())
}

/// The path of text `text` in parameters `v` (as JSON, the variant's name
/// above them skipped, as keys' paths are).
fn text_at(v: &serde_json::Value, text: &str) -> Option<String> {
    fn walk(v: &serde_json::Value, path: String, text: &str) -> Option<String> {
        let join = |k: &str| {
            if path.is_empty() {
                k.to_string()
            } else {
                format!("{path}/{k}")
            }
        };
        match v {
            serde_json::Value::String(s) if s == text => Some(path),
            serde_json::Value::Object(o) => o.iter().find_map(|(k, x)| walk(x, join(k), text)),
            serde_json::Value::Array(a) => a
                .iter()
                .enumerate()
                .find_map(|(k, x)| walk(x, join(&k.to_string()), text)),
            _ => None,
        }
    }
    let inner = match v {
        serde_json::Value::Object(o) if o.len() == 1 => o.values().next()?,
        v => v,
    };
    walk(inner, String::new(), text)
}

/// After the parameters were drawn: what a probed row put the probe in place
/// of, found in operator `id`'s parameters by its path (`probed_path`), and
/// the parameters put back as they were (`before`, taken that frame), found
/// or not. Returns the path and the number's value as the operator holds it
/// (`old` for text, or without `before`).
pub fn found_row(
    graph: &mut NodeGraphState,
    id: NodeId,
    old: f64,
    before: Option<serde_json::Value>,
) -> Option<(String, f64)> {
    let node = graph.nodes.iter_mut().find(|n| n.id == id)?;
    let mut v = serde_json::to_value(&node.node_type).ok()?;
    let path = probed_path(&v, before.as_ref());
    let was: Vec<(String, f64)> = before.as_ref().map(keys::keyable).unwrap_or_default();
    let value = path
        .as_ref()
        .and_then(|path| was.iter().find(|(p, _)| p == path))
        .map_or(old, |(_, x)| *x);
    // The parameters as they were.
    match before.and_then(|b| serde_json::from_value(b).ok()) {
        Some(t) => node.node_type = t,
        None => {
            if let Some(path) = &path {
                keys::set(&mut v, path, value);
                if let Ok(t) = serde_json::from_value(v) {
                    node.node_type = t;
                }
            }
        }
    }
    path.map(|p| (p, value))
}

/// The menu of a number right-clicked on its row, while it is open, and the
/// Promote window. Returns what to tell.
pub fn row_menu(ctx: &egui::Context, graph: &mut NodeGraphState) -> Option<String> {
    let Some(m) = ctx.data(|d| d.get_temp::<RowMenu>(row_menu_id())) else {
        return promote_window(ctx, graph);
    };
    let node = graph.nodes.iter().find(|n| n.id == m.node)?;
    let state = NumberState {
        keyed: node.channels.iter().any(|c| c.path == m.path),
        follows: node.links.iter().any(|l| l.path == m.path),
        promoted: graph
            .inside
            .parameters
            .iter()
            .any(|q| q.targets.iter().any(|t| t.node == node.name && t.path == m.path)),
    };
    let promoting = !graph.inside.path.is_empty() && !graph.inside.locked;
    let copied: Option<Copied> = ctx.data(|d| d.get_temp(copied_id()));
    let mut ask = None;
    let area = egui::Area::new(row_menu_id())
        .order(egui::Order::Foreground)
        .fixed_pos(m.at)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.label(
                    egui::RichText::new(format!("{} {}", node.name, keys::label(&m.path)))
                        .small()
                        .weak(),
                );
                ask = number_menu(ui, m.node, &m.path, m.value, &state, copied.as_ref(), promoting);
            });
        });
    let away = ctx.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer();
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if ask.is_some() || away || escape {
        ctx.data_mut(|d| d.remove::<RowMenu>(row_menu_id()));
    }
    let told = ask.and_then(|a| carry_out(ctx, graph, m.node, a));
    promote_window(ctx, graph).or(told)
}

/// What the Promote window is filling in.
#[derive(Clone)]
struct Promoting {
    node: String,
    path: String,
    /// Its value now: the parameter's default.
    default: serde_json::Value,
    /// A new parameter (None) or another target for this one.
    into: Option<String>,
    label: String,
    group: String,
    range: bool,
    min: f64,
    max: f64,
}

fn promoting_id() -> egui::Id {
    egui::Id::new("promote_window")
}

/// Open the Promote window for parameter `path` (a number, text) of inner
/// operator `node`; `value`, a number's, when it cannot be read there.
fn promote_open(ctx: &egui::Context, node: &str, path: &str, value: f64, graph: &NodeGraphState) {
    let label = keys::label(path);
    let default = graph
        .nodes
        .iter()
        .find(|n| n.name == node)
        .and_then(|n| serde_json::to_value(&n.node_type).ok())
        .and_then(|v| crate::compounds::get(&v, path).cloned())
        .unwrap_or_else(|| serde_json::json!(value));
    let p = Promoting {
        node: node.to_string(),
        path: path.to_string(),
        default,
        into: None,
        label: format!("{node} {label}"),
        group: graph.inside.path.last().cloned().unwrap_or_default(),
        range: false,
        min: 0.0,
        max: 1.0,
    };
    ctx.data_mut(|d| d.insert_temp(promoting_id(), p));
}

/// The Promote window, while it is open: a new parameter's label, group and
/// range, or another parameter this number becomes a target of. Returns what
/// to tell once promoted.
fn promote_window(ctx: &egui::Context, graph: &mut NodeGraphState) -> Option<String> {
    let mut p: Promoting = ctx.data(|d| d.get_temp(promoting_id()))?;
    let mut open = true;
    let mut done = false;
    let existing: Vec<String> = graph.inside.parameters.iter().map(|q| q.name.clone()).collect();
    egui::Window::new("Promote")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(format!(
                "{} {} onto the compound's parameters",
                p.node,
                keys::label(&p.path)
            ));
            egui::Grid::new("promote_grid").num_columns(2).show(ui, |ui| {
                ui.label("As");
                egui::ComboBox::from_id_salt("promote_into")
                    .selected_text(p.into.clone().unwrap_or_else(|| "A new parameter".into()))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut p.into, None, "A new parameter");
                        for name in &existing {
                            ui.selectable_value(&mut p.into, Some(name.clone()), format!("Another target of {name}"));
                        }
                    });
                ui.end_row();
                if p.into.is_none() {
                    ui.label("Label");
                    ui.text_edit_singleline(&mut p.label);
                    ui.end_row();
                    ui.label("Group");
                    ui.text_edit_singleline(&mut p.group);
                    ui.end_row();
                    ui.label("Range");
                    ui.add_enabled_ui(p.default.is_number(), |ui| {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut p.range, "");
                            ui.add_enabled(
                                p.range,
                                egui::DragValue::new(&mut p.min).update_while_editing(false).speed(0.01),
                            );
                            ui.add_enabled(
                                p.range,
                                egui::DragValue::new(&mut p.max).update_while_editing(false).speed(0.01),
                            );
                        })
                    });
                    ui.end_row();
                }
            });
            if ui.button("Promote").clicked() {
                done = true;
            }
        });
    if done {
        let told = promote(graph, &p);
        ctx.data_mut(|d| d.remove::<Promoting>(promoting_id()));
        return Some(told);
    }
    if open {
        ctx.data_mut(|d| d.insert_temp(promoting_id(), p));
    } else {
        ctx.data_mut(|d| d.remove::<Promoting>(promoting_id()));
    }
    None
}

/// Put a parameter (a number, text) on the compound's parameters, as `p` says:
/// a new parameter, of its kind, its default its value now, or another target
/// of one. Returns what to tell.
fn promote(graph: &mut NodeGraphState, p: &Promoting) -> String {
    use crate::compounds::{Parameter, ParameterType, Target};
    let kind = ParameterType::of(&p.default);
    let target = Target {
        node: p.node.clone(),
        path: p.path.clone(),
    };
    let params = &mut graph.inside.parameters;
    if let Some(name) = &p.into {
        if let Some(q) = params.iter_mut().find(|q| q.name == *name) {
            if !q.targets.contains(&target) {
                q.targets.push(target);
            }
            return format!("{} {} is another target of {name}", p.node, keys::label(&p.path));
        }
    }
    let base: String = p
        .label
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let base = if base.is_empty() { "parameter".to_string() } else { base };
    let name = (1..)
        .map(|k| if k == 1 { base.clone() } else { format!("{base}{k}") })
        .find(|n| !params.iter().any(|q| q.name == *n))
        .unwrap_or_default();
    params.push(Parameter {
        name: name.clone(),
        label: p.label.trim().to_string(),
        kind,
        default: p.default.clone(),
        range: (p.range && p.default.is_number()).then_some([p.min, p.max]),
        group: p.group.trim().to_string(),
        help: String::new(),
        targets: vec![target],
    });
    format!(
        "Promoted as {}: the compound's instances set it, once you leave the compound",
        p.label.trim()
    )
}

/// Take number `path` of inner operator `node` off the compound's parameters:
/// a parameter left with no target goes.
fn remove_promotion(graph: &mut NodeGraphState, node: &str, path: &str) {
    for q in &mut graph.inside.parameters {
        q.targets.retain(|t| !(t.node == node && t.path == path));
    }
    graph.inside.parameters.retain(|q| !q.targets.is_empty());
}

/// S, tapped: key every number of the selected operator at the playhead. A tap,
/// so a navigation style that holds S to move the camera keeps it: a mouse
/// button pressed while S is down makes it a hold. Not while typing.
pub fn s_key(ctx: &egui::Context, graph: &mut NodeGraphState) -> Option<String> {
    if ctx.egui_wants_keyboard_input() {
        return None;
    }
    let (pressed, released, mouse) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::S) && !i.modifiers.any(),
            i.key_released(egui::Key::S),
            i.pointer.any_down(),
        )
    });
    let id_used = egui::Id::new("s_key_used");
    let mut used: bool = ctx.data(|d| d.get_temp(id_used)).unwrap_or(false);
    if pressed {
        used = false;
    }
    if mouse && ctx.input(|i| i.key_down(egui::Key::S)) {
        used = true;
    }
    ctx.data_mut(|d| d.insert_temp(id_used, used));
    if !released || used {
        return None;
    }
    let id = graph.selected_node?;
    let n = graph.key_all(id);
    let name = graph
        .nodes
        .iter()
        .find(|x| x.id == id)
        .map(|x| x.name.clone())
        .unwrap_or_default();
    let at = graph
        .nodes
        .iter()
        .find(|x| x.id == id)
        .map(|x| graph.node_frame(x))
        .unwrap_or(0.0)
        .round();
    Some(format!(
        "Keyed {n} number{} of {name} at frame {at}",
        if n == 1 { "" } else { "s" }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn promoting(node: &str, path: &str, into: Option<&str>, label: &str) -> Promoting {
        Promoting {
            node: node.into(),
            path: path.into(),
            default: serde_json::json!(2.5),
            into: into.map(str::to_string),
            label: label.into(),
            group: "Shape".into(),
            range: true,
            min: 0.0,
            max: 10.0,
        }
    }

    /// A row probed put the probe in its number's place: found by its path,
    /// the value is put back. Not found, the parameters are put back all the
    /// same.
    #[test]
    fn a_probed_row_is_found_by_its_path() {
        use crate::types::NodeType;
        let mut g = NodeGraphState::default();
        let id = g.add_node(
            "Move".into(),
            NodeType::Transform {
                translation: glam::Vec3::new(0.0, crate::properties::form::PROBE as f32, 0.0),
                rotation: glam::Vec3::ZERO,
                scale: glam::Vec3::ONE,
            },
            egui::pos2(0.0, 0.0),
        );
        let found = found_row(&mut g, id, 1.5, None);
        assert_eq!(found, Some(("translation/1".to_string(), 1.5)));
        assert!(matches!(&g.nodes[0].node_type, NodeType::Transform { translation, .. } if translation.y == 1.5));

        // A row in degrees: the probe reaches the operator as radians, found
        // by what changed; the parameters are put back as they were.
        let before = serde_json::to_value(&g.nodes[0].node_type).unwrap();
        if let NodeType::Transform { rotation, .. } = &mut g.nodes[0].node_type {
            rotation.z = (crate::properties::form::PROBE as f32).to_radians();
        }
        let found = found_row(&mut g, id, 0.0, Some(before.clone()));
        assert_eq!(found.map(|f| f.0).as_deref(), Some("rotation/2"));
        assert!(matches!(&g.nodes[0].node_type, NodeType::Transform { rotation, .. } if rotation.z == 0.0));

        // A row whose number is not one of the operator's: nothing found, and
        // nothing kept of what it wrote.
        if let NodeType::Transform { scale, .. } = &mut g.nodes[0].node_type {
            scale.x = 1.0 + 1e-12_f32;
        }
        assert_eq!(found_row(&mut g, id, 0.0, Some(before.clone())), None);
        assert_eq!(serde_json::to_value(&g.nodes[0].node_type).unwrap(), before);
    }

    /// Promote makes a parameter, its default the number's value; again on
    /// another number, another target of it; Remove Promotion takes targets
    /// away, and a parameter with none goes.
    #[test]
    fn numbers_are_promoted_and_taken_back() {
        let mut g = NodeGraphState::default();
        promote(&mut g, &promoting("Move", "translation/1", None, "Height"));
        let q = &g.inside.parameters[0];
        assert_eq!(
            (q.name.as_str(), q.label.as_str(), q.group.as_str()),
            ("height", "Height", "Shape")
        );
        assert_eq!(
            (q.default.clone(), q.range),
            (serde_json::json!(2.5), Some([0.0, 10.0]))
        );
        promote(&mut g, &promoting("Move2", "translation/1", Some("height"), ""));
        assert_eq!(g.inside.parameters.len(), 1);
        assert_eq!(g.inside.parameters[0].targets.len(), 2);
        promote(&mut g, &promoting("Move", "scale/0", None, "Height"));
        assert_eq!(g.inside.parameters[1].name, "height2", "names are not taken twice");
        remove_promotion(&mut g, "Move", "translation/1");
        assert_eq!(g.inside.parameters[0].targets.len(), 1);
        remove_promotion(&mut g, "Move2", "translation/1");
        assert_eq!(g.inside.parameters.len(), 1);
    }
}
