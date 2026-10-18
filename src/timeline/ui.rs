
use super::{Playback, RulerUnit, TimelineSource, TimelineState};
use crate::core::anim::AnimData;
use crate::node_graph::NodeGraphState;

mod xsi {
    // Light-theme colours. `theme::c` returns the dark counterpart in dark mode.
    #![allow(non_snake_case)]
    use egui::Color32;
    pub fn PANEL_BG() -> Color32 { crate::theme::c(104, 104, 104) }
    pub fn TRACK_BG() -> Color32 { crate::theme::c( 78,  78,  78) }
    pub fn INPUT_BAR() -> Color32 { crate::theme::c(116, 116, 116) }
    pub fn CLIP_BAR() -> Color32 { crate::theme::c( 96, 122, 148) }
    pub fn CLIP_EDGE() -> Color32 { crate::theme::c(160, 195, 225) }
    pub fn TICK() -> Color32 { crate::theme::c(150, 150, 150) }
    pub fn TICK_MINOR() -> Color32 { crate::theme::c(112, 112, 112) }
    pub fn TEXT() -> Color32 { crate::theme::c(248, 248, 248) }
    pub fn TEXT_DIM() -> Color32 { crate::theme::c(210, 210, 210) }
    pub fn PLAYHEAD() -> Color32 { crate::theme::c(235, 190,  95) }
}

const TRACK_H:   f32 = 40.0;
const TRACK_PAD: f32 = 12.0;

/// Frame range of `other` in frames of `clip`'s rate, matched by timecode.
fn range_in(clip: &AnimData, other: &AnimData) -> (i64, i64) {
    (
        clip.frame_at(other.tc_seconds(other.start_frame)),
        clip.frame_at(other.tc_seconds(other.end_frame())),
    )
}

/// Advances playback and draws the panel. `dt` is the frame's delta time.
/// `keys` is false while a text field has keyboard focus.
pub fn draw_timeline(
    ui:    &mut egui::Ui,
    pb:    &mut Playback,
    state: &TimelineState,
    graph: &mut NodeGraphState,
    dt:    f64,
    keys:  bool,
    // False when the navigation style uses Space itself.
    space_plays: bool,
) {
    egui::Frame::NONE.fill(xsi::PANEL_BG()).inner_margin(6.0).show(ui, |ui| {
        match state {
            TimelineState::Source(src) => draw_source(ui, pb, src, graph, dt, keys, space_plays),
            // Not a clip: the scene's own range and rate, and the keys of
            // the selected operator.
            TimelineState::NoTimeData | TimelineState::Nothing => draw_scene(ui, pb, graph, dt, keys, space_plays),
        }
    });
}

/// A key on the timeline being dragged: the operator, the frame it was
/// on when the drag began, the frame it is on now.
#[derive(Clone, Copy)]
struct KeyDrag { node: crate::types::NodeId, now: f64 }

/// The selected operator's keys as red ticks on the track, at `x_of(frame)`
/// (scene frames). Dragging a tick moves the keys on it, by whole frames;
/// right-click offers to set or delete keys at the playhead. Returns true
/// while a key is being dragged, so the track does not scrub.
fn key_ticks(
    ui:    &egui::Ui,
    painter: &egui::Painter,
    resp:  &egui::Response,
    rect:  egui::Rect,
    graph: &mut NodeGraphState,
    x_of:  &dyn Fn(f64) -> f32,
    frame_of: &dyn Fn(f32) -> f64,
) -> bool {
    let Some(id) = graph.selected_node else { return false };
    // A Local Animation's keys are on its own frames: drawn where they fall.
    let off = graph.nodes.iter().find(|n| n.id == id).map(|n| graph.local_offset(n)).unwrap_or(0.0);
    let x_of = |f: f64| x_of(f + off);
    let frame_of = |x: f32| frame_of(x) - off;
    let frames = graph.key_frames(id);
    let drag_id = egui::Id::new("timeline_key_drag");
    let mut drag: Option<KeyDrag> = ui.data(|d| d.get_temp(drag_id));
    let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
    let near = |p: egui::Pos2| frames.iter().copied().find(|f| (x_of(*f) - p.x).abs() <= 4.0);
    if resp.drag_started_by(egui::PointerButton::Primary) {
        if let Some(f) = pointer.and_then(near) { drag = Some(KeyDrag { node: id, now: f }); }
    }
    if let (Some(d), Some(p)) = (drag.as_mut(), pointer) {
        if resp.dragged() && d.node == id {
            let to = frame_of(p.x).round();
            if (to - d.now).abs() > 0.5 { graph.move_keys(id, d.now, to); d.now = to; }
        }
    }
    if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) { drag = None; }
    ui.data_mut(|d| match drag { Some(k) => { d.insert_temp(drag_id, k); } None => { d.remove::<KeyDrag>(drag_id); } });
    let red = crate::keys_ui::key_red();
    for f in graph.key_frames(id) {
        let x = x_of(f);
        if x < rect.min.x - 1.0 || x > rect.max.x + 1.0 { continue; }
        let lit = drag.is_some_and(|d| (d.now - f).abs() < 1e-6) || pointer.is_some_and(|p| (x - p.x).abs() <= 4.0 && rect.contains(p));
        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(x - if lit { 2.0 } else { 1.5 }, rect.min.y + 18.0), egui::pos2(x + if lit { 2.0 } else { 1.5 }, rect.max.y - 3.0)), 0.0,
            if lit { egui::Color32::from_rgb(255, 120, 100) } else { red });
    }
    if pointer.is_some_and(|p| rect.contains(p) && near(p).is_some()) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    drag.is_some()
}

/// Right-click on the track: key or unkey the selected operator here.
fn key_menu(resp: &egui::Response, graph: &mut NodeGraphState) {
    let Some(id) = graph.selected_node else { return };
    let Some(frame) = graph.nodes.iter().find(|n| n.id == id).map(|n| graph.node_frame(n).round()) else { return };
    resp.context_menu(|ui| {
        if ui.button(format!("Key all at frame {frame} (S)")).clicked() { graph.key_all(id); ui.close(); }
        let here = graph.key_frames(id).iter().any(|f| (f - frame).abs() < 1e-6);
        if ui.add_enabled(here, egui::Button::new(format!("Delete keys at frame {frame}"))).clicked() { graph.delete_keys_at(id, frame); ui.close(); }
    });
}

/// The scene's range when what is selected is not a clip: first and last
/// frame and rate, saved with the graph, and the keys of the selected
/// operator.
fn draw_scene(ui: &mut egui::Ui, pb: &mut Playback, graph: &mut NodeGraphState, dt: f64, keys: bool, space_plays: bool) {
    let sc = graph.scene;
    let fps = sc.fps();
    let (start, end) = (sc.start, sc.end.max(sc.start + 1));
    let set_frame = |pb: &mut Playback, f: i64| { pb.time = sc.seconds(f.clamp(start, end) as f64); };
    let cur = |pb: &Playback| sc.frame(pb.time).round() as i64;

    if pb.playing {
        pb.frac += dt * fps;
        let whole = pb.frac.floor();
        pb.frac -= whole;
        let f = cur(pb) + whole as i64;
        if f > end {
            if pb.looping { set_frame(pb, start) } else { set_frame(pb, end); pb.playing = false; }
        } else { set_frame(pb, f.max(start)); }
    } else {
        pb.frac = 0.0;
        let f = cur(pb);
        if f < start || f > end { set_frame(pb, f.clamp(start, end)); }
    }
    if keys {
        let (space, left, right, home, endk) = ui.input(|i| (
            space_plays && i.key_pressed(egui::Key::Space),
            i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowRight),
            i.key_pressed(egui::Key::Home), i.key_pressed(egui::Key::End),
        ));
        let f = cur(pb);
        if space { pb.playing = !pb.playing; }
        if left  { pb.playing = false; set_frame(pb, f - 1); }
        if right { pb.playing = false; set_frame(pb, f + 1); }
        if home  { set_frame(pb, start); }
        if endk  { set_frame(pb, end); }
    }
    let frame = cur(pb);

    let mut scene = graph.scene;
    ui.horizontal(|ui| {
        if ui.button("⏮").on_hover_text("First frame (Home)").clicked() { set_frame(pb, start); }
        if ui.button("◀").on_hover_text("Previous frame (Left)").clicked() { pb.playing = false; set_frame(pb, frame - 1); }
        let label = if pb.playing { "⏸" } else { "▶" };
        if ui.add(egui::Button::new(label).min_size(egui::vec2(34.0, 0.0))).on_hover_text("Play / pause (Space)").clicked() {
            pb.playing = !pb.playing;
            if pb.playing && frame >= end { set_frame(pb, start); }
        }
        if ui.button("▶").on_hover_text("Next frame (Right)").clicked() { pb.playing = false; set_frame(pb, frame + 1); }
        if ui.button("⏭").on_hover_text("Last frame (End)").clicked() { set_frame(pb, end); }
        // Previous and next key of the selected operator.
        let off = graph.selected_node.and_then(|id| graph.nodes.iter().find(|n| n.id == id)).map(|n| graph.local_offset(n)).unwrap_or(0.0);
        let keyed: Vec<f64> = graph.selected_node.map(|id| graph.key_frames(id)).unwrap_or_default().into_iter().map(|f| f + off).collect();
        let prev = keyed.iter().rev().find(|k| **k < frame as f64 - 1e-6).copied();
        let next = keyed.iter().find(|k| **k > frame as f64 + 1e-6).copied();
        if ui.add_enabled(prev.is_some(), egui::Button::new("⏪")).on_hover_text("Previous key of the selected operator").clicked() {
            if let Some(p) = prev { pb.playing = false; pb.time = sc.seconds(p); }
        }
        if ui.add_enabled(next.is_some(), egui::Button::new("⏩")).on_hover_text("Next key of the selected operator").clicked() {
            if let Some(n) = next { pb.playing = false; pb.time = sc.seconds(n); }
        }
        ui.checkbox(&mut pb.looping, "Loop");
        ui.separator();
        ui.colored_label(xsi::TEXT_DIM(), "Frame");
        let mut f = frame;
        if ui.add(egui::DragValue::new(&mut f).range(start..=end).speed(0.25)).changed() { pb.playing = false; set_frame(pb, f); }
        ui.separator();
        ui.colored_label(xsi::TEXT_DIM(), "Range");
        ui.add(egui::DragValue::new(&mut scene.start).speed(0.25)).on_hover_text("The scene's first frame");
        ui.add(egui::DragValue::new(&mut scene.end).speed(0.25)).on_hover_text("The scene's last frame");
        let rates: [(u32, u32, &str); 8] = [(24, 1, "24"), (25, 1, "25"), (30, 1, "30"), (48, 1, "48"), (50, 1, "50"), (60, 1, "60"), (24000, 1001, "23.976"), (30000, 1001, "29.97")];
        let now = rates.iter().find(|r| r.0 == scene.fps_num && r.1 == scene.fps_den).map(|r| r.2.to_string())
            .unwrap_or_else(|| format!("{:.3}", scene.fps()));
        egui::ComboBox::from_id_salt("scene_rate").selected_text(format!("{now} fps")).width(80.0).show_ui(ui, |ui| {
            for (n, d, l) in rates { if ui.selectable_label(scene.fps_num == n && scene.fps_den == d, format!("{l} fps")).clicked() { scene.fps_num = n; scene.fps_den = d; } }
        }).response.on_hover_text("The scene's rate: keys are set on its frames");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let what = graph.selected_node.and_then(|id| graph.nodes.iter().find(|n| n.id == id))
                .map(|n| {
                    let range = graph.time_range(n.id).map(|r| format!("  |  frames {:.0} to {:.0}", r.0, r.1)).unwrap_or_default();
                    let local = if matches!(n.node_type, crate::types::NodeType::LocalAnimation { .. }) { format!("  |  local frame {}", graph.node_frame(n)) } else { String::new() };
                    format!("{}: {} key frame{}{range}{local}", n.name, keyed.len(), if keyed.len() == 1 { "" } else { "s" })
                })
                .unwrap_or_else(|| "Scene".into());
            ui.colored_label(xsi::TEXT(), egui::RichText::new(what).strong());
        });
    });
    // A range of at least one frame; the playhead follows when the rate changes.
    scene.end = scene.end.max(scene.start + 1);
    if scene != graph.scene {
        let at = graph.scene.frame(pb.time);
        graph.scene = scene;
        pb.time = scene.seconds(at);
    }

    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), TRACK_H), egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, xsi::TRACK_BG());
    let x0 = rect.min.x + TRACK_PAD;
    let x1 = rect.max.x - TRACK_PAD;
    let span = (end - start).max(1) as f32;
    let ppf = (x1 - x0) / span;
    let x_of = |f: f64| x0 + (f - start as f64) as f32 * ppf;
    let frame_of = |x: f32| start as f64 + ((x - x0) / ppf) as f64;

    // Ruler: ticks on whole steps of frames, labelled.
    let steps = [1i64, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000];
    let major = *steps.iter().find(|s| **s as f32 * ppf >= 48.0).unwrap_or(&5000);
    let minor = steps.iter().copied().filter(|s| major % *s == 0 && *s < major).find(|s| *s as f32 * ppf >= 5.0);
    if let Some(minor) = minor {
        let mut f = start.div_euclid(minor) * minor;
        while f <= end {
            if f >= start && f % major != 0 {
                let x = x_of(f as f64);
                painter.line_segment([egui::pos2(x, rect.min.y + 12.0), egui::pos2(x, rect.min.y + 18.0)], egui::Stroke::new(1.0, xsi::TICK_MINOR()));
            }
            f += minor;
        }
    }
    let mut f = start.div_euclid(major) * major;
    while f <= end {
        if f >= start {
            let x = x_of(f as f64);
            painter.line_segment([egui::pos2(x, rect.min.y + 5.0), egui::pos2(x, rect.min.y + 18.0)], egui::Stroke::new(1.0, xsi::TICK()));
            painter.text(egui::pos2(x + 3.0, rect.min.y + 2.0), egui::Align2::LEFT_TOP, f.to_string(), egui::FontId::proportional(10.0), xsi::TEXT_DIM());
        }
        f += major;
    }
    // The range as a bar; the selected operator's own range over it, as a
    // clip's is: a Local Animation's, the keys in and above it, a Merge's
    // widest.
    painter.rect_filled(egui::Rect::from_min_max(egui::pos2(x0 - 1.0, rect.min.y + 18.0), egui::pos2(x1 + 1.0, rect.max.y - 3.0)), 2.0, xsi::INPUT_BAR());
    if let Some((a, b)) = graph.selected_node.and_then(|id| graph.time_range(id)) {
        let r = egui::Rect::from_min_max(egui::pos2(x_of(a).max(x0) - 1.0, rect.min.y + 18.0), egui::pos2(x_of(b).min(x1) + 1.0, rect.max.y - 3.0));
        if r.width() > 0.0 {
            painter.rect_filled(r, 2.0, xsi::CLIP_BAR());
            painter.rect_stroke(r, 2.0, egui::Stroke::new(1.0, xsi::CLIP_EDGE()), egui::StrokeKind::Middle);
        }
    }

    let dragging_key = key_ticks(ui, &painter, &resp, rect, graph, &x_of, &frame_of);
    key_menu(&resp, graph);
    if !dragging_key && (resp.dragged_by(egui::PointerButton::Primary) || resp.clicked() || resp.drag_started_by(egui::PointerButton::Primary)) {
        if let Some(p) = resp.interact_pointer_pos() {
            pb.playing = false;
            set_frame(pb, frame_of(p.x).round() as i64);
        }
    }
    let frame = cur(pb);
    let px = x_of(frame as f64);
    painter.line_segment([egui::pos2(px, rect.min.y), egui::pos2(px, rect.max.y)], egui::Stroke::new(2.0, xsi::PLAYHEAD()));
    let galley = painter.layout_no_wrap(frame.to_string(), egui::FontId::proportional(10.0), egui::Color32::BLACK);
    let w = galley.size().x + 8.0;
    let tag = egui::Rect::from_min_size(egui::pos2((px + 2.0).min(rect.max.x - w), rect.max.y - 16.0), egui::vec2(w, 14.0));
    painter.rect_filled(tag, 2.0, xsi::PLAYHEAD());
    painter.galley(egui::pos2(tag.min.x + 4.0, tag.min.y + 1.0), galley, egui::Color32::BLACK);
}

fn draw_source(ui: &mut egui::Ui, pb: &mut Playback, src: &TimelineSource, graph: &mut NodeGraphState, dt: f64, keys: bool, space_plays: bool) {
    let clip = &src.clip;
    let fps  = clip.rate.fps();

    // ── Ranges, all in absolute frames at the clip's rate ────────────────────
    let (out_start, out_end) = (clip.start_frame, clip.end_frame());
    let (mut ext_start, mut ext_end) = (out_start, out_end);
    // The incoming clip is shown only where it shares time with the output.
    // After a Set Timecode the two are hours apart and it would only shrink
    // the view.
    let input_range = src.input.as_ref()
        .map(|i| range_in(clip, i))
        .filter(|(a, b)| *a <= out_end && *b >= out_start);
    if let Some((a, b)) = input_range {
        ext_start = ext_start.min(a);
        ext_end   = ext_end.max(b);
    }

    let set_frame = |pb: &mut Playback, f: i64| {
        pb.time = clip.tc_seconds(f.clamp(ext_start, ext_end));
    };

    // ── Advance ──────────────────────────────────────────────────────────────
    if pb.playing {
        pb.frac += dt * fps;
        let whole = pb.frac.floor();
        pb.frac  -= whole;
        let f = clip.frame_at(pb.time) + whole as i64;
        if f < out_start {
            set_frame(pb, out_start);
        } else if f > out_end {
            if pb.looping && out_end > out_start {
                set_frame(pb, out_start);
            } else {
                set_frame(pb, out_end);
                pb.playing = false;
            }
        } else {
            set_frame(pb, f);
        }
    } else {
        pb.frac = 0.0;
        let f = clip.frame_at(pb.time);
        if f < ext_start || f > ext_end {
            set_frame(pb, if f < ext_start { out_start } else { out_end });
        }
    }

    // ── Keys ─────────────────────────────────────────────────────────────────
    if keys {
        let (space, left, right, home, end) = ui.input(|i| (
            space_plays && i.key_pressed(egui::Key::Space),
            i.key_pressed(egui::Key::ArrowLeft),
            i.key_pressed(egui::Key::ArrowRight),
            i.key_pressed(egui::Key::Home),
            i.key_pressed(egui::Key::End),
        ));
        let cur = clip.frame_at(pb.time);
        if space { pb.playing = !pb.playing; }
        if left  { pb.playing = false; set_frame(pb, cur - 1); }
        if right { pb.playing = false; set_frame(pb, cur + 1); }
        if home  { set_frame(pb, out_start); }
        if end   { set_frame(pb, out_end); }
    }

    let mut frame = clip.frame_at(pb.time).clamp(ext_start, ext_end);

    // ── Transport row ────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        if ui.button("⏮").on_hover_text("First frame (Home)").clicked() { set_frame(pb, out_start); }
        if ui.button("◀").on_hover_text("Previous frame (Left)").clicked() {
            pb.playing = false; set_frame(pb, frame - 1);
        }
        let label = if pb.playing { "⏸" } else { "▶" };
        if ui.add(egui::Button::new(label).min_size(egui::vec2(34.0, 0.0)))
            .on_hover_text("Play / pause (Space)").clicked()
        {
            pb.playing = !pb.playing;
            if pb.playing && frame >= out_end { set_frame(pb, out_start); }
        }
        if ui.button("▶").on_hover_text("Next frame (Right)").clicked() {
            pb.playing = false; set_frame(pb, frame + 1);
        }
        if ui.button("⏭").on_hover_text("Last frame (End)").clicked() { set_frame(pb, out_end); }
        ui.checkbox(&mut pb.looping, "Loop");

        ui.separator();
        ui.colored_label(xsi::TEXT_DIM(), "Frame");
        let mut f = frame;
        if ui.add(egui::DragValue::new(&mut f).range(ext_start..=ext_end).speed(0.25)).changed() {
            pb.playing = false;
            set_frame(pb, f);
        }
        ui.colored_label(xsi::TEXT(),
            egui::RichText::new(clip.timecode(frame).to_string()).monospace().size(14.0));

        ui.separator();
        ui.selectable_value(&mut pb.ruler, RulerUnit::Frames,   "Frames");
        ui.selectable_value(&mut pb.ruler, RulerUnit::Timecode, "Timecode");

        // ── What the timeline is following ───────────────────────────────────
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let rate = format!("{} fps{}", clip.rate.label(), if clip.drop_frame { " DF" } else { "" });
            ui.colored_label(xsi::TEXT_DIM(), format!(
                "{}  |  {} frames  |  {} - {}  |  {} joints",
                rate, clip.frames,
                clip.timecode(out_start), clip.timecode(out_end),
                clip.joints.len(),
            ));
            ui.colored_label(xsi::TEXT(), egui::RichText::new(format!(
                "{}{} / {}",
                if src.from_selection { "" } else { "viewed: " },
                src.node_name, clip.name,
            )).strong());
        });
    });

    frame = clip.frame_at(pb.time).clamp(ext_start, ext_end);

    // ── Scrub track ──────────────────────────────────────────────────────────
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), TRACK_H), egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, xsi::TRACK_BG());

    let x0   = rect.min.x + TRACK_PAD;
    let x1   = rect.max.x - TRACK_PAD;
    let span = (ext_end - ext_start).max(1) as f32;
    let ppf  = (x1 - x0) / span;                         // pixels per frame
    let x_of = |f: i64| x0 + (f - ext_start) as f32 * ppf;

    let bar_top = rect.min.y + 18.0;
    let bar = |a: i64, b: i64| egui::Rect::from_min_max(
        egui::pos2(x_of(a) - 1.0, bar_top), egui::pos2(x_of(b) + 1.0, rect.max.y - 3.0));

    // Clip entering the node (context), then the clip the node outputs.
    if let Some((a, b)) = input_range {
        painter.rect_filled(bar(a, b), 2.0, xsi::INPUT_BAR());
    }
    painter.rect_filled(bar(out_start, out_end), 2.0, xsi::CLIP_BAR());
    painter.rect_stroke(bar(out_start, out_end), 2.0, egui::Stroke::new(1.0_f32, xsi::CLIP_EDGE()), egui::StrokeKind::Middle);

    // ── Ruler ────────────────────────────────────────────────────────────────
    // Tick spacing follows the clip's timebase so major ticks land on seconds.
    let tb = clip.rate.timebase();
    let mut steps: Vec<i64> = vec![1, 2, 5, 10];
    for m in [1, 2, 5, 10, 30, 60, 120, 300, 600, 1800, 3600] { steps.push(tb * m); }
    steps.sort_unstable();
    steps.dedup();

    let label_px = if pb.ruler == RulerUnit::Timecode { 96.0 } else { 56.0 };
    let major_i  = steps.iter().position(|s| *s as f32 * ppf >= label_px).unwrap_or(steps.len() - 1);
    let major    = steps[major_i];
    let minor    = steps[..major_i].iter().copied()
        .find(|s| *s as f32 * ppf >= 5.0 && major % *s == 0);

    if let Some(minor) = minor {
        let mut f = ext_start.div_euclid(minor) * minor;
        while f <= ext_end {
            if f >= ext_start && f % major != 0 {
                let x = x_of(f);
                painter.line_segment(
                    [egui::pos2(x, rect.min.y + 12.0), egui::pos2(x, bar_top)],
                    egui::Stroke::new(1.0_f32, xsi::TICK_MINOR()));
            }
            f += minor;
        }
    }
    let mut f = ext_start.div_euclid(major) * major;
    while f <= ext_end {
        if f >= ext_start {
            let x = x_of(f);
            painter.line_segment(
                [egui::pos2(x, rect.min.y + 5.0), egui::pos2(x, bar_top)],
                egui::Stroke::new(1.0_f32, xsi::TICK()));
            let text = match pb.ruler {
                RulerUnit::Frames   => f.to_string(),
                RulerUnit::Timecode => clip.timecode(f).to_string(),
            };
            painter.text(egui::pos2(x + 3.0, rect.min.y + 2.0), egui::Align2::LEFT_TOP,
                text, egui::FontId::proportional(10.0), xsi::TEXT_DIM());
        }
        f += major;
    }

    // ── Keys of the selected operator, by the scene's frames ─────────────────
    let sc = graph.scene;
    let key_x = |f: f64| x_of(clip.frame_at(sc.seconds(f)));
    let key_frame = |x: f32| sc.frame(clip.tc_seconds(ext_start + ((x - x0) / ppf).round() as i64));
    let dragging_key = key_ticks(ui, &painter, &resp, rect, graph, &key_x, &key_frame);
    key_menu(&resp, graph);

    // ── Scrub ────────────────────────────────────────────────────────────────
    if !dragging_key && (resp.dragged_by(egui::PointerButton::Primary) || resp.clicked() || resp.drag_started_by(egui::PointerButton::Primary)) {
        if let Some(p) = resp.interact_pointer_pos() {
            let f = ext_start + ((p.x - x0) / ppf).round() as i64;
            pb.playing = false;
            set_frame(pb, f);
            frame = clip.frame_at(pb.time).clamp(ext_start, ext_end);
        }
    }

    // ── Playhead ─────────────────────────────────────────────────────────────
    let px = x_of(frame);
    painter.line_segment(
        [egui::pos2(px, rect.min.y), egui::pos2(px, rect.max.y)],
        egui::Stroke::new(2.0_f32, xsi::PLAYHEAD()));
    let tag = match pb.ruler {
        RulerUnit::Frames   => frame.to_string(),
        RulerUnit::Timecode => clip.timecode(frame).to_string(),
    };
    let galley = painter.layout_no_wrap(tag, egui::FontId::proportional(10.0), egui::Color32::BLACK);
    let w      = galley.size().x + 8.0;
    let tag_x  = (px + 2.0).min(rect.max.x - w);
    let tag_r  = egui::Rect::from_min_size(
        egui::pos2(tag_x, rect.max.y - 16.0), egui::vec2(w, 14.0));
    painter.rect_filled(tag_r, 2.0, xsi::PLAYHEAD());
    painter.galley(egui::pos2(tag_r.min.x + 4.0, tag_r.min.y + 1.0), galley, egui::Color32::BLACK);
}
