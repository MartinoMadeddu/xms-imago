//! Edit Poly in the Viewer: picking components, the manipulators, and the
//! wireframe and selection drawn over the mesh being edited. The same
//! behaviour as under Bevy (`modelling::pick_system`, `overlay_system`), on
//! egui's input and the Viewer's own lines.
//!
//! Click replaces the selection, Ctrl adds, Shift removes; dragging draws a
//! box. Alt (or whatever the navigation style takes the left button with)
//! is left to the camera. Q, W, E, R switch between select, move, rotate and
//! scale; dragging a manipulator handle writes a Transform operation.
//! Delete adds a Delete operation.

use glam::{Quat, Vec2, Vec3};

use crate::core::manip::{self, Handle, Tool, AXES, RING_STEPS};
use crate::core::poly::{self, Component as Picked, PickMode, PickView, PolyMesh, PolyOp, PolyOpKind, PolySelection, SubLevel};
use crate::modelling::{self, Drag, PolyTool};
use crate::types::NodeType;

use super::scene::linear_f;

/// Lines drawn this frame over the scene: `depth` hidden by what is in
/// front (the wireframe on its surface), `top` over everything (the
/// manipulator). x, y, z and a linear colour per vertex, in pairs.
#[derive(Default)]
pub struct Overlay {
    pub depth: Vec<f32>,
    pub top:   Vec<f32>,
}

fn line(out: &mut Vec<f32>, a: Vec3, b: Vec3, c: [f32; 4]) {
    out.extend_from_slice(&a.to_array()); out.extend_from_slice(&c);
    out.extend_from_slice(&b.to_array()); out.extend_from_slice(&c);
}

/// An sRGB colour with alpha, as the overlay's linear colour.
fn colour(r: f32, g: f32, b: f32, a: f32) -> [f32; 4] {
    let c = linear_f([r, g, b]);
    [c[0], c[1], c[2], a]
}

/// A ring of lines about `centre`, square to `normal`.
fn circle(out: &mut Vec<f32>, centre: Vec3, normal: Vec3, radius: f32, c: [f32; 4]) {
    let n = normal.normalize_or_zero();
    let u = n.any_orthonormal_vector();
    let v = n.cross(u);
    let at = |k: usize| { let a = k as f32 / 24.0 * std::f32::consts::TAU; centre + (u * a.cos() + v * a.sin()) * radius };
    for k in 0..24 { line(out, at(k), at(k + 1), c); }
}

/// A line with a head at `tip`.
fn arrow(out: &mut Vec<f32>, from: Vec3, tip: Vec3, c: [f32; 4]) {
    line(out, from, tip, c);
    let d = tip - from;
    let len = d.length();
    if len < 1e-8 { return; }
    let dir = d / len;
    let u = dir.any_orthonormal_vector();
    let v = dir.cross(u);
    let back = tip - dir * len * 0.12;
    for side in [u, -u, v, -v] { line(out, tip, back + side * len * 0.05, c); }
}

/// Picking and manipulating for this frame, and the lines to draw for it.
/// Nothing when no Edit Poly node is selected.
pub fn run(
    ctx:      &egui::Context,
    rect:     egui::Rect,
    view:     &PickView,
    graph:    &mut crate::node_graph::NodeGraphState,
    subnets:  &crate::ice::SubnetStore,
    nav:      &crate::viewport::nav::NavSettings,
    tool:     &mut PolyTool,
    drag:     &mut Drag,
) -> Overlay {
    let eval = |sid: crate::types::SubnetId, mesh: &crate::core::geo::Geo, template: Option<&crate::core::geo::Geo>| {
        subnets.get(sid).map(|sg| sg.evaluate(mesh, template)).unwrap_or_else(|| mesh.clone())
    };
    let Some(stage) = modelling::stage(graph, &eval) else { *drag = Drag::Idle; tool.hot = None; return Overlay::default() };
    let origin = Vec2::new(rect.min.x, rect.min.y);
    let (cursor, pressed, down, keys, held, shift, ctrl) = ctx.input(|i| (
        i.pointer.hover_pos().map(|p| Vec2::new(p.x, p.y)),
        i.pointer.button_pressed(egui::PointerButton::Primary),
        i.pointer.button_down(egui::PointerButton::Primary),
        [i.key_pressed(egui::Key::Q), i.key_pressed(egui::Key::W), i.key_pressed(egui::Key::E), i.key_pressed(egui::Key::R), i.key_pressed(egui::Key::Delete)],
        super::held(i),
        i.modifiers.shift,
        i.modifiers.ctrl,
    ));
    // Keys that hand the left button to the camera in the navigation style.
    let alt = nav.style.claims_left_button(held);
    let over = cursor.is_some_and(|c| {
        let p = egui::pos2(c.x, c.y);
        rect.contains(p) && ctx.layer_id_at(p).map(|l| l == egui::LayerId::background()).unwrap_or(true)
    });
    let free = over && !alt;
    let typing = ctx.wants_keyboard_input();

    if !typing && !ctrl && !alt {
        for (k, t) in [Tool::Select, Tool::Move, Tool::Rotate, Tool::Scale].into_iter().enumerate() {
            if keys[k] { tool.tool = t; }
        }
    }

    let gizmo = stage.gizmo(tool.tool, view);
    tool.hot = match &*drag {
        Drag::Handle { handle, .. } => Some(*handle),
        Drag::Idle if free => gizmo.and_then(|(c, size)| manip::hit_handle(view, tool.tool, c, size, cursor.unwrap() - origin, 9.0)),
        _ => None,
    };

    // Delete: a Delete operation on the current selection.
    if free && !typing && !stage.editing && keys[4] && stage.selection.count(&stage.mesh) > 0 {
        if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == stage.node) {
            if let NodeType::EditPoly { ops, pending, auto_collapse, .. } = &mut node.node_type {
                let level = pending.level;
                poly::push_op(ops, *auto_collapse, PolyOp::new(pending.clone(), PolyOpKind::Delete));
                *pending = PolySelection { level, ..Default::default() };
            }
        }
        return Overlay::default();
    }

    if pressed {
        *drag = Drag::Idle;
        if let (true, Some(c)) = (free, cursor) {
            *drag = Drag::Marquee(c);
            if let (Some(handle), Some((pivot, size))) = (tool.hot, gizmo) {
                // Start a handle drag: continue the last Transform if it is
                // live and on this selection, otherwise add a new one.
                if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == stage.node) {
                    if let NodeType::EditPoly { ops, pending, auto_collapse, .. } = &mut node.node_type {
                        pending.bake(&stage.mesh);
                        let reuse = ops.last()
                            .filter(|op| op.enabled && !op.collapsed && op.selection == *pending)
                            .and_then(|op| modelling::transform_of(&op.kind))
                            // A scale along world axes only combines with an unrotated transform.
                            .filter(|(_, r, _)| tool.tool != Tool::Scale || r.angle_between(Quat::IDENTITY) < 1e-4);
                        let base = match reuse {
                            Some(b) => b,
                            None => {
                                poly::push_op(ops, *auto_collapse, PolyOp::new(pending.clone(), PolyOpKind::identity_transform()));
                                (Vec3::ZERO, Quat::IDENTITY, Vec3::ONE)
                            }
                        };
                        *drag = Drag::Handle { handle, start: c - origin, pivot, size, op: ops.len() - 1, base };
                    }
                }
            }
        }
    }

    match &*drag {
        Drag::Idle => {}
        Drag::Handle { handle, start, pivot, size, op, base } => {
            if !down { *drag = Drag::Idle; }
            else {
                let now = cursor.map(|c| c - origin).unwrap_or(*start);
                let delta = manip::drag_delta(view, tool.tool, *handle, *pivot, *size, *start, now);
                let (t, r, s) = manip::compose(*base, delta);
                if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == stage.node) {
                    if let NodeType::EditPoly { ops, .. } = &mut node.node_type {
                        if let Some(PolyOpKind::Transform { translate, rotate, scale, .. }) = ops.get_mut(*op).map(|o| &mut o.kind) {
                            let new = (t.to_array(), r.to_array(), s.to_array());
                            if (*translate, *rotate, *scale) != new { (*translate, *rotate, *scale) = new; }
                        }
                    }
                }
            }
        }
        Drag::Marquee(start) => {
            let start = *start;
            let now = cursor.unwrap_or(start);
            let boxed = start.distance(now) > 4.0;
            if down {
                if boxed {
                    let r = egui::Rect::from_two_pos(egui::pos2(start.x, start.y), egui::pos2(now.x, now.y));
                    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("poly_marquee")));
                    painter.rect_filled(r, 0.0, egui::Color32::from_rgba_unmultiplied(120, 170, 255, 30));
                    painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(150, 190, 255)));
                }
            } else {
                // Released: apply.
                *drag = Drag::Idle;
                let level = stage.selection.level;
                let picked: Vec<Picked> = if boxed {
                    poly::pick_rect(&stage.mesh, view, level, start - origin, now - origin)
                } else {
                    poly::pick_point(&stage.mesh, view, level, now - origin, 9.0).into_iter().collect()
                };
                let picked = poly::widen_pick(&stage.mesh, level, picked);
                let mode = if ctrl { PickMode::Add } else if shift { PickMode::Remove } else { PickMode::Replace };
                if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == stage.node) {
                    if let NodeType::EditPoly { ops, pending, edit, .. } = &mut node.node_type {
                        let target = match edit.filter(|i| *i < ops.len()) {
                            Some(i) => &mut ops[i].selection,
                            None    => pending,
                        };
                        target.apply_pick(&stage.mesh, &picked, mode);
                    }
                }
            }
        }
    }
    if matches!(drag, Drag::Handle { .. } | Drag::Marquee(_)) { ctx.request_repaint(); }
    overlay(&stage, tool, view)
}

/// The manipulator, and the wireframe and selection of the mesh being
/// edited. Only the side facing the camera is drawn, which is also the side
/// that can be picked.
fn overlay(stage: &modelling::Stage, tool: &PolyTool, view: &PickView) -> Overlay {
    let mut o = Overlay::default();
    let mesh: &PolyMesh = &stage.mesh;

    // Manipulator, drawn over everything.
    if let Some((c, size)) = stage.gizmo(tool.tool, view) {
        let col = |h: Handle| {
            if tool.hot == Some(h) { return colour(1.0, 0.9, 0.2, 1.0); }
            match h {
                Handle::Axis(0) => colour(0.92, 0.22, 0.22, 1.0),
                Handle::Axis(1) => colour(0.35, 0.8, 0.2, 1.0),
                Handle::Axis(_) => colour(0.25, 0.5, 1.0, 1.0),
                Handle::Centre  => colour(0.9, 0.9, 0.9, 1.0),
            }
        };
        for a in 0..3 {
            let c_a = col(Handle::Axis(a));
            let tip = c + AXES[a] * size;
            match tool.tool {
                Tool::Move => arrow(&mut o.top, c, tip, c_a),
                Tool::Scale => {
                    line(&mut o.top, c, tip, c_a);
                    let k = size * 0.06;
                    for b in 0..3 { line(&mut o.top, tip - AXES[b] * k, tip + AXES[b] * k, c_a); }
                }
                Tool::Rotate => for k in 0..RING_STEPS {
                    line(&mut o.top, manip::ring_point(c, size, a, k), manip::ring_point(c, size, a, k + 1), c_a);
                },
                Tool::Select => {}
            }
        }
        if tool.tool != Tool::Rotate {
            circle(&mut o.top, c, view.eye - c, size * 0.07, col(Handle::Centre));
        }
    }

    let (vis_verts, vis_edges) = poly::visible_components(mesh, view);
    if vis_edges.len() > 60_000 { return o; }   // too dense to be useful as lines

    let sel = stage.selection.resolve(mesh);
    let wire     = colour(0.86, 0.88, 0.92, 0.55);
    let selected = colour(1.0, 0.32, 0.18, 1.0);
    let point    = colour(0.35, 0.65, 1.0, 1.0);

    // Edges of selected polygons are outlined too, so a polygon selection reads clearly.
    let mask = stage.selection.poly_mask(mesh);
    let mut outlined = sel.edges.clone();
    for (p, poly) in mesh.polys.iter().enumerate() {
        if !mask[p] { continue; }
        for i in 0..poly.len() { outlined.insert(poly::edge_key(poly[i], poly[(i + 1) % poly.len()])); }
    }
    for e in &vis_edges {
        let (a, b) = (mesh.verts[e[0] as usize], mesh.verts[e[1] as usize]);
        line(&mut o.depth, a, b, if outlined.contains(e) { selected } else { wire });
    }
    if stage.selection.level == SubLevel::Vertex && mesh.verts.len() <= 8_000 {
        for (v, pos) in mesh.verts.iter().enumerate() {
            if !vis_verts[v] { continue; }
            let r = pos.distance(view.eye) * 0.006;
            let (c, r) = if sel.verts[v] { (selected, r * 1.6) } else { (point, r) };
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] { line(&mut o.depth, *pos - axis * r, *pos + axis * r, c); }
        }
    }
    o
}
