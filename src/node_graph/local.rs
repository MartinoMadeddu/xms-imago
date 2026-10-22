//! Animation in a time of its own: Local Animation and Merge Animation, and
//! the time range everything in the graph spans.
//!
//! A **Local Animation** sits after an operator and keys that operator's
//! parameters on frames of its own: local frame 0 falls on scene frame
//! `start`, and the animation lasts until `end`, held before and after. It
//! cooks the operator again with those parameters; the operator itself is
//! untouched, so one operator can feed several Local Animations.
//!
//! Every operator has a **time range**: a Local Animation's, a clip's, the
//! frames its own keys span, and the ranges of what comes into it, put
//! together. So a Merge of two animations shows the widest range.
//!
//! A **Merge Animation** merges its inputs as Merge does, each first moved
//! (or stretched) in time: to start, end or be centred where input A is,
//! or stretched over the longest range, or squeezed into the shortest.
//! Animated parameters upstream are cooked at the time the input is moved
//! to; clips are moved and retimed.

use std::sync::atomic::{AtomicU64, Ordering};

use super::{EvalCache, GraphNode, NodeGraphState};
use crate::core::keys;
use crate::types::{AnimAlign, EvalResult, IceGraphId, NodeId, NodeType};

/// The time an input is being cooked at, set while Merge Animation cooks
/// it at another time; unset, the playhead. Seconds, as bits.
pub struct EvalTime(AtomicU64);

const UNSET: u64 = u64::MAX;

impl Default for EvalTime { fn default() -> Self { EvalTime(AtomicU64::new(UNSET)) } }
impl Clone for EvalTime { fn clone(&self) -> Self { EvalTime(AtomicU64::new(self.0.load(Ordering::Relaxed))) } }

impl EvalTime {
    fn get(&self) -> Option<f64> { let b = self.0.load(Ordering::Relaxed); (b != UNSET).then(|| f64::from_bits(b)) }
    fn set(&self, t: Option<f64>) { self.0.store(t.map(f64::to_bits).unwrap_or(UNSET), Ordering::Relaxed); }
}

/// A frame range, first and last, in scene frames.
pub type Range = (f64, f64);

fn union(a: Option<Range>, b: Option<Range>) -> Option<Range> {
    match (a, b) { (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.max(b.1))), (a, None) => a, (None, b) => b }
}

/// How an input of Merge Animation is moved: it is cooked at frame
/// `k * F + c` when the scene is on frame `F`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shift { pub k: f64, pub c: f64 }

impl Shift {
    pub const NONE: Shift = Shift { k: 1.0, c: 0.0 };
    pub fn at(&self, f: f64) -> f64 { self.k * f + self.c }
    /// Where an input's range lands in the scene.
    pub fn place(&self, r: Range) -> Range { ((r.0 - self.c) / self.k, (r.1 - self.c) / self.k) }
}

/// How each input is moved, from the inputs' ranges (None: no time of its
/// own, left where it is).
pub fn shifts(align: AnimAlign, ranges: &[Option<Range>]) -> Vec<Shift> {
    let reference = ranges.iter().flatten().next().copied();
    let len = |r: &Range| r.1 - r.0;
    let longest = ranges.iter().flatten().copied().max_by(|a, b| len(a).total_cmp(&len(b)));
    let shortest = ranges.iter().flatten().copied().min_by(|a, b| len(a).total_cmp(&len(b)));
    ranges.iter().map(|r| {
        let (Some(r), Some(a)) = (r, reference) else { return Shift::NONE };
        let onto = |t: Range| {
            // Input frame r.0 on scene frame t.0, r.1 on t.1.
            if len(r) <= 1e-9 || len(&t) <= 1e-9 { return Shift { k: 1.0, c: r.0 - t.0 }; }
            let k = len(r) / len(&t);
            Shift { k, c: r.0 - t.0 * k }
        };
        match align {
            AnimAlign::FrameToFrame => Shift::NONE,
            AnimAlign::First => Shift { k: 1.0, c: r.0 - a.0 },
            AnimAlign::Last => Shift { k: 1.0, c: r.1 - a.1 },
            AnimAlign::Middle => Shift { k: 1.0, c: (r.0 + r.1) * 0.5 - (a.0 + a.1) * 0.5 },
            AnimAlign::Stretch => longest.map(onto).unwrap_or(Shift::NONE),
            AnimAlign::Shrink => shortest.map(onto).unwrap_or(Shift::NONE),
        }
    }).collect()
}

impl NodeGraphState {
    /// The time the graph is cooked at: the playhead, or the time Merge
    /// Animation has moved an input to.
    pub fn now(&self) -> f64 { self.eval_time.get().unwrap_or(self.time) }

    /// Run `f` with the graph cooked at `seconds`.
    pub fn with_time<R>(&self, seconds: f64, f: impl FnOnce() -> R) -> R {
        let was = self.eval_time.get();
        self.eval_time.set(Some(seconds));
        let r = f();
        self.eval_time.set(was);
        r
    }

    /// Where a node's frame 0 falls in the scene: a Local Animation's
    /// start, else 0.
    pub fn local_offset(&self, node: &GraphNode) -> f64 {
        match node.node_type { NodeType::LocalAnimation { start, .. } => start as f64, _ => 0.0 }
    }

    /// The frame a node's keys are read on: the scene's, or a Local
    /// Animation's own, held at its ends.
    pub fn node_frame(&self, node: &GraphNode) -> f64 {
        let f = self.frame();
        match node.node_type {
            NodeType::LocalAnimation { start, end, .. } => (f - start as f64).clamp(0.0, (end - start).max(0) as f64),
            _ => f,
        }
    }

    /// The scene frames an operator's output spans: Local Animations,
    /// clips and keys in it and above it, put together. None: it does not
    /// change in time.
    pub fn time_range(&self, id: NodeId) -> Option<Range> { self.range_depth(id, 0) }

    fn range_depth(&self, id: NodeId, depth: usize) -> Option<Range> {
        if depth > 128 { return None; }
        let node = self.nodes.iter().find(|n| n.id == id)?;
        let input = |k: usize| node.inputs.get(k).and_then(|s| s.connected_output).and_then(|(src, _)| self.range_depth(src, depth + 1));
        let mut r: Option<Range> = None;
        // Its own keys.
        if !node.channels.is_empty() {
            let off = self.local_offset(node);
            let frames: Vec<f64> = node.channels.iter().flat_map(|c| c.keys.iter().map(|k| k.frame + off)).collect();
            if let (Some(a), Some(b)) = (frames.iter().copied().reduce(f64::min), frames.iter().copied().reduce(f64::max)) { r = Some((a, b)); }
        }
        match &node.node_type {
            NodeType::LocalAnimation { start, end, .. } => {
                r = union(r, Some((*start as f64, (*end).max(*start) as f64)));
                r = union(r, input(0));
            }
            NodeType::MergeAnimation { align, .. } => {
                let ranges: Vec<Option<Range>> = (0..node.inputs.len()).map(input).collect();
                for (s, rr) in shifts(*align, &ranges).iter().zip(&ranges) {
                    r = union(r, rr.map(|x| s.place(x)));
                }
            }
            t => {
                // A clip's frames, at the scene's rate.
                if t.is_anim_generator() || matches!(t, NodeType::LoadFbx { .. } | NodeType::LoadFbxDir { .. }) {
                    if let Some(c) = self.eval_anim(id) {
                        r = union(r, Some((self.scene.frame(c.tc_seconds(c.start_frame)), self.scene.frame(c.tc_seconds(c.end_frame())))));
                    }
                }
                for k in 0..node.inputs.len() { r = union(r, input(k)); }
            }
        }
        r
    }

    /// What a Local Animation cooks: the operator wired into it, which of
    /// its outputs, and its parameters with the keyed ones set at the local
    /// frame.
    pub fn local_override(&self, la: &GraphNode) -> Option<(NodeId, usize, NodeType)> {
        let (src, out) = la.inputs.first()?.connected_output?;
        let x = self.nodes.iter().find(|n| n.id == src)?;
        if matches!(x.node_type, NodeType::LocalAnimation { .. } | NodeType::MergeAnimation { .. }) {
            // Over another time operator: nothing of its own to key.
            return None;
        }
        let mut v = serde_json::to_value(&*self.animated_type(x)).ok()?;
        let f = self.node_frame(la);
        for c in &la.channels {
            if let Some(p) = c.path.strip_prefix("inherited/") {
                if !c.keys.is_empty() { keys::set(&mut v, p, c.value_at(f)); }
            }
        }
        Some((src, out, serde_json::from_value(v).ok()?))
    }

    pub(crate) fn cook_local(
        &self,
        la: &GraphNode,
        cache: &mut EvalCache,
        eval_subnet: &impl Fn(IceGraphId, &crate::core::geo::Geo, &[crate::ice::Reference]) -> crate::core::geo::Geo,
    ) -> Option<EvalResult> {
        match self.local_override(la) {
            Some((src, out, t)) => self.cook_as(src, out, cache, eval_subnet, Some(&t)),
            // Nothing to key: what comes in passes through.
            None => la.inputs.first()?.connected_output.and_then(|(src, out)| self.eval_node_out(src, out, cache, eval_subnet)),
        }
    }

    /// The inputs of a Merge Animation with how each is moved.
    pub fn merge_shifts(&self, node: &GraphNode) -> Vec<(Option<(NodeId, usize)>, Option<Range>, Shift)> {
        let NodeType::MergeAnimation { align, .. } = node.node_type else { return vec![] };
        let wired: Vec<Option<(NodeId, usize)>> = node.inputs.iter().map(|s| s.connected_output).collect();
        let ranges: Vec<Option<Range>> = wired.iter().map(|w| w.and_then(|(src, _)| self.time_range(src))).collect();
        let s = shifts(align, &ranges);
        wired.into_iter().zip(ranges).zip(s).map(|((w, r), s)| (w, r, s)).collect()
    }

    pub(crate) fn cook_merge_animation(
        &self,
        node: &GraphNode,
        eval_subnet: &impl Fn(IceGraphId, &crate::core::geo::Geo, &[crate::ice::Reference]) -> crate::core::geo::Geo,
    ) -> Option<EvalResult> {
        let NodeType::MergeAnimation { match_rate, .. } = node.node_type else { return None };
        let f = self.frame();
        let mut results = vec![];
        for (w, range, s) in self.merge_shifts(node) {
            let Some((src, out)) = w else { continue };
            let seconds = self.scene.seconds(s.at(f));
            // Its own pass at its own time: what is above it may be keyed.
            let r = self.with_time(seconds, || self.eval_node_out(src, out, &mut Default::default(), eval_subnet));
            let Some(r) = r else { continue };
            let r = match r {
                EvalResult::Anim(c) => {
                    let mut c = (*c).clone();
                    if match_rate { c = c.resampled(crate::core::anim::FrameRate::new(self.scene.fps_num, self.scene.fps_den)); }
                    if (s.k - 1.0).abs() > 1e-6 { c = c.time_warped(s.k as f32, false); }
                    if let Some(range) = range {
                        // Its first frame where the shift puts it.
                        let first = s.place(range).0;
                        c.start_frame = c.frame_at(self.scene.seconds(first));
                    }
                    EvalResult::Anim(std::sync::Arc::new(c))
                }
                other => other,
            };
            results.push(r);
        }
        if results.is_empty() { return None; }
        // A clip on input A is put out on its own: several clips are not
        // held side by side.
        if let Some(EvalResult::Anim(_)) = results.first() { return results.into_iter().next(); }
        super::nodes::evaluate_node_type(&NodeType::merge(), &results, eval_subnet, 0)
    }

    /// The cook key of a Local Animation or Merge Animation: what it cooks,
    /// at the time it cooks it. None for any other operator. `Some(None)`:
    /// it cannot be keyed.
    pub(crate) fn time_key(&self, node: &GraphNode, output: usize, depth: usize) -> Option<Option<u64>> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        match &node.node_type {
            NodeType::LocalAnimation { .. } => {
                ("local", output).hash(&mut h);
                match self.local_override(node) {
                    Some((src, out, t)) => {
                        serde_json::to_string(&t).ok().hash(&mut h);
                        match self.cook_key_depth(src, out, depth + 1) { Some(k) => k.hash(&mut h), None => return Some(None) }
                    }
                    None => match node.inputs.first().and_then(|s| s.connected_output) {
                        Some((src, out)) => match self.cook_key_depth(src, out, depth + 1) { Some(k) => k.hash(&mut h), None => return Some(None) },
                        None => 0u8.hash(&mut h),
                    },
                }
            }
            NodeType::MergeAnimation { .. } => {
                ("merge animation", serde_json::to_string(&node.node_type).ok()).hash(&mut h);
                let f = self.frame();
                for (w, _, s) in self.merge_shifts(node) {
                    let Some((src, out)) = w else { 0u8.hash(&mut h); continue };
                    (s.k.to_bits(), s.c.to_bits()).hash(&mut h);
                    let k = self.with_time(self.scene.seconds(s.at(f)), || self.cook_key_depth(src, out, depth + 1));
                    match k { Some(k) => k.hash(&mut h), None => return Some(None) }
                }
            }
            _ => return None,
        }
        Some(Some(h.finish()))
    }

    /// Keep each Local Animation's copy of the parameters it inherits in
    /// step with the operator wired into it. Run every frame; changes
    /// nothing when they already match.
    pub fn sync_inherited(&mut self) {
        let wanted: Vec<(NodeId, serde_json::Value)> = self.nodes.iter().filter_map(|n| {
            let NodeType::LocalAnimation { inherited, .. } = &n.node_type else { return None };
            let src = n.inputs.first()?.connected_output.map(|(s, _)| s);
            let v = src.and_then(|s| self.nodes.iter().find(|x| x.id == s))
                .filter(|x| !matches!(x.node_type, NodeType::LocalAnimation { .. } | NodeType::MergeAnimation { .. }))
                .and_then(|x| match serde_json::to_value(&x.node_type).ok()? {
                    serde_json::Value::Object(o) if o.len() == 1 => o.into_iter().next().map(|(_, v)| v),
                    _ => None,
                })
                .unwrap_or(serde_json::Value::Null);
            (v != *inherited).then_some((n.id, v))
        }).collect();
        for (id, v) in wanted {
            if let Some(NodeType::LocalAnimation { inherited, .. }) = self.nodes.iter_mut().find(|n| n.id == id).map(|n| &mut n.node_type) {
                *inherited = v;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass(_: IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]) -> crate::core::geo::Geo { m.clone() }

    fn centre(g: &NodeGraphState, id: NodeId, f: f64) -> Vec<glam::Vec3> {
        let mut g = g.clone();
        g.time = g.scene.seconds(f);
        match g.eval_node(id, &mut Default::default(), &pass) {
            Some(EvalResult::Named(p)) => p.iter().map(|p| { let b = crate::viewport::bounds::world_bounds(p).unwrap(); (b.0 + b.1) * 0.5 }).collect(),
            other => panic!("{:?}", other.map(|_| ())),
        }
    }

    /// Shifts: first, last and middle move; stretch and shrink scale onto
    /// the longest and shortest.
    #[test]
    fn inputs_are_lined_up_in_time() {
        let r = [Some((10.0, 30.0)), Some((50.0, 60.0)), None];
        let p = |a: AnimAlign| shifts(a, &r).iter().zip(&r).map(|(s, r)| r.map(|r| s.place(r))).collect::<Vec<_>>();
        assert_eq!(p(AnimAlign::FrameToFrame), [Some((10.0, 30.0)), Some((50.0, 60.0)), None]);
        assert_eq!(p(AnimAlign::First), [Some((10.0, 30.0)), Some((10.0, 20.0)), None]);
        assert_eq!(p(AnimAlign::Last), [Some((10.0, 30.0)), Some((20.0, 30.0)), None]);
        assert_eq!(p(AnimAlign::Middle), [Some((10.0, 30.0)), Some((15.0, 25.0)), None]);
        assert_eq!(p(AnimAlign::Stretch), [Some((10.0, 30.0)), Some((10.0, 30.0)), None]);
        assert_eq!(p(AnimAlign::Shrink), [Some((50.0, 60.0)), Some((50.0, 60.0)), None]);
    }

    /// A Local Animation keys the Transform before it on frames of its own:
    /// local frame 0 on scene frame 50, held after its end. The Transform
    /// itself is not changed, and a second Local Animation on it moves
    /// separately. Its keyable numbers are the inherited ones.
    #[test]
    fn local_animation_keys_the_operator_before_it() {
        let mut g = NodeGraphState::default();
        let cube = g.add_node("Cube".into(), NodeType::CreateCube { size: 1.0, location: "/cube".into() }, egui::pos2(0.0, 0.0));
        let mv = g.add_node("Move".into(), NodeType::Transform { translation: glam::Vec3::ZERO, rotation: glam::Vec3::ZERO, scale: glam::Vec3::ONE }, egui::pos2(0.0, 100.0));
        let la = g.add_node("Slide".into(), NodeType::LocalAnimation { start: 50, end: 60, inherited: serde_json::Value::Null }, egui::pos2(0.0, 200.0));
        g.add_connection(cube, 0, mv, 0);
        g.add_connection(mv, 0, la, 0);
        g.sync_inherited();
        assert!(g.keyable_values(la).iter().all(|(p, _)| p.starts_with("inherited/")));
        assert!(g.keyable_values(la).iter().any(|(p, _)| p == "inherited/translation/0"));
        g.set_key_value(la, "inherited/translation/0", 0.0, 0.0);
        g.set_key_value(la, "inherited/translation/0", 10.0, 5.0);
        for k in &mut g.nodes.iter_mut().find(|n| n.id == la).unwrap().channels[0].keys { k.in_type = keys::Tangent::Linear; k.out_type = keys::Tangent::Linear; }
        assert!(centre(&g, la, 40.0)[0].x.abs() < 1e-4, "held before");
        assert!((centre(&g, la, 55.0)[0].x - 2.5).abs() < 1e-3);
        assert!((centre(&g, la, 90.0)[0].x - 5.0).abs() < 1e-3, "held after");
        assert!(centre(&g, mv, 55.0)[0].x.abs() < 1e-4, "the Transform itself is untouched");
        assert_eq!(g.time_range(la), Some((50.0, 60.0)));
        // Keys are read on local frames.
        let mut h = g.clone();
        h.time = h.scene.seconds(55.0);
        assert_eq!(h.node_frame(h.nodes.iter().find(|n| n.id == la).unwrap()), 5.0);
    }

    /// Two Local Animations of different ranges: Merge shows the widest
    /// range; Merge Animation set to First frame plays the second one from
    /// where the first starts.
    #[test]
    fn merge_animation_lines_up_its_inputs() {
        let mut g = NodeGraphState::default();
        let p = |x: f32, y: f32| egui::pos2(x, y);
        let a_cube = g.add_node("A".into(), NodeType::CreateCube { size: 1.0, location: "/a".into() }, p(0.0, 0.0));
        let a_mv = g.add_node("MoveA".into(), NodeType::Transform { translation: glam::Vec3::ZERO, rotation: glam::Vec3::ZERO, scale: glam::Vec3::ONE }, p(0.0, 100.0));
        let a_la = g.add_node("LocalA".into(), NodeType::LocalAnimation { start: 1, end: 21, inherited: serde_json::Value::Null }, p(0.0, 200.0));
        let b_cube = g.add_node("B".into(), NodeType::CreateCube { size: 1.0, location: "/b".into() }, p(200.0, 0.0));
        let b_mv = g.add_node("MoveB".into(), NodeType::Transform { translation: glam::Vec3::ZERO, rotation: glam::Vec3::ZERO, scale: glam::Vec3::ONE }, p(200.0, 100.0));
        let b_la = g.add_node("LocalB".into(), NodeType::LocalAnimation { start: 41, end: 51, inherited: serde_json::Value::Null }, p(200.0, 200.0));
        for (c, m, l) in [(a_cube, a_mv, a_la), (b_cube, b_mv, b_la)] { g.add_connection(c, 0, m, 0); g.add_connection(m, 0, l, 0); }
        let merge = g.add_node("Merge".into(), NodeType::merge(), p(100.0, 300.0));
        g.add_connection(a_la, 0, merge, 0);
        g.add_connection(b_la, 0, merge, 1);
        g.sync_inherited();
        for (l, len) in [(a_la, 20.0), (b_la, 10.0)] {
            g.set_key_value(l, "inherited/translation/1", 0.0, 0.0);
            g.set_key_value(l, "inherited/translation/1", len, 4.0);
            for k in &mut g.nodes.iter_mut().find(|n| n.id == l).unwrap().channels[0].keys { k.in_type = keys::Tangent::Linear; k.out_type = keys::Tangent::Linear; }
        }
        assert_eq!(g.time_range(merge), Some((1.0, 51.0)), "Merge shows the widest range");

        let ma = g.add_node("Line up".into(), NodeType::MergeAnimation { align: AnimAlign::First, match_rate: false, inputs: 2 }, p(100.0, 400.0));
        g.add_connection(a_la, 0, ma, 0);
        g.add_connection(b_la, 0, ma, 1);
        assert_eq!(g.time_range(ma), Some((1.0, 21.0)));
        // Scene frame 6: A is a quarter up, B halfway (it started on 1 too).
        let c = centre(&g, ma, 6.0);
        let (a, b) = (c.iter().find(|v| v.x.abs() < 1e-3 && v.y >= 0.0).copied().unwrap(), c[1]);
        assert!((a.y - 1.0).abs() < 1e-3, "{c:?}");
        assert!((b.y - 2.0).abs() < 1e-3, "{c:?}");
        // Stretch: B over A's 20 frames: at frame 11 both are halfway.
        if let NodeType::MergeAnimation { align, .. } = &mut g.nodes.iter_mut().find(|n| n.id == ma).unwrap().node_type { *align = AnimAlign::Stretch; }
        let c = centre(&g, ma, 11.0);
        assert!((c[0].y - 2.0).abs() < 1e-3 && (c[1].y - 2.0).abs() < 1e-3, "{c:?}");
        // The playhead moved back: the cooked results follow.
        let c = centre(&g, ma, 1.0);
        assert!(c[0].y.abs() < 1e-3 && c[1].y.abs() < 1e-3, "{c:?}");
    }

    /// A clip on input A, stretched over a longer Local Animation on input
    /// B, plays slower: its frames are spread over B's range.
    #[test]
    fn merge_animation_retimes_a_clip() {
        let mut g = NodeGraphState::default();
        let p = |x: f32, y: f32| egui::pos2(x, y);
        let clip = g.add_node("Walk".into(), NodeType::TestClip { seconds: 1.0, fps_num: 24, fps_den: 1 }, p(0.0, 0.0));
        let cube = g.add_node("Cube".into(), NodeType::CreateCube { size: 1.0, location: "/c".into() }, p(200.0, 0.0));
        let mv = g.add_node("Move".into(), NodeType::Transform { translation: glam::Vec3::ZERO, rotation: glam::Vec3::ZERO, scale: glam::Vec3::ONE }, p(200.0, 100.0));
        let la = g.add_node("Long".into(), NodeType::LocalAnimation { start: 1, end: 97, inherited: serde_json::Value::Null }, p(200.0, 200.0));
        g.add_connection(cube, 0, mv, 0);
        g.add_connection(mv, 0, la, 0);
        let ma = g.add_node("Fit".into(), NodeType::MergeAnimation { align: AnimAlign::Stretch, match_rate: false, inputs: 2 }, p(100.0, 300.0));
        g.add_connection(clip, 0, ma, 0);
        g.add_connection(la, 0, ma, 1);
        let before = g.eval_anim(clip).unwrap();
        let after = g.eval_anim(ma).unwrap();
        assert!(after.frames as f64 > before.frames as f64 * 3.0, "{} -> {}", before.frames, after.frames);
    }
}
