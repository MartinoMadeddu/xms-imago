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
use crate::types::{EvalResult, InstancedBy, Placement};

/// The scene with each prototype's placements made again where its
/// instancer changed since they were made. An instancer nested in another's
/// prototype is itself placed this way, so a change to the outer one goes
/// down to the inner one's prototypes: one pass per level.
pub fn sync(r: EvalResult) -> EvalResult {
    let EvalResult::Named(mut prims) = r else { return r };
    if !prims.iter().any(|p| p.instanced_by.is_some()) { return EvalResult::Named(prims); }
    for _ in 0..5 {
        let instancers: HashMap<String, (Arc<Geo>, Placement)> = prims.iter()
            .filter(|p| p.instancing.is_some())
            .filter_map(|p| p.geo.clone().map(|g| (p.path.clone(), (g, p.place.clone()))))
            .collect();
        let mut changed = false;
        for p in prims.iter_mut() {
            let Some(by) = p.instanced_by.clone() else { continue };
            let Some((geo, place)) = instancers.get(&by.instancer) else { continue };
            if Arc::ptr_eq(&by.geo, geo) && by.place.same(place) { continue; }
            p.place = Placement::Many(copies(geo, place, &by).into());
            p.instanced_by = Some(Arc::new(InstancedBy { geo: geo.clone(), place: place.clone(), ..(*by).clone() }));
            changed = true;
        }
        if !changed { break; }
    }
    EvalResult::Named(prims)
}

/// Where a prototype mesh is drawn: at each of the instancer's points that
/// use its prototype and are not hidden, scaled, then turned, then moved,
/// then placed where the instancer is: at each of its copies, for one
/// nested in another instancer's prototype.
pub fn copies(geo: &Geo, place: &Placement, by: &InstancedBy) -> Vec<Mat4> {
    let at: Vec<Mat4> = match place { Placement::InPlace => vec![Mat4::IDENTITY], placed => placed.matrices().to_vec() };
    let points = geo.points();
    let attr = |name: &str| geo.attr(Context::Point, name);
    let index = attr("protoIndex").and_then(|a| a.column.ints());
    let orient = attr("orient").and_then(|a| a.column.quats());
    let scale = attr("scale").and_then(|a| a.column.vec3s());
    let ids = attr("id").and_then(|a| a.column.ints());
    let local: Vec<Mat4> = (0..points.len())
        .filter(|&k| index.map_or(by.index == 0, |ix| ix.get(k).copied() == Some(by.index as i32)))
        .filter(|&k| {
            let id = ids.and_then(|i| i.get(k).copied()).map_or(k as i64, |i| i as i64);
            !by.hidden.contains(&id)
        })
        .map(|k| {
            let s = scale.and_then(|s| s.get(k)).copied().unwrap_or([1.0; 3]);
            // Stored (i, j, k, real).
            let q = orient.and_then(|o| o.get(k)).map(|q| Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize()).unwrap_or(Quat::IDENTITY);
            Mat4::from_scale_rotation_translation(Vec3::from(s), q, Vec3::from(points[k])) * by.inside
        })
        .collect();
    // Every copy of the instancer, then every point in it.
    at.iter().flat_map(|a| local.iter().map(move |p| *a * *p)).collect()
}

/// An instance's path: its instancer's, with its number (`/forest/trees[12]`).
/// What the Scene Explorer lists and the Viewer picks; not a prim.
pub fn instance_name(instancer: &str, i: usize) -> String { format!("{instancer}[{i}]") }

/// An instance's path taken apart: its instancer's path and its number.
pub fn instance_path(path: &str) -> Option<(&str, usize)> {
    let (inst, rest) = path.strip_suffix(']')?.rsplit_once('[')?;
    Some((inst, rest.parse().ok()?))
}

/// The instancer's points a prototype mesh is drawn at, in the order of its
/// copies (`copies`' `local`): those of its prototype, not hidden.
fn points_of(geo: &Geo, by: &InstancedBy) -> Vec<usize> {
    let index = geo.attr(Context::Point, "protoIndex").and_then(|a| a.column.ints());
    let ids = geo.attr(Context::Point, "id").and_then(|a| a.column.ints());
    (0..geo.points().len())
        .filter(|&k| index.map_or(by.index == 0, |ix| ix.get(k).copied() == Some(by.index as i32)))
        .filter(|&k| {
            let id = ids.and_then(|i| i.get(k).copied()).map_or(k as i64, |i| i as i64);
            !by.hidden.contains(&id)
        })
        .collect()
}

/// Which point of its instancer copy `copy` of a prototype mesh is drawn
/// at: the instance a click on that copy picks.
pub fn point_of_copy(by: &InstancedBy, copy: usize) -> Option<usize> {
    let points = points_of(&by.geo, by);
    if points.is_empty() { return None; }
    points.get(copy % points.len()).copied()
}

/// Where instance `i` of an instancer is: its point's placement (scaled,
/// turned, moved) at every copy of the instancer. A prototype mesh of it is
/// drawn there, after its own place within the prototype.
pub fn instance_matrices(geo: &Geo, place: &Placement, i: usize) -> Vec<Mat4> {
    let Some(p) = geo.points().get(i) else { return vec![] };
    let attr = |name: &str| geo.attr(Context::Point, name);
    let s = attr("scale").and_then(|a| a.column.vec3s()).and_then(|v| v.get(i)).copied().unwrap_or([1.0; 3]);
    let q = attr("orient").and_then(|a| a.column.quats()).and_then(|v| v.get(i))
        .map(|q| Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize()).unwrap_or(Quat::IDENTITY);
    let local = Mat4::from_scale_rotation_translation(Vec3::from(s), q, Vec3::from(*p));
    let at: Vec<Mat4> = match place { Placement::InPlace => vec![Mat4::IDENTITY], placed => placed.matrices().to_vec() };
    at.iter().map(|a| *a * local).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Prim;
    use crate::core::geo_mesh::GeoMesh;
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

    /// An instancer in a prototype of another: placed at its points, and
    /// its own prototype at its points in every one of its copies. Moving
    /// the outer one moves them all.
    #[test]
    fn a_nested_instancer_follows_the_one_it_is_in() {
        let mut prims = scene();
        // /inst/proto/inner: two points, drawn at both copies of prototype 0.
        let inner_geo = Arc::new(Geo::from_points(vec![[0.0, 1.0, 0.0], [0.0, 2.0, 0.0]]));
        let outer = &prims[0];
        let by = InstancedBy { instancer: "/inst".into(), index: 0, inside: Mat4::IDENTITY, hidden: Arc::new(vec![11]),
            geo: outer.geo.clone().unwrap(), place: outer.place.clone() };
        let inner_place = Placement::Many(copies(outer.geo.as_ref().unwrap(), &outer.place, &by).into());
        let inner = Prim { geo: Some(inner_geo.clone()), place: inner_place.clone(), instancing: Some(Arc::new(vec!["/inst/proto/inner/leaf".into()])),
            instanced_by: Some(Arc::new(by)), ..Prim::new("/inst/proto/inner".into(), inner_geo.to_mesh()) };
        let leaf_by = InstancedBy { instancer: "/inst/proto/inner".into(), index: 0, inside: Mat4::IDENTITY, hidden: Arc::new(vec![]),
            geo: inner_geo.clone(), place: inner_place.clone() };
        let leaf = Prim { place: Placement::Many(copies(&inner_geo, &inner_place, &leaf_by).into()), instanced_by: Some(Arc::new(leaf_by)),
            ..Prim::new("/inst/proto/inner/leaf/box".into(), MeshData::default()) };
        prims.extend([inner, leaf]);
        // Two copies of the inner instancer (x 0 and 9), two points each.
        let at = |p: &Prim| p.place.matrices().iter().map(|m| (m.w_axis.x, m.w_axis.y)).collect::<Vec<_>>();
        assert_eq!(at(&prims[3]), vec![(0.0, 1.0), (0.0, 2.0), (9.0, 1.0), (9.0, 2.0)]);
        // The outer instancer placed elsewhere: the leaf follows, two levels down.
        prims[0].place = prims[0].place.moved(Mat4::from_translation(Vec3::new(0.0, 0.0, 3.0)));
        let EvalResult::Named(out) = sync(EvalResult::Named(prims)) else { panic!() };
        assert_eq!(out[3].place.copies(), 4);
        assert!(out[3].place.matrices().iter().all(|m| m.w_axis.z == 3.0), "the leaf moved with the outer instancer");
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

    #[test]
    fn instances_are_named_and_found_by_their_copies() {
        assert_eq!(instance_name("/forest/trees", 12), "/forest/trees[12]");
        assert_eq!(instance_path("/forest/trees[12]"), Some(("/forest/trees", 12)));
        assert_eq!(instance_path("/forest/trees"), None);
        assert_eq!(instance_path("/a[b]"), None);
        let prims = scene();
        let by = prims[1].instanced_by.clone().unwrap();
        // Copies 0 and 1 are points 0 and 2: point 1 (id 11) is hidden.
        assert_eq!((point_of_copy(&by, 0), point_of_copy(&by, 1)), (Some(0), Some(2)));
        assert_eq!(instance_matrices(prims[0].geo.as_ref().unwrap(), &prims[0].place, 2)[0].w_axis.x, 9.0);
    }
}
