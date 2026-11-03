//! Build Skeleton: joints from guides. Each component makes its joints where
//! its locators are (spine and neck joints spread along their curve, twist
//! joints along their bone), aims each at the next, turns its pole axis to
//! the pole, names it and orients it for the target, and gives it its limits.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat3, Mat4, Quat, Vec3};

use super::presets::{self, Axis, Orient, Preset};
use super::{
    human_limits, locator_name, Component, Finger, JointInfo, Limits, PartInfo, RigInfo, Role, RotOrder, Side,
    TreadPath,
};
use crate::core::anim::{AnimData, FrameRate, Joint};
use crate::core::human::Slot;
use crate::core::xform::Transform;

/// How a skeleton is built.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BuildSettings {
    pub preset: Preset,
    /// Axes the target's way (false), or `aim`, `up` and `mirror` for every
    /// joint (true).
    pub custom_axes: bool,
    pub aim: Axis,
    pub up: Axis,
    /// The right side mirrored in behaviour: its aim and up reversed.
    pub mirror: bool,
    pub order: RotOrder,
    pub root: bool,
    /// Joint limits on; off, the range of motion has none to keep to.
    pub limits: bool,
}

impl Default for BuildSettings {
    fn default() -> Self {
        BuildSettings::of(Preset::Unreal)
    }
}

impl BuildSettings {
    /// What a target sets.
    pub fn of(preset: Preset) -> BuildSettings {
        let (aim, up, mirror) = preset.custom();
        BuildSettings {
            preset,
            custom_axes: false,
            aim,
            up,
            mirror,
            order: preset.order(),
            root: preset.root(),
            limits: true,
        }
    }
}

/// A joint being built.
struct Proto {
    role: Role,
    side: Side,
    part: String,
    pos: Vec3,
    parent: Option<usize>,
    /// Where its X points.
    aim: Option<Vec3>,
    /// The direction its Y leans to.
    pole: Vec3,
    /// A twist joint: the joint whose frame it takes.
    twin: Option<usize>,
    /// A machine part's or an animal's own limits; None: a human's.
    limits: Option<Limits>,
    radius: f32,
    follow: Option<(usize, f32)>,
    tread: Option<TreadPath>,
    size: Vec3,
}

/// The canonical frame: X to `aim`, Y toward `pole` at right angles to X.
fn canonical(pos: Vec3, aim: Option<Vec3>, pole: Vec3, parent: Option<Mat3>) -> Mat3 {
    let x = aim
        .map(|a| a - pos)
        .filter(|d| d.length_squared() > 1e-10)
        .map(|d| d.normalize())
        .or(parent.map(|p| p.x_axis))
        .unwrap_or(Vec3::Y);
    let mut y = pole - x * pole.dot(x);
    if y.length_squared() < 1e-8 {
        y = parent.map(|p| p.y_axis - x * p.y_axis.dot(x)).unwrap_or(Vec3::ZERO);
    }
    if y.length_squared() < 1e-8 {
        y = x.any_orthonormal_vector();
    }
    let y = y.normalize();
    Mat3::from_cols(x, y, x.cross(y))
}

/// The point at `t` along a curve through `a`, `b` (its middle) and `c`.
fn curve(a: Vec3, b: Vec3, c: Vec3, t: f32) -> Vec3 {
    let ctrl = b * 2.0 - (a + c) * 0.5;
    let u = 1.0 - t;
    a * u * u + ctrl * 2.0 * t * u + c * t * t
}

fn slot(role: Role, side: Side, spine: u8, neck_first: bool) -> Option<Slot> {
    let left = side != Side::Right;
    Some(match role {
        Role::Pelvis => Slot::Hips,
        Role::Spine(k) if k == spine => Slot::Chest,
        Role::Neck(1) if neck_first => Slot::Neck,
        Role::Head => Slot::Head,
        Role::Clavicle => {
            if left {
                Slot::LeftClavicle
            } else {
                Slot::RightClavicle
            }
        }
        Role::UpperArm => {
            if left {
                Slot::LeftUpperArm
            } else {
                Slot::RightUpperArm
            }
        }
        Role::LowerArm => {
            if left {
                Slot::LeftLowerArm
            } else {
                Slot::RightLowerArm
            }
        }
        Role::Hand => {
            if left {
                Slot::LeftHand
            } else {
                Slot::RightHand
            }
        }
        Role::Thigh => {
            if left {
                Slot::LeftThigh
            } else {
                Slot::RightThigh
            }
        }
        Role::Calf => {
            if left {
                Slot::LeftCalf
            } else {
                Slot::RightCalf
            }
        }
        Role::Foot => {
            if left {
                Slot::LeftFoot
            } else {
                Slot::RightFoot
            }
        }
        Role::Ball => {
            if left {
                Slot::LeftToe
            } else {
                Slot::RightToe
            }
        }
        _ => return None,
    })
}

/// The skeleton built from `guides` (a clip made by Rig Guide operators).
pub fn build(guides: &AnimData, s: &BuildSettings) -> Result<AnimData, String> {
    let info = guides
        .rig
        .as_deref()
        .filter(|i| !i.parts.is_empty())
        .ok_or("No guides: connect a Rig Guide")?;
    let world: Vec<Vec3> = guides.world_pose(0).iter().map(|m| m.w_axis.truncate()).collect();
    let at: HashMap<&str, Vec3> = guides
        .joints
        .iter()
        .zip(&world)
        .map(|(j, p)| (j.name.as_str(), *p))
        .collect();
    let get = |part: &str, name: &str| -> Option<Vec3> { at.get(locator_name(part, name).as_str()).copied() };
    let front = Vec3::Z;

    let mut protos: Vec<Proto> = vec![];
    // Which joint each locator became, for what attaches to it.
    let mut made: HashMap<String, usize> = HashMap::new();
    let root = if s.root {
        protos.push(Proto {
            role: Role::Root,
            side: Side::Centre,
            part: "root".into(),
            pos: Vec3::ZERO,
            parent: None,
            aim: None,
            pole: front,
            twin: None,
            limits: Some(Limits::none()),
            radius: 0.0,
            follow: None,
            tread: None,
            size: Vec3::ZERO,
        });
        Some(0)
    } else {
        None
    };
    let mut spine_count = 0u8;
    let mut has_neck = false;
    let hand_parts: Vec<&PartInfo> = info.parts.iter().filter(|p| p.component == Component::Hand).collect();

    for part in &info.parts {
        let name = part.name();
        let side = part.side;
        let attach = part.attach.as_ref().and_then(|a| made.get(a).copied()).or(root);
        let push = |protos: &mut Vec<Proto>,
                    role: Role,
                    pos: Vec3,
                    parent: Option<usize>,
                    aim: Option<Vec3>,
                    pole: Vec3|
         -> usize {
            protos.push(Proto {
                role,
                side,
                part: name.clone(),
                pos,
                parent,
                aim,
                pole,
                twin: None,
                limits: None,
                radius: 0.0,
                follow: None,
                tread: None,
                size: Vec3::ZERO,
            });
            protos.len() - 1
        };
        let need = |n: &str| get(&name, n).ok_or_else(|| format!("{name} has no {n} locator"));
        match part.component {
            Component::Spine => {
                let (h, m, c) = (need("hips")?, need("mid")?, need("chest")?);
                let n = part.segments.clamp(1, 9) as u8;
                spine_count = n;
                let pts: Vec<Vec3> = (1..=n)
                    .map(|k| curve(h, m, c, (k as f32 - 0.5) / (n as f32 - 0.5).max(0.5)))
                    .collect();
                let pelvis = push(&mut protos, Role::Pelvis, h, attach, Some(pts[0]), front);
                made.insert(locator_name(&name, "hips"), pelvis);
                let mut up = pelvis;
                for k in 0..n as usize {
                    let aim = pts
                        .get(k + 1)
                        .copied()
                        .or(Some(pts[k] + (pts[k] - if k > 0 { pts[k - 1] } else { h })));
                    up = push(&mut protos, Role::Spine(k as u8 + 1), pts[k], Some(up), aim, front);
                    if k == n as usize / 2 {
                        made.insert(locator_name(&name, "mid"), up);
                    }
                }
                made.insert(locator_name(&name, "chest"), up);
            }
            Component::Neck => {
                has_neck = true;
                let (b, h, e) = (need("neck")?, need("head")?, need("head_end")?);
                let n = part.segments.clamp(1, 9) as u8;
                let mut up = attach;
                for k in 0..n {
                    let p = b.lerp(h, k as f32 / n as f32);
                    let aim = b.lerp(h, (k + 1) as f32 / n as f32);
                    let j = push(&mut protos, Role::Neck(k + 1), p, up, Some(aim), front);
                    if k == 0 {
                        made.insert(locator_name(&name, "neck"), j);
                    }
                    up = Some(j);
                }
                let head = push(&mut protos, Role::Head, h, up, Some(e), front);
                made.insert(locator_name(&name, "head"), head);
            }
            Component::Arm => {
                let (c, sh, el, wr, end, pl) = (
                    need("clavicle")?,
                    need("shoulder")?,
                    need("elbow")?,
                    need("wrist")?,
                    need("hand_end")?,
                    need("pole")?,
                );
                let pole = pl - el;
                // The hand aims at its middle finger's knuckle when a hand is on it.
                let hand_aim = hand_parts
                    .iter()
                    .find(|h| h.attach.as_deref() == Some(&locator_name(&name, "wrist")))
                    .and_then(|h| {
                        let hn = h.name();
                        let ks: Vec<Vec3> = [Finger::Middle, Finger::Index, Finger::Ring]
                            .iter()
                            .filter_map(|f| get(&hn, &format!("{}_1", f.word())))
                            .collect();
                        ks.first().copied()
                    })
                    .unwrap_or(end);
                let cl = push(&mut protos, Role::Clavicle, c, attach, Some(sh), pole);
                let ua = push(&mut protos, Role::UpperArm, sh, Some(cl), Some(el), pole);
                let la = push(&mut protos, Role::LowerArm, el, Some(ua), Some(wr), pole);
                let hand = push(&mut protos, Role::Hand, wr, Some(la), Some(hand_aim), pole);
                for (loc, j) in [("clavicle", cl), ("shoulder", ua), ("elbow", la), ("wrist", hand)] {
                    made.insert(locator_name(&name, loc), j);
                }
                let t = part.twists.min(5);
                for k in 1..=t {
                    let f = k as f32 / (t + 1) as f32;
                    let j = push(
                        &mut protos,
                        Role::UpperArmTwist(k as u8),
                        sh.lerp(el, f),
                        Some(ua),
                        Some(el),
                        pole,
                    );
                    protos[j].twin = Some(ua);
                    let j = push(
                        &mut protos,
                        Role::LowerArmTwist(k as u8),
                        wr.lerp(el, f),
                        Some(la),
                        Some(wr),
                        pole,
                    );
                    protos[j].twin = Some(la);
                }
            }
            Component::Leg => {
                let (hp, kn, an, ba, to, pl) = (
                    need("hip")?,
                    need("knee")?,
                    need("ankle")?,
                    need("ball")?,
                    need("toe")?,
                    need("pole")?,
                );
                let pole = pl - kn;
                let th = push(&mut protos, Role::Thigh, hp, attach, Some(kn), pole);
                let ca = push(&mut protos, Role::Calf, kn, Some(th), Some(an), pole);
                let fo = push(&mut protos, Role::Foot, an, Some(ca), Some(ba), Vec3::Y);
                let bl = push(&mut protos, Role::Ball, ba, Some(fo), Some(to), Vec3::Y);
                for (loc, j) in [("hip", th), ("knee", ca), ("ankle", fo), ("ball", bl)] {
                    made.insert(locator_name(&name, loc), j);
                }
                let t = part.twists.min(5);
                for k in 1..=t {
                    let f = k as f32 / (t + 1) as f32;
                    let j = push(
                        &mut protos,
                        Role::ThighTwist(k as u8),
                        hp.lerp(kn, f),
                        Some(th),
                        Some(kn),
                        pole,
                    );
                    protos[j].twin = Some(th);
                    let j = push(
                        &mut protos,
                        Role::CalfTwist(k as u8),
                        an.lerp(kn, f),
                        Some(ca),
                        Some(an),
                        pole,
                    );
                    protos[j].twin = Some(ca);
                }
            }
            Component::AnimalLeg => {
                let names = ["hip", "knee", "hock", "fetlock", "hoof", "toe"];
                let pts: Vec<Vec3> = names.iter().map(|n| need(n)).collect::<Result<_, _>>()?;
                // Each joint's pole: where its apex points between its
                // neighbours; the hip's, the knee's.
                let apex = |k: usize| -> Vec3 {
                    let (a, b, c) = (pts[k - 1], pts[k], pts[k + 1]);
                    let ac = c - a;
                    let t = ((b - a).dot(ac) / ac.length_squared().max(1e-12)).clamp(0.0, 1.0);
                    (b - (a + ac * t)).normalize_or(front)
                };
                let mut up = attach;
                for k in 0..5usize {
                    let pole = apex((k).clamp(1, 4));
                    let j = push(
                        &mut protos,
                        Role::AnimalLeg(k as u8),
                        pts[k],
                        up,
                        Some(pts[k + 1]),
                        pole,
                    );
                    protos[j].limits = Some(super::animal_limits(k as u8));
                    made.insert(locator_name(&name, names[k]), j);
                    up = Some(j);
                }
            }
            Component::Chain => {
                let (a, m, e, pl) = (need("start")?, need("mid")?, need("end")?, need("pole")?);
                let n = part.segments.clamp(1, 64) as usize;
                let pts: Vec<Vec3> = (0..=n).map(|k| curve(a, m, e, k as f32 / n as f32)).collect();
                let l = part.mech.limits;
                let mut up = attach;
                for k in 0..n {
                    let j = push(
                        &mut protos,
                        Role::Chain(k as u8 + 1),
                        pts[k],
                        up,
                        Some(pts[k + 1]),
                        pl - pts[k],
                    );
                    let swing = l[0].abs().max(l[1].abs()) * 0.5;
                    protos[j].limits = Some(Limits::new([-10.0, 10.0], [-swing, swing], [l[0], l[1]]));
                    if k == 0 {
                        made.insert(locator_name(&name, "start"), j);
                    }
                    if k == n / 2 {
                        made.insert(locator_name(&name, "mid"), j);
                    }
                    made.insert(locator_name(&name, "end"), j);
                    up = Some(j);
                }
            }
            Component::Body => {
                let (c, f) = (need("centre")?, need("front")?);
                let j = push(&mut protos, Role::Body, c, attach, Some(f), Vec3::Y);
                let side = get(&name, "side").map(|x| x.distance(c)).unwrap_or(0.0);
                let top = get(&name, "top").map(|x| x.distance(c)).unwrap_or(0.0);
                protos[j].size = Vec3::new(side, top, f.distance(c));
                protos[j].limits = Some(Limits::new([-4.0, 4.0], [0.0; 2], [-3.0, 3.0]));
                made.insert(locator_name(&name, "centre"), j);
            }
            Component::Hinge => {
                let (p, ax, tip) = (need("pivot")?, need("axis")?, need("tip")?);
                let axis = (ax - p).normalize_or(Vec3::X);
                let out = tip - p;
                let out = (out - axis * out.dot(axis)).normalize_or(axis.any_orthonormal_vector());
                let j = push(
                    &mut protos,
                    Role::Hinge,
                    p,
                    attach,
                    Some(p + out * tip.distance(p).max(1e-3)),
                    axis.cross(out),
                );
                protos[j].limits = Some(Limits::hinge(part.mech.limits[0], part.mech.limits[1]));
                protos[j].radius = ax.distance(p);
                made.insert(locator_name(&name, "pivot"), j);
                made.insert(locator_name(&name, "tip"), j);
            }
            Component::Rotor | Component::Gear => {
                let (c, ax, tip) = (need("centre")?, need("axis")?, need("tip")?);
                let axis = (ax - c).normalize_or(Vec3::Y);
                let out = tip - c;
                let radius = (out - axis * out.dot(axis)).length();
                let out = (out - axis * out.dot(axis)).normalize_or(axis.any_orthonormal_vector());
                // A gear on another gear's centre meshes with it: it hangs
                // from what that gear hangs from and turns the other way.
                let leader = attach.filter(|a| protos[*a].role == Role::Gear && part.component == Component::Gear);
                let parent = match leader {
                    Some(g) => protos[g].parent,
                    None => attach,
                };
                let role = if part.component == Component::Gear {
                    Role::Gear
                } else {
                    Role::Rotor
                };
                let hub = push(&mut protos, role, c, parent, Some(c + axis * radius.max(1e-3)), out);
                protos[hub].limits = Some(Limits::spinning());
                protos[hub].radius = radius;
                if let Some(g) = leader {
                    protos[hub].follow = Some((g, -protos[g].radius / radius.max(1e-6)));
                }
                made.insert(locator_name(&name, "centre"), hub);
                if part.component == Component::Rotor {
                    let n = part.mech.count;
                    for k in 0..n {
                        let turn = Quat::from_axis_angle(axis, k as f32 / n as f32 * std::f32::consts::TAU);
                        let dir = turn * out;
                        let b = push(
                            &mut protos,
                            Role::Blade(k as u8 + 1),
                            c,
                            Some(hub),
                            Some(c + dir * radius),
                            axis,
                        );
                        let l = part.mech.limits;
                        protos[b].limits = Some(Limits::new([l[0], l[1]], [0.0; 2], [0.0; 2]));
                        protos[b].radius = radius;
                    }
                }
            }
            Component::Piston => {
                let (b, e) = (need("base")?, need("end")?);
                let dir = (e - b).normalize_or(Vec3::Y);
                let pole = dir.any_orthonormal_vector();
                let base = push(&mut protos, Role::PistonBase, b, attach, Some(e), pole);
                protos[base].limits = Some(Limits::none());
                let rod = push(
                    &mut protos,
                    Role::PistonRod,
                    e,
                    Some(base),
                    Some(e + dir * b.distance(e)),
                    pole,
                );
                protos[rod].twin = Some(base);
                let t = part.mech.travel;
                protos[rod].limits = Some(Limits::sliding(-t, t));
                made.insert(locator_name(&name, "base"), base);
                made.insert(locator_name(&name, "end"), rod);
            }
            Component::Wheel => {
                let (c, ax, top) = (need("centre")?, need("axle")?, need("top")?);
                let up_dir = Vec3::Y;
                let radius = top.distance(c);
                let mut up = attach;
                if part.mech.travel > 0.0 {
                    let j = push(&mut protos, Role::Suspension, c, up, Some(c + up_dir * radius), front);
                    protos[j].limits = Some(Limits::sliding(-part.mech.travel, part.mech.travel));
                    up = Some(j);
                }
                if part.mech.steer {
                    let j = push(&mut protos, Role::Steer, c, up, Some(c + up_dir * radius), front);
                    let l = part.mech.limits;
                    protos[j].limits = Some(Limits::new([l[0], l[1]], [0.0; 2], [0.0; 2]));
                    up = Some(j);
                }
                let w = push(&mut protos, Role::Wheel, c, up, Some(ax), up_dir);
                protos[w].limits = Some(Limits::spinning());
                protos[w].radius = radius;
                made.insert(locator_name(&name, "centre"), w);
            }
            Component::Tread => {
                let (f, r, top) = (need("front")?, need("rear")?, need("top")?);
                let radius = top.distance(f);
                let back = (r - f).normalize_or(Vec3::NEG_Z);
                let axle = back.cross((top - f).normalize_or(Vec3::Y)).normalize_or(Vec3::X);
                let path = TreadPath {
                    front: f,
                    rear: r,
                    axle,
                    radius,
                    start: 0.0,
                };
                let n = part.mech.count.max(2);
                let len = path.length();
                for k in 0..n {
                    let s0 = k as f32 * len / n as f32;
                    let (p, t) = path.at(s0);
                    // Out of the loop: away from the line between the wheels.
                    let fr = r - f;
                    let tt = ((p - f).dot(fr) / fr.length_squared().max(1e-12)).clamp(0.0, 1.0);
                    let outward = (p - (f + fr * tt)).normalize_or(Vec3::Y);
                    let j = push(
                        &mut protos,
                        Role::TreadLink(k as u16 + 1),
                        p,
                        attach,
                        Some(p + t * (len / n as f32)),
                        outward,
                    );
                    protos[j].limits = Some(Limits::none());
                    protos[j].radius = radius;
                    protos[j].tread = Some(TreadPath { start: s0, ..path });
                }
            }
            Component::Hand => {
                let Some(hand) = attach.filter(|h| protos[*h].role == Role::Hand) else {
                    return Err(format!("{name} is not on an arm's wrist"));
                };
                // The back of the hand: across the knuckles, little finger to
                // index, then about the hand's X, mirrored on the right.
                let x = protos[hand]
                    .aim
                    .map(|a| (a - protos[hand].pos).normalize_or_zero())
                    .unwrap_or(Vec3::X);
                let k1 = |f: Finger| get(&name, &format!("{}_1", f.word()));
                let across = match (
                    k1(Finger::Index).or(k1(Finger::Middle)),
                    k1(Finger::Pinky).or(k1(Finger::Ring)),
                ) {
                    (Some(a), Some(b)) => (a - b).normalize_or_zero(),
                    _ => front,
                };
                let back = if side == Side::Right {
                    x.cross(across)
                } else {
                    across.cross(x)
                };
                let back = if back.length_squared() > 1e-8 {
                    back.normalize()
                } else {
                    protos[hand].pole
                };
                protos[hand].pole = back;
                for f in &part.fingers {
                    let w = f.word();
                    let pole = if *f == Finger::Thumb {
                        (back + across).normalize_or(back)
                    } else {
                        back
                    };
                    let mut up = hand;
                    let first = if *f == Finger::Thumb || !part.metacarpals { 1 } else { 0 };
                    for k in first..=3u8 {
                        let here = if k == 0 { "meta".to_string() } else { k.to_string() };
                        let next = if k == 3 { "end".to_string() } else { (k + 1).to_string() };
                        let (Some(p), Some(a)) =
                            (get(&name, &format!("{w}_{here}")), get(&name, &format!("{w}_{next}")))
                        else {
                            continue;
                        };
                        let j = push(&mut protos, Role::Finger(*f, k), p, Some(up), Some(a), pole);
                        made.insert(locator_name(&name, &format!("{w}_{here}")), j);
                        up = j;
                    }
                }
            }
        }
    }

    // Frames: canonical, then the target's.
    let n = protos.len();
    let mut canon: Vec<Mat3> = vec![Mat3::IDENTITY; n];
    for i in 0..n {
        let p = &protos[i];
        canon[i] = match p.twin {
            Some(t) if t < i => canon[t],
            _ => canonical(p.pos, p.aim, p.pole, p.parent.map(|q| canon[q])),
        };
    }
    let mut frames: Vec<Mat3> = vec![Mat3::IDENTITY; n];
    for i in 0..n {
        let p = &protos[i];
        let o = if s.custom_axes && p.role != Role::Root {
            Orient::Frame {
                aim: s.aim,
                up: s.up,
                flip: s.mirror.then_some(Side::Right),
            }
        } else {
            presets::orient(s.preset, p.role)
        };
        frames[i] = match o {
            Orient::World => Mat3::IDENTITY,
            Orient::Fixed(m) => m,
            Orient::Parent => p.parent.map(|q| frames[q]).unwrap_or(Mat3::IDENTITY),
            Orient::Frame { aim, up, flip } => {
                let f = if flip == Some(p.side) { -1.0 } else { 1.0 };
                presets::frame(aim, up, canon[i].x_axis * f, canon[i].y_axis * f).unwrap_or(canon[i])
            }
        };
    }

    // How far each upper arm hangs below the horizontal, for its limits.
    let drop = |side: Side| -> f32 {
        protos
            .iter()
            .position(|p| p.role == Role::UpperArm && p.side == side)
            .map(|i| (-canon[i].x_axis.y).clamp(-1.0, 1.0).asin().to_degrees().max(0.0))
            .unwrap_or(45.0)
    };
    let (drop_l, drop_r) = (drop(Side::Left), drop(Side::Right));

    let mut joints = Vec::with_capacity(n);
    let mut infos = Vec::with_capacity(n);
    let worlds: Vec<Mat4> = (0..n)
        .map(|i| {
            Mat4::from_cols(
                frames[i].x_axis.extend(0.0),
                frames[i].y_axis.extend(0.0),
                frames[i].z_axis.extend(0.0),
                protos[i].pos.extend(1.0),
            )
        })
        .collect();
    let mut used: HashMap<String, usize> = HashMap::new();
    for (i, p) in protos.iter().enumerate() {
        let local = match p.parent {
            Some(q) => worlds[q].inverse() * worlds[i],
            None => worlds[i],
        };
        let (_, rot, pos) = local.to_scale_rotation_translation();
        let mut nm = presets::name(s.preset, p.role, p.side, &p.part);
        // Two components of one kind on a side (a second arm) keep apart.
        let count = used.entry(nm.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            nm = format!("{nm}{}", *count);
        }
        let mut j = Joint::new(
            nm,
            p.parent,
            Transform {
                translation: pos,
                rotation: rot.normalize(),
                scale: Vec3::ONE,
            },
        );
        j.is_bone = true;
        joints.push(j);
        let limits = (s.limits && p.role != Role::Root).then(|| {
            let l = p
                .limits
                .unwrap_or_else(|| human_limits(p.role, if p.side == Side::Right { drop_r } else { drop_l }));
            if p.side == Side::Right {
                l.mirrored()
            } else {
                l
            }
        });
        infos.push(JointInfo {
            role: p.role,
            side: p.side,
            part: p.part.clone(),
            canon: Quat::from_mat3(&(frames[i].inverse() * canon[i])).normalize(),
            limits,
            order: s.order,
            deform: p.role != Role::Root,
            tip: p.aim.unwrap_or(p.pos + canon[i].x_axis * 0.05),
            radius: p.radius,
            follow: p.follow,
            tread: p.tread,
            size: p.size,
        });
    }
    let human: Vec<(Slot, String)> = protos
        .iter()
        .enumerate()
        .filter_map(|(i, p)| slot(p.role, p.side, spine_count, has_neck).map(|sl| (sl, joints[i].name.clone())))
        .collect();
    let skin = guides.skin.as_ref().map(|sk| {
        let mut sk = (**sk).clone();
        sk.joint = vec![0; sk.positions.len()];
        sk.weights.clear();
        sk.bind = vec![worlds.first().copied().unwrap_or(Mat4::IDENTITY); n.max(1)];
        Arc::new(sk)
    });
    let rig = RigInfo {
        parts: info.parts.clone(),
        guides: vec![],
        joints: infos,
    };
    Ok(AnimData {
        name: "Skeleton".into(),
        tracks: Arc::new(vec![vec![]; n]),
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
        human: Some(Arc::new(human)),
        rig: Some(Arc::new(rig)),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::core::rig::guides::add_part;

    pub(crate) fn human_parts() -> Vec<PartInfo> {
        let p = |component, side, segments| PartInfo {
            component,
            side,
            index: 0,
            attach: None,
            segments,
            twists: 2,
            fingers: Finger::ALL.to_vec(),
            metacarpals: true,
            label: String::new(),
            mech: crate::core::rig::MechParams::of(component),
        };
        vec![
            p(Component::Spine, Side::Centre, 5),
            p(Component::Neck, Side::Centre, 2),
            p(Component::Arm, Side::Left, 0),
            p(Component::Arm, Side::Right, 0),
            p(Component::Hand, Side::Left, 0),
            p(Component::Hand, Side::Right, 0),
            p(Component::Leg, Side::Left, 0),
            p(Component::Leg, Side::Right, 0),
        ]
    }

    /// Reference guides for a whole human.
    pub(crate) fn human_guides() -> AnimData {
        let mut g: Option<AnimData> = None;
        for part in human_parts() {
            g = Some(add_part(g.as_ref(), &part, None, None, &[], false, &[]));
        }
        g.unwrap()
    }

    /// A human from the reference guides, for Unreal: the MetaHuman's names
    /// and hierarchy, its axes where the example character has them, every
    /// joint where its locator is, the human slots found.
    #[test]
    fn a_human_for_unreal() {
        let g = human_guides();
        let sk = build(&g, &BuildSettings::of(Preset::Unreal)).unwrap();
        let ix = |n: &str| {
            sk.joints
                .iter()
                .position(|j| j.name == n)
                .unwrap_or_else(|| panic!("no {n}"))
        };
        let w = sk.world_pose(0);
        let pos = |n: &str| w[ix(n)].w_axis.truncate();
        let axis = |n: &str, k: usize| w[ix(n)].col(k).truncate().normalize();
        assert_eq!(sk.joints[ix("pelvis")].parent, Some(ix("root")));
        assert_eq!(sk.joints[ix("clavicle_l")].parent, Some(ix("spine_05")));
        assert_eq!(sk.joints[ix("neck_01")].parent, Some(ix("spine_05")));
        assert_eq!(sk.joints[ix("index_01_l")].parent, Some(ix("index_metacarpal_l")));
        assert_eq!(sk.joints[ix("upperarm_twist_02_l")].parent, Some(ix("upperarm_l")));
        assert!((pos("lowerarm_l") - Vec3::new(0.362, 1.245, -0.009)).length() < 1e-4);
        assert!((pos("calf_r") - Vec3::new(-0.111, 0.514, 0.019)).length() < 1e-4);
        // As measured on the MetaHuman: spine Y back; left arm X to the child,
        // right to the parent; left thigh X to the parent; foot as the calf.
        assert!(axis("spine_03", 1).z < -0.9, "{}", axis("spine_03", 1));
        assert!(axis("upperarm_l", 0).dot((pos("lowerarm_l") - pos("upperarm_l")).normalize()) > 0.99);
        assert!(axis("upperarm_r", 0).dot((pos("lowerarm_r") - pos("upperarm_r")).normalize()) < -0.99);
        assert!(axis("thigh_l", 0).y > 0.95 && axis("thigh_r", 0).y < -0.95);
        assert!(axis("thigh_l", 1).z < -0.9 && axis("thigh_r", 1).z > 0.9);
        assert!((axis("foot_l", 1) - axis("calf_l", 1)).length() < 1e-4);
        assert!(axis("upperarm_l", 1).z < -0.9 && axis("upperarm_r", 1).z > 0.9);
        let h = crate::core::human::Human::of(&sk);
        assert_eq!(h.found(), crate::core::human::SLOTS, "{:?}", sk.human);
        // Every joint's canonical frame: X along its bone.
        let rig = sk.rig.as_deref().unwrap();
        let elbow = ix("lowerarm_r");
        let c = Mat3::from_quat(w[elbow].to_scale_rotation_translation().1 * rig.joints[elbow].canon);
        assert!(c.x_axis.dot((pos("hand_r") - pos("lowerarm_r")).normalize()) > 0.99);
        // The elbow's pole is behind it.
        assert!(c.y_axis.z < -0.5, "{}", c.y_axis);
    }

    /// The same guides for HumanIK and Maya: names, world-aligned or mirrored
    /// axes, no root.
    #[test]
    fn other_targets() {
        let g = human_guides();
        let hik = build(&g, &BuildSettings::of(Preset::HumanIk)).unwrap();
        assert_eq!(hik.joints[0].name, "Hips");
        assert!(hik.joints.iter().any(|j| j.name == "LeftHandIndex3"));
        assert!(hik
            .world_pose(0)
            .iter()
            .all(|m| (m.x_axis.truncate() - Vec3::X).length() < 1e-4));
        let maya = build(&g, &BuildSettings::of(Preset::Maya)).unwrap();
        let w = maya.world_pose(0);
        let ix = |n: &str| {
            maya.joints
                .iter()
                .position(|j| j.name == n)
                .unwrap_or_else(|| panic!("no {n}"))
        };
        let (l, r) = (ix("arm_L0_forearm_jnt"), ix("arm_R0_forearm_jnt"));
        // Mirrored in behaviour: the right X reverses the left's, mirrored.
        let (xl, xr) = (w[l].x_axis.truncate(), w[r].x_axis.truncate());
        assert!((xl + Vec3::new(-xr.x, xr.y, xr.z)).length() < 1e-3, "{xl} {xr}");
    }

    /// Autofit on the example character's mesh (a MetaHuman body without its
    /// head), built for Unreal, against the character's own joints of the
    /// same names: every joint found, within a few centimetres, its bone axis
    /// the same. The proportions autofit uses were taken from this body, so
    /// this holds the fit to what it was made from; it says nothing of other
    /// bodies.
    #[test]
    fn autofit_finds_the_example_characters_joints() {
        use crate::types::{EvalResult, NodeType};
        let path = crate::examples::path(crate::examples::CHARACTER_FBX);
        if !std::path::Path::new(&path).is_file() {
            println!("SKIPPED: {path} not found");
            return;
        }
        let mut g = crate::node_graph::NodeGraphState::default();
        let who = g.add_node("C".into(), NodeType::LoadFbx { path, take: 0 }, egui::pos2(0.0, 0.0));
        let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
        let Some(EvalResult::Anim(rest)) = g.eval_node_out(who, 2, &mut Default::default(), &pass) else {
            panic!("no rest")
        };
        let Some(EvalResult::Named(prims)) = g.eval_node_out(who, 1, &mut Default::default(), &pass) else {
            panic!("no mesh")
        };
        let points: Vec<Vec3> = prims
            .iter()
            .filter_map(crate::core::rig::surface::Surface::of)
            .flat_map(|s| s.points)
            .collect();
        let body = crate::core::rig::fit::measure(&points).unwrap();
        assert!(!body.has_head && body.arms_apart == [true, true], "{body:?}");
        let mut guides: Option<AnimData> = None;
        for part in human_parts() {
            guides = Some(add_part(guides.as_ref(), &part, None, Some(&body), &[], true, &[]));
        }
        let sk = build(guides.as_ref().unwrap(), &BuildSettings::default()).unwrap();
        let (w, rw) = (sk.world_pose(0), rest.world_pose(0));
        let (mut sum, mut most) = (0.0f32, 0.0f32);
        for (i, j) in sk.joints.iter().enumerate() {
            let r = rest
                .joints
                .iter()
                .position(|x| x.name == j.name)
                .unwrap_or_else(|| panic!("{} not in the character", j.name));
            let d = w[i].w_axis.distance(rw[r].w_axis);
            sum += d;
            most = most.max(d);
            let x = w[i]
                .x_axis
                .truncate()
                .normalize()
                .dot(rw[r].x_axis.truncate().normalize());
            assert!(x > 0.95, "{}: X off by {x}", j.name);
        }
        let mean = sum / sk.joints.len() as f32;
        assert!(mean < 0.015 && most < 0.05, "mean {mean} most {most}");
    }
}
