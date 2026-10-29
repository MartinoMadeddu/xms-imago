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
            slider(ui, v, |v| egui::Slider::new(v, range).suffix(unit).max_decimals(3))
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
            slider(ui, v, |v| egui::Slider::new(v, range).suffix(unit))
        })
    }

    /// A number with no fixed range, dragged or typed.
    pub fn drag(&mut self, label: &str, hint: Option<&str>, v: &mut f32, speed: f64, unit: &str) -> egui::Response {
        self.row(label, hint, |ui| {
            let size = [ui.available_width(), ui.spacing().interact_size.y];
            number(ui, Some(size), v, |d| d.speed(speed).max_decimals(3).suffix(unit))
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
            let size = [ui.available_width(), ui.spacing().interact_size.y];
            number(ui, Some(size), v, |d| d.range(range).speed(0.1))
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
                    changed |= number(ui, Some([w, h]), a, |d| d.speed(speed).max_decimals(4).prefix(p)).changed();
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

    /// Text, changed on Enter or leaving the field (`typed`).
    pub fn text(&mut self, label: &str, hint: Option<&str>, v: &mut String, placeholder: &str) -> egui::Response {
        self.row(label, hint, |ui| {
            typed(ui, v, |ui, t| {
                ui.field(
                    egui::TextEdit::singleline(t)
                        .hint_text(placeholder)
                        .desired_width(ui.available_width()),
                )
            })
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
            typed(ui, v, |ui, t| {
                let r = ui.field(
                    egui::TextEdit::singleline(&mut *t)
                        .hint_text(placeholder)
                        .desired_width(ui.available_width()),
                );
                crate::variables::complete(ui, &r, t, &[]);
                r
            })
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

/// A text field over `v` (drawn by `add` over the text being typed) that
/// changes `v` on Enter or on leaving the field, not as each letter is typed,
/// so what it sets off (a cook) happens once; Escape leaves `v` as it was.
/// Its response is changed only then. A change made to the text while the
/// field is not being typed in (a variable's name picked from the list under
/// it) is kept at once.
pub fn typed(
    ui: &mut egui::Ui,
    v: &mut String,
    add: impl FnOnce(&mut egui::Ui, &mut String) -> egui::Response,
) -> egui::Response {
    let row = ui.next_auto_id();
    let key = row.with("typed");
    let draft: Option<String> = ui.data(|d| d.get_temp(key));
    let mut text = draft.clone().unwrap_or_else(|| v.clone());
    let start = text.clone();
    let mut r = add(ui, &mut text);
    r.flags.remove(egui::response::Flags::CHANGED);
    // Text that follows another: an orange edge.
    if follows(row) {
        ui.painter().rect_stroke(
            r.rect.expand(1.0),
            3.0,
            egui::Stroke::new(2.0, follows_orange()),
            egui::StrokeKind::Outside,
        );
    }
    // Probed (see `probing`): the probe text in its place.
    if probe_row(ui, &r, 0.0, Some(v.as_str()), row) {
        ui.memory_mut(|m| m.surrender_focus(r.id));
        ui.data_mut(|d| d.remove::<String>(key));
        *v = PROBE_TEXT.to_string();
        r.mark_changed();
        return r;
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(key, text));
        return r;
    }
    if draft.is_some() {
        ui.data_mut(|d| d.remove::<String>(key));
    }
    let keep = if r.lost_focus() {
        !ui.input(|i| i.key_pressed(egui::Key::Escape))
    } else {
        text != start
    };
    if keep && text != *v {
        *v = text;
        r.mark_changed();
    }
    r
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
            let r = number(ui, Some([w, h]), a, |d| {
                d.speed(speed).max_decimals(3).prefix(p).suffix(unit)
            });
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

/// What a text row's text stands for while probed, as `PROBE` for a number's:
/// letters and digits only, so a row that makes its text a path or a name
/// keeps them (found in what it makes, in any case).
pub const PROBE_TEXT: &str = "xmsprobe7777777";

/// Finding which of an operator's numbers a row shows.
#[derive(Default)]
struct Probing {
    on: bool,
    /// The number right-clicked: its value before, where the pointer was.
    clicked: Option<(f64, egui::Pos2)>,
    /// A text row right-clicked: its text before.
    text: Option<String>,
    /// The row being found this frame, while nothing is clicked.
    finding: Option<egui::Id>,
    /// What the rows are known for: the operator and its references.
    whose: u64,
    /// Each row's number, by the row's key: its path, None for a row with
    /// none found.
    rows: std::collections::HashMap<egui::Id, Option<String>>,
    /// The numbers that follow another (references), drawn orange.
    linked: Vec<String>,
}

thread_local! {
    static PROBING: std::cell::RefCell<Probing> = std::cell::RefCell::new(Probing::default());
}

/// A row probed this frame: right-clicked (its value before, where the
/// pointer was), or being found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Probe {
    Clicked(f64, egui::Pos2),
    Finding(egui::Id),
}

/// While on, a right-click on a number's row (a slider, a drag field, X Y Z,
/// U V) puts `PROBE` in its place for the caller to find the number's path in
/// the operator's parameters, then put the value back (`probed`). And while
/// numbers of the operator follow others (`linked`), one row a frame not yet
/// known is probed the same way, untouched, so the rows of those numbers are
/// found and drawn orange. On only while an operator's own parameters are
/// drawn: never for an ICE node's. The rows found are kept while `whose` (the
/// operator and its references) is the same.
pub fn probing(on: bool, whose: u64, linked: Vec<String>) {
    PROBING.with(|p| {
        let mut p = p.borrow_mut();
        if p.whose != whose {
            p.rows.clear();
            p.whose = whose;
        }
        p.on = on;
        p.clicked = None;
        p.text = None;
        p.finding = None;
        p.linked = linked;
    });
}

/// The row probed this frame. Probing stops.
pub fn probed() -> Option<Probe> {
    PROBING.with(|p| {
        let mut p = p.borrow_mut();
        p.on = false;
        let clicked = p.clicked.take().map(|(v, at)| Probe::Clicked(v, at));
        clicked.or(p.finding.take().map(Probe::Finding))
    })
}

/// The text of the text row right-clicked this frame (see `probed`), when it
/// was a text row.
pub fn probed_text() -> Option<String> {
    PROBING.with(|p| p.borrow().text.clone())
}

/// Whether a row's number is `PROBE` this frame, put back after the panel:
/// what a change to it would set off (a Merge making its inputs) waits.
pub fn probe_put() -> bool {
    PROBING.with(|p| {
        let p = p.borrow();
        p.clicked.is_some() || p.finding.is_some()
    })
}

/// The number the row being found shows: its path, or None.
pub fn found(row: egui::Id, path: Option<String>) {
    PROBING.with(|p| {
        p.borrow_mut().rows.insert(row, path);
    });
}

/// Whether the row with key `row`, while probing, shows a number that follows
/// another.
fn follows(row: egui::Id) -> bool {
    PROBING.with(|p| {
        let p = p.borrow();
        p.on && matches!(p.rows.get(&row), Some(Some(path)) if p.linked.contains(path))
    })
}

/// A number a row edits: written as `PROBE` and read back.
pub trait Probed {
    fn get(&self) -> f64;
    fn put(&mut self, x: f64);
}

macro_rules! probed {
    ($($t:ty),*) => {$(
        impl Probed for $t {
            fn get(&self) -> f64 {
                *self as f64
            }
            fn put(&mut self, x: f64) {
                *self = x as $t;
            }
        }
    )*};
}
probed!(f32, f64, i32, i64, u32, u64, usize);

/// A number that follows another: desaturated orange.
pub fn follows_orange() -> egui::Color32 {
    egui::Color32::from_rgb(160, 118, 78)
}

/// A number field (a `DragValue` on `v`, made by `make`), at `size` or its
/// own, dark when fields are, orange when it follows another: a right-click
/// on it, while probing, is taken (see `probing`). Every number field of the
/// Parameters pane is one, so each has the references and Promote menu.
pub fn number<T: egui::emath::Numeric + Probed>(
    ui: &mut egui::Ui,
    size: Option<[f32; 2]>,
    v: &mut T,
    make: impl FnOnce(egui::DragValue<'_>) -> egui::DragValue<'_>,
) -> egui::Response {
    let row = ui.next_auto_id();
    let orange = follows(row);
    let d = make(egui::DragValue::new(&mut *v)).update_while_editing(false);
    let mut r = if orange {
        ui.scope(|ui| {
            let vis = ui.visuals_mut();
            let hover = egui::Color32::from_rgb(182, 136, 92);
            for (w, fill) in [
                (&mut vis.widgets.inactive, follows_orange()),
                (&mut vis.widgets.open, follows_orange()),
                (&mut vis.widgets.hovered, hover),
                (&mut vis.widgets.active, hover),
            ] {
                w.bg_fill = fill;
                w.weak_bg_fill = fill;
                w.fg_stroke.color = egui::Color32::WHITE;
            }
            match size {
                Some(s) => ui.add_sized(s, d),
                None => ui.add(d),
            }
        })
        .inner
    } else {
        match size {
            Some(s) => ui.field_sized(s, d),
            None => ui.field(d),
        }
    };
    probe(ui, &mut r, v, row);
    r
}

/// A slider (made by `make` on `v`), on orange when it follows another: a
/// right-click does not move it, and, while probing, is taken (see
/// `probing`).
pub fn slider<T: egui::emath::Numeric + Probed>(
    ui: &mut egui::Ui,
    v: &mut T,
    make: impl FnOnce(&mut T) -> egui::Slider<'_>,
) -> egui::Response {
    let row = ui.next_auto_id();
    let behind = follows(row).then(|| ui.painter().add(egui::Shape::Noop));
    let before = v.get();
    let mut r = ui.add(make(&mut *v).update_while_editing(false));
    if let Some(at) = behind {
        let rect = r.rect.expand(2.0);
        ui.painter()
            .set(at, egui::Shape::rect_filled(rect, 3.0, follows_orange()));
    }
    // egui moves a slider under any button: not under the right one.
    if r.secondary_clicked() || (r.is_pointer_button_down_on() && ui.input(|i| i.pointer.secondary_down())) {
        v.put(before);
    }
    probe(ui, &mut r, v, row);
    r
}

/// While probing: a row's number right-clicked, or a row not yet known, one
/// a frame, on a frame where nothing but the pointer moving happens: `PROBE`
/// in its place, marked changed so a caller that copies the number writes it
/// back. Until every row is known, another frame is asked for.
fn probe(ui: &egui::Ui, r: &mut egui::Response, v: &mut impl Probed, row: egui::Id) {
    let was = v.get();
    if probe_row(ui, r, was, None, row) {
        v.put(PROBE);
        r.mark_changed();
    }
}

/// Whether row `row` (its response `r`, its number `was`, or its text) is
/// probed this frame (see `probe`): right-clicked, or found while nothing is
/// done.
fn probe_row(ui: &egui::Ui, r: &egui::Response, was: f64, text: Option<&str>, row: egui::Id) -> bool {
    let clicked = r.secondary_clicked();
    let still = || {
        let quiet = ui.input(|i| {
            !i.pointer.any_down()
                && !i.pointer.any_released()
                && i.events
                    .iter()
                    .all(|e| matches!(e, egui::Event::PointerMoved(_) | egui::Event::MouseMoved(_)))
        });
        quiet && ui.memory(|m| m.focused().is_none())
    };
    PROBING.with(|p| {
        let mut p = p.borrow_mut();
        if !p.on || p.clicked.is_some() {
            return false;
        }
        if clicked {
            p.clicked = Some((was, r.interact_pointer_pos().unwrap_or(r.rect.left_bottom())));
            p.text = text.map(str::to_string);
        } else if !p.linked.is_empty() && !p.rows.contains_key(&row) {
            ui.ctx().request_repaint();
            if p.finding.is_some() || !still() {
                return false;
            }
            p.finding = Some(row);
        } else {
            return false;
        }
        true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// While probing, the number right-clicked is taken and probing stops;
    /// not probing, nothing is. A row found is kept for the same operator and
    /// references, and shows orange when its number follows another.
    #[test]
    fn probing_takes_one_number() {
        probing(true, 1, vec!["scale/2".into()]);
        PROBING.with(|p| p.borrow_mut().clicked = Some((2.0, egui::pos2(1.0, 1.0))));
        assert!(probe_put(), "a change set off by the probe waits");
        assert_eq!(probed(), Some(Probe::Clicked(2.0, egui::pos2(1.0, 1.0))));
        assert!(!probe_put());
        assert_eq!(probed(), None);
        let row = egui::Id::new("row");
        found(row, Some("scale/2".into()));
        assert!(!follows(row), "not probing: no row is drawn orange");
        probing(true, 1, vec!["scale/2".into()]);
        assert!(follows(row), "the row found is kept for the same operator");
        probing(true, 1, vec![]);
        assert!(!follows(row), "it follows no more");
        probing(true, 2, vec!["scale/2".into()]);
        assert!(!follows(row), "another operator: its rows are found again");
        let mut x = 3.0f32;
        x.put(PROBE);
        assert_eq!(x.get(), PROBE, "the probe is a number a float holds exactly");
        let mut n = 4u32;
        n.put(PROBE);
        assert_eq!(n.get(), PROBE);
    }
    /// A text field changes its text on Enter, not as each letter is typed;
    /// Escape leaves it as it was.
    #[test]
    fn text_is_changed_on_enter() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("field");
        let frame = |v: &mut String, events: Vec<egui::Event>| {
            let mut changed = false;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                changed |= typed(ui, v, |ui, t| ui.add(egui::TextEdit::singleline(t).id(id))).changed();
            })
            .drop_without_applying_deltas();
            changed
        };
        let key = |k: egui::Key| egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut v = "1".to_string();
        frame(&mut v, vec![]);
        ctx.memory_mut(|m| m.request_focus(id));
        assert!(!frame(&mut v, vec![egui::Event::Text("2".into())]));
        assert!(!frame(&mut v, vec![egui::Event::Text("5".into())]));
        assert_eq!(v, "1", "not while typing");
        let mut changed = frame(&mut v, vec![key(egui::Key::Enter)]);
        changed |= frame(&mut v, vec![]);
        assert!(changed);
        assert_eq!(v, "125", "on Enter");
        ctx.memory_mut(|m| m.request_focus(id));
        frame(&mut v, vec![egui::Event::Text("9".into())]);
        let mut changed = frame(&mut v, vec![key(egui::Key::Escape)]);
        changed |= frame(&mut v, vec![]);
        assert!(!changed);
        assert_eq!(v, "125", "Escape leaves it");
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
