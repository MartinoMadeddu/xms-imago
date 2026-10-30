//! Delta Mush, and weights baked from it.
//!
//! Delta mush (Mancewicz et al., 2014): the rest mesh is smoothed, and what
//! smoothing took away is kept at each point, in a frame made from the
//! smoothed surface there. A deformed mesh is smoothed the same way and the
//! detail put back in the smoothed surface's frames. Under a hard skin the
//! seams between regions are smoothed out while the detail stays.
//!
//! Bake Weights: the hard skin, moved through a range of motion and mushed,
//! is the target; each point gets the blend of up to four joints (from the
//! regions around it) that follows that target best over all those frames,
//! in the least squares sense, the weights at least zero and summing to one.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat3, Mat4, Vec3};
use rayon::prelude::*;

use super::bind::{joint_names, read_weights, surfaces, write_weights};
use super::surface::Surface;
use crate::core::anim::AnimData;
use crate::core::geo_mesh::GeoMesh;
use crate::types::{Placement, Prim};

/// How much a mesh is smoothed.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MushSettings {
    pub iterations: u32,
    /// How far each pass moves a point toward its neighbours, 0 to 1.
    pub step: f32,
}

impl Default for MushSettings {
    fn default() -> Self {
        MushSettings {
            iterations: 20,
            step: 0.5,
        }
    }
}

/// A surface's frame at each point: tangent, normal, bitangent.
fn frames(s: &Surface, pos: &[Vec3]) -> Vec<Mat3> {
    let normals = s.normals(pos);
    (0..pos.len())
        .map(|i| {
            let n = normals[i];
            let t = s.near[i]
                .first()
                .map(|q| pos[*q as usize] - pos[i])
                .map(|d| d - n * d.dot(n))
                .filter(|d| d.length_squared() > 1e-16)
                .map(|d| d.normalize())
                .unwrap_or_else(|| n.any_orthonormal_vector());
            Mat3::from_cols(t, n, t.cross(n))
        })
        .collect()
}

/// A surface ready to be mushed: its rest detail.
pub struct Mush {
    pub surface: Surface,
    settings: MushSettings,
    deltas: Vec<Vec3>,
}

impl Mush {
    pub fn new(surface: Surface, settings: MushSettings) -> Mush {
        let smooth = surface.smooth(&surface.points, settings.iterations, settings.step);
        let f = frames(&surface, &smooth);
        let deltas = (0..smooth.len())
            .map(|i| f[i].transpose() * (surface.points[i] - smooth[i]))
            .collect();
        Mush {
            surface,
            settings,
            deltas,
        }
    }

    /// `deformed` (the surface's points moved) mushed.
    pub fn apply(&self, deformed: &[Vec3]) -> Vec<Vec3> {
        let smooth = self
            .surface
            .smooth(deformed, self.settings.iterations, self.settings.step);
        let f = frames(&self.surface, &smooth);
        (0..smooth.len()).map(|i| smooth[i] + f[i] * self.deltas[i]).collect()
    }
}

/// Each joint of `clip` as a joint of `rest`, by name; the skin matrices
/// (where a joint is in the clip's frame against where it is at rest).
fn skin_matrices(rest: &AnimData, clip: &AnimData, frame: usize) -> Vec<Mat4> {
    let rw = rest.world_pose(0);
    let cw = clip.world_pose(frame);
    let by: HashMap<&str, usize> = clip
        .joints
        .iter()
        .enumerate()
        .map(|(i, j)| (j.name.as_str(), i))
        .collect();
    rest.joints
        .iter()
        .enumerate()
        .map(|(j, jt)| match by.get(jt.name.as_str()) {
            Some(c) => cw[*c] * rw[j].inverse(),
            None => Mat4::IDENTITY,
        })
        .collect()
}

/// Points moved by weights and skin matrices.
fn blend(points: &[Vec3], weights: &[[(u32, f32); 4]], m: &[Mat4]) -> Vec<Vec3> {
    points
        .par_iter()
        .zip(weights.par_iter())
        .map(|(p, w)| {
            let mut out = Vec3::ZERO;
            for (j, x) in w {
                if *x > 0.0 {
                    out += m
                        .get(*j as usize)
                        .copied()
                        .unwrap_or(Mat4::IDENTITY)
                        .transform_point3(*p)
                        * *x;
                }
            }
            out
        })
        .collect()
}

/// A primitive with its points at `world` positions.
pub fn with_points(p: &Prim, world: &[Vec3]) -> Prim {
    let place = match &p.place {
        Placement::One(m) => *m,
        _ => Mat4::IDENTITY,
    };
    let inv = place.inverse();
    let Some(geo) = &p.geo else { return p.clone() };
    let mut g = (**geo).clone();
    let pts: Vec<[f32; 3]> = world.iter().map(|w| inv.transform_point3(*w).to_array()).collect();
    *g.points_mut() = pts;
    Prim {
        mesh: Arc::new(g.to_mesh()),
        geo: Some(Arc::new(g)),
        ..p.clone()
    }
}

/// Delta Mush: the weighted primitives skinned to `clip`'s frame `frame`
/// against `rest`, and mushed. Primitives without weights are left as they
/// are.
pub fn delta_mush(prims: &[Prim], rest: &AnimData, clip: &AnimData, frame: usize, s: MushSettings) -> Vec<Prim> {
    let m = skin_matrices(rest, clip, frame);
    let mut out = prims.to_vec();
    for (i, surf) in surfaces(prims) {
        let Some(w) = read_weights(&prims[i], rest) else {
            continue;
        };
        let moved = blend(&surf.points, &w, &m);
        let mush = Mush::new(surf, s);
        out[i] = with_points(&prims[i], &mush.apply(&moved));
    }
    out
}

/// How well baked weights follow the mushed skin.
#[derive(Clone, Debug, Default)]
pub struct BakeReport {
    pub frames: usize,
    pub points: usize,
    /// Distance from the mushed skin, metres: mean and most, over every
    /// point of every frame used.
    pub mean: f32,
    pub max: f32,
    /// The same for the hard skin, unmushed: what the bake started from.
    pub hard_mean: f32,
    pub hard_max: f32,
    /// The region of the point farthest off.
    pub worst: Option<String>,
}

/// The heaviest joint of each point's neighbours within `rings` edges.
fn candidates(s: &Surface, labels: &[u32], rings: usize, most: usize) -> Vec<Vec<u32>> {
    (0..labels.len())
        .into_par_iter()
        .map(|i| {
            let mut seen: Vec<u32> = vec![i as u32];
            let mut front = vec![i as u32];
            let mut count: HashMap<u32, usize> = HashMap::new();
            *count.entry(labels[i]).or_default() += 1000;
            for _ in 0..rings {
                let mut next = vec![];
                for p in &front {
                    for q in &s.near[*p as usize] {
                        if !seen.contains(q) {
                            seen.push(*q);
                            next.push(*q);
                            *count.entry(labels[*q as usize]).or_default() += 1;
                        }
                    }
                }
                front = next;
            }
            let mut c: Vec<(u32, usize)> = count.into_iter().collect();
            c.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            c.into_iter().take(most).map(|x| x.0).collect()
        })
        .collect()
}

/// Solve `a x = b` (n×n, row-major) by elimination with pivoting.
fn linear(mut a: Vec<f64>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for c in 0..n {
        let p = (c..n).max_by(|x, y| a[x * n + c].abs().total_cmp(&a[y * n + c].abs()))?;
        if a[p * n + c].abs() < 1e-14 {
            return None;
        }
        if p != c {
            for k in 0..n {
                a.swap(p * n + k, c * n + k);
            }
            b.swap(p, c);
        }
        for r in c + 1..n {
            let f = a[r * n + c] / a[c * n + c];
            for k in c..n {
                a[r * n + k] -= f * a[c * n + k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: f64 = (r + 1..n).map(|k| a[r * n + k] * x[k]).sum();
        x[r] = (b[r] - s) / a[r * n + r];
    }
    Some(x)
}

/// Minimise wᵀGw - 2hᵀw with every weight at least zero and the weights
/// summing to one (G: k×k, row-major). Exact: every subset of the joints is
/// tried with its weights free but summing to one, and the best whose
/// weights are all at least zero wins. Small k (at most 6) keeps it cheap.
fn solve(g: &[f64], h: &[f64], k: usize) -> Vec<f64> {
    let mut best: (f64, Vec<f64>) = (f64::MAX, (0..k).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect());
    for mask in 1u32..(1 << k) {
        let on: Vec<usize> = (0..k).filter(|i| mask & (1 << i) != 0).collect();
        let m = on.len();
        // [2G 1; 1ᵀ 0] [w; μ] = [2h; 1]
        let mut a = vec![0.0; (m + 1) * (m + 1)];
        let mut b = vec![0.0; m + 1];
        for (r, i) in on.iter().enumerate() {
            for (c, j) in on.iter().enumerate() {
                a[r * (m + 1) + c] = 2.0 * g[i * k + j];
            }
            a[r * (m + 1) + m] = 1.0;
            a[m * (m + 1) + r] = 1.0;
            b[r] = 2.0 * h[*i];
        }
        b[m] = 1.0;
        let Some(x) = linear(a, b) else { continue };
        if x[..m].iter().any(|w| *w < -1e-9) {
            continue;
        }
        let mut w = vec![0.0; k];
        for (r, i) in on.iter().enumerate() {
            w[*i] = x[r].max(0.0);
        }
        let mut f = 0.0;
        for i in 0..k {
            for j in 0..k {
                f += w[i] * g[i * k + j] * w[j];
            }
            f -= 2.0 * h[i] * w[i];
        }
        // Fewer joints win a tie.
        if f < best.0 - 1e-12 * (1.0 + f.abs()) {
            best = (f, w);
        }
    }
    best.1
}

/// Bake Weights: the hard skin on `prims` moved through `clip` (at most
/// `samples` of its frames, spread over it), mushed, and fitted.
pub fn bake(
    prims: &[Prim],
    rest: &AnimData,
    clip: &AnimData,
    s: MushSettings,
    samples: usize,
) -> (Vec<Prim>, BakeReport) {
    let names = joint_names(rest);
    let mut out = prims.to_vec();
    let mut report = BakeReport::default();
    let total = clip.frames.max(1);
    let n_frames = samples.clamp(1, total);
    let frames: Vec<usize> = (0..n_frames)
        .map(|k| {
            if n_frames == 1 {
                0
            } else {
                k * (total - 1) / (n_frames - 1)
            }
        })
        .collect();
    report.frames = frames.len();
    let matrices: Vec<Vec<Mat4>> = frames.iter().map(|f| skin_matrices(rest, clip, *f)).collect();
    let (mut err_sum, mut err_n, mut hard_sum) = (0.0f64, 0usize, 0.0f64);
    for (i, surf) in surfaces(prims) {
        let Some(w) = read_weights(&prims[i], rest) else {
            continue;
        };
        let labels: Vec<u32> = w.iter().map(|x| x[0].0).collect();
        let hard: Vec<[(u32, f32); 4]> = labels.iter().map(|l| super::bind::hard(*l)).collect();
        let rings = (s.iterations as f32 * s.step).sqrt().ceil() as usize + 2;
        let cands = candidates(&surf, &labels, rings, 6);
        let n = surf.points.len();
        report.points += n;
        // Normal equations, per point: G (k×k) and h (k).
        let mut gs: Vec<Vec<f64>> = cands.iter().map(|c| vec![0.0; c.len() * c.len()]).collect();
        let mut hs: Vec<Vec<f64>> = cands.iter().map(|c| vec![0.0; c.len()]).collect();
        let points = surf.points.clone();
        let mush = Mush::new(surf, s);
        let mut targets_all: Vec<Vec<Vec3>> = vec![];
        for m in &matrices {
            let moved = blend(&points, &hard, m);
            let target = mush.apply(&moved);
            gs.par_iter_mut()
                .zip(hs.par_iter_mut())
                .enumerate()
                .for_each(|(v, (g, h))| {
                    let c = &cands[v];
                    let k = c.len();
                    // Where each joint takes the point, and the target, as moves
                    // from where it rests: small numbers, kept in doubles.
                    let p = points[v];
                    let cols: Vec<glam::DVec3> = c
                        .iter()
                        .map(|j| {
                            (m.get(*j as usize)
                                .copied()
                                .unwrap_or(Mat4::IDENTITY)
                                .transform_point3(p)
                                - p)
                                .as_dvec3()
                        })
                        .collect();
                    let t = (target[v] - p).as_dvec3();
                    for r in 0..k {
                        for q in 0..k {
                            g[r * k + q] += cols[r].dot(cols[q]);
                        }
                        h[r] += cols[r].dot(t);
                    }
                });
            for (a, b) in moved.iter().zip(&target) {
                let d = a.distance(*b);
                hard_sum += d as f64;
                report.hard_max = report.hard_max.max(d);
            }
            targets_all.push(target);
        }
        let solved: Vec<[(u32, f32); 4]> = (0..n)
            .into_par_iter()
            .map(|v| {
                let c = &cands[v];
                let w = solve(&gs[v], &hs[v], c.len());
                let mut pairs: Vec<(u32, f32)> = c
                    .iter()
                    .copied()
                    .zip(w.iter().map(|x| *x as f32))
                    .filter(|x| x.1 > 0.005)
                    .collect();
                pairs.sort_by(|a, b| b.1.total_cmp(&a.1));
                pairs.truncate(4);
                let sum: f32 = pairs.iter().map(|x| x.1).sum();
                let mut top = [(labels[v], 0.0f32); 4];
                if sum <= 1e-6 {
                    top[0].1 = 1.0;
                } else {
                    for (k, p) in pairs.iter().enumerate() {
                        top[k] = (p.0, p.1 / sum);
                    }
                }
                top
            })
            .collect();
        for (m, target) in matrices.iter().zip(&targets_all) {
            let got = blend(&points, &solved, m);
            for (v, (a, b)) in got.iter().zip(target).enumerate() {
                let d = a.distance(*b);
                err_sum += d as f64;
                err_n += 1;
                if d > report.max {
                    report.max = d;
                    report.worst = rest.joints.get(labels[v] as usize).map(|j| j.name.clone());
                }
            }
        }
        out[i] = write_weights(&prims[i], &solved, &names);
    }
    if err_n > 0 {
        report.mean = (err_sum / err_n as f64) as f32;
        report.hard_mean = (hard_sum / err_n as f64) as f32;
    }
    (out, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rig::bind::tests::{tube, two_bones};

    /// The solver: a blend of two found again; a target outside what the
    /// joints reach held to the nearest blend, no weight below zero.
    #[test]
    fn the_solver_finds_a_blend() {
        // Columns a = (1,0), b = (0,1); target 0.3a + 0.7b.
        let g = [1.0, 0.0, 0.0, 1.0];
        let w = solve(&g, &[0.3, 0.7], 2);
        assert!((w[0] - 0.3).abs() < 1e-9 && (w[1] - 0.7).abs() < 1e-9, "{w:?}");
        // Target 1.5a - 0.5b: the best within is all a.
        let w = solve(&g, &[1.5, -0.5], 2);
        assert!((w[0] - 1.0).abs() < 1e-9 && w[1].abs() < 1e-9, "{w:?}");
    }

    /// Mushing an undeformed surface gives it back; a tube bent at a hard
    /// joint, baked, gets weights that blend across the joint and follow the
    /// mushed bend better than the hard skin did.
    #[test]
    fn a_bent_tube_bakes_smooth() {
        let s = tube();
        let m = Mush::new(s.clone(), MushSettings::default());
        let back = m.apply(&s.points);
        assert!(back.iter().zip(&s.points).all(|(a, b)| a.distance(*b) < 1e-4));

        let rest = two_bones();
        let mut clip = rest.clone();
        let bend: Vec<crate::core::xform::Transform> = (0..10)
            .map(|f| {
                let mut t = rest.joints[1].rest;
                t.rotation = glam::Quat::from_rotation_z(f as f32 * 0.12);
                t
            })
            .collect();
        clip.tracks = Arc::new(vec![vec![], bend, vec![]]);
        clip.frames = 10;
        let prim = Prim::new("/tube".into(), {
            let mut md = crate::types::MeshData::default();
            md.vertices = s.points.iter().map(|p| p.to_array()).collect();
            for poly in &s.polys {
                md.indices
                    .extend([poly[0], poly[1], poly[2], poly[0], poly[2], poly[3]]);
            }
            md
        });
        let prim = Prim {
            geo: Some(Arc::new(crate::core::geo::Geo::from_polygons(
                s.points.iter().map(|p| p.to_array()).collect(),
                vec![4; s.polys.len()],
                s.polys.concat(),
            ))),
            ..prim
        };
        let hard = super::super::bind::hard_bind(&[prim], &rest);
        let (baked, r) = bake(&hard, &rest, &clip, MushSettings::default(), 10);
        let w = read_weights(&baked[0], &rest).unwrap();
        // At the joint (ring 10) both bones weigh; far from it one.
        let ring = |k: usize| w[k * 8];
        assert!(ring(10)[1].1 > 0.1, "{:?}", ring(10));
        assert!(
            ring(1)[0].1 > 0.99 && ring(19)[0].1 > 0.99,
            "{:?} {:?}",
            ring(1),
            ring(19)
        );
        assert!(r.mean < r.hard_mean, "{r:?}");
    }
}
