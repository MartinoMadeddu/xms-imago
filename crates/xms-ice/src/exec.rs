//! A plan run over a `Geo`.
//!
//! The uniform program runs once. The elements are then cut into batches,
//! run in parallel: each thread keeps its registers (one batch long) from
//! batch to batch, reads its slice of the columns the plan reads, and writes
//! its slice of the columns the plan sets. Nothing else is read or copied:
//! the result shares every other column with the input.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Quat, Vec2, Vec3, Vec4};
use rayon::prelude::*;

use xms_geo::{Attr, Column, Context, Geo, Topology};

use super::compile::{InstancerPlan, Ins, Key, Plan, Program, Write, NO_COUNT};
use super::graph::{ArrayOp, Compose, Math, Reduce};
use super::types::{Loc, Ty, Value};

/// Elements per batch: long enough that each instruction's loop does real
/// work, short enough that a thread's registers stay in cache.
pub const BATCH: usize = 4096;

/// One register: a value per element of the batch; in the uniform program
/// one value, or an array's values.
pub(crate) enum Reg {
    Bool(Vec<bool>),
    Int(Vec<i32>),
    Float(Vec<f32>),
    Vec2(Vec<Vec2>),
    Vec3(Vec<Vec3>),
    Vec4(Vec<Vec4>),
    Quat(Vec<Quat>),
    Sample(Vec<Loc>),
    /// Paths: only ever an array, in the uniform program.
    Token(Vec<Arc<str>>),
    /// A list per element (Get Array Data): element `e`'s items are
    /// `items[offsets[e]..offsets[e + 1]]`.
    List(Vec<u32>, Box<Reg>),
}

impl Reg {
    fn len(&self) -> usize {
        match self {
            Reg::Bool(v) => v.len(),
            Reg::Int(v) => v.len(),
            Reg::Float(v) => v.len(),
            Reg::Vec2(v) => v.len(),
            Reg::Vec3(v) => v.len(),
            Reg::Vec4(v) => v.len(),
            Reg::Quat(v) => v.len(),
            Reg::Sample(v) => v.len(),
            Reg::Token(v) => v.len(),
            Reg::List(offsets, _) => offsets.len().saturating_sub(1),
        }
    }
}

macro_rules! reg_access {
    ($name:ident, $variant:ident, $t:ty) => {
        /// The register as `$t`s, emptied, ready to be filled.
        fn $name(reg: &mut Reg) -> &mut Vec<$t> {
            if !matches!(reg, Reg::$variant(_)) { *reg = Reg::$variant(Vec::with_capacity(BATCH)); }
            let Reg::$variant(v) = reg else { unreachable!() };
            v.clear();
            v
        }
    };
}
reg_access!(bools, Bool, bool);
reg_access!(ints, Int, i32);
reg_access!(floats, Float, f32);
reg_access!(vec2s, Vec2, Vec2);
reg_access!(vec3s, Vec3, Vec3);
reg_access!(vec4s, Vec4, Vec4);
reg_access!(quats, Quat, Quat);
reg_access!(locs, Sample, Loc);
reg_access!(tokens, Token, Arc<str>);

fn registers(p: &Program) -> Vec<Reg> {
    p.regs.iter().map(|t| match t {
        Ty::Bool => Reg::Bool(vec![]),
        Ty::Int => Reg::Int(vec![]),
        Ty::Float => Reg::Float(vec![]),
        Ty::Vec2 => Reg::Vec2(vec![]),
        Ty::Vec3 => Reg::Vec3(vec![]),
        Ty::Vec4 => Reg::Vec4(vec![]),
        Ty::Quat => Reg::Quat(vec![]),
        Ty::Sample => Reg::Sample(vec![]),
        Ty::Token => Reg::Token(vec![]),
    }).collect()
}

/// The value at element `i` (zero, or no rotation, past the end): element
/// 0 of a uniform register is its value.
fn value_at(reg: &Reg, i: usize) -> Value {
    match reg {
        Reg::Bool(v) => Value::Bool(v.get(i).copied().unwrap_or_default()),
        Reg::Int(v) => Value::Int(v.get(i).copied().unwrap_or_default()),
        Reg::Float(v) => Value::Float(v.get(i).copied().unwrap_or_default()),
        Reg::Vec2(v) => Value::Vec2(v.get(i).copied().unwrap_or_default().to_array()),
        Reg::Vec3(v) => Value::Vec3(v.get(i).copied().unwrap_or_default().to_array()),
        Reg::Vec4(v) => Value::Vec4(v.get(i).copied().unwrap_or_default().to_array()),
        Reg::Quat(v) => Value::Quat(v.get(i).copied().unwrap_or_default().to_array()),
        Reg::Sample(v) => Value::Sample(v.get(i).copied().unwrap_or_default()),
        Reg::Token(_) => unreachable!("a path is never one value: the compiler refuses"),
        Reg::List(..) => unreachable!("a list is never one value: the compiler refuses"),
    }
}

fn fill(reg: &mut Reg, value: Value, len: usize) {
    match value.ty() {
        Ty::Bool => bools(reg).resize(len, value.as_bool()),
        Ty::Int => ints(reg).resize(len, value.as_i32()),
        Ty::Float => floats(reg).resize(len, value.as_f32()),
        Ty::Vec2 => vec2s(reg).resize(len, value.as_vec2()),
        Ty::Vec3 => vec3s(reg).resize(len, value.as_vec3()),
        Ty::Vec4 => vec4s(reg).resize(len, value.as_vec4()),
        Ty::Quat => quats(reg).resize(len, value.as_quat()),
        Ty::Sample => locs(reg).resize(len, value.as_loc()),
        Ty::Token => unreachable!("a path is never one value: the compiler refuses"),
    }
}

/// A column the plan reads, its indices when it is indexed, and how the
/// plan's elements reach it.
struct Source<'a> {
    column:  Option<&'a Column>,
    indices: Option<&'a [u32]>,
    map:     Arc<Map>,
    reduce:  Reduce,
    /// The context the column is in.
    context: Context,
}

/// How an element of the plan's context reaches the elements of the
/// context an attribute is in.
pub(crate) enum Map {
    /// The same element (or element 0 of an Object attribute).
    Direct,
    /// One element each: a corner's point, a corner's face.
    One(Vec<u32>),
    /// Several each, `items[offsets[e]..offsets[e + 1]]`: a face's corners
    /// or points, the faces or corners around a point.
    Many { offsets: Vec<u32>, items: Vec<u32> },
}

/// Offsets of a list of counts: 0, then each running total.
fn offsets(counts: impl Iterator<Item = usize>) -> Vec<u32> {
    let mut out = vec![0u32];
    let mut total = 0u32;
    for c in counts { total += c as u32; out.push(total); }
    out
}

/// The corners of each face (a polygon, or a curve's control points) and
/// the point of each corner.
fn faces(geo: &Geo) -> (Vec<u32>, Vec<u32>) {
    match &geo.topology {
        Topology::Points => (vec![0], vec![]),
        Topology::Mesh { counts, indices, .. } => (offsets(counts.iter().map(|&c| c as usize)), indices.to_vec()),
        // A curve's control points are its points, in order.
        Topology::Curves { counts, .. } => {
            let off = offsets(counts.iter().map(|&c| c as usize));
            let n = *off.last().unwrap_or(&0);
            (off, (0..n).collect())
        }
    }
}

/// Lists by element of `count` elements, from (element, item) pairs.
fn group(count: usize, pairs: impl Iterator<Item = (u32, u32)> + Clone) -> Map {
    let mut counts = vec![0usize; count];
    for (e, _) in pairs.clone() { if let Some(c) = counts.get_mut(e as usize) { *c += 1; } }
    let offsets = offsets(counts.iter().copied());
    let mut fill: Vec<u32> = offsets[..count].to_vec();
    let mut items = vec![0u32; *offsets.last().unwrap_or(&0) as usize];
    for (e, i) in pairs {
        if let Some(f) = fill.get_mut(e as usize) { items[*f as usize] = i; *f += 1; }
    }
    Map::Many { offsets, items }
}

/// How the elements of `to` reach those of `from`, on `geo`.
pub(crate) fn mapping(geo: &Geo, to: Context, from: Context) -> Map {
    use Context::*;
    if to == from || from == Object { return Map::Direct; }
    // Everything, for the one Object element.
    if to == Object {
        return Map::Many { offsets: vec![0, geo.count(from) as u32], items: (0..geo.count(from) as u32).collect() };
    }
    let (face_off, corner_point) = faces(geo);
    // The face of each corner: made only for the mappings that need it.
    let corner_face = |face_off: &[u32]| -> Vec<u32> {
        let n_faces = face_off.len().saturating_sub(1);
        (0..n_faces).flat_map(|f| (face_off[f]..face_off[f + 1]).map(move |_| f as u32)).collect()
    };
    let n = geo.count(to);
    match (to, from) {
        (Corner, Point) => Map::One(corner_point),
        (Corner, Face) => Map::One(corner_face(&face_off)),
        (Face, Corner) => Map::Many { items: (0..*face_off.last().unwrap_or(&0)).collect(), offsets: face_off },
        (Face, Point) => Map::Many { offsets: face_off, items: corner_point },
        (Point, Corner) => group(n, corner_point.iter().enumerate().map(|(c, &p)| (p, c as u32))),
        (Point, Face) => {
            let corner_face = corner_face(&face_off);
            group(n, corner_point.iter().zip(&corner_face).map(|(&p, &f)| (p, f)))
        }
        _ => Map::Direct,
    }
}

/// Elements `start..start + len` of `src` (through `indices` when there are
/// some), converted; missing values are `zero`.
fn gather<T: Copy, U: Copy>(src: &[T], indices: Option<&[u32]>, start: usize, len: usize, out: &mut Vec<U>, zero: U, conv: impl Fn(T) -> U) {
    match indices {
        None if start + len <= src.len() => out.extend(src[start..start + len].iter().map(|&v| conv(v))),
        _ => out.extend((start..start + len).map(|e| {
            let i = indices.map_or(Some(e), |ix| ix.get(e).map(|&i| i as usize));
            i.and_then(|i| src.get(i)).map_or(zero, |&v| conv(v))
        })),
    }
}

fn load(s: &Source, start: usize, len: usize, reg: &mut Reg, ty: Ty) {
    let ix = s.indices;
    match &*s.map {
        Map::Direct => match (s.column, ty) {
            (Some(Column::Bool(v)), _) => gather(v, ix, start, len, bools(reg), false, |x| x),
            (Some(Column::Int(v)), _) => gather(v, ix, start, len, ints(reg), 0, |x| x),
            (Some(Column::Float(v)), _) => gather(v, ix, start, len, floats(reg), 0.0, |x| x),
            (Some(Column::Vec2(v)), _) => gather(v, ix, start, len, vec2s(reg), Vec2::ZERO, Vec2::from_array),
            (Some(Column::Vec3(v)), _) => gather(v, ix, start, len, vec3s(reg), Vec3::ZERO, Vec3::from_array),
            (Some(Column::Vec4(v)), _) => gather(v, ix, start, len, vec4s(reg), Vec4::ZERO, Vec4::from_array),
            (Some(Column::Quat(v)), _) => gather(v, ix, start, len, quats(reg), Quat::IDENTITY, Quat::from_array),
            // Not there in this geometry (it was when compiled): zeros.
            _ => fill(reg, zero(ty), len),
        },
        Map::One(map) => match (s.column, ty) {
            (Some(Column::Bool(v)), _) => through(v, ix, map, start, len, bools(reg), false, |x| x),
            (Some(Column::Int(v)), _) => through(v, ix, map, start, len, ints(reg), 0, |x| x),
            (Some(Column::Float(v)), _) => through(v, ix, map, start, len, floats(reg), 0.0, |x| x),
            (Some(Column::Vec2(v)), _) => through(v, ix, map, start, len, vec2s(reg), Vec2::ZERO, Vec2::from_array),
            (Some(Column::Vec3(v)), _) => through(v, ix, map, start, len, vec3s(reg), Vec3::ZERO, Vec3::from_array),
            (Some(Column::Vec4(v)), _) => through(v, ix, map, start, len, vec4s(reg), Vec4::ZERO, Vec4::from_array),
            (Some(Column::Quat(v)), _) => through(v, ix, map, start, len, quats(reg), Quat::IDENTITY, Quat::from_array),
            _ => fill(reg, zero(ty), len),
        },
        Map::Many { offsets, items } => {
            let lists = Lists { offsets, items, indices: ix, reduce: s.reduce };
            match (s.column, ty) {
                (Some(Column::Bool(v)), _) => {
                    // Any of them, or (Min) all of them.
                    let all = s.reduce == Reduce::Min;
                    bools(reg).extend((start..start + len).map(|e| {
                        let mut vals = lists.values(v, e);
                        if all { vals.all(|b| b) } else { vals.any(|b| b) }
                    }));
                }
                (Some(Column::Int(v)), _) => ints(reg).extend((start..start + len).map(|e| lists.reduce(v, e, 0, |x| x as f64, |x| x.round() as i32))),
                (Some(Column::Float(v)), _) => floats(reg).extend((start..start + len).map(|e| lists.reduce(v, e, 0.0, |x| x as f64, |x| x as f32))),
                (Some(Column::Vec2(v)), _) => vec2s(reg).extend((start..start + len).map(|e| Vec2::from_array(lists.reduce_n(v, e)))),
                (Some(Column::Vec3(v)), _) => vec3s(reg).extend((start..start + len).map(|e| Vec3::from_array(lists.reduce_n(v, e)))),
                (Some(Column::Vec4(v)), _) => vec4s(reg).extend((start..start + len).map(|e| Vec4::from_array(lists.reduce_n(v, e)))),
                // Rotations averaged component by component, then made unit again.
                (Some(Column::Quat(v)), _) => quats(reg).extend((start..start + len).map(|e| Quat::from_array(lists.reduce_n(v, e)).normalize())),
                _ => fill(reg, zero(ty), len),
            }
        }
    }
}

/// Elements `start..start + len` through a one-to-one `map`, then through
/// the attribute's `indices`; missing values are `zero`.
#[allow(clippy::too_many_arguments)]
fn through<T: Copy, U: Copy>(src: &[T], indices: Option<&[u32]>, map: &[u32], start: usize, len: usize, out: &mut Vec<U>, zero: U, conv: impl Fn(T) -> U) {
    out.extend((start..start + len).map(|e| {
        map.get(e).and_then(|&m| value_index(indices, m as usize)).and_then(|i| src.get(i)).map_or(zero, |&v| conv(v))
    }));
}

fn value_index(indices: Option<&[u32]>, element: usize) -> Option<usize> {
    match indices {
        Some(ix) => ix.get(element).map(|&i| i as usize),
        None => Some(element),
    }
}

/// The lists of a many-to-one map, and how each becomes one value.
struct Lists<'a> {
    offsets: &'a [u32],
    items:   &'a [u32],
    indices: Option<&'a [u32]>,
    reduce:  Reduce,
}

impl Lists<'_> {
    /// The values element `e` reaches.
    fn values<'s, T: Copy>(&'s self, src: &'s [T], e: usize) -> impl Iterator<Item = T> + 's {
        let (a, b) = (self.offsets.get(e).copied().unwrap_or(0) as usize, self.offsets.get(e + 1).copied().unwrap_or(0) as usize);
        self.items.get(a..b).unwrap_or(&[]).iter()
            .filter_map(move |&i| value_index(self.indices, i as usize).and_then(|i| src.get(i)).copied())
    }

    /// One number from element `e`'s: their average, sum, least or most.
    fn reduce<T: Copy, U>(&self, src: &[T], e: usize, empty: U, to: impl Fn(T) -> f64, back: impl Fn(f64) -> U) -> U {
        let mut n = 0usize;
        let mut acc: Option<f64> = None;
        for v in self.values(src, e) {
            let x = to(v);
            n += 1;
            acc = Some(match (acc, self.reduce) {
                (None, _) => x,
                (Some(a), Reduce::Average | Reduce::Sum) => a + x,
                (Some(a), Reduce::Min) => a.min(x),
                (Some(a), Reduce::Max) => a.max(x),
            });
        }
        match acc {
            None => empty,
            Some(a) if self.reduce == Reduce::Average => back(a / n as f64),
            Some(a) => back(a),
        }
    }

    /// The same, component by component.
    fn reduce_n<const N: usize>(&self, src: &[[f32; N]], e: usize) -> [f32; N] {
        let mut n = 0usize;
        let mut acc: Option<[f32; N]> = None;
        for v in self.values(src, e) {
            n += 1;
            acc = Some(match acc {
                None => v,
                Some(mut a) => {
                    for k in 0..N {
                        a[k] = match self.reduce {
                            Reduce::Average | Reduce::Sum => a[k] + v[k],
                            Reduce::Min => a[k].min(v[k]),
                            Reduce::Max => a[k].max(v[k]),
                        };
                    }
                    a
                }
            });
        }
        match acc {
            None => [0.0; N],
            Some(mut a) => {
                if self.reduce == Reduce::Average { for x in a.iter_mut() { *x /= n as f32; } }
                a
            }
        }
    }
}

fn zero(ty: Ty) -> Value {
    match ty {
        Ty::Bool => Value::Bool(false),
        Ty::Int => Value::Int(0),
        Ty::Float => Value::Float(0.0),
        Ty::Vec2 => Value::Vec2([0.0; 2]),
        Ty::Vec3 => Value::Vec3([0.0; 3]),
        Ty::Vec4 => Value::Vec4([0.0; 4]),
        Ty::Quat => Value::Quat([0.0, 0.0, 0.0, 1.0]),
        Ty::Sample => Value::Sample(Loc::default()),
        Ty::Token => unreachable!("a path is never one value: the compiler refuses"),
    }
}

fn map1<A: Copy, T>(a: &[A], out: &mut Vec<T>, f: impl Fn(A) -> T) { out.extend(a.iter().map(|&x| f(x))); }
fn map2<A: Copy, B: Copy, T>(a: &[A], b: &[B], out: &mut Vec<T>, f: impl Fn(A, B) -> T) {
    out.extend(a.iter().zip(b).map(|(&x, &y)| f(x, y)));
}
fn map3<A: Copy, B: Copy, C: Copy, T>(a: &[A], b: &[B], c: &[C], out: &mut Vec<T>, f: impl Fn(A, B, C) -> T) {
    out.extend(a.iter().zip(b).zip(c).map(|((&x, &y), &z)| f(x, y, z)));
}

/// Add, subtract, multiply, divide, min and max of two registers of one
/// type. Each operation is its own loop, so each compiles to a tight one.
macro_rules! same_type {
    ($op:expr, $a:expr, $b:expr, $out:expr, $min:expr, $max:expr) => {
        match $op {
            Math::Add => map2($a, $b, $out, |x, y| x + y),
            Math::Subtract => map2($a, $b, $out, |x, y| x - y),
            Math::Multiply => map2($a, $b, $out, |x, y| x * y),
            Math::Divide => map2($a, $b, $out, |x, y| x / y),
            Math::Min => map2($a, $b, $out, $min),
            Math::Max => map2($a, $b, $out, $max),
            _ => return false,
        }
    };
}

/// A vector by a float: scale or divide.
macro_rules! by_float {
    ($op:expr, $v:expr, $f:expr, $out:expr) => {
        match $op {
            Math::Multiply => map2($v, $f, $out, |x, y| x * y),
            Math::Divide => map2($v, $f, $out, |x, y| x / y),
            _ => return false,
        }
    };
}

/// One maths instruction. `false` when the types do not fit, which the
/// compiler has ruled out.
fn math(op: Math, a: &Reg, b: Option<&Reg>, c: Option<&Reg>, out: &mut Reg) -> bool {
    use Reg::*;
    match op {
        // Rotations.
        Math::Multiply if matches!(a, Quat(_)) => match (a, b) {
            (Quat(a), Some(Quat(b))) => map2(a, b, quats(out), |x, y| x * y),
            _ => return false,
        },
        Math::Normalize if matches!(a, Quat(_)) => match a { Quat(a) => map1(a, quats(out), |x| x.normalize()), _ => return false },
        Math::Lerp if matches!(a, Quat(_)) => match (a, b, c) {
            (Quat(a), Some(Quat(b)), Some(Float(t))) => map3(a, b, t, quats(out), |x, y, t| x.slerp(y, t)),
            _ => return false,
        },
        Math::Rotate => match (a, b) {
            (Quat(q), Some(Vec3(v))) => map2(q, v, vec3s(out), |q, v| q * v),
            _ => return false,
        },
        Math::AxisAngle => match (a, b) {
            (Vec3(axis), Some(Float(deg))) => map2(axis, deg, quats(out), |a, d| glam::Quat::from_axis_angle(a.try_normalize().unwrap_or(glam::Vec3::Y), d.to_radians())),
            _ => return false,
        },
        Math::Euler => match a {
            Vec3(e) => map1(e, quats(out), |e| glam::Quat::from_euler(glam::EulerRot::XYZ, e.x.to_radians(), e.y.to_radians(), e.z.to_radians())),
            _ => return false,
        },
        Math::Add | Math::Subtract | Math::Multiply | Math::Divide | Math::Min | Math::Max => match (a, b) {
            (Int(a), Some(Int(b))) => match op {
                Math::Add => map2(a, b, ints(out), i32::wrapping_add),
                Math::Subtract => map2(a, b, ints(out), i32::wrapping_sub),
                Math::Multiply => map2(a, b, ints(out), i32::wrapping_mul),
                Math::Divide => map2(a, b, ints(out), |x, y| x.checked_div(y).unwrap_or(0)),
                Math::Min => map2(a, b, ints(out), i32::min),
                _ => map2(a, b, ints(out), i32::max),
            },
            (Float(a), Some(Float(b))) => same_type!(op, a, b, floats(out), f32::min, f32::max),
            (Vec2(a), Some(Vec2(b))) => same_type!(op, a, b, vec2s(out), |x: glam::Vec2, y| x.min(y), |x: glam::Vec2, y| x.max(y)),
            (Vec3(a), Some(Vec3(b))) => same_type!(op, a, b, vec3s(out), |x: glam::Vec3, y| x.min(y), |x: glam::Vec3, y| x.max(y)),
            (Vec4(a), Some(Vec4(b))) => same_type!(op, a, b, vec4s(out), |x: glam::Vec4, y| x.min(y), |x: glam::Vec4, y| x.max(y)),
            (Vec2(v), Some(Float(f))) => by_float!(op, v, f, vec2s(out)),
            (Vec3(v), Some(Float(f))) => by_float!(op, v, f, vec3s(out)),
            (Vec4(v), Some(Float(f))) => by_float!(op, v, f, vec4s(out)),
            (Float(f), Some(Vec2(v))) if op == Math::Multiply => map2(f, v, vec2s(out), |x, y| x * y),
            (Float(f), Some(Vec3(v))) if op == Math::Multiply => map2(f, v, vec3s(out), |x, y| x * y),
            (Float(f), Some(Vec4(v))) if op == Math::Multiply => map2(f, v, vec4s(out), |x, y| x * y),
            _ => return false,
        },
        Math::Lerp => match (a, b, c) {
            (Float(a), Some(Float(b)), Some(Float(t))) => map3(a, b, t, floats(out), |x, y, t| x + (y - x) * t),
            (Vec2(a), Some(Vec2(b)), Some(Float(t))) => map3(a, b, t, vec2s(out), |x, y, t| x.lerp(y, t)),
            (Vec3(a), Some(Vec3(b)), Some(Float(t))) => map3(a, b, t, vec3s(out), |x, y, t| x.lerp(y, t)),
            (Vec4(a), Some(Vec4(b)), Some(Float(t))) => map3(a, b, t, vec4s(out), |x, y, t| x.lerp(y, t)),
            _ => return false,
        },
        Math::Negate | Math::Abs => {
            let abs = op == Math::Abs;
            match a {
                Int(a) => map1(a, ints(out), |x| if abs { x.wrapping_abs() } else { x.wrapping_neg() }),
                Float(a) => map1(a, floats(out), |x| if abs { x.abs() } else { -x }),
                Vec2(a) => map1(a, vec2s(out), |x| if abs { x.abs() } else { -x }),
                Vec3(a) => map1(a, vec3s(out), |x| if abs { x.abs() } else { -x }),
                Vec4(a) => map1(a, vec4s(out), |x| if abs { x.abs() } else { -x }),
                Bool(_) | Quat(_) | Sample(_) | Token(_) | List(..) => return false,
            }
        }
        Math::Sin => match a { Float(a) => map1(a, floats(out), f32::sin), _ => return false },
        Math::Cos => match a { Float(a) => map1(a, floats(out), f32::cos), _ => return false },
        Math::Dot => match (a, b) {
            (Vec2(a), Some(Vec2(b))) => map2(a, b, floats(out), |x, y| x.dot(y)),
            (Vec3(a), Some(Vec3(b))) => map2(a, b, floats(out), |x, y| x.dot(y)),
            (Vec4(a), Some(Vec4(b))) => map2(a, b, floats(out), |x, y| x.dot(y)),
            _ => return false,
        },
        Math::Cross => match (a, b) {
            (Vec3(a), Some(Vec3(b))) => map2(a, b, vec3s(out), |x, y| x.cross(y)),
            _ => return false,
        },
        Math::Length => match a {
            Vec2(a) => map1(a, floats(out), |x| x.length()),
            Vec3(a) => map1(a, floats(out), |x| x.length()),
            Vec4(a) => map1(a, floats(out), |x| x.length()),
            _ => return false,
        },
        Math::Normalize => match a {
            Vec2(a) => map1(a, vec2s(out), |x| x.normalize_or_zero()),
            Vec3(a) => map1(a, vec3s(out), |x| x.normalize_or_zero()),
            Vec4(a) => map1(a, vec4s(out), |x| x.normalize_or_zero()),
            _ => return false,
        },
    }
    true
}

/// A written column, filled batch by batch.
enum Out {
    Bool(Vec<bool>),
    Int(Vec<i32>),
    Float(Vec<f32>),
    Vec2(Vec<[f32; 2]>),
    Vec3(Vec<[f32; 3]>),
    Vec4(Vec<[f32; 4]>),
    Quat(Vec<[f32; 4]>),
}

/// One batch's part of a written column.
enum Slice<'a> {
    Bool(&'a mut [bool]),
    Int(&'a mut [i32]),
    Float(&'a mut [f32]),
    Vec2(&'a mut [[f32; 2]]),
    Vec3(&'a mut [[f32; 3]]),
    Vec4(&'a mut [[f32; 4]]),
    Quat(&'a mut [[f32; 4]]),
}

fn store(reg: &Reg, out: &mut Slice) {
    match (reg, out) {
        (Reg::Bool(r), Slice::Bool(o)) => o.copy_from_slice(&r[..o.len()]),
        (Reg::Int(r), Slice::Int(o)) => o.copy_from_slice(&r[..o.len()]),
        (Reg::Float(r), Slice::Float(o)) => o.copy_from_slice(&r[..o.len()]),
        (Reg::Vec2(r), Slice::Vec2(o)) => o.iter_mut().zip(r).for_each(|(o, v)| *o = v.to_array()),
        (Reg::Vec3(r), Slice::Vec3(o)) => o.iter_mut().zip(r).for_each(|(o, v)| *o = v.to_array()),
        (Reg::Vec4(r), Slice::Vec4(o)) => o.iter_mut().zip(r).for_each(|(o, v)| *o = v.to_array()),
        (Reg::Quat(r), Slice::Quat(o)) => o.iter_mut().zip(r).for_each(|(o, v)| *o = v.to_array()),
        _ => {}
    }
}

/// What a program reads from: the plan's sources, its parameters, the
/// uniform registers (for the batch program).
struct Env<'a> {
    sources:  Vec<Source<'a>>,
    params:   Vec<Value>,
    uniforms: Vec<Reg>,
    /// The geometry, for what reads its topology (scattering, surface samples).
    geo:      &'a Geo,
    /// The paths of the primitives on each reference input, from input 1.
    references: &'a [Vec<String>],
}

/// Run `program` for elements `start..start + len`.
fn exec(program: &Program, regs: &mut [Reg], env: &Env, start: usize, len: usize, outs: &mut [Slice]) {
    for ins in &program.code {
        match *ins {
            Ins::Const { dst, value } => fill(&mut regs[dst], value, len),
            Ins::Param { dst, index } => fill(&mut regs[dst], env.params[index], len),
            Ins::Load { dst, read } => load(&env.sources[read], start, len, &mut regs[dst], program.regs[dst]),
            Ins::Broadcast { dst, from } => fill(&mut regs[dst], value_at(&env.uniforms[from], 0), len),
            // Lists: item by item, a per-element value meeting each item.
            Ins::Math { op, dst, args } if args[..op.arity()].iter().any(|&r| matches!(regs[r], Reg::List(..))) => {
                let arity = op.arity();
                let offsets = args[..arity].iter().find_map(|&r| match &regs[r] { Reg::List(o, _) => Some(o.clone()), _ => None }).unwrap_or_default();
                let spread: Vec<Option<Reg>> = args[..arity].iter().map(|&r| match &regs[r] { Reg::List(..) => None, one => Some(per_item(one, &offsets)) }).collect();
                let arg = |k: usize| (k < arity).then(|| match (&spread[k], &regs[args[k]]) {
                    (Some(s), _) => s,
                    (None, Reg::List(_, items)) => &**items,
                    (None, other) => other,
                });
                let mut items = Reg::Bool(vec![]);
                math(op, arg(0).expect("every operation takes an input"), arg(1), arg(2), &mut items);
                regs[dst] = Reg::List(offsets, Box::new(items));
            }
            Ins::Math { op, dst, args } => {
                let mut out = std::mem::replace(&mut regs[dst], Reg::Bool(vec![]));
                // An array meeting one value (in the uniform program): the
                // value spread over the array's length.
                let arity = op.arity();
                let n = args[..arity].iter().map(|&r| regs[r].len()).max().unwrap_or(0);
                let spread: Vec<Option<Reg>> = args[..arity].iter().map(|&r| (regs[r].len() == 1 && n > 1).then(|| {
                    let mut s = Reg::Bool(vec![]);
                    fill(&mut s, value_at(&regs[r], 0), n);
                    s
                })).collect();
                let arg = |k: usize| (k < arity).then(|| spread[k].as_ref().unwrap_or(&regs[args[k]]));
                math(op, arg(0).expect("every operation takes an input"), arg(1), arg(2), &mut out);
                regs[dst] = out;
            }
            Ins::ToFloat { dst, src } => {
                let mut out = std::mem::replace(&mut regs[dst], Reg::Bool(vec![]));
                match &regs[src] {
                    Reg::Int(v) => floats(&mut out).extend(v.iter().map(|&i| i as f32)),
                    Reg::List(o, items) => if let Reg::Int(v) = &**items {
                        out = Reg::List(o.clone(), Box::new(Reg::Float(v.iter().map(|&i| i as f32).collect())));
                    },
                    _ => {}
                }
                regs[dst] = out;
            }
            Ins::LoadList { dst, read, flat } => {
                let (offsets, items) = load_list(&env.sources[read], start, len, program.regs[dst]);
                regs[dst] = if flat { items } else { Reg::List(offsets, Box::new(items)) };
            }
            Ins::Random { dst, seed, args, array, key } => {
                let mut out = std::mem::replace(&mut regs[dst], Reg::Bool(vec![]));
                let (base, n) = drawn_span(regs, args, array, key, start, len);
                let keys = Keys::of(regs, key, args[3]);
                random(seed, base, n, &keys, &regs[args[0]], &regs[args[1]], &mut out);
                regs[dst] = out;
            }
            Ins::Concat { dst, ref srcs } => {
                let mut out = std::mem::replace(&mut regs[dst], Reg::Bool(vec![]));
                concat(srcs.iter().map(|&r| &regs[r]), &mut out);
                regs[dst] = out;
            }
            Ins::Scatter { dst, seed, count, weight } => {
                let n = value_at(&regs[count], 0).as_i32().max(0) as usize;
                let made = scatter(env.geo, seed, n, weight.map(|w| &env.sources[w]));
                *locs(&mut regs[dst]) = made;
            }
            Ins::AtLocation { dst, read, loc } => {
                let mut out = std::mem::replace(&mut regs[dst], Reg::Bool(vec![]));
                if let Reg::Sample(at) = &regs[loc] { at_location(env.geo, &env.sources[read], at, &mut out); }
                regs[dst] = out;
            }
            Ins::RandomOrient { dst, seed, args, array, key } => {
                let made = {
                    let (base, n) = drawn_span(regs, args, array, key, start, len);
                    let keys = Keys::of(regs, key, args[3]);
                    let normals: &[Vec3] = match &regs[args[0]] { Reg::Vec3(v) => v, _ => &[] };
                    let turns: &[Vec3] = match &regs[args[1]] { Reg::Vec3(v) => v, _ => &[] };
                    random_orientations(seed, base, n, &keys, normals, turns)
                };
                *quats(&mut regs[dst]) = made;
            }
            Ins::Reference { dst, input, ref paths } => {
                let there = env.references.get(input - 1).map(Vec::as_slice).unwrap_or_default();
                let out = tokens(&mut regs[dst]);
                match paths {
                    None => out.extend(there.iter().map(|p| Arc::from(p.as_str()))),
                    Some(list) => out.extend(list.iter().filter(|p| there.contains(p)).map(|p| Arc::from(p.as_str()))),
                }
            }
            Ins::Compose { op, dst, args } => {
                let mut out = std::mem::replace(&mut regs[dst], Reg::Bool(vec![]));
                compose(op, &args[..op.inputs().len()].iter().map(|&r| &regs[r]).collect::<Vec<_>>(), &mut out);
                regs[dst] = out;
            }
            Ins::Array { op, dst, args } => {
                let mut out = std::mem::replace(&mut regs[dst], Reg::Bool(vec![]));
                match &regs[args[0]] {
                    Reg::List(offsets, items) => list_op(op, offsets, items, &regs[args[1]], &mut out),
                    a => array_op(op, a, &regs[args[1]], &mut out),
                }
                regs[dst] = out;
            }
            Ins::Store { write, src } => store(&regs[src], &mut outs[write]),
        }
    }
}

/// What an Instancer made: USD's PointInstancer, in the space of the
/// geometry the graph ran on. Instance `i` is prototype
/// `prototypes[proto_indices[i]]` at `positions[i]`, turned by
/// `orientations[i]` (i, j, k, real) and scaled by `scales[i]`. `path`: the
/// instancer prim's, as the Instancer was given it (relative: beside the
/// primitive).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Instances {
    pub path:          String,
    pub prototypes:    Vec<String>,
    pub proto_indices: Vec<i32>,
    pub positions:     Vec<[f32; 3]>,
    pub orientations:  Vec<[f32; 4]>,
    pub scales:        Vec<[f32; 3]>,
}

/// What a plan gives: the geometry, and the instances its Instancers made.
pub struct Ran {
    pub geo:       Geo,
    pub instances: Vec<Instances>,
}

/// An Instancer's instances, from its inputs in the uniform registers: an
/// array of a value gives instance `i` its element `i`, one value gives
/// every instance that value.
fn instances(p: &InstancerPlan, regs: &[Reg]) -> Instances {
    fn at<T: Copy>(v: &[T], i: usize, none: T) -> T { v.get(i).or(v.first()).copied().unwrap_or(none) }
    let positions: &[Vec3] = match &regs[p.positions] { Reg::Vec3(v) => v, _ => &[] };
    let prototypes: Vec<String> = match &regs[p.protos] { Reg::Token(v) => v.iter().map(|t| t.to_string()).collect(), _ => vec![] };
    let kinds = prototypes.len();
    if kinds == 0 { return Instances { path: p.path.clone(), ..Default::default() }; }
    let picks: &[i32] = match &regs[p.index] { Reg::Int(v) => v, _ => &[] };
    let orients: &[Quat] = match &regs[p.orient] { Reg::Quat(v) => v, _ => &[] };
    let n = positions.len();
    let mut out = Instances { path: p.path.clone(), prototypes, ..Default::default() };
    out.positions = positions.iter().map(|v| v.to_array()).collect();
    // -1: picked at random, the same for the same seed.
    out.proto_indices = (0..n).map(|i| {
        let pick = at(picks, i, -1);
        let k = if pick < 0 { ((unit(p.seed, i, 7) * kinds as f32) as usize).min(kinds - 1) } else { (pick as usize).min(kinds - 1) };
        k as i32
    }).collect();
    out.orientations = (0..n).map(|i| at(orients, i, Quat::IDENTITY).normalize().to_array()).collect();
    out.scales = match &regs[p.scale] {
        Reg::Float(v) => (0..n).map(|i| [at(v, i, 1.0); 3]).collect(),
        Reg::Vec3(v) => (0..n).map(|i| at(v, i, Vec3::ONE).to_array()).collect(),
        _ => vec![[1.0; 3]; n],
    };
    out
}

/// What random values are drawn for (see `compile::Key`), read from the
/// seed register.
struct Keys<'a> { kind: Key, values: &'a [i32] }

impl<'a> Keys<'a> {
    fn of(regs: &'a [Reg], kind: Key, reg: usize) -> Self {
        let values: &[i32] = match (kind, regs.get(reg)) { (Key::Element, _) => &[], (_, Some(Reg::Int(v))) => v, _ => &[] };
        Keys { kind, values }
    }

    /// The seed and the number value `i` (of `base + i`) is drawn for: its
    /// own number; its own number with one seed mixed into the node's; or
    /// its seed's.
    fn at(&self, seed: u32, base: usize, i: usize) -> (u32, usize) {
        match self.kind {
            Key::Element => (seed, base + i),
            Key::One => (seed ^ (self.values.first().copied().unwrap_or(0) as u32).wrapping_mul(0x9E37_79B1), base + i),
            Key::Each => (seed, self.values.get(i).copied().unwrap_or(0) as u32 as usize),
        }
    }
}

/// Where a draw's values are numbered from, and how many: an array's (its
/// count, its normals', or its seeds'), else the batch's elements.
fn drawn_span(regs: &[Reg], args: [usize; 4], array: bool, key: Key, start: usize, len: usize) -> (usize, usize) {
    if !array { return (start, len); }
    let n = match regs.get(args[2]) {
        _ if args[2] == NO_COUNT => if key == Key::Element { 0 } else { regs[args[3]].len() },
        Some(Reg::Int(c)) => c.first().copied().unwrap_or(0).max(0) as usize,
        Some(other) => other.len(),
        None => 0,
    };
    (0, n)
}

/// What each of elements `start..start + len` touches in the source's
/// context, every value kept: the lists' offsets (from 0) and their items.
fn load_list(s: &Source, start: usize, len: usize, ty: Ty) -> (Vec<u32>, Reg) {
    let mut offsets = Vec::with_capacity(len + 1);
    offsets.push(0u32);
    let mut elems: Vec<usize> = vec![];
    for e in start..start + len {
        match &*s.map {
            // The same element; the one Object value.
            Map::Direct => elems.push(if s.context == Context::Object { 0 } else { e }),
            Map::One(m) => elems.extend(m.get(e).map(|&i| i as usize)),
            Map::Many { offsets: o, items } => {
                let (a, b) = (o.get(e).copied().unwrap_or(0) as usize, o.get(e + 1).copied().unwrap_or(0) as usize);
                elems.extend(items.get(a..b).unwrap_or(&[]).iter().map(|&i| i as usize));
            }
        }
        offsets.push(elems.len() as u32);
    }
    (offsets, pick(s, &elems, ty))
}

/// The values of a source's column at these elements (through its indices).
fn pick(s: &Source, elems: &[usize], ty: Ty) -> Reg {
    fn at<T: Copy, U>(v: &[T], ix: Option<&[u32]>, elems: &[usize], zero: U, conv: impl Fn(T) -> U) -> Vec<U> where U: Copy {
        elems.iter().map(|&e| value_index(ix, e).and_then(|i| v.get(i)).map_or(zero, |&x| conv(x))).collect()
    }
    let ix = s.indices;
    match s.column {
        Some(Column::Bool(v)) => Reg::Bool(at(v, ix, elems, false, |x| x)),
        Some(Column::Int(v)) => Reg::Int(at(v, ix, elems, 0, |x| x)),
        Some(Column::Float(v)) => Reg::Float(at(v, ix, elems, 0.0, |x| x)),
        Some(Column::Vec2(v)) => Reg::Vec2(at(v, ix, elems, Vec2::ZERO, Vec2::from_array)),
        Some(Column::Vec3(v)) => Reg::Vec3(at(v, ix, elems, Vec3::ZERO, Vec3::from_array)),
        Some(Column::Vec4(v)) => Reg::Vec4(at(v, ix, elems, Vec4::ZERO, Vec4::from_array)),
        Some(Column::Quat(v)) => Reg::Quat(at(v, ix, elems, Quat::IDENTITY, Quat::from_array)),
        // Not there in this geometry (it was when compiled): zeros.
        _ => { let mut r = Reg::Bool(vec![]); fill(&mut r, zero(ty), elems.len()); r }
    }
}

/// A value per element as a value per item of the lists `offsets` gives:
/// element `e`'s value once for each of its items.
fn per_item(reg: &Reg, offsets: &[u32]) -> Reg {
    fn spread<T: Copy>(v: &[T], offsets: &[u32]) -> Vec<T> {
        let mut out = Vec::with_capacity(offsets.last().copied().unwrap_or(0) as usize);
        for e in 0..offsets.len().saturating_sub(1) {
            let Some(&x) = v.get(e).or(v.last()) else { break };
            out.extend(std::iter::repeat_n(x, (offsets[e + 1] - offsets[e]) as usize));
        }
        out
    }
    match reg {
        Reg::Bool(v) => Reg::Bool(spread(v, offsets)),
        Reg::Int(v) => Reg::Int(spread(v, offsets)),
        Reg::Float(v) => Reg::Float(spread(v, offsets)),
        Reg::Vec2(v) => Reg::Vec2(spread(v, offsets)),
        Reg::Vec3(v) => Reg::Vec3(spread(v, offsets)),
        Reg::Vec4(v) => Reg::Vec4(spread(v, offsets)),
        Reg::Quat(v) => Reg::Quat(spread(v, offsets)),
        Reg::Sample(v) => Reg::Sample(spread(v, offsets)),
        Reg::Token(_) | Reg::List(..) => Reg::Bool(vec![]),
    }
}

/// An array node on lists: one result per element, from its items (its
/// count, the item at the index, their sum or average). An empty list
/// gives zero, or no rotation.
fn list_op(op: ArrayOp, offsets: &[u32], items: &Reg, index: &Reg, out: &mut Reg) {
    let n = offsets.len().saturating_sub(1);
    let range = |e: usize| offsets[e] as usize..offsets[e + 1] as usize;
    let index_at = |e: usize| match index { Reg::Int(v) => v.get(e).or(v.last()).copied().unwrap_or(0), _ => 0 };
    macro_rules! item {
        ($v:expr, $zero:expr) => {
            (0..n).map(|e| {
                let r = range(e);
                if r.is_empty() { $zero } else { $v[r.start + (index_at(e).max(0) as usize).min(r.len() - 1)] }
            }).collect()
        };
    }
    macro_rules! total {
        ($v:expr, $zero:expr, $div:expr) => {
            (0..n).map(|e| {
                let r = range(e);
                let s = $v[r.clone()].iter().fold($zero, |a, &b| a + b);
                if op == ArrayOp::Average && !r.is_empty() { ($div)(s, r.len()) } else { s }
            }).collect()
        };
    }
    *out = match op {
        ArrayOp::Size => Reg::Int((0..n).map(|e| range(e).len() as i32).collect()),
        ArrayOp::Element => match items {
            Reg::Bool(v) => Reg::Bool(item!(v, false)),
            Reg::Int(v) => Reg::Int(item!(v, 0)),
            Reg::Float(v) => Reg::Float(item!(v, 0.0)),
            Reg::Vec2(v) => Reg::Vec2(item!(v, Vec2::ZERO)),
            Reg::Vec3(v) => Reg::Vec3(item!(v, Vec3::ZERO)),
            Reg::Vec4(v) => Reg::Vec4(item!(v, Vec4::ZERO)),
            Reg::Quat(v) => Reg::Quat(item!(v, Quat::IDENTITY)),
            Reg::Sample(v) => Reg::Sample(item!(v, Loc::default())),
            Reg::Token(_) | Reg::List(..) => Reg::Bool(vec![]),
        },
        ArrayOp::Sum | ArrayOp::Average => match items {
            Reg::Int(v) => Reg::Int(total!(v, 0i32, |s: i32, n: usize| s / n as i32)),
            Reg::Float(v) => Reg::Float(total!(v, 0.0f32, |s: f32, n: usize| s / n as f32)),
            Reg::Vec2(v) => Reg::Vec2(total!(v, Vec2::ZERO, |s: Vec2, n: usize| s / n as f32)),
            Reg::Vec3(v) => Reg::Vec3(total!(v, Vec3::ZERO, |s: Vec3, n: usize| s / n as f32)),
            Reg::Vec4(v) => Reg::Vec4(total!(v, Vec4::ZERO, |s: Vec4, n: usize| s / n as f32)),
            _ => Reg::Bool(vec![]),
        },
    };
}

/// A vector from its parts: element `i` of each (one value meets every
/// element of the arrays or batch it is with).
fn compose(op: Compose, parts: &[&Reg], out: &mut Reg) {
    fn at<T: Copy + Default>(v: &[T], i: usize) -> T { v.get(i).or(v.last()).copied().unwrap_or_default() }
    let n = parts.iter().map(|r| r.len()).max().unwrap_or(0);
    let f = |k: usize, i: usize| match parts.get(k) { Some(Reg::Float(v)) => at(v, i), _ => 0.0 };
    match op {
        Compose::Vec2 => *vec2s(out) = (0..n).map(|i| Vec2::new(f(0, i), f(1, i))).collect(),
        Compose::Vec3 => *vec3s(out) = (0..n).map(|i| Vec3::new(f(0, i), f(1, i), f(2, i))).collect(),
        Compose::Vec4 => *vec4s(out) = (0..n).map(|i| Vec4::new(f(0, i), f(1, i), f(2, i), f(3, i))).collect(),
        Compose::Vec2Z => {
            let xy: &[Vec2] = match parts.first() { Some(Reg::Vec2(v)) => v, _ => &[] };
            *vec3s(out) = (0..n).map(|i| at(xy, i).extend(f(1, i))).collect();
        }
        Compose::Vec3W => {
            let xyz: &[Vec3] = match parts.first() { Some(Reg::Vec3(v)) => v, _ => &[] };
            *vec4s(out) = (0..n).map(|i| at(xyz, i).extend(f(1, i))).collect();
        }
        // Made unit; all zeros, no rotation.
        Compose::Quat => *quats(out) = (0..n).map(|i| {
            let q = Vec4::new(f(0, i), f(1, i), f(2, i), f(3, i));
            if q.length_squared() > 0.0 { Quat::from_vec4(q).normalize() } else { Quat::IDENTITY }
        }).collect(),
    }
}

/// Arrays (or single values) one after the other, into `out`.
fn concat<'a>(parts: impl Iterator<Item = &'a Reg>, out: &mut Reg) {
    let mut first = true;
    for part in parts {
        // `out` takes the first part's type, emptied.
        if first {
            match part {
                Reg::Bool(_) => { bools(out); }
                Reg::Int(_) => { ints(out); }
                Reg::Float(_) => { floats(out); }
                Reg::Vec2(_) => { vec2s(out); }
                Reg::Vec3(_) => { vec3s(out); }
                Reg::Vec4(_) => { vec4s(out); }
                Reg::Quat(_) => { quats(out); }
                Reg::Sample(_) => { locs(out); }
                Reg::Token(_) => { tokens(out); }
                Reg::List(..) => {}
            }
            first = false;
        }
        match (&mut *out, part) {
            (Reg::Bool(o), Reg::Bool(p)) => o.extend_from_slice(p),
            (Reg::Int(o), Reg::Int(p)) => o.extend_from_slice(p),
            (Reg::Float(o), Reg::Float(p)) => o.extend_from_slice(p),
            (Reg::Vec2(o), Reg::Vec2(p)) => o.extend_from_slice(p),
            (Reg::Vec3(o), Reg::Vec3(p)) => o.extend_from_slice(p),
            (Reg::Vec4(o), Reg::Vec4(p)) => o.extend_from_slice(p),
            (Reg::Quat(o), Reg::Quat(p)) => o.extend_from_slice(p),
            (Reg::Sample(o), Reg::Sample(p)) => o.extend_from_slice(p),
            (Reg::Token(o), Reg::Token(p)) => o.extend(p.iter().cloned()),
            _ => {}
        }
    }
}

/// `n` rotations, numbered from `base`: each one's +Y along its normal
/// (element `i` of `normals`, or the one there is; +Y when none), then
/// turned in its own frame at random, up to its turn (element `i` of
/// `turns`, or the one there is) in degrees about each axis, either way.
/// The same for the same seed. In parallel: each is its own hash.
fn random_orientations(seed: u32, base: usize, n: usize, keys: &Keys, normals: &[Vec3], turns: &[Vec3]) -> Vec<Quat> {
    let one = |i: usize| {
        let (s, e) = keys.at(seed, base, i);
        let normal = normals.get(i).or(normals.first()).copied().unwrap_or(Vec3::Y);
        let half = turns.get(i).or(turns.first()).copied().unwrap_or(Vec3::ZERO) * 0.5;
        let up = Quat::from_rotation_arc(Vec3::Y, normal.try_normalize().unwrap_or(Vec3::Y));
        let spin = Quat::from_euler(glam::EulerRot::XYZ,
            ((unit(s, e, 1) * 2.0 - 1.0) * half.x).to_radians(),
            ((unit(s, e, 2) * 2.0 - 1.0) * half.y).to_radians(),
            ((unit(s, e, 3) * 2.0 - 1.0) * half.z).to_radians());
        (up * spin).normalize()
    };
    // A batch is already one thread's: only a large array is split.
    if n >= BATCH { (0..n).into_par_iter().map(one).collect() } else { (0..n).map(one).collect() }
}

/// Run `plan` over `geo`. `params` gives values to the graph's parameters
/// by name; those not given take their defaults. `references`: the paths on
/// each of the operator's reference inputs, from input 1 (what Get
/// Reference reads). The geometry given back shares every column the plan
/// does not set with `geo`.
pub fn run(plan: &Plan, geo: &Geo, params: &HashMap<String, Value>, references: &[Vec<String>]) -> Ran {
    // How the plan's elements reach each context read: made once per run,
    // shared by the reads of that context.
    let mut maps: HashMap<Context, Arc<Map>> = HashMap::new();
    // The columns made from the topology, one per read of one.
    let made: Vec<Option<Column>> = plan.reads.iter()
        .map(|r| if r.derived { crate::derived::column(geo, r.context, &r.name) } else { None })
        .collect();
    let sources = plan.reads.iter().zip(&made).map(|(r, made)| {
        let attr = if r.derived { None } else { geo.attr(r.context, &r.name) };
        // Read at surface samples: by their corners and faces, not
        // element by element.
        let map = if r.at_location { Arc::new(Map::Direct) } else {
            maps.entry(r.context).or_insert_with(|| Arc::new(mapping(geo, plan.context, r.context))).clone()
        };
        Source {
            column: made.as_ref().or(attr.map(|a| &a.column)),
            indices: attr.and_then(|a| a.indices.as_deref().map(|v| v.as_slice())),
            map, reduce: r.reduce, context: r.context,
        }
    }).collect();
    let params = plan.params.iter()
        .map(|(name, default)| params.get(name).and_then(|v| v.convert(default.ty())).unwrap_or(*default))
        .collect();
    let mut env = Env { sources, params, uniforms: vec![], geo, references };

    // Once: what does not vary per element.
    let mut uniforms = registers(&plan.uniform);
    exec(&plan.uniform, &mut uniforms, &env, 0, 1, &mut []);
    let made: Vec<Instances> = plan.instancers.iter().map(|p| instances(p, &uniforms)).collect();
    env.uniforms = uniforms;

    let n = geo.count(plan.context);
    if plan.writes.is_empty() || n == 0 { return Ran { geo: geo.clone(), instances: made }; }

    // The written columns, then each batch's slice of each.
    let mut cols: Vec<Out> = plan.writes.iter().map(|w| match w.ty {
        Ty::Bool => Out::Bool(vec![false; n]),
        Ty::Int => Out::Int(vec![0; n]),
        Ty::Float => Out::Float(vec![0.0; n]),
        Ty::Vec2 => Out::Vec2(vec![[0.0; 2]; n]),
        Ty::Vec3 => Out::Vec3(vec![[0.0; 3]; n]),
        Ty::Vec4 => Out::Vec4(vec![[0.0; 4]; n]),
        Ty::Quat => Out::Quat(vec![[0.0, 0.0, 0.0, 1.0]; n]),
        Ty::Sample | Ty::Token => unreachable!("surface samples and paths are not written: the compiler refuses"),
    }).collect();
    let batches = n.div_ceil(BATCH);
    let mut tasks: Vec<Vec<Slice>> = (0..batches).map(|_| Vec::with_capacity(cols.len())).collect();
    for col in &mut cols {
        match col {
            Out::Bool(v) => v.chunks_mut(BATCH).zip(&mut tasks).for_each(|(c, t)| t.push(Slice::Bool(c))),
            Out::Int(v) => v.chunks_mut(BATCH).zip(&mut tasks).for_each(|(c, t)| t.push(Slice::Int(c))),
            Out::Float(v) => v.chunks_mut(BATCH).zip(&mut tasks).for_each(|(c, t)| t.push(Slice::Float(c))),
            Out::Vec2(v) => v.chunks_mut(BATCH).zip(&mut tasks).for_each(|(c, t)| t.push(Slice::Vec2(c))),
            Out::Vec3(v) => v.chunks_mut(BATCH).zip(&mut tasks).for_each(|(c, t)| t.push(Slice::Vec3(c))),
            Out::Vec4(v) => v.chunks_mut(BATCH).zip(&mut tasks).for_each(|(c, t)| t.push(Slice::Vec4(c))),
            Out::Quat(v) => v.chunks_mut(BATCH).zip(&mut tasks).for_each(|(c, t)| t.push(Slice::Quat(c))),
        }
    }
    let env = &env;
    tasks.into_par_iter().enumerate().for_each_init(
        || registers(&plan.batch),
        |regs, (k, mut outs)| {
            let start = k * BATCH;
            exec(&plan.batch, regs, env, start, BATCH.min(n - start), &mut outs);
        },
    );

    // Only the written columns are new; everything else stays shared.
    let mut out = geo.clone();
    for (w, col) in plan.writes.iter().zip(cols) {
        let column = match col {
            Out::Bool(v) => Column::Bool(Arc::new(v)),
            Out::Int(v) => Column::Int(Arc::new(v)),
            Out::Float(v) => Column::Float(Arc::new(v)),
            Out::Vec2(v) => Column::Vec2(Arc::new(v)),
            Out::Vec3(v) => Column::Vec3(Arc::new(v)),
            Out::Vec4(v) => Column::Vec4(Arc::new(v)),
            Out::Quat(v) => Column::Quat(Arc::new(v)),
        };
        let column = if w.context == plan.context { column } else { carry(geo, &column, plan.context, w) };
        // One attribute of a name: set in one context, gone from the others
        // (as USD has one primvar of a name).
        for c in Context::ALL { if c != w.context { out.remove(c, &w.name); } }
        out.set(w.context, &w.name, Attr::new(column, w.role));
    }
    Ran { geo: out, instances: made }
}

/// A column of the plan's elements carried into the context `w` writes:
/// each element there reads the plan's elements it touches, as Get Data
/// reads across, made one by `w.reduce`.
fn carry(geo: &Geo, column: &Column, from: Context, w: &Write) -> Column {
    let n = geo.count(w.context);
    // The one Object value, for every element.
    let map = if from == Context::Object { Map::One(vec![0; n]) } else { mapping(geo, w.context, from) };
    let source = Source { column: Some(column), indices: None, map: Arc::new(map), reduce: w.reduce, context: from };
    let mut reg = Reg::Bool(vec![]);
    load(&source, 0, n, &mut reg, w.ty);
    match reg {
        Reg::Bool(v) => Column::Bool(Arc::new(v)),
        Reg::Int(v) => Column::Int(Arc::new(v)),
        Reg::Float(v) => Column::Float(Arc::new(v)),
        Reg::Vec2(v) => Column::Vec2(Arc::new(v.into_iter().map(|x| x.to_array()).collect())),
        Reg::Vec3(v) => Column::Vec3(Arc::new(v.into_iter().map(|x| x.to_array()).collect())),
        Reg::Vec4(v) => Column::Vec4(Arc::new(v.into_iter().map(|x| x.to_array()).collect())),
        Reg::Quat(v) => Column::Quat(Arc::new(v.into_iter().map(|x| x.to_array()).collect())),
        Reg::Sample(_) | Reg::Token(_) | Reg::List(..) => unreachable!("surface samples, paths and lists are not written: the compiler refuses"),
    }
}

/// A number in [0, 1) from a seed, an element (or array index) and a lane
/// (a vector's component): the same for the same three, every time, on any
/// machine (SplitMix64's finaliser).
fn unit(seed: u32, element: usize, lane: u32) -> f32 {
    let mut x = ((seed as u64) << 32 | lane as u64) ^ (element as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// `n` random values between `lo` and `hi` (each one value, or one per
/// element), for elements numbered from `base`. Ints include `hi`.
fn random(seed: u32, base: usize, n: usize, keys: &Keys, lo: &Reg, hi: &Reg, out: &mut Reg) {
    // Element `i` of a register one value long, or one per element.
    fn at<T: Copy>(v: &[T], i: usize) -> T { v[i.min(v.len() - 1)] }
    if lo.len() == 0 || hi.len() == 0 { return; }
    let u = |i: usize, lane: u32| { let (s, e) = keys.at(seed, base, i); unit(s, e, lane) };
    match (lo, hi) {
        (Reg::Int(a), Reg::Int(b)) => ints(out).extend((0..n).map(|i| {
            let (x, y) = (at(a, i), at(b, i));
            let span = (y as i64 - x as i64 + 1).max(1);
            (x as i64 + ((u(i, 0) as f64 * span as f64) as i64).min(span - 1)) as i32
        })),
        (Reg::Float(a), Reg::Float(b)) => floats(out).extend((0..n).map(|i| {
            let (x, y) = (at(a, i), at(b, i));
            x + (y - x) * u(i, 0)
        })),
        (Reg::Vec2(a), Reg::Vec2(b)) => vec2s(out).extend((0..n).map(|i| {
            let (x, y) = (at(a, i), at(b, i));
            x + (y - x) * Vec2::new(u(i, 0), u(i, 1))
        })),
        (Reg::Vec3(a), Reg::Vec3(b)) => vec3s(out).extend((0..n).map(|i| {
            let (x, y) = (at(a, i), at(b, i));
            x + (y - x) * Vec3::new(u(i, 0), u(i, 1), u(i, 2))
        })),
        (Reg::Vec4(a), Reg::Vec4(b)) => vec4s(out).extend((0..n).map(|i| {
            let (x, y) = (at(a, i), at(b, i));
            x + (y - x) * Vec4::new(u(i, 0), u(i, 1), u(i, 2), u(i, 3))
        })),
        _ => {}
    }
}

/// An array's size, element (index in `b`, clamped), sum or average.
fn array_op(op: ArrayOp, a: &Reg, b: &Reg, out: &mut Reg) {
    let n = a.len();
    match op {
        ArrayOp::Size => ints(out).push(n as i32),
        ArrayOp::Element => {
            let i = value_at(b, 0).as_i32().clamp(0, n.saturating_sub(1) as i32) as usize;
            let v = if n == 0 { zero(value_at(a, 0).ty()) } else { value_at(a, i) };
            fill(out, v, 1);
        }
        ArrayOp::Sum | ArrayOp::Average => {
            let avg = op == ArrayOp::Average && n > 0;
            match a {
                Reg::Int(v) => {
                    let s: i64 = v.iter().map(|&x| x as i64).sum();
                    ints(out).push(if avg { (s / n as i64) as i32 } else { s as i32 });
                }
                Reg::Float(v) => { let s: f32 = v.iter().sum(); floats(out).push(if avg { s / n as f32 } else { s }); }
                Reg::Vec2(v) => { let s: Vec2 = v.iter().sum(); vec2s(out).push(if avg { s / n as f32 } else { s }); }
                Reg::Vec3(v) => { let s: Vec3 = v.iter().sum(); vec3s(out).push(if avg { s / n as f32 } else { s }); }
                Reg::Vec4(v) => { let s: Vec4 = v.iter().sum(); vec4s(out).push(if avg { s / n as f32 } else { s }); }
                _ => {}
            }
        }
    }
}

/// The point each corner is at, and each face's first corner.
fn corner_points(geo: &Geo) -> (&[u32], &[u32]) {
    match &geo.topology {
        Topology::Mesh { counts, indices, .. } => (counts.as_slice(), indices.as_slice()),
        _ => (&[], &[]),
    }
}

/// A number of a float or int column at element `e` (through its indices).
fn number_at(s: &Source, e: usize) -> f32 {
    let i = value_index(s.indices, e);
    match s.column {
        Some(Column::Float(v)) => i.and_then(|i| v.get(i)).copied().unwrap_or(0.0),
        Some(Column::Int(v)) => i.and_then(|i| v.get(i)).map(|&x| x as f32).unwrap_or(0.0),
        _ => 0.0,
    }
}

/// `count` places on the mesh's surface, spread evenly by area (scaled by
/// `weight`, per face or per point), the same for the same seed: each a
/// triangle of a face's fan picked by area, then a point in it.
fn scatter(geo: &Geo, seed: u32, count: usize, weight: Option<&Source>) -> Vec<Loc> {
    let (counts, corner_point) = corner_points(geo);
    let pts = geo.points();
    let at = |c: u32| Vec3::from_array(corner_point.get(c as usize).and_then(|&p| pts.get(p as usize)).copied().unwrap_or_default());
    let mut tris: Vec<(u32, [u32; 3])> = Vec::with_capacity(corner_point.len());
    let mut sums: Vec<f64> = Vec::with_capacity(corner_point.len());
    let (mut total, mut first) = (0.0f64, 0u32);
    for (face, &n) in counts.iter().enumerate() {
        for k in 1..n.saturating_sub(1) {
            let c = [first, first + k, first + k + 1];
            let area = (at(c[1]) - at(c[0])).cross(at(c[2]) - at(c[0])).length() * 0.5;
            let scale = match weight {
                None => 1.0,
                Some(w) if w.context == Context::Face => number_at(w, face),
                Some(w) => c.iter().map(|&c| number_at(w, corner_point.get(c as usize).copied().unwrap_or(0) as usize)).sum::<f32>() / 3.0,
            };
            total += (area * scale.max(0.0)) as f64;
            tris.push((face as u32, c));
            sums.push(total);
        }
        first += n;
    }
    if total <= 0.0 || tris.is_empty() { return vec![]; }
    // Each place is its own hash: made in parallel, the same either way.
    (0..count).into_par_iter().map(|i| {
        let t = sums.partition_point(|&s| s <= unit(seed, i, 0) as f64 * total).min(tris.len() - 1);
        let (face, corners) = tris[t];
        // Uniform in the triangle: a point in the unit square, folded.
        let (mut a, mut b) = (unit(seed, i, 1), unit(seed, i, 2));
        if a + b > 1.0 { (a, b) = (1.0 - a, 1.0 - b); }
        Loc { face, corners, weights: [1.0 - a - b, a, b] }
    }).collect()
}

/// An attribute at surface samples: a point's or a corner's weighted
/// across the sample's triangle, a face's that face's, the object's its one value.
/// Numbers and vectors are blended; a rotation blended and made unit again;
/// ints and bools take the nearest corner's.
fn at_location(geo: &Geo, s: &Source, at: &[Loc], out: &mut Reg) {
    let (_, corner_point) = corner_points(geo);
    // The elements a sample reads, with their weights.
    let elements = |l: &Loc| -> [(usize, f32); 3] {
        let w = l.weights;
        match s.context {
            Context::Point => std::array::from_fn(|k| (corner_point.get(l.corners[k] as usize).copied().unwrap_or(0) as usize, w[k])),
            Context::Corner => std::array::from_fn(|k| (l.corners[k] as usize, w[k])),
            Context::Face => [(l.face as usize, 1.0), (l.face as usize, 0.0), (l.face as usize, 0.0)],
            Context::Object => [(0, 1.0), (0, 0.0), (0, 0.0)],
        }
    };
    fn get<T: Copy + Default>(v: &[T], ix: Option<&[u32]>, e: usize) -> T {
        value_index(ix, e).and_then(|i| v.get(i)).copied().unwrap_or_default()
    }
    let nearest = |l: &Loc| { let e = elements(l); e.iter().copied().fold(e[0], |b, x| if x.1 > b.1 { x } else { b }).0 };
    let ix = s.indices;
    match s.column {
        Some(Column::Float(v)) => floats(out).extend(at.iter().map(|l| elements(l).iter().map(|&(e, w)| get(v, ix, e) * w).sum::<f32>())),
        Some(Column::Vec2(v)) => vec2s(out).extend(at.iter().map(|l| elements(l).iter().map(|&(e, w)| Vec2::from_array(get(v, ix, e)) * w).sum::<Vec2>())),
        Some(Column::Vec3(v)) => vec3s(out).extend(at.iter().map(|l| elements(l).iter().map(|&(e, w)| Vec3::from_array(get(v, ix, e)) * w).sum::<Vec3>())),
        Some(Column::Vec4(v)) => vec4s(out).extend(at.iter().map(|l| elements(l).iter().map(|&(e, w)| Vec4::from_array(get(v, ix, e)) * w).sum::<Vec4>())),
        Some(Column::Quat(v)) => quats(out).extend(at.iter().map(|l| {
            let q = elements(l).iter().map(|&(e, w)| Vec4::from_array(get(v, ix, e)) * w).sum::<Vec4>();
            Quat::from_vec4(q).normalize()
        })),
        Some(Column::Int(v)) => ints(out).extend(at.iter().map(|l| get(v, ix, nearest(l)))),
        Some(Column::Bool(v)) => bools(out).extend(at.iter().map(|l| get(v, ix, nearest(l)))),
        _ => {}
    }
}
