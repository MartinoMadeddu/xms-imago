//! Properties: the values a prim holds beyond its geometry, as Imago keeps
//! them, and how what is set on a prim reaches the prims below it.
//!
//! - `Value` and `ValueType`: one value of a USD attribute or metadata, by
//!   type. An Object value (a constant primvar) becomes an Object attribute of
//!   a primitive's `Geo` (`Value::attr`); a Property value is kept in the
//!   primitive's `props`.
//! - `SetContext`: whether a value is an Object value (a constant primvar) or a
//!   Property (the prim's own attribute or metadata).
//! - The properties Imago knows (`PROPERTY_PRESETS`, `OBJECT_PRESETS`): their
//!   types, their values where USD fixes them, and how each is written
//!   (`written`: an attribute, `uniform`, or metadata).
//! - `Opinions`: values on a prim that is not a geometry primitive (a group, a
//!   camera, a light, a material, an Xform Imago made), kept in its stage's
//!   tree (`StageTree::opinions`) or carried by the primitives below it
//!   (`Prim::above`).
//! - Inheritance, in Imago as in USD: `visibility` invisible hides what is
//!   below (`invisible`), `purpose` comes from the nearest prim that has one
//!   (`purpose_from`, `purpose_of`), Object values reach the primitives below
//!   that have no attribute of that name (`above`, `inherited_primvars`).
//!
//! Set Attribute (`core::set_attribute`) is the operator that sets them; Load
//! USD reads what a file authors as the same values; Write USD writes them; the
//! Viewer, the Scene Explorer and the Primitive Inspector read the inheritance
//! from here.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::core::geo::{Attr, Column, Role};
use crate::types::{Prim, Purpose};

// Values

/// The USD type of a value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ValueType {
    Bool,
    Int,
    #[default]
    Float,
    Double,
    String,
    Token,
    Float2,
    Float3,
    Float4,
    Color3f,
    Asset,
    IntArray,
    FloatArray,
    Float3Array,
    Color3fArray,
    StringArray,
    TokenArray,
}

impl ValueType {
    /// The arrays a value can be, as USD's `int[]`, `float[]`…
    pub const ARRAYS: [ValueType; 6] = [
        ValueType::IntArray,
        ValueType::FloatArray,
        ValueType::Float3Array,
        ValueType::Color3fArray,
        ValueType::StringArray,
        ValueType::TokenArray,
    ];
    /// The arrays an Object value can be: a list a `Geo` column holds.
    pub const OBJECT_ARRAYS: [ValueType; 5] = [
        ValueType::IntArray,
        ValueType::FloatArray,
        ValueType::Float3Array,
        ValueType::Color3fArray,
        ValueType::TokenArray,
    ];

    /// An array's element: its type alone. `None` for the rest.
    pub fn element(self) -> Option<ValueType> {
        Some(match self {
            ValueType::IntArray => ValueType::Int,
            ValueType::FloatArray => ValueType::Float,
            ValueType::Float3Array => ValueType::Float3,
            ValueType::Color3fArray => ValueType::Color3f,
            ValueType::StringArray => ValueType::String,
            ValueType::TokenArray => ValueType::Token,
            _ => return None,
        })
    }

    pub const ALL: [ValueType; 11] = [
        ValueType::Bool,
        ValueType::Int,
        ValueType::Float,
        ValueType::Double,
        ValueType::String,
        ValueType::Token,
        ValueType::Float2,
        ValueType::Float3,
        ValueType::Float4,
        ValueType::Color3f,
        ValueType::Asset,
    ];
    /// The types an Object value can have: those a `Geo` column holds.
    pub const OBJECT: [ValueType; 8] = [
        ValueType::Bool,
        ValueType::Int,
        ValueType::Float,
        ValueType::Token,
        ValueType::Float2,
        ValueType::Float3,
        ValueType::Float4,
        ValueType::Color3f,
    ];

    /// The type's name in USD.
    pub fn usd(self) -> &'static str {
        match self {
            ValueType::Bool => "bool",
            ValueType::Int => "int",
            ValueType::Float => "float",
            ValueType::Double => "double",
            ValueType::String => "string",
            ValueType::Token => "token",
            ValueType::Float2 => "float2",
            ValueType::Float3 => "float3",
            ValueType::Float4 => "float4",
            ValueType::Color3f => "color3f",
            ValueType::Asset => "asset",
            ValueType::IntArray => "int[]",
            ValueType::FloatArray => "float[]",
            ValueType::Float3Array => "float3[]",
            ValueType::Color3fArray => "color3f[]",
            ValueType::StringArray => "string[]",
            ValueType::TokenArray => "token[]",
        }
    }

    /// A value of this type, kept from `from` where it can be: an array of
    /// one from a value, a value from an array's first element.
    pub fn convert(self, from: &Value) -> Value {
        if let Some(element) = self.element() {
            let items: Vec<Value> = from.items().iter().map(|v| element.convert(v)).collect();
            return Value::array(self, &items);
        }
        let first;
        let from = match from.items().into_iter().next() {
            Some(v) if from.kind().element().is_some() => {
                first = v;
                &first
            }
            _ => from,
        };
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
            _ => unreachable!("arrays are made above"),
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
    IntArray(Vec<i32>),
    FloatArray(Vec<f32>),
    Float3Array(Vec<[f32; 3]>),
    Color3fArray(Vec<[f32; 3]>),
    StringArray(Vec<String>),
    TokenArray(Vec<String>),
}

impl Default for Value {
    fn default() -> Self {
        Value::Float(0.0)
    }
}

impl Value {
    /// The value with the variables in its text put in (`$JOB/tex/sky.exr`,
    /// `$OPNAME`); `keep_frames`: the frame's left for a renderer (`$F4`).
    /// Numbers as they are.
    pub fn with_variables_put_in(&self, keep_frames: bool) -> Value {
        let put = |s: &str| {
            if keep_frames {
                crate::core::vars::expand_keeping_frames(s)
            } else {
                crate::core::vars::expand(s, &[])
            }
        };
        match self {
            Value::String(s) if crate::core::vars::has_vars(s) => Value::String(put(s)),
            Value::Token(s) if crate::core::vars::has_vars(s) => Value::Token(put(s)),
            Value::Asset(s) if crate::core::vars::has_vars(s) => Value::Asset(put(s.trim())),
            v => v.clone(),
        }
    }

    pub fn kind(&self) -> ValueType {
        match self {
            Value::Bool(_) => ValueType::Bool,
            Value::Int(_) => ValueType::Int,
            Value::Float(_) => ValueType::Float,
            Value::Double(_) => ValueType::Double,
            Value::String(_) => ValueType::String,
            Value::Token(_) => ValueType::Token,
            Value::Float2(_) => ValueType::Float2,
            Value::Float3(_) => ValueType::Float3,
            Value::Float4(_) => ValueType::Float4,
            Value::Color3f(_) => ValueType::Color3f,
            Value::Asset(_) => ValueType::Asset,
            Value::IntArray(_) => ValueType::IntArray,
            Value::FloatArray(_) => ValueType::FloatArray,
            Value::Float3Array(_) => ValueType::Float3Array,
            Value::Color3fArray(_) => ValueType::Color3fArray,
            Value::StringArray(_) => ValueType::StringArray,
            Value::TokenArray(_) => ValueType::TokenArray,
        }
    }

    /// An array's elements, each a value of its own; a value alone, itself.
    pub fn items(&self) -> Vec<Value> {
        match self {
            Value::IntArray(v) => v.iter().map(|x| Value::Int(*x)).collect(),
            Value::FloatArray(v) => v.iter().map(|x| Value::Float(*x)).collect(),
            Value::Float3Array(v) => v.iter().map(|x| Value::Float3(*x)).collect(),
            Value::Color3fArray(v) => v.iter().map(|x| Value::Color3f(*x)).collect(),
            Value::StringArray(v) => v.iter().map(|x| Value::String(x.clone())).collect(),
            Value::TokenArray(v) => v.iter().map(|x| Value::Token(x.clone())).collect(),
            one => vec![one.clone()],
        }
    }

    /// An array of type `ty` (`ValueType::ARRAYS`) of `items`, each made its
    /// element's type.
    pub fn array(ty: ValueType, items: &[Value]) -> Value {
        let element = ty.element().unwrap_or(ValueType::Float);
        let all: Vec<Value> = items.iter().map(|v| element.convert(v)).collect();
        macro_rules! of {
            ($v:ident, $x:pat => $e:expr) => {
                all.iter()
                    .filter_map(|v| match v {
                        Value::$v($x) => Some($e),
                        _ => None,
                    })
                    .collect()
            };
        }
        match ty {
            ValueType::IntArray => Value::IntArray(of!(Int, x => *x)),
            ValueType::Float3Array => Value::Float3Array(of!(Float3, x => *x)),
            ValueType::Color3fArray => Value::Color3fArray(of!(Color3f, x => *x)),
            ValueType::StringArray => Value::StringArray(of!(String, x => x.clone())),
            ValueType::TokenArray => Value::TokenArray(of!(Token, x => x.clone())),
            _ => Value::FloatArray(of!(Float, x => *x)),
        }
    }

    /// An array's elements as text, a comma between them (`1, 2, 3`,
    /// `(1, 0, 0), (0, 1, 0)`, `a, b`), as the Parameters show it.
    pub fn list_text(&self) -> String {
        self.items()
            .iter()
            .map(|v| match v {
                Value::String(s) | Value::Token(s) => s.clone(),
                other => other.usda(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// An array of type `ty` read back from `list_text`'s text: tuples in
    /// brackets, numbers or words between commas.
    pub fn parse_list(ty: ValueType, text: &str) -> Value {
        let mut items: Vec<Value> = vec![];
        let mut rest = text.trim();
        while !rest.is_empty() {
            let (one, next) = if rest.starts_with('(') {
                match rest.find(')') {
                    Some(i) => (&rest[1..i], &rest[i + 1..]),
                    None => (&rest[1..], ""),
                }
            } else {
                match rest.find(',') {
                    Some(i) => (&rest[..i], &rest[i..]),
                    None => (rest, ""),
                }
            };
            let one = one.trim().trim_matches('"');
            items.push(match ty.element() {
                Some(ValueType::String) | Some(ValueType::Token) => Value::String(one.to_string()),
                _ => Value::FloatArray(one.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).collect()),
            });
            rest = next.trim_start().trim_start_matches(',').trim_start();
        }
        // A tuple's numbers become its element: (1, 0, 0) a float3.
        let items: Vec<Value> = items
            .iter()
            .map(|v| match v {
                Value::FloatArray(f) => match ty.element() {
                    Some(ValueType::Float3) | Some(ValueType::Color3f) => {
                        let at = |i: usize| f.get(i).copied().unwrap_or(0.0);
                        Value::Float3([at(0), at(1), at(2)])
                    }
                    _ => Value::Float(f.first().copied().unwrap_or(0.0)),
                },
                other => other.clone(),
            })
            .collect();
        Value::array(ty, &items)
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
            Value::String(s) | Value::Token(s) | Value::Asset(s) => {
                s.trim().parse::<f32>().map(|x| vec![x]).unwrap_or_default()
            }
            array => array.items().iter().flat_map(Value::floats).collect(),
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
            Value::Double(x) => {
                if x.is_finite() {
                    format!("{x}")
                } else {
                    "0".into()
                }
            }
            Value::String(s) | Value::Token(s) => quoted(s),
            Value::Float2(v) => tuple(v),
            Value::Float3(v) | Value::Color3f(v) => tuple(v),
            Value::Float4(v) => tuple(v),
            Value::Asset(s) => format!("@{}@", s.replace('@', "")),
            array => format!(
                "[{}]",
                array.items().iter().map(Value::usda).collect::<Vec<_>>().join(", ")
            ),
        }
    }

    /// The value as the Primitive Inspector shows it: text as it is, numbers as
    /// `.usda` writes them.
    pub fn display(&self) -> String {
        self.text()
    }

    /// Its numbers, for the types that are numbers.
    pub fn numbers(&self) -> Option<Vec<f32>> {
        match self {
            Value::String(_) | Value::Token(_) | Value::Asset(_) | Value::StringArray(_) | Value::TokenArray(_) => None,
            other => Some(other.floats()),
        }
    }

    /// The token, when the value is one (or a string).
    pub fn token(&self) -> Option<&str> {
        match self {
            Value::Token(s) | Value::String(s) => Some(s.trim()),
            _ => None,
        }
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
            Value::Double(_) | Value::String(_) | Value::Asset(_) | Value::StringArray(_) => None,
            // An array: one list, as ICE stores a list per element.
            Value::IntArray(v) => Some(Attr::lists(Column::Int(Arc::new(v.clone())), v.len(), Role::None)),
            Value::FloatArray(v) => Some(Attr::lists(Column::Float(Arc::new(v.clone())), v.len(), Role::None)),
            Value::Float3Array(v) => Some(Attr::lists(Column::Vec3(Arc::new(v.clone())), v.len(), Role::None)),
            Value::Color3fArray(v) => Some(Attr::lists(Column::Vec3(Arc::new(v.clone())), v.len(), Role::Color)),
            Value::TokenArray(v) => Some(Attr::lists(
                Column::Token(Arc::new(v.iter().map(|s| Arc::from(s.as_str())).collect())),
                v.len(),
                Role::None,
            )),
        }
    }
}

// Contexts and the properties Imago knows

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
pub enum Written {
    Attribute,
    Uniform,
    Metadata,
}

/// A name the name field offers, with its type and, where USD fixes them, its
/// values.
pub struct Preset {
    pub name: &'static str,
    pub ty: ValueType,
    pub tokens: &'static [&'static str],
    pub written: Written,
    pub hint: &'static str,
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
    Preset {
        name: "displayColor",
        ty: ValueType::Color3f,
        tokens: &[],
        written: Written::Attribute,
        hint: "The colour a renderer uses without a material, and a shader can read",
    },
    Preset {
        name: "displayOpacity",
        ty: ValueType::Float,
        tokens: &[],
        written: Written::Attribute,
        hint: "The opacity a renderer uses without a material",
    },
];

/// The preset of a name, in a context.
pub fn preset(context: SetContext, name: &str) -> Option<&'static Preset> {
    let list = match context {
        SetContext::Object => OBJECT_PRESETS,
        SetContext::Property => PROPERTY_PRESETS,
    };
    list.iter().find(|p| p.name == name.trim())
}

/// How a Property of this name is written. Names it does not know are
/// attributes.
pub fn written(name: &str) -> Written {
    preset(SetContext::Property, name).map_or(Written::Attribute, |p| p.written)
}

// Opinions on prims without geometry

/// The values set on one path of a stage that is not a geometry primitive.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Opinions {
    /// Object values: constant primvars.
    pub primvars: BTreeMap<String, Value>,
    /// Property values: attributes and metadata by name.
    pub props: BTreeMap<String, Value>,
    /// Its own transform, relative to its parent, set by Edit Primitive
    /// (column-major): written as its `xformOp:transform`.
    pub transform: Option<[f64; 16]>,
    /// A variant set made on it (Variant, Create), with every variant: written
    /// as its `variantSet`.
    pub variant_set: Option<crate::core::variant::MadeSet>,
}

impl Opinions {
    pub fn is_empty(&self) -> bool {
        self.primvars.is_empty() && self.props.is_empty() && self.transform.is_none() && self.variant_set.is_none()
    }

    /// `other` over these: its values win.
    pub fn merge(&mut self, other: &Opinions) {
        self.primvars
            .extend(other.primvars.iter().map(|(k, v)| (k.clone(), v.clone())));
        self.props
            .extend(other.props.iter().map(|(k, v)| (k.clone(), v.clone())));
        if other.transform.is_some() {
            self.transform = other.transform;
        }
        if other.variant_set.is_some() {
            self.variant_set = other.variant_set.clone();
        }
    }
}

// Inheritance

/// A purpose from its token. A render prim stays one with its proxy.
pub fn purpose(token: &str, was: Purpose) -> Purpose {
    match token {
        "render" => match was {
            Purpose::Render { has_proxy } => Purpose::Render { has_proxy },
            _ => Purpose::Render { has_proxy: false },
        },
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

/// The values set on the prims above a primitive, the furthest first: on prims
/// of its stage, and on the Xforms Imago made above it.
pub fn above(p: &Prim) -> Vec<(&str, &Opinions)> {
    let mut v: Vec<(&str, &Opinions)> = p.above.iter().map(|(k, o)| (k.as_str(), o)).collect();
    if let Some(t) = &p.stage {
        v.extend(
            t.opinions
                .iter()
                .filter(|(path, _)| **path != p.path && is_below(&p.path, path))
                .map(|(k, o)| (k.as_str(), o)),
        );
    }
    v.sort_by_key(|(k, _)| k.matches('/').count());
    v
}

/// Whether a primitive is invisible: set so on itself, or on a prim above it
/// (USD inherits `invisible`).
pub fn invisible(p: &Prim) -> bool {
    let off = |v: Option<&Value>| v.and_then(Value::token) == Some("invisible");
    off(p.props.get("visibility")) || above(p).iter().any(|(_, o)| off(o.props.get("visibility")))
}

/// How the scene says a primitive is drawn: USD's `model:drawMode`, on it or on
/// the nearest prim above it that has one (`inherited` passes on to the one
/// above). `None`: nothing says.
pub fn draw_mode(p: &Prim) -> Option<crate::types::DrawMode> {
    let mode = |o: &BTreeMap<String, Value>| {
        o.get("model:drawMode")
            .and_then(Value::token)
            .and_then(crate::types::DrawMode::from_token)
    };
    if let Some(m) = mode(p.props.as_ref()) {
        return Some(m);
    }
    above(p).iter().rev().find_map(|(_, o)| mode(&o.props))
}

/// Where a primitive's purpose comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PurposeFrom {
    /// Set on the primitive itself: by Set Attribute, or made so in Imago.
    Set,
    /// Authored on the primitive in its file.
    File,
    /// Inherited from the prim above at this path: set there by Set Attribute
    /// (`true`), or authored there in the file (`false`).
    Above(String, bool),
    /// Nothing says: the default.
    Unset,
}

/// Where a primitive's purpose comes from. As in USD, a purpose on the
/// primitive itself wins; else the nearest prim above it with one, whether the
/// file authors it there or Set Attribute set it there (on the same prim, Set
/// Attribute's is the stronger opinion). A purpose a loaded primitive only
/// inherited from its file does not hide a nearer one.
pub fn purpose_from(p: &Prim) -> PurposeFrom {
    let set_here = p
        .source
        .as_ref()
        .map_or(p.purpose != Purpose::Default, |s| s.purpose != p.purpose);
    if set_here {
        return PurposeFrom::Set;
    }
    let tree = p.stage.as_deref();
    let authored = |path: &str| tree.is_some_and(|t| t.authored(path, "purpose").is_some());
    if authored(&p.path) {
        return PurposeFrom::File;
    }
    let depth = |path: &str| path.matches('/').count();
    let set_above = above(p)
        .into_iter()
        .rev()
        .find(|(_, o)| o.props.get("purpose").and_then(Value::token).is_some())
        .map(|(path, _)| path.to_string());
    let file_above = if p.purpose != Purpose::Default {
        parents(&p.path).into_iter().rev().find(|a| authored(a))
    } else {
        None
    };
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
    if !above(p).iter().any(|(_, o)| o.props.contains_key("purpose")) {
        return p.purpose;
    }
    match purpose_from(p) {
        PurposeFrom::Above(path, true) => above(p)
            .into_iter()
            .find(|(k, _)| *k == path)
            .and_then(|(_, o)| {
                o.props
                    .get("purpose")
                    .and_then(Value::token)
                    .map(|t| purpose(t, Purpose::Default))
            })
            .unwrap_or(p.purpose),
        _ => p.purpose,
    }
}

/// The Object values a primitive inherits from the prims above it, the nearest
/// winning, where it has no attribute of that name itself.
pub fn inherited_primvars(p: &Prim) -> Vec<(String, Value)> {
    let mut values: BTreeMap<String, Value> = BTreeMap::new();
    for (_, o) in above(p) {
        for (k, v) in &o.primvars {
            values.insert(k.clone(), v.clone());
        }
    }
    let own = |name: &str| {
        p.geo.as_ref().map_or(false, |g| g.find(name).is_some()) || p.mesh.primvars.iter().any(|v| v.name == name)
    };
    values.into_iter().filter(|(k, _)| !own(k)).collect()
}
