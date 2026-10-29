//! Alembic (`.abc`) as a USD layer format, read with `xms-alembic` and made
//! into USD as OpenUSD's `usdAbc` makes it, so an `.abc` opens, sublayers and
//! references as any other layer.
//!
//! - **Objects** are prims, their names made valid USD names. A transform with
//!   one shape below it is one prim: the shape's, with the transform's ops.
//! - **Transforms** are one `xformOp:transform` matrix; one that does not
//!   inherit its parent's starts with `!resetXformStack!`.
//! - **Shapes:** a PolyMesh or SubD is a `Mesh` (`orientation` left-handed, as
//!   Alembic winds polygons), curves `BasisCurves`, `NurbsCurves` or
//!   `HermiteCurves`, points `Points`, a camera a `Camera` (apertures from
//!   centimetres to millimetres), a face set a `GeomSubset`.
//! - **Parameters:** `uv` is `primvars:st`, kept indexed; normals and widths
//!   are written expanded; `.arbGeomParams` are `primvars:*` and
//!   `.userProperties` `userProperties:*`.
//! - **Instances:** an instance's source is written once, as an `over` at the
//!   root, instanceable; the source and its instances reference it.
//! - **Time:** seconds are time codes at the frames per second the file says
//!   (`_ai_DCC_FPS`, else 24); what does not change is a default value.
//!
//! The whole file is read when the layer opens.

use std::collections::{HashMap, HashSet};

use openusd::sdf::{self, FileFormat, FileFormatCaps, FormatError, LayerData, LayerRegistry, SpecType};
use openusd::vt::Value;
use openusd::{ar, gf, tf};
use xms_alembic as abc;

type R<T> = Result<T, String>;

/// The Alembic layer format.
pub struct Alembic;

static ALEMBIC: Alembic = Alembic;

/// Make `openusd` open `.abc` files; once is enough, more do nothing.
pub fn register_formats() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        LayerRegistry::register_format(&ALEMBIC);
    });
}

impl FileFormat for Alembic {
    fn format_id(&self) -> tf::Token {
        "abc".into()
    }

    fn extensions(&self) -> &[&str] {
        &["abc"]
    }

    fn caps(&self) -> FileFormatCaps {
        FileFormatCaps::READ
    }

    fn matches_content(&self, prefix: &[u8]) -> bool {
        prefix.starts_with(b"Ogawa")
    }

    fn read_bytes(&self, bytes: ar::AssetBuffer, source_name: &str) -> Result<LayerData, FormatError> {
        let bytes = match bytes {
            ar::AssetBuffer::Owned(b) => b,
            shared => {
                let b: &[u8] = &shared;
                b.to_vec()
            }
        };
        let fail = |e: String| FormatError::Decode(format!("{source_name}: {e}").into());
        let archive = abc::Archive::from_bytes(bytes).map_err(|e| fail(e.to_string()))?;
        Ok(Box::new(layer(&archive).map_err(fail)?))
    }

    fn write(&self, _data: &dyn sdf::AbstractData, _sink: &mut dyn sdf::WriteSeek) -> Result<(), FormatError> {
        Err(FormatError::Encode {
            reason: "Alembic files are read, not written".into(),
        })
    }
}

/// An archive as a USD layer.
pub fn layer(archive: &abc::Archive) -> R<sdf::Data> {
    let fps = archive
        .metadata()
        .get("_ai_DCC_FPS")
        .and_then(|f| f.trim().parse::<f64>().ok())
        .filter(|f| *f > 0.0)
        .unwrap_or(24.0);
    let mut c = Convert {
        archive,
        data: sdf::Data::new(),
        fps,
        range: None,
        protos: HashMap::new(),
    };
    let root = archive.root().map_err(err)?;
    let mut used = HashSet::new();
    let tops: Vec<(abc::Object, String)> = root
        .children()
        .map_err(err)?
        .into_iter()
        .map(|o| {
            let n = clean(&o.name, &mut used);
            (o, n)
        })
        .collect();

    // Instance sources, each to be written once at the root.
    let mut sources = vec![];
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if let Some(s) = o.instance_source().map_err(err)? {
            sources.push(s);
        } else {
            stack.extend(o.children().map_err(err)?);
        }
    }
    sources.sort();
    sources.dedup();
    for s in sources {
        let name = s.rsplit('/').next().unwrap_or("prototype").to_string();
        let path = child_path(&sdf::Path::abs_root(), &clean(&name, &mut used))?;
        c.protos.insert(s, Proto { path, ty: None });
    }

    let mut names = vec![];
    for (o, n) in &tops {
        c.child(o, &sdf::Path::abs_root(), n)?;
        names.push(n.clone());
    }
    let mut protos: Vec<&Proto> = c.protos.values().filter(|p| p.ty.is_some()).collect();
    protos.sort_by(|a, b| a.path.as_str().cmp(b.path.as_str()));
    let proto_names: Vec<String> = protos
        .iter()
        .filter_map(|p| p.path.name().map(str::to_string))
        .collect();

    let s = c.data.create_spec(sdf::Path::abs_root(), SpecType::PseudoRoot);
    if let Some(first) = names.first() {
        s.add("defaultPrim", Value::Token(first.as_str().into()));
    }
    s.add("upAxis", Value::Token("Y".into()));
    s.add("timeCodesPerSecond", Value::Double(fps));
    s.add("framesPerSecond", Value::Double(fps));
    if let Some((a, b)) = c.range {
        s.add("startTimeCode", Value::Double(a));
        s.add("endTimeCode", Value::Double(b));
    }
    names.extend(proto_names);
    if !names.is_empty() {
        s.add("primChildren", Value::token_vec(names));
    }
    Ok(c.data)
}

fn err(e: abc::Error) -> String {
    e.to_string()
}

/// A prototype: where it is, and its type once written.
struct Proto {
    path: sdf::Path,
    ty: Option<String>,
}

struct Convert<'a> {
    archive: &'a abc::Archive,
    data: sdf::Data,
    fps: f64,
    /// The first and last time code of what changes.
    range: Option<(f64, f64)>,
    /// By the Alembic path of their source.
    protos: HashMap<String, Proto>,
}

/// An attribute's value: one, or one a time code.
enum Sampled {
    Default(Value),
    Samples(Vec<(f64, Value)>),
}

/// An attribute to write.
struct Attr {
    ty: String,
    uniform: bool,
    custom: bool,
    interpolation: Option<&'static str>,
    value: Sampled,
}

/// A prim's attributes, by name, in order.
#[derive(Default)]
struct Props(Vec<(String, Attr)>);

impl Props {
    fn set(&mut self, name: impl Into<String>, a: Attr) {
        let name = name.into();
        match self.0.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = a,
            None => self.0.push((name, a)),
        }
    }

    /// A varying attribute of type `ty`, when there is a value.
    fn put(&mut self, name: &str, ty: &str, value: Option<Sampled>) {
        if let Some(value) = value {
            self.set(name, attr(ty, value));
        }
    }

    /// A uniform token.
    fn token(&mut self, name: &str, t: &str) {
        self.set(
            name,
            Attr {
                uniform: true,
                ..attr("token", Sampled::Default(Value::Token(t.into())))
            },
        );
    }
}

fn attr(ty: &str, value: Sampled) -> Attr {
    Attr {
        ty: ty.to_string(),
        uniform: false,
        custom: false,
        interpolation: None,
        value,
    }
}

impl<'a> Convert<'a> {
    /// The time code of `t` seconds, noted in the range.
    fn code(&mut self, t: f64) -> f64 {
        const P: f64 = 1.0e10;
        let c = (P * t * self.fps).round() / P;
        self.range = Some(match self.range {
            Some((a, b)) => (a.min(c), b.max(c)),
            None => (c, c),
        });
        c
    }

    /// `n` samples of a value, one at each `time`: a default when there is one
    /// or they are all the same. `None` when there are none, or one fails to
    /// convert.
    fn sampled(
        &mut self,
        n: usize,
        constant: bool,
        time: impl Fn(usize) -> f64,
        mut value: impl FnMut(usize) -> R<Option<Value>>,
    ) -> R<Option<Sampled>> {
        if n == 0 {
            return Ok(None);
        }
        if n == 1 || constant {
            return Ok(value(0)?.map(Sampled::Default));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let Some(v) = value(i)? else { return Ok(None) };
            out.push((self.code(time(i)), v));
        }
        Ok(Some(Sampled::Samples(out)))
    }

    /// A property's samples, each converted by `f`.
    fn prop(&mut self, p: &abc::Property, f: impl Fn(abc::Values) -> Option<Value>) -> R<Option<Sampled>> {
        self.sampled(
            p.num_samples(),
            p.is_constant(),
            |i| p.time(i),
            |i| Ok(f(p.sample(i).map_err(err)?)),
        )
    }

    fn opt(&mut self, p: &Option<abc::Property>, f: impl Fn(abc::Values) -> Option<Value>) -> R<Option<Sampled>> {
        match p {
            Some(p) => self.prop(p, f),
            None => Ok(None),
        }
    }

    /// A parameter's samples, its indices applied, each converted by `f`.
    fn expanded(&mut self, g: &Option<abc::GeomParam>, f: impl Fn(abc::Values) -> Option<Value>) -> R<Option<Sampled>> {
        let Some(g) = g else { return Ok(None) };
        let extent = g.extent.max(1) as usize;
        self.sampled(
            g.num_samples(),
            g.is_constant(),
            |i| g.time(i),
            |i| {
                let s = g.sample(i).map_err(err)?;
                Ok(f(match s.indices {
                    Some(ix) => {
                        let v = s.values.to_f32();
                        let mut out = Vec::with_capacity(ix.len() * extent);
                        for k in ix {
                            let at = k as usize * extent;
                            out.extend(v.get(at..at + extent).ok_or("an index past its values")?);
                        }
                        abc::Values::F32(out)
                    }
                    None => s.values,
                }))
            },
        )
    }

    /// A parameter as `name` (and `name:indices` when it is indexed): `ty`
    /// and `role` say how its values are written, else they are taken from
    /// what it is.
    fn param(&mut self, props: &mut Props, name: &str, g: &abc::GeomParam, role: Option<&str>) -> R<()> {
        let extent = g.extent.max(1) as usize;
        let array = g.values.header.kind == abc::Kind::Array;
        let role = role.unwrap_or(&g.interpretation).to_string();
        let mut ty = None;
        let values = self.sampled(
            g.num_samples(),
            g.is_constant(),
            |i| g.time(i),
            |i| {
                let v = g.sample(i).map_err(err)?.values;
                let v = if role == "texCoord" {
                    abc::Values::F32(v.to_f32())
                } else {
                    v
                };
                Ok(convert(&v, extent, &role, array).map(|(t, v)| {
                    ty = Some(t);
                    v
                }))
            },
        )?;
        let (Some(values), Some(ty)) = (values, ty) else {
            return Ok(());
        };
        let interpolation = interpolation(g.scope);
        props.set(
            name,
            Attr {
                interpolation,
                ..attr(&ty, values)
            },
        );
        if g.indices.is_some() {
            let ix = self.sampled(
                g.num_samples(),
                g.is_constant(),
                |i| g.time(i),
                |i| {
                    let s = g.sample(i).map_err(err)?;
                    Ok(s.indices
                        .map(|ix| Value::IntVec(ix.into_iter().map(|k| k as i32).collect())))
                },
            )?;
            props.put(&format!("{name}:indices"), "int[]", ix);
        }
        Ok(())
    }

    /// Arbitrary parameters as primvars, user properties as such.
    fn extra(&mut self, props: &mut Props, arb: &[abc::GeomParam], user: &[abc::Property]) -> R<()> {
        for g in arb {
            self.param(props, &format!("primvars:{}", ident(&g.name)), g, None)?;
        }
        self.plain(props, "userProperties:", true, user)
    }

    /// Properties as attributes of their names after `prefix`, their types
    /// what their values are.
    fn plain(&mut self, props: &mut Props, prefix: &str, custom: bool, list: &[abc::Property]) -> R<()> {
        for p in list {
            if p.header.kind == abc::Kind::Compound {
                continue;
            }
            let extent = p.header.extent.max(1) as usize;
            let role = p.header.metadata.get("interpretation").unwrap_or("").to_string();
            let array = p.header.kind == abc::Kind::Array;
            let mut ty = None;
            let v = self.sampled(
                p.num_samples(),
                p.is_constant(),
                |i| p.time(i),
                |i| {
                    Ok(convert(&p.sample(i).map_err(err)?, extent, &role, array).map(|(t, v)| {
                        ty = Some(t);
                        v
                    }))
                },
            )?;
            if let (Some(v), Some(ty)) = (v, ty) {
                props.set(
                    format!("{prefix}{}", ident(&p.header.name)),
                    Attr { custom, ..attr(&ty, v) },
                );
            }
        }
        Ok(())
    }

    /// Visibility, from the object's `visible`.
    fn visibility(&mut self, obj: &abc::Object, props: &mut Props) -> R<()> {
        let Some(p) = abc::visibility(obj).map_err(err)? else {
            return Ok(());
        };
        let v = self.sampled(
            p.num_samples(),
            p.is_constant(),
            |i| p.time(i),
            |i| {
                let hidden = abc::visibility_at(&p, i).map_err(err)? == abc::Visibility::Hidden;
                Ok(Some(Value::Token(
                    if hidden { "invisible" } else { "inherited" }.into(),
                )))
            },
        )?;
        props.put("visibility", "token", v);
        Ok(())
    }

    /// What every shape has: its bounds, parameters and user properties.
    fn shape_extra(&mut self, props: &mut Props, e: &abc::Extra) -> R<()> {
        let extent = self.opt(&e.bounds, |v| {
            let b = v.to_f64();
            (b.len() >= 6).then(|| {
                Value::Vec3fVec(vec![
                    [b[0] as f32, b[1] as f32, b[2] as f32].into(),
                    [b[3] as f32, b[4] as f32, b[5] as f32].into(),
                ])
            })
        })?;
        props.put("extent", "float3[]", extent);
        self.extra(props, &e.arb, &e.user)
    }

    /// One object's type and attributes, its children aside.
    fn shape(&mut self, obj: &abc::Object, g: &abc::Geom, props: &mut Props) -> R<String> {
        self.visibility(obj, props)?;
        Ok(match g {
            abc::Geom::Xform(x) => {
                if x.vals.is_some() {
                    let times = x.times();
                    let m = self.sampled(
                        x.num_samples(),
                        x.is_constant(),
                        |i| times[i.min(times.len() - 1)],
                        |i| Ok(Some(Value::Matrix4d(gf::Mat4d::from(x.matrix_at(i).map_err(err)?)))),
                    )?;
                    props.put("xformOp:transform", "matrix4d", m);
                    let mut order = vec!["xformOp:transform"];
                    if !x.inherits_at(0).map_err(err)? {
                        order.insert(0, "!resetXformStack!");
                    }
                    props.set(
                        "xformOpOrder",
                        Attr {
                            uniform: true,
                            ..attr("token[]", Sampled::Default(Value::token_vec(order)))
                        },
                    );
                }
                self.extra(props, &x.arb, &x.user)?;
                "Xform".to_string()
            }
            abc::Geom::PolyMesh(m) | abc::Geom::SubD(m) => {
                let p = self.prop(&m.positions, |v| points(&v, "point"))?;
                props.put("points", "point3f[]", p);
                let v = self.opt(&m.velocities, |v| points(&v, "vector"))?;
                props.put("velocities", "vector3f[]", v);
                let n = self.expanded(&m.normals, |v| points(&v, "normal"))?;
                if let Some(n) = n {
                    props.set(
                        "normals",
                        Attr {
                            interpolation: m.normals.as_ref().and_then(|g| interpolation(g.scope)),
                            ..attr("normal3f[]", n)
                        },
                    );
                }
                let i = self.prop(&m.face_indices, ints)?;
                props.put("faceVertexIndices", "int[]", i);
                let c = self.prop(&m.face_counts, ints)?;
                props.put("faceVertexCounts", "int[]", c);
                if let Some(uv) = &m.uvs {
                    self.param(props, "primvars:st", uv, Some("texCoord"))?;
                }
                props.token("orientation", "leftHanded");
                match &m.subd {
                    None => props.token("subdivisionScheme", "none"),
                    Some(s) => self.subd(props, s)?,
                }
                self.shape_extra(props, &m.extra)?;
                "Mesh".to_string()
            }
            abc::Geom::Curves(c) => self.curves(props, c)?.to_string(),
            abc::Geom::Points(p) => {
                let pos = self.prop(&p.positions, |v| points(&v, "point"))?;
                props.put("points", "point3f[]", pos);
                let v = self.opt(&p.velocities, |v| points(&v, "vector"))?;
                props.put("velocities", "vector3f[]", v);
                self.widths(props, &p.widths)?;
                let ids = self.opt(&p.ids, |v| Some(Value::Int64Vec(v.to_i64())))?;
                props.put("ids", "int64[]", ids);
                self.shape_extra(props, &p.extra)?;
                "Points".to_string()
            }
            abc::Geom::Camera(cam) => {
                type Lens = fn(&abc::CameraSample) -> f64;
                let lenses: [(&str, Lens); 5] = [
                    ("focalLength", |s| s.focal_length),
                    ("horizontalAperture", |s| {
                        s.horizontal_aperture * s.lens_squeeze_ratio * 10.0
                    }),
                    ("verticalAperture", |s| {
                        s.vertical_aperture * s.lens_squeeze_ratio * 10.0
                    }),
                    ("horizontalApertureOffset", |s| {
                        s.horizontal_offset * s.lens_squeeze_ratio * 10.0
                    }),
                    ("verticalApertureOffset", |s| {
                        s.vertical_offset * s.lens_squeeze_ratio * 10.0
                    }),
                ];
                let p = &cam.core;
                for (name, lens) in lenses {
                    let v = self.sampled(
                        p.num_samples(),
                        p.is_constant(),
                        |i| p.time(i),
                        |i| Ok(Some(Value::Float(lens(&cam.sample_at(i).map_err(err)?) as f32))),
                    )?;
                    props.put(name, "float", v);
                }
                let clip = self.sampled(
                    p.num_samples(),
                    p.is_constant(),
                    |i| p.time(i),
                    |i| {
                        let s = cam.sample_at(i).map_err(err)?;
                        Ok(Some(Value::Vec2f([s.near as f32, s.far as f32].into())))
                    },
                )?;
                props.put("clippingRange", "float2", clip);
                self.shape_extra(props, &cam.extra)?;
                "Camera".to_string()
            }
            abc::Geom::FaceSet(f) => {
                let i = self.prop(&f.faces, ints)?;
                props.put("indices", "int[]", i);
                props.token("elementType", "face");
                props.token("familyName", "materialBind");
                "GeomSubset".to_string()
            }
            // An object of no schema read here: its properties as they are, its
            // type the one USD gave it when USD wrote it.
            abc::Geom::Other(_) | abc::Geom::None => {
                let c = obj.properties().map_err(err)?;
                let names: Vec<String> = c
                    .headers()
                    .iter()
                    .filter(|h| h.kind != abc::Kind::Compound && h.name != "visible")
                    .map(|h| h.name.clone())
                    .collect();
                let mut loose = vec![];
                for n in names {
                    loose.extend(c.property(&n).map_err(err)?);
                }
                self.plain(props, "", false, &loose)?;
                obj.metadata.get("Usd:typeName").unwrap_or("Scope").to_string()
            }
        })
    }

    fn subd(&mut self, props: &mut Props, s: &abc::SubD) -> R<()> {
        let scheme = match &s.scheme {
            Some(p) => p.sample(0).map_err(err)?.strings().first().cloned().unwrap_or_default(),
            None => String::new(),
        };
        let scheme = match scheme.as_str() {
            "loop" => "loop",
            "bilinear" => "bilinear",
            _ => "catmullClark",
        };
        props.token("subdivisionScheme", scheme);
        let boundary = self.opt(&s.interpolate_boundary, |v| {
            let t = match v.to_i64().first()? {
                0 => "none",
                2 => "edgeOnly",
                _ => "edgeAndCorner",
            };
            Some(Value::Token(t.into()))
        })?;
        props.put("interpolateBoundary", "token", boundary);
        let fv = self.opt(&s.face_varying_interpolate_boundary, |v| {
            let t = match v.to_i64().first()? {
                0 => "all",
                2 => "none",
                3 => "boundaries",
                _ => "cornersPlus1",
            };
            Some(Value::Token(t.into()))
        })?;
        props.put("faceVaryingLinearInterpolation", "token", fv);
        for (name, p) in [
            ("holeIndices", &s.holes),
            ("cornerIndices", &s.corner_indices),
            ("creaseIndices", &s.crease_indices),
            ("creaseLengths", &s.crease_lengths),
        ] {
            let v = self.opt(p, ints)?;
            props.put(name, "int[]", v);
        }
        for (name, p) in [
            ("cornerSharpnesses", &s.corner_sharpnesses),
            ("creaseSharpnesses", &s.crease_sharpnesses),
        ] {
            let v = self.opt(p, |v| Some(Value::FloatVec(v.to_f32())))?;
            props.put(name, "float[]", v);
        }
        Ok(())
    }

    fn widths(&mut self, props: &mut Props, w: &Option<abc::GeomParam>) -> R<()> {
        if let Some(v) = self.expanded(w, |v| Some(Value::FloatVec(v.to_f32())))? {
            props.set(
                "widths",
                Attr {
                    interpolation: w.as_ref().and_then(|g| interpolation(g.scope)),
                    ..attr("float[]", v)
                },
            );
        }
        Ok(())
    }

    fn curves(&mut self, props: &mut Props, c: &abc::Curves) -> R<&'static str> {
        let shape = c.shape_at(0).map_err(err)?;
        let ty = match (shape.kind, shape.basis) {
            (abc::CurveType::VariableOrder, _) => {
                let o = self.opt(&c.orders, ints)?;
                props.put("order", "int[]", o);
                let k = self.opt(&c.knots, |v| Some(Value::DoubleVec(v.to_f64())))?;
                props.put("knots", "double[]", k);
                "NurbsCurves"
            }
            (abc::CurveType::Cubic, abc::CurveBasis::Hermite) => {
                // Alembic interleaves points and tangents: P0, T0, P1, T1…
                let every = |odd: bool| {
                    move |v: abc::Values| {
                        let p = abc::chunked::<f32, 3>(&v.to_f32());
                        let p = p
                            .into_iter()
                            .skip(odd as usize)
                            .step_by(2)
                            .map(gf::Vec3f::from)
                            .collect();
                        Some(Value::Vec3fVec(p))
                    }
                };
                let p = self.prop(&c.positions, every(false))?;
                props.put("points", "point3f[]", p);
                let t = self.prop(&c.positions, every(true))?;
                props.put("tangents", "vector3f[]", t);
                "HermiteCurves"
            }
            (kind, basis) => {
                props.token(
                    "type",
                    if kind == abc::CurveType::Linear {
                        "linear"
                    } else {
                        "cubic"
                    },
                );
                let basis = match basis {
                    abc::CurveBasis::Bspline => "bspline",
                    abc::CurveBasis::CatmullRom => "catmullRom",
                    _ => "bezier",
                };
                props.token("basis", basis);
                props.token("wrap", if shape.periodic { "periodic" } else { "nonperiodic" });
                "BasisCurves"
            }
        };
        if ty != "HermiteCurves" {
            let p = self.prop(&c.positions, |v| points(&v, "point"))?;
            props.put("points", "point3f[]", p);
            let v = self.opt(&c.velocities, |v| points(&v, "vector"))?;
            props.put("velocities", "vector3f[]", v);
        }
        if let Some(n) = self.expanded(&c.normals, |v| points(&v, "normal"))? {
            props.set(
                "normals",
                Attr {
                    interpolation: c.normals.as_ref().and_then(|g| interpolation(g.scope)),
                    ..attr("normal3f[]", n)
                },
            );
        }
        let n = self.prop(&c.counts, ints)?;
        props.put("curveVertexCounts", "int[]", n);
        self.widths(props, &c.widths)?;
        self.shape_extra(props, &c.extra)?;
        Ok(ty)
    }

    /// The one shape below transform `obj` that is written with it, when
    /// there is one.
    fn merged(&self, obj: &abc::Object<'a>, x: &abc::Xform) -> R<Option<(abc::Object<'a>, abc::Geom<'a>)>> {
        let kids = obj.children().map_err(err)?;
        let [kid] = kids.as_slice() else { return Ok(None) };
        if self.protos.contains_key(&obj.path) || self.protos.contains_key(&kid.path) || kid.is_instance() {
            return Ok(None);
        }
        let g = abc::geom(kid).map_err(err)?;
        let extra = match &g {
            abc::Geom::PolyMesh(m) | abc::Geom::SubD(m) => &m.extra,
            abc::Geom::Curves(c) => &c.extra,
            abc::Geom::Points(p) => &p.extra,
            abc::Geom::Camera(c) => &c.extra,
            _ => return Ok(None),
        };
        if (!x.arb.is_empty() && !extra.arb.is_empty()) || (!x.user.is_empty() && !extra.user.is_empty()) {
            return Ok(None);
        }
        Ok(Some((kid.clone(), g)))
    }

    /// Object `obj` as the prim at `path`, with its children. Its type.
    fn prim(&mut self, obj: &abc::Object<'a>, path: &sdf::Path, over: bool) -> R<String> {
        let g = abc::geom(obj).map_err(err)?;
        let mut props = Props::default();
        let mut ty = self.shape(obj, &g, &mut props)?;
        let mut holder = obj.clone();
        let merged = match &g {
            abc::Geom::Xform(x) => self.merged(obj, x)?,
            _ => None,
        };
        if let Some((kid, kg)) = merged {
            ty = self.shape(&kid, &kg, &mut props)?;
            holder = kid;
        }

        let mut used = HashSet::new();
        let mut names = vec![];
        for kid in holder.children().map_err(err)? {
            if kid.schema().is_some_and(|s| s.starts_with("AbcGeom_FaceSet")) {
                let exclusive = kid
                    .properties()
                    .and_then(|p| p.compound(".faceset"))
                    .ok()
                    .flatten()
                    .and_then(|c| c.property(".facesExclusive").ok().flatten())
                    .and_then(|p| p.sample(0).ok())
                    .is_some_and(|v| v.to_i64().first() == Some(&1));
                props.token(
                    "subsetFamily:materialBind:familyType",
                    if exclusive { "nonOverlapping" } else { "unrestricted" },
                );
            }
            let name = clean(&kid.name, &mut used);
            self.child(&kid, path, &name)?;
            names.push(name);
        }

        let mut order = vec![];
        for (name, a) in props.0 {
            let Ok(at) = path.append_property(name.as_str()) else {
                continue;
            };
            let s = self.data.create_spec(at, SpecType::Attribute);
            s.add("typeName", Value::Token(a.ty.as_str().into()));
            if a.custom {
                s.add("custom", Value::Bool(true));
            }
            if a.uniform {
                s.add("variability", Value::Variability(sdf::Variability::Uniform));
            }
            if let Some(i) = a.interpolation {
                s.add("interpolation", Value::Token(i.into()));
            }
            match a.value {
                Sampled::Default(v) => s.add("default", v),
                Sampled::Samples(v) => s.add("timeSamples", Value::TimeSamples(v)),
            }
            order.push(tf::Token::from(name.as_str()));
        }
        let s = self.data.create_spec(path.clone(), SpecType::Prim);
        s.add(
            "specifier",
            Value::Specifier(if over {
                sdf::Specifier::Over
            } else {
                sdf::Specifier::Def
            }),
        );
        s.add("typeName", Value::Token(ty.as_str().into()));
        if over {
            s.add("instanceable", Value::Bool(true));
        }
        if !order.is_empty() {
            s.add("propertyChildren", Value::TokenVec(order));
        }
        if !names.is_empty() {
            s.add("primChildren", Value::token_vec(names));
        }
        Ok(ty)
    }

    /// Child `obj` of the prim at `parent`, named `name`: a prim, or a
    /// reference to its prototype when it is an instance or a source.
    fn child(&mut self, obj: &abc::Object<'a>, parent: &sdf::Path, name: &str) -> R<()> {
        let path = child_path(parent, name)?;
        let source = match obj.instance_source().map_err(err)? {
            Some(s) => Some(s),
            None => self.protos.contains_key(&obj.path).then(|| obj.path.clone()),
        };
        let Some(source) = source else {
            self.prim(obj, &path, false)?;
            return Ok(());
        };
        let (proto, ty) = self.proto(&source)?;
        let reference = sdf::Reference {
            prim_path: proto,
            ..Default::default()
        };
        let s = self.data.create_spec(path, SpecType::Prim);
        s.add("specifier", Value::Specifier(sdf::Specifier::Def));
        if !ty.is_empty() {
            s.add("typeName", Value::Token(ty.as_str().into()));
        }
        s.add(
            "references",
            Value::ReferenceListOp(sdf::ReferenceListOp::explicit([reference])),
        );
        Ok(())
    }

    /// The prototype of the object at `source`, written the first time: its
    /// path and type.
    fn proto(&mut self, source: &str) -> R<(sdf::Path, String)> {
        let p = self
            .protos
            .get_mut(source)
            .ok_or_else(|| format!("{source}: not an instance source"))?;
        let path = p.path.clone();
        if let Some(ty) = &p.ty {
            return Ok((path, ty.clone()));
        }
        // Noted before it is written: an instance inside its own source
        // refers to it rather than writing it again.
        p.ty = Some(String::new());
        let obj = self
            .archive
            .object(source)
            .map_err(err)?
            .ok_or_else(|| format!("{source}: an instance's source is not in the file"))?;
        let ty = self.prim(&obj, &path, true)?;
        if let Some(p) = self.protos.get_mut(source) {
            p.ty = Some(ty.clone());
        }
        Ok((path, ty))
    }
}

fn child_path(parent: &sdf::Path, name: &str) -> R<sdf::Path> {
    let text = if parent.is_abs_root() {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    };
    sdf::path(&text).map_err(|e| format!("{text}: {e}"))
}

fn interpolation(s: abc::Scope) -> Option<&'static str> {
    Some(match s {
        abc::Scope::Constant => "constant",
        abc::Scope::Uniform => "uniform",
        abc::Scope::Varying => "varying",
        abc::Scope::Vertex => "vertex",
        abc::Scope::FaceVarying => "faceVarying",
        abc::Scope::Unknown => return None,
    })
}

fn ints(v: abc::Values) -> Option<Value> {
    Some(Value::IntVec(v.to_i32()))
}

/// Three-float elements, as points, normals or vectors.
fn points(v: &abc::Values, role: &str) -> Option<Value> {
    convert(&abc::Values::F32(v.to_f32()), 3, role, true).map(|(_, v)| v)
}

/// `name` as a prim's name: letters, digits and `_`, not starting with a
/// digit, and none of `used` (then numbered: `name_1`…).
fn clean(name: &str, used: &mut HashSet<String>) -> String {
    let mut n = ident(name.trim_start_matches([' ', '_'])).replace(':', "_");
    if n.is_empty() {
        n = "_".into();
    }
    let mut out = n.clone();
    let mut i = 0;
    while used.contains(&out) {
        i += 1;
        out = format!("{n}_{i}");
    }
    used.insert(out.clone());
    out
}

/// Characters that cannot be in a USD name as `_` (`:` kept: namespaces).
fn ident(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == ':' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.replace_range(0..1, "_");
    }
    out
}

/// Alembic values as a USD type and value: `extent` of them an element,
/// `role` what they are (`point`, `normal`, `vector`, `rgb`, `rgba`,
/// `matrix`, or `texCoord`), all of them or the first element.
fn convert(v: &abc::Values, extent: usize, role: &str, array: bool) -> Option<(String, Value)> {
    use abc::Values as V;
    fn pick<T>(array: bool, x: Vec<T>, all: fn(Vec<T>) -> Value, one: fn(T) -> Value) -> Option<Value> {
        if array {
            Some(all(x))
        } else {
            x.into_iter().next().map(one)
        }
    }
    fn elems<T: Copy + Default, const N: usize, G: From<[T; N]>>(
        array: bool,
        x: &[T],
        all: fn(Vec<G>) -> Value,
        one: fn(G) -> Value,
    ) -> Option<Value> {
        pick(
            array,
            abc::chunked::<T, N>(x).into_iter().map(G::from).collect(),
            all,
            one,
        )
    }
    // The type's name for an element of `n` values of `scalar` (`f`, `d`,
    // `h` for floats, as roles spell them).
    let named = |scalar: &str, letter: &str, n: usize| -> String {
        let prefix = match (role, n) {
            ("point", 3) => "point",
            ("normal", 3) => "normal",
            ("vector", 3) => "vector",
            ("rgb", 3) | ("rgba", 4) => "color",
            ("texCoord", 2) => "texCoord",
            _ => {
                return if n == 1 {
                    scalar.to_string()
                } else {
                    format!("{scalar}{n}")
                }
            }
        };
        format!("{prefix}{n}{letter}")
    };
    let (ty, value) = match (v, extent) {
        (V::Bool(x), 1) => ("bool".to_string(), pick(array, x.clone(), Value::BoolVec, Value::Bool)?),
        (V::U8(x), 1) => (
            "uchar".to_string(),
            pick(array, x.clone(), Value::UcharVec, Value::Uchar)?,
        ),
        (V::U32(x), 1) => ("uint".to_string(), pick(array, x.clone(), Value::UintVec, Value::Uint)?),
        (V::I64(x), 1) => (
            "int64".to_string(),
            pick(array, x.clone(), Value::Int64Vec, Value::Int64)?,
        ),
        (V::U64(x), 1) => (
            "uint64".to_string(),
            pick(array, x.clone(), Value::Uint64Vec, Value::Uint64)?,
        ),
        (V::String(x), 1) => (
            "string".to_string(),
            pick(array, x.clone(), Value::StringVec, Value::String)?,
        ),
        (V::I8(_) | V::I16(_) | V::U16(_) | V::I32(_), n) => {
            let x = v.to_i32();
            let value = match n {
                1 => pick(array, x, Value::IntVec, Value::Int),
                2 => elems::<i32, 2, gf::Vec2i>(array, &x, Value::Vec2iVec, Value::Vec2i),
                3 => elems::<i32, 3, gf::Vec3i>(array, &x, Value::Vec3iVec, Value::Vec3i),
                4 => elems::<i32, 4, gf::Vec4i>(array, &x, Value::Vec4iVec, Value::Vec4i),
                _ => None,
            }?;
            (named("int", "i", n), value)
        }
        (V::F16(x), n) => {
            let x: Vec<gf::f16> = x.iter().map(|&f| gf::f16::from_f32(f)).collect();
            let value = match n {
                1 => pick(array, x, Value::HalfVec, Value::Half),
                2 => elems::<gf::f16, 2, gf::Vec2h>(array, &x, Value::Vec2hVec, Value::Vec2h),
                3 => elems::<gf::f16, 3, gf::Vec3h>(array, &x, Value::Vec3hVec, Value::Vec3h),
                4 => elems::<gf::f16, 4, gf::Vec4h>(array, &x, Value::Vec4hVec, Value::Vec4h),
                _ => None,
            }?;
            (named("half", "h", n), value)
        }
        (V::F32(x), n) => {
            let value = match n {
                1 => pick(array, x.clone(), Value::FloatVec, Value::Float),
                2 => elems::<f32, 2, gf::Vec2f>(array, x, Value::Vec2fVec, Value::Vec2f),
                3 => elems::<f32, 3, gf::Vec3f>(array, x, Value::Vec3fVec, Value::Vec3f),
                4 => elems::<f32, 4, gf::Vec4f>(array, x, Value::Vec4fVec, Value::Vec4f),
                16 => return convert(&V::F64(v.to_f64()), 16, role, array),
                _ => None,
            }?;
            (named("float", "f", n), value)
        }
        (V::F64(x), n) => {
            let value = match n {
                1 => pick(array, x.clone(), Value::DoubleVec, Value::Double),
                2 => elems::<f64, 2, gf::Vec2d>(array, x, Value::Vec2dVec, Value::Vec2d),
                3 => elems::<f64, 3, gf::Vec3d>(array, x, Value::Vec3dVec, Value::Vec3d),
                4 => elems::<f64, 4, gf::Vec4d>(array, x, Value::Vec4dVec, Value::Vec4d),
                16 => elems::<f64, 16, gf::Mat4d>(array, x, Value::Matrix4dVec, Value::Matrix4d),
                _ => None,
            }?;
            let ty = if n == 16 {
                "matrix4d".to_string()
            } else {
                named("double", "d", n)
            };
            (ty, value)
        }
        _ => return None,
    };
    Some((if array { format!("{ty}[]") } else { ty }, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openusd::usd;

    fn data(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../xms-alembic/tests/data")
            .join(name)
    }

    fn open(name: &str) -> usd::Stage {
        register_formats();
        usd::Stage::open(data(name).to_str().unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    fn get(stage: &usd::Stage, attr: &str) -> Value {
        let (prim, name) = attr.split_once('.').unwrap();
        let prim = stage.prim(prim).unwrap();
        prim.attribute(name)
            .get::<Value>()
            .unwrap_or_else(|e| panic!("{attr}: {e}"))
            .unwrap_or_else(|| panic!("{attr}: no value"))
    }

    /// Blender's cube, as `usdAbc` gives it (its baseline,
    /// `indexedTextureCoordinates.def.usda`): the transform and its mesh one
    /// prim, texture coordinates indexed, polygons left-handed.
    #[test]
    fn a_cube_is_a_mesh_as_usd_abc_makes_it() {
        let stage = open("indexedTextureCoordinates.abc");
        let roots: Vec<String> = stage
            .root_prims()
            .unwrap()
            .iter()
            .map(|t| t.as_str().to_string())
            .collect();
        assert_eq!(roots, vec!["Cube"]);
        let cube = stage.prim("/Cube").unwrap();
        assert_eq!(
            cube.type_name().unwrap().map(|t| t.as_str().to_string()),
            Some("Mesh".to_string())
        );
        assert!(
            !stage.prim("/Cube/CubeShape").unwrap().is_valid().unwrap(),
            "merged with its transform"
        );
        assert_eq!(get(&stage, "/Cube.faceVertexCounts"), Value::IntVec(vec![4; 6]));
        let Value::Vec3fVec(p) = get(&stage, "/Cube.points") else {
            panic!("points")
        };
        assert_eq!(
            (p.len(), p[0], p[7]),
            (8, [-1.0, -1.0, 1.0].into(), [1.0, 1.0, -1.0].into())
        );
        assert_eq!(get(&stage, "/Cube.orientation"), Value::Token("leftHanded".into()));
        assert_eq!(get(&stage, "/Cube.subdivisionScheme"), Value::Token("none".into()));
        let Value::Vec2fVec(st) = get(&stage, "/Cube.primvars:st") else {
            panic!("st")
        };
        assert_eq!(st.len(), 14);
        let Value::IntVec(ix) = get(&stage, "/Cube.primvars:st:indices") else {
            panic!("indices")
        };
        assert_eq!(&ix[..6], &[0, 1, 2, 3, 4, 5]);
        assert_eq!(get(&stage, "/Cube.userProperties:meshtype"), Value::Bool(false));
        // Not inheriting its parent's transform.
        assert_eq!(
            get(&stage, "/Cube.xformOpOrder"),
            Value::token_vec(["!resetXformStack!", "xformOp:transform"])
        );
    }

    /// Arbitrary parameters are primvars: a colour per face corner, indexed.
    #[test]
    fn arbitrary_parameters_are_primvars() {
        let stage = open("hasColorSet.abc");
        let prim = stage.prim("/hasColorSet").unwrap();
        let set = prim.attribute("primvars:colorSet");
        assert!(set.is_defined().unwrap());
        assert!(matches!(
            get(&stage, "/hasColorSet.primvars:colorSet"),
            Value::Vec3fVec(_)
        ));
        assert!(matches!(
            get(&stage, "/hasColorSet.primvars:colorSet:indices"),
            Value::IntVec(_)
        ));
        let root = stage.root_layer();
        let ty = root
            .data()
            .try_field(&sdf::path("/hasColorSet.primvars:colorSet").unwrap(), "typeName")
            .unwrap()
            .map(|v| v.into_owned());
        assert_eq!(ty, Some(Value::Token("color3f[]".into())));
        let interpolation = root
            .data()
            .try_field(&sdf::path("/hasColorSet.primvars:colorSet").unwrap(), "interpolation")
            .unwrap()
            .map(|v| v.into_owned());
        assert_eq!(interpolation, Some(Value::Token("faceVarying".into())));
    }

    /// Points moving a unit a frame at 30 fps: a sample a frame, at frames 1
    /// to 30.
    #[test]
    fn moving_points_are_time_samples_at_the_file_frame_rate() {
        let stage = open("testusdtimecode.abc");
        let root = stage.root_layer();
        let meta = |f: &str| {
            root.data()
                .try_field(&sdf::Path::abs_root(), f)
                .unwrap()
                .map(|v| v.into_owned())
        };
        assert_eq!(meta("timeCodesPerSecond"), Some(Value::Double(30.0)));
        assert_eq!(meta("startTimeCode"), Some(Value::Double(1.0)));
        assert_eq!(meta("endTimeCode"), Some(Value::Double(30.0)));
        drop(root);
        let prim = stage.prim("/font1").unwrap();
        let points = prim.attribute("points");
        let at = |t: f64| match points.get_at::<Value>(usd::TimeCode::new(t)).unwrap() {
            Some(Value::Vec3fVec(p)) => p[0],
            other => panic!("{other:?}"),
        };
        assert_eq!(at(1.0), [0.5, -0.5, 0.5].into());
        assert_eq!(at(30.0), [29.5, -0.5, 0.5].into());
    }

    /// Instances, as `usdAbc` makes them (`nestedInstancing.def.usda`): each
    /// source written once, at the root, and referenced where it was and by
    /// its instances.
    #[test]
    fn instances_reference_their_source() {
        let stage = open("nestedInstancing.abc");
        let root = stage.root_layer();
        let spec = |p: &str, f: &str| {
            root.data()
                .try_field(&sdf::path(p).unwrap(), f)
                .unwrap()
                .map(|v| v.into_owned())
        };
        for proto in ["/a", "/instance0"] {
            assert_eq!(
                spec(proto, "specifier"),
                Some(Value::Specifier(sdf::Specifier::Over)),
                "{proto}"
            );
            assert_eq!(spec(proto, "instanceable"), Some(Value::Bool(true)), "{proto}");
        }
        drop(root);
        for p in ["/rootNode/instance0", "/rootNode/c/instance1"] {
            let prim = stage.prim(p).unwrap();
            assert!(prim.is_valid().unwrap(), "{p}");
            assert!(prim.is_instance().unwrap(), "{p}");
        }
        let Value::Matrix4d(m) = get(&stage, "/a.xformOp:transform") else {
            panic!("a's transform")
        };
        assert_eq!(&m.0[12..16], &[-2.0, 0.0, 0.0, 1.0]);
    }

    /// A camera's lens in USD's units, its transform with it: the frustum
    /// `usdAbc`'s test expects (±42735 wide at its far plane, 100000 away).
    #[test]
    fn a_camera_has_its_lens_in_millimetres() {
        let stage = open("testAbcCamera.abc");
        let cam = stage.prim("/World/main_cam").unwrap();
        assert_eq!(
            cam.type_name().unwrap().map(|t| t.as_str().to_string()),
            Some("Camera".to_string())
        );
        let at = |name: &str| match cam.attribute(name).get_at::<Value>(usd::TimeCode::new(0.0)).unwrap() {
            Some(Value::Float(f)) => f as f64,
            other => panic!("{name}: {other:?}"),
        };
        let (h, focal) = (at("horizontalAperture"), at("focalLength"));
        assert!((h - 10000.0).abs() < 1e-3, "{h}");
        assert!((100000.0 * h / 2.0 / focal - 42734.978).abs() < 1.0, "{h} {focal}");
        let clip = cam
            .attribute("clippingRange")
            .get_at::<Value>(usd::TimeCode::new(0.0))
            .unwrap();
        assert_eq!(clip, Some(Value::Vec2f([0.1, 100000.0].into())));
    }

    /// Names USD cannot have are made into names it can, kept apart.
    #[test]
    fn names_are_made_valid() {
        let mut used = HashSet::new();
        assert_eq!(clean("pCube1", &mut used), "pCube1");
        assert_eq!(clean("ns:pCube1", &mut used), "ns_pCube1");
        assert_eq!(clean("1 shape-a", &mut used), "__shape_a");
        assert_eq!(clean("pCube1", &mut used), "pCube1_1");
        assert_eq!(ident("ns:Cd 2"), "ns:Cd_2", "properties keep their namespaces");
    }
}
