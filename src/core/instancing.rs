//! Point instancers: a points primitive, one point per instance, whose
//! prototypes' meshes are drawn at its points.
//!
//! The instancer's points are the source: `points` (USD `positions`),
//! `orient`, `scale`, `protoIndex` and `id` per point. Each prototype mesh
//! carries its placements (`Placement::Many`) for drawing, made from those
//! points. When an operator changes the instancer (its points, its
//! attributes or where it is placed), `sync` makes its prototypes'
//! placements again, so the copies follow. Nothing is made again while the
//! instancer is unchanged: each prototype keeps the points and placement its
//! placements were made from, and compares them by pointer.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};

use crate::core::geo::{Context, Geo};
use crate::types::{EvalResult, InstancedBy, Placement, Prim};

/// The scene with each prototype's placements made again where its
/// instancer changed since they were made.
pub fn sync(r: EvalResult) -> EvalResult {
    let EvalResult::Named(mut prims) = r else { return r };
    if !prims.iter().any(|p| p.instanced_by.is_some()) { return EvalResult::Named(prims); }
    let instancers: HashMap<String, (Arc<Geo>, Placement)> = prims.iter()
        .filter(|p| p.instancing.is_some())
        .filter_map(|p| p.geo.clone().map(|g| (p.path.clone(), (g, p.place.clone()))))
        .collect();
    for p in prims.iter_mut() {
        let Some(by) = p.instanced_by.clone() else { continue };
        let Some((geo, place)) = instancers.get(&by.instancer) else { continue };
        if Arc::ptr_eq(&by.geo, geo) && by.place.same(place) { continue; }
        p.place = Placement::Many(copies(geo, place, &by).into());
        p.instanced_by = Some(Arc::new(InstancedBy { geo: geo.clone(), place: place.clone(), ..(*by).clone() }));
    }
    EvalResult::Named(prims)
}

/// Where a prototype mesh is drawn: at each of the instancer's points that
/// use its prototype and are not hidden, scaled, then turned, then moved,
/// then placed where the instancer is.
pub fn copies(geo: &Geo, place: &Placement, by: &InstancedBy) -> Vec<Mat4> {
    let at = match place { Placement::One(m) => *m, _ => Mat4::IDENTITY };
    let points = geo.points();
    let attr = |name: &str| geo.attr(Context::Point, name);
    let index = attr("protoIndex").and_then(|a| a.column.ints());
    let orient = attr("orient").and_then(|a| a.column.quats());
    let scale = attr("scale").and_then(|a| a.column.vec3s());
    let ids = attr("id").and_then(|a| a.column.ints());
    (0..points.len())
        .filter(|&k| index.map_or(by.index == 0, |ix| ix.get(k).copied() == Some(by.index as i32)))
        .filter(|&k| {
            let id = ids.and_then(|i| i.get(k).copied()).map_or(k as i64, |i| i as i64);
            !by.hidden.contains(&id)
        })
        .map(|k| {
            let s = scale.and_then(|s| s.get(k)).copied().unwrap_or([1.0; 3]);
            // Stored (i, j, k, real).
            let q = orient.and_then(|o| o.get(k)).map(|q| Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize()).unwrap_or(Quat::IDENTITY);
            at * Mat4::from_scale_rotation_translation(Vec3::from(s), q, Vec3::from(points[k])) * by.inside
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geo::{Attr, Column, Role};
    use crate::types::MeshData;

    /// An instancer at /inst of three points, the middle one hidden, and a
    /// prototype mesh drawn at the two points of prototype 0.
    fn scene() -> Vec<Prim> {
        let mut geo = Geo::from_points(vec![[0.0, 0.0, 0.0], [5.0, 0.0, 0.0], [9.0, 0.0, 0.0]]);
        geo.set(Context::Point, "protoIndex", Attr::new(Column::Int(Arc::new(vec![0, 0, 0])), Role::None));
        geo.set(Context::Point, "id", Attr::new(Column::Int(Arc::new(vec![10, 11, 12])), Role::None));
        let geo = Arc::new(geo);
        let place = Placement::One(Mat4::IDENTITY);
        let instancer = Prim { geo: Some(geo.clone()), place: place.clone(), instancing: Some(Arc::new(vec!["/inst/proto".into()])),
            ..Prim::new("/inst".into(), geo.to_mesh()) };
        let by = InstancedBy { instancer: "/inst".into(), index: 0, inside: Mat4::IDENTITY, hidden: Arc::new(vec![11]), geo: geo.clone(), place: place.clone() };
        let at: Vec<Mat4> = copies(&geo, &place, &by);
        let proto = Prim { place: Placement::Many(at.into()), instanced_by: Some(Arc::new(by)), ..Prim::new("/inst/proto/box".into(), MeshData::default()) };
        vec![instancer, proto]
    }

    fn xs(p: &Prim) -> Vec<f32> { p.place.matrices().iter().map(|m| m.w_axis.x).collect() }

    #[test]
    fn copies_are_made_from_the_points() {
        let prims = scene();
        assert_eq!(xs(&prims[1]), vec![0.0, 9.0], "the hidden id 11 is not drawn");
        // Unchanged: nothing is made again, the placements are the very same.
        let EvalResult::Named(same) = sync(EvalResult::Named(prims.clone())) else { panic!() };
        assert!(same[1].place.same(&prims[1].place));
    }

    #[test]
    fn moving_the_points_moves_the_copies() {
        let mut prims = scene();
        let mut g = (**prims[0].geo.as_ref().unwrap()).clone();
        g.points_mut()[2] = [20.0, 0.0, 0.0];
        prims[0].geo = Some(Arc::new(g));
        let EvalResult::Named(out) = sync(EvalResult::Named(prims)) else { panic!() };
        assert_eq!(xs(&out[1]), vec![0.0, 20.0]);
        // Placing the instancer elsewhere moves them all.
        let mut prims = out;
        prims[0].place = prims[0].place.moved(Mat4::from_translation(Vec3::new(0.0, 0.0, 3.0)));
        let EvalResult::Named(out) = sync(EvalResult::Named(prims)) else { panic!() };
        assert!(out[1].place.matrices().iter().all(|m| m.w_axis.z == 3.0));
    }
}
