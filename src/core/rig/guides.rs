//! Guide locators: each component's, where they go on a reference body, where
//! autofit puts them on a mesh, and the guide clip they make: one joint per
//! locator, chained as the component reads (the shoulder under the clavicle,
//! the elbow under the shoulder), its first under the locator it attaches to.
//! Locators that are not on the joint chain (an elbow's pole, the heel) are
//! not bones.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat3, Vec3};

use super::fit::{self, Body};
use super::surface::Surface;
use super::{locator_name, Component, Finger, GuideTag, PartInfo, RigInfo, Side};
use crate::core::anim::{AnimData, FrameRate, Joint, SkinMesh};
use crate::core::xform::Transform;

/// One locator of a component: its name, the locator of the same component
/// it hangs from (None: the one the component attaches to), and whether it is
/// on the joint chain.
#[derive(Clone, Debug, PartialEq)]
pub struct Locator {
    pub name: String,
    pub parent: Option<String>,
    pub bone: bool,
}

fn loc(name: &str, parent: Option<&str>, bone: bool) -> Locator {
    Locator {
        name: name.into(),
        parent: parent.map(Into::into),
        bone,
    }
}

/// A component's locators, in order: parents before children.
pub fn locators(part: &PartInfo) -> Vec<Locator> {
    match part.component {
        Component::Spine => vec![
            loc("hips", None, true),
            loc("mid", Some("hips"), true),
            loc("chest", Some("mid"), true),
        ],
        Component::Neck => vec![
            loc("neck", None, true),
            loc("head", Some("neck"), true),
            loc("head_end", Some("head"), true),
        ],
        Component::Arm => vec![
            loc("clavicle", None, true),
            loc("shoulder", Some("clavicle"), true),
            loc("elbow", Some("shoulder"), true),
            loc("wrist", Some("elbow"), true),
            loc("hand_end", Some("wrist"), true),
            loc("pole", Some("elbow"), false),
        ],
        Component::Hand => {
            let mut out = vec![];
            for f in &part.fingers {
                let w = f.word();
                let names: Vec<String> = finger_locators(*f).iter().map(|n| format!("{w}_{n}")).collect();
                for (k, n) in names.iter().enumerate() {
                    out.push(loc(n, if k == 0 { None } else { Some(names[k - 1].as_str()) }, true));
                }
            }
            out
        }
        Component::Leg => vec![
            loc("hip", None, true),
            loc("knee", Some("hip"), true),
            loc("ankle", Some("knee"), true),
            loc("ball", Some("ankle"), true),
            loc("toe", Some("ball"), true),
            loc("heel", Some("ankle"), false),
            loc("pole", Some("knee"), false),
        ],
    }
}

/// A finger's locators from the palm out.
pub fn finger_locators(f: Finger) -> &'static [&'static str] {
    if f == Finger::Thumb {
        &["1", "2", "3", "end"]
    } else {
        &["meta", "1", "2", "3", "end"]
    }
}

/// The locator a component attaches to unless told otherwise.
pub fn default_attach(component: Component, side: Side, index: u32) -> Option<String> {
    let s = side.letter();
    match component {
        Component::Spine => None,
        Component::Neck | Component::Arm => Some("spine_C0_chest".into()),
        Component::Hand => Some(format!("arm_{s}{index}_wrist")),
        Component::Leg => Some("spine_C0_hips".into()),
    }
}

// The reference body: a 1.7 m adult in an A pose, the left side, metres (the
// example character's joints, rounded).
const SPINE: [(&str, [f32; 3]); 3] = [
    ("hips", [0.0, 0.982, 0.025]),
    ("mid", [0.0, 1.182, 0.051]),
    ("chest", [0.0, 1.405, 0.016]),
];
const NECK: [(&str, [f32; 3]); 3] = [
    ("neck", [0.0, 1.568, -0.009]),
    ("head", [0.0, 1.677, 0.014]),
    ("head_end", [0.0, 1.80, 0.02]),
];
const ARM: [(&str, [f32; 3]); 5] = [
    ("clavicle", [0.0125, 1.5065, -0.0136]),
    ("shoulder", [0.180, 1.483, -0.017]),
    ("elbow", [0.362, 1.245, -0.009]),
    ("wrist", [0.503, 1.077, 0.162]),
    ("hand_end", [0.558, 0.935, 0.280]),
];
const LEG: [(&str, [f32; 3]); 6] = [
    ("hip", [0.089, 0.981, 0.029]),
    ("knee", [0.111, 0.514, 0.019]),
    ("ankle", [0.130, 0.092, 0.003]),
    ("ball", [0.156, 0.009, 0.151]),
    ("toe", [0.147, 0.009, 0.229]),
    ("heel", [0.133, 0.0, -0.064]),
];
/// The reference hand's frame: X out along the hand, Y the back of the hand,
/// Z = X×Y; its length from the wrist to the fingertips.
const HAND_FRAME: [[f32; 3]; 3] = [[0.315, -0.729, 0.608], [0.870, 0.478, 0.122], [-0.379, 0.490, 0.785]];
const HAND_LENGTH: f32 = 0.192;
/// Its fingers' joints in that frame, over its length, relaxed.
const FINGERS: [(&str, [f32; 3]); 19] = [
    ("thumb_1", [0.088, -0.046, 0.123]),
    ("thumb_2", [0.311, -0.074, 0.279]),
    ("thumb_3", [0.470, -0.107, 0.324]),
    ("index_meta", [0.169, 0.032, 0.117]),
    ("index_1", [0.461, 0.101, 0.148]),
    ("index_2", [0.694, 0.074, 0.163]),
    ("index_3", [0.827, 0.029, 0.169]),
    ("middle_meta", [0.154, 0.030, 0.023]),
    ("middle_1", [0.452, 0.126, 0.016]),
    ("middle_2", [0.713, 0.099, 0.014]),
    ("middle_3", [0.871, 0.022, 0.015]),
    ("ring_meta", [0.151, 0.026, -0.039]),
    ("ring_1", [0.417, 0.084, -0.097]),
    ("ring_2", [0.643, 0.061, -0.137]),
    ("ring_3", [0.800, -0.032, -0.155]),
    ("pinky_meta", [0.146, 0.013, -0.108]),
    ("pinky_1", [0.380, 0.013, -0.191]),
    ("pinky_2", [0.533, -0.020, -0.238]),
    ("pinky_3", [0.629, -0.079, -0.257]),
];

fn v(a: [f32; 3]) -> Vec3 {
    Vec3::from_array(a)
}

/// On the right, mirrored across X.
fn sided(p: Vec3, side: Side) -> Vec3 {
    Vec3::new(p.x * side.sign(), p.y, p.z)
}

/// Where a hinge's pole goes: out from the joint `b`, away from the line from
/// `a` to `c`, as far as half the chain; `fallback` (a direction) when the
/// chain is straight.
pub fn pole(a: Vec3, b: Vec3, c: Vec3, fallback: Vec3) -> Vec3 {
    let ac = c - a;
    let t = if ac.length_squared() > 1e-10 {
        (b - a).dot(ac) / ac.length_squared()
    } else {
        0.0
    };
    let off = b - (a + ac * t);
    let len = (a.distance(b) + b.distance(c)) * 0.5;
    let dir = if off.length() > len * 0.02 {
        off.normalize()
    } else {
        fallback
    };
    b + dir * len
}

/// Fingers from a hand frame (columns: X out, Y back of the hand, Z), the
/// wrist and the hand's length. Fingertips continue the last segment.
fn fingers(frame: Mat3, wrist: Vec3, length: f32, side: Side) -> HashMap<String, Vec3> {
    let mut out = HashMap::new();
    // The table is a left hand; a right hand's frame is mirrored, so Z flips.
    let flip = Vec3::new(1.0, 1.0, side.sign());
    for (n, p) in FINGERS {
        out.insert(n.to_string(), wrist + frame * (v(p) * flip) * length);
    }
    for f in Finger::ALL {
        let w = f.word();
        let (a, b) = (out[&format!("{w}_2")], out[&format!("{w}_3")]);
        out.insert(format!("{w}_end"), b + (b - a) * 0.85);
    }
    out
}

/// The reference positions of a component's locators.
pub fn reference(part: &PartInfo) -> HashMap<String, Vec3> {
    let side = part.side;
    let mut out: HashMap<String, Vec3> = HashMap::new();
    let table: &[(&str, [f32; 3])] = match part.component {
        Component::Spine => &SPINE,
        Component::Neck => &NECK,
        Component::Arm => &ARM,
        Component::Leg => &LEG,
        Component::Hand => &[],
    };
    for (n, p) in table {
        out.insert(n.to_string(), sided(v(*p), side));
    }
    match part.component {
        Component::Arm => {
            let p = pole(out["shoulder"], out["elbow"], out["wrist"], Vec3::NEG_Z);
            out.insert("pole".into(), p);
        }
        Component::Leg => {
            let p = pole(out["hip"], out["knee"], out["ankle"], Vec3::Z);
            out.insert("pole".into(), p);
        }
        Component::Hand => {
            let wrist = sided(v(ARM[3].1), side);
            let mut f = Mat3::from_cols(v(HAND_FRAME[0]), v(HAND_FRAME[1]), v(HAND_FRAME[2]));
            if side == Side::Right {
                let m = Mat3::from_diagonal(Vec3::new(-1.0, 1.0, 1.0));
                f = m * f;
            }
            out.extend(fingers(f, wrist, HAND_LENGTH, side));
        }
        _ => {}
    }
    out
}

/// The locators autofit places on a measured body. Hands are placed on the
/// wrist of the arm guides they attach to, when those are in `guides`.
pub fn autofit(part: &PartInfo, body: &Body, guides: Option<&AnimData>) -> HashMap<String, Vec3> {
    let side = part.side;
    let s = side.sign();
    let mut out: HashMap<String, Vec3> = HashMap::new();
    let (floor, crotch) = (body.floor, body.crotch);
    // Where the neck starts: just above the top of a mesh without a head.
    let neck_base = if body.has_head {
        body.neck - 0.02 * (body.top - body.floor)
    } else {
        body.top + 0.01
    };
    let armpit = (body.armpit[0] + body.armpit[1]) * 0.5;
    let span = neck_base - armpit;
    let shoulder_y = |sd: Side| body.armpit(sd) + 0.68 * (neck_base - body.armpit(sd));
    let hip_y = crotch + 0.30 * (neck_base - crotch);
    let at = |y: f32| Vec3::new(0.0, y, body.torso_z_at(y));
    match part.component {
        Component::Spine => {
            let chest = crotch + 0.818 * (neck_base - crotch);
            out.insert("hips".into(), at(hip_y));
            out.insert("mid".into(), at(hip_y + (chest - hip_y) * 0.47));
            out.insert("chest".into(), at(chest));
        }
        Component::Neck => {
            let unit = neck_base - floor;
            let (head, end) = if body.has_head {
                (neck_base + 0.42 * (body.top - neck_base), body.top)
            } else {
                (neck_base + 0.07 * unit, neck_base + 0.15 * unit)
            };
            out.insert("neck".into(), at(neck_base.min(body.top)));
            let z = body.torso_z_at(neck_base.min(body.top));
            out.insert("head".into(), Vec3::new(0.0, head, z + 0.02));
            out.insert("head_end".into(), Vec3::new(0.0, end, z + 0.03));
        }
        Component::Arm => {
            let half = body.chest_half(side);
            let y = shoulder_y(side);
            let shoulder = Vec3::new(s * half * 1.03, y, body.torso_z_at(y));
            let cy = y + 0.105 * span;
            out.insert("clavicle".into(), Vec3::new(s * half * 0.07, cy, body.torso_z_at(cy)));
            out.insert("shoulder".into(), shoulder);
            let line = fit::arm_line(body.arm_points(side), shoulder);
            if line.len() > 3 {
                let (elbow, wrist) = (fit::along(&line, 0.448), fit::along(&line, 0.761));
                out.insert("elbow".into(), elbow);
                out.insert("wrist".into(), wrist);
                out.insert("hand_end".into(), *line.last().unwrap());
                out.insert("pole".into(), pole(shoulder, elbow, wrist, Vec3::NEG_Z));
            } else {
                // No arm found: the reference arm, from this shoulder.
                let r = reference(part);
                let d = shoulder - r["shoulder"];
                for n in ["elbow", "wrist", "hand_end", "pole"] {
                    out.insert(n.into(), r[n] + d);
                }
            }
        }
        Component::Leg => {
            let y_hip = hip_y;
            let hip = body.leg_at(side, crotch - 0.03 * (crotch - floor));
            let hip = Vec3::new(hip.x * 0.9, y_hip, hip.z);
            let ankle_y = floor + 0.094 * (y_hip - floor);
            let knee_y = ankle_y + 0.475 * (y_hip - ankle_y);
            let knee = body.leg_at(side, knee_y);
            let ankle = body.leg_at(side, ankle_y);
            let (toe, heel) = (body.toe(side), body.heel(side));
            let ball_z = heel.z + 0.733 * (toe.z - heel.z);
            let ball = Vec3::new(heel.x + 0.733 * (toe.x - heel.x) + 0.012 * s, floor + 0.01, ball_z);
            out.insert("hip".into(), hip);
            out.insert("knee".into(), knee);
            out.insert("ankle".into(), ankle);
            out.insert("ball".into(), ball);
            out.insert("toe".into(), Vec3::new(toe.x, floor + 0.01, toe.z));
            out.insert("heel".into(), Vec3::new(heel.x, floor, heel.z));
            out.insert("pole".into(), pole(hip, knee, ankle, Vec3::Z));
        }
        Component::Hand => {
            // On the arm it attaches to: its wrist, and the arm's points past it.
            let attach = part
                .attach
                .clone()
                .or_else(|| default_attach(part.component, side, part.index));
            let arm_part = attach
                .as_deref()
                .and_then(|a| a.strip_suffix("_wrist"))
                .map(str::to_string);
            let world = guides.map(|g| g.world_pose(0));
            let find = |name: &str| -> Option<Vec3> {
                let g = guides?;
                let i = g.joints.iter().position(|j| j.name == name)?;
                Some(world.as_ref()?[i].w_axis.truncate())
            };
            let wrist = arm_part.as_deref().and_then(|a| find(&locator_name(a, "wrist")));
            let shoulder = arm_part.as_deref().and_then(|a| find(&locator_name(a, "shoulder")));
            let fitted = match (wrist, shoulder) {
                (Some(w), Some(sh)) => {
                    let reach = w.distance(sh);
                    let hand: Vec<Vec3> = body
                        .arm_points(side)
                        .iter()
                        .filter(|p| p.distance(sh) > reach)
                        .copied()
                        .collect();
                    let length = hand.iter().map(|p| p.distance(w)).fold(0.0f32, f32::max);
                    fit::hand_frame(&hand, w, side, Vec3::Z).map(|f| {
                        // The hand's length along its own axis.
                        let along = hand.iter().map(|p| (*p - w).dot(f.x_axis)).fold(0.0f32, f32::max);
                        fingers(f, w, if along > 1e-4 { along } else { length }, side)
                    })
                }
                _ => None,
            };
            match fitted {
                Some(f) => out.extend(f),
                None => out.extend(reference(part)),
            }
        }
    }
    out
}

/// A locator's offset by hand, after autofit.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GuideEdit {
    /// The locator, without the component's name: `elbow`.
    pub locator: String,
    /// Metres.
    pub offset: [f32; 3],
}

/// The guides coming in with one component added: its locators where
/// autofit (or the reference body) puts them, moved by `edits`. The mesh, if
/// any, rides along as a skin bound to the first locator, so the guides are
/// seen on it.
pub fn add_part(
    incoming: Option<&AnimData>,
    part: &PartInfo,
    mesh: Option<&[Surface]>,
    body: Option<&Body>,
    autofit_on: bool,
    edits: &[GuideEdit],
) -> AnimData {
    let name = part.name();
    let mut joints: Vec<Joint> = incoming.map(|g| g.joints.clone()).unwrap_or_default();
    let mut info: RigInfo = incoming.and_then(|g| g.rig.as_deref().cloned()).unwrap_or_default();
    info.guides.resize(joints.len(), None);
    // A component of the same name is replaced: its locators taken out, what
    // hung from them left where it is.
    if info.parts.iter().any(|p| p.name() == name) {
        let world: Vec<Vec3> = incoming
            .map(|g| g.world_pose(0).iter().map(|m| m.w_axis.truncate()).collect())
            .unwrap_or_default();
        let mut map: Vec<Option<usize>> = vec![None; joints.len()];
        let (mut kept, mut kept_world, mut tags) = (vec![], vec![], vec![]);
        for (i, j) in joints.iter().enumerate() {
            if info.guides[i].as_ref().is_some_and(|g| g.part == name) {
                continue;
            }
            map[i] = Some(kept.len());
            let mut j = j.clone();
            j.parent = j.parent.and_then(|p| map[p]);
            let up = j.parent.map(|p| kept_world[p]).unwrap_or(Vec3::ZERO);
            j.rest = Transform::from_translation(world[i] - up);
            kept.push(j);
            kept_world.push(world[i]);
            tags.push(info.guides[i].clone());
        }
        joints = kept;
        info.guides = tags;
        info.parts.retain(|p| p.name() != name);
    }
    let world: Vec<Vec3> = {
        let mut w: Vec<Vec3> = vec![];
        for j in &joints {
            let up = j.parent.map(|p| w[p]).unwrap_or(Vec3::ZERO);
            w.push(up + j.rest.translation);
        }
        w
    };
    let positions = match (autofit_on, body) {
        (true, Some(b)) => autofit(part, b, incoming),
        _ => reference(part),
    };
    let attach = part
        .attach
        .clone()
        .or_else(|| default_attach(part.component, part.side, part.index));
    let attach_ix = attach.as_ref().and_then(|a| joints.iter().position(|j| &j.name == a));
    let mut placed: HashMap<String, (usize, Vec3)> = HashMap::new();
    for l in locators(part) {
        let mut p = positions.get(&l.name).copied().unwrap_or(Vec3::ZERO);
        if let Some(e) = edits.iter().find(|e| e.locator == l.name) {
            p += Vec3::from_array(e.offset);
        }
        let parent = match &l.parent {
            Some(n) => placed.get(n).map(|x| x.0),
            None => attach_ix,
        };
        let up = parent
            .map(|i| {
                placed
                    .values()
                    .find(|x| x.0 == i)
                    .map(|x| x.1)
                    .unwrap_or_else(|| world[i])
            })
            .unwrap_or(Vec3::ZERO);
        let mut j = Joint::new(
            locator_name(&name, &l.name),
            parent,
            Transform::from_translation(p - up),
        );
        j.is_bone = l.bone;
        placed.insert(l.name.clone(), (joints.len(), p));
        joints.push(j);
        info.guides.push(Some(GuideTag {
            part: name.clone(),
            name: l.name.clone(),
        }));
    }
    let mut part = part.clone();
    part.attach = attach;
    info.parts.push(part);
    let skin = mesh.map(|m| Arc::new(still_skin(m, &joints)));
    AnimData {
        name: "Guides".into(),
        tracks: Arc::new(vec![vec![]; joints.len()]),
        joints,
        rate: FrameRate::from_fps(24.0),
        drop_frame: false,
        start_frame: 1,
        frames: 1,
        source: String::new(),
        source_dir: String::new(),
        subject: String::new(),
        skin,
        space: None,
        human: None,
        rig: Some(Arc::new(info)),
    }
}

/// A mesh that stays where it is, as a skin on the first joint of `joints`:
/// shown with the guides or the skeleton, deforming nothing.
pub fn still_skin(mesh: &[Surface], joints: &[Joint]) -> SkinMesh {
    let mut out = SkinMesh::default();
    for m in mesh {
        let base = out.positions.len() as u32;
        out.positions.extend(m.points.iter().copied());
        out.normals.extend(m.normals(&m.points));
        for poly in &m.polys {
            for k in 1..poly.len().saturating_sub(1) {
                let (a, b, c) = (base + poly[0], base + poly[k], base + poly[k + 1]);
                out.faces.push([a, b, c, c]);
            }
        }
    }
    out.joint = vec![0; out.positions.len()];
    // Joint 0's world: a root's rest is its world.
    let w = joints
        .first()
        .map(|j| j.rest.compute_matrix())
        .unwrap_or(glam::Mat4::IDENTITY);
    out.bind = vec![w; joints.len().max(1)];
    out
}
