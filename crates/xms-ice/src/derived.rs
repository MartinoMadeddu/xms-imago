//! Attributes made from the topology rather than stored: the columns the
//! Primitive Inspector shows that no `Geo` holds, readable by Get Data like
//! any other (a stored attribute of the same name wins).
//!
//! | Context | Name               | Type  | What                                        |
//! |---------|--------------------|-------|---------------------------------------------|
//! | Face    | `corners`          | Int   | corners of the polygon (control points of a curve) |
//! | Face    | `firstCorner`      | Int   | its first corner: the others follow it      |
//! | Face    | `N`                | Vec3  | the polygon's normal (polygons only)        |
//! | Face    | `area`             | Float | the polygon's area (polygons only)          |
//! | Corner  | `ptIndex`          | Int   | the point the corner is at                  |
//! | Corner  | `faceIndex`        | Int   | the face it is a corner of                  |
//! | Corner  | `localCornerIndex` | Int   | its place in that face (0 for the first)    |
//!
//! Only the types are looked up when compiling; a column is made when a plan
//! that reads it runs, once per run.

use std::sync::Arc;

use xms_geo::{Column, Context, Geo, Topology};

use super::types::Ty;

/// Every derived attribute: its context, name and type.
pub const DERIVED: &[(Context, &str, Ty)] = &[
    (Context::Face, "corners", Ty::Int),
    (Context::Face, "firstCorner", Ty::Int),
    (Context::Face, "N", Ty::Vec3),
    (Context::Face, "area", Ty::Float),
    (Context::Corner, "ptIndex", Ty::Int),
    (Context::Corner, "faceIndex", Ty::Int),
    (Context::Corner, "localCornerIndex", Ty::Int),
];

/// Polygons only: a normal and an area need a surface.
fn polygons_only(name: &str) -> bool { matches!(name, "N" | "area") }

/// The type of the derived attribute `name` in `ctx` on `geo`, if `geo`'s
/// topology has it.
pub fn ty(geo: &Geo, ctx: Context, name: &str) -> Option<Ty> {
    let (_, _, t) = DERIVED.iter().find(|(c, n, _)| *c == ctx && *n == name)?;
    match &geo.topology {
        Topology::Points => None,
        Topology::Curves { .. } if polygons_only(name) => None,
        _ => Some(*t),
    }
}

/// The context a derived attribute named `name` is in on `geo`, if any.
pub fn context_of(geo: &Geo, name: &str) -> Option<Context> {
    DERIVED.iter().map(|(c, _, _)| *c).find(|c| ty(geo, *c, name).is_some())
}

/// The counts of each face, and the point of each corner.
fn faces(geo: &Geo) -> (&[u32], Vec<u32>) {
    match &geo.topology {
        Topology::Points => (&[], vec![]),
        Topology::Mesh { counts, indices, .. } => (counts.as_slice(), indices.to_vec()),
        Topology::Curves { counts, .. } => {
            let n: u32 = counts.iter().sum();
            (counts.as_slice(), (0..n).collect())
        }
    }
}

/// The column of the derived attribute `name` in `ctx`, made from `geo`'s
/// topology. `None` when `ty` says there is none.
pub fn column(geo: &Geo, ctx: Context, name: &str) -> Option<Column> {
    ty(geo, ctx, name)?;
    let (counts, corner_point) = faces(geo);
    let int = |v: Vec<i32>| Some(Column::Int(Arc::new(v)));
    match (ctx, name) {
        (Context::Face, "corners") => int(counts.iter().map(|&c| c as i32).collect()),
        (Context::Face, "firstCorner") => int(counts.iter().scan(0i32, |at, &c| { let s = *at; *at += c as i32; Some(s) }).collect()),
        (Context::Face, "N") | (Context::Face, "area") => {
            let pts = geo.points();
            let at = |c: u32| pts.get(corner_point[c as usize] as usize).copied().unwrap_or([0.0; 3]);
            let mut start = 0u32;
            // Newell's: twice the area, along the normal; flat or not.
            let area_normals: Vec<[f32; 3]> = counts.iter().map(|&n| {
                let mut s = [0.0f32; 3];
                for k in 0..n {
                    let (a, b) = (at(start + k), at(start + (k + 1) % n));
                    s[0] += (a[1] - b[1]) * (a[2] + b[2]);
                    s[1] += (a[2] - b[2]) * (a[0] + b[0]);
                    s[2] += (a[0] - b[0]) * (a[1] + b[1]);
                }
                start += n;
                if matches!(geo.topology, Topology::Mesh { left_handed: true, .. }) { [-s[0], -s[1], -s[2]] } else { s }
            }).collect();
            let len = |v: &[f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            Some(if name == "N" {
                Column::Vec3(Arc::new(area_normals.iter().map(|v| {
                    let l = len(v);
                    if l > 0.0 { [v[0] / l, v[1] / l, v[2] / l] } else { [0.0; 3] }
                }).collect()))
            } else {
                Column::Float(Arc::new(area_normals.iter().map(|v| len(v) * 0.5).collect()))
            })
        }
        (Context::Corner, "ptIndex") => int(corner_point.iter().map(|&p| p as i32).collect()),
        (Context::Corner, "faceIndex") => int(counts.iter().enumerate().flat_map(|(f, &c)| (0..c).map(move |_| f as i32)).collect()),
        (Context::Corner, "localCornerIndex") => int(counts.iter().flat_map(|&c| 0..c as i32).collect()),
        _ => None,
    }
}
