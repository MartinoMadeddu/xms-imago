//! Set Attribute: one value per primitive, set by the scene graph.
//!
//! An operator holds rows; each sets one value on the primitives its Prims
//! Location tab names. A row's context says what the value is in USD:
//!
//! | Context    | Written as                                       |
//! |------------|--------------------------------------------------|
//! | `Object`   | `primvars:name`, `interpolation = "constant"`    |
//! | `Property` | the prim's own attribute or metadata, by name    |
//!
//! Where a value lands in Imago:
//!
//! - On a geometry primitive (a `Prim`), an Object value is an Object
//!   attribute of its `Geo`, as one read from a file is: the Primitive
//!   Inspector and ICE see it, and Write USD writes it as any changed
//!   attribute. A Property value is kept in the primitive's `props`, apart
//!   from `purpose`, which is the primitive's purpose, and `active = off`,
//!   which takes the primitive out of the scene (written as deactivated).
//! - On a prim of a stage without geometry (an Xform, a camera, a light, a
//!   material), values are opinions in its stage tree (`StageTree::opinions`),
//!   written as `over`s on that path. `active = off` there takes what is
//!   below it out of the scene too, and `visibility = invisible` hides it in
//!   the Viewer, as USD inherits them.
//!
//! Unlike the other operators' Primitive Locations, a location here names
//! the prim at that path, not everything below it (`locations::names`): a
//! value on a group is one opinion on the group. `/set/**` names `/set` and
//! everything below it, `/set/*` its children.
//!
//! Values per point, face or corner are not set here: they are ICE's.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::core::geo::{Attr, Column, Context, Geo, Role};
use crate::core::locations;
use crate::types::{EvalResult, Prim, Purpose};

// ── Values ──────────────────────────────────────────────────────────────────

/// The USD type of a value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ValueType {
    Bool, Int, #[default] Float, Double, String, Token, Float2, Float3, Float4, Color3f, Asset,
}

impl ValueType {
    pub const ALL: [ValueType; 11] = [
        ValueType::Bool, ValueType::Int, ValueType::Float, ValueType::Double, ValueType::String, ValueType::Token,
        ValueType::Float2, ValueType::Float3, ValueType::Float4, ValueType::Color3f, ValueType::Asset,
    ];
    /// The types an Object value can have: those a `Geo` column holds.
    pub const OBJECT: [ValueType; 8] = [
        ValueType::Bool, ValueType::Int, ValueType::Float, ValueType::Token,
        ValueType::Float2, ValueType::Float3, ValueType::Float4, ValueType::Color3f,
    ];

    /// The type's name in USD.
    pub fn usd(self) -> &'static str {
        match self {
            ValueType::Bool => "bool", ValueType::Int => "int", ValueType::Float => "float", ValueType::Double => "double",
            ValueType::String => "string", ValueType::Token => "token", ValueType::Float2 => "float2",
            ValueType::Float3 => "float3", ValueType::Float4 => "float4", ValueType::Color3f => "color3f",
            ValueType::Asset => "asset",
        }
    }

    /// A value of this type, kept from `from` where it can be.
    pub fn convert(self, from: &Value) -> Value {
        let f = from.floats();
        let at = |i: usize| f.get(i).copied().unwrap_or(0.0);
        let text = from.text();
        match self {
            ValueType::Bool => Value::Bool(at(0) != 0.0),
            ValueType::Int => Value::Int(at(0) as i32),
            ValueType::Float => Value::Float(at(0)),
            ValueType::Double => Value::Double(at(0) as f64),
            ValueType::String => Value::String(text),
            ValueType::Token => Value::Token(text),
            ValueType::Asset => Value::Asset(text),
            ValueType::Float2 => Value::Float2([at(0), at(1)]),
            ValueType::Float3 => Value::Float3([at(0), at(1), at(2)]),
            ValueType::Float4 => Value::Float4([at(0), at(1), at(2), at(3)]),
            ValueType::Color3f => Value::Color3f([at(0), at(1), at(2)]),
        }
    }
}

/// One value, with its type.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Value {
    Bool(bool),
    Int(i32),
    Float(f32),
    Double(f64),
    String(String),
    Token(String),
    Float2([f32; 2]),
    Float3([f32; 3]),
    Float4([f32; 4]),
    Color3f([f32; 3]),
    /// A file path (`@path@` in USD).
    Asset(String),
}

impl Default for Value {
    fn default() -> Self { Value::Float(0.0) }
}

impl Value {
    pub fn kind(&self) -> ValueType {
        match self {
            Value::Bool(_) => ValueType::Bool, Value::Int(_) => ValueType::Int, Value::Float(_) => ValueType::Float,
            Value::Double(_) => ValueType::Double, Value::String(_) => ValueType::String, Value::Token(_) => ValueType::Token,
            Value::Float2(_) => ValueType::Float2, Value::Float3(_) => ValueType::Float3, Value::Float4(_) => ValueType::Float4,
            Value::Color3f(_) => ValueType::Color3f, Value::Asset(_) => ValueType::Asset,
        }
    }

    /// Its numbers, for keeping what can be kept when the type changes.
    fn floats(&self) -> Vec<f32> {
        match self {
            Value::Bool(b) => vec![*b as u8 as f32],
            Value::Int(i) => vec![*i as f32],
            Value::Float(x) => vec![*x],
            Value::Double(x) => vec![*x as f32],
            Value::Float2(v) => v.to_vec(),
            Value::Float3(v) | Value::Color3f(v) => v.to_vec(),
            Value::Float4(v) => v.to_vec(),
            Value::String(s) | Value::Token(s) | Value::Asset(s) => s.trim().parse::<f32>().map(|x| vec![x]).unwrap_or_default(),
        }
    }

    /// Its text: the string itself, or the value written out.
    fn text(&self) -> String {
        match self {
            Value::String(s) | Value::Token(s) | Value::Asset(s) => s.clone(),
            other => other.usda(),
        }
    }

    /// The value as `.usda` writes it.
    pub fn usda(&self) -> String {
        let num = |x: f32| if x.is_finite() { format!("{x}") } else { "0".into() };
        let tuple = |v: &[f32]| format!("({})", v.iter().map(|x| num(*x)).collect::<Vec<_>>().join(", "));
        let quoted = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
        match self {
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(x) => num(*x),
            Value::Double(x) => if x.is_finite() { format!("{x}") } else { "0".into() },
            Value::String(s) | Value::Token(s) => quoted(s),
            Value::Float2(v) => tuple(v),
            Value::Float3(v) | Value::Color3f(v) => tuple(v),
            Value::Float4(v) => tuple(v),
            Value::Asset(s) => format!("@{}@", s.replace('@', "")),
        }
    }

    /// The token, when the value is one (or a string).
    pub fn token(&self) -> Option<&str> {
        match self { Value::Token(s) | Value::String(s) => Some(s.trim()), _ => None }
    }

    /// As an Object attribute of a `Geo`: one value. `None` for the types a
    /// column does not hold (double, string, asset).
    pub fn attr(&self) -> Option<Attr> {
        let one = |c: Column| Some(Attr::new(c, Role::None));
        match self {
            Value::Bool(b) => one(Column::Bool(Arc::new(vec![*b]))),
            Value::Int(i) => one(Column::Int(Arc::new(vec![*i]))),
            Value::Float(x) => one(Column::Float(Arc::new(vec![*x]))),
            Value::Token(s) => one(Column::Token(Arc::new(vec![Arc::from(s.as_str())]))),
            Value::Float2(v) => one(Column::Vec2(Arc::new(vec![*v]))),
            Value::Float3(v) => one(Column::Vec3(Arc::new(vec![*v]))),
            Value::Float4(v) => one(Column::Vec4(Arc::new(vec![*v]))),
            Value::Color3f(v) => Some(Attr::new(Column::Vec3(Arc::new(vec![*v])), Role::Color)),
            Value::Double(_) | Value::String(_) | Value::Asset(_) => None,
        }
    }
}

// ── Rows ────────────────────────────────────────────────────────────────────

/// What a row's value is in USD.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SetContext {
    /// A constant primvar: data for rendering, read by shaders, inherited by
    /// the prims below a group that do not set their own.
    #[default]
    Object,
    /// The prim's own attribute or metadata: what the prim is and how it is
    /// used. Not seen by shaders, not inherited.
    Property,
}

/// One row of a Set Attribute operator: one value.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SetRow {
    pub context: SetContext,
    pub name:    String,
    pub value:   Value,
}

/// How a Property is written: as an attribute (`uniform` or not), or as
/// metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Written { Attribute, Uniform, Metadata }

/// A name the name field offers, with its type and, where USD fixes them,
/// its values.
pub struct Preset {
    pub name:    &'static str,
    pub ty:      ValueType,
    pub tokens:  &'static [&'static str],
    pub written: Written,
    pub hint:    &'static str,
}

/// The common Properties.
pub const PROPERTY_PRESETS: &[Preset] = &[
    Preset { name: "visibility", ty: ValueType::Token, tokens: &["inherited", "invisible"], written: Written::Attribute,
        hint: "Invisible: not drawn, and nothing below it is. Inherited: as its parent" },
    Preset { name: "purpose", ty: ValueType::Token, tokens: &["default", "render", "proxy", "guide"], written: Written::Uniform,
        hint: "What drawing the prim is for. The Scene Explorer's purpose toggles show or hide by it" },
    Preset { name: "active", ty: ValueType::Bool, tokens: &[], written: Written::Metadata,
        hint: "Off: the prim and everything below it are not part of the scene. Written as deactivated" },
    Preset { name: "kind", ty: ValueType::Token, tokens: &["component", "subcomponent", "group", "assembly"], written: Written::Metadata,
        hint: "Its place in the model hierarchy: an asset is a component, groups of assets are groups and assemblies" },
    Preset { name: "instanceable", ty: ValueType::Bool, tokens: &[], written: Written::Metadata,
        hint: "On: prims with the same composed content share it, as native instances" },
    Preset { name: "subdivisionScheme", ty: ValueType::Token, tokens: &["catmullClark", "loop", "bilinear", "none"], written: Written::Uniform,
        hint: "On meshes: the subdivision the renderer does. Edit Poly's Subdivide changes the mesh instead" },
    Preset { name: "doubleSided", ty: ValueType::Bool, tokens: &[], written: Written::Uniform,
        hint: "On: both sides of each face are drawn" },
];

/// The common Object values.
pub const OBJECT_PRESETS: &[Preset] = &[
    Preset { name: "displayColor", ty: ValueType::Color3f, tokens: &[], written: Written::Attribute,
        hint: "The colour a renderer uses without a material, and a shader can read" },
    Preset { name: "displayOpacity", ty: ValueType::Float, tokens: &[], written: Written::Attribute,
        hint: "The opacity a renderer uses without a material" },
];

/// The preset of a name, in a context.
pub fn preset(context: SetContext, name: &str) -> Option<&'static Preset> {
    let list = match context { SetContext::Object => OBJECT_PRESETS, SetContext::Property => PROPERTY_PRESETS };
    list.iter().find(|p| p.name == name.trim())
}

/// How a Property of this name is written. Names it does not know are
/// attributes.
pub fn written(name: &str) -> Written {
    preset(SetContext::Property, name).map_or(Written::Attribute, |p| p.written)
}

impl SetRow {
    /// A row for a preset, with its first value.
    pub fn from_preset(context: SetContext, p: &Preset) -> SetRow {
        let value = match (p.ty, p.tokens.first()) {
            (ValueType::Token, Some(t)) => Value::Token(t.to_string()),
            (ValueType::Bool, _) => Value::Bool(p.name != "active"),
            (ValueType::Color3f, _) => Value::Color3f([0.8, 0.8, 0.8]),
            (ValueType::Float, _) if p.name == "displayOpacity" => Value::Float(1.0),
            (ty, _) => ty.convert(&Value::default()),
        };
        SetRow { context, name: p.name.to_string(), value }
    }

    fn name(&self) -> &str { self.name.trim() }

    /// Whether the row sets anything: a name, and for Object a type a column holds.
    pub fn is_usable(&self) -> bool {
        !self.name().is_empty() && (self.context == SetContext::Property || self.value.attr().is_some())
    }

    /// Whether it takes what it is set on out of the scene.
    fn deactivates(&self) -> bool {
        self.context == SetContext::Property && self.name() == "active" && self.value == Value::Bool(false)
    }
}

// ── Opinions on prims without geometry ──────────────────────────────────────

/// The values set on one path of a stage that is not a geometry primitive.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Opinions {
    /// Object values: constant primvars.
    pub primvars: BTreeMap<String, Value>,
    /// Property values: attributes and metadata by name.
    pub props:    BTreeMap<String, Value>,
}

impl Opinions {
    pub fn is_empty(&self) -> bool { self.primvars.is_empty() && self.props.is_empty() }
}

// ── The operator ────────────────────────────────────────────────────────────

/// What a Set Attribute operator puts out: the primitives coming in with
/// its rows set on those `locations` names (or, with no locations, on the
/// picked primitives, or every one).
pub fn set_attributes(input: &EvalResult, rows: &[SetRow], locations: &[String]) -> EvalResult {
    let EvalResult::Named(prims) = input else { return input.clone() };
    let rows: Vec<&SetRow> = rows.iter().filter(|r| r.is_usable()).collect();
    if rows.is_empty() { return input.clone(); }

    let named = locations::is_set(locations);
    let any_picked = prims.iter().any(|p| p.picked);
    let targeted = |p: &Prim| if named { locations::any_names(locations, &p.path) } else { !any_picked || p.picked };

    // Geometry primitives.
    let mut out: Vec<Prim> = Vec::with_capacity(prims.len());
    for p in prims {
        if !targeted(p) { out.push(p.clone()); continue; }
        if let Some(q) = set_on_prim(p, &rows) { out.push(q); }
    }
    if !named { return EvalResult::Named(out); }

    // Prims of a stage without geometry: opinions in its tree. Each stage
    // once; every primitive of it then shares the new tree.
    let present: std::collections::HashSet<String> = prims.iter().map(|p| p.path.clone()).collect();
    let mut trees: Vec<Arc<crate::usd_scene::StageTree>> = vec![];
    for p in &out {
        if let Some(t) = &p.stage { if !trees.iter().any(|x| Arc::ptr_eq(x, t)) { trees.push(t.clone()); } }
    }
    let mut gone: Vec<String> = vec![];
    for old in trees {
        let mut tree = (*old).clone();
        let mut changed = false;
        for node in &old.nodes {
            if present.contains(&node.path) || !locations::any_names(locations, &node.path) { continue; }
            let o = tree.opinions.entry(node.path.clone()).or_default();
            for r in &rows {
                match r.context {
                    SetContext::Object => { o.primvars.insert(r.name().to_string(), r.value.clone()); }
                    SetContext::Property => { o.props.insert(r.name().to_string(), r.value.clone()); }
                }
                if r.deactivates() { gone.push(node.path.clone()); }
            }
            changed = true;
        }
        if !changed { continue; }
        let new = Arc::new(tree);
        for p in out.iter_mut() {
            if p.stage.as_ref().is_some_and(|t| Arc::ptr_eq(t, &old)) { p.stage = Some(new.clone()); }
        }
    }
    // What is below a deactivated prim is not part of the scene.
    if !gone.is_empty() {
        out.retain(|p| !gone.iter().any(|g| is_below(&p.path, g)));
    }
    EvalResult::Named(out)
}

/// One geometry primitive with the rows set on it. `None` when a row takes
/// it out of the scene.
fn set_on_prim(p: &Prim, rows: &[&SetRow]) -> Option<Prim> {
    let mut q = p.clone();
    let mut geo: Option<Geo> = None;
    for r in rows {
        match r.context {
            SetContext::Object => {
                let Some(attr) = r.value.attr() else { continue };
                // Geometry merged from several prims has no `Geo` left: one
                // from its mesh, which is then written whole.
                let g = geo.get_or_insert_with(|| p.geo.as_deref().cloned().unwrap_or_else(|| Geo::from_mesh(&p.mesh)));
                g.set(Context::Object, r.name(), attr);
            }
            SetContext::Property => match r.name() {
                "active" => if r.deactivates() { return None; },
                "purpose" => if let Some(t) = r.value.token() { q.purpose = purpose(t, p.purpose); },
                name => { Arc::make_mut(&mut q.props).insert(name.to_string(), r.value.clone()); }
            },
        }
    }
    if let Some(g) = geo {
        q.mesh = Arc::new(g.to_mesh());
        q.geo = Some(Arc::new(g));
    }
    Some(q)
}

/// A purpose from its token. A render prim stays one with its proxy.
fn purpose(token: &str, was: Purpose) -> Purpose {
    match token {
        "render" => match was { Purpose::Render { has_proxy } => Purpose::Render { has_proxy }, _ => Purpose::Render { has_proxy: false } },
        "proxy" => Purpose::Proxy,
        "guide" => Purpose::Guide,
        _ => Purpose::Default,
    }
}

/// `path` is `parent` or below it.
fn is_below(path: &str, parent: &str) -> bool {
    let parent = parent.trim_end_matches('/');
    path == parent || path.strip_prefix(parent).is_some_and(|rest| rest.starts_with('/'))
}

/// Whether a primitive is invisible: set so on itself, or on a prim above
/// it in its stage (USD inherits `invisible`).
pub fn invisible(p: &Prim) -> bool {
    let off = |v: Option<&Value>| v.and_then(Value::token) == Some("invisible");
    if off(p.props.get("visibility")) { return true; }
    p.stage.as_ref().is_some_and(|t| t.opinions.iter()
        .any(|(path, o)| path != &p.path && is_below(&p.path, path) && off(o.props.get("visibility"))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MeshData;
    use crate::usd_scene::{StageNode, StageTree};

    fn node(path: &str, ty: &str) -> StageNode {
        StageNode { path: path.into(), name: path.rsplit('/').next().unwrap().into(), type_name: ty.into(),
            depth: path.matches('/').count(), ..Default::default() }
    }

    /// A stage `/car` (Xform) with `/car/body` and `/car/wheels/w1` (meshes),
    /// and a camera, as primitives.
    fn car() -> Vec<Prim> {
        let tree = Arc::new(StageTree { nodes: vec![
            node("/car", "Xform"), node("/car/body", "Mesh"), node("/car/wheels", "Xform"),
            node("/car/wheels/w1", "Mesh"), node("/cam", "Camera"),
        ], ..Default::default() });
        ["/car/body", "/car/wheels/w1"].iter().map(|path| {
            let mesh = crate::node_graph::nodes::create_cube(1.0);
            let geo = Arc::new(Geo::from_mesh(&mesh));
            Prim { geo: Some(geo), stage: Some(tree.clone()), ..Prim::new(path.to_string(), mesh) }
        }).collect()
    }

    fn row(context: SetContext, name: &str, value: Value) -> SetRow { SetRow { context, name: name.into(), value } }
    fn locs(l: &[&str]) -> Vec<String> { l.iter().map(|s| s.to_string()).collect() }
    fn prims(r: EvalResult) -> Vec<Prim> { match r { EvalResult::Named(p) => p, _ => panic!("primitives") } }

    #[test]
    fn an_object_value_is_an_object_attribute_of_the_named_prims() {
        let colour = row(SetContext::Object, "displayColor", Value::Color3f([1.0, 0.0, 0.0]));
        let out = prims(set_attributes(&EvalResult::Named(car()), &[colour], &locs(&["/car/body"])));
        let body = out[0].geo.as_ref().unwrap();
        let attr = body.attr(Context::Object, "displayColor").expect("set on /car/body");
        assert_eq!((attr.column.vec3s().unwrap(), attr.role), (&[[1.0, 0.0, 0.0]][..], Role::Color));
        assert!(out[0].mesh.primvars.iter().any(|v| v.name == "displayColor"), "the mesh view follows");
        assert!(out[1].geo.as_ref().unwrap().attr(Context::Object, "displayColor").is_none(), "/car/wheels/w1 not named");
    }

    #[test]
    fn a_location_names_its_prim_not_what_is_below() {
        let kind = row(SetContext::Property, "kind", Value::Token("component".into()));
        let out = prims(set_attributes(&EvalResult::Named(car()), &[kind.clone()], &locs(&["/car"])));
        // On the group, as an opinion in the stage: not on each mesh.
        assert!(out.iter().all(|p| p.props.is_empty()));
        let tree = out[0].stage.as_ref().unwrap();
        assert_eq!(tree.opinions["/car"].props["kind"], Value::Token("component".into()));
        assert!(Arc::ptr_eq(tree, out[1].stage.as_ref().unwrap()), "every prim of the stage shares the new tree");
        // `**` names everything below.
        let out = prims(set_attributes(&EvalResult::Named(car()), &[kind.clone()], &locs(&["/car/**"])));
        assert!(out.iter().all(|p| p.props.contains_key("kind")));
        let tree = out[0].stage.as_ref().unwrap();
        assert!(tree.opinions.contains_key("/car/wheels") && tree.opinions.contains_key("/car"), "groups too, /car itself among them");
        // `*` its children only.
        let kind = row(SetContext::Property, "kind", Value::Token("group".into()));
        let out = prims(set_attributes(&EvalResult::Named(car()), &[kind], &locs(&["/car/*"])));
        assert!(out[0].props.contains_key("kind") && !out[1].props.contains_key("kind"));
        let tree = out[0].stage.as_ref().unwrap();
        assert!(tree.opinions.contains_key("/car/wheels") && !tree.opinions.contains_key("/car"));
    }

    #[test]
    fn properties_purpose_active_and_visibility() {
        let set = |r: SetRow, l: &[&str]| prims(set_attributes(&EvalResult::Named(car()), &[r], &locs(l)));
        // Purpose is the primitive's purpose.
        let out = set(row(SetContext::Property, "purpose", Value::Token("guide".into())), &["/car/body"]);
        assert_eq!(out[0].purpose, Purpose::Guide);
        assert!(out[0].props.is_empty());
        // Active off takes the prim out, or what is below a group.
        let off = row(SetContext::Property, "active", Value::Bool(false));
        assert_eq!(set(off.clone(), &["/car/body"]).len(), 1);
        let out = set(off, &["/car/wheels"]);
        assert_eq!(out.iter().map(|p| p.path.as_str()).collect::<Vec<_>>(), vec!["/car/body"]);
        assert_eq!(out[0].stage.as_ref().unwrap().opinions["/car/wheels"].props["active"], Value::Bool(false));
        // Invisible on a group hides what is below it.
        let out = set(row(SetContext::Property, "visibility", Value::Token("invisible".into())), &["/car/wheels"]);
        assert!(!invisible(&out[0]) && invisible(&out[1]));
        let out = set(row(SetContext::Property, "visibility", Value::Token("invisible".into())), &["/car/body"]);
        assert!(invisible(&out[0]) && !invisible(&out[1]));
    }

    #[test]
    fn without_locations_every_prim_or_the_picked_ones() {
        let r = row(SetContext::Property, "doubleSided", Value::Bool(true));
        let out = prims(set_attributes(&EvalResult::Named(car()), &[r.clone()], &[]));
        assert!(out.iter().all(|p| p.props.get("doubleSided") == Some(&Value::Bool(true))));
        assert!(out[0].stage.as_ref().unwrap().opinions.is_empty(), "no locations: geometry only");
        let mut input = car();
        input[1].picked = true;
        let out = prims(set_attributes(&EvalResult::Named(input), &[r], &[]));
        assert!(!out[0].props.contains_key("doubleSided") && out[1].props.contains_key("doubleSided"));
    }

    #[test]
    fn rows_that_say_nothing_change_nothing() {
        let input = EvalResult::Named(car());
        let blank = row(SetContext::Property, "  ", Value::Bool(true));
        // A string has no column: not an Object value.
        let string = row(SetContext::Object, "note", Value::String("x".into()));
        let out = prims(set_attributes(&input, &[blank, string], &locs(&["/car/body"])));
        assert!(Arc::ptr_eq(out[0].geo.as_ref().unwrap(), car_geo(&input)));
        let mesh = EvalResult::Single(MeshData::default());
        assert!(matches!(set_attributes(&mesh, &[row(SetContext::Object, "a", Value::Float(1.0))], &[]), EvalResult::Single(_)));
    }

    fn car_geo(r: &EvalResult) -> &Arc<Geo> {
        match r { EvalResult::Named(p) => p[0].geo.as_ref().unwrap(), _ => panic!() }
    }

    #[test]
    fn values_convert_and_write() {
        assert_eq!(ValueType::Float3.convert(&Value::Float(2.0)), Value::Float3([2.0, 0.0, 0.0]));
        assert_eq!(ValueType::Token.convert(&Value::Int(3)), Value::Token("3".into()));
        assert_eq!(ValueType::Int.convert(&Value::String(" 7 ".into())), Value::Int(7));
        assert_eq!(Value::Color3f([1.0, 0.5, 0.0]).usda(), "(1, 0.5, 0)");
        assert_eq!(Value::Token("a\"b".into()).usda(), "\"a\\\"b\"");
        assert_eq!(Value::Asset("tex/a.png".into()).usda(), "@tex/a.png@");
        assert_eq!(Value::Bool(false).usda(), "false");
        assert_eq!(written("kind"), Written::Metadata);
        assert_eq!(written("subdivisionScheme"), Written::Uniform);
        assert_eq!(written("userProperties:assetId"), Written::Attribute);
    }
}
