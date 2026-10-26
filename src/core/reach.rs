//! Reach: hands and feet sent where another clip has them, and the human pass
//! of the Body Collide node on the result.
//!
//! After a retarget the target's hands and feet go where its own bones put
//! them: longer legs leave the feet under the floor or above it, and they
//! slide. Reach takes the goal clip (the motion before the retarget), finds
//! where its hands and feet are against its hips, scales that by the ratio of
//! the hip heights, and moves the clip's hands and feet there with a two-bone
//! solve on each arm and leg. The hand and the foot keep the turn they had. The
//! elbow or knee first stays in the plane it bent in; then `human_ik::humanize`
//! puts every limb back within what a human body does, against the clip as it
//! came in, keeping the wrists and ankles where the solve put them.

use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

use super::anim::{AnimData, Track};
use super::human::{Human, Slot};
use super::human_ik::{humanize, Limits, Report};

/// What a Reach node sets.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReachSettings {
    pub feet: bool,
    pub hands: bool,
    /// How much of the way to the goal, 0 to 1.
    pub weight: f32,
    /// The human pass after the solve.
    pub limits: Limits,
}

impl Default for ReachSettings {
    fn default() -> Self {
        Self {
            feet: true,
            hands: false,
            weight: 1.0,
            limits: Limits::default(),
        }
    }
}

fn rot(m: &Mat4) -> Quat {
    m.to_scale_rotation_translation().1.normalize()
}
fn pos(m: &Mat4) -> Vec3 {
    m.w_axis.truncate()
}

/// How far each end missed its goal, before and after, metres.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReachReport {
    pub limbs: usize,
    pub miss_before: f32,
    pub miss_after: f32,
    pub human: Report,
}

fn reports() -> &'static std::sync::Mutex<std::collections::HashMap<usize, ReachReport>> {
    static R: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<usize, ReachReport>>> =
        std::sync::OnceLock::new();
    R.get_or_init(Default::default)
}

/// What the node that made `clip` did, for its panel.
pub fn report_for(clip: &Arc<AnimData>) -> Option<ReachReport> {
    reports().lock().ok()?.get(&(Arc::as_ptr(clip) as usize)).copied()
}

pub fn remember_report(clip: &Arc<AnimData>, report: ReachReport) {
    if let Ok(mut r) = reports().lock() {
        if r.len() > 32 {
            r.clear();
        }
        r.insert(Arc::as_ptr(clip) as usize, report);
    }
}

/// `f(0)`, `f(1)` … `f(count - 1)`, worked out in runs of frames on every core,
/// in order.
pub fn on_every_core<T: Send>(count: usize, f: &(impl Fn(usize) -> T + Sync)) -> Vec<T> {
    let threads = std::thread::available_parallelism()
        .map(|t| t.get())
        .unwrap_or(1)
        .clamp(1, 16);
    let run = count.div_ceil(threads).max(32);
    let mut out = Vec::with_capacity(count);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..count)
            .step_by(run)
            .map(|a| scope.spawn(move || (a..(a + run).min(count)).map(f).collect::<Vec<T>>()))
            .collect();
        for w in workers {
            out.extend(w.join().expect("worker"));
        }
    });
    out
}

/// `clip` with its hands and feet where `goal` has them, scaled to its size.
pub fn reach(clip: &AnimData, goal: &AnimData, settings: &ReachSettings) -> (AnimData, ReachReport) {
    let mut report = ReachReport::default();
    let (hc, hg) = (Human::of(clip), Human::of(goal));
    let (Some(hip_c), Some(hip_g)) = (hc.get(Slot::Hips), hg.get(Slot::Hips)) else {
        return (clip.clone(), report);
    };
    // Size: hip heights where the skins were bound.
    let (nc, ng) = (clip.neutral_world(), goal.neutral_world());
    let ratio = if pos(&ng[hip_g]).y.abs() > 1e-4 {
        pos(&nc[hip_c]).y / pos(&ng[hip_g]).y
    } else {
        1.0
    };

    // Limbs found in both, that this node moves.
    let pairs: Vec<(usize, usize, usize, usize)> = hc
        .limbs
        .iter()
        .zip(&hg.limbs)
        .filter_map(|(lc, lg)| {
            if (lc.arm && !settings.hands) || (!lc.arm && !settings.feet) {
                return None;
            }
            Some((lc.upper?, lc.mid?, lc.end?, lg.end?))
        })
        .collect();
    report.limbs = pairs.len();
    if pairs.is_empty() || settings.weight <= 0.0 {
        return (clip.clone(), report);
    }

    let frames = clip.frames.max(1);
    let n = clip.joints.len();
    let mut tracks: Vec<Track> = (*clip.tracks).clone();
    for &(u, m, e, _) in &pairs {
        for j in [u, m, e] {
            if tracks[j].len() < frames {
                tracks[j] = (0..frames).map(|f| clip.local(j, f)).collect();
            }
        }
    }
    // Which way each elbow or knee bends, in the frame of its upper bone,
    // from the frames where it is bent: a straight limb has no plane of its own.
    let bend_dirs: Vec<Vec3> = pairs
        .iter()
        .map(|&(u, m, e, _)| {
            let mut sum = Vec3::ZERO;
            let step = (frames / 300).max(1);
            for f in (0..frames).step_by(step) {
                let wc = clip.world_pose(f);
                let (s, el, wr) = (pos(&wc[u]), pos(&wc[m]), pos(&wc[e]));
                let d = (wr - s).normalize_or_zero();
                let off = (el - s) - d * (el - s).dot(d);
                if off.length() > 0.01 * s.distance(el) {
                    sum += rot(&wc[u]).inverse() * off.normalize();
                }
            }
            if sum.length() > 1e-3 {
                return sum.normalize();
            }
            // Never bent: knees forward, elbows back, from the hips' facing.
            let (rl, rr) = (nc[hc.limbs[2].upper.unwrap_or(u)], nc[hc.limbs[3].upper.unwrap_or(u)]);
            let across = (pos(&rl) - pos(&rr)).normalize_or_zero();
            let forward = across.cross(Vec3::Y).normalize_or_zero();
            let arm = hc.limbs.iter().any(|l| l.arm && l.end == Some(e));
            rot(&nc[u]).inverse() * if arm { -forward } else { forward }
        })
        .collect();
    let w = settings.weight.clamp(0.0, 1.0);
    // Frames do not depend on each other: each is solved on its own, on
    // every core, and the turns written back after.
    let solve = |f: usize| -> (f32, Vec<(usize, Quat)>) {
        let mut miss = 0.0f32;
        let mut turns = vec![];
        let wc = clip.world_pose(f);
        let wg = goal.world_pose(f.min(goal.frames.max(1) - 1));
        for (k, &(u, m, e, ge)) in pairs.iter().enumerate() {
            let (s, el, wr) = (pos(&wc[u]), pos(&wc[m]), pos(&wc[e]));
            let want = pos(&wc[hip_c]) + (pos(&wg[ge]) - pos(&wg[hip_g])) * ratio;
            let target = wr.lerp(want, w);
            miss = miss.max(wr.distance(want));
            let (l1, l2) = (s.distance(el), el.distance(wr));
            if l1 < 1e-5 || l2 < 1e-5 {
                continue;
            }
            // Never quite straight: a straight limb has no plane to bend in.
            let to = target - s;
            let dist = to.length().clamp((l1 - l2).abs() + 1e-4, (l1 + l2) * 0.9995);
            let dir = to.normalize_or_zero();
            if dir == Vec3::ZERO {
                continue;
            }
            // The elbow or knee stays on its side of the line to the end.
            let mut pole = (el - s) - dir * (el - s).dot(dir);
            if pole.length() < 0.01 * l1 {
                // Straight in the clip: bend the way this joint bends.
                pole = rot(&wc[u]) * bend_dirs[k];
                pole -= dir * pole.dot(dir);
            }
            let pole = pole.normalize_or_zero();
            if pole == Vec3::ZERO {
                continue;
            }
            let cos_a = ((l1 * l1 + dist * dist - l2 * l2) / (2.0 * l1 * dist)).clamp(-1.0, 1.0);
            let sin_a = (1.0 - cos_a * cos_a).sqrt();
            let elbow = s + (dir * cos_a + pole * sin_a) * l1;
            let end = s + dir * dist;
            // World turns: each bone swung onto its new direction.
            let q_u =
                (Quat::from_rotation_arc((el - s).normalize(), (elbow - s).normalize()) * rot(&wc[u])).normalize();
            let q_m =
                (Quat::from_rotation_arc((wr - el).normalize(), (end - elbow).normalize()) * rot(&wc[m])).normalize();
            let q_e = rot(&wc[e]);
            let parent_rot = |j: usize| {
                clip.joints[j]
                    .parent
                    .filter(|p| *p < n)
                    .map(|p| rot(&wc[p]))
                    .unwrap_or(Quat::IDENTITY)
            };
            turns.push((u, parent_rot(u).inverse() * q_u));
            turns.push((m, q_u.inverse() * q_m));
            turns.push((e, q_m.inverse() * q_e));
        }
        (miss, turns)
    };
    let mut miss_before = 0.0f32;
    for (f, (miss, turns)) in on_every_core(frames, &solve).into_iter().enumerate() {
        miss_before = miss_before.max(miss);
        for (j, q) in turns {
            tracks[j][f].rotation = q.normalize();
        }
    }
    let solved = AnimData {
        tracks: Arc::new(tracks),
        ..clip.clone()
    };
    let (out, human) = humanize(clip, &solved, &hc, &settings.limits, None);
    // How far the ends are from their goals now.
    let miss_after = on_every_core(frames, &|f: usize| {
        let wo = out.world_pose(f);
        let wg = goal.world_pose(f.min(goal.frames.max(1) - 1));
        pairs
            .iter()
            .map(|&(_, _, e, ge)| {
                let want = pos(&wo[hip_c]) + (pos(&wg[ge]) - pos(&wg[hip_g])) * ratio;
                pos(&wo[e]).distance(want)
            })
            .fold(0.0f32, f32::max)
    })
    .into_iter()
    .fold(0.0f32, f32::max);
    report.miss_before = miss_before;
    report.miss_after = miss_after;
    report.human = human;
    (out, report)
}

/// A clip that something moved, put back within human limits against the clip
/// before it was moved: the Body Collide node's human pass on its own.
pub fn human_limits(reference: &AnimData, moved: &AnimData, limits: &Limits) -> (AnimData, Report) {
    let human = Human::of(reference);
    if reference.joints.len() != moved.joints.len() || !human.usable() {
        return (
            moved.clone(),
            Report {
                frames: moved.frames.max(1),
                ..Default::default()
            },
        );
    }
    humanize(reference, moved, &human, limits, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::anim::{create_test_clip, FrameRate};

    fn clip() -> AnimData {
        create_test_clip(2.0, FrameRate::new(30, 1))
    }
    fn joint(c: &AnimData, name: &str) -> usize {
        c.joints.iter().position(|j| j.name.ends_with(name)).unwrap()
    }
    fn at(c: &AnimData, f: usize, name: &str) -> Vec3 {
        c.world_pose(f)[joint(c, name)].w_axis.truncate()
    }

    /// The test skeleton with shins a third longer, as another character.
    fn long_shins() -> AnimData {
        let c = clip();
        let joints = c
            .joints
            .iter()
            .map(|j| {
                let mut out = j.clone();
                out.name = j.name.replace("Take01:", "Tall_");
                if j.name.ends_with("LeftFoot") || j.name.ends_with("RightFoot") {
                    out.rest.translation *= 1.33;
                }
                out
            })
            .collect();
        AnimData {
            joints,
            frames: 1,
            tracks: Arc::new(vec![vec![]; c.joints.len()]),
            ..c
        }
    }

    #[test]
    fn feet_go_where_the_source_puts_them_scaled() {
        let source = clip();
        let target = long_shins();
        let (fk, _) = source.retargeted(&target);
        let (out, report) = reach(&fk, &source, &ReachSettings::default());
        assert_eq!(report.limbs, 2);
        // The longer shins put the feet a long way off before.
        assert!(report.miss_before > 0.05, "{report:?}");
        assert!(report.miss_after < 0.002, "{report:?}");
        let hips = |c: &AnimData| c.rest_world()[joint(c, "Hips")].w_axis.y;
        let ratio = hips(&target) / hips(&source);
        for f in [0, 9, 23, 44] {
            for side in ["Left", "Right"] {
                let want =
                    at(&out, f, "Hips") + (at(&source, f, &format!("{side}Foot")) - at(&source, f, "Hips")) * ratio;
                assert!(
                    at(&out, f, &format!("{side}Foot")).distance(want) < 0.002,
                    "frame {f} {side}"
                );
                // Knees still bend forward: the knee is in front of the line from hip to ankle.
                let (h, k, a) = (
                    at(&out, f, &format!("{side}UpLeg")),
                    at(&out, f, &format!("{side}Leg")),
                    at(&out, f, &format!("{side}Foot")),
                );
                let d = (a - h).normalize();
                let off = (k - h) - d * (k - h).dot(d);
                assert!(off.z > -1e-3, "frame {f} {side} knee points back: {off}");
            }
        }
        // The arms were left alone.
        assert!(at(&out, 20, "LeftHand").distance(at(&fk, 20, "LeftHand")) < 1e-5);
        // Bones keep their lengths.
        let len = |c: &AnimData, f| at(c, f, "LeftLeg").distance(at(c, f, "LeftFoot"));
        assert!((len(&out, 15) - len(&target, 0)).abs() < 1e-4);
    }

    #[test]
    fn human_limits_puts_back_an_elbow_bent_the_wrong_way() {
        let reference = clip();
        // Each forearm bent the other way round.
        let mut tracks = (*reference.tracks).clone();
        for name in ["LeftForeArm", "RightForeArm"] {
            let j = joint(&reference, name);
            for t in tracks[j].iter_mut() {
                t.rotation = t.rotation.inverse();
            }
        }
        let moved = AnimData {
            tracks: Arc::new(tracks),
            ..reference.clone()
        };
        let (_, report) = human_limits(&reference, &moved, &Limits::default());
        assert!(report.flips_before > 0, "{report:?}");
        assert_eq!(report.flips_after, 0, "{report:?}");
        // Off, it hands the clip on unchanged.
        let (same, _) = human_limits(
            &reference,
            &moved,
            &Limits {
                on: false,
                ..Default::default()
            },
        );
        assert!(at(&same, 10, "LeftHand").distance(at(&moved, 10, "LeftHand")) < 1e-6);
    }
}
