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

pub(crate) mod camera;
mod edit;
mod gpu;
mod scene;

use std::sync::Arc;

use glam::Vec2;

pub use gpu::install;
use crate::viewport::nav::{Held, NavAction, NavSettings, NavStyle};

/// What the Viewer reads, and the playback it can toggle (Houdini's Space).
pub struct ViewerInput<'a> {
    /// Mutable: Edit Poly's picking and manipulators write to its node.
    pub graph:     &'a mut crate::node_graph::NodeGraphState,
    pub subnets:   &'a crate::ice::SubnetStore,
    pub hierarchy: &'a crate::types::SceneHierarchy,
    pub playback:  &'a mut crate::timeline::Playback,
    pub revision:  u64,
    pub nav:       NavSettings,
    /// Edit Poly's tool (select, move, rotate, scale) and its hot handle.
    pub tool:      &'a mut crate::modelling::PolyTool,
}

/// Keys and buttons as the navigation styles see them. Option, and Command
/// on a Mac, count as Alt, as under Bevy.
pub(crate) fn held(i: &egui::InputState) -> Held {
    let m = i.modifiers;
    Held {
        alt:   m.alt || m.mac_cmd,
        shift: m.shift,
        ctrl:  m.ctrl,
        space: i.key_down(egui::Key::Space),
        s:     i.key_down(egui::Key::S),
        lmb:   i.pointer.button_down(egui::PointerButton::Primary),
        mmb:   i.pointer.button_down(egui::PointerButton::Middle),
        rmb:   i.pointer.button_down(egui::PointerButton::Secondary),
    }
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
    /// What the left button is doing for Edit Poly.
    edit_drag:  crate::modelling::Drag,
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
        self.frame_keys(ui.ctx(), rect, &input);

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
                graph: &*input.graph, subnets: input.subnets, hierarchy: input.hierarchy, time: input.playback.time,
            };
            self.scene = Arc::new(scene::build(&inputs, &mut self.cache, self.generation));
        }

        // Edit Poly: picking, manipulators, and the lines they draw.
        let aspect = rect.width() / rect.height().max(1.0);
        let view = crate::core::poly::PickView {
            view_proj: self.camera.view_proj(aspect),
            size:      glam::Vec2::new(rect.width(), rect.height()),
            eye:       self.camera.eye,
        };
        let overlay = edit::run(ui.ctx(), rect, &view, input.graph, input.subnets, &input.nav, input.tool, &mut self.edit_drag);

        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32];
        let frame = gpu::Frame {
            view_proj: self.camera.view_proj(rect.width() / rect.height().max(1.0)),
            eye:       self.camera.eye,
            size,
            clear:     scene::linear(crate::theme::viewport_bg()),
            scene:     self.scene.clone(),
            overlay,
        };
        ui.painter().add(eframe::egui_wgpu::Callback::new_paint_callback(rect, gpu::ViewerPaint(frame)));
        self.origin_label(ui, rect);
    }

    /// "0,0,0" at the origin, while it is inside the Viewer.
    fn origin_label(&self, ui: &egui::Ui, rect: egui::Rect) {
        let p = self.camera.view_proj(rect.width() / rect.height().max(1.0)) * glam::Vec4::new(0.0, 0.0, 0.0, 1.0);
        if p.w <= 0.0 { return; }
        let at = egui::pos2(rect.min.x + (p.x / p.w * 0.5 + 0.5) * rect.width(), rect.min.y + (0.5 - p.y / p.w * 0.5) * rect.height());
        if !rect.shrink(12.0).contains(at) { return; }
        let painter = ui.painter_at(rect);
        let galley = painter.layout_no_wrap("0,0,0".into(), egui::FontId::proportional(10.0), egui::Color32::WHITE);
        let frame = egui::Rect::from_min_size(at, galley.size() + egui::vec2(8.0, 8.0));
        painter.rect_filled(frame, 0.0, egui::Color32::from_rgba_premultiplied(0, 0, 0, 180));
        painter.galley(at + egui::vec2(4.0, 4.0), galley, egui::Color32::WHITE);
    }

    /// F, G, Z or . frames the selection (an Edit Poly node's selected
    /// elements, or its whole mesh); A or H frames everything shown. Keys
    /// from Maya, Houdini, Max, Blender and Modo. The pointer has to be over
    /// the Viewer.
    fn frame_keys(&mut self, ctx: &egui::Context, rect: egui::Rect, input: &ViewerInput) {
        use glam::Vec3;
        if ctx.wants_keyboard_input() { return; }
        let (selection, all, held, pointer) = ctx.input(|i| {
            let k = |key| i.key_pressed(key);
            (k(egui::Key::F) || k(egui::Key::G) || k(egui::Key::Z) || k(egui::Key::Period),
             k(egui::Key::A) || k(egui::Key::H),
             i.modifiers.ctrl || i.modifiers.alt || i.modifiers.mac_cmd || i.key_down(egui::Key::Space) || i.key_down(egui::Key::S),
             i.pointer.hover_pos())
        });
        if !(selection || all) || held || !pointer.is_some_and(|p| rect.contains(p)) { return; }

        let subnets = input.subnets;
        let eval = |sid: crate::types::SubnetId, mesh: &crate::core::geo::Geo, template: Option<&crate::core::geo::Geo>| {
            subnets.get(sid).map(|sg| sg.evaluate(mesh, template)).unwrap_or_else(|| mesh.clone())
        };
        let mut target: Option<(Vec3, f32)> = None;
        if let Some(stage) = crate::modelling::stage(input.graph, &eval) {
            if selection {
                let picked = stage.selection.vertex_set(&stage.mesh);
                target = camera::bounds_of(stage.mesh.verts.iter().enumerate().filter(|(v, _)| picked[*v]).map(|(_, p)| *p));
                // One vertex has no size: show its surroundings.
                if let Some((_, r)) = &mut target {
                    let whole = camera::bounds_of(stage.mesh.verts.iter().copied()).map(|b| b.1).unwrap_or(1.0);
                    *r = r.max(whole * 0.08);
                }
            }
            if target.is_none() { target = camera::bounds_of(stage.mesh.verts.iter().copied()); }
        }
        if target.is_none() {
            let mut points: Vec<Vec3> = vec![];
            match input.graph.evaluate_for_viewport_packed(&eval) {
                // Primitives by their boxes: instanced copies are not made
                // into one mesh just to be measured.
                Some(crate::types::EvalResult::Named(prims)) => {
                    for (lo, hi) in prims.iter().filter_map(crate::viewport::bounds::world_bounds) { points.extend([lo, hi]); }
                }
                Some(other) => {
                    let mesh = other.shared_mesh();
                    points.extend(mesh.vertices.iter().map(|v| Vec3::from_array(*v)));
                    points.extend(mesh.points.iter().map(|v| Vec3::from_array(*v)));
                    points.extend(mesh.curve_points.iter().map(|v| Vec3::from_array(*v)));
                }
                None => {}
            }
            for clip in input.graph.display_clips() {
                points.extend(clip.world_pose(clip.index_at(input.playback.time)).iter().map(|m| m.w_axis.truncate()));
            }
            target = camera::bounds_of(points.into_iter());
        }
        let Some((centre, radius)) = target else { return };
        let dir = self.camera.eye - self.camera.target;
        self.camera.eye = camera::frame_position(centre, radius, dir, self.camera.fov, rect.width() / rect.height().max(1.0));
        self.camera.target = centre;
        ctx.request_repaint();
    }

    /// Orbit, pan and zoom, with the keys and buttons of the navigation style.
    fn navigate(&mut self, ctx: &egui::Context, rect: egui::Rect, nav: &NavSettings, playback: &mut crate::timeline::Playback) {
        let typing = ctx.wants_keyboard_input();
        let (mut h, pressed, delta, wheel, pointer, space_down, space_up) = ctx.input(|i| {
            (held(i), i.pointer.any_pressed(), i.pointer.delta(), i.raw_scroll_delta.y, i.pointer.hover_pos(),
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
