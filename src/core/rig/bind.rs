//! Hard Bind and Envelope: every point of the mesh given to one joint, along
//! the surface, then where one joint's region ends moved by hand.
//!
//! Hard bind: each deforming joint's bone is a segment (from the joint to the
//! point it aims at; a bone with twist joints split between them). A point
//! whose nearest bone is close to it (within 1.6 times that bone's nearest
//! distance to the mesh) seeds that bone's region; the regions then grow
//! across the surface from their seeds, each point going to the region that
//! reaches it first. Growing along the surface, not through space, keeps a
//! hand resting on a thigh off the thigh, and fingers off each other.
//!
//! Envelope: per joint, the boundary with the region before it (its parent's)
//! is a plane across its bone at the joint, which can be slid along the bone
//! (`shift`, a fraction of the bone's length), tilted (two angles about the
//! joint's canonical Y and Z), and the region grown or shrunk along the
//! surface (`grow`, metres). The weights stay 0 or 1.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat3, Quat, Vec3};

use super::surface::Surface;
use super::RigInfo;
use crate::core::anim::AnimData;
use crate::core::geo::{Attr, Column, Context, Role as GeoRole};
use crate::core::geo_mesh::GeoMesh;
use crate::fbx_loader::{SKIN_JOINTS, SKIN_WEIGHTS};
use crate::types::Prim;

/// Each joint's bone in the rest pose (the skeleton's first frame), None for
/// a joint that does not deform. A bone shared with twist joints is split
/// between them, each from where it is to the next along the bone.
pub fn segments(sk: &AnimData) -> Vec<Option<(Vec3, Vec3)>> {
    let w = sk.world_pose(0);
    let pos: Vec<Vec3> = w.iter().map(|m| m.w_axis.truncate()).collect();
    let n = sk.joints.len();
    let Some(rig) = sk.rig.as_deref().filter(|r| r.joints.len() == n) else {
        // No rig: every joint with a child, a bone to its first child.
        return (0..n)
            .map(|j| {
                let kid = (0..n).find(|c| sk.joints[*c].parent == Some(j))?;
                Some((pos[j], pos[kid]))
            })
            .collect();
    };
    let mut out: Vec<Option<(Vec3, Vec3)>> = (0..n)
        .map(|j| rig.joints[j].deform.then_some((pos[j], rig.joints[j].tip)))
        .collect();
    // Bones with twist joints: each joint on the bone takes the stretch
    // around where it is, the bone's own joint the stretch at its end. A
    // twist turns the skin along the bone more the farther from its start
    // (see `rom`), and the joint on each stretch turns as much.
    for j in 0..n {
        let info = &rig.joints[j];
        if !info.deform || info.role.is_twist() {
            continue;
        }
        let on = twists_of(rig, j);
        if on.is_empty() {
            continue;
        }
        let (a, b) = (pos[j], info.tip);
        let d = b - a;
        let frac = |p: Vec3| ((p - a).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
        let mut all: Vec<(f32, usize)> = on.iter().map(|k| (frac(pos[*k]), *k)).collect();
        all.push((1.0, j));
        all.sort_by(|x, y| x.0.total_cmp(&y.0));
        for (i, (f, k)) in all.iter().enumerate() {
            let lo = if i == 0 { 0.0 } else { (all[i - 1].0 + f) * 0.5 };
            let hi = all.get(i + 1).map(|x| (x.0 + f) * 0.5).unwrap_or(1.0);
            out[*k] = Some((a + d * lo, a + d * hi));
        }
    }
    out
}

/// The twist joints on joint `j`'s bone.
pub fn twists_of(rig: &RigInfo, j: usize) -> Vec<usize> {
    let info = &rig.joints[j];
    (0..rig.joints.len())
        .filter(|t| {
            let ti = &rig.joints[*t];
            ti.role.is_twist() && ti.role.bone() == info.role && ti.side == info.side && ti.part == info.part
        })
        .collect()
}

fn segment_distance(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = b - a;
    let t = if ab.length_squared() < 1e-12 {
        0.0
    } else {
        ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    };
    p.distance(a + ab * t)
}

/// The nearest bone to `p`, and how near.
fn nearest(p: Vec3, segs: &[(usize, Vec3, Vec3)]) -> (usize, f32) {
    let mut best = (0usize, f32::MAX);
    for (j, a, b) in segs {
        let d = segment_distance(p, *a, *b);
        if d < best.1 {
            best = (*j, d);
        }
    }
    best
}

/// One joint per point of each surface, by regions grown along it.
pub fn hard_labels(surfaces: &[Surface], sk: &AnimData) -> Vec<Vec<u32>> {
    let segs: Vec<(usize, Vec3, Vec3)> = segments(sk)
        .into_iter()
        .enumerate()
        .filter_map(|(j, s)| s.map(|(a, b)| (j, a, b)))
        .collect();
    if segs.is_empty() {
        return surfaces.iter().map(|s| vec![0; s.points.len()]).collect();
    }
    use rayon::prelude::*;
    let near: Vec<Vec<(usize, f32)>> = surfaces
        .iter()
        .map(|s| s.points.par_iter().map(|p| nearest(*p, &segs)).collect())
        .collect();
    // Each bone's nearest distance to the mesh.
    let mut reach: HashMap<usize, f32> = HashMap::new();
    for (j, d) in near.iter().flatten() {
        let e = reach.entry(*j).or_insert(f32::MAX);
        *e = e.min(*d);
    }
    surfaces
        .iter()
        .zip(&near)
        .map(|(s, near)| {
            let seeds: Vec<(u32, u32, f32)> = near
                .iter()
                .enumerate()
                .filter(|(_, (j, d))| *d <= reach[j] * 1.6 + 1e-4)
                .map(|(i, (j, d))| (i as u32, *j as u32, *d))
                .collect();
            let (label, _) = s.flood(&seeds);
            label
                .iter()
                .zip(near)
                .map(|(l, (j, _))| l.unwrap_or(*j as u32))
                .collect()
        })
        .collect()
}

/// Weights written on a primitive as Load FBX and Capture write them:
/// `skel:joint0..3` (joint names) and `skel:jointWeights`.
pub fn write_weights(p: &Prim, weights: &[[(u32, f32); 4]], names: &Arc<Vec<Arc<str>>>) -> Prim {
    let Some(geo) = p
        .geo
        .clone()
        .or_else(|| Some(Arc::new(crate::core::geo::Geo::from_mesh(&p.mesh))))
    else {
        return p.clone();
    };
    let mut g = (*geo).clone();
    for k in 0..4 {
        let ix: Vec<u32> = weights.iter().map(|w| w[k].0).collect();
        g.set(
            Context::Point,
            SKIN_JOINTS[k],
            Attr {
                column: Column::Token(names.clone()),
                indices: Some(Arc::new(ix)),
                role: GeoRole::None,
            },
        );
    }
    let ws: Vec<[f32; 4]> = weights.iter().map(|w| [w[0].1, w[1].1, w[2].1, w[3].1]).collect();
    g.set(
        Context::Point,
        SKIN_WEIGHTS,
        Attr::new(Column::Vec4(Arc::new(ws)), GeoRole::None),
    );
    Prim {
        mesh: Arc::new(g.to_mesh()),
        geo: Some(Arc::new(g)),
        ..p.clone()
    }
}

pub fn joint_names(sk: &AnimData) -> Arc<Vec<Arc<str>>> {
    Arc::new(sk.joints.iter().map(|j| Arc::from(j.name.as_str())).collect())
}

/// One joint, weight one.
pub fn hard(label: u32) -> [(u32, f32); 4] {
    [(label, 1.0), (label, 0.0), (label, 0.0), (label, 0.0)]
}

/// The primitives (those with polygons) and their surfaces.
pub fn surfaces(prims: &[Prim]) -> Vec<(usize, Surface)> {
    prims
        .iter()
        .enumerate()
        .filter_map(|(i, p)| Surface::of(p).map(|s| (i, s)))
        .collect()
}

/// Hard Bind: every point of every primitive to one joint of `sk`.
pub fn hard_bind(prims: &[Prim], sk: &AnimData) -> Vec<Prim> {
    let surf = surfaces(prims);
    let only: Vec<Surface> = surf.iter().map(|s| s.1.clone()).collect();
    let labels = hard_labels(&only, sk);
    let names = joint_names(sk);
    let mut out = prims.to_vec();
    for ((i, _), l) in surf.iter().zip(labels) {
        let w: Vec<[(u32, f32); 4]> = l.iter().map(|j| hard(*j)).collect();
        out[*i] = write_weights(&prims[*i], &w, &names);
    }
    out
}

/// The weights on a primitive as joints of `sk` (by name), heaviest first:
/// None for a primitive without weights.
pub fn read_weights(p: &Prim, sk: &AnimData) -> Option<Vec<[(u32, f32); 4]>> {
    let geo = p.geo.as_ref()?;
    let by: HashMap<&str, u32> = sk
        .joints
        .iter()
        .enumerate()
        .map(|(i, j)| (j.name.as_str(), i as u32))
        .collect();
    let tok: Vec<(&[Arc<str>], &[u32])> = SKIN_JOINTS
        .iter()
        .map(|n| {
            let a = geo.attr(Context::Point, n)?;
            Some((a.column.tokens()?, a.indices.as_deref()?.as_slice()))
        })
        .collect::<Option<Vec<_>>>()?;
    let w = geo.attr(Context::Point, SKIN_WEIGHTS)?.column.vec4s()?;
    let n = geo.point_count();
    let mut out = Vec::with_capacity(n);
    for v in 0..n {
        let mut top = [(0u32, 0.0f32); 4];
        for k in 0..4 {
            let (names, ix) = tok[k];
            let j = ix
                .get(v)
                .and_then(|i| names.get(*i as usize))
                .and_then(|nm| by.get(&**nm))
                .copied();
            top[k] = (
                j.unwrap_or(0),
                if j.is_some() {
                    w.get(v).map(|x| x[k]).unwrap_or(0.0)
                } else {
                    0.0
                },
            );
        }
        top.sort_by(|a, b| b.1.total_cmp(&a.1));
        out.push(top);
    }
    Some(out)
}

/// One joint's region edited by hand.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EnvelopeEdit {
    pub joint: String,
    /// The boundary with the region before it slid along its bone: a
    /// fraction of the bone's length, + toward its end.
    #[serde(default)]
    pub shift: f32,
    /// The boundary tilted, degrees, about the joint's canonical Y and Z.
    #[serde(default)]
    pub tilt: [f32; 2],
    /// The region grown (+) or shrunk (-) along the surface, metres.
    #[serde(default)]
    pub grow: f32,
}

/// The joint whose region lies before `j`'s along the body: the bone that
/// ends where `j` starts, else its parent.
fn before(j: usize, sk: &AnimData, segs: &[Option<(Vec3, Vec3)>]) -> Option<usize> {
    let (start, end) = segs[j]?;
    let len = start.distance(end).max(1e-4);
    let by_end = segs
        .iter()
        .enumerate()
        .filter(|(i, s)| *i != j && s.is_some())
        .map(|(i, s)| (i, s.unwrap().1.distance(start)))
        .filter(|(_, d)| *d < len * 0.05)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|x| x.0);
    by_end.or_else(|| {
        let mut p = sk.joints[j].parent;
        while let Some(q) = p {
            if segs[q].is_some() {
                return Some(q);
            }
            p = sk.joints[q].parent;
        }
        None
    })
}

/// The canonical frame of joint `j` in the rest pose.
fn canonical(sk: &AnimData, rig: Option<&RigInfo>, j: usize, seg: (Vec3, Vec3)) -> Mat3 {
    let w = sk.world_pose(0)[j];
    let (_, r, _) = w.to_scale_rotation_translation();
    match rig.and_then(|r| r.joints.get(j)) {
        Some(info) => Mat3::from_quat(r * info.canon),
        None => {
            let x = (seg.1 - seg.0).normalize_or(Vec3::Y);
            let y = x.any_orthonormal_vector();
            Mat3::from_cols(x, y, x.cross(y))
        }
    }
}

/// Labels changed by edits, on one surface.
pub fn envelope_labels(s: &Surface, labels: &mut [u32], sk: &AnimData, edits: &[EnvelopeEdit]) {
    let segs = segments(sk);
    for e in edits {
        let Some(j) = sk.joints.iter().position(|x| x.name == e.joint) else {
            continue;
        };
        let Some(seg) = segs[j] else { continue };
        let jl = j as u32;
        if e.shift != 0.0 || e.tilt != [0.0, 0.0] {
            if let Some(p) = before(j, sk, &segs) {
                let c = canonical(sk, sk.rig.as_deref(), j, seg);
                let len = seg.0.distance(seg.1);
                let origin = seg.0 + c.x_axis * e.shift * len;
                let turn = Quat::from_axis_angle(c.y_axis, e.tilt[0].to_radians())
                    * Quat::from_axis_angle(c.z_axis, e.tilt[1].to_radians());
                let normal = turn * c.x_axis;
                // Only the two regions trade points, and only near the joint.
                let near = len * 1.5;
                let pl = p as u32;
                for (i, l) in labels.iter_mut().enumerate() {
                    if *l != jl && *l != pl {
                        continue;
                    }
                    let q = s.points[i];
                    if q.distance(seg.0) > near.max(segs[p].map(|(a, b)| a.distance(b)).unwrap_or(0.0)) {
                        continue;
                    }
                    *l = if (q - origin).dot(normal) >= 0.0 { jl } else { pl };
                }
            }
        }
        if e.grow > 0.0 {
            let seeds: Vec<(u32, u32, f32)> = labels
                .iter()
                .enumerate()
                .filter(|(_, l)| **l == jl)
                .map(|(i, _)| (i as u32, jl, 0.0))
                .collect();
            let (_, dist) = s.flood(&seeds);
            for (i, d) in dist.iter().enumerate() {
                if *d <= e.grow {
                    labels[i] = jl;
                }
            }
        } else if e.grow < 0.0 {
            let seeds: Vec<(u32, u32, f32)> = labels
                .iter()
                .enumerate()
                .filter(|(_, l)| **l != jl)
                .map(|(i, l)| (i as u32, *l, 0.0))
                .collect();
            let (lab, dist) = s.flood(&seeds);
            for i in 0..labels.len() {
                if labels[i] == jl && dist[i] <= -e.grow {
                    if let Some(o) = lab[i] {
                        labels[i] = o;
                    }
                }
            }
        }
    }
}

/// Envelope: the regions of the weights coming in (each point's heaviest
/// joint), edited, written back as a hard skin.
pub fn envelope(prims: &[Prim], sk: &AnimData, edits: &[EnvelopeEdit]) -> Vec<Prim> {
    let names = joint_names(sk);
    let mut out = prims.to_vec();
    for (i, s) in surfaces(prims) {
        let Some(w) = read_weights(&prims[i], sk) else { continue };
        let mut labels: Vec<u32> = w.iter().map(|x| x[0].0).collect();
        envelope_labels(&s, &mut labels, sk, edits);
        let w: Vec<[(u32, f32); 4]> = labels.iter().map(|j| hard(*j)).collect();
        out[i] = write_weights(&prims[i], &w, &names);
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::core::anim::{FrameRate, Joint};
    use crate::core::xform::Transform;

    /// A tube along X from 0 to 2, rings of 8, 21 rings.
    pub(crate) fn tube() -> Surface {
        let mut pts = vec![];
        for r in 0..21 {
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                pts.push(Vec3::new(r as f32 * 0.1, a.cos() * 0.1, a.sin() * 0.1));
            }
        }
        let mut polys = vec![];
        for r in 0..20u32 {
            for k in 0..8u32 {
                let (a, b) = (r * 8 + k, r * 8 + (k + 1) % 8);
                polys.push(vec![a, b, b + 8, a + 8]);
            }
        }
        Surface::new(pts, polys)
    }

    /// Two joints along X: at 0 and at 1, the second to 2.
    pub(crate) fn two_bones() -> AnimData {
        let mut a = crate::core::anim::create_test_clip(0.1, FrameRate::from_fps(24.0));
        a.joints = vec![
            Joint::new("a", None, Transform::from_translation(Vec3::ZERO)),
            Joint::new("b", Some(0), Transform::from_translation(Vec3::X)),
            Joint::new("c", Some(1), Transform::from_translation(Vec3::X)),
        ];
        a.tracks = Arc::new(vec![vec![], vec![], vec![]]);
        a.frames = 1;
        a
    }

    /// A tube over two bones splits at the joint between them; an envelope
    /// edit slides the split, and growing takes a ring.
    #[test]
    fn a_tube_splits_at_its_joint() {
        let s = tube();
        let sk = two_bones();
        let l = hard_labels(std::slice::from_ref(&s), &sk).remove(0);
        // Rings 0..9 to a, 11..20 to b (c, the end, has no bone).
        assert!(l[..80].iter().all(|j| *j == 0), "{:?}", &l[..80]);
        assert!(l[88..168].iter().all(|j| *j == 1));
        let mut m = l.clone();
        envelope_labels(
            &s,
            &mut m,
            &sk,
            &[EnvelopeEdit {
                joint: "b".into(),
                shift: -0.25,
                tilt: [0.0, 0.0],
                grow: 0.0,
            }],
        );
        // Now from x = 0.75 on.
        assert_eq!(m[7 * 8], 0);
        assert_eq!(m[8 * 8], 1);
        let mut g = l.clone();
        envelope_labels(
            &s,
            &mut g,
            &sk,
            &[EnvelopeEdit {
                joint: "b".into(),
                shift: 0.0,
                tilt: [0.0, 0.0],
                grow: 0.15,
            }],
        );
        assert!(g.iter().filter(|j| **j == 1).count() > l.iter().filter(|j| **j == 1).count());
    }
}
