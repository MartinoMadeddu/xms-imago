//! Manipulators for operators that place something: the selected Transform
//! Camera or Light node gets the move, rotate and scale handles in the Viewer,
//! the same ones Edit Poly has (`core::manip`), and a drag writes straight into
//! the node's parameters. Q, W, E, R choose the tool; with Select the pivot is
//! shown as a small tripod, so it is clear where the handles would be. Nothing
//! when another kind of node is selected.
//!
//! A Transform node's pivot is its translation: its rotation and scale turn and
//! stretch about that point, which is what the handles show. A Camera's is the
//! camera itself.

use glam::{EulerRot, Quat, Vec2, Vec3};

use crate::core::manip::{self, Delta, Handle, Tool, AXES, RING_STEPS};
use crate::core::poly::PickView;
use crate::modelling::PolyTool;
use crate::node_graph::NodeGraphState;
use crate::types::{NodeId, NodeType};

use super::edit::{arrow, circle, colour, line, Overlay};

/// What the selected node offers to the handles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub node: NodeId,
    /// Where the handles sit.
    pub pivot: Vec3,
    /// Translate, rotate, scale as the node holds them now.
    pub base: (Vec3, Quat, Vec3),
    /// A clip's Transform scales the same on every axis.
    pub uniform: bool,
    /// Cameras have no size: no Scale tool.
    pub scales: bool,
}

/// The target the selected node makes, if any.
pub fn target(graph: &NodeGraphState) -> Option<Target> {
    let id = graph.selected_node?;
    let node = graph.nodes.iter().find(|n| n.id == id)?;
    // Keyed parameters as at the playhead.
    match &*graph.animated_type(node) {
        NodeType::Transform {
            translation,
            rotation,
            scale,
        } => {
            let uniform = node
                .inputs
                .first()
                .and_then(|i| i.connected_output)
                .and_then(|(src, _)| graph.nodes.iter().find(|n| n.id == src))
                .is_some_and(|n| n.node_type.is_anim());
            Some(Target {
                node: id,
                pivot: *translation,
                base: (
                    *translation,
                    Quat::from_euler(EulerRot::XYZ, rotation.x, rotation.y, rotation.z),
                    *scale,
                ),
                uniform,
                scales: true,
            })
        }
        NodeType::Camera { translate, rotate, .. } | NodeType::Light { translate, rotate, .. } => Some(Target {
            node: id,
            pivot: Vec3::from(*translate),
            base: (
                Vec3::from(*translate),
                Quat::from_euler(
                    EulerRot::XYZ,
                    rotate[0].to_radians(),
                    rotate[1].to_radians(),
                    rotate[2].to_radians(),
                ),
                Vec3::ONE,
            ),
            uniform: true,
            scales: false,
        }),
        _ => None,
    }
}

/// Put the part of a transform a drag changed back into the node: only that
/// part, so a move leaves the rotation written as it was.
pub fn write(graph: &mut NodeGraphState, id: NodeId, delta: Delta, (t, r, s): (Vec3, Quat, Vec3)) {
    let euler = || {
        let (x, y, z) = r.to_euler(EulerRot::XYZ);
        Vec3::new(x, y, z)
    };
    // As at the playhead: a keyed number dragged is keyed there.
    graph.edit_animated(id, |node_type| match node_type {
        NodeType::Transform {
            translation,
            rotation,
            scale,
        } => match delta {
            Delta::Translate(_) => {
                if *translation != t {
                    *translation = t;
                }
            }
            Delta::Rotate(_) => {
                let e = euler();
                if *rotation != e {
                    *rotation = e;
                }
            }
            Delta::Scale(_) => {
                if *scale != s {
                    *scale = s;
                }
            }
        },
        NodeType::Camera { translate, rotate, .. } | NodeType::Light { translate, rotate, .. } => match delta {
            Delta::Translate(_) => {
                if *translate != t.to_array() {
                    *translate = t.to_array();
                }
            }
            Delta::Rotate(_) => {
                let e = euler();
                let d = [e.x.to_degrees(), e.y.to_degrees(), e.z.to_degrees()];
                if *rotate != d {
                    *rotate = d;
                }
            }
            Delta::Scale(_) => {}
        },
        _ => {}
    });
}

/// A handle being dragged.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub node: NodeId,
    pub handle: Handle,
    pub start: Vec2,
    pub pivot: Vec3,
    pub size: f32,
    pub base: (Vec3, Quat, Vec3),
    pub tool: Tool,
}

/// The tool a target can use: Scale on a camera falls back to Move.
fn tool_for(t: &Target, tool: Tool) -> Tool {
    if tool == Tool::Scale && !t.scales {
        Tool::Move
    } else {
        tool
    }
}

/// Keys, hit test and drag for this frame; the handles to draw. Nothing when
/// the selected node has no handles. `free`: the cursor is over the Viewer and
/// the navigation style does not claim the left button.
pub fn run(
    ctx: &egui::Context,
    rect: egui::Rect,
    view: &PickView,
    graph: &mut NodeGraphState,
    tool: &mut PolyTool,
    drag: &mut Option<Drag>,
    free: bool,
) -> Overlay {
    let Some(t) = target(graph) else {
        *drag = None;
        return Overlay::default();
    };
    let origin = Vec2::new(rect.min.x, rect.min.y);
    let (cursor, pressed, down, keys, ctrl, alt) = ctx.input(|i| {
        (
            i.pointer.hover_pos().map(|p| Vec2::new(p.x - origin.x, p.y - origin.y)),
            i.pointer.button_pressed(egui::PointerButton::Primary),
            i.pointer.button_down(egui::PointerButton::Primary),
            [
                i.key_pressed(egui::Key::Q),
                i.key_pressed(egui::Key::W),
                i.key_pressed(egui::Key::E),
                i.key_pressed(egui::Key::R),
            ],
            i.modifiers.ctrl,
            i.modifiers.alt || i.modifiers.mac_cmd,
        )
    });
    if !ctx.egui_wants_keyboard_input() && !ctrl && !alt {
        for (k, which) in [Tool::Select, Tool::Move, Tool::Rotate, Tool::Scale]
            .into_iter()
            .enumerate()
        {
            if keys[k] {
                tool.tool = which;
            }
        }
    }
    let used = tool_for(&t, tool.tool);
    let size = manip::gizmo_size(view, t.pivot);

    // A drag in progress, or over.
    if let Some(d) = *drag {
        if !down || d.node != t.node {
            *drag = None;
        } else {
            let now = cursor.unwrap_or(d.start);
            let mut delta = manip::drag_delta(view, d.tool, d.handle, d.pivot, d.size, d.start, now);
            if let (true, Delta::Scale(s)) = (t.uniform, delta) {
                // One axis pulled scales all three alike.
                let k = if (s.x - 1.0).abs() > 1e-6 {
                    s.x
                } else if (s.y - 1.0).abs() > 1e-6 {
                    s.y
                } else {
                    s.z
                };
                delta = Delta::Scale(Vec3::splat(k));
            }
            write(graph, d.node, delta, manip::compose(d.base, delta));
            ctx.request_repaint();
        }
    }
    tool.hot = match drag {
        Some(d) => Some(d.handle),
        None if free => cursor.and_then(|c| manip::hit_handle(view, used, t.pivot, size, c, 9.0)),
        None => None,
    };
    if pressed && free && drag.is_none() {
        if let (Some(handle), Some(c)) = (tool.hot, cursor) {
            *drag = Some(Drag {
                node: t.node,
                handle,
                start: c,
                pivot: t.pivot,
                size,
                base: t.base,
                tool: used,
            });
        }
    }
    let mut o = Overlay::default();
    draw(&mut o, t.pivot, size, used, tool.hot, view);
    o
}

/// The handles of `tool` at `c`: arrows for Move, rings for Rotate, boxed lines
/// for Scale; with Select a small tripod marking the pivot.
pub fn draw(o: &mut Overlay, c: Vec3, size: f32, tool: Tool, hot: Option<Handle>, view: &PickView) {
    let col = |h: Handle| {
        if hot == Some(h) {
            return colour(1.0, 0.9, 0.2, 1.0);
        }
        match h {
            Handle::Axis(0) => colour(0.92, 0.22, 0.22, 1.0),
            Handle::Axis(1) => colour(0.35, 0.8, 0.2, 1.0),
            Handle::Axis(_) => colour(0.25, 0.5, 1.0, 1.0),
            Handle::Centre => colour(0.9, 0.9, 0.9, 1.0),
        }
    };
    for a in 0..3 {
        let c_a = col(Handle::Axis(a));
        let tip = c + AXES[a] * size;
        match tool {
            Tool::Move => arrow(&mut o.top, c, tip, c_a),
            Tool::Scale => {
                line(&mut o.top, c, tip, c_a);
                let k = size * 0.06;
                for b in 0..3 {
                    line(&mut o.top, tip - AXES[b] * k, tip + AXES[b] * k, c_a);
                }
            }
            Tool::Rotate => {
                for k in 0..RING_STEPS {
                    line(
                        &mut o.top,
                        manip::ring_point(c, size, a, k),
                        manip::ring_point(c, size, a, k + 1),
                        c_a,
                    );
                }
            }
            Tool::Select => {
                let dim = {
                    let mut x = c_a;
                    x[3] = 0.7;
                    x
                };
                line(&mut o.top, c, c + AXES[a] * size * 0.45, dim);
            }
        }
    }
    if tool != Tool::Rotate {
        circle(
            &mut o.top,
            c,
            view.eye - c,
            size * if tool == Tool::Select { 0.04 } else { 0.07 },
            col(Handle::Centre),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Mat4;

    fn view() -> PickView {
        let size = Vec2::new(800.0, 600.0);
        let eye = Vec3::new(3.0, 2.5, 5.0);
        let proj = Mat4::perspective_infinite_reverse_rh(0.8, size.x / size.y, 0.1);
        PickView {
            view_proj: proj * Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y),
            size,
            eye,
        }
    }

    /// A Transform node's handles sit at its translation; a drag along X moves
    /// it along X and nothing else; a ring turns it; a camera's rotation comes
    /// back in degrees.
    #[test]
    fn handles_write_into_the_node() {
        let mut g = NodeGraphState::default();
        let cube = g.add_node(
            "Cube".into(),
            NodeType::CreateCube {
                size: 1.0,
                location: "/cube".into(),
            },
            egui::pos2(0.0, 0.0),
        );
        let mv = g.add_node(
            "Move".into(),
            NodeType::Transform {
                translation: Vec3::new(0.2, 0.0, 0.0),
                rotation: Vec3::ZERO,
                scale: Vec3::ONE,
            },
            egui::pos2(0.0, 100.0),
        );
        g.add_connection(cube, 0, mv, 0);
        g.selected_node = Some(mv);
        let t = target(&g).unwrap();
        assert_eq!(t.pivot, Vec3::new(0.2, 0.0, 0.0));
        assert!(!t.uniform && t.scales);
        let v = view();
        let size = manip::gizmo_size(&v, t.pivot);
        let from = v.project(t.pivot + Vec3::X * 0.5).unwrap();
        let to = v.project(t.pivot + Vec3::X * 1.0).unwrap();
        let d = manip::drag_delta(&v, Tool::Move, Handle::Axis(0), t.pivot, size, from, to);
        write(&mut g, mv, d, manip::compose(t.base, d));
        let NodeType::Transform {
            translation,
            rotation,
            scale,
        } = &g.nodes.iter().find(|n| n.id == mv).unwrap().node_type
        else {
            panic!()
        };
        assert!(
            (translation.x - 0.7).abs() < 2e-3 && translation.y.abs() < 1e-4 && translation.z.abs() < 1e-4,
            "{translation}"
        );
        assert_eq!((*rotation, *scale), (Vec3::ZERO, Vec3::ONE));
        // A quarter turn about Y comes back as 90 degrees in the Euler.
        let t = target(&g).unwrap();
        let d = Delta::Rotate(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        write(&mut g, mv, d, manip::compose(t.base, d));
        let NodeType::Transform { rotation, .. } = &g.nodes.iter().find(|n| n.id == mv).unwrap().node_type else {
            panic!()
        };
        assert!((rotation.y - std::f32::consts::FRAC_PI_2).abs() < 1e-4, "{rotation}");

        let cam = g.add_node(
            "Cam".into(),
            NodeType::Camera {
                location: "/camera".into(),
                translate: [0.0, 1.0, 4.0],
                rotate: [0.0; 3],
                camera: Default::default(),
            },
            egui::pos2(0.0, 200.0),
        );
        g.selected_node = Some(cam);
        let t = target(&g).unwrap();
        assert!(!t.scales && t.pivot == Vec3::new(0.0, 1.0, 4.0));
        let d = Delta::Rotate(Quat::from_rotation_x(-30f32.to_radians()));
        write(&mut g, cam, d, manip::compose(t.base, d));
        let NodeType::Camera { rotate, .. } = &g.nodes.iter().find(|n| n.id == cam).unwrap().node_type else {
            panic!()
        };
        assert!((rotate[0] + 30.0).abs() < 1e-3, "{rotate:?}");
    }

    /// A Transform on a clip scales alike on every axis.
    #[test]
    fn a_clip_transform_scales_evenly() {
        let mut g = NodeGraphState::default();
        let clip = g.add_node(
            "Clip".into(),
            NodeType::TestClip {
                seconds: 1.0,
                fps_num: 30,
                fps_den: 1,
            },
            egui::pos2(0.0, 0.0),
        );
        let mv = g.add_node(
            "Move".into(),
            NodeType::Transform {
                translation: Vec3::ZERO,
                rotation: Vec3::ZERO,
                scale: Vec3::ONE,
            },
            egui::pos2(0.0, 100.0),
        );
        g.add_connection(clip, 0, mv, 0);
        g.selected_node = Some(mv);
        assert!(target(&g).unwrap().uniform);
        g.selected_node = Some(clip);
        assert!(target(&g).is_none());
    }
}
