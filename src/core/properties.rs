//! Properties: the values a prim holds beyond its geometry, as Imago keeps
//! them, and how what is set on a prim reaches the prims below it.
//!
//! - `Value` and `ValueType`: one value of a USD attribute or metadata, by
//!   type. An Object value (a constant primvar) becomes an Object attribute
//!   of a primitive's `Geo` (`Value::attr`); a Property value is kept in the
//!   primitive's `props`.
//! - `SetContext`: whether a value is an Object value (a constant primvar)
//!   or a Property (the prim's own attribute or metadata).
//! - The properties Imago knows (`PROPERTY_PRESETS`, `OBJECT_PRESETS`): their
//!   types, their values where USD fixes them, and how each is written
//!   (`written`: an attribute, `uniform`, or metadata).
//! - `Opinions`: values on a prim that is not a geometry primitive (a group,
//!   a camera, a light, a material, an Xform Imago made), kept in its
//!   stage's tree (`StageTree::opinions`) or carried by the primitives below
//!   it (`Prim::above`).
//! - Inheritance, in Imago as in USD: `visibility` invisible hides what is
//!   below (`invisible`), `purpose` comes from the nearest prim that has one
//!   (`purpose_from`, `purpose_of`), Object values reach the primitives below
//!   that have no attribute of that name (`above`, `inherited_primvars`).
//!
//! Set Attribute (`core::set_attribute`) is the operator that sets them;
//! Load USD reads what a file authors as the same values; Write USD writes
//! them; the Viewer, the Scene Explorer and the Primitive Inspector read the
//! inheritance from here.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::core::geo::{Attr, Column, Role};
use crate::types::{Prim, Purpose};

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
    /// The value with the variables in its text put in (`$JOB/tex/sky.exr`,
    /// `$OPNAME`); `keep_frames`: the frame's left for a renderer (`$F4`).
    /// Numbers as they are.
    pub fn with_variables_put_in(&self, keep_frames: bool) -> Value {
        let put = |s: &str| if keep_frames { crate::core::vars::expand_keeping_frames(s) } else { crate::core::vars::expand(s, &[]) };
        match self {
            Value::String(s) if crate::core::vars::has_vars(s) => Value::String(put(s)),
            Value::Token(s) if crate::core::vars::has_vars(s) => Value::Token(put(s)),
            Value::Asset(s) if crate::core::vars::has_vars(s) => Value::Asset(put(s.trim())),
            v => v.clone(),
        }
    }

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

    /// The value as the Primitive Inspector shows it: text as it is,
    /// numbers as `.usda` writes them.
    pub fn display(&self) -> String { self.text() }

    /// Its numbers, for the types that are numbers.
    pub fn numbers(&self) -> Option<Vec<f32>> {
        match self {
            Value::String(_) | Value::Token(_) | Value::Asset(_) => None,
            other => Some(other.floats()),
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


// ── Contexts and the properties Imago knows ─────────────────────────────────

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
    Preset { name: "model:drawMode", ty: ValueType::Token, tokens: &["default", "bounds", "origin", "cards", "inherited"], written: Written::Uniform,
        hint: "How viewports draw the prim and what is below it: as geometry, as its bounding box, as axes at its origin. The Viewer draws cards as bounds. Set with model:applyDrawMode on, on a model (kind component)" },
    Preset { name: "model:applyDrawMode", ty: ValueType::Bool, tokens: &[], written: Written::Uniform,
        hint: "On: viewports draw this model as its model:drawMode says" },
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

    /// `other` over these: its values win.
    pub fn merge(&mut self, other: &Opinions) {
        self.primvars.extend(other.primvars.iter().map(|(k, v)| (k.clone(), v.clone())));
        self.props.extend(other.props.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
}

// ── Inheritance ─────────────────────────────────────────────────────────────

/// A purpose from its token. A render prim stays one with its proxy.
pub fn purpose(token: &str, was: Purpose) -> Purpose {
    match token {
        "render" => match was { Purpose::Render { has_proxy } => Purpose::Render { has_proxy }, _ => Purpose::Render { has_proxy: false } },
        "proxy" => Purpose::Proxy,
        "guide" => Purpose::Guide,
        _ => Purpose::Default,
    }
}

/// `path` is `parent` or below it.
pub fn is_below(path: &str, parent: &str) -> bool {
    let parent = parent.trim_end_matches('/');
    path == parent || path.strip_prefix(parent).is_some_and(|rest| rest.starts_with('/'))
}

/// The prims above a path, the root first: `/a/b/c` has `/a` and `/a/b`.
pub fn parents(path: &str) -> Vec<String> {
    let names: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    (1..names.len()).map(|n| format!("/{}", names[..n].join("/"))).collect()
}

/// The values set on the prims above a primitive, the furthest first: on
/// prims of its stage, and on the Xforms Imago made above it.
pub fn above(p: &Prim) -> Vec<(&str, &Opinions)> {
    let mut v: Vec<(&str, &Opinions)> = p.above.iter().map(|(k, o)| (k.as_str(), o)).collect();
    if let Some(t) = &p.stage {
        v.extend(t.opinions.iter()
            .filter(|(path, _)| **path != p.path && is_below(&p.path, path))
            .map(|(k, o)| (k.as_str(), o)));
    }
    v.sort_by_key(|(k, _)| k.matches('/').count());
    v
}

/// Whether a primitive is invisible: set so on itself, or on a prim above
/// it (USD inherits `invisible`).
pub fn invisible(p: &Prim) -> bool {
    let off = |v: Option<&Value>| v.and_then(Value::token) == Some("invisible");
    off(p.props.get("visibility")) || above(p).iter().any(|(_, o)| off(o.props.get("visibility")))
}

/// How the scene says a primitive is drawn: USD's `model:drawMode`, on it
/// or on the nearest prim above it that has one (`inherited` passes on to
/// the one above). `None`: nothing says.
pub fn draw_mode(p: &Prim) -> Option<crate::types::DrawMode> {
    let mode = |o: &BTreeMap<String, Value>| o.get("model:drawMode").and_then(Value::token).and_then(crate::types::DrawMode::from_token);
    if let Some(m) = mode(p.props.as_ref()) { return Some(m); }
    above(p).iter().rev().find_map(|(_, o)| mode(&o.props))
}

/// Where a primitive's purpose comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PurposeFrom {
    /// Set on the primitive itself: by Set Attribute, or made so in Imago.
    Set,
    /// Authored on the primitive in its file.
    File,
    /// Inherited from the prim above at this path: set there by Set
    /// Attribute (`true`), or authored there in the file (`false`).
    Above(String, bool),
    /// Nothing says: the default.
    Unset,
}

/// Where a primitive's purpose comes from. As in USD, a purpose on the
/// primitive itself wins; else the nearest prim above it with one, whether
/// the file authors it there or Set Attribute set it there (on the same
/// prim, Set Attribute's is the stronger opinion). A purpose a loaded
/// primitive only inherited from its file does not hide a nearer one.
pub fn purpose_from(p: &Prim) -> PurposeFrom {
    let set_here = p.source.as_ref().map_or(p.purpose != Purpose::Default, |s| s.purpose != p.purpose);
    if set_here { return PurposeFrom::Set; }
    let tree = p.stage.as_deref();
    let authored = |path: &str| tree.is_some_and(|t| t.authored(path, "purpose").is_some());
    if authored(&p.path) { return PurposeFrom::File; }
    let depth = |path: &str| path.matches('/').count();
    let set_above = above(p).into_iter().rev()
        .find(|(_, o)| o.props.get("purpose").and_then(Value::token).is_some())
        .map(|(path, _)| path.to_string());
    let file_above = if p.purpose != Purpose::Default { parents(&p.path).into_iter().rev().find(|a| authored(a)) } else { None };
    match (set_above, file_above) {
        (Some(s), Some(f)) if depth(&f) > depth(&s) => PurposeFrom::Above(f, false),
        (Some(s), _) => PurposeFrom::Above(s, true),
        (None, Some(f)) => PurposeFrom::Above(f, false),
        (None, None) => PurposeFrom::Unset,
    }
}

/// The purpose a primitive is drawn with (see `purpose_from`).
pub fn purpose_of(p: &Prim) -> Purpose {
    // Most primitives: nothing above them set a purpose.
    if !above(p).iter().any(|(_, o)| o.props.contains_key("purpose")) { return p.purpose; }
    match purpose_from(p) {
        PurposeFrom::Above(path, true) => above(p).into_iter()
            .find(|(k, _)| *k == path)
            .and_then(|(_, o)| o.props.get("purpose").and_then(Value::token).map(|t| purpose(t, Purpose::Default)))
            .unwrap_or(p.purpose),
        _ => p.purpose,
    }
}

/// The Object values a primitive inherits from the prims above it, the
/// nearest winning, where it has no attribute of that name itself.
pub fn inherited_primvars(p: &Prim) -> Vec<(String, Value)> {
    let mut values: BTreeMap<String, Value> = BTreeMap::new();
    for (_, o) in above(p) {
        for (k, v) in &o.primvars { values.insert(k.clone(), v.clone()); }
    }
    let own = |name: &str| p.geo.as_ref().map_or(false, |g| g.find(name).is_some()) || p.mesh.primvars.iter().any(|v| v.name == name);
    values.into_iter().filter(|(k, _)| !own(k)).collect()
}
