use std::collections::HashSet;
use crate::types::{ConnectionId, NodeId, IceGraphNodeType};
use super::{IceGraph, IceGraphNode};

// ── ICE look, named ───────────────────────────────────────────────────────────
// Modelled on Softimage ICE: nodes in solid colours by kind (green for data
// and maths, blue for getting and setting geometry, teal for the tree's in and
// out) with a two-line title (the node's own name in bold, its type beneath)
// and a collapse box; a row per port,
// outputs first then inputs; small dots on the node's edge coloured by data
// type, wires in the same colour; a light grey canvas with a fine grid. Every
// size and colour is here, so the look can be tuned in one place.

const TITLE_H:  f32 = 32.0;   // two lines: the node's name, then its type
const FOLDED_H: f32 = 20.0;   // a collapsed node: its name only
const ROW_H:    f32 = 15.0;   // one port per row
const PAD_B:    f32 = 4.0;    // space under the last row
const MIN_W:    f32 = 96.0;
const TITLE_CH: f32 = 7.4;    // width of a title character, for sizing nodes
const LABEL_CH: f32 = 5.9;    // width of a label character
const ICONS_W:  f32 = 20.0;   // collapse box, right of the title
const PORT_R:   f32 = 3.2;
const SOCK_HIT: f32 = 16.0;   // invisible hit area around a port
const ROUNDING: f32 = 5.0;
const TYPE_PT:  f32 = 9.5;    // the type line under the name
const TITLE_PT: f32 = 12.5;
const LABEL_PT: f32 = 10.5;
const GRID:     f32 = 12.0;   // canvas grid spacing
const WIRE_W:   f32 = 1.6;

mod pal {
    #![allow(non_snake_case)]
    use egui::Color32;
    // ICE's own look, the same in both themes.
    pub fn BG() -> Color32 { Color32::from_rgb(170, 170, 170) }
    pub fn GRID() -> Color32 { Color32::from_rgb(161, 161, 161) }
    pub fn BREADCRUMB_BG() -> Color32 { crate::theme::c( 78,  78,  82) }
    pub fn TEXT() -> Color32 { crate::theme::c(248, 248, 248) }
    pub fn TEXT_DIM() -> Color32 { crate::theme::c(210, 210, 210) }

    pub fn INK() -> Color32 { Color32::from_rgb( 12,  12,  12) }       // titles and labels
    pub fn INK_TYPE() -> Color32 { Color32::from_rgba_unmultiplied(12, 12, 12, 150) }  // the type line
    pub fn DIVIDER() -> Color32 { Color32::from_black_alpha(80) }     // the line under the title
    pub fn RIM() -> Color32 { Color32::from_rgb( 38,  38,  38) }       // the node's edge
    pub fn FRAME() -> Color32 { Color32::from_rgb(105, 105, 105) }     // the grey ring around it
    pub fn SELECTED() -> Color32 { crate::theme::selection() }  // the ring of a selected node
    pub fn PORT_RIM() -> Color32 { Color32::from_rgb( 30,  30,  30) }

    // Node colours, by kind.
    pub fn GREEN() -> Color32 { Color32::from_rgb(152, 196,  98) }     // data, constants, maths, generators
    pub fn BLUE() -> Color32  { Color32::from_rgb(102, 150, 204) }     // get / set geometry
    pub fn TEAL() -> Color32  { Color32::from_rgb(112, 186, 154) }     // the tree's in and out

    // Data types: the colours every graph of XMS gives them (`type_colours`).
    pub fn ERROR() -> Color32   { Color32::from_rgb(230,  40,  30) }   // the node a graph broke at
    pub fn OTHER() -> Color32   { crate::type_colours::DataType::Other.colour() }   // execute and the rest: dark grey
}

/// Where everything of a node sits, relative to its top-left corner.
struct Layout {
    size:    egui::Vec2,
    inputs:  Vec<egui::Vec2>,
    outputs: Vec<egui::Vec2>,
}

/// The title: the node type's full name in PascalCase (`GetData`,
/// `CrossProduct`, `IceTree`).
fn title_text(node: &IceGraphNode) -> String {
    crate::types::subnet_node_label(&node.node_type)
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect()
}

/// A constant's value, or the attribute a Get or Set Data names, shown in
/// a row of its own under the title.
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
            Value::Vec4(v) => list(v),
        }),
        // Across contexts: where from, and how the values become one.
        IceGraphNodeType::GetData { name, context: Some(c), reduce } => Some(format!("{}.{name} ({})", context_word(*c), reduce_word(*reduce))),
        IceGraphNodeType::SetData { name, context: Some(c), reduce } => Some(format!("{}.{name} ({})", context_word(*c), reduce_word(*reduce))),
        IceGraphNodeType::GetData { name, .. } | IceGraphNodeType::SetData { name, .. } => Some(format!("self.{name}")),
        IceGraphNodeType::Root { context } => Some(format!("per {context:?}")),
        IceGraphNodeType::Math { .. } => None,
    }
}

/// A context as Get Data's line under the title names it.
fn context_word(c: crate::core::geo::Context) -> &'static str {
    use crate::core::geo::Context;
    match c { Context::Point => "point", Context::Face => "face", Context::Corner => "corner", Context::Object => "object" }
}

fn reduce_word(r: xms_ice::Reduce) -> &'static str {
    match r { xms_ice::Reduce::Average => "average", xms_ice::Reduce::Sum => "sum", xms_ice::Reduce::Min => "min", xms_ice::Reduce::Max => "max" }
}

/// A number as short as it reads: 1, 0.5, 0.125.
fn short(v: f32) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// Rows: the outputs, then the inputs. Collapsed, a node is its title row
/// alone, every port on its edge at mid-height.
fn layout(node: &IceGraphNode, collapsed: bool) -> Layout {
    let title = (node.name.chars().count() as f32 * TITLE_CH).max(title_text(node).chars().count() as f32 * LABEL_CH) + ICONS_W + 18.0;
    let label = |n: &str| n.chars().count() as f32 * LABEL_CH + 22.0;
    let value = value_text(node);
    let widest = node.outputs.iter().map(|o| label(&o.name))
        .chain(node.inputs.iter().map(|i| label(&i.name)))
        .chain(value.as_deref().map(label))
        .fold(0.0, f32::max);
    let w = title.max(widest).max(MIN_W).round();
    if collapsed {
        let mid = FOLDED_H * 0.5;
        return Layout {
            size:    egui::vec2(w, FOLDED_H),
            inputs:  vec![egui::vec2(0.0, mid); node.inputs.len()],
            outputs: vec![egui::vec2(w, mid); node.outputs.len()],
        };
    }
    // A constant's value takes the first row.
    let first = value.is_some() as usize;
    let row_y = |r: usize| TITLE_H + (r as f32 + 0.5) * ROW_H;
    let outs = node.outputs.len();
    Layout {
        size:    egui::vec2(w, TITLE_H + (first + outs + node.inputs.len()) as f32 * ROW_H + PAD_B),
        outputs: (0..outs).map(|r| egui::vec2(w, row_y(first + r))).collect(),
        inputs:  (0..node.inputs.len()).map(|r| egui::vec2(0.0, row_y(first + outs + r))).collect(),
    }
}

fn input_socket_pos(node: &IceGraphNode, idx: usize, collapsed: &HashSet<NodeId>) -> egui::Pos2 {
    node.position + layout(node, collapsed.contains(&node.id)).inputs.get(idx).copied().unwrap_or_default()
}

fn output_socket_pos(node: &IceGraphNode, idx: usize, collapsed: &HashSet<NodeId>) -> egui::Pos2 {
    node.position + layout(node, collapsed.contains(&node.id)).outputs.get(idx).copied().unwrap_or_default()
}

/// A node's colour, by kind.
fn tint(t: &IceGraphNodeType) -> egui::Color32 {
    match t {
        IceGraphNodeType::Root { .. } => pal::TEAL(),
        IceGraphNodeType::GetData { .. } | IceGraphNodeType::SetData { .. } => pal::BLUE(),
        IceGraphNodeType::Constant { .. } | IceGraphNodeType::Math { .. } => pal::GREEN(),
    }
}

/// Which nodes are collapsed: view state, kept with the canvas, not saved.
fn collapsed_id() -> egui::Id { egui::Id::new("ice_collapsed_nodes") }

// ── Breadcrumb ────────────────────────────────────────────────────────────────

pub fn draw_breadcrumb(ui: &mut egui::Ui, subnet_name: &str) -> bool {
    let mut exit = false;
    ui.horizontal(|ui| {
        egui::Frame::NONE
            .fill(pal::BREADCRUMB_BG())
            .inner_margin(egui::vec2(8.0, 4.0))
            .show(ui, |ui| {
                if ui.link(egui::RichText::new("Root").color(pal::TEXT())).clicked() { exit = true; }
                ui.label(egui::RichText::new(" › ").color(pal::TEXT_DIM()));
                ui.label(egui::RichText::new(subnet_name)
                    .color(egui::Color32::from_rgb(180, 200, 230))
                    .strong());
            });
    });
    exit
}

// ── Subnet node properties ────────────────────────────────────────────────────

/// The parameters of a node inside an ICE subnet, laid out like every
/// other node's (see `properties::form`).
pub fn draw_subnet_node_properties(ui: &mut egui::Ui, graph: &mut IceGraph) {
    use crate::core::geo::Context;
    use crate::properties::form::group;
    use xms_ice::Value;
    // The geometry the graph last ran on, for Get Data's list: listed only
    // while the list is open.
    let last_input = graph.last_input.clone();
    // The context the graph runs over: Set Data's Several is for writing
    // from it into another.
    let runs_over = graph.nodes.iter().find_map(|n| match n.node_type { IceGraphNodeType::Root { context } => Some(context), _ => None })
        .unwrap_or(Context::Point);
    let Some(node) = graph.selected_node.and_then(|s| graph.nodes.iter_mut().find(|n| n.id == s)) else {
        ui.label("Select a node to see its parameters.");
        return;
    };
    let title = format!("{}  {}", crate::types::subnet_node_icon(&node.node_type), crate::types::subnet_node_label(&node.node_type));
    group(ui, &title, None, |f| { f.text("Name", None, &mut node.name, ""); });
    match &mut node.node_type {
        IceGraphNodeType::Root { context } => group(ui, "Runs over", None, |f| {
            f.choice("Context", Some("The elements every Execute port runs over"), context,
                     &[(Context::Point, "Point"), (Context::Face, "Face"), (Context::Corner, "Corner"), (Context::Object, "Object")]);
        }),
        IceGraphNodeType::GetData { name, context, reduce } => {
            group(ui, "Attribute", None, |f| {
                f.text("Name", Some("P is the points, N the normals, uv the first texture coordinates; any other name is an attribute of the geometry"), name, "P");
                // Every attribute of the geometry the graph last ran on, as the
                // Primitive Inspector lists them: choosing one sets the name
                // and where it is read from. Listed when opened, not before.
                if last_input.lock().is_ok_and(|g| g.is_some()) {
                    f.row("Attributes", Some("The attributes of the geometry coming in: pick one to read it"), |ui| {
                        egui::ComboBox::from_id_salt("ice_get_data_attrs").width(ui.available_width() - 8.0)
                            .selected_text("Pick an attribute...").show_ui(ui, |ui| {
                                for (c, n, kind) in &super::readable(&last_input) {
                                    if ui.selectable_label(*n == *name, format!("{n}   {} {kind}", context_word(*c))).clicked() {
                                        *name = if n == "points" { "P".into() } else { n.clone() };
                                        *context = Some(*c);
                                    }
                                }
                            });
                    });
                }
                f.choice("Context", Some("Where the attribute is read from. Auto: the graph's own context, or Object. Another context is read across: a face reads its points, a corner its point or face, a point the faces or corners around it"),
                         context, &[(None, "Auto"), (Some(Context::Point), "Point"), (Some(Context::Face), "Face"), (Some(Context::Corner), "Corner"), (Some(Context::Object), "Object")]);
                if context.is_some_and(|c| c != Context::Object) {
                    f.choice("Several", Some("When an element touches several of the context read (a face's points, the faces around a point), how they become one value"),
                             reduce, &[(xms_ice::Reduce::Average, "Average"), (xms_ice::Reduce::Sum, "Sum"), (xms_ice::Reduce::Min, "Min"), (xms_ice::Reduce::Max, "Max")]);
                }
            })
        }
        IceGraphNodeType::SetData { name, context, reduce } => group(ui, "Attribute", None, |f| {
            f.text("Name", Some("P is the points, N the normals; any other name makes or sets an attribute"), name, "P");
            f.choice("Context", Some("Where the attribute is written. Auto: the graph's own context. Into another, each element there takes the values of the graph's elements it touches: a face its points, a point the faces around it, the object all of them"),
                     context, &[(None, "Auto"), (Some(Context::Point), "Point"), (Some(Context::Face), "Face"), (Some(Context::Corner), "Corner"), (Some(Context::Object), "Object")]);
            if context.is_some_and(|c| c != runs_over) && runs_over != Context::Object {
                f.choice("Several", Some("When an element written touches several of the graph's (a face's points, the faces around a point), how their values become one"),
                         reduce, &[(xms_ice::Reduce::Average, "Average"), (xms_ice::Reduce::Sum, "Sum"), (xms_ice::Reduce::Min, "Min"), (xms_ice::Reduce::Max, "Max")]);
            }
        }),
        IceGraphNodeType::Constant { value } => group(ui, "Value", None, |f| { value_row(f, "Value", value); }),
        IceGraphNodeType::Math { .. } => {}
    }
    // What each input holds while nothing is wired to it.
    let free: Vec<usize> = node.inputs.iter().enumerate()
        .filter(|(_, i)| i.connected_output.is_none() && i.value.is_some())
        .map(|(k, _)| k).collect();
    if !free.is_empty() {
        group(ui, "Inputs", Some("Used while nothing is wired in"), |f| {
            for k in free {
                let input = &mut node.inputs[k];
                let Some(value) = input.value.as_mut() else { continue };
                let mut kind = value.ty();
                if f.choice(&format!("{} type", input.name), None, &mut kind,
                            &[(xms_ice::Ty::Float, "Scalar"), (xms_ice::Ty::Int, "Integer"), (xms_ice::Ty::Vec3, "3D Vector"), (xms_ice::Ty::Bool, "Boolean")]) {
                    *value = match kind {
                        xms_ice::Ty::Int => Value::Int(0),
                        xms_ice::Ty::Vec3 => value.convert(xms_ice::Ty::Vec3).unwrap_or(Value::Vec3([0.0; 3])),
                        xms_ice::Ty::Bool => Value::Bool(false),
                        _ => value.convert(xms_ice::Ty::Float).unwrap_or(Value::Float(0.0)),
                    };
                }
                value_row(f, &input.name, value);
            }
        });
    }
}

/// A row editing one value, by its type.
fn value_row(f: &mut crate::properties::form::Rows, label: &str, value: &mut xms_ice::Value) {
    use xms_ice::Value;
    match value {
        Value::Bool(b) => { f.toggle(label, None, b); }
        Value::Int(i) => { f.row(label, None, |ui| ui.add(egui::DragValue::new(i).speed(1))); }
        Value::Float(x) => { f.drag(label, None, x, 0.01, ""); }
        Value::Vec2(v) => { f.uv(label, None, v, 0.01); }
        Value::Vec3(v) => { f.xyz(label, None, v, 0.01, ""); }
        Value::Vec4(v) => {
            f.row(label, None, |ui| for c in v.iter_mut() { ui.add(egui::DragValue::new(c).speed(0.01)); });
        }
    }
}

// ── Main canvas ───────────────────────────────────────────────────────────────

/// Draw an ICE tree and handle its editing and navigation, as the main
/// network does: Shift+drag or middle-drag pans, the wheel zooms toward the
/// pointer, F or A frames every node. Returns true when the Up arrow asks to
/// go back up to the scene network.
pub fn draw_subnet_graph(ui: &mut egui::Ui, graph: &mut IceGraph) -> bool {
    let (response, painter) = ui.allocate_painter(
        egui::Vec2::new(ui.available_width(), ui.available_height()),
        egui::Sense::click_and_drag(),
    );
    let canvas_rect = response.rect;
    // The types of the outputs, wired or not, for the ports and wires.
    graph.refresh_types();
    let mut collapsed: HashSet<NodeId> = ui.data(|d| d.get_temp(collapsed_id())).unwrap_or_default();
    let keys_free = response.hovered() && !ui.ctx().egui_wants_keyboard_input();

    // ── Up arrow: back up to the scene network ───────────────────────────────
    if keys_free && ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
        return true;
    }

    // ── Delete or Backspace: delete the selected node ────────────────────────
    // Pointer anywhere over the canvas, nodes included, as in the main network.
    // Not with the pointer over a window above the graph (the file browser).
    let ptr_in_canvas = ui.input(|i| i.pointer.hover_pos())
        .is_some_and(|p| canvas_rect.contains(p) && ui.ctx().layer_id_at(p).is_none_or(|l| l == ui.layer_id()));
    if ptr_in_canvas && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
    {
        if let Some(id) = graph.selected_node { graph.remove_node(id); }
    }

    // ── Frame every node (F or A) ────────────────────────────────────────────
    if keys_free && ui.input(|i| i.key_pressed(egui::Key::F) || i.key_pressed(egui::Key::A))
        && canvas_rect.width() > 40.0 && !graph.nodes.is_empty()
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

    // ── Zoom (wheel, toward the pointer) ─────────────────────────────────────
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let cursor = ui.input(|i| i.pointer.hover_pos()).unwrap_or(canvas_rect.center());
            let (z0, pan0) = (graph.zoom, graph.pan_offset);
            let under = (cursor - canvas_rect.min - pan0) / z0;   // canvas point under the pointer
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
            painter.line_segment([egui::pos2(x, canvas_rect.min.y), egui::pos2(x, canvas_rect.max.y)], grid);
            x += step;
        }
        let mut y = canvas_rect.min.y + pan.y.rem_euclid(step);
        while y < canvas_rect.max.y {
            painter.line_segment([egui::pos2(canvas_rect.min.x, y), egui::pos2(canvas_rect.max.x, y)], grid);
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
            let tp = to_screen(input_socket_pos(tn,   conn.to_input, &collapsed));
            if let Some(ptr) = response.hover_pos() {
                if is_near_bezier_h(ptr, fp, tp, 10.0) { hovered = Some(conn.id); }
            }
            let colour = out_colour(graph, fn_.id, conn.from_output);
            draw_wire_h(&painter, fp, tp, colour, hovered == Some(conn.id), z);
        }
    }
    if let Some(cid) = hovered {
        if response.secondary_clicked() { graph.remove_connection(cid); }
    }

    // In-progress wire — use raw pointer pos, not response.hover_pos()
    if let Some((fid, fo)) = graph.connecting_from {
        if let Some(fn_) = graph.nodes.iter().find(|n| n.id == fid) {
            let fp  = to_screen(output_socket_pos(fn_, fo, &collapsed));
            let ptr = ui.input(|i| i.pointer.hover_pos());
            let colour = out_colour(graph, fid, fo);
            if let Some(ptr) = ptr { draw_wire_h(&painter, fp, ptr, colour, false, z); }
        }
    }

    // Cancel on Escape
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) { graph.connecting_from = None; }

    // Draw all nodes — sockets handle their own press/release via raw input
    let nodes_clone = graph.nodes.clone();
    for node in &nodes_clone {
        draw_subnet_node(ui, &painter, graph, node, &to_screen, &to_canvas, z, &mut collapsed);
    }
    // Where the graph broke, the last time it ran: a red ring on the node,
    // and why, along the top of the canvas.
    let broken = graph.error.lock().ok().and_then(|e| e.clone());
    if let Some((id, message)) = broken {
        if let Some(node) = graph.nodes.iter().find(|n| n.id == id) {
            let size = layout(node, collapsed.contains(&id)).size * z;
            let r = egui::Rect::from_min_size(to_screen(node.position), size).expand(3.0 * z);
            painter.rect_stroke(r, ROUNDING * z, egui::Stroke::new(2.0_f32, pal::ERROR()), egui::StrokeKind::Outside);
        }
        painter.text(canvas_rect.left_top() + egui::vec2(8.0, 8.0), egui::Align2::LEFT_TOP, message,
                     egui::FontId::proportional(12.0), pal::ERROR());
    }
    ui.data_mut(|d| d.insert_temp(collapsed_id(), collapsed));

    // Cancel wire only if mouse released and no socket consumed it
    // (drag_stopped on the background canvas means we missed all sockets)
    if graph.connecting_from.is_some() && response.drag_stopped() {
        graph.connecting_from = None;
    }

    // Pan with Shift+drag or MMB
    let is_mmb = ui.input(|i| i.pointer.middle_down());
    if response.dragged() && (ui.input(|i| i.modifiers.shift) || is_mmb) {
        graph.pan_offset += response.drag_delta();
    }

    // ── Add a node: Tab or right-click on the canvas, as in every graph ─────
    // A menu with a search line: ↓ and ↑ through what is found, Enter adds.
    let menu_id = egui::Id::new(("ice_add", graph.id.0));
    let mut menu_at: Option<egui::Pos2> = ui.data(|d| d.get_temp::<Option<egui::Pos2>>(menu_id)).flatten();
    let mut opened_now = false;
    if response.hovered() && hovered.is_none() && (response.secondary_clicked() || ui.input(|i| i.key_pressed(egui::Key::Tab))) {
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
                    if let Some(t) = add_menu(ui, opened_now) {
                        let name = crate::types::subnet_node_label(&t).to_string();
                        let id = graph.add_node(name, t, to_canvas(at));
                        graph.selected_node = Some(id);
                        close = true;
                    }
                });
            });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) { close = true; }
        if !opened_now && ui.input(|i| i.pointer.any_click()) && !area.response.contains_pointer() && !ui.ctx().any_popup_open() {
            close = true;
        }
        if close { menu_at = None; }
    }
    ui.data_mut(|d| d.insert_temp(menu_id, menu_at));
    false
}

/// Every node the add menu offers: its category, the node as it is made,
/// and a line on what it does.
fn catalog() -> Vec<(&'static str, IceGraphNodeType, &'static str)> {
    use xms_ice::{Math, Value};
    let mut out = vec![
        ("Data Access", IceGraphNodeType::get("P"), "Read an attribute of the geometry: P (the points) or any other by name, in the graph's context or from another (a face reading its points)"),
        ("Data Access", IceGraphNodeType::set("P"), "Write an attribute; its Execute goes into the ICE Tree"),
        ("Constants", IceGraphNodeType::Constant { value: Value::Float(0.0) }, "A number"),
        ("Constants", IceGraphNodeType::Constant { value: Value::Int(0) }, "A whole number"),
        ("Constants", IceGraphNodeType::Constant { value: Value::Vec2([0.0; 2]) }, "Two numbers"),
        ("Constants", IceGraphNodeType::Constant { value: Value::Vec3([0.0; 3]) }, "Three numbers: a position, a direction, a colour"),
        ("Constants", IceGraphNodeType::Constant { value: Value::Vec4([0.0; 4]) }, "Four numbers"),
        ("Constants", IceGraphNodeType::Constant { value: Value::Bool(false) }, "True or false"),
    ];
    for op in Math::ALL {
        out.push(("Math", IceGraphNodeType::Math { op }, ""));
    }
    out
}

/// The add menu: a search line, then the nodes by category. The node
/// chosen, if one was.
fn add_menu(ui: &mut egui::Ui, opened_now: bool) -> Option<IceGraphNodeType> {
    let search_id = egui::Id::new("ice_add_search");
    let mut search: String = if opened_now { String::new() } else { ui.data(|d| d.get_temp(search_id)).unwrap_or_default() };
    let field = ui.add(egui::TextEdit::singleline(&mut search).hint_text("Search nodes").desired_width(180.0));
    if opened_now { field.request_focus(); }
    ui.data_mut(|d| d.insert_temp(search_id, search.clone()));
    ui.separator();
    let label = |t: &IceGraphNodeType| format!("{}  {}", crate::types::subnet_node_icon(t), crate::types::subnet_node_label(t));
    let words = search.trim().to_lowercase();
    let mut chosen = None;
    if !words.is_empty() {
        let found: Vec<(&'static str, IceGraphNodeType, &'static str)> = catalog().into_iter()
            .filter(|(category, t, hint)| {
                let text = format!("{} {category} {hint}", crate::types::subnet_node_label(t)).to_lowercase();
                words.split_whitespace().all(|w| text.contains(w))
            })
            .collect();
        let (hi, moved, enter) = crate::menus::search_keys(ui, egui::Id::new("ice_add_pick"), &words, found.len(), opened_now);
        for (k, (category, t, hint)) in found.iter().enumerate() {
            let mut r = ui.selectable_label(k == hi, format!("{}   {category}", label(t)));
            if !hint.is_empty() { r = r.on_hover_text(*hint); }
            if moved && k == hi { r.scroll_to_me(None); }
            if r.clicked() { chosen = Some(t.clone()); }
        }
        if found.is_empty() { ui.label("No node by that name"); }
        if enter { chosen = found.get(hi).map(|(_, t, _)| t.clone()); }
        return chosen;
    }
    // Categories, each a submenu, as in the scene graph's menu.
    let mut categories: Vec<&'static str> = vec![];
    for (c, _, _) in catalog() { if !categories.contains(&c) { categories.push(c); } }
    for category in categories {
        crate::menus::menu(ui, category, |ui| {
            for (c, t, hint) in catalog() {
                if c != category { continue; }
                let mut r = ui.button(label(&t));
                if !hint.is_empty() { r = r.on_hover_text(hint); }
                if r.clicked() { chosen = Some(t); ui.close(); }
            }
        });
    }
    chosen
}

/// A type's colour on ports and wires; Execute and what is not known yet
/// are dark grey.
fn ty_colour(t: Option<xms_ice::Ty>) -> egui::Color32 {
    use crate::type_colours::DataType;
    use xms_ice::Ty;
    match t {
        Some(Ty::Float) => DataType::Float,
        Some(Ty::Int) => DataType::Integer,
        Some(Ty::Bool) => DataType::Boolean,
        Some(Ty::Vec2 | Ty::Vec3) => DataType::Vector,
        // Four numbers: a quaternion, a rotation.
        Some(Ty::Vec4) => DataType::Vector4,
        None => DataType::Other,
    }.colour()
}

/// An output's colour: the type it gives, as the graph's last run found it.
fn out_colour(graph: &IceGraph, node: NodeId, output: usize) -> egui::Color32 {
    ty_colour(graph.output_type(node, output))
}

/// An input's colour: what is wired into it, or the value it holds.
fn in_colour(graph: &IceGraph, input: &super::IceGraphInputSocket) -> egui::Color32 {
    if input.value_hint == "Execute" { return pal::OTHER(); }
    match input.connected_output {
        Some((from, out)) => out_colour(graph, from, out),
        None => ty_colour(input.value.map(|v| v.ty())),
    }
}

// ── Single node ───────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn draw_subnet_node(
    ui:          &mut egui::Ui,
    painter:     &egui::Painter,
    graph:       &mut IceGraph,
    node:        &IceGraphNode,
    to_screen:   &impl Fn(egui::Pos2) -> egui::Pos2,
    to_canvas:   &impl Fn(egui::Pos2) -> egui::Pos2,
    z:           f32,
    collapsed:   &mut HashSet<NodeId>,
) {
    let id      = node.id;
    let folded  = collapsed.contains(&id);
    let np      = to_screen(node.position);
    let lay     = layout(node, folded);
    let rect    = egui::Rect::from_min_size(np, lay.size * z);
    let font    = |pt: f32| egui::FontId::proportional(pt * z);
    let is_sel  = graph.selected_node == Some(id);
    let body    = tint(&node.node_type);

    // A grey ring, the body, a dark edge; the selection blue around it when
    // selected, wide enough to see at any zoom.
    let w = crate::node_graph::ui::ring_width(z, is_sel);
    let ring = rect.expand(w);
    painter.rect_filled(ring, if ROUNDING > 0.0 { ROUNDING * z + w } else { 0.0 }, if is_sel { pal::SELECTED() } else { pal::FRAME() });
    painter.rect_filled(rect, ROUNDING * z, body);
    painter.rect_stroke(rect, ROUNDING * z, egui::Stroke::new(1.0_f32, pal::RIM()), egui::StrokeKind::Middle);

    // Two-line title: the node's own name in bold (drawn twice, a hair
    // apart), its type in small dim text beneath. Collapsed, only the name.
    let name_y = if folded { np.y + FOLDED_H * 0.5 * z } else { np.y + 12.0 * z };
    for dx in [0.0, 0.6] {
        painter.text(egui::pos2(np.x + (7.0 + dx) * z, name_y), egui::Align2::LEFT_CENTER, &node.name,
            font(TITLE_PT), pal::INK());
    }
    if !folded {
        painter.text(egui::pos2(np.x + 7.0 * z, np.y + 25.0 * z), egui::Align2::LEFT_CENTER, title_text(node),
            font(TYPE_PT), pal::INK_TYPE());
        // A thin line between the title and the ports, short of the edges.
        let y = np.y + TITLE_H * z - 0.5;
        painter.line_segment([egui::pos2(rect.min.x + 4.0 * z, y), egui::pos2(rect.max.x - 4.0 * z, y)],
            egui::Stroke::new(1.0_f32, pal::DIVIDER()));
    }

    // A constant's value, under the title.
    if let (false, Some(v)) = (folded, value_text(node)) {
        painter.text(egui::pos2(np.x + 7.0 * z, np.y + (TITLE_H + ROW_H * 0.5) * z), egui::Align2::LEFT_CENTER, v,
            egui::FontId::monospace(LABEL_PT * z), pal::INK());
    }

    // The collapse box, at the right of the title.
    let fold_rect = egui::Rect::from_center_size(egui::pos2(rect.max.x - 11.0 * z, np.y + (if folded { FOLDED_H * 0.5 } else { 12.0 }) * z), egui::vec2(11.0, 9.0) * z);
    draw_fold_box(painter, fold_rect);

    // ── IMPORTANT: title bar allocated FIRST → lowest hit-test priority ───────
    let title_rect = egui::Rect::from_min_size(np, egui::vec2(lay.size.x, if folded { FOLDED_H } else { TITLE_H }) * z);
    let dr = ui.allocate_rect(title_rect, egui::Sense::click_and_drag());
    let fold = ui.allocate_rect(fold_rect.expand(2.0 * z), egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if folded { "Expand" } else { "Collapse" });
    if fold.clicked() {
        if !collapsed.remove(&id) { collapsed.insert(id); }
    }

    // ── Output ports (right edge) — allocated before inputs, highest priority
    for (i, out) in node.outputs.iter().enumerate() {
        let sp  = np + lay.outputs[i] * z;
        let hit = egui::Rect::from_center_size(sp, egui::Vec2::splat((SOCK_HIT * z).max(10.0)));
        let sr  = ui.allocate_rect(hit, egui::Sense::click_and_drag());
        let wiring = graph.connecting_from == Some((id, i));
        draw_port(painter, sp, out_colour(graph, id, i), sr.hovered() || wiring, z);
        if !folded {
            painter.text(egui::pos2(sp.x - 7.0 * z, sp.y), egui::Align2::RIGHT_CENTER, &out.name,
                font(LABEL_PT), pal::INK());
        }

        // Fire on the very first frame of press
        if sr.drag_started() || (sr.is_pointer_button_down_on() && graph.connecting_from.is_none()) {
            graph.connecting_from = Some((id, i));
        }
    }

    // ── Input ports (left edge) ───────────────────────────────────────────────
    for (i, inp) in node.inputs.iter().enumerate() {
        let sp  = np + lay.inputs[i] * z;
        let hit = egui::Rect::from_center_size(sp, egui::Vec2::splat((SOCK_HIT * z).max(10.0)));
        let sr  = ui.allocate_rect(hit, egui::Sense::drag());
        // Hovered while a wire is being drawn: a target.
        let target = sr.hovered() && graph.connecting_from.is_some();
        draw_port(painter, sp, in_colour(graph, inp), target, z);
        if !folded {
            painter.text(egui::pos2(sp.x + 7.0 * z, sp.y), egui::Align2::LEFT_CENTER, &inp.name,
                font(LABEL_PT), pal::INK());
        }

        // Complete wire: mouse released while this socket is hovered.
        // Use raw input so the canvas response can't steal the release.
        if sr.hovered() && ui.input(|s| s.pointer.primary_released()) {
            if let Some((fn_, fo)) = graph.connecting_from {
                if fn_ != id { graph.add_connection(fn_, fo, id, i); }
                graph.connecting_from = None;
            }
        }
    }

    // ── Title bar interactions (lowest priority — allocated first) ────────────
    if dr.clicked() { graph.selected_node = Some(id); }
    let terminal = matches!(node.node_type, IceGraphNodeType::Root { .. });
    if !terminal {
        dr.context_menu(|ui| {
            if ui.button("Delete").clicked() {
                graph.remove_node(id);
                ui.close();
            }
        });
    }
    // Only drag the node if we are not mid-wire
    if dr.drag_started() && graph.connecting_from.is_none() {
        graph.dragging_node = Some(id);
        graph.drag_offset   = dr.interact_pointer_pos().unwrap_or_default() - np;
    }
    if dr.dragged() && graph.dragging_node == Some(id) {
        if let Some(ptr) = dr.interact_pointer_pos() {
            let new_pos = to_canvas(ptr - graph.drag_offset);
            if let Some(n) = graph.nodes.iter_mut().find(|n| n.id == id) { n.position = new_pos; }
        }
    }
    if dr.drag_stopped() { graph.dragging_node = None; }
}

/// A port: a small dot in its type's colour on the node's edge, larger
/// under the pointer.
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
        painter.line_segment([egui::pos2(r.min.x + 2.0, y), egui::pos2(r.max.x - 2.0, y)], egui::Stroke::new(1.0_f32, pal::RIM()));
    }
}

// ── Wire drawing ──────────────────────────────────────────────────────────────

/// A wire from an output to an input: a thin curve in the colour of the data
/// it carries.
pub fn draw_wire_h(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, colour: egui::Color32, hovered: bool, z: f32) {
    let off = (to.x - from.x).abs().max(60.0 * z) * 0.5;
    let c1  = egui::pos2(from.x + off, from.y);
    let c2  = egui::pos2(to.x   - off, to.y);
    let pts: Vec<egui::Pos2> = (0..=24)
        .map(|i| bezier(from, c1, c2, to, i as f32 / 24.0))
        .collect();
    painter.add(egui::Shape::line(pts, egui::Stroke::new(
        (if hovered { WIRE_W + 1.4 } else { WIRE_W }) * z.clamp(0.7, 1.5),
        if hovered { egui::Color32::WHITE } else { colour })));
}

pub(crate) fn is_near_bezier_h(p: egui::Pos2, from: egui::Pos2, to: egui::Pos2, thresh: f32) -> bool {
    let off = (to.x - from.x).abs().max(60.0) * 0.5;
    let c1  = egui::pos2(from.x + off, from.y);
    let c2  = egui::pos2(to.x   - off, to.y);
    (0..=20).any(|i| {
        let q = bezier(from, c1, c2, to, i as f32 / 20.0);
        ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt() < thresh
    })
}

fn bezier(p0: egui::Pos2, p1: egui::Pos2, p2: egui::Pos2, p3: egui::Pos2, t: f32) -> egui::Pos2 {
    let mt = 1.0 - t;
    egui::pos2(
        mt*mt*mt*p0.x + 3.0*mt*mt*t*p1.x + 3.0*mt*t*t*p2.x + t*t*t*p3.x,
        mt*mt*mt*p0.y + 3.0*mt*mt*t*p1.y + 3.0*mt*t*t*p2.y + t*t*t*p3.y,
    )
}