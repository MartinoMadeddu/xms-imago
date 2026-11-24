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
//!
//! Curves and points are picked by their lines as drawn, as lights and cameras
//! are: the pointer near one of them.
//!
//! A box dragged over the picture selects every primitive it touches, behind
//! others too: a triangle of a mesh, or a line of a curve, a point, a light or
//! a camera, in it.

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

/// How near the pointer has to be to a line, in pixels.
const NEAR_LINE: f32 = 6.0;

/// A prim's curves and points as the Viewer draws them (`strands::lines`), in
/// its own space. Empty for a prim without them, and for a point instancer,
/// whose points are not drawn.
fn strand_lines(p: &Prim) -> Vec<[Vec3; 2]> {
    if p.instancing.is_some() || !crate::strands::has_strands(&p.mesh) {
        return vec![];
    }
    let (pos, idx) = crate::strands::lines(&p.mesh);
    idx.chunks_exact(2)
        .map(|s| {
            [
                Vec3::from_array(pos[s[0] as usize]),
                Vec3::from_array(pos[s[1] as usize]),
            ]
        })
        .collect()
}

/// Where copy `m` of a prim's strands is on screen, roughly: the box around
/// its corners that are in front of the camera. `None` when the copy is
/// wholly behind it or has no bounds.
fn screen_box(view: &PickView, local: Bounds, m: &Mat4) -> Option<(Vec2, Vec2)> {
    let (b0, b1) = bounds::moved_bounds(local, m);
    let corners: Vec<Vec2> = (0..8)
        .filter_map(|i: usize| {
            view.project(Vec3::new(
                if i & 1 == 0 { b0.x } else { b1.x },
                if i & 2 == 0 { b0.y } else { b1.y },
                if i & 4 == 0 { b0.z } else { b1.z },
            ))
        })
        .collect();
    match corners.len() {
        0 => None,
        8 => Some(
            corners
                .iter()
                .fold((Vec2::MAX, Vec2::MIN), |(a, b), s| (a.min(*s), b.max(*s))),
        ),
        // Partly behind the camera: its corners in front do not bound it.
        _ => Some((Vec2::MIN, Vec2::MAX)),
    }
}

/// How far along the ray (`o`, `d`) the segment `a`–`b` is where it passes
/// within `NEAR_LINE` of `pixel` on screen, if it does.
fn near_line(view: &PickView, pixel: Vec2, o: Vec3, d: Vec3, a: Vec3, b: Vec3) -> Option<f32> {
    let (sa, sb) = (view.project(a)?, view.project(b)?);
    if to_segment(pixel, sa, sb) > NEAR_LINE {
        return None;
    }
    let t = (a.lerp(b, 0.5) - o).dot(d);
    (t > 0.0).then_some(t)
}

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
/// or curves, points, a light or a camera whose lines pass near it, the
/// nearest.
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
            if let Some(t) = near_line(view, pixel, o, d, a, b) {
                if best.is_none_or(|x| t < x.0) {
                    best = Some((t, p, 0));
                }
            }
        }
    }
    for p in prims.iter().filter(|p| !standins.contains(&p.path)) {
        let lines = strand_lines(p);
        let Some(local) = bounds::local_bounds(&p.mesh).filter(|_| !lines.is_empty()) else {
            continue;
        };
        for (k, m) in matrices(p).iter().enumerate() {
            // Cheap first: the copy's box on screen, missed by the pointer.
            let Some((lo, hi)) = screen_box(view, local, m) else {
                continue;
            };
            if (pixel + NEAR_LINE).cmplt(lo).any() || (pixel - NEAR_LINE).cmpgt(hi).any() {
                continue;
            }
            for [a, b] in &lines {
                let (a, b) = (m.transform_point3(*a), m.transform_point3(*b));
                if let Some(t) = near_line(view, pixel, o, d, a, b) {
                    if best.is_none_or(|x| t < x.0) {
                        best = Some((t, p, k));
                    }
                }
            }
        }
    }
    for p in prims {
        let boxed = standins.contains(&p.path);
        // A prim of curves or points only, drawn as its box, is picked by it.
        if p.mesh.indices.is_empty() && !boxed {
            continue;
        }
        let Some(local) = bounds::local_bounds(&p.mesh) else {
            continue;
        };
        for (k, m) in matrices(p).iter().enumerate() {
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
    best.map(|(_, p, copy)| copy_path(p, copy_points(p).as_deref(), copy))
}

/// Where `p`'s copies are: its placements, or where it is.
fn matrices(p: &Prim) -> &[Mat4] {
    if p.place.is_placed() {
        p.place.matrices()
    } else {
        &[Mat4::IDENTITY]
    }
}

/// What selecting copy `copy` of `p` selects: a point instancer's instance
/// (`/forest/trees[12]`) for a copy of its prototype, else the prim. `points`:
/// the instancer's points `p` is drawn at (`instancing::points_of`), made once
/// for all its copies.
fn copy_path(p: &Prim, points: Option<&[usize]>, copy: usize) -> String {
    let at = p.instanced_by.as_ref().zip(points);
    match at.and_then(|(by, points)| crate::core::instancing::point_of_copy(points, copy).map(|i| (by, i))) {
        Some((by, i)) => crate::core::instancing::instance_name(&by.instancer, i),
        None => p.path.clone(),
    }
}

/// The points a copy of `p` is drawn at, if it is a prototype's.
fn copy_points(p: &Prim) -> Option<Vec<usize>> {
    p.instanced_by.as_deref().map(crate::core::instancing::points_of)
}

/// Whether the convex shape `points` (a triangle, a segment) and the box `lo`
/// to `hi` overlap, in 2D: no side of either separates them.
fn overlaps(points: &[Vec2], lo: Vec2, hi: Vec2) -> bool {
    let corners = [lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y)];
    let apart = |axis: Vec2| {
        let span = |ps: &[Vec2]| {
            ps.iter()
                .map(|p| p.dot(axis))
                .fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(x), b.max(x)))
        };
        let ((a0, a1), (b0, b1)) = (span(points), span(&corners[..]));
        a1 < b0 || b1 < a0
    };
    let sides = (0..points.len()).map(|k| (points[(k + 1) % points.len()] - points[k]).perp());
    !(apart(Vec2::X) || apart(Vec2::Y) || sides.into_iter().any(|n| n != Vec2::ZERO && apart(n)))
}

/// The paths of the primitives the box from `a` to `b` (pixels) touches, as
/// drawn (`standins` by their boxes, copies of a point instancer's prototype
/// as its instances), behind others too. A triangle partly behind the camera
/// does not count.
///
/// A scatter has a million copies: they are tried on every core, a copy's box
/// first. One whose box on screen misses is passed over, one whose box is all
/// inside is taken; only those across the edge are tried triangle by
/// triangle.
pub fn prims_in_rect(
    view: &PickView,
    a: Vec2,
    b: Vec2,
    prims: &[Prim],
    standins: &std::collections::HashSet<String>,
) -> Vec<String> {
    use rayon::prelude::*;
    let (lo, hi) = (a.min(b), a.max(b));
    let each: Vec<Vec<String>> = prims
        .par_iter()
        .map(|p| touched(view, lo, hi, p, standins.contains(&p.path)))
        .collect();
    // Once each, in the order of the prims.
    let mut seen = std::collections::HashSet::new();
    each.into_iter().flatten().filter(|p| seen.insert(p.clone())).collect()
}

/// Where a box on screen (`s0`, `s1`) is against the box dragged.
#[derive(PartialEq)]
enum Against {
    Apart,
    Inside,
    Across,
}

fn against(s0: Vec2, s1: Vec2, lo: Vec2, hi: Vec2) -> Against {
    if s1.cmplt(lo).any() || s0.cmpgt(hi).any() {
        Against::Apart
    } else if s0.cmpge(lo).all() && s1.cmple(hi).all() {
        Against::Inside
    } else {
        Against::Across
    }
}

/// What of `p` the box `lo` to `hi` touches: the prim, or its instances.
/// `boxed`: drawn as its box.
fn touched(view: &PickView, lo: Vec2, hi: Vec2, p: &Prim, boxed: bool) -> Vec<String> {
    use rayon::prelude::*;
    let mut out = vec![];
    if p.light.is_some() || p.camera.is_some() {
        let hit = glyph_lines(p)
            .iter()
            .any(|[a, b]| match (view.project(*a), view.project(*b)) {
                (Some(sa), Some(sb)) => overlaps(&[sa, sb], lo, hi),
                _ => false,
            });
        if hit {
            out.push(p.path.clone());
        }
    }
    let lines = if boxed { vec![] } else { strand_lines(p) };
    let meshed = !p.mesh.indices.is_empty() || boxed;
    let Some(local) = bounds::local_bounds(&p.mesh).filter(|_| !lines.is_empty() || meshed) else {
        return out;
    };
    let points = copy_points(p);
    let points = points.as_deref();
    let copies = matrices(p);
    let strands = |m: &Mat4| match screen_box(view, local, m) {
        None => false,
        Some((s0, s1)) => match against(s0, s1, lo, hi) {
            Against::Apart => false,
            Against::Inside => true,
            Against::Across => lines.iter().any(|[a, b]| {
                match (
                    view.project(m.transform_point3(*a)),
                    view.project(m.transform_point3(*b)),
                ) {
                    (Some(sa), Some(sb)) => overlaps(&[sa, sb], lo, hi),
                    _ => false,
                }
            }),
        },
    };
    let mesh = |m: &Mat4| {
        let (b0, b1) = bounds::moved_bounds(local, m);
        let corner = |i: usize| {
            Vec3::new(
                if i & 1 == 0 { b0.x } else { b1.x },
                if i & 2 == 0 { b0.y } else { b1.y },
                if i & 4 == 0 { b0.z } else { b1.z },
            )
        };
        let screen: Vec<Option<Vec2>> = (0..8).map(|i| view.project(corner(i))).collect();
        // Cheap first: a copy wholly in front, by its box on screen.
        if screen.iter().all(Option::is_some) {
            let (s0, s1) = screen
                .iter()
                .flatten()
                .fold((Vec2::MAX, Vec2::MIN), |(a, b), s| (a.min(*s), b.max(*s)));
            match against(s0, s1, lo, hi) {
                Against::Apart => return false,
                Against::Inside => return true,
                Against::Across => {}
            }
        }
        if boxed {
            // The box's faces, as the corners' quads.
            return [
                [0, 1, 3, 2],
                [4, 5, 7, 6],
                [0, 1, 5, 4],
                [2, 3, 7, 6],
                [0, 2, 6, 4],
                [1, 3, 7, 5],
            ]
            .iter()
            .any(|face| {
                let q: Option<Vec<Vec2>> = face.iter().map(|i| screen[*i]).collect();
                q.is_some_and(|q| overlaps(&q, lo, hi))
            });
        }
        let at: Vec<Option<Vec2>> = p
            .mesh
            .vertices
            .iter()
            .map(|v| view.project(m.transform_point3(Vec3::from_array(*v))))
            .collect();
        p.mesh
            .indices
            .chunks_exact(3)
            .any(|t| match (at[t[0] as usize], at[t[1] as usize], at[t[2] as usize]) {
                (Some(x), Some(y), Some(z)) => overlaps(&[x, y, z], lo, hi),
                _ => false,
            })
    };
    let hit = |m: &Mat4| (!lines.is_empty() && strands(m)) || (meshed && mesh(m));
    if copies.len() == 1 {
        if hit(&copies[0]) {
            out.push(copy_path(p, points, 0));
        }
        return out;
    }
    let hits: Vec<usize> = copies
        .par_iter()
        .enumerate()
        .filter(|&(_, m)| hit(m))
        .map(|(k, _)| k)
        .collect();
    out.extend(hits.into_iter().map(|k| copy_path(p, points, k)));
    out
}

/// The lines of the selected lights and cameras, drawn in the selection's
/// colour: they have no bounds to outline.
pub fn selected_glyphs(prims: &[Prim], selected: &std::collections::HashSet<String>) -> Vec<[Vec3; 2]> {
    prims
        .iter()
        .filter(|p| (p.light.is_some() || p.camera.is_some()) && selected.contains(&p.path))
        .flat_map(glyph_lines)
        .collect()
}

/// More boxes than this, and each instancer's selected instances are outlined
/// by one box around them all, the selected prims by one around theirs: a box
/// over a scatter can select a hundred thousand instances.
pub const OUTLINES: usize = 2000;

/// The world bounds of the primitives whose paths are selected, to outline; a
/// selected instance (`/forest/trees[12]`), the box of its prototype where
/// that instance is.
pub fn selected_bounds(prims: &[Prim], selected: &std::collections::HashSet<String>) -> Vec<Bounds> {
    use rayon::prelude::*;
    use std::collections::HashMap;
    let mut out: Vec<Bounds> = prims
        .par_iter()
        .filter(|p| selected.contains(&p.path))
        .filter_map(bounds::world_bounds)
        .collect();
    // The selected instances, by instancer.
    let mut chosen: HashMap<&str, Vec<usize>> = HashMap::new();
    for (inst, i) in selected
        .iter()
        .filter_map(|s| crate::core::instancing::instance_path(s))
    {
        chosen.entry(inst).or_default().push(i);
    }
    let mut many = out.len() > OUTLINES;
    let mut instanced: Vec<Vec<Bounds>> = vec![];
    if !chosen.is_empty() {
        // Each prototype's box, in its own space: around its meshes.
        let mut protos: HashMap<(&str, i32), Bounds> = HashMap::new();
        for p in prims {
            let Some(by) = p
                .instanced_by
                .as_ref()
                .filter(|by| chosen.contains_key(by.instancer.as_str()))
            else {
                continue;
            };
            let Some(local) = bounds::local_bounds(&p.mesh) else {
                continue;
            };
            let b = bounds::moved_bounds(local, &by.inside);
            protos
                .entry((by.instancer.as_str(), by.index as i32))
                .and_modify(|a| *a = (a.0.min(b.0), a.1.max(b.1)))
                .or_insert(b);
        }
        for p in prims.iter().filter(|p| p.instancing.is_some()) {
            let (Some(ix), Some(geo)) = (chosen.get(p.path.as_str()), &p.geo) else {
                continue;
            };
            let index = geo
                .attr(crate::core::geo::Context::Point, "protoIndex")
                .and_then(|a| a.column.ints());
            let boxes: Vec<Bounds> = ix
                .par_iter()
                .flat_map_iter(|&i| {
                    let proto = index.and_then(|x| x.get(i)).copied().unwrap_or(0);
                    let b = protos.get(&(p.path.as_str(), proto)).copied();
                    let at = b.map(|_| crate::core::instancing::instance_matrices(geo, &p.place, i));
                    at.into_iter()
                        .flatten()
                        .filter_map(move |m| b.map(|b| bounds::moved_bounds(b, &m)))
                })
                .collect();
            many |= boxes.len() > OUTLINES;
            instanced.push(boxes);
        }
        many |= out.len() + instanced.iter().map(Vec::len).sum::<usize>() > OUTLINES;
    }
    let around = |bs: &[Bounds]| bs.iter().copied().reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)));
    if many {
        out = around(&out).into_iter().collect();
        out.extend(instanced.iter().filter_map(|bs| around(bs)));
    } else {
        out.extend(instanced.into_iter().flatten());
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
        let b = selected_bounds(&three, &["/right".to_string(), "/front".to_string()].into());
        assert_eq!(b.len(), 2);
        assert!((b[0].1.y - 3.5).abs() < 1e-5, "{:?}", b[0]);
    }

    /// A box touching a cube's corner selects it, one between the cubes
    /// nothing, one around both both; a light by its lines.
    #[test]
    fn a_box_selects_what_it_touches() {
        let cube = std::sync::Arc::new(crate::node_graph::nodes::create_cube(1.0));
        let at = |x: f32| Prim {
            mesh: cube.clone(),
            place: Placement::One(Mat4::from_translation(Vec3::new(x, 0.0, 0.0))),
            ..Prim::new(format!("/c{x}"), MeshData::default())
        };
        let light = crate::core::light::light_prim(
            "/lights/rect",
            crate::core::light::Light::new(crate::core::light::LightType::Rect),
            Placement::One(Mat4::from_translation(Vec3::new(0.0, 3.0, 0.0))),
        );
        let shown = [at(-2.0), at(2.0), light];
        let v = view();
        let s = |p: Vec3| v.project(p).unwrap();
        let none = Default::default();
        let boxed = |a: Vec3, b: Vec3| prims_in_rect(&v, s(a), s(b), &shown, &none);
        // Over the left cube's right edge only.
        assert_eq!(
            boxed(Vec3::new(-1.6, -0.1, 0.5), Vec3::new(-1.0, 0.1, 0.5)),
            vec!["/c-2"]
        );
        assert!(boxed(Vec3::new(-0.5, -0.1, 0.0), Vec3::new(0.5, 0.1, 0.0)).is_empty());
        assert_eq!(
            boxed(Vec3::new(-3.0, -1.0, 0.0), Vec3::new(3.0, 1.0, 0.0)),
            vec!["/c-2", "/c2"]
        );
        // Inside the light's rectangle, between its lines: not it; across its
        // edge: it.
        assert!(boxed(Vec3::new(-0.2, 3.35, 0.0), Vec3::new(0.2, 3.45, 0.0)).is_empty());
        assert_eq!(
            boxed(Vec3::new(0.4, 2.9, 0.0), Vec3::new(0.6, 3.1, 0.0)),
            vec!["/lights/rect"]
        );
        // The box's corners either way round.
        assert_eq!(boxed(Vec3::new(3.0, 1.0, 0.0), Vec3::new(1.0, -1.0, 0.0)), vec!["/c2"]);
    }

    #[test]
    fn shapes_and_boxes_overlap_as_they_should() {
        let (lo, hi) = (Vec2::ZERO, Vec2::splat(10.0));
        let tri = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| [a, b, c].map(|(x, y)| Vec2::new(x, y));
        assert!(overlaps(&tri((2.0, 2.0), (3.0, 2.0), (2.0, 3.0)), lo, hi));
        // Around the box: overlaps though no corner is in it.
        assert!(overlaps(&tri((-50.0, -50.0), (50.0, -50.0), (0.0, 50.0)), lo, hi));
        // Its long side passing a corner of the box without touching it, the
        // boxes around the two overlapping.
        assert!(!overlaps(&tri((0.0, 21.0), (21.0, 0.0), (30.0, 30.0)), lo, hi));
        // A segment across the box, and one beside it.
        assert!(overlaps(&[Vec2::new(-5.0, 5.0), Vec2::new(15.0, 5.0)], lo, hi));
        assert!(!overlaps(&[Vec2::new(-5.0, 11.0), Vec2::new(15.0, 11.0)], lo, hi));
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
        assert!(!selected_glyphs(&shown, &["/lights/rect".to_string()].into()).is_empty());
        assert!(selected_glyphs(&shown, &Default::default()).is_empty());
    }

    /// A curve is picked by its line, as drawn, the pointer near it, not beside
    /// it; a box across it selects it, one beside it not; a copy is picked
    /// where it is; a point cloud by its points.
    #[test]
    fn curves_and_points_are_picked_by_their_lines() {
        let hair = MeshData {
            curve_points: vec![[-1.0, -1.0, 0.0], [-1.0, 0.0, 0.0], [-1.0, 1.0, 0.0]],
            curve_counts: vec![3],
            ..MeshData::default()
        };
        let curve = Prim {
            place: Placement::Many(vec![Mat4::IDENTITY, Mat4::from_translation(Vec3::new(3.0, 0.0, 0.0))].into()),
            ..Prim::new("/hair".into(), hair)
        };
        let cloud = Prim::new(
            "/cloud".into(),
            MeshData {
                points: vec![[0.0, 2.0, 0.0]],
                ..MeshData::default()
            },
        );
        let v = view();
        let shown = [curve, cloud];
        let at = |p: Vec3| v.project(p).unwrap();
        let px = |x: f32| at(Vec3::new(x, 0.5, 0.0));
        assert_eq!(prim_at(&v, px(-1.0), &shown).as_deref(), Some("/hair"));
        assert_eq!(prim_at(&v, px(2.0), &shown).as_deref(), Some("/hair"));
        assert_eq!(prim_at(&v, px(0.0), &shown), None);
        assert_eq!(
            prim_at(&v, at(Vec3::new(0.0, 2.0, 0.0)), &shown).as_deref(),
            Some("/cloud")
        );
        let none = Default::default();
        let boxed = |a: Vec3, b: Vec3| prims_in_rect(&v, at(a), at(b), &shown, &none);
        assert_eq!(
            boxed(Vec3::new(-1.5, 0.4, 0.0), Vec3::new(-0.5, 0.6, 0.0)),
            vec!["/hair"]
        );
        assert!(boxed(Vec3::new(-0.5, -0.5, 0.0), Vec3::new(0.5, 0.5, 0.0)).is_empty());
    }

    /// A point instancer of `n` points in a row along X, `step` apart, the
    /// second hidden, and a cube drawn at each of the others.
    fn scatter(n: usize, step: f32) -> Vec<Prim> {
        use crate::core::geo::{Attr, Column, Context, Geo, Role};
        use crate::core::geo_mesh::GeoMesh;
        use std::sync::Arc;
        let xs = (0..n).map(|k| [(k as f32 - 2.0) * step, 0.0, 0.0]).collect();
        let mut geo = Geo::from_points(xs);
        geo.set(
            Context::Point,
            "protoIndex",
            Attr::new(Column::Int(Arc::new(vec![0; n])), Role::None),
        );
        let geo = Arc::new(geo);
        let place = Placement::One(Mat4::IDENTITY);
        let instancer = Prim {
            geo: Some(geo.clone()),
            place: place.clone(),
            instancing: Some(Arc::new(vec!["/inst/cube".into()])),
            ..Prim::new("/inst".into(), geo.to_mesh())
        };
        let by = crate::types::InstancedBy {
            instancer: "/inst".into(),
            index: 0,
            inside: Mat4::IDENTITY,
            hidden: Arc::new(vec![1]),
            geo: geo.clone(),
            place: place.clone(),
            file: None,
        };
        let cube = Prim {
            mesh: Arc::new(crate::node_graph::nodes::create_cube(1.0)),
            place: Placement::Many(crate::core::instancing::copies(&geo, &place, &by).into()),
            instanced_by: Some(Arc::new(by)),
            ..Prim::new("/inst/cube".into(), MeshData::default())
        };
        vec![instancer, cube]
    }

    /// A box over a scatter selects the instances it touches, by their
    /// points (a hidden one is not drawn, so not counted); they are outlined
    /// one by one, and past `OUTLINES` by one box around them all.
    #[test]
    fn a_box_over_a_scatter_selects_its_instances() {
        let shown = scatter(5, 1.2);
        let v = view();
        let s = |p: Vec3| v.project(p).unwrap();
        let none = Default::default();
        let boxed = |a: Vec3, b: Vec3| prims_in_rect(&v, s(a), s(b), &shown, &none);
        // Over the cube at 0 and where the hidden one at -1.2 would be.
        assert_eq!(
            boxed(Vec3::new(-1.5, -0.1, 0.5), Vec3::new(0.3, 0.1, 0.5)),
            vec!["/inst[2]"]
        );
        assert_eq!(
            boxed(Vec3::new(-3.0, -1.0, 0.0), Vec3::new(3.0, 1.0, 0.0)),
            vec!["/inst[0]", "/inst[2]", "/inst[3]", "/inst[4]"]
        );
        // Wholly inside: taken by its box.
        assert_eq!(
            boxed(Vec3::new(-0.6, -0.6, 0.0), Vec3::new(0.6, 0.6, 0.0)),
            vec!["/inst[2]"]
        );
        let mut b = selected_bounds(&shown, &["/inst[2]".to_string(), "/inst[4]".to_string()].into());
        b.sort_by(|x, y| x.0.x.total_cmp(&y.0.x));
        assert_eq!(b.len(), 2);
        assert!((b[0].0.x + 0.5).abs() < 1e-5 && (b[1].1.x - 2.9).abs() < 1e-5, "{b:?}");
        // Many: one box around them.
        let many = scatter(OUTLINES + 10, 1.0);
        let all = (0..OUTLINES + 10)
            .filter(|&k| k != 1)
            .map(|k| format!("/inst[{k}]"))
            .collect();
        let b = selected_bounds(&many, &all);
        assert_eq!(b.len(), 1);
        assert!((b[0].0.x + 2.5).abs() < 1e-4, "{b:?}");
    }
}
