//! Range of Motion: a clip that moves every joint of a built skeleton through
//! what its limits allow. First each joint alone (both sides at once), about
//! each of its axes: to the most it bends one way, back, the other way, back.
//! Then poses a body takes: arms up, forward, crossed; a squat, a lunge,
//! sitting; bending, twisting and leaning the torso; looking around; a fist
//! and an open hand. Every angle is a fraction of the joint's limits, so the
//! poses stay within them and follow them when they change. Without limits
//! (turned off on Build Skeleton) each axis goes to 90 degrees either way.
//!
//! Twist joints: when a bone with twist joints turns about its own X, each
//! twist joint turns back by the part of it that does not reach that far
//! along the bone, so the skin twists more toward the bone's end.

use std::sync::Arc;

use glam::Vec3;

use super::{Finger, JointInfo, Limits, Role, Side};
use crate::core::anim::{AnimData, FrameRate, Track};

/// How a range of motion is made.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RomSettings {
    /// Each joint about each axis.
    pub sweeps: bool,
    /// Whole-body poses.
    pub poses: bool,
    /// Frames from one key pose to the next.
    pub frames: u32,
    /// How far into the limits, 0 to 1.
    pub amount: f32,
}

impl Default for RomSettings {
    fn default() -> Self {
        RomSettings {
            sweeps: true,
            poses: true,
            frames: 8,
            amount: 1.0,
        }
    }
}

/// Which joints a move turns.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Sel {
    Is(Role),
    Spine,
    Neck,
    /// A finger's joints (not the thumb's) of this number.
    Fingers(u8),
    Finger(Finger, u8),
}

impl Sel {
    fn takes(self, j: &JointInfo) -> bool {
        match self {
            Sel::Is(r) => j.role == r,
            Sel::Spine => matches!(j.role, Role::Spine(_)),
            Sel::Neck => matches!(j.role, Role::Neck(_)),
            Sel::Fingers(k) => matches!(j.role, Role::Finger(f, n) if f != Finger::Thumb && n == k),
            Sel::Finger(f, k) => j.role == Role::Finger(f, k),
        }
    }
}

/// One turn: these joints (on `side`, or both), about `axis`, a fraction
/// of their limits (-1 the minimum, 1 the maximum).
#[derive(Clone, Copy, Debug)]
struct Move {
    sel: Sel,
    side: Option<Side>,
    axis: usize,
    f: f32,
}

fn mv(sel: Sel, axis: usize, f: f32) -> Move {
    Move {
        sel,
        side: None,
        axis,
        f,
    }
}

fn on(side: Side, sel: Sel, axis: usize, f: f32) -> Move {
    Move {
        sel,
        side: Some(side),
        axis,
        f,
    }
}

const X: usize = 0;
const Y: usize = 1;
const Z: usize = 2;

/// The joints swept alone, in order.
fn sweep_groups() -> Vec<(&'static str, Sel)> {
    let mut g = vec![
        ("pelvis", Sel::Is(Role::Pelvis)),
        ("spine", Sel::Spine),
        ("neck", Sel::Neck),
        ("head", Sel::Is(Role::Head)),
        ("clavicles", Sel::Is(Role::Clavicle)),
        ("upper arms", Sel::Is(Role::UpperArm)),
        ("forearms", Sel::Is(Role::LowerArm)),
        ("hands", Sel::Is(Role::Hand)),
    ];
    for (k, name) in [(1, "thumbs at the base"), (2, "thumbs, middle"), (3, "thumbs, tip")] {
        g.push((name, Sel::Finger(Finger::Thumb, k)));
    }
    for (k, name) in [
        (0, "metacarpals"),
        (1, "knuckles"),
        (2, "fingers, middle"),
        (3, "fingertips"),
    ] {
        g.push((name, Sel::Fingers(k)));
    }
    g.extend([
        ("thighs", Sel::Is(Role::Thigh)),
        ("knees", Sel::Is(Role::Calf)),
        ("ankles", Sel::Is(Role::Foot)),
        ("toes", Sel::Is(Role::Ball)),
    ]);
    g
}

/// Whole-body poses, from rest and back to it.
fn poses() -> Vec<(&'static str, Vec<Move>)> {
    use Side::{Left as L, Right as R};
    let fist = |f: f32| {
        vec![
            mv(Sel::Fingers(1), Z, -f),
            mv(Sel::Fingers(2), Z, -f),
            mv(Sel::Fingers(3), Z, -f),
            mv(Sel::Finger(Finger::Thumb, 1), Z, -f * 0.6),
            mv(Sel::Finger(Finger::Thumb, 2), Z, -f * 0.8),
            mv(Sel::Finger(Finger::Thumb, 3), Z, -f * 0.8),
        ]
    };
    vec![
        (
            "arms up",
            vec![
                mv(Sel::Is(Role::UpperArm), Y, -0.9),
                mv(Sel::Is(Role::Clavicle), Y, -0.8),
            ],
        ),
        ("arms out", vec![mv(Sel::Is(Role::UpperArm), Y, -0.45)]),
        ("arms forward", vec![mv(Sel::Is(Role::UpperArm), Z, -0.55)]),
        (
            "arms forward, up",
            vec![
                mv(Sel::Is(Role::UpperArm), Z, -0.95),
                mv(Sel::Is(Role::Clavicle), Y, -0.6),
            ],
        ),
        ("arms back", vec![mv(Sel::Is(Role::UpperArm), Z, 0.8)]),
        (
            "hug",
            vec![
                mv(Sel::Is(Role::UpperArm), Z, -0.5),
                mv(Sel::Is(Role::UpperArm), Y, 0.8),
                mv(Sel::Is(Role::LowerArm), Z, -0.6),
                mv(Sel::Is(Role::Clavicle), Z, 0.8),
            ],
        ),
        (
            "hands to shoulders",
            vec![
                mv(Sel::Is(Role::UpperArm), Z, -0.3),
                mv(Sel::Is(Role::LowerArm), Z, -0.95),
            ],
        ),
        (
            "arms turned in",
            vec![
                mv(Sel::Is(Role::UpperArm), Y, -0.45),
                mv(Sel::Is(Role::UpperArm), X, -0.8),
                mv(Sel::Is(Role::LowerArm), X, -0.8),
            ],
        ),
        (
            "arms turned out",
            vec![
                mv(Sel::Is(Role::UpperArm), Y, -0.45),
                mv(Sel::Is(Role::UpperArm), X, 0.8),
                mv(Sel::Is(Role::LowerArm), X, 0.8),
            ],
        ),
        (
            "wrists",
            vec![mv(Sel::Is(Role::Hand), Z, -0.8), mv(Sel::Is(Role::Hand), Y, 0.6)],
        ),
        ("fist", fist(0.9)),
        (
            "hand open",
            vec![
                mv(Sel::Fingers(1), Z, 0.8),
                mv(Sel::Finger(Finger::Index, 1), Y, -0.8),
                mv(Sel::Finger(Finger::Ring, 1), Y, 0.5),
                mv(Sel::Finger(Finger::Pinky, 1), Y, 0.9),
                mv(Sel::Finger(Finger::Thumb, 1), Y, 0.8),
            ],
        ),
        (
            "bend forward",
            vec![
                mv(Sel::Is(Role::Pelvis), Z, 0.6),
                mv(Sel::Spine, Z, 1.0),
                mv(Sel::Neck, Z, 0.6),
            ],
        ),
        (
            "bend back",
            vec![
                mv(Sel::Spine, Z, -1.0),
                mv(Sel::Neck, Z, -0.6),
                mv(Sel::Is(Role::Head), Z, -0.5),
            ],
        ),
        ("lean left", vec![mv(Sel::Spine, Y, -1.0), mv(Sel::Neck, Y, -0.5)]),
        ("lean right", vec![mv(Sel::Spine, Y, 1.0), mv(Sel::Neck, Y, 0.5)]),
        ("twist left", vec![mv(Sel::Spine, X, 1.0), mv(Sel::Neck, X, 0.5)]),
        ("twist right", vec![mv(Sel::Spine, X, -1.0), mv(Sel::Neck, X, -0.5)]),
        (
            "look left",
            vec![mv(Sel::Neck, X, 0.9), mv(Sel::Is(Role::Head), X, 0.9)],
        ),
        (
            "look right",
            vec![mv(Sel::Neck, X, -0.9), mv(Sel::Is(Role::Head), X, -0.9)],
        ),
        (
            "look up",
            vec![mv(Sel::Neck, Z, -0.8), mv(Sel::Is(Role::Head), Z, -0.8)],
        ),
        (
            "look down",
            vec![mv(Sel::Neck, Z, 0.8), mv(Sel::Is(Role::Head), Z, 0.8)],
        ),
        (
            "squat",
            vec![
                mv(Sel::Is(Role::Thigh), Z, 0.85),
                mv(Sel::Is(Role::Calf), Z, -0.85),
                mv(Sel::Is(Role::Foot), Z, 0.9),
                mv(Sel::Spine, Z, 0.6),
            ],
        ),
        (
            "sit",
            vec![mv(Sel::Is(Role::Thigh), Z, 0.75), mv(Sel::Is(Role::Calf), Z, -0.6)],
        ),
        (
            "lunge left",
            vec![
                on(L, Sel::Is(Role::Thigh), Z, 0.6),
                on(L, Sel::Is(Role::Calf), Z, -0.6),
                on(L, Sel::Is(Role::Foot), Z, 0.5),
                on(R, Sel::Is(Role::Thigh), Z, -0.9),
                on(R, Sel::Is(Role::Calf), Z, -0.4),
                on(R, Sel::Is(Role::Ball), Z, 0.8),
            ],
        ),
        (
            "lunge right",
            vec![
                on(R, Sel::Is(Role::Thigh), Z, 0.6),
                on(R, Sel::Is(Role::Calf), Z, -0.6),
                on(R, Sel::Is(Role::Foot), Z, 0.5),
                on(L, Sel::Is(Role::Thigh), Z, -0.9),
                on(L, Sel::Is(Role::Calf), Z, -0.4),
                on(L, Sel::Is(Role::Ball), Z, 0.8),
            ],
        ),
        (
            "knee up left",
            vec![on(L, Sel::Is(Role::Thigh), Z, 1.0), on(L, Sel::Is(Role::Calf), Z, -0.8)],
        ),
        (
            "knee up right",
            vec![on(R, Sel::Is(Role::Thigh), Z, 1.0), on(R, Sel::Is(Role::Calf), Z, -0.8)],
        ),
        ("legs apart", vec![mv(Sel::Is(Role::Thigh), Y, 1.0)]),
        (
            "legs crossed",
            vec![mv(Sel::Is(Role::Thigh), Y, -0.8), mv(Sel::Is(Role::Thigh), Z, 0.3)],
        ),
        (
            "tiptoe",
            vec![mv(Sel::Is(Role::Foot), Z, -0.9), mv(Sel::Is(Role::Ball), Z, 0.9)],
        ),
    ]
}

/// What a range of motion holds, for its panel: each part's name and its
/// frames (first, last).
#[derive(Clone, Debug, Default)]
pub struct RomReport {
    pub parts: Vec<(String, i64, i64)>,
    pub frames: usize,
}

type Key = Vec<[f32; 3]>;

/// A joint's limits, or a right angle either way without them.
fn limits_of(j: &JointInfo) -> Limits {
    j.limits.unwrap_or(Limits {
        min: [-90.0; 3],
        max: [90.0; 3],
    })
}

fn key_of(rig: &[JointInfo], moves: &[Move], amount: f32) -> Key {
    let mut k: Key = vec![[0.0; 3]; rig.len()];
    for m in moves {
        for (i, j) in rig.iter().enumerate() {
            if !j.deform || j.role.is_twist() || !m.sel.takes(j) || m.side.is_some_and(|s| s != j.side) {
                continue;
            }
            // The right side's twist and swing are mirrored: the same
            // fraction turns it the other way, as the mirror image.
            let f = if j.side == Side::Right && m.axis != Z {
                -m.f
            } else {
                m.f
            };
            k[i][m.axis] = limits_of(j).at(m.axis, f * amount);
        }
    }
    k
}

/// The range of motion of a built skeleton. None without rig information.
pub fn range_of_motion(sk: &AnimData, s: &RomSettings) -> Option<(AnimData, RomReport)> {
    let rig = sk.rig.as_deref().filter(|r| r.joints.len() == sk.joints.len())?;
    let infos = &rig.joints;
    let n = sk.joints.len();
    let rest = vec![[0.0f32; 3]; n];
    let step = s.frames.max(1) as usize;
    // Key poses, each with the part it belongs to.
    let mut keys: Vec<(Key, String)> = vec![(rest.clone(), String::new())];
    if s.sweeps {
        for (name, sel) in sweep_groups() {
            for axis in 0..3 {
                let any = infos.iter().any(|j| {
                    j.deform && !j.role.is_twist() && sel.takes(j) && {
                        let l = limits_of(j);
                        l.min[axis] != 0.0 || l.max[axis] != 0.0
                    }
                });
                if !any {
                    continue;
                }
                let label = format!("{name} {}", ["twist", "swing", "bend"][axis]);
                for f in [1.0, 0.0, -1.0, 0.0] {
                    keys.push((key_of(infos, &[mv(sel, axis, f)], s.amount), label.clone()));
                }
            }
        }
    }
    if s.poses {
        for (name, moves) in poses() {
            let k = key_of(infos, &moves, s.amount);
            if k == rest {
                continue;
            }
            keys.push((k, name.to_string()));
            keys.push((rest.clone(), name.to_string()));
        }
    }
    // Where each twist joint is along its bone, 0 at the bone's start.
    let world = sk.world_pose(0);
    let pos: Vec<Vec3> = world.iter().map(|m| m.w_axis.truncate()).collect();
    let twist_of: Vec<Option<(usize, f32)>> = (0..n)
        .map(|t| {
            let ti = &infos[t];
            if !ti.role.is_twist() {
                return None;
            }
            let b = (0..n).find(|b| {
                let bi = &infos[*b];
                bi.role == ti.role.bone() && bi.side == ti.side && bi.part == ti.part
            })?;
            let d = infos[b].tip - pos[b];
            Some((
                b,
                ((pos[t] - pos[b]).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0),
            ))
        })
        .collect();

    let frames = (keys.len() - 1) * step + 1;
    let mut tracks: Vec<Track> = vec![Vec::with_capacity(frames); n];
    let mut report = RomReport {
        frames,
        ..Default::default()
    };
    let first = sk.start_frame;
    for (k, w) in keys.windows(2).enumerate() {
        let label = &w[1].1;
        let at = first + (k * step) as i64;
        match report.parts.last_mut() {
            Some(p) if &p.0 == label => p.2 = at + step as i64,
            _ => report.parts.push((label.clone(), at, at + step as i64)),
        }
    }
    for f in 0..frames {
        let (k, t) = if f == frames - 1 {
            (keys.len() - 2, 1.0)
        } else {
            (f / step, (f % step) as f32 / step as f32)
        };
        // Ease in and out.
        let t = t * t * (3.0 - 2.0 * t);
        let (a, b) = (&keys[k].0, &keys[k + 1].0);
        let angle = |j: usize| -> [f32; 3] { [0, 1, 2].map(|x| a[j][x] + (b[j][x] - a[j][x]) * t) };
        for j in 0..n {
            let rest = sk.joints[j].rest;
            let mut deg = angle(j);
            if let Some((bone, frac)) = twist_of[j] {
                deg = [-angle(bone)[0] * (1.0 - frac), 0.0, 0.0];
            }
            let r = infos[j].order.quat(deg.map(f32::to_radians));
            let c = infos[j].canon;
            let mut x = rest;
            x.rotation = (rest.rotation * c * r * c.inverse()).normalize();
            tracks[j].push(x);
        }
    }
    let mut out = sk.clone();
    out.name = "RangeOfMotion".into();
    out.tracks = Arc::new(tracks);
    out.frames = frames;
    out.rate = FrameRate::from_fps(24.0);
    out.skin = None;
    Some((out, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rig::build::{build, BuildSettings};
    use crate::core::rig::presets::Preset;

    /// A range of motion of a whole human: every bend within the limits, the
    /// knees and elbows never past straight, both sides mirrored, and the
    /// parts listed in order.
    #[test]
    fn a_human_moves_within_its_limits() {
        let g = crate::core::rig::build::tests::human_guides();
        for preset in Preset::ALL {
            let sk = build(&g, &BuildSettings::of(preset)).unwrap();
            let (rom, report) = range_of_motion(&sk, &RomSettings::default()).unwrap();
            assert!(rom.frames > 200, "{}", rom.frames);
            assert_eq!(report.frames, rom.frames);
            assert_eq!(report.parts.first().unwrap().0, "pelvis twist");
            assert!(report.parts.iter().any(|p| p.0 == "squat"));
            let rig = sk.rig.as_deref().unwrap();
            let ix = |r: Role, s: Side| rig.joint(r, s).unwrap();
            let (knee, hip) = (ix(Role::Calf, Side::Left), ix(Role::Thigh, Side::Left));
            let rest_w = sk.world_pose(0);
            let rest_len = rest_w[knee].w_axis.distance(rest_w[hip].w_axis);
            let mut bent = false;
            for f in 0..rom.frames {
                let w = rom.world_pose(f);
                // The knee bends only backwards: the shin never toward the
                // front of the thigh (its canonical Y, the knee's pole).
                let c = glam::Mat3::from_quat(w[hip].to_scale_rotation_translation().1 * rig.joints[hip].canon);
                let ankle = w[ix(Role::Foot, Side::Left)].w_axis.truncate();
                let shin = (ankle - w[knee].w_axis.truncate()).normalize();
                assert!(
                    shin.dot(c.y_axis) <= 0.02,
                    "{preset:?} frame {f}: {}",
                    shin.dot(c.y_axis)
                );
                if shin.dot(c.x_axis) < 0.2 {
                    bent = true;
                }
                // Bones keep their length.
                assert!((w[knee].w_axis.distance(w[hip].w_axis) - rest_len).abs() < 1e-3);
                // Left and right mirrored in the sweeps of the arms.
                let arms = report.parts.iter().filter(|p| {
                    p.0.starts_with("upper arms")
                        || p.0.starts_with("forearms")
                        || p.0.starts_with("hands")
                        || p.0.starts_with("knuckles")
                });
                if arms
                    .clone()
                    .any(|p| (p.1..=p.2).contains(&(f as i64 + rom.start_frame)))
                {
                    let (l, r) = (
                        w[ix(Role::Hand, Side::Left)].w_axis,
                        w[ix(Role::Hand, Side::Right)].w_axis,
                    );
                    assert!(
                        (l.x + r.x).abs() < 2e-3 && (l.y - r.y).abs() < 2e-3 && (l.z - r.z).abs() < 2e-3,
                        "{preset:?} frame {f}: {l} {r} {:?}",
                        report
                            .parts
                            .iter()
                            .filter(|p| (p.1..=p.2).contains(&(f as i64 + rom.start_frame)))
                            .collect::<Vec<_>>()
                    );
                }
            }
            assert!(bent, "{preset:?}: the knee never bent");
        }
    }
}
