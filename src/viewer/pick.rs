//! Clicking a primitive in the Viewer selects it, as clicking an object in any
//! 3D program does. The selection is the Scene Explorer's: the same rows light
//! up there, the Primitive Inspector shows the prim, and Ctrl adds or takes
//! away as a click on a row does. The selected primitives are outlined by their
//! bounds, so what is chosen can be seen.
//!
//! The pick is a ray through the pixel against the primitives at the view flag:
//! their bounds first, then the triangles of those the ray enters, nearest hit
//! wins. A primitive under a closed row of the Scene Explorer is still picked
//! by its own geometry.

use glam::{Mat4, Vec2, Vec3};

use crate::core::poly::{ray_triangle, PickView};
use crate::types::Prim;
use crate::viewport::bounds::{self, Bounds};

/// Where a ray enters a box, if it does.
fn ray_box(o: Vec3, d: Vec3, b: Bounds) -> Option<f32> {
    let (mut t0, mut t1) = (0.0f32, f32::MAX);
    for k in 0..3 {
        if d[k].abs() < 1e-9 {
            if o[k] < b.0[k] || o[k] > b.1[k] {
                return None;
            }
            continue;
        }
        let (a, c) = ((b.0[k] - o[k]) / d[k], (b.1[k] - o[k]) / d[k]);
        t0 = t0.max(a.min(c));
        t1 = t1.min(a.max(c));
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

/// Where a ray first hits a mesh placed by `m`: in the mesh's own space, so the
/// ray is moved, not the triangles.
fn ray_mesh(o: Vec3, d: Vec3, mesh: &crate::types::MeshData, m: &Mat4) -> Option<f32> {
    let inv = m.inverse();
    let (lo, ld) = (inv.transform_point3(o), inv.transform_vector3(d));
    let mut best: Option<f32> = None;
    let v = |i: u32| Vec3::from_array(mesh.vertices[i as usize]);
    for tri in mesh.indices.chunks_exact(3) {
        if let Some(t) = ray_triangle(lo, ld, v(tri[0]), v(tri[1]), v(tri[2])) {
            if best.is_none_or(|b| t < b) {
                best = Some(t);
            }
        }
    }
    // Back to world distance along the ray.
    best.map(|t| (m.transform_point3(lo + ld * t) - o).dot(d.normalize_or_zero()))
}

/// The lines a primitive without geometry is drawn with: a light's shape, a
/// camera's pyramid. Empty for the rest.
pub fn glyph_lines(p: &Prim) -> Vec<[Vec3; 2]> {
    let world = p.place.matrices().first().copied().unwrap_or(Mat4::IDENTITY);
    if let Some(l) = &p.light {
        return l.glyph(world);
    }
    if let Some(c) = &p.camera {
        return c.glyph(world);
    }
    vec![]
}

/// How near the pointer has to be to a light's or a camera's lines, in pixels.
const NEAR_LINE: f32 = 6.0;

/// Distance from a point to a segment, in 2D.
fn to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = if ab.length_squared() < 1e-9 {
        0.0
    } else {
        ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    };
    (a + ab * t).distance(p)
}

/// The path of the primitive under `pixel`, among `prims`: a mesh the ray hits,
/// or a light or a camera whose lines pass near it, the nearest.
#[cfg(test)]
pub fn prim_at(view: &PickView, pixel: Vec2, prims: &[Prim]) -> Option<String> {
    prim_at_drawn(view, pixel, prims, &Default::default())
}

/// As `prim_at`, for the scene as drawn: a prim drawn as its box or its origin
/// (`standins`) is picked by its box, not its triangles; a copy of a point
/// instancer's prototype picks that instance (`/forest/trees[12]`).
pub fn prim_at_drawn(
    view: &PickView,
    pixel: Vec2,
    prims: &[Prim],
    standins: &std::collections::HashSet<String>,
) -> Option<String> {
    let (o, d) = view.ray(pixel);
    let d = d.normalize_or_zero();
    // The nearest hit: the prim, and which of its copies.
    let mut best: Option<(f32, &Prim, usize)> = None;
    for p in prims.iter().filter(|p| p.light.is_some() || p.camera.is_some()) {
        for [a, b] in glyph_lines(p) {
            let (Some(sa), Some(sb)) = (view.project(a), view.project(b)) else {
                continue;
            };
            if to_segment(pixel, sa, sb) > NEAR_LINE {
                continue;
            }
            let t = (a.lerp(b, 0.5) - o).dot(d);
            if t > 0.0 && best.is_none_or(|x| t < x.0) {
                best = Some((t, p, 0));
            }
        }
    }
    for p in prims {
        if p.mesh.indices.is_empty() {
            continue;
        }
        let Some(local) = bounds::local_bounds(&p.mesh) else {
            continue;
        };
        let matrices: &[Mat4] = if p.place.is_placed() {
            p.place.matrices()
        } else {
            &[Mat4::IDENTITY]
        };
        let boxed = standins.contains(&p.path);
        for (k, m) in matrices.iter().enumerate() {
            // Cheap first: the copy's box; drawn as a box, the box is it.
            let Some(entry) = ray_box(o, d, bounds::moved_bounds(local, m)) else {
                continue;
            };
            let hit = if boxed { Some(entry) } else { ray_mesh(o, d, &p.mesh, m) };
            if let Some(t) = hit {
                if t > 0.0 && best.is_none_or(|b| t < b.0) {
                    best = Some((t, p, k));
                }
            }
        }
    }
    best.map(|(_, p, copy)| {
        match p
            .instanced_by
            .as_ref()
            .and_then(|by| crate::core::instancing::point_of_copy(by, copy).map(|i| (by, i)))
        {
            Some((by, i)) => crate::core::instancing::instance_name(&by.instancer, i),
            None => p.path.clone(),
        }
    })
}

/// The lines of the selected lights and cameras, drawn in the selection's
/// colour: they have no bounds to outline.
pub fn selected_glyphs(prims: &[Prim], selected: &[String]) -> Vec<[Vec3; 2]> {
    prims
        .iter()
        .filter(|p| selected.iter().any(|s| *s == p.path))
        .flat_map(glyph_lines)
        .collect()
}

/// The world bounds of the primitives whose paths are selected, to outline; a
/// selected instance (`/forest/trees[12]`), the box of each prim of its
/// prototype where that instance is.
pub fn selected_bounds(prims: &[Prim], selected: &[String]) -> Vec<Bounds> {
    let mut out: Vec<Bounds> = prims
        .iter()
        .filter(|p| selected.iter().any(|s| *s == p.path))
        .filter_map(bounds::world_bounds)
        .collect();
    for (inst, i) in selected
        .iter()
        .filter_map(|s| crate::core::instancing::instance_path(s))
    {
        let Some(instancer) = prims.iter().find(|p| p.path == inst && p.instancing.is_some()) else {
            continue;
        };
        let Some(geo) = &instancer.geo else { continue };
        let proto = geo
            .attr(crate::core::geo::Context::Point, "protoIndex")
            .and_then(|a| a.column.ints())
            .and_then(|ix| ix.get(i))
            .copied()
            .unwrap_or(0);
        let at = crate::core::instancing::instance_matrices(geo, &instancer.place, i);
        for p in prims {
            let Some(by) = p
                .instanced_by
                .as_ref()
                .filter(|by| by.instancer == inst && by.index as i32 == proto)
            else {
                continue;
            };
            let Some(local) = bounds::local_bounds(&p.mesh) else {
                continue;
            };
            out.extend(at.iter().map(|m| bounds::moved_bounds(local, &(*m * by.inside))));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MeshData, Placement};

    fn view() -> PickView {
        let size = Vec2::new(800.0, 600.0);
        let eye = Vec3::new(0.0, 0.0, 6.0);
        let proj = Mat4::perspective_infinite_reverse_rh(0.8, size.x / size.y, 0.1);
        PickView {
            view_proj: proj * Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y),
            size,
            eye,
        }
    }

    /// Two cubes side by side: the pixel over each one picks it, the pixel
    /// between them nothing, a copy placed by a matrix is picked where the copy
    /// is, and the nearer of two in line wins.
    #[test]
    fn the_primitive_under_the_pixel_is_picked() {
        let cube = std::sync::Arc::new(crate::node_graph::nodes::create_cube(1.0));
        let left = Prim {
            place: Placement::One(Mat4::from_translation(Vec3::new(-2.0, 0.0, 0.0))),
            ..Prim::new("/left".into(), MeshData::default())
        };
        let left = Prim {
            mesh: cube.clone(),
            ..left
        };
        let right = Prim {
            mesh: cube.clone(),
            place: Placement::Many(
                vec![
                    Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0)),
                    Mat4::from_translation(Vec3::new(2.0, 3.0, 0.0)),
                ]
                .into(),
            ),
            ..Prim::new("/right".into(), MeshData::default())
        };
        let v = view();
        let shown = [left, right];
        let at = |p: Vec3| v.project(p).unwrap();
        assert_eq!(
            prim_at(&v, at(Vec3::new(-2.0, 0.0, 0.5)), &shown).as_deref(),
            Some("/left")
        );
        assert_eq!(
            prim_at(&v, at(Vec3::new(2.0, 0.0, 0.5)), &shown).as_deref(),
            Some("/right")
        );
        assert_eq!(
            prim_at(&v, at(Vec3::new(2.0, 3.0, 0.5)), &shown).as_deref(),
            Some("/right")
        );
        assert_eq!(prim_at(&v, at(Vec3::new(0.0, 0.0, 0.0)), &shown), None);
        // The nearer of two in line wins: a cube in front of the left one.
        let front = Prim {
            mesh: cube.clone(),
            place: Placement::One(Mat4::from_translation(Vec3::new(-2.0, 0.0, 2.0))),
            ..Prim::new("/front".into(), MeshData::default())
        };
        let three = [shown[0].clone(), shown[1].clone(), front];
        assert_eq!(
            prim_at(&v, at(Vec3::new(-2.0, 0.0, 0.5)), &three).as_deref(),
            Some("/front")
        );
        let b = selected_bounds(&three, &["/right".into(), "/front".into()]);
        assert_eq!(b.len(), 2);
        assert!((b[0].1.y - 3.5).abs() < 1e-5, "{:?}", b[0]);
    }

    /// A light is picked by its lines: the pointer on its rectangle's edge, not
    /// in the empty middle; selected, its lines are what is outlined.
    #[test]
    fn a_light_is_picked_by_its_shape() {
        use crate::core::light::{Light, LightType};
        let rect = crate::core::light::light_prim(
            "/lights/rect",
            Light::new(LightType::Rect),
            Placement::One(Mat4::IDENTITY),
        );
        let v = view();
        let shown = [rect];
        // The rectangle's left edge, half way up (1 by 1, in the XY plane).
        let edge = v.project(Vec3::new(-0.5, 0.2, 0.0)).unwrap();
        assert_eq!(prim_at(&v, edge, &shown).as_deref(), Some("/lights/rect"));
        let off = v.project(Vec3::new(-1.5, 1.5, 0.0)).unwrap();
        assert_eq!(prim_at(&v, off, &shown), None);
        assert!(!selected_glyphs(&shown, &["/lights/rect".into()]).is_empty());
        assert!(selected_glyphs(&shown, &[]).is_empty());
    }
}
