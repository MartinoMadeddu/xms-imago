//! Ramps: keys of a value along 0 to 1, and the widget to edit them.
//!
//! A colour ramp is a gradient bar with its keys as handles below it; a float
//! ramp a small curve with its keys as points. Click to add a key (a colour
//! key takes the colour there; a float key the value clicked), drag a key to
//! move it (between the keys either side: they keep their order), right-click it (or Delete with it selected) to take it away. The
//! key selected has its position and value in fields under the ramp, and the
//! ramp one interpolation: Step, Linear, Smooth, and Cubic where the target
//! has it. A ramp is one Rust type, mapped to what holds it: MaterialX's
//! `ramp` node (`materialx::params`), and next Arnold's ramps and ICE's.

/// How a ramp goes from one key to the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interp {
    /// The key's value until the next key.
    Step,
    Linear,
    /// Smoothstep between the two keys (MaterialX's `smooth`).
    Smooth,
    /// Catmull-Rom through the keys around.
    Cubic,
}

impl Interp {
    pub fn label(self) -> &'static str {
        match self {
            Interp::Step => "Step",
            Interp::Linear => "Linear",
            Interp::Smooth => "Smooth",
            Interp::Cubic => "Cubic",
        }
    }
}

/// What a ramp holds: a float, or a colour (RGBA, linear).
pub trait RampValue: Copy + PartialEq {
    fn add(a: Self, b: Self) -> Self;
    fn sub(a: Self, b: Self) -> Self;
    fn scale(a: Self, s: f32) -> Self;
    fn mix(a: Self, b: Self, f: f32) -> Self {
        Self::add(a, Self::scale(Self::sub(b, a), f))
    }
}

impl RampValue for f32 {
    fn add(a: f32, b: f32) -> f32 {
        a + b
    }
    fn sub(a: f32, b: f32) -> f32 {
        a - b
    }
    fn scale(a: f32, s: f32) -> f32 {
        a * s
    }
}

impl RampValue for [f32; 4] {
    fn add(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        std::array::from_fn(|i| a[i] + b[i])
    }
    fn sub(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        std::array::from_fn(|i| a[i] - b[i])
    }
    fn scale(a: [f32; 4], s: f32) -> [f32; 4] {
        std::array::from_fn(|i| a[i] * s)
    }
}

/// A key: where along the ramp (0 to 1), and its value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key<T> {
    pub pos: f32,
    pub value: T,
}

/// Keys, in any order, and how the ramp goes between them.
#[derive(Clone, Debug, PartialEq)]
pub struct Ramp<T> {
    pub keys: Vec<Key<T>>,
    pub interp: Interp,
}

impl<T: RampValue> Ramp<T> {
    /// The keys by position.
    pub fn sorted(&self) -> Vec<Key<T>> {
        let mut k = self.keys.clone();
        k.sort_by(|a, b| a.pos.total_cmp(&b.pos));
        k
    }

    /// The value at `t`: the first key's before it, the last's after.
    pub fn eval(&self, t: f32) -> Option<T> {
        let k = self.sorted();
        let (first, last) = (k.first()?, k.last()?);
        if t <= first.pos {
            return Some(first.value);
        }
        if t >= last.pos {
            return Some(last.value);
        }
        let i = k.windows(2).position(|w| t >= w[0].pos && t < w[1].pos)?;
        let (a, b) = (k[i], k[i + 1]);
        let span = b.pos - a.pos;
        let f = if span > 0.0 { (t - a.pos) / span } else { 1.0 };
        Some(match self.interp {
            Interp::Step => a.value,
            Interp::Linear => T::mix(a.value, b.value, f),
            Interp::Smooth => T::mix(a.value, b.value, f * f * (3.0 - 2.0 * f)),
            Interp::Cubic => {
                // Hermite with Catmull-Rom tangents; at an end, its key stands
                // for the one beyond.
                let before = if i > 0 { k[i - 1] } else { a };
                let after = k.get(i + 2).copied().unwrap_or(b);
                let m0 = T::scale(T::sub(b.value, before.value), 0.5);
                let m1 = T::scale(T::sub(after.value, a.value), 0.5);
                let (f2, f3) = (f * f, f * f * f);
                let h00 = 2.0 * f3 - 3.0 * f2 + 1.0;
                let h10 = f3 - 2.0 * f2 + f;
                let h01 = -2.0 * f3 + 3.0 * f2;
                let h11 = f3 - f2;
                T::add(
                    T::add(T::scale(a.value, h00), T::scale(m0, h10)),
                    T::add(T::scale(b.value, h01), T::scale(m1, h11)),
                )
            }
        })
    }
}

/// What a widget is told: which interpolations the target has, how few keys
/// and how many it takes.
pub struct Limits<'a> {
    pub interps: &'a [Interp],
    pub min_keys: usize,
    pub max_keys: usize,
}

/// The key selected in a ramp's widget.
fn selected_id(id: egui::Id) -> egui::Id {
    id.with("ramp_selected")
}

/// A linear colour as the screen shows it (sRGB).
fn shown(c: [f32; 4]) -> egui::Color32 {
    egui::Color32::from(egui::Rgba::from_rgba_unmultiplied(
        c[0].max(0.0),
        c[1].max(0.0),
        c[2].max(0.0),
        c[3].clamp(0.0, 1.0),
    ))
}

/// A colour ramp: the gradient bar, its handles, the interpolation, and the
/// key selected's position and colour. Whether it changed.
pub fn colour_ramp(ui: &mut egui::Ui, id: egui::Id, ramp: &mut Ramp<[f32; 4]>, limits: &Limits) -> bool {
    const BAR_H: f32 = 22.0;
    const HANDLE_H: f32 = 12.0;
    let mut changed = false;
    let mut selected: Option<usize> = ui
        .data(|d| d.get_temp(selected_id(id)))
        .filter(|k| *k < ramp.keys.len());
    let width = ui.available_width().max(120.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, BAR_H + HANDLE_H + 2.0), egui::Sense::hover());
    let bar = egui::Rect::from_min_size(rect.min, egui::vec2(width, BAR_H));
    let x_of = |t: f32| bar.min.x + t.clamp(0.0, 1.0) * bar.width();
    let t_of = |x: f32| ((x - bar.min.x) / bar.width()).clamp(0.0, 1.0);
    let painter = ui.painter_at(rect.expand(2.0));

    // The gradient, in thin slices, over a checker for alpha.
    let checker = |r: egui::Rect| {
        let s = 6.0;
        let mut y = r.min.y;
        let mut row = 0;
        while y < r.max.y {
            let mut x = r.min.x + if row % 2 == 0 { 0.0 } else { s };
            while x < r.max.x {
                let c =
                    egui::Rect::from_min_max(egui::pos2(x, y), egui::pos2((x + s).min(r.max.x), (y + s).min(r.max.y)));
                painter.rect_filled(c, 0.0, egui::Color32::from_gray(150));
                x += 2.0 * s;
            }
            y += s;
            row += 1;
        }
    };
    painter.rect_filled(bar, 2.0, egui::Color32::from_gray(200));
    checker(bar);
    let slices = (bar.width() / 2.0).ceil().max(1.0) as usize;
    for s in 0..slices {
        let (a, b) = (s as f32 / slices as f32, (s + 1) as f32 / slices as f32);
        let c = ramp.eval((a + b) * 0.5).unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let r = egui::Rect::from_min_max(egui::pos2(x_of(a), bar.min.y), egui::pos2(x_of(b) + 0.5, bar.max.y));
        painter.rect_filled(r, 0.0, shown(c));
    }
    painter.rect_stroke(
        bar,
        2.0,
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );

    // A click on the bar: a key there, of the colour there.
    let bar_resp = ui
        .interact(bar, id.with("bar"), egui::Sense::click())
        .on_hover_text("Click: a key here. Drag a handle to move it; right-click it to take it away");
    if bar_resp.clicked() && ramp.keys.len() < limits.max_keys {
        if let Some(p) = bar_resp.interact_pointer_pos() {
            let t = t_of(p.x);
            let value = ramp.eval(t).unwrap_or([1.0; 4]);
            selected = Some(insert(&mut ramp.keys, Key { pos: t, value }));
            changed = true;
        }
    }

    // The handles: a triangle under each key, of its colour.
    let mut remove = None;
    for k in 0..ramp.keys.len() {
        let x = x_of(ramp.keys[k].pos);
        let top = bar.max.y + 1.0;
        let hit = egui::Rect::from_center_size(egui::pos2(x, top + HANDLE_H * 0.5), egui::vec2(12.0, HANDLE_H + 2.0));
        let r = ui.interact(hit, id.with(("key", k)), egui::Sense::click_and_drag());
        if r.drag_started() || r.clicked() {
            selected = Some(k);
        }
        if r.dragged() {
            if let Some(p) = r.interact_pointer_pos() {
                ramp.keys[k].pos = between(&ramp.keys, k, t_of(p.x));
                changed = true;
            }
        }
        if r.secondary_clicked() && ramp.keys.len() > limits.min_keys {
            remove = Some(k);
        }
        let on = selected == Some(k);
        let points = vec![
            egui::pos2(x, top),
            egui::pos2(x + 6.0, top + HANDLE_H),
            egui::pos2(x - 6.0, top + HANDLE_H),
        ];
        let stroke = if on {
            egui::Stroke::new(2.0, ui.visuals().selection.stroke.color)
        } else {
            egui::Stroke::new(1.0, egui::Color32::from_gray(40))
        };
        painter.add(egui::Shape::convex_polygon(points, shown(ramp.keys[k].value), stroke));
    }
    if let Some(k) = selected {
        let delete = ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
        if delete && ui.memory(|m| m.focused().is_none()) && ramp.keys.len() > limits.min_keys {
            remove = Some(k);
        }
    }
    if let Some(k) = remove {
        ramp.keys.remove(k);
        selected = None;
        changed = true;
    }

    // The interpolation, and the key selected.
    ui.horizontal(|ui| {
        changed |= interp_menu(ui, id, &mut ramp.interp, limits.interps);
        if let Some(k) = selected {
            ui.separator();
            ui.label("Position");
            let (lo, hi) = (between(&ramp.keys, k, 0.0), between(&ramp.keys, k, 1.0));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut ramp.keys[k].pos)
                        .range(lo..=hi)
                        .speed(0.005)
                        .max_decimals(3),
                )
                .changed();
            let mut c = ramp.keys[k].value;
            if ui.color_edit_button_rgba_unmultiplied(&mut c).changed() {
                ramp.keys[k].value = c;
                changed = true;
            }
            for v in ramp.keys[k].value.iter_mut() {
                changed |= ui
                    .add(
                        egui::DragValue::new(v)
                            .speed(0.01)
                            .max_decimals(3)
                            .update_while_editing(false),
                    )
                    .changed();
            }
        } else {
            ui.label(egui::RichText::new(format!("{} keys", ramp.keys.len())).weak());
        }
    });
    if flip_reset(ui, ramp, [0.0, 0.0, 0.0, 1.0], [1.0; 4]) {
        selected = None;
        changed = true;
    }
    ui.data_mut(|d| d.insert_temp(selected_id(id), selected));
    changed
}

/// A float ramp: a curve from `range.0` to `range.1`, its keys as points; the
/// interpolation, and the key selected's position and value. Whether it
/// changed.
#[allow(dead_code)] // Arnold's ramp_float and ICE's Ramp come next.
pub fn float_ramp(ui: &mut egui::Ui, id: egui::Id, ramp: &mut Ramp<f32>, range: (f32, f32), limits: &Limits) -> bool {
    const H: f32 = 90.0;
    let mut changed = false;
    let mut selected: Option<usize> = ui
        .data(|d| d.get_temp(selected_id(id)))
        .filter(|k| *k < ramp.keys.len());
    let width = ui.available_width().max(120.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, H), egui::Sense::click());
    let area = rect.shrink(5.0);
    let (lo, hi) = (range.0, if range.1 > range.0 { range.1 } else { range.0 + 1.0 });
    let pos_of = |t: f32, v: f32| {
        egui::pos2(
            area.min.x + t.clamp(0.0, 1.0) * area.width(),
            area.max.y - ((v - lo) / (hi - lo)).clamp(-0.1, 1.1) * area.height(),
        )
    };
    let tv_of = |p: egui::Pos2| {
        (
            ((p.x - area.min.x) / area.width()).clamp(0.0, 1.0),
            lo + ((area.max.y - p.y) / area.height()).clamp(0.0, 1.0) * (hi - lo),
        )
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
    let grid = egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
    for q in 1..4 {
        let x = area.min.x + area.width() * q as f32 / 4.0;
        let y = area.min.y + area.height() * q as f32 / 4.0;
        painter.line_segment([egui::pos2(x, area.min.y), egui::pos2(x, area.max.y)], grid);
        painter.line_segment([egui::pos2(area.min.x, y), egui::pos2(area.max.x, y)], grid);
    }
    let steps = (area.width() / 3.0).ceil().max(2.0) as usize;
    let curve: Vec<egui::Pos2> = (0..=steps)
        .filter_map(|s| {
            let t = s as f32 / steps as f32;
            ramp.eval(t).map(|v| pos_of(t, v))
        })
        .collect();
    painter.add(egui::Shape::line(
        curve,
        egui::Stroke::new(1.5, ui.visuals().text_color()),
    ));
    let resp = resp.on_hover_text("Click: a key here. Drag a key to move it; right-click it to take it away");
    if resp.clicked() && ramp.keys.len() < limits.max_keys {
        if let Some(p) = resp.interact_pointer_pos() {
            let (t, v) = tv_of(p);
            selected = Some(insert(&mut ramp.keys, Key { pos: t, value: v }));
            changed = true;
        }
    }
    let mut remove = None;
    for k in 0..ramp.keys.len() {
        let at = pos_of(ramp.keys[k].pos, ramp.keys[k].value);
        let r = ui.interact(
            egui::Rect::from_center_size(at, egui::vec2(12.0, 12.0)),
            id.with(("key", k)),
            egui::Sense::click_and_drag(),
        );
        if r.drag_started() || r.clicked() {
            selected = Some(k);
        }
        if r.dragged() {
            if let Some(p) = r.interact_pointer_pos() {
                let (t, v) = tv_of(p);
                ramp.keys[k].pos = between(&ramp.keys, k, t);
                ramp.keys[k].value = v;
                changed = true;
            }
        }
        if r.secondary_clicked() && ramp.keys.len() > limits.min_keys {
            remove = Some(k);
        }
        let on = selected == Some(k);
        painter.circle(
            at,
            if on { 5.0 } else { 4.0 },
            if on {
                ui.visuals().selection.bg_fill
            } else {
                ui.visuals().text_color()
            },
            egui::Stroke::new(1.0, egui::Color32::from_gray(30)),
        );
    }
    if let Some(k) = remove {
        ramp.keys.remove(k);
        selected = None;
        changed = true;
    }
    ui.horizontal(|ui| {
        changed |= interp_menu(ui, id, &mut ramp.interp, limits.interps);
        if let Some(k) = selected {
            ui.separator();
            ui.label("Position");
            let (lo, hi) = (between(&ramp.keys, k, 0.0), between(&ramp.keys, k, 1.0));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut ramp.keys[k].pos)
                        .range(lo..=hi)
                        .speed(0.005)
                        .max_decimals(3),
                )
                .changed();
            ui.label("Value");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut ramp.keys[k].value)
                        .speed(0.01)
                        .max_decimals(3),
                )
                .changed();
        }
    });
    if flip_reset(ui, ramp, lo, hi) {
        selected = None;
        changed = true;
    }
    ui.data_mut(|d| d.insert_temp(selected_id(id), selected));
    changed
}

/// A key put in among the keys by its place (they stay in order, as a target
/// holding them in order has them): its index.
fn insert<T>(keys: &mut Vec<Key<T>>, key: Key<T>) -> usize {
    let at = keys.iter().position(|k| k.pos > key.pos).unwrap_or(keys.len());
    keys.insert(at, key);
    at
}

/// Where a key dragged to `t` goes: between the keys either side of it, so
/// the keys keep their order (and a target that holds them in order, as
/// MaterialX's ramp, keeps the one dragged).
fn between<T>(keys: &[Key<T>], k: usize, t: f32) -> f32 {
    let at = keys[k].pos;
    let lo = keys
        .iter()
        .enumerate()
        .filter(|(i, o)| *i != k && (o.pos < at || (o.pos == at && *i < k)))
        .map(|(_, o)| o.pos)
        .fold(0.0_f32, f32::max);
    let hi = keys
        .iter()
        .enumerate()
        .filter(|(i, o)| *i != k && (o.pos > at || (o.pos == at && *i > k)))
        .map(|(_, o)| o.pos)
        .fold(1.0_f32, f32::min);
    t.clamp(lo, hi.max(lo))
}

/// The interpolation, of those the target has.
fn interp_menu(ui: &mut egui::Ui, id: egui::Id, interp: &mut Interp, offered: &[Interp]) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id.with("interp"))
        .selected_text(interp.label())
        .width(80.0)
        .show_ui(ui, |ui| {
            for i in offered {
                if ui.selectable_label(*interp == *i, i.label()).clicked() {
                    *interp = *i;
                    changed = true;
                }
            }
        });
    changed
}

/// Flip (the keys end to end) and Reset (two keys, `from` to `to`).
fn flip_reset<T: RampValue>(ui: &mut egui::Ui, ramp: &mut Ramp<T>, from: T, to: T) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui.small_button("⇄ Flip").on_hover_text("The keys end to end").clicked() {
            for k in ramp.keys.iter_mut() {
                k.pos = 1.0 - k.pos;
            }
            ramp.keys.reverse();
            changed = true;
        }
        if ui
            .small_button("↺ Reset")
            .on_hover_text("Two keys, from start to end")
            .clicked()
        {
            ramp.keys = vec![Key { pos: 0.0, value: from }, Key { pos: 1.0, value: to }];
            changed = true;
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(interp: Interp) -> Ramp<f32> {
        Ramp {
            keys: vec![
                Key { pos: 1.0, value: 1.0 },
                Key { pos: 0.0, value: 0.0 },
                Key { pos: 0.5, value: 0.2 },
            ],
            interp,
        }
    }

    #[test]
    fn keys_are_read_in_order_whatever_their_order() {
        let r = ramp(Interp::Linear);
        assert_eq!(r.eval(-1.0), Some(0.0));
        assert_eq!(r.eval(2.0), Some(1.0));
        assert!((r.eval(0.25).unwrap() - 0.1).abs() < 1e-6);
        assert!((r.eval(0.75).unwrap() - 0.6).abs() < 1e-6);
    }

    #[test]
    fn step_holds_smooth_eases_cubic_passes_the_keys() {
        assert_eq!(ramp(Interp::Step).eval(0.49), Some(0.0));
        assert_eq!(ramp(Interp::Step).eval(0.5), Some(0.2));
        // Smoothstep: a quarter of the way, 0.15625 of the value.
        let s = ramp(Interp::Smooth).eval(0.125).unwrap();
        assert!((s - 0.2 * 0.15625).abs() < 1e-6, "{s}");
        let c = ramp(Interp::Cubic);
        for (t, v) in [(0.0, 0.0), (0.5, 0.2), (1.0, 1.0)] {
            assert!((c.eval(t).unwrap() - v).abs() < 1e-5);
        }
        assert!(c.eval(0.25).unwrap() > 0.0 && c.eval(0.75).unwrap() > 0.2);
    }

    #[test]
    fn a_key_dragged_stays_between_its_neighbours() {
        let r = ramp(Interp::Linear);
        // The key at 0.5 (index 2) between 0 and 1; the one at 0 below 0.5.
        assert_eq!(between(&r.keys, 2, 1.4), 1.0);
        assert_eq!(between(&r.keys, 1, 0.9), 0.5);
        assert_eq!(between(&r.keys, 2, 0.3), 0.3);
    }

    #[test]
    fn a_colour_ramp_mixes_each_channel() {
        let r = Ramp {
            keys: vec![
                Key {
                    pos: 0.0,
                    value: [0.0, 0.0, 1.0, 1.0],
                },
                Key {
                    pos: 1.0,
                    value: [1.0, 0.0, 0.0, 0.0],
                },
            ],
            interp: Interp::Linear,
        };
        assert_eq!(r.eval(0.5), Some([0.5, 0.0, 0.5, 0.5]));
        assert_eq!(
            Ramp::<f32> {
                keys: vec![],
                interp: Interp::Linear
            }
            .eval(0.5),
            None
        );
    }
}
