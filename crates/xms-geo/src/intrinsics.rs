//! Intrinsics: what a primitive has from its topology and points rather than
//! stored, read-only, every one named `intrinsic:…` so that it can never be
//! taken for a stored attribute. The one list ICE reads them by (Get Data, Get
//! Array Data), the Primitive Inspector shows them by and the docs list.
//!
//! | Where  | Name                         | Type  | On             | What |
//! |--------|------------------------------|-------|----------------|------|
//! | Point  | `intrinsic:ptIndex`          | Int   | all            | the point's own number |
//! | Point  | `intrinsic:curveu`           | Float | curves         | how far along its curve, by length: 0 to 1 |
//! | Point  | `intrinsic:faceIndices`      | Int list | mesh, curves | the faces the point is on |
//! | Point  | `intrinsic:tangent`          | Vec3  | mesh, curves   | along U of the texture coordinates (else the edges); along the curve |
//! | Face   | `intrinsic:faceIndex`        | Int   | mesh, curves   | the face's own number |
//! | Face   | `intrinsic:corners`          | Int   | mesh, curves   | its corners (a curve's control points) |
//! | Face   | `intrinsic:firstCorner`      | Int   | mesh, curves   | its first corner: the others follow it |
//! | Face   | `intrinsic:N`                | Vec3  | mesh           | the polygon's normal |
//! | Face   | `intrinsic:area`             | Float | mesh           | the polygon's area |
//! | Face   | `intrinsic:length`           | Float | curves         | the curve's length along its control points |
//! | Face   | `intrinsic:ptIndices`        | Int list | mesh, curves | its points, corner by corner |
//! | Corner | `intrinsic:ptIndex`          | Int   | mesh, curves   | the point the corner is at |
//! | Corner | `intrinsic:faceIndex`        | Int   | mesh, curves   | the face it is a corner of |
//! | Corner | `intrinsic:localCornerIndex` | Int   | mesh, curves   | its place in that face (0 for the first) |
//! | Corner | `intrinsic:tangent`          | Vec3  | mesh           | along U, its own across a UV seam |
//! | Object | `intrinsic:pointCount`       | Int   | all            | how many points |
//! | Object | `intrinsic:faceCount`        | Int   | all            | how many faces (polygons or curves) |
//! | Object | `intrinsic:cornerCount`      | Int   | all            | how many corners |
//! | Object | `intrinsic:boundsMin`        | Vec3  | all            | the low corner of its points' box |
//! | Object | `intrinsic:boundsMax`        | Vec3  | all            | the high corner of its points' box |
//! | Object | `intrinsic:transform`        | Mat4  | all            | where it is placed in the world |
//! | Object | `intrinsic:typeName`         | Token | all            | Mesh, BasisCurves or Points (not in ICE yet) |
//! | Object | `intrinsic:path`             | Token | all            | its path (not in ICE yet) |
//! | Edge   | `intrinsic:ptIndices`        | Int list | mesh, curves | its two points (Inspector only) |
//! | Edge   | `intrinsic:length`           | Float | mesh, curves   | its length (Inspector only) |
//! | Place  | `intrinsic:triP`             | Vec3 list | mesh, curves | the points a place is made from |
//! | Place  | `intrinsic:bary`             | Float list | mesh, curves | their weights there |
//!
//! A place is a Polygon and a Param UV (Closest Point on Surface's, Scatter on
//! Surface's): what is "at a place" is read there only.

use std::collections::HashSet;
use std::sync::Arc;

use super::{Column, Context, CurveWrap, Geo, Kind, Topology};

/// What every intrinsic's name starts with.
pub const PREFIX: &str = "intrinsic:";
/// The points a place is made from, and their weights there.
pub const TRI_P: &str = "intrinsic:triP";
pub const BARY: &str = "intrinsic:bary";
/// Tangents, as the Tangents operator makes them (`frames`).
pub const TANGENT: &str = "intrinsic:tangent";

/// Where an intrinsic is: per element of a context, per edge (the Primitive
/// Inspector's, not a context of ICE's), or at a place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    In(Context),
    Edge,
    Place,
}

/// Which primitives have it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum On {
    All,
    MeshAndCurves,
    Mesh,
    Curves,
}

impl On {
    fn has(self, topology: &Topology) -> bool {
        match (self, topology) {
            (On::All, _) => true,
            (On::MeshAndCurves, Topology::Mesh { .. } | Topology::Curves { .. }) => true,
            (On::Mesh, Topology::Mesh { .. }) => true,
            (On::Curves, Topology::Curves { .. }) => true,
            _ => false,
        }
    }
}

/// One intrinsic.
#[derive(Clone, Copy, Debug)]
pub struct Intrinsic {
    pub scope: Scope,
    pub name: &'static str,
    pub kind: Kind,
    pub on: On,
    /// A list per element: the values of `(context, name)` it touches (Get
    /// Array Data reads it).
    pub gathers: Option<(Context, &'static str)>,
    /// What its values number, when they are element numbers: the Inspector
    /// counts them on across the primitives it lists.
    pub numbers: Option<Context>,
    /// Read by ICE (not yet: tokens, and edges).
    pub ice: bool,
    pub about: &'static str,
}

const fn of(scope: Scope, name: &'static str, kind: Kind, on: On, about: &'static str) -> Intrinsic {
    Intrinsic {
        scope,
        name,
        kind,
        on,
        gathers: None,
        numbers: None,
        ice: true,
        about,
    }
}

const fn numbering(i: Intrinsic, what: Context) -> Intrinsic {
    Intrinsic {
        numbers: Some(what),
        ..i
    }
}

const fn gathering(i: Intrinsic, context: Context, name: &'static str) -> Intrinsic {
    Intrinsic {
        gathers: Some((context, name)),
        numbers: Some(context),
        ..i
    }
}

const fn not_in_ice(i: Intrinsic) -> Intrinsic {
    Intrinsic { ice: false, ..i }
}

use Context::{Corner, Face, Object, Point};
use Scope::{Edge, In, Place};

/// Every intrinsic, by where it is.
pub const ALL: &[Intrinsic] = &[
    numbering(of(In(Point), "intrinsic:ptIndex", Kind::Int, On::All, "The point's own number."), Point),
    of(In(Point), "intrinsic:curveu", Kind::Float, On::Curves, "How far along its curve the point is, by length: 0 at its first point, 1 at its last."),
    gathering(of(In(Point), "intrinsic:faceIndices", Kind::Int, On::MeshAndCurves, "The faces the point is on, as a list."), Face, "intrinsic:faceIndex"),
    of(In(Point), TANGENT, Kind::Vec3, On::MeshAndCurves, "The point's tangent: on a mesh along U of its texture coordinates (st, else any), along its edges with none, square to its normals; on a curve along it."),
    numbering(of(In(Face), "intrinsic:faceIndex", Kind::Int, On::MeshAndCurves, "The face's own number (a polygon's or a curve's)."), Face),
    of(In(Face), "intrinsic:corners", Kind::Int, On::MeshAndCurves, "How many corners the face has: a curve's are its control points."),
    numbering(of(In(Face), "intrinsic:firstCorner", Kind::Int, On::MeshAndCurves, "The face's first corner: its others follow it."), Corner),
    of(In(Face), "intrinsic:N", Kind::Vec3, On::Mesh, "The polygon's normal."),
    of(In(Face), "intrinsic:area", Kind::Float, On::Mesh, "The polygon's area."),
    of(In(Face), "intrinsic:length", Kind::Float, On::Curves, "The curve's length along its control points (closed when it is periodic)."),
    gathering(of(In(Face), "intrinsic:ptIndices", Kind::Int, On::MeshAndCurves, "The face's points, corner by corner, as a list."), Point, "intrinsic:ptIndex"),
    numbering(of(In(Corner), "intrinsic:ptIndex", Kind::Int, On::MeshAndCurves, "The point the corner is at."), Point),
    numbering(of(In(Corner), "intrinsic:faceIndex", Kind::Int, On::MeshAndCurves, "The face the corner is a corner of."), Face),
    of(In(Corner), "intrinsic:localCornerIndex", Kind::Int, On::MeshAndCurves, "The corner's place in its face: 0 for the first."),
    of(In(Corner), TANGENT, Kind::Vec3, On::Mesh, "The corner's tangent along U of the texture coordinates (else its edge), its own across a UV seam."),
    of(In(Object), "intrinsic:pointCount", Kind::Int, On::All, "How many points the primitive has."),
    of(In(Object), "intrinsic:faceCount", Kind::Int, On::All, "How many faces it has: polygons, or curves."),
    of(In(Object), "intrinsic:cornerCount", Kind::Int, On::All, "How many corners its faces have."),
    of(In(Object), "intrinsic:boundsMin", Kind::Vec3, On::All, "The low corner of the box around its points."),
    of(In(Object), "intrinsic:boundsMax", Kind::Vec3, On::All, "The high corner of the box around its points."),
    of(In(Object), "intrinsic:transform", Kind::Mat4, On::All, "Where the primitive is placed in the world."),
    not_in_ice(of(In(Object), "intrinsic:typeName", Kind::Token, On::All, "What it is: Mesh, BasisCurves or Points.")),
    not_in_ice(of(In(Object), "intrinsic:path", Kind::Token, On::All, "Its path in the scene.")),
    not_in_ice(numbering(of(Edge, "intrinsic:ptIndices", Kind::Int, On::MeshAndCurves, "The edge's two points."), Point)),
    not_in_ice(of(Edge, "intrinsic:length", Kind::Float, On::MeshAndCurves, "The edge's length.")),
    of(Place, TRI_P, Kind::Vec3, On::MeshAndCurves, "The points a place is made from (three, four, all of a larger polygon's, or a curve segment's two), as a list."),
    of(Place, BARY, Kind::Float, On::MeshAndCurves, "The weights of the points a place is made from, as a list (Get Data: the first three, as a vector)."),
];

/// Whether `name` is an intrinsic's: it starts with `intrinsic:`.
pub fn is_intrinsic(name: &str) -> bool {
    name.starts_with(PREFIX)
}

/// The intrinsic named `name` in `scope`.
pub fn get(scope: Scope, name: &str) -> Option<&'static Intrinsic> {
    ALL.iter().find(|i| i.scope == scope && i.name == name)
}

/// The intrinsic named `name` in `ctx` that `geo` has, a value per element
/// (not a list).
pub fn on(geo: &Geo, ctx: Context, name: &str) -> Option<&'static Intrinsic> {
    get(In(ctx), name).filter(|i| i.gathers.is_none() && i.on.has(&geo.topology))
}

/// The context of the intrinsic named `name` that `geo` has, a value per
/// element.
pub fn context_of(geo: &Geo, name: &str) -> Option<Context> {
    Context::ALL.into_iter().find(|c| on(geo, *c, name).is_some())
}

/// What the list intrinsic `name` (in `ctx`, or wherever it is) gathers: the
/// context and the intrinsic per element whose values it lists.
pub fn gathered(name: &str, ctx: Option<Context>) -> Option<(Context, &'static str)> {
    ALL.iter()
        .filter(|i| i.name == name && ctx.map_or(true, |c| i.scope == In(c)))
        .find_map(|i| i.gathers)
}

/// The intrinsics `geo` has in `ctx`, in the order listed: values per element
/// and lists.
pub fn of_geo(geo: &Geo, ctx: Context) -> impl Iterator<Item = &'static Intrinsic> + '_ {
    ALL.iter()
        .filter(move |i| i.scope == In(ctx) && i.on.has(&geo.topology))
}

/// The intrinsics of an edge, when `geo` has edges.
pub fn of_edges(geo: &Geo) -> impl Iterator<Item = &'static Intrinsic> + '_ {
    ALL.iter().filter(move |i| i.scope == Edge && i.on.has(&geo.topology))
}

/// What kind of primitive `geo` is, as USD names it.
pub fn type_name(geo: &Geo) -> &'static str {
    match geo.topology {
        Topology::Points => "Points",
        Topology::Mesh { .. } => "Mesh",
        Topology::Curves { .. } => "BasisCurves",
    }
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

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The column of the intrinsic named `name` in `ctx`, a value per element of
/// `geo`, placed at `world` (column-major, as `intrinsic:transform` gives it;
/// none: where it is). `None` when `geo` has none of that name there, for a
/// list, and for `intrinsic:path`, which is the scene's to give.
pub fn column(geo: &Geo, ctx: Context, name: &str, world: Option<[f64; 16]>) -> Option<Column> {
    on(geo, ctx, name)?;
    let (counts, corner_point) = faces(geo);
    let pts = geo.points();
    let int = |v: Vec<i32>| Some(Column::Int(Arc::new(v)));
    let closed = matches!(
        geo.topology,
        Topology::Curves {
            wrap: CurveWrap::Periodic,
            ..
        }
    );
    match (ctx, name) {
        (Point, "intrinsic:ptIndex") => int((0..pts.len() as i32).collect()),
        (Point | Corner, TANGENT) => geo
            .made_tangents(ctx, super::frames::TangentsFrom::Uv, "", super::frames::Weighting::Area)
            .map(|(t, ..)| t.column),
        (Point, "intrinsic:curveu") => {
            let mut u = Vec::with_capacity(pts.len());
            let mut first = 0usize;
            for &n in counts {
                let curve = pts.get(first..first + n as usize).unwrap_or(&[]);
                let start = u.len();
                let mut along = 0.0f32;
                for (k, &p) in curve.iter().enumerate() {
                    if k > 0 {
                        along += distance(curve[k - 1], p);
                    }
                    u.push(along);
                }
                if along > 0.0 {
                    u[start..].iter_mut().for_each(|x| *x /= along);
                }
                first += n as usize;
            }
            u.resize(pts.len(), 0.0);
            Some(Column::Float(Arc::new(u)))
        }
        (Face, "intrinsic:faceIndex") => int((0..counts.len() as i32).collect()),
        (Face, "intrinsic:corners") => int(counts.iter().map(|&c| c as i32).collect()),
        (Face, "intrinsic:firstCorner") => int(counts
            .iter()
            .scan(0i32, |at, &c| {
                let s = *at;
                *at += c as i32;
                Some(s)
            })
            .collect()),
        (Face, "intrinsic:N") | (Face, "intrinsic:area") => {
            let at = |c: u32| pts.get(corner_point[c as usize] as usize).copied().unwrap_or([0.0; 3]);
            let mut start = 0u32;
            // Newell's: twice the area, along the normal; flat or not.
            let area_normals: Vec<[f32; 3]> = counts
                .iter()
                .map(|&n| {
                    let mut s = [0.0f32; 3];
                    for k in 0..n {
                        let (a, b) = (at(start + k), at(start + (k + 1) % n));
                        s[0] += (a[1] - b[1]) * (a[2] + b[2]);
                        s[1] += (a[2] - b[2]) * (a[0] + b[0]);
                        s[2] += (a[0] - b[0]) * (a[1] + b[1]);
                    }
                    start += n;
                    if matches!(geo.topology, Topology::Mesh { left_handed: true, .. }) {
                        [-s[0], -s[1], -s[2]]
                    } else {
                        s
                    }
                })
                .collect();
            let len = |v: &[f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            Some(if name == "intrinsic:N" {
                Column::Vec3(Arc::new(
                    area_normals
                        .iter()
                        .map(|v| {
                            let l = len(v);
                            if l > 0.0 {
                                [v[0] / l, v[1] / l, v[2] / l]
                            } else {
                                [0.0; 3]
                            }
                        })
                        .collect(),
                ))
            } else {
                Column::Float(Arc::new(area_normals.iter().map(|v| len(v) * 0.5).collect()))
            })
        }
        (Face, "intrinsic:length") => {
            let mut first = 0usize;
            let lengths = counts
                .iter()
                .map(|&n| {
                    let curve = pts.get(first..first + n as usize).unwrap_or(&[]);
                    first += n as usize;
                    let open: f32 = curve.windows(2).map(|w| distance(w[0], w[1])).sum();
                    match curve {
                        [a, .., b] if closed && curve.len() > 2 => open + distance(*b, *a),
                        _ => open,
                    }
                })
                .collect();
            Some(Column::Float(Arc::new(lengths)))
        }
        (Corner, "intrinsic:ptIndex") => int(corner_point.iter().map(|&p| p as i32).collect()),
        (Corner, "intrinsic:faceIndex") => int(counts
            .iter()
            .enumerate()
            .flat_map(|(f, &c)| (0..c).map(move |_| f as i32))
            .collect()),
        (Corner, "intrinsic:localCornerIndex") => int(counts.iter().flat_map(|&c| 0..c as i32).collect()),
        (Object, "intrinsic:pointCount") => int(vec![pts.len() as i32]),
        (Object, "intrinsic:faceCount") => int(vec![geo.topology.primitive_count() as i32]),
        (Object, "intrinsic:cornerCount") => int(vec![geo.topology.corner_count() as i32]),
        (Object, "intrinsic:boundsMin") | (Object, "intrinsic:boundsMax") => {
            let (lo, hi) = geo.bounds().unwrap_or(([0.0; 3], [0.0; 3]));
            Some(Column::Vec3(Arc::new(vec![if name == "intrinsic:boundsMin" {
                lo
            } else {
                hi
            }])))
        }
        (Object, "intrinsic:transform") => {
            let identity = [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ];
            Some(Column::Mat4(Arc::new(vec![world.unwrap_or(identity)])))
        }
        (Object, "intrinsic:typeName") => Some(Column::Token(Arc::new(vec![type_name(geo).into()]))),
        _ => None,
    }
}

/// The edges of `geo`: each polygon edge once (by its points, the lower
/// first, as polygons first meet it), then each curve segment (closing a
/// periodic curve).
pub fn edges(geo: &Geo) -> Vec<[u32; 2]> {
    match &geo.topology {
        Topology::Points => vec![],
        Topology::Mesh { counts, indices, .. } => {
            let (mut seen, mut out) = (HashSet::new(), vec![]);
            let mut start = 0usize;
            for &n in counts.iter() {
                let face = &indices[start..start + n as usize];
                for k in 0..face.len() {
                    let (a, b) = (face[k], face[(k + 1) % face.len()]);
                    let e = [a.min(b), a.max(b)];
                    if a != b && seen.insert(e) {
                        out.push(e);
                    }
                }
                start += n as usize;
            }
            out
        }
        Topology::Curves { counts, wrap, .. } => {
            let mut out = vec![];
            let mut at = 0u32;
            for &n in counts.iter() {
                for k in 1..n {
                    out.push([at + k - 1, at + k]);
                }
                if *wrap == CurveWrap::Periodic && n > 2 {
                    out.push([at + n - 1, at]);
                }
                at += n;
            }
            out
        }
    }
}

/// The length of each edge of `edges`.
pub fn edge_lengths(geo: &Geo, edges: &[[u32; 2]]) -> Vec<f32> {
    let pts = geo.points();
    let at = |i: u32| pts.get(i as usize).copied().unwrap_or([0.0; 3]);
    edges.iter().map(|e| distance(at(e[0]), at(e[1]))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad_and_triangle() -> Geo {
        Geo::from_polygons(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
                [2.0, 0.0, 0.0],
            ],
            vec![4, 3],
            vec![0, 1, 2, 3, 1, 4, 2],
        )
    }

    fn two_curves(wrap: CurveWrap) -> Geo {
        let mut g = Geo::from_points(vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 3.0],
        ]);
        g.topology = Topology::curves(vec![3, 2], super::super::CurveBasis::Linear, wrap);
        g
    }

    /// Every name is an intrinsic's, listed once where it is.
    #[test]
    fn every_intrinsic_is_named_with_the_prefix_and_listed_once() {
        for (k, i) in ALL.iter().enumerate() {
            assert!(is_intrinsic(i.name), "{}", i.name);
            assert!(
                !ALL[..k].iter().any(|j| j.scope == i.scope && j.name == i.name),
                "{} twice",
                i.name
            );
        }
    }

    /// Every intrinsic a primitive has per element makes a column as long as
    /// its context has elements (path aside: the scene's to give).
    #[test]
    fn every_column_is_as_long_as_its_context() {
        let shapes = [
            quad_and_triangle(),
            two_curves(CurveWrap::Nonperiodic),
            Geo::from_points(vec![[0.0; 3], [1.0; 3]]),
        ];
        for g in &shapes {
            for ctx in Context::ALL {
                for i in of_geo(g, ctx).filter(|i| i.gathers.is_none() && i.name != "intrinsic:path") {
                    let c = column(g, ctx, i.name, None).unwrap_or_else(|| panic!("{} on {:?}", i.name, g.topology));
                    assert_eq!(c.kind(), i.kind, "{}", i.name);
                    assert_eq!(c.len(), g.count(ctx), "{} on {}", i.name, type_name(g));
                }
            }
        }
    }

    #[test]
    fn a_mesh_numbers_its_faces_and_corners() {
        let g = quad_and_triangle();
        let ints = |ctx, name| column(&g, ctx, name, None).unwrap().ints().unwrap().to_vec();
        assert_eq!(ints(Face, "intrinsic:corners"), vec![4, 3]);
        assert_eq!(ints(Face, "intrinsic:firstCorner"), vec![0, 4]);
        assert_eq!(ints(Corner, "intrinsic:faceIndex"), vec![0, 0, 0, 0, 1, 1, 1]);
        assert_eq!(ints(Corner, "intrinsic:localCornerIndex"), vec![0, 1, 2, 3, 0, 1, 2]);
        assert_eq!(ints(Object, "intrinsic:cornerCount"), vec![7]);
        let area = column(&g, Face, "intrinsic:area", None).unwrap();
        assert_eq!(area.floats().unwrap(), &[1.0, 0.5]);
        assert!(
            on(&g, Face, "intrinsic:length").is_none(),
            "a polygon has an area, not a length"
        );
        assert_eq!(edges(&g).len(), 6);
    }

    #[test]
    fn curves_have_a_length_and_a_u_along_them() {
        let open = two_curves(CurveWrap::Nonperiodic);
        let length = |g: &Geo| {
            column(g, Face, "intrinsic:length", None)
                .unwrap()
                .floats()
                .unwrap()
                .to_vec()
        };
        assert_eq!(length(&open), vec![2.0, 2.0]);
        let u = column(&open, Point, "intrinsic:curveu", None).unwrap();
        assert_eq!(u.floats().unwrap(), &[0.0, 0.5, 1.0, 0.0, 1.0]);
        // Closed, the first curve comes back to its start; two points do not
        // close.
        let closed = two_curves(CurveWrap::Periodic);
        assert!((length(&closed)[0] - (2.0 + 2f32.sqrt())).abs() < 1e-6);
        assert_eq!(length(&closed)[1], 2.0);
        assert_eq!(edges(&closed).len(), 3 + 1);
        assert!(on(&open, Face, "intrinsic:area").is_none());
        assert_eq!(
            gathered("intrinsic:ptIndices", Some(Face)),
            Some((Point, "intrinsic:ptIndex"))
        );
        assert_eq!(
            gathered("intrinsic:faceIndices", None),
            Some((Face, "intrinsic:faceIndex"))
        );
    }

    #[test]
    fn the_object_has_its_counts_bounds_and_placement() {
        let g = quad_and_triangle();
        let vec3 = |name| column(&g, Object, name, None).unwrap().vec3s().unwrap()[0];
        assert_eq!(vec3("intrinsic:boundsMin"), [0.0, 0.0, 0.0]);
        assert_eq!(vec3("intrinsic:boundsMax"), [2.0, 1.0, 0.0]);
        let mut placed = [0.0f64; 16];
        placed[0] = 2.0;
        let m = column(&g, Object, "intrinsic:transform", Some(placed)).unwrap();
        assert_eq!(m.mat4s().unwrap()[0][0], 2.0);
        assert!(column(&g, Object, "intrinsic:path", None).is_none());
        assert_eq!(type_name(&g), "Mesh");
        assert!(!get(In(Object), "intrinsic:path").unwrap().ice);
    }
}
