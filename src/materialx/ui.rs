//! The MaterialX graph in the Node Graph pane: its nodes, wires and editing.
//!
//! Drawn as the ICE graph is (Softimage's look: solid nodes coloured by
//! kind, a two-line title, a row per port, outputs first, small dots
//! coloured by data type and wires in the same colour, a light grey
//! canvas), and navigated the same way: Shift+drag or middle-drag pans,
//! the wheel zooms, F or A frames, ↑ goes back to the scene graph.
//!
//! A wire goes from an output to an input. Dropped on an input, it is
//! connected if the types join (the nodes become the definitions that take
//! them, see `xms_matx::Graph::connect`), else the canvas says why. Dropped
//! on a node's body, it offers the inputs that take it. A wire that a later
//! change made wrong is drawn red. Nodes with many inputs (the surface
//! shaders) show the connected ones only, until unfolded.

use xms_matx::{Graph, NodeDef, Type};

use super::{lib, View};
use crate::ice::ui::{draw_fold_box, draw_port, draw_wire_h, is_near_bezier_h};
use crate::types::NodeId;

const TITLE_H: f32 = 32.0;
const FOLDED_H: f32 = 20.0;
const ROW_H: f32 = 15.0;
const PAD_B: f32 = 4.0;
const MIN_W: f32 = 120.0;
const TITLE_CH: f32 = 7.4;
const LABEL_CH: f32 = 5.9;
const ICONS_W: f32 = 20.0;
const SOCK_HIT: f32 = 16.0;
const ROUNDING: f32 = 5.0;
const TYPE_PT: f32 = 9.5;
const TITLE_PT: f32 = 12.5;
const LABEL_PT: f32 = 10.5;
const GRID: f32 = 12.0;
/// More inputs than this: the node shows its connected ones only, until
/// unfolded.
const FEW: usize = 8;

mod pal {
    #![allow(non_snake_case)]
    use egui::Color32;
    pub fn BG() -> Color32 { Color32::from_rgb(170, 170, 170) }
    pub fn GRID() -> Color32 { Color32::from_rgb(161, 161, 161) }
    pub fn INK() -> Color32 { Color32::from_rgb(12, 12, 12) }
    pub fn INK_TYPE() -> Color32 { Color32::from_rgba_unmultiplied(12, 12, 12, 150) }
    pub fn DIVIDER() -> Color32 { Color32::from_black_alpha(80) }
    pub fn RIM() -> Color32 { Color32::from_rgb(38, 38, 38) }
    pub fn FRAME() -> Color32 { Color32::from_rgb(105, 105, 105) }
    pub fn WRONG() -> Color32 { Color32::from_rgb(255, 30, 30) }
    pub fn MESSAGE() -> Color32 { Color32::from_rgb(120, 10, 10) }
}

/// A node's colour, by its group in the library.
fn tint(group: &str) -> egui::Color32 {
    let (r, g, b) = match group {
        // Shaders and the material: the purple of material sockets.
        "pbr" | "shader" | "light" | "npr" => (176, 146, 214),
        "material" => (150, 112, 196),
        "texture2d" | "texture3d" => (102, 150, 204),
        "procedural" | "procedural2d" | "procedural3d" => (214, 170, 104),
        "geometric" | "application" => (112, 186, 154),
        "math" | "adjustment" | "channel" | "conditional" | "compositing" | "convolution2d" => (152, 196, 98),
        _ => (178, 178, 170),
    };
    egui::Color32::from_rgb(r, g, b)
}

/// A data type's colour, on its ports and wires.
pub fn type_colour(t: Type) -> egui::Color32 {
    let (r, g, b) = match t {
        Type::Float | Type::FloatArray => (70, 230, 70),
        Type::Integer | Type::Boolean | Type::IntegerArray => (20, 120, 40),
        Type::Color3 | Type::Color4 | Type::Color3Array | Type::Color4Array => (240, 130, 60),
        Type::Vector2 | Type::Vector3 | Type::Vector4 | Type::Vector2Array | Type::Vector3Array | Type::Vector4Array => (235, 225, 60),
        Type::Matrix33 | Type::Matrix44 => (140, 150, 40),
        Type::String | Type::Filename | Type::Geomname | Type::StringArray | Type::GeomnameArray => (200, 210, 225),
        Type::Bsdf | Type::Edf | Type::Vdf => (225, 120, 205),
        Type::SurfaceShader | Type::DisplacementShader | Type::VolumeShader | Type::LightShader | Type::Material => (140, 95, 190),
        Type::None => (85, 85, 85),
    };
    egui::Color32::from_rgb(r, g, b)
}

/// Where everything of a node sits, relative to its top-left corner.
struct Layout {
    size: egui::Vec2,
    /// Each output: its name, type and place.
    outputs: Vec<(String, Type, egui::Vec2)>,
    /// Each input shown: its name, type and place.
    inputs: Vec<(String, Type, egui::Vec2)>,
    folded: bool,
    /// Inputs not shown.
    hidden: usize,
}

/// Whether a node shows every input: a few, unless toggled; many, if toggled.
fn shows_all(def: &NodeDef, id: u32, view: &View) -> bool {
    let many = def.inputs.iter().filter(|i| i.ui.visible).count() > FEW;
    many == view.toggled.contains(&id)
}

fn layout(graph: &Graph, id: u32, view: &View) -> Layout {
    let lib = lib();
    let (Some(node), Some(def)) = (graph.node(id), graph.def(lib, id)) else {
        return Layout { size: egui::vec2(MIN_W, FOLDED_H), outputs: vec![], inputs: vec![], folded: true, hidden: 0 };
    };
    let all = shows_all(def, id, view);
    let shown: Vec<(String, Type)> = def.inputs.iter()
        .filter(|i| node.binding(&i.name).is_some_and(|b| matches!(b, xms_matx::Binding::Connect { .. })) || (all && i.ui.visible))
        .map(|i| (i.name.clone(), i.ty))
        .collect();
    let hidden = def.inputs.iter().filter(|i| i.ui.visible).count().saturating_sub(shown.len());
    let title = (node.name.chars().count() as f32 * TITLE_CH).max(subtitle(node, def).chars().count() as f32 * LABEL_CH) + ICONS_W + 18.0;
    let label = |n: &str| n.chars().count() as f32 * LABEL_CH + 22.0;
    let widest = def.outputs.iter().map(|o| label(&o.name)).chain(shown.iter().map(|(n, _)| label(n))).fold(0.0, f32::max);
    let w = title.max(widest).max(MIN_W).round();
    let outs = def.outputs.len();
    // A line under the inputs says how many are not shown.
    let more = (hidden > 0) as usize;
    let row_y = |r: usize| TITLE_H + (r as f32 + 0.5) * ROW_H;
    Layout {
        size: egui::vec2(w, TITLE_H + (outs + shown.len() + more) as f32 * ROW_H + PAD_B),
        outputs: def.outputs.iter().enumerate().map(|(r, o)| (o.name.clone(), o.ty, egui::vec2(w, row_y(r)))).collect(),
        inputs: shown.into_iter().enumerate().map(|(r, (n, t))| (n, t, egui::vec2(0.0, row_y(outs + r)))).collect(),
        folded: !all,
        hidden,
    }
}

/// The type line under a node's name: its node and output type.
fn subtitle(node: &xms_matx::Node, def: &NodeDef) -> String {
    let ty = if def.outputs.len() > 1 { "multioutput".to_string() } else { def.out_type().name().to_string() };
    format!("{}  {ty}", node.node)
}

/// Where an input's port is drawn: its row, or, for an input not shown,
/// the node's left edge under the title.
fn input_pos(graph: &Graph, id: u32, input: &str, view: &View) -> Option<egui::Pos2> {
    let node = graph.node(id)?;
    let lay = layout(graph, id, view);
    let at = lay.inputs.iter().find(|(n, _, _)| n == input).map(|(_, _, p)| *p).unwrap_or(egui::vec2(0.0, TITLE_H * 0.5));
    Some(egui::pos2(node.pos[0], node.pos[1]) + at)
}

fn output_pos(graph: &Graph, id: u32, output: &str, view: &View) -> Option<egui::Pos2> {
    let node = graph.node(id)?;
    let lay = layout(graph, id, view);
    let at = lay.outputs.iter().find(|(n, _, _)| n == output).map(|(_, _, p)| *p)?;
    Some(egui::pos2(node.pos[0], node.pos[1]) + at)
}

/// Draw the graph of the MaterialX operator `op` and handle its editing.
/// Returns true when ↑ asks to go back up to the scene graph.
pub fn draw(ui: &mut egui::Ui, op: NodeId, graph: &mut Graph) -> bool {
    let lib = lib();
    let mut view = super::view(ui.ctx(), op);
    let (response, painter) = ui.allocate_painter(egui::vec2(ui.available_width(), ui.available_height()), egui::Sense::click_and_drag());
    let canvas = response.rect;
    let keys_free = response.hovered() && !ui.ctx().egui_wants_keyboard_input();
    let ptr_in_canvas = ui.input(|i| i.pointer.hover_pos())
        .is_some_and(|p| canvas.contains(p) && ui.ctx().layer_id_at(p).is_none_or(|l| l == ui.layer_id()));

    if keys_free && ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
        super::set_view(ui.ctx(), op, view);
        return true;
    }
    // Delete or Backspace: the selected node. Not while text is typed.
    if ptr_in_canvas && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
    {
        if let Some(id) = view.selected.take() { graph.remove(id); }
    }
    // Frame every node: F or A, and once when the graph is first opened.
    let frame = !view.framed || keys_free && ui.input(|i| i.key_pressed(egui::Key::F) || i.key_pressed(egui::Key::A));
    if frame && canvas.width() > 40.0 && !graph.nodes.is_empty() {
        let mut bounds = egui::Rect::NOTHING;
        for n in &graph.nodes {
            bounds = bounds.union(egui::Rect::from_min_size(egui::pos2(n.pos[0], n.pos[1]), layout(graph, n.id, &view).size));
        }
        let room = canvas.size() - egui::vec2(60.0, 60.0);
        view.zoom = (room.x / bounds.width()).min(room.y / bounds.height()).clamp(0.3, 1.25);
        view.pan = canvas.size() * 0.5 - bounds.center().to_vec2() * view.zoom;
        view.framed = true;
        ui.ctx().request_repaint();
    }
    // Zoom toward the pointer.
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let cursor = ui.input(|i| i.pointer.hover_pos()).unwrap_or(canvas.center());
            let under = (cursor - canvas.min - view.pan) / view.zoom;
            view.zoom = (view.zoom * (1.0 + scroll * 0.002)).clamp(0.15, 4.0);
            view.pan = cursor - canvas.min - under * view.zoom;
        }
    }
    let (pan, z) = (view.pan, view.zoom);
    let to_screen = move |p: egui::Pos2| canvas.min + p.to_vec2() * z + pan;
    let to_canvas = move |p: egui::Pos2| egui::Pos2::ZERO + (p - canvas.min - pan) / z;

    painter.rect_filled(canvas, 0.0, pal::BG());
    let step = GRID * z;
    if step >= 5.0 {
        let stroke = egui::Stroke::new(1.0_f32, pal::GRID());
        let mut x = canvas.min.x + pan.x.rem_euclid(step);
        while x < canvas.max.x { painter.line_segment([egui::pos2(x, canvas.min.y), egui::pos2(x, canvas.max.y)], stroke); x += step; }
        let mut y = canvas.min.y + pan.y.rem_euclid(step);
        while y < canvas.max.y { painter.line_segment([egui::pos2(canvas.min.x, y), egui::pos2(canvas.max.x, y)], stroke); y += step; }
    }

    // ── Wires ────────────────────────────────────────────────────────────────
    let mut hovered_wire: Option<(u32, String)> = None;
    for n in &graph.nodes {
        for (input, b) in &n.inputs {
            let xms_matx::Binding::Connect { node: src, output } = b else { continue };
            let (Some(fp), Some(tp)) = (output_pos(graph, *src, output, &view), input_pos(graph, n.id, input, &view)) else { continue };
            let (fp, tp) = (to_screen(fp), to_screen(tp));
            if response.hover_pos().is_some_and(|p| is_near_bezier_h(p, fp, tp, 10.0)) { hovered_wire = Some((n.id, input.clone())); }
            let ok = graph.wire_ok(lib, n.id, input);
            let colour = if ok { graph.output_type(lib, *src, output).map(type_colour).unwrap_or(pal::RIM()) } else { pal::WRONG() };
            draw_wire_h(&painter, fp, tp, colour, hovered_wire.as_ref() == Some(&(n.id, input.clone())), z);
        }
    }
    // Right-click a wire: take it off.
    if let Some((to, input)) = &hovered_wire {
        if response.secondary_clicked() { graph.disconnect(*to, input); }
    }
    // The wire being drawn.
    if let Some((from, output)) = view.wiring.clone() {
        if let (Some(fp), Some(ptr)) = (output_pos(graph, from, &output, &view), ui.input(|i| i.pointer.hover_pos())) {
            let colour = graph.output_type(lib, from, &output).map(type_colour).unwrap_or(pal::RIM());
            draw_wire_h(&painter, to_screen(fp), ptr, colour, false, z);
        }
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) { view.wiring = None; view.drop = None; }

    // ── Nodes ────────────────────────────────────────────────────────────────
    let ids: Vec<u32> = graph.nodes.iter().map(|n| n.id).collect();
    for id in ids {
        draw_node(ui, &painter, graph, id, &mut view, &to_screen, &to_canvas);
    }
    // A wire let go of over nothing: dropped.
    if view.wiring.is_some() && ui.input(|i| i.pointer.primary_released()) { view.wiring = None; }

    // Pan with Shift+drag or the middle button.
    if response.dragged() && (ui.input(|i| i.modifiers.shift) || ui.input(|i| i.pointer.middle_down())) {
        view.pan += response.drag_delta();
    }
    // A click on the canvas: nothing selected.
    if response.clicked() { view.selected = None; }

    // ── The inputs a dropped wire can go into ────────────────────────────────
    if let Some(drop) = view.drop.clone() {
        let offered = graph.inputs_taking(lib, drop.from.0, &drop.from.1, drop.to);
        let mut close = false;
        let area = egui::Area::new(egui::Id::new(("materialx_drop", op.0)))
            .fixed_pos(drop.at)
            .order(egui::Order::Foreground)
            .constrain(true)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(160.0);
                    let name = graph.node(drop.to).map(|n| n.name.clone()).unwrap_or_default();
                    ui.label(egui::RichText::new(format!("Into {name}")).small());
                    ui.separator();
                    if offered.is_empty() { ui.label("No input takes it"); }
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for input in &offered {
                            let ty = graph.def(lib, drop.to).and_then(|d| d.input(input)).map(|i| i.ty.name()).unwrap_or("");
                            if ui.button(format!("{input}  {ty}")).clicked() {
                                view.message = graph.connect(lib, drop.from.0, &drop.from.1, drop.to, input).err().unwrap_or_default();
                                close = true;
                            }
                        }
                    });
                });
            });
        if ui.input(|i| i.pointer.any_click()) && !area.response.contains_pointer() && !ui.ctx().any_popup_open() {
            close = true;
        }
        if close { view.drop = None; }
    }

    // ── Add a node: Tab or right-click on the canvas ─────────────────────────
    let menu_id = egui::Id::new(("materialx_add", op.0));
    let mut menu_at: Option<egui::Pos2> = ui.data(|d| d.get_temp::<Option<egui::Pos2>>(menu_id)).flatten();
    let mut opened_now = false;
    if response.hovered() && hovered_wire.is_none() && (response.secondary_clicked() || ui.input(|i| i.key_pressed(egui::Key::Tab))) {
        menu_at = ui.input(|i| i.pointer.hover_pos());
        opened_now = true;
        ui.data_mut(|d| d.insert_temp(egui::Id::new("materialx_search"), String::new()));
    }
    if let Some(at) = menu_at {
        let mut close = false;
        let area = egui::Area::new(menu_id.with("area"))
            .fixed_pos(at)
            .order(egui::Order::Foreground)
            .constrain(true)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(200.0);
                    if let Some(node) = add_menu(ui, opened_now) {
                        let p = to_canvas(at);
                        if let Some(id) = graph.add(lib, &node, [p.x, p.y]) {
                            view.selected = Some(id);
                        }
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

    // Why the last wire was refused.
    if !view.message.is_empty() {
        painter.text(canvas.left_bottom() + egui::vec2(8.0, -8.0), egui::Align2::LEFT_BOTTOM, &view.message,
            egui::FontId::proportional(12.0), pal::MESSAGE());
    }
    // A node removed elsewhere (undo): nothing selected.
    if view.selected.is_some_and(|s| graph.node(s).is_none()) { view.selected = None; }
    super::set_view(ui.ctx(), op, view);
    false
}

/// The add menu: a search line, then the library's nodes by group. The
/// node chosen, if one was.
fn add_menu(ui: &mut egui::Ui, opened_now: bool) -> Option<String> {
    let lib = lib();
    let search_id = egui::Id::new("materialx_search");
    let mut search: String = ui.data(|d| d.get_temp(search_id)).unwrap_or_default();
    let field = ui.add(egui::TextEdit::singleline(&mut search).hint_text("Search nodes").desired_width(190.0));
    if opened_now { field.request_focus(); }
    ui.data_mut(|d| d.insert_temp(search_id, search.clone()));
    ui.separator();
    let mut chosen = None;
    let label = |node: &str| -> String {
        match lib.default_def(node).and_then(|d| d.ui_name.clone()) {
            Some(ui_name) => format!("{node}  ({ui_name})"),
            None => node.to_string(),
        }
    };
    let s = search.trim().to_lowercase();
    if !s.is_empty() {
        let found: Vec<&String> = lib.nodes().iter().filter(|n| n.to_lowercase().contains(&s)).collect();
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for node in found.iter().take(60) {
                let group = lib.group(node).unwrap_or("");
                if ui.button(format!("{}   {group}", label(node.as_str()))).clicked() { chosen = Some(node.to_string()); }
            }
            if found.is_empty() { ui.label("No node by that name"); }
        });
        // Enter takes the first.
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) { chosen = found.first().map(|n| n.to_string()); }
        return chosen;
    }
    // The surface shaders and the material first: what a material needs.
    for node in ["standard_surface", "open_pbr_surface", "UsdPreviewSurface", "gltf_pbr", "surfacematerial", "image", "tiledimage"] {
        if lib.default_def(node).is_some() && ui.button(label(node)).clicked() { chosen = Some(node.to_string()); }
    }
    ui.separator();
    for group in groups() {
        ui.menu_button(group_label(group), |ui| {
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                for node in lib.nodes().iter().filter(|n| lib.group(n) == Some(group)) {
                    if ui.button(label(node.as_str())).clicked() { chosen = Some(node.clone()); ui.close(); }
                }
            });
        });
    }
    chosen
}

/// The library's node groups, in the order a material is built.
fn groups() -> Vec<&'static str> {
    const ORDER: [&str; 21] = [
        "pbr", "material", "texture2d", "texture3d", "procedural2d", "procedural3d", "procedural", "geometric",
        "math", "adjustment", "channel", "conditional", "compositing", "convolution2d", "colortransform",
        "organization", "translation", "shader", "light", "npr", "application",
    ];
    let lib = lib();
    let mut out: Vec<&'static str> = ORDER.iter().copied().filter(|g| lib.nodes().iter().any(|n| lib.group(n) == Some(*g))).collect();
    // A group a newer library adds, at the end.
    for n in lib.nodes() {
        if let Some(g) = lib.group(n) {
            if !g.is_empty() && !out.contains(&g) { out.push(g); }
        }
    }
    out
}

fn group_label(group: &str) -> String {
    match group {
        "pbr" => "Shading (pbr)".into(),
        "texture2d" => "Texture 2D".into(),
        "texture3d" => "Texture 3D".into(),
        "procedural2d" => "Procedural 2D".into(),
        "procedural3d" => "Procedural 3D".into(),
        "convolution2d" => "Convolution 2D".into(),
        "colortransform" => "Colour transform".into(),
        "npr" => "NPR".into(),
        g => {
            let mut c = g.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        }
    }
}

fn draw_node(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    graph: &mut Graph,
    id: u32,
    view: &mut View,
    to_screen: &impl Fn(egui::Pos2) -> egui::Pos2,
    to_canvas: &impl Fn(egui::Pos2) -> egui::Pos2,
) {
    let lib = lib();
    let z = view.zoom;
    let lay = layout(graph, id, view);
    let Some(node) = graph.node(id).cloned() else { return };
    let def = graph.def(lib, id);
    let np = to_screen(egui::pos2(node.pos[0], node.pos[1]));
    let rect = egui::Rect::from_min_size(np, lay.size * z);
    let font = |pt: f32| egui::FontId::proportional(pt * z);
    let selected = view.selected == Some(id);
    let body = tint(def.map(|d| d.group.as_str()).unwrap_or(""));

    let w = crate::node_graph::ui::ring_width(z, selected);
    painter.rect_filled(rect.expand(w), ROUNDING * z + w, if selected { crate::theme::selection() } else { pal::FRAME() });
    painter.rect_filled(rect, ROUNDING * z, body);
    painter.rect_stroke(rect, ROUNDING * z, egui::Stroke::new(1.0_f32, pal::RIM()), egui::StrokeKind::Middle);

    for dx in [0.0, 0.6] {
        painter.text(egui::pos2(np.x + (7.0 + dx) * z, np.y + 12.0 * z), egui::Align2::LEFT_CENTER, &node.name, font(TITLE_PT), pal::INK());
    }
    if let Some(d) = def {
        painter.text(egui::pos2(np.x + 7.0 * z, np.y + 25.0 * z), egui::Align2::LEFT_CENTER, subtitle(&node, d), font(TYPE_PT), pal::INK_TYPE());
    } else {
        painter.text(egui::pos2(np.x + 7.0 * z, np.y + 25.0 * z), egui::Align2::LEFT_CENTER, format!("{} not in the library", node.def),
            font(TYPE_PT), pal::WRONG());
    }
    let y = np.y + TITLE_H * z - 0.5;
    painter.line_segment([egui::pos2(rect.min.x + 4.0 * z, y), egui::pos2(rect.max.x - 4.0 * z, y)], egui::Stroke::new(1.0_f32, pal::DIVIDER()));

    // The fold box: every input, or the connected ones.
    let fold_rect = egui::Rect::from_center_size(egui::pos2(rect.max.x - 11.0 * z, np.y + 12.0 * z), egui::vec2(11.0, 9.0) * z);
    draw_fold_box(painter, fold_rect);

    // Title first: the lowest priority for the pointer.
    let title_rect = egui::Rect::from_min_size(np, egui::vec2(lay.size.x, TITLE_H) * z);
    let dr = ui.allocate_rect(title_rect, egui::Sense::click_and_drag());
    // The rows below the title: a wire dropped here offers the inputs.
    let rows_rect = egui::Rect::from_min_max(egui::pos2(rect.min.x + SOCK_HIT * 0.5 * z, title_rect.max.y), rect.max);
    let rows = ui.allocate_rect(rows_rect, egui::Sense::click());
    let fold = ui.allocate_rect(fold_rect.expand(2.0 * z), egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if lay.folded { "Show every input" } else { "Show the connected inputs only" });
    if fold.clicked() {
        if let Some(k) = view.toggled.iter().position(|t| *t == id) { view.toggled.remove(k); } else { view.toggled.push(id); }
    }
    if lay.hidden > 0 {
        let y = rect.max.y - (PAD_B + ROW_H * 0.5) * z;
        painter.text(egui::pos2(np.x + 9.0 * z, y), egui::Align2::LEFT_CENTER, format!("{} more", lay.hidden), font(LABEL_PT), pal::INK_TYPE());
    }

    // Outputs, on the right.
    for (name, ty, at) in &lay.outputs {
        let sp = np + *at * z;
        let sr = ui.allocate_rect(egui::Rect::from_center_size(sp, egui::Vec2::splat((SOCK_HIT * z).max(10.0))), egui::Sense::click_and_drag());
        let wiring = view.wiring.as_ref() == Some(&(id, name.clone()));
        draw_port(painter, sp, type_colour(*ty), sr.hovered() || wiring, z);
        painter.text(egui::pos2(sp.x - 7.0 * z, sp.y), egui::Align2::RIGHT_CENTER, name, font(LABEL_PT), pal::INK());
        if sr.drag_started() || (sr.is_pointer_button_down_on() && view.wiring.is_none()) {
            view.wiring = Some((id, name.clone()));
            view.message.clear();
        }
    }
    // Inputs, on the left.
    for (name, ty, at) in &lay.inputs {
        let sp = np + *at * z;
        let sr = ui.allocate_rect(egui::Rect::from_center_size(sp, egui::Vec2::splat((SOCK_HIT * z).max(10.0))), egui::Sense::drag());
        // While a wire is drawn the pointer is over the port, though the
        // wire's drag holds the hover.
        let target = sr.contains_pointer() && view.wiring.is_some();
        draw_port(painter, sp, type_colour(*ty), target, z);
        painter.text(egui::pos2(sp.x + 7.0 * z, sp.y), egui::Align2::LEFT_CENTER, name, font(LABEL_PT), pal::INK());
        if sr.contains_pointer() && ui.input(|i| i.pointer.primary_released()) {
            if let Some((from, output)) = view.wiring.take() {
                if from != id {
                    view.message = graph.connect(lib, from, &output, id, name).err().unwrap_or_default();
                }
            }
        }
    }
    // A wire let go of over the node's rows or title, not on a port: the
    // inputs that take it.
    let released = ui.input(|i| i.pointer.primary_released());
    if released && (rows.contains_pointer() || dr.contains_pointer()) {
        if let Some((from, output)) = view.wiring.take() {
            if from != id {
                let at = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
                view.drop = Some(super::Drop { from: (from, output), to: id, at });
            }
        }
    }

    if dr.clicked() || rows.clicked() { view.selected = Some(id); }
    dr.context_menu(|ui| {
        if ui.button("Delete").clicked() {
            graph.remove(id);
            if view.selected == Some(id) { view.selected = None; }
            ui.close();
        }
    });
    if dr.drag_started() && view.wiring.is_none() {
        view.dragging = Some(id);
        view.drag_offset = dr.interact_pointer_pos().unwrap_or_default() - np;
        view.selected = Some(id);
    }
    if dr.dragged() && view.dragging == Some(id) {
        if let Some(ptr) = dr.interact_pointer_pos() {
            let p = to_canvas(ptr - view.drag_offset);
            if let Some(n) = graph.node_mut(id) { n.pos = [p.x.round(), p.y.round()]; }
        }
    }
    if dr.drag_stopped() { view.dragging = None; }
}
