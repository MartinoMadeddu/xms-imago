//! Surfaces a graph queries: the meshes and curves on the operator's reference
//! inputs (Closest Point on Surface), moved into the space the graph runs in,
//! with a BVH over their faces; and the graph's own geometry, for reading at
//! its own samples.
//!
//! A place on a surface is a face and its parametric coordinates (`Loc`), as
//! Houdini's `xyzdist` and `primuv` have them. A curve is a face whose corners
//! are its points, in order; it is searched along its control points joined by
//! straight lines, as a linear curve is drawn.

use std::sync::{Arc, OnceLock};

use glam::{Mat3, Mat4, Vec3};
use rayon::prelude::*;

use xms_geo::{Geo, Topology};

use super::spatial::{closest_on_triangle, Aabb, Bvh};
use super::types::Loc;

/// What one of the operator's reference inputs holds: the prims wired there by
/// path (groups too, for Get Reference), and the meshes among them.
#[derive(Clone, Debug, Default)]
pub struct Reference {
    pub paths: Vec<String>,
    pub meshes: Vec<RefMesh>,
}

/// A mesh on a reference input: its path, where it is in the world (one entry
/// per copy of an instanced one), and its geometry.
#[derive(Clone, Debug)]
pub struct RefMesh {
    pub path: String,
    pub world: Mat4,
    pub geo: RefGeo,
}

/// A reference mesh's geometry: given, or made the first time a graph needs it
/// (a mesh with no `Geo` yet is not converted for graphs that never query it).
/// Shared by its clones.
#[derive(Clone)]
pub struct RefGeo {
    cell: Arc<OnceLock<Arc<Geo>>>,
    make: Option<Arc<dyn Fn() -> Geo + Send + Sync>>,
}

impl RefGeo {
    pub fn ready(geo: Arc<Geo>) -> RefGeo {
        let cell = OnceLock::new();
        let _ = cell.set(geo);
        RefGeo {
            cell: Arc::new(cell),
            make: None,
        }
    }

    /// Made by `make` when first asked for.
    pub fn later(make: impl Fn() -> Geo + Send + Sync + 'static) -> RefGeo {
        RefGeo {
            cell: Arc::new(OnceLock::new()),
            make: Some(Arc::new(make)),
        }
    }

    pub fn get(&self) -> &Arc<Geo> {
        self.cell
            .get_or_init(|| Arc::new(self.make.as_ref().map(|m| m()).unwrap_or_default()))
    }
}

impl std::fmt::Debug for RefGeo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.cell.get().is_some() {
            "RefGeo(made)"
        } else {
            "RefGeo(later)"
        })
    }
}

/// A mesh or curves of a surface: its geometry, where each face's (curve's)
/// corners start, and the matrix into the graph's space.
pub(crate) struct Mesh {
    pub geo: Arc<Geo>,
    pub face_off: Vec<u32>,
    /// Curves: a corner is a point (`corner_points` is every point in order).
    curve_points: Vec<u32>,
    pub to_graph: Mat4,
    /// For normals: the inverse transpose of `to_graph`'s 3×3.
    pub normals: Mat3,
    /// Not the identity: values with a role are moved.
    pub moved: bool,
}

impl Mesh {
    fn new(geo: Arc<Geo>, to_graph: Mat4) -> Mesh {
        let mut face_off = vec![0u32];
        let mut curve_points = vec![];
        if let Topology::Mesh { counts, .. } | Topology::Curves { counts, .. } = &geo.topology {
            let mut total = 0u32;
            for &c in counts.iter() {
                total += c;
                face_off.push(total);
            }
            if matches!(geo.topology, Topology::Curves { .. }) {
                curve_points = (0..total).collect();
            }
        }
        let normals = Mat3::from_mat4(to_graph).inverse().transpose();
        Mesh {
            geo,
            face_off,
            curve_points,
            to_graph,
            normals,
            moved: to_graph != Mat4::IDENTITY,
        }
    }

    /// The point of each corner.
    pub fn corner_points(&self) -> &[u32] {
        match &self.geo.topology {
            Topology::Mesh { indices, .. } => indices.as_slice(),
            Topology::Curves { .. } => &self.curve_points,
            _ => &[],
        }
    }

    /// The corners of face `f`.
    pub fn corners(&self, f: u32) -> std::ops::Range<u32> {
        let f = f as usize;
        match (self.face_off.get(f), self.face_off.get(f + 1)) {
            (Some(&a), Some(&b)) => a..b,
            _ => 0..0,
        }
    }

    fn faces(&self) -> usize {
        self.face_off.len().saturating_sub(1)
    }

    fn is_curves(&self) -> bool {
        matches!(self.geo.topology, Topology::Curves { .. })
    }

    /// Where corner `c` is, in the graph's space.
    fn corner_at(&self, c: u32) -> Vec3 {
        let p = self
            .corner_points()
            .get(c as usize)
            .and_then(|&p| self.geo.points().get(p as usize))
            .copied()
            .unwrap_or_default();
        self.to_graph.transform_point3(Vec3::from_array(p))
    }

    /// The corners a place on face `f` at `uv` is made from, and their weights
    /// (they sum to 1), by Houdini's rules (see `Loc`), into `out`.
    pub fn weights(&self, f: u32, uv: [f32; 2], out: &mut Vec<(u32, f32)>) {
        out.clear();
        let r = self.corners(f);
        let (first, n) = (r.start, r.end - r.start);
        let (u, v) = (uv[0].clamp(0.0, 1.0), uv[1].clamp(0.0, 1.0));
        if self.is_curves() {
            if n < 2 {
                out.extend((n == 1).then_some((first, 1.0)));
                return;
            }
            let s = u * (n - 1) as f32;
            let j = (s.floor() as u32).min(n - 2);
            let t = s - j as f32;
            out.extend([(first + j, 1.0 - t), (first + j + 1, t)]);
            return;
        }
        match n {
            0 => {}
            1 | 2 => out.push((first, 1.0)),
            3 => out.extend([(first, 1.0 - u - v), (first + 1, u), (first + 2, v)]),
            4 => out.extend([
                (first, (1.0 - u) * (1.0 - v)),
                (first + 1, (1.0 - u) * v),
                (first + 2, u * v),
                (first + 3, u * (1.0 - v)),
            ]),
            _ => {
                // The edge from corner e to the next, `t` along it; `v` of the
                // way to the middle, which is every corner's average.
                let s = u * n as f32;
                let e = (s.floor() as u32).min(n - 1);
                let t = s - e as f32;
                let mid = v / n as f32;
                out.extend((0..n).map(|k| {
                    let w = if k == e {
                        (1.0 - v) * (1.0 - t)
                    } else if k == (e + 1) % n {
                        (1.0 - v) * t
                    } else {
                        0.0
                    };
                    (first + k, w + mid)
                }));
            }
        }
    }

    /// Face `f` as the triangles it is searched (and scattered) on, in the
    /// graph's space, each with what it is of the face.
    fn triangles(&self, f: u32, out: &mut Vec<(Part, [Vec3; 3])>) {
        out.clear();
        let r = self.corners(f);
        let (first, n) = (r.start, r.end - r.start);
        let at = |k: u32| self.corner_at(first + k);
        if self.is_curves() {
            for j in 0..n.saturating_sub(1) {
                out.push((Part::Segment(j), [at(j), at(j + 1), at(j + 1)]));
            }
            return;
        }
        match n {
            0..=2 => {}
            3 => out.push((Part::Tri, [at(0), at(1), at(2)])),
            4 => {
                out.push((Part::Quad, [at(0), at(1), at(2)]));
                out.push((Part::Quad, [at(0), at(2), at(3)]));
            }
            _ => {
                let mid = (0..n).map(at).sum::<Vec3>() / n as f32;
                for e in 0..n {
                    out.push((Part::Fan(e), [mid, at(e), at((e + 1) % n)]));
                }
            }
        }
    }

    /// The parametric coordinates of the place of weights `w` on a triangle of
    /// face `f` (`triangles`) that is `part` of it, at `q`.
    fn uv_of(&self, f: u32, part: Part, w: [f32; 3], q: Vec3) -> [f32; 2] {
        let n = self.corners(f).len() as f32;
        match part {
            Part::Tri => [w[1], w[2]],
            Part::Quad => {
                let first = self.corners(f).start;
                let p = [0, 1, 2, 3].map(|k| self.corner_at(first + k));
                bilinear_uv(p, q)
            }
            Part::Fan(e) => {
                let edge = w[1] + w[2];
                let t = if edge > 0.0 { w[2] / edge } else { 0.0 };
                [(e as f32 + t) / n, w[0]]
            }
            Part::Segment(j) => [(j as f32 + w[1]) / (n - 1.0).max(1.0), 0.0],
        }
    }
}

/// What of its face a triangle searched is: the face itself, half of a quad, a
/// fan triangle of a larger polygon from the edge from corner `e`, or a curve's
/// segment from point `j` (its last corner its second again).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Tri,
    Quad,
    Fan(u32),
    Segment(u32),
}

/// The bilinear coordinates of the place of quad `p` nearest `q`: solved by a
/// few Gauss-Newton steps from the parallelogram's answer, kept on the quad.
fn bilinear_uv(p: [Vec3; 4], q: Vec3) -> [f32; 2] {
    let at = |u: f32, v: f32| {
        p[0] * ((1.0 - u) * (1.0 - v)) + p[1] * ((1.0 - u) * v) + p[2] * (u * v) + p[3] * (u * (1.0 - v))
    };
    // As if it were a parallelogram: q = P0 + u (P3 - P0) + v (P1 - P0).
    let (a, b, d) = (p[3] - p[0], p[1] - p[0], q - p[0]);
    let (aa, ab, bb) = (a.dot(a), a.dot(b), b.dot(b));
    let det = aa * bb - ab * ab;
    let (mut u, mut v) = if det.abs() > f32::EPSILON {
        (
            (bb * a.dot(d) - ab * b.dot(d)) / det,
            (aa * b.dot(d) - ab * a.dot(d)) / det,
        )
    } else {
        (0.5, 0.5)
    };
    for _ in 0..8 {
        (u, v) = (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
        let r = at(u, v) - q;
        let du = (p[3] - p[0]) * (1.0 - v) + (p[2] - p[1]) * v;
        let dv = (p[1] - p[0]) * (1.0 - u) + (p[2] - p[3]) * u;
        let (a11, a12, a22) = (du.dot(du), du.dot(dv), dv.dot(dv));
        let det = a11 * a22 - a12 * a12;
        if det.abs() <= f32::EPSILON {
            break;
        }
        let (g1, g2) = (du.dot(r), dv.dot(r));
        let (su, sv) = ((a22 * g1 - a12 * g2) / det, (a11 * g2 - a12 * g1) / det);
        (u, v) = (u - su, v - sv);
        if su.abs() + sv.abs() < 1e-7 {
            break;
        }
    }
    [u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)]
}

/// A triangle of a surface, in the graph's space: the mesh and face it is of,
/// and what of the face it is.
struct Tri {
    mesh: u32,
    face: u32,
    part: Part,
    p: [Vec3; 3],
}

/// Meshes to read (at samples, or element by element) and to search.
pub(crate) struct Surface {
    pub meshes: Vec<Mesh>,
    /// The number of the first face of each mesh, and of all the faces after:
    /// faces are numbered across the meshes in order (a Primitive).
    first_face: Vec<u32>,
    /// Its triangles and a BVH over them: made the first time it is searched,
    /// not for reading alone.
    index: OnceLock<(Vec<Tri>, Option<Bvh>)>,
}

impl Surface {
    fn new(meshes: Vec<Mesh>) -> Surface {
        let mut first_face = vec![0u32];
        for m in &meshes {
            first_face.push(first_face.last().copied().unwrap_or(0) + m.faces() as u32);
        }
        Surface {
            meshes,
            first_face,
            index: OnceLock::new(),
        }
    }

    /// The graph's own geometry, as its own samples (Scatter's) read it.
    pub fn own(geo: &Geo) -> Surface {
        Surface::new(vec![Mesh::new(Arc::new(geo.clone()), Mat4::IDENTITY)])
    }

    /// The reference prims `paths` name (a group names those below it), in the
    /// space of the graph (`space`: where its primitive is in the world).
    /// Polygons and curves are searched; points only read.
    pub fn of(paths: &[Arc<str>], references: &[Reference], space: Mat4) -> Surface {
        let back = space.inverse();
        let named = |path: &str| {
            paths.iter().any(|p| {
                let p: &str = p;
                path == p || (path.starts_with(p) && path[p.len()..].starts_with('/')) || p == "/"
            })
        };
        Surface::new(
            references
                .iter()
                .flat_map(|r| &r.meshes)
                .filter(|m| named(&m.path))
                .map(|m| Mesh::new(m.geo.get().clone(), back * m.world))
                .collect(),
        )
    }

    /// A sample's Primitive: its face's number across the meshes; -1 for a
    /// sample of nothing.
    pub fn primitive(&self, l: &Loc) -> i32 {
        match self.first_face.get(l.mesh as usize) {
            Some(&f) if l.found() => (f + l.face) as i32,
            _ => -1,
        }
    }

    /// The sample at `uv` on Primitive `prim`; a sample of nothing for a
    /// Primitive the surface does not have.
    pub fn sample(&self, prim: i32, uv: [f32; 2]) -> Loc {
        let total = self.first_face.last().copied().unwrap_or(0);
        if prim < 0 || prim as u32 >= total {
            return Loc {
                mesh: Loc::NONE,
                ..Loc::default()
            };
        }
        let mesh = self.first_face.partition_point(|&f| f <= prim as u32) - 1;
        Loc {
            mesh: mesh as u32,
            face: prim as u32 - self.first_face[mesh],
            uv,
        }
    }

    /// The corners a sample is made from and their weights (see
    /// `Mesh::weights`), into `out`; none for a sample of nothing.
    pub fn weights(&self, l: &Loc, out: &mut Vec<(u32, f32)>) {
        match self.meshes.get(l.mesh as usize) {
            Some(m) => m.weights(l.face, l.uv, out),
            None => out.clear(),
        }
    }

    /// Its faces as the triangles they are searched on, and their BVH, made
    /// once.
    fn index(&self) -> &(Vec<Tri>, Option<Bvh>) {
        self.index.get_or_init(|| {
            let tris: Vec<Tri> = self
                .meshes
                .par_iter()
                .enumerate()
                .flat_map_iter(|(k, m)| {
                    let mut out = vec![];
                    let mut parts = vec![];
                    for f in 0..m.faces() as u32 {
                        m.triangles(f, &mut parts);
                        out.extend(parts.iter().map(|&(part, p)| Tri {
                            mesh: k as u32,
                            face: f,
                            part,
                            p,
                        }));
                    }
                    out
                })
                .collect();
            let boxes: Vec<Aabb> = tris.iter().map(|t| Aabb::around(&t.p)).collect();
            let bvh = Bvh::build(&boxes);
            (tris, bvh)
        })
    }

    /// The sample nearest `p` within `radius` (0 or less: anywhere), and how
    /// far it is. Found nothing: a sample on no mesh, and the radius.
    pub fn closest(&self, p: Vec3, radius: f32) -> (Loc, f32) {
        let limit = if radius > 0.0 { radius * radius } else { f32::INFINITY };
        let mut best = limit;
        let mut hit: Option<(u32, [f32; 3], Vec3)> = None;
        let (tris, bvh) = self.index();
        if let Some(bvh) = bvh {
            bvh.nearest(p, &mut best, |i, best| {
                let t = &tris[i as usize];
                let w = match t.part {
                    Part::Segment(_) => closest_on_segment(p, t.p[0], t.p[1]),
                    _ => closest_on_triangle(p, t.p[0], t.p[1], t.p[2]),
                };
                let q = t.p[0] * w[0] + t.p[1] * w[1] + t.p[2] * w[2];
                let d = (p - q).length_squared();
                if d <= *best {
                    *best = d;
                    hit = Some((i, w, q));
                }
            });
        }
        match hit {
            Some((i, w, q)) => {
                let t = &tris[i as usize];
                let uv = self.meshes[t.mesh as usize].uv_of(t.face, t.part, w, q);
                (
                    Loc {
                        mesh: t.mesh,
                        face: t.face,
                        uv,
                    },
                    best.sqrt(),
                )
            }
            None => (
                Loc {
                    mesh: Loc::NONE,
                    ..Loc::default()
                },
                radius.max(0.0),
            ),
        }
    }

    /// `count` places on its first mesh's polygons, spread evenly by area
    /// (scaled by `weight`: a number per face, or per corner's point), the same
    /// for the same seed: each a triangle of a face picked by area (the
    /// triangles it is searched on), then a place in it. `unit(i, k)`: a
    /// number in [0, 1) for place `i`, draw `k`.
    pub fn scatter(
        &self,
        count: usize,
        weight: impl Fn(u32, &[u32]) -> f32,
        unit: impl Fn(usize, u32) -> f32 + Sync,
    ) -> Vec<Loc> {
        let Some(m) = self.meshes.first().filter(|m| !m.is_curves()) else {
            return vec![];
        };
        let cp = m.corner_points();
        let mut tris: Vec<(u32, Part, [Vec3; 3])> = vec![];
        let mut sums: Vec<f64> = vec![];
        let (mut total, mut parts, mut points) = (0.0f64, vec![], vec![]);
        for f in 0..m.faces() as u32 {
            m.triangles(f, &mut parts);
            points.clear();
            points.extend(m.corners(f).map(|c| cp.get(c as usize).copied().unwrap_or(0)));
            let scale = weight(f, &points).max(0.0);
            for &(part, p) in &parts {
                let area = (p[1] - p[0]).cross(p[2] - p[0]).length() * 0.5;
                total += (area * scale) as f64;
                tris.push((f, part, p));
                sums.push(total);
            }
        }
        if total <= 0.0 || tris.is_empty() {
            return vec![];
        }
        // Each place is its own hash: made in parallel, the same either way.
        (0..count)
            .into_par_iter()
            .map(|i| {
                let t = sums
                    .partition_point(|&s| s <= unit(i, 0) as f64 * total)
                    .min(tris.len() - 1);
                let (face, part, p) = tris[t];
                // Uniform in the triangle: a point in the unit square, folded.
                let (mut a, mut b) = (unit(i, 1), unit(i, 2));
                if a + b > 1.0 {
                    (a, b) = (1.0 - a, 1.0 - b);
                }
                let w = [1.0 - a - b, a, b];
                let q = p[0] * w[0] + p[1] * w[1] + p[2] * w[2];
                Loc {
                    mesh: 0,
                    face,
                    uv: m.uv_of(face, part, w, q),
                }
            })
            .collect()
    }
}

/// The weights of the place nearest `p` on the segment from `a` to `b`, as a
/// triangle's: `[1 - t, t, 0]`.
fn closest_on_segment(p: Vec3, a: Vec3, b: Vec3) -> [f32; 3] {
    let d = b - a;
    let len2 = d.length_squared();
    let t = if len2 > 0.0 {
        ((p - a).dot(d) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    [1.0 - t, t, 0.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Houdini's parametric coordinates: weights read back where they were
    /// found, on a triangle, a quad, a pentagon and a curve.
    #[test]
    fn places_are_their_primitive_and_uv() {
        let mut g = Geo::from_polygons(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [2.0, 0.0, 0.0],
                [2.0, 2.0, 0.0],
                [4.0, 2.0, 0.0],
                [4.0, 0.0, 0.0],
                [6.0, 0.0, 0.0],
                [7.0, 0.0, 0.0],
                [7.5, 1.0, 0.0],
                [6.5, 2.0, 0.0],
                [5.5, 1.0, 0.0],
            ],
            vec![3, 4, 5],
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        );
        let s = Surface::own(&g);
        let mut w = vec![];
        // The triangle: u and v are corners 1's and 2's weights.
        let (l, _) = s.closest(Vec3::new(0.25, 0.5, 1.0), 0.0);
        assert_eq!(s.primitive(&l), 0);
        assert!(
            (l.uv[0] - 0.25).abs() < 1e-6 && (l.uv[1] - 0.5).abs() < 1e-6,
            "{:?}",
            l.uv
        );
        // The quad: corner 1 is (u, v) = (0, 1), corner 3 (1, 0).
        let (l, _) = s.closest(Vec3::new(2.5, 1.5, -1.0), 0.0);
        assert_eq!(s.primitive(&l), 1);
        assert!(
            (l.uv[0] - 0.25).abs() < 1e-5 && (l.uv[1] - 0.75).abs() < 1e-5,
            "{:?}",
            l.uv
        );
        s.weights(&l, &mut w);
        let at: Vec3 = w.iter().map(|&(c, k)| s.meshes[0].corner_at(c) * k).sum();
        assert!((at - Vec3::new(2.5, 1.5, 0.0)).length() < 1e-5, "{at}");
        // The pentagon: back where it was found, and 1 at its middle.
        for q in [
            Vec3::new(6.5, 1.0, 0.0),
            Vec3::new(6.2, 0.4, 0.0),
            Vec3::new(7.1, 1.2, 0.5),
        ] {
            let (l, _) = s.closest(q, 0.0);
            assert_eq!(s.primitive(&l), 2);
            s.weights(&l, &mut w);
            let at: Vec3 = w.iter().map(|&(c, k)| s.meshes[0].corner_at(c) * k).sum();
            assert!((at - Vec3::new(q.x, q.y, 0.0)).length() < 1e-5, "{q}: {at}");
            assert!((w.iter().map(|x| x.1).sum::<f32>() - 1.0).abs() < 1e-6);
        }
        let mid = s.closest(Vec3::new(6.5, 0.8, 0.0), 0.0).0;
        assert!(mid.uv[1] > 0.999, "the middle: {:?}", mid.uv);
        // A curve of four points: u by segment.
        g = Geo::from_points(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [3.0, 0.0, 0.0], [3.0, 1.0, 0.0]]);
        g.topology = Topology::curves(vec![4], xms_geo::CurveBasis::Linear, xms_geo::CurveWrap::Nonperiodic);
        let s = Surface::own(&g);
        let (l, _) = s.closest(Vec3::new(2.0, 0.5, 0.0), 0.0);
        assert!((l.uv[0] - 1.5 / 3.0).abs() < 1e-6 && l.uv[1] == 0.0, "{:?}", l.uv);
        assert_eq!(s.sample(0, l.uv), l);
        assert!(!s.sample(1, l.uv).found() && !s.sample(-1, l.uv).found());
    }
}
