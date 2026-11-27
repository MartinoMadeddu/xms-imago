//! Painting in the Viewer, with a Paint operator selected (`core::paint`):
//! the left button over a prim paints its points, the brush a ring on the
//! surface. The prims painted are those coming into the operator, so a
//! point painted is the point the operator writes.
//!
//! While a stroke is painted only its values and their colours change; the
//! operator, and everything below it, cooks once when the stroke ends. The
//! operator's channel is shown on the prims it painted, a float through a
//! ramp, an ID in its palette colours, whatever the display flag shows.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use glam::{Mat4, Vec2, Vec3};

use crate::core::geo::Geo;
use crate::core::paint::{self, Channel, ChannelKind, Op, Surface};
use crate::types::{EvalResult, NodeId, NodeType};

use super::{edit, Viewer, ViewerInput};

/// What the Viewer colours: the selected Paint operator's channel on each
/// prim it painted (a stroke's values while one is painted).
#[derive(Clone, Default)]
pub struct PaintShow {
    pub prims: HashMap<String, Arc<Vec<f32>>>,
    pub kind: ChannelKind,
    /// Changes whenever what is shown does.
    pub key: u64,
}

/// A prim the brush can paint: as it comes into the operator.
pub struct Target {
    pub path: String,
    pub geo: Arc<Geo>,
    pub mesh: Arc<crate::types::MeshData>,
    pub world: Mat4,
    /// Made when first painted or hovered.
    pub surface: Option<Arc<Surface>>,
}

/// The prims coming into a Paint operator, kept while its input's cook is.
#[derive(Default)]
pub struct Targets {
    key: Option<u64>,
    pub list: Vec<Target>,
}

/// A stroke being painted.
pub struct Stroke {
    pub node: NodeId,
    pub target: usize,
    pub path: String,
    pub channel: Channel,
    pub values: Arc<Vec<f32>>,
    /// Where the last dab was.
    pub last: Vec3,
    /// Changes with each dab, for the Viewer to draw again.
    pub generation: u64,
}

/// The Paint operator selected, if any: its id, channels and active channel.
fn selected_paint(graph: &crate::node_graph::NodeGraphState) -> Option<(NodeId, Vec<Channel>, usize)> {
    let id = graph.selected_node?;
    let n = graph.nodes.iter().find(|n| n.id == id)?;
    match &n.node_type {
        NodeType::Paint { channels, active, .. } if !n.bypassed => Some((id, channels.clone(), *active)),
        _ => None,
    }
}

/// What to colour for the selected Paint operator, if one is.
pub fn show(graph: &crate::node_graph::NodeGraphState, stroke: Option<&Stroke>) -> Option<PaintShow> {
    let (id, channels, active) = selected_paint(graph)?;
    let channel = channels.get(active)?;
    let n = graph.nodes.iter().find(|n| n.id == id)?;
    let NodeType::Paint { data, .. } = &n.node_type else { return None };
    let mut prims: HashMap<String, Arc<Vec<f32>>> = data
        .prims
        .iter()
        .filter_map(|(path, p)| Some((path.clone(), p.values.get(&channel.name)?.clone())))
        .collect();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (id.0, active, data.hash, &channel.name, channel.kind == ChannelKind::Id).hash(&mut h);
    if let Some(s) = stroke.filter(|s| s.node == id && s.channel.name == channel.name) {
        s.generation.hash(&mut h);
        s.target.hash(&mut h);
        prims.insert(s.path.clone(), s.values.clone());
    }
    Some(PaintShow {
        prims,
        kind: channel.kind,
        key: h.finish(),
    })
}

/// A value's colour, linear: a float from grey through orange to a pale
/// yellow; an ID in its palette colour.
pub fn colour(v: f32, kind: ChannelKind) -> [f32; 4] {
    let lin = |c: egui::Color32| {
        let f = |x: u8| {
            let s = x as f32 / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        [f(c.r()), f(c.g()), f(c.b()), 1.0]
    };
    match kind {
        ChannelKind::Id => lin(crate::properties::paint::id_colour(v.round() as i32)),
        ChannelKind::Float => {
            let t = v.clamp(0.0, 1.0);
            let stops: [(f32, [f32; 3]); 4] = [
                (0.0, [0.16, 0.16, 0.16]),
                (0.35, [0.45, 0.10, 0.12]),
                (0.7, [0.95, 0.40, 0.06]),
                (1.0, [1.0, 0.88, 0.45]),
            ];
            for w in stops.windows(2) {
                let ((a, ca), (b, cb)) = (w[0], w[1]);
                if t <= b {
                    let k = (t - a) / (b - a);
                    return [ca[0] + (cb[0] - ca[0]) * k, ca[1] + (cb[1] - ca[1]) * k, ca[2] + (cb[2] - ca[2]) * k, 1.0];
                }
            }
            [1.0, 0.88, 0.45, 1.0]
        }
    }
}

impl Viewer {
    /// The prims coming into the selected Paint operator `id`.
    fn paint_targets(&mut self, input: &ViewerInput, id: NodeId) {
        let graph = &*input.graph;
        let Some(n) = graph.nodes.iter().find(|n| n.id == id) else { return };
        let Some((src, out)) = n.inputs.first().and_then(|i| i.connected_output) else {
            self.paint.targets = Targets::default();
            return;
        };
        let key = graph.cook_key(src, out).map(|k| k ^ (id.0 as u64).rotate_left(40));
        if key.is_some() && key == self.paint.targets.key {
            return;
        }
        let pass = |sid: crate::types::IceGraphId, mesh: &Geo, t: &[crate::ice::Reference]| {
            input.subnets.get(sid).map(|sg| sg.evaluate(mesh, t)).unwrap_or_else(|| mesh.clone())
        };
        let prims = match graph.eval_node_out(src, out, &mut Default::default(), &pass) {
            Some(EvalResult::Named(p)) => p,
            _ => vec![],
        };
        let list = prims
            .into_iter()
            .filter(|p| !p.mesh.indices.is_empty())
            .map(|p| {
                let geo = Arc::new(paint::geo_of(&p));
                Target {
                    world: p.place.matrices().first().copied().unwrap_or(Mat4::IDENTITY),
                    path: p.path.clone(),
                    geo,
                    mesh: p.mesh.clone(),
                    surface: None,
                }
            })
            .collect();
        self.paint.targets = Targets { key, list };
    }

    /// The brush: true while a Paint operator is selected (the left button
    /// is the brush's then, not selection's).
    pub(super) fn paint(
        &mut self,
        ctx: &egui::Context,
        rect: egui::Rect,
        view: &crate::core::poly::PickView,
        input: &mut ViewerInput,
        free: bool,
        overlay: &mut edit::Overlay,
    ) -> bool {
        let Some((id, channels, active)) = selected_paint(input.graph) else {
            self.paint.stroke = None;
            return false;
        };
        let Some(channel) = channels.get(active).cloned() else { return true };
        self.paint_targets(input, id);
        let (pos, pressed, down, smaller, larger) = ctx.input(|i| {
            (
                i.pointer.hover_pos(),
                i.pointer.button_pressed(egui::PointerButton::Primary),
                i.pointer.button_down(egui::PointerButton::Primary),
                i.key_pressed(egui::Key::OpenBracket),
                i.key_pressed(egui::Key::CloseBracket),
            )
        });
        let mut b = paint::brush();
        if free && (smaller || larger) {
            b.radius *= if larger { 1.25 } else { 0.8 };
            paint::set_brush(b);
        }

        // Under the pointer: the nearest prim hit.
        let hit = pos.filter(|p| rect.contains(*p)).and_then(|p| {
            let px = Vec2::new(p.x - rect.min.x, p.y - rect.min.y);
            let (o, d) = view.ray(px);
            let d = d.normalize_or_zero();
            let mut best: Option<(f32, usize)> = None;
            for (i, t) in self.paint.targets.list.iter().enumerate() {
                if let Some(dist) = super::pick::ray_mesh(o, d, &t.mesh, &t.world) {
                    if dist > 0.0 && best.is_none_or(|(b, _)| dist < b) {
                        best = Some((dist, i));
                    }
                }
            }
            best.map(|(t, i)| (o + d * t, i))
        });

        // Shift smooths while it is held (floats only).
        let shift = ctx.input(|i| i.modifiers.shift);
        let mut b = b;
        if shift && channel.kind == ChannelKind::Float {
            b.op = Op::Smooth;
        }

        // Flood: a click fills the prim under the brush.
        if b.op == Op::Flood {
            if pressed && free {
                if let Some((_, i)) = hit {
                    let s = self.surface(i);
                    let mut values = self.values_of(input, id, i, &channel);
                    paint::flood(Arc::make_mut(&mut values).as_mut_slice(), &s, &b, &channel);
                    self.commit(input, id, i, &channel, values, "flood");
                }
            }
            self.paint.stroke = None;
        } else
        // A stroke: begun over a prim, dabs along the way, kept when let go.
        if pressed && free {
            if let Some((at, i)) = hit {
                let values = self.values_of(input, id, i, &channel);
                self.paint.stroke = Some(Stroke {
                    node: id,
                    target: i,
                    path: self.paint.targets.list[i].path.clone(),
                    channel: channel.clone(),
                    values,
                    last: at,
                    generation: 0,
                });
                self.dab(at, &b);
            }
        } else if down {
            if let (Some(s), Some((at, i))) = (self.paint.stroke.as_ref(), hit) {
                if s.target == i {
                    // Dabs a quarter of the radius apart, from the last one.
                    let from = s.last;
                    let step = (b.radius * 0.25).max(1e-4);
                    let n = ((at - from).length() / step).floor() as usize;
                    for k in 1..=n.min(64) {
                        self.dab(from + (at - from) * (k as f32 / n as f32), &b);
                    }
                }
            }
        } else if let Some(s) = self.paint.stroke.take() {
            self.commit(input, s.node, s.target, &s.channel, s.values, "paint");
        }

        // The brush: a ring on the surface, about the normal there.
        if let Some((at, i)) = hit {
            let s = self.surface(i);
            let mut near = vec![];
            s.tree.near(at.to_array(), 1, 0.0, &mut near);
            let n = near.first().map(|(j, _)| s.normals[*j as usize]).unwrap_or(Vec3::Y);
            let (u, v) = n.any_orthonormal_pair();
            let c = [1.0, 1.0, 1.0, 1.0];
            let ring = |r: f32, out: &mut Vec<f32>| {
                for k in 0..48 {
                    let a0 = k as f32 / 48.0 * std::f32::consts::TAU;
                    let a1 = (k + 1) as f32 / 48.0 * std::f32::consts::TAU;
                    let p0 = at + (u * a0.cos() + v * a0.sin()) * r + n * r * 0.02;
                    let p1 = at + (u * a1.cos() + v * a1.sin()) * r + n * r * 0.02;
                    edit::line(out, p0, p1, c);
                }
            };
            ring(b.radius, &mut overlay.top);
            ring(b.radius * 0.5, &mut overlay.top);
            edit::line(&mut overlay.top, at, at + n * b.radius * 0.5, c);
        }
        true
    }

    /// The brush's view of target `i`, made once.
    fn surface(&mut self, i: usize) -> Arc<Surface> {
        let t = &mut self.paint.targets.list[i];
        t.surface
            .get_or_insert_with(|| Arc::new(Surface::new(&t.geo, t.world)))
            .clone()
    }

    /// The values of `channel` on target `i`: the operator's, else the
    /// channel's default at each point.
    fn values_of(&self, input: &ViewerInput, id: NodeId, i: usize, channel: &Channel) -> Arc<Vec<f32>> {
        let t = &self.paint.targets.list[i];
        let n = t.geo.point_count();
        let kept = input.graph.nodes.iter().find(|x| x.id == id).and_then(|x| match &x.node_type {
            NodeType::Paint { data, .. } => data
                .prims
                .get(&t.path)
                .filter(|p| p.topology == paint::topology_hash(&t.geo) && p.rest.len() == n)
                .and_then(|p| p.values.get(&channel.name).cloned()),
            _ => None,
        });
        kept.filter(|v| v.len() == n).unwrap_or_else(|| Arc::new(vec![channel.default; n]))
    }

    /// One dab of the stroke at `at`.
    fn dab(&mut self, at: Vec3, b: &paint::Brush) {
        let Some(target) = self.paint.stroke.as_ref().map(|s| s.target) else { return };
        let surface = self.surface(target);
        let Some(s) = self.paint.stroke.as_mut() else { return };
        if paint::dab(Arc::make_mut(&mut s.values).as_mut_slice(), &surface, at, b, &s.channel) > 0 {
            s.generation = s.generation.wrapping_add(1);
        }
        s.last = at;
    }

    /// Values painted on target `i` kept by the operator: a step of the
    /// history, and a cook.
    fn commit(&mut self, input: &mut ViewerInput, id: NodeId, i: usize, channel: &Channel, values: Arc<Vec<f32>>, verb: &str) {
        let Some(t) = self.paint.targets.list.get(i) else { return };
        let (path, geo) = (t.path.clone(), t.geo.clone());
        if let Some(n) = input.graph.nodes.iter_mut().find(|n| n.id == id) {
            if let NodeType::Paint { data, .. } = &mut n.node_type {
                *data.values_mut(&path, &geo, channel) = (*values).clone();
                data.rehash();
                crate::history::note_edit(format!("{}: {} {} on {path}", n.name, verb, channel.name));
            }
        }
    }
}

/// The Viewer's painting state.
#[derive(Default)]
pub struct State {
    pub targets: Targets,
    pub stroke: Option<Stroke>,
}

/// The brush's tools as a column of square buttons at the Viewer's left,
/// under `top`, while a Paint operator is selected: the same as its
/// Parameters, and Mirror X.
pub fn toolbar(ctx: &egui::Context, rect: egui::Rect, slot: u64, top: f32, graph: &crate::node_graph::NodeGraphState) {
    let Some((_, channels, active)) = selected_paint(graph) else { return };
    let kind = channels.get(active).map(|c| c.kind);
    let mut b = paint::brush();
    let before = b;
    egui::Area::new(egui::Id::new(("viewer_paint_tools", slot)))
        .order(egui::Order::Background)
        .fixed_pos(egui::pos2(rect.min.x + 10.0, top + 6.0))
        .constrain_to(rect)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(crate::theme::c(90, 90, 90).gamma_multiply(0.92))
                .stroke(egui::Stroke::new(1.0_f32, crate::theme::c(70, 70, 70)))
                .corner_radius(4.0)
                .inner_margin(3.0)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
                    ui.vertical(|ui| {
                        let size = egui::vec2(22.0, 22.0);
                        crate::properties::paint::tool_buttons(ui, &mut b, kind, size);
                        ui.separator();
                        let m = crate::properties::paint::square(ui, crate::icons::FLIP_HORIZONTAL, None, b.mirror, true, size)
                            .on_hover_text("Mirror X: also on the other side of the prim's own X = 0");
                        if m.clicked() {
                            b.mirror = !b.mirror;
                        }
                    });
                });
        });
    if b != before {
        paint::set_brush(b);
    }
}
