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
//!
//! Machines, animal legs and chains: their joints are swept by part (both
//! sides at once): a hinge to each of its limits, a spinning joint a full
//! turn each way, a sliding joint (a piston's rod, a suspension) to each end
//! of its travel. Wheels and treads move together, as one drive: a turn of
//! the largest wheel forward and back, each wheel turning by its own radius,
//! each tread's links running round its loop as far. A gear meshing with
//! another turns with it, the other way, by the ratio of their radii. Then
//! poses: driving while steering each way, every part to its maximum and to
//! its minimum, an animal's legs folded, a chain curled each way.

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
    /// The joints of one kind of the parts of one word (`aileron` on both
    /// sides), by the kind's discriminant and the word's hash.
    Kind(std::mem::Discriminant<Role>, u64),
    /// Every joint of a kind, whatever its part.
    AnyKind(std::mem::Discriminant<Role>),
    /// Every wheel and tread, by distance (`f` turns of the largest wheel).
    Drive,
    /// Every machine part, animal leg or chain joint that turns or slides
    /// within limits.
    Mech,
}

/// A part's word: its name without side and index (`aileron_L0` is
/// `aileron`).
fn word(part: &str) -> &str {
    part.rsplit_once('_').map(|x| x.0).unwrap_or(part)
}

fn hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// A machine part's, an animal leg's or a chain's joint: swept by part.
fn mech(r: Role) -> bool {
    matches!(
        r,
        Role::AnimalLeg(_)
            | Role::Chain(_)
            | Role::Body
            | Role::Hinge
            | Role::Rotor
            | Role::Blade(_)
            | Role::PistonBase
            | Role::PistonRod
            | Role::Suspension
            | Role::Steer
            | Role::Wheel
            | Role::Gear
            | Role::TreadLink(_)
    )
}

impl Sel {
    fn takes(self, j: &JointInfo) -> bool {
        match self {
            Sel::Is(r) => j.role == r,
            Sel::Spine => matches!(j.role, Role::Spine(_)),
            Sel::Neck => matches!(j.role, Role::Neck(_)),
            Sel::Fingers(k) => matches!(j.role, Role::Finger(f, n) if f != Finger::Thumb && n == k),
            Sel::Finger(f, k) => j.role == Role::Finger(f, k),
            Sel::Kind(d, w) => std::mem::discriminant(&j.role) == d && hash(word(&j.part)) == w,
            Sel::AnyKind(d) => std::mem::discriminant(&j.role) == d,
            Sel::Drive => matches!(j.role, Role::Wheel | Role::TreadLink(_)),
            Sel::Mech => {
                mech(j.role) && !matches!(j.role, Role::Wheel | Role::TreadLink(_) | Role::Body) && j.follow.is_none()
            }
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
/// A slide along X, as a fourth axis.
const SLIDE: usize = 3;

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

/// The machine parts', animal legs' and chains' groups, in the order their
/// parts come: one per kind of joint per word.
fn mech_groups(infos: &[JointInfo]) -> Vec<(String, Sel)> {
    let mut out: Vec<(String, Sel)> = vec![];
    let mut drive = false;
    for j in infos.iter().filter(|j| j.deform && mech(j.role)) {
        if matches!(j.role, Role::Wheel | Role::TreadLink(_)) {
            drive = true;
            continue;
        }
        if j.follow.is_some() {
            continue;
        }
        let kind = match j.role {
            Role::AnimalLeg(k) => ["hips", "stifles", "hocks", "fetlocks", "hooves"][k.min(4) as usize],
            Role::Blade(_) => "blades",
            Role::Steer => "steering",
            Role::Suspension => "suspension",
            Role::PistonRod => "stroke",
            Role::PistonBase => continue,
            _ => "",
        };
        let sel = Sel::Kind(std::mem::discriminant(&j.role), hash(word(&j.part)));
        // Animal legs' joints by their number, all legs together.
        let sel = match j.role {
            Role::AnimalLeg(k) => Sel::Is(Role::AnimalLeg(k)),
            _ => sel,
        };
        if !out.iter().any(|g| g.1 == sel) {
            let label = match j.role {
                Role::AnimalLeg(_) => format!("legs, {kind}"),
                _ if kind.is_empty() => word(&j.part).to_string(),
                _ => format!("{} {kind}", word(&j.part)),
            };
            out.push((label, sel));
        }
    }
    if drive {
        out.push(("drive forward and back".into(), Sel::Drive));
    }
    out
}

/// What a range of motion holds, for its panel: each part's name and its
/// frames (first, last).
#[derive(Clone, Debug, Default)]
pub struct RomReport {
    pub parts: Vec<(String, i64, i64)>,
    pub frames: usize,
}

/// Per joint: degrees about X, Y and Z, and a slide (or a tread link's run
/// round its loop).
type Key = Vec<[f32; 4]>;

/// A joint's limits, or a right angle either way without them (a wheel, a
/// rotor and a gear still spin).
fn limits_of(j: &JointInfo) -> Limits {
    j.limits.unwrap_or_else(|| {
        let mut l = Limits::new([-90.0, 90.0], [-90.0, 90.0], [-90.0, 90.0]);
        l.spin = matches!(j.role, Role::Wheel | Role::Rotor | Role::Gear);
        l
    })
}

fn key_of(rig: &[JointInfo], moves: &[Move], amount: f32) -> Key {
    let mut k: Key = vec![[0.0; 4]; rig.len()];
    // How far one turn of the largest wheel goes.
    let reach = rig
        .iter()
        .filter(|j| matches!(j.role, Role::Wheel | Role::TreadLink(_)))
        .map(|j| j.radius)
        .fold(0.0f32, f32::max)
        * std::f32::consts::TAU;
    for m in moves {
        for (i, j) in rig.iter().enumerate() {
            if !j.deform || j.role.is_twist() || !m.sel.takes(j) || m.side.is_some_and(|s| s != j.side) {
                continue;
            }
            // The right side's twist and swing are mirrored: the same
            // fraction turns it the other way, as the mirror image. Not a
            // wheel's steering: both wheels turn the same way.
            let f = if j.side == Side::Right && m.axis != Z && m.axis != SLIDE && j.role != Role::Steer {
                -m.f
            } else {
                m.f
            } * amount;
            let l = limits_of(j);
            if m.sel == Sel::Drive {
                // Forward: a wheel's top toward the front, a tread's top run
                // forward (its links run back along it as `s` grows).
                let d = reach * m.f * amount;
                match j.role {
                    Role::Wheel => {
                        let turn = (d / j.radius.max(1e-6)).to_degrees();
                        k[i][X] = if j.side == Side::Right { -turn } else { turn };
                    }
                    _ => k[i][SLIDE] = -d,
                }
            } else if m.axis == SLIDE {
                k[i][SLIDE] = l.slide_at(f);
            } else if m.axis == X && l.spin {
                k[i][X] = 360.0 * f;
            } else {
                k[i][m.axis] = l.at(m.axis, f);
            }
        }
    }
    k
}

/// The range of motion of a built skeleton. None without rig information.
pub fn range_of_motion(sk: &AnimData, s: &RomSettings) -> Option<(AnimData, RomReport)> {
    let rig = sk.rig.as_deref().filter(|r| r.joints.len() == sk.joints.len())?;
    let infos = &rig.joints;
    let n = sk.joints.len();
    let rest = vec![[0.0f32; 4]; n];
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
        for (name, sel) in mech_groups(infos) {
            let joints: Vec<&JointInfo> = infos.iter().filter(|j| j.deform && sel.takes(j)).collect();
            let mut axes: Vec<(usize, &str)> = vec![];
            if sel == Sel::Drive {
                axes.push((X, ""));
            } else {
                // A body rolls and pitches; a hinge turns; steering and a
                // blade's pitch need no word.
                let words = match joints.first().map(|j| j.role) {
                    Some(Role::Body) => ["roll", "yaw", "pitch"],
                    Some(Role::Hinge) => ["", "", ""],
                    Some(Role::Steer | Role::Blade(_)) => ["", "", ""],
                    _ => ["twist", "swing", "bend"],
                };
                for (axis, what) in [(X, words[0]), (Y, words[1]), (Z, words[2])] {
                    if joints.iter().any(|j| {
                        let l = limits_of(j);
                        l.min[axis] != 0.0 || l.max[axis] != 0.0
                    }) {
                        axes.push((axis, what));
                    }
                }
                if joints.iter().any(|j| limits_of(j).slide != [0.0; 2]) {
                    axes.push((SLIDE, ""));
                }
            }
            for (axis, what) in axes {
                let spin = axis == X && sel != Sel::Drive && joints.iter().all(|j| limits_of(j).spin);
                let label = match (what, spin) {
                    (_, true) => format!("{name}, a turn each way"),
                    ("", _) => name.clone(),
                    (w, false) => format!("{name} {w}"),
                };
                for f in [1.0, 0.0, -1.0, 0.0] {
                    keys.push((key_of(infos, &[mv(sel, axis, f)], s.amount), label.clone()));
                }
            }
        }
    }
    if s.poses {
        let steer = Sel::AnyKind(std::mem::discriminant(&Role::Steer));
        let fold = |f: f32| {
            (1..4u8)
                .map(|k| mv(Sel::Is(Role::AnimalLeg(k)), Z, -f))
                .collect::<Vec<_>>()
        };
        let curl = |f: f32| vec![mv(Sel::AnyKind(std::mem::discriminant(&Role::Chain(0))), Z, f)];
        let machine: Vec<(&str, Vec<Move>)> = vec![
            ("drive, turning left", vec![mv(Sel::Drive, X, 1.5), mv(steer, X, 1.0)]),
            ("drive, turning right", vec![mv(Sel::Drive, X, 1.5), mv(steer, X, -1.0)]),
            (
                "every part to its maximum",
                [X, Y, Z, SLIDE].map(|a| mv(Sel::Mech, a, 1.0)).to_vec(),
            ),
            (
                "every part to its minimum",
                [X, Y, Z, SLIDE].map(|a| mv(Sel::Mech, a, -1.0)).to_vec(),
            ),
            ("legs folded", fold(0.8)),
            ("curled one way", curl(1.0)),
            ("curled the other way", curl(-1.0)),
        ];
        for (name, moves) in poses().into_iter().chain(machine) {
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
        let mut at: Vec<[f32; 4]> = (0..n)
            .map(|j| [0, 1, 2, 3].map(|x| a[j][x] + (b[j][x] - a[j][x]) * t))
            .collect();
        // Gears meshing: each turns with the one it meshes with.
        for j in 0..n {
            if let Some((leader, ratio)) = infos[j].follow {
                at[j][X] = at[leader][X] * ratio;
            }
        }
        for j in 0..n {
            let rest = sk.joints[j].rest;
            let c = infos[j].canon;
            if let Some(path) = &infos[j].tread {
                // Round its loop: placed there, in what it hangs from.
                let (p, along) = path.at(path.start + at[j][SLIDE]);
                let fr = path.rear - path.front;
                let tt = ((p - path.front).dot(fr) / fr.length_squared().max(1e-12)).clamp(0.0, 1.0);
                let outward = (p - (path.front + fr * tt)).normalize_or(Vec3::Y);
                let y = (outward - along * outward.dot(along)).normalize_or(along.any_orthonormal_vector());
                let canon = glam::Quat::from_mat3(&glam::Mat3::from_cols(along, y, along.cross(y)));
                let w = glam::Mat4::from_rotation_translation((canon * c.inverse()).normalize(), p);
                let up = sk.joints[j].parent.map(|q| world[q]).unwrap_or(glam::Mat4::IDENTITY);
                let (_, r, tr) = (up.inverse() * w).to_scale_rotation_translation();
                let mut x = rest;
                x.rotation = r.normalize();
                x.translation = tr;
                tracks[j].push(x);
                continue;
            }
            let mut deg = [at[j][X], at[j][Y], at[j][Z]];
            if let Some((bone, frac)) = twist_of[j] {
                deg = [-at[bone][X] * (1.0 - frac), 0.0, 0.0];
            }
            let r = infos[j].order.quat(deg.map(f32::to_radians));
            let mut x = rest;
            x.rotation = (rest.rotation * c * r * c.inverse()).normalize();
            if at[j][SLIDE] != 0.0 {
                x.translation = rest.translation + rest.rotation * (c * Vec3::X) * at[j][SLIDE];
            }
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

    /// A machine: wheels roll forward on both sides and steer the same way,
    /// a meshing gear turns the other way by the ratio of the radii, a
    /// tread's links stay on their loop, a suspension slides within its
    /// travel.
    #[test]
    fn a_machine_moves_as_its_parts_do() {
        use crate::core::rig::guides::add_part;
        use crate::core::rig::{Component, MechParams, PartInfo};
        let p = |component: Component, side: Side, index: u32, attach: Option<&str>, steer: bool| PartInfo {
            component,
            side,
            index,
            attach: attach.map(Into::into),
            segments: 1,
            twists: 0,
            fingers: vec![],
            metacarpals: false,
            label: String::new(),
            mech: MechParams {
                steer,
                ..MechParams::of(component)
            },
        };
        let mut g = None;
        for part in [
            p(Component::Body, Side::Centre, 0, None, false),
            p(Component::Wheel, Side::Left, 0, None, true),
            p(Component::Wheel, Side::Right, 0, None, true),
            p(Component::Gear, Side::Centre, 0, None, false),
            p(Component::Gear, Side::Centre, 1, Some("gear_C0_centre"), false),
            p(Component::Tread, Side::Left, 0, None, false),
        ] {
            g = Some(add_part(g.as_ref(), &part, None, None, &[], false, &[]));
        }
        let sk = build(&g.unwrap(), &BuildSettings::default()).unwrap();
        let rig = sk.rig.as_deref().unwrap();
        let find = |r: Role, part: &str| rig.joints.iter().position(|j| j.role == r && j.part == part).unwrap();
        let (wl, wr) = (find(Role::Wheel, "wheel_L0"), find(Role::Wheel, "wheel_R0"));
        let (sl, sr) = (find(Role::Steer, "wheel_L0"), find(Role::Steer, "wheel_R0"));
        // A quarter turn of drive and full steering, as keys.
        let k = key_of(
            &rig.joints,
            &[
                mv(Sel::Drive, X, 0.25),
                mv(Sel::AnyKind(std::mem::discriminant(&Role::Steer)), X, 1.0),
            ],
            1.0,
        );
        // A quarter turn of the largest (the tread's wheels, 0.4), by each
        // wheel's own radius.
        let quarter = 90.0 * 0.4 / rig.joints[wl].radius;
        assert!(
            (k[wl][X] - quarter).abs() < 1e-3 && (k[wr][X] + quarter).abs() < 1e-3,
            "{:?} {:?}",
            k[wl],
            k[wr]
        );
        assert_eq!(k[sl][X], k[sr][X]);
        let (rom, report) = range_of_motion(&sk, &RomSettings::default()).unwrap();
        let rest = sk.world_pose(0);
        let at = |name: &str| {
            report
                .parts
                .iter()
                .find(|p| p.0 == name)
                .unwrap_or_else(|| panic!("no {name}: {:?}", report.parts))
        };
        // Driving forward: each wheel's top goes toward the front.
        let drive = at("drive forward and back");
        let f = (drive.1 - rom.start_frame) as usize + 2;
        let w = rom.world_pose(f);
        for wheel in [wl, wr] {
            let top = rest[wheel].w_axis.truncate() + glam::Vec3::Y * rig.joints[wheel].radius;
            let moved = (w[wheel] * rest[wheel].inverse()).transform_point3(top);
            assert!(moved.z > top.z + 1e-3, "{}: {} {}", sk.joints[wheel].name, top, moved);
        }
        // Gears: the second turns the other way, as much (same radius).
        let (ga, gb) = (find(Role::Gear, "gear_C0"), find(Role::Gear, "gear_C1"));
        let turn = |pose: &[glam::Mat4], j: usize| {
            let q = (pose[j] * rest[j].inverse()).to_scale_rotation_translation().1;
            let (axis, angle) = q.to_axis_angle();
            let a = angle * axis.dot(glam::Vec3::Y).signum();
            (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
        };
        let gear = at("gear, a turn each way");
        let w = rom.world_pose((gear.1 - rom.start_frame) as usize + 3);
        assert!(turn(&w, ga).abs() > 0.1);
        assert!(
            (turn(&w, ga) + turn(&w, gb)).abs() < 1e-3,
            "{} {}",
            turn(&w, ga),
            turn(&w, gb)
        );
        // Tread links on their loop at every frame, as the hull carries it.
        let path = rig.joints.iter().find_map(|j| j.tread).unwrap();
        let body = find(Role::Body, "body_C0");
        for f in (0..rom.frames).step_by(7) {
            let w = rom.world_pose(f);
            let hull = (w[body] * rest[body].inverse()).inverse();
            for (j, info) in rig.joints.iter().enumerate().filter(|(_, j)| j.tread.is_some()) {
                let q = hull.transform_point3(w[j].w_axis.truncate());
                let fr = path.rear - path.front;
                let t = ((q - path.front).dot(fr) / fr.length_squared()).clamp(0.0, 1.0);
                let off = q.distance(path.front + fr * t);
                assert!((off - path.radius).abs() < 1e-3, "frame {f} {}: {off}", info.part);
            }
        }
    }
}
