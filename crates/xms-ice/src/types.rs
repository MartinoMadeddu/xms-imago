//! The types of values in an ICE graph.

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Ty {
    Bool,
    Int,
    Float,
    Vec2,
    Vec3,
    Vec4,
    /// A rotation (i, j, k, real), as USD's `quatf`: made from an axis and an
    /// angle or from Euler angles, combined by multiplying, applied to vectors.
    Quat,
    /// A 4×4 matrix: a frame (three axes and an origin) or any transform,
    /// as USD's `matrix4d` holds it (stored column-major: the columns are
    /// the X, Y and Z axes and the origin, as USD's rows are).
    Mat4,
    /// A surface sample: a place on a surface, where Scatter on Surface
    /// puts points (its third output) or where Closest Point on Surface
    /// finds the nearest, read by Get Data at Samples. Not a USD type: what
    /// reaches USD is positions and normals.
    #[serde(alias = "Location")]
    Sample,
    /// A name or a path: the primitives of a reference input (Get
    /// Reference), only ever as an array.
    Token,
}

/// A surface sample (`Ty::Sample`), a place on a mesh's surface: a face, the three corners of the triangle
/// of it the place is in (the face's fan from its first corner), and the
/// place's weights on those corners (they sum to 1). `mesh`: which mesh of
/// the surface it is on (0 for the geometry the graph runs on, Scatter's;
/// a reference mesh's number for Closest Point on Surface's); `NONE` for a
/// sample that found nothing, which reads as zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Loc {
    pub face:    u32,
    pub corners: [u32; 3],
    pub weights: [f32; 3],
    #[serde(default)]
    pub mesh:    u32,
}

impl Loc {
    /// A sample on nothing: nothing was found.
    pub const NONE: u32 = u32::MAX;
    pub fn found(&self) -> bool { self.mesh != Loc::NONE }
}

impl Ty {
    pub fn is_vector(self) -> bool { matches!(self, Ty::Vec2 | Ty::Vec3 | Ty::Vec4) }
    /// Added, subtracted, scaled: a number or a vector, not a bool or a rotation.
    pub fn is_numeric(self) -> bool { !matches!(self, Ty::Bool | Ty::Quat | Ty::Mat4 | Ty::Sample | Ty::Token) }

    /// How the editor names it: on ports, in errors.
    pub fn name(self) -> &'static str {
        match self {
            Ty::Bool => "boolean",
            Ty::Int => "integer",
            Ty::Float => "scalar",
            Ty::Vec2 => "2D vector",
            Ty::Vec3 => "3D vector",
            Ty::Vec4 => "4D vector",
            Ty::Quat => "rotation",
            Ty::Mat4 => "matrix",
            Ty::Sample => "surface sample",
            Ty::Token => "path",
        }
    }
}

/// One value: a constant, a parameter, or what an unconnected input holds.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Value {
    Bool(bool),
    Int(i32),
    Float(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    /// i, j, k, real.
    Quat([f32; 4]),
    /// Column by column: the X, Y and Z axes and the origin.
    Mat4([f32; 16]),
    #[serde(alias = "Location")]
    Sample(Loc),
}

impl Value {
    pub fn ty(&self) -> Ty {
        match self {
            Value::Bool(_) => Ty::Bool,
            Value::Int(_) => Ty::Int,
            Value::Float(_) => Ty::Float,
            Value::Vec2(_) => Ty::Vec2,
            Value::Vec3(_) => Ty::Vec3,
            Value::Vec4(_) => Ty::Vec4,
            Value::Quat(_) => Ty::Quat,
            Value::Mat4(_) => Ty::Mat4,
            Value::Sample(_) => Ty::Sample,
        }
    }

    /// This value as `ty`, where that makes sense: an int as a float, a
    /// float or an int spread over a vector's components. An unconnected
    /// input takes the type of what it meets this way.
    pub fn convert(self, ty: Ty) -> Option<Value> {
        let f = match self {
            Value::Int(i) => Some(i as f32),
            Value::Float(f) => Some(f),
            _ => None,
        };
        Some(match (ty, f) {
            _ if self.ty() == ty => self,
            (Ty::Float, Some(f)) => Value::Float(f),
            (Ty::Vec2, Some(f)) => Value::Vec2([f; 2]),
            (Ty::Vec3, Some(f)) => Value::Vec3([f; 3]),
            (Ty::Vec4, Some(f)) => Value::Vec4([f; 4]),
            _ => return None,
        })
    }

    pub(crate) fn as_f32(&self) -> f32 { if let Value::Float(f) = self { *f } else { 0.0 } }
    pub(crate) fn as_i32(&self) -> i32 { if let Value::Int(i) = self { *i } else { 0 } }
    pub(crate) fn as_bool(&self) -> bool { if let Value::Bool(b) = self { *b } else { false } }
    pub(crate) fn as_vec2(&self) -> Vec2 { if let Value::Vec2(v) = self { Vec2::from_array(*v) } else { Vec2::ZERO } }
    pub(crate) fn as_vec3(&self) -> Vec3 { if let Value::Vec3(v) = self { Vec3::from_array(*v) } else { Vec3::ZERO } }
    pub(crate) fn as_vec4(&self) -> Vec4 { if let Value::Vec4(v) = self { Vec4::from_array(*v) } else { Vec4::ZERO } }
    pub(crate) fn as_quat(&self) -> Quat { if let Value::Quat(q) = self { Quat::from_array(*q) } else { Quat::IDENTITY } }
    pub(crate) fn as_mat4(&self) -> Mat4 { if let Value::Mat4(m) = self { Mat4::from_cols_array(m) } else { Mat4::IDENTITY } }
    pub(crate) fn as_loc(&self) -> Loc { if let Value::Sample(l) = self { *l } else { Loc::default() } }
}
