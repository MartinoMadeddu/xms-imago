use super::{Playback, RulerUnit, Speed, TimelineSource, TimelineState};
use crate::core::anim::AnimData;
use crate::node_graph::NodeGraphState;

mod xsi {
    // Light-theme colours. `theme::c` returns the dark counterpart in dark mode.
    #![allow(non_snake_case)]
    use egui::Color32;
    pub fn PANEL_BG() -> Color32 {
        crate::theme::c(104, 104, 104)
    }
    pub fn TRACK_BG() -> Color32 {
        crate::theme::c(78, 78, 78)
    }
    pub fn INPUT_BAR() -> Color32 {
        crate::theme::c(116, 116, 116)
    }
    pub fn CLIP_BAR() -> Color32 {
        crate::theme::c(96, 122, 148)
    }
    pub fn CLIP_EDGE() -> Color32 {
        crate::theme::c(160, 195, 225)
    }
    pub fn TICK() -> Color32 {
        crate::theme::c(150, 150, 150)
    }
    pub fn TICK_MINOR() -> Color32 {
        crate::theme::c(112, 112, 112)
    }
    pub fn TEXT() -> Color32 {
        crate::theme::c(248, 248, 248)
    }
    pub fn TEXT_DIM() -> Color32 {
        crate::theme::c(210, 210, 210)
    }
    pub fn PLAYHEAD() -> Color32 {
        crate::theme::c(235, 190, 95)
    }
}

const TRACK_H: f32 = 40.0;
const TRACK_PAD: f32 = 12.0;

/// Frame range of `other` in frames of `clip`'s rate, matched by timecode.
fn range_in(clip: &AnimData, other: &AnimData) -> (i64, i64) {
    (
        clip.frame_at(other.tc_seconds(other.start_frame)),
        clip.frame_at(other.tc_seconds(other.end_frame())),
    )
}

/// A key on the timeline being dragged: the operator, the frame it was on when
/// the drag began, the frame it is on now.
#[derive(Clone, Copy)]
struct KeyDrag {
    node: crate::types::NodeId,
    now: f64,
}

/// The selected operator's keys as red ticks on the track, at `x_of(frame)`
/// (scene frames). Dragging a tick moves the keys on it, by whole frames;
/// right-click offers to set or delete keys at the playhead. Returns true while
/// a key is being dragged, so the track does not scrub.
fn key_ticks(
    ui: &egui::Ui,
    painter: &egui::Painter,
    resp: &egui::Response,
    rect: egui::Rect,
    graph: &mut NodeGraphState,
    x_of: &dyn Fn(f64) -> f32,
    frame_of: &dyn Fn(f32) -> f64,
) -> bool {
    let Some(id) = graph.selected_node else { return false };
    // A Local Animation's keys are on its own frames: drawn where they fall.
    let off = graph
        .nodes
        .iter()
        .find(|n| n.id == id)
        .map(|n| graph.local_offset(n))
        .unwrap_or(0.0);
    let x_of = |f: f64| x_of(f + off);
    let frame_of = |x: f32| frame_of(x) - off;
    let frames = graph.key_frames(id);
    let drag_id = egui::Id::new("timeline_key_drag");
    let mut drag: Option<KeyDrag> = ui.data(|d| d.get_temp(drag_id));
    let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
    let near = |p: egui::Pos2| frames.iter().copied().find(|f| (x_of(*f) - p.x).abs() <= 4.0);
    if resp.drag_started_by(egui::PointerButton::Primary) {
        // Where the button went down: a drag starts only once it has moved.
        let origin = ui.input(|i| i.pointer.press_origin()).or(pointer);
        if let Some(f) = origin.and_then(near) {
            drag = Some(KeyDrag { node: id, now: f });
        }
    }
    if let (Some(d), Some(p)) = (drag.as_mut(), pointer) {
        if resp.dragged() && d.node == id {
            let to = frame_of(p.x).round();
            if (to - d.now).abs() > 0.5 {
                graph.move_keys(id, d.now, to);
                d.now = to;
            }
        }
    }
    if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
        drag = None;
    }
    ui.data_mut(|d| match drag {
        Some(k) => {
            d.insert_temp(drag_id, k);
        }
        None => {
            d.remove::<KeyDrag>(drag_id);
        }
    });
    let red = crate::keys_ui::key_red();
    for f in graph.key_frames(id) {
        let x = x_of(f);
        if x < rect.min.x - 1.0 || x > rect.max.x + 1.0 {
            continue;
        }
        let lit = drag.is_some_and(|d| (d.now - f).abs() < 1e-6)
            || pointer.is_some_and(|p| (x - p.x).abs() <= 4.0 && rect.contains(p));
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x - if lit { 2.0 } else { 1.5 }, rect.min.y + 18.0),
                egui::pos2(x + if lit { 2.0 } else { 1.5 }, rect.max.y - 3.0),
            ),
            0.0,
            if lit {
                egui::Color32::from_rgb(255, 120, 100)
            } else {
                red
            },
        );
    }
    if pointer.is_some_and(|p| rect.contains(p) && near(p).is_some()) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    drag.is_some()
}

/// Right-click on the track: key or unkey the selected operator here.
fn key_menu(resp: &egui::Response, graph: &mut NodeGraphState) {
    let Some(id) = graph.selected_node else { return };
    let Some(frame) = graph
        .nodes
        .iter()
        .find(|n| n.id == id)
        .map(|n| graph.node_frame(n).round())
    else {
        return;
    };
    resp.context_menu(|ui| {
        if ui.button(format!("Key all at frame {frame} (S)")).clicked() {
            graph.key_all(id);
            ui.close();
        }
        let here = graph.key_frames(id).iter().any(|f| (f - frame).abs() < 1e-6);
        if ui
            .add_enabled(here, egui::Button::new(format!("Delete keys at frame {frame}")))
            .clicked()
        {
            graph.delete_keys_at(id, frame);
            ui.close();
        }
    });
}


/// Height of the range slider along the bottom.
const RANGE_H: f32 = 20.0;

/// Advances playback and draws the panel. `dt` is the frame's delta time.
/// `keys` is false while a text field has keyboard focus.
pub fn draw_timeline(
    ui: &mut egui::Ui,
    pb: &mut Playback,
    state: &TimelineState,
    graph: &mut NodeGraphState,
    dt: f64,
    keys: bool,
    // False when the navigation style uses Space itself.
    space_plays: bool,
) {
    egui::Frame::NONE
        .fill(xsi::PANEL_BG())
        .inner_margin(6.0)
        .show(ui, |ui| {
            // The pane is filled: the track takes what the rows leave.
            ui.set_min_height(ui.available_height());
            match state {
                TimelineState::Source(src) => draw_source(ui, pb, src, graph, dt, keys, space_plays),
                // Not a clip: the scene's own range and rate, and the keys of
                // the selected operator.
                TimelineState::NoTimeData | TimelineState::Nothing => draw_scene(ui, pb, graph, dt, keys, space_plays),
            }
        });
}

/// The frames the timeline is drawn on: a clip's, or the scene's.
struct Axis<'a> {
    /// Every frame that can be shown.
    full: (i64, i64),
    /// The frames that play when no view in and out is set.
    play: (i64, i64),
    /// Its rate.
    fps: f64,
    /// A frame's playhead time, and the frame at a playhead time.
    time: &'a dyn Fn(i64) -> f64,
    frame: &'a dyn Fn(f64) -> i64,
    timecode: &'a dyn Fn(i64) -> String,
}

impl Axis<'_> {
    /// View in and out: the frames shown, within the whole.
    fn view(&self, pb: &Playback) -> (i64, i64) {
        let (lo, hi) = self.full;
        pb.view
            .map(|(a, b)| ((self.frame)(a).clamp(lo, hi), (self.frame)(b).clamp(lo, hi)))
            .filter(|(a, b)| b > a)
            .unwrap_or(self.full)
    }
    /// The frames that play: the view's, else the clip's own.
    fn playing(&self, pb: &Playback) -> (i64, i64) {
        if pb.view.is_none() {
            return self.play;
        }
        self.view(pb)
    }
    fn at(&self, pb: &Playback) -> i64 {
        (self.frame)(pb.time)
    }
    fn set(&self, pb: &mut Playback, f: i64) {
        pb.time = (self.time)(f.clamp(self.full.0, self.full.1));
    }
    fn label(&self, pb: &Playback, f: i64) -> String {
        match pb.ruler {
            RulerUnit::Frames => f.to_string(),
            RulerUnit::Timecode => (self.timecode)(f),
        }
    }
    /// Set view in and out, or the whole range again.
    fn set_view(&self, pb: &mut Playback, a: i64, b: i64) {
        pb.view = if a <= self.full.0 && b >= self.full.1 {
            None
        } else {
            Some(((self.time)(a), (self.time)(b)))
        };
    }
}

/// The playhead moved on while playing, looping in what plays; kept in the
/// range while stopped.
fn advance(pb: &mut Playback, ax: &Axis, dt: f64) {
    let (lo, hi) = ax.playing(pb);
    if !pb.playing {
        pb.frac = 0.0;
        let f = ax.at(pb);
        if f < ax.full.0 || f > ax.full.1 {
            ax.set(pb, f);
        }
        return;
    }
    let whole = match pb.speed {
        Speed::EveryFrame => {
            pb.frac = 0.0;
            1.0
        }
        s => {
            let fps = if let Speed::Fps(f) = s { f } else { ax.fps };
            pb.frac += dt * fps;
            let w = pb.frac.floor();
            pb.frac -= w;
            w
        }
    };
    let f = ax.at(pb) + whole as i64;
    if f < lo {
        ax.set(pb, lo);
    } else if f > hi {
        if pb.looping && hi > lo {
            ax.set(pb, lo);
        } else {
            ax.set(pb, hi);
            pb.playing = false;
        }
    } else {
        ax.set(pb, f);
    }
}

/// Space, the arrows, Home and End.
fn key_input(ui: &egui::Ui, pb: &mut Playback, ax: &Axis, space_plays: bool) {
    let (space, left, right, home, end) = ui.input(|i| {
        (
            space_plays && i.key_pressed(egui::Key::Space),
            i.key_pressed(egui::Key::ArrowLeft),
            i.key_pressed(egui::Key::ArrowRight),
            i.key_pressed(egui::Key::Home),
            i.key_pressed(egui::Key::End),
        )
    });
    let (lo, hi) = ax.playing(pb);
    let f = ax.at(pb);
    if space {
        pb.playing = !pb.playing;
    }
    if left {
        pb.playing = false;
        ax.set(pb, f - 1);
    }
    if right {
        pb.playing = false;
        ax.set(pb, f + 1);
    }
    if home {
        ax.set(pb, lo);
    }
    if end {
        ax.set(pb, hi);
    }
}

/// The selected operator's keys, as playhead times.
fn key_times(graph: &NodeGraphState) -> Vec<f64> {
    let Some(id) = graph.selected_node else { return vec![] };
    let off = graph
        .nodes
        .iter()
        .find(|n| n.id == id)
        .map(|n| graph.local_offset(n))
        .unwrap_or(0.0);
    graph
        .key_frames(id)
        .into_iter()
        .map(|f| graph.scene.seconds(f + off))
        .collect()
}

/// The top row: transport, keys, the frame, the ruler's unit and the playback
/// speed; then `info`, at the right.
fn transport(ui: &mut egui::Ui, pb: &mut Playback, ax: &Axis, keyed: &[f64], info: impl FnOnce(&mut egui::Ui)) {
    let (lo, hi) = ax.playing(pb);
    let frame = ax.at(pb).clamp(ax.full.0, ax.full.1);
    ui.horizontal(|ui| {
        if ui.button("⏮").on_hover_text("First frame (Home)").clicked() {
            ax.set(pb, lo);
        }
        let now = pb.time;
        let prev = keyed.iter().rev().find(|t| **t < now - 1e-6).copied();
        let next = keyed.iter().find(|t| **t > now + 1e-6).copied();
        if ui
            .add_enabled(prev.is_some(), egui::Button::new("⏪"))
            .on_hover_text("Previous key of the selected operator")
            .clicked()
        {
            if let Some(t) = prev {
                pb.playing = false;
                pb.time = t;
            }
        }
        if ui.button("◀").on_hover_text("Previous frame (Left)").clicked() {
            pb.playing = false;
            ax.set(pb, frame - 1);
        }
        let label = if pb.playing { "⏸" } else { "▶" };
        if ui
            .add(egui::Button::new(label).min_size(egui::vec2(34.0, 0.0)))
            .on_hover_text("Play / pause (Space)")
            .clicked()
        {
            pb.playing = !pb.playing;
            if pb.playing && frame >= hi {
                ax.set(pb, lo);
            }
        }
        if ui.button("▶").on_hover_text("Next frame (Right)").clicked() {
            pb.playing = false;
            ax.set(pb, frame + 1);
        }
        if ui
            .add_enabled(next.is_some(), egui::Button::new("⏩"))
            .on_hover_text("Next key of the selected operator")
            .clicked()
        {
            if let Some(t) = next {
                pb.playing = false;
                pb.time = t;
            }
        }
        if ui.button("⏭").on_hover_text("Last frame (End)").clicked() {
            ax.set(pb, hi);
        }
        ui.checkbox(&mut pb.looping, "Loop");

        ui.separator();
        ui.colored_label(xsi::TEXT_DIM(), "Frame");
        let mut f = frame;
        if ui
            .add(
                egui::DragValue::new(&mut f)
                    .update_while_editing(false)
                    .range(ax.full.0..=ax.full.1)
                    .speed(0.25),
            )
            .changed()
        {
            pb.playing = false;
            ax.set(pb, f);
        }
        ui.colored_label(
            xsi::TEXT(),
            egui::RichText::new((ax.timecode)(frame)).monospace().size(14.0),
        );

        ui.separator();
        ui.selectable_value(&mut pb.ruler, RulerUnit::Frames, "Frames")
            .on_hover_text("The ruler in frames");
        ui.selectable_value(&mut pb.ruler, RulerUnit::Timecode, "Timecode")
            .on_hover_text("The ruler in timecode");

        ui.separator();
        egui::ComboBox::from_id_salt("timeline_speed")
            .selected_text(pb.speed.label(ax.fps))
            .width(130.0)
            .height(320.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut pb.speed, Speed::Rate, Speed::Rate.label(ax.fps));
                for f in Speed::FPS {
                    ui.selectable_value(&mut pb.speed, Speed::Fps(f), Speed::Fps(f).label(ax.fps));
                }
                ui.selectable_value(&mut pb.speed, Speed::EveryFrame, "Every frame");
            })
            .response
            .on_hover_text(
                "Playback speed: real time at the rate of what is shown, a rate of its own, or every frame however long each takes",
            );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), info);
    });
}

/// Where frames fall on a track: `view` across `rect`, inset.
#[derive(Clone, Copy)]
struct Span {
    rect: egui::Rect,
    x0: f32,
    ppf: f32,
    first: i64,
}

impl Span {
    fn new(rect: egui::Rect, view: (i64, i64)) -> Self {
        let x0 = rect.min.x + TRACK_PAD;
        let x1 = rect.max.x - TRACK_PAD;
        Span {
            rect,
            x0,
            ppf: (x1 - x0) / (view.1 - view.0).max(1) as f32,
            first: view.0,
        }
    }
    fn x(&self, f: f64) -> f32 {
        self.x0 + (f - self.first as f64) as f32 * self.ppf
    }
    fn frame(&self, x: f32) -> f64 {
        self.first as f64 + ((x - self.x0) / self.ppf) as f64
    }
}

/// The track: what is left of the pane above the range slider.
fn track(ui: &mut egui::Ui) -> (egui::Rect, egui::Response) {
    let h = (ui.available_height() - RANGE_H - ui.spacing().item_spacing.y).max(TRACK_H);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::click_and_drag());
    ui.painter().rect_filled(rect, 3.0, xsi::TRACK_BG());
    (rect, resp)
}

/// Ticks and labels along the top of the track, major ticks on seconds when
/// they are far enough apart.
fn ruler(painter: &egui::Painter, s: &Span, ax: &Axis, pb: &Playback, view: (i64, i64)) {
    let rect = s.rect;
    let bar_top = rect.min.y + 18.0;
    let tb = (ax.fps.round() as i64).max(1);
    let mut steps: Vec<i64> = vec![1, 2, 5, 10];
    for m in [1, 2, 5, 10, 30, 60, 120, 300, 600, 1800, 3600] {
        steps.push(tb * m);
    }
    steps.sort_unstable();
    steps.dedup();
    let label_px = if pb.ruler == RulerUnit::Timecode { 96.0 } else { 48.0 };
    let major_i = steps
        .iter()
        .position(|st| *st as f32 * s.ppf >= label_px)
        .unwrap_or(steps.len() - 1);
    let major = steps[major_i];
    let minor = steps[..major_i]
        .iter()
        .copied()
        .find(|st| *st as f32 * s.ppf >= 5.0 && major % *st == 0);
    if let Some(minor) = minor {
        let mut f = view.0.div_euclid(minor) * minor;
        while f <= view.1 {
            if f >= view.0 && f % major != 0 {
                let x = s.x(f as f64);
                painter.line_segment(
                    [egui::pos2(x, rect.min.y + 12.0), egui::pos2(x, bar_top)],
                    egui::Stroke::new(1.0_f32, xsi::TICK_MINOR()),
                );
            }
            f += minor;
        }
    }
    let mut f = view.0.div_euclid(major) * major;
    while f <= view.1 {
        if f >= view.0 {
            let x = s.x(f as f64);
            painter.line_segment(
                [egui::pos2(x, rect.min.y + 5.0), egui::pos2(x, bar_top)],
                egui::Stroke::new(1.0_f32, xsi::TICK()),
            );
            painter.text(
                egui::pos2(x + 3.0, rect.min.y + 2.0),
                egui::Align2::LEFT_TOP,
                ax.label(pb, f),
                egui::FontId::proportional(10.0),
                xsi::TEXT_DIM(),
            );
        }
        f += major;
    }
}

/// Clicking or dragging on the track moves the playhead; the playhead and its
/// frame drawn over everything.
fn scrub_and_playhead(painter: &egui::Painter, s: &Span, resp: &egui::Response, ax: &Axis, pb: &mut Playback, dragging_key: bool) {
    if !dragging_key
        && (resp.dragged_by(egui::PointerButton::Primary)
            || resp.clicked()
            || resp.drag_started_by(egui::PointerButton::Primary))
    {
        if let Some(p) = resp.interact_pointer_pos() {
            pb.playing = false;
            ax.set(pb, s.frame(p.x).round() as i64);
        }
    }
    let frame = ax.at(pb).clamp(ax.full.0, ax.full.1);
    let rect = s.rect;
    let px = s.x(frame as f64);
    painter.line_segment(
        [egui::pos2(px, rect.min.y), egui::pos2(px, rect.max.y)],
        egui::Stroke::new(2.0_f32, xsi::PLAYHEAD()),
    );
    let galley = painter.layout_no_wrap(ax.label(pb, frame), egui::FontId::proportional(10.0), egui::Color32::BLACK);
    let w = galley.size().x + 8.0;
    let tag = egui::Rect::from_min_size(
        egui::pos2((px + 2.0).min(rect.max.x - w), rect.max.y - 16.0),
        egui::vec2(w, 14.0),
    );
    painter.rect_filled(tag, 2.0, xsi::PLAYHEAD());
    painter.galley(egui::pos2(tag.min.x + 4.0, tag.min.y + 1.0), galley, egui::Color32::BLACK);
}

/// What a drag on the range slider moves.
#[derive(Clone, Copy, PartialEq)]
enum RangeDrag {
    In,
    Out,
    Both { grab: f64, a: i64, b: i64 },
}

/// The bottom row, a range slider as in other programs: the first frame, view
/// in, the slider over the whole range with the view on it, view out, the last
/// frame. The first and last frames are the scene's, editable, with its rate,
/// when `scene` is given; a clip's own otherwise.
fn range_row(ui: &mut egui::Ui, pb: &mut Playback, ax: &Axis, scene: Option<&mut crate::node_graph::SceneRange>) {
    let (vin, vout) = ax.view(pb);
    let field = |ui: &mut egui::Ui, v: &mut i64, tip: &str| {
        ui.add(egui::DragValue::new(v).update_while_editing(false).speed(0.25))
            .on_hover_text(tip)
            .changed()
    };
    ui.horizontal(|ui| {
        ui.set_height(RANGE_H);
        let mut scene = scene;
        match scene.as_deref_mut() {
            Some(sc) => {
                field(ui, &mut sc.start, "The scene's first frame");
            }
            None => {
                ui.colored_label(xsi::TEXT_DIM(), ax.full.0.to_string())
                    .on_hover_text("The clip's first frame");
            }
        }
        let mut a = vin;
        if field(ui, &mut a, "View in: the first frame shown and played") {
            let a = a.clamp(ax.full.0, vout - 1);
            ax.set_view(pb, a, vout);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(sc) = scene {
                scene_rate(ui, sc);
                field(ui, &mut sc.end, "The scene's last frame");
            } else {
                ui.colored_label(xsi::TEXT_DIM(), ax.full.1.to_string())
                    .on_hover_text("The clip's last frame");
            }
            if ui
                .add_enabled(pb.view.is_some(), egui::Button::new("↔"))
                .on_hover_text("View the whole range (or double-click the slider)")
                .clicked()
            {
                pb.view = None;
            }
            let mut b = vout;
            if field(ui, &mut b, "View out: the last frame shown and played") {
                let b = b.clamp(vin + 1, ax.full.1);
                ax.set_view(pb, vin, b);
            }
            let w = ui.available_width().max(40.0);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, RANGE_H - 4.0), egui::Sense::click_and_drag());
            slider(ui, rect, &resp, pb, ax, (vin, vout));
        });
    });
}

/// The range slider itself: the whole range, the view on it with a handle at
/// each end. Drag a handle to set view in or out, the middle to move both,
/// double-click for the whole range.
fn slider(ui: &egui::Ui, rect: egui::Rect, resp: &egui::Response, pb: &mut Playback, ax: &Axis, view: (i64, i64)) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, xsi::TRACK_BG());
    let x0 = rect.min.x + 6.0;
    let ppf = (rect.width() - 12.0) / (ax.full.1 - ax.full.0).max(1) as f32;
    let x = |f: i64| x0 + (f - ax.full.0) as f32 * ppf;
    let frame = |px: f32| ax.full.0 as f64 + ((px - x0) / ppf) as f64;
    let id = egui::Id::new("timeline_range_drag");
    let mut drag: Option<RangeDrag> = ui.data(|d| d.get_temp(id));
    let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
    let (ax_, bx) = (x(view.0), x(view.1));
    let near = |p: egui::Pos2| {
        if (p.x - ax_).abs() <= 6.0 {
            Some(RangeDrag::In)
        } else if (p.x - bx).abs() <= 6.0 {
            Some(RangeDrag::Out)
        } else if p.x > ax_ && p.x < bx {
            Some(RangeDrag::Both {
                grab: frame(p.x),
                a: view.0,
                b: view.1,
            })
        } else {
            None
        }
    };
    if resp.double_clicked() {
        pb.view = None;
        drag = None;
    } else if resp.drag_started_by(egui::PointerButton::Primary) {
        // Where the button went down: a drag starts only once it has moved.
        let origin = ui.input(|i| i.pointer.press_origin()).or(pointer);
        drag = origin.and_then(near);
    }
    if let (Some(d), Some(p), true) = (drag, pointer, resp.dragged()) {
        let f = frame(p.x).round() as i64;
        match d {
            RangeDrag::In => ax.set_view(pb, f.clamp(ax.full.0, view.1 - 1), view.1),
            RangeDrag::Out => ax.set_view(pb, view.0, f.clamp(view.0 + 1, ax.full.1)),
            RangeDrag::Both { grab, a, b } => {
                let len = b - a;
                let shift = (frame(p.x) - grab).round() as i64;
                let a = (a + shift).clamp(ax.full.0, ax.full.1 - len);
                ax.set_view(pb, a, a + len);
            }
        }
    }
    if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
        drag = None;
    }
    ui.data_mut(|d| match drag {
        Some(k) => {
            d.insert_temp(id, k);
        }
        None => {
            d.remove::<RangeDrag>(id);
        }
    });
    // The view, then its handles, then the playhead.
    let (ax_, bx) = (x(view.0), x(view.1));
    let bar = egui::Rect::from_min_max(egui::pos2(ax_, rect.min.y + 2.0), egui::pos2(bx, rect.max.y - 2.0));
    painter.rect_filled(bar, 2.0, xsi::CLIP_BAR());
    for hx in [ax_, bx] {
        painter.rect_filled(
            egui::Rect::from_center_size(egui::pos2(hx, rect.center().y), egui::vec2(5.0, rect.height() - 2.0)),
            1.5,
            xsi::CLIP_EDGE(),
        );
    }
    let p = x(ax.at(pb).clamp(ax.full.0, ax.full.1));
    painter.line_segment(
        [egui::pos2(p, rect.min.y), egui::pos2(p, rect.max.y)],
        egui::Stroke::new(1.5_f32, xsi::PLAYHEAD()),
    );
    if pointer.is_some_and(|p| rect.contains(p) && matches!(near(p), Some(RangeDrag::In | RangeDrag::Out))) || drag.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
}

/// The scene's rate: its frames are counted at it, keys are set on them.
fn scene_rate(ui: &mut egui::Ui, scene: &mut crate::node_graph::SceneRange) {
    let rates: [(u32, u32, &str); 8] = [
        (24, 1, "24"),
        (25, 1, "25"),
        (30, 1, "30"),
        (48, 1, "48"),
        (50, 1, "50"),
        (60, 1, "60"),
        (24000, 1001, "23.976"),
        (30000, 1001, "29.97"),
    ];
    let now = rates
        .iter()
        .find(|r| r.0 == scene.fps_num && r.1 == scene.fps_den)
        .map(|r| r.2.to_string())
        .unwrap_or_else(|| format!("{:.3}", scene.fps()));
    egui::ComboBox::from_id_salt("scene_rate")
        .selected_text(format!("{now} fps"))
        .width(80.0)
        .show_ui(ui, |ui| {
            for (n, d, l) in rates {
                if ui
                    .selectable_label(scene.fps_num == n && scene.fps_den == d, format!("{l} fps"))
                    .clicked()
                {
                    scene.fps_num = n;
                    scene.fps_den = d;
                }
            }
        })
        .response
        .on_hover_text("The scene's rate: its frames are counted at it and keys are set on them. How fast it plays is the speed at the top");
}

/// The scene's range when what is selected is not a clip: first and last frame
/// and rate, saved with the graph, and the keys of the selected operator.
fn draw_scene(
    ui: &mut egui::Ui,
    pb: &mut Playback,
    graph: &mut NodeGraphState,
    dt: f64,
    keys: bool,
    space_plays: bool,
) {
    let sc = graph.scene;
    let rate = crate::core::anim::FrameRate::new(sc.fps_num, sc.fps_den);
    let full = (sc.start, sc.end.max(sc.start + 1));
    let time = |f: i64| sc.seconds(f as f64);
    let frame = |t: f64| sc.frame(t).round() as i64;
    let timecode = |f: i64| crate::core::anim::frame_to_timecode(f, rate, false).to_string();
    let ax = Axis {
        full,
        play: full,
        fps: sc.fps(),
        time: &time,
        frame: &frame,
        timecode: &timecode,
    };
    advance(pb, &ax, dt);
    if keys {
        key_input(ui, pb, &ax, space_plays);
    }
    let keyed = key_times(graph);
    transport(ui, pb, &ax, &keyed, |ui| {
        let what = graph
            .selected_node
            .and_then(|id| graph.nodes.iter().find(|n| n.id == id))
            .map(|n| {
                let range = graph
                    .time_range(n.id)
                    .map(|r| format!("  |  frames {:.0} to {:.0}", r.0, r.1))
                    .unwrap_or_default();
                let local = if matches!(n.node_type, crate::types::NodeType::LocalAnimation { .. }) {
                    format!("  |  local frame {}", graph.node_frame(n))
                } else {
                    String::new()
                };
                format!(
                    "{}: {} key frame{}{range}{local}",
                    n.name,
                    keyed.len(),
                    if keyed.len() == 1 { "" } else { "s" }
                )
            })
            .unwrap_or_else(|| "Scene".into());
        ui.colored_label(xsi::TEXT(), egui::RichText::new(what).strong());
    });

    let view = ax.view(pb);
    let (rect, resp) = track(ui);
    let painter = ui.painter_at(rect);
    let s = Span::new(rect, view);
    // The range as a bar; the selected operator's own range over it, as a
    // clip's is: a Local Animation's, the keys in and above it, a Merge's
    // widest.
    painter.rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(s.x(full.0 as f64) - 1.0, rect.min.y + 18.0),
            egui::pos2(s.x(full.1 as f64) + 1.0, rect.max.y - 3.0),
        ),
        2.0,
        xsi::INPUT_BAR(),
    );
    if let Some((a, b)) = graph.selected_node.and_then(|id| graph.time_range(id)) {
        let r = egui::Rect::from_min_max(
            egui::pos2(s.x(a) - 1.0, rect.min.y + 18.0),
            egui::pos2(s.x(b) + 1.0, rect.max.y - 3.0),
        );
        if r.width() > 0.0 {
            painter.rect_filled(r, 2.0, xsi::CLIP_BAR());
            painter.rect_stroke(r, 2.0, egui::Stroke::new(1.0, xsi::CLIP_EDGE()), egui::StrokeKind::Middle);
        }
    }
    ruler(&painter, &s, &ax, pb, view);
    let x_of = |f: f64| s.x(f);
    let frame_of = |x: f32| s.frame(x);
    let dragging_key = key_ticks(ui, &painter, &resp, rect, graph, &x_of, &frame_of);
    key_menu(&resp, graph);
    scrub_and_playhead(&painter, &s, &resp, &ax, pb, dragging_key);

    let mut scene = graph.scene;
    range_row(ui, pb, &ax, Some(&mut scene));
    // A range of at least one frame; the playhead follows when the rate changes.
    scene.end = scene.end.max(scene.start + 1);
    if scene != graph.scene {
        let at = graph.scene.frame(pb.time);
        graph.scene = scene;
        pb.time = scene.seconds(at);
    }
}

/// A clip's range, rate and timecode, the clip entering the node behind it.
fn draw_source(
    ui: &mut egui::Ui,
    pb: &mut Playback,
    src: &TimelineSource,
    graph: &mut NodeGraphState,
    dt: f64,
    keys: bool,
    space_plays: bool,
) {
    let clip = &src.clip;
    // Ranges, all in absolute frames at the clip's rate
    let (out_start, out_end) = (clip.start_frame, clip.end_frame());
    let (mut ext_start, mut ext_end) = (out_start, out_end);
    // The incoming clip is shown only where it shares time with the output.
    // After a Set Timecode the two are hours apart and it would only shrink
    // the view.
    let input_range = src
        .input
        .as_ref()
        .map(|i| range_in(clip, i))
        .filter(|(a, b)| *a <= out_end && *b >= out_start);
    if let Some((a, b)) = input_range {
        ext_start = ext_start.min(a);
        ext_end = ext_end.max(b);
    }
    let time = |f: i64| clip.tc_seconds(f);
    let frame = |t: f64| clip.frame_at(t);
    let timecode = |f: i64| clip.timecode(f).to_string();
    let ax = Axis {
        full: (ext_start, ext_end),
        play: (out_start, out_end),
        fps: clip.rate.fps(),
        time: &time,
        frame: &frame,
        timecode: &timecode,
    };
    advance(pb, &ax, dt);
    if keys {
        key_input(ui, pb, &ax, space_plays);
    }
    let keyed = key_times(graph);
    transport(ui, pb, &ax, &keyed, |ui| {
        let rate = format!("{} fps{}", clip.rate.label(), if clip.drop_frame { " DF" } else { "" });
        ui.colored_label(
            xsi::TEXT_DIM(),
            format!(
                "{}  |  {} frames  |  {} - {}  |  {} joints",
                rate,
                clip.frames,
                clip.timecode(out_start),
                clip.timecode(out_end),
                clip.joints.len(),
            ),
        );
        ui.colored_label(
            xsi::TEXT(),
            egui::RichText::new(format!(
                "{}{} / {}",
                if src.from_selection { "" } else { "viewed: " },
                src.node_name,
                clip.name,
            ))
            .strong(),
        );
    });

    let view = ax.view(pb);
    let (rect, resp) = track(ui);
    let painter = ui.painter_at(rect);
    let s = Span::new(rect, view);
    let bar = |a: i64, b: i64| {
        egui::Rect::from_min_max(
            egui::pos2(s.x(a as f64) - 1.0, rect.min.y + 18.0),
            egui::pos2(s.x(b as f64) + 1.0, rect.max.y - 3.0),
        )
    };
    // Clip entering the node (context), then the clip the node outputs.
    if let Some((a, b)) = input_range {
        painter.rect_filled(bar(a, b), 2.0, xsi::INPUT_BAR());
    }
    painter.rect_filled(bar(out_start, out_end), 2.0, xsi::CLIP_BAR());
    painter.rect_stroke(
        bar(out_start, out_end),
        2.0,
        egui::Stroke::new(1.0_f32, xsi::CLIP_EDGE()),
        egui::StrokeKind::Middle,
    );
    ruler(&painter, &s, &ax, pb, view);

    // Keys of the selected operator, by the scene's frames
    let sc = graph.scene;
    let key_x = |f: f64| s.x(clip.frame_at(sc.seconds(f)) as f64);
    let key_frame = |x: f32| sc.frame(clip.tc_seconds(s.frame(x).round() as i64));
    let dragging_key = key_ticks(ui, &painter, &resp, rect, graph, &key_x, &key_frame);
    key_menu(&resp, graph);
    scrub_and_playhead(&painter, &s, &resp, &ax, pb, dragging_key);

    range_row(ui, pb, &ax, None);
}
