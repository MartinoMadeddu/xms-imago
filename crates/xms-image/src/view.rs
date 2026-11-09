//! An image on screen (egui): a layer of it, through a look, zoomed and panned,
//! with the values under the pointer.
//!
//! - **Controls** (`ImageView::controls`): the layer (AOV), what of it (RGB, R,
//!   G, B, A, luminance), exposure, gamma, sRGB, Fit.
//! - **Canvas** (`ImageView::canvas`): the image in its frame (the display
//!   window, outlined when the pixels go beyond it: overscan); the wheel zooms
//!   about the pointer, a drag pans, a double-click or F fits it; the pixel
//!   under the pointer and its values in the corner.
//! - **Snapshots** (`snapshots_bar`, `crate::snaps`): the image kept, as many as
//!   wanted, to go back and forth between them and the active one (the render
//!   coming in): a menu, a slider, − and +, Snap, a name, ✕ to let one go.

use std::sync::{Arc, Mutex};

use crate::image::{Image, Layer, Look, Show, Transform};
use crate::snaps::Snaps;

/// Draws an image into a rectangle of the canvas, in place of the view's own
/// texture: the image, its layer, what of it and its look.
pub type GpuDraw<'a> = dyn Fn(&egui::Painter, egui::Rect, &Arc<Image>, &Layer, Show, &Look) + 'a;

/// What a view shows, and how.
pub struct ImageView {
    image: Option<Arc<Image>>,
    /// What it is (a file's path, a render), shown in the corner.
    pub title: String,
    /// The layer shown, by name, kept when another image comes.
    pub layer: String,
    pub show: Show,
    pub look: Look,
    /// The display transform the program gives, in place of sRGB's curve.
    pub display: Option<Transform>,
    /// Screen points per image pixel, when not fitted.
    zoom: f32,
    /// The frame's centre from the canvas's, in screen pixels.
    pan: egui::Vec2,
    fit: bool,
    /// Asked for 1:1 (the button, or H): one image pixel per screen pixel.
    actual: bool,
    texture: Option<egui::TextureHandle>,
    /// What the texture holds: the image, layer, show, look and display
    /// transform it was made from.
    made: Option<(usize, String, Show, [u32; 3], Option<u64>)>,
    /// The images kept: the program's (`Snaps::shared`) unless given others.
    pub snaps: Arc<Mutex<Snaps>>,
    /// The snapshot shown, by id; `None` (or one no longer kept): the active
    /// image.
    pub showing: Option<usize>,
}

impl Default for ImageView {
    fn default() -> Self {
        ImageView {
            image: None,
            title: String::new(),
            layer: "RGBA".into(),
            show: Show::Colour,
            look: Look::default(),
            display: None,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            fit: true,
            actual: false,
            texture: None,
            made: None,
            snaps: Snaps::shared(),
            showing: None,
        }
    }
}

impl ImageView {
    /// Shows an image: the same layer when it has one, else its first. A
    /// different size is fitted.
    pub fn set(&mut self, image: Arc<Image>, title: impl Into<String>) {
        let resized = self
            .image
            .as_ref()
            .is_none_or(|i| (i.width, i.height, i.frame) != (image.width, image.height, image.frame));
        let layers = image.layers();
        if !layers.iter().any(|l| l.name == self.layer) {
            self.layer = layers.first().map(|l| l.name.clone()).unwrap_or_default();
        }
        self.image = Some(image);
        self.title = title.into();
        if resized {
            self.fit = true;
        }
    }

    pub fn image(&self) -> Option<&Arc<Image>> {
        self.image.as_ref()
    }

    /// Nothing shown.
    pub fn clear(&mut self) {
        self.image = None;
        self.title.clear();
        self.made = None;
    }

    /// The snapshot shown, and what it is, while one is.
    pub fn snap_shown(&self) -> Option<(Arc<Image>, String)> {
        let snaps = self.snaps.lock().unwrap_or_else(|e| e.into_inner());
        snaps
            .get(self.showing)
            .map(|s| (s.image.clone(), format!("{}   {}", s.name, s.title)))
    }

    /// The image shown, and what it is: a snapshot, or the active one.
    pub fn shown(&self) -> Option<(Arc<Image>, String)> {
        self.snap_shown()
            .or_else(|| self.image.clone().map(|i| (i, self.title.clone())))
    }

    /// The active image shown again (a render starting).
    pub fn show_active(&mut self) {
        self.showing = None;
    }

    fn current(&self) -> Option<(Arc<Image>, Layer)> {
        let (image, _) = self.shown()?;
        let layers = image.layers();
        let layer = layers.iter().find(|l| l.name == self.layer).or(layers.first())?.clone();
        Some((image, layer))
    }

    /// The layer, what of it, exposure, gamma, sRGB and Fit, in a row.
    pub fn controls(&mut self, ui: &mut egui::Ui) {
        let layers = self.shown().map(|(i, _)| i.layers()).unwrap_or_default();
        ui.add_enabled_ui(!layers.is_empty(), |ui| {
            egui::ComboBox::from_id_salt("xms_image_layer")
                .width(150.0)
                .selected_text(if self.layer.is_empty() {
                    "Layer"
                } else {
                    self.layer.as_str()
                })
                .show_ui(ui, |ui| {
                    for l in &layers {
                        let n = l.channels.len();
                        if ui
                            .selectable_label(self.layer == l.name, format!("{}   ({n})", l.name))
                            .clicked()
                        {
                            self.layer = l.name.clone();
                        }
                    }
                })
                .response
                .on_hover_text("The layer (AOV) shown");
        });
        egui::ComboBox::from_id_salt("xms_image_show")
            .width(52.0)
            .selected_text(self.show.label())
            .show_ui(ui, |ui| {
                for s in Show::ALL {
                    ui.selectable_value(&mut self.show, s, s.label())
                        .on_hover_text(match s {
                            Show::Colour => "Its colour",
                            Show::Red => "Red alone",
                            Show::Green => "Green alone",
                            Show::Blue => "Blue alone",
                            Show::Alpha => "Alpha",
                            Show::Luminance => "Luminance",
                        });
                }
            })
            .response
            .on_hover_text("What of the layer is shown: its colour, one channel, alpha or luminance");
        ui.separator();
        ui.add(
            egui::DragValue::new(&mut self.look.exposure)
                .speed(0.05)
                .range(-20.0..=20.0)
                .max_decimals(2)
                .prefix("E "),
        )
        .on_hover_text("Exposure, in stops: each doubles the values. Double-click to type");
        ui.add(
            egui::DragValue::new(&mut self.look.gamma)
                .speed(0.01)
                .range(0.1..=5.0)
                .max_decimals(2)
                .prefix("G "),
        )
        .on_hover_text("Gamma. Double-click to type");
        // With a display transform, it does what sRGB's curve does.
        if self.display.is_none() {
            ui.checkbox(&mut self.look.srgb, "sRGB")
                .on_hover_text("Renders are linear: sRGB's transfer shows them as they are meant to be seen");
        }
        if ui.button("Reset").on_hover_text("Exposure 0, gamma 1").clicked() {
            self.look = Look::default();
        }
        if ui
            .button("Fit")
            .on_hover_text("The frame in the view (double-click it, or F)")
            .clicked()
        {
            self.fit = true;
        }
        if ui
            .button("1:1")
            .on_hover_text("One image pixel per screen pixel, centred (H)")
            .clicked()
        {
            self.actual = true;
        }
    }

    /// The snapshots bar, the active image the view's (`snapshots_bar`).
    pub fn snapshots(&mut self, ui: &mut egui::Ui) {
        let active = self.image.clone().map(|i| (i, self.title.clone()));
        snapshots_bar(
            ui,
            "xms_image_snaps",
            &self.snaps,
            &mut self.showing,
            "Active Render",
            active.map(|a| move || a),
        );
    }

    /// The image in the space left, with zoom, pan and the values under the
    /// pointer.
    /// `gpu`: draws the image itself into the rectangle given (the program's
    /// GPU path, the colour transform a shader there); without it, the view
    /// makes a texture of its own on the CPU.
    pub fn canvas(&mut self, ui: &mut egui::Ui, gpu: Option<&GpuDraw>) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(38));
        let Some((image, layer)) = self.current() else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No image",
                egui::FontId::proportional(14.0),
                egui::Color32::from_gray(140),
            );
            return;
        };
        // The texture: made again when the image, layer, show or look change.
        let key = (
            Arc::as_ptr(&image) as usize,
            layer.name.clone(),
            self.show,
            [
                self.look.exposure.to_bits(),
                self.look.gamma.to_bits(),
                self.look.srgb as u32,
            ],
            self.display.as_ref().map(|t| t.id),
        );
        if gpu.is_none() && (self.made.as_ref() != Some(&key) || self.texture.is_none()) {
            let pixels = image.display_through(&layer, self.show, &self.look, self.display.as_ref());
            let colour = egui::ColorImage::from_rgba_unmultiplied([image.width.max(1), image.height.max(1)], &pixels);
            match &mut self.texture {
                Some(t) => t.set(colour, egui::TextureOptions::NEAREST),
                None => {
                    self.texture = Some(
                        ui.ctx()
                            .load_texture("xms_image", colour, egui::TextureOptions::NEAREST),
                    )
                }
            }
            self.made = Some(key);
        }

        // The frame: the display window, or the pixels when it has none.
        let (fx, fy, fw, fh) = if image.frame.2 > 0 && image.frame.3 > 0 {
            image.frame
        } else {
            (image.origin.0, image.origin.1, image.width, image.height)
        };
        let fitted = (rect.width() / fw as f32).min(rect.height() / fh as f32) * 0.95;
        if self.fit {
            self.zoom = fitted.max(1e-3);
            self.pan = egui::Vec2::ZERO;
        }
        // 1:1 is one image pixel per physical pixel: on a high-density screen
        // a point is several.
        let ppp = ui.ctx().pixels_per_point();
        if std::mem::take(&mut self.actual) {
            self.zoom = 1.0 / ppp;
            self.pan = egui::Vec2::ZERO;
            self.fit = false;
        }

        // Zoom about the pointer; pan by dragging; fit by double-click or F,
        // 1:1 by H.
        if resp.hovered() {
            if ui.input(|i| i.key_pressed(egui::Key::H)) && !ui.ctx().egui_wants_keyboard_input() {
                self.zoom = 1.0 / ppp;
                self.pan = egui::Vec2::ZERO;
                self.fit = false;
            }
            let (scroll, pinch, f) =
                ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta(), i.key_pressed(egui::Key::F)));
            let factor = (scroll * 0.0015).exp() * pinch;
            if (factor - 1.0).abs() > 1e-4 {
                if let Some(at) = resp.hover_pos() {
                    let new = (self.zoom * factor).clamp(0.01, 256.0);
                    let k = new / self.zoom;
                    let from_centre = at - rect.center() - self.pan;
                    self.pan += from_centre * (1.0 - k);
                    self.zoom = new;
                    self.fit = false;
                }
            }
            if f {
                self.fit = true;
            }
        }
        if resp.dragged() {
            self.pan += resp.drag_delta();
            self.fit = false;
        }
        if resp.double_clicked() {
            self.fit = true;
        }

        let s = self.zoom;
        let frame_min = rect.center() + self.pan - egui::vec2(fw as f32, fh as f32) * s * 0.5;
        let image_min = frame_min + egui::vec2((image.origin.0 - fx) as f32, (image.origin.1 - fy) as f32) * s;
        let image_rect = egui::Rect::from_min_size(image_min, egui::vec2(image.width as f32, image.height as f32) * s);
        if let Some(draw) = gpu {
            draw(&painter, image_rect, &image, &layer, self.show, &self.look);
        } else if let Some(t) = &self.texture {
            painter.image(
                t.id(),
                image_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        // The frame outlined where the pixels go beyond it (overscan).
        let frame_rect = egui::Rect::from_min_size(frame_min, egui::vec2(fw as f32, fh as f32) * s);
        if frame_rect != image_rect {
            painter.rect_stroke(
                frame_rect,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(220, 200, 90)),
                egui::StrokeKind::Outside,
            );
        }

        // What it is and how big, top left; the zoom, top right.
        let font = egui::FontId::monospace(11.5);
        let label = |painter: &egui::Painter, at: egui::Pos2, align: egui::Align2, text: String| {
            let galley = painter.layout_no_wrap(text, font.clone(), egui::Color32::from_gray(225));
            let r = align.anchor_size(at, galley.size()).expand(4.0);
            painter.rect_filled(r, 3.0, egui::Color32::from_black_alpha(170));
            painter.galley(r.min + egui::vec2(4.0, 4.0), galley, egui::Color32::WHITE);
        };
        let shown_title = self.shown().map(|(_, t)| t).unwrap_or_default();
        let title = if shown_title.trim().is_empty() {
            String::new()
        } else {
            format!("{}   ", shown_title.trim())
        };
        label(
            &painter,
            rect.left_top() + egui::vec2(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            format!("{title}{} × {}   {}", image.width, image.height, layer.name),
        );
        label(
            &painter,
            rect.right_top() + egui::vec2(-8.0, 8.0),
            egui::Align2::RIGHT_TOP,
            // Image pixels to screen pixels: 100% at 1:1.
            format!("{:.0}%", s * ppp * 100.0),
        );

        // The pixel under the pointer, and its values.
        if let Some(at) = resp.hover_pos() {
            let p = (at - image_min) / s;
            let (x, y) = (p.x.floor() as i64, p.y.floor() as i64);
            let values = image.probe(&layer, x, y);
            if !values.is_empty() {
                let text: Vec<String> = values
                    .iter()
                    .map(|(n, v)| format!("{} {v:.4}", n.rsplit('.').next().unwrap_or(n)))
                    .collect();
                label(
                    &painter,
                    rect.left_bottom() + egui::vec2(8.0, -8.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!(
                        "{}, {}   {}",
                        x + image.origin.0 as i64,
                        y + image.origin.1 as i64,
                        text.join("   ")
                    ),
                );
            }
        }
    }
}

/// A snapshots bar: which image is shown (the menu, the slider, − and +: the
/// snapshots oldest first, the active image, `active_label`, last), Snap, the
/// name of the one shown, ✕ to let it go. `showing`: the one this view shows,
/// by id. `active`: the image Snap keeps, made only when Snap is pressed;
/// `None` greys Snap.
pub fn snapshots_bar<F: FnOnce() -> (Arc<Image>, String)>(
    ui: &mut egui::Ui,
    id_salt: &str,
    snaps: &Mutex<Snaps>,
    showing: &mut Option<usize>,
    active_label: &str,
    active: Option<F>,
) {
    let mut snaps = snaps.lock().unwrap_or_else(|e| e.into_inner());
    let n = snaps.list.len();
    let at = snaps.position(*showing);
    let label = |p: usize, snaps: &Snaps| match snaps.list.get(p) {
        Some(s) => s.name.clone(),
        None => active_label.to_string(),
    };
    let mut pick = None;
    egui::ComboBox::from_id_salt((id_salt, "menu"))
        .width(140.0)
        .selected_text(label(at, &*snaps))
        .show_ui(ui, |ui| {
            for p in 0..=n {
                if ui.selectable_label(at == p, label(p, &*snaps)).clicked() {
                    pick = Some(p);
                }
            }
        })
        .response
        .on_hover_text(format!(
            "The image shown: a snapshot, or {}",
            active_label.to_lowercase()
        ));
    if let Some(p) = pick {
        *showing = snaps.at(p);
    }
    if ui
        .add_enabled(at < n, egui::Button::new("✕"))
        .on_hover_text("Let this snapshot go (from every pane)")
        .clicked()
    {
        if let Some(id) = *showing {
            *showing = snaps.remove(id);
        }
    }
    let n = snaps.list.len();
    ui.add_enabled_ui(n > 0, |ui| {
        ui.spacing_mut().slider_width = (ui.available_width() * 0.4).clamp(80.0, 360.0);
        let mut p = snaps.position(*showing);
        if ui
            .add(egui::Slider::new(&mut p, 0..=n.max(1)).show_value(false))
            .on_hover_text(format!(
                "The snapshots, oldest first; {} at the right end",
                active_label.to_lowercase()
            ))
            .changed()
        {
            *showing = snaps.at(p.min(n));
        }
        let p = snaps.position(*showing);
        if ui.small_button("−").on_hover_text("The one before").clicked() {
            *showing = snaps.at(p.saturating_sub(1));
        }
        if ui.small_button("+").on_hover_text("The one after").clicked() {
            *showing = snaps.at((p + 1).min(n));
        }
    });
    ui.separator();
    let snap = ui
        .add_enabled(active.is_some(), egui::Button::new("📷 Snap"))
        .on_hover_text("Keep the active image, to compare with what comes next (in every pane's snapshots)")
        .clicked();
    if let (true, Some(active)) = (snap, active) {
        let (image, title) = active();
        *showing = Some(snaps.keep(image, title));
    }
    if let Some(s) = snaps.list.iter_mut().find(|s| Some(s.id) == *showing) {
        ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(140.0))
            .on_hover_text("The snapshot's name");
    }
    if n > 0 || snap {
        let kept = snaps.list.len();
        ui.label(
            egui::RichText::new(format!("{kept} kept, {:.0} MB", snaps.bytes() as f64 / 1_048_576.0))
                .small()
                .weak(),
        );
    }
}
