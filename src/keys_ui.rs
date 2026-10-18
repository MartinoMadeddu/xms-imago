//! Keying in the interface: the Keys group under a node's parameters, and S
//! to key the selected node at the playhead.
//!
//! The Keys group lists every number of the selected operator that can be
//! keyed, with its value at the playhead and a diamond: red and filled on a
//! key, red and hollow when the number is keyed elsewhere, grey when it is
//! not keyed. Clicking the diamond sets a key there, or takes it away.

use crate::core::keys;
use crate::node_graph::keying::kind_of;
use crate::node_graph::NodeGraphState;
use crate::types::NodeId;

/// Red of keys, as on the timeline and in the Graph Editor.
pub fn key_red() -> egui::Color32 { crate::theme::c(215, 60, 50) }

/// A diamond button: filled, hollow, or grey.
fn diamond(ui: &mut egui::Ui, on_key: bool, keyed: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
    let c = rect.center();
    let r = if resp.hovered() { 5.5 } else { 4.6 };
    let pts = vec![c + egui::vec2(0.0, -r), c + egui::vec2(r, 0.0), c + egui::vec2(0.0, r), c + egui::vec2(-r, 0.0)];
    let red = key_red();
    let grey = ui.visuals().weak_text_color();
    let (fill, edge) = match (on_key, keyed) {
        (true, _) => (red, red),
        (false, true) => (egui::Color32::TRANSPARENT, red),
        _ => (egui::Color32::TRANSPARENT, grey),
    };
    ui.painter().add(egui::Shape::convex_polygon(pts, fill, egui::Stroke::new(1.4, edge)));
    resp
}

/// The Keys group of the selected operator, under its parameters.
pub fn channel_box(ui: &mut egui::Ui, graph: &mut NodeGraphState, id: NodeId) {
    let Some(node) = graph.nodes.iter().find(|n| n.id == id) else { return };
    let values = graph.keyable_values(id);
    if values.is_empty() { return; }
    let kind = kind_of(&node.node_type);
    let frame = graph.frame().round();
    let animated = !node.channels.is_empty();
    let rows: Vec<(String, f64, bool, bool)> = values.into_iter().map(|(p, v)| {
        let ch = node.channels.iter().find(|c| c.path == p);
        let keyed = ch.is_some();
        let on = ch.and_then(|c| c.key_at(frame)).is_some();
        (p, v, keyed, on)
    }).collect();
    let mut toggle: Option<(String, bool)> = None;
    let (mut key_all, mut delete_here) = (false, false);
    ui.add_space(6.0);
    egui::CollapsingHeader::new(egui::RichText::new("Keys").strong())
        .id_salt(("keys", id.0))
        .default_open(animated)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Key all").on_hover_text("Key every number below at the playhead (S over the Viewer, the graph or the timeline)").clicked() { key_all = true; }
                if ui.add_enabled(rows.iter().any(|r| r.3), egui::Button::new("Delete keys here")).on_hover_text("Take away this operator's keys on this frame").clicked() { delete_here = true; }
                ui.label(egui::RichText::new(format!("frame {frame}")).weak());
            });
            egui::Grid::new(("keys_grid", id.0)).num_columns(3).spacing([6.0, 2.0]).show(ui, |ui| {
                for (p, v, keyed, on) in &rows {
                    let tip = match (on, keyed) {
                        (true, _) => "A key here: click to take it away",
                        (false, true) => "Keyed on other frames: click to key it here",
                        _ => "Not keyed: click to key it here",
                    };
                    if diamond(ui, *on, *keyed).on_hover_text(tip).clicked() { toggle = Some((p.clone(), *on)); }
                    let label = egui::RichText::new(keys::label(p));
                    ui.label(if *keyed { label.color(key_red()) } else { label });
                    let shown = v * keys::display_scale(&kind, p);
                    ui.label(egui::RichText::new(format!("{shown:.3}")).monospace().weak());
                    ui.end_row();
                }
            });
        });
    if let Some((p, on)) = toggle {
        if on { graph.remove_key(id, &p, frame); } else { graph.set_key(id, &p, frame); }
    }
    if key_all { graph.key_all(id); }
    if delete_here { graph.delete_keys_at(id, frame); }
}

/// S, tapped: key every number of the selected operator at the playhead. A
/// tap, so a navigation style that holds S to move the camera keeps it: a
/// mouse button pressed while S is down makes it a hold. Not while typing.
pub fn s_key(ctx: &egui::Context, graph: &mut NodeGraphState) -> Option<String> {
    if ctx.egui_wants_keyboard_input() { return None; }
    let (pressed, released, mouse) = ctx.input(|i| (
        i.key_pressed(egui::Key::S) && !i.modifiers.any(),
        i.key_released(egui::Key::S),
        i.pointer.any_down(),
    ));
    let id_used = egui::Id::new("s_key_used");
    let mut used: bool = ctx.data(|d| d.get_temp(id_used)).unwrap_or(false);
    if pressed { used = false; }
    if mouse && ctx.input(|i| i.key_down(egui::Key::S)) { used = true; }
    ctx.data_mut(|d| d.insert_temp(id_used, used));
    if !released || used { return None; }
    let id = graph.selected_node?;
    let n = graph.key_all(id);
    let name = graph.nodes.iter().find(|x| x.id == id).map(|x| x.name.clone()).unwrap_or_default();
    Some(format!("Keyed {n} number{} of {name} at frame {}", if n == 1 { "" } else { "s" }, graph.frame().round()))
}
