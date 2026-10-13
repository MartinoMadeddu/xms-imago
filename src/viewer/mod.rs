//! The Viewer's 3D view on wgpu, inside the egui interface (stage 2 of the
//! eframe rework). Bevy-free: the eframe shell owns a `Viewer` and the
//! Viewer pane calls `show`.
//!
//! - `camera`: orbit, pan and zoom, with the chosen navigation style.
//! - `scene`: what to draw, from the graph's output, rebuilt only when the
//!   graph, the playhead, the theme, the display mode or the Scene
//!   Explorer's choices change.
//! - `gpu`: the picture, drawn into a texture of its own and laid into the
//!   pane by egui's paint callback.

mod camera;
mod gpu;
mod scene;

use std::sync::Arc;

use glam::Vec2;

pub use gpu::install;
use crate::viewport::nav::{Held, NavAction, NavSettings, NavStyle};

/// What the Viewer reads, and the playback it can toggle (Houdini's Space).
pub struct ViewerInput<'a> {
    pub graph:     &'a crate::node_graph::NodeGraphState,
    pub subnets:   &'a crate::ice::SubnetStore,
    pub hierarchy: &'a crate::types::SceneHierarchy,
    pub playback:  &'a mut crate::timeline::Playback,
    pub revision:  u64,
    pub nav:       NavSettings,
}

#[derive(Default)]
struct Drag {
    /// The drag began in the Viewer.
    active:     bool,
    /// Space was used to navigate since it went down (Houdini style).
    space_used: bool,
}

#[derive(Default)]
pub struct Viewer {
    camera:     camera::Camera,
    drag:       Drag,
    scene:      Arc<scene::Scene>,
    cache:      scene::GeometryCache,
    /// What the scene was last built for.
    built:      Option<(u64, u64, u64, u64, u64)>,
    generation: u64,
}

impl Viewer {
    pub fn show(&mut self, ui: &mut egui::Ui, input: ViewerInput) {
        let rect = ui.max_rect();
        ui.allocate_rect(rect, egui::Sense::click_and_drag());
        self.navigate(ui.ctx(), rect, &input.nav, input.playback);

        // The scene again only when what it is made from changed.
        let key = (
            input.revision,
            input.playback.time.to_bits(),
            crate::theme::revision() ^ crate::viewport::textures::generation().rotate_left(32),
            input.hierarchy.display_revision,
            crate::viewport::display::mode() as u64,
        );
        if self.built != Some(key) {
            self.built = Some(key);
            self.generation = self.generation.wrapping_add(1);
            let inputs = scene::Inputs {
                graph: input.graph, subnets: input.subnets, hierarchy: input.hierarchy, time: input.playback.time,
            };
            self.scene = Arc::new(scene::build(&inputs, &mut self.cache, self.generation));
        }

        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32];
        let frame = gpu::Frame {
            view_proj: self.camera.view_proj(rect.width() / rect.height().max(1.0)),
            eye:       self.camera.eye,
            size,
            clear:     scene::linear(crate::theme::viewport_bg()),
            scene:     self.scene.clone(),
        };
        ui.painter().add(eframe::egui_wgpu::Callback::new_paint_callback(rect, gpu::ViewerPaint(frame)));
    }

    /// Orbit, pan and zoom, with the keys and buttons of the navigation style.
    fn navigate(&mut self, ctx: &egui::Context, rect: egui::Rect, nav: &NavSettings, playback: &mut crate::timeline::Playback) {
        let typing = ctx.wants_keyboard_input();
        let (mut h, pressed, delta, wheel, pointer, space_down, space_up) = ctx.input(|i| {
            let m = i.modifiers;
            let h = Held {
                // Option, and Command on a Mac, count as Alt, as under Bevy.
                alt:   m.alt || m.mac_cmd,
                shift: m.shift,
                ctrl:  m.ctrl,
                space: i.key_down(egui::Key::Space),
                s:     i.key_down(egui::Key::S),
                lmb:   i.pointer.button_down(egui::PointerButton::Primary),
                mmb:   i.pointer.button_down(egui::PointerButton::Middle),
                rmb:   i.pointer.button_down(egui::PointerButton::Secondary),
            };
            (h, i.pointer.any_pressed(), i.pointer.delta(), i.raw_scroll_delta.y, i.pointer.hover_pos(),
             i.key_pressed(egui::Key::Space), i.key_released(egui::Key::Space))
        });
        if typing { h.space = false; h.s = false; }

        // Houdini style: Space navigates while held, and plays on a tap.
        if nav.style == NavStyle::Houdini && !typing {
            if space_down { self.drag.space_used = false; }
            if h.space && (h.lmb || h.mmb || h.rmb) { self.drag.space_used = true; }
            if space_up && !self.drag.space_used { playback.playing = !playback.playing; }
        }

        // Over the Viewer itself, not a window or menu above it.
        let over = pointer.is_some_and(|p| rect.contains(p)
            && ctx.layer_id_at(p).map(|l| l == egui::LayerId::background()).unwrap_or(true));

        // A drag belongs to the camera only if it began in the Viewer.
        if pressed && !(h.lmb && h.mmb && self.drag.active) { self.drag.active = over; }
        if !(h.lmb || h.mmb || h.rmb) { self.drag.active = false; }

        let mut d = Vec2::new(delta.x, delta.y);
        // A jump this size is the cursor being placed, not dragged.
        if d.length() > 400.0 { d = Vec2::ZERO; }
        let action = if self.drag.active { nav.style.action(h) } else { None };
        match action {
            Some(NavAction::Orbit) => self.camera.orbit(d),
            Some(NavAction::Pan) => self.camera.pan(d, nav.style.pan_sign()),
            Some(NavAction::Zoom) => self.camera.zoom(nav.zoom(d) * 0.01),
            None => {}
        }
        // egui gives the wheel in points, 50 a notch.
        if wheel != 0.0 && over { self.camera.zoom(wheel / 50.0 * 0.12); }
        if action.is_some() { ctx.request_repaint(); }
    }
}
