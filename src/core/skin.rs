//! Skin weights kept on the geometry, by joint name, and put on any
//! animation whose joints have those names. After Houdini's KineFX: the
//! geometry carries its weights, the skeleton it was weighted in is a pose of
//! its own (the rest, or capture, pose), and the animation is a third thing.
//! Change the animation and the same weights deform the same mesh.
//!
//! Weights are point attributes: `skel:joint0` to `skel:joint3`, a joint
//! name per point (indexed, so each name is held once), and
//! `skel:jointWeights`, the four weights. Load FBX puts them on the meshes it
//! reads; Capture makes them by proximity to the bones.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use glam::{Mat4, Vec3};

use super::anim::{AnimData, SkinMesh};
use super::geo::{Attr, Column, Context, Geo, Role, Topology};
use crate::fbx_loader::{SKIN_JOINTS, SKIN_WEIGHTS};
use crate::types::{NamedMesh, Placement};

fn pos(m: &Mat4) -> Vec3 { m.w_axis.truncate() }

/// The geometry of a primitive, and the matrix that places it.
fn geo_and_place(p: &NamedMesh) -> Option<(Arc<Geo>, Mat4)> {
    let place = match &p.place { Placement::InPlace => Mat4::IDENTITY, Placement::One(m) => *m, Placement::Many(_) => return None };
    let geo = p.geo.clone().unwrap_or_else(|| Arc::new(Geo::from_mesh(&p.mesh)));
    Some((geo, place))
}

/// A joint's name without its namespace.
fn short(name: &str) -> &str { name.rsplit(|c| c == ':' || c == '|').next().unwrap_or(name) }

/// Where each joint of `clip` is in `skeleton`: by name, then by name
/// without namespace.
fn by_name(names: &[&str], skeleton: &AnimData) -> Vec<Option<usize>> {
    let exact: HashMap<&str, usize> = skeleton.joints.iter().enumerate().map(|(i, j)| (j.name.as_str(), i)).collect();
    let loose: HashMap<&str, usize> = skeleton.joints.iter().enumerate().map(|(i, j)| (short(&j.name), i)).collect();
    names.iter().map(|n| exact.get(n).or_else(|| loose.get(short(n))).copied()).collect()
}

// ── Capture ──────────────────────────────────────────────────────────────────

/// Bones of a skeleton as segments: from each joint to each of its
/// children, weighting the joint. Joints above the hips (a root on the
/// floor, a reference node) weight nothing.
fn bones(skeleton: &AnimData, world: &[Mat4]) -> Vec<(usize, Vec3, Vec3)> {
    let n = skeleton.joints.len();
    let human = super::human::Human::of(skeleton);
    let hips = human.get(super::human::Slot::Hips);
    let above_hips = |j: usize| -> bool {
        let Some(h) = hips else { return skeleton.joints[j].parent.is_none() };
        let mut c = skeleton.joints[h].parent;
        while let Some(p) = c { if p == j { return true; } c = skeleton.joints[p].parent; }
        false
    };
    let mut out = vec![];
    for j in 0..n {
        if above_hips(j) { continue; }
        let kids: Vec<usize> = (0..n).filter(|c| skeleton.joints[*c].parent == Some(j)).collect();
        if kids.is_empty() {
            out.push((j, pos(&world[j]), pos(&world[j])));
        }
        for c in kids { out.push((j, pos(&world[j]), pos(&world[c]))); }
    }
    out
}

fn segment_distance(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = b - a;
    let t = if ab.length_squared() < 1e-12 { 0.0 } else { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) };
    p.distance(a + ab * t)
}

/// Weights for every point of the primitives, by proximity to the bones of
/// `skeleton` in its first frame. `falloff`: how fast a bone's weight drops
/// with distance against the nearest bone's; higher is a harder skin.
/// Only the weight attributes are new; everything else stays shared.
pub fn capture(prims: &[NamedMesh], skeleton: &AnimData, falloff: f32) -> Vec<NamedMesh> {
    let world = skeleton.world_pose(0);
    let segs = bones(skeleton, &world);
    if segs.is_empty() { return prims.to_vec(); }
    let names: Arc<Vec<Arc<str>>> = Arc::new(skeleton.joints.iter().map(|j| Arc::from(j.name.as_str())).collect());
    let power = falloff.max(0.5);
    prims.iter().map(|p| {
        let Some((geo, place)) = geo_and_place(p) else { return p.clone() };
        let mut joint_ix: [Vec<u32>; 4] = Default::default();
        let mut weights = Vec::with_capacity(geo.point_count());
        for q in geo.points() {
            let w = place.transform_point3(Vec3::from_array(*q));
            // Nearest distance to each joint's bones.
            let mut best: HashMap<usize, f32> = HashMap::new();
            for (j, a, b) in &segs {
                let d = segment_distance(w, *a, *b);
                let e = best.entry(*j).or_insert(f32::MAX);
                if d < *e { *e = d; }
            }
            let mut near: Vec<(usize, f32)> = best.into_iter().collect();
            near.sort_by(|a, b| a.1.total_cmp(&b.1));
            near.truncate(4);
            let d0 = near[0].1.max(1e-4);
            let mut top = [(0u32, 0.0f32); 4];
            for (k, (j, d)) in near.iter().enumerate() { top[k] = (*j as u32, (d0 / d.max(1e-4)).powf(power)); }
            let sum: f32 = top.iter().map(|t| t.1).sum();
            for k in 0..4 { joint_ix[k].push(top[k].0); }
            weights.push([top[0].1 / sum, top[1].1 / sum, top[2].1 / sum, top[3].1 / sum]);
        }
        let mut g = (*geo).clone();
        for (k, ix) in joint_ix.into_iter().enumerate() {
            g.set(Context::Point, SKIN_JOINTS[k], Attr { column: Column::Token(names.clone()), indices: Some(Arc::new(ix)), role: Role::None });
        }
        g.set(Context::Point, SKIN_WEIGHTS, Attr::new(Column::Vec4(Arc::new(weights)), Role::None));
        let mesh = Arc::new(g.to_mesh());
        NamedMesh { geo: Some(Arc::new(g)), mesh, ..p.clone() }
    }).collect()
}

// ── Skin ─────────────────────────────────────────────────────────────────────

/// What a Skin node found, for its panel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkinReport {
    pub points: usize,
    /// Primitives with weights, and without (left out).
    pub weighted: usize,
    pub unweighted: usize,
    /// Joint names in the weights that the animation has, and those it has not.
    pub matched: usize,
    pub missing: Vec<String>,
    /// Joints of the animation the rest skeleton does not have: they bind
    /// where the animation rests.
    pub not_in_rest: usize,
}

fn reports() -> &'static Mutex<HashMap<usize, SkinReport>> {
    static R: OnceLock<Mutex<HashMap<usize, SkinReport>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The report of the Skin node that made `clip`.
pub fn report_for(clip: &Arc<AnimData>) -> Option<SkinReport> {
    reports().lock().ok()?.get(&(Arc::as_ptr(clip) as usize)).cloned()
}

pub fn remember_report(clip: &Arc<AnimData>, report: SkinReport) {
    if let Ok(mut r) = reports().lock() {
        if r.len() > 32 { r.clear(); }
        r.insert(Arc::as_ptr(clip) as usize, report);
    }
}

/// `clip` with the skin the primitives carry: their weights, by joint
/// name, bound where `rest` has those joints in its first frame.
pub fn skin(prims: &[NamedMesh], rest: &AnimData, clip: &AnimData) -> (AnimData, SkinReport) {
    let mut report = SkinReport::default();
    let rest_world = rest.world_pose(0);
    let clip_names: Vec<&str> = clip.joints.iter().map(|j| j.name.as_str()).collect();
    let in_rest = by_name(&clip_names, rest);
    let neutral = clip.neutral_world();
    let bind: Vec<Mat4> = in_rest.iter().enumerate().map(|(j, r)| match r { Some(r) => rest_world[*r], None => neutral[j] }).collect();
    report.not_in_rest = in_rest.iter().filter(|r| r.is_none()).count();

    let mut out = SkinMesh { bind, ..Default::default() };
    let mut missing: Vec<String> = vec![];
    let mut matched: std::collections::HashSet<String> = Default::default();
    for p in prims {
        let Some((geo, place)) = geo_and_place(p) else { report.unweighted += 1; continue };
        let tokens: Vec<Option<(&[Arc<str>], &[u32])>> = SKIN_JOINTS.iter().map(|n| {
            let a = geo.attr(Context::Point, n)?;
            Some((a.column.tokens()?, a.indices.as_deref()?.as_slice()))
        }).collect();
        let weights = geo.attr(Context::Point, SKIN_WEIGHTS).and_then(|a| a.column.vec4s());
        let (Some(weights), true) = (weights, tokens.iter().all(|t| t.is_some())) else { report.unweighted += 1; continue };
        report.weighted += 1;
        // Each name of the primitive's table, as a joint of the animation.
        let mut joint_of: HashMap<*const str, Option<usize>> = HashMap::new();
        let base = out.positions.len() as u32;
        for (v, q) in geo.points().iter().enumerate() {
            out.positions.push(place.transform_point3(Vec3::from_array(*q)));
            let mut top = [(0u32, 0.0f32); 4];
            for k in 0..4 {
                let (names, ix) = tokens[k].unwrap();
                let Some(name) = ix.get(v).and_then(|i| names.get(*i as usize)) else { continue };
                let w = weights.get(v).map(|w| w[k]).unwrap_or(0.0);
                if w <= 0.0 { continue; }
                let j = *joint_of.entry(Arc::as_ptr(name)).or_insert_with(|| {
                    let found = by_name(&[name], clip)[0];
                    if found.is_some() { matched.insert(name.to_string()); } else if !missing.iter().any(|m| m.as_str() == &**name) { missing.push(name.to_string()); }
                    found
                });
                if let Some(j) = j { top[k] = (j as u32, w); }
            }
            top.sort_by(|a, b| b.1.total_cmp(&a.1));
            let sum: f32 = top.iter().map(|t| t.1).sum();
            if sum > 1e-8 { for t in top.iter_mut() { t.1 /= sum; } } else { top = [(0, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)]; }
            out.joint.push(top[0].0);
            out.weights.push(top);
        }
        if let Topology::Mesh { counts, indices, left_handed, .. } = &geo.topology {
            let mut at = 0usize;
            for &n in counts.iter() {
                let n = n as usize;
                let mut c: Vec<u32> = indices[at..at + n].iter().map(|i| base + i).collect();
                if *left_handed ^ (place.determinant() < 0.0) { c.reverse(); }
                match n {
                    3 => out.faces.push([c[0], c[1], c[2], c[2]]),
                    4 => out.faces.push([c[0], c[1], c[2], c[3]]),
                    _ => for k in 1..n - 1 { out.faces.push([c[0], c[k], c[k + 1], c[k + 1]]); },
                }
                at += n;
            }
        }
    }
    report.points = out.positions.len();
    report.matched = matched.len();
    report.missing = missing;
    if out.positions.is_empty() { return (clip.clone(), report); }
    let mut normals = vec![Vec3::ZERO; out.positions.len()];
    for f in &out.faces {
        let p = |k: usize| out.positions[f[k] as usize];
        let n = (p(1) - p(0)).cross(p(2) - p(0)) + if SkinMesh::is_tri(f) { Vec3::ZERO } else { (p(2) - p(0)).cross(p(3) - p(0)) };
        for k in 0..4 { normals[f[k] as usize] += n; }
    }
    out.normals = normals.into_iter().map(|n| n.normalize_or(Vec3::Y)).collect();
    (AnimData { skin: Some(Arc::new(out)), ..clip.clone() }, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::anim::{create_test_clip, FrameRate};

    fn clip() -> AnimData { create_test_clip(1.0, FrameRate::new(30, 1)) }
    fn joint(c: &AnimData, name: &str) -> usize { c.joints.iter().position(|j| j.name.ends_with(name)).unwrap() }

    /// A thin box along the left forearm and hand, at rest.
    fn sleeve(c: &AnimData) -> NamedMesh {
        let w = c.rest_world();
        let (a, b) = (pos(&w[joint(c, "LeftForeArm")]), pos(&w[joint(c, "LeftHand")]));
        let mut pts = vec![];
        for t in [0.1f32, 0.5, 0.9, 1.1] {
            let p = a + (b - a) * t;
            for (dx, dz) in [(0.02, 0.02), (-0.02, 0.02), (-0.02, -0.02), (0.02, -0.02)] { pts.push((p + Vec3::new(dx, 0.0, dz)).to_array()); }
        }
        let mut counts = vec![];
        let mut ix = vec![];
        for ring in 0..3u32 {
            for k in 0..4u32 {
                let (a, b) = (ring * 4 + k, ring * 4 + (k + 1) % 4);
                counts.push(4);
                ix.extend([a, b, b + 4, a + 4]);
            }
        }
        let geo = Geo::from_polygons(pts, counts, ix);
        let mesh = geo.to_mesh();
        NamedMesh { geo: Some(Arc::new(geo)), ..NamedMesh::new("/sleeve".into(), mesh) }
    }

    #[test]
    fn captured_weights_follow_the_nearest_bones() {
        let c = clip();
        let input = sleeve(&c);
        // Weighted in the pose the sleeve was made in: the rest pose.
        let out = capture(&[input.clone()], &c.bind_clip(), 6.0);
        let g = out[0].geo.as_ref().unwrap();
        g.validate().unwrap();
        let first = g.attr(Context::Point, SKIN_JOINTS[0]).unwrap();
        let name = |v: usize| first.column.tokens().unwrap()[first.indices.as_ref().unwrap()[v] as usize].to_string();
        // Midway down the forearm, the forearm; at the elbow, the forearm or the upper arm.
        assert!(name(4).ends_with("LeftForeArm"), "{}", name(4));
        assert!(name(0).ends_with("LeftForeArm") || name(0).ends_with("LeftArm"), "{}", name(0));
        let w = g.attr(Context::Point, SKIN_WEIGHTS).unwrap().column.vec4s().unwrap();
        for x in w { assert!((x.iter().sum::<f32>() - 1.0).abs() < 1e-4); }
        // The points themselves are still the very same column.
        let points = |g: &Geo| g.attr(Context::Point, crate::core::geo::POINTS).unwrap().column.clone();
        assert!(points(g).same(&points(input.geo.as_ref().unwrap())));
    }

    #[test]
    fn weights_go_onto_another_clip_by_name() {
        let c = clip();
        let rest = c.bind_clip();
        let captured = capture(&[sleeve(&c)], &rest, 6.0);
        // The same skeleton under another namespace, moving.
        let mut other = c.clone();
        for j in &mut other.joints { j.name = j.name.replace("Take01:", "Other:"); }
        let (skinned, report) = skin(&captured, &rest, &other);
        assert_eq!(report.weighted, 1);
        assert!(report.missing.is_empty(), "{report:?}");
        assert_eq!(report.not_in_rest, 0);
        let s = skinned.skin.as_ref().unwrap();
        assert_eq!(s.positions.len(), 16);
        assert_eq!(s.faces.len(), 12);
        // Each point keeps its place on its bone through the motion.
        let hand = joint(&other, "LeftForeArm");
        let v = 4;
        assert_eq!(s.joint[v] as usize, hand);
        for f in [0usize, 7, 20] {
            let w = other.world_pose(f);
            let moved = (w[hand] * s.bind[hand].inverse()).transform_point3(s.positions[v]);
            let local_then = s.bind[hand].inverse().transform_point3(s.positions[v]);
            let local_now = w[hand].inverse().transform_point3(moved);
            assert!(local_then.distance(local_now) < 1e-4);
        }
        // Names the animation does not have are reported.
        let mut alien = c.clone();
        for j in &mut alien.joints { j.name = format!("x_{}", j.name.len()); }
        let (_, r) = skin(&captured, &rest, &alien);
        assert!(!r.missing.is_empty());
    }
}
