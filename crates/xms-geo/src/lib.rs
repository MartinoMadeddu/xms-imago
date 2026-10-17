//! Geometry as typed attribute columns per context.
//!
//! The one geometry model for packed primitives, the nodes and ICE. A `Geo`
//! is a topology (points, polygons or curves) and, for each context, a set of
//! named attributes. Contexts are where an attribute's values live, and match
//! USD's primvar interpolations:
//!
//! | `Context`   | values per        | USD interpolation |
//! |-------------|-------------------|-------------------|
//! | `Object`    | one for the prim  | `constant`        |
//! | `Point`     | point             | `vertex`          |
//! | `Face`      | polygon or curve  | `uniform`         |
//! | `Corner`    | polygon corner    | `faceVarying`     |
//!
//! Columns are shared: cloning a `Geo` copies pointers, and writing to a
//! column (through `Arc::make_mut`) copies that column only. So a node that
//! moves the points of a mesh with forty attributes copies one column, and
//! an attribute still pointing at the column it was loaded with is, by that
//! fact, unedited: edits are what no longer shares with the source.
//!
//! Names follow USD (`points`, `normals`, `st`, `displayColor`, `widths`, and
//! a primvar's own name for the rest), so reading and writing USD is a
//! one-to-one mapping. Interfaces can show friendlier aliases on top.
//!
//! Its own crate, depending on nothing: the data model the app, the ICE
//! engine (`xms-ice`) and any other tool share. The app converts it to and
//! from its `MeshData` (`GeoMesh` in the app).

// Some of the API (subdivision schemes, quaternion and matrix columns) is
// for what reads and writes USD next.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::Arc;


// ── Names ───────────────────────────────────────────────────────────────────

/// Point positions (USD `points`).
pub const POINTS: &str = "points";
/// Normals (USD `normals`, or `primvars:normals`).
pub const NORMALS: &str = "normals";
/// Texture coordinates (USD `primvars:st`).
pub const ST: &str = "st";
/// Point and curve widths (USD `widths`).
pub const WIDTHS: &str = "widths";
/// Names texture coordinates go by, `st` first (USD's convention).
pub const UV_NAMES: [&str; 5] = [ST, "st0", "UVMap", "map1", "uv"];

// ── Curves ──────────────────────────────────────────────────────────────────

/// How curves pass through their control points (USD `basis`, with `type`
/// linear as `Linear`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CurveBasis {
    #[default]
    Linear,
    Bezier,
    Bspline,
    CatmullRom,
}

/// How curves end (USD `wrap`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CurveWrap {
    #[default]
    Nonperiodic,
    Periodic,
    Pinned,
}

// ── Context ─────────────────────────────────────────────────────────────────

/// Where an attribute's values live.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Context {
    Object,
    Point,
    /// A polygon, or a curve: one value per face. (Not a USD primitive:
    /// one value per primitive is `Object`.)
    Face,
    Corner,
}

impl Context {
    pub const ALL: [Context; 4] = [Context::Object, Context::Point, Context::Face, Context::Corner];

    #[inline]
    fn slot(self) -> usize {
        match self { Context::Object => 0, Context::Point => 1, Context::Face => 2, Context::Corner => 3 }
    }

    /// The USD interpolation token for writing.
    pub fn usd_interpolation(self) -> &'static str {
        match self {
            Context::Object => "constant",
            Context::Point => "vertex",
            Context::Face => "uniform",
            Context::Corner => "faceVarying",
        }
    }

    /// From a USD interpolation token. `varying` reads as per point: for
    /// polygons and points it has as many values as there are points.
    pub fn from_usd(token: &str) -> Option<Context> {
        Some(match token {
            "constant" => Context::Object,
            "vertex" | "varying" => Context::Point,
            "uniform" => Context::Face,
            "faceVarying" => Context::Corner,
            _ => return None,
        })
    }
}

// ── Columns ─────────────────────────────────────────────────────────────────

/// The type of a column's values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Bool, Int, Float, Vec2, Vec3, Vec4, Quat, Mat4, Token }

/// A typed, shared column of values.
#[derive(Clone, Debug)]
pub enum Column {
    Bool(Arc<Vec<bool>>),
    Int(Arc<Vec<i32>>),
    Float(Arc<Vec<f32>>),
    Vec2(Arc<Vec<[f32; 2]>>),
    Vec3(Arc<Vec<[f32; 3]>>),
    Vec4(Arc<Vec<[f32; 4]>>),
    /// (i, j, k, real), as USD stores quaternions.
    Quat(Arc<Vec<[f32; 4]>>),
    /// Row-major, as USD.
    Mat4(Arc<Vec<[f64; 16]>>),
    Token(Arc<Vec<Arc<str>>>),
}

macro_rules! column_access {
    ($get:ident, $get_mut:ident, $variant:ident, $t:ty) => {
        /// The values, when the column is of this type.
        #[inline]
        pub fn $get(&self) -> Option<&[$t]> {
            match self { Column::$variant(v) => Some(v), _ => None }
        }
        /// The values to write, when the column is of this type. Copies them
        /// first if they are shared.
        pub fn $get_mut(&mut self) -> Option<&mut Vec<$t>> {
            match self { Column::$variant(v) => Some(Arc::make_mut(v)), _ => None }
        }
    };
}

impl Column {
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            Column::Bool(v) => v.len(),
            Column::Int(v) => v.len(),
            Column::Float(v) => v.len(),
            Column::Vec2(v) => v.len(),
            Column::Vec3(v) => v.len(),
            Column::Vec4(v) => v.len(),
            Column::Quat(v) => v.len(),
            Column::Mat4(v) => v.len(),
            Column::Token(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool { self.len() == 0 }

    #[inline]
    pub fn kind(&self) -> Kind {
        match self {
            Column::Bool(_) => Kind::Bool,
            Column::Int(_) => Kind::Int,
            Column::Float(_) => Kind::Float,
            Column::Vec2(_) => Kind::Vec2,
            Column::Vec3(_) => Kind::Vec3,
            Column::Vec4(_) => Kind::Vec4,
            Column::Quat(_) => Kind::Quat,
            Column::Mat4(_) => Kind::Mat4,
            Column::Token(_) => Kind::Token,
        }
    }

    /// Whether two columns are the very same data (not merely equal values).
    /// An attribute whose column is still the source's is unedited.
    pub fn same(&self, other: &Column) -> bool {
        match (self, other) {
            (Column::Bool(a), Column::Bool(b)) => Arc::ptr_eq(a, b),
            (Column::Int(a), Column::Int(b)) => Arc::ptr_eq(a, b),
            (Column::Float(a), Column::Float(b)) => Arc::ptr_eq(a, b),
            (Column::Vec2(a), Column::Vec2(b)) => Arc::ptr_eq(a, b),
            (Column::Vec3(a), Column::Vec3(b)) => Arc::ptr_eq(a, b),
            (Column::Vec4(a), Column::Vec4(b)) => Arc::ptr_eq(a, b),
            (Column::Quat(a), Column::Quat(b)) => Arc::ptr_eq(a, b),
            (Column::Mat4(a), Column::Mat4(b)) => Arc::ptr_eq(a, b),
            (Column::Token(a), Column::Token(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    column_access!(bools, bools_mut, Bool, bool);
    column_access!(ints, ints_mut, Int, i32);
    column_access!(floats, floats_mut, Float, f32);
    column_access!(vec2s, vec2s_mut, Vec2, [f32; 2]);
    column_access!(vec3s, vec3s_mut, Vec3, [f32; 3]);
    column_access!(vec4s, vec4s_mut, Vec4, [f32; 4]);
    column_access!(quats, quats_mut, Quat, [f32; 4]);
    column_access!(mat4s, mat4s_mut, Mat4, [f64; 16]);
    column_access!(tokens, tokens_mut, Token, Arc<str>);
}

// ── Attributes ──────────────────────────────────────────────────────────────

/// What an attribute's values mean, for writing USD (`point3f` against
/// `normal3f`, `color3f`, `texCoord2f`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    None,
    Point,
    Normal,
    Vector,
    Color,
    TexCoord,
}

/// A named attribute: its values, and optionally indices into them (USD
/// indexed primvars, where shared values such as UVs on a seam are stored
/// once). With indices, the attribute has one index per element of its
/// context; without, one value.
#[derive(Clone, Debug)]
pub struct Attr {
    pub column:  Column,
    pub indices: Option<Arc<Vec<u32>>>,
    pub role:    Role,
}

impl Attr {
    pub fn new(column: Column, role: Role) -> Self {
        Self { column, indices: None, role }
    }

    pub fn indexed(column: Column, indices: Vec<u32>, role: Role) -> Self {
        Self { column, indices: Some(Arc::new(indices)), role }
    }

    /// How many elements of its context the attribute covers.
    #[inline]
    pub fn len(&self) -> usize {
        self.indices.as_ref().map_or(self.column.len(), |i| i.len())
    }

    pub fn is_empty(&self) -> bool { self.len() == 0 }

    /// The value index for an element of the context.
    #[inline]
    pub fn value_index(&self, element: usize) -> Option<usize> {
        match &self.indices {
            Some(ix) => ix.get(element).map(|&i| i as usize),
            None => Some(element),
        }
    }

    /// Whether this is still the very data of `other` (values and indices).
    pub fn same(&self, other: &Attr) -> bool {
        self.column.same(&other.column)
            && match (&self.indices, &other.indices) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }
}

// ── Topology ────────────────────────────────────────────────────────────────

/// How a mesh is subdivided when rendered (USD `subdivisionScheme`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Subdiv {
    /// USD's default.
    #[default]
    CatmullClark,
    Loop,
    Bilinear,
    None,
}

/// What the points make up.
#[derive(Clone, Debug, Default)]
pub enum Topology {
    #[default]
    Points,
    /// Polygons: corners per polygon, and the point of each corner.
    /// `left_handed`: corners wind clockwise (USD `orientation`).
    Mesh {
        counts:      Arc<Vec<u32>>,
        indices:     Arc<Vec<u32>>,
        left_handed: bool,
        subdiv:      Subdiv,
    },
    /// Curves: control points per curve, end to end through the points.
    Curves {
        counts: Arc<Vec<u32>>,
        basis:  CurveBasis,
        wrap:   CurveWrap,
    },
}

impl Topology {
    pub fn mesh(counts: Vec<u32>, indices: Vec<u32>) -> Self {
        Topology::Mesh { counts: Arc::new(counts), indices: Arc::new(indices), left_handed: false, subdiv: Subdiv::default() }
    }

    pub fn curves(counts: Vec<u32>, basis: CurveBasis, wrap: CurveWrap) -> Self {
        Topology::Curves { counts: Arc::new(counts), basis, wrap }
    }

    /// Polygons or curves.
    pub fn primitive_count(&self) -> usize {
        match self {
            Topology::Points => 0,
            Topology::Mesh { counts, .. } | Topology::Curves { counts, .. } => counts.len(),
        }
    }

    /// Polygon corners (curve control points, for curves).
    pub fn corner_count(&self) -> usize {
        match self {
            Topology::Points => 0,
            Topology::Mesh { indices, .. } => indices.len(),
            Topology::Curves { counts, .. } => counts.iter().map(|&c| c as usize).sum(),
        }
    }

    /// Whether two topologies are the very same data.
    pub fn same(&self, other: &Topology) -> bool {
        match (self, other) {
            (Topology::Points, Topology::Points) => true,
            (Topology::Mesh { counts: a, indices: ai, left_handed: al, subdiv: asd },
             Topology::Mesh { counts: b, indices: bi, left_handed: bl, subdiv: bsd }) =>
                Arc::ptr_eq(a, b) && Arc::ptr_eq(ai, bi) && al == bl && asd == bsd,
            (Topology::Curves { counts: a, basis: ab, wrap: aw }, Topology::Curves { counts: b, basis: bb, wrap: bw }) =>
                Arc::ptr_eq(a, b) && ab == bb && aw == bw,
            _ => false,
        }
    }
}

// ── Geo ─────────────────────────────────────────────────────────────────────

/// A topology and its attributes, per context.
#[derive(Clone, Debug, Default)]
pub struct Geo {
    pub topology: Topology,
    attrs: [BTreeMap<Arc<str>, Attr>; 4],
}

impl Geo {
    /// Points only.
    pub fn from_points(points: Vec<[f32; 3]>) -> Self {
        let mut g = Geo::default();
        g.set(Context::Point, POINTS, Attr::new(Column::Vec3(Arc::new(points)), Role::Point));
        g
    }

    /// Polygons: corners per polygon and the point of each corner.
    pub fn from_polygons(points: Vec<[f32; 3]>, counts: Vec<u32>, indices: Vec<u32>) -> Self {
        let mut g = Geo::from_points(points);
        g.topology = Topology::mesh(counts, indices);
        g
    }

    // ── Attributes ─────────────────────────────────────────────────────────

    #[inline]
    pub fn attr(&self, ctx: Context, name: &str) -> Option<&Attr> {
        self.attrs[ctx.slot()].get(name)
    }

    #[inline]
    pub fn attr_mut(&mut self, ctx: Context, name: &str) -> Option<&mut Attr> {
        self.attrs[ctx.slot()].get_mut(name)
    }

    pub fn set(&mut self, ctx: Context, name: &str, attr: Attr) {
        self.attrs[ctx.slot()].insert(Arc::from(name), attr);
    }

    pub fn remove(&mut self, ctx: Context, name: &str) -> Option<Attr> {
        self.attrs[ctx.slot()].remove(name)
    }

    /// The attributes of a context, by name.
    pub fn attrs(&self, ctx: Context) -> impl Iterator<Item = (&str, &Attr)> {
        self.attrs[ctx.slot()].iter().map(|(k, v)| (&**k, v))
    }

    /// Where an attribute is, in whichever context has it.
    pub fn find(&self, name: &str) -> Option<(Context, &Attr)> {
        Context::ALL.into_iter().find_map(|c| self.attr(c, name).map(|a| (c, a)))
    }

    // ── Points ─────────────────────────────────────────────────────────────

    #[inline]
    pub fn points(&self) -> &[[f32; 3]] {
        self.attr(Context::Point, POINTS).and_then(|a| a.column.vec3s()).unwrap_or(&[])
    }

    /// The points to write: copied first if shared.
    pub fn points_mut(&mut self) -> &mut Vec<[f32; 3]> {
        if self.attr(Context::Point, POINTS).and_then(|a| a.column.vec3s()).is_none() {
            self.set(Context::Point, POINTS, Attr::new(Column::Vec3(Arc::new(vec![])), Role::Point));
        }
        self.attr_mut(Context::Point, POINTS).and_then(|a| a.column.vec3s_mut()).expect("points are a Vec3 column")
    }

    #[inline]
    pub fn point_count(&self) -> usize { self.points().len() }

    /// How many elements a context has.
    #[inline]
    pub fn count(&self, ctx: Context) -> usize {
        match ctx {
            Context::Object => 1,
            Context::Point => self.point_count(),
            Context::Face => self.topology.primitive_count(),
            Context::Corner => self.topology.corner_count(),
        }
    }

    /// Low and high corners of the points.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut it = self.points().iter();
        let first = *it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| {
            ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
        }))
    }

    /// Every attribute covers its context, every index points at a value,
    /// and the topology points at existing points.
    pub fn validate(&self) -> Result<(), String> {
        let points = self.point_count();
        match &self.topology {
            Topology::Points => {}
            Topology::Mesh { counts, indices, .. } => {
                let corners: usize = counts.iter().map(|&c| c as usize).sum();
                if corners != indices.len() {
                    return Err(format!("polygon corners: counts add up to {corners}, there are {} indices", indices.len()));
                }
                if let Some(bad) = indices.iter().find(|&&i| i as usize >= points) {
                    return Err(format!("polygon corner points at point {bad}, there are {points} points"));
                }
            }
            Topology::Curves { counts, .. } => {
                let cvs: usize = counts.iter().map(|&c| c as usize).sum();
                if cvs > points {
                    return Err(format!("curves need {cvs} points, there are {points}"));
                }
            }
        }
        for ctx in Context::ALL {
            let expected = self.count(ctx);
            for (name, attr) in self.attrs(ctx) {
                if attr.len() != expected {
                    return Err(format!("{name} ({ctx:?}) has {} elements, {expected} expected", attr.len()));
                }
                if let Some(ix) = &attr.indices {
                    let values = attr.column.len();
                    if let Some(bad) = ix.iter().find(|&&i| i as usize >= values) {
                        return Err(format!("{name}: index {bad} past its {values} values"));
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit quad split from a triangle: two polygons sharing an edge.
    fn quad_and_triangle() -> Geo {
        let points = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [2.0, 0.0, 0.0]];
        let mut g = Geo::from_polygons(points, vec![4, 3], vec![0, 1, 2, 3, 1, 4, 2]);
        let st = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.5, 0.0], [0.9, 0.0], [0.5, 0.5]];
        g.set(Context::Corner, ST, Attr::new(Column::Vec2(Arc::new(st)), Role::TexCoord));
        g
    }

    #[test]
    fn contexts_are_usd_interpolations() {
        for ctx in Context::ALL {
            assert_eq!(Context::from_usd(ctx.usd_interpolation()), Some(ctx));
        }
        assert_eq!(Context::from_usd("varying"), Some(Context::Point));
    }

    #[test]
    fn counts_follow_the_topology_and_validate_checks_them() {
        let g = quad_and_triangle();
        assert_eq!((g.count(Context::Object), g.count(Context::Point), g.count(Context::Face), g.count(Context::Corner)), (1, 5, 2, 7));
        assert!(g.validate().is_ok());
        let mut bad = g.clone();
        bad.set(Context::Face, "id", Attr::new(Column::Int(Arc::new(vec![1, 2, 3])), Role::None));
        assert!(bad.validate().is_err());
    }

    #[test]
    fn writing_copies_only_what_is_written() {
        let source = quad_and_triangle();
        let mut edited = source.clone();
        edited.points_mut()[0][2] = 1.0;
        let (a, b) = (source.attr(Context::Point, POINTS).unwrap(), edited.attr(Context::Point, POINTS).unwrap());
        assert!(!a.same(b), "the written column is a copy");
        assert_eq!(source.points()[0][2], 0.0, "the source is untouched");
        assert!(source.attr(Context::Corner, ST).unwrap().same(edited.attr(Context::Corner, ST).unwrap()), "the rest is still shared");
        assert!(source.topology.same(&edited.topology));
    }

    #[test]
    fn indexed_attributes_store_shared_values_once() {
        let mut g = quad_and_triangle();
        // Two colours for seven corners.
        g.set(Context::Corner, "displayColor", Attr::indexed(
            Column::Vec3(Arc::new(vec![[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]])), vec![0, 0, 0, 0, 1, 1, 1], Role::Color));
        assert!(g.validate().is_ok());
        let a = g.attr(Context::Corner, "displayColor").unwrap();
        assert_eq!((a.len(), a.column.len(), a.value_index(5)), (7, 2, Some(1)));
    }
}
