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
mod gizmo;
mod gpu;
mod pick;
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
    /// Mutable: a click on a primitive selects it, as in the Scene Explorer.
    pub hierarchy: &'a mut crate::types::SceneHierarchy,
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
    /// A handle of a Transform or Camera node being dragged.
    gizmo_drag: Option<gizmo::Drag>,
    /// Where the left button went down over the Viewer, for a click that
    /// picks a primitive (a drag is not a click).
    press:      Option<egui::Pos2>,
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
    pub fn show(&mut self, ui: &mut egui::Ui, mut input: ViewerInput) {
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
        let mut overlay = edit::run(ui.ctx(), rect, &view, input.graph, input.subnets, &input.nav, input.tool, &mut self.edit_drag);
        // Handles of the selected Transform or Camera node.
        {
            let (cursor, held) = ui.ctx().input(|i| (i.pointer.hover_pos(), held(i)));
            let over = cursor.is_some_and(|p| rect.contains(p)
                && ui.ctx().layer_id_at(p).map(|l| l == egui::LayerId::background()).unwrap_or(true));
            let free = over && !input.nav.style.claims_left_button(held);
            let g = gizmo::run(ui.ctx(), rect, &view, input.graph, input.tool, &mut self.gizmo_drag, free);
            overlay.top.extend(g.top);
            self.pick_prims(ui.ctx(), rect, &view, &mut input, free, &mut overlay);
        }

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

    /// A click on a primitive selects it in the Scene Explorer (Ctrl adds
    /// or takes away, a click on nothing clears), and the selected
    /// primitives are outlined by their bounds. Not while an Edit Poly node
    /// is selected, whose clicks pick components, nor on a handle.
    fn pick_prims(&mut self, ctx: &egui::Context, rect: egui::Rect, view: &crate::core::poly::PickView, input: &mut ViewerInput, free: bool, overlay: &mut edit::Overlay) {
        let editing = input.graph.selected_node
            .and_then(|id| input.graph.nodes.iter().find(|n| n.id == id))
            .is_some_and(|n| matches!(n.node_type, crate::types::NodeType::EditPoly { .. }));
        let (pressed, released, pos, ctrl) = ctx.input(|i| (
            i.pointer.button_pressed(egui::PointerButton::Primary),
            i.pointer.button_released(egui::PointerButton::Primary),
            i.pointer.latest_pos(), i.modifiers.ctrl || i.modifiers.mac_cmd,
        ));
        if pressed { self.press = if free && !editing && input.tool.hot.is_none() && self.gizmo_drag.is_none() { pos } else { None }; }
        let subnets = input.subnets;
        let eval = |sid: crate::types::IceGraphId, mesh: &crate::core::geo::Geo, template: &[crate::ice::Reference]| {
            subnets.get(sid).map(|sg| sg.evaluate(mesh, template)).unwrap_or_else(|| mesh.clone())
        };
        let prims = match input.graph.evaluate_for_viewport_packed(&eval) {
            Some(crate::types::EvalResult::Named(p)) => p,
            _ => vec![],
        };
        if released {
            if let (Some(start), Some(end)) = (self.press.take(), pos) {
                if start.distance(end) <= 4.0 && free {
                    let origin = Vec2::new(rect.min.x, rect.min.y);
                    match pick::prim_at(view, Vec2::new(end.x, end.y) - origin, &prims) {
                        Some(path) => input.hierarchy.click(&path, ctrl, false, &[]),
                        None => if !ctrl { input.hierarchy.clear_selection() },
                    }
                    ctx.request_repaint();
                }
            }
        }
        if !input.hierarchy.selected_paths().is_empty() {
            let c = edit::colour(1.0, 0.75, 0.2, 1.0);
            for (lo, hi) in pick::selected_bounds(&prims, input.hierarchy.selected_paths()) {
                let p = |x: f32, y: f32, z: f32| glam::Vec3::new(x, y, z);
                let corners = [p(lo.x, lo.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, hi.y, lo.z), p(lo.x, hi.y, lo.z),
                               p(lo.x, lo.y, hi.z), p(hi.x, lo.y, hi.z), p(hi.x, hi.y, hi.z), p(lo.x, hi.y, hi.z)];
                for (a, b) in [(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)] {
                    edit::line(&mut overlay.top, corners[a], corners[b], c);
                }
            }
            // Lights and cameras: their own lines, in the selection's colour.
            for [a, b] in pick::selected_glyphs(&prims, input.hierarchy.selected_paths()) {
                edit::line(&mut overlay.top, a, b, c);
            }
        }
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
        let eval = |sid: crate::types::IceGraphId, mesh: &crate::core::geo::Geo, template: &[crate::ice::Reference]| {
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
