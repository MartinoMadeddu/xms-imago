//! The layout every node's properties share.
//!
//! A node's parameters are in groups. A group is a card with a title; under it,
//! rows. A row is a label, right-aligned in a column of the same width in every
//! group, and its value to the right. Values fill the width they are given, so
//! sliders, fields and lists line up down the panel.
//!
//! Explanations go in tooltips, on a group's title or a row's label, not in the
//! panel. What the panel shows is the parameters and the state of the node:
//! what came in, what goes out, what went wrong.

/// Colours of the panel. Light theme values; `theme::c` gives the dark
/// counterpart.
#[allow(non_snake_case)]
pub mod ink {
    use egui::Color32;
    pub fn PANEL_BG() -> Color32 {
        crate::theme::c(118, 118, 118)
    }
    pub fn GROUP_BG() -> Color32 {
        crate::theme::c(108, 108, 108)
    }
    pub fn TITLE() -> Color32 {
        crate::theme::c(248, 248, 248)
    }
    pub fn LABEL() -> Color32 {
        crate::theme::c(222, 222, 222)
    }
    pub fn VALUE() -> Color32 {
        crate::theme::c(240, 240, 240)
    }
    pub fn DIM() -> Color32 {
        crate::theme::c(200, 200, 200)
    }
    /// The rail of a slider, so it shows on a group.
    pub fn RAIL() -> Color32 {
        crate::theme::c(146, 146, 146)
    }
    /// Fields you type in (text, numbers), with dark fields on (see
    /// `theme::dark_fields`): dark grey, in every theme, with light text.
    /// Buttons, drop-downs and checkboxes keep the theme's.
    pub fn FIELD() -> Color32 {
        Color32::from_gray(52)
    }
    pub fn FIELD_HOVER() -> Color32 {
        Color32::from_gray(66)
    }
    pub fn FIELD_TEXT() -> Color32 {
        Color32::from_gray(232)
    }
    /// A Primitive Location field's background, in the code font: its own grey
    /// between the panel and the other fields; with dark fields on, a dark
    /// field a step lighter.
    pub fn LOCATION() -> Color32 {
        if crate::theme::dark_fields() {
            Color32::from_gray(70)
        } else {
            crate::theme::c(140, 140, 140)
        }
    }
    /// Done, found, written: green. Dark on a light panel, light on a dark one,
    /// so it reads on the group behind it in every theme.
    pub fn OK() -> Color32 {
        if light_groups() {
            Color32::from_rgb(0, 60, 15)
        } else {
            Color32::from_rgb(170, 240, 170)
        }
    }
    /// Failed, missing: red, likewise.
    pub fn BAD() -> Color32 {
        if light_groups() {
            Color32::from_rgb(110, 0, 0)
        } else {
            Color32::from_rgb(255, 185, 170)
        }
    }

    /// Whether the groups are light enough that dark text reads better on them
    /// than light text: their luminance over 18%.
    pub fn light_groups() -> bool {
        luminance(GROUP_BG()) > 0.18
    }

    /// Relative luminance (Rec. 709, linear light) of a colour.
    pub fn luminance(c: Color32) -> f32 {
        let lin = |v: u8| {
            let x = v as f32 / 255.0;
            if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
    }
}

/// Space between groups.
pub const GAP: f32 = 6.0;

/// How a status line reads.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Bad,
    Info,
}

/// Rows of one group.
pub struct Rows<'a> {
    pub ui: &'a mut egui::Ui,
    label_w: f32,
}

/// Width of the label column: the same in every group of a panel.
fn label_width(ui: &egui::Ui) -> f32 {
    (ui.available_width() * 0.34).clamp(76.0, 132.0)
}

/// A group with a title, and an optional tooltip on the title.
pub fn group(ui: &mut egui::Ui, title: &str, hint: Option<&str>, body: impl FnOnce(&mut Rows)) {
    group_with(ui, title, hint, |_| {}, body);
}

/// A group whose title row also holds small controls, at its right.
pub fn group_with(
    ui: &mut egui::Ui,
    title: &str,
    hint: Option<&str>,
    header: impl FnOnce(&mut egui::Ui),
    body: impl FnOnce(&mut Rows),
) {
    let label_w = label_width(ui);
    egui::Frame::NONE
        .fill(ink::GROUP_BG())
        .inner_margin(egui::Margin {
            left: 8,
            right: 8,
            top: 5,
            bottom: 7,
        })
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let r = ui.label(egui::RichText::new(title).strong().size(12.5).color(ink::TITLE()));
                if let Some(h) = hint {
                    r.on_hover_text(h);
                }
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
        self.ui
            .horizontal(|ui| {
                let h = ui.spacing().interact_size.y;
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(label_w, h), egui::Sense::hover());
                let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                painter.text(
                    rect.right_center() - egui::vec2(8.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    label,
                    egui::FontId::proportional(12.5),
                    ink::LABEL(),
                );
                if let Some(hint) = hint {
                    resp.on_hover_text(hint);
                } else if label.len() > 16 {
                    resp.on_hover_text(label);
                }
                let w = ui.available_width();
                ui.allocate_ui_with_layout(egui::vec2(w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(w);
                    value(ui)
                })
                .inner
            })
            .inner
    }

    /// Rows that are greyed out and cannot be changed while `on` is false.
    pub fn enabled(&mut self, on: bool, body: impl FnOnce(&mut Rows)) {
        let label_w = self.label_w;
        self.ui.add_enabled_ui(on, |ui| body(&mut Rows { ui, label_w }));
    }

    /// Content across the whole group, without a label.
    pub fn wide<R>(&mut self, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        add(self.ui)
    }

    /// A number on a slider, with its unit.
    pub fn slider(
        &mut self,
        label: &str,
        hint: Option<&str>,
        v: &mut f32,
        range: std::ops::RangeInclusive<f32>,
        unit: &str,
    ) -> egui::Response {
        self.row(label, hint, |ui| {
            slider_look(ui);
            let mut r = ui.add(egui::Slider::new(&mut *v, range).suffix(unit).max_decimals(3));
            probe(&mut r, v);
            r
        })
    }

    /// A whole number on a slider.
    pub fn slider_u32(
        &mut self,
        label: &str,
        hint: Option<&str>,
        v: &mut u32,
        range: std::ops::RangeInclusive<u32>,
        unit: &str,
    ) -> egui::Response {
        self.row(label, hint, |ui| {
            slider_look(ui);
            let mut r = ui.add(egui::Slider::new(&mut *v, range).suffix(unit));
            probe(&mut r, v);
            r
        })
    }

    /// A number with no fixed range, dragged or typed.
    pub fn drag(&mut self, label: &str, hint: Option<&str>, v: &mut f32, speed: f64, unit: &str) -> egui::Response {
        self.row(label, hint, |ui| {
            let h = ui.spacing().interact_size.y;
            let mut r = ui.field_sized(
                [ui.available_width(), h],
                egui::DragValue::new(&mut *v).speed(speed).max_decimals(3).suffix(unit),
            );
            probe(&mut r, v);
            r
        })
    }

    /// A whole number, dragged or typed, within a range.
    pub fn drag_u32(
        &mut self,
        label: &str,
        hint: Option<&str>,
        v: &mut u32,
        range: std::ops::RangeInclusive<u32>,
    ) -> egui::Response {
        self.row(label, hint, |ui| {
            let h = ui.spacing().interact_size.y;
            let mut r = ui.field_sized(
                [ui.available_width(), h],
                egui::DragValue::new(&mut *v).range(range).speed(0.1),
            );
            probe(&mut r, v);
            r
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
                    let mut r = ui.field_sized(
                        [w, h],
                        egui::DragValue::new(&mut *a).speed(speed).max_decimals(4).prefix(p),
                    );
                    probe(&mut r, a);
                    changed |= r.changed();
                }
                changed
            })
            .inner
        })
    }

    pub fn toggle(&mut self, label: &str, hint: Option<&str>, v: &mut bool) -> egui::Response {
        self.row(label, hint, |ui| check(ui, v))
    }

    /// One of a few named choices: side by side when they fit, a list when not.
    pub fn choice<T: PartialEq + Clone>(
        &mut self,
        label: &str,
        hint: Option<&str>,
        v: &mut T,
        options: &[(T, &str)],
    ) -> bool {
        let id = self.ui.id().with(label);
        self.row(label, hint, |ui| choice_widget(ui, id, v, options))
    }

    pub fn text(&mut self, label: &str, hint: Option<&str>, v: &mut String, placeholder: &str) -> egui::Response {
        self.row(label, hint, |ui| {
            ui.field(
                egui::TextEdit::singleline(v)
                    .hint_text(placeholder)
                    .desired_width(ui.available_width()),
            )
        })
    }

    /// `text` for text whose variables are put in where it is used (a primitive
    /// location, a file): typing `$` lists them, and what it becomes shows
    /// under it when that differs. `keep_frames`: the frame's are left for the
    /// renderer (an Output Image).
    pub fn text_vars(
        &mut self,
        label: &str,
        hint: Option<&str>,
        v: &mut String,
        placeholder: &str,
        keep_frames: bool,
    ) -> egui::Response {
        let r = self.row(label, hint, |ui| {
            let r = ui.field(
                egui::TextEdit::singleline(&mut *v)
                    .hint_text(placeholder)
                    .desired_width(ui.available_width()),
            );
            crate::variables::complete(ui, &r, v, &[]);
            r
        });
        if v.contains('$') || v.contains('{') {
            let shown = if keep_frames {
                crate::core::vars::expand_keeping_frames(v.trim())
            } else {
                crate::core::vars::expand(v.trim(), &[])
            };
            if !shown.is_empty() && shown != v.trim() {
                self.status(Status::Info, format!("→ {shown}"));
            }
        }
        r
    }

    /// A value that is shown, not edited.
    pub fn value(&mut self, label: &str, text: impl Into<String>) {
        let text = text.into();
        self.row(label, None, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(text).color(ink::VALUE())).wrap())
        });
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
/// `ui.add(..)` would be. With dark fields on, dark grey with light text; off
/// (the default), as any field of the theme. Only the field itself: buttons,
/// drop-downs and checkboxes beside it keep the theme's look.
pub trait FieldExt {
    fn field(&mut self, widget: impl egui::Widget) -> egui::Response;
    /// `field`, at a given size.
    fn field_sized(&mut self, size: impl Into<egui::Vec2>, widget: impl egui::Widget) -> egui::Response;
}

impl FieldExt for egui::Ui {
    fn field(&mut self, widget: impl egui::Widget) -> egui::Response {
        if !crate::theme::dark_fields() {
            return self.add(widget);
        }
        self.scope(|ui| {
            field_look(ui);
            ui.add(widget)
        })
        .inner
    }
    fn field_sized(&mut self, size: impl Into<egui::Vec2>, widget: impl egui::Widget) -> egui::Response {
        if !crate::theme::dark_fields() {
            return self.add_sized(size, widget);
        }
        self.scope(|ui| {
            field_look(ui);
            ui.add_sized(size, widget)
        })
        .inner
    }
}

/// The look of a field you type in: dark grey with light text, a light text
/// cursor, lighter when hovered or being edited. Used by `field`, in a scope of
/// its own, so nothing else drawn after it takes it.
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

/// Tabs over a panel. Each tab is its label and an optional tooltip; a click
/// opens one (`open`), and the responses are returned for drops on a tab. Drawn
/// as the panes' tabs (`theme::dock_style`): side by side across the width, the
/// open one joined to the page; or, with the colour editor's "1995 tabs", as
/// index-card tabs (`tabs_1995`).
pub fn tabs(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    labels: &[(&str, Option<&str>)],
    open: &mut usize,
) -> Vec<egui::Response> {
    if crate::theme::tabs_1995() {
        tabs_1995(ui, id, labels, open)
    } else {
        pane_tabs(ui, id, labels, open)
    }
}

/// `tabs` as the panes' tab bars draw theirs: the same colours, outline, height
/// and text, each tab as wide as its title (as 1995's are, not sharing the
/// width), a line under those not open and on to the end.
fn pane_tabs(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    labels: &[(&str, Option<&str>)],
    open: &mut usize,
) -> Vec<egui::Response> {
    use egui::{pos2, vec2, Rect, Sense, Stroke, StrokeKind, TextStyle, WidgetText};
    let dock = crate::theme::dock_style(ui.style());
    let id = ui.id().with(("form_tabs", id));
    let h = dock.tab_bar.height;
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    const PAD: f32 = 10.0; // text to the tab's sides, as 1995's
    let galleys: Vec<_> = labels
        .iter()
        .map(|(t, _)| WidgetText::from(*t).into_galley(ui, None, f32::INFINITY, TextStyle::Button))
        .collect();
    let mut x = bar.left();
    let rects: Vec<Rect> = galleys
        .iter()
        .map(|g| {
            let r = Rect::from_min_size(pos2(x, bar.top()), vec2(g.size().x + 2.0 * PAD, h));
            x = r.right();
            r
        })
        .collect();
    let responses: Vec<egui::Response> = rects
        .iter()
        .enumerate()
        .map(|(k, r)| {
            let r = ui.interact(*r, id.with(k), Sense::click());
            match labels[k].1 {
                Some(h) => r.on_hover_text(h),
                None => r,
            }
        })
        .collect();
    if let Some(k) = responses.iter().position(|r| r.clicked()) {
        *open = k;
    }
    let painter = ui.painter_at(bar);
    let px = ui.ctx().pixels_per_point().recip();
    for (k, r) in rects.iter().enumerate() {
        let opened = k == *open;
        let st = if opened {
            &dock.tab.active
        } else if responses[k].hovered() {
            &dock.tab.hovered
        } else {
            &dock.tab.inactive
        };
        painter.rect_filled(*r, st.corner_radius, st.bg_fill);
        painter.rect_stroke(
            *r,
            st.corner_radius,
            Stroke::new(1.0, st.outline_color),
            StrokeKind::Inside,
        );
        if opened {
            // Joined to the page under it.
            painter.hline(
                (r.left() + 1.5)..=(r.right() - 1.5),
                r.bottom() - 0.5,
                Stroke::new(2.0, st.bg_fill),
            );
        } else {
            painter.hline(
                r.x_range(),
                bar.bottom() - px,
                Stroke::new(px, dock.tab_bar.hline_color),
            );
        }
        let g = galleys[k].clone();
        painter.galley_with_override_text_color(r.center() - g.size() / 2.0, g, st.text_color);
    }
    // The bar's line on from the last tab to the pane's edge.
    if x < bar.right() {
        painter.hline(
            x..=bar.right(),
            bar.bottom() - px,
            Stroke::new(px, dock.tab_bar.hline_color),
        );
    }
    ui.add_space(4.0);
    responses
}

/// `tabs` as index-card tabs: the open one raised, in front of the others and
/// joined to the page below it, with a blue band along its top; the others set
/// back behind it, lighter under the pointer.
fn tabs_1995(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    labels: &[(&str, Option<&str>)],
    open: &mut usize,
) -> Vec<egui::Response> {
    use egui::{pos2, vec2, Color32, CornerRadius, FontId, Rect, Sense, Stroke};
    const PAD: f32 = 10.0; // text to the tab's sides
    const H: f32 = 22.0; // the open tab's height
    const BACK: f32 = 3.0; // how much lower the closed tabs start
    const OVER: f32 = 2.0; // how far the open tab reaches over its neighbours
    const CUT: f32 = 3.0; // the corners' cut
    let id = ui.id().with(("form_tabs", id));
    let font = FontId::proportional(12.5);
    let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), H + 1.0), Sense::hover());
    let base = strip.bottom() - 1.0;
    // Each tab where it sits when closed.
    let galleys: Vec<_> = labels
        .iter()
        .map(|(t, _)| ui.painter().layout_no_wrap(t.to_string(), font.clone(), ink::LABEL()))
        .collect();
    let mut x = strip.left() + OVER;
    let rects: Vec<Rect> = galleys
        .iter()
        .map(|g| {
            let w = g.size().x + 2.0 * PAD;
            let r = Rect::from_min_max(pos2(x, strip.top() + BACK), pos2(x + w, base));
            x += w;
            r
        })
        .collect();
    let responses: Vec<egui::Response> = rects
        .iter()
        .enumerate()
        .map(|(k, r)| {
            let r = ui.interact(*r, id.with(k), Sense::click());
            match labels[k].1 {
                Some(h) => r.on_hover_text(h),
                None => r,
            }
        })
        .collect();
    if let Some(k) = responses.iter().position(|r| r.clicked()) {
        *open = k;
    }
    let open_now = (*open).min(labels.len().saturating_sub(1));

    // The page's colour, and the bevel's: light above and to the left,
    // shadow and dark to the right, as raised things were drawn.
    let page = ink::PANEL_BG();
    let light = page.lerp_to_gamma(Color32::WHITE, 0.45);
    let shadow = page.lerp_to_gamma(Color32::BLACK, 0.3);
    let dark = page.lerp_to_gamma(Color32::BLACK, 0.6);
    let blue = ui.visuals().selection.bg_fill;
    let painter = ui.painter_at(strip.expand(1.0));
    let line = |a: egui::Pos2, b: egui::Pos2, c: Color32| {
        painter.line_segment([a, b], Stroke::new(1.0, c));
    };
    let draw = |k: usize| {
        let opened = k == open_now;
        let r = if opened {
            Rect::from_min_max(
                pos2(rects[k].left() - OVER, strip.top()),
                pos2(rects[k].right() + OVER, base + 1.0),
            )
        } else {
            rects[k]
        };
        let fill = if opened {
            page
        } else if responses[k].hovered() {
            page.lerp_to_gamma(Color32::WHITE, 0.1)
        } else {
            page.lerp_to_gamma(Color32::BLACK, 0.12)
        };
        painter.rect_filled(
            r,
            CornerRadius {
                nw: CUT as u8,
                ne: CUT as u8,
                sw: 0,
                se: 0,
            },
            fill,
        );
        let (l, t, rt, b) = (r.left() + 0.5, r.top() + 0.5, r.right() - 0.5, r.bottom());
        line(pos2(l, b), pos2(l, t + CUT), light);
        line(pos2(l, t + CUT), pos2(l + CUT, t), light);
        line(pos2(l + CUT, t), pos2(rt - CUT, t), light);
        line(pos2(rt - CUT, t), pos2(rt, t + CUT), dark);
        line(pos2(rt, t + CUT), pos2(rt, b), dark);
        line(pos2(rt - 1.0, t + CUT), pos2(rt - 1.0, b), shadow);
        if opened {
            painter.rect_filled(
                Rect::from_min_max(pos2(l + 2.0, t + 1.5), pos2(rt - 2.0, t + 4.0)),
                CornerRadius::same(1),
                blue,
            );
        }
        let g = galleys[k].clone();
        let at = r.center() - g.size() / 2.0 + vec2(0.0, if opened { 1.0 } else { 0.0 });
        painter.galley_with_override_text_color(at, g, if opened { ink::TITLE() } else { ink::LABEL() });
    };
    // The closed tabs behind, the page's edge, then the open tab over both.
    for k in (0..labels.len()).filter(|k| *k != open_now) {
        draw(k);
    }
    line(pos2(strip.left(), base + 0.5), pos2(strip.right(), base + 0.5), light);
    if !labels.is_empty() {
        draw(open_now);
    }
    ui.add_space(4.0);
    responses
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
    let (mark, col) = match s {
        Status::Ok => ("✔ ", ink::OK()),
        Status::Bad => ("× ", ink::BAD()),
        Status::Info => ("", ink::DIM()),
    };
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
            let mut r = ui.field_sized(
                [w, h],
                egui::DragValue::new(&mut *a)
                    .speed(speed)
                    .max_decimals(3)
                    .prefix(p)
                    .suffix(unit),
            );
            probe(&mut r, a);
            changed |= r.changed();
        }
        changed
    })
    .inner
}

/// Side-by-side buttons for up to four short choices, else a drop-down.
pub fn choice_widget<T: PartialEq + Clone>(ui: &mut egui::Ui, id: egui::Id, v: &mut T, options: &[(T, &str)]) -> bool {
    let chars: usize = options.iter().map(|o| o.1.len()).sum();
    let fits = options.len() <= 4 && (chars as f32) * 7.5 + options.len() as f32 * 18.0 < ui.available_width();
    let before = options.iter().position(|o| o.0 == *v);
    if fits {
        ui.horizontal(|ui| {
            for (t, name) in options {
                if ui.selectable_label(*v == *t, *name).clicked() {
                    *v = t.clone();
                }
            }
        });
    } else {
        let current = options.iter().find(|o| o.0 == *v).map(|o| o.1).unwrap_or("");
        egui::ComboBox::from_id_salt(id)
            .width(ui.available_width() - 8.0)
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (t, name) in options {
                    if ui.selectable_label(*v == *t, *name).clicked() {
                        *v = t.clone();
                    }
                }
            });
    }
    options.iter().position(|o| o.0 == *v) != before
}

// Finding a row's number in the operator's parameters

/// What the parameters a right-click on a number's row stands for (see
/// `probing`): no operator's number is likely to be it.
pub const PROBE: f64 = 7_777_777.0;

thread_local! {
    /// Probing on, and the number right-clicked: its value before, and where
    /// the pointer was.
    static PROBING: std::cell::RefCell<(bool, Option<(f64, egui::Pos2)>)> =
        const { std::cell::RefCell::new((false, None)) };
}

/// While on, a right-click on a number's row (a slider, a drag field, X Y Z,
/// U V) puts `PROBE` in its place for the caller to find the number's path in
/// the operator's parameters, then put the value back (`probed`). On only
/// while an operator's own parameters are drawn: never for an ICE node's.
pub fn probing(on: bool) {
    PROBING.with(|p| *p.borrow_mut() = (on, None));
}

/// The number right-clicked while probing: its value before, where the pointer
/// was. Probing stops.
pub fn probed() -> Option<(f64, egui::Pos2)> {
    PROBING.with(|p| std::mem::replace(&mut *p.borrow_mut(), (false, None)).1)
}

/// A number a row edits: written as `PROBE` and read back.
pub trait Probed {
    fn get(&self) -> f64;
    fn put(&mut self, x: f64);
}

impl Probed for f32 {
    fn get(&self) -> f64 {
        *self as f64
    }
    fn put(&mut self, x: f64) {
        *self = x as f32;
    }
}

impl Probed for u32 {
    fn get(&self) -> f64 {
        *self as f64
    }
    fn put(&mut self, x: f64) {
        *self = x as u32;
    }
}

/// A row's number right-clicked, while probing: `PROBE` in its place, marked
/// changed so a caller that copies the number writes it back.
fn probe(r: &mut egui::Response, v: &mut impl Probed) {
    if !r.secondary_clicked() {
        return;
    }
    let at = r.interact_pointer_pos().unwrap_or(r.rect.left_bottom());
    PROBING.with(|p| {
        let mut p = p.borrow_mut();
        if p.0 && p.1.is_none() {
            p.1 = Some((v.get(), at));
            v.put(PROBE);
            r.mark_changed();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// While probing, the first number right-clicked is taken and probing
    /// stops; not probing, nothing is.
    #[test]
    fn probing_takes_one_number() {
        probing(true);
        PROBING.with(|p| p.borrow_mut().1 = Some((2.0, egui::pos2(1.0, 1.0))));
        assert_eq!(probed(), Some((2.0, egui::pos2(1.0, 1.0))));
        assert_eq!(probed(), None);
        let mut x = 3.0f32;
        x.put(PROBE);
        assert_eq!(x.get(), PROBE, "the probe is a number a float holds exactly");
        let mut n = 4u32;
        n.put(PROBE);
        assert_eq!(n.get(), PROBE);
    }
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
