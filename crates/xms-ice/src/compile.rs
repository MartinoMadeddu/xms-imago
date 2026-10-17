//! Graph to plan.
//!
//! Every value is given a register in one of two programs. The **uniform**
//! program runs once per evaluation, on registers one element long: constants,
//! parameters, Object attributes and maths on nothing but those. The **batch**
//! program runs per batch of elements: what varies per element. A uniform
//! value a batch instruction needs is broadcast into a batch register once,
//! at the start of each batch.
//!
//! Only what reaches the root is compiled, so a node left unwired costs
//! nothing, and only the attributes the compiled nodes read are read.

use std::collections::{HashMap, HashSet};

use xms_geo::{Column, Context, Geo, Role, POINTS};

use super::derived;
use super::graph::{Graph, Input, Math, Op, Reduce};
use super::types::{Ty, Value};

/// A plan: what to read, the two programs, what to write. Made once per
/// graph (and geometry layout); run as often as wanted, with new parameter
/// values each time.
#[derive(Clone, Debug)]
pub struct Plan {
    /// The elements the graph runs over.
    pub context: Context,
    pub(crate) reads:   Vec<Read>,
    pub(crate) params:  Vec<(String, Value)>,
    pub(crate) uniform: Program,
    pub(crate) batch:   Program,
    pub(crate) writes:  Vec<Write>,
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
    pub fn batch_len(&self) -> usize { self.batch.code.len() }
}

#[derive(Clone, Debug)]
pub(crate) struct Read {
    /// The column's name in the `Geo` (`points` for `P`).
    pub name:    String,
    /// The context the attribute is in: the plan's own, Object, or another
    /// one read across (see `Op::GetData`).
    pub context: Context,
    /// How the values an element touches across contexts become one.
    pub reduce:  Reduce,
    /// Made from the topology when run (`derived`), not a stored column.
    pub derived: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Write {
    pub name:    String,
    pub ty:      Ty,
    pub role:    Role,
    /// Where it is written: the plan's context, or another one, each of
    /// whose elements takes the plan's elements it touches, made one.
    pub context: Context,
    pub reduce:  Reduce,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Program {
    pub code: Vec<Ins>,
    /// The type of each register.
    pub regs: Vec<Ty>,
}

#[derive(Clone, Debug)]
pub(crate) enum Ins {
    Const { dst: usize, value: Value },
    /// Parameter number `index` of the plan.
    Param { dst: usize, index: usize },
    /// Read number `read`: the batch's elements, or element 0 in the uniform program.
    Load { dst: usize, read: usize },
    /// Uniform register `from`, spread over a batch register.
    Broadcast { dst: usize, from: usize },
    Math { op: Math, dst: usize, args: [usize; 3] },
    /// Into write number `write`.
    Store { write: usize, src: usize },
}

/// Why a graph does not compile, and at which node: shown on the node, and
/// on the wire into it, in the editor.
#[derive(Clone, Debug, PartialEq)]
pub struct CompileError {
    pub node:    usize,
    pub message: String,
}

fn error<T>(node: usize, message: impl Into<String>) -> Result<T, CompileError> {
    Err(CompileError { node, message: message.into() })
}

/// The name of an attribute's column: ICE's `P` is the points.
fn column_name(name: &str) -> &str { if name == "P" { POINTS } else { name } }

/// The column an attribute name reads in `ctx`: the name itself, else the
/// short names ICE and Houdini use (`P` the points, `N` the normals, `uv`
/// the first texture coordinates).
fn column_in<'a>(geo: &'a Geo, ctx: Context, name: &'a str) -> &'a str {
    if geo.attr(ctx, name).is_some() { return name; }
    match name {
        "P" => POINTS,
        "N" if geo.attr(ctx, xms_geo::NORMALS).is_some() => xms_geo::NORMALS,
        "uv" | "UV" => xms_geo::UV_NAMES.iter().copied().find(|n| geo.attr(ctx, n).is_some()).unwrap_or(name),
        _ => name,
    }
}

/// What reading `name` in `ctx` finds: a stored attribute, or one made
/// from the topology (`derived`), with its type (`None`: a type ICE does
/// not read).
struct Found { ty: Option<Ty>, derived: bool }

fn find_in(geo: &Geo, ctx: Context, name: &str) -> Option<Found> {
    if let Some(a) = geo.attr(ctx, column_in(geo, ctx, name)) {
        return Some(Found { ty: column_ty(&a.column), derived: false });
    }
    derived::ty(geo, ctx, name).map(|t| Found { ty: Some(t), derived: true })
}

/// The context `name` is in, stored or derived, if any.
fn context_of(geo: &Geo, name: &str) -> Option<Context> {
    geo.find(column_name(name)).map(|(c, _)| c).or_else(|| derived::context_of(geo, name))
}

fn column_ty(c: &Column) -> Option<Ty> {
    Some(match c {
        Column::Bool(_) => Ty::Bool,
        Column::Int(_) => Ty::Int,
        Column::Float(_) => Ty::Float,
        Column::Vec2(_) => Ty::Vec2,
        Column::Vec3(_) => Ty::Vec3,
        // A quaternion reads as its four numbers (i, j, k, real).
        Column::Vec4(_) | Column::Quat(_) => Ty::Vec4,
        Column::Mat4(_) | Column::Token(_) => return None,
    })
}

/// The type a maths node gives for the types it gets, or `None` when it
/// does not take them.
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
        _ => None,
    }
}

#[derive(Clone, Copy, Debug)]
struct Val { reg: usize, uniform: bool, ty: Ty }

struct Lower<'a> {
    graph:     &'a Graph,
    geo:       &'a Geo,
    plan:      Plan,
    /// Node outputs already lowered.
    done:      HashMap<usize, Val>,
    /// Nodes being lowered: meeting one again is a cycle.
    visiting:  HashSet<usize>,
    /// Batch registers holding a uniform register, so each is broadcast once.
    broadcast: HashMap<usize, usize>,
    reads:     HashMap<String, usize>,
    /// Attributes set so far, in Execute order: a later Get Data of one of
    /// them gets the value set, as when the ports run one after the other.
    written:   HashMap<String, Val>,
    set:       HashSet<usize>,
}

impl Lower<'_> {
    fn emit(&mut self, uniform: bool, ty: Ty, make: impl FnOnce(usize) -> Ins) -> Val {
        let p = if uniform { &mut self.plan.uniform } else { &mut self.plan.batch };
        let reg = p.regs.len();
        p.regs.push(ty);
        p.code.push(make(reg));
        Val { reg, uniform, ty }
    }

    /// `v` in a batch register.
    fn varying(&mut self, v: Val) -> usize {
        if !v.uniform { return v.reg; }
        if let Some(&r) = self.broadcast.get(&v.reg) { return r; }
        let r = self.emit(false, v.ty, |dst| Ins::Broadcast { dst, from: v.reg }).reg;
        self.broadcast.insert(v.reg, r);
        r
    }

    fn constant(&mut self, value: Value) -> Val {
        self.emit(true, value.ty(), |dst| Ins::Const { dst, value })
    }

    /// The value of a node's output.
    fn value(&mut self, node: usize) -> Result<Val, CompileError> {
        if let Some(v) = self.done.get(&node) { return Ok(*v); }
        if !self.visiting.insert(node) { return error(node, "This node feeds itself: a cycle"); }
        let graph = self.graph;
        let n = graph.nodes.get(node).ok_or(CompileError { node, message: "No such node".into() })?;
        if let Some(a) = n.op.arity() {
            if n.inputs.len() != a { return error(node, format!("Takes {a} inputs, has {}", n.inputs.len())); }
        }
        let v = match &n.op {
            Op::Root { .. } => return error(node, "The root has no output"),
            Op::SetData { .. } => return error(node, "Set Data gives an Execute: wire it into the root"),
            Op::Constant(value) => self.constant(*value),
            Op::Parameter { name, default } => {
                let index = match self.plan.params.iter().position(|(p, _)| p == name) {
                    Some(i) => i,
                    None => { self.plan.params.push((name.clone(), *default)); self.plan.params.len() - 1 }
                };
                self.emit(true, default.ty(), |dst| Ins::Param { dst, index })
            }
            Op::GetData { name, context, reduce } => self.get_data(node, name, *context, *reduce)?,
            Op::Math(op) => self.math(node, *op)?,
        };
        self.visiting.remove(&node);
        self.done.insert(node, v);
        Ok(v)
    }

    fn get_data(&mut self, node: usize, name: &str, from: Option<Context>, reduce: Reduce) -> Result<Val, CompileError> {
        let (geo, context) = (self.geo, self.plan.context);
        let own = from.is_none() || from == Some(context);
        // Set by an earlier port, in the graph's own context: the new values.
        if own {
            if let Some(v) = self.written.get(column_in(geo, context, name)) { return Ok(*v); }
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
        let column = if found.derived { name } else { column_in(geo, attr_context, name) };
        let Some(ty) = found.ty else {
            return error(node, format!("'{name}' is of a type ICE does not read yet (matrices, tokens)"));
        };
        // Object is one value for every element.
        let uniform = attr_context == Context::Object;
        // Read across contexts, each reduction is a read of its own.
        let reduce = if attr_context == context || uniform { Reduce::Average } else { reduce };
        let key = format!("{attr_context:?}/{column}/{reduce:?}");
        let read = match self.reads.get(&key) {
            Some(&r) => r,
            None => {
                self.plan.reads.push(Read { name: column.to_string(), context: attr_context, reduce, derived: found.derived });
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
            if let Input::Wire { node: from, .. } = input { vals[i] = Some(self.value(*from)?); }
        }
        let wired = vals.iter().flatten().map(|v| v.ty).find(|t| *t != Ty::Float).or(vals.iter().flatten().map(|v| v.ty).next());
        for (i, input) in inputs.iter().enumerate() {
            if let Input::Value(value) = input {
                let keep_float = value.ty() == Ty::Float
                    && (matches!(op, Math::Multiply | Math::Divide) || (op == Math::Lerp && i == 2));
                let value = match wired {
                    Some(t) if !keep_float => value.convert(t).unwrap_or(*value),
                    _ => *value,
                };
                vals[i] = Some(self.constant(value));
            }
        }
        let vals: Vec<Val> = vals.into_iter().flatten().collect();
        let tys: Vec<Ty> = vals.iter().map(|v| v.ty).collect();
        let Some(ty) = math_ty(op, &tys) else {
            return error(node, format!("{op:?} does not take {tys:?}"));
        };
        let uniform = vals.iter().all(|v| v.uniform);
        let mut args = [0usize; 3];
        for (i, v) in vals.iter().enumerate() {
            args[i] = if uniform { v.reg } else { self.varying(*v) };
        }
        Ok(self.emit(uniform, ty, |dst| Ins::Math { op, dst, args }))
    }

    /// A Set Data node, run from the root.
    fn set_data(&mut self, node: usize) -> Result<(), CompileError> {
        if !self.set.insert(node) { return Ok(()); }
        let (graph, geo) = (self.graph, self.geo);
        let n = &graph.nodes[node];
        let Op::SetData { name, context: to, reduce } = &n.op else { return error(node, "Only Execute outputs go into the root") };
        let context = to.unwrap_or(self.plan.context);
        // Into its own context, one value each: nothing to make one.
        let reduce = if context == self.plan.context { Reduce::Average } else { *reduce };
        let own = context == self.plan.context;
        if name.trim().is_empty() { return error(node, "Set Data needs the name of the attribute it sets"); }
        if column_name(name) == POINTS && context != Context::Point {
            return error(node, format!("The points are per Point; this writes them per {context:?}"));
        }
        let column = column_in(geo, context, name).to_string();
        let mut v = match n.inputs.first() {
            Some(Input::Wire { node: from, .. }) => self.value(*from)?,
            Some(Input::Value(value)) => self.constant(*value),
            None => return error(node, "Set Data takes a value"),
        };
        let existing = geo.attr(context, &column);
        if let Some(t) = existing.and_then(|a| column_ty(&a.column)) {
            if t != v.ty {
                // A float or int value takes the attribute's type, as a value would.
                match n.inputs.first() {
                    Some(Input::Value(value)) if value.convert(t).is_some() => v = self.constant(value.convert(t).unwrap()),
                    _ => return error(node, format!("'{name}' is {t:?}; this sets {:?}", v.ty)),
                }
            }
        }
        let role = existing.map(|a| a.role).unwrap_or(if column == POINTS { Role::Point } else { Role::None });
        let write = match self.plan.writes.iter().position(|w| w.name == column && w.context == context) {
            Some(i) => i,
            None => {
                self.plan.writes.push(Write { name: column.clone(), ty: v.ty, role, context, reduce });
                self.plan.writes.len() - 1
            }
        };
        // The same attribute set twice: the later value is the one kept.
        self.plan.writes[write].ty = v.ty;
        self.plan.writes[write].reduce = reduce;
        let src = self.varying(v);
        self.plan.batch.code.push(Ins::Store { write, src });
        // Read back by a later port in the graph's own context; written
        // into another, it is there once the graph has run.
        if own { self.written.insert(column, v); }
        Ok(())
    }
}

/// Compile `graph` for running on geometry laid out like `geo` (the same
/// attributes, of the same types). Its root is the node with `Op::Root`.
pub fn compile(graph: &Graph, geo: &Geo) -> Result<Plan, CompileError> {
    let Some(root) = graph.nodes.iter().position(|n| matches!(n.op, Op::Root { .. })) else {
        return error(0, "The graph has no root");
    };
    let Op::Root { context } = graph.nodes[root].op else { unreachable!() };
    let mut l = Lower {
        graph, geo,
        plan: Plan { context, reads: vec![], params: vec![], uniform: Program::default(), batch: Program::default(), writes: vec![] },
        done: HashMap::new(), visiting: HashSet::new(), broadcast: HashMap::new(),
        reads: HashMap::new(), written: HashMap::new(), set: HashSet::new(),
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
        if let Ins::Store { write, .. } = ins { last.insert(*write, i); }
    }
    let code = std::mem::take(&mut l.plan.batch.code);
    l.plan.batch.code = code.into_iter().enumerate()
        .filter(|(i, ins)| !matches!(ins, Ins::Store { write, .. } if last.get(write) != Some(i)))
        .map(|(_, ins)| ins)
        .collect();
    Ok(l.plan)
}

/// The type of each node's output, by node, for showing on ports and wires:
/// what `compile` would give it on geometry laid out like `geo`, for every
/// node, wired into the root or not. `None` for the root, for Set Data
/// (its output is an Execute), and for what does not resolve (an attribute
/// `geo` does not have, maths on types it does not take).
pub fn output_types(graph: &Graph, geo: &Geo) -> Vec<Option<Ty>> {
    let context = graph.nodes.iter()
        .find_map(|n| match n.op { Op::Root { context } => Some(context), _ => None })
        .unwrap_or(Context::Point);
    // Per node: not yet looked at (None), or its type.
    let mut memo: Vec<Option<Option<Ty>>> = vec![None; graph.nodes.len()];
    (0..graph.nodes.len()).map(|i| type_of(graph, geo, context, i, &mut memo, 0)).collect()
}

fn type_of(graph: &Graph, geo: &Geo, context: Context, node: usize, memo: &mut Vec<Option<Option<Ty>>>, depth: usize) -> Option<Ty> {
    if depth > 256 { return None; }
    if let Some(t) = memo.get(node).copied().flatten() { return t; }
    let n = graph.nodes.get(node)?;
    // Met again while looking: a cycle, no type.
    memo[node] = Some(None);
    let t = match &n.op {
        Op::Root { .. } | Op::SetData { .. } => None,
        Op::Constant(v) => Some(v.ty()),
        Op::Parameter { default, .. } => Some(default.ty()),
        Op::GetData { name, context: from, .. } => {
            let at = |c: Context| find_in(geo, c, name);
            match from {
                Some(c) => at(*c),
                None => at(context).or_else(|| at(Context::Object)),
            }.and_then(|f| f.ty)
        }
        Op::Math(op) => {
            let wired: Vec<Option<Ty>> = n.inputs.iter().map(|i| match i {
                Input::Wire { node: from, .. } => type_of(graph, geo, context, *from, memo, depth + 1),
                Input::Value(_) => None,
            }).collect();
            // As `Lower::math`: values take the type of what they meet.
            let met = wired.iter().flatten().copied().find(|t| *t != Ty::Float).or(wired.iter().flatten().copied().next());
            let tys: Option<Vec<Ty>> = n.inputs.iter().enumerate().map(|(i, input)| match input {
                Input::Wire { .. } => wired[i],
                Input::Value(v) => {
                    let keep_float = v.ty() == Ty::Float && (matches!(op, Math::Multiply | Math::Divide) || (*op == Math::Lerp && i == 2));
                    Some(match met { Some(t) if !keep_float => v.convert(t).unwrap_or(*v).ty(), _ => v.ty() })
                }
            }).collect();
            tys.and_then(|t| math_ty(*op, &t))
        }
    };
    memo[node] = Some(t);
    t
}
