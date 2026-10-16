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
    pub subnets:   &'a crate::ice::IceGraphStore,
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
    /// The scene camera looked through last frame.
    looked:     Option<String>,
    /// The Viewer's own camera from before it looked through one: back to
    /// it when Perspective is chosen again.
    saved:      Option<camera::Camera>,
}

impl Viewer {
    pub fn show(&mut self, ui: &mut egui::Ui, input: ViewerInput) {
        let rect = ui.max_rect();
        ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let aspect = rect.width() / rect.height().max(1.0);
        self.follow_look_through();
        self.navigate(ui.ctx(), rect, &input.nav, input.playback);
        self.frame_keys(ui.ctx(), rect, &input);

        // The scene again only when what it is made from changed.
        let key = (
            input.revision,
            input.playback.time.to_bits(),
            crate::theme::revision() ^ crate::viewport::textures::generation().rotate_left(32),
            input.hierarchy.display_revision,
            // The camera looked through is not drawn.
            crate::viewport::display::mode() as u64 ^ (self.looked.as_deref().map(hash_of).unwrap_or(0) << 8),
        );
        if self.built != Some(key) {
            self.built = Some(key);
            self.generation = self.generation.wrapping_add(1);
            let inputs = scene::Inputs {
                graph: &*input.graph, subnets: input.subnets, hierarchy: input.hierarchy, time: input.playback.time,
            };
            self.scene = Arc::new(scene::build(&inputs, &mut self.cache, self.generation));
        }

        // Through a camera of the scene, or the Viewer's own.
        let looking = self.looking();
        let (view_proj, eye) = match &looking {
            Some((_, world, cam)) => (cam.view_proj(*world, aspect), world.w_axis.truncate()),
            None => (self.camera.view_proj(aspect), self.camera.eye),
        };

        // Edit Poly: picking, manipulators, and the lines they draw.
        let view = crate::core::poly::PickView {
            view_proj,
            size:      glam::Vec2::new(rect.width(), rect.height()),
            eye,
        };
        let overlay = edit::run(ui.ctx(), rect, &view, input.graph, input.subnets, &input.nav, input.tool, &mut self.edit_drag);

        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32];
        let frame = gpu::Frame {
            view_proj,
            eye,
            size,
            clear:     scene::linear(crate::theme::viewport_bg()),
            scene:     self.scene.clone(),
            overlay,
        };
        ui.painter().add(eframe::egui_wgpu::Callback::new_paint_callback(rect, gpu::ViewerPaint(frame)));
        self.origin_label(ui, rect, view_proj);
        if let Some((path, _, cam)) = &looking { gate(ui, rect, &path, cam); }
    }

    /// The camera of the scene looked through, as the scene has it now.
    fn looking(&self) -> Option<(String, glam::Mat4, crate::core::camera::Camera)> {
        let path = camera::looking_through()?;
        self.scene.cameras.iter().find(|c| c.0 == path).cloned()
    }

    /// Choosing a camera keeps the Viewer's own; choosing Perspective again
    /// gives it back as it was.
    fn follow_look_through(&mut self) {
        let now = camera::looking_through();
        if now == self.looked { return; }
        match (&self.looked, &now) {
            (None, Some(_)) => self.saved = Some(self.camera),
            (Some(_), None) => if let Some(c) = self.saved.take() { self.camera = c; },
            _ => {}
        }
        self.looked = now;
    }

    /// Navigating or framing leaves the camera looked through: the Viewer's
    /// own takes its place, where it was, its field of view and its focus
    /// distance (or five metres) to turn about.
    fn leave_camera(&mut self, aspect: f32) {
        let Some((_, world, cam)) = self.looking() else { return };
        let (at, forward, _, unit) = crate::core::camera::frame(world);
        let dist = if cam.focus_distance > 0.0 { cam.focus_distance * unit } else { 5.0 };
        self.camera = camera::Camera { eye: at, target: at + forward * dist, fov: cam.fov(aspect) };
        camera::look_through(None);
        self.looked = None;
        self.saved = None;
    }

    /// "0,0,0" at the origin, while it is inside the Viewer.
    fn origin_label(&self, ui: &egui::Ui, rect: egui::Rect, view_proj: glam::Mat4) {
        let p = view_proj * glam::Vec4::new(0.0, 0.0, 0.0, 1.0);
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
        if ctx.egui_wants_keyboard_input() { return; }
        let (selection, all, held, pointer) = ctx.input(|i| {
            let k = |key| i.key_pressed(key);
            (k(egui::Key::F) || k(egui::Key::G) || k(egui::Key::Z) || k(egui::Key::Period),
             k(egui::Key::A) || k(egui::Key::H),
             i.modifiers.ctrl || i.modifiers.alt || i.modifiers.mac_cmd || i.key_down(egui::Key::Space) || i.key_down(egui::Key::S),
             i.pointer.hover_pos())
        });
        if !(selection || all) || held || !pointer.is_some_and(|p| rect.contains(p)) { return; }

        let subnets = input.subnets;
        let eval = |sid: crate::types::IceGraphId, mesh: &crate::core::geo::Geo, template: Option<&crate::core::geo::Geo>| {
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
        self.leave_camera(rect.width() / rect.height().max(1.0));
        let dir = self.camera.eye - self.camera.target;
        self.camera.eye = camera::frame_position(centre, radius, dir, self.camera.fov, rect.width() / rect.height().max(1.0));
        self.camera.target = centre;
        ctx.request_repaint();
    }

    /// Orbit, pan and zoom, with the keys and buttons of the navigation style.
    fn navigate(&mut self, ctx: &egui::Context, rect: egui::Rect, nav: &NavSettings, playback: &mut crate::timeline::Playback) {
        let typing = ctx.egui_wants_keyboard_input();
        let (mut h, pressed, delta, wheel, pointer, space_down, space_up) = ctx.input(|i| {
            (held(i), i.pointer.any_pressed(), i.pointer.delta(), wheel_points(i), i.pointer.hover_pos(),
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
        if action.is_some() || (wheel != 0.0 && over) { self.leave_camera(rect.width() / rect.height().max(1.0)); }
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

fn hash_of(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Looking through a camera: what is outside its gate darkened, the gate's
/// edge, and the camera's name at the top, in the middle.
fn gate(ui: &egui::Ui, rect: egui::Rect, path: &str, cam: &crate::core::camera::Camera) {
    let (w, h) = cam.gate_in_view(rect.width() / rect.height().max(1.0));
    let g = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width() * w, rect.height() * h));
    let painter = ui.painter_at(rect);
    let shade = egui::Color32::from_black_alpha(120);
    if g.left() > rect.left() + 0.5 {
        painter.rect_filled(egui::Rect::from_min_max(rect.min, egui::pos2(g.left(), rect.bottom())), 0.0, shade);
        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(g.right(), rect.top()), rect.max), 0.0, shade);
    }
    if g.top() > rect.top() + 0.5 {
        painter.rect_filled(egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), g.top())), 0.0, shade);
        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(rect.left(), g.bottom()), rect.max), 0.0, shade);
    }
    painter.rect_stroke(g.shrink(0.5), 0.0, egui::Stroke::new(1.0, egui::Color32::from_gray(190)), egui::StrokeKind::Inside);
    let name = path.rsplit('/').next().unwrap_or(path);
    painter.text(g.center_top() + egui::vec2(0.0, 10.0), egui::Align2::CENTER_TOP, format!("🎥 {name}"),
        egui::FontId::proportional(15.0), egui::Color32::from_gray(230));
}

/// The wheel this frame, in points as egui counted them before 0.33 (50 a
/// notch), straight from its events: not eased over the frames after, so a
/// notch zooms by the same step at once.
fn wheel_points(i: &egui::InputState) -> f32 {
    i.events.iter().map(|e| match e {
        egui::Event::MouseWheel { unit, delta, .. } => match unit {
            egui::MouseWheelUnit::Point => delta.y,
            egui::MouseWheelUnit::Line => delta.y * 50.0,
            egui::MouseWheelUnit::Page => delta.y * i.viewport_rect().height(),
        },
        _ => 0.0,
    }).sum()
}
