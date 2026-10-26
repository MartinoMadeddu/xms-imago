use super::{GraphNode, NodeGraphState};
use crate::types::{node_type_icon, ConnectionId, IceGraphId, NodeId, NodeType, RetimeMode};
use glam::Vec3;

pub const NODE_WIDTH: f32 = 180.0;
pub const NODE_HEIGHT: f32 = 50.0;
const SOCKET_RADIUS: f32 = 4.5;
const SOCKET_HIT: f32 = 22.0;
const NODE_ROUNDING: f32 = 5.0;
/// A Pass Through's width: narrow, so it reads as a point in the layout, not an
/// operator.
pub const PASS_WIDTH: f32 = 120.0;

/// How wide a node is drawn: a Pass Through narrower than the rest.
pub fn node_width(node: &GraphNode) -> f32 {
    if matches!(node.node_type, NodeType::PassThrough) {
        PASS_WIDTH
    } else {
        NODE_WIDTH
    }
}

/// How round a node's corners are: a Pass Through square-cornered.
fn node_rounding(node: &GraphNode) -> f32 {
    if matches!(node.node_type, NodeType::PassThrough) {
        0.0
    } else {
        NODE_ROUNDING
    }
}
/// The band around a node with a graph inside: a light grey, the same in every
/// theme.
fn dive_band() -> egui::Color32 {
    egui::Color32::from_rgb(200, 200, 200)
}

#[allow(dead_code)]
mod xsi {
    // Light-theme colours. `theme::c` returns the dark counterpart in dark mode.
    #![allow(non_snake_case)]
    use egui::Color32;
    pub fn BG() -> Color32 {
        crate::theme::c(100, 100, 100)
    }
    pub fn GRID() -> Color32 {
        crate::theme::c(90, 90, 90)
    }
    pub fn NODE_BODY() -> Color32 {
        crate::theme::raised(130, 130, 130)
    }
    pub fn NODE_BODY_SEL() -> Color32 {
        crate::theme::c(110, 120, 135)
    }
    pub fn NODE_TITLE() -> Color32 {
        crate::theme::raised(105, 105, 105)
    }
    pub fn NODE_TITLE_SUB() -> Color32 {
        crate::theme::c(80, 90, 100)
    }
    pub fn BORDER() -> Color32 {
        crate::theme::outline(70, 70, 70)
    }
    pub fn BORDER_SEL() -> Color32 {
        crate::theme::c(180, 200, 220)
    }
    pub fn TEXT() -> Color32 {
        crate::theme::c(248, 248, 248)
    }
    pub fn TEXT_DIM() -> Color32 {
        crate::theme::c(210, 210, 210)
    }
    pub fn WIRE() -> Color32 {
        crate::theme::c(160, 160, 155)
    }
    pub fn WIRE_HOV() -> Color32 {
        crate::theme::c(220, 185, 90)
    }
    pub fn SOCK_IN() -> Color32 {
        crate::theme::c(100, 140, 100)
    }
    pub fn SOCK_IN_CONN() -> Color32 {
        crate::theme::c(130, 175, 130)
    }
    pub fn SOCK_IN_HOV() -> Color32 {
        crate::theme::c(170, 215, 170)
    }
    pub fn SOCK_OUT() -> Color32 {
        crate::theme::c(150, 120, 85)
    }
    pub fn SOCK_OUT_HOV() -> Color32 {
        crate::theme::c(200, 165, 120)
    }
    pub fn SOCK_OUT_DRAG() -> Color32 {
        crate::theme::c(230, 195, 100)
    }
    pub fn SEL_RECT() -> Color32 {
        Color32::from_rgba_premultiplied(100, 140, 200, 40)
    }
    pub fn SEL_RECT_BORDER() -> Color32 {
        crate::theme::c(120, 160, 220)
    }
    /// The view flag's eye: the accent, bright on a dark title, dark on a light
    /// one, so it stands out from the title it sits on.
    pub fn VIEW_FLAG() -> Color32 {
        if crate::theme::is_dark() {
            crate::theme::c(100, 180, 255)
        } else {
            crate::theme::accent_level(110)
        }
    }
    pub fn BYPASS() -> Color32 {
        crate::theme::c(235, 170, 60)
    }
    pub fn FROZEN() -> Color32 {
        crate::theme::c(110, 200, 235)
    }
    pub fn GHOST() -> Color32 {
        crate::theme::c(230, 120, 190)
    }
    pub fn BYPASS_VEIL() -> Color32 {
        Color32::from_rgba_unmultiplied(128, 128, 128, 120)
    }
    /// The view flag's eye under the mouse: a step towards the title's ink.
    pub fn VIEW_FLAG_HOV() -> Color32 {
        if crate::theme::is_dark() {
            crate::theme::c(150, 210, 255)
        } else {
            crate::theme::accent_level(70)
        }
    }
    // Sockets carrying a material: the materials' purple (`type_colours`).
    pub fn SOCK_MAT() -> Color32 {
        crate::type_colours::DataType::Material.colour()
    }
    pub fn SOCK_MAT_CONN() -> Color32 {
        crate::type_colours::connected(SOCK_MAT())
    }
    pub fn SOCK_MAT_HOV() -> Color32 {
        crate::type_colours::hovered(SOCK_MAT())
    }
}

// SOCKET POSITION HELPERS

pub fn output_socket_pos(node: &GraphNode, out_idx: usize) -> egui::Pos2 {
    // A material comes out on the right, to go into a Material Binding's side.
    if side_output(node) {
        let t = (out_idx + 1) as f32 / (node.outputs.len() + 1) as f32;
        return egui::pos2(
            node.position.x + node_width(node),
            node.position.y + node_height(node) * t,
        );
    }
    let t = (out_idx + 1) as f32 / (node.outputs.len() + 1) as f32;
    egui::pos2(
        node.position.x + node_width(node) * t,
        node.position.y + node_height(node),
    )
}

/// Where the first input on a node's side is, from its top, and the step to the
/// next.
const SIDE_TOP: f32 = 36.0;
const SIDE_STEP: f32 = 16.0;

/// Inputs on the node's left side rather than on top: Material Binding's
/// material inputs, the scene coming in on top.
pub fn side_input(node: &GraphNode, in_idx: usize) -> bool {
    in_idx > 0 && matches!(node.node_type, NodeType::MaterialBinding { .. })
}

/// Outputs on the node's right side rather than below it: a material's.
pub fn side_output(node: &GraphNode) -> bool {
    matches!(node.node_type, NodeType::Material { .. } | NodeType::MaterialX { .. })
}

/// A graph inside an operator, to open in the pane: an ICE node's ICE graph, or
/// a MaterialX operator's graph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dive {
    Ice(IceGraphId),
    MaterialX(NodeId),
}

/// The graph an operator holds, if it holds one.
pub fn dive_of(node: &GraphNode) -> Option<Dive> {
    match &node.node_type {
        NodeType::Ice { id, .. } => Some(Dive::Ice(*id)),
        NodeType::MaterialX { .. } => Some(Dive::MaterialX(node.id)),
        _ => None,
    }
}

/// Sockets that carry a material, drawn purple: a Material's output and a
/// Material Binding's material inputs.
fn material_socket(node: &GraphNode, input: Option<usize>) -> bool {
    match input {
        Some(k) => side_input(node, k),
        None => side_output(node),
    }
}

/// A node's height: taller when its side inputs need the room.
pub fn node_height(node: &GraphNode) -> f32 {
    let side = (0..node.inputs.len()).filter(|&k| side_input(node, k)).count();
    if side == 0 {
        return NODE_HEIGHT;
    }
    (SIDE_TOP + SIDE_STEP * (side - 1) as f32 + 14.0).max(NODE_HEIGHT)
}

pub fn input_socket_pos(node: &GraphNode, in_idx: usize) -> egui::Pos2 {
    if side_input(node, in_idx) {
        let k = (0..in_idx).filter(|&k| side_input(node, k)).count();
        return egui::pos2(node.position.x, node.position.y + SIDE_TOP + SIDE_STEP * k as f32);
    }
    let top = (0..node.inputs.len()).filter(|&k| !side_input(node, k)).count();
    let at = (0..in_idx).filter(|&k| !side_input(node, k)).count();
    let t = (at + 1) as f32 / (top + 1) as f32;
    egui::pos2(node.position.x + node_width(node) * t, node.position.y)
}

// GRAPH CANVAS

pub fn draw_node_graph(ui: &mut egui::Ui, graph: &mut NodeGraphState) -> Option<Dive> {
    let mut dive_into: Option<Dive> = None;

    let (response, painter) = ui.allocate_painter(
        egui::Vec2::new(ui.available_width(), ui.available_height()),
        egui::Sense::click_and_drag(),
    );
    let canvas_rect = response.rect;
    let pan = graph.pan_offset;
    let zoom = graph.zoom;

    // to_screen: canvas-space → screen-space, applying zoom around canvas origin
    let to_screen = |p: egui::Pos2| canvas_rect.min + egui::vec2(p.x * zoom + pan.x, p.y * zoom + pan.y);
    // to_canvas: screen-space → canvas-space
    let to_canvas = |p: egui::Pos2| {
        egui::pos2(
            (p.x - canvas_rect.min.x - pan.x) / zoom,
            (p.y - canvas_rect.min.y - pan.y) / zoom,
        )
    };

    // Frame every node
    let frame_key = response.hovered()
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| i.key_pressed(egui::Key::F) || i.key_pressed(egui::Key::A));
    if (graph.frame_request || frame_key) && canvas_rect.width() > 40.0 && !graph.nodes.is_empty() {
        graph.frame_request = false;
        let mut bounds = egui::Rect::NOTHING;
        for node in &graph.nodes {
            bounds = bounds.union(egui::Rect::from_min_size(
                node.position,
                egui::vec2(node_width(node), node_height(node)),
            ));
        }
        let room = canvas_rect.size() - egui::vec2(60.0, 70.0);
        // Not so small that the names cannot be read: a graph too large
        // for that is shown from its middle.
        graph.zoom = (room.x / bounds.width()).min(room.y / bounds.height()).clamp(0.45, 1.0);
        let middle = bounds.center().to_vec2() * graph.zoom;
        graph.pan_offset = canvas_rect.size() * 0.5 - middle;
        ui.ctx().request_repaint();
    }

    // Add-node menu: Tab, right-click, or right-click on an output
    let mut opened_now = false;
    if response.hovered() && ui.input(|i| i.key_pressed(egui::Key::Tab)) {
        let cursor = ui.input(|i| i.pointer.hover_pos()).unwrap_or(canvas_rect.center());
        graph.tab_menu_screen_pos = Some(cursor);
        graph.tab_menu_canvas_pos = Some(to_canvas(cursor));
        graph.menu_from = None;
        opened_now = true;
    }
    // A right-click anywhere on the canvas. Over an output socket the menu
    // belongs to that socket; over a wire it is left to the wire.
    let right_click = ui
        .input(|i| {
            if i.pointer.secondary_clicked() {
                i.pointer.interact_pos()
            } else {
                None
            }
        })
        .filter(|p| canvas_rect.contains(*p) && ui.rect_contains_pointer(canvas_rect));
    if let Some(cursor) = right_click {
        let socket = graph.nodes.iter().find_map(|node| {
            (0..node.outputs.len())
                .find(|o| to_screen(output_socket_pos(node, *o)).distance(cursor) <= SOCKET_HIT * 0.5 + 2.0)
                .map(|o| (node.id, o))
        });
        if socket.is_some() || response.secondary_clicked() {
            graph.tab_menu_screen_pos = Some(cursor);
            graph.tab_menu_canvas_pos = Some(to_canvas(cursor));
            graph.menu_from = socket;
            graph.connecting_from = None;
            opened_now = true;
        }
    }
    if let Some(screen_pos) = graph.tab_menu_screen_pos {
        let canvas_pos = graph.tab_menu_canvas_pos.unwrap_or_default();
        let before: Vec<NodeId> = graph.nodes.iter().map(|n| n.id).collect();
        let mut close = false;
        let area_resp = egui::Area::new(egui::Id::new("tab_add_node"))
            .fixed_pos(screen_pos)
            .order(egui::Order::Foreground)
            .constrain(true)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(180.0);
                    if let Some((from, _)) = graph.menu_from {
                        if let Some(node) = graph.nodes.iter().find(|n| n.id == from) {
                            ui.label(egui::RichText::new(format!("After {}", node.name)).small());
                            ui.separator();
                        }
                    }
                    if add_node_menu(ui, graph, canvas_pos, opened_now) {
                        close = true;
                    }
                });
            });
        // Where the new node goes: under the socket it was asked from and
        // wired to it, or centred on the place that was clicked.
        let fresh: Vec<NodeId> = graph
            .nodes
            .iter()
            .map(|n| n.id)
            .filter(|id| !before.contains(id))
            .collect();
        if let Some(new_id) = fresh.first().copied() {
            let from = graph
                .menu_from
                .and_then(|(id, out)| graph.nodes.iter().find(|n| n.id == id).map(|n| (id, out, n.position)));
            let mut at = match from {
                Some((_, _, pos)) => pos + egui::vec2(0.0, NODE_HEIGHT + 50.0),
                None => canvas_pos - egui::vec2(NODE_WIDTH * 0.5, NODE_HEIGHT * 0.5),
            };
            // Step sideways until the place is free.
            let taken = |p: egui::Pos2, graph: &NodeGraphState| {
                graph.nodes.iter().any(|n| {
                    n.id != new_id
                        && (n.position.x - p.x).abs() < NODE_WIDTH + 10.0
                        && (n.position.y - p.y).abs() < NODE_HEIGHT + 10.0
                })
            };
            for _ in 0..40 {
                if !taken(at, graph) {
                    break;
                }
                at.x += NODE_WIDTH + 30.0;
            }
            if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == new_id) {
                node.position = at;
            }
            if let Some((src, out, _)) = from {
                let has_input = graph
                    .nodes
                    .iter()
                    .find(|n| n.id == new_id)
                    .map(|n| !n.inputs.is_empty())
                    .unwrap_or(false);
                if has_input {
                    graph.add_connection(src, out, new_id, 0);
                    // Added under the viewed operator, from its viewed
                    // output: the view moves down to it, as in Houdini.
                    if graph.view_flag == Some(src) && graph.viewed_output() == out {
                        graph.set_view_flag(new_id);
                    }
                }
            }
            graph.selected_node = Some(new_id);
            graph.selected_nodes = vec![new_id];
            // Never leave a new node out of sight: move the view just enough.
            let rect = egui::Rect::from_min_size(to_screen(at), egui::vec2(NODE_WIDTH, NODE_HEIGHT) * zoom);
            let view = canvas_rect.shrink(12.0);
            let mut shift = egui::Vec2::ZERO;
            if rect.max.x > view.max.x {
                shift.x = view.max.x - rect.max.x;
            }
            if rect.min.x + shift.x < view.min.x {
                shift.x = view.min.x - rect.min.x;
            }
            if rect.max.y > view.max.y {
                shift.y = view.max.y - rect.max.y;
            }
            if rect.min.y + shift.y < view.min.y {
                shift.y = view.min.y - rect.min.y;
            }
            graph.pan_offset += shift;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if !opened_now
            && ui.input(|i| i.pointer.any_click())
            && !area_resp.response.contains_pointer()
            && !ui.ctx().any_popup_open()
        {
            close = true;
        }
        if close {
            graph.tab_menu_screen_pos = None;
            graph.tab_menu_canvas_pos = None;
            graph.menu_from = None;
        }
    }

    let grid_spacing = 50.0 * zoom;
    let offset_x = pan.x % grid_spacing;
    let offset_y = pan.y % grid_spacing;
    let mut x = canvas_rect.min.x + offset_x;
    while x < canvas_rect.max.x {
        painter.line_segment(
            [egui::pos2(x, canvas_rect.min.y), egui::pos2(x, canvas_rect.max.y)],
            egui::Stroke::new(1.0, xsi::GRID()),
        );
        x += grid_spacing;
    }
    let mut y = canvas_rect.min.y + offset_y;
    while y < canvas_rect.max.y {
        painter.line_segment(
            [egui::pos2(canvas_rect.min.x, y), egui::pos2(canvas_rect.max.x, y)],
            egui::Stroke::new(1.0, xsi::GRID()),
        );
        y += grid_spacing;
    }

    // Connections
    let mut hovered_conn: Option<ConnectionId> = None;
    for conn in &graph.connections.clone() {
        if let (Some(fn_), Some(tn)) = (
            graph.nodes.iter().find(|n| n.id == conn.from_node),
            graph.nodes.iter().find(|n| n.id == conn.to_node),
        ) {
            let fp = to_screen(output_socket_pos(fn_, conn.from_output));
            let tp = to_screen(input_socket_pos(tn, conn.to_input));
            let side = (side_output(fn_), side_input(tn, conn.to_input));
            let is_sel = graph.selected_connection == Some(conn.id)
                || graph
                    .unplug
                    .as_ref()
                    .is_some_and(|p| p.holds(conn.from_node, &conn.from_output, conn.to_node, &conn.to_input));
            if let Some(ptr) = response.hover_pos() {
                if is_near_bezier(ptr, fp, tp, side, 10.0) {
                    hovered_conn = Some(conn.id);
                }
            }
            draw_wire_to(&painter, fp, tp, side, hovered_conn == Some(conn.id), is_sel);
        }
    }
    if let Some(cid) = hovered_conn {
        // LMB click → select wire, clear node selection
        if response.clicked() {
            graph.selected_connection = Some(cid);
            graph.selected_nodes.clear();
            graph.selected_node = None;
        }
        // RMB → delete immediately (existing behaviour)
        if response.secondary_clicked() {
            graph.remove_connection(cid);
        }
    }

    // In-progress wire
    if let Some((fid, fo)) = graph.connecting_from {
        if let Some(fn_) = graph.nodes.iter().find(|n| n.id == fid) {
            let fp = to_screen(output_socket_pos(fn_, fo));
            let ptr = ui.input(|i| i.pointer.hover_pos());
            if let Some(ptr) = ptr {
                draw_wire_to(&painter, fp, ptr, (side_output(fn_), false), false, false);
            }
        }
    }
    // Wires picked up off an output: from the pointer to their inputs.
    if let Some(ptr) = ui.input(|i| i.pointer.hover_pos()) {
        for (to, input) in &graph.carrying {
            if let Some(tn) = graph.nodes.iter().find(|n| n.id == *to) {
                draw_wire_to(
                    &painter,
                    ptr,
                    to_screen(input_socket_pos(tn, *input)),
                    (false, side_input(tn, *input)),
                    false,
                    true,
                );
            }
        }
    }

    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        graph.connecting_from = None;
        graph.unplug = None;
    }

    // Delete selected nodes or wire
    // Use canvas_rect.contains(pointer) instead of response.hovered() so that
    // the delete key fires even when the pointer is over a node widget. Not
    // while text is typed (a file name in the file browser, a parameter), and
    // not with the pointer over a window above the graph: Backspace there is
    // the text's.
    let ptr_in_canvas = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| canvas_rect.contains(p) && ui.ctx().layer_id_at(p).is_none_or(|l| l == ui.layer_id()));
    if ptr_in_canvas
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
    {
        graph.delete_selected();
    }

    // Down arrow: into the selected ICE or MaterialX node, as a double-click does
    if ptr_in_canvas && !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
        let selected = graph
            .selected_node
            .and_then(|id| graph.nodes.iter().find(|n| n.id == id));
        if let Some(d) = selected.and_then(dive_of) {
            dive_into = Some(d);
        }
    }

    // Nodes
    let nodes_clone = graph.nodes.clone();
    for node in &nodes_clone {
        if let Some(id) = draw_node(ui, &painter, graph, node, &to_screen, &to_canvas, canvas_rect) {
            dive_into = Some(id);
        }
    }

    // Marquee select
    // The middle button pans, unless it went down on a node: then it
    // shows that node's information.
    let is_panning =
        ui.input(|i| i.modifiers.shift) || (ui.input(|i| i.pointer.middle_down()) && graph.info_node.is_none());
    let is_wiring = graph.connecting_from.is_some();
    let is_dragging = graph.dragging_node.is_some();

    if response.drag_started() && !is_panning && !is_wiring && !is_dragging && graph.tab_menu_screen_pos.is_none() {
        if let Some(pos) = response.interact_pointer_pos() {
            graph.marquee_start = Some(to_canvas(pos));
        }
    }

    if response.dragged() && !is_panning && !is_wiring && !is_dragging {
        if let (Some(start), Some(cur)) = (
            graph.marquee_start,
            response.interact_pointer_pos().map(|p| to_canvas(p)),
        ) {
            let r = egui::Rect::from_two_pos(start, cur);
            // Highlight nodes inside the marquee
            let newly_selected: Vec<NodeId> = graph
                .nodes
                .iter()
                .filter(|n| {
                    let node_rect = egui::Rect::from_min_size(n.position, egui::vec2(node_width(n), node_height(n)));
                    r.intersects(node_rect)
                })
                .map(|n| n.id)
                .collect();
            graph.selected_nodes = newly_selected;

            // Draw the marquee rectangle in screen space
            let sr = egui::Rect::from_two_pos(to_screen(start), to_screen(cur));
            // Light: the selection blue (the dark schemes' tint, added to a
            // pale canvas, came out white).
            let (fill, edge) = if crate::theme::is_dark() {
                (xsi::SEL_RECT(), xsi::SEL_RECT_BORDER())
            } else {
                crate::theme::marquee_light()
            };
            painter.rect_filled(sr, 2.0, fill);
            painter.rect_stroke(sr, 2.0, egui::Stroke::new(1.0, edge), egui::StrokeKind::Middle);
        }
    }

    if response.drag_stopped() {
        if graph.marquee_start.is_some() {
            // Commit marquee selection — selected_nodes already set above.
            // If nothing was selected, clear.
            graph.marquee_start = None;
            // Set selected_node to first in selection for properties panel sync
            graph.selected_node = graph.selected_nodes.first().copied();
        }
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
        graph.pressed_input = None;
    }

    // Cancel wire if mouse released over empty canvas
    if graph.connecting_from.is_some() && response.drag_stopped() {
        graph.connecting_from = None;
    }

    // Click on empty canvas: a picked port's wires come off; a wire being
    // drawn is let go of.
    if response.clicked() && !is_panning && hovered_conn.is_none() {
        if let Some(picked) = graph.unplug.take() {
            let off: Vec<ConnectionId> = graph
                .connections
                .iter()
                .filter(|c| picked.holds(c.from_node, &c.from_output, c.to_node, &c.to_input))
                .map(|c| c.id)
                .collect();
            for cid in off {
                graph.remove_connection(cid);
            }
        }
        graph.connecting_from = None;
    }

    // Click on empty canvas → clear all selection
    if response.clicked() && !is_panning && !is_wiring && hovered_conn.is_none() {
        graph.selected_nodes.clear();
        graph.selected_node = None;
        graph.selected_connection = None;
    }

    // Pan (shift+drag or MMB)
    if response.dragged() && is_panning {
        graph.pan_offset += response.drag_delta();
    }

    // Zoom (scroll wheel, zoom toward cursor)
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let factor = 1.0 + scroll * 0.002;
            let cursor_s = ui.input(|i| i.pointer.hover_pos()).unwrap_or(canvas_rect.center());
            // cursor in canvas space before zoom change
            let cursor_c = to_canvas(cursor_s);
            graph.zoom = (graph.zoom * factor).clamp(0.15, 4.0);
            // Recompute pan so cursor_c stays under cursor_s after zoom
            graph.pan_offset = egui::vec2(
                cursor_s.x - canvas_rect.min.x - cursor_c.x * graph.zoom,
                cursor_s.y - canvas_rect.min.y - cursor_c.y * graph.zoom,
            );
        }
    }

    dive_into
}

// SINGLE NODE

/// One name per node type, the same in the menu, on the node and in the
/// properties.
fn node_type_label(t: &NodeType) -> &'static str {
    crate::types::node_type_label(t)
}

fn draw_node(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    graph: &mut NodeGraphState,
    node: &GraphNode,
    to_screen: &impl Fn(egui::Pos2) -> egui::Pos2,
    to_canvas: &impl Fn(egui::Pos2) -> egui::Pos2,
    _canvas_rect: egui::Rect,
) -> Option<Dive> {
    let mut dive: Option<Dive> = None;
    let zoom = graph.zoom;
    let np = to_screen(node.position);
    let height = node_height(node);
    let width = node_width(node);
    let rounding = node_rounding(node);
    let rect = egui::Rect::from_min_size(np, egui::vec2(width * zoom, height * zoom));
    let id = node.id;
    let is_sel = graph.selected_node == Some(id) || graph.selected_nodes.contains(&id);
    let is_sub = matches!(node.node_type, NodeType::Ice { .. });

    // Node body
    // As the ICE nodes: a grey ring around the node (the selection blue when
    // selected), one colour over the whole node, a dark edge, the name in
    // bold over the type.
    // One colour over the whole node, by the add-menu category it is in.
    let (body, ink, ink_dim) = node_colours(&node.node_type);
    let _ = is_sub;
    let w = ring_width(zoom, is_sel);
    // A node with a graph inside (ICE, MaterialX) always wears a band as
    // wide as the selection's, in a light grey: it looks selected in
    // another colour. Selected, the selection goes around the band.
    let dives = dive_of(node).is_some();
    if dives {
        let band = ring_width(zoom, true);
        let r = rounding * zoom;
        if is_sel {
            painter.rect_filled(rect.expand(band + w), r + band + w, crate::theme::selection());
        } else {
            // A hair of dark outside the band, so it reads on the canvas.
            painter.rect_filled(
                rect.expand(band + 1.0),
                r + band + 1.0,
                egui::Color32::from_rgb(70, 70, 70),
            );
        }
        painter.rect_filled(rect.expand(band), r + band, dive_band());
    } else {
        let ring = rect.expand(w);
        painter.rect_filled(
            ring,
            rounding * zoom + w,
            if is_sel {
                crate::theme::selection()
            } else {
                egui::Color32::from_rgb(105, 105, 105)
            },
        );
    }
    painter.rect_filled(rect, rounding * zoom, body);
    // And a small arrow into it, at the bottom right.
    if dives {
        let c = egui::pos2(rect.max.x - 10.0 * zoom, rect.max.y - 9.0 * zoom);
        let s = 3.5 * zoom;
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(c.x - s, c.y - s * 0.6),
                egui::pos2(c.x + s, c.y - s * 0.6),
                egui::pos2(c.x, c.y + s * 0.8),
            ],
            ink_dim,
            egui::Stroke::NONE,
        ));
        painter.line_segment(
            [egui::pos2(c.x - s, c.y + s * 1.3), egui::pos2(c.x + s, c.y + s * 1.3)],
            egui::Stroke::new(1.2 * zoom, ink_dim),
        );
    }

    let title_h = 32.0 * zoom;
    let title_rect = egui::Rect::from_min_size(np, egui::vec2(width * zoom, title_h));
    painter.rect_stroke(
        rect,
        rounding * zoom,
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(38, 38, 38)),
        egui::StrokeKind::Middle,
    );

    // Icon, then the name in bold (drawn twice, a hair apart), the type
    // beneath in PascalCase, all from the left, after the bypass ring.
    // The node's icon leads the name, as in the add menu.
    // A Pass Through, narrow, has no icon: its name has the room.
    let text_x = np.x + 24.0 * zoom;
    let pass = matches!(node.node_type, NodeType::PassThrough);
    if !pass {
        painter.text(
            egui::pos2(text_x, np.y + 12.0 * zoom),
            egui::Align2::LEFT_CENTER,
            node_type_icon(&node.node_type),
            egui::FontId::proportional(12.0 * zoom),
            ink,
        );
    }
    let name_x = if pass { text_x } else { text_x + 17.0 * zoom };
    // The freeze and ghost buttons show when the node is under the
    // pointer or one of them is on; the name stops short of them.
    let hovered_node = ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| rect.contains(p));
    let flags_shown = hovered_node || node.frozen || node.ghost;
    let flag_size = 16.0 * zoom;
    let name_end = np.x + width * zoom - flag_size * if flags_shown { 3.0 } else { 1.0 } - 8.0 * zoom;
    let name_painter = painter.with_clip_rect(painter.clip_rect().intersect(egui::Rect::from_min_max(
        egui::pos2(np.x, np.y),
        egui::pos2(name_end, np.y + 32.0 * zoom),
    )));
    for dx in [0.0, 0.6] {
        name_painter.text(
            egui::pos2(name_x + dx * zoom, np.y + 12.0 * zoom),
            egui::Align2::LEFT_CENTER,
            &node.name,
            egui::FontId::proportional(12.5 * zoom),
            ink,
        );
    }
    let type_name: String = node_type_label(&node.node_type)
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect();
    name_painter.text(
        egui::pos2(name_x, np.y + 25.0 * zoom),
        egui::Align2::LEFT_CENTER,
        type_name,
        egui::FontId::proportional(9.5 * zoom),
        ink_dim,
    );

    let dr = ui.allocate_rect(title_rect, egui::Sense::click_and_drag());

    // The view flag button, at the top right of the title bar.
    let has_view_flag = graph.has_view_flag(id);
    let eye_icon = if has_view_flag { "👁" } else { "○" };
    let eye_size = 16.0 * zoom;
    let eye_pos = egui::pos2(np.x + width * zoom - eye_size - 4.0 * zoom, np.y + 4.0 * zoom);
    let eye_rect = egui::Rect::from_min_size(eye_pos, egui::vec2(eye_size, eye_size));

    let eye_response = ui.allocate_rect(eye_rect, egui::Sense::click());

    let eye_color = if has_view_flag {
        if eye_response.hovered() {
            xsi::VIEW_FLAG_HOV()
        } else {
            xsi::VIEW_FLAG()
        }
    } else {
        if eye_response.hovered() {
            ink
        } else {
            ink_dim
        }
    };

    // The eye fills its square; the empty circle stays small.
    let glyph_size = zoom * if has_view_flag { 16.0 } else { 12.0 };
    painter.text(
        egui::pos2(eye_pos.x + eye_size / 2.0, eye_pos.y + eye_size / 2.0),
        egui::Align2::CENTER_CENTER,
        eye_icon,
        egui::FontId::proportional(glyph_size),
        eye_color,
    );

    // A click puts the view flag here. On the viewed operator it moves on
    // to its next output, when it has several. The flag never goes off.
    let outputs: Vec<String> = node.outputs.iter().map(|o| o.name.clone()).collect();
    let viewed = outputs.get(graph.viewed_output()).cloned().unwrap_or_default();
    let click_at = if dr.clicked() { dr.interact_pointer_pos() } else { None };
    let on_eye = click_at.map(|p| eye_rect.contains(p)).unwrap_or(false);
    if eye_response.clicked() || on_eye {
        graph.set_view_flag(id);
    }

    if eye_response.hovered() {
        let tooltip_text = match (has_view_flag, outputs.len() > 1) {
            (true, true) => format!("The scene is this operator's {viewed} output. Click to view its next output"),
            (true, false) => "The scene is this operator's output: the Viewer, the Scene Explorer and the Primitive Inspector show it".to_string(),
            (false, _) => "Click to make this operator's output the scene".to_string(),
        };
        eye_response.on_hover_text(tooltip_text);
    }

    // Freeze and ghost buttons, left of the eye
    // Drawn by hand, as the bypass ring is: a snowflake (frozen: keeps its
    // result) and a wireframe square (ghosted: drawn grey in the Viewer).
    let mut on_flag = false;
    if flags_shown {
        for (k, which) in [(2.0, 0u8), (1.0, 1u8)] {
            let at = egui::pos2(eye_pos.x - flag_size * k - 2.0 * zoom * (k - 1.0), eye_pos.y);
            let r_rect = egui::Rect::from_min_size(at, egui::vec2(flag_size, flag_size));
            let resp = ui.allocate_rect(r_rect, egui::Sense::click());
            let over = ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| r_rect.contains(p));
            let on = if which == 0 { node.frozen } else { node.ghost };
            let col = if on {
                if which == 0 {
                    xsi::FROZEN()
                } else {
                    xsi::GHOST()
                }
            } else if over {
                ink
            } else {
                ink_dim
            };
            let c = r_rect.center();
            let r = 4.6 * zoom;
            let st = egui::Stroke::new(1.3 * zoom, col);
            if which == 0 {
                for a in 0..3 {
                    let ang = a as f32 * std::f32::consts::PI / 3.0 + std::f32::consts::FRAC_PI_2;
                    let d = egui::vec2(ang.cos(), ang.sin()) * r;
                    painter.line_segment([c - d, c + d], st);
                }
            } else {
                let q = egui::Rect::from_center_size(c, egui::vec2(r * 1.8, r * 1.8));
                painter.rect_stroke(q, 0.0, st, egui::StrokeKind::Middle);
                painter.line_segment([q.left_top(), q.right_bottom()], st);
            }
            let hit = click_at.is_some_and(|p| r_rect.contains(p));
            on_flag |= hit;
            if resp.clicked() || hit {
                if which == 0 {
                    graph.toggle_frozen(id);
                } else {
                    graph.toggle_ghost(id);
                }
            }
            resp.on_hover_text(match (which, on) {
                (0, true)  => "Frozen: keeps the result it had when frozen; what is above it can change without it cooking again. Click to let it cook again",
                (0, false) => "Freeze: keep this node's result as it is now",
                (_, true)  => "Ghosted: its output is drawn in the Viewer as a grey wireframe. Click to stop",
                _          => "Ghost: draw this node's output in the Viewer as a grey wireframe, beside the scene",
            });
        }
    }
    // A frozen node wears a bar of its colour along the bottom.
    if node.frozen {
        let bar = egui::Rect::from_min_max(
            egui::pos2(rect.min.x + 6.0 * zoom, rect.max.y - 4.0 * zoom),
            egui::pos2(rect.max.x - 6.0 * zoom, rect.max.y - 2.0 * zoom),
        );
        painter.rect_filled(bar, 1.0 * zoom, xsi::FROZEN());
    }

    // Middle button held on a node: its information
    let (mid_pressed, mid_down, press) = ui.input(|i| {
        (
            i.pointer.button_pressed(egui::PointerButton::Middle),
            i.pointer.middle_down(),
            i.pointer.press_origin(),
        )
    });
    if mid_pressed && press.is_some_and(|p| rect.contains(p)) {
        graph.info_node = Some(id);
    }
    if !mid_down && graph.info_node == Some(id) {
        graph.info_node = None;
    }

    // Bypass button, left of the title
    let on_bypass;
    {
        let by_rect = egui::Rect::from_min_size(
            egui::pos2(np.x + 4.0 * zoom, np.y + 4.0 * zoom),
            egui::vec2(eye_size, eye_size),
        );
        let by = ui.allocate_rect(by_rect, egui::Sense::click());
        let over = ui
            .input(|i| i.pointer.hover_pos())
            .map(|p| by_rect.contains(p))
            .unwrap_or(false);
        let col = if node.bypassed {
            xsi::BYPASS()
        } else if over {
            ink
        } else {
            ink_dim
        };
        // Drawn by hand: a ring, with a bar through it when bypassed.
        let c = by_rect.center();
        let r = 4.5 * zoom;
        painter.circle_stroke(c, r, egui::Stroke::new(1.3 * zoom, col));
        if node.bypassed {
            let d = egui::vec2(r, -r) * 1.25;
            painter.line_segment([c - d, c + d], egui::Stroke::new(1.6 * zoom, col));
        }
        on_bypass = click_at.map(|p| by_rect.contains(p)).unwrap_or(false);
        if by.clicked() || on_bypass {
            graph.toggle_bypass(id);
        }
        by.on_hover_text(if node.bypassed {
            "Bypassed: the first input passes through unchanged. Click to turn the node back on"
        } else {
            "Bypass this node"
        });
    }
    if node.bypassed {
        painter.rect_filled(rect, rounding * zoom, xsi::BYPASS_VEIL());
    }

    // Output sockets
    for (i, out) in node.outputs.iter().enumerate() {
        let sp = to_screen(output_socket_pos(node, i));
        let purple = material_socket(node, None);
        let hit = egui::Rect::from_center_size(sp, egui::vec2(SOCKET_HIT, SOCKET_HIT));
        let sr = ui.allocate_rect(hit, egui::Sense::click_and_drag());

        let is_wiring = graph.connecting_from == Some((id, i));
        let r = SOCKET_RADIUS * zoom * if sr.hovered() || is_wiring { 1.3 } else { 1.0 };
        painter.circle_filled(sp, r + 1.0, egui::Color32::from_rgb(30, 30, 30));
        painter.circle_filled(
            sp,
            r,
            if is_wiring {
                xsi::SOCK_OUT_DRAG()
            } else if sr.hovered() {
                if purple {
                    xsi::SOCK_MAT_HOV()
                } else {
                    xsi::SOCK_OUT_HOV()
                }
            } else if purple {
                xsi::SOCK_MAT()
            } else {
                xsi::SOCK_OUT()
            },
        );
        if side_output(node) {
            painter.text(
                egui::pos2(sp.x + SOCKET_RADIUS * zoom + 4.0, sp.y),
                egui::Align2::LEFT_CENTER,
                &out.name,
                egui::FontId::proportional(9.0 * zoom),
                xsi::TEXT_DIM(),
            );
        } else {
            painter.text(
                egui::pos2(sp.x, sp.y + SOCKET_RADIUS * zoom + 3.0),
                egui::Align2::CENTER_TOP,
                &out.name,
                egui::FontId::proportional(9.0 * zoom),
                xsi::TEXT_DIM(),
            );
        }

        // The left button draws a wire. The right button opens the add-node
        // menu for this socket, handled with the canvas.
        let left = ui.input(|i| i.pointer.primary_down());
        if left
            && (sr.drag_started_by(egui::PointerButton::Primary)
                || (sr.is_pointer_button_down_on() && graph.connecting_from.is_none()))
        {
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
        if sr.contains_pointer() && ui.input(|i| i.pointer.primary_released()) && !graph.carrying.is_empty() {
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
            crate::graph_ports::draw_picked(painter, sp, SOCKET_RADIUS * zoom);
        }
    }

    // Input sockets
    for (i, inp) in node.inputs.iter().enumerate() {
        let sp = to_screen(input_socket_pos(node, i));
        let side = side_input(node, i);
        let hit = egui::Rect::from_center_size(sp, egui::vec2(SOCKET_HIT, SOCKET_HIT));
        let sr = ui.allocate_rect(hit, egui::Sense::click_and_drag());

        let r = SOCKET_RADIUS * zoom * if sr.hovered() { 1.3 } else { 1.0 };
        painter.circle_filled(sp, r + 1.0, egui::Color32::from_rgb(30, 30, 30));
        let purple = material_socket(node, Some(i));
        painter.circle_filled(
            sp,
            r,
            if sr.hovered() {
                if purple {
                    xsi::SOCK_MAT_HOV()
                } else {
                    xsi::SOCK_IN_HOV()
                }
            } else if inp.connected_output.is_some() {
                if purple {
                    xsi::SOCK_MAT_CONN()
                } else {
                    xsi::SOCK_IN_CONN()
                }
            } else if purple {
                xsi::SOCK_MAT()
            } else {
                xsi::SOCK_IN()
            },
        );
        if side {
            painter.text(
                egui::pos2(sp.x - SOCKET_RADIUS * zoom - 4.0, sp.y),
                egui::Align2::RIGHT_CENTER,
                &inp.name,
                egui::FontId::proportional(9.0 * zoom),
                xsi::TEXT_DIM(),
            );
        } else {
            painter.text(
                egui::pos2(sp.x, sp.y - SOCKET_RADIUS * zoom - 3.0),
                egui::Align2::CENTER_BOTTOM,
                &inp.name,
                egui::FontId::proportional(9.0 * zoom),
                xsi::TEXT_DIM(),
            );
        }

        // Dragged with a wire in it: the wire picked up, its end following
        // the pointer, to go into another input or, let go of over the
        // canvas, nowhere. Which socket was pressed is found from where the
        // press was: the node's title, under the socket, would take the drag.
        if ui.input(|i| i.pointer.primary_pressed() && i.pointer.press_origin().is_some_and(|p| hit.contains(p))) {
            graph.pressed_input = Some((id, i));
        }
        let pulled = graph.pressed_input == Some((id, i))
            && ui.input(|i| i.pointer.primary_down() && i.pointer.is_decidedly_dragging());
        if pulled && graph.connecting_from.is_none() {
            graph.pressed_input = None;
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
        if sr.contains_pointer() && ui.input(|inp| inp.pointer.primary_released()) {
            if let Some((fn_, fo)) = graph.connecting_from {
                if fn_ != id {
                    graph.add_connection(fn_, fo, id, i);
                    graph.keep_an_input_free(id);
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
            crate::graph_ports::draw_picked(painter, sp, SOCKET_RADIUS * zoom);
        }
    }

    // Body double-click → rename
    // Clear of the inputs on the left side, so a wire is dropped on them.
    let body_rect = egui::Rect::from_min_size(
        egui::pos2(np.x + SOCKET_HIT * 0.5, np.y + title_h),
        egui::vec2(width * zoom - SOCKET_HIT * 0.5, (height - 32.0) * zoom),
    );
    let body_sense = ui.allocate_rect(body_rect, egui::Sense::click());
    if body_sense.double_clicked() {
        graph.renaming_node = Some(id);
        graph.rename_buffer = node.name.clone();
    }

    if graph.renaming_node == Some(id) {
        let edit_rect = egui::Rect::from_min_size(
            egui::pos2(np.x + 4.0, np.y + title_h + 2.0),
            egui::vec2(width * zoom - 8.0, 22.0),
        );
        let r = ui.put(
            edit_rect,
            egui::TextEdit::singleline(&mut graph.rename_buffer).font(egui::FontId::proportional(11.0)),
        );
        // One word: a space typed is an underscore.
        if graph.rename_buffer.chars().any(char::is_whitespace) {
            graph.rename_buffer = super::node_name(&graph.rename_buffer);
        }
        r.request_focus();
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let new_name = graph.rename_buffer.clone();
            if let Some(n) = graph.nodes.iter_mut().find(|n| n.id == id) {
                n.name = new_name;
            }
            graph.renaming_node = None;
        } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) || (!r.has_focus() && !r.gained_focus()) {
            graph.renaming_node = None;
        }
    }

    // Title bar drag / click
    if dr.clicked() && !on_eye && !on_bypass && !on_flag {
        graph.unplug = None;
        // Clicking a node: select it. If not shift-held, clear multi-selection.
        if !ui.input(|i| i.modifiers.shift) {
            graph.selected_nodes.clear();
        }
        graph.selected_node = Some(id);
        if !graph.selected_nodes.contains(&id) {
            graph.selected_nodes.push(id);
        }
    }
    if dr.double_clicked() {
        if let Some(d) = dive_of(node) {
            dive = Some(d);
        }
    }
    // The left button moves nodes; the middle one, held, shows information.
    if dr.drag_started_by(egui::PointerButton::Primary) && graph.connecting_from.is_none() {
        // If dragging a node not in the current selection, replace selection
        if !graph.selected_nodes.contains(&id) {
            graph.selected_nodes = vec![id];
            graph.selected_node = Some(id);
        }
        graph.dragging_node = Some(id);
        graph.drag_offset = dr.interact_pointer_pos().unwrap_or_default() - np;
    }
    if dr.dragged_by(egui::PointerButton::Primary) && graph.dragging_node == Some(id) {
        if let Some(ptr) = dr.interact_pointer_pos() {
            let new_pos = to_canvas(ptr - graph.drag_offset);
            if let Some(n) = graph.nodes.iter_mut().find(|n| n.id == id) {
                let delta = new_pos - n.position;
                n.position = new_pos;
                // Move all other selected nodes by the same delta
                if graph.selected_nodes.len() > 1 {
                    let ids: Vec<NodeId> = graph.selected_nodes.iter().copied().filter(|&i| i != id).collect();
                    for other_id in ids {
                        if let Some(other) = graph.nodes.iter_mut().find(|n| n.id == other_id) {
                            other.position += delta;
                        }
                    }
                }
            }
        }
    }
    if dr.drag_stopped() {
        graph.dragging_node = None;
    }

    // A prim dragged from the Scene Explorer onto an operator that works on
    // primitives joins its Primitive Locations; the operator is selected
    // with its Prims Location tab open.
    if crate::core::locations::takes_locations(&node.node_type)
        && egui::DragAndDrop::has_payload_of_type::<crate::types::DraggedPrims>(ui.ctx())
    {
        let over = ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| rect.contains(p));
        if over {
            painter.rect_stroke(
                rect.expand(3.0),
                6.0 * zoom,
                egui::Stroke::new(2.0, crate::theme::selection()),
                egui::StrokeKind::Outside,
            );
            if ui.input(|i| i.pointer.any_released()) {
                if let Some(p) = egui::DragAndDrop::take_payload::<crate::types::DraggedPrims>(ui.ctx()) {
                    if let Some(n) = graph.nodes.iter_mut().find(|n| n.id == node.id) {
                        for path in &p.0 {
                            if !n.locations.iter().any(|l| l.trim() == path) {
                                n.locations.push(path.clone());
                            }
                        }
                    }
                    graph.selected_node = Some(node.id);
                    graph.selected_nodes = vec![node.id];
                    ui.data_mut(|d| d.insert_temp(egui::Id::new(("parameters_tab", node.id.0)), true));
                }
            }
        }
    }

    dive
}

/// The palette node bodies take, by add-menu category.
const CATEGORY_COLOURS: [(&str, [u8; 3]); 10] = [
    // The pastel palette at 80%, one colour per category.
    ("Create", [0xAE, 0xC0, 0x96]),     // light green   (#DAF0BB)
    ("Modify", [0x96, 0xA2, 0xC0]),     // light blue    (#BCCAF0)
    ("UV", [0xC0, 0xB2, 0x88]),         // sand          (#F0DEAA)
    ("File", [0xC0, 0x9E, 0x96]),       // salmon        (#F0C5BB)
    ("Primitives", [0x55, 0x5F, 0x7C]), // slate blue    (#6A779B)
    ("Animation", [0x96, 0xA2, 0xC0]),  // light blue    (#BCCAF0)
    ("Mocap", [0xAB, 0x98, 0xBA]),      // mauve         (#D6BEE8)
    ("Materials", [0xC0, 0x96, 0xA8]),  // rose          (#F0BCD2)
    ("Render", [0x96, 0xBA, 0xC0]),     // light cyan    (#BBE8F0)
    ("Lights", [0xC0, 0xB8, 0x78]),     // pale yellow   (#F0E696)
];

/// The ICE node's own colour, whatever its category: the teal of Softimage's
/// ICETree, at the same 80% as the palette.
const ICE_NODE_COLOUR: [u8; 3] = [0x5A, 0x95, 0x7B]; // teal          (#70BA9A)

/// Cameras stand apart from the shapes they are listed with: mid grey, with
/// dark text as the other nodes have.
const CAMERA_NODE_COLOUR: [u8; 3] = [0x6E, 0x6E, 0x6E];

/// System Commands look like a 90s terminal: black, green text.
const SYSTEM_NODE: ([u8; 3], [u8; 3], [u8; 3]) = ([0x0C, 0x0C, 0x0C], [0x4C, 0xF0, 0x4C], [0x2E, 0xB0, 0x2E]);

/// A node's body colour, and the text colours that read on it: black on the
/// light colours, white on the dark ones. The ICE node has its own colour;
/// nodes in no category keep the theme's node grey.
fn node_colours(t: &NodeType) -> (egui::Color32, egui::Color32, egui::Color32) {
    // The ICE node stands apart from its category.
    if matches!(t, NodeType::Ice { .. }) {
        let [r, g, b] = ICE_NODE_COLOUR;
        return colours_on(r, g, b);
    }
    if matches!(t, NodeType::Camera { .. }) {
        let [r, g, b] = CAMERA_NODE_COLOUR;
        // Dark text on its mid grey, a darker dim than on the light colours.
        return (
            egui::Color32::from_rgb(r, g, b),
            egui::Color32::from_rgb(10, 10, 10),
            egui::Color32::from_rgb(38, 38, 38),
        );
    }
    if is_system_command(t) {
        let rgb = |[r, g, b]: [u8; 3]| egui::Color32::from_rgb(r, g, b);
        return (rgb(SYSTEM_NODE.0), rgb(SYSTEM_NODE.1), rgb(SYSTEM_NODE.2));
    }
    use std::sync::OnceLock;
    type Table = Vec<(std::mem::Discriminant<NodeType>, [u8; 3])>;
    static TABLE: OnceLock<Table> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        catalog()
            .into_iter()
            .flat_map(|(_, category, nodes)| {
                let colour = CATEGORY_COLOURS
                    .iter()
                    .find(|(c, _)| *c == category)
                    .map(|(_, rgb)| *rgb);
                nodes
                    .into_iter()
                    .filter_map(move |(n, _)| colour.map(|c| (std::mem::discriminant(&n), c)))
            })
            .collect()
    });
    let Some(&(_, [r, g, b])) = table.iter().find(|(d, _)| *d == std::mem::discriminant(t)) else {
        return (xsi::NODE_BODY(), xsi::TEXT(), xsi::TEXT_DIM());
    };
    colours_on(r, g, b)
}

/// Whether a node is in the System Commands category of the add menu.
fn is_system_command(t: &NodeType) -> bool {
    use std::sync::OnceLock;
    static KINDS: OnceLock<Vec<std::mem::Discriminant<NodeType>>> = OnceLock::new();
    KINDS
        .get_or_init(|| {
            catalog()
                .into_iter()
                .filter(|(_, c, _)| *c == "System Commands")
                .flat_map(|(_, _, nodes)| nodes.into_iter().map(|(n, _)| std::mem::discriminant(&n)))
                .collect()
        })
        .contains(&std::mem::discriminant(t))
}

/// A body colour and the text colours that read on it.
fn colours_on(r: u8, g: u8, b: u8) -> (egui::Color32, egui::Color32, egui::Color32) {
    let light = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32 > 125.0;
    let (ink, dim) = if light {
        (egui::Color32::from_rgb(15, 15, 15), egui::Color32::from_rgb(70, 70, 70))
    } else {
        (
            egui::Color32::from_rgb(248, 248, 248),
            egui::Color32::from_rgb(215, 215, 215),
        )
    };
    (egui::Color32::from_rgb(r, g, b), ink, dim)
}

// ADD-NODE MENU

/// Every node the menu offers, by category: the node as it is made, and one
/// line on what it does. Names and icons come from the node types, so the menu,
/// the node and the properties say the same thing.
pub fn catalog() -> Vec<(&'static str, &'static str, Vec<(NodeType, &'static str)>)> {
    use crate::types::SplitPick;
    vec![
        ("◼", "Create", vec![
            (NodeType::cube(1.0), "A cube"),
            (NodeType::sphere(0.5, 32), "A sphere"),
            (NodeType::grid(10, 10, 2.0), "A flat grid of quads"),
            (NodeType::camera(), "A camera at a Primitive Location, with its lens, gate and clipping as USD has them. Look through it from the Viewer's camera menu"),
            (NodeType::TestClip { seconds: 4.0, fps_num: 30, fps_den: 1 }, "A walk on the spot, to try the animation nodes"),
        ]),
        ("📝", "Placeholder", vec![
            (NodeType::Placeholder { inputs: vec!["In".into()], outputs: vec!["Out".into()], description: String::new() }, "A node that does not exist yet: name it, give it inputs and outputs, describe what it should do. Data goes through unchanged"),
        ]),
        ("↓", "Organise", vec![
            (NodeType::SetVariable { vars: vec![crate::core::vars::Variable::new("shot", "")] }, "Variables of its own, written {name}, for every operator below it: a shot, a variant, a folder. What comes in goes on"),
            (NodeType::PassThrough, "Hands the scene on unchanged: for laying out big graphs, a place to gather wires or branch from"),
        ]),
        ("📂", "File", vec![
            (NodeType::load_usd(String::new()), "A USD file, as primitives: sublayered, or its default prim referenced or payloaded at a Primitive Location"),
            (NodeType::WriteUsd { path: String::new() }, "What the scene graph changed in its USD stages, as an override layer"),
            (NodeType::LoadFbx { path: String::new(), take: 0 }, "Everything in an FBX file: the clip, every mesh with its weights and materials, the rest skeleton"),
            (NodeType::LoadFbxDir { dir: String::new(), index: 0, take: 0 }, "One FBX of a folder at a time; Write FBX can run over all of them"),
            (NodeType::WriteFbx { path: crate::types::DEFAULT_WRITE_PATH.into(), mesh: false }, "Write the clip as FBX, with the skeleton it came in with"),
        ]),
        ("🔨", "Modify", vec![
            (NodeType::Transform { translation: Vec3::ZERO, rotation: Vec3::ZERO, scale: Vec3::ONE }, "Translate, rotate and scale anything: a mesh, primitives or a clip"),
            (NodeType::EditPoly { ops: vec![], pending: Default::default(), edit: None, auto_collapse: false }, "Polygon modelling in one node"),
            (NodeType::merge(), "Any number of inputs as one: primitives side by side, or one mesh. Wiring the last input adds another"),
            (NodeType::Ice { id: IceGraphId(usize::MAX), name: "ICreationEngine".into(), references: 1 }, "ICE: a graph inside a node, run on every point, face or corner of each primitive"),
        ]),
        ("📦", "Primitives", vec![
            (NodeType::PickPrims { pattern: String::new() }, "Choose primitives for the operators after this one to work on"),
            (NodeType::PrunePrims { pattern: String::new(), keep: false }, "Remove primitives, or keep only some"),
            (NodeType::SetAttribute { rows: vec![] }, "Set values on prims: a primvar for rendering, or a property such as visibility, purpose, kind or active"),
            (NodeType::UnpackPrims, "Merge primitives into one mesh"),
        ]),
        ("🎨", "Materials", vec![
            (NodeType::Material { surface: Default::default() }, "A UsdPreviewSurface material: colour, roughness, metallic, opacity and textures. Named by the node"),
            (NodeType::materialx(), "A material made from a graph of MaterialX nodes: standard_surface, open_pbr_surface, textures, maths. Double-click it to open its graph. Named by the node"),
            (NodeType::MaterialBinding { library: crate::types::material_library(), inputs: 1, bindings: vec![] }, "Put materials in the scene and bind them: each Prims Location row binds the material input of its number"),
        ]),
        ("💡", "Lights", vec![
            (NodeType::light(crate::core::light::Light::new(crate::core::light::LightType::Dome)), "Light from every direction, from an environment image (an HDR): the sky and everything around"),
            (NodeType::light(crate::core::light::Light::new(crate::core::light::LightType::Rect)), "A rectangle that emits from one side, along its -Z: a softbox, a window"),
            (NodeType::light(crate::core::light::Light::point()), "A point: light from one place in every direction (a sphere light treated as a point)"),
            (NodeType::light(crate::core::light::Light::new(crate::core::light::LightType::Sphere)), "A sphere that emits from its surface: a bulb"),
            (NodeType::light(crate::core::light::Light::new(crate::core::light::LightType::Disk)), "A disk that emits from one side, along its -Z: a spot's lens"),
            (NodeType::light(crate::core::light::Light::new(crate::core::light::LightType::Cylinder)), "A cylinder along its X: a tube"),
            (NodeType::light(crate::core::light::Light::new(crate::core::light::LightType::Distant)), "Parallel light from far away, along its -Z: the sun"),
        ]),
        ("📽", "Render", vec![
            (NodeType::RenderCommons { scene: Default::default() }, "The RenderSettings prim: camera, resolution, overscan, motion blur, depth of field, purposes; and the RenderProduct, the image file it makes"),
            (NodeType::RenderSettings { renderer: Default::default() }, "A renderer's settings (Karma, Arnold, or a Custom one typed in) on the scene's RenderSettings prim, and its standard AOVs as RenderVars"),
            (NodeType::RenderVariables { vars: Default::default() }, "RenderVars (AOVs) of a product: add or override them row by row: name, source, type, format, filter"),
        ]),
        ("🖥", "System Commands", vec![
            (NodeType::RenderCommand { command: Default::default() }, "Run a command line on the system: husk on the layer the Write USD above writes ({usd}), over the scene's frames. Runs when you press Run, never on cook"),
        ]),
        ("🗺", "UV", vec![
            (NodeType::UvUnwrap { method: crate::core::uv::UvMethod::Conformal, angle: 66.0, margin: 0.02, axis: 1, tiles: 1 }, "Make texture coordinates"),
            (NodeType::UvTransform { offset: [0.0; 2], rotate: 0.0, scale: [1.0; 2] }, "Translate, rotate and scale the whole UV layout"),
            (NodeType::UvEdit { edits: vec![] }, "Move single UV islands, in the UV Editor"),
        ]),
        ("🎬", "Animation", vec![
            (NodeType::LocalAnimation { start: 1, end: 48, inherited: serde_json::Value::Null }, "Key the parameters of the operator before it, on frames of its own, from Start to End"),
            (NodeType::MergeAnimation { align: crate::types::AnimAlign::First, match_rate: false, inputs: 2 }, "Inputs as one, each moved in time: to start, end or be centred with input A, or stretched or squeezed to fit"),
            (NodeType::RenameJoints { find: String::new(), replace: String::new(), strip_namespace: true, prefix: String::new() }, "Rename joints by pattern"),
            (NodeType::TrimClip { head: 0, tail: 0 }, "Cut frames from the start and the end"),
            (NodeType::Retime { fps_num: 30, fps_den: 1, mode: RetimeMode::Resample }, "Change the frame rate"),
            (NodeType::SetTimecode { hours: 1, minutes: 0, seconds: 0, frames: 0, drop_frame: false }, "Change where the clip starts"),
            (NodeType::TimeWarp { speed: 1.0, reverse: false }, "Play faster, slower or backwards"),
            (NodeType::BlendClips { blend: 15, align: true }, "One clip, then another, blended"),
            (NodeType::LoopClip { blend: 15 }, "Ease the end into the start, so the clip cycles"),
            (NodeType::MirrorClip, "Swap left and right"),
            (NodeType::SmoothClip { radius: 3, amount: 1.0, translations: true }, "Filter out jitter"),
            (NodeType::InPlace { keep_height: true, to_root: false }, "Hold the hips over their starting point"),
            (NodeType::FloorClip { height: 0.0 }, "Put the lowest point on the floor"),
            (NodeType::PruneJoints { words: "finger, thumb".into() }, "Remove joints and everything below them"),
        ]),
        ("🏃", "Mocap", vec![
            (NodeType::SplitSkeleton { picks: vec![SplitPick::Character(0), SplitPick::Character(1)] }, "One output per character in the take"),
            (NodeType::Characterize { picks: vec![] }, "Say which joint is which part of a human, for Body Collide and Retarget"),
            (NodeType::Retarget, "The motion of one skeleton on another"),
            (NodeType::Reach { settings: Default::default() }, "Hands and feet where the motion before the retarget has them, within human limits"),
            (NodeType::HumanLimits { limits: Default::default() }, "A clip something moved, put back within what a human body can do"),
            (NodeType::Capture { falloff: 6.0 }, "Skin weights for a mesh, by proximity to the bones"),
            (NodeType::Skin, "Put a mesh's skin weights on any animation, by joint name"),
            (NodeType::AutoTPose { set_hip_height: false, hip_height: 90.0 }, "A neutral pose, one frame"),
            (NodeType::FixPose { edits: vec![] }, "Correct joints by hand, on every frame"),
            (NodeType::ProxySkin { thickness: 1.0 }, "A body of spheres and cylinders, bound to the skeleton"),
            (NodeType::Ragdoll { settings: Default::default(), view: Default::default(), limits: Default::default() }, "Keep the character out of the set and out of itself, within what a human body can do"),
        ]),
    ]
}

/// Name for a new node of this type: its label without spaces.
pub fn default_name(t: &NodeType) -> String {
    node_type_label(t).chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Width in pixels of the ring around a node. It follows the zoom, but a
/// selected node keeps a ring of at least 3 pixels however far the graph is
/// zoomed out, and at most 6 however far in, so the selection always shows.
pub fn ring_width(zoom: f32, selected: bool) -> f32 {
    if selected {
        (4.0 * zoom).clamp(3.0, 6.0)
    } else {
        2.0 * zoom.max(0.5)
    }
}

fn add_node_menu(ui: &mut egui::Ui, graph: &mut NodeGraphState, cp: egui::Pos2, opened_now: bool) -> bool {
    let mut added = false;
    ui.set_min_width(150.0);
    // Type to find a node: by its name, its category or what it does.
    let search_id = egui::Id::new("scene_add_search");
    let mut search: String = if opened_now {
        String::new()
    } else {
        ui.data(|d| d.get_temp(search_id)).unwrap_or_default()
    };
    let field = ui.add(
        egui::TextEdit::singleline(&mut search)
            .hint_text("Search nodes")
            .desired_width(170.0),
    );
    if opened_now {
        field.request_focus();
    }
    ui.data_mut(|d| d.insert_temp(search_id, search.clone()));
    let words = search.trim().to_lowercase();
    if !words.is_empty() {
        ui.separator();
        let found: Vec<(NodeType, &'static str, &'static str)> = catalog()
            .into_iter()
            .flat_map(|(_, category, nodes)| nodes.into_iter().map(move |(t, hint)| (t, hint, category)))
            .filter(|(t, hint, category)| {
                let text = format!("{} {} {}", node_type_label(t), category, hint).to_lowercase();
                words.split_whitespace().all(|w| text.contains(w))
            })
            .collect();
        // ↓ and ↑ move through what is found, Enter takes it.
        let (hi, moved, enter) =
            crate::menus::search_keys(ui, egui::Id::new("scene_add_pick"), &words, found.len(), opened_now);
        let mut chosen = if enter {
            found.get(hi).map(|(t, _, _)| t.clone())
        } else {
            None
        };
        for (k, (t, hint, category)) in found.iter().enumerate() {
            let label = format!("{}  {}   {category}", node_type_icon(t), node_type_label(t));
            let r = ui.selectable_label(k == hi, label).on_hover_text(*hint);
            if moved && k == hi {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                chosen = Some(t.clone());
            }
        }
        if found.is_empty() {
            ui.label("No node by that name");
        }
        if let Some(t) = chosen {
            let name = default_name(&t);
            graph.add_node(name, t, cp);
            added = true;
        }
        return added;
    }
    ui.separator();
    // Categories look like the items in them: menu rows, not buttons.
    ui.visuals_mut().button_frame = false;
    for (icon, category, nodes) in catalog() {
        crate::menus::menu(ui, format!("{icon}  {category}"), |ui| {
            ui.set_min_width(170.0);
            for (t, hint) in nodes {
                let label = format!("{}  {}", node_type_icon(&t), node_type_label(&t));
                if ui.button(label).on_hover_text(hint).clicked() {
                    let name = default_name(&t);
                    graph.add_node(name, t, cp);
                    added = true;
                    // Closes the category's menu. The Tab menu around it is
                    // an area of its own, closed by the caller on `added`.
                    ui.close();
                }
            }
        });
    }
    added
}

// BEZIER HELPERS

#[allow(dead_code)]
pub fn draw_wire(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, hovered: bool, selected: bool) {
    draw_wire_to(painter, from, to, (false, false), hovered, selected);
}

/// The curve's handles: down out of an output below the node or right out of
/// one on its right side, and down into an input on top or right into one on
/// the left side. `side`: (the output is on the side, the input is).
fn handles(from: egui::Pos2, to: egui::Pos2, side: (bool, bool)) -> (egui::Pos2, egui::Pos2) {
    let off = (to.y - from.y).abs().max(60.0) * 0.5;
    let across = (to.x - from.x).abs().max(60.0) * 0.5;
    let c1 = if side.0 {
        egui::pos2(from.x + across, from.y)
    } else {
        egui::pos2(from.x, from.y + off)
    };
    let c2 = if side.1 {
        egui::pos2(to.x - across, to.y)
    } else {
        egui::pos2(to.x, to.y - off)
    };
    (c1, c2)
}

/// A wire between an output and an input, either of them on a side.
pub fn draw_wire_to(
    painter: &egui::Painter,
    from: egui::Pos2,
    to: egui::Pos2,
    side: (bool, bool),
    hovered: bool,
    selected: bool,
) {
    let (c1, c2) = handles(from, to, side);
    let pts: Vec<egui::Pos2> = (0..=20).map(|i| bezier(from, c1, c2, to, i as f32 / 20.0)).collect();
    let (width, color) = if selected {
        (3.5, egui::Color32::from_rgb(220, 120, 80))
    } else if hovered {
        (3.5, xsi::WIRE_HOV())
    } else {
        (2.0, xsi::WIRE())
    };
    painter.add(egui::Shape::line(pts, egui::Stroke::new(width, color)));
}

fn is_near_bezier(p: egui::Pos2, from: egui::Pos2, to: egui::Pos2, side: (bool, bool), thresh: f32) -> bool {
    let (c1, c2) = handles(from, to, side);
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
// NODE INFORMATION (middle button held on a node)

/// Lines describing what one output of a node puts out.
pub fn output_info(r: &Option<crate::types::EvalResult>) -> Vec<String> {
    use crate::types::EvalResult;
    match r {
        None => vec!["Nothing".into()],
        Some(EvalResult::Single(m)) => vec![format!(
            "One mesh: {} points, {} triangles",
            m.vertices.len(),
            m.indices.len() / 3
        )],
        Some(EvalResult::Named(prims)) => {
            let copies: usize = prims.iter().map(|p| p.place.copies().max(1)).sum();
            let points: usize = prims
                .iter()
                .map(|p| p.mesh.vertices.len() * p.place.copies().max(1))
                .sum();
            let tris: usize = prims
                .iter()
                .map(|p| p.mesh.indices.len() / 3 * p.place.copies().max(1))
                .sum();
            let cameras = prims.iter().filter(|p| p.camera.is_some()).count();
            let renders = prims.iter().filter(|p| p.render.is_some()).count();
            let mut mats: Vec<&str> = prims.iter().filter_map(|p| p.material.as_deref()).collect();
            mats.sort();
            mats.dedup();
            let mut out = vec![format!(
                "{} primitive{}{}",
                prims.len(),
                if prims.len() == 1 { "" } else { "s" },
                if copies != prims.len() {
                    format!(", {copies} copies drawn")
                } else {
                    String::new()
                }
            )];
            out.push(format!("{points} points, {tris} triangles"));
            if !mats.is_empty() {
                out.push(format!(
                    "{} material{}",
                    mats.len(),
                    if mats.len() == 1 { "" } else { "s" }
                ));
            }
            if cameras > 0 {
                out.push(format!("{cameras} camera{}", if cameras == 1 { "" } else { "s" }));
            }
            if renders > 0 {
                out.push(format!("{renders} render prim{}", if renders == 1 { "" } else { "s" }));
            }
            if let Some(p) = prims.first() {
                out.push(format!("First: {}", p.path));
            }
            out
        }
        Some(EvalResult::Anim(c)) => {
            let mut out = vec![format!(
                "Clip: {} joints, {} frames at {}/{} fps",
                c.joints.len(),
                c.frames,
                c.rate.num,
                c.rate.den
            )];
            out.push(format!("{:.2} s, from frame {}", c.duration_seconds(), c.start_frame));
            if let Some(k) = &c.skin {
                out.push(format!("Skinned mesh: {} points", k.positions.len()));
            }
            out
        }
        Some(EvalResult::Material(m)) => vec![format!("Material {}", m.name)],
    }
}

/// While the middle button is held on a node: a panel beside it with its type,
/// its flags, what is wired in, what each output puts out and how long it last
/// took to cook. Its outputs are cooked if they are not yet.
pub fn info_popup(
    ctx: &egui::Context,
    graph: &NodeGraphState,
    canvas: egui::Rect,
    eval_subnet: &impl Fn(
        crate::types::IceGraphId,
        &crate::core::geo::Geo,
        &[crate::ice::Reference],
    ) -> crate::core::geo::Geo,
) {
    let Some(id) = graph.info_node else { return };
    let Some(node) = graph.nodes.iter().find(|n| n.id == id) else {
        return;
    };
    let (pan, zoom) = (graph.pan_offset, graph.zoom);
    let right = canvas.min
        + egui::vec2(
            (node.position.x + node_width(node)) * zoom + pan.x + 10.0,
            node.position.y * zoom + pan.y,
        );
    egui::Area::new(egui::Id::new("node_info"))
        .order(egui::Order::Tooltip)
        .fixed_pos(right)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(320.0);
                ui.label(egui::RichText::new(&node.name).strong());
                ui.label(egui::RichText::new(node_type_label(&node.node_type)).weak());
                let mut flags = vec![];
                if graph.view_flag == Some(id) {
                    flags.push("viewed");
                }
                if node.bypassed {
                    flags.push("bypassed");
                }
                if node.frozen {
                    flags.push("frozen");
                }
                if node.ghost {
                    flags.push("ghosted");
                }
                if !flags.is_empty() {
                    ui.label(flags.join(", "));
                }
                ui.separator();
                for (i, inp) in node.inputs.iter().enumerate() {
                    let from = inp
                        .connected_output
                        .and_then(|(src, out)| graph.nodes.iter().find(|n| n.id == src).map(|n| (n, out)))
                        .map(|(n, out)| {
                            if n.outputs.len() > 1 {
                                format!(
                                    "{}.{}",
                                    n.name,
                                    n.outputs.get(out).map(|o| o.name.as_str()).unwrap_or("?")
                                )
                            } else {
                                n.name.clone()
                            }
                        });
                    if from.is_some() || i == 0 {
                        ui.label(format!(
                            "In {}: {}",
                            inp.name,
                            from.unwrap_or_else(|| "not wired".into())
                        ));
                    }
                }
                for (k, out) in node.outputs.iter().enumerate() {
                    let r = graph.eval_node_out(id, k, &mut Default::default(), eval_subnet);
                    ui.label(egui::RichText::new(format!("Out {}", out.name)).strong());
                    for line in output_info(&r) {
                        ui.label(line);
                    }
                }
                if let Some(ms) = graph.cook_times.get(id) {
                    ui.separator();
                    ui.label(egui::RichText::new(format!("Last cooked in {ms:.1} ms (its own work)")).weak());
                }
            });
        });
}
