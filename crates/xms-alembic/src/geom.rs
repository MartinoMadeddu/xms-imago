//! The AbcGeom schemas: what an object is, read from its properties.
//!
//! Each schema's properties are in a compound of the object's properties:
//! `.xform` for a transform, `.geom` for meshes, curves, points and cameras,
//! `.faceset` for a face set. Each property keeps its own samples and times:
//! a mesh's points can move while its polygons stay, so they are read one by
//! one (`num_samples`, `time`, the value at a sample).
//!
//! | Schema | What it reads |
//! |---|---|
//! | `AbcGeom_Xform_v3` | its operations (scale, translate, rotate, matrix), their values, whether it inherits its parent's transform |
//! | `AbcGeom_PolyMesh_v1` | points, face counts and indices, normals, texture coordinates, velocities |
//! | `AbcGeom_SubD_v1` | as a mesh, and its scheme, creases, corners, holes and boundary rules |
//! | `AbcGeom_Curve_v2` | points, vertex counts, type, basis and wrap, widths, normals, weights, orders and knots |
//! | `AbcGeom_Points_v1` | points, ids, velocities, widths |
//! | `AbcGeom_Camera_v1` | lens and film back: focal length (mm), apertures and offsets (cm), clipping, f-stop, focus distance, shutter |
//! | `AbcGeom_FaceSet_v1` | the faces of its mesh it names |
//!
//! Every geometry also has its arbitrary parameters (`.arbGeomParams`: colours,
//! more texture coordinates, any per-point or per-face value) and user
//! properties (`.userProperties`).

use crate::archive::{Compound, Kind, Object, Property};
use crate::value::{chunked, Values};
use crate::Error;

/// A row-major 4×4 matrix, as Alembic and USD write them: points are rows,
/// the translation is the last row.
pub type Matrix = [[f64; 4]; 4];

pub const IDENTITY: Matrix = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// What an object is.
#[derive(Debug)]
pub enum Geom<'a> {
    Xform(Xform<'a>),
    PolyMesh(Mesh<'a>),
    SubD(Mesh<'a>),
    Curves(Curves<'a>),
    Points(Points<'a>),
    Camera(Camera<'a>),
    FaceSet(FaceSet<'a>),
    /// A schema not read yet (NuPatch, Light), by name.
    Other(String),
    /// No schema: a plain object (a group).
    None,
}

/// An object's schema, read.
pub fn geom<'a>(obj: &Object<'a>) -> Result<Geom<'a>, Error> {
    let Some(schema) = obj.schema().map(str::to_string) else {
        return Ok(Geom::None);
    };
    let props = obj.properties()?;
    let compound = |name: &str| -> Result<Compound<'a>, Error> {
        props
            .compound(name)?
            .ok_or_else(|| Error::Missing(format!("{}: no {name}", obj.path)))
    };
    let is = |prefix: &str| schema.starts_with(prefix);
    Ok(if is("AbcGeom_Xform") {
        Geom::Xform(Xform::read(compound(".xform")?)?)
    } else if is("AbcGeom_PolyMesh") {
        Geom::PolyMesh(Mesh::read(compound(".geom")?, &obj.path, false)?)
    } else if is("AbcGeom_SubD") {
        Geom::SubD(Mesh::read(compound(".geom")?, &obj.path, true)?)
    } else if is("AbcGeom_Curve") {
        Geom::Curves(Curves::read(compound(".geom")?, &obj.path)?)
    } else if is("AbcGeom_Points") {
        Geom::Points(Points::read(compound(".geom")?, &obj.path)?)
    } else if is("AbcGeom_Camera") {
        Geom::Camera(Camera::read(compound(".geom")?, &obj.path)?)
    } else if is("AbcGeom_FaceSet") {
        Geom::FaceSet(FaceSet::read(compound(".faceset")?, &obj.path)?)
    } else {
        Geom::Other(schema)
    })
}

/// Whether an object is shown: its own say, or its parent's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    /// As its parent.
    Deferred,
    Hidden,
    Visible,
}

/// An object's visibility property (`visible`), when it has one.
pub fn visibility<'a>(obj: &Object<'a>) -> Result<Option<Property<'a>>, Error> {
    obj.properties()?.property("visible")
}

/// A visibility property's value at sample `i`.
pub fn visibility_at(p: &Property, i: usize) -> Result<Visibility, Error> {
    Ok(match p.sample(i)?.to_i64().first() {
        Some(0) => Visibility::Hidden,
        Some(1) => Visibility::Visible,
        _ => Visibility::Deferred,
    })
}

/// A property that must be there.
fn needed<'a>(c: &Compound<'a>, name: &str, path: &str) -> Result<Property<'a>, Error> {
    c.property(name)?
        .ok_or_else(|| Error::Missing(format!("{path}: no {name}")))
}

/// The most samples of these properties, and the times of the one that has
/// them: when the schema changes.
fn sample_times(props: &[Option<&Property>]) -> Vec<f64> {
    props
        .iter()
        .flatten()
        .max_by_key(|p| p.num_samples())
        .map(|p| p.times())
        .unwrap_or_default()
}

// Geometry parameters

/// Where a parameter's values are: one for the object, one a face, a point,
/// a face's corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Constant,
    Uniform,
    Varying,
    Vertex,
    FaceVarying,
    Unknown,
}

impl Scope {
    fn of(text: Option<&str>) -> Scope {
        match text {
            Some("con") => Scope::Constant,
            Some("uni") => Scope::Uniform,
            Some("var") => Scope::Varying,
            Some("vtx") => Scope::Vertex,
            Some("fvr") => Scope::FaceVarying,
            _ => Scope::Unknown,
        }
    }
}

/// A geometry parameter (normals, texture coordinates, widths, a colour…):
/// its values, and their indices when it is indexed.
#[derive(Clone, Debug)]
pub struct GeomParam<'a> {
    pub name: String,
    pub scope: Scope,
    /// What its values are (`point`, `normal`, `vector`, `rgb`, `rgba`…), as
    /// it says.
    pub interpretation: String,
    /// Values an element has.
    pub extent: u8,
    pub values: Property<'a>,
    pub indices: Option<Property<'a>>,
}

/// A parameter's values and indices at one sample.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamSample {
    /// Flat: `extent` of them an element.
    pub values: Values,
    pub indices: Option<Vec<u32>>,
}

impl<'a> GeomParam<'a> {
    /// Parameter `name` of compound `c`: an array property, or a compound of
    /// `.vals` and `.indices`.
    pub fn read(c: &Compound<'a>, name: &str) -> Result<Option<GeomParam<'a>>, Error> {
        let Some(h) = c.header(name).cloned() else {
            return Ok(None);
        };
        let (values, indices) = match h.kind {
            Kind::Compound => {
                let inner = c.compound(name)?.ok_or_else(|| Error::Missing(name.to_string()))?;
                let Some(vals) = inner.property(".vals")? else {
                    return Ok(None);
                };
                (vals, inner.property(".indices")?)
            }
            _ => match c.property(name)? {
                Some(p) => (p, None),
                None => return Ok(None),
            },
        };
        let meta = |k: &str| {
            h.metadata
                .get(k)
                .or_else(|| values.header.metadata.get(k))
                .map(str::to_string)
        };
        Ok(Some(GeomParam {
            name: name.to_string(),
            scope: Scope::of(meta("geoScope").as_deref()),
            interpretation: meta("interpretation").unwrap_or_default(),
            extent: values.header.extent,
            values,
            indices,
        }))
    }

    pub fn num_samples(&self) -> usize {
        self.values
            .num_samples()
            .max(self.indices.as_ref().map_or(0, |i| i.num_samples()))
    }

    pub fn is_constant(&self) -> bool {
        self.values.is_constant() && self.indices.as_ref().is_none_or(|i| i.is_constant())
    }

    /// When sample `i` is, in seconds.
    pub fn time(&self, i: usize) -> f64 {
        match &self.indices {
            Some(ix) if ix.num_samples() > self.values.num_samples() => ix.time(i),
            _ => self.values.time(i),
        }
    }

    pub fn times(&self) -> Vec<f64> {
        (0..self.num_samples()).map(|i| self.time(i)).collect()
    }

    pub fn sample(&self, i: usize) -> Result<ParamSample, Error> {
        let indices = match &self.indices {
            Some(ix) => Some(ix.sample(i)?.to_i64().into_iter().map(|x| x as u32).collect()),
            None => None,
        };
        Ok(ParamSample {
            values: self.values.sample(i)?,
            indices,
        })
    }
}

/// The parameters of a compound (`.arbGeomParams`).
fn params<'a>(c: Option<Compound<'a>>) -> Result<Vec<GeomParam<'a>>, Error> {
    let Some(c) = c else { return Ok(vec![]) };
    let names: Vec<String> = c.headers().iter().map(|h| h.name.clone()).collect();
    let mut out = vec![];
    for n in names {
        if let Some(p) = GeomParam::read(&c, &n)? {
            out.push(p);
        }
    }
    Ok(out)
}

/// The scalar and array properties of a compound (`.userProperties`).
fn plain<'a>(c: Option<Compound<'a>>) -> Result<Vec<Property<'a>>, Error> {
    let Some(c) = c else { return Ok(vec![]) };
    let names: Vec<String> = c.headers().iter().map(|h| h.name.clone()).collect();
    let mut out = vec![];
    for n in names {
        if let Some(p) = c.property(&n)? {
            out.push(p);
        }
    }
    Ok(out)
}

/// What every geometry schema has besides its own: arbitrary parameters, user
/// properties, its bounds.
#[derive(Clone, Debug)]
pub struct Extra<'a> {
    pub arb: Vec<GeomParam<'a>>,
    pub user: Vec<Property<'a>>,
    /// `.selfBnds`: min and max corners, six doubles a sample.
    pub bounds: Option<Property<'a>>,
}

impl<'a> Extra<'a> {
    fn read(c: &Compound<'a>) -> Result<Extra<'a>, Error> {
        Ok(Extra {
            arb: params(c.compound(".arbGeomParams")?)?,
            user: plain(c.compound(".userProperties")?)?,
            bounds: c.property(".selfBnds")?,
        })
    }
}

// Transforms

/// A transform operation, with which of its values it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XformOp {
    Scale,
    Translate,
    /// About an axis, by degrees: axis x, y, z then the angle.
    Rotate,
    Matrix,
    RotateX,
    RotateY,
    RotateZ,
}

impl XformOp {
    fn of(code: u8) -> XformOp {
        match code >> 4 {
            0 => XformOp::Scale,
            1 => XformOp::Translate,
            2 => XformOp::Rotate,
            3 => XformOp::Matrix,
            4 => XformOp::RotateX,
            5 => XformOp::RotateY,
            _ => XformOp::RotateZ,
        }
    }

    /// How many values it takes.
    pub fn channels(self) -> usize {
        match self {
            XformOp::RotateX | XformOp::RotateY | XformOp::RotateZ => 1,
            XformOp::Scale | XformOp::Translate => 3,
            XformOp::Rotate => 4,
            XformOp::Matrix => 16,
        }
    }
}

/// An Xform: its operations in order (the first is outermost, as USD's
/// `xformOpOrder`), their values over time, and whether it inherits its
/// parent's transform.
#[derive(Clone, Debug)]
pub struct Xform<'a> {
    pub ops: Vec<XformOp>,
    /// The operations' values, all of them each sample.
    pub vals: Option<Property<'a>>,
    pub inherits: Option<Property<'a>>,
    pub arb: Vec<GeomParam<'a>>,
    pub user: Vec<Property<'a>>,
}

impl<'a> Xform<'a> {
    fn read(c: Compound<'a>) -> Result<Xform<'a>, Error> {
        let ops = match c.property(".ops")? {
            Some(p) if p.num_samples() > 0 => match p.sample(0)? {
                Values::U8(codes) => codes.into_iter().map(XformOp::of).collect(),
                _ => vec![],
            },
            _ => vec![],
        };
        Ok(Xform {
            ops,
            vals: c.property(".vals")?,
            inherits: c.property(".inherits")?,
            arb: params(c.compound(".arbGeomParams")?)?,
            user: plain(c.compound(".userProperties")?)?,
        })
    }

    pub fn num_samples(&self) -> usize {
        self.vals
            .as_ref()
            .map_or(0, |v| v.num_samples())
            .max(self.inherits.as_ref().map_or(0, |i| i.num_samples()))
            .max(1)
    }

    pub fn is_constant(&self) -> bool {
        self.vals.as_ref().is_none_or(|v| v.is_constant()) && self.inherits.as_ref().is_none_or(|i| i.is_constant())
    }

    pub fn times(&self) -> Vec<f64> {
        let t = sample_times(&[self.vals.as_ref(), self.inherits.as_ref()]);
        if t.is_empty() {
            vec![0.0]
        } else {
            t
        }
    }

    /// The operations' values at sample `i`.
    pub fn values_at(&self, i: usize) -> Result<Vec<f64>, Error> {
        match &self.vals {
            Some(v) => Ok(v.sample(i)?.to_f64()),
            None => Ok(vec![]),
        }
    }

    /// The transform at sample `i`, its operations put together.
    pub fn matrix_at(&self, i: usize) -> Result<Matrix, Error> {
        Ok(matrix_of(&self.ops, &self.values_at(i)?))
    }

    /// Whether it inherits its parent's transform at sample `i` (it does
    /// unless it says otherwise).
    pub fn inherits_at(&self, i: usize) -> Result<bool, Error> {
        match &self.inherits {
            Some(p) => Ok(p.sample(i)?.to_i64().first().is_none_or(|&b| b != 0)),
            None => Ok(true),
        }
    }
}

/// Row-major product, as Imath multiplies.
pub fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [[0.0; 4]; 4];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, x) in row.iter_mut().enumerate() {
            *x = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

/// A rotation of `degrees` about `axis`, for points as rows.
fn rotation(axis: [f64; 3], degrees: f64) -> Matrix {
    let len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if len == 0.0 {
        return IDENTITY;
    }
    let [x, y, z] = axis.map(|a| a / len);
    let (s, c) = degrees.to_radians().sin_cos();
    let t = 1.0 - c;
    [
        [x * x * t + c, x * y * t + z * s, x * z * t - y * s, 0.0],
        [x * y * t - z * s, y * y * t + c, y * z * t + x * s, 0.0],
        [x * z * t + y * s, y * z * t - x * s, z * z * t + c, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// The transform of `ops` with their `values` (all channels, in order): each
/// operation in turn, the first outermost.
pub fn matrix_of(ops: &[XformOp], values: &[f64]) -> Matrix {
    let mut out = IDENTITY;
    let mut at = 0;
    for &op in ops {
        let v = |k: usize| values.get(at + k).copied().unwrap_or(0.0);
        let m = match op {
            XformOp::Matrix => {
                let mut m = IDENTITY;
                for (j, row) in m.iter_mut().enumerate() {
                    for (k, x) in row.iter_mut().enumerate() {
                        *x = v(4 * j + k);
                    }
                }
                m
            }
            XformOp::RotateX => rotation([1.0, 0.0, 0.0], v(0)),
            XformOp::RotateY => rotation([0.0, 1.0, 0.0], v(0)),
            XformOp::RotateZ => rotation([0.0, 0.0, 1.0], v(0)),
            XformOp::Rotate => rotation([v(0), v(1), v(2)], v(3)),
            XformOp::Scale => {
                let mut m = IDENTITY;
                m[0][0] = v(0);
                m[1][1] = v(1);
                m[2][2] = v(2);
                m
            }
            XformOp::Translate => {
                let mut m = IDENTITY;
                m[3][0] = v(0);
                m[3][1] = v(1);
                m[3][2] = v(2);
                m
            }
        };
        at += op.channels();
        out = multiply(&m, &out);
    }
    out
}

// Meshes

/// A polygon mesh or a subdivision surface.
#[derive(Clone, Debug)]
pub struct Mesh<'a> {
    pub positions: Property<'a>,
    pub face_counts: Property<'a>,
    pub face_indices: Property<'a>,
    pub normals: Option<GeomParam<'a>>,
    pub uvs: Option<GeomParam<'a>>,
    pub velocities: Option<Property<'a>>,
    /// A subdivision surface's own: its scheme (`catmull-clark`, `loop`,
    /// `bilinear`), creases, corners and holes, and boundary rules.
    pub subd: Option<SubD<'a>>,
    pub extra: Extra<'a>,
}

/// What a subdivision surface has beyond a mesh.
#[derive(Clone, Debug)]
pub struct SubD<'a> {
    pub scheme: Option<Property<'a>>,
    pub crease_indices: Option<Property<'a>>,
    pub crease_lengths: Option<Property<'a>>,
    pub crease_sharpnesses: Option<Property<'a>>,
    pub corner_indices: Option<Property<'a>>,
    pub corner_sharpnesses: Option<Property<'a>>,
    pub holes: Option<Property<'a>>,
    pub interpolate_boundary: Option<Property<'a>>,
    pub face_varying_interpolate_boundary: Option<Property<'a>>,
    pub face_varying_propagate_corners: Option<Property<'a>>,
}

impl<'a> Mesh<'a> {
    /// `subd`: a subdivision surface's schema.
    fn read(c: Compound<'a>, path: &str, subd: bool) -> Result<Mesh<'a>, Error> {
        let subd = if subd {
            Some(SubD {
                scheme: c.property(".scheme")?,
                crease_indices: c.property(".creaseIndices")?,
                crease_lengths: c.property(".creaseLengths")?,
                crease_sharpnesses: c.property(".creaseSharpnesses")?,
                corner_indices: c.property(".cornerIndices")?,
                corner_sharpnesses: c.property(".cornerSharpnesses")?,
                holes: c.property(".holes")?,
                interpolate_boundary: c.property(".interpolateBoundary")?,
                face_varying_interpolate_boundary: c.property(".faceVaryingInterpolateBoundary")?,
                face_varying_propagate_corners: c.property(".faceVaryingPropagateCorners")?,
            })
        } else {
            None
        };
        Ok(Mesh {
            positions: needed(&c, "P", path)?,
            face_counts: needed(&c, ".faceCounts", path)?,
            face_indices: needed(&c, ".faceIndices", path)?,
            normals: GeomParam::read(&c, "N")?,
            uvs: GeomParam::read(&c, "uv")?,
            velocities: c.property(".velocities")?,
            subd,
            extra: Extra::read(&c)?,
        })
    }

    pub fn num_samples(&self) -> usize {
        self.positions
            .num_samples()
            .max(self.face_counts.num_samples())
            .max(self.face_indices.num_samples())
    }

    /// When the points or the polygons change.
    pub fn times(&self) -> Vec<f64> {
        sample_times(&[Some(&self.positions), Some(&self.face_counts), Some(&self.face_indices)])
    }

    pub fn positions_at(&self, i: usize) -> Result<Vec<[f32; 3]>, Error> {
        Ok(chunked(&self.positions.sample(i)?.to_f32()))
    }

    pub fn face_counts_at(&self, i: usize) -> Result<Vec<i32>, Error> {
        Ok(self.face_counts.sample(i)?.to_i32())
    }

    pub fn face_indices_at(&self, i: usize) -> Result<Vec<i32>, Error> {
        Ok(self.face_indices.sample(i)?.to_i32())
    }
}

// Curves

/// Curves' type: Alembic's cubic curves name their basis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveType {
    Cubic,
    Linear,
    /// Each curve its own order (`.orders`): NURBS curves.
    VariableOrder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveBasis {
    None,
    Bezier,
    Bspline,
    CatmullRom,
    Hermite,
    Power,
}

/// Curves' type, wrap and basis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CurveShape {
    pub kind: CurveType,
    pub periodic: bool,
    pub basis: CurveBasis,
}

/// Curves: their points, how many each curve has, their shape, widths,
/// normals and texture coordinates.
#[derive(Clone, Debug)]
pub struct Curves<'a> {
    pub positions: Property<'a>,
    pub counts: Property<'a>,
    /// Type, wrap and basis, four bytes a sample.
    pub shape: Property<'a>,
    pub widths: Option<GeomParam<'a>>,
    pub normals: Option<GeomParam<'a>>,
    pub uvs: Option<GeomParam<'a>>,
    /// Rational curves' point weights.
    pub weights: Option<Property<'a>>,
    pub orders: Option<Property<'a>>,
    pub knots: Option<Property<'a>>,
    pub velocities: Option<Property<'a>>,
    pub extra: Extra<'a>,
}

impl<'a> Curves<'a> {
    fn read(c: Compound<'a>, path: &str) -> Result<Curves<'a>, Error> {
        Ok(Curves {
            positions: needed(&c, "P", path)?,
            counts: needed(&c, "nVertices", path)?,
            shape: needed(&c, "curveBasisAndType", path)?,
            widths: GeomParam::read(&c, "width")?,
            normals: GeomParam::read(&c, "N")?,
            uvs: GeomParam::read(&c, "uv")?,
            weights: c.property("w")?,
            orders: c.property(".orders")?,
            knots: c.property(".knots")?,
            velocities: c.property(".velocities")?,
            extra: Extra::read(&c)?,
        })
    }

    pub fn num_samples(&self) -> usize {
        self.positions.num_samples().max(self.counts.num_samples())
    }

    pub fn times(&self) -> Vec<f64> {
        sample_times(&[Some(&self.positions), Some(&self.counts)])
    }

    pub fn positions_at(&self, i: usize) -> Result<Vec<[f32; 3]>, Error> {
        Ok(chunked(&self.positions.sample(i)?.to_f32()))
    }

    pub fn counts_at(&self, i: usize) -> Result<Vec<i32>, Error> {
        Ok(self.counts.sample(i)?.to_i32())
    }

    pub fn shape_at(&self, i: usize) -> Result<CurveShape, Error> {
        let b = self.shape.sample(i)?.to_i64();
        let at = |k: usize| b.get(k).copied().unwrap_or(0);
        Ok(CurveShape {
            kind: match at(0) {
                0 => CurveType::Cubic,
                1 => CurveType::Linear,
                _ => CurveType::VariableOrder,
            },
            periodic: at(1) == 1,
            basis: match at(2) {
                1 => CurveBasis::Bezier,
                2 => CurveBasis::Bspline,
                3 => CurveBasis::CatmullRom,
                4 => CurveBasis::Hermite,
                5 => CurveBasis::Power,
                _ => CurveBasis::None,
            },
        })
    }
}

// Points

/// Points: their positions, ids, velocities and widths.
#[derive(Clone, Debug)]
pub struct Points<'a> {
    pub positions: Property<'a>,
    pub ids: Option<Property<'a>>,
    pub velocities: Option<Property<'a>>,
    pub widths: Option<GeomParam<'a>>,
    pub extra: Extra<'a>,
}

impl<'a> Points<'a> {
    fn read(c: Compound<'a>, path: &str) -> Result<Points<'a>, Error> {
        Ok(Points {
            positions: needed(&c, "P", path)?,
            ids: c.property(".pointIds")?,
            velocities: c.property(".velocities")?,
            widths: GeomParam::read(&c, ".widths")?,
            extra: Extra::read(&c)?,
        })
    }

    pub fn times(&self) -> Vec<f64> {
        sample_times(&[Some(&self.positions)])
    }

    pub fn positions_at(&self, i: usize) -> Result<Vec<[f32; 3]>, Error> {
        Ok(chunked(&self.positions.sample(i)?.to_f32()))
    }
}

// Cameras

/// A camera's values at one sample, in Alembic's units: the focal length in
/// millimetres, apertures and offsets in centimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraSample {
    pub focal_length: f64,
    pub horizontal_aperture: f64,
    pub horizontal_offset: f64,
    pub vertical_aperture: f64,
    pub vertical_offset: f64,
    pub lens_squeeze_ratio: f64,
    /// Left, right, top, bottom.
    pub overscan: [f64; 4],
    pub f_stop: f64,
    pub focus_distance: f64,
    pub shutter_open: f64,
    pub shutter_close: f64,
    pub near: f64,
    pub far: f64,
}

/// A camera: its lens and film back over time.
#[derive(Clone, Debug)]
pub struct Camera<'a> {
    /// Sixteen doubles a sample.
    pub core: Property<'a>,
    pub extra: Extra<'a>,
}

impl<'a> Camera<'a> {
    fn read(c: Compound<'a>, path: &str) -> Result<Camera<'a>, Error> {
        Ok(Camera {
            core: needed(&c, ".core", path)?,
            extra: Extra::read(&c)?,
        })
    }

    pub fn times(&self) -> Vec<f64> {
        self.core.times()
    }

    pub fn sample_at(&self, i: usize) -> Result<CameraSample, Error> {
        let v = self.core.sample(i)?.to_f64();
        let at = |k: usize| v.get(k).copied().unwrap_or(0.0);
        Ok(CameraSample {
            focal_length: at(0),
            horizontal_aperture: at(1),
            horizontal_offset: at(2),
            vertical_aperture: at(3),
            vertical_offset: at(4),
            lens_squeeze_ratio: at(5),
            overscan: [at(6), at(7), at(8), at(9)],
            f_stop: at(10),
            focus_distance: at(11),
            shutter_open: at(12),
            shutter_close: at(13),
            near: at(14),
            far: at(15),
        })
    }
}

// Face sets

/// The faces of its parent mesh a face set names (a material's, a part's).
#[derive(Clone, Debug)]
pub struct FaceSet<'a> {
    pub faces: Property<'a>,
}

impl<'a> FaceSet<'a> {
    fn read(c: Compound<'a>, path: &str) -> Result<FaceSet<'a>, Error> {
        Ok(FaceSet {
            faces: needed(&c, ".faces", path)?,
        })
    }

    pub fn faces_at(&self, i: usize) -> Result<Vec<i32>, Error> {
        Ok(self.faces.sample(i)?.to_i32())
    }
}
