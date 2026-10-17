//! A plan run over a `Geo`.
//!
//! The uniform program runs once. The elements are then cut into batches,
//! run in parallel: each thread keeps its registers (one batch long) from
//! batch to batch, reads its slice of the columns the plan reads, and writes
//! its slice of the columns the plan sets. Nothing else is read or copied:
//! the result shares every other column with the input.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Vec2, Vec3, Vec4};
use rayon::prelude::*;

use xms_geo::{Attr, Column, Geo};

use super::compile::{Ins, Plan, Program};
use super::graph::Math;
use super::types::{Ty, Value};

/// Elements per batch: long enough that each instruction's loop does real
/// work, short enough that a thread's registers stay in cache.
pub const BATCH: usize = 4096;

/// One register: a value per element of the batch (one, in the uniform program).
pub(crate) enum Reg {
    Bool(Vec<bool>),
    Int(Vec<i32>),
    Float(Vec<f32>),
    Vec2(Vec<Vec2>),
    Vec3(Vec<Vec3>),
    Vec4(Vec<Vec4>),
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

fn registers(p: &Program) -> Vec<Reg> {
    p.regs.iter().map(|t| match t {
        Ty::Bool => Reg::Bool(vec![]),
        Ty::Int => Reg::Int(vec![]),
        Ty::Float => Reg::Float(vec![]),
        Ty::Vec2 => Reg::Vec2(vec![]),
        Ty::Vec3 => Reg::Vec3(vec![]),
        Ty::Vec4 => Reg::Vec4(vec![]),
    }).collect()
}

/// The value at element 0: a uniform register's value.
fn first(reg: &Reg) -> Value {
    match reg {
        Reg::Bool(v) => Value::Bool(v.first().copied().unwrap_or_default()),
        Reg::Int(v) => Value::Int(v.first().copied().unwrap_or_default()),
        Reg::Float(v) => Value::Float(v.first().copied().unwrap_or_default()),
        Reg::Vec2(v) => Value::Vec2(v.first().copied().unwrap_or_default().to_array()),
        Reg::Vec3(v) => Value::Vec3(v.first().copied().unwrap_or_default().to_array()),
        Reg::Vec4(v) => Value::Vec4(v.first().copied().unwrap_or_default().to_array()),
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
    }
}

/// A column the plan reads, and its indices when it is indexed.
struct Source<'a> {
    column:  Option<&'a Column>,
    indices: Option<&'a [u32]>,
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
    match (s.column, ty) {
        (Some(Column::Bool(v)), _) => gather(v, ix, start, len, bools(reg), false, |x| x),
        (Some(Column::Int(v)), _) => gather(v, ix, start, len, ints(reg), 0, |x| x),
        (Some(Column::Float(v)), _) => gather(v, ix, start, len, floats(reg), 0.0, |x| x),
        (Some(Column::Vec2(v)), _) => gather(v, ix, start, len, vec2s(reg), Vec2::ZERO, Vec2::from_array),
        (Some(Column::Vec3(v)), _) => gather(v, ix, start, len, vec3s(reg), Vec3::ZERO, Vec3::from_array),
        (Some(Column::Vec4(v)), _) => gather(v, ix, start, len, vec4s(reg), Vec4::ZERO, Vec4::from_array),
        // Not there in this geometry (it was when compiled): zeros.
        _ => fill(reg, zero(ty), len),
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
fn math(op: Math, regs: &[Reg], args: [usize; 3], out: &mut Reg) -> bool {
    use Reg::*;
    let (a, b, c) = (&regs[args[0]], regs.get(args[1]), regs.get(args[2]));
    match op {
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
                Bool(_) => return false,
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
}

/// One batch's part of a written column.
enum Slice<'a> {
    Bool(&'a mut [bool]),
    Int(&'a mut [i32]),
    Float(&'a mut [f32]),
    Vec2(&'a mut [[f32; 2]]),
    Vec3(&'a mut [[f32; 3]]),
    Vec4(&'a mut [[f32; 4]]),
}

fn store(reg: &Reg, out: &mut Slice) {
    match (reg, out) {
        (Reg::Bool(r), Slice::Bool(o)) => o.copy_from_slice(&r[..o.len()]),
        (Reg::Int(r), Slice::Int(o)) => o.copy_from_slice(&r[..o.len()]),
        (Reg::Float(r), Slice::Float(o)) => o.copy_from_slice(&r[..o.len()]),
        (Reg::Vec2(r), Slice::Vec2(o)) => o.iter_mut().zip(r).for_each(|(o, v)| *o = v.to_array()),
        (Reg::Vec3(r), Slice::Vec3(o)) => o.iter_mut().zip(r).for_each(|(o, v)| *o = v.to_array()),
        (Reg::Vec4(r), Slice::Vec4(o)) => o.iter_mut().zip(r).for_each(|(o, v)| *o = v.to_array()),
        _ => {}
    }
}

/// What a program reads from: the plan's sources, its parameters, the
/// uniform registers (for the batch program).
struct Env<'a> {
    sources:  Vec<Source<'a>>,
    params:   Vec<Value>,
    uniforms: Vec<Reg>,
}

/// Run `program` for elements `start..start + len`.
fn exec(program: &Program, regs: &mut [Reg], env: &Env, start: usize, len: usize, outs: &mut [Slice]) {
    for ins in &program.code {
        match *ins {
            Ins::Const { dst, value } => fill(&mut regs[dst], value, len),
            Ins::Param { dst, index } => fill(&mut regs[dst], env.params[index], len),
            Ins::Load { dst, read } => load(&env.sources[read], start, len, &mut regs[dst], program.regs[dst]),
            Ins::Broadcast { dst, from } => fill(&mut regs[dst], first(&env.uniforms[from]), len),
            Ins::Math { op, dst, args } => {
                let mut out = std::mem::replace(&mut regs[dst], Reg::Bool(vec![]));
                math(op, regs, args, &mut out);
                regs[dst] = out;
            }
            Ins::Store { write, src } => store(&regs[src], &mut outs[write]),
        }
    }
}

/// Run `plan` over `geo`. `params` gives values to the graph's parameters
/// by name; those not given take their defaults. The result shares every
/// column the plan does not set with `geo`.
pub fn run(plan: &Plan, geo: &Geo, params: &HashMap<String, Value>) -> Geo {
    let sources = plan.reads.iter().map(|r| {
        let attr = geo.attr(r.context, &r.name);
        Source { column: attr.map(|a| &a.column), indices: attr.and_then(|a| a.indices.as_deref().map(|v| v.as_slice())) }
    }).collect();
    let params = plan.params.iter()
        .map(|(name, default)| params.get(name).and_then(|v| v.convert(default.ty())).unwrap_or(*default))
        .collect();
    let mut env = Env { sources, params, uniforms: vec![] };

    // Once: what does not vary per element.
    let mut uniforms = registers(&plan.uniform);
    exec(&plan.uniform, &mut uniforms, &env, 0, 1, &mut []);
    env.uniforms = uniforms;

    let n = geo.count(plan.context);
    if plan.writes.is_empty() || n == 0 { return geo.clone(); }

    // The written columns, then each batch's slice of each.
    let mut cols: Vec<Out> = plan.writes.iter().map(|w| match w.ty {
        Ty::Bool => Out::Bool(vec![false; n]),
        Ty::Int => Out::Int(vec![0; n]),
        Ty::Float => Out::Float(vec![0.0; n]),
        Ty::Vec2 => Out::Vec2(vec![[0.0; 2]; n]),
        Ty::Vec3 => Out::Vec3(vec![[0.0; 3]; n]),
        Ty::Vec4 => Out::Vec4(vec![[0.0; 4]; n]),
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
        };
        out.set(plan.context, &w.name, Attr::new(column, w.role));
    }
    out
}
