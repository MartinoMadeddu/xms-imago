//! The Graph Editor: the curves of the selected operator's keyed numbers,
//! frame across, value up, to shape by their keys and tangents.
//!
//! | To | Do this |
//! |---|---|
//! | Select keys | Click; drag a box on the empty canvas; Shift adds or takes away, Ctrl takes away |
//! | Move keys | Drag a selected key; Shift keeps to the direction first moved in. Snap rounds frames to whole ones |
//! | Shape a tangent | Drag a handle of a selected key: its sides become Fixed, together unless broken |
//! | Insert a key | Double-click a curve, or I with the pointer over one |
//! | Delete keys | Delete or Backspace |
//! | Pan, zoom | Middle-drag (or Alt+middle) pans; the wheel zooms about the pointer (Shift: frames only, Ctrl: values only); Alt+right-drag zooms |
//! | Frame | A: every curve; F: the selected keys |
//! | Copy, paste | Ctrl+C, Ctrl+V: the selected keys, pasted at the playhead |
//! | Move the playhead | Click or drag in the ruler along the top |
//!
//! The channels list at the left shows the operator's keyed numbers; click
//! one to see only it, Ctrl+click to add one. Above the canvas: the tangent
//! types, Break and Unify, Weighted, the infinities before and after, Snap,
//! and the frame and value of the selected keys to type in.

use std::collections::HashSet;

use crate::core::keys::{self, Channel, Infinity, Key, Tangent};
use crate::node_graph::keying::kind_of;
use crate::node_graph::NodeGraphState;
use crate::timeline::Playback;
use crate::types::NodeId;

/// The Graph Editor's own state: what it looks at and what is selected.
pub struct GraphEditor {
    node:      Option<NodeId>,
    /// Channels shown; empty: all.
    shown:     Vec<String>,
    /// Selected keys, by channel and frame.
    selected:  Vec<(String, f64)>,
    /// Frames and values in view: left, right, bottom, top.
    view:      [f64; 4],
    /// The view has been framed on this node's curves.
    framed:    Option<NodeId>,
    drag:      Drag,
    pub snap:  bool,
    /// Keys copied: channel, and keys with frames from the first copied.
    clipboard: Vec<(String, Vec<Key>)>,
}

impl Default for GraphEditor {
    fn default() -> Self {
        GraphEditor { node: None, shown: vec![], selected: vec![], view: [0.0, 120.0, -1.0, 1.0], framed: None, drag: Drag::None, snap: true, clipboard: vec![] }
    }
}

#[derive(Clone)]
enum Drag {
    None,
    /// Selected keys moving: the channels as the drag began, the pointer
    /// there, the direction kept with Shift (0 frames, 1 values).
    Keys { start: egui::Pos2, before: Vec<Channel>, sel: Vec<(String, f64)>, axis: Option<u8> },
    /// A tangent handle: channel, key frame, which side.
    Handle { path: String, frame: f64, inward: bool },
    Box { start: egui::Pos2 },
    Pan,
    Zoom { start: egui::Pos2, view: [f64; 4] },
    Scrub,
}

/// The colour of a channel: X red, Y green, Z blue, others by turn.
fn channel_colour(path: &str, k: usize) -> egui::Color32 {
    match path.rsplit('/').next() {
        Some("0") => egui::Color32::from_rgb(230, 80, 80),
        Some("1") => egui::Color32::from_rgb(110, 210, 90),
        Some("2") => egui::Color32::from_rgb(90, 150, 255),
        _ => [egui::Color32::from_rgb(230, 200, 80), egui::Color32::from_rgb(200, 120, 230), egui::Color32::from_rgb(80, 210, 210), egui::Color32::from_rgb(240, 150, 80)][k % 4],
    }
}

/// Screen from curve space and back, for one frame of drawing.
#[derive(Clone, Copy)]
struct Map { rect: egui::Rect, view: [f64; 4] }

impl Map {
    fn x(&self, f: f64) -> f32 { self.rect.min.x + ((f - self.view[0]) / (self.view[1] - self.view[0])) as f32 * self.rect.width() }
    fn y(&self, v: f64) -> f32 { self.rect.max.y - ((v - self.view[2]) / (self.view[3] - self.view[2])) as f32 * self.rect.height() }
    fn p(&self, f: f64, v: f64) -> egui::Pos2 { egui::pos2(self.x(f), self.y(v)) }
    fn f(&self, x: f32) -> f64 { self.view[0] + ((x - self.rect.min.x) / self.rect.width()) as f64 * (self.view[1] - self.view[0]) }
    fn v(&self, y: f32) -> f64 { self.view[2] + ((self.rect.max.y - y) / self.rect.height()) as f64 * (self.view[3] - self.view[2]) }
    /// Frames per pixel and values per pixel.
    fn per_px(&self) -> (f64, f64) { ((self.view[1] - self.view[0]) / self.rect.width() as f64, (self.view[3] - self.view[2]) / self.rect.height() as f64) }
}

/// A tidy step for grid lines: 1, 2 or 5 times a power of ten, at least
/// `min` units.
fn nice_step(min: f64) -> f64 {
    let p = 10f64.powf(min.max(1e-9).log10().floor());
    for m in [1.0, 2.0, 5.0, 10.0] { if m * p >= min { return m * p; } }
    10.0 * p
}

/// The view around keys (frames, values), with a margin.
fn around(points: impl Iterator<Item = (f64, f64)>) -> Option<[f64; 4]> {
    let mut b: Option<[f64; 4]> = None;
    for (f, v) in points {
        b = Some(match b { None => [f, f, v, v], Some(b) => [b[0].min(f), b[1].max(f), b[2].min(v), b[3].max(v)] });
    }
    b.map(|b| {
        let (w, h) = ((b[1] - b[0]).max(10.0), (b[3] - b[2]).max(1.0));
        let (cf, cv) = ((b[0] + b[1]) * 0.5, (b[2] + b[3]) * 0.5);
        [cf - w * 0.7, cf + w * 0.7, cv - h * 0.7, cv + h * 0.7]
    })
}

impl GraphEditor {
    fn is_selected(&self, path: &str, frame: f64) -> bool { self.selected.iter().any(|(p, f)| p == path && (f - frame).abs() < 1e-6) }

    fn select(&mut self, path: &str, frame: f64, mode: Mode) {
        let there = self.is_selected(path, frame);
        match mode {
            Mode::Replace => self.selected = vec![(path.to_string(), frame)],
            Mode::Toggle => if there { self.selected.retain(|(p, f)| !(p == path && (f - frame).abs() < 1e-6)) } else { self.selected.push((path.to_string(), frame)) },
            Mode::Remove => self.selected.retain(|(p, f)| !(p == path && (f - frame).abs() < 1e-6)),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode { Replace, Toggle, Remove }

/// The Graph Editor pane.
pub fn draw(ui: &mut egui::Ui, ed: &mut GraphEditor, graph: &mut NodeGraphState, pb: &mut Playback) {
    let id = graph.selected_node;
    if ed.node != id { ed.node = id; ed.selected.clear(); ed.shown.clear(); ed.drag = Drag::None; }
    let Some(id) = id else { return empty(ui, "Select an operator to see the curves of its keys.") };
    let Some(node) = graph.nodes.iter().find(|n| n.id == id) else { return };
    if node.channels.is_empty() {
        return empty(ui, &format!("{} has no keys. Key its numbers in Parameters (Keys), or press S to key them all at the playhead.", node.name));
    }
    let kind = kind_of(&node.node_type);
    let name = node.name.clone();
    // Selected keys that are gone (undone, deleted elsewhere) go.
    let present: HashSet<(String, u64)> = node.channels.iter().flat_map(|c| c.keys.iter().map(move |k| (c.path.clone(), k.frame.to_bits()))).collect();
    ed.selected.retain(|(p, f)| present.contains(&(p.clone(), f.to_bits())));
    let paths: Vec<String> = node.channels.iter().map(|c| c.path.clone()).collect();
    ed.shown.retain(|p| paths.contains(p));
    let visible: Vec<String> = if ed.shown.is_empty() { paths.clone() } else { ed.shown.clone() };
    let scale_of = |p: &str| keys::display_scale(&kind, p);

    toolbar(ui, ed, graph, id, &kind);
    ui.separator();

    egui::Panel::left(egui::Id::new("graph_editor_channels")).resizable(true).default_size(150.0).show(ui, |ui| {
        ui.label(egui::RichText::new(&name).strong());
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (k, p) in paths.iter().enumerate() {
                let on = ed.shown.is_empty() || ed.shown.contains(p);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 2.0, channel_colour(p, k));
                    let resp = ui.selectable_label(!ed.shown.is_empty() && on, keys::label(p));
                    if resp.clicked() {
                        let add = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                        if add { if ed.shown.contains(p) { ed.shown.retain(|x| x != p) } else { ed.shown.push(p.clone()) } }
                        else if ed.shown.len() == 1 && ed.shown[0] == *p { ed.shown.clear(); }
                        else { ed.shown = vec![p.clone()]; }
                    }
                });
            }
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Click: only that curve. Ctrl+click: add it. Click it again: all").small().weak());
    });

    // ── The canvas ──────────────────────────────────────────────────────────
    // What is left of the pane, as far as it is seen.
    let rect = ui.available_rect_before_wrap().intersect(ui.clip_rect());
    let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, crate::theme::c(72, 72, 72));
    let ruler = egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.min.y + 18.0));
    let plot = egui::Rect::from_min_max(egui::pos2(rect.min.x + 44.0, ruler.max.y), rect.max);

    // Framed on the curves when the operator changes.
    let node = graph.nodes.iter().find(|n| n.id == id).unwrap();
    let all_points = |only: &[String]| -> Vec<(f64, f64)> {
        node.channels.iter().filter(|c| only.contains(&c.path))
            .flat_map(|c| { let s = scale_of(&c.path); c.keys.iter().map(move |k| (k.frame, k.value * s)) }).collect()
    };
    if ed.framed != Some(id) {
        if let Some(v) = around(all_points(&visible).into_iter()) { ed.view = v; }
        ed.framed = Some(id);
    }
    let map = Map { rect: plot, view: ed.view };

    // Grid and labels.
    painter.rect_filled(ruler, 0.0, crate::theme::c(60, 60, 60));
    let (fpp, vpp) = map.per_px();
    let fstep = nice_step(fpp * 60.0).max(1.0);
    let vstep = nice_step(vpp * 32.0);
    let grid = crate::theme::c(84, 84, 84);
    let ink = crate::theme::c(200, 200, 200);
    let mut f = (ed.view[0] / fstep).floor() * fstep;
    while f <= ed.view[1] {
        let x = map.x(f);
        if x >= plot.min.x {
            painter.line_segment([egui::pos2(x, plot.min.y), egui::pos2(x, plot.max.y)], egui::Stroke::new(1.0, grid));
            painter.text(egui::pos2(x + 2.0, ruler.min.y + 2.0), egui::Align2::LEFT_TOP, format!("{}", f as i64), egui::FontId::proportional(10.0), ink);
        }
        f += fstep;
    }
    let mut v = (ed.view[2] / vstep).floor() * vstep;
    while v <= ed.view[3] {
        let y = map.y(v);
        if y >= plot.min.y {
            painter.line_segment([egui::pos2(plot.min.x, y), egui::pos2(plot.max.x, y)], egui::Stroke::new(if v.abs() < vstep * 1e-6 { 1.6 } else { 1.0 }, grid));
            let digits = if vstep >= 1.0 { 0 } else { (-vstep.log10()).ceil() as usize };
            let v = if v.abs() < vstep * 1e-6 { 0.0 } else { v };
            painter.text(egui::pos2(rect.min.x + 2.0, y), egui::Align2::LEFT_CENTER, format!("{v:.digits$}"), egui::FontId::proportional(10.0), ink);
        }
        v += vstep;
    }
    // The scene's range, shaded.
    let (s0, s1) = (map.x(graph.scene.start as f64), map.x(graph.scene.end as f64));
    painter.rect_filled(egui::Rect::from_min_max(egui::pos2(s0.max(plot.min.x), plot.min.y), egui::pos2(s1.min(plot.max.x), plot.max.y)), 0.0, egui::Color32::from_white_alpha(6));

    // Curves: sampled every two pixels; beyond the keys (infinity) fainter.
    let plot_painter = painter.with_clip_rect(plot);
    for (k, c) in node.channels.iter().enumerate() {
        if !visible.contains(&c.path) || c.keys.is_empty() { continue; }
        let s = scale_of(&c.path);
        let col = channel_colour(&c.path, k);
        let (first, last) = (c.keys[0].frame, c.keys[c.keys.len() - 1].frame);
        let mut inside = vec![];
        let mut before = vec![];
        let mut after = vec![];
        let mut x = plot.min.x;
        while x <= plot.max.x + 2.0 {
            let f = map.f(x);
            let p = map.p(f, c.value_at(f) * s);
            if f < first { before.push(p) } else if f > last { after.push(p) } else { inside.push(p) }
            x += 2.0;
        }
        // The keys themselves, so a sharp corner is drawn sharp.
        for kk in &c.keys { inside.push(map.p(kk.frame, kk.value * s)); }
        inside.sort_by(|a, b| a.x.total_cmp(&b.x));
        if let (Some(b), Some(i)) = (before.last().copied(), inside.first().copied()) { before.push(i); let _ = b; }
        if let Some(i) = inside.last().copied() { after.insert(0, i); }
        let faint = col.gamma_multiply(0.45);
        if before.len() > 1 { plot_painter.add(egui::Shape::line(before, egui::Stroke::new(1.2, faint))); }
        if after.len() > 1 { plot_painter.add(egui::Shape::line(after, egui::Stroke::new(1.2, faint))); }
        if inside.len() > 1 { plot_painter.add(egui::Shape::line(inside, egui::Stroke::new(1.6, col))); }
    }

    // Keys, and the handles of the selected ones.
    let handle_len = 42.0f32;
    let mut handles: Vec<(String, f64, bool, egui::Pos2, egui::Pos2)> = vec![];   // path, frame, inward, key, tip
    let mut key_points: Vec<(String, f64, egui::Pos2)> = vec![];
    for (k, c) in node.channels.iter().enumerate() {
        if !visible.contains(&c.path) { continue; }
        let s = scale_of(&c.path);
        for (i, key) in c.keys.iter().enumerate() {
            let at = map.p(key.frame, key.value * s);
            key_points.push((c.path.clone(), key.frame, at));
            let sel = ed.is_selected(&c.path, key.frame);
            if sel {
                let (si, so) = c.slopes(i);
                for (inward, slope) in [(true, si), (false, so)] {
                    if inward && i == 0 && c.pre == Infinity::Constant && c.keys.len() > 1 { continue; }
                    if !inward && i + 1 == c.keys.len() && c.post == Infinity::Constant && c.keys.len() > 1 { continue; }
                    if !inward && key.out_type == Tangent::Stepped { continue; }
                    let dir = if inward { -1.0 } else { 1.0 };
                    let tip = if c.weighted && ((inward && i > 0) || (!inward && i + 1 < c.keys.len())) {
                        let w = if inward { c.weights(i - 1).1 } else { c.weights(i).0 };
                        map.p(key.frame + dir * w, (key.value + dir * slope * w) * s)
                    } else {
                        // A handle of a fixed length on screen along the slope.
                        let d = egui::vec2(1.0 / fpp as f32, -(slope * s) as f32 / vpp as f32).normalized() * handle_len * dir as f32;
                        at + d
                    };
                    handles.push((c.path.clone(), key.frame, inward, at, tip));
                }
            }
            let _ = k;
        }
    }
    let handle_col = crate::theme::c(120, 200, 230);
    for (_, _, _, at, tip) in &handles {
        plot_painter.line_segment([*at, *tip], egui::Stroke::new(1.0, handle_col));
        plot_painter.circle_filled(*tip, 3.2, handle_col);
    }
    for (p, f, at) in &key_points {
        let sel = ed.is_selected(p, *f);
        let r = if sel { 3.6 } else { 3.0 };
        plot_painter.rect_filled(egui::Rect::from_center_size(*at, egui::vec2(r * 2.0, r * 2.0)), 0.0,
            if sel { egui::Color32::from_rgb(255, 225, 60) } else { egui::Color32::from_gray(20) });
        plot_painter.rect_stroke(egui::Rect::from_center_size(*at, egui::vec2(r * 2.0, r * 2.0)), 0.0, egui::Stroke::new(1.0, egui::Color32::from_gray(230)), egui::StrokeKind::Middle);
    }

    // The playhead.
    let frame_now = graph.frame();
    let px = map.x(frame_now);
    if px >= plot.min.x && px <= plot.max.x {
        painter.line_segment([egui::pos2(px, ruler.min.y), egui::pos2(px, plot.max.y)], egui::Stroke::new(1.6, crate::theme::c(235, 190, 95)));
        painter.text(egui::pos2(px + 3.0, ruler.max.y - 2.0), egui::Align2::LEFT_BOTTOM, format!("{}", frame_now.round() as i64), egui::FontId::proportional(10.0), crate::theme::c(235, 190, 95));
    }

    // ── Interaction ─────────────────────────────────────────────────────────
    let (mods, pointer, wheel, mid_pressed, mid_down, right_pressed, right_down, primary_pressed, primary_down) = ui.input(|i| (
        i.modifiers, i.pointer.hover_pos(), i.smooth_scroll_delta,
        i.pointer.button_pressed(egui::PointerButton::Middle), i.pointer.middle_down(),
        i.pointer.button_pressed(egui::PointerButton::Secondary), i.pointer.secondary_down(),
        i.pointer.primary_pressed(), i.pointer.primary_down(),
    ));
    let over = pointer.is_some_and(|p| rect.contains(p));
    let near_key = |p: egui::Pos2| key_points.iter().filter(|k| k.2.distance(p) <= 6.0).min_by(|a, b| a.2.distance(p).total_cmp(&b.2.distance(p))).cloned();
    let near_handle = |p: egui::Pos2| handles.iter().find(|h| h.4.distance(p) <= 6.0).cloned();

    // Zoom with the wheel about the pointer.
    if over && wheel.y != 0.0 {
        let k = (1.0f64 - wheel.y as f64 * 0.0015).clamp(0.5, 2.0);
        let p = pointer.unwrap();
        let (cf, cv) = (map.f(p.x), map.v(p.y));
        if !mods.ctrl { ed.view[0] = cf + (ed.view[0] - cf) * k; ed.view[1] = cf + (ed.view[1] - cf) * k; }
        if !mods.shift { ed.view[2] = cv + (ed.view[2] - cv) * k; ed.view[3] = cv + (ed.view[3] - cv) * k; }
    }
    if over && mid_pressed { ed.drag = Drag::Pan; }
    if over && right_pressed && mods.alt { ed.drag = Drag::Zoom { start: pointer.unwrap(), view: ed.view }; }
    if over && primary_pressed {
        let p = pointer.unwrap();
        ed.drag = if ruler.contains(p) { Drag::Scrub }
        else if let Some(h) = near_handle(p) { Drag::Handle { path: h.0, frame: h.1, inward: h.2 } }
        else if let Some(k) = near_key(p) {
            let mode = if mods.ctrl { Mode::Remove } else if mods.shift { Mode::Toggle } else if ed.is_selected(&k.0, k.1) { Mode::Toggle } else { Mode::Replace };
            if mode == Mode::Toggle && !mods.shift { /* already selected: keep the selection to move it */ } else { ed.select(&k.0, k.1, mode); }
            if ed.is_selected(&k.0, k.1) {
                Drag::Keys { start: p, before: node.channels.clone(), sel: ed.selected.clone(), axis: None }
            } else { Drag::None }
        } else { Drag::Box { start: p } };
    }
    let delta = ui.input(|i| i.pointer.delta());
    let mut new_channels: Option<Vec<Channel>> = None;
    let mut new_sel: Option<Vec<(String, f64)>> = None;
    match ed.drag.clone() {
        Drag::Pan if mid_down => {
            let (fpp, vpp) = map.per_px();
            ed.view[0] -= delta.x as f64 * fpp; ed.view[1] -= delta.x as f64 * fpp;
            ed.view[2] += delta.y as f64 * vpp; ed.view[3] += delta.y as f64 * vpp;
        }
        Drag::Zoom { start, view } if right_down => {
            if let Some(p) = pointer {
                let d = p - start;
                let (kx, ky) = ((-d.x as f64 * 0.005).exp(), (d.y as f64 * 0.005).exp());
                let (cf, cv) = (Map { rect: plot, view }.f(start.x), Map { rect: plot, view }.v(start.y));
                ed.view = [cf + (view[0] - cf) * kx, cf + (view[1] - cf) * kx, cv + (view[2] - cv) * ky, cv + (view[3] - cv) * ky];
            }
        }
        Drag::Scrub if primary_down => if let Some(p) = pointer {
            pb.playing = false;
            pb.time = graph.scene.seconds(map.f(p.x).round());
        },
        Drag::Box { start } if primary_down => if let Some(p) = pointer {
            let r = egui::Rect::from_two_pos(start, p);
            plot_painter.rect_filled(r, 0.0, egui::Color32::from_rgba_unmultiplied(120, 170, 255, 30));
            plot_painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(150, 190, 255)), egui::StrokeKind::Middle);
        },
        Drag::Box { start } => {
            // Released: what is inside is selected (Shift: toggled, Ctrl:
            // taken away); a click on nothing clears.
            if let Some(p) = pointer.or(Some(start)) {
                let r = egui::Rect::from_two_pos(start, p);
                let inside: Vec<(String, f64)> = key_points.iter().filter(|k| r.contains(k.2)).map(|k| (k.0.clone(), k.1)).collect();
                if mods.ctrl { for (path, f) in inside { ed.select(&path, f, Mode::Remove); } }
                else if mods.shift { for (path, f) in inside { ed.select(&path, f, Mode::Toggle); } }
                else { ed.selected = inside; }
            }
            ed.drag = Drag::None;
        }
        Drag::Keys { start, before, sel, axis } if primary_down => if let Some(p) = pointer {
            let mut d = p - start;
            let mut axis = axis;
            if mods.shift && axis.is_none() && d.length() > 6.0 { axis = Some(if d.x.abs() >= d.y.abs() { 0 } else { 1 }); }
            if !mods.shift { axis = None; }
            match axis { Some(0) => d.y = 0.0, Some(_) => d.x = 0.0, None => {} }
            let (fpp, vpp) = map.per_px();
            let mut df = d.x as f64 * fpp;
            if ed.snap { df = df.round(); }
            let dv = -(d.y as f64) * vpp;
            let mut chans = before.clone();
            for c in &mut chans {
                let s = scale_of(&c.path);
                let picked: Vec<f64> = sel.iter().filter(|(p, _)| *p == c.path).map(|(_, f)| *f).collect();
                if picked.is_empty() { continue; }
                // The moved keys replace any they land on.
                let moved: Vec<Key> = c.keys.iter().filter(|k| picked.iter().any(|f| (f - k.frame).abs() < 1e-6))
                    .map(|k| Key { frame: k.frame + df, value: k.value + dv / s, ..*k }).collect();
                c.keys.retain(|k| !picked.iter().any(|f| (f - k.frame).abs() < 1e-6) && !moved.iter().any(|m| (m.frame - k.frame).abs() < 1e-6));
                c.keys.extend(moved);
                c.sort();
            }
            new_channels = Some(chans);
            new_sel = Some(sel.iter().map(|(p, f)| (p.clone(), f + df)).collect());
            if let Drag::Keys { axis: a, .. } = &mut ed.drag { *a = axis; }
        },
        Drag::Handle { path, frame, inward } if primary_down => if let Some(p) = pointer {
            if let Some(c) = node.channels.iter().find(|c| c.path == path) {
                if let Some(i) = c.key_at(frame) {
                    let s = scale_of(&path);
                    let key = c.keys[i];
                    let (hf, hv) = (map.f(p.x), map.v(p.y) / s);
                    let dt = if inward { (key.frame - hf).max(1e-3) } else { (hf - key.frame).max(1e-3) };
                    let slope = if inward { (key.value - hv) / dt } else { (hv - key.value) / dt };
                    let mut c2 = c.clone();
                    c2.set_slope(i, inward, slope, Some(dt));
                    let mut chans = node.channels.clone();
                    if let Some(x) = chans.iter_mut().find(|x| x.path == path) { *x = c2; }
                    new_channels = Some(chans);
                }
            }
        },
        Drag::None => {}
        _ => ed.drag = Drag::None,
    }
    if let Some(s) = new_sel { ed.selected = s; }
    if let Some(chans) = new_channels {
        if let Some(n) = graph.nodes.iter_mut().find(|n| n.id == id) { n.channels = chans; }
    }

    // Keys pressed over the canvas.
    if over && !ui.ctx().egui_wants_keyboard_input() {
        let (del, frame_all, frame_sel, insert, copy, paste) = ui.input(|i| (
            i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
            i.key_pressed(egui::Key::A) && !i.modifiers.any(),
            i.key_pressed(egui::Key::F) && !i.modifiers.any(),
            i.key_pressed(egui::Key::I),
            i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
            i.events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::V, pressed: true, modifiers, .. } if modifiers.command)),
        ));
        let node = graph.nodes.iter().find(|n| n.id == id).unwrap();
        if frame_all {
            let pts: Vec<(f64, f64)> = node.channels.iter().filter(|c| visible.contains(&c.path))
                .flat_map(|c| { let s = scale_of(&c.path); c.keys.iter().map(move |k| (k.frame, k.value * s)) }).collect();
            if let Some(v) = around(pts.into_iter()) { ed.view = v; }
        }
        if frame_sel {
            let pts: Vec<(f64, f64)> = node.channels.iter().flat_map(|c| { let s = scale_of(&c.path); let sel = &ed.selected; c.keys.iter().filter(move |k| sel.iter().any(|(p, f)| *p == c.path && (f - k.frame).abs() < 1e-6)).map(move |k| (k.frame, k.value * s)) }).collect();
            if let Some(v) = around(pts.into_iter()) { ed.view = v; }
        }
        if copy && !ed.selected.is_empty() {
            let first = ed.selected.iter().map(|(_, f)| *f).fold(f64::MAX, f64::min);
            ed.clipboard = node.channels.iter().filter_map(|c| {
                let ks: Vec<Key> = c.keys.iter().filter(|k| ed.is_selected(&c.path, k.frame)).map(|k| Key { frame: k.frame - first, ..*k }).collect();
                (!ks.is_empty()).then(|| (c.path.clone(), ks))
            }).collect();
        }
        if paste && !ed.clipboard.is_empty() {
            let at = graph.frame().round();
            let clip = ed.clipboard.clone();
            if let Some(n) = graph.nodes.iter_mut().find(|n| n.id == id) {
                ed.selected.clear();
                for (path, ks) in clip {
                    let i = match n.channels.iter().position(|c| c.path == path) { Some(i) => i, None => { n.channels.push(Channel::new(&path)); n.channels.len() - 1 } };
                    for k in ks {
                        let f = at + k.frame;
                        if let Some(j) = n.channels[i].key_at(f) { n.channels[i].keys.remove(j); }
                        n.channels[i].keys.push(Key { frame: f, ..k });
                        ed.selected.push((path.clone(), f));
                    }
                    n.channels[i].sort();
                }
            }
        }
        if del && !ed.selected.is_empty() {
            let sel = std::mem::take(&mut ed.selected);
            for (p, f) in sel { graph.remove_key(id, &p, f); }
        }
        if insert { if let Some(p) = pointer { insert_on_curve(ed, graph, id, &visible, &map, p, &scale_of); } }
    }
    if resp.double_clicked() { if let Some(p) = pointer { insert_on_curve(ed, graph, id, &visible, &map, p, &scale_of); } }
}

/// A key on the nearest visible curve under the pointer, at its frame,
/// with the value the curve has there, so the curve keeps its shape.
fn insert_on_curve(ed: &mut GraphEditor, graph: &mut NodeGraphState, id: NodeId, visible: &[String], map: &Map, p: egui::Pos2, scale_of: &dyn Fn(&str) -> f64) {
    let Some(node) = graph.nodes.iter().find(|n| n.id == id) else { return };
    let mut f = map.f(p.x);
    if ed.snap { f = f.round(); }
    let best = node.channels.iter().filter(|c| visible.contains(&c.path))
        .map(|c| (c.path.clone(), c.value_at(f), (map.y(c.value_at(f) * scale_of(&c.path)) - p.y).abs()))
        .min_by(|a, b| a.2.total_cmp(&b.2));
    if let Some((path, v, d)) = best {
        if d <= 12.0 {
            graph.set_key_value(id, &path, f, v);
            ed.selected = vec![(path, f)];
        }
    }
}

fn empty(ui: &mut egui::Ui, msg: &str) {
    ui.add_space(8.0);
    ui.label(egui::RichText::new(msg).weak());
}

/// Tangent types, Break, Unify, Weighted, infinities, Snap, and the frame
/// and value of the selected keys.
fn toolbar(ui: &mut egui::Ui, ed: &mut GraphEditor, graph: &mut NodeGraphState, id: NodeId, kind: &str) {
    let Some(node) = graph.nodes.iter_mut().find(|n| n.id == id) else { return };
    let sel = ed.selected.clone();
    let has = !sel.is_empty();
    let chans_of_sel: Vec<String> = { let mut v: Vec<String> = sel.iter().map(|(p, _)| p.clone()).collect(); v.sort(); v.dedup(); v };
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Tangents").weak());
        for t in Tangent::ALL {
            if ui.add_enabled(has, egui::Button::new(t.label()).small()).on_hover_text(tangent_tip(t)).clicked() {
                for c in &mut node.channels {
                    for k in &mut c.keys {
                        if sel.iter().any(|(p, f)| *p == c.path && (f - k.frame).abs() < 1e-6) {
                            k.out_type = t;
                            k.in_type = if t == Tangent::Stepped { Tangent::Flat } else { t };
                        }
                    }
                }
            }
        }
        ui.separator();
        if ui.add_enabled(has, egui::Button::new("Break").small()).on_hover_text("The in and out handles of the selected keys move apart").clicked() {
            for c in &mut node.channels { for k in &mut c.keys { if sel.iter().any(|(p, f)| *p == c.path && (f - k.frame).abs() < 1e-6) { k.broken = true; } } }
        }
        if ui.add_enabled(has, egui::Button::new("Unify").small()).on_hover_text("The in and out handles of the selected keys move together again, on the out handle's slope").clicked() {
            for c in &mut node.channels {
                let picked: Vec<usize> = c.keys.iter().enumerate().filter(|(_, k)| sel.iter().any(|(p, f)| *p == c.path && (f - k.frame).abs() < 1e-6)).map(|(i, _)| i).collect();
                for i in picked {
                    let (_, so) = c.slopes(i);
                    let k = &mut c.keys[i];
                    k.broken = false;
                    if k.in_type != k.out_type || k.in_type == Tangent::Fixed { k.in_type = Tangent::Fixed; k.out_type = Tangent::Fixed; k.in_slope = so; k.out_slope = so; }
                }
            }
        }
        ui.separator();
        // Weighted, and the infinities, for the channels of the selected keys
        // (every shown channel when none is selected).
        let targets: Vec<String> = if has { chans_of_sel.clone() } else { ed_visible(ed, node) };
        let first = node.channels.iter().find(|c| targets.contains(&c.path)).cloned();
        if let Some(first) = first {
            let mut w = first.weighted;
            if ui.checkbox(&mut w, "Weighted").on_hover_text("Handles of any length: drag one out to hold the value longer near its key").changed() {
                for c in &mut node.channels { if targets.contains(&c.path) { c.weighted = w; } }
            }
            for (label, pre) in [("Before", true), ("After", false)] {
                let now = if pre { first.pre } else { first.post };
                egui::ComboBox::from_id_salt(("infinity", pre)).selected_text(format!("{label}: {}", now.label())).width(150.0).show_ui(ui, |ui| {
                    for inf in Infinity::ALL {
                        if ui.selectable_label(inf == now, inf.label()).clicked() {
                            for c in &mut node.channels { if targets.contains(&c.path) { if pre { c.pre = inf } else { c.post = inf } } }
                        }
                    }
                }).response.on_hover_text(if pre { "What the curve does before its first key" } else { "What the curve does after its last key" });
            }
        }
        ui.separator();
        ui.checkbox(&mut ed.snap, "Snap").on_hover_text("Keys moved and inserted land on whole frames");
        // The selected keys' frame and value, to type in.
        if let Some((p0, f0)) = sel.first().cloned() {
            ui.separator();
            let s = keys::display_scale(kind, &p0);
            let v0 = node.channels.iter().find(|c| c.path == p0).and_then(|c| c.key_at(f0).map(|i| c.keys[i].value)).unwrap_or(0.0);
            let (mut f, mut v) = (f0, v0 * s);
            ui.label(egui::RichText::new("Frame").weak());
            let fr = ui.add(egui::DragValue::new(&mut f).speed(0.25).max_decimals(2));
            ui.label(egui::RichText::new("Value").weak());
            let vr = ui.add(egui::DragValue::new(&mut v).speed(0.01).max_decimals(4));
            if fr.changed() || vr.changed() {
                let (df, dv) = (f - f0, v / s - v0);
                let only_one = sel.len() == 1;
                for c in &mut node.channels {
                    let sc = keys::display_scale(kind, &c.path);
                    for k in &mut c.keys {
                        if sel.iter().any(|(p, ff)| *p == c.path && (ff - k.frame).abs() < 1e-6) {
                            k.frame += df;
                            // One key: its value as typed; several: moved alike.
                            if vr.changed() { if only_one { k.value = v / sc } else { k.value += dv * s / sc } }
                        }
                    }
                    c.sort();
                }
                ed.selected = sel.iter().map(|(p, ff)| (p.clone(), ff + df)).collect();
            }
            if sel.len() > 1 { ui.label(egui::RichText::new(format!("{} keys", sel.len())).weak()); }
        }
    });
}

fn ed_visible(ed: &GraphEditor, node: &crate::node_graph::GraphNode) -> Vec<String> {
    if ed.shown.is_empty() { node.channels.iter().map(|c| c.path.clone()).collect() } else { ed.shown.clone() }
}

fn tangent_tip(t: Tangent) -> &'static str {
    match t {
        Tangent::Auto => "Smooth, never overshooting the keys on either side; flat at peaks and troughs",
        Tangent::Spline => "Smooth through the keys on either side",
        Tangent::Clamped => "Spline, flat where a neighbour has the same value",
        Tangent::Linear => "Straight to the next key",
        Tangent::Flat => "Level at the key",
        Tangent::Stepped => "Hold the value until the next key",
        Tangent::Plateau => "Spline, flat at peaks, troughs and ends, never overshooting",
        Tangent::Fixed => "As dragged",
    }
}
