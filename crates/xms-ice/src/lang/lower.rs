//! A snippet's syntax tree to engine nodes. The body is worked through once,
//! statement by statement, each variable and attribute holding the value it has
//! at that point (a wire, or a constant): every assignment is a new value. An
//! `if` works out both sides and joins what they changed with If (`Select`)
//! nodes; a loop of a count known when compiling is worked through that many
//! times; a function is worked through where it is called. What comes out is
//! nodes of the engine's own kinds, compiled with the rest of the graph.

use std::collections::HashMap;

use xms_geo::{Context, Geo};

use super::lex::Pos;
use super::parse::{Expr, ExprKind, Function, Param, Snippet, Stmt, Type};
use super::Error;
use crate::compile::attribute_type;
use crate::graph::{
    ArrayOp, Compose, Decompose, Graph, Input, Math, Op, Resample, CLOSEST_PARAM_UVS, CLOSEST_POLYGONS, NEAR_DISTANCES,
    NEAR_POINTS,
};
use crate::types::{Ty, Value};

/// How many times a loop may run, worked through, before it is refused.
pub const LOOP_LIMIT: usize = 1024;

/// What a 3D vector stands for, as OSL's types say: `transform` moves a point,
/// turns a vector, and turns a normal by the inverse transpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Point,
    Vector,
    Normal,
    Color,
}

/// A value's type in the language: the engine's, a 3D vector's role, and
/// whether it is a list per element (an array).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct T {
    ty: Ty,
    role: Option<Role>,
    list: bool,
}

impl T {
    fn of(ty: Ty) -> T {
        T {
            ty,
            role: (ty == Ty::Vec3).then_some(Role::Vector),
            list: false,
        }
    }
    fn listed(self, list: bool) -> T {
        T { list, ..self }
    }
    fn name(self) -> String {
        let base = match (self.ty, self.role) {
            (Ty::Vec3, Some(Role::Point)) => "point",
            (Ty::Vec3, Some(Role::Normal)) => "normal",
            (Ty::Vec3, Some(Role::Color)) => "color",
            (Ty::Vec3, _) => "vector",
            (Ty::Bool, _) => "int (a comparison)",
            (Ty::Int, _) => "int",
            (Ty::Float, _) => "float",
            (Ty::Vec2, _) => "vector2",
            (Ty::Vec4, _) => "vector4",
            (Ty::Quat, _) => "quat",
            (Ty::Mat4, _) => "matrix",
            (Ty::Token, _) if self.list => return "geometry".into(),
            (Ty::Token, _) => "string",
            (Ty::Sample, _) => "surface sample",
        };
        if self.list {
            format!("{base}[]")
        } else {
            base.to_string()
        }
    }
}

/// A type as written, as the language's.
fn written(t: Type, array: bool, pos: Pos) -> Result<T, Error> {
    let (ty, role) = match t {
        Type::Int => (Ty::Int, None),
        Type::Float => (Ty::Float, None),
        Type::Point => (Ty::Vec3, Some(Role::Point)),
        Type::Vector => (Ty::Vec3, Some(Role::Vector)),
        Type::Normal => (Ty::Vec3, Some(Role::Normal)),
        Type::Color => (Ty::Vec3, Some(Role::Color)),
        Type::Vector2 => (Ty::Vec2, None),
        Type::Vector4 => (Ty::Vec4, None),
        Type::Quat => (Ty::Quat, None),
        Type::Matrix => (Ty::Mat4, None),
        Type::Geometry => return Ok(T::of(Ty::Token).listed(true)),
        Type::String | Type::Void => {
            return Err(Error::at(pos, format!("A {} is not a value a snippet holds", t.name())));
        }
    };
    Ok(T { ty, role, list: array })
}

/// A value: where it comes from, its type, and what it is when known while
/// compiling (a number, a string), so loops can count and indices pick.
#[derive(Clone, Debug)]
struct V {
    at: Input,
    t: T,
    known: Option<Known>,
    /// Changed by each assignment: an `if` joins what its sides changed.
    version: u32,
}

#[derive(Clone, Debug, PartialEq)]
enum Known {
    Int(i32),
    Float(f32),
    Str(String),
}

/// A snippet lowered: its outputs' values, its attributes' Set Data nodes (in
/// the order they were first set: Execute ports, to wire into the root), and
/// where in the text each node came from.
#[derive(Clone, Debug, Default)]
pub struct Lowered {
    pub outputs: Vec<(String, Input)>,
    pub sets: Vec<usize>,
    pub at: Vec<(usize, Pos)>,
}

impl Lowered {
    /// Where in the text node `node` came from, for an error the engine gives.
    pub fn pos_of(&self, node: usize) -> Option<Pos> {
        self.at.iter().find(|(n, _)| *n == node).map(|(_, p)| *p)
    }
}

/// Lower `snippet`, run over `context`, into `graph`. `inputs`: the value of
/// each input parameter (a wire, or its own value), by name; one left out takes
/// its default. `geo`: the geometry it will run on, for the types of the
/// attributes it reads (without it, only declared attributes are known).
pub fn lower(
    snippet: &Snippet,
    context: Context,
    inputs: &[(String, Input)],
    geo: Option<&Geo>,
    graph: &mut Graph,
) -> Result<Lowered, Error> {
    if let Some(pinned) = snippet.meta_str("context") {
        match context_named(pinned) {
            Some(c) if c == context => {}
            Some(_) => {
                let at = format!("{context:?}").to_lowercase();
                return Err(Error::at(
                    snippet.pos,
                    format!("This snippet runs over a {pinned} (its context), not a {at}"),
                ));
            }
            None => {
                return Err(Error::at(
                    snippet.pos,
                    format!("\"{pinned}\" is not a context: \"point\", \"face\", \"corner\" or \"object\""),
                ))
            }
        }
    }
    let mut l = Lower::new(graph, context, geo, &snippet.functions, snippet.pos);
    for p in &snippet.params {
        let t = written(p.ty, p.array, p.pos)?;
        let v = match inputs.iter().find(|(n, _)| *n == p.name) {
            // A geometry with nothing wired: its default, a reference input.
            Some((_, Input::Value(_))) if p.ty == Type::Geometry => l.default(p)?,
            // A value, as the parameter's type (an int given to a float).
            Some((_, Input::Value(value))) if !p.output => {
                let value = value.convert(t.ty).unwrap_or(*value);
                l.value(Input::Value(value), t, None)
            }
            Some((_, at)) if !p.output => l.value(at.clone(), t, None),
            _ => l.default(p)?,
        };
        l.scopes[0].insert(p.name.clone(), v);
    }
    l.stmts(&snippet.body)?;
    let mut out = Lowered::default();
    for p in snippet.params.iter().filter(|p| p.output) {
        out.outputs.push((p.name.clone(), l.scopes[0][&p.name].at.clone()));
    }
    for name in l.order.clone() {
        let v = l.attrs[&name].clone();
        let set = l.node(Op::set(&name), vec![v.at]);
        out.sets.push(set);
    }
    out.at = l.at;
    Ok(out)
}

/// A parameter's default as a value, for the node's port: worked out from the
/// text when it is a constant (`0.5`, `color(1, 0, 0)`).
pub fn default_value(p: &Param) -> Option<Value> {
    let mut graph = Graph::default();
    let mut l = Lower::new(&mut graph, Context::Point, None, &[], p.pos);
    match l.default(p).ok()?.at {
        Input::Value(value) => Some(value),
        Input::Wire { .. } => None,
    }
}

/// A snippet's parameter as the node's port: its name, its engine type (a
/// geometry is `Token`: paths), whether it is an output, its default value, its
/// label (`[[ string label = … ]]`).
#[derive(Clone, Debug, PartialEq)]
pub struct Port {
    pub name: String,
    pub ty: Ty,
    pub output: bool,
    pub default: Option<Value>,
    pub label: Option<String>,
    /// A geometry's reference input, by its default (`geometry ground = 2`):
    /// 1 when it has none.
    pub reference: Option<usize>,
}

/// The reference input a geometry parameter reads, by its default: `= 2`, or
/// 1 with none; `None` for one given by get_reference, or not a geometry.
fn reference_of(p: &Param) -> Option<usize> {
    if p.ty != Type::Geometry {
        return None;
    }
    match p.default.as_ref().map(|e| &e.kind) {
        None => Some(1),
        Some(ExprKind::Int(n)) => Some((*n).max(0) as usize),
        _ => None,
    }
}

/// The ports a snippet's signature makes, in order.
pub fn ports(snippet: &Snippet) -> Vec<Port> {
    snippet
        .params
        .iter()
        .map(|p| Port {
            name: p.name.clone(),
            ty: written(p.ty, p.array, p.pos).map(|t| t.ty).unwrap_or(Ty::Float),
            output: p.output,
            default: default_value(p),
            reference: reference_of(p),
            label: p
                .meta
                .iter()
                .find(|(n, _)| n == "label")
                .and_then(|(_, e)| match &e.kind {
                    ExprKind::Str(s) => Some(s.clone()),
                    _ => None,
                }),
        })
        .collect()
}

/// The context a snippet pins in its metadata (`[[ string context = "face"
/// ]]`), if it does and it names one.
pub fn pinned(snippet: &Snippet) -> Option<Context> {
    snippet.meta_str("context").and_then(context_named)
}

struct Lower<'a> {
    graph: &'a mut Graph,
    context: Context,
    geo: Option<&'a Geo>,
    functions: &'a [Function],
    /// Variables, innermost scope last.
    scopes: Vec<HashMap<String, V>>,
    /// Attributes set so far, by name: what they hold now.
    attrs: HashMap<String, V>,
    /// Attributes declared in the body (`vector @vel;`): their types.
    declared: HashMap<String, T>,
    /// Attributes in the order first set.
    order: Vec<String>,
    /// Attributes read from the geometry: one Get Data each, however often
    /// read.
    reads: HashMap<String, V>,
    at: Vec<(usize, Pos)>,
    /// The statement being lowered: where nodes made now come from.
    pos: Pos,
    next: u32,
    /// Functions being worked through, one inside another.
    depth: usize,
    /// What the function being worked through gave back.
    returned: Option<V>,
}

/// The engine's parts of a vector, by member name.
fn component(member: &str, ty: Ty) -> Option<usize> {
    let size = match ty {
        Ty::Vec2 => 2,
        Ty::Vec3 => 3,
        Ty::Vec4 | Ty::Quat => 4,
        _ => return None,
    };
    let k = match member {
        "x" | "r" => 0,
        "y" | "g" => 1,
        "z" | "b" => 2,
        "w" | "a" => 3,
        _ => return None,
    };
    (k < size).then_some(k)
}

fn decompose_of(ty: Ty) -> Option<(Decompose, Compose)> {
    Some(match ty {
        Ty::Vec2 => (Decompose::Vec2, Compose::Vec2),
        Ty::Vec3 => (Decompose::Vec3, Compose::Vec3),
        Ty::Vec4 => (Decompose::Vec4, Compose::Vec4),
        Ty::Quat => (Decompose::Quat, Compose::Quat),
        _ => return None,
    })
}

fn context_named(name: &str) -> Option<Context> {
    Some(match name {
        "point" => Context::Point,
        "face" => Context::Face,
        "corner" => Context::Corner,
        "object" => Context::Object,
        _ => return None,
    })
}

/// ACEScg's luminance weights (AP1, Imago's working space).
const LUMINANCE: [f32; 3] = [0.272_228_7, 0.674_081_8, 0.053_689_5];

impl<'a> Lower<'a> {
    fn new(graph: &'a mut Graph, context: Context, geo: Option<&'a Geo>, functions: &'a [Function], pos: Pos) -> Self {
        Lower {
            graph,
            context,
            geo,
            functions,
            scopes: vec![HashMap::new()],
            attrs: HashMap::new(),
            declared: HashMap::new(),
            order: vec![],
            reads: HashMap::new(),
            at: vec![],
            pos,
            next: 0,
            depth: 0,
            returned: None,
        }
    }

    /// A parameter's value when nothing is given: its default, or zero (no
    /// prims, for a geometry).
    fn default(&mut self, p: &Param) -> Result<V, Error> {
        let t = written(p.ty, p.array, p.pos)?;
        // A geometry's: every prim on its reference input (`= 1`, the first
        // after the scene, when not written), or what get_reference gives.
        if p.ty == Type::Geometry {
            if let Some(input) = reference_of(p) {
                let pos = p.default.as_ref().map_or(p.pos, |e| e.pos);
                if input == 0 {
                    return self.err(pos, "Reference inputs are numbered from 1: geometry ground = 1");
                }
                return Ok(self.made(Op::GetReference { input, paths: None }, vec![], 0, t));
            }
        }
        match &p.default {
            Some(e) => {
                let v = self.expr(e)?;
                self.convert(v, t, e.pos)
            }
            None => Ok(self.zero(t)),
        }
    }

    fn err<R>(&self, pos: Pos, message: impl Into<String>) -> Result<R, Error> {
        Err(Error::at(pos, message))
    }

    fn value(&mut self, at: Input, t: T, known: Option<Known>) -> V {
        self.next += 1;
        V {
            at,
            t,
            known,
            version: self.next,
        }
    }

    fn constant(&mut self, value: Value) -> V {
        let known = match value {
            Value::Int(i) => Some(Known::Int(i)),
            Value::Float(f) => Some(Known::Float(f)),
            _ => None,
        };
        self.value(Input::Value(value), T::of(value.ty()), known)
    }

    fn node(&mut self, op: Op, inputs: Vec<Input>) -> usize {
        let n = self.graph.add(op, inputs);
        self.at.push((n, self.pos));
        n
    }

    /// A node's output as a value of type `t`.
    fn made(&mut self, op: Op, inputs: Vec<Input>, output: usize, t: T) -> V {
        let node = self.node(op, inputs);
        self.value(Input::Wire { node, output }, t, None)
    }

    fn math(&mut self, op: Math, args: &[&V], t: T) -> V {
        let inputs = args.iter().map(|v| v.at.clone()).collect();
        self.made(Op::Math(op), inputs, 0, t)
    }

    fn zero(&mut self, t: T) -> V {
        let value = match t.ty {
            Ty::Bool => Value::Bool(false),
            Ty::Int => Value::Int(0),
            Ty::Float => Value::Float(0.0),
            Ty::Vec2 => Value::Vec2([0.0; 2]),
            Ty::Vec3 => Value::Vec3([0.0; 3]),
            Ty::Vec4 => Value::Vec4([0.0; 4]),
            Ty::Quat => Value::Quat([0.0, 0.0, 0.0, 1.0]),
            Ty::Mat4 => Value::Mat4(glam::Mat4::IDENTITY.to_cols_array()),
            Ty::Sample | Ty::Token => Value::Int(0),
        };
        let mut v = self.constant(value);
        v.t = t;
        v
    }

    fn lookup(&self, name: &str) -> Option<&V> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    /// Give variable `name` a new value, where it was declared.
    fn assign_var(&mut self, name: &str, v: V, pos: Pos) -> Result<(), Error> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(old) = scope.get_mut(name) {
                *old = v;
                return Ok(());
            }
        }
        self.err(pos, format!("{name} is not declared: `float {name} = …;`"))
    }

    // Types.

    /// `v` as type `to`: an int as a float, a number spread over a vector, a
    /// comparison as an int, an int as a condition. Roles follow `to`.
    fn convert(&mut self, v: V, to: T, pos: Pos) -> Result<V, Error> {
        if v.t.ty == to.ty && v.t.list == to.list {
            return Ok(V { t: to, ..v });
        }
        if v.t.list && !to.list {
            return self.err(pos, format!("A {} is a list: one value is wanted here", v.t.name()));
        }
        let to_one = T { list: v.t.list, ..to };
        let out = match (v.t.ty, to.ty) {
            (Ty::Int, Ty::Float) => match (&v.at, &v.known) {
                (_, Some(Known::Int(i))) => {
                    let mut c = self.constant(Value::Float(*i as f32));
                    c.t = to_one;
                    c
                }
                _ => {
                    let one = self.constant(Value::Float(1.0));
                    self.math(Math::Multiply, &[&v, &one], to_one)
                }
            },
            (Ty::Bool, Ty::Int | Ty::Float) => {
                let (one, zero) = if to.ty == Ty::Int {
                    (Value::Int(1), Value::Int(0))
                } else {
                    (Value::Float(1.0), Value::Float(0.0))
                };
                let (one, zero) = (self.constant(one), self.constant(zero));
                self.math(Math::Select, &[&v, &one, &zero], to_one)
            }
            (Ty::Int | Ty::Float, Ty::Bool) => {
                let zero = self.constant(if v.t.ty == Ty::Int {
                    Value::Int(0)
                } else {
                    Value::Float(0.0)
                });
                self.math(Math::NotEqual, &[&v, &zero], to_one)
            }
            (Ty::Int | Ty::Float, Ty::Vec2 | Ty::Vec3 | Ty::Vec4) => {
                let f = self.convert(v, T::of(Ty::Float).listed(to_one.list), pos)?;
                if let Input::Value(value) = &f.at {
                    if let Some(spread) = value.convert(to.ty) {
                        let mut c = self.constant(spread);
                        c.t = to_one;
                        return Ok(c);
                    }
                }
                let (_, compose) = decompose_of(to.ty).expect("a vector");
                let parts = vec![f.at.clone(); compose.inputs().len()];
                self.made(Op::Compose(compose), parts, 0, to_one)
            }
            _ => {
                return self.err(pos, format!("A {} where a {} is wanted", v.t.name(), to.name()));
            }
        };
        Ok(out)
    }

    /// `v` as a `ty`, a list if it is one.
    fn to(&mut self, v: V, ty: Ty, pos: Pos) -> Result<V, Error> {
        let t = T::of(ty).listed(v.t.list);
        self.convert(v, t, pos)
    }

    /// Two values made one type for maths: ints with floats are floats, a
    /// number with a vector is that vector (spread), comparisons are ints.
    fn unify(&mut self, a: V, b: V, pos: Pos) -> Result<(V, V), Error> {
        let a = if a.t.ty == Ty::Bool {
            self.convert(a, T::of(Ty::Int), pos)?
        } else {
            a
        };
        let b = if b.t.ty == Ty::Bool {
            self.convert(b, T::of(Ty::Int), pos)?
        } else {
            b
        };
        let rank = |t: Ty| match t {
            Ty::Int => 0,
            Ty::Float => 1,
            Ty::Vec2 | Ty::Vec3 | Ty::Vec4 => 2,
            _ => 3,
        };
        let (ra, rb) = (rank(a.t.ty), rank(b.t.ty));
        if ra == 3 || rb == 3 || a.t.ty == b.t.ty || (ra == 2 && rb == 2) {
            return Ok((a, b));
        }
        if ra < rb {
            let to = T { list: a.t.list, ..b.t };
            Ok((self.convert(a, to, pos)?, b))
        } else {
            let to = T { list: b.t.list, ..a.t };
            Ok((a, self.convert(b, to, pos)?))
        }
    }

    // Expressions.

    fn expr(&mut self, e: &Expr) -> Result<V, Error> {
        let pos = e.pos;
        match &e.kind {
            ExprKind::Int(i) => Ok(self.constant(Value::Int(*i))),
            ExprKind::Float(f) => Ok(self.constant(Value::Float(*f))),
            ExprKind::Str(s) => {
                let mut v = self.constant(Value::Int(0));
                v.t = T::of(Ty::Token);
                v.known = Some(Known::Str(s.clone()));
                Ok(v)
            }
            ExprKind::Var(name) => match self.lookup(name) {
                Some(v) => Ok(v.clone()),
                None => self.err(pos, format!("{name} is not declared")),
            },
            ExprKind::Attr(name) => self.read_attr(name, pos),
            ExprKind::Unary(op, a) => {
                let a = self.expr(a)?;
                match *op {
                    "-" => {
                        if let Some(k) = &a.known {
                            match k {
                                Known::Int(i) => return Ok(self.constant(Value::Int(i.wrapping_neg()))),
                                Known::Float(f) => return Ok(self.constant(Value::Float(-f))),
                                Known::Str(_) => {}
                            }
                        }
                        let a = if a.t.ty == Ty::Bool {
                            self.convert(a, T::of(Ty::Int), pos)?
                        } else {
                            a
                        };
                        let t = a.t;
                        Ok(self.math(Math::Negate, &[&a], t))
                    }
                    _ => {
                        if let Some(Known::Int(i)) = a.known {
                            return Ok(self.constant(Value::Int((i == 0) as i32)));
                        }
                        let b = self.convert(a, T::of(Ty::Bool), pos)?;
                        let t = b.t;
                        Ok(self.math(Math::Not, &[&b], t))
                    }
                }
            }
            ExprKind::Binary(op, a, b) => {
                let (a, b) = (self.expr(a)?, self.expr(b)?);
                self.binary(op, a, b, pos)
            }
            ExprKind::Ternary(c, a, b) => {
                let c = self.expr(c)?;
                let (a, b) = (self.expr(a)?, self.expr(b)?);
                self.select(c, a, b, pos)
            }
            ExprKind::Member(of, member) => {
                let v = self.expr(of)?;
                self.part(v, member, pos)
            }
            ExprKind::Index(of, i) => {
                let (v, i) = (self.expr(of)?, self.expr(i)?);
                self.index(v, i, pos)
            }
            ExprKind::Call(name, args) => self.call(name, args, pos),
        }
    }

    fn binary(&mut self, op: &str, a: V, b: V, pos: Pos) -> Result<V, Error> {
        // Worked out now when both are known: loops count with these.
        if let (Some(x), Some(y)) = (num(&a.known), num(&b.known)) {
            let ints = matches!((&a.known, &b.known), (Some(Known::Int(_)), Some(Known::Int(_))));
            let (xi, yi) = (x as i32, y as i32);
            let int = |i: i32| Some(Value::Int(i));
            let folded = match op {
                "+" if ints => int(xi.wrapping_add(yi)),
                "-" if ints => int(xi.wrapping_sub(yi)),
                "*" if ints => int(xi.wrapping_mul(yi)),
                "/" if ints => xi.checked_div(yi).map(Value::Int),
                "%" if ints => xi.checked_rem(yi).map(Value::Int),
                "+" => Some(Value::Float((x + y) as f32)),
                "-" => Some(Value::Float((x - y) as f32)),
                "*" => Some(Value::Float((x * y) as f32)),
                "/" => Some(Value::Float((x / y) as f32)),
                "<" => int((x < y) as i32),
                "<=" => int((x <= y) as i32),
                ">" => int((x > y) as i32),
                ">=" => int((x >= y) as i32),
                "==" => int((x == y) as i32),
                "!=" => int((x != y) as i32),
                "&&" => int((x != 0.0 && y != 0.0) as i32),
                "||" => int((x != 0.0 || y != 0.0) as i32),
                _ => None,
            };
            if let Some(v) = folded {
                return Ok(self.constant(v));
            }
        }
        let list = a.t.list || b.t.list;
        match op {
            "&&" | "||" => {
                let a = self.to(a, Ty::Bool, pos)?;
                let b = self.to(b, Ty::Bool, pos)?;
                let m = if op == "&&" { Math::And } else { Math::Or };
                Ok(self.math(m, &[&a, &b], T::of(Ty::Bool).listed(list)))
            }
            "<" | "<=" | ">" | ">=" | "==" | "!=" => {
                let (a, b) = self.unify(a, b, pos)?;
                let ordered = matches!(op, "<" | "<=" | ">" | ">=");
                if ordered && !matches!(a.t.ty, Ty::Int | Ty::Float) {
                    return self.err(
                        pos,
                        format!("A {} is not less or greater than another: compare numbers", a.t.name()),
                    );
                }
                if a.t.ty != b.t.ty {
                    return self.err(pos, format!("A {} and a {} do not compare", a.t.name(), b.t.name()));
                }
                let m = match op {
                    "<" => Math::Less,
                    "<=" => Math::LessOrEqual,
                    ">" => Math::Greater,
                    ">=" => Math::GreaterOrEqual,
                    "==" => Math::Equal,
                    _ => Math::NotEqual,
                };
                Ok(self.math(m, &[&a, &b], T::of(Ty::Bool).listed(list)))
            }
            "+" | "-" | "*" | "/" | "%" => {
                let (a, b) = self.unify(a, b, pos)?;
                let m = match op {
                    "+" => Math::Add,
                    "-" => Math::Subtract,
                    "*" => Math::Multiply,
                    "/" => Math::Divide,
                    _ => Math::Mod,
                };
                let t = match (a.t.ty, b.t.ty) {
                    (x, y) if x == y && x.is_numeric() => {
                        if op == "%" && !matches!(x, Ty::Int | Ty::Float) {
                            return self.err(pos, "% takes ints or floats");
                        }
                        // A point less a point is a vector; a point moved by
                        // a vector stays a point.
                        let role = match (op, a.t.role, b.t.role) {
                            ("-", Some(Role::Point), Some(Role::Point)) => Some(Role::Vector),
                            (_, Some(Role::Point), _) | (_, _, Some(Role::Point)) => Some(Role::Point),
                            (_, r, _) => r,
                        };
                        T { role, ..a.t }
                    }
                    (Ty::Mat4, Ty::Mat4) | (Ty::Quat, Ty::Quat) if op == "*" => a.t,
                    (x, y) if x.is_vector() && y.is_vector() => {
                        return self.err(
                            pos,
                            format!("A {} and a {} are of different sizes", a.t.name(), b.t.name()),
                        )
                    }
                    _ => return self.err(pos, format!("{op} does not take a {} and a {}", a.t.name(), b.t.name())),
                };
                Ok(self.math(m, &[&a, &b], t.listed(list)))
            }
            _ => self.err(pos, format!("Unknown operator {op}")),
        }
    }

    fn select(&mut self, c: V, a: V, b: V, pos: Pos) -> Result<V, Error> {
        if let Some(k) = num(&c.known) {
            return Ok(if k != 0.0 { a } else { b });
        }
        let c = self.convert(c, T::of(Ty::Bool).listed(false), pos)?;
        let (a, b) = self.unify(a, b, pos)?;
        if a.t.ty != b.t.ty || a.t.list != b.t.list {
            return self.err(
                pos,
                format!("Both sides are of one type, not a {} and a {}", a.t.name(), b.t.name()),
            );
        }
        let t = a.t;
        Ok(self.math(Math::Select, &[&c, &a, &b], t))
    }

    /// `v.x`, `c.r`.
    fn part(&mut self, v: V, member: &str, pos: Pos) -> Result<V, Error> {
        let Some(k) = component(member, v.t.ty) else {
            return self.err(pos, format!("A {} has no .{member}", v.t.name()));
        };
        let (d, _) = decompose_of(v.t.ty).expect("a vector");
        Ok(self.made(
            Op::Decompose(d),
            vec![v.at.clone()],
            k,
            T::of(Ty::Float).listed(v.t.list),
        ))
    }

    /// `v` with part `k` set to `x`.
    fn with_part(&mut self, v: V, k: usize, x: V, pos: Pos) -> Result<V, Error> {
        let x = self.convert(x, T::of(Ty::Float), pos)?;
        let (d, c) = decompose_of(v.t.ty).expect("a vector");
        let n = self.node(Op::Decompose(d), vec![v.at.clone()]);
        let parts = (0..c.inputs().len())
            .map(|i| {
                if i == k {
                    x.at.clone()
                } else {
                    Input::Wire { node: n, output: i }
                }
            })
            .collect();
        Ok(self.made(Op::Compose(c), parts, 0, v.t))
    }

    /// `a[i]`: an item of a list, or a part of a vector (its index known).
    fn index(&mut self, v: V, i: V, pos: Pos) -> Result<V, Error> {
        if v.t.list {
            let i = self.convert(i, T::of(Ty::Int), pos)?;
            let t = T { list: false, ..v.t };
            return Ok(self.made(Op::Array(ArrayOp::Element), vec![v.at, i.at], 0, t));
        }
        match (i.known, v.t.ty.is_vector()) {
            (Some(Known::Int(k)), true) if (0..4).contains(&k) => {
                let member = ["x", "y", "z", "w"][k as usize];
                self.part(v, member, pos)
            }
            (_, true) => self.err(
                pos,
                "A vector's part by a number known when compiling: p[0], p[1], p[2]",
            ),
            _ => self.err(pos, format!("A {} has no items", v.t.name())),
        }
    }

    // Attributes.

    /// The type attribute `name` is in the snippet's context: declared, or the
    /// geometry's.
    fn attr_type(&self, name: &str) -> Option<T> {
        if let Some(t) = self.declared.get(name) {
            return Some(*t);
        }
        if xms_geo::intrinsics::is_intrinsic(name) {
            let i = xms_geo::intrinsics::get(xms_geo::intrinsics::Scope::In(self.context), name)?;
            let ty = match i.kind {
                xms_geo::Kind::Int => Ty::Int,
                xms_geo::Kind::Float => Ty::Float,
                xms_geo::Kind::Vec3 => Ty::Vec3,
                xms_geo::Kind::Mat4 => Ty::Mat4,
                _ => return None,
            };
            return Some(T::of(ty).listed(i.gathers.is_some()));
        }
        let ty = attribute_type(self.geo?, self.context, name)?;
        let mut t = T::of(ty);
        if ty == Ty::Vec3 {
            t.role = Some(match name {
                "P" => Role::Point,
                "N" => Role::Normal,
                "Cd" | "displayColor" => Role::Color,
                _ => Role::Vector,
            });
        }
        Some(t)
    }

    fn read_attr(&mut self, name: &str, pos: Pos) -> Result<V, Error> {
        if let Some(v) = self.attrs.get(name) {
            return Ok(v.clone());
        }
        let Some(t) = self.attr_type(name) else {
            if let Some(geo) = self.geo {
                if let Some(c) = Context::ALL
                    .into_iter()
                    .find(|c| attribute_type(geo, *c, name).is_some())
                {
                    let c = format!("{c:?}").to_lowercase();
                    return self.err(
                        pos,
                        format!("@{name} is per {c}, not per element here: get_data(@{name}, \"{c}\") reads it"),
                    );
                }
                return self.err(
                    pos,
                    format!("No attribute @{name}: declare it to make it (`float @{name} = 0;`)"),
                );
            }
            return self.err(
                pos,
                format!("@{name}'s type is not known here: declare it (`vector @{name};`)"),
            );
        };
        // A list intrinsic (a face's ptIndices): its list.
        if t.list {
            let op = Op::GetArrayData {
                name: name.into(),
                context: None,
            };
            return Ok(self.made(op, vec![], 0, t));
        }
        // Declared and not on the geometry (nor set yet): zero.
        let intrinsic = xms_geo::intrinsics::is_intrinsic(name);
        let there = intrinsic
            || self
                .geo
                .is_some_and(|g| attribute_type(g, self.context, name).is_some());
        if !there && self.geo.is_some() {
            return Ok(self.zero(t));
        }
        if let Some(v) = self.reads.get(name) {
            return Ok(v.clone());
        }
        let v = self.made(Op::get(name), vec![], 0, t);
        self.reads.insert(name.to_string(), v.clone());
        Ok(v)
    }

    fn set_attr(&mut self, name: &str, v: V, pos: Pos) -> Result<(), Error> {
        if xms_geo::intrinsics::is_intrinsic(name) {
            return self.err(pos, format!("@{name} is an intrinsic: read only"));
        }
        let v = match self.attr_type(name) {
            Some(t) => self.convert(v, t, pos)?,
            None if matches!(v.known, Some(Known::Int(_))) => {
                return self.err(
                    pos,
                    format!("@{name} is new and 1 could be an int or a float: declare it (`int @{name};` or `float @{name};`)"),
                );
            }
            // A comparison stored is an int, as OSL's are.
            None if v.t.ty == Ty::Bool => self.convert(v, T::of(Ty::Int), pos)?,
            None => v,
        };
        if v.t.list {
            return self.err(pos, format!("@{name} holds one value per element, not a list"));
        }
        if !self.order.iter().any(|n| n == name) {
            self.order.push(name.to_string());
        }
        self.next += 1;
        let v = V {
            version: self.next,
            ..v
        };
        self.attrs.insert(name.to_string(), v);
        Ok(())
    }

    // Statements.

    fn stmts(&mut self, body: &[Stmt]) -> Result<(), Error> {
        for s in body {
            if self.returned.is_some() {
                break;
            }
            self.stmt(s)?;
        }
        Ok(())
    }

    fn scoped(&mut self, body: &[Stmt]) -> Result<(), Error> {
        self.scopes.push(HashMap::new());
        let done = self.stmts(body);
        self.scopes.pop();
        done
    }

    fn stmt(&mut self, s: &Stmt) -> Result<(), Error> {
        match s {
            Stmt::Block(body) => self.scoped(body),
            Stmt::Decl { ty, array, names } => {
                for (name, init, pos) in names {
                    self.pos = *pos;
                    let t = written(*ty, *array, *pos)?;
                    let v = match init {
                        Some(e) => {
                            let v = self.expr(e)?;
                            self.convert(v, t, e.pos)?
                        }
                        // A list to be given (an output of near_points).
                        None if *array => self.value(Input::Value(Value::Int(0)), t, None),
                        None => self.zero(t),
                    };
                    self.scopes.last_mut().expect("a scope").insert(name.clone(), v);
                }
                Ok(())
            }
            Stmt::AttrDecl { ty, name, init, pos } => {
                self.pos = *pos;
                let t = written(*ty, false, *pos)?;
                if let (Some(geo), true) = (self.geo, !xms_geo::intrinsics::is_intrinsic(name)) {
                    if let Some(found) = attribute_type(geo, self.context, name) {
                        if found != t.ty {
                            return self.err(
                                *pos,
                                format!(
                                    "@{name} is declared a {} but the geometry's is a {}",
                                    t.name(),
                                    found.name()
                                ),
                            );
                        }
                    }
                }
                self.declared.insert(name.clone(), t);
                // With a value: set to it, as VEX's `f@x = …` is.
                match init {
                    Some(e) => {
                        let v = self.expr(e)?;
                        self.set_attr(name, v, e.pos)
                    }
                    None => Ok(()),
                }
            }
            Stmt::Assign { target, op, value, pos } => {
                self.pos = *pos;
                let mut v = self.expr(value)?;
                if *op != "=" {
                    let old = self.expr(target)?;
                    v = self.binary(&op[..1], old, v, *pos)?;
                }
                self.assign(target, v)
            }
            Stmt::Expr(e) => {
                self.pos = e.pos;
                self.expr(e).map(|_| ())
            }
            Stmt::If { cond, then, otherwise } => {
                self.pos = cond.pos;
                let c = self.expr(cond)?;
                // Known while compiling: only that side.
                if let Some(k) = num(&c.known) {
                    return match (k != 0.0, otherwise) {
                        (true, _) => self.stmt(then),
                        (false, Some(o)) => self.stmt(o),
                        (false, None) => Ok(()),
                    };
                }
                let c = self.convert(c, T::of(Ty::Bool), cond.pos)?;
                let (scopes, attrs, order) = (self.scopes.clone(), self.attrs.clone(), self.order.clone());
                self.stmt(then)?;
                let then_scopes = std::mem::replace(&mut self.scopes, scopes);
                let then_attrs = std::mem::replace(&mut self.attrs, attrs.clone());
                let then_order = std::mem::replace(&mut self.order, order);
                if let Some(o) = otherwise {
                    self.stmt(o)?;
                }
                if self.returned.is_some() {
                    return self.err(
                        cond.pos,
                        "`return` inside an `if`: not yet; set a variable, and return it at the end",
                    );
                }
                self.join(&c, then_scopes, then_attrs, then_order, &attrs, cond.pos)
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
                pos,
            } => {
                self.scopes.push(HashMap::new());
                if let Some(i) = init {
                    self.stmt(i)?;
                }
                let mut runs = 0;
                loop {
                    if let Some(c) = cond {
                        let v = self.expr(c)?;
                        match num(&v.known) {
                            Some(k) if k == 0.0 => break,
                            Some(_) => {}
                            None => {
                                return self.err(
                                    c.pos,
                                    "A loop's count must be known when compiling (`i < 4`): loops over a count per element come later",
                                );
                            }
                        }
                    } else {
                        return self.err(*pos, "A loop needs a condition");
                    }
                    runs += 1;
                    if runs > LOOP_LIMIT {
                        return self.err(*pos, format!("This loop runs more than {LOOP_LIMIT} times"));
                    }
                    self.stmt(body)?;
                    if let Some(s) = step {
                        self.stmt(s)?;
                    }
                }
                self.scopes.pop();
                Ok(())
            }
            Stmt::While { pos, .. } => self.err(*pos, "`while` comes with loops over a count per element: not yet"),
            Stmt::Return(value, pos) => {
                if self.depth == 0 {
                    return self.err(*pos, "`return` is for functions; a snippet sets its outputs");
                }
                let v = match value {
                    Some(e) => self.expr(e)?,
                    None => self.zero(T::of(Ty::Int)),
                };
                self.returned = Some(v);
                Ok(())
            }
        }
    }

    /// After an `if`: each variable and attribute either side changed is If
    /// (`Select`) of the two; an attribute first set on one side keeps, on the
    /// other, the value it had (the geometry's, its declared value, or zero).
    fn join(
        &mut self,
        c: &V,
        then_scopes: Vec<HashMap<String, V>>,
        then_attrs: HashMap<String, V>,
        then_order: Vec<String>,
        before: &HashMap<String, V>,
        pos: Pos,
    ) -> Result<(), Error> {
        for (level, scope) in then_scopes.into_iter().enumerate() {
            for (name, t) in scope {
                let Some(e) = self.scopes[level].get(&name).cloned() else {
                    continue;
                };
                if t.version != e.version {
                    let v = self.select(c.clone(), t, e, pos)?;
                    self.scopes[level].insert(name, v);
                }
            }
        }
        let mut names = self.order.clone();
        for n in then_order {
            if !names.contains(&n) {
                names.push(n);
            }
        }
        for name in names.clone() {
            let t = then_attrs.get(&name).cloned();
            let e = self.attrs.get(&name).cloned();
            let old = before.get(&name).cloned();
            let (t, e) = match (t, e) {
                (Some(t), Some(e)) if t.version == e.version => continue,
                (Some(t), Some(e)) => (t, e),
                (None, None) => continue,
                (Some(t), None) => {
                    let old = match old {
                        Some(o) => o,
                        None => self.read_attr(&name, pos)?,
                    };
                    (t, old)
                }
                (None, Some(e)) => {
                    let old = match old {
                        Some(o) => o,
                        None => self.read_attr(&name, pos)?,
                    };
                    (old, e)
                }
            };
            let v = self.select(c.clone(), t, e, pos)?;
            self.attrs.insert(name, v);
        }
        self.order = names;
        Ok(())
    }

    fn assign(&mut self, target: &Expr, v: V) -> Result<(), Error> {
        let pos = target.pos;
        match &target.kind {
            ExprKind::Var(name) => {
                let Some(old) = self.lookup(name).cloned() else {
                    return self.err(pos, format!("{name} is not declared: `float {name} = …;`"));
                };
                let v = self.convert(v, old.t, pos)?;
                let v = self.value(v.at, old.t, v.known);
                self.assign_var(name, v, pos)
            }
            ExprKind::Attr(name) => self.set_attr(name, v, pos),
            ExprKind::Member(of, member) => self.assign_part(of, member, v, pos),
            ExprKind::Index(of, i) => match i.kind {
                ExprKind::Int(k @ 0..=3) => self.assign_part(of, ["x", "y", "z", "w"][k as usize], v, pos),
                _ => self.err(pos, "A vector's part is set by a number written here: p[0] = …"),
            },
            _ => self.err(
                pos,
                "Only a variable, an attribute (@name) or a part of one (p.x) is assigned to",
            ),
        }
    }

    /// `p.x = v`: the whole vector made again with that part.
    fn assign_part(&mut self, of: &Expr, member: &str, v: V, pos: Pos) -> Result<(), Error> {
        let whole = self.expr(of)?;
        let Some(k) = component(member, whole.t.ty) else {
            return self.err(pos, format!("A {} has no .{member}", whole.t.name()));
        };
        let new = self.with_part(whole, k, v, pos)?;
        self.assign(of, new)
    }

    // Calls.

    fn call(&mut self, name: &str, args: &[Expr], pos: Pos) -> Result<V, Error> {
        if let Some(f) = self.functions.iter().find(|f| f.name == name) {
            return self.inline(f, args, pos);
        }
        // Functions with outputs take places to put them, not values.
        match name {
            "near_points" => return self.near_points(args, pos),
            "closest_point_on_surface" => return self.closest(args, pos),
            "get_data" | "get_array_data" => return self.get_data(name == "get_array_data", args, pos),
            _ => {}
        }
        let vals = args.iter().map(|a| self.expr(a)).collect::<Result<Vec<V>, Error>>()?;
        self.builtin(name, vals, pos)
    }

    fn inline(&mut self, f: &Function, args: &[Expr], pos: Pos) -> Result<V, Error> {
        if self.depth > 16 {
            return self.err(
                pos,
                format!(
                    "{} calls itself, or too deep: functions are worked through where called",
                    f.name
                ),
            );
        }
        if args.len() != f.params.len() {
            return self.err(pos, format!("{} takes {} arguments", f.name, f.params.len()));
        }
        let mut scope = HashMap::new();
        for (p, a) in f.params.iter().zip(args) {
            let t = written(p.ty, p.array, p.pos)?;
            let v = self.expr(a)?;
            let v = self.convert(v, t, a.pos)?;
            scope.insert(p.name.clone(), v);
        }
        // A function sees its parameters, not the caller's variables.
        let outer = std::mem::replace(&mut self.scopes, vec![scope]);
        self.depth += 1;
        let ran = self.stmts(&f.body);
        self.depth -= 1;
        let inner = std::mem::replace(&mut self.scopes, outer);
        ran?;
        let given = self.returned.take();
        // Output parameters back into the caller's places.
        for (p, a) in f.params.iter().zip(args).filter(|(p, _)| p.output) {
            let v = inner[0][&p.name].clone();
            self.assign(a, v)?;
        }
        match (f.ty, given) {
            (Type::Void, _) => Ok(self.zero(T::of(Ty::Int))),
            (_, None) => self.err(f.pos, format!("{} gives nothing back: `return …;`", f.name)),
            (ty, Some(v)) => {
                let t = written(ty, false, f.pos)?;
                self.convert(v, t, pos)
            }
        }
    }

    fn arity(&self, name: &str, vals: &[V], n: &[usize], pos: Pos) -> Result<(), Error> {
        if n.contains(&vals.len()) {
            Ok(())
        } else {
            let counts: Vec<String> = n.iter().map(|k| k.to_string()).collect();
            self.err(pos, format!("{name} takes {} arguments", counts.join(" or ")))
        }
    }

    fn builtin(&mut self, name: &str, vals: Vec<V>, pos: Pos) -> Result<V, Error> {
        let float = T::of(Ty::Float);
        let one = |m: Math, l: &mut Self, v: V| -> Result<V, Error> {
            let list = v.t.list;
            let v = l.convert(v, float.listed(list), pos)?;
            Ok(l.math(m, &[&v], float.listed(list)))
        };
        match name {
            // Constructors.
            "point" | "vector" | "normal" | "color" | "vector2" | "vector4" | "quat" | "float" | "matrix" => {
                let t = written(Type::of(name).expect("a type"), false, pos)?;
                match (name, vals.len()) {
                    ("matrix", 1) => match num(&vals[0].known) {
                        Some(k) => Ok(self.constant(Value::Mat4((glam::Mat4::IDENTITY * k as f32).to_cols_array()))),
                        None => self.err(pos, "matrix(1): a number known when compiling"),
                    },
                    (_, 1) => {
                        let v = vals.into_iter().next().expect("one");
                        self.convert(v, t, pos)
                    }
                    (_, n) if decompose_of(t.ty).is_some_and(|(_, c)| c.inputs().len() == n) => {
                        let (_, c) = decompose_of(t.ty).expect("a vector");
                        let mut parts = vec![];
                        let mut known = vec![];
                        let mut list = false;
                        for v in vals {
                            list |= v.t.list;
                            let f = self.to(v, Ty::Float, pos)?;
                            known.push(num(&f.known));
                            parts.push(f.at);
                        }
                        if let Some(k) = known.iter().copied().collect::<Option<Vec<f64>>>() {
                            let k: Vec<f32> = k.into_iter().map(|x| x as f32).collect();
                            let value = match t.ty {
                                Ty::Vec2 => Value::Vec2([k[0], k[1]]),
                                Ty::Vec3 => Value::Vec3([k[0], k[1], k[2]]),
                                Ty::Vec4 => Value::Vec4([k[0], k[1], k[2], k[3]]),
                                _ => Value::Quat([k[0], k[1], k[2], k[3]]),
                            };
                            let mut v = self.constant(value);
                            v.t = t;
                            return Ok(v);
                        }
                        Ok(self.made(Op::Compose(c), parts, 0, t.listed(list)))
                    }
                    _ => self.err(pos, format!("{name}() takes one value, or its parts")),
                }
            }
            "abs" | "sign" | "floor" | "ceil" | "round" | "sqrt" | "exp" | "log" | "sin" | "cos" | "tan" | "asin"
            | "acos" => {
                self.arity(name, &vals, &[1], pos)?;
                let v = vals.into_iter().next().expect("one");
                if name == "abs" && v.t.ty != Ty::Float {
                    let t = v.t;
                    return Ok(self.math(Math::Abs, &[&v], t));
                }
                let m = match name {
                    "abs" => Math::Abs,
                    "sign" => Math::Sign,
                    "floor" => Math::Floor,
                    "ceil" => Math::Ceil,
                    "round" => Math::Round,
                    "sqrt" => Math::Sqrt,
                    "exp" => Math::Exp,
                    "log" => Math::Log,
                    "sin" => Math::Sin,
                    "cos" => Math::Cos,
                    "tan" => Math::Tan,
                    "asin" => Math::Asin,
                    _ => Math::Acos,
                };
                one(m, self, v)
            }
            "pow" | "atan2" | "fmod" => {
                self.arity(name, &vals, &[2], pos)?;
                let list = vals.iter().any(|v| v.t.list);
                let mut vals = vals.into_iter();
                let a = vals.next().expect("two");
                let b = vals.next().expect("two");
                let a = self.to(a.clone(), Ty::Float, pos)?;
                let b = self.to(b.clone(), Ty::Float, pos)?;
                let m = match name {
                    "pow" => Math::Pow,
                    "atan2" => Math::Atan2,
                    _ => Math::Mod,
                };
                Ok(self.math(m, &[&a, &b], float.listed(list)))
            }
            "mod" => {
                self.arity(name, &vals, &[2], pos)?;
                let mut vals = vals.into_iter();
                let (a, b) = (vals.next().expect("two"), vals.next().expect("two"));
                self.binary("%", a, b, pos)
            }
            "min" | "max" if vals.len() == 2 => {
                let mut vals = vals.into_iter();
                let (a, b) = (vals.next().expect("two"), vals.next().expect("two"));
                let (a, b) = self.unify(a, b, pos)?;
                let t = T {
                    list: a.t.list || b.t.list,
                    ..a.t
                };
                let m = if name == "min" { Math::Min } else { Math::Max };
                Ok(self.math(m, &[&a, &b], t))
            }
            "min" | "max" => self.err(pos, format!("{name} of a list is not yet: {name}(a, b) of two values")),
            "clamp" => {
                self.arity(name, &vals, &[3], pos)?;
                let mut vals = vals.into_iter();
                let (x, lo, hi) = (
                    vals.next().expect("3"),
                    vals.next().expect("3"),
                    vals.next().expect("3"),
                );
                let up = self.builtin("max", vec![x, lo], pos)?;
                self.builtin("min", vec![up, hi], pos)
            }
            "mix" => {
                self.arity(name, &vals, &[3], pos)?;
                let mut vals = vals.into_iter();
                let (a, b, t) = (
                    vals.next().expect("3"),
                    vals.next().expect("3"),
                    vals.next().expect("3"),
                );
                let (a, b) = self.unify(a, b, pos)?;
                let (a, b) = if a.t.ty == Ty::Int {
                    (self.convert(a, float, pos)?, self.convert(b, float, pos)?)
                } else {
                    (a, b)
                };
                let t = self.to(t.clone(), Ty::Float, pos)?;
                let out = T {
                    list: a.t.list || b.t.list || t.t.list,
                    ..a.t
                };
                Ok(self.math(Math::Lerp, &[&a, &b, &t], out))
            }
            "step" => {
                self.arity(name, &vals, &[2], pos)?;
                let mut vals = vals.into_iter();
                let (edge, x) = (vals.next().expect("2"), vals.next().expect("2"));
                let below = self.binary("<", x, edge, pos)?;
                let (zero, one) = (self.constant(Value::Float(0.0)), self.constant(Value::Float(1.0)));
                self.select(below, zero, one, pos)
            }
            "smoothstep" => {
                self.arity(name, &vals, &[3], pos)?;
                let mut vals = vals.into_iter();
                let (a, b, x) = (
                    vals.next().expect("3"),
                    vals.next().expect("3"),
                    vals.next().expect("3"),
                );
                let over = self.binary("-", x, a.clone(), pos)?;
                let span = self.binary("-", b, a, pos)?;
                let t = self.binary("/", over, span, pos)?;
                let (zero, one) = (self.constant(Value::Float(0.0)), self.constant(Value::Float(1.0)));
                let t = self.builtin("clamp", vec![t, zero, one], pos)?;
                // t * t * (3 - 2 t)
                let (two, three) = (self.constant(Value::Float(2.0)), self.constant(Value::Float(3.0)));
                let tt = self.binary("*", t.clone(), t.clone(), pos)?;
                let twice = self.binary("*", two, t, pos)?;
                let rest = self.binary("-", three, twice, pos)?;
                self.binary("*", tt, rest, pos)
            }
            "radians" | "degrees" => {
                self.arity(name, &vals, &[1], pos)?;
                let k = if name == "radians" {
                    std::f32::consts::PI / 180.0
                } else {
                    180.0 / std::f32::consts::PI
                };
                let by = self.constant(Value::Float(k));
                self.binary("*", vals.into_iter().next().expect("one"), by, pos)
            }
            "dot" | "cross" | "distance" => {
                self.arity(name, &vals, &[2], pos)?;
                let mut vals = vals.into_iter();
                let (a, b) = (vals.next().expect("2"), vals.next().expect("2"));
                let (a, b) = self.unify(a, b, pos)?;
                if !a.t.ty.is_vector() || a.t.ty != b.t.ty {
                    return self.err(pos, format!("{name} takes two vectors of one size"));
                }
                let list = a.t.list || b.t.list;
                let (m, t) = match name {
                    "dot" => (Math::Dot, float),
                    "distance" => (Math::Distance, float),
                    _ => (
                        Math::Cross,
                        T {
                            role: Some(Role::Vector),
                            ..a.t
                        },
                    ),
                };
                Ok(self.math(m, &[&a, &b], t.listed(list)))
            }
            "length" | "normalize" => {
                self.arity(name, &vals, &[1], pos)?;
                let v = vals.into_iter().next().expect("one");
                if !v.t.ty.is_vector() && v.t.ty != Ty::Quat {
                    return self.err(pos, format!("{name} takes a vector"));
                }
                let (m, t) = if name == "length" {
                    (Math::Length, float.listed(v.t.list))
                } else {
                    (Math::Normalize, v.t)
                };
                Ok(self.math(m, &[&v], t))
            }
            "luminance" => {
                self.arity(name, &vals, &[1], pos)?;
                let v = vals.into_iter().next().expect("one");
                let w = self.constant(Value::Vec3(LUMINANCE));
                let list = v.t.list;
                Ok(self.math(Math::Dot, &[&v, &w], float.listed(list)))
            }
            "inverse" | "transpose" => {
                self.arity(name, &vals, &[1], pos)?;
                let v = vals.into_iter().next().expect("one");
                if v.t.ty != Ty::Mat4 {
                    return self.err(pos, format!("{name} takes a matrix"));
                }
                let m = if name == "inverse" {
                    Math::Invert
                } else {
                    Math::Transpose
                };
                let t = v.t;
                Ok(self.math(m, &[&v], t))
            }
            "transform" => {
                self.arity(name, &vals, &[2], pos)?;
                let mut vals = vals.into_iter();
                let (m, p) = (vals.next().expect("2"), vals.next().expect("2"));
                if m.t.ty != Ty::Mat4 || p.t.ty != Ty::Vec3 {
                    return self.err(pos, "transform takes a matrix and a point, vector or normal");
                }
                let t = p.t;
                Ok(match p.t.role {
                    Some(Role::Point) => self.math(Math::TransformPoint, &[&m, &p], t),
                    Some(Role::Normal) => {
                        let mat = T::of(Ty::Mat4).listed(m.t.list);
                        let inv = self.math(Math::Invert, &[&m], mat);
                        let it = self.math(Math::Transpose, &[&inv], mat);
                        let turned = self.math(Math::TransformVector, &[&it, &p], t);
                        self.math(Math::Normalize, &[&turned], t)
                    }
                    _ => self.math(Math::TransformVector, &[&m, &p], t),
                })
            }
            "rotate" => {
                self.arity(name, &vals, &[2], pos)?;
                let mut vals = vals.into_iter();
                let (q, v) = (vals.next().expect("2"), vals.next().expect("2"));
                if q.t.ty != Ty::Quat || v.t.ty != Ty::Vec3 {
                    return self.err(
                        pos,
                        "rotate takes a quat and a vector (OSL's angle-and-axis form is not yet)",
                    );
                }
                let t = v.t;
                Ok(self.math(Math::Rotate, &[&q, &v], t))
            }
            "arraylength" | "sum" | "average" | "len" => {
                if name == "len" {
                    return self.err(pos, "arraylength(a): OSL's name");
                }
                self.arity(name, &vals, &[1], pos)?;
                let v = vals.into_iter().next().expect("one");
                if !v.t.list {
                    return self.err(pos, format!("{name} takes a list (a[])"));
                }
                let (op, t) = match name {
                    "arraylength" => (ArrayOp::Size, T::of(Ty::Int)),
                    "sum" => (ArrayOp::Sum, T { list: false, ..v.t }),
                    _ => (ArrayOp::Average, T { list: false, ..v.t }),
                };
                let v = if name != "arraylength" && v.t.ty == Ty::Bool {
                    self.convert(v, T::of(Ty::Int).listed(true), pos)?
                } else {
                    v
                };
                let t = if name != "arraylength" && t.ty == Ty::Bool {
                    T::of(Ty::Int)
                } else {
                    t
                };
                Ok(self.made(Op::Array(op), vec![v.at], 0, t))
            }
            "random" => {
                self.arity(name, &vals, &[2, 3], pos)?;
                let mut vals = vals.into_iter();
                let (lo, hi) = (vals.next().expect("2"), vals.next().expect("2"));
                let (lo, hi) = self.unify(lo, hi, pos)?;
                let seed = match vals.next() {
                    Some(s) => self.to(s.clone(), Ty::Int, pos)?.at,
                    None => Input::Value(Value::Int(0)),
                };
                let t = lo.t;
                Ok(self.made(
                    Op::Random { seed: 0 },
                    vec![lo.at, hi.at, Input::Value(Value::Int(0)), seed],
                    0,
                    t,
                ))
            }
            "random_orientation" => {
                self.arity(name, &vals, &[1, 2], pos)?;
                let mut vals = vals.into_iter();
                let n = vals.next().expect("1");
                let n = self.to(n.clone(), Ty::Vec3, pos)?;
                let seed = match vals.next() {
                    Some(s) => self.convert(s, T::of(Ty::Int), pos)?.at,
                    None => Input::Value(Value::Int(0)),
                };
                let inputs = vec![
                    n.at,
                    Input::Value(Value::Vec3([0.0; 3])),
                    Input::Value(Value::Int(0)),
                    seed,
                ];
                Ok(self.made(Op::RandomOrientation { seed: 0 }, inputs, 0, T::of(Ty::Quat)))
            }
            "get_reference" => {
                let Some(Known::Int(input)) = vals.first().and_then(|v| v.known.clone()) else {
                    return self.err(
                        pos,
                        "get_reference(1, \"/path\"…): a reference input's number, then paths",
                    );
                };
                let mut paths = vec![];
                for v in &vals[1..] {
                    match &v.known {
                        Some(Known::Str(s)) => paths.push(s.clone()),
                        _ => return self.err(pos, "get_reference's paths are strings (\"/assets/rock*\")"),
                    }
                }
                let op = Op::GetReference {
                    input: input.max(0) as usize,
                    paths: (!paths.is_empty()).then_some(paths),
                };
                Ok(self.made(op, vec![], 0, T::of(Ty::Token).listed(true)))
            }
            "index" => {
                self.arity(name, &vals, &[0], pos)?;
                match self.context {
                    Context::Point => self.read_attr("intrinsic:ptIndex", pos),
                    Context::Face => self.read_attr("intrinsic:faceIndex", pos),
                    Context::Object => Ok(self.constant(Value::Int(0))),
                    Context::Corner => self.err(pos, "A corner's own number is not an intrinsic yet"),
                }
            }
            "range" => self.err(pos, "range(n) comes with arrays made in a snippet: not yet"),
            // OSL's names: "perlin" and "simplex" signed, "uperlin" (noise's
            // own) and "usimplex" from 0 to 1, "cell" one value per cube.
            "noise" | "snoise" | "cellnoise" => {
                let (kind, mut args) = match (name, vals.first().and_then(|v| v.known.clone())) {
                    ("noise", Some(Known::Str(k))) => (k, vals[1..].to_vec()),
                    ("noise", _) => ("uperlin".to_string(), vals),
                    ("snoise", _) => ("perlin".to_string(), vals),
                    _ => ("cell".to_string(), vals),
                };
                if args.len() != 1 {
                    return self.err(
                        pos,
                        format!("{name} of one point: noise(\"perlin\", p), snoise(p), cellnoise(p)"),
                    );
                }
                let p = args.remove(0);
                if p.t.ty != Ty::Vec3 {
                    return self.err(
                        pos,
                        format!(
                            "Noise of a point (a {} here): a 1D, 2D and 4D noise are not yet",
                            p.t.name()
                        ),
                    );
                }
                let (m, unsigned) = match kind.as_str() {
                    "perlin" | "snoise" => (Math::Perlin, false),
                    "uperlin" | "noise" => (Math::Perlin, true),
                    "simplex" => (Math::Simplex, false),
                    "usimplex" => (Math::Simplex, true),
                    "cell" | "cellnoise" => (Math::CellNoise, false),
                    other => {
                        return self.err(
                            pos,
                            format!(
                                "\"{other}\" noise: \"perlin\", \"uperlin\", \"simplex\", \"usimplex\" or \"cell\""
                            ),
                        )
                    }
                };
                let list = p.t.list;
                let n = self.math(m, &[&p], float.listed(list));
                if !unsigned {
                    return Ok(n);
                }
                // From 0 to 1: half of one more.
                let (one, half) = (self.constant(Value::Float(1.0)), self.constant(Value::Float(0.5)));
                let up = self.binary("+", n, one, pos)?;
                self.binary("*", up, half, pos)
            }
            _ => self.err(pos, format!("Unknown function {name}")),
        }
    }

    /// An attribute argument (`@x`): its name.
    fn attr_arg(&self, e: Option<&Expr>, what: &str, pos: Pos) -> Result<String, Error> {
        match e.map(|e| &e.kind) {
            Some(ExprKind::Attr(name)) => Ok(name.clone()),
            _ => self.err(pos, format!("{what}'s first argument is the attribute read: @name")),
        }
    }

    /// `get_data(@x, …)` and `get_array_data(@x, …)`, as the Get Data and Get
    /// Array Data nodes: in a context (`"point"`, a resample), at an index, at
    /// a face and Param UV, or element by element; from a reference (a
    /// `geometry`) when last.
    fn get_data(&mut self, array: bool, args: &[Expr], pos: Pos) -> Result<V, Error> {
        let what = if array { "get_array_data" } else { "get_data" };
        let name = self.attr_arg(args.first(), what, pos)?;
        let mut rest = args[1..]
            .iter()
            .map(|a| self.expr(a))
            .collect::<Result<Vec<V>, Error>>()?;
        // A reference, last; one with nothing in it (an unwired geometry) is
        // the graph's own.
        let geometry = match rest.last() {
            Some(v) if v.t.ty == Ty::Token && v.t.list => match rest.pop().map(|g| g.at) {
                Some(at @ Input::Wire { .. }) => Some(at),
                _ => None,
            },
            _ => None,
        };
        let declared = self.declared.get(&name).copied();
        let found = |l: &Self, c: Option<Context>| -> Option<T> {
            declared.or_else(|| {
                let geo = l.geo?;
                let cs = match c {
                    Some(c) => vec![c],
                    None => vec![Context::Point, Context::Face, Context::Corner, Context::Object],
                };
                cs.into_iter().find_map(|c| attribute_type(geo, c, &name)).map(T::of)
            })
        };
        let typed = |l: &Self, c: Option<Context>| -> Result<T, Error> {
            found(l, c).ok_or_else(|| {
                Error::at(
                    pos,
                    format!("No attribute @{name} to read: declare it (`vector @{name};`)"),
                )
            })
        };
        let geo_in = |mut v: Vec<Input>| {
            v.extend(geometry.clone());
            v
        };
        match rest.as_slice() {
            // In a context, made one (get_data) or kept (get_array_data).
            [c, more @ ..] if matches!(c.known, Some(Known::Str(_))) => {
                let Some(Known::Str(c)) = &c.known else { unreachable!() };
                let Some(context) = context_named(c) else {
                    return self.err(pos, format!("\"{c}\" is not a context: \"point\", \"face\", \"corner\" or \"object\""));
                };
                let resample = match more {
                    [] => Resample::Average,
                    [r] => match &r.known {
                        Some(Known::Str(r)) => match r.as_str() {
                            "average" => Resample::Average,
                            "sum" => Resample::Sum,
                            "min" => Resample::Min,
                            "max" => Resample::Max,
                            _ => return self.err(pos, format!("\"{r}\" is not a resample: \"average\", \"sum\", \"min\" or \"max\"")),
                        },
                        _ => return self.err(pos, "The resample is a string: \"average\", \"sum\", \"min\", \"max\""),
                    },
                    _ => return self.err(pos, format!("{what}(@{name}, \"{c}\"…) takes at most a resample after the context")),
                };
                let t = typed(self, Some(context))?;
                let inputs = geo_in(vec![]);
                Ok(if array {
                    let op = Op::GetArrayData { name, context: Some(context) };
                    self.made(op, inputs, 0, t.listed(true))
                } else {
                    let op = Op::GetData { name, context: Some(context), resample };
                    self.made(op, inputs, 0, t)
                })
            }
            // Element by element, from a reference of the same topology.
            [] => {
                if geometry.is_none() {
                    return self.read_attr(&name, pos);
                }
                let t = typed(self, Some(self.context))?;
                let inputs = geo_in(vec![]);
                let op = if array {
                    Op::GetArrayData { name, context: None }
                } else {
                    Op::get(&name)
                };
                Ok(self.made(op, inputs, 0, t.listed(array)))
            }
            // At an index (or a list of them).
            [i] if matches!(i.t.ty, Ty::Int | Ty::Bool) => {
                let i = self.to(i.clone(), Ty::Int, pos)?;
                let t = typed(self, None)?;
                let list = array || i.t.list;
                let inputs = geo_in(vec![i.at]);
                let op = if array {
                    Op::GetArrayDataAtIndex { name, context: None }
                } else {
                    Op::GetDataAtIndex { name, context: None }
                };
                Ok(self.made(op, inputs, 0, t.listed(list)))
            }
            // At a face and Param UV.
            [face, uv] => {
                let face = self.convert(face.clone(), T::of(Ty::Int), pos)?;
                let uv = self.convert(uv.clone(), T::of(Ty::Vec2), pos)?;
                let t = typed(self, None)?;
                let inputs = geo_in(vec![face.at, uv.at]);
                let op = if array {
                    Op::GetArrayDataAt { name, context: None }
                } else {
                    Op::GetDataAt { name, context: None }
                };
                Ok(self.made(op, inputs, 0, t.listed(array)))
            }
            _ => self.err(
                pos,
                format!("{what}(@{name}, \"point\"), (@{name}, index), (@{name}, face, uv), each with a geometry last if from a reference"),
            ),
        }
    }

    /// `near_points(origin, count, radius, [geometry,] points, distances)`.
    fn near_points(&mut self, args: &[Expr], pos: Pos) -> Result<V, Error> {
        let (ins, outs) = match args.len() {
            5 => (&args[..3], &args[3..]),
            6 => (&args[..4], &args[4..]),
            _ => return self.err(pos, "near_points(origin, count, radius, [geometry,] points, distances)"),
        };
        let mut vals = ins.iter().map(|a| self.expr(a)).collect::<Result<Vec<V>, Error>>()?;
        let geometry = if vals.len() == 4 {
            let g = vals.pop().expect("four");
            if !(g.t.ty == Ty::Token && g.t.list) {
                return self.err(pos, "near_points' fourth argument is a geometry (a reference)");
            }
            g.at
        } else {
            Input::Value(Value::Int(0))
        };
        let origin = self.convert(vals[0].clone(), T::of(Ty::Vec3), pos)?;
        let count = self.convert(vals[1].clone(), T::of(Ty::Int), pos)?;
        let radius = self.convert(vals[2].clone(), T::of(Ty::Float), pos)?;
        let node = self.node(Op::NearPoints, vec![geometry, origin.at, radius.at, count.at]);
        let points = self.value(
            Input::Wire {
                node,
                output: NEAR_POINTS,
            },
            T::of(Ty::Int).listed(true),
            None,
        );
        let dists = self.value(
            Input::Wire {
                node,
                output: NEAR_DISTANCES,
            },
            T::of(Ty::Float).listed(true),
            None,
        );
        self.assign_out(&outs[0], points.clone())?;
        self.assign_out(&outs[1], dists)?;
        Ok(self.made(Op::Array(ArrayOp::Size), vec![points.at], 0, T::of(Ty::Int)))
    }

    /// `closest_point_on_surface(origin, geometry, face, uv, position)`.
    fn closest(&mut self, args: &[Expr], pos: Pos) -> Result<V, Error> {
        if args.len() != 5 {
            return self.err(pos, "closest_point_on_surface(origin, geometry, face, uv, position)");
        }
        let origin = self.expr(&args[0])?;
        let origin = self.convert(origin, T::of(Ty::Vec3), pos)?;
        let geometry = self.expr(&args[1])?;
        if !(geometry.t.ty == Ty::Token && geometry.t.list && matches!(geometry.at, Input::Wire { .. })) {
            return self.err(
                args[1].pos,
                "The surface searched is a reference with prims in it (a geometry): the graph's own is not yet",
            );
        }
        let node = self.node(
            Op::ClosestPoint,
            vec![geometry.at.clone(), origin.at, Input::Value(Value::Float(0.0))],
        );
        let face = self.value(
            Input::Wire {
                node,
                output: CLOSEST_POLYGONS,
            },
            T::of(Ty::Int),
            None,
        );
        let uv = self.value(
            Input::Wire {
                node,
                output: CLOSEST_PARAM_UVS,
            },
            T::of(Ty::Vec2),
            None,
        );
        let point = T {
            role: Some(Role::Point),
            ..T::of(Ty::Vec3)
        };
        let at = self.made(
            Op::GetDataAt {
                name: "P".into(),
                context: None,
            },
            vec![face.at.clone(), uv.at.clone(), geometry.at],
            0,
            point,
        );
        self.assign_out(&args[2], face.clone())?;
        self.assign_out(&args[3], uv)?;
        self.assign_out(&args[4], at)?;
        let zero = self.constant(Value::Int(0));
        self.binary(">=", face, zero, pos)
    }

    fn assign_out(&mut self, target: &Expr, v: V) -> Result<(), Error> {
        if let ExprKind::Var(name) = &target.kind {
            if self.lookup(name).is_some_and(|old| old.t.list != v.t.list) {
                let old = self.lookup(name).expect("there").t;
                return self.err(
                    target.pos,
                    format!("{name} is a {}; this gives a {}", old.name(), v.t.name()),
                );
            }
        }
        self.assign(target, v)
    }
}

fn num(k: &Option<Known>) -> Option<f64> {
    match k {
        Some(Known::Int(i)) => Some(*i as f64),
        Some(Known::Float(f)) => Some(*f as f64),
        _ => None,
    }
}
