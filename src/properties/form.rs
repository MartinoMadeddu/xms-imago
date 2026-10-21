//! The layout every node's properties share.
//!
//! A node's parameters are in groups. A group is a card with a title; under
//! it, rows. A row is a label, right-aligned in a column of the same width
//! in every group, and its value to the right. Values fill the width they
//! are given, so sliders, fields and lists line up down the panel.
//!
//! Explanations go in tooltips, on a group's title or a row's label, not in
//! the panel. What the panel shows is the parameters and the state of the
//! node: what came in, what goes out, what went wrong.


/// Colours of the panel. Light theme values; `theme::c` gives the dark
/// counterpart.
#[allow(non_snake_case)]
pub mod ink {
    use egui::Color32;
    pub fn PANEL_BG() -> Color32 { crate::theme::c(118, 118, 118) }
    pub fn GROUP_BG() -> Color32 { crate::theme::c(108, 108, 108) }
    pub fn TITLE() -> Color32 { crate::theme::c(248, 248, 248) }
    pub fn LABEL() -> Color32 { crate::theme::c(222, 222, 222) }
    pub fn VALUE() -> Color32 { crate::theme::c(240, 240, 240) }
    pub fn DIM() -> Color32 { crate::theme::c(200, 200, 200) }
    /// The rail of a slider, so it shows on a group.
    pub fn RAIL() -> Color32 { crate::theme::c(146, 146, 146) }
    /// Fields you type in (text, numbers), with dark fields on (see
    /// `theme::dark_fields`): dark grey, in every theme, with light text.
    /// Buttons, drop-downs and checkboxes keep the theme's.
    pub fn FIELD() -> Color32 { Color32::from_gray(52) }
    pub fn FIELD_HOVER() -> Color32 { Color32::from_gray(66) }
    pub fn FIELD_TEXT() -> Color32 { Color32::from_gray(232) }
    /// A Primitive Location field's background, in the code font: its own
    /// grey between the panel and the other fields; with dark fields on, a
    /// dark field a step lighter.
    pub fn LOCATION() -> Color32 {
        if crate::theme::dark_fields() { Color32::from_gray(70) } else { crate::theme::c(140, 140, 140) }
    }
    /// Done, found, written: green. Dark on a light panel, light on a dark
    /// one, so it reads on the group behind it in every theme.
    pub fn OK() -> Color32 { if light_groups() { Color32::from_rgb(0, 60, 15) } else { Color32::from_rgb(170, 240, 170) } }
    /// Failed, missing: red, likewise.
    pub fn BAD() -> Color32 { if light_groups() { Color32::from_rgb(110, 0, 0) } else { Color32::from_rgb(255, 185, 170) } }

    /// Whether the groups are light enough that dark text reads better on
    /// them than light text: their luminance over 18%.
    pub fn light_groups() -> bool { luminance(GROUP_BG()) > 0.18 }

    /// Relative luminance (Rec. 709, linear light) of a colour.
    pub fn luminance(c: Color32) -> f32 {
        let lin = |v: u8| { let x = v as f32 / 255.0; if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) } };
        0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
    }
}

/// Space between groups.
pub const GAP: f32 = 6.0;

/// How a status line reads.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status { Ok, Bad, Info }

/// Rows of one group.
pub struct Rows<'a> {
    pub ui:  &'a mut egui::Ui,
    label_w: f32,
}

/// Width of the label column: the same in every group of a panel.
fn label_width(ui: &egui::Ui) -> f32 { (ui.available_width() * 0.34).clamp(76.0, 132.0) }

/// A group with a title, and an optional tooltip on the title.
pub fn group(ui: &mut egui::Ui, title: &str, hint: Option<&str>, body: impl FnOnce(&mut Rows)) {
    group_with(ui, title, hint, |_| {}, body);
}

/// A group whose title row also holds small controls, at its right.
pub fn group_with(
    ui: &mut egui::Ui, title: &str, hint: Option<&str>,
    header: impl FnOnce(&mut egui::Ui), body: impl FnOnce(&mut Rows),
) {
    let label_w = label_width(ui);
    egui::Frame::NONE
        .fill(ink::GROUP_BG())
        .inner_margin(egui::Margin { left: 8, right: 8, top: 5, bottom: 7 })
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let r = ui.label(egui::RichText::new(title).strong().size(12.5).color(ink::TITLE()));
                if let Some(h) = hint { r.on_hover_text(h); }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), header);
            });
            ui.add_space(3.0);
            ui.spacing_mut().item_spacing.y = 4.0;
            body(&mut Rows { ui, label_w });
        });
    ui.add_space(GAP);
}

impl<'a> Rows<'a> {
    /// A row: the label, then whatever `value` draws in the value column.
    pub fn row<R>(&mut self, label: &str, hint: Option<&str>, value: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let label_w = self.label_w;
        self.ui.horizontal(|ui| {
            let h = ui.spacing().interact_size.y;
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(label_w, h), egui::Sense::hover());
            let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
            painter.text(rect.right_center() - egui::vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, label,
                egui::FontId::proportional(12.5), ink::LABEL());
            if let Some(hint) = hint { resp.on_hover_text(hint); }
            else if label.len() > 16 { resp.on_hover_text(label); }
            let w = ui.available_width();
            ui.allocate_ui_with_layout(egui::vec2(w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(w);
                value(ui)
            }).inner
        }).inner
    }

    /// Rows that are greyed out and cannot be changed while `on` is false.
    pub fn enabled(&mut self, on: bool, body: impl FnOnce(&mut Rows)) {
        let label_w = self.label_w;
        self.ui.add_enabled_ui(on, |ui| body(&mut Rows { ui, label_w }));
    }

    /// Content across the whole group, without a label.
    pub fn wide<R>(&mut self, add: impl FnOnce(&mut egui::Ui) -> R) -> R { add(self.ui) }

    /// A number on a slider, with its unit.
    pub fn slider(&mut self, label: &str, hint: Option<&str>, v: &mut f32, range: std::ops::RangeInclusive<f32>, unit: &str) -> egui::Response {
        self.row(label, hint, |ui| {
            slider_look(ui);
            ui.add(egui::Slider::new(v, range).suffix(unit).max_decimals(3))
        })
    }

    /// A whole number on a slider.
    pub fn slider_u32(&mut self, label: &str, hint: Option<&str>, v: &mut u32, range: std::ops::RangeInclusive<u32>, unit: &str) -> egui::Response {
        self.row(label, hint, |ui| {
            slider_look(ui);
            ui.add(egui::Slider::new(v, range).suffix(unit))
        })
    }

    /// A number with no fixed range, dragged or typed.
    pub fn drag(&mut self, label: &str, hint: Option<&str>, v: &mut f32, speed: f64, unit: &str) -> egui::Response {
        self.row(label, hint, |ui| {
            let h = ui.spacing().interact_size.y;
            ui.field_sized([ui.available_width(), h], egui::DragValue::new(v).speed(speed).max_decimals(3).suffix(unit))
        })
    }

    /// A whole number, dragged or typed, within a range.
    pub fn drag_u32(&mut self, label: &str, hint: Option<&str>, v: &mut u32, range: std::ops::RangeInclusive<u32>) -> egui::Response {
        self.row(label, hint, |ui| {
            let h = ui.spacing().interact_size.y;
            ui.field_sized([ui.available_width(), h], egui::DragValue::new(v).range(range).speed(0.1))
        })
    }

    /// X, Y and Z side by side.
    pub fn xyz(&mut self, label: &str, hint: Option<&str>, v: &mut [f32; 3], speed: f64, unit: &str) -> bool {
        self.row(label, hint, |ui| xyz_fields(ui, v, speed, unit))
    }

    /// Two values side by side: U and V.
    pub fn uv(&mut self, label: &str, hint: Option<&str>, v: &mut [f32; 2], speed: f64) -> bool {
        self.row(label, hint, |ui| {
            ui.horizontal(|ui| {
                let gap = ui.spacing().item_spacing.x;
                let w = ((ui.available_width() - gap) / 2.0).max(30.0);
                let h = ui.spacing().interact_size.y;
                let mut changed = false;
                for (a, p) in v.iter_mut().zip(["U ", "V "]) {
                    changed |= ui.field_sized([w, h], egui::DragValue::new(a).speed(speed).max_decimals(4).prefix(p)).changed();
                }
                changed
            }).inner
        })
    }

    pub fn toggle(&mut self, label: &str, hint: Option<&str>, v: &mut bool) -> egui::Response {
        self.row(label, hint, |ui| check(ui, v))
    }

    /// One of a few named choices: side by side when they fit, a list when not.
    pub fn choice<T: PartialEq + Clone>(&mut self, label: &str, hint: Option<&str>, v: &mut T, options: &[(T, &str)]) -> bool {
        let id = self.ui.id().with(label);
        self.row(label, hint, |ui| choice_widget(ui, id, v, options))
    }

    pub fn text(&mut self, label: &str, hint: Option<&str>, v: &mut String, placeholder: &str) -> egui::Response {
        self.row(label, hint, |ui| {
            ui.field(egui::TextEdit::singleline(v).hint_text(placeholder).desired_width(ui.available_width()))
        })
    }

    /// A value that is shown, not edited.
    pub fn value(&mut self, label: &str, text: impl Into<String>) {
        let text = text.into();
        self.row(label, None, |ui| ui.add(egui::Label::new(egui::RichText::new(text).color(ink::VALUE())).wrap()));
    }

    /// A line of state in the value column: loaded, missing, solved.
    pub fn status(&mut self, s: Status, text: impl Into<String>) {
        let text = text.into();
        self.row("", None, |ui| status_label(ui, s, &text));
    }

    /// Buttons in the value column.
    pub fn buttons<R>(&mut self, label: &str, hint: Option<&str>, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        self.row(label, hint, |ui| ui.horizontal_wrapped(add).inner)
    }
}

/// A field you type in (a `TextEdit` or `DragValue`): `ui.field(..)` where
/// `ui.add(..)` would be. With dark fields on, dark grey with light text;
/// off (the default), as any field of the theme. Only the field itself:
/// buttons, drop-downs and checkboxes beside it keep the theme's look.
pub trait FieldExt {
    fn field(&mut self, widget: impl egui::Widget) -> egui::Response;
    /// `field`, at a given size.
    fn field_sized(&mut self, size: impl Into<egui::Vec2>, widget: impl egui::Widget) -> egui::Response;
}

impl FieldExt for egui::Ui {
    fn field(&mut self, widget: impl egui::Widget) -> egui::Response {
        if !crate::theme::dark_fields() { return self.add(widget); }
        self.scope(|ui| { field_look(ui); ui.add(widget) }).inner
    }
    fn field_sized(&mut self, size: impl Into<egui::Vec2>, widget: impl egui::Widget) -> egui::Response {
        if !crate::theme::dark_fields() { return self.add_sized(size, widget); }
        self.scope(|ui| { field_look(ui); ui.add_sized(size, widget) }).inner
    }
}

/// The look of a field you type in: dark grey with light text, a light
/// text cursor, lighter when hovered or being edited. Used by `field`, in a
/// scope of its own, so nothing else drawn after it takes it.
fn field_look(ui: &mut egui::Ui) {
    let v = ui.visuals_mut();
    v.extreme_bg_color = ink::FIELD();
    v.text_cursor.stroke.color = ink::FIELD_TEXT();
    for w in [&mut v.widgets.inactive, &mut v.widgets.open] {
        w.bg_fill = ink::FIELD();
        w.weak_bg_fill = ink::FIELD();
        w.fg_stroke.color = ink::FIELD_TEXT();
    }
    for w in [&mut v.widgets.hovered, &mut v.widgets.active] {
        w.bg_fill = ink::FIELD_HOVER();
        w.weak_bg_fill = ink::FIELD_HOVER();
        w.fg_stroke.color = egui::Color32::WHITE;
    }
    // Hints inside the field, light too.
    v.widgets.noninteractive.fg_stroke.color = ink::FIELD_TEXT();
}

/// A checkbox whose box shows on the group's background in every theme.
pub fn check(ui: &mut egui::Ui, v: &mut bool) -> egui::Response {
    let w = &mut ui.visuals_mut().widgets;
    for s in [&mut w.inactive, &mut w.hovered] {
        s.bg_stroke = egui::Stroke::new(1.0, ink::LABEL());
    }
    w.inactive.bg_fill = ink::PANEL_BG();
    ui.add(egui::Checkbox::without_text(v))
}

/// Sliders fill the value column, with a rail that shows on the group.
pub fn slider_look(ui: &mut egui::Ui) {
    ui.spacing_mut().slider_width = (ui.available_width() - 66.0).max(40.0);
    ui.visuals_mut().widgets.inactive.bg_fill = ink::RAIL();
}

pub fn status_label(ui: &mut egui::Ui, s: Status, text: &str) -> egui::Response {
    let (mark, col) = match s { Status::Ok => ("✔ ", ink::OK()), Status::Bad => ("× ", ink::BAD()), Status::Info => ("", ink::DIM()) };
    ui.add(egui::Label::new(egui::RichText::new(format!("{mark}{text}")).color(col)).wrap())
}

/// X, Y, Z fields filling the width.
pub fn xyz_fields(ui: &mut egui::Ui, v: &mut [f32; 3], speed: f64, unit: &str) -> bool {
    ui.horizontal(|ui| {
        let gap = ui.spacing().item_spacing.x;
        let w = ((ui.available_width() - 2.0 * gap) / 3.0).max(30.0);
        let h = ui.spacing().interact_size.y;
        let mut changed = false;
        for (a, p) in v.iter_mut().zip(["X ", "Y ", "Z "]) {
            changed |= ui.field_sized([w, h], egui::DragValue::new(a).speed(speed).max_decimals(3).prefix(p).suffix(unit)).changed();
        }
        changed
    }).inner
}

/// Side-by-side buttons for up to four short choices, else a drop-down.
pub fn choice_widget<T: PartialEq + Clone>(ui: &mut egui::Ui, id: egui::Id, v: &mut T, options: &[(T, &str)]) -> bool {
    let chars: usize = options.iter().map(|o| o.1.len()).sum();
    let fits = options.len() <= 4 && (chars as f32) * 7.5 + options.len() as f32 * 18.0 < ui.available_width();
    let before = options.iter().position(|o| o.0 == *v);
    if fits {
        ui.horizontal(|ui| {
            for (t, name) in options {
                if ui.selectable_label(*v == *t, *name).clicked() { *v = t.clone(); }
            }
        });
    } else {
        let current = options.iter().find(|o| o.0 == *v).map(|o| o.1).unwrap_or("");
        egui::ComboBox::from_id_salt(id).width(ui.available_width() - 8.0).selected_text(current).show_ui(ui, |ui| {
            for (t, name) in options {
                if ui.selectable_label(*v == *t, *name).clicked() { *v = t.clone(); }
            }
        });
    }
    options.iter().position(|o| o.0 == *v) != before
}

#[cfg(test)]
mod tests {
    use super::ink::luminance;
    use egui::Color32;

    /// Contrast as WCAG has it: (lighter + 0.05) / (darker + 0.05).
    fn contrast(a: Color32, b: Color32) -> f32 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// The status colours read on the greys the groups take, light or dark:
    /// 2.75 to 1 at the least, at the middle grey where dark and light read
    /// alike, far more away from it; the old ones were under 2 on light greys.
    #[test]
    fn status_colours_read_on_the_groups() {
        let pick = |bg: Color32, dark: Color32, light: Color32| if luminance(bg) > 0.18 { dark } else { light };
        for g in [40u8, 60, 80, 100, 108, 118, 125, 130, 150, 170, 190, 210] {
            let bg = Color32::from_gray(g);
            let ok = pick(bg, Color32::from_rgb(0, 60, 15), Color32::from_rgb(170, 240, 170));
            let bad = pick(bg, Color32::from_rgb(110, 0, 0), Color32::from_rgb(255, 185, 170));
            assert!(contrast(ok, bg) >= 2.75, "green on grey {g}: {}", contrast(ok, bg));
            assert!(contrast(bad, bg) >= 2.75, "red on grey {g}: {}", contrast(bad, bg));
        }
    }
}
