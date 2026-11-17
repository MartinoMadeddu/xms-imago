//! Ramps: keys of a value along 0 to 1, and the widget to edit them.
//!
//! A colour ramp is a gradient bar with its keys as handles below it; a float
//! ramp a small curve with its keys as points. Click to add a key (a colour
//! key takes the colour there; a float key the value clicked), drag a key to
//! move it (between the keys either side: they keep their order), right-click
//! it (or Delete with it selected) to take it away. Under it: the
//! interpolation (Step, Linear, Smooth, and Cubic where the target has it),
//! Flip and Reset, then a row per key: its number (selecting its handle), its
//! position, its value (a colour's swatch and RGBA, a float), 🗑. A ramp is one
//! Rust type (`xms_ice::ramp`), mapped to what holds it: MaterialX's `ramp`
//! node (`materialx::params`) and ICE's Ramp node (`ice::ui`).

pub use xms_ice::ramp::{Interp, Key, Ramp, RampValue};

/// What a widget is told: which interpolations the target has, how few keys
/// and how many it takes.
pub struct Limits<'a> {
    pub interps: &'a [Interp],
    pub min_keys: usize,
    pub max_keys: usize,
    /// Whether a colour's alpha is edited (MaterialX's ramp gives it; ICE's
    /// colour ramp gives RGB).
    pub alpha: bool,
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

    // The interpolation, Flip and Reset; then every key: its place and
    // colour.
    let (more, reset) = controls(ui, id, ramp, limits.interps, [0.0, 0.0, 0.0, 1.0], [1.0; 4]);
    changed |= more;
    if reset {
        selected = None;
    }
    let alpha = limits.alpha;
    changed |= key_rows(ui, id, ramp, &mut selected, limits.min_keys, |ui, c| {
        let mut changed = false;
        if alpha {
            let mut v = *c;
            if ui.color_edit_button_rgba_unmultiplied(&mut v).changed() {
                *c = v;
                changed = true;
            }
        } else {
            let mut v = [c[0], c[1], c[2]];
            if ui.color_edit_button_rgb(&mut v).changed() {
                c[..3].copy_from_slice(&v);
                changed = true;
            }
        }
        let shown = if alpha { 4 } else { 3 };
        for x in c.iter_mut().take(shown) {
            changed |= ui
                .add_sized(
                    [46.0, ui.spacing().interact_size.y],
                    egui::DragValue::new(x)
                        .speed(0.01)
                        .max_decimals(3)
                        .update_while_editing(false),
                )
                .changed();
        }
        changed
    });
    ui.data_mut(|d| d.insert_temp(selected_id(id), selected));
    changed
}

/// A float ramp: a curve from `range.0` to `range.1`, its keys as points; the
/// interpolation, and the key selected's position and value. Whether it
/// changed.
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
    let (more, reset) = controls(ui, id, ramp, limits.interps, lo, hi);
    changed |= more;
    if reset {
        selected = None;
    }
    changed |= key_rows(ui, id, ramp, &mut selected, limits.min_keys, |ui, v| {
        ui.add_sized(
            [70.0, ui.spacing().interact_size.y],
            egui::DragValue::new(v)
                .speed(0.01)
                .max_decimals(3)
                .update_while_editing(false),
        )
        .changed()
    });
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

/// The interpolation (of those the target has), Flip (the keys end to end)
/// and Reset (two keys, `from` to `to`). Whether it changed, and whether the
/// keys were remade (their selection then gone).
fn controls<T: RampValue>(
    ui: &mut egui::Ui,
    id: egui::Id,
    ramp: &mut Ramp<T>,
    interps: &[Interp],
    from: T,
    to: T,
) -> (bool, bool) {
    let (mut changed, mut remade) = (false, false);
    ui.horizontal(|ui| {
        changed |= interp_menu(ui, id, &mut ramp.interp, interps);
        if ui.small_button("Flip").on_hover_text("The keys end to end").clicked() {
            for k in ramp.keys.iter_mut() {
                k.pos = 1.0 - k.pos;
            }
            ramp.keys.reverse();
            remade = true;
        }
        if ui
            .small_button("↺ Reset")
            .on_hover_text("Two keys, from start to end")
            .clicked()
        {
            ramp.keys = vec![Key { pos: 0.0, value: from }, Key { pos: 1.0, value: to }];
            remade = true;
        }
    });
    (changed || remade, remade)
}

/// A row per key, in order: its number (click: selected, its handle lit), its
/// position (between the keys either side), its value (`value`: a colour's
/// swatch and RGBA, a float), and 🗑 while there are more than `min_keys`.
fn key_rows<T: RampValue>(
    ui: &mut egui::Ui,
    id: egui::Id,
    ramp: &mut Ramp<T>,
    selected: &mut Option<usize>,
    min_keys: usize,
    mut value: impl FnMut(&mut egui::Ui, &mut T) -> bool,
) -> bool {
    let mut changed = false;
    let mut remove = None;
    egui::Grid::new(id.with("keys"))
        .num_columns(4)
        .spacing([6.0, 2.0])
        .striped(true)
        .show(ui, |ui| {
            for k in 0..ramp.keys.len() {
                let on = *selected == Some(k);
                if ui
                    .selectable_label(on, format!("{}", k + 1))
                    .on_hover_text("Select its handle")
                    .clicked()
                {
                    *selected = if on { None } else { Some(k) };
                }
                let (lo, hi) = (between(&ramp.keys, k, 0.0), between(&ramp.keys, k, 1.0));
                changed |= ui
                    .add_sized(
                        [56.0, ui.spacing().interact_size.y],
                        egui::DragValue::new(&mut ramp.keys[k].pos)
                            .range(lo..=hi)
                            .speed(0.005)
                            .max_decimals(3),
                    )
                    .on_hover_text("Position, 0 to 1: between the keys either side")
                    .changed();
                ui.horizontal(|ui| changed |= value(ui, &mut ramp.keys[k].value));
                let can = ramp.keys.len() > min_keys;
                if ui
                    .add_enabled(can, egui::Button::new("🗑").small())
                    .on_hover_text("Take the key away")
                    .clicked()
                {
                    remove = Some(k);
                }
                ui.end_row();
            }
        });
    if let Some(k) = remove {
        ramp.keys.remove(k);
        *selected = None;
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_dragged_stays_between_its_neighbours() {
        let keys = [
            Key { pos: 1.0, value: 1.0 },
            Key { pos: 0.0, value: 0.0 },
            Key { pos: 0.5, value: 0.2 },
        ];
        // The key at 0.5 (index 2) between 0 and 1; the one at 0 below 0.5.
        assert_eq!(between(&keys, 2, 1.4), 1.0);
        assert_eq!(between(&keys, 1, 0.9), 0.5);
        assert_eq!(between(&keys, 2, 0.3), 0.3);
    }
}
