//! The MaterialX node library: every node definition (`nodedef`) of the
//! vendored libraries, read into typed inputs and outputs.
//!
//! A MaterialX node, such as `add` or `standard_surface`, has one definition
//! per type it works on (`ND_add_float`, `ND_add_color3`, `ND_add_color3FA`
//! ...), and sometimes per version (`standard_surface` 1.0.0 and 1.0.1). A node
//! in a graph is one node: which of its definitions it is follows from what is
//! connected to it (`Library::resolve`).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::types::{Type, Value};
use crate::xml::{self, Element};

/// The vendored library files, in the order they are read: stdlib first (it
/// declares the data types), then pbrlib and the shading models.
pub const FILES: &[(&str, &str)] = &[
    (
        "stdlib/stdlib_defs.mtlx",
        include_str!("../libraries/stdlib/stdlib_defs.mtlx"),
    ),
    (
        "stdlib/stdlib_ng.mtlx",
        include_str!("../libraries/stdlib/stdlib_ng.mtlx"),
    ),
    (
        "pbrlib/pbrlib_defs.mtlx",
        include_str!("../libraries/pbrlib/pbrlib_defs.mtlx"),
    ),
    (
        "pbrlib/pbrlib_ng.mtlx",
        include_str!("../libraries/pbrlib/pbrlib_ng.mtlx"),
    ),
    (
        "bxdf/standard_surface.mtlx",
        include_str!("../libraries/bxdf/standard_surface.mtlx"),
    ),
    (
        "bxdf/open_pbr_surface.mtlx",
        include_str!("../libraries/bxdf/open_pbr_surface.mtlx"),
    ),
    (
        "bxdf/usd_preview_surface.mtlx",
        include_str!("../libraries/bxdf/usd_preview_surface.mtlx"),
    ),
    ("bxdf/gltf_pbr.mtlx", include_str!("../libraries/bxdf/gltf_pbr.mtlx")),
    (
        "bxdf/disney_principled.mtlx",
        include_str!("../libraries/bxdf/disney_principled.mtlx"),
    ),
    (
        "bxdf/lama/lama_add.mtlx",
        include_str!("../libraries/bxdf/lama/lama_add.mtlx"),
    ),
    (
        "bxdf/lama/lama_conductor.mtlx",
        include_str!("../libraries/bxdf/lama/lama_conductor.mtlx"),
    ),
    (
        "bxdf/lama/lama_dielectric.mtlx",
        include_str!("../libraries/bxdf/lama/lama_dielectric.mtlx"),
    ),
    (
        "bxdf/lama/lama_diffuse.mtlx",
        include_str!("../libraries/bxdf/lama/lama_diffuse.mtlx"),
    ),
    (
        "bxdf/lama/lama_emission.mtlx",
        include_str!("../libraries/bxdf/lama/lama_emission.mtlx"),
    ),
    (
        "bxdf/lama/lama_generalized_schlick.mtlx",
        include_str!("../libraries/bxdf/lama/lama_generalized_schlick.mtlx"),
    ),
    (
        "bxdf/lama/lama_iridescence.mtlx",
        include_str!("../libraries/bxdf/lama/lama_iridescence.mtlx"),
    ),
    (
        "bxdf/lama/lama_layer.mtlx",
        include_str!("../libraries/bxdf/lama/lama_layer.mtlx"),
    ),
    (
        "bxdf/lama/lama_mix.mtlx",
        include_str!("../libraries/bxdf/lama/lama_mix.mtlx"),
    ),
    (
        "bxdf/lama/lama_sheen.mtlx",
        include_str!("../libraries/bxdf/lama/lama_sheen.mtlx"),
    ),
    (
        "bxdf/lama/lama_sss.mtlx",
        include_str!("../libraries/bxdf/lama/lama_sss.mtlx"),
    ),
    (
        "bxdf/lama/lama_surface.mtlx",
        include_str!("../libraries/bxdf/lama/lama_surface.mtlx"),
    ),
    (
        "bxdf/lama/lama_translucent.mtlx",
        include_str!("../libraries/bxdf/lama/lama_translucent.mtlx"),
    ),
    (
        "bxdf/translation/open_pbr_to_standard_surface.mtlx",
        include_str!("../libraries/bxdf/translation/open_pbr_to_standard_surface.mtlx"),
    ),
    (
        "bxdf/translation/standard_surface_to_gltf_pbr.mtlx",
        include_str!("../libraries/bxdf/translation/standard_surface_to_gltf_pbr.mtlx"),
    ),
    (
        "bxdf/translation/standard_surface_to_open_pbr.mtlx",
        include_str!("../libraries/bxdf/translation/standard_surface_to_open_pbr.mtlx"),
    ),
    (
        "bxdf/translation/standard_surface_to_usd.mtlx",
        include_str!("../libraries/bxdf/translation/standard_surface_to_usd.mtlx"),
    ),
    (
        "cmlib/cmlib_defs.mtlx",
        include_str!("../libraries/cmlib/cmlib_defs.mtlx"),
    ),
    ("cmlib/cmlib_ng.mtlx", include_str!("../libraries/cmlib/cmlib_ng.mtlx")),
    (
        "nprlib/nprlib_defs.mtlx",
        include_str!("../libraries/nprlib/nprlib_defs.mtlx"),
    ),
    (
        "nprlib/nprlib_ng.mtlx",
        include_str!("../libraries/nprlib/nprlib_ng.mtlx"),
    ),
    (
        "lights/lights_defs.mtlx",
        include_str!("../libraries/lights/lights_defs.mtlx"),
    ),
];

/// The MaterialX version the vendored libraries are.
pub const MATERIALX_VERSION: &str = "1.39";

/// How an input is shown: the `ui*` attributes of its definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ui {
    /// Its label (`uiname`); the input's name when it has none.
    pub name: Option<String>,
    /// The group it is shown in (`uifolder`), `/` between nested folders.
    pub folder: Option<String>,
    pub min: Option<Value>,
    pub max: Option<Value>,
    pub soft_min: Option<Value>,
    pub soft_max: Option<Value>,
    pub step: Option<Value>,
    /// Shown only when advanced inputs are.
    pub advanced: bool,
    /// False: not shown at all (`uivisible="false"`).
    pub visible: bool,
}

/// An input of a node definition.
#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub name: String,
    pub ty: Type,
    /// Its default (`value`). None for closures and shaders, and for inputs
    /// that take a geometric property instead (`default_geomprop`).
    pub value: Option<Value>,
    /// Set once per material, not per point: a file name, an address mode.
    pub uniform: bool,
    /// When nothing is connected and no value set, this geometric property
    /// (`UV0`, `Nworld`, ...) is read instead.
    pub default_geomprop: Option<String>,
    /// The choices of a string input (`enum`), or the names of an integer's.
    pub choices: Vec<String>,
    /// The values the choices stand for (`enumvalues`), when they are not the
    /// names themselves.
    pub choice_values: Option<String>,
    pub doc: Option<String>,
    pub ui: Ui,
    /// Every attribute as written, these and the rest (`unittype`, `unit`,
    /// `colorspace`, ...).
    pub attrs: Vec<(String, String)>,
}

impl Input {
    /// Its label: `uiname`, or the name.
    pub fn label(&self) -> &str {
        self.ui.name.as_deref().unwrap_or(&self.name)
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

/// An output of a node definition.
#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub name: String,
    pub ty: Type,
    /// The input passed through when the node is disabled.
    pub default_input: Option<String>,
    pub attrs: Vec<(String, String)>,
}

/// A node definition: one node (`add`) for one set of types (`ND_add_color3`).
#[derive(Clone, Debug, PartialEq)]
pub struct NodeDef {
    /// `ND_add_color3`: what a document, and USD's `info:id`, names it by.
    pub name: String,
    /// `add`: the node it defines.
    pub node: String,
    /// `math`, `texture2d`, `pbr`, ...
    pub group: String,
    /// Its label, when the library gives one (`OpenPBR Surface`).
    pub ui_name: Option<String>,
    pub version: Option<String>,
    pub is_default_version: bool,
    /// The definition whose inputs and outputs it starts from.
    pub inherit: Option<String>,
    pub doc: Option<String>,
    /// Its inputs, inherited ones included, in document order.
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    /// The library file it was read from (`stdlib/stdlib_defs.mtlx`).
    pub file: String,
}

impl NodeDef {
    pub fn input(&self, name: &str) -> Option<&Input> {
        self.inputs.iter().find(|i| i.name == name)
    }
    pub fn output(&self, name: &str) -> Option<&Output> {
        self.outputs.iter().find(|o| o.name == name)
    }

    /// The type of its output, when it has one; else its first.
    pub fn out_type(&self) -> Type {
        self.outputs.first().map(|o| o.ty).unwrap_or(Type::None)
    }
}

/// What went wrong reading a library.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadError {
    pub file: String,
    pub what: String,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.file, self.what)
    }
}

impl std::error::Error for LoadError {}

/// The node definitions of a set of documents.
#[derive(Clone, Debug, Default)]
pub struct Library {
    defs: Vec<NodeDef>,
    by_name: HashMap<String, usize>,
    /// Each node's definitions, in document order.
    by_node: HashMap<String, Vec<usize>>,
    /// Nodes in the order they are first defined.
    nodes: Vec<String>,
    /// Each versioned node's default version.
    default_version: HashMap<String, String>,
    /// The data types declared (`typedef`).
    pub typedefs: Vec<String>,
}

impl Library {
    /// MaterialX's own libraries, vendored with this crate. Read once.
    pub fn standard() -> &'static Library {
        static LIB: OnceLock<Library> = OnceLock::new();
        LIB.get_or_init(|| Library::read(FILES).expect("the vendored MaterialX libraries read"))
    }

    /// The node definitions of `files` (name, text), read in order.
    pub fn read(files: &[(&str, &str)]) -> Result<Library, LoadError> {
        let mut lib = Library::default();
        for (file, text) in files {
            let root = xml::parse(text).map_err(|e| LoadError {
                file: file.to_string(),
                what: e.to_string(),
            })?;
            if root.name != "materialx" {
                return Err(LoadError {
                    file: file.to_string(),
                    what: format!("root is <{}>, not <materialx>", root.name),
                });
            }
            for el in &root.children {
                match el.name.as_str() {
                    "typedef" => lib.typedefs.extend(el.attr("name").map(String::from)),
                    "nodedef" => {
                        let def = read_nodedef(el, file).map_err(|what| LoadError {
                            file: file.to_string(),
                            what,
                        })?;
                        lib.add(def).map_err(|what| LoadError {
                            file: file.to_string(),
                            what,
                        })?;
                    }
                    _ => {}
                }
            }
        }
        lib.inherit()?;
        Ok(lib)
    }

    fn add(&mut self, def: NodeDef) -> Result<(), String> {
        if self.by_name.contains_key(&def.name) {
            return Err(format!("{} is defined twice", def.name));
        }
        let i = self.defs.len();
        self.by_name.insert(def.name.clone(), i);
        let list = self.by_node.entry(def.node.clone()).or_default();
        if list.is_empty() {
            self.nodes.push(def.node.clone());
        }
        list.push(i);
        if def.is_default_version {
            if let Some(v) = &def.version {
                self.default_version.insert(def.node.clone(), v.clone());
            }
        }
        self.defs.push(def);
        Ok(())
    }

    /// Give each definition that inherits its parent's inputs and outputs: the
    /// parent's first, in their order, its own replacing those of the same
    /// name, then its new ones.
    fn inherit(&mut self) -> Result<(), LoadError> {
        let mut done = vec![false; self.defs.len()];
        for i in 0..self.defs.len() {
            self.inherit_one(i, &mut done, 0)?;
        }
        Ok(())
    }

    fn inherit_one(&mut self, i: usize, done: &mut [bool], depth: usize) -> Result<(), LoadError> {
        if done[i] {
            return Ok(());
        }
        let err = |def: &NodeDef, what: String| LoadError {
            file: def.file.clone(),
            what,
        };
        if depth > 16 {
            return Err(err(&self.defs[i], format!("{} inherits in a loop", self.defs[i].name)));
        }
        if let Some(parent) = self.defs[i].inherit.clone() {
            let Some(&p) = self.by_name.get(&parent) else {
                return Err(err(
                    &self.defs[i],
                    format!("{} inherits {parent}, which is not defined", self.defs[i].name),
                ));
            };
            self.inherit_one(p, done, depth + 1)?;
            let (inputs, outputs) = (self.defs[p].inputs.clone(), self.defs[p].outputs.clone());
            let def = &mut self.defs[i];
            def.inputs = merge(inputs, std::mem::take(&mut def.inputs), |x| &x.name);
            def.outputs = merge(outputs, std::mem::take(&mut def.outputs), |x| &x.name);
        }
        done[i] = true;
        Ok(())
    }

    /// Every definition, in document order.
    pub fn defs(&self) -> &[NodeDef] {
        &self.defs
    }

    /// The definition a document names: `ND_add_color3`.
    pub fn def(&self, name: &str) -> Option<&NodeDef> {
        self.by_name.get(name).map(|&i| &self.defs[i])
    }

    /// Every node, in the order the libraries first define it.
    pub fn nodes(&self) -> &[String] {
        &self.nodes
    }

    /// The group a node is listed under: its first definition's.
    pub fn group(&self, node: &str) -> Option<&str> {
        self.variants_all(node).next().map(|d| d.group.as_str())
    }

    /// A node's definitions of every version.
    pub fn variants_all<'a>(&'a self, node: &str) -> impl Iterator<Item = &'a NodeDef> + 'a {
        self.by_node.get(node).into_iter().flatten().map(|&i| &self.defs[i])
    }

    /// A node's definitions at its current version: the default version for a
    /// versioned node (`standard_surface` 1.0.1, not 1.0.0).
    pub fn variants<'a>(&'a self, node: &str) -> impl Iterator<Item = &'a NodeDef> + 'a {
        self.variants_all(node).filter(move |d| self.is_current(d))
    }

    /// At its node's default version, or unversioned.
    pub fn is_current(&self, def: &NodeDef) -> bool {
        match (&def.version, self.default_version.get(&def.node)) {
            (Some(v), Some(default)) => v == default,
            _ => true,
        }
    }

    /// The definition a new node starts as: its first at the current version
    /// (`add` starts as `ND_add_float`).
    pub fn default_def(&self, node: &str) -> Option<&NodeDef> {
        self.variants(node).next()
    }

    /// Whether `def` takes what is connected to it: each `(input, type)` in
    /// `connected` is an input of that type, and its output is `output`, if one
    /// is asked for.
    pub fn fits(def: &NodeDef, connected: &[(&str, Type)], output: Option<Type>) -> bool {
        connected
            .iter()
            .all(|(name, ty)| def.input(name).is_some_and(|i| i.ty == *ty))
            && output.is_none_or(|t| def.outputs.iter().any(|o| o.ty == t))
    }

    /// The definition of `node` that takes what is connected (see `fits`): the
    /// first at the current version, so with nothing connected the node's
    /// default. None when no definition does: the connection is between types
    /// the node does not join.
    pub fn resolve(&self, node: &str, connected: &[(&str, Type)], output: Option<Type>) -> Option<&NodeDef> {
        self.variants(node).find(|d| Library::fits(d, connected, output))
    }
}

/// `base` with `own` laid over it: entries of the same name replaced in place,
/// new ones after.
fn merge<T>(base: Vec<T>, own: Vec<T>, name: impl Fn(&T) -> &String) -> Vec<T> {
    let mut out = base;
    for x in own {
        match out.iter().position(|b| name(b) == name(&x)) {
            Some(k) => out[k] = x,
            None => out.push(x),
        }
    }
    out
}

fn read_nodedef(el: &Element, file: &str) -> Result<NodeDef, String> {
    let name = el.attr("name").ok_or("a nodedef has no name")?.to_string();
    let node = el
        .attr("node")
        .ok_or_else(|| format!("{name} has no node"))?
        .to_string();
    let mut def = NodeDef {
        group: el.attr("nodegroup").unwrap_or("").to_string(),
        ui_name: el.attr("uiname").map(String::from),
        version: el.attr("version").map(String::from),
        is_default_version: el.attr("isdefaultversion") == Some("true"),
        inherit: el.attr("inherit").map(String::from),
        doc: el.attr("doc").map(String::from),
        inputs: vec![],
        outputs: vec![],
        file: file.to_string(),
        name,
        node,
    };
    for c in &el.children {
        match c.name.as_str() {
            "input" => def
                .inputs
                .push(read_input(c).map_err(|e| format!("{}: {e}", def.name))?),
            "output" => def
                .outputs
                .push(read_output(c).map_err(|e| format!("{}: {e}", def.name))?),
            _ => {}
        }
    }
    Ok(def)
}

fn read_type(el: &Element) -> Result<Type, String> {
    let t = el
        .attr("type")
        .ok_or_else(|| format!("{} has no type", el.attr("name").unwrap_or("?")))?;
    Type::from_name(t).ok_or_else(|| format!("unknown type {t}"))
}

fn read_input(el: &Element) -> Result<Input, String> {
    let name = el.attr("name").ok_or("an input has no name")?.to_string();
    let ty = read_type(el)?;
    let value = match el.attr("value") {
        // Closures and shaders have no value: `value=""` on them is nothing.
        Some(_) if ty.is_shading() || ty == Type::None => None,
        Some(text) => Some(Value::parse(ty, text).ok_or_else(|| format!("{name}: {text:?} is not a {ty}"))?),
        None => None,
    };
    // UI limits are written in the input's type; an integer's choices may
    // carry floats in odd documents, so a limit that does not read is
    // dropped rather than refused.
    let limit = |key: &str| el.attr(key).and_then(|t| Value::parse(ty, t));
    let ui = Ui {
        name: el.attr("uiname").map(String::from),
        folder: el.attr("uifolder").map(String::from),
        min: limit("uimin"),
        max: limit("uimax"),
        soft_min: limit("uisoftmin"),
        soft_max: limit("uisoftmax"),
        step: limit("uistep"),
        advanced: el.attr("uiadvanced") == Some("true"),
        visible: el.attr("uivisible") != Some("false"),
    };
    Ok(Input {
        value,
        uniform: el.attr("uniform") == Some("true"),
        default_geomprop: el.attr("defaultgeomprop").map(String::from),
        choices: el
            .attr("enum")
            .map(|e| e.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default(),
        choice_values: el.attr("enumvalues").map(String::from),
        doc: el.attr("doc").map(String::from),
        ui,
        attrs: el.attrs.clone(),
        name,
        ty,
    })
}

fn read_output(el: &Element) -> Result<Output, String> {
    Ok(Output {
        name: el.attr("name").ok_or("an output has no name")?.to_string(),
        ty: read_type(el)?,
        default_input: el.attr("defaultinput").map(String::from),
        attrs: el.attrs.clone(),
    })
}
