//! Colour schemes: Light, Dark and ADHD, each editable.
//!
//! Every panel keeps its own colour table, written once (in the tones of the
//! original grey theme: surfaces from grey 40 to 150, text above). Each of
//! those colours goes through `c`, which maps it into the current scheme, so
//! one switch restyles the whole interface. A scheme is six colours: the two
//! ends of the surface ramp, text, dim text, an accent and an outline. The
//! egui widget style is derived from the scheme in `apply`. The scheme is
//! remembered between sessions.
//!
//! Light follows Softimage XSI: light warm greys, black text, buttons a step
//! lighter than the panel, a mid-grey viewport.

use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use egui::{self, Color32, Stroke};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Preset { Light, Dark, Adhd }

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Light, Preset::Dark, Preset::Adhd];
    pub fn label(self) -> &'static str {
        match self { Preset::Light => "Light", Preset::Dark => "Dark", Preset::Adhd => "ADHD" }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Preset::Light => "Light greys and black text, after Softimage XSI",
            Preset::Dark  => "Neutral dark greys with strong text contrast",
            Preset::Adhd  => "Dark blue surfaces with orange for everything selected or active",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Scheme {
    /// The preset this scheme started from.
    pub preset:  Preset,
    /// Darkest surface: canvases, text fields.
    pub low:     [u8; 3],
    /// Lightest surface: node bodies, buttons.
    pub high:    [u8; 3],
    pub text:    [u8; 3],
    pub dim:     [u8; 3],
    /// Selection, active flags, highlights.
    pub accent:  [u8; 3],
    pub outline: [u8; 3],
    /// How heavy text is drawn, 0 (thinnest) to 100 (heaviest): see
    /// `text_gamma`. Schemes saved without it take their preset's.
    #[serde(default = "unset_text_weight")]
    pub text_weight: u8,
    /// Fields you type in (text, numbers) dark grey with light text, in
    /// the Parameters pane. On in the Light preset, off in Dark and ADHD:
    /// choosing a preset takes its setting. Not a colour, so Reset keeps it
    /// and it does not make a scheme "edited".
    /// `DARK_FIELD_BG` overrides it: see `dark_fields`.
    #[serde(default)]
    pub dark_fields: bool,
    /// The Parameters pane's tabs drawn as 1995's were: bevelled, the open
    /// one raised over the others. Off: drawn as the panes' tabs. Off in
    /// every preset; a switch, as `dark_fields`: kept by Reset and by
    /// choosing a preset, never an edit.
    #[serde(default)]
    pub tabs_1995: bool,
}

/// Text weight at which `text_gamma` is 1.3: the Light preset's.
pub const TEXT_WEIGHT: u8 = 50;
/// The dark presets' text weight: light text on dark surfaces, set by eye
/// against the Bevy shell at 1x (Martino).
pub const DARK_TEXT_WEIGHT: u8 = 80;
/// A scheme read from a file written before text weight existed; `settled`
/// gives it its preset's.
fn unset_text_weight() -> u8 { u8::MAX }

impl Scheme {
    pub const fn preset(preset: Preset) -> Scheme {
        match preset {
            Preset::Light => Scheme { preset, low: [112, 112, 110], high: [206, 206, 202], text: [10, 10, 10],
                                      dim: [45, 45, 43], accent: [74, 112, 168], outline: [72, 72, 70], text_weight: TEXT_WEIGHT, dark_fields: true, tabs_1995: false },
            Preset::Dark  => Scheme { preset, low: [12, 12, 12], high: [64, 64, 64], text: [234, 234, 234],
                                      dim: [182, 182, 182], accent: [60, 110, 170], outline: [96, 96, 96], text_weight: DARK_TEXT_WEIGHT, dark_fields: false, tabs_1995: false },
            Preset::Adhd  => Scheme { preset, low: [6, 12, 28], high: [36, 56, 96], text: [246, 238, 224],
                                      dim: [170, 184, 210], accent: [255, 140, 40], outline: [80, 108, 160], text_weight: DARK_TEXT_WEIGHT, dark_fields: false, tabs_1995: false },
        }
    }
    /// Its colours or text weight differ from its preset's.
    pub fn is_edited(&self) -> bool { Scheme { dark_fields: Scheme::preset(self.preset).dark_fields, tabs_1995: false, ..*self } != Scheme::preset(self.preset) }
    /// Its preset, keeping its dark fields setting.
    fn reset(&self) -> Scheme { Scheme { dark_fields: self.dark_fields, tabs_1995: self.tabs_1995, ..Scheme::preset(self.preset) } }

    /// Preset `p`, keeping this scheme's tabs.
    fn with_preset(&self, p: Preset) -> Scheme { Scheme { tabs_1995: self.tabs_1995, ..Scheme::preset(p) } }
    /// A text weight not set in the file becomes the preset's.
    fn settled(mut self) -> Scheme {
        if self.text_weight > 100 { self.text_weight = Scheme::preset(self.preset).text_weight; }
        self
    }
    /// Text darker than the surfaces: the Light scheme, however edited.
    fn dark_text(&self) -> bool {
        let lum = |c: [u8; 3]| c[0] as u32 + c[1] as u32 + c[2] as u32;
        lum(self.text) < lum(self.low).min(lum(self.high))
    }
}

static SCHEME: RwLock<Scheme> = RwLock::new(Scheme::preset(Preset::Light));
static REVISION: AtomicU64 = AtomicU64::new(0);

pub fn scheme() -> Scheme { *SCHEME.read().unwrap_or_else(|e| e.into_inner()) }

/// Counts scheme changes, for anything that caches colours.
pub fn revision() -> u64 { REVISION.load(Ordering::Relaxed) }

/// True for every scheme that is not based on Light.
pub fn is_dark() -> bool { scheme().preset != Preset::Light }

/// `DARK_FIELD_BG` as set when Imago started: `Some(true)` for 1,
/// `Some(false)` for 0, `None` when unset (the scheme's setting decides).
pub fn dark_fields_env() -> Option<bool> {
    static ENV: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();
    *ENV.get_or_init(|| match std::env::var("DARK_FIELD_BG").ok()?.trim() {
        "1" | "true" | "on" | "yes" => Some(true),
        "0" | "false" | "off" | "no" => Some(false),
        _ => None,
    })
}

/// The Parameters pane's tabs drawn as 1995's (the colour editor's box).
pub fn tabs_1995() -> bool { scheme().tabs_1995 }

/// The panes' tab bars as Imago draws them, from egui's style: the
/// Parameters pane's own tabs take the same look.
pub fn dock_style(style: &egui::Style) -> egui_dock::Style {
    let mut style = egui_dock::Style::from_egui(style);
    style.tab_bar.fill_tab_bar = true;
    // Tab titles: readable when idle, brightest on the tab in front.
    style.tab.inactive.text_color = c(223, 223, 223);
    style.tab.hovered.text_color  = c(250, 250, 250);
    style.tab.active.text_color   = c(250, 250, 250);
    style.tab.focused.text_color  = c(245, 245, 245);
    // Light: tabs behind sit darker than the panel, the one in front is
    // the lightest. egui_dock tints them towards the text-field colour,
    // which in Light is near white and made them look in front. The
    // dark schemes are right as they are.
    if !is_dark() {
        let behind = c(102, 102, 102);
        style.tab.inactive.bg_fill = behind;
        style.tab.inactive_with_kb_focus.bg_fill = behind;
        style.tab.hovered.bg_fill = c(110, 110, 110);
    }
    style
}

/// Fields you type in drawn dark grey with light text: `DARK_FIELD_BG` if
/// set, else the colour editor's "Dark fields" box (off by default).
pub fn dark_fields() -> bool { dark_fields_env().unwrap_or(scheme().dark_fields) }

/// The viewport's background: XSI's mid grey in Light (black wires and
/// boxes read on it), Bevy's dark grey otherwise.
pub fn viewport_bg() -> [u8; 3] {
    if scheme().dark_text() { [132, 132, 130] } else { [43, 44, 47] }
}

/// The Light scheme of earlier versions (dark greys, white text), as saved
/// by them: read back as today's Light.
const OLD_LIGHT: Scheme = Scheme { preset: Preset::Light, low: [40, 40, 40], high: [150, 150, 150], text: [230, 230, 230],
                                   dim: [190, 190, 190], accent: [100, 130, 160], outline: [70, 70, 70], text_weight: TEXT_WEIGHT, dark_fields: false, tabs_1995: false };

fn store(s: Scheme) {
    *SCHEME.write().unwrap_or_else(|e| e.into_inner()) = s;
    REVISION.fetch_add(1, Ordering::Relaxed);
}

/// Switch scheme, restyle egui and remember the choice.
pub fn set_scheme(ctx: &egui::Context, s: Scheme) {
    if s == scheme() { return; }
    store(s);
    apply(ctx);
    if let Some(file) = crate::file_browser::config_path("theme.json") {
        if let Some(dir) = file.parent() { let _ = std::fs::create_dir_all(dir); }
        if let Ok(text) = serde_json::to_string_pretty(&s) { let _ = std::fs::write(file, text); }
    }
}

/// Read the remembered scheme and style egui. Call once at startup.
pub fn init(ctx: &egui::Context) {
    let read = |name: &str| crate::file_browser::config_path(name).and_then(|f| std::fs::read_to_string(f).ok());
    let saved = read("theme.json").and_then(|t| serde_json::from_str::<Scheme>(&t).ok())
        // Written by versions that had light and dark only.
        .or_else(|| read("theme.txt").filter(|t| t.trim() == "dark").map(|_| Scheme::preset(Preset::Dark)))
        .map(Scheme::settled)
        .map(|s| if s == OLD_LIGHT { Scheme::preset(Preset::Light) } else { s })
        .unwrap_or(Scheme::preset(Preset::Light));
    store(saved);
    apply(ctx);
}

fn rgb(c: [u8; 3]) -> Color32 { Color32::from_rgb(c[0], c[1], c[2]) }

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2])]
}

fn lift(c: [u8; 3], by: u8) -> [u8; 3] { [c[0].saturating_add(by), c[1].saturating_add(by), c[2].saturating_add(by)] }

/// The accent at a given brightness.
fn accent_at(s: &Scheme, level: u8) -> [u8; 3] {
    let top = s.accent.iter().copied().max().unwrap_or(1).max(1) as f32;
    let k = level as f32 / top;
    [(s.accent[0] as f32 * k).min(255.0) as u8, (s.accent[1] as f32 * k).min(255.0) as u8, (s.accent[2] as f32 * k).min(255.0) as u8]
}

/// Neutral surface of the light theme (grey 40 to 150) on the scheme's ramp.
fn surface(s: &Scheme, level: f32) -> [u8; 3] { mix(s.low, s.high, (level - 40.0) / 110.0) }

fn map(s: &Scheme, r: u8, g: u8, b: u8) -> [u8; 3] {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let avg = (r as f32 + g as f32 + b as f32) / 3.0;
    let neutral = max - min <= 12;
    let bluish  = b as i32 >= r as i32 + 8 && b >= g;
    if max >= 150 {
        // Text: 150..230 runs from below dim text up to full text.
        if neutral {
            let t = (avg - 150.0) / 80.0;
            let faint = mix(s.dim, surface(s, 118.0), 0.25);
            return if t <= 0.5 { mix(faint, s.dim, t * 2.0) } else { mix(s.dim, s.text, (t - 0.5) * 2.0) };
        }
        // Bright blues are the light theme's highlights: they become the accent.
        if b as i32 >= r as i32 + 25 && b >= g { return accent_at(s, max); }
        // Wires, axes, warnings: colours that carry meaning stay as they are.
        return [r, g, b];
    }
    // Tinted surface. Blue tints mark selection: they take the accent.
    if !neutral {
        let base = surface(s, avg);
        if bluish { return mix(base, s.accent, 0.5); }
        let level = (base[0] as f32 + base[1] as f32 + base[2] as f32) / 3.0;
        let k = (level / avg.max(1.0)).max(0.45);
        return [(r as f32 * k).min(255.0) as u8, (g as f32 * k).min(255.0) as u8, (b as f32 * k).min(255.0) as u8];
    }
    surface(s, avg)
}

/// A colour of the light theme, in the current scheme.
pub fn c(r: u8, g: u8, b: u8) -> Color32 { rgb(map(&scheme(), r, g, b)) }

/// A surface that sits on top of a canvas (a node body or title): lifted
/// above the plain mapping so nodes stand out from the canvas behind them.
pub fn raised(r: u8, g: u8, b: u8) -> Color32 {
    let s = scheme();
    rgb(lift(map(&s, r, g, b), 24))
}

/// An outline: the scheme's outline colour.
pub fn outline(_r: u8, _g: u8, _b: u8) -> Color32 {
    rgb(scheme().outline)
}

/// The accent colour at full strength: active flags, highlights.
pub fn accent() -> Color32 { let s = scheme(); rgb(accent_at(&s, 255)) }

/// What is selected: the blue of a selected row in the Scene Explorer and
/// the Operator Stack, also around selected nodes.
pub fn selection() -> Color32 { c(90, 105, 120) }

/// A box being dragged to select, in Light: the selection blue, its inside
/// see-through. The dark schemes keep their own (see the callers).
pub fn marquee_light() -> (Color32, Color32) {
    let edge = selection();
    (edge.gamma_multiply(0.3), edge)
}

/// Style egui's own widgets for the current scheme.
pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    let s = scheme();

    if s.dark_text() {
        // Light, after XSI: a light warm-grey panel, buttons a step lighter
        // with a fine edge, light fields, black text everywhere.
        let panel  = surface(&s, 118.0);
        let button = lift(s.high, 8);
        let field  = lift(s.high, 22);
        let edge   = mix(panel, s.outline, 0.55);
        let (text, dim) = (rgb(s.text), rgb(mix(s.dim, s.text, 0.35)));
        style.visuals = egui::Visuals::light();
        let v = &mut style.visuals;
        v.panel_fill       = rgb(panel);
        v.window_fill      = rgb(surface(&s, 126.0));
        v.extreme_bg_color = rgb(field);          // text fields, drag values
        v.faint_bg_color   = rgb(surface(&s, 110.0));
        v.code_bg_color    = rgb(field);
        v.window_stroke    = Stroke::new(1.0_f32, rgb(s.outline));
        v.window_shadow    = egui::epaint::Shadow { color: Color32::from_black_alpha(40), ..v.window_shadow };
        v.popup_shadow     = egui::epaint::Shadow { color: Color32::from_black_alpha(40), ..v.popup_shadow };

        v.widgets.noninteractive.bg_fill      = rgb(panel);
        v.widgets.noninteractive.weak_bg_fill = rgb(panel);
        v.widgets.noninteractive.bg_stroke    = Stroke::new(1.0_f32, rgb(edge));   // separators
        v.widgets.noninteractive.fg_stroke    = Stroke::new(1.0_f32, dim);

        v.widgets.inactive.bg_fill      = rgb(button);
        v.widgets.inactive.weak_bg_fill = rgb(button);
        v.widgets.inactive.bg_stroke    = Stroke::new(1.0_f32, rgb(edge));
        v.widgets.inactive.fg_stroke    = Stroke::new(1.0_f32, text);

        v.widgets.hovered.bg_fill      = rgb(lift(button, 14));
        v.widgets.hovered.weak_bg_fill = rgb(lift(button, 14));
        v.widgets.hovered.bg_stroke    = Stroke::new(1.0_f32, rgb(s.accent));
        v.widgets.hovered.fg_stroke    = Stroke::new(1.5_f32, text);

        v.widgets.active.bg_fill      = rgb(mix(button, s.outline, 0.15));
        v.widgets.active.weak_bg_fill = rgb(mix(button, s.outline, 0.15));
        v.widgets.active.bg_stroke    = Stroke::new(1.0_f32, rgb(s.accent));
        v.widgets.active.fg_stroke    = Stroke::new(2.0_f32, text);

        v.widgets.open.bg_fill      = rgb(surface(&s, 128.0));
        v.widgets.open.weak_bg_fill = rgb(button);
        v.widgets.open.bg_stroke    = Stroke::new(1.0_f32, rgb(s.outline));
        v.widgets.open.fg_stroke    = Stroke::new(1.0_f32, text);

        v.selection.bg_fill = rgb(mix(panel, s.accent, 0.45));
        v.selection.stroke  = Stroke::new(1.0_f32, text);
        v.hyperlink_color   = rgb(mix(s.accent, s.text, 0.3));
    } else {
        let panel   = surface(&s, 118.0);
        let field   = surface(&s, 72.0);
        let button  = lift(s.high, 12);
        let bright  = accent_at(&s, 255);
        let (text, dim) = (rgb(s.text), rgb(s.dim));
        style.visuals = egui::Visuals::dark();
        let v = &mut style.visuals;
        v.panel_fill       = rgb(panel);
        v.window_fill      = rgb(surface(&s, 126.0));
        v.extreme_bg_color = rgb(field);          // text fields, drag values
        v.faint_bg_color   = rgb(surface(&s, 128.0));
        v.code_bg_color    = rgb(field);
        v.window_stroke    = Stroke::new(1.0_f32, rgb(s.outline));

        v.widgets.noninteractive.bg_fill      = rgb(panel);
        v.widgets.noninteractive.weak_bg_fill = rgb(panel);
        v.widgets.noninteractive.bg_stroke    = Stroke::new(1.0_f32, rgb(mix(panel, s.outline, 0.5)));   // separators
        v.widgets.noninteractive.fg_stroke    = Stroke::new(1.0_f32, dim);

        v.widgets.inactive.bg_fill      = rgb(button);
        v.widgets.inactive.weak_bg_fill = rgb(button);
        v.widgets.inactive.bg_stroke    = Stroke::NONE;
        v.widgets.inactive.fg_stroke    = Stroke::new(1.0_f32, text);

        v.widgets.hovered.bg_fill      = rgb(lift(button, 22));
        v.widgets.hovered.weak_bg_fill = rgb(lift(button, 22));
        v.widgets.hovered.bg_stroke    = Stroke::new(1.0_f32, rgb(bright));
        v.widgets.hovered.fg_stroke    = Stroke::new(1.5_f32, Color32::WHITE);

        v.widgets.active.bg_fill      = rgb(lift(button, 40));
        v.widgets.active.weak_bg_fill = rgb(lift(button, 40));
        v.widgets.active.bg_stroke    = Stroke::new(1.0_f32, rgb(bright));
        v.widgets.active.fg_stroke    = Stroke::new(2.0_f32, Color32::WHITE);

        v.widgets.open.bg_fill      = rgb(surface(&s, 90.0));
        v.widgets.open.weak_bg_fill = rgb(lift(s.high, 4));
        v.widgets.open.bg_stroke    = Stroke::new(1.0_f32, rgb(s.outline));
        v.widgets.open.fg_stroke    = Stroke::new(1.0_f32, text);

        v.selection.bg_fill = rgb(mix(panel, s.accent, 0.6));
        v.selection.stroke  = Stroke::new(1.0_f32, rgb(mix(bright, [255, 255, 255], 0.35)));
        v.hyperlink_color   = rgb(bright);
    }

    // Text as it looked under Bevy, also on screens of ordinary density
    // (1x, Full HD), where the edges of glyphs are most of what is seen.
    // Bevy blended egui in linear light, which thins dark text on a light
    // ground and fills out light text on a dark one; eframe blends in gamma
    // space, and egui turns glyph coverage into opacity by one curve for all
    // text. Node titles are dark text on light bodies even in the dark
    // schemes, so one curve has to suit both: a gamma above 1, set by the
    // scheme's text weight. No hinting: Bevy's text was not hinted, and at
    // 1x hinting makes small text blocky. Glyphs sit on whole pixels, as
    // before.
    let text = &mut style.visuals.text_options;
    text.color_transfer_function = egui::epaint::FontColorTransferFunction::Gamma(text_gamma(&s));
    text.font_hinting = false;
    text.subpixel_binning = false;
    // Text sizes of egui 0.28, which the panes are laid out for (0.33 made
    // body text 13 and code 13).
    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles.insert(TextStyle::Body, FontId::new(12.5, FontFamily::Proportional));
    style.text_styles.insert(TextStyle::Button, FontId::new(12.5, FontFamily::Proportional));
    style.text_styles.insert(TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace));

    // egui keeps a style for the system's light mode and one for its dark
    // mode, and follows the system: both are the scheme's, so the system's
    // setting changes nothing (with one only, Imago started in egui's own
    // white style).
    ctx.set_style_of(egui::Theme::Dark, style.clone());
    ctx.set_style_of(egui::Theme::Light, style);
}

/// How glyph coverage becomes opacity, from the scheme's text weight: a
/// gamma of 1.3 at 50 (Light), about 0.77 at 80 (Dark, ADHD), down to about
/// 0.55 (heavier) at 100 and up to about 3 (thinner) at 0.
pub fn text_gamma(s: &Scheme) -> f32 {
    1.3 * 2f32.powf((TEXT_WEIGHT as f32 - s.text_weight as f32) / 40.0)
}

/// The scheme menu of the top bar: the three presets and the editor.
pub fn menu(ui: &mut egui::Ui) {
    let now = scheme();
    for p in Preset::ALL {
        let label = if now.preset == p && now.is_edited() { format!("{} (edited)", p.label()) } else { p.label().to_string() };
        if ui.radio(now.preset == p, label).on_hover_text(p.hint()).clicked() {
            if now.preset != p { set_scheme(ui.ctx(), now.with_preset(p)); }
            ui.close();
        }
    }
    ui.separator();
    if ui.button("Edit colours...").clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(editor_id(), true));
        ui.close();
    }
}

fn editor_id() -> egui::Id { egui::Id::new("theme_editor_open") }

/// The colour scheme editor, shown while it is open. Changes apply at once.
pub fn editor(ctx: &egui::Context) {
    let mut open = ctx.data(|d| d.get_temp::<bool>(editor_id())).unwrap_or(false);
    if !open { return; }
    let mut s = scheme();
    egui::Window::new("Colour scheme").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Start from:");
            for p in Preset::ALL {
                if ui.selectable_label(s.preset == p, p.label()).on_hover_text(p.hint()).clicked() { s = s.with_preset(p); }
            }
        });
        ui.separator();
        egui::Grid::new("theme_editor_grid").num_columns(3).spacing([10.0, 6.0]).show(ui, |ui| {
            let row = |ui: &mut egui::Ui, name: &str, what: &str, c: &mut [u8; 3]| {
                ui.label(name);
                ui.color_edit_button_srgb(c);
                ui.label(egui::RichText::new(what).small());
                ui.end_row();
            };
            row(ui, "Darkest surface",  "canvases, text fields",          &mut s.low);
            row(ui, "Lightest surface", "nodes, buttons",                 &mut s.high);
            row(ui, "Text",             "names, values",                  &mut s.text);
            row(ui, "Dim text",         "labels, hints",                  &mut s.dim);
            row(ui, "Accent",           "selection, active flags",        &mut s.accent);
            row(ui, "Outline",          "node borders, window edges",     &mut s.outline);
            ui.label("Text weight");
            ui.add(egui::Slider::new(&mut s.text_weight, 0..=100).show_value(true));
            ui.label(egui::RichText::new("thinner or heavier text").small());
            ui.end_row();
            ui.label("Dark fields");
            match dark_fields_env() {
                // Set from outside: shown, not changed here.
                Some(on) => {
                    let mut shown = on;
                    ui.add_enabled(false, egui::Checkbox::without_text(&mut shown))
                        .on_disabled_hover_text(format!("Set by DARK_FIELD_BG={}", if on { 1 } else { 0 }));
                }
                None => { ui.checkbox(&mut s.dark_fields, ""); }
            }
            ui.label(egui::RichText::new("text and number fields dark grey, light text").small());
            ui.end_row();
            ui.label("1995 tabs");
            ui.checkbox(&mut s.tabs_1995, "");
            ui.label(egui::RichText::new("the Parameters pane's tabs bevelled, the open one raised; off: as the panes' tabs").small());
            ui.end_row();
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.add_enabled(s.is_edited(), egui::Button::new(format!("Reset to {}", s.preset.label()))).clicked() {
                s = s.reset();
            }
            ui.label(egui::RichText::new("Saved as you edit.").small());
        });
    });
    set_scheme(ctx, s);
    ctx.data_mut(|d| d.insert_temp(editor_id(), open));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lum(c: [u8; 3]) -> f32 {
        let f = |v: u8| { let x = v as f32 / 255.0; if x <= 0.03928 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) } };
        0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2])
    }
    fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
        let (hi, lo) = if lum(a) > lum(b) { (lum(a), lum(b)) } else { (lum(b), lum(a)) };
        (hi + 0.05) / (lo + 0.05)
    }
    const DARKS: [Preset; 2] = [Preset::Dark, Preset::Adhd];

    #[test]
    fn light_is_light_with_black_text() {
        let s = Scheme::preset(Preset::Light);
        assert!(s.dark_text());
        let panel = map(&s, 118, 118, 118);
        // XSI's panel grey, give or take.
        assert!((165..=185).contains(&panel[0]), "{panel:?}");
        // Text of every weight reads on the panel and on the lighter groups.
        for text in [248u8, 230, 210] {
            assert!(contrast(map(&s, text, text, text), panel) >= 7.0, "text {text}");
        }
        assert!(contrast(map(&s, 190, 190, 190), panel) >= 4.5, "dim text");
        assert!(contrast(map(&s, 170, 170, 170), panel) >= 3.0, "dim labels");
        // Nodes stand out from the graph canvas, lighter.
        let canvas = map(&s, 100, 100, 100);
        let body = lift(map(&s, 130, 130, 130), 24);
        assert!(lum(body) > lum(canvas) && contrast(body, canvas) > 1.3);
        // A Light scheme saved by an earlier version comes back as today's.
        assert_ne!(OLD_LIGHT, s);
    }

    /// Dark fields: on in Light, off in Dark and ADHD; read as off from a
    /// file saved without it, kept by Reset and not counted as an edit.
    #[test]
    fn dark_fields_are_a_switch_not_a_colour() {
        assert!(Scheme::preset(Preset::Light).dark_fields);
        assert!(!Scheme::preset(Preset::Dark).dark_fields && !Scheme::preset(Preset::Adhd).dark_fields);
        let old = r#"{"preset":"Dark","low":[12,12,12],"high":[64,64,64],"text":[234,234,234],"dim":[182,182,182],"accent":[60,110,170],"outline":[96,96,96],"text_weight":80}"#;
        let read: Scheme = serde_json::from_str(old).unwrap();
        assert!(!read.dark_fields && !read.is_edited());
        let on = Scheme { dark_fields: true, ..Scheme::preset(Preset::Light) };
        assert!(!on.is_edited());
        assert!(!Scheme { dark_fields: false, ..Scheme::preset(Preset::Light) }.is_edited(), "turned off: still the preset");
        let edited = Scheme { text_weight: 20, ..on };
        assert!(edited.is_edited() && edited.reset().dark_fields && !edited.reset().is_edited());
    }

    /// 1995 tabs: off in every preset and in files saved without it; a
    /// switch kept by Reset and by choosing a preset, never an edit.
    #[test]
    fn tabs_1995_are_a_switch() {
        assert!(Preset::ALL.iter().all(|p| !Scheme::preset(*p).tabs_1995));
        let old = r#"{"preset":"Dark","low":[12,12,12],"high":[64,64,64],"text":[234,234,234],"dim":[182,182,182],"accent":[60,110,170],"outline":[96,96,96],"text_weight":80}"#;
        assert!(!serde_json::from_str::<Scheme>(old).unwrap().tabs_1995);
        let on = Scheme { tabs_1995: true, ..Scheme::preset(Preset::Light) };
        assert!(!on.is_edited());
        assert!(Scheme { text_weight: 20, ..on }.reset().tabs_1995);
        assert!(on.with_preset(Preset::Adhd).tabs_1995);
    }

    /// Light starts at 50, the dark presets at 80; a heavier weight gives a
    /// lower gamma (more opaque edges), and a scheme saved before the
    /// setting existed loads with its preset's.
    #[test]
    fn text_weight() {
        assert!((text_gamma(&Scheme::preset(Preset::Light)) - 1.3).abs() < 1e-6);
        for p in DARKS { assert_eq!(Scheme::preset(p).text_weight, 80); }
        let mut s = Scheme::preset(Preset::Light);
        let mut last = f32::MAX;
        for w in [0u8, 25, 50, 75, 100] {
            s.text_weight = w;
            let g = text_gamma(&s);
            assert!(g < last && g > 0.3 && g < 4.0, "weight {w}: {g}");
            last = g;
        }
        let old = r#"{"preset":"Dark","low":[12,12,12],"high":[64,64,64],"text":[234,234,234],"dim":[182,182,182],"accent":[60,110,170],"outline":[96,96,96]}"#;
        let back: Scheme = serde_json::from_str(old).unwrap();
        assert_eq!(back.settled(), Scheme::preset(Preset::Dark));
    }

    #[test]
    fn surfaces_keep_their_order() {
        // Levels used as surfaces in the light theme, darkest to lightest.
        let levels = [45u8, 58, 66, 72, 78, 90, 100, 105, 108, 118, 130];
        for p in DARKS {
            let s = Scheme::preset(p);
            let mapped: Vec<f32> = levels.iter().map(|v| lum(map(&s, *v, *v, *v))).collect();
            assert!(mapped.windows(2).all(|w| w[0] <= w[1]), "{p:?} {mapped:?}");
        }
    }

    #[test]
    fn text_reads_on_every_surface() {
        for p in DARKS {
            let s = Scheme::preset(p);
            for surface in [100u8, 118, 130] {
                let bg = map(&s, surface, surface, surface);
                for text in [230u8, 210, 190] {
                    let k = contrast(map(&s, text, text, text), bg);
                    assert!(k >= 5.5, "{p:?} text {text} on {surface}: {k:.1}");
                }
                // The dimmest labels.
                assert!(contrast(map(&s, 170, 170, 170), bg) >= 4.0, "{p:?} dim on {surface}");
            }
        }
    }

    #[test]
    fn nodes_stand_out_from_the_canvas() {
        for p in DARKS {
            let s = Scheme::preset(p);
            let canvas = map(&s, 100, 100, 100);
            let body   = lift(map(&s, 130, 130, 130), 24);
            let title  = lift(map(&s, 105, 105, 105), 24);
            assert!(contrast(body, canvas) > 1.6, "{p:?} {:.2}", contrast(body, canvas));
            assert!(contrast(title, canvas) > 1.3, "{p:?} {:.2}", contrast(title, canvas));
            assert!(contrast(s.outline, canvas) > 2.0, "{p:?}");
        }
    }

    #[test]
    fn selection_stays_visible() {
        // Selected row on the panel, selected node on the canvas, clip on the track.
        for p in DARKS {
            let s = Scheme::preset(p);
            for (sel, bg) in [((90, 105, 120), (118, 118, 118)), ((110, 120, 135), (130, 130, 130)), ((96, 122, 148), (78, 78, 78))] {
                let k = contrast(map(&s, sel.0, sel.1, sel.2), map(&s, bg.0, bg.1, bg.2));
                assert!(k > 1.35, "{p:?} {sel:?} on {bg:?}: {k:.2}");
            }
        }
    }

    #[test]
    fn adhd_is_dark_blue_and_orange() {
        let s = Scheme::preset(Preset::Adhd);
        for level in [72u8, 100, 118, 130] {
            let [r, g, b] = map(&s, level, level, level);
            assert!(b > g && g > r && lum([r, g, b]) < 0.06, "surface {level}: {r},{g},{b}");
        }
        // Selection tints and bright highlights turn orange; meaning-carrying colours stay.
        let [r, _, b] = map(&s, 90, 105, 120);
        assert!(r > b);
        let [r, g, b] = map(&s, 100, 180, 255);
        assert!(r == 255 && r > g && g > b);
        assert_eq!(map(&s, 220, 70, 70), [220, 70, 70]);
    }

    #[test]
    fn scheme_survives_a_round_trip() {
        let mut s = Scheme::preset(Preset::Adhd);
        s.accent = [1, 2, 3];
        let back: Scheme = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        assert!(back.is_edited() && !Scheme::preset(Preset::Dark).is_edited());
    }
}
