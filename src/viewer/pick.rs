//! Clicking a primitive in the Viewer selects it, as clicking an object in
//! any 3D program does. The selection is the Scene Explorer's: the same
//! rows light up there, the Primitive Inspector shows the prim, and Ctrl
//! adds or takes away as a click on a row does. The selected primitives are
//! outlined by their bounds, so what is chosen can be seen.
//!
//! The pick is a ray through the pixel against the primitives at the view
//! flag: their bounds first, then the triangles of those the ray enters,
//! nearest hit wins. A primitive under a closed row of the Scene Explorer
//! is still picked by its own geometry.

use glam::{Mat4, Vec2, Vec3};

use crate::core::poly::{ray_triangle, PickView};
use crate::types::Prim;
use crate::viewport::bounds::{self, Bounds};

/// Where a ray enters a box, if it does.
fn ray_box(o: Vec3, d: Vec3, b: Bounds) -> Option<f32> {
    let (mut t0, mut t1) = (0.0f32, f32::MAX);
    for k in 0..3 {
        if d[k].abs() < 1e-9 {
            if o[k] < b.0[k] || o[k] > b.1[k] { return None; }
            continue;
        }
        let (a, c) = ((b.0[k] - o[k]) / d[k], (b.1[k] - o[k]) / d[k]);
        t0 = t0.max(a.min(c));
        t1 = t1.min(a.max(c));
        if t0 > t1 { return None; }
    }
    Some(t0)
}

/// Where a ray first hits a mesh placed by `m`: in the mesh's own space,
/// so the ray is moved, not the triangles.
fn ray_mesh(o: Vec3, d: Vec3, mesh: &crate::types::MeshData, m: &Mat4) -> Option<f32> {
    let inv = m.inverse();
    let (lo, ld) = (inv.transform_point3(o), inv.transform_vector3(d));
    let mut best: Option<f32> = None;
    let v = |i: u32| Vec3::from_array(mesh.vertices[i as usize]);
    for tri in mesh.indices.chunks_exact(3) {
        if let Some(t) = ray_triangle(lo, ld, v(tri[0]), v(tri[1]), v(tri[2])) {
            if best.is_none_or(|b| t < b) { best = Some(t); }
        }
    }
    // Back to world distance along the ray.
    best.map(|t| (m.transform_point3(lo + ld * t) - o).dot(d.normalize_or_zero()))
}

/// The path of the primitive under `pixel`, among `prims`.
pub fn prim_at(view: &PickView, pixel: Vec2, prims: &[Prim]) -> Option<String> {
    let (o, d) = view.ray(pixel);
    let d = d.normalize_or_zero();
    let mut best: Option<(f32, &str)> = None;
    for p in prims {
        if p.mesh.indices.is_empty() { continue; }
        let Some(local) = bounds::local_bounds(&p.mesh) else { continue };
        let matrices: &[Mat4] = if p.place.is_placed() { p.place.matrices() } else { &[Mat4::IDENTITY] };
        for m in matrices {
            // Cheap first: the copy's box.
            if ray_box(o, d, bounds::moved_bounds(local, m)).is_none() { continue; }
            if let Some(t) = ray_mesh(o, d, &p.mesh, m) {
                if t > 0.0 && best.is_none_or(|b| t < b.0) { best = Some((t, p.path.as_str())); }
            }
        }
    }
    best.map(|(_, path)| path.to_string())
}

/// The world bounds of the primitives whose paths are selected, to outline.
pub fn selected_bounds(prims: &[Prim], selected: &[String]) -> Vec<Bounds> {
    prims.iter().filter(|p| selected.iter().any(|s| *s == p.path)).filter_map(bounds::world_bounds).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MeshData, Placement};

    fn view() -> PickView {
        let size = Vec2::new(800.0, 600.0);
        let eye = Vec3::new(0.0, 0.0, 6.0);
        let proj = Mat4::perspective_infinite_reverse_rh(0.8, size.x / size.y, 0.1);
        PickView { view_proj: proj * Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y), size, eye }
    }

    /// Two cubes side by side: the pixel over each one picks it, the pixel
    /// between them nothing, a copy placed by a matrix is picked where the
    /// copy is, and the nearer of two in line wins.
    #[test]
    fn the_primitive_under_the_pixel_is_picked() {
        let cube = std::sync::Arc::new(crate::node_graph::nodes::create_cube(1.0));
        let left = Prim { place: Placement::One(Mat4::from_translation(Vec3::new(-2.0, 0.0, 0.0))), ..Prim::new("/left".into(), MeshData::default()) };
        let left = Prim { mesh: cube.clone(), ..left };
        let right = Prim { mesh: cube.clone(), place: Placement::Many(vec![Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0)), Mat4::from_translation(Vec3::new(2.0, 3.0, 0.0))].into()), ..Prim::new("/right".into(), MeshData::default()) };
        let v = view();
        let shown = [left, right];
        let at = |p: Vec3| v.project(p).unwrap();
        assert_eq!(prim_at(&v, at(Vec3::new(-2.0, 0.0, 0.5)), &shown).as_deref(), Some("/left"));
        assert_eq!(prim_at(&v, at(Vec3::new(2.0, 0.0, 0.5)), &shown).as_deref(), Some("/right"));
        assert_eq!(prim_at(&v, at(Vec3::new(2.0, 3.0, 0.5)), &shown).as_deref(), Some("/right"));
        assert_eq!(prim_at(&v, at(Vec3::new(0.0, 0.0, 0.0)), &shown), None);
        // The nearer of two in line wins: a cube in front of the left one.
        let front = Prim { mesh: cube.clone(), place: Placement::One(Mat4::from_translation(Vec3::new(-2.0, 0.0, 2.0))), ..Prim::new("/front".into(), MeshData::default()) };
        let three = [shown[0].clone(), shown[1].clone(), front];
        assert_eq!(prim_at(&v, at(Vec3::new(-2.0, 0.0, 0.5)), &three).as_deref(), Some("/front"));
        let b = selected_bounds(&three, &["/right".into(), "/front".into()]);
        assert_eq!(b.len(), 2);
        assert!((b[0].1.y - 3.5).abs() < 1e-5, "{:?}", b[0]);
    }
}
