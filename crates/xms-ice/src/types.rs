//! The types of values in an ICE graph.

use glam::{Vec2, Vec3, Vec4};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Ty {
    Bool,
    Int,
    Float,
    Vec2,
    Vec3,
    Vec4,
}

impl Ty {
    pub fn is_vector(self) -> bool { matches!(self, Ty::Vec2 | Ty::Vec3 | Ty::Vec4) }
    pub fn is_numeric(self) -> bool { self != Ty::Bool }
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
}
