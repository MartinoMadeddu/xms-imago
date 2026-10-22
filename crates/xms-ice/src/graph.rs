//! The graph as the engine sees it. The editor's graph (positions, names,
//! selection) is turned into this before compiling.

use xms_geo::Context;

use super::types::{Ty, Value};

/// Nodes by position in `nodes`; wires point at a node's position and output.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub op:     Op,
    /// One per input of `op`, in order.
    pub inputs: Vec<Input>,
}

/// What an input takes: a wire from another node's output, or its own value.
#[derive(Clone, Debug)]
pub enum Input {
    Wire { node: usize, output: usize },
    Value(Value),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// The graph's end. Its inputs are Execute ports, run in order; it runs
    /// over every element of `context`.
    Root { context: Context },
    /// An attribute of the geometry, by name (`P` is the points). `context`:
    /// where the attribute is read from; None, the graph's own context (or
    /// Object). From another context, each element reads the elements it
    /// touches: a corner its point or its face, a face its corners or
    /// points, a point the faces or corners around it; where it touches
    /// several, `reduce` makes them one value.
    GetData { name: String, context: Option<Context>, reduce: Reduce },
    /// Set an attribute: input 0 is the value. Its output is an Execute,
    /// to wire into the root. `context`: where the attribute is written;
    /// None, the graph's own. Into another context, each element there
    /// takes the values of the graph's elements it touches (a face its
    /// points, a point the faces around it), made one by `reduce`.
    SetData { name: String, context: Option<Context>, reduce: Reduce },
    /// A value given when the graph runs (time, a promoted parameter);
    /// `default` when none is given. Changing it does not recompile.
    Parameter { name: String, default: Value },
    Constant(Value),
    Math(Math),
    /// A random value between input 0 (min) and input 1 (max), the same each
    /// time for the same seed and element. Input 2 (an int) is a count:
    /// 0 gives one value per element; more gives an array of that many (in
    /// a graph's one-value part: an Object graph, or from one-value inputs).
    /// Input 3, the Seed: unwired, each value is drawn for its element (its
    /// index in an array); wired, an int, one for all (every value moves)
    /// or one per element (`ptIndex`, an `id`, a face's index read from its
    /// points: elements of one seed get one value). `seed` is mixed in.
    Random { seed: u32 },
    /// The values of an attribute each element touches in `context` (its
    /// own when `None`), kept, not made one: a face's points' positions, its
    /// point numbers (`ptIndices`, Point `ptIndex`), a point's faces
    /// (`faceIndices`). A list per element, worked on by maths item by item
    /// and by the array nodes (size, an item, sum, average); in an Object
    /// graph the one list is an array.
    GetArrayData { name: String, context: Option<Context> },
    /// Arrays one after the other (single values count as one element),
    /// all of one type: every wired input, in order; the free one is not.
    Concat,
    /// What arrays give: their size, an element, their sum or average.
    Array(ArrayOp),
    /// A vector (or a rotation) from its parts: numbers, or a smaller
    /// vector and a number (see `Compose`). Per element, one value, or
    /// arrays element by element.
    Compose(Compose),
    /// A vector (or a rotation) taken apart (see `Decompose`): one output
    /// per part.
    Decompose(Decompose),
    /// Places on the mesh's surface, spread evenly by area: input 0 (an
    /// int) of them, the same for the same seed. `weight`: the name of a
    /// per-face or per-point float that scales how many land where (0 none,
    /// 2 twice as many); empty, none. In an Object graph. Three outputs,
    /// each an array: 0 the positions (USD's `point3f[]`), 1 the normals
    /// there (the mesh's, or each face's own), 2 the surface samples
    /// themselves (`Ty::Sample`), for Get Data at Samples.
    Scatter { seed: u32, weight: String },
    /// An attribute read at surface samples (input 0): a point's or a
    /// corner's interpolated across the triangle the sample is in, a
    /// face's that face's, the object's its one value. `P` is the points;
    /// `N` the normals, or the face's own when there are none.
    GetDataAt { name: String },
    /// Random rotations: each one's +Y turned along input 0 (a normal), then
    /// turned at random in its own frame up to input 1 (a vector of degrees)
    /// about each axis, either way; the same for the same seed. In any
    /// context, one per element (N per point, per face); an array as long
    /// as the normals when they are an array (Scatter's), or of input 2's
    /// count. Input 3, the Seed, as Random's. What a PointInstancer's
    /// `orientations` takes, and an `orient` attribute.
    RandomOrientation { seed: u32 },
    /// Paths on the operator's reference input number `input` (from 1): all
    /// of them, or (`paths`) those listed that are there, in the list's
    /// order. An array; none wired, none. (The app turns its rows, which
    /// may hold patterns, into this list.)
    GetReference { input: usize, paths: Option<Vec<String>> },
    /// Make a point instancer: an Execute, run from the root, in an Object
    /// graph. Its inputs are USD's PointInstancer's: 0 the positions (an
    /// array of 3D vectors), 1 the prototypes (an array of paths), 2 which
    /// prototype each takes (`protoIndices`: an int for all, or an array;
    /// -1, picked at random, the same for the same seed), 3 the
    /// orientations (a rotation for all, or an array), 4 the scales (a
    /// number or a 3D vector, for all or an array). `path`: the instancer
    /// prim's path; relative, beside the primitive the graph runs on.
    Instancer { path: String, seed: u32 },
}

/// What Compose makes, and from what (see `Op::Compose`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Compose {
    /// X and Y: a 2D vector.
    Vec2,
    /// X, Y and Z: a 3D vector.
    Vec3,
    /// X, Y, Z and W: a 4D vector.
    Vec4,
    /// A 2D vector and Z: a 3D vector.
    Vec2Z,
    /// A 3D vector and W: a 4D vector.
    Vec3W,
    /// I, J, K and the real part: a rotation (a quaternion, as USD's
    /// `quatf`), made unit.
    Quat,
}

impl Compose {
    pub const ALL: [Compose; 6] = [Compose::Vec2, Compose::Vec3, Compose::Vec4, Compose::Vec2Z, Compose::Vec3W, Compose::Quat];

    /// Its inputs: name and type.
    pub fn inputs(self) -> &'static [(&'static str, Ty)] {
        match self {
            Compose::Vec2 => &[("X", Ty::Float), ("Y", Ty::Float)],
            Compose::Vec3 => &[("X", Ty::Float), ("Y", Ty::Float), ("Z", Ty::Float)],
            Compose::Vec4 => &[("X", Ty::Float), ("Y", Ty::Float), ("Z", Ty::Float), ("W", Ty::Float)],
            Compose::Vec2Z => &[("XY", Ty::Vec2), ("Z", Ty::Float)],
            Compose::Vec3W => &[("XYZ", Ty::Vec3), ("W", Ty::Float)],
            Compose::Quat => &[("I", Ty::Float), ("J", Ty::Float), ("K", Ty::Float), ("Real", Ty::Float)],
        }
    }

    /// What it makes.
    pub fn output(self) -> Ty {
        match self {
            Compose::Vec2 => Ty::Vec2,
            Compose::Vec3 | Compose::Vec2Z => Ty::Vec3,
            Compose::Vec4 | Compose::Vec3W => Ty::Vec4,
            Compose::Quat => Ty::Quat,
        }
    }

    /// As its dropdown names it.
    pub fn label(self) -> &'static str {
        match self {
            Compose::Vec2 => "Floats to 2D Vector",
            Compose::Vec3 => "Floats to 3D Vector",
            Compose::Vec4 => "Floats to 4D Vector",
            Compose::Vec2Z => "2D Vector and Float to 3D Vector",
            Compose::Vec3W => "3D Vector and Float to 4D Vector",
            Compose::Quat => "Floats to Rotation (i, j, k, real)",
        }
    }
}

/// What Decompose takes apart, and into what (see `Op::Decompose`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Decompose {
    /// A 2D vector into X and Y.
    Vec2,
    /// A 3D vector into X, Y and Z.
    Vec3,
    /// A 4D vector into X, Y, Z and W.
    Vec4,
    /// A 3D vector into a 2D vector (XY) and Z.
    Vec3Z,
    /// A 4D vector into a 3D vector (XYZ) and W.
    Vec4W,
    /// A rotation into its I, J, K and real part.
    Quat,
    /// A rotation into its axis and its angle, in degrees.
    AxisAngle,
}

impl Decompose {
    pub const ALL: [Decompose; 7] = [Decompose::Vec2, Decompose::Vec3, Decompose::Vec4, Decompose::Vec3Z, Decompose::Vec4W, Decompose::Quat, Decompose::AxisAngle];

    /// What it takes.
    pub fn input(self) -> Ty {
        match self {
            Decompose::Vec2 => Ty::Vec2,
            Decompose::Vec3 | Decompose::Vec3Z => Ty::Vec3,
            Decompose::Vec4 | Decompose::Vec4W => Ty::Vec4,
            Decompose::Quat | Decompose::AxisAngle => Ty::Quat,
        }
    }

    /// Its outputs: name and type, in order.
    pub fn outputs(self) -> &'static [(&'static str, Ty)] {
        match self {
            Decompose::Vec2 => &[("X", Ty::Float), ("Y", Ty::Float)],
            Decompose::Vec3 => &[("X", Ty::Float), ("Y", Ty::Float), ("Z", Ty::Float)],
            Decompose::Vec4 => &[("X", Ty::Float), ("Y", Ty::Float), ("Z", Ty::Float), ("W", Ty::Float)],
            Decompose::Vec3Z => &[("XY", Ty::Vec2), ("Z", Ty::Float)],
            Decompose::Vec4W => &[("XYZ", Ty::Vec3), ("W", Ty::Float)],
            Decompose::Quat => &[("I", Ty::Float), ("J", Ty::Float), ("K", Ty::Float), ("Real", Ty::Float)],
            Decompose::AxisAngle => &[("Axis", Ty::Vec3), ("Angle", Ty::Float)],
        }
    }

    /// As its dropdown names it.
    pub fn label(self) -> &'static str {
        match self {
            Decompose::Vec2 => "2D Vector to Floats",
            Decompose::Vec3 => "3D Vector to Floats",
            Decompose::Vec4 => "4D Vector to Floats",
            Decompose::Vec3Z => "3D Vector to 2D Vector and Float",
            Decompose::Vec4W => "4D Vector to 3D Vector and Float",
            Decompose::Quat => "Rotation to Floats (i, j, k, real)",
            Decompose::AxisAngle => "Rotation to Axis and Angle",
        }
    }
}

/// Operations on an array (see `Op::Array`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ArrayOp {
    /// How many it holds.
    Size,
    /// The element at input 1 (clamped to the array; zero when empty).
    Element,
    Sum,
    Average,
}

impl ArrayOp {
    pub const ALL: [ArrayOp; 4] = [ArrayOp::Size, ArrayOp::Element, ArrayOp::Sum, ArrayOp::Average];
    pub fn arity(self) -> usize { if self == ArrayOp::Element { 2 } else { 1 } }
}

/// How the values an element touches in another context become one: a
/// face reading its points' positions gets their average, by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Reduce {
    #[default]
    Average,
    Sum,
    Min,
    Max,
}

impl Reduce {
    pub const ALL: [Reduce; 4] = [Reduce::Average, Reduce::Sum, Reduce::Min, Reduce::Max];
}

/// Maths, on any type it makes sense for: `Add` adds floats or vectors,
/// `Multiply` also scales a vector by a float.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Math {
    Add,
    Subtract,
    Multiply,
    Divide,
    Min,
    Max,
    /// a, b, t.
    Lerp,
    Negate,
    Abs,
    Sin,
    Cos,
    Dot,
    Cross,
    Length,
    Normalize,
    /// A vector (input 1) turned by a rotation (input 0).
    Rotate,
    /// A rotation about an axis (input 0) by an angle in degrees (input 1).
    AxisAngle,
    /// A rotation from Euler angles in degrees (X, then Y, then Z).
    Euler,
}

impl Math {
    /// Every operation, in the order the add menu lists them.
    pub const ALL: [Math; 18] = [
        Math::Add, Math::Subtract, Math::Multiply, Math::Divide, Math::Min, Math::Max, Math::Lerp,
        Math::Negate, Math::Abs, Math::Sin, Math::Cos,
        Math::Dot, Math::Cross, Math::Length, Math::Normalize,
        Math::Rotate, Math::AxisAngle, Math::Euler,
    ];

    pub fn arity(self) -> usize {
        match self {
            Math::Lerp => 3,
            Math::Negate | Math::Abs | Math::Sin | Math::Cos | Math::Length | Math::Normalize | Math::Euler => 1,
            _ => 2,
        }
    }
}

impl Op {
    /// How many inputs a node of this kind has. The root has as many as
    /// its node lists.
    pub fn arity(&self) -> Option<usize> {
        match self {
            Op::Root { .. } => None,
            Op::GetData { .. } | Op::Parameter { .. } | Op::Constant(_) => Some(0),
            Op::SetData { .. } => Some(1),
            Op::Math(m) => Some(m.arity()),
            Op::Random { .. } => Some(4),
            // As many as it has.
            Op::Concat => None,
            Op::Array(a) => Some(a.arity()),
            Op::Compose(c) => Some(c.inputs().len()),
            Op::Decompose(_) => Some(1),
            Op::Scatter { .. } | Op::GetDataAt { .. } => Some(1),
            Op::RandomOrientation { .. } => Some(4),
            Op::GetReference { .. } | Op::GetArrayData { .. } => Some(0),
            Op::Instancer { .. } => Some(5),
        }
    }
}

/// Scatter on Surface's outputs (see `Op::Scatter`).
pub const SCATTER_POSITIONS: usize = 0;
pub const SCATTER_NORMALS: usize = 1;
pub const SCATTER_SAMPLES: usize = 2;

impl Op {
    /// Get Data of an attribute in the graph's own context.
    pub fn get(name: &str) -> Op { Op::GetData { name: name.into(), context: None, reduce: Reduce::Average } }

    /// Set Data of an attribute in the graph's own context.
    pub fn set(name: &str) -> Op { Op::SetData { name: name.into(), context: None, reduce: Reduce::Average } }
}

impl Graph {
    pub fn add(&mut self, op: Op, inputs: Vec<Input>) -> usize {
        self.nodes.push(Node { op, inputs });
        self.nodes.len() - 1
    }
}

/// A wire from output 0 of `node`.
pub fn wire(node: usize) -> Input { Input::Wire { node, output: 0 } }
