//! The Tools pane: the add menu's categories down a narrow column, each a
//! button in the colour its nodes have (after XSI's tool bars). A click opens
//! a box beside it of the category's operators (after Shake's tool tabs), to
//! drag onto the Node Graph (added where it is let go, into the wire it is
//! let go on, and selected) or to click (added after the selected operator,
//! `NodeGraphState::add_after_selected`). Esc, a click away or on the
//! category again closes it.

use crate::node_graph::ui::{catalog, category_colours};
use crate::node_graph::NodeGraphState;
use crate::types::NodeType;
use crate::types::{node_type_icon, node_type_label};

/// An operator dragged from the Tools pane, for the Node Graph to take.
pub struct DraggedOperator(pub NodeType);

/// The add menu's categories in the order the pane lists them: from getting
/// a scene to rendering it, then what organises a graph.
const ORDER: [&str; 16] = [
    "File",
    "Create",
    "Primitives",
    "Lights",
    "Modify",
    "UV",
    "Materials",
    "Paint",
    "Animation",
    "Mocap",
    "Rigging",
    "Image",
    "Render",
    "System Commands",
    "Organise",
    "Placeholder",
];

/// The catalog in `ORDER` (icon, category, its operators), any category not
/// named there last.
fn bands() -> Vec<(&'static str, &'static str, Vec<(NodeType, &'static str)>)> {
    let mut all = catalog();
    all.sort_by_key(|(_, c, _)| ORDER.iter().position(|o| o == c).unwrap_or(ORDER.len()));
    all
}

pub fn draw(ui: &mut egui::Ui, graph: &mut NodeGraphState) {
    let open_id = egui::Id::new("tools_open");
    let mut open: Option<String> = ui.data(|d| d.get_temp(open_id)).unwrap_or_default();
    let mut shown: Option<(egui::Rect, &'static str, &'static str, Vec<(NodeType, &'static str)>)> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        for (icon, category, nodes) in bands() {
            let on = open.as_deref() == Some(category);
            let r = band(ui, icon, category, on);
            if r.clicked() {
                open = if on { None } else { Some(category.to_string()) };
            }
            if open.as_deref() == Some(category) {
                shown = Some((r.rect, icon, category, nodes));
            }
        }
    });
    // The box of the category open, beside its button.
    if let Some((button, icon, category, nodes)) = shown {
        let dragging = egui::DragAndDrop::has_any_payload(ui.ctx());
        let area = egui::Area::new(egui::Id::new("tools_box"))
            .order(egui::Order::Foreground)
            .fixed_pos(button.right_top() + egui::vec2(6.0, 0.0))
            .constrain(true)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(210.0);
                    let (fill, ink) = colours(category, ui.visuals().text_color());
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(210.0, 20.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, fill);
                    ui.painter().text(
                        rect.left_center() + egui::vec2(6.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        format!("{icon}  {category}"),
                        egui::FontId::proportional(12.5),
                        ink,
                    );
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (t, hint) in nodes {
                        row(ui, graph, category, t, hint);
                    }
                });
            });
        // Closed by Esc, or a click away from it and the bar (not while one
        // of its operators is carried off).
        let away = ui.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !area.response.rect.contains(p) && !ui.max_rect().contains(p))
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) || (away && !dragging) {
            open = None;
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
}

/// A category's band colour and the text on it: its nodes' colour, else the
/// node grey with `text`.
fn colours(category: &str, text: egui::Color32) -> (egui::Color32, egui::Color32) {
    match category_colours(category) {
        Some((fill, ink, _)) => (fill, ink),
        None => (crate::theme::raised(130, 130, 130), text),
    }
}

/// A category's button, in its colour: a click opens its box of operators.
/// `on`: its box is open.
fn band(ui: &mut egui::Ui, icon: &str, category: &str, on: bool) -> egui::Response {
    let (fill, ink) = colours(category, ui.visuals().text_color());
    let (rect, r) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 3.0, fill);
    if on || r.hovered() {
        let stroke = if on {
            egui::Stroke::new(2.0, ui.visuals().selection.stroke.color)
        } else {
            egui::Stroke::new(1.0, ink.gamma_multiply(0.5))
        };
        p.rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Inside);
    }
    p.text(
        rect.left_center() + egui::vec2(7.0, 0.0),
        egui::Align2::LEFT_CENTER,
        format!("{icon}  {category}"),
        egui::FontId::proportional(12.5),
        ink,
    );
    r.on_hover_text("Its operators: drag one onto the Node Graph, or click it to add it after the selected one")
}

/// An operator's row: clicked, added after the selected operator; dragged,
/// carried to the Node Graph, drawn under the pointer as it goes.
fn row(ui: &mut egui::Ui, graph: &mut NodeGraphState, category: &str, t: NodeType, hint: &str) {
    let label = format!("{}  {}", node_type_icon(&t), node_type_label(&t));
    let r = ui
        .add_sized(
            [ui.available_width(), 22.0],
            egui::Button::new(egui::RichText::new(&label).size(12.0))
                .sense(egui::Sense::click_and_drag())
                .wrap_mode(egui::TextWrapMode::Truncate),
        )
        .on_hover_text(format!(
            "{hint}\n\nClick: after the selected operator. Drag: onto the Node Graph, onto a wire to go into it"
        ));
    if r.drag_started() {
        egui::DragAndDrop::set_payload(ui.ctx(), DraggedOperator(t.clone()));
    }
    if r.dragged() {
        if let Some(at) = ui.ctx().pointer_interact_pos() {
            ghost(ui.ctx(), at, colours(category, ui.visuals().text_color()), &label);
        }
    } else if r.clicked() {
        graph.add_after_selected(t);
    }
}

/// The operator being dragged, beside the pointer, in its category's colour.
fn ghost(ctx: &egui::Context, at: egui::Pos2, (fill, ink): (egui::Color32, egui::Color32), label: &str) {
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("tools_ghost")));
    let font = egui::FontId::proportional(12.5);
    let text = p.layout_no_wrap(label.to_string(), font, ink);
    let rect = egui::Rect::from_min_size(at + egui::vec2(14.0, 10.0), text.size() + egui::vec2(16.0, 10.0));
    p.rect_filled(rect, 4.0, fill.gamma_multiply(0.85));
    p.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, egui::Color32::from_black_alpha(120)),
        egui::StrokeKind::Inside,
    );
    p.galley(rect.min + egui::vec2(8.0, 5.0), text, ink);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every category of the add menu is a band, in the pane's order.
    #[test]
    fn every_category_is_a_band_in_order() {
        let listed: Vec<&str> = bands().into_iter().map(|(_, c, _)| c).collect();
        assert_eq!(listed.len(), catalog().len());
        let known: Vec<&str> = listed.iter().copied().filter(|c| ORDER.contains(c)).collect();
        assert_eq!(
            known,
            ORDER.iter().copied().filter(|o| listed.contains(o)).collect::<Vec<_>>()
        );
        assert_eq!(listed[0], "File");
    }
}
