//! Graph to plan.
//!
//! Every value is given a register in one of two programs. The **uniform**
//! program runs once per evaluation, on registers one element long: constants,
//! parameters, Object attributes and maths on nothing but those. The **batch**
//! program runs per batch of elements: what varies per element. A uniform value
//! a batch instruction needs is broadcast into a batch register once, at the
//! start of each batch.
//!
//! Only what reaches the root is compiled, so a node left unwired costs
//! nothing, and only the attributes the compiled nodes read are read.

use std::collections::{HashMap, HashSet};

use xms_geo::{Column, Context, Geo, Role, POINTS};

use super::derived;
use super::graph::{
    ArrayOp, Compose, Decompose, Graph, Input, Math, Op, Resample, CLOSEST_DISTANCE, SCATTER_NORMALS, SCATTER_SAMPLES,
};
use super::surface::Reference;
use super::types::{Ty, Value};

/// A plan: what to read, the two programs, what to write. Made once per graph
/// (and geometry layout); run as often as wanted, with new parameter values
/// each time.
#[derive(Clone, Debug)]
pub struct Plan {
    /// The elements the graph runs over.
    pub context: Context,
    pub(crate) reads: Vec<Read>,
    pub(crate) params: Vec<(String, Value)>,
    pub(crate) uniform: Program,
    pub(crate) batch: Program,
    pub(crate) writes: Vec<Write>,
    /// Instancers, made from uniform registers once the plan has run.
    pub(crate) instancers: Vec<InstancerPlan>,
    /// Surfaces Closest Point on Surface searches: the uniform register of each
    /// one's paths (Get Reference's), made into a BVH when first run.
    pub(crate) surfaces: Vec<usize>,
}

/// An Instancer as the plan runs it: the uniform registers holding its inputs.
#[derive(Clone, Debug)]
pub(crate) struct InstancerPlan {
    pub path: String,
    pub seed: u32,
    pub positions: usize,
    pub protos: usize,
    pub index: usize,
    pub orient: usize,
    pub scale: usize,
}

impl Plan {
    /// The attributes the plan reads, with their context.
    pub fn reads(&self) -> impl Iterator<Item = (&str, Context)> {
        self.reads.iter().map(|r| (r.name.as_str(), r.context))
    }

    /// The attributes the plan sets.
    pub fn writes(&self) -> impl Iterator<Item = &str> {
        self.writes.iter().map(|w| w.name.as_str())
    }

    /// Instructions per batch: what each element costs.
    pub fn batch_len(&self) -> usize {
        self.batch.code.len()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Read {
    /// The column's name in the `Geo` (`points` for `P`).
    pub name: String,
    /// The context the attribute is in: the plan's own, Object, or another one
    /// read across (see `Op::GetData`).
    pub context: Context,
    /// How the values an element touches across contexts become one.
    pub resample: Resample,
    /// Made from the topology when run (`derived`), not a stored column.
    pub derived: bool,
    /// Read at surface samples (Get Data at Samples, Scatter's weight and
    /// outputs), not element by element: no mapping between contexts is made
    /// for it.
    pub at_location: bool,
    /// Read at samples of a surface, mesh by mesh: of the graph's own geometry
    /// (`Some(None)`), or of surface `k` (`Some(Some(k))`): by Get Data at
    /// Samples on Closest Point's samples, and by Get Array Data at Samples.
    /// `name` is then the name as given (each mesh finds its own column).
    pub sampled: Option<Option<usize>>,
    /// Read from surface `k`'s first prim instead of the geometry run on,
    /// element by element (the same topology): Get Data and Get Array Data with
    /// a Geometry. `name` is then the name as given.
    pub from: Option<usize>,
}

#[derive(Clone, Debug)]
pub(crate) struct Write {
    pub name: String,
    pub ty: Ty,
    pub role: Role,
    /// Where it is written: the plan's context, or another one, each of whose
    /// elements takes the plan's elements it touches, made one.
    pub context: Context,
    pub resample: Resample,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Program {
    pub code: Vec<Ins>,
    /// The type of each register.
    pub regs: Vec<Ty>,
}

#[derive(Clone, Debug)]
pub(crate) enum Ins {
    Const {
        dst: usize,
        value: Value,
    },
    /// Parameter number `index` of the plan.
    Param {
        dst: usize,
        index: usize,
    },
    /// Read number `read`: the batch's elements, or element 0 in the uniform
    /// program.
    Load {
        dst: usize,
        read: usize,
    },
    /// Uniform register `from`, spread over a batch register.
    Broadcast {
        dst: usize,
        from: usize,
    },
    Math {
        op: Math,
        dst: usize,
        args: [usize; 3],
    },
    /// An int as a float, where an int meets a float.
    ToFloat {
        dst: usize,
        src: usize,
    },
    /// Random values between `args[0]` and `args[1]`: one per element, or, when
    /// `array`, an array of `args[2]` (an int; `NO_COUNT`, as many as the keys
    /// in `args[3]`) of them. `key`: what each is drawn for.
    Random {
        dst: usize,
        seed: u32,
        args: [usize; 4],
        array: bool,
        key: Key,
    },
    /// Arrays (or single values) one after the other.
    Concat {
        dst: usize,
        srcs: Vec<usize>,
    },
    /// Read number `read` as lists: what each element touches in the read's
    /// context, every value kept. `flat`: in the one-value program, the one
    /// element's list as an array.
    LoadList {
        dst: usize,
        read: usize,
        flat: bool,
    },
    /// An array's size, element `args[1]`, sum or average.
    Array {
        op: ArrayOp,
        dst: usize,
        args: [usize; 2],
    },
    /// A vector from its parts, in `args` (as many as `op` takes).
    Compose {
        op: Compose,
        dst: usize,
        args: [usize; 4],
    },
    /// A vector taken apart, into `dsts` (as many as `op` gives).
    Decompose {
        op: Decompose,
        dsts: [usize; 4],
        src: usize,
    },
    /// `count` (a register) places on the surface; `weight`, a read scaling how
    /// many land where.
    Scatter {
        dst: usize,
        seed: u32,
        count: usize,
        weight: Option<usize>,
    },
    /// Read number `read` at the surface samples in register `loc`.
    AtLocation {
        dst: usize,
        read: usize,
        loc: usize,
    },
    /// The samples of surface `surface` nearest the positions in `origin`,
    /// within the distances in `radius` (0: anywhere), into `dst`, and how far
    /// each is into `dist`.
    Closest {
        dst: usize,
        dist: usize,
        surface: usize,
        origin: usize,
        radius: usize,
    },
    /// Read number `read` (a `sampled` one) at the samples in `loc`, on surface
    /// `surface` (`None`: the graph's own geometry).
    AtSurface {
        dst: usize,
        read: usize,
        loc: usize,
        surface: Option<usize>,
    },
    /// Read number `read` as lists at the samples in `loc`: the values of the
    /// face each is on (its points', its corners'). `flat`: one sample, its
    /// list as an array.
    ListAt {
        dst: usize,
        read: usize,
        loc: usize,
        surface: Option<usize>,
        flat: bool,
    },
    /// The weights of the samples in `loc` on their triangles' corners: a 3D
    /// vector each, or (`list`) three numbers each.
    Bary {
        dst: usize,
        loc: usize,
        list: bool,
    },
    /// The three points of each sample's triangle, in the graph's space: a list
    /// each (`flat`: one sample's, as an array).
    TriP {
        dst: usize,
        loc: usize,
        surface: Option<usize>,
        flat: bool,
    },
    /// Random rotations up along the normals in `args[0]`, turned at random up
    /// to `args[1]` degrees: one per element, or, when `array`, as many as the
    /// normals (an array) or `args[2]` (a count; `NO_COUNT`, as many as the
    /// keys in `args[3]`). `key`: what each is drawn for.
    RandomOrient {
        dst: usize,
        seed: u32,
        args: [usize; 4],
        array: bool,
        key: Key,
    },
    /// The paths of reference input `input` (from 1): all, or those of `paths`
    /// that are there.
    Reference {
        dst: usize,
        input: usize,
        paths: Option<Vec<String>>,
    },
    /// Into write number `write`.
    Store {
        write: usize,
        src: usize,
    },
}

/// Why a graph does not compile, and at which node: shown on the node, and on
/// the wire into it, in the editor.
#[derive(Clone, Debug, PartialEq)]
pub struct CompileError {
    pub node: usize,
    pub message: String,
}

fn error<T>(node: usize, message: impl Into<String>) -> Result<T, CompileError> {
    Err(CompileError {
        node,
        message: message.into(),
    })
}

/// What a list per element holds the items of, for lists to meet in maths only
/// when they are as long as each other, element by element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListOf {
    /// What each element touches in a context (Get Array Data).
    Touch(Context),
    /// The face of each sample in a register (Get Array Data at Samples).
    Face(usize),
    /// The triangle of each sample in a register: three items (`triP`, `bary`
    /// as a list).
    Tri(usize),
}

/// The contexts an attribute read at samples is looked for in: the one asked
/// for, else point, corner, face, object.
fn sample_contexts(context: Option<Context>) -> Vec<Context> {
    match context {
        Some(c) => vec![c],
        None => vec![Context::Point, Context::Corner, Context::Face, Context::Object],
    }
}

/// The name of an attribute's column: ICE's `P` is the points.
fn column_name(name: &str) -> &str {
    if name == "P" {
        POINTS
    } else {
        name
    }
}

/// The column an attribute name reads in `ctx`: the name itself, else the short
/// names ICE and Houdini use (`P` the points, `N` the normals, `uv` the first
/// texture coordinates).
pub(crate) fn column_in<'a>(geo: &'a Geo, ctx: Context, name: &'a str) -> &'a str {
    if geo.attr(ctx, name).is_some() {
        return name;
    }
    match name {
        "P" => POINTS,
        "N" if geo.attr(ctx, xms_geo::NORMALS).is_some() => xms_geo::NORMALS,
        "uv" | "UV" => xms_geo::UV_NAMES
            .iter()
            .copied()
            .find(|n| geo.attr(ctx, n).is_some())
            .unwrap_or(name),
        _ => name,
    }
}

/// What reading `name` in `ctx` finds: a stored attribute, or one made from the
/// topology (`derived`), with its type (`None`: a type ICE does not read).
struct Found {
    ty: Option<Ty>,
    derived: bool,
}

fn find_in(geo: &Geo, ctx: Context, name: &str) -> Option<Found> {
    if let Some(a) = geo.attr(ctx, column_in(geo, ctx, name)) {
        return Some(Found {
            ty: column_ty(&a.column),
            derived: false,
        });
    }
    derived::ty(geo, ctx, name).map(|t| Found {
        ty: Some(t),
        derived: true,
    })
}

/// The context `name` is in, stored or derived, if any.
fn context_of(geo: &Geo, name: &str) -> Option<Context> {
    geo.find(column_name(name))
        .map(|(c, _)| c)
        .or_else(|| derived::context_of(geo, name))
}

fn column_ty(c: &Column) -> Option<Ty> {
    Some(match c {
        Column::Bool(_) => Ty::Bool,
        Column::Int(_) => Ty::Int,
        Column::Float(_) => Ty::Float,
        Column::Vec2(_) => Ty::Vec2,
        Column::Vec3(_) => Ty::Vec3,
        Column::Vec4(_) => Ty::Vec4,
        Column::Quat(_) => Ty::Quat,
        Column::Mat4(_) => Ty::Mat4,
        Column::Token(_) => return None,
    })
}

/// The type a maths node gives for the types it gets, or `None` when it does
/// not take them.
fn math_ty(op: Math, args: &[Ty]) -> Option<Ty> {
    use Ty::*;
    let numeric = |t: Ty| t.is_numeric();
    match (op, args) {
        (Math::Add | Math::Subtract | Math::Min | Math::Max, [a, b]) if a == b && numeric(*a) => Some(*a),
        (Math::Multiply | Math::Divide, [a, b]) if a == b && numeric(*a) => Some(*a),
        (Math::Multiply | Math::Divide, [v, Float]) if v.is_vector() => Some(*v),
        (Math::Multiply, [Float, v]) if v.is_vector() => Some(*v),
        (Math::Lerp, [a, b, Float]) if a == b && (*a == Float || a.is_vector()) => Some(*a),
        (Math::Negate | Math::Abs, [a]) if numeric(*a) => Some(*a),
        (Math::Sin | Math::Cos, [Float]) => Some(Float),
        (Math::Dot, [a, b]) if a == b && a.is_vector() => Some(Float),
        (Math::Cross, [Vec3, Vec3]) => Some(Vec3),
        (Math::Length, [v]) if v.is_vector() => Some(Float),
        (Math::Normalize, [v]) if v.is_vector() => Some(*v),
        // Rotations: combined by multiplying, blended along the shortest arc.
        (Math::Multiply, [Quat, Quat]) => Some(Quat),
        (Math::Normalize, [Quat]) => Some(Quat),
        (Math::Lerp, [Quat, Quat, Float]) => Some(Quat),
        (Math::Rotate, [Quat, Vec3]) => Some(Vec3),
        (Math::AxisAngle, [Vec3, Float]) => Some(Quat),
        (Math::Euler, [Vec3]) => Some(Quat),
        (Math::Distance, [a, b]) if a == b && a.is_vector() => Some(Float),
        // Matrices: combined by multiplying (the right one first), inverted,
        // transposed, applied to positions and directions.
        (Math::Multiply, [Mat4, Mat4]) => Some(Mat4),
        (Math::Invert | Math::Transpose, [Mat4]) => Some(Mat4),
        (Math::TransformPoint | Math::TransformVector, [Mat4, Vec3]) => Some(Vec3),
        _ => None,
    }
}

/// Whether input `i` of `op` stays a float when a value: a scale, a lerp's t,
/// an angle. Every other value takes the type of what it meets.
fn keeps_float(op: Math, i: usize) -> bool {
    matches!(op, Math::Multiply | Math::Divide) || (op == Math::Lerp && i == 2) || (op == Math::AxisAngle && i == 1)
}

/// The type an array operation gives for the element type it gets.
fn array_ty(op: ArrayOp, of: Ty) -> Option<Ty> {
    match op {
        ArrayOp::Size => Some(Ty::Int),
        // A path is never one value: it stays in its array.
        ArrayOp::Element => (of != Ty::Token).then_some(of),
        ArrayOp::Sum | ArrayOp::Average => of.is_numeric().then_some(of),
    }
}

/// What a random value is drawn for: the element (or the array's index), unless
/// a Seed is wired. One seed for all moves every value at once; a seed per
/// element (`ptIndex`, an `id`, a face's index from its points) draws each for
/// its seed, so elements of one seed get one value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Key {
    Element,
    One,
    Each,
}

/// No count register: an array as long as its keys.
pub(crate) const NO_COUNT: usize = usize::MAX;

/// A value in a register: of a type, in one of the two programs, and in the
/// uniform one possibly an array (one register of many values).
#[derive(Clone, Copy, Debug)]
struct Val {
    reg: usize,
    uniform: bool,
    ty: Ty,
    array: bool,
    /// A list per element (`array` and not `uniform`, from Get Array Data):
    /// what its items are. Lists of one kind have the same lengths, element by
    /// element, and meet in maths.
    of: Option<ListOf>,
    /// Surface samples: the surface they are on (`plan.surfaces`), `None` for
    /// the graph's own geometry.
    surface: Option<usize>,
}

impl Val {
    /// A list per element, not the graph's one array.
    fn is_list(&self) -> bool {
        self.array && !self.uniform
    }
}

struct Lower<'a> {
    graph: &'a Graph,
    geo: &'a Geo,
    plan: Plan,
    /// Node outputs already lowered: output 0 by node.
    done: HashMap<usize, Val>,
    /// Other outputs already lowered (Scatter's normals and samples' reads).
    outs: HashMap<(usize, usize), Val>,
    /// The reference inputs: their meshes, for the types of what is read at
    /// their samples.
    refs: &'a [Reference],
    /// Nodes being lowered: meeting one again is a cycle.
    visiting: HashSet<usize>,
    /// Batch registers holding a uniform register, so each is broadcast once.
    broadcast: HashMap<usize, usize>,
    reads: HashMap<String, usize>,
    /// Attributes set so far, in Execute order: a later Get Data of one of them
    /// gets the value set, as when the ports run one after the other.
    written: HashMap<String, Val>,
    set: HashSet<usize>,
}

impl Lower<'_> {
    fn emit(&mut self, uniform: bool, ty: Ty, make: impl FnOnce(usize) -> Ins) -> Val {
        let p = if uniform {
            &mut self.plan.uniform
        } else {
            &mut self.plan.batch
        };
        let reg = p.regs.len();
        p.regs.push(ty);
        p.code.push(make(reg));
        Val {
            reg,
            uniform,
            ty,
            array: false,
            of: None,
            surface: None,
        }
    }

    /// A register of `ty`, filled by an instruction that fills others too.
    fn alloc(&mut self, uniform: bool, ty: Ty) -> usize {
        let p = if uniform {
            &mut self.plan.uniform
        } else {
            &mut self.plan.batch
        };
        p.regs.push(ty);
        p.regs.len() - 1
    }

    /// `v` in a batch register. Never an array: the compiler checks before.
    fn varying(&mut self, v: Val) -> usize {
        if !v.uniform {
            return v.reg;
        }
        if let Some(&r) = self.broadcast.get(&v.reg) {
            return r;
        }
        let r = self.emit(false, v.ty, |dst| Ins::Broadcast { dst, from: v.reg }).reg;
        self.broadcast.insert(v.reg, r);
        r
    }

    fn constant(&mut self, value: Value) -> Val {
        self.emit(true, value.ty(), |dst| Ins::Const { dst, value })
    }

    /// The value of output `output` of a node: Scatter on Surface's positions
    /// and normals are read at its samples (once each); every other node has
    /// one output.
    fn wired(&mut self, node: usize, output: usize) -> Result<Val, CompileError> {
        let graph = self.graph;
        if let Some(v) = self.outs.get(&(node, output)) {
            return Ok(*v);
        }
        match graph.nodes.get(node).map(|n| &n.op) {
            // Lowered once, every output kept (in `outs`).
            Some(Op::Decompose(_)) => {
                self.value(node)?;
                return self.outs.get(&(node, output)).copied().ok_or(CompileError {
                    node,
                    message: format!("No output {output}"),
                });
            }
            // Its samples are its value; its distances kept in `outs`.
            Some(Op::ClosestPoint) if output == CLOSEST_DISTANCE => {
                self.value(node)?;
                return self.outs.get(&(node, output)).copied().ok_or(CompileError {
                    node,
                    message: format!("No output {output}"),
                });
            }
            Some(Op::Scatter { .. }) if output != SCATTER_SAMPLES => {}
            _ => return self.value(node),
        }
        let samples = self.value(node)?;
        let read = if output == SCATTER_NORMALS {
            // The mesh's normals where it has some, else each face's own.
            let found = [Context::Point, Context::Corner, Context::Face]
                .into_iter()
                .find_map(|c| {
                    find_in(self.geo, c, "N")
                        .filter(|f| f.ty == Some(Ty::Vec3))
                        .map(|f| (c, f.derived))
                });
            let Some((c, derived)) = found else {
                return error(node, "No normals to give: the mesh has none, nor faces");
            };
            self.location_read(c, "N", derived)
        } else {
            self.location_read(Context::Point, "P", false)
        };
        let mut v = self.emit(true, Ty::Vec3, |dst| Ins::AtLocation {
            dst,
            read,
            loc: samples.reg,
        });
        v.array = true;
        self.outs.insert((node, output), v);
        Ok(v)
    }

    /// The value of a node's output (Scatter's: its samples).
    fn value(&mut self, node: usize) -> Result<Val, CompileError> {
        if let Some(v) = self.done.get(&node) {
            return Ok(*v);
        }
        if !self.visiting.insert(node) {
            return error(node, "This node feeds itself: a cycle");
        }
        let graph = self.graph;
        let n = graph.nodes.get(node).ok_or(CompileError {
            node,
            message: "No such node".into(),
        })?;
        if let Some(a) = n.op.arity() {
            if n.inputs.len() != a {
                return error(node, format!("Takes {a} inputs, has {}", n.inputs.len()));
            }
        }
        let v = match &n.op {
            Op::Root { .. } => return error(node, "The root has no output"),
            Op::SetData { .. } => return error(node, "Set Data gives an Execute: wire it into the root"),
            Op::Constant(value) => self.constant(*value),
            Op::Parameter { name, default } => {
                let index = match self.plan.params.iter().position(|(p, _)| p == name) {
                    Some(i) => i,
                    None => {
                        self.plan.params.push((name.clone(), *default));
                        self.plan.params.len() - 1
                    }
                };
                self.emit(true, default.ty(), |dst| Ins::Param { dst, index })
            }
            // With a Geometry: read from it.
            Op::GetData {
                name,
                context,
                resample,
            } if !n.inputs.is_empty() => self.get_data_from(node, name, *context, *resample)?,
            Op::GetData {
                name,
                context,
                resample,
            } => self.get_data(node, name, *context, *resample)?,
            Op::Math(op) => self.math(node, *op)?,
            Op::Random { seed } => self.random(node, *seed)?,
            Op::Array(op) => self.array(node, *op)?,
            Op::Compose(c) => self.compose(node, *c)?,
            Op::Decompose(d) => self.decompose(node, *d)?,
            Op::Scatter { seed, weight } => self.scatter(node, *seed, weight)?,
            Op::GetDataAt { name, context } => self.at_location(node, name, *context)?,
            Op::RandomOrientation { seed } => self.random_orientation(node, *seed)?,
            Op::Concat => self.concat(node)?,
            Op::GetArrayData { name, context } if !n.inputs.is_empty() => {
                self.get_array_data_from(node, name, *context)?
            }
            Op::GetArrayData { name, context } => self.get_array_data(node, name, *context)?,
            Op::ClosestPoint => self.closest(node)?,
            Op::GetArrayDataAt { name, context } => self.array_at(node, name, *context)?,
            Op::Instancer { .. } => return error(node, "The Instancer gives an Execute: wire it into the root"),
            Op::GetReference { input, paths } => {
                if *input == 0 {
                    return error(node, "Reference inputs are numbered from 1");
                }
                let (input, paths) = (*input, paths.clone());
                let mut v = self.emit(true, Ty::Token, |dst| Ins::Reference { dst, input, paths });
                v.array = true;
                v
            }
        };
        self.visiting.remove(&node);
        self.done.insert(node, v);
        Ok(v)
    }

    fn get_data(
        &mut self,
        node: usize,
        name: &str,
        from: Option<Context>,
        resample: Resample,
    ) -> Result<Val, CompileError> {
        let (geo, context) = (self.geo, self.plan.context);
        let own = from.is_none() || from == Some(context);
        // Set by an earlier port, in the graph's own context: the new values.
        if own {
            if let Some(v) = self.written.get(column_in(geo, context, name)) {
                return Ok(*v);
            }
        }
        let attr_context = match from {
            // The graph's own context varies per element; Object is one value.
            None => {
                if find_in(geo, context, name).is_some() {
                    context
                } else if find_in(geo, Context::Object, name).is_some() {
                    Context::Object
                } else if let Some(other) = context_of(geo, name) {
                    return error(node, format!("'{name}' is per {other:?}; this graph runs per {context:?}: set the Get Data's context to {other:?} to read it across"));
                } else {
                    return error(node, format!("No attribute '{name}'"));
                }
            }
            Some(c) => {
                if find_in(geo, c, name).is_none() {
                    return match context_of(geo, name) {
                        Some(other) => error(node, format!("'{name}' is per {other:?}, not per {c:?}")),
                        None => error(node, format!("No attribute '{name}'")),
                    };
                }
                c
            }
        };
        let found = find_in(geo, attr_context, name).expect("found above");
        let column = if found.derived {
            name
        } else {
            column_in(geo, attr_context, name)
        };
        let Some(ty) = found.ty else {
            return error(node, format!("'{name}' is of a type ICE does not read yet (tokens)"));
        };
        // Object is one value for every element.
        let uniform = attr_context == Context::Object;
        // Read across contexts, each reduction is a read of its own.
        let resample = if attr_context == context || uniform {
            Resample::Average
        } else {
            resample
        };
        let key = format!("{attr_context:?}/{column}/{resample:?}");
        let read = match self.reads.get(&key) {
            Some(&r) => r,
            None => {
                self.plan.reads.push(Read {
                    name: column.to_string(),
                    context: attr_context,
                    resample,
                    derived: found.derived,
                    at_location: false,
                    sampled: None,
                    from: None,
                });
                self.reads.insert(key, self.plan.reads.len() - 1);
                self.plan.reads.len() - 1
            }
        };
        Ok(self.emit(uniform, ty, |dst| Ins::Load { dst, read }))
    }

    fn math(&mut self, node: usize, op: Math) -> Result<Val, CompileError> {
        let graph = self.graph;
        let inputs = &graph.nodes[node].inputs;
        // Wired inputs first: an unconnected value takes the type of what it
        // meets (a 1 next to a vector is that vector's 1, 1, 1), except the
        // float of a scale or of a lerp's t.
        let mut vals: Vec<Option<Val>> = vec![None; inputs.len()];
        for (i, input) in inputs.iter().enumerate() {
            if let Input::Wire { node: from, output } = input {
                vals[i] = Some(self.wired(*from, *output)?);
            }
        }
        let wired = vals.iter().flatten().map(|v| v.ty).find(|t| *t != Ty::Float).or(vals
            .iter()
            .flatten()
            .map(|v| v.ty)
            .next());
        for (i, input) in inputs.iter().enumerate() {
            if let Input::Value(value) = input {
                let keep_float = value.ty() == Ty::Float && keeps_float(op, i);
                let value = match wired {
                    Some(t) if !keep_float => value.convert(t).unwrap_or(*value),
                    _ => *value,
                };
                vals[i] = Some(self.constant(value));
            }
        }
        let mut vals: Vec<Val> = vals.into_iter().flatten().collect();
        // An int meeting a float is taken as a float (a face's corner count
        // plus 0.5).
        if vals.iter().any(|v| v.ty == Ty::Float) {
            for v in vals.iter_mut().filter(|v| v.ty == Ty::Int) {
                let (src, array, of) = (v.reg, v.array, v.of);
                *v = self.emit(v.uniform, Ty::Float, |dst| Ins::ToFloat { dst, src });
                v.array = array;
                v.of = of;
            }
        }
        let tys: Vec<Ty> = vals.iter().map(|v| v.ty).collect();
        let Some(ty) = math_ty(op, &tys) else {
            return error(node, format!("{op:?} does not take {tys:?}"));
        };
        let uniform = vals.iter().all(|v| v.uniform);
        // An array is one value of the graph's one-value part, worked on
        // element by element. A list per element (Get Array Data) is worked
        // on item by item, a per-element value meeting each of its items;
        // lists meet lists of the same context.
        let array = vals.iter().any(|v| v.array);
        let list = vals.iter().find(|v| v.is_list()).and_then(|v| v.of);
        if let Some(of) = list {
            if vals.iter().any(|v| v.array && v.uniform) {
                return error(node, "An Object array and a list per element do not meet");
            }
            if vals.iter().any(|v| v.is_list() && v.of != Some(of)) {
                return error(node, "Lists of two kinds do not meet: their lengths differ element by element (a face's points and the faces around a point; a sample's face and an element's own)");
            }
        } else if array && !uniform {
            return error(node, "An array meets per-element values: arrays work in the graph's one-value part (an Object graph) for now");
        }
        let mut args = [0usize; 3];
        for (i, v) in vals.iter().enumerate() {
            args[i] = if uniform { v.reg } else { self.varying(*v) };
        }
        let mut out = self.emit(uniform, ty, |dst| Ins::Math { op, dst, args });
        out.array = array;
        out.of = list;
        Ok(out)
    }

    /// An input as a value: wired, or its own value taken as `ty` where it
    /// converts.
    fn input(&mut self, node: usize, i: usize, ty: Option<Ty>) -> Result<Val, CompileError> {
        let graph = self.graph;
        match &graph.nodes[node].inputs[i] {
            Input::Wire { node: from, output } => self.wired(*from, *output),
            Input::Value(v) => Ok(self.constant(ty.and_then(|t| v.convert(t)).unwrap_or(*v))),
        }
    }

    /// A Seed input (input `i`): unwired, the element's own number; wired, an
    /// int, one for all or one per element (or per element of an array).
    fn seed_input(&mut self, node: usize, i: usize) -> Result<Option<Val>, CompileError> {
        let graph = self.graph;
        match graph.nodes[node].inputs.get(i) {
            Some(Input::Wire { node: from, output }) => {
                let v = self.wired(*from, *output)?;
                if v.ty != Ty::Int {
                    return error(
                        node,
                        format!("The Seed is an int (ptIndex, faceIndex, an id), not a {}", v.ty.name()),
                    );
                }
                Ok(Some(v))
            }
            _ => Ok(None),
        }
    }

    /// The count of an array of random values (input `i`): `None` for 0 (one
    /// per element), else one int for the whole graph, or an array wired in
    /// (Scatter's positions): as many as it holds.
    fn count_input(&mut self, node: usize, i: usize) -> Result<Option<Val>, CompileError> {
        let graph = self.graph;
        let count = match &graph.nodes[node].inputs[i] {
            Input::Value(Value::Int(c)) if *c <= 0 => None,
            Input::Value(Value::Float(c)) if *c <= 0.0 => None,
            _ => Some(self.input(node, i, Some(Ty::Int))?),
        };
        // An array: its size.
        let count = match count {
            Some(a) if a.array && a.uniform => Some(self.emit(true, Ty::Int, |dst| Ins::Array {
                op: ArrayOp::Size,
                dst,
                args: [a.reg, a.reg],
            })),
            Some(a) if a.is_list() => {
                return error(
                    node,
                    "The count is one int, or the graph's array (Object graphs); a list per element is not",
                )
            }
            other => other,
        };
        if let Some(c) = count {
            if c.ty != Ty::Int || !c.uniform || c.array {
                return error(
                    node,
                    "The count is one int for the whole graph (a value, a parameter, an Object attribute)",
                );
            }
        }
        Ok(count)
    }

    fn random(&mut self, node: usize, seed: u32) -> Result<Val, CompileError> {
        let graph = self.graph;
        let inputs = &graph.nodes[node].inputs;
        // Min and max meet at one type: what is wired, else the min's.
        let wired = (0..2).find_map(|i| match &inputs[i] {
            Input::Wire { node: from, output } => Some((*from, *output)),
            _ => None,
        });
        let ty = match wired {
            Some((from, output)) => Some(self.wired(from, output)?.ty),
            None => match (&inputs[0], &inputs[1]) {
                // An int and a float meet as floats.
                (Input::Value(a), Input::Value(b)) if a.ty() != b.ty() => Some(Ty::Float),
                (Input::Value(a), _) => Some(a.ty()),
                _ => None,
            },
        };
        let (lo, hi) = (self.input(node, 0, ty)?, self.input(node, 1, ty)?);
        if lo.ty != hi.ty || !(lo.ty.is_numeric()) {
            return error(
                node,
                format!(
                    "Random takes two numbers or vectors of one type, not {:?} and {:?}",
                    lo.ty, hi.ty
                ),
            );
        }
        if lo.array || hi.array {
            return error(node, "Random's min and max are single values");
        }
        let count = self.count_input(node, 2)?;
        let key = self.seed_input(node, 3)?;
        let ty = lo.ty;
        self.drawn(
            node,
            ty,
            [Some(lo), Some(hi)],
            count,
            key,
            None,
            |dst, args, array, key| Ins::Random {
                dst,
                seed,
                args,
                array,
                key,
            },
        )
    }

    /// A random value of `ty` from `vals` (registers taken one per element or
    /// one for all): an array when there is a count, a seed per element of an
    /// array, or (`array_of`) an array to take the length of; else one per
    /// element of the graph's context (one, per Object).
    #[allow(clippy::too_many_arguments)]
    fn drawn(
        &mut self,
        node: usize,
        ty: Ty,
        vals: [Option<Val>; 2],
        count: Option<Val>,
        key: Option<Val>,
        array_of: Option<Val>,
        make: impl FnOnce(usize, [usize; 4], bool, Key) -> Ins,
    ) -> Result<Val, CompileError> {
        let key_kind = match key {
            None => Key::Element,
            Some(k) if k.uniform && !k.array => Key::One,
            Some(_) => Key::Each,
        };
        let array = count.is_some() || array_of.is_some() || key.is_some_and(|k| k.array);
        if array {
            // In the graph's one-value part: everything one value, or arrays.
            if vals.iter().flatten().chain(key.iter()).any(|v| !v.uniform) {
                return error(node, "An array of random values takes one value per input, or arrays (Object graphs); per-element inputs give one value per element: set the Count to 0");
            }
            let k = key.map(|k| k.reg).unwrap_or(0);
            let c = match (count, array_of) {
                (Some(c), _) => c.reg,
                (None, Some(a)) => a.reg,
                (None, None) => NO_COUNT,
            };
            let regs = vals.map(|v| v.map(|v| v.reg).unwrap_or(0));
            let mut v = self.emit(true, ty, |dst| make(dst, [regs[0], regs[1], c, k], true, key_kind));
            v.array = true;
            return Ok(v);
        }
        // One value, in an Object graph: its one element.
        if self.plan.context == Context::Object && vals.iter().flatten().chain(key.iter()).all(|v| v.uniform) {
            let regs = vals.map(|v| v.map(|v| v.reg).unwrap_or(0));
            let k = key.map(|k| k.reg).unwrap_or(0);
            return Ok(self.emit(true, ty, |dst| make(dst, [regs[0], regs[1], 0, k], false, key_kind)));
        }
        // One per element.
        let regs = [
            vals[0].map(|v| self.varying(v)).unwrap_or(0),
            vals[1].map(|v| self.varying(v)).unwrap_or(0),
        ];
        let k = match key {
            Some(k) => self.varying(k),
            None => 0,
        };
        Ok(self.emit(false, ty, |dst| make(dst, [regs[0], regs[1], 0, k], false, key_kind)))
    }

    /// Get Array Data: the values of `name` each element touches in `from` (its
    /// own context when not given), kept as a list: a face's points' positions,
    /// its point numbers (`ptIndices`). In an Object graph, the one list, as
    /// the graph's array.
    fn get_array_data(&mut self, node: usize, name: &str, from: Option<Context>) -> Result<Val, CompileError> {
        let (geo, context) = (self.geo, self.plan.context);
        // The intrinsic lists by their own names: a face's points, a
        // point's faces.
        let (name, from) = match (name.trim(), from) {
            ("ptIndices", None) => ("ptIndex", Some(Context::Point)),
            ("faceIndices", None) => ("faceIndex", Some(Context::Face)),
            (n, f) => (n, f),
        };
        if name.is_empty() {
            return error(node, "Get Array Data needs the name of the attribute it reads");
        }
        let Some(from) = from.or_else(|| context_of(geo, name)) else {
            return error(node, format!("No attribute '{name}'"));
        };
        let Some(found) = find_in(geo, from, name) else {
            return match context_of(geo, name) {
                Some(other) => error(node, format!("'{name}' is per {other:?}, not per {from:?}")),
                None => error(node, format!("No attribute '{name}'")),
            };
        };
        let Some(ty) = found.ty else {
            return error(node, format!("'{name}' is of a type ICE does not read yet (tokens)"));
        };
        let column = if found.derived {
            name.to_string()
        } else {
            column_in(geo, from, name).to_string()
        };
        let key = format!("list:{from:?}/{column}");
        let read = match self.reads.get(&key) {
            Some(&r) => r,
            None => {
                self.plan.reads.push(Read {
                    name: column,
                    context: from,
                    resample: Resample::Average,
                    derived: found.derived,
                    at_location: false,
                    sampled: None,
                    from: None,
                });
                self.reads.insert(key, self.plan.reads.len() - 1);
                self.plan.reads.len() - 1
            }
        };
        let flat = context == Context::Object;
        let mut v = self.emit(flat, ty, |dst| Ins::LoadList { dst, read, flat });
        v.array = true;
        if !flat {
            v.of = Some(ListOf::Touch(from));
        }
        Ok(v)
    }

    /// A read of `name` in `ctx`, at surface samples: stored or derived.
    fn location_read(&mut self, ctx: Context, name: &str, derived: bool) -> usize {
        let column = if derived {
            name.to_string()
        } else {
            column_in(self.geo, ctx, name).to_string()
        };
        let key = format!("@{ctx:?}/{column}");
        if let Some(&r) = self.reads.get(&key) {
            return r;
        }
        self.plan.reads.push(Read {
            name: column,
            context: ctx,
            resample: Resample::Average,
            derived,
            at_location: true,
            sampled: None,
            from: None,
        });
        self.reads.insert(key, self.plan.reads.len() - 1);
        self.plan.reads.len() - 1
    }

    fn scatter(&mut self, node: usize, seed: u32, weight: &str) -> Result<Val, CompileError> {
        if self.plan.context != Context::Object {
            return error(
                node,
                "Scatter on Surface gives one array for the whole primitive: set the Execution Ports to Object",
            );
        }
        if !matches!(self.geo.topology, xms_geo::Topology::Mesh { .. }) {
            return error(node, "Scatter on Surface scatters on polygons; this primitive has none");
        }
        let count = self.input(node, 0, Some(Ty::Int))?;
        if count.ty != Ty::Int || !count.uniform || count.array {
            return error(node, "The count is one int for the whole primitive");
        }
        let weight = match weight.trim() {
            "" => None,
            name => {
                let found = [Context::Face, Context::Point]
                    .into_iter()
                    .find_map(|c| find_in(self.geo, c, name).map(|f| (c, f)));
                match found {
                    Some((c, f)) if matches!(f.ty, Some(Ty::Float | Ty::Int)) => {
                        Some(self.location_read(c, name, f.derived))
                    }
                    Some(_) => return error(node, format!("'{name}' is not a number per face or per point")),
                    None => {
                        return error(
                            node,
                            format!("No per-face or per-point attribute '{name}' to weight by"),
                        )
                    }
                }
            }
        };
        let mut v = self.emit(true, Ty::Sample, |dst| Ins::Scatter {
            dst,
            seed,
            count: count.reg,
            weight,
        });
        v.array = true;
        Ok(v)
    }

    fn at_location(&mut self, node: usize, name: &str, context: Option<Context>) -> Result<Val, CompileError> {
        let loc = self.input(node, 0, None)?;
        if loc.ty != Ty::Sample {
            return error(
                node,
                "Get Data's Samples are surface samples (Scatter on Surface's, Closest Point on Surface's)",
            );
        }
        if name.trim().is_empty() {
            return error(node, "Get Data needs the name of the attribute it reads");
        }
        let contexts = sample_contexts(context);
        // Read on another prim of the same topology, when a Geometry is wired.
        let loc = match self.geometry_input(node, 1)? {
            Some(s) => Val {
                surface: Some(s),
                ..loc
            },
            None => loc,
        };
        // What every sample has: its weights on its triangle's corners.
        match name.trim() {
            "bary" => {
                let mut v = self.emit(loc.uniform, Ty::Vec3, |dst| Ins::Bary {
                    dst,
                    loc: loc.reg,
                    list: false,
                });
                v.array = loc.array;
                return Ok(v);
            }
            "triP" => {
                return error(
                    node,
                    "triP is a sample's triangle's three points, a list: read it with Get Array Data",
                )
            }
            _ => {}
        }
        // Samples of reference meshes: read mesh by mesh, in the graph's space.
        if let Some(s) = loc.surface {
            let (read, ty) = self.sampled_read(node, name.trim(), Some(s), &contexts)?;
            let mut v = self.emit(loc.uniform, ty, |dst| Ins::AtSurface {
                dst,
                read,
                loc: loc.reg,
                surface: Some(s),
            });
            v.array = loc.array;
            return Ok(v);
        }
        // Per point first, then per corner, per face, the object's.
        let found = contexts
            .iter()
            .find_map(|&c| find_in(self.geo, c, name).map(|f| (c, f)));
        let Some((ctx, found)) = found else {
            return error(node, format!("No attribute '{name}'"));
        };
        let Some(ty) = found.ty.filter(|t| *t != Ty::Sample) else {
            return error(node, format!("'{name}' is of a type ICE does not read yet (tokens)"));
        };
        let read = self.location_read(ctx, name, found.derived);
        let mut v = self.emit(loc.uniform, ty, |dst| Ins::AtLocation {
            dst,
            read,
            loc: loc.reg,
        });
        v.array = loc.array;
        Ok(v)
    }

    /// A Geometry input (input `i`): Get Reference's paths, a surface of the
    /// plan's; `None` when it is not wired.
    fn geometry_input(&mut self, node: usize, i: usize) -> Result<Option<usize>, CompileError> {
        let graph = self.graph;
        match graph.nodes[node].inputs.get(i) {
            Some(Input::Wire { node: from, output }) => {
                let paths = self.wired(*from, *output)?;
                if paths.ty != Ty::Token || !paths.uniform {
                    return error(
                        node,
                        "The Geometry is the paths of prims on a reference input: wire Get Reference",
                    );
                }
                self.plan.surfaces.push(paths.reg);
                Ok(Some(self.plan.surfaces.len() - 1))
            }
            _ => Ok(None),
        }
    }

    /// Where `name` is on the reference prims, in the first of `contexts` one
    /// has it: the context, what was found and how many elements that prim has
    /// there.
    fn on_refs(&self, name: &str, contexts: &[Context]) -> Option<(Context, Found, usize)> {
        self.refs.iter().flat_map(|r| &r.meshes).find_map(|m| {
            let g = m.geo.get();
            contexts
                .iter()
                .find_map(|&c| find_in(g, c, name).map(|f| (c, f, g.count(c))))
        })
    }

    /// Get Data with a Geometry: the attribute read from that prim, element by
    /// element, as the graph's own would be (across contexts too).
    fn get_data_from(
        &mut self,
        node: usize,
        name: &str,
        from: Option<Context>,
        resample: Resample,
    ) -> Result<Val, CompileError> {
        let Some(s) = self.geometry_input(node, 0)? else {
            return error(node, "Get Data reads from a Geometry wired in");
        };
        let (name, context) = (name.trim(), self.plan.context);
        if name.is_empty() {
            return error(node, "Get Data needs the name of the attribute it reads");
        }
        let contexts = match from {
            Some(c) => vec![c],
            None => vec![context, Context::Object],
        };
        let Some((c, found, count)) = self.on_refs(name, &contexts) else {
            return error(node, format!("No attribute '{name}' on the Geometry's prims"));
        };
        let Some(ty) = found.ty else {
            return error(node, format!("'{name}' is of a type ICE does not read yet (tokens)"));
        };
        if c != Context::Object && count != self.geo.count(c) {
            return error(node, format!("The Geometry has {count} elements per {c:?}, this geometry {}: reading element by element takes the same topology", self.geo.count(c)));
        }
        let uniform = c == Context::Object;
        let resample = if c == context || uniform {
            Resample::Average
        } else {
            resample
        };
        let key = format!("from{s}:{c:?}/{name}/{resample:?}");
        let read = match self.reads.get(&key) {
            Some(&r) => r,
            None => {
                self.plan.reads.push(Read {
                    name: name.to_string(),
                    context: c,
                    resample,
                    derived: found.derived,
                    at_location: false,
                    sampled: None,
                    from: Some(s),
                });
                self.reads.insert(key, self.plan.reads.len() - 1);
                self.plan.reads.len() - 1
            }
        };
        Ok(self.emit(uniform, ty, |dst| Ins::Load { dst, read }))
    }

    /// Get Array Data with a Geometry: lists read from that prim, element by
    /// element, as Get Array Data reads the graph's own.
    fn get_array_data_from(&mut self, node: usize, name: &str, from: Option<Context>) -> Result<Val, CompileError> {
        let Some(s) = self.geometry_input(node, 0)? else {
            return error(node, "Get Array Data reads from a Geometry wired in");
        };
        let context = self.plan.context;
        let (name, from) = match (name.trim(), from) {
            ("ptIndices", None) => ("ptIndex", Some(Context::Point)),
            ("faceIndices", None) => ("faceIndex", Some(Context::Face)),
            (n, f) => (n, f),
        };
        if name.is_empty() {
            return error(node, "Get Array Data needs the name of the attribute it reads");
        }
        let contexts = match from {
            Some(c) => vec![c],
            None => vec![Context::Point, Context::Corner, Context::Face, Context::Object],
        };
        let Some((c, found, count)) = self.on_refs(name, &contexts) else {
            return error(node, format!("No attribute '{name}' on the Geometry's prims"));
        };
        let Some(ty) = found.ty else {
            return error(node, format!("'{name}' is of a type ICE does not read yet (tokens)"));
        };
        if c != Context::Object && count != self.geo.count(c) {
            return error(node, format!("The Geometry has {count} elements per {c:?}, this geometry {}: reading element by element takes the same topology", self.geo.count(c)));
        }
        let key = format!("listfrom{s}:{c:?}/{name}");
        let read = match self.reads.get(&key) {
            Some(&r) => r,
            None => {
                self.plan.reads.push(Read {
                    name: name.to_string(),
                    context: c,
                    resample: Resample::Average,
                    derived: found.derived,
                    at_location: false,
                    sampled: None,
                    from: Some(s),
                });
                self.reads.insert(key, self.plan.reads.len() - 1);
                self.plan.reads.len() - 1
            }
        };
        let flat = context == Context::Object;
        let mut v = self.emit(flat, ty, |dst| Ins::LoadList { dst, read, flat });
        v.array = true;
        if !flat {
            v.of = Some(ListOf::Touch(c));
        }
        Ok(v)
    }

    /// A read of `name` at samples of a surface (`None`: the graph's own), in
    /// the first of `contexts` it is in (on the first mesh that has it, for a
    /// reference surface): the read and its type.
    fn sampled_read(
        &mut self,
        node: usize,
        name: &str,
        surface: Option<usize>,
        contexts: &[Context],
    ) -> Result<(usize, Ty), CompileError> {
        let found = match surface {
            None => contexts
                .iter()
                .find_map(|&c| find_in(self.geo, c, name).map(|f| (c, f))),
            Some(_) => self.refs.iter().flat_map(|r| &r.meshes).find_map(|m| {
                let g = m.geo.get();
                contexts.iter().find_map(|&c| find_in(g, c, name).map(|f| (c, f)))
            }),
        };
        let Some((ctx, found)) = found else {
            let on = if surface.is_some() {
                " on the reference meshes"
            } else {
                ""
            };
            return error(node, format!("No attribute '{name}'{on}"));
        };
        let Some(ty) = found.ty.filter(|t| *t != Ty::Sample) else {
            return error(node, format!("'{name}' is of a type ICE does not read yet (tokens)"));
        };
        let key = format!("@{surface:?}:{ctx:?}/{name}");
        let read = match self.reads.get(&key) {
            Some(&r) => r,
            None => {
                self.plan.reads.push(Read {
                    name: name.to_string(),
                    context: ctx,
                    resample: Resample::Average,
                    derived: found.derived,
                    at_location: true,
                    sampled: Some(surface),
                    from: None,
                });
                self.reads.insert(key, self.plan.reads.len() - 1);
                self.plan.reads.len() - 1
            }
        };
        Ok((read, ty))
    }

    /// Closest Point on Surface: the samples of the reference meshes (input 0,
    /// their paths) nearest each origin (input 1), within the radius (input 2;
    /// 0, anywhere); its second output how far each is.
    fn closest(&mut self, node: usize) -> Result<Val, CompileError> {
        let paths = self.input(node, 0, None)?;
        if paths.ty != Ty::Token || !paths.uniform {
            return error(
                node,
                "The Geometry is the paths of meshes on a reference input: wire Get Reference",
            );
        }
        let origin = self.input(node, 1, Some(Ty::Vec3))?;
        if origin.ty != Ty::Vec3 || origin.is_list() {
            return error(
                node,
                "The Origin is a 3D vector: one per element (P), or an array in an Object graph",
            );
        }
        let mut radius = self.input(node, 2, Some(Ty::Float))?;
        if radius.ty == Ty::Int {
            let src = radius.reg;
            radius = self.emit(radius.uniform, Ty::Float, |dst| Ins::ToFloat { dst, src });
        }
        if radius.ty != Ty::Float || radius.array {
            return error(
                node,
                "The Radius is one number, for all or per element; 0 searches without a limit",
            );
        }
        let uniform = origin.uniform && radius.uniform;
        if origin.array && !uniform {
            return error(node, "An array of origins takes one radius for all");
        }
        let surface = self.plan.surfaces.len();
        self.plan.surfaces.push(paths.reg);
        let (o, r) = if uniform {
            (origin.reg, radius.reg)
        } else {
            (self.varying(origin), self.varying(radius))
        };
        let dist = self.alloc(uniform, Ty::Float);
        let mut v = self.emit(uniform, Ty::Sample, |dst| Ins::Closest {
            dst,
            dist,
            surface,
            origin: o,
            radius: r,
        });
        v.array = origin.array;
        v.surface = Some(surface);
        self.outs.insert(
            (node, CLOSEST_DISTANCE),
            Val {
                reg: dist,
                uniform,
                ty: Ty::Float,
                array: origin.array,
                of: None,
                surface: None,
            },
        );
        Ok(v)
    }

    /// Get Array Data at Samples: at each sample, the values of the face it is
    /// on, kept as a list: its points' positions (P), their numbers
    /// (ptIndices), its corners' UVs. One sample per element gives a list per
    /// element; one sample in all, an array.
    fn array_at(&mut self, node: usize, name: &str, context: Option<Context>) -> Result<Val, CompileError> {
        let loc = self.input(node, 0, None)?;
        if loc.ty != Ty::Sample {
            return error(
                node,
                "Get Array Data's Samples are surface samples (Closest Point on Surface's)",
            );
        }
        if loc.array {
            return error(node, "Get Array Data reads at one sample per element (Closest Point's, per point): an array of samples would give a list per sample, not yet");
        }
        // Read on another prim of the same topology, when a Geometry is wired.
        let loc = match self.geometry_input(node, 1)? {
            Some(s) => Val {
                surface: Some(s),
                ..loc
            },
            None => loc,
        };
        // What every sample has: its triangle's three points, in the
        // graph's space, and its weights on them, item by item.
        if matches!(name.trim(), "triP" | "bary") {
            let (flat, surface) = (loc.uniform, loc.surface);
            let mut v = if name.trim() == "bary" {
                self.emit(flat, Ty::Float, |dst| Ins::Bary {
                    dst,
                    loc: loc.reg,
                    list: true,
                })
            } else {
                self.emit(flat, Ty::Vec3, |dst| Ins::TriP {
                    dst,
                    loc: loc.reg,
                    surface,
                    flat,
                })
            };
            v.array = true;
            if !flat {
                v.of = Some(ListOf::Tri(loc.reg));
            }
            return Ok(v);
        }
        let (name, contexts) = match name.trim() {
            "ptIndices" => ("ptIndex", vec![Context::Point]),
            "" => return error(node, "Get Array Data needs the name of the attribute it reads"),
            n => (n, sample_contexts(context)),
        };
        let (read, ty) = self.sampled_read(node, name, loc.surface, &contexts)?;
        let flat = loc.uniform;
        let surface = loc.surface;
        let mut v = self.emit(flat, ty, |dst| Ins::ListAt {
            dst,
            read,
            loc: loc.reg,
            surface,
            flat,
        });
        v.array = true;
        if !flat {
            v.of = Some(ListOf::Face(loc.reg));
        }
        Ok(v)
    }

    fn array(&mut self, node: usize, op: ArrayOp) -> Result<Val, CompileError> {
        let a = self.input(node, 0, None)?;
        if !a.array {
            return error(
                node,
                format!("{op:?} takes an array (Get Array Data, Random with a count)"),
            );
        }
        let Some(ty) = array_ty(op, a.ty) else {
            return error(node, format!("{op:?} does not take an array of {:?}", a.ty));
        };
        let b = if op.arity() > 1 {
            self.input(node, 1, Some(Ty::Int))?
        } else {
            a
        };
        // A list per element: one result per element, its index one for all
        // or one per element.
        if a.is_list() {
            if op == ArrayOp::Element && (b.ty != Ty::Int || b.array) {
                return error(node, "The element's index is an int, one for all or one per element");
            }
            let b = if op == ArrayOp::Element { self.varying(b) } else { a.reg };
            let mut v = self.emit(false, ty, |dst| Ins::Array {
                op,
                dst,
                args: [a.reg, b],
            });
            v.surface = a.surface;
            return Ok(v);
        }
        if op == ArrayOp::Element && (b.ty != Ty::Int || !b.uniform || b.array) {
            return error(node, "The element's index is one int");
        }
        // A sample picked out stays on its surface.
        let mut v = self.emit(true, ty, |dst| Ins::Array {
            op,
            dst,
            args: [a.reg, b.reg],
        });
        v.surface = a.surface;
        Ok(v)
    }

    /// Random Orientation: a rotation per element, or arrays of them (as long
    /// as its normals, or its count).
    fn random_orientation(&mut self, node: usize, seed: u32) -> Result<Val, CompileError> {
        let normals = self.input(node, 0, Some(Ty::Vec3))?;
        let turn = self.input(node, 1, Some(Ty::Vec3))?;
        if normals.ty != Ty::Vec3 {
            return error(node, "The normals are 3D vectors (N, Scatter's normals)");
        }
        if turn.ty != Ty::Vec3 || turn.array {
            return error(node, "The random rotation is a 3D vector of degrees");
        }
        let count = self.count_input(node, 2)?;
        let key = self.seed_input(node, 3)?;
        let array_of = normals.array.then_some(normals);
        self.drawn(
            node,
            Ty::Quat,
            [Some(normals), Some(turn)],
            count,
            key,
            array_of,
            |dst, args, array, key| Ins::RandomOrient {
                dst,
                seed,
                args,
                array,
                key,
            },
        )
    }

    /// Compose: a vector from its parts, each taken as its type (an int as a
    /// number). One value, per element, or arrays element by element.
    fn compose(&mut self, node: usize, op: Compose) -> Result<Val, CompileError> {
        let mut vals: Vec<Val> = vec![];
        for (i, &(name, ty)) in op.inputs().iter().enumerate() {
            let mut v = self.input(node, i, Some(ty))?;
            if v.ty == Ty::Int && ty == Ty::Float {
                let (src, array) = (v.reg, v.array);
                v = self.emit(v.uniform, Ty::Float, |dst| Ins::ToFloat { dst, src });
                v.array = array;
            }
            if v.ty != ty {
                return error(node, format!("{name} is a {}, not a {}", ty.name(), v.ty.name()));
            }
            if v.is_list() {
                return error(
                    node,
                    "Compose works on values and the graph's arrays, not on lists per element yet",
                );
            }
            vals.push(v);
        }
        let uniform = vals.iter().all(|v| v.uniform);
        let array = vals.iter().any(|v| v.array);
        if array && !uniform {
            return error(node, "An array meets per-element values: arrays work in the graph's one-value part (an Object graph) for now");
        }
        let mut args = [0usize; 4];
        for (i, v) in vals.iter().enumerate() {
            args[i] = if uniform { v.reg } else { self.varying(*v) };
        }
        let mut out = self.emit(uniform, op.output(), |dst| Ins::Compose { op, dst, args });
        out.array = array;
        Ok(out)
    }

    /// Decompose: a vector taken apart, one register per part, all made by one
    /// instruction; output 0 given back, every output kept in `outs`.
    fn decompose(&mut self, node: usize, op: Decompose) -> Result<Val, CompileError> {
        let v = self.input(node, 0, Some(op.input()))?;
        if v.ty != op.input() {
            return error(
                node,
                format!("Decompose takes a {} here, not a {}", op.input().name(), v.ty.name()),
            );
        }
        if v.is_list() {
            return error(
                node,
                "Decompose works on values and the graph's arrays, not on lists per element yet",
            );
        }
        let p = if v.uniform {
            &mut self.plan.uniform
        } else {
            &mut self.plan.batch
        };
        let mut dsts = [0usize; 4];
        for (k, (_, ty)) in op.outputs().iter().enumerate() {
            dsts[k] = p.regs.len();
            p.regs.push(*ty);
        }
        p.code.push(Ins::Decompose { op, dsts, src: v.reg });
        for (k, (_, ty)) in op.outputs().iter().enumerate() {
            self.outs.insert(
                (node, k),
                Val {
                    reg: dsts[k],
                    uniform: v.uniform,
                    ty: *ty,
                    array: v.array,
                    of: None,
                    surface: None,
                },
            );
        }
        Ok(self.outs[&(node, 0)])
    }

    /// Concatenate Arrays: its wired inputs one after the other, arrays or
    /// single values, all of one type.
    fn concat(&mut self, node: usize) -> Result<Val, CompileError> {
        let graph = self.graph;
        let mut parts: Vec<Val> = vec![];
        for input in &graph.nodes[node].inputs {
            if let Input::Wire { node: from, output } = input {
                parts.push(self.wired(*from, *output)?);
            }
        }
        let Some(first) = parts.first().copied() else {
            return error(node, "Concatenate Arrays joins what is wired into it: nothing is");
        };
        if let Some(other) = parts.iter().find(|v| v.ty != first.ty) {
            return error(
                node,
                format!(
                    "Concatenate Arrays joins arrays of one type: {} and {}",
                    first.ty.name(),
                    other.ty.name()
                ),
            );
        }
        if parts.iter().any(|v| !v.uniform) {
            return error(node, "Concatenate Arrays joins arrays, in the graph's one-value part (an Object graph): per-element values are not arrays");
        }
        // Samples join samples of the same surface.
        if parts.iter().any(|v| v.surface != first.surface) {
            return error(
                node,
                "Concatenate Arrays joins samples of one surface: these are of two",
            );
        }
        let srcs: Vec<usize> = parts.iter().map(|v| v.reg).collect();
        let mut v = self.emit(true, first.ty, |dst| Ins::Concat { dst, srcs });
        v.array = true;
        v.surface = first.surface;
        Ok(v)
    }

    /// An Instancer, run from the root: its inputs worked out in the uniform
    /// program, the instances made from them once it has run.
    fn instancer(&mut self, node: usize, path: &str, seed: u32) -> Result<(), CompileError> {
        if self.plan.context != Context::Object {
            return error(
                node,
                "The Instancer makes one point instancer for the whole primitive: set the Execution Ports to Object",
            );
        }
        let path = path.trim();
        if path.is_empty() || path.ends_with('/') || path.contains("//") {
            return error(node, "The Instancer needs a Primitive Location: where the instancer is made (a name alone: beside the primitive)");
        }
        let positions = self.input(node, 0, None)?;
        if positions.ty != Ty::Vec3 || !positions.array {
            return error(
                node,
                "The Instancer's positions are an array of 3D vectors (Scatter on Surface's Positions)",
            );
        }
        let protos = self.input(node, 1, None)?;
        if protos.ty != Ty::Token || !protos.array {
            return error(node, "The Instancer's prototypes are paths (Get Reference)");
        }
        let index = self.input(node, 2, Some(Ty::Int))?;
        let orient = self.input(node, 3, Some(Ty::Quat))?;
        let scale = self.input(node, 4, None)?;
        // An int scale is a number.
        let scale = if scale.ty == Ty::Int {
            let (src, array) = (scale.reg, scale.array);
            let mut v = self.emit(true, Ty::Float, |dst| Ins::ToFloat { dst, src });
            v.array = array;
            v
        } else {
            scale
        };
        for (v, ok, what) in [
            (index, index.ty == Ty::Int, "proto indices are ints"),
            (orient, orient.ty == Ty::Quat, "orientations are rotations"),
            (
                scale,
                matches!(scale.ty, Ty::Float | Ty::Vec3),
                "scales are numbers or 3D vectors",
            ),
        ] {
            if !ok || !v.uniform {
                return error(node, format!("The Instancer's {what}, one for all or an array"));
            }
        }
        self.plan.instancers.push(InstancerPlan {
            path: path.to_string(),
            seed,
            positions: positions.reg,
            protos: protos.reg,
            index: index.reg,
            orient: orient.reg,
            scale: scale.reg,
        });
        Ok(())
    }

    /// A Set Data node, run from the root.
    fn set_data(&mut self, node: usize) -> Result<(), CompileError> {
        if !self.set.insert(node) {
            return Ok(());
        }
        if let Op::Instancer { path, seed } = &self.graph.nodes[node].op {
            let (path, seed) = (path.clone(), *seed);
            return self.instancer(node, &path, seed);
        }
        let (graph, geo) = (self.graph, self.geo);
        let n = &graph.nodes[node];
        let Op::SetData {
            name,
            context: to,
            resample,
        } = &n.op
        else {
            return error(node, "Only Execute outputs go into the root");
        };
        let context = to.unwrap_or(self.plan.context);
        // Into its own context, one value each: nothing to make one.
        let resample = if context == self.plan.context {
            Resample::Average
        } else {
            *resample
        };
        let own = context == self.plan.context;
        if name.trim().is_empty() {
            return error(node, "Set Data needs the name of the attribute it sets");
        }
        if column_name(name) == POINTS && context != Context::Point {
            return error(
                node,
                format!("The points are per Point; this writes them per {context:?}"),
            );
        }
        let column = column_in(geo, context, name).to_string();
        let mut v = match n.inputs.first() {
            Some(Input::Wire { node: from, output }) => self.wired(*from, *output)?,
            Some(Input::Value(value)) => self.constant(*value),
            None => return error(node, "Set Data takes a value"),
        };
        if v.array {
            return error(
                node,
                "An array is not written as an attribute (yet): it is one value of many",
            );
        }
        if v.ty == Ty::Sample {
            return error(
                node,
                "A surface sample is not written as an attribute: read what is there with a Get Data wired to it",
            );
        }
        if v.ty == Ty::Token {
            return error(node, "Paths are not written as attributes");
        }
        let existing = geo.attr(context, &column);
        if let Some(t) = existing.and_then(|a| column_ty(&a.column)) {
            if t != v.ty {
                // A float or int value takes the attribute's type, as a value would.
                match n.inputs.first() {
                    Some(Input::Value(value)) if value.convert(t).is_some() => {
                        v = self.constant(value.convert(t).unwrap())
                    }
                    _ => return error(node, format!("'{name}' is {t:?}; this sets {:?}", v.ty)),
                }
            }
        }
        let role = existing
            .map(|a| a.role)
            .unwrap_or(if column == POINTS { Role::Point } else { Role::None });
        let write = match self
            .plan
            .writes
            .iter()
            .position(|w| w.name == column && w.context == context)
        {
            Some(i) => i,
            None => {
                self.plan.writes.push(Write {
                    name: column.clone(),
                    ty: v.ty,
                    role,
                    context,
                    resample,
                });
                self.plan.writes.len() - 1
            }
        };
        // The same attribute set twice: the later value is the one kept.
        self.plan.writes[write].ty = v.ty;
        self.plan.writes[write].resample = resample;
        let src = self.varying(v);
        self.plan.batch.code.push(Ins::Store { write, src });
        // Read back by a later port in the graph's own context; written
        // into another, it is there once the graph has run.
        if own {
            self.written.insert(column, v);
        }
        Ok(())
    }
}

/// Compile `graph` for running on geometry laid out like `geo` (the same
/// attributes, of the same types). Its root is the node with `Op::Root`.
pub fn compile(graph: &Graph, geo: &Geo) -> Result<Plan, CompileError> {
    compile_with(graph, geo, &[])
}

/// `compile`, with the operator's reference inputs: what is read at their
/// meshes' samples is typed by them.
pub fn compile_with(graph: &Graph, geo: &Geo, refs: &[Reference]) -> Result<Plan, CompileError> {
    let Some(root) = graph.nodes.iter().position(|n| matches!(n.op, Op::Root { .. })) else {
        return error(0, "The graph has no root");
    };
    let Op::Root { context } = graph.nodes[root].op else {
        unreachable!()
    };
    let mut l = Lower {
        graph,
        geo,
        refs,
        plan: Plan {
            context,
            reads: vec![],
            params: vec![],
            uniform: Program::default(),
            batch: Program::default(),
            writes: vec![],
            instancers: vec![],
            surfaces: vec![],
        },
        done: HashMap::new(),
        outs: HashMap::new(),
        visiting: HashSet::new(),
        broadcast: HashMap::new(),
        reads: HashMap::new(),
        written: HashMap::new(),
        set: HashSet::new(),
    };
    for input in &graph.nodes[root].inputs {
        match input {
            Input::Wire { node, .. } => l.set_data(*node)?,
            Input::Value(_) => {}
        }
    }
    // Writes are stored in the order set; a write set twice keeps its last
    // store only, so earlier stores of it are dropped.
    let mut last: HashMap<usize, usize> = HashMap::new();
    for (i, ins) in l.plan.batch.code.iter().enumerate() {
        if let Ins::Store { write, .. } = ins {
            last.insert(*write, i);
        }
    }
    let code = std::mem::take(&mut l.plan.batch.code);
    l.plan.batch.code = code
        .into_iter()
        .enumerate()
        .filter(|(i, ins)| !matches!(ins, Ins::Store { write, .. } if last.get(write) != Some(i)))
        .map(|(_, ins)| ins)
        .collect();
    Ok(l.plan)
}

/// The type of each node's output, by node, for showing on ports and wires:
/// what `compile` would give it on geometry laid out like `geo`, for every
/// node, wired into the root or not. `None` for the root, for Set Data (its
/// output is an Execute), and for what does not resolve (an attribute `geo`
/// does not have, maths on types it does not take).
pub fn output_types(graph: &Graph, geo: &Geo) -> Vec<Option<Ty>> {
    let context = graph
        .nodes
        .iter()
        .find_map(|n| match n.op {
            Op::Root { context } => Some(context),
            _ => None,
        })
        .unwrap_or(Context::Point);
    // Per node: not yet looked at (None), or its type.
    let mut memo: Vec<Option<Option<Ty>>> = vec![None; graph.nodes.len()];
    (0..graph.nodes.len())
        .map(|i| type_of(graph, geo, context, i, &mut memo, 0))
        .collect()
}

fn type_of(
    graph: &Graph,
    geo: &Geo,
    context: Context,
    node: usize,
    memo: &mut Vec<Option<Option<Ty>>>,
    depth: usize,
) -> Option<Ty> {
    if depth > 256 {
        return None;
    }
    if let Some(t) = memo.get(node).copied().flatten() {
        return t;
    }
    let n = graph.nodes.get(node)?;
    // Met again while looking: a cycle, no type.
    memo[node] = Some(None);
    let t = match &n.op {
        Op::Root { .. } | Op::SetData { .. } | Op::Instancer { .. } => None,
        Op::Constant(v) => Some(v.ty()),
        Op::Parameter { default, .. } => Some(default.ty()),
        Op::GetData {
            name, context: from, ..
        } => {
            let at = |c: Context| find_in(geo, c, name);
            match from {
                Some(c) => at(*c),
                None => at(context).or_else(|| at(Context::Object)),
            }
            .and_then(|f| f.ty)
        }
        Op::Math(op) => {
            let wired: Vec<Option<Ty>> = n
                .inputs
                .iter()
                .map(|i| match i {
                    Input::Wire { node: from, output } => wire_ty(graph, geo, context, *from, *output, memo, depth + 1),
                    Input::Value(_) => None,
                })
                .collect();
            // As `Lower::math`: values take the type of what they meet.
            let met = wired.iter().flatten().copied().find(|t| *t != Ty::Float).or(wired
                .iter()
                .flatten()
                .copied()
                .next());
            let tys: Option<Vec<Ty>> = n
                .inputs
                .iter()
                .enumerate()
                .map(|(i, input)| match input {
                    Input::Wire { .. } => wired[i],
                    Input::Value(v) => {
                        let keep_float = v.ty() == Ty::Float && keeps_float(*op, i);
                        Some(match met {
                            Some(t) if !keep_float => v.convert(t).unwrap_or(*v).ty(),
                            _ => v.ty(),
                        })
                    }
                })
                .collect();
            // As `Lower::math`: an int meeting a float is a float.
            let tys = tys.map(|t| {
                if t.contains(&Ty::Float) {
                    t.into_iter()
                        .map(|x| if x == Ty::Int { Ty::Float } else { x })
                        .collect()
                } else {
                    t
                }
            });
            tys.and_then(|t: Vec<Ty>| math_ty(*op, &t))
        }
        Op::Random { .. } => {
            let wired = n.inputs.iter().take(2).find_map(|i| match i {
                Input::Wire { node: from, output } => wire_ty(graph, geo, context, *from, *output, memo, depth + 1),
                Input::Value(_) => None,
            });
            wired.or_else(|| match (n.inputs.first(), n.inputs.get(1)) {
                (Some(Input::Value(a)), Some(Input::Value(b))) if a.ty() != b.ty() => Some(Ty::Float),
                (Some(Input::Value(a)), _) => Some(a.ty()),
                _ => None,
            })
        }
        // Its samples; its positions and normals by `wire_ty`.
        Op::Scatter { .. } => Some(Ty::Sample),
        // Its samples; its distances by `wire_ty`.
        Op::ClosestPoint => Some(Ty::Sample),
        // Of the graph's own geometry when it has it; a reference mesh's is
        // known when the graph runs.
        Op::GetArrayDataAt { name, .. } if name.trim() == "triP" => Some(Ty::Vec3),
        Op::GetArrayDataAt { name, .. } if name.trim() == "bary" => Some(Ty::Float),
        Op::GetDataAt { name, .. } if name.trim() == "bary" => Some(Ty::Vec3),
        Op::GetArrayDataAt { name, context: from } => {
            let name = if name.trim() == "ptIndices" {
                "ptIndex"
            } else {
                name.trim()
            };
            sample_contexts(*from)
                .into_iter()
                .find_map(|c| find_in(geo, c, name))
                .and_then(|f| f.ty)
        }
        Op::GetArrayData { name, context: from } => {
            let (name, from) = match (name.trim(), from) {
                ("ptIndices", None) => ("ptIndex", Some(Context::Point)),
                ("faceIndices", None) => ("faceIndex", Some(Context::Face)),
                (n, f) => (n, *f),
            };
            from.or_else(|| context_of(geo, name))
                .and_then(|c| find_in(geo, c, name))
                .and_then(|f| f.ty)
        }
        Op::RandomOrientation { .. } => Some(Ty::Quat),
        Op::Concat => n.inputs.iter().find_map(|i| match i {
            Input::Wire { node: from, output } => wire_ty(graph, geo, context, *from, *output, memo, depth + 1),
            Input::Value(_) => None,
        }),
        Op::GetReference { .. } => Some(Ty::Token),
        Op::GetDataAt { name, context: from } => sample_contexts(*from)
            .into_iter()
            .find_map(|c| find_in(geo, c, name))
            .and_then(|f| f.ty),
        Op::Compose(c) => Some(c.output()),
        Op::Decompose(d) => Some(d.outputs()[0].1),
        Op::Array(op) => {
            let of = match n.inputs.first() {
                Some(Input::Wire { node: from, output }) => {
                    wire_ty(graph, geo, context, *from, *output, memo, depth + 1)
                }
                _ => None,
            };
            of.and_then(|t| array_ty(*op, t))
        }
    };
    memo[node] = Some(t);
    t
}

/// The type of output `output` of a node: Scatter on Surface's positions and
/// normals are 3D vectors, its samples samples; every other node has one
/// output, `type_of`'s.
fn wire_ty(
    graph: &Graph,
    geo: &Geo,
    context: Context,
    node: usize,
    output: usize,
    memo: &mut Vec<Option<Option<Ty>>>,
    depth: usize,
) -> Option<Ty> {
    match graph.nodes.get(node).map(|n| &n.op) {
        Some(Op::Scatter { .. }) if output != SCATTER_SAMPLES => Some(Ty::Vec3),
        Some(Op::ClosestPoint) if output == CLOSEST_DISTANCE => Some(Ty::Float),
        Some(Op::Decompose(d)) => d.outputs().get(output).map(|o| o.1),
        _ => type_of(graph, geo, context, node, memo, depth),
    }
}

/// Whether each node's output is an array, by node, for the editor: an array is
/// drawn and named apart from a single value. What makes one: Scatter on
/// Surface, Get Reference, Random Value with a count; what keeps one: maths on
/// an array, Get Data at Samples; Random Orientation.
pub fn output_arrays(graph: &Graph) -> Vec<bool> {
    let mut memo: Vec<Option<bool>> = vec![None; graph.nodes.len()];
    (0..graph.nodes.len())
        .map(|i| is_array(graph, i, &mut memo, 0))
        .collect()
}

fn is_array(graph: &Graph, node: usize, memo: &mut Vec<Option<bool>>, depth: usize) -> bool {
    if depth > 256 {
        return false;
    }
    if let Some(a) = memo[node] {
        return a;
    }
    // Met again while looking: a cycle, not an array.
    memo[node] = Some(false);
    let n = &graph.nodes[node];
    let a = match &n.op {
        Op::Scatter { .. }
        | Op::GetReference { .. }
        | Op::Concat
        | Op::GetArrayData { .. }
        | Op::GetArrayDataAt { .. } => true,
        // As its origins: an array of them, an array of samples.
        Op::ClosestPoint => match n.inputs.get(1) {
            Some(Input::Wire { node: from, .. }) => is_array(graph, *from, memo, depth + 1),
            _ => false,
        },
        // A count, a seed per element of an array; Random Orientation's
        // normals as an array too.
        Op::Random { .. } | Op::RandomOrientation { .. } => {
            let counted = match n.inputs.get(2) {
                Some(Input::Value(Value::Int(c))) => *c > 0,
                Some(Input::Value(Value::Float(c))) => *c > 0.0,
                Some(Input::Wire { .. }) => true,
                _ => false,
            };
            let from_array = |i: usize, memo: &mut Vec<Option<bool>>| match n.inputs.get(i) {
                Some(Input::Wire { node: from, .. }) => is_array(graph, *from, memo, depth + 1),
                _ => false,
            };
            counted || from_array(3, memo) || (matches!(n.op, Op::RandomOrientation { .. }) && from_array(0, memo))
        }
        Op::Math(_) | Op::GetDataAt { .. } | Op::Compose(_) | Op::Decompose(_) => {
            let mut any = false;
            for input in &n.inputs {
                if let Input::Wire { node: from, .. } = input {
                    any |= is_array(graph, *from, memo, depth + 1);
                }
            }
            any
        }
        _ => false,
    };
    memo[node] = Some(a);
    a
}
