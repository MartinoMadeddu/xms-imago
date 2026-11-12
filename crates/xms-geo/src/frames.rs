//! Normals and tangents made from the geometry: a polygon mesh's normals per
//! point, corner or face; its tangents (along U of its texture coordinates, or
//! along its edges) per point or corner; a curve's tangents along it, per
//! point. What the Normals and Tangents operators write, and the ICE nodes of
//! the same names read.
//!
//! The math is on plain arrays (points, polygon counts and corners), so the
//! same functions serve a whole `Geo` and the engine's columns.

use std::collections::HashMap;

pub type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn length(a: V3) -> f32 {
    dot(a, a).sqrt()
}
/// `a` made one long; `fallback` when it has no direction.
fn unit_or(a: V3, fallback: V3) -> V3 {
    let l = length(a);
    if l > 1e-20 && l.is_finite() {
        scale(a, 1.0 / l)
    } else {
        fallback
    }
}

pub const UP: V3 = [0.0, 1.0, 0.0];
pub const ALONG_X: V3 = [1.0, 0.0, 0.0];

/// How the faces around a point weigh in its normal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Weighting {
    /// By each face's area: big faces count more (Houdini's, Maya's).
    #[default]
    Area,
    /// By the angle each face makes at the point: even across a fan of thin
    /// triangles.
    Angle,
    /// Each face the same.
    Uniform,
}

impl Weighting {
    pub const ALL: [Weighting; 3] = [Weighting::Area, Weighting::Angle, Weighting::Uniform];
    pub fn label(self) -> &'static str {
        match self {
            Weighting::Area => "Area",
            Weighting::Angle => "Angle",
            Weighting::Uniform => "Uniform",
        }
    }
}

/// A polygon mesh, as arrays: corners per polygon and the point of each
/// corner. `left_handed`: corners wind clockwise (USD `orientation`), so the
/// normals face the other way.
#[derive(Clone, Copy, Debug)]
pub struct Polygons<'a> {
    pub points: &'a [V3],
    pub counts: &'a [u32],
    pub indices: &'a [u32],
    pub left_handed: bool,
}

impl Polygons<'_> {
    /// Each polygon's first corner.
    fn starts(&self) -> Vec<usize> {
        let mut at = 0usize;
        self.counts
            .iter()
            .map(|&n| {
                let s = at;
                at += n as usize;
                s
            })
            .collect()
    }

    fn point(&self, corner: usize) -> Option<V3> {
        self.points.get(*self.indices.get(corner)? as usize).copied()
    }

    /// Each polygon's normal, as long as twice its area (Newell's: right for
    /// polygons not quite flat).
    pub fn face_areas(&self) -> Vec<V3> {
        let starts = self.starts();
        self.counts
            .iter()
            .zip(starts)
            .map(|(&n, s)| {
                let mut v = [0.0; 3];
                for k in 0..n as usize {
                    let (Some(a), Some(b)) = (self.point(s + k), self.point(s + (k + 1) % n as usize)) else {
                        continue;
                    };
                    v[0] += (a[1] - b[1]) * (a[2] + b[2]);
                    v[1] += (a[2] - b[2]) * (a[0] + b[0]);
                    v[2] += (a[0] - b[0]) * (a[1] + b[1]);
                }
                if self.left_handed {
                    scale(v, -1.0)
                } else {
                    v
                }
            })
            .collect()
    }

    /// Each polygon's normal, one long.
    pub fn face_normals(&self) -> Vec<V3> {
        self.face_areas().into_iter().map(|v| unit_or(v, UP)).collect()
    }

    /// The angle each corner makes, between its two edges.
    fn corner_angles(&self) -> Vec<f32> {
        let mut out = vec![0.0; self.indices.len()];
        for (&n, s) in self.counts.iter().zip(self.starts()) {
            let n = n as usize;
            for k in 0..n {
                let (Some(p), Some(prev), Some(next)) = (
                    self.point(s + k),
                    self.point(s + (k + n - 1) % n),
                    self.point(s + (k + 1) % n),
                ) else {
                    continue;
                };
                let (a, b) = (unit_or(sub(prev, p), [0.0; 3]), unit_or(sub(next, p), [0.0; 3]));
                out[s + k] = dot(a, b).clamp(-1.0, 1.0).acos();
            }
        }
        out
    }

    /// What each corner gives its point's normal: its face's normal weighed.
    fn corner_weights(&self, w: Weighting) -> Vec<V3> {
        let areas = self.face_areas();
        let angles = matches!(w, Weighting::Angle).then(|| self.corner_angles());
        let mut out = vec![[0.0; 3]; self.indices.len()];
        for (f, (&n, s)) in self.counts.iter().zip(self.starts()).enumerate() {
            for k in 0..n as usize {
                let c = s + k;
                out[c] = match w {
                    Weighting::Area => areas[f],
                    Weighting::Angle => scale(unit_or(areas[f], [0.0; 3]), angles.as_ref().map_or(0.0, |a| a[c])),
                    Weighting::Uniform => unit_or(areas[f], [0.0; 3]),
                };
            }
        }
        out
    }

    /// Normals per point: the faces around each point, weighed. A point no
    /// face uses points up.
    pub fn point_normals(&self, w: Weighting) -> Vec<V3> {
        let mut sum = vec![[0.0; 3]; self.points.len()];
        for (c, v) in self.corner_weights(w).into_iter().enumerate() {
            if let Some(p) = self.indices.get(c).map(|&p| p as usize).filter(|&p| p < sum.len()) {
                sum[p] = add(sum[p], v);
            }
        }
        sum.into_iter().map(|v| unit_or(v, UP)).collect()
    }

    /// Normals per corner: the faces around its point within `cusp` degrees of
    /// its own face, weighed; an edge sharper than that is hard. 180: every
    /// face, as per point; 0: each face's own, flat.
    pub fn corner_normals(&self, w: Weighting, cusp: f32) -> Vec<V3> {
        let faces = self.face_normals();
        let weights = self.corner_weights(w);
        let mut face_of = vec![0usize; self.indices.len()];
        for (f, (&n, s)) in self.counts.iter().zip(self.starts()).enumerate() {
            for k in 0..n as usize {
                if let Some(x) = face_of.get_mut(s + k) {
                    *x = f;
                }
            }
        }
        // The corners at each point.
        let mut at: Vec<Vec<usize>> = vec![vec![]; self.points.len()];
        for (c, &p) in self.indices.iter().enumerate() {
            if let Some(a) = at.get_mut(p as usize) {
                a.push(c);
            }
        }
        let cos = cusp.clamp(0.0, 180.0).to_radians().cos() - 1e-6;
        self.indices
            .iter()
            .enumerate()
            .map(|(c, &p)| {
                let own = faces[face_of[c]];
                let mut v = [0.0; 3];
                for &o in at.get(p as usize).map_or(&[][..], |a| a.as_slice()) {
                    if o == c || dot(faces[face_of[o]], own) >= cos {
                        v = add(v, weights[o]);
                    }
                }
                unit_or(v, own)
            })
            .collect()
    }

    /// Tangents and signs along U of the texture coordinates, one per corner
    /// (`uv`, a corner's coordinates): what normal maps are baked against.
    /// Corners at a point with the same coordinates share theirs; across a seam
    /// they do not. `normals`: one per corner, the tangents made square to
    /// them; the sign says which way V goes (the bitangent is
    /// `sign · normal × tangent`).
    pub fn uv_tangents(&self, uv: &[[f32; 2]], normals: &[V3]) -> (Vec<V3>, Vec<f32>) {
        let mut t_sum: HashMap<(u32, [u32; 2]), (V3, V3)> = HashMap::new();
        let key = |c: usize| {
            let p = self.indices.get(c).copied().unwrap_or(0);
            let u = uv.get(c).copied().unwrap_or([0.0; 2]);
            (p, [u[0].to_bits(), u[1].to_bits()])
        };
        for (&n, s) in self.counts.iter().zip(self.starts()) {
            let n = n as usize;
            for k in 1..n.saturating_sub(1) {
                let tri = [s, s + k, s + k + 1];
                let (Some(p0), Some(p1), Some(p2)) = (self.point(tri[0]), self.point(tri[1]), self.point(tri[2]))
                else {
                    continue;
                };
                let (Some(u0), Some(u1), Some(u2)) = (uv.get(tri[0]), uv.get(tri[1]), uv.get(tri[2])) else {
                    continue;
                };
                let (e1, e2) = (sub(p1, p0), sub(p2, p0));
                let (du1, dv1, du2, dv2) = (u1[0] - u0[0], u1[1] - u0[1], u2[0] - u0[0], u2[1] - u0[1]);
                let det = du1 * dv2 - du2 * dv1;
                if det.abs() < 1e-20 {
                    continue;
                }
                let r = 1.0 / det;
                let t = scale(sub(scale(e1, dv2), scale(e2, dv1)), r);
                let b = scale(sub(scale(e2, du1), scale(e1, du2)), r);
                for c in tri {
                    let e = t_sum.entry(key(c)).or_insert(([0.0; 3], [0.0; 3]));
                    e.0 = add(e.0, t);
                    e.1 = add(e.1, b);
                }
            }
        }
        let mut tangents = Vec::with_capacity(self.indices.len());
        let mut signs = Vec::with_capacity(self.indices.len());
        for c in 0..self.indices.len() {
            let n = normals.get(c).copied().unwrap_or(UP);
            let (t, b) = t_sum.get(&key(c)).copied().unwrap_or(([0.0; 3], [0.0; 3]));
            let (t, sign) = square_to(n, t, b);
            tangents.push(t);
            signs.push(sign);
        }
        (tangents, signs)
    }

    /// Tangents along each corner's edge to the next corner of its face, made
    /// square to `normals` (one per corner): for meshes with no texture
    /// coordinates. Signs are all 1.
    pub fn edge_tangents(&self, normals: &[V3]) -> Vec<V3> {
        let mut out = vec![ALONG_X; self.indices.len()];
        for (&n, s) in self.counts.iter().zip(self.starts()) {
            let n = n as usize;
            for k in 0..n {
                let (Some(a), Some(b)) = (self.point(s + k), self.point(s + (k + 1) % n)) else {
                    continue;
                };
                let nrm = normals.get(s + k).copied().unwrap_or(UP);
                out[s + k] = square_to(nrm, sub(b, a), [0.0; 3]).0;
            }
        }
        out
    }

    /// Per-corner values made one per point: their average, one long.
    pub fn to_points(&self, per_corner: &[V3], fallback: V3) -> Vec<V3> {
        let mut sum = vec![[0.0; 3]; self.points.len()];
        for (c, &p) in self.indices.iter().enumerate() {
            if let (Some(s), Some(v)) = (sum.get_mut(p as usize), per_corner.get(c)) {
                *s = add(*s, *v);
            }
        }
        sum.into_iter().map(|v| unit_or(v, fallback)).collect()
    }
}

/// `t` made square to `n` and one long (Gram–Schmidt); with `b`, the sign of
/// the frame (-1 where `n × t` and `b` point apart: mirrored coordinates).
fn square_to(n: V3, t: V3, b: V3) -> (V3, f32) {
    let flat = sub(t, scale(n, dot(n, t)));
    // No direction along the surface: any one square to the normal.
    let other = if n[0].abs() < 0.9 { ALONG_X } else { UP };
    let t = unit_or(flat, unit_or(cross(other, n), ALONG_X));
    let sign = if dot(cross(n, t), b) < 0.0 { -1.0 } else { 1.0 };
    (t, sign)
}

/// Curves' tangents, one per point: the direction along the curve there, from
/// the point before to the point after (at an end, the one segment there; a
/// periodic curve goes round). `counts`: points per curve, end to end.
pub fn curve_tangents(points: &[V3], counts: &[u32], periodic: bool) -> Vec<V3> {
    let mut out = vec![ALONG_X; points.len()];
    let mut s = 0usize;
    for &n in counts {
        let n = n as usize;
        if s + n > points.len() {
            break;
        }
        let p = &points[s..s + n];
        for i in 0..n {
            let (before, after) = if periodic && n > 2 {
                (p[(i + n - 1) % n], p[(i + 1) % n])
            } else {
                (p[i.saturating_sub(1)], p[(i + 1).min(n - 1)])
            };
            let d = sub(after, before);
            // Two points in the same place: the curve's direction anywhere.
            let whole = sub(p[n - 1], p[0]);
            out[s + i] = unit_or(d, unit_or(whole, ALONG_X));
        }
        s += n;
    }
    out
}

/// Curves' normals, one per point: square to the curve, turning no more than
/// it does (parallel transport, a rotation-minimising frame), starting from
/// `up` made square to the first tangent. What ribbons face.
pub fn curve_normals(points: &[V3], counts: &[u32], periodic: bool, up: V3) -> Vec<V3> {
    let tangents = curve_tangents(points, counts, periodic);
    let mut out = vec![UP; points.len()];
    let mut s = 0usize;
    for &n in counts {
        let n = n as usize;
        if n == 0 || s + n > points.len() {
            break;
        }
        let mut normal = square_to(tangents[s], up, [0.0; 3]).0;
        out[s] = normal;
        for i in 1..n {
            let (a, b) = (tangents[s + i - 1], tangents[s + i]);
            // Turned as the tangent turns: about their cross, by their angle.
            let axis = cross(a, b);
            let sin = length(axis);
            let cos = dot(a, b).clamp(-1.0, 1.0);
            if sin > 1e-8 {
                let k = scale(axis, 1.0 / sin);
                // Rodrigues.
                normal = add(
                    add(scale(normal, cos), scale(cross(k, normal), sin)),
                    scale(k, dot(k, normal) * (1.0 - cos)),
                );
            }
            normal = square_to(b, normal, [0.0; 3]).0;
            out[s + i] = normal;
        }
        s += n;
    }
    out
}

// On a whole geometry

use crate::{Attr, Column, Context, CurveWrap, Geo, Role, Topology, UV_NAMES};
use std::sync::Arc;

/// Where tangents come from on a polygon mesh.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TangentsFrom {
    /// Along U of the texture coordinates (normal maps are baked against
    /// these); along the edges where the mesh has none.
    #[default]
    Uv,
    /// Along each corner's edge to the next corner of its face.
    Edges,
}

impl Geo {
    fn polygons(&self) -> Option<Polygons<'_>> {
        let Topology::Mesh {
            counts,
            indices,
            left_handed,
            ..
        } = &self.topology
        else {
            return None;
        };
        Some(Polygons {
            points: self.points(),
            counts,
            indices,
            left_handed: *left_handed,
        })
    }

    fn curves(&self) -> Option<(&[u32], bool)> {
        match &self.topology {
            Topology::Curves { counts, wrap, .. } => Some((counts, *wrap == CurveWrap::Periodic)),
            _ => None,
        }
    }

    /// A corner's values of a 2D attribute, wherever it lives.
    fn per_corner_uv(&self, name: &str) -> Option<Vec<[f32; 2]>> {
        let Topology::Mesh { counts, indices, .. } = &self.topology else {
            return None;
        };
        let (ctx, a) = self.find(name)?;
        let values = a.column.vec2s()?;
        let mut faces = Vec::with_capacity(indices.len());
        for (f, &n) in counts.iter().enumerate() {
            faces.extend(std::iter::repeat_n(f, n as usize));
        }
        Some(
            (0..indices.len())
                .map(|c| {
                    let element = match ctx {
                        Context::Object => 0,
                        Context::Point => indices[c] as usize,
                        Context::Face => faces.get(c).copied().unwrap_or(0),
                        Context::Corner => c,
                    };
                    a.value_index(element)
                        .and_then(|i| values.get(i).copied())
                        .unwrap_or([0.0; 2])
                })
                .collect(),
        )
    }

    /// The texture coordinates tangents follow: `name`, else `st` or another
    /// of their names, else any 2D texture coordinates.
    pub fn uv_name(&self, name: &str) -> Option<String> {
        let is_uv = |a: &Attr| a.column.vec2s().is_some();
        if !name.trim().is_empty() {
            return self
                .find(name.trim())
                .filter(|(_, a)| is_uv(a))
                .map(|_| name.trim().to_string());
        }
        UV_NAMES
            .iter()
            .find(|n| self.find(n).is_some_and(|(_, a)| is_uv(a)))
            .map(|n| n.to_string())
            .or_else(|| {
                Context::ALL.into_iter().find_map(|c| {
                    self.attrs(c)
                        .find(|(_, a)| a.role == Role::TexCoord && is_uv(a))
                        .map(|(n, _)| n.to_string())
                })
            })
    }

    /// Normals made from the geometry, per `ctx`: a mesh's per Point (the
    /// faces around, weighed), Corner (hard past `cusp` degrees) or Face; a
    /// curve's per Point, square to it, turning as little as it does from
    /// `up`. `None` for points, or a context it has none in.
    pub fn made_normals(&self, ctx: Context, w: Weighting, cusp: f32, up: V3) -> Option<Attr> {
        let values = if let Some(m) = self.polygons() {
            match ctx {
                Context::Point => m.point_normals(w),
                Context::Corner => m.corner_normals(w, cusp),
                Context::Face => m.face_normals(),
                Context::Object => return None,
            }
        } else if let Some((counts, periodic)) = self.curves() {
            if ctx != Context::Point {
                return None;
            }
            curve_normals(self.points(), counts, periodic, up)
        } else {
            return None;
        };
        Some(Attr::new(Column::Vec3(Arc::new(values)), Role::Normal))
    }

    /// Tangents made from the geometry, per `ctx` (Point or Corner), and for
    /// a mesh the sign of each frame (`bitangent = sign · normal × tangent`).
    /// A mesh's follow U of `uv` (`TangentsFrom::Uv`, its texture coordinates)
    /// or its edges; a curve's go along it, per point. The texture
    /// coordinates used, if any, last.
    pub fn made_tangents(
        &self,
        ctx: Context,
        from: TangentsFrom,
        uv: &str,
        w: Weighting,
    ) -> Option<(Attr, Option<Attr>, Option<String>)> {
        let vec3 = |v: Vec<V3>| Attr::new(Column::Vec3(Arc::new(v)), Role::Vector);
        if let Some((counts, periodic)) = self.curves() {
            if ctx != Context::Point {
                return None;
            }
            return Some((vec3(curve_tangents(self.points(), counts, periodic)), None, None));
        }
        let m = self.polygons()?;
        if !matches!(ctx, Context::Point | Context::Corner) {
            return None;
        }
        // Square to the normals the mesh has (per corner), else to its own.
        let normals: Vec<V3> = match self.find(crate::NORMALS) {
            Some((c, a)) if a.column.vec3s().is_some() => {
                let v = a.column.vec3s().unwrap_or(&[]);
                let mut faces = Vec::with_capacity(m.indices.len());
                for (f, &n) in m.counts.iter().enumerate() {
                    faces.extend(std::iter::repeat_n(f, n as usize));
                }
                (0..m.indices.len())
                    .map(|k| {
                        let element = match c {
                            Context::Object => 0,
                            Context::Point => m.indices[k] as usize,
                            Context::Face => faces[k],
                            Context::Corner => k,
                        };
                        a.value_index(element).and_then(|i| v.get(i).copied()).unwrap_or(UP)
                    })
                    .collect()
            }
            _ => m.corner_normals(w, 180.0),
        };
        let name = (from == TangentsFrom::Uv).then(|| self.uv_name(uv)).flatten();
        let (tangents, signs) = match name.as_deref().and_then(|n| self.per_corner_uv(n)) {
            Some(uv) => m.uv_tangents(&uv, &normals),
            None => (m.edge_tangents(&normals), vec![1.0; m.indices.len()]),
        };
        let used = name.filter(|_| from == TangentsFrom::Uv);
        match ctx {
            Context::Corner => Some((
                vec3(tangents),
                Some(Attr::new(Column::Float(Arc::new(signs)), Role::None)),
                used,
            )),
            _ => {
                let t = m.to_points(&tangents, ALONG_X);
                // A point's sign: its first corner's.
                let mut s = vec![1.0; m.points.len()];
                for (c, &p) in m.indices.iter().enumerate().rev() {
                    if let Some(x) = s.get_mut(p as usize) {
                        *x = signs[c];
                    }
                }
                Some((vec3(t), Some(Attr::new(Column::Float(Arc::new(s)), Role::None)), used))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: V3, b: V3) -> bool {
        length(sub(a, b)) < 1e-4
    }

    /// A unit cube's six quads, outward.
    fn cube() -> (Vec<V3>, Vec<u32>, Vec<u32>) {
        let p = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
        let idx = vec![
            0, 3, 2, 1, // -z
            4, 5, 6, 7, // +z
            0, 1, 5, 4, // -y
            3, 7, 6, 2, // +y
            0, 4, 7, 3, // -x
            1, 2, 6, 5, // +x
        ];
        (p, vec![4; 6], idx)
    }

    #[test]
    fn a_cube_s_faces_point_out_and_its_corners_point_diagonally() {
        let (p, counts, idx) = cube();
        let m = Polygons {
            points: &p,
            counts: &counts,
            indices: &idx,
            left_handed: false,
        };
        let f = m.face_normals();
        assert!(close(f[0], [0.0, 0.0, -1.0]) && close(f[1], [0.0, 0.0, 1.0]) && close(f[5], [1.0, 0.0, 0.0]));
        let d = 1.0 / 3f32.sqrt();
        for w in Weighting::ALL {
            assert!(close(m.point_normals(w)[6], [d, d, d]), "{w:?}");
        }
        // Hard at 60°: each corner its face's; soft at 180°: the point's.
        let hard = m.corner_normals(Weighting::Area, 60.0);
        assert!(close(hard[4], [0.0, 0.0, 1.0]));
        let soft = m.corner_normals(Weighting::Area, 180.0);
        assert!(close(soft[6], [d, d, d]));
        // Wound the other way, they point in.
        let flipped = Polygons { left_handed: true, ..m };
        assert!(close(flipped.face_normals()[1], [0.0, 0.0, -1.0]));
    }

    #[test]
    fn angle_weighting_is_even_across_a_fan_of_thin_triangles() {
        // A point on a ridge: one big face on one side, a fan of four on the
        // other. By area the big face wins; by angle the sides are even.
        let p = vec![
            [0.0, 0.0, 0.0],  // the point
            [-4.0, 0.0, 4.0], // big face, tilted up to +x side normal
            [-4.0, 0.0, -4.0],
            [1.0, -1.0, 0.0], // fan
            [1.0, -1.0, 1.0],
            [1.0, -1.0, 2.0],
        ];
        let counts = vec![3, 3, 3];
        let idx = vec![0, 2, 1, 0, 3, 4, 0, 4, 5];
        let m = Polygons {
            points: &p,
            counts: &counts,
            indices: &idx,
            left_handed: false,
        };
        let area = m.point_normals(Weighting::Area)[0];
        let angle = m.point_normals(Weighting::Angle)[0];
        assert!(!close(area, angle));
    }

    #[test]
    fn uv_tangents_follow_u_and_flip_where_mirrored() {
        // A quad in XZ facing up, U along x, V along -z.
        let p = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 0.0, 0.0]];
        let counts = vec![4];
        let idx = vec![0, 1, 2, 3];
        let m = Polygons {
            points: &p,
            counts: &counts,
            indices: &idx,
            left_handed: false,
        };
        let n = m.corner_normals(Weighting::Area, 180.0);
        assert!(close(n[0], [0.0, 1.0, 0.0]), "{:?}", n[0]);
        let uv = vec![[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
        let (t, s) = m.uv_tangents(&uv, &n);
        assert!(t.iter().all(|t| close(*t, [1.0, 0.0, 0.0])), "{t:?}");
        assert!(s.iter().all(|&s| s == 1.0), "{s:?}");
        // U mirrored: the tangent turns round, the sign stays (V the same way).
        let mirrored: Vec<[f32; 2]> = uv.iter().map(|u| [1.0 - u[0], u[1]]).collect();
        let (t, s) = m.uv_tangents(&mirrored, &n);
        assert!(close(t[0], [-1.0, 0.0, 0.0]) && s[0] == -1.0, "{t:?} {s:?}");
        // Along the edges, with no coordinates.
        let e = m.edge_tangents(&n);
        assert!(close(e[0], [0.0, 0.0, 1.0]) && close(e[1], [1.0, 0.0, 0.0]));
        let pts = m.to_points(&e, ALONG_X);
        assert_eq!(pts.len(), 4);
    }

    #[test]
    fn curves_tangents_and_normals_follow_them() {
        // An L: along x, then along y; and a periodic square.
        let p = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
            [2.0, 2.0, 0.0],
        ];
        let t = curve_tangents(&p, &[5], false);
        assert!(close(t[0], [1.0, 0.0, 0.0]) && close(t[4], [0.0, 1.0, 0.0]));
        let d = 1.0 / 2f32.sqrt();
        assert!(close(t[2], [d, d, 0.0]), "the corner: half way {:?}", t[2]);
        let n = curve_normals(&p, &[5], false, [0.0, 0.0, 1.0]);
        for (k, v) in n.iter().enumerate() {
            assert!(dot(*v, t[k]).abs() < 1e-4, "square to the curve at {k}");
            assert!(close(*v, [0.0, 0.0, 1.0]), "a flat curve keeps its up: {v:?}");
        }
        let sq = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let t = curve_tangents(&sq, &[4], true);
        assert!(close(t[0], [d, -d, 0.0]), "round from the last: {:?}", t[0]);
    }

    #[test]
    fn a_geometry_s_normals_and_tangents_by_context() {
        let (p, counts, idx) = cube();
        let mut g = Geo::from_polygons(p, counts, idx);
        let corner = g.made_normals(Context::Corner, Weighting::Area, 60.0, UP).unwrap();
        assert_eq!(corner.len(), 24);
        assert_eq!(corner.role, Role::Normal);
        assert_eq!(
            g.made_normals(Context::Face, Weighting::Area, 60.0, UP).unwrap().len(),
            6
        );
        assert!(g.made_normals(Context::Object, Weighting::Area, 60.0, UP).is_none());
        // No coordinates: along the edges, every sign 1.
        let (t, s, used) = g
            .made_tangents(Context::Corner, TangentsFrom::Uv, "", Weighting::Area)
            .unwrap();
        assert_eq!((t.len(), used), (24, None));
        assert!(s.unwrap().column.floats().unwrap().iter().all(|&x| x == 1.0));
        // With coordinates per corner, found by their name.
        let uv: Vec<[f32; 2]> = (0..24)
            .map(|c| [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]][c % 4])
            .collect();
        g.set(
            Context::Corner,
            "st",
            Attr::new(Column::Vec2(Arc::new(uv)), Role::TexCoord),
        );
        let (t, _, used) = g
            .made_tangents(Context::Point, TangentsFrom::Uv, "", Weighting::Area)
            .unwrap();
        assert_eq!((t.len(), used.as_deref()), (8, Some("st")));
        for v in t.column.vec3s().unwrap() {
            assert!((length(*v) - 1.0).abs() < 1e-4);
        }
        // A curve: along it, per point.
        let mut c = Geo::from_points(vec![[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]]);
        c.topology = Topology::curves(vec![3], crate::CurveBasis::default(), CurveWrap::Nonperiodic);
        let (t, s, _) = c
            .made_tangents(Context::Point, TangentsFrom::Uv, "", Weighting::Area)
            .unwrap();
        assert!(s.is_none() && close(t.column.vec3s().unwrap()[1], [1.0, 0.0, 0.0]));
        let n = c.made_normals(Context::Point, Weighting::Area, 60.0, UP).unwrap();
        assert!(close(n.column.vec3s().unwrap()[0], UP));
        assert!(Geo::from_points(vec![[0.0; 3]])
            .made_normals(Context::Point, Weighting::Area, 60.0, UP)
            .is_none());
    }
}
