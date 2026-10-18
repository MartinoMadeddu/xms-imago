//! `Geo` to and from the app's `MeshData`, which the Viewer, Edit Poly and
//! the UV operators still work on. A trait, because `Geo` lives in its own
//! crate (`xms-geo`) and `MeshData` here.

use std::sync::Arc;

use crate::core::geo::{Attr, Column, Context, Geo, Kind, Role, Topology, NORMALS, POINTS, ST, UV_NAMES, WIDTHS};
use crate::types::{MeshData, PrimVar, PrimVarInterp};

pub trait GeoMesh {
    /// The `MeshData` view of this geometry, for code that still works on
    /// `MeshData`: polygons (wound counter-clockwise) and their fan
    /// triangulation, normals per point, UVs per triangle corner, points and
    /// their widths, curves, and every other attribute with one to four
    /// float components as a `PrimVar`.
    fn to_mesh(&self) -> MeshData;
    /// The same view without the other attributes as `PrimVar`s: what the
    /// Viewer and the mesh tools need, made fast for a stage of thousands of
    /// primitives. The geometry keeps them; `primvars_view` makes them when
    /// asked (the Primitive Inspector, on a geometry tab).
    fn to_mesh_lean(&self) -> MeshData;
    /// Every attribute but the points, normals, widths and UVs (which the
    /// view holds itself) with one to four float components, as `PrimVar`s.
    fn primvars_view(&self) -> Vec<PrimVar>;
    /// This geometry with the changes a `MeshData` operation made to its
    /// view (`before` what it was given, `after` what it returned), only what
    /// changed replaced.
    fn with_mesh_edits(&self, before: &MeshData, after: &MeshData) -> Geo;
    /// A `Geo` from `MeshData`.
    fn from_mesh(m: &MeshData) -> Geo;
}

impl GeoMesh for Geo {

    /// The `MeshData` view of this geometry, for code that still works on
    /// `MeshData`: polygons (wound counter-clockwise) and their fan
    /// triangulation, normals per point, UVs per triangle corner, points and
    /// their widths, curves, and every other attribute with one to four
    /// float components as a `PrimVar`.
    fn to_mesh(&self) -> MeshData {
        let mut m = self.to_mesh_lean();
        m.primvars = self.primvars_view();
        m
    }

    fn primvars_view(&self) -> Vec<PrimVar> {
        let mut out = vec![];
        for ctx in Context::ALL {
            for (name, attr) in self.attrs(ctx) {
                if matches!(name, POINTS | NORMALS | WIDTHS) || UV_NAMES.contains(&name) { continue; }
                if let Some(values) = float_rows(attr, self.count(ctx)) {
                    out.push(PrimVar { name: name.to_string(), interp: interp_of(ctx), values });
                }
            }
        }
        out
    }

    fn to_mesh_lean(&self) -> MeshData {
        let mut m = MeshData::default();
        match &self.topology {
            Topology::Points => {
                m.points = self.points().to_vec();
                if let Some(w) = self.attr(Context::Point, WIDTHS).and_then(|a| expand_floats(a, self.count(Context::Point))) {
                    m.widths = w;
                } else if let Some(w) = self.attr(Context::Object, WIDTHS).and_then(|a| a.column.floats()) {
                    m.widths = w.to_vec();
                }
            }
            Topology::Curves { counts, basis, wrap } => {
                m.curve_points = self.points().to_vec();
                m.curve_counts = counts.to_vec();
                m.curve_basis = *basis;
                m.curve_wrap = *wrap;
            }
            Topology::Mesh { counts, indices, left_handed, .. } => {
                m.vertices = self.points().to_vec();
                // Corners of each polygon, in the winding drawn.
                let mut polys: Vec<Vec<u32>> = Vec::with_capacity(counts.len());
                let mut corner_of: Vec<Vec<usize>> = Vec::with_capacity(counts.len());
                let mut at = 0usize;
                for &n in counts.iter() {
                    let n = n as usize;
                    let corners: Vec<usize> = if *left_handed { (0..n).rev().map(|k| at + k).collect() } else { (at..at + n).collect() };
                    polys.push(corners.iter().map(|&c| indices[c]).collect());
                    corner_of.push(corners);
                    at += n;
                }
                // UVs per triangle corner, from wherever `st` lives (or the
                // first of its other names, or any texture coordinates).
                let is_uv = |a: &&Attr| a.column.kind() == Kind::Vec2;
                let st = UV_NAMES.iter().find_map(|n| self.find(n).filter(|(_, a)| is_uv(a)))
                    .or_else(|| Context::ALL.into_iter().find_map(|c| {
                        self.attrs(c).find(|(_, a)| a.role == Role::TexCoord && is_uv(a)).map(|(_, a)| (c, a))
                    }));
                let uv = |ctx: Context, a: &Attr, poly: usize, corner: usize| -> [f32; 2] {
                    let element = match ctx {
                        Context::Object => 0,
                        Context::Point => indices[corner] as usize,
                        Context::Face => poly,
                        Context::Corner => corner,
                    };
                    a.value_index(element).and_then(|i| a.column.vec2s().and_then(|v| v.get(i).copied())).unwrap_or([0.0; 2])
                };
                for (p, corners) in corner_of.iter().enumerate() {
                    for k in 1..corners.len().saturating_sub(1) {
                        for c in [corners[0], corners[k], corners[k + 1]] {
                            m.indices.push(indices[c]);
                            if let Some((ctx, a)) = st { m.uvs.push(uv(ctx, a, p, c)); }
                        }
                    }
                }
                m.face_count = polys.len();
                m.polys = polys;
                if let Some(n) = self.attr(Context::Point, NORMALS).and_then(|a| expand_vec3s(a, self.point_count())) {
                    m.normals = n;
                }
            }
        }
        m
    }

    /// This geometry with the changes a `MeshData` operation made to its
    /// view: `before` is the view the operation was given (`to_mesh`),
    /// `after` what it returned, both in this geometry's space. Only what
    /// changed is replaced, so everything else stays shared with `self`:
    ///
    /// - points moved: new points (normals are dropped, as they no longer
    ///   match; the viewport makes its own);
    /// - UVs changed: a new `st` per polygon corner;
    /// - polygons changed: new topology from `after`, keeping the attributes
    ///   whose context still has the same elements (object ones always, point
    ///   ones while the points are the same in number). Per-polygon and
    ///   per-corner ones cannot follow a topology change and are left out,
    ///   apart from `st`, which comes from `after`.
    fn with_mesh_edits(&self, before: &MeshData, after: &MeshData) -> Geo {
        let Topology::Mesh { left_handed, .. } = &self.topology else {
            // Curves and points: their positions are all a MeshData edit can change.
            let mut g = self.clone();
            let moved = if after.curve_counts.is_empty() { &after.points } else { &after.curve_points };
            if moved.len() == self.point_count() && moved.as_slice() != self.points() {
                *g.points_mut() = moved.clone();
                g.remove(Context::Point, NORMALS);
            }
            return g;
        };
        let left_handed = *left_handed;
        if before.polys != after.polys {
            let mut g = Geo::from_mesh(after);
            for (name, attr) in self.attrs(Context::Object) {
                g.set(Context::Object, name, attr.clone());
            }
            if after.vertices.len() == self.point_count() {
                for (name, attr) in self.attrs(Context::Point) {
                    if name != POINTS && name != NORMALS { g.set(Context::Point, name, attr.clone()); }
                }
            }
            return g;
        }
        let mut g = self.clone();
        if before.vertices != after.vertices {
            *g.points_mut() = after.vertices.clone();
            g.remove(Context::Point, NORMALS);
        }
        if before.uvs != after.uvs && after.uvs.len() == after.indices.len() {
            // The view's polygons are wound for drawing: a left-handed mesh's
            // run backwards, so its corner UVs are turned back round.
            let mut st = corner_uvs(&after.polys, &after.uvs);
            if left_handed {
                let mut at = 0usize;
                for p in &after.polys {
                    st[at..at + p.len()].reverse();
                    at += p.len();
                }
            }
            for name in UV_NAMES { for ctx in Context::ALL { g.remove(ctx, name); } }
            g.set(Context::Corner, ST, Attr::new(Column::Vec2(Arc::new(st)), Role::TexCoord));
        }
        g
    }

    /// A `Geo` from `MeshData`. Polygons are kept when the triangles are
    /// their fan (as `MeshData` builds them), so UVs map back to polygon
    /// corners; otherwise the triangles become the polygons.
    fn from_mesh(m: &MeshData) -> Geo {
        let mut g;
        if !m.curve_counts.is_empty() {
            g = Geo::from_points(m.curve_points.clone());
            g.topology = Topology::curves(m.curve_counts.clone(), m.curve_basis, m.curve_wrap);
        } else if m.vertices.is_empty() {
            g = Geo::from_points(m.points.clone());
            if !m.widths.is_empty() {
                let ctx = if m.widths.len() == m.points.len() { Context::Point } else { Context::Object };
                let w = if ctx == Context::Object { vec![m.widths[0]] } else { m.widths.clone() };
                g.set(ctx, WIDTHS, Attr::new(Column::Float(Arc::new(w)), Role::None));
            }
        } else {
            let fan_of_polys = !m.polys.is_empty() && is_fan(&m.polys, &m.indices);
            let polys: Vec<Vec<u32>> = if fan_of_polys { m.polys.clone() } else { m.indices.chunks_exact(3).map(|t| t.to_vec()).collect() };
            let counts: Vec<u32> = polys.iter().map(|p| p.len() as u32).collect();
            let indices: Vec<u32> = polys.iter().flatten().copied().collect();
            // UVs come per triangle corner: back to polygon corners through the fan.
            let st: Option<Vec<[f32; 2]>> = (m.uvs.len() == m.indices.len() && !m.uvs.is_empty()).then(|| corner_uvs(&polys, &m.uvs));
            g = Geo::from_polygons(m.vertices.clone(), counts, indices);
            if let Some(st) = st {
                g.set(Context::Corner, ST, Attr::new(Column::Vec2(Arc::new(st)), Role::TexCoord));
            }
            if m.normals.len() == m.vertices.len() && !m.normals.is_empty() {
                g.set(Context::Point, NORMALS, Attr::new(Column::Vec3(Arc::new(m.normals.clone())), Role::Normal));
            }
            if !m.points.is_empty() {
                // A mesh with scattered points too keeps them as an object attribute.
                g.set(Context::Object, "scatterPoints", Attr::new(Column::Vec3(Arc::new(m.points.clone())), Role::Point));
            }
        }
        for pv in &m.primvars {
            let ctx = context_of(&pv.interp);
            if let Some(column) = column_from_rows(&pv.values) {
                g.set(ctx, &pv.name, Attr::new(column, Role::None));
            }
        }
        g
    }
}

/// UVs per triangle corner (fan triangulation of `polys`, in order) as UVs
/// per polygon corner.
fn corner_uvs(polys: &[Vec<u32>], uvs: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut out = Vec::with_capacity(polys.iter().map(|p| p.len()).sum());
    let mut tri = 0usize;   // first triangle corner of the current polygon
    for p in polys {
        let n = p.len();
        for k in 0..n {
            let at = match k {
                0 => tri,
                k if k + 1 < n => tri + 3 * (k - 1) + 1,
                _ => tri + 3 * (n - 3) + 2,
            };
            out.push(uvs.get(at).copied().unwrap_or([0.0; 2]));
        }
        tri += 3 * n.saturating_sub(2);
    }
    out
}

/// Whether `indices` is the fan triangulation of `polys`, in order.
fn is_fan(polys: &[Vec<u32>], indices: &[u32]) -> bool {
    let mut at = 0usize;
    for p in polys {
        for k in 1..p.len().saturating_sub(1) {
            if indices.get(at..at + 3) != Some(&[p[0], p[k], p[k + 1]][..]) { return false; }
            at += 3;
        }
    }
    at == indices.len()
}

fn interp_of(ctx: Context) -> PrimVarInterp {
    match ctx {
        Context::Object => PrimVarInterp::Constant,
        Context::Point => PrimVarInterp::Vertex,
        Context::Face => PrimVarInterp::Uniform,
        Context::Corner => PrimVarInterp::FaceVarying,
    }
}

fn context_of(interp: &PrimVarInterp) -> Context {
    match interp {
        PrimVarInterp::Constant => Context::Object,
        PrimVarInterp::Vertex => Context::Point,
        PrimVarInterp::Uniform => Context::Face,
        PrimVarInterp::FaceVarying => Context::Corner,
    }
}

/// An attribute's values for every element, indices resolved.
fn expand<T: Copy>(a: &Attr, values: &[T], count: usize) -> Option<Vec<T>> {
    (0..count).map(|e| a.value_index(e).and_then(|i| values.get(i).copied())).collect()
}

fn expand_floats(a: &Attr, count: usize) -> Option<Vec<f32>> {
    expand(a, a.column.floats()?, count)
}

fn expand_vec3s(a: &Attr, count: usize) -> Option<Vec<[f32; 3]>> {
    expand(a, a.column.vec3s()?, count)
}

/// Float-component values per element, for `PrimVar`.
fn float_rows(a: &Attr, count: usize) -> Option<Vec<Vec<f32>>> {
    let rows: Vec<Vec<f32>> = match &a.column {
        Column::Float(v) => v.iter().map(|x| vec![*x]).collect(),
        Column::Int(v) => v.iter().map(|x| vec![*x as f32]).collect(),
        // Shown and written as 0 and 1.
        Column::Bool(v) => v.iter().map(|x| vec![if *x { 1.0 } else { 0.0 }]).collect(),
        Column::Vec2(v) => v.iter().map(|x| x.to_vec()).collect(),
        Column::Vec3(v) => v.iter().map(|x| x.to_vec()).collect(),
        Column::Vec4(v) | Column::Quat(v) => v.iter().map(|x| x.to_vec()).collect(),
        _ => return None,
    };
    expand(a, &(0..rows.len()).collect::<Vec<_>>(), count).map(|ix| ix.into_iter().map(|i| rows[i].clone()).collect())
}

/// A column from `PrimVar` rows, by their component count.
fn column_from_rows(rows: &[Vec<f32>]) -> Option<Column> {
    let width = rows.first().map_or(1, |r| r.len());
    if rows.iter().any(|r| r.len() != width) { return None; }
    let at = |r: &Vec<f32>, i: usize| r.get(i).copied().unwrap_or(0.0);
    Some(match width {
        1 => Column::Float(Arc::new(rows.iter().map(|r| at(r, 0)).collect())),
        2 => Column::Vec2(Arc::new(rows.iter().map(|r| [at(r, 0), at(r, 1)]).collect())),
        3 => Column::Vec3(Arc::new(rows.iter().map(|r| [at(r, 0), at(r, 1), at(r, 2)]).collect())),
        4 => Column::Vec4(Arc::new(rows.iter().map(|r| [at(r, 0), at(r, 1), at(r, 2), at(r, 3)]).collect())),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CurveBasis, CurveWrap};

    /// A unit quad split from a triangle: two polygons sharing an edge.
    fn quad_and_triangle() -> Geo {
        let points = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [2.0, 0.0, 0.0]];
        let mut g = Geo::from_polygons(points, vec![4, 3], vec![0, 1, 2, 3, 1, 4, 2]);
        let st = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.5, 0.0], [0.9, 0.0], [0.5, 0.5]];
        g.set(Context::Corner, ST, Attr::new(Column::Vec2(Arc::new(st)), Role::TexCoord));
        g
    }

    #[test]
    fn a_mesh_goes_to_mesh_data_and_back() {
        let g = quad_and_triangle();
        let m = g.to_mesh();
        assert_eq!(m.polys, vec![vec![0, 1, 2, 3], vec![1, 4, 2]]);
        assert_eq!(m.indices, vec![0, 1, 2, 0, 2, 3, 1, 4, 2]);
        assert_eq!(m.uvs.len(), m.indices.len());
        assert_eq!(m.uvs[4], [1.0, 1.0]);   // second triangle, second corner: polygon corner 2

        let back = Geo::from_mesh(&m);
        assert!(back.validate().is_ok());
        assert_eq!(back.points(), g.points());
        assert_eq!(back.topology.corner_count(), 7);
        assert_eq!(back.attr(Context::Corner, ST).unwrap().column.vec2s(), g.attr(Context::Corner, ST).unwrap().column.vec2s());
    }

    #[test]
    fn left_handed_meshes_turn_their_polygons_round_in_the_view() {
        let mut g = quad_and_triangle();
        if let Topology::Mesh { left_handed, .. } = &mut g.topology { *left_handed = true; }
        assert_eq!(g.to_mesh().polys[1], vec![2, 4, 1]);
    }

    #[test]
    fn points_and_curves_keep_their_kind() {
        let mut cloud = Geo::from_points(vec![[0.0; 3], [1.0, 0.0, 0.0]]);
        cloud.set(Context::Point, WIDTHS, Attr::new(Column::Float(Arc::new(vec![0.5, 0.25])), Role::None));
        let m = cloud.to_mesh();
        assert_eq!((m.points.len(), m.widths.clone()), (2, vec![0.5, 0.25]));
        assert_eq!(Geo::from_mesh(&m).attr(Context::Point, WIDTHS).unwrap().column.floats(), Some(&[0.5, 0.25][..]));

        let mut hair = Geo::from_points(vec![[0.0; 3], [0.0, 1.0, 0.0], [0.0, 2.0, 0.0], [0.0, 3.0, 0.0]]);
        hair.topology = Topology::curves(vec![4], CurveBasis::CatmullRom, CurveWrap::Nonperiodic);
        let m = hair.to_mesh();
        assert_eq!((m.curve_counts.clone(), m.curve_basis), (vec![4], CurveBasis::CatmullRom));
        assert!(matches!(Geo::from_mesh(&m).topology, Topology::Curves { basis: CurveBasis::CatmullRom, .. }));
    }

    #[test]
    fn an_edit_replaces_only_what_it_changed() {
        let mut g = quad_and_triangle();
        g.set(Context::Point, NORMALS, Attr::new(Column::Vec3(Arc::new(vec![[0.0, 0.0, 1.0]; 5])), Role::Normal));
        g.set(Context::Object, "displayColor", Attr::new(Column::Vec3(Arc::new(vec![[1.0, 0.0, 0.0]])), Role::Color));
        let before = g.to_mesh();

        // UVs only: points, normals and topology stay shared.
        let mut after = before.clone();
        for uv in after.uvs.iter_mut() { uv[0] += 1.0; }
        let e = g.with_mesh_edits(&before, &after);
        assert!(e.attr(Context::Point, POINTS).unwrap().same(g.attr(Context::Point, POINTS).unwrap()));
        assert!(e.attr(Context::Point, NORMALS).is_some() && e.topology.same(&g.topology));
        assert!(!e.attr(Context::Corner, ST).unwrap().same(g.attr(Context::Corner, ST).unwrap()));
        assert_eq!(e.attr(Context::Corner, ST).unwrap().column.vec2s().unwrap()[2], [2.0, 1.0]);
        assert!(e.validate().is_ok());

        // Points moved: new points, stale normals gone, UVs still shared.
        let mut after = before.clone();
        after.vertices[4][1] = 3.0;
        let e = g.with_mesh_edits(&before, &after);
        assert_eq!(e.points()[4], [2.0, 3.0, 0.0]);
        assert!(e.attr(Context::Point, NORMALS).is_none());
        assert!(e.attr(Context::Corner, ST).unwrap().same(g.attr(Context::Corner, ST).unwrap()));

        // Polygons changed: new topology, object attributes kept.
        let after = MeshData::from_polys(before.vertices.clone(), vec![vec![0, 1, 2, 3]]);
        let e = g.with_mesh_edits(&before, &after);
        assert_eq!(e.topology.primitive_count(), 1);
        assert!(e.attr(Context::Object, "displayColor").is_some());
        assert!(e.validate().is_ok());
    }

    #[test]
    fn a_left_handed_mesh_keeps_its_uv_corners_in_its_own_order() {
        let mut g = quad_and_triangle();
        if let Topology::Mesh { left_handed, .. } = &mut g.topology { *left_handed = true; }
        let before = g.to_mesh();
        let mut after = before.clone();
        for uv in after.uvs.iter_mut() { uv[1] += 0.5; }
        let e = g.with_mesh_edits(&before, &after);
        let (old, new) = (g.attr(Context::Corner, ST).unwrap().column.vec2s().unwrap(), e.attr(Context::Corner, ST).unwrap().column.vec2s().unwrap());
        for (o, n) in old.iter().zip(new) { assert_eq!([o[0], o[1] + 0.5], *n); }
    }

    #[test]
    fn other_attributes_travel_as_primvars() {
        let mut g = quad_and_triangle();
        g.set(Context::Face, "heat", Attr::new(Column::Float(Arc::new(vec![0.25, 0.75])), Role::None));
        let m = g.to_mesh();
        let pv = m.primvars.iter().find(|p| p.name == "heat").unwrap();
        assert_eq!((pv.interp.clone(), pv.values.clone()), (PrimVarInterp::Uniform, vec![vec![0.25], vec![0.75]]));
        let back = Geo::from_mesh(&m);
        assert_eq!(back.attr(Context::Face, "heat").unwrap().column.floats(), Some(&[0.25, 0.75][..]));
    }
}
