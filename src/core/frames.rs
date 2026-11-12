//! Normals and Tangents: what the primitives' geometry is made to face, made
//! from it (`xms_geo::frames`). On the primitives the Prims Location tab names
//! (picked), or every one.
//!
//! Normals: a mesh's per Point (smooth, the faces around each point weighed),
//! Corner (hard where faces meet at more than the cusp angle) or Face (flat);
//! a curve's per point, square to it (what ribbons face). Written as
//! `normals`, the one there was in another context taken away (USD has one).
//!
//! Tangents: a mesh's along U of its texture coordinates (else its edges), per
//! Point or Corner, square to its normals; a curve's along it, per point.
//! Written as `tangents`; with Bitangents, `bitangents` too (`normal ×
//! tangent`, turned where the coordinates are mirrored).

use std::sync::Arc;

use crate::core::geo::frames::{TangentsFrom, Weighting, UP};
use crate::core::geo::{Attr, Column, Context, Geo, Role, NORMALS};
use crate::types::EvalResult;

pub const TANGENTS: &str = "tangents";
pub const BITANGENTS: &str = "bitangents";

/// `attr` as `name` in `ctx`, and in no other context.
fn only_in(g: &mut Geo, ctx: Context, name: &str, attr: Attr) {
    for c in Context::ALL {
        g.remove(c, name);
    }
    g.set(ctx, name, attr);
}

/// The geometry with normals made per `ctx`; `None` where it has none to make
/// (points).
pub fn with_normals(geo: &Geo, ctx: Context, w: Weighting, cusp: f32) -> Option<Geo> {
    let n = geo.made_normals(ctx, w, cusp, UP)?;
    let mut g = geo.clone();
    only_in(&mut g, ctx, NORMALS, n);
    Some(g)
}

/// The geometry with tangents made per `ctx`, and bitangents when asked.
pub fn with_tangents(geo: &Geo, ctx: Context, from: TangentsFrom, uv: &str, bitangents: bool) -> Option<Geo> {
    let (t, signs, _) = geo.made_tangents(ctx, from, uv, Weighting::Area)?;
    let mut g = geo.clone();
    if bitangents {
        // Square to the tangent and the normal, the normal the mesh's own in
        // this context (or made here).
        let normals = geo
            .attr(ctx, NORMALS)
            .filter(|a| a.indices.is_none())
            .and_then(|a| a.column.vec3s().map(|v| v.to_vec()))
            .filter(|v| v.len() == t.len())
            .or_else(|| {
                geo.made_normals(ctx, Weighting::Area, 180.0, UP)
                    .and_then(|a| a.column.vec3s().map(|v| v.to_vec()))
            });
        if let (Some(n), Some(tv)) = (normals, t.column.vec3s()) {
            let s = signs.as_ref().and_then(|s| s.column.floats());
            let b: Vec<[f32; 3]> = tv
                .iter()
                .zip(&n)
                .enumerate()
                .map(|(k, (t, n))| {
                    let b = glam::Vec3::from(*n).cross(glam::Vec3::from(*t));
                    (b * s.and_then(|s| s.get(k)).copied().unwrap_or(1.0)).to_array()
                })
                .collect();
            only_in(
                &mut g,
                ctx,
                BITANGENTS,
                Attr::new(Column::Vec3(Arc::new(b)), Role::Vector),
            );
        }
    } else {
        for c in Context::ALL {
            g.remove(c, BITANGENTS);
        }
    }
    only_in(&mut g, ctx, TANGENTS, t);
    Some(g)
}

/// Normals.
pub fn normals(input: &EvalResult, ctx: Context, w: Weighting, cusp: f32) -> EvalResult {
    crate::core::attribute_edit::on_prims(input, &|g| with_normals(g, ctx, w, cusp))
}

/// Tangents.
pub fn tangents(input: &EvalResult, ctx: Context, from: TangentsFrom, uv: &str, bitangents: bool) -> EvalResult {
    crate::core::attribute_edit::on_prims(input, &|g| with_tangents(g, ctx, from, uv, bitangents))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geo::{CurveBasis, CurveWrap, Topology};

    fn quad() -> Geo {
        Geo::from_polygons(
            vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 0.0, 0.0]],
            vec![4],
            vec![0, 1, 2, 3],
        )
    }

    #[test]
    fn normals_go_in_one_context() {
        let g = with_normals(&quad(), Context::Point, Weighting::Area, 60.0).unwrap();
        assert_eq!(
            g.attr(Context::Point, NORMALS).unwrap().column.vec3s().unwrap()[0],
            [0.0, 1.0, 0.0]
        );
        let g = with_normals(&g, Context::Corner, Weighting::Area, 60.0).unwrap();
        assert!(g.attr(Context::Point, NORMALS).is_none(), "one normals");
        assert_eq!(g.attr(Context::Corner, NORMALS).unwrap().len(), 4);
        assert!(with_normals(&Geo::from_points(vec![[0.0; 3]]), Context::Point, Weighting::Area, 60.0).is_none());
    }

    #[test]
    fn tangents_and_bitangents_on_meshes_and_curves() {
        let g = with_tangents(&quad(), Context::Corner, TangentsFrom::Edges, "", true).unwrap();
        let t = g.attr(Context::Corner, TANGENTS).unwrap().column.vec3s().unwrap()[0];
        let b = g.attr(Context::Corner, BITANGENTS).unwrap().column.vec3s().unwrap()[0];
        // Along the first edge (+z), the bitangent up × z = +x.
        assert_eq!(t, [0.0, 0.0, 1.0]);
        assert!((b[0] - 1.0).abs() < 1e-5, "{b:?}");
        let g = with_tangents(&g, Context::Point, TangentsFrom::Edges, "", false).unwrap();
        assert!(g.attr(Context::Corner, TANGENTS).is_none() && g.find(BITANGENTS).is_none());
        let mut c = Geo::from_points(vec![[0.0; 3], [0.0, 1.0, 0.0], [0.0, 2.0, 0.0]]);
        c.topology = Topology::curves(vec![3], CurveBasis::default(), CurveWrap::Nonperiodic);
        let c = with_tangents(&c, Context::Point, TangentsFrom::Uv, "", false).unwrap();
        assert_eq!(
            c.attr(Context::Point, TANGENTS).unwrap().column.vec3s().unwrap()[1],
            [0.0, 1.0, 0.0]
        );
    }
}
