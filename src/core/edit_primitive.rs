//! Edit Primitive: the prims its Primitive Locations name, edited as what they
//! are. Its Type (Xform, Mesh, Curves, Points, Camera, Light) gives its form:
//! the same values the operator making that kind of prim has, showing the
//! target's own; a value changed in the form is an override, the only thing
//! the operator applies (and Write USD writes). Untouched values follow the
//! target, file changes and animation included.
//!
//! Every type has the Xform section: Translate, Rotate and Scale (the prim's
//! own transform, relative to its parent, as USD composes it: a group moves
//! with what is below it), visibility, purpose, active and kind. A mesh adds
//! doubleSided and subdivisionScheme; curves their type, basis and wrap; a
//! camera its lens and gate; a light its UsdLux values. A RenderSettings prim
//! has Render Commons' values alone: a file's is written as an over of what is
//! overridden, and stays the one rendered when it was.
//!
//! Overrides are kept by name (`translate`, `focalLength`, `inputs:intensity`)
//! as plain JSON, so only they are saved and keyed.

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::{DMat4, EulerRot, Mat4, Quat, Vec3};

use crate::core::geo::{CurveBasis, CurveWrap, Topology};
use crate::core::locations;
use crate::core::properties::{SetContext, Value};
use crate::core::set_attribute::SetRow;
use crate::types::{EvalResult, Placement, Prim};
use crate::usd_scene::StageTree;

/// What an Edit Primitive edits its targets as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EditKind {
    #[default]
    Xform,
    Mesh,
    Curves,
    Points,
    Camera,
    Light,
    /// A RenderSettings prim: a file's, or one Imago made.
    RenderSettings,
}

impl EditKind {
    pub const ALL: [EditKind; 7] = [
        EditKind::Xform,
        EditKind::Mesh,
        EditKind::Curves,
        EditKind::Points,
        EditKind::Camera,
        EditKind::Light,
        EditKind::RenderSettings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EditKind::Xform => "Xform",
            EditKind::Mesh => "Mesh",
            EditKind::Curves => "Curves",
            EditKind::Points => "Points",
            EditKind::Camera => "Camera",
            EditKind::Light => "Light",
            EditKind::RenderSettings => "RenderSettings",
        }
    }

    /// What a primitive is.
    pub fn of(p: &Prim) -> EditKind {
        if p.camera.is_some() {
            return EditKind::Camera;
        }
        if p.light.is_some() {
            return EditKind::Light;
        }
        if p.render
            .as_ref()
            .is_some_and(|r| r.kind == crate::core::render::RenderKind::Settings)
        {
            return EditKind::RenderSettings;
        }
        match p.geo.as_ref().map(|g| &g.topology) {
            Some(Topology::Mesh { .. }) => EditKind::Mesh,
            Some(Topology::Curves { .. }) => EditKind::Curves,
            Some(Topology::Points) => EditKind::Points,
            None if !p.mesh.vertices.is_empty() => EditKind::Mesh,
            None if !p.mesh.curve_points.is_empty() => EditKind::Curves,
            None if !p.mesh.points.is_empty() => EditKind::Points,
            None => EditKind::Xform,
        }
    }

    /// Whether a primitive of kind `of` takes this form: every prim but a
    /// RenderSettings takes the Xform section.
    pub fn takes(self, of: EditKind) -> bool {
        self == of || (self == EditKind::Xform && of != EditKind::RenderSettings)
    }
}

/// One value of the form: its name (what the override is kept by, USD's for
/// what is written), label, help, the target's value, and the tokens it may
/// take.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub label: String,
    pub help: String,
    pub group: String,
    pub value: Value,
    pub choices: Vec<String>,
    /// A slider's range, where the operator making the prim has one.
    pub range: Option<(f64, f64)>,
}

fn param(group: &str, name: &str, label: &str, help: &str, value: Value) -> Param {
    Param {
        name: name.into(),
        label: label.into(),
        help: help.into(),
        group: group.into(),
        value,
        choices: vec![],
        range: None,
    }
}

fn tokens(p: Param, choices: &[&str]) -> Param {
    Param {
        choices: choices.iter().map(|c| c.to_string()).collect(),
        ..p
    }
}

/// The Xform section's transform values.
pub const TRANSFORM: [&str; 3] = ["translate", "rotate", "scale"];
/// The Xform section's (and a mesh's) values, set as Set Attribute sets them.
const PROPERTIES: [&str; 6] = [
    "visibility",
    "purpose",
    "active",
    "kind",
    "doubleSided",
    "subdivisionScheme",
];

/// Curves' values, kept in their topology.
const CURVE: [&str; 3] = ["type", "basis", "wrap"];

/// Curves' basis and wrap: their topology's, else what the Viewer draws.
fn curve_shape(p: &Prim) -> (CurveBasis, CurveWrap) {
    match p.geo.as_deref().map(|g| &g.topology) {
        Some(Topology::Curves { basis, wrap, .. }) => (*basis, *wrap),
        _ => (p.mesh.curve_basis, p.mesh.curve_wrap),
    }
}

/// Basis and wrap as USD's `type`, `basis` and `wrap` tokens. Linear curves
/// have no basis of their own: USD's fallback, bezier.
fn curve_tokens((basis, wrap): (CurveBasis, CurveWrap)) -> (&'static str, &'static str, &'static str) {
    let (ty, b) = match basis {
        CurveBasis::Linear => ("linear", "bezier"),
        CurveBasis::Bezier => ("cubic", "bezier"),
        CurveBasis::Bspline => ("cubic", "bspline"),
        CurveBasis::CatmullRom => ("cubic", "catmullRom"),
    };
    let w = match wrap {
        CurveWrap::Nonperiodic => "nonperiodic",
        CurveWrap::Periodic => "periodic",
        CurveWrap::Pinned => "pinned",
    };
    (ty, b, w)
}

/// The basis and wrap of USD's tokens.
fn curve_shape_of(ty: &str, basis: &str, wrap: &str) -> (CurveBasis, CurveWrap) {
    let b = match (ty, basis) {
        ("linear", _) => CurveBasis::Linear,
        (_, "bspline") => CurveBasis::Bspline,
        (_, "catmullRom") => CurveBasis::CatmullRom,
        _ => CurveBasis::Bezier,
    };
    let w = match wrap {
        "periodic" => CurveWrap::Periodic,
        "pinned" => CurveWrap::Pinned,
        _ => CurveWrap::Nonperiodic,
    };
    (b, w)
}

/// A target: a primitive, or a prim with none (a group of a stage, or an
/// Xform Imago made above its primitives).
pub enum Target<'a> {
    Prim(&'a Prim),
    Group { path: String, tree: Option<&'a StageTree> },
}

/// The targets the locations name in a scene, primitives first, in order; a
/// group once.
pub fn targets<'a>(prims: &'a [Prim], list: &[String]) -> Vec<Target<'a>> {
    let mut out: Vec<Target> = prims
        .iter()
        .filter(|p| locations::any_names(list, &p.path))
        .map(Target::Prim)
        .collect();
    let paths: std::collections::HashSet<&str> = prims.iter().map(|p| p.path.as_str()).collect();
    let mut groups: std::collections::HashSet<String> = Default::default();
    for p in prims {
        // The prims above it: its stage's, or the Xforms Imago made.
        for g in crate::core::properties::parents(&p.path) {
            if !groups.contains(&g) && !paths.contains(g.as_str()) && locations::any_names(list, &g) {
                groups.insert(g.clone());
                out.push(Target::Group {
                    tree: p.stage.as_deref().filter(|t| t.node(&g).is_some()),
                    path: g,
                });
            }
        }
    }
    // The other prims of the stages with no primitive of their own: a
    // RenderSettings, a scope of materials, a camera rig.
    let mut trees: Vec<&'a StageTree> = vec![];
    for t in prims.iter().filter_map(|p| p.stage.as_deref()) {
        if trees.iter().any(|x| std::ptr::eq(*x, t)) {
            continue;
        }
        trees.push(t);
        for n in &t.nodes {
            if !groups.contains(&n.path) && !paths.contains(n.path.as_str()) && locations::any_names(list, &n.path) {
                groups.insert(n.path.clone());
                out.push(Target::Group {
                    tree: Some(t),
                    path: n.path.clone(),
                });
            }
        }
    }
    out
}

impl Target<'_> {
    pub fn path(&self) -> &str {
        match self {
            Target::Prim(p) => &p.path,
            Target::Group { path, .. } => path,
        }
    }

    pub fn kind(&self) -> EditKind {
        match self {
            Target::Prim(p) => EditKind::of(p),
            Target::Group { path, tree } => match tree.and_then(|t| t.node(path)) {
                Some(n) if n.type_name == "RenderSettings" => EditKind::RenderSettings,
                _ => EditKind::Xform,
            },
        }
    }
}

/// A transform's translate, rotate (degrees, X then Y then Z) and scale.
fn parts(m: DMat4) -> [[f32; 3]; 3] {
    let (s, r, t) = m.as_mat4().to_scale_rotation_translation();
    let (x, y, z) = r.to_euler(EulerRot::XYZ);
    [
        t.to_array(),
        [x.to_degrees(), y.to_degrees(), z.to_degrees()],
        s.to_array(),
    ]
}

fn compose(p: [[f32; 3]; 3]) -> DMat4 {
    let r = p[1].map(f32::to_radians);
    Mat4::from_scale_rotation_translation(
        Vec3::from(p[2]),
        Quat::from_euler(EulerRot::XYZ, r[0], r[1], r[2]),
        Vec3::from(p[0]),
    )
    .as_dmat4()
}

/// Where a primitive's parents put it, and its own transform under them: from
/// the stage it came from (as loaded), else its placement alone. `None` for
/// instanced geometry, placed many times.
fn frame_of(p: &Prim) -> Option<(DMat4, DMat4)> {
    let now = match &p.place {
        Placement::One(m) => m.as_dmat4(),
        Placement::InPlace => DMat4::IDENTITY,
        Placement::Many(_) => return None,
    };
    // As loaded, with what a group above it moved since (Edit Primitive).
    let parents = match (&p.stage, &p.source) {
        (Some(tree), Some(src)) => match (tree.node(&p.path), &src.place) {
            (Some(node), Placement::One(loaded)) => {
                let parent = crate::core::properties::parents(&p.path).pop().unwrap_or_default();
                let above = tree.world(&parent, true) * tree.world(&parent, false).inverse();
                above * loaded.as_dmat4() * node.local.inverse()
            }
            _ => DMat4::IDENTITY,
        },
        _ => DMat4::IDENTITY,
    };
    Some((parents, parents.inverse() * now))
}

/// A group's place in the world and its own transform: a stage's prim from its
/// tree (its parents' transforms and the stage's root correction), an Xform
/// Imago made where it is (none).
fn group_frame(path: &str, tree: Option<&StageTree>) -> (DMat4, DMat4) {
    let Some(tree) = tree else {
        return (DMat4::IDENTITY, DMat4::IDENTITY);
    };
    let local = tree.local(path, true).unwrap_or(DMat4::IDENTITY);
    (tree.world(path, true) * local.inverse(), local)
}

/// A value authored on a stage prim, by name.
fn authored<'a>(tree: Option<&'a StageTree>, path: &str, name: &str) -> Option<&'a Value> {
    tree?
        .node(path)?
        .authored
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v)
}

/// The form of `kind` with the values of `target`.
pub fn params(kind: EditKind, target: &Target) -> Vec<Param> {
    if kind == EditKind::RenderSettings {
        return render_params(target);
    }
    let (path, tree, prim) = match target {
        Target::Prim(p) => (p.path.as_str(), p.stage.as_deref(), Some(*p)),
        Target::Group { path, tree } => (path.as_str(), *tree, None),
    };
    let local = match prim {
        Some(p) => frame_of(p).map(|f| f.1),
        None => Some(group_frame(path, tree).1),
    };
    let [t, r, s] = parts(local.unwrap_or(DMat4::IDENTITY));
    let prop = |name: &str| {
        prim.and_then(|p| p.props.get(name).cloned())
            .or_else(|| {
                tree.and_then(|t| t.opinions.get(path))
                    .and_then(|o| o.props.get(name).cloned())
            })
            .or_else(|| authored(tree, path, name).cloned())
    };
    let token = |name: &str, or: &str| match prop(name) {
        Some(Value::Token(t)) | Some(Value::String(t)) => t,
        _ => or.to_string(),
    };
    let visibility = match prop("visibility") {
        Some(Value::Token(t)) => t,
        _ if tree.and_then(|t| t.node(path)).is_some_and(|n| n.hidden) => "invisible".into(),
        _ => "inherited".into(),
    };
    let purpose = match prim {
        Some(p) => match crate::core::properties::purpose_of(p) {
            crate::types::Purpose::Default => "default",
            crate::types::Purpose::Render { .. } => "render",
            crate::types::Purpose::Proxy => "proxy",
            crate::types::Purpose::Guide => "guide",
        }
        .to_string(),
        None => token("purpose", "default"),
    };
    let x = "Xform";
    let mut out = vec![
        param(
            x,
            "translate",
            "Translate",
            "Its own transform, relative to its parent: what is below it moves with it",
            Value::Float3(t),
        ),
        param(x, "rotate", "Rotate", "Degrees, X then Y then Z", Value::Float3(r)),
        param(x, "scale", "Scale", "", Value::Float3(s)),
        tokens(
            param(
                x,
                "visibility",
                "Visibility",
                "Invisible hides it and what is below it",
                Value::Token(visibility),
            ),
            &["inherited", "invisible"],
        ),
        tokens(
            param(
                x,
                "purpose",
                "Purpose",
                "What kind of drawing it is for: render geometry, a proxy, a guide",
                Value::Token(purpose),
            ),
            &["default", "render", "proxy", "guide"],
        ),
        param(
            x,
            "active",
            "Active",
            "Off takes it, and what is below it, out of the scene",
            Value::Bool(true),
        ),
        tokens(
            param(
                x,
                "kind",
                "Kind",
                "Its place in the model hierarchy",
                Value::Token(token("kind", "")),
            ),
            &["", "component", "subcomponent", "assembly", "group"],
        ),
    ];
    match (kind, prim) {
        (EditKind::Mesh, _) => {
            let double = matches!(prop("doubleSided"), Some(Value::Bool(true)));
            let g = "Mesh";
            out.push(param(
                g,
                "doubleSided",
                "Double sided",
                "Drawn from both sides",
                Value::Bool(double),
            ));
            out.push(tokens(
                param(
                    g,
                    "subdivisionScheme",
                    "Subdivision",
                    "How it is smoothed when subdivided",
                    Value::Token(token("subdivisionScheme", "catmullClark")),
                ),
                &["catmullClark", "loop", "bilinear", "none"],
            ));
        }
        (EditKind::Curves, Some(p)) => {
            let (ty, basis, wrap) = curve_tokens(curve_shape(p));
            let g = "Curves";
            out.push(tokens(
                param(
                    g,
                    "type",
                    "Type",
                    "USD type: linear, or cubic through Basis",
                    Value::Token(ty.into()),
                ),
                &["linear", "cubic"],
            ));
            out.push(tokens(
                param(
                    g,
                    "basis",
                    "Basis",
                    "USD basis: how cubic curves pass through their points (not used by linear ones)",
                    Value::Token(basis.into()),
                ),
                &["bezier", "bspline", "catmullRom"],
            ));
            out.push(tokens(
                param(
                    g,
                    "wrap",
                    "Wrap",
                    "USD wrap: open (nonperiodic), closed (periodic), or reaching its end points (pinned)",
                    Value::Token(wrap.into()),
                ),
                &["nonperiodic", "periodic", "pinned"],
            ));
        }
        (EditKind::Camera, Some(p)) => {
            let c = p.camera.as_deref().cloned().unwrap_or_default();
            let g = "Lens";
            let f = |v: f32| Value::Float(v);
            out.push(tokens(
                param(
                    g,
                    "projection",
                    "Projection",
                    "",
                    Value::Token(c.projection.token().into()),
                ),
                &["perspective", "orthographic"],
            ));
            let mut focal = param(
                g,
                "focalLength",
                "Focal length",
                "In tenths of a scene unit: millimetres in a centimetre scene",
                f(c.focal_length),
            );
            focal.range = Some((4.0, 300.0));
            out.push(focal);
            out.push(param(
                g,
                "horizontalAperture",
                "Filmback H",
                "The gate's width",
                f(c.horizontal_aperture),
            ));
            out.push(param(
                g,
                "verticalAperture",
                "Filmback V",
                "The gate's height",
                f(c.vertical_aperture),
            ));
            out.push(param(
                g,
                "horizontalApertureOffset",
                "Filmback offset H",
                "Written, not shown in the Viewer yet",
                f(c.horizontal_aperture_offset),
            ));
            out.push(param(
                g,
                "verticalApertureOffset",
                "Filmback offset V",
                "Written, not shown in the Viewer yet",
                f(c.vertical_aperture_offset),
            ));
            out.push(param(
                "Clipping",
                "clippingRange",
                "Clipping range",
                "Near and far, in scene units",
                Value::Float2(c.clipping_range),
            ));
            out.push(param(
                "Depth of field",
                "fStop",
                "f-stop",
                "0: no depth of field",
                f(c.f_stop),
            ));
            out.push(param(
                "Depth of field",
                "focusDistance",
                "Focus distance",
                "In scene units",
                f(c.focus_distance),
            ));
        }
        (EditKind::Light, Some(p)) => {
            if let Some(l) = p.light.as_deref() {
                for lp in l.params() {
                    out.push(Param {
                        name: lp.name.clone(),
                        label: lp.shown().to_string(),
                        help: lp.help.clone(),
                        group: lp.group.clone(),
                        value: l.value(&lp.name).unwrap_or_else(|| l.fallback(lp)),
                        choices: lp.choices.clone(),
                        range: lp.min.zip(lp.max),
                    });
                }
            }
        }
        _ => {}
    }
    out
}

/// A RenderSettings prim's form, Render Commons' values by their USD names,
/// with the target's: what a file authors (USD's fallbacks where it does not;
/// its camera is not read yet), or what Imago set on one it made.
fn render_params(target: &Target) -> Vec<Param> {
    use crate::core::render::RAttr;
    // A value written by a render operator, read back as `like` is.
    let parsed = |a: &RAttr, like: &Value| -> Option<Value> {
        let t = a.literal.trim();
        let nums: Vec<f32> = t
            .split(|c: char| c == '(' || c == ')' || c == ',' || c.is_whitespace())
            .filter_map(|x| x.parse().ok())
            .collect();
        Some(match like {
            Value::Bool(_) => Value::Bool(t == "true" || t == "1"),
            Value::Token(_) => Value::Token(t.trim_matches('"').to_string()),
            Value::Float(_) => Value::Float(*nums.first()?),
            Value::Float2(_) => Value::Float2([*nums.first()?, *nums.get(1)?]),
            Value::Float4(_) => Value::Float4([*nums.first()?, *nums.get(1)?, *nums.get(2)?, *nums.get(3)?]),
            _ => return None,
        })
    };
    let value = |name: &str, fallback: Value| -> Value {
        let found = match target {
            Target::Prim(p) => p.render.as_deref().and_then(|r| {
                if name == "camera" {
                    return r
                        .rels
                        .get("camera")
                        .and_then(|c| c.targets.first())
                        .map(|c| Value::Token(c.clone()));
                }
                r.attrs.get(name).and_then(|a| parsed(a, &fallback))
            }),
            Target::Group { path, tree } => authored(*tree, path, name).cloned().map(|v| match (&fallback, v) {
                (Value::Float2(_), Value::Float2(x)) => Value::Float2(x),
                (Value::Float2(_), Value::Float3(x)) => Value::Float2([x[0], x[1]]),
                (_, v) => v,
            }),
        };
        found.filter(|v| v.kind() == fallback.kind()).unwrap_or(fallback)
    };
    let g = "Render Settings";
    vec![
        param(
            g,
            "camera",
            "Camera",
            "The camera rendered through (rel camera). A file's is not read yet: empty shows it is kept",
            value("camera", Value::Token(String::new())),
        ),
        param(
            g,
            "resolution",
            "Resolution",
            "Width and height in pixels (uniform int2 resolution)",
            value("resolution", Value::Float2([2048.0, 1080.0])),
        ),
        param(
            g,
            "pixelAspectRatio",
            "Pixel Aspect",
            "",
            value("pixelAspectRatio", Value::Float(1.0)),
        ),
        tokens(
            param(
                g,
                "aspectRatioConformPolicy",
                "Aspect Conform",
                "How the camera's aperture and the image's aspect are made to agree",
                value("aspectRatioConformPolicy", Value::Token("expandAperture".into())),
            ),
            &[
                "expandAperture",
                "cropAperture",
                "adjustApertureWidth",
                "adjustApertureHeight",
                "adjustPixelAspectRatio",
            ],
        ),
        param(
            g,
            "dataWindowNDC",
            "Data Window",
            "The part of the frame rendered, from 0 to 1 (overscan beyond it)",
            value("dataWindowNDC", Value::Float4([0.0, 0.0, 1.0, 1.0])),
        ),
        param(
            g,
            "disableMotionBlur",
            "Disable Motion Blur",
            "",
            value("disableMotionBlur", Value::Bool(false)),
        ),
        param(
            g,
            "disableDepthOfField",
            "Disable Depth of Field",
            "",
            value("disableDepthOfField", Value::Bool(false)),
        ),
    ]
}

/// The overrides of a RenderSettings prim's form written into it: its camera
/// as a relationship, its resolution as two whole numbers, the rest as they
/// are, all `uniform` as the schema has them.
fn render_overrides(
    r: &mut crate::core::render::RenderPrim,
    shown: &[Param],
    edits: &BTreeMap<String, serde_json::Value>,
) {
    use crate::core::render::RAttr;
    for q in shown {
        let Some(j) = edits.get(&q.name) else { continue };
        match (q.name.as_str(), typed(&q.value, j)) {
            ("camera", Value::Token(c)) if c.trim().is_empty() => {
                r.rels.remove("camera");
            }
            ("camera", Value::Token(c)) => r.targets("camera", vec![c.trim().to_string()]),
            ("resolution", Value::Float2([w, h])) => r.set(
                "resolution",
                RAttr::int2([w.round() as i32, h.round() as i32]).uniform(),
            ),
            (name, v) => r.set(name, RAttr::of(&v).uniform()),
        }
    }
}

/// A value as an override keeps it: plain JSON.
pub fn plain(v: &Value) -> serde_json::Value {
    use serde_json::json;
    match v {
        Value::Bool(b) => json!(b),
        Value::Int(i) => json!(i),
        Value::Float(x) => json!(x),
        Value::Double(x) => json!(x),
        Value::String(s) | Value::Token(s) | Value::Asset(s) => json!(s),
        Value::Float2(a) => json!(a),
        Value::Float3(a) | Value::Color3f(a) => json!(a),
        Value::Float4(a) => json!(a),
    }
}

/// An override read back, of the type of `like` (the target's value).
pub fn typed(like: &Value, j: &serde_json::Value) -> Value {
    crate::core::render::value_of(like.kind(), j)
}

/// The targets edited: each takes the overrides of its kind, its transform
/// first (a group's moves what is below it), then its properties.
pub fn edit(
    input: &EvalResult,
    list: &[String],
    kind: EditKind,
    edits: &BTreeMap<String, serde_json::Value>,
) -> EvalResult {
    let EvalResult::Named(prims) = input else {
        return input.clone();
    };
    if edits.is_empty() || !locations::is_set(list) {
        return input.clone();
    }
    let mut out: Vec<Prim> = prims.clone();
    let moves = TRANSFORM.iter().any(|n| edits.contains_key(*n));
    // Each primitive named: its own transform, its lens or light.
    for p in out.iter_mut().filter(|p| locations::any_names(list, &p.path)) {
        let of = EditKind::of(p);
        if !kind.takes(of) {
            continue;
        }
        let shown = params(kind, &Target::Prim(p));
        let value = |name: &str| -> Option<Value> {
            let j = edits.get(name)?;
            let like = shown.iter().find(|q| q.name == name)?;
            Some(typed(&like.value, j))
        };
        if moves && p.instanced_by.is_none() {
            if let Some((parents, local)) = frame_of(p) {
                let mut now = parts(local);
                for (k, name) in TRANSFORM.iter().enumerate() {
                    if let Some(Value::Float3(v)) = value(name) {
                        now[k] = v;
                    }
                }
                p.place = Placement::One((parents * compose(now)).as_mat4());
            }
        }
        // A file's camera or light: what is written is an override of its
        // values; one Imago made is written whole from its values.
        let from_file = p.stage.is_some();
        let mut written: Vec<(String, Value)> = vec![];
        if kind == EditKind::Camera {
            if let Some(c) = p.camera.as_deref() {
                let mut c = c.clone();
                for q in shown.iter().filter(|q| q.group != "Xform") {
                    let Some(v) = value(&q.name) else { continue };
                    let x = match &v {
                        Value::Float(x) => *x,
                        _ => 0.0,
                    };
                    match (q.name.as_str(), &v) {
                        ("projection", Value::Token(t)) => {
                            c.projection = if t == "orthographic" {
                                crate::core::camera::Projection::Orthographic
                            } else {
                                crate::core::camera::Projection::Perspective
                            }
                        }
                        ("focalLength", _) => c.focal_length = x,
                        ("horizontalAperture", _) => c.horizontal_aperture = x,
                        ("verticalAperture", _) => c.vertical_aperture = x,
                        ("horizontalApertureOffset", _) => c.horizontal_aperture_offset = x,
                        ("verticalApertureOffset", _) => c.vertical_aperture_offset = x,
                        ("clippingRange", Value::Float2(r)) => c.clipping_range = *r,
                        ("fStop", _) => c.f_stop = x,
                        ("focusDistance", _) => c.focus_distance = x,
                        _ => continue,
                    }
                    written.push((q.name.clone(), v));
                }
                p.camera = Some(Arc::new(c));
            }
        }
        // Curves: their type, basis and wrap, in the geometry (written with
        // its topology) and in what the Viewer draws.
        if kind == EditKind::Curves && CURVE.iter().any(|n| edits.contains_key(*n)) {
            let (ty, basis, wrap) = curve_tokens(curve_shape(p));
            let pick = |name: &str, or: &str| match value(name) {
                Some(Value::Token(t)) => t,
                _ => or.to_string(),
            };
            let (b, w) = curve_shape_of(&pick("type", ty), &pick("basis", basis), &pick("wrap", wrap));
            if let Some(geo) = p.geo.as_deref() {
                if let Topology::Curves { counts, .. } = &geo.topology {
                    let mut geo = geo.clone();
                    geo.topology = Topology::Curves {
                        counts: counts.clone(),
                        basis: b,
                        wrap: w,
                    };
                    p.geo = Some(Arc::new(geo));
                }
            }
            let mut mesh = (*p.mesh).clone();
            mesh.curve_basis = b;
            mesh.curve_wrap = w;
            p.mesh = Arc::new(mesh);
        }
        if kind == EditKind::Light {
            if let Some(l) = p.light.as_deref() {
                let mut l = l.clone();
                for q in shown.iter().filter(|q| q.group != "Xform") {
                    if let Some(v) = value(&q.name) {
                        l.set(&q.name, v.clone());
                        written.push((q.name.clone(), v));
                    }
                }
                p.light = Some(Arc::new(l));
            }
        }
        if kind == EditKind::RenderSettings {
            if let Some(r) = p.render.as_deref() {
                let mut r = r.clone();
                render_overrides(&mut r, &shown, edits);
                p.render = Some(Arc::new(r));
            }
        }
        if from_file && !written.is_empty() {
            let mut props = (*p.props).clone();
            props.extend(written);
            p.props = Arc::new(props);
        }
    }
    // A file's RenderSettings prim named: an over of it, with the overrides.
    if kind == EditKind::RenderSettings {
        for t in targets(prims, list) {
            if t.kind() != EditKind::RenderSettings {
                continue;
            }
            let Target::Group { path, .. } = &t else { continue };
            let mut r = crate::core::render::RenderPrim::new(crate::core::render::RenderKind::Settings);
            r.over = true;
            render_overrides(&mut r, &params(kind, &t), edits);
            if !r.attrs.is_empty() || !r.rels.is_empty() {
                out.push(crate::core::render::render_prim(path, r));
            }
        }
    }
    // Groups named: what is below each moves with it.
    if moves && kind == EditKind::Xform {
        // Each stage's tree as this operator has left it, by the one loaded.
        let mut trees: std::collections::HashMap<*const StageTree, Arc<StageTree>> = Default::default();
        for t in targets(prims, list) {
            if !kind.takes(t.kind()) {
                continue;
            }
            let Target::Group { path, tree } = t else { continue };
            let (parents, local) = group_frame(&path, tree);
            let mut now = parts(local);
            for (k, name) in TRANSFORM.iter().enumerate() {
                if let Some(j) = edits.get(*name) {
                    if let Value::Float3(v) = typed(&Value::Float3(now[k]), j) {
                        now[k] = v;
                    }
                }
            }
            let delta = (parents * compose(now) * local.inverse() * parents.inverse()).as_mat4();
            let below = format!("{path}/");
            for p in out.iter_mut().filter(|p| p.path.starts_with(&below)) {
                p.place = match &p.place {
                    Placement::InPlace => Placement::One(delta),
                    place => place.moved(delta),
                };
            }
            // A stage's group keeps its new transform as its own, written on
            // it: every primitive of the stage then shares the new tree.
            if let Some(loaded) = tree {
                let key: *const StageTree = loaded;
                let now_tree: &StageTree = trees.get(&key).map(|t| &**t).unwrap_or(loaded);
                let mut new = now_tree.clone();
                new.opinions.entry(path.clone()).or_default().transform = Some(compose(now).to_cols_array());
                let new = Arc::new(new);
                let was: *const StageTree = now_tree;
                for p in out.iter_mut() {
                    if p.stage.as_deref().is_some_and(|t| std::ptr::eq(t, was)) {
                        p.stage = Some(new.clone());
                    }
                }
                trees.insert(key, new);
            }
        }
    }
    // Visibility, purpose, active, kind and a mesh's values: as Set Attribute
    // sets them, on the targets of this kind, groups among them.
    let taken: Vec<String> = targets(prims, list)
        .iter()
        .filter(|t| kind.takes(t.kind()))
        .map(|t| t.path().to_string())
        .collect();
    let rows: Vec<SetRow> = PROPERTIES
        .iter()
        .filter_map(|name| {
            let j = edits.get(*name)?;
            let like = match *name {
                "active" | "doubleSided" => Value::Bool(false),
                _ => Value::Token(String::new()),
            };
            Some(SetRow {
                context: SetContext::Property,
                name: name.to_string(),
                value: typed(&like, j),
            })
        })
        .collect();
    let out = EvalResult::Named(out);
    // None of this kind: nothing (an empty list would be every primitive).
    if rows.is_empty() || taken.is_empty() {
        out
    } else {
        crate::core::set_attribute::set_attributes(&out, &rows, &taken)
    }
}

/// Where an Edit Primitive's handles work: on the first prim named of its
/// type, in its parents' space. Their place in the world, and its translate,
/// rotate (degrees) and scale as the operator leaves them: its overrides,
/// else the prim's own.
pub fn handles(
    prims: &[Prim],
    list: &[String],
    kind: EditKind,
    edits: &BTreeMap<String, serde_json::Value>,
) -> Option<(Mat4, [[f32; 3]; 3])> {
    // A RenderSettings prim is not placed.
    if kind == EditKind::RenderSettings {
        return None;
    }
    let all = targets(prims, list);
    let (parents, local) = match all.iter().find(|t| kind.takes(t.kind()))? {
        Target::Prim(p) => frame_of(p)?,
        Target::Group { path, tree } => group_frame(path, *tree),
    };
    let mut now = parts(local);
    for (k, name) in TRANSFORM.iter().enumerate() {
        if let Some(j) = edits.get(*name) {
            if let Value::Float3(v) = typed(&Value::Float3(now[k]), j) {
                now[k] = v;
            }
        }
    }
    Some((parents.as_mat4(), now))
}

/// What the targets of a list are in a scene: how many, the kind of the first,
/// and how many are of another kind than `kind` (left as they are).
pub fn survey(prims: &[Prim], list: &[String], kind: EditKind) -> (usize, Option<EditKind>, usize) {
    let all = targets(prims, list);
    let first = all.first().map(|t| t.kind());
    let other = all.iter().filter(|t| !kind.takes(t.kind())).count();
    (all.len(), first, other)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MeshData;

    fn scene() -> Vec<Prim> {
        let cube = crate::node_graph::nodes::create_cube(1.0);
        let at = |x: f32| Placement::One(Mat4::from_translation(Vec3::new(x, 0.0, 0.0)));
        let camera = Prim {
            camera: Some(Arc::new(crate::core::camera::Camera::default())),
            place: at(0.0),
            ..Prim::new("/shot/cam".into(), MeshData::default())
        };
        vec![
            Prim {
                place: at(1.0),
                ..Prim::new("/set/table/top".into(), cube.clone())
            },
            Prim {
                place: at(2.0),
                ..Prim::new("/set/table/leg".into(), cube)
            },
            camera,
        ]
    }

    fn named(r: &EvalResult) -> &[Prim] {
        match r {
            EvalResult::Named(p) => p,
            _ => panic!("primitives"),
        }
    }

    /// The form shows the target's values; only what is overridden changes.
    #[test]
    fn a_prim_takes_only_its_overrides() {
        let prims = scene();
        let list = vec!["/set/table/top".to_string()];
        let shown = params(EditKind::Mesh, &Target::Prim(&prims[0]));
        let translate = shown.iter().find(|p| p.name == "translate").unwrap();
        assert_eq!(translate.value, Value::Float3([1.0, 0.0, 0.0]));
        assert!(shown.iter().any(|p| p.name == "subdivisionScheme"));
        // Nothing overridden: nothing changes.
        let none = edit(
            &EvalResult::Named(prims.clone()),
            &list,
            EditKind::Mesh,
            &BTreeMap::new(),
        );
        assert!(named(&none)[0].place.same(&prims[0].place));
        // Rotate overridden: turned where it was, the leg left alone.
        let mut edits = BTreeMap::new();
        edits.insert("rotate".to_string(), serde_json::json!([0.0, 90.0, 0.0]));
        let out = edit(&EvalResult::Named(prims.clone()), &list, EditKind::Mesh, &edits);
        let Placement::One(m) = named(&out)[0].place else {
            panic!()
        };
        assert!((m.w_axis.truncate() - Vec3::X).length() < 1e-5, "it stays where it was");
        assert!((m.transform_vector3(Vec3::Z) - Vec3::X).length() < 1e-5, "and turns");
        assert!(named(&out)[1].place.same(&prims[1].place));
    }

    /// Curves show their type, basis and wrap; overridden, the geometry and
    /// what the Viewer draws take them.
    #[test]
    fn curves_take_their_type_basis_and_wrap() {
        use crate::core::geo_mesh::GeoMesh;
        let mut hair =
            crate::core::geo::Geo::from_points(vec![[0.0; 3], [0.0, 1.0, 0.0], [0.0, 2.0, 0.0], [0.0, 3.0, 0.0]]);
        hair.topology = Topology::curves(vec![4], CurveBasis::Bspline, CurveWrap::Nonperiodic);
        let prims = vec![Prim {
            geo: Some(Arc::new(hair.clone())),
            ..Prim::new("/hair".into(), hair.to_mesh())
        }];
        let shown = params(EditKind::Curves, &Target::Prim(&prims[0]));
        let token = |name: &str| shown.iter().find(|p| p.name == name).map(|p| p.value.clone());
        assert_eq!(token("type"), Some(Value::Token("cubic".into())));
        assert_eq!(token("basis"), Some(Value::Token("bspline".into())));
        assert_eq!(token("wrap"), Some(Value::Token("nonperiodic".into())));
        let mut edits = BTreeMap::new();
        edits.insert("basis".to_string(), serde_json::json!("catmullRom"));
        edits.insert("wrap".to_string(), serde_json::json!("pinned"));
        let out = edit(
            &EvalResult::Named(prims.clone()),
            &["/hair".to_string()],
            EditKind::Curves,
            &edits,
        );
        let p = &named(&out)[0];
        assert!(matches!(
            p.geo.as_deref().unwrap().topology,
            Topology::Curves {
                basis: CurveBasis::CatmullRom,
                wrap: CurveWrap::Pinned,
                ..
            }
        ));
        assert_eq!(
            (p.mesh.curve_basis, p.mesh.curve_wrap),
            (CurveBasis::CatmullRom, CurveWrap::Pinned)
        );
        // Linear: no basis of its own.
        edits.insert("type".to_string(), serde_json::json!("linear"));
        let out = edit(
            &EvalResult::Named(prims),
            &["/hair".to_string()],
            EditKind::Curves,
            &edits,
        );
        assert_eq!(named(&out)[0].mesh.curve_basis, CurveBasis::Linear);
    }

    /// A group named: what is below it moves with it, as USD composes a
    /// group's transform.
    #[test]
    fn a_group_moves_what_is_below_it() {
        let prims = scene();
        let list = vec!["/set/table".to_string()];
        let (n, first, other) = survey(&prims, &list, EditKind::Xform);
        assert_eq!((n, first, other), (1, Some(EditKind::Xform), 0));
        let mut edits = BTreeMap::new();
        edits.insert("translate".to_string(), serde_json::json!([0.0, 3.0, 0.0]));
        let out = edit(&EvalResult::Named(prims.clone()), &list, EditKind::Xform, &edits);
        for (k, x) in [(0, 1.0), (1, 2.0)] {
            let Placement::One(m) = named(&out)[k].place else {
                panic!()
            };
            assert!((m.w_axis.truncate() - Vec3::new(x, 3.0, 0.0)).length() < 1e-5);
        }
        assert!(named(&out)[2].place.same(&prims[2].place), "the camera is not below it");
    }

    /// A file's RenderSettings prim: its form shows what it authors (USD's
    /// fallbacks elsewhere), and the overrides make an over of it, nothing
    /// else written; it stays the one rendered.
    #[test]
    fn a_files_render_settings_take_their_overrides_as_an_over() {
        use crate::usd_scene::{StageNode, StageTree};
        let node = |path: &str, ty: &str, depth: usize| StageNode {
            path: path.into(),
            type_name: ty.into(),
            depth,
            ..Default::default()
        };
        let mut rs = node("/Render/rendersettings", "RenderSettings", 2);
        rs.authored = vec![("pixelAspectRatio".into(), Value::Float(2.0))];
        let tree = Arc::new(StageTree {
            nodes: vec![node("/Render", "Scope", 1), rs],
            render_settings: Some("/Render/rendersettings".into()),
            ..Default::default()
        });
        let card = Prim {
            stage: Some(tree),
            ..Prim::new("/card".into(), crate::node_graph::nodes::create_cube(1.0))
        };
        let list = vec!["/Render/rendersettings".to_string()];
        let prims = vec![card];
        assert_eq!(survey(&prims, &list, EditKind::Xform).1, Some(EditKind::RenderSettings));
        let shown = params(EditKind::RenderSettings, &targets(&prims, &list)[0]);
        let at = |name: &str| shown.iter().find(|p| p.name == name).unwrap().value.clone();
        assert_eq!(at("pixelAspectRatio"), Value::Float(2.0), "the file's");
        assert_eq!(at("resolution"), Value::Float2([2048.0, 1080.0]), "USD's fallback");
        assert!(shown.iter().all(|p| p.group != "Xform"), "not placed");
        let mut edits = BTreeMap::new();
        edits.insert("resolution".to_string(), serde_json::json!([1280.0, 720.0]));
        edits.insert("camera".to_string(), serde_json::json!("/cam"));
        let out = edit(&EvalResult::Named(prims), &list, EditKind::RenderSettings, &edits);
        let over = named(&out)
            .iter()
            .find(|p| p.path == "/Render/rendersettings")
            .expect("an over of it");
        let r = over.render.as_deref().unwrap();
        assert!(r.over && !r.primary);
        assert_eq!(
            r.attrs.get("resolution").unwrap().line("resolution"),
            "uniform int2 resolution = (1280, 720)"
        );
        assert!(!r.attrs.contains_key("pixelAspectRatio"), "only the overrides");
        assert_eq!(r.rels.get("camera").unwrap().targets, vec!["/cam".to_string()]);
        assert_eq!(
            crate::core::render::layer_settings(named(&out)),
            Some("/Render/rendersettings".into())
        );
    }

    /// A camera's lens: the overridden values, the rest as they were; a
    /// mesh named with the Camera type is left as it is.
    #[test]
    fn a_camera_takes_its_lens_overrides() {
        let prims = scene();
        let list = vec!["/shot/cam".to_string(), "/set/table/top".to_string()];
        let (_, first, other) = survey(&prims, &list, EditKind::Camera);
        assert_eq!((first, other), (Some(EditKind::Mesh), 1), "the mesh is of another kind");
        let mut edits = BTreeMap::new();
        edits.insert("focalLength".to_string(), serde_json::json!(85.0));
        edits.insert("visibility".to_string(), serde_json::json!("invisible"));
        let out = edit(&EvalResult::Named(prims.clone()), &list, EditKind::Camera, &edits);
        let cam = named(&out)[2].camera.as_deref().unwrap();
        assert_eq!(cam.focal_length, 85.0);
        assert_eq!(
            cam.horizontal_aperture,
            crate::core::camera::Camera::default().horizontal_aperture
        );
        assert!(named(&out)[0].camera.is_none());
        assert!(!named(&out)[0].props.contains_key("visibility"), "nor made invisible");
        // Made in Imago: written whole from its values, not as properties too.
        assert!(!named(&out)[2].props.contains_key("focalLength"));
        assert_eq!(
            named(&out)[2].props.get("visibility"),
            Some(&Value::Token("invisible".into()))
        );
        // An override kept as plain JSON reads back as its type.
        assert_eq!(
            typed(&Value::Float(0.0), &plain(&Value::Float(85.0))),
            Value::Float(85.0)
        );
    }
}
