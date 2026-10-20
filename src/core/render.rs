//! Rendering, as USD has it (UsdRender): the prims a renderer reads to know
//! what to render and how.
//!
//! - **RenderSettings** (`/Render/Settings` by default): the camera, the
//!   resolution and its aspect, the data window (overscan), motion blur and
//!   depth of field on or off, the purposes rendered, the products made.
//!   Renderer settings are namespaced attributes on the same prim
//!   (`karma:global:…`, `arnold:global:…`, or any namespace of your own).
//! - **RenderProduct** (`/Render/Products/beauty`): one image file
//!   (`productName`), and the RenderVars it holds, in order (`orderedVars`).
//! - **RenderVar** (`/Render/Products/Vars/<name>`): one AOV: where the
//!   renderer takes it from (`sourceName`, `sourceType`: raw, lpe, primvar,
//!   intrinsic) and its type (`dataType`).
//!
//! Three operators make them, each passing the scene that comes in through
//! with its render prims added or changed:
//!
//! - **RenderScene Settings** (`SceneSettings`): the RenderSettings prim and
//!   a default RenderProduct. The one the layer names (`renderSettingsPrimPath`).
//! - **Render Settings** (`RendererSettings`): a renderer's settings, from
//!   its definition (`Definition`, a JSON file), on the RenderSettings prim
//!   the scene has: the one named, else the first, else one it makes. Its
//!   AOVs tab adds the renderer's standard AOVs as RenderVars of the product.
//! - **Render Variables** (`Variables`): RenderVars of a product, set or
//!   overridden row by row.
//!
//! A render prim is a primitive with no geometry, as a camera or a material
//! is (`Prim::render`); Write USD writes it as `def RenderSettings` and so
//! on, under Scopes. One found in a file is changed with an `over`.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::core::properties::{Value, ValueType};
use crate::types::{prim_location, EvalResult, MeshData, Prim};

// ── Render prims ───────────────────────────────────────────────────────────

/// What a render prim is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderKind { Settings, Product, Var }

impl RenderKind {
    /// Its type in USD.
    pub fn usd(self) -> &'static str {
        match self { RenderKind::Settings => "RenderSettings", RenderKind::Product => "RenderProduct", RenderKind::Var => "RenderVar" }
    }
}

/// One attribute of a render prim, as `.usda` writes it: its type, its
/// value, and whether it is `uniform` (the schema's own attributes are).
#[derive(Clone, Debug, PartialEq)]
pub struct RAttr {
    pub ty:      String,
    pub literal: String,
    pub uniform: bool,
}

impl RAttr {
    /// A value of one of the types Set Attribute knows.
    pub fn of(v: &Value) -> Self { RAttr { ty: v.kind().usd().into(), literal: v.usda(), uniform: false } }
    /// Two whole numbers (`resolution`).
    pub fn int2(v: [i32; 2]) -> Self { RAttr { ty: "int2".into(), literal: format!("({}, {})", v[0], v[1]), uniform: false } }
    /// A list of tokens (`includedPurposes`).
    pub fn tokens(v: &[String]) -> Self {
        let items: Vec<String> = v.iter().map(|t| Value::Token(t.clone()).usda()).collect();
        RAttr { ty: "token[]".into(), literal: format!("[{}]", items.join(", ")), uniform: false }
    }
    /// The same, `uniform`.
    pub fn uniform(mut self) -> Self { self.uniform = true; self }

    /// Its line in a prim's body.
    pub fn line(&self, name: &str) -> String {
        format!("{}{} {name} = {}", if self.uniform { "uniform " } else { "" }, self.ty, self.literal)
    }

    /// As the Primitive Inspector shows it: text without its quotes.
    pub fn display(&self) -> String {
        let t = &self.literal;
        if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') { t[1..t.len() - 1].to_string() } else { t.clone() }
    }
}

/// A relationship's targets. `append`: added to what the file has (an
/// `over` on a prim of a file, whose targets Imago does not read), else
/// they are all of them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RRel {
    pub targets: Vec<String>,
    pub append:  bool,
}

/// A RenderSettings, RenderProduct or RenderVar prim.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderPrim {
    pub kind:    RenderKind,
    /// A prim of a file, changed: written as an `over`.
    pub over:    bool,
    /// The RenderSettings prim the layer names (`renderSettingsPrimPath`):
    /// the one RenderScene Settings made last.
    pub primary: bool,
    pub attrs:   BTreeMap<String, RAttr>,
    pub rels:    BTreeMap<String, RRel>,
}

impl RenderPrim {
    pub fn new(kind: RenderKind) -> Self {
        RenderPrim { kind, over: false, primary: false, attrs: BTreeMap::new(), rels: BTreeMap::new() }
    }

    pub fn set(&mut self, name: &str, attr: RAttr) { self.attrs.insert(name.to_string(), attr); }

    /// The relationship's first target.
    pub fn first(&self, rel: &str) -> Option<&str> { self.rels.get(rel).and_then(|r| r.targets.first()).map(String::as_str) }

    /// Adds a target to a relationship, once. On an `over`, it is appended
    /// to the file's.
    pub fn target(&mut self, rel: &str, path: &str) {
        let over = self.over;
        let r = self.rels.entry(rel.to_string()).or_insert_with(|| RRel { targets: vec![], append: over });
        if !r.targets.iter().any(|t| t == path) { r.targets.push(path.to_string()); }
    }

    /// The relationship's targets, all of them: what the file had is replaced.
    pub fn targets(&mut self, rel: &str, paths: Vec<String>) {
        self.rels.insert(rel.to_string(), RRel { targets: paths, append: false });
    }

    /// Its body in `.usda`: the attributes, then the relationships, by name.
    pub fn lines(&self) -> Vec<String> {
        let mut out: Vec<String> = self.attrs.iter().map(|(n, a)| a.line(n)).collect();
        for (name, r) in &self.rels {
            if r.targets.is_empty() { continue; }
            let targets: Vec<String> = r.targets.iter().map(|t| format!("<{t}>")).collect();
            let list = if targets.len() == 1 && !r.append { targets[0].clone() } else { format!("[{}]", targets.join(", ")) };
            out.push(format!("{}rel {name} = {list}", if r.append { "append " } else { "" }));
        }
        out
    }
}

/// A primitive that is a render prim.
pub fn render_prim(path: &str, r: RenderPrim) -> Prim {
    Prim { render: Some(Arc::new(r)), ..Prim::new(path.to_string(), MeshData::default()) }
}

// ── The scene coming in ────────────────────────────────────────────────────

/// What an operator takes in, as primitives: a mesh is one; nothing, none.
fn prims_in(input: Option<&EvalResult>) -> Vec<Prim> {
    match input {
        Some(EvalResult::Named(p)) => p.clone(),
        Some(EvalResult::Single(m)) => vec![Prim::new("/mesh".into(), m.clone())],
        _ => vec![],
    }
}

/// The paths of a type among the prims of the stages coming in, in stage order.
fn stage_paths(prims: &[Prim], type_name: &str) -> Vec<String> {
    let mut trees: Vec<&Arc<crate::usd_scene::StageTree>> = vec![];
    for p in prims {
        if let Some(t) = &p.stage { if !trees.iter().any(|x| Arc::ptr_eq(x, t)) { trees.push(t); } }
    }
    let mut out: Vec<String> = vec![];
    for n in trees.iter().flat_map(|t| t.nodes.iter()) {
        if n.type_name == type_name && !out.contains(&n.path) { out.push(n.path.clone()); }
    }
    out
}

/// The render prims made in Imago of a kind, in order.
fn made(prims: &[Prim], kind: RenderKind) -> Vec<String> {
    prims.iter().filter(|p| p.render.as_ref().is_some_and(|r| r.kind == kind)).map(|p| p.path.clone()).collect()
}

/// Every path a render prim of this kind has, made in Imago first, then the files'.
fn all_of(prims: &[Prim], kind: RenderKind) -> Vec<String> {
    let mut out = made(prims, kind);
    for p in stage_paths(prims, kind.usd()) { if !out.contains(&p) { out.push(p); } }
    out
}

/// The cameras of the scene: those made in Imago, then the files'.
pub fn cameras(prims: &[Prim]) -> Vec<String> {
    let mut out: Vec<String> = prims.iter().filter(|p| p.camera.is_some()).map(|p| p.path.clone()).collect();
    for p in stage_paths(prims, "Camera") { if !out.contains(&p) { out.push(p); } }
    out
}

/// Where the RenderSettings prim an operator works on comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    /// The one the layer names: made by RenderScene Settings.
    Named(String),
    /// The first one coming in, made in Imago.
    First(String),
    /// The first one of a file: changed with an `over`.
    InFile(String),
    /// None coming in: one is made at the operator's Primitive Location.
    Made(String),
}

impl Found {
    pub fn path(&self) -> &str {
        match self { Found::Named(p) | Found::First(p) | Found::InFile(p) | Found::Made(p) => p }
    }
    /// What the Properties panel says about it.
    pub fn describe(&self) -> String {
        match self {
            Found::Named(p) => format!("On {p}: the RenderSettings prim the scene names"),
            Found::First(p) => format!("On {p}: the first RenderSettings prim coming in"),
            Found::InFile(p) => format!("On {p}, from the file: written as an over"),
            Found::Made(p) => format!("No RenderSettings prim coming in: makes {p}"),
        }
    }
}

/// The RenderSettings prim of a scene, without following wires: the one
/// named (`renderSettingsPrimPath`, which RenderScene Settings sets), else
/// the first made in Imago, else the first of a file, else `fallback`.
pub fn find_settings(prims: &[Prim], fallback: &str) -> Found {
    let settings = |p: &&Prim| p.render.as_ref().is_some_and(|r| r.kind == RenderKind::Settings);
    if let Some(p) = prims.iter().filter(settings).find(|p| p.render.as_ref().is_some_and(|r| r.primary)) {
        return Found::Named(p.path.clone());
    }
    if let Some(p) = prims.iter().find(settings) { return Found::First(p.path.clone()); }
    if let Some(p) = stage_paths(prims, "RenderSettings").into_iter().next() { return Found::InFile(p); }
    Found::Made(fallback.to_string())
}

/// What the render operators' panels show of the scene coming in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Context {
    pub cameras:  Vec<String>,
    pub products: Vec<String>,
    /// The RenderSettings prim found, if any (`None`: one would be made).
    pub settings: Option<Found>,
}

/// The cameras, products and RenderSettings prim of what comes in.
pub fn context(input: &EvalResult) -> Context {
    let prims = prims_in(Some(input));
    let found = find_settings(&prims, "");
    Context {
        cameras: cameras(&prims),
        products: all_of(&prims, RenderKind::Product),
        settings: (!matches!(found, Found::Made(_))).then_some(found),
    }
}

/// The render prim at a path, to be changed: the one coming in, an `over`
/// for one of a file, or a new one.
fn take(prims: &[Prim], path: &str, kind: RenderKind) -> RenderPrim {
    if let Some(r) = prims.iter().find(|p| p.path == path).and_then(|p| p.render.as_ref()).filter(|r| r.kind == kind) {
        return (**r).clone();
    }
    let mut r = RenderPrim::new(kind);
    r.over = stage_paths(prims, kind.usd()).iter().any(|p| p == path);
    r
}

/// Puts a render prim at its path: in place of what was there, else last.
fn put(prims: &mut Vec<Prim>, path: &str, r: RenderPrim) {
    let prim = render_prim(path, r);
    match prims.iter().position(|p| p.path == path) {
        Some(k) => prims[k] = prim,
        None => prims.push(prim),
    }
}

/// The parent of a path (`/Render` for `/Render/Settings`), empty at the root.
fn parent(path: &str) -> &str {
    match path.rfind('/') { Some(k) => &path[..k], None => "" }
}

/// Where a product's RenderVars go: `Vars` beside it.
pub fn vars_root(product: &str) -> String { format!("{}/Vars", parent(product)) }

/// The product a RenderSettings prim at `settings` makes when it has none:
/// `Products/beauty` beside it.
pub fn default_product(settings: &str) -> String { format!("{}/Products/beauty", parent(settings)) }

/// A RenderVar prim made from a row: its source and type, the AOV's name
/// in the image (`driver:parameters:aov:name`, which husk needs on every
/// RenderVar), and any other attributes the renderer reads on it.
fn var_prim(prims: &[Prim], path: &str, aov: &str, source: &str, source_type: &str, data_type: &str, extras: &[(String, Value)]) -> RenderPrim {
    let mut v = take(prims, path, RenderKind::Var);
    v.set(AOV_NAME, RAttr::of(&Value::String(aov.trim().to_string())));
    v.set("sourceName", RAttr::of(&Value::String(source.to_string())).uniform());
    v.set("sourceType", RAttr::of(&Value::Token(source_type.to_string())).uniform());
    v.set("dataType", RAttr::of(&Value::Token(data_type.to_string())).uniform());
    for (name, value) in extras {
        if !name.trim().is_empty() { v.set(name.trim(), RAttr::of(value)); }
    }
    v
}

/// The AOV's name in the image, on a RenderVar: husk wants it on every
/// one, as a `string` (as Houdini writes it; a `token` is not found).
pub const AOV_NAME: &str = "driver:parameters:aov:name";

/// A RenderVar's name made a prim name: letters, digits and `_`.
pub fn var_name(name: &str) -> String {
    prim_location(name, "var").trim_start_matches('/').replace('/', "_")
}

// ── RenderScene Settings ───────────────────────────────────────────────────

/// The `aspectRatioConformPolicy` values, as UsdRender has them.
pub const CONFORM: [(&str, &str); 5] = [
    ("expandAperture", "Expand Aperture"),
    ("cropAperture", "Crop Aperture"),
    ("adjustApertureWidth", "Adjust Aperture Width"),
    ("adjustApertureHeight", "Adjust Aperture Height"),
    ("adjustPixelAspectRatio", "Adjust Pixel Aspect Ratio"),
];

/// The purposes a renderer can render (`includedPurposes`).
pub const PURPOSES: [&str; 4] = ["default", "render", "proxy", "guide"];

/// Common image sizes.
pub const RESOLUTIONS: [(&str, [u32; 2]); 7] = [
    ("HD 720", [1280, 720]),
    ("HD 1080", [1920, 1080]),
    ("2K DCI", [2048, 1080]),
    ("UHD", [3840, 2160]),
    ("4K DCI", [4096, 2160]),
    ("Square 1K", [1024, 1024]),
    ("Square 2K", [2048, 2048]),
];

/// RenderScene Settings: what is rendered, whatever renders it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SceneSettings {
    /// The RenderSettings prim's path.
    pub location:       String,
    /// The camera rendered through. Empty: the first camera of the scene.
    pub camera:         String,
    /// Width and height, in pixels.
    pub resolution:     [u32; 2],
    pub pixel_aspect:   f32,
    /// How a resolution whose aspect is not the camera's gate's is fitted.
    pub conform:        String,
    /// How much is rendered beyond the frame, on every side, as a share of
    /// it: `dataWindowNDC` from -overscan to 1 + overscan.
    pub overscan:       f32,
    pub motion_blur:    bool,
    pub depth_of_field: bool,
    /// The purposes rendered.
    pub purposes:       Vec<String>,
    /// The RenderProduct's path, and the image file it makes.
    pub product:        String,
    pub output:         String,
}

impl Default for SceneSettings {
    fn default() -> Self {
        SceneSettings {
            location: "/Render/Settings".into(),
            camera: String::new(),
            resolution: [1920, 1080],
            pixel_aspect: 1.0,
            conform: "expandAperture".into(),
            overscan: 0.0,
            motion_blur: true,
            depth_of_field: true,
            purposes: vec!["default".into(), "render".into()],
            product: "/Render/Products/beauty".into(),
            output: "render/beauty.$F4.exr".into(),
        }
    }
}

impl SceneSettings {
    pub fn path(&self) -> String { prim_location(&self.location, "/Render/Settings") }
    pub fn product_path(&self) -> String { prim_location(&self.product, &default_product(&self.path())) }
}

/// The scene coming in, with the RenderSettings prim and its product.
pub fn scene_settings(input: Option<&EvalResult>, s: &SceneSettings) -> EvalResult {
    let mut prims = prims_in(input);
    let path = s.path();
    let product = s.product_path();
    // The one named from now on: no other is.
    for p in prims.iter_mut() {
        if let Some(r) = p.render.as_ref().filter(|r| r.primary && p.path != path) {
            let mut r = (**r).clone();
            r.primary = false;
            p.render = Some(Arc::new(r));
        }
    }
    let mut st = take(&prims, &path, RenderKind::Settings);
    st.primary = !st.over;
    let camera = if s.camera.trim().is_empty() { cameras(&prims).into_iter().next() } else { Some(prim_location(&s.camera, "/camera")) };
    match camera {
        Some(c) => st.targets("camera", vec![c]),
        None => { st.rels.remove("camera"); }
    }
    st.set("resolution", RAttr::int2([s.resolution[0].max(1) as i32, s.resolution[1].max(1) as i32]).uniform());
    st.set("pixelAspectRatio", RAttr::of(&Value::Float(s.pixel_aspect.max(1e-3))).uniform());
    st.set("aspectRatioConformPolicy", RAttr::of(&Value::Token(s.conform.clone())).uniform());
    let o = s.overscan.max(0.0);
    st.set("dataWindowNDC", RAttr::of(&Value::Float4([-o, -o, 1.0 + o, 1.0 + o])).uniform());
    st.set("disableMotionBlur", RAttr::of(&Value::Bool(!s.motion_blur)).uniform());
    st.set("disableDepthOfField", RAttr::of(&Value::Bool(!s.depth_of_field)).uniform());
    let purposes: Vec<String> = PURPOSES.iter().filter(|p| s.purposes.iter().any(|x| x == *p)).map(|p| p.to_string()).collect();
    st.set("includedPurposes", RAttr::tokens(&purposes).uniform());
    st.target("products", &product);

    let mut pr = take(&prims, &product, RenderKind::Product);
    pr.set("productType", RAttr::of(&Value::Token("raster".into())).uniform());
    pr.set("productName", RAttr::of(&Value::Token(s.output.trim().to_string())));

    put(&mut prims, &path, st);
    put(&mut prims, &product, pr);
    EvalResult::Named(prims)
}

// ── Renderer definitions ───────────────────────────────────────────────────

/// One renderer setting a definition offers.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Param {
    /// The attribute's name, namespace and all (`karma:global:pathtracedsamples`).
    pub name:    String,
    /// `bool`, `int`, `float`, `double`, `string`, `token`, `asset`,
    /// `float2`, `float3`, `float4` or `color3f`.
    #[serde(rename = "type")]
    pub ty:      String,
    pub default: serde_json::Value,
    #[serde(default)] pub group:   String,
    #[serde(default)] pub label:   String,
    #[serde(default)] pub help:    String,
    #[serde(default)] pub min:     Option<f64>,
    #[serde(default)] pub max:     Option<f64>,
    /// The values a token may take: a list to pick from.
    #[serde(default)] pub choices: Vec<String>,
    /// The name is the renderer's as far as Imago knows, not checked against it.
    #[serde(default)] pub unconfirmed: bool,
}

impl Param {
    /// Its type.
    pub fn value_type(&self) -> ValueType { value_type(&self.ty) }
    /// Its default, of its type.
    pub fn default_value(&self) -> Value { value_of(self.value_type(), &self.default) }
    /// The label shown: its own, else its name.
    pub fn shown(&self) -> &str { if self.label.is_empty() { &self.name } else { &self.label } }
}

/// One of a renderer's standard AOVs.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Aov {
    /// The RenderVar's name.
    pub name:        String,
    #[serde(rename = "sourceName")]
    pub source_name: String,
    #[serde(rename = "sourceType", default = "raw")]
    pub source_type: String,
    #[serde(rename = "dataType", default = "color3f")]
    pub data_type:   String,
    #[serde(default)] pub group:   String,
    #[serde(default)] pub label:   String,
    /// Ticked on a new Render Settings operator.
    #[serde(default)] pub default: bool,
    /// The channels' format in the file (`driver:parameters:aov:format`:
    /// `color4h`, `float3`…). Empty: not written.
    #[serde(default)] pub format: String,
    /// Filtered over the pixel's samples (`driver:parameters:aov:multiSampled`).
    #[serde(default, rename = "multiSampled")] pub multi_sampled: Option<bool>,
    /// Other attributes on its RenderVar.
    #[serde(default)] pub extras: Vec<Param>,
    /// Its source is the renderer's as far as Imago knows, not seen in one of its files.
    #[serde(default)] pub unconfirmed: bool,
}

impl Aov {
    /// What its RenderVar carries beyond its source and type: the
    /// definition's defaults for every RenderVar, its format, its sampling,
    /// its own extras; a later one of the same name wins.
    pub fn var_extras(&self, def: &Definition) -> Vec<(String, Value)> {
        let mut out: Vec<(String, Value)> = def.var_defaults.iter().map(|p| (p.name.clone(), p.default_value())).collect();
        if !self.format.trim().is_empty() { out.push(("driver:parameters:aov:format".into(), Value::Token(self.format.trim().into()))); }
        if let Some(m) = self.multi_sampled { out.push(("driver:parameters:aov:multiSampled".into(), Value::Bool(m))); }
        out.extend(self.extras.iter().map(|p| (p.name.clone(), p.default_value())));
        out
    }
}

fn raw() -> String { "raw".into() }
fn color3f() -> String { "color3f".into() }

/// A renderer's definition: its settings and its standard AOVs. Karma's and
/// Arnold's come with Imago (`assets/renderers`); a JSON file of the same
/// shape in the renderers folder (`renderers_folder`) adds a renderer, or
/// replaces one of the same name.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Definition {
    /// Its key, saved with the operator (`karma`).
    pub name:      String,
    pub label:     String,
    /// What the settings typed in its extra rows are put under (`karma:global`).
    pub namespace: String,
    #[serde(default)] pub about:  String,
    #[serde(default)] pub params: Vec<Param>,
    #[serde(default)] pub aovs:   Vec<Aov>,
    /// Attributes every RenderVar of its AOVs carries (Karma's
    /// `driver:parameters:aov:clearValue`, `…:karma:filter`).
    #[serde(default)] pub var_defaults: Vec<Param>,
}

const EMBEDDED: [&str; 2] = [
    include_str!("../../assets/renderers/karma.json"),
    include_str!("../../assets/renderers/arnold.json"),
];

/// The folder of the user's own renderer definitions: `$XMS_RENDERERS`,
/// else `renderers` in Imago's config folder.
pub fn renderers_folder() -> Option<std::path::PathBuf> {
    std::env::var_os("XMS_RENDERERS").map(std::path::PathBuf::from)
        .or_else(|| crate::file_browser::config_path("renderers"))
}

/// Every renderer definition, read once: those that come with Imago, then
/// the folder's, by file name. Files that do not read are listed in
/// `definition_errors`.
pub fn definitions() -> &'static [Arc<Definition>] {
    &loaded().0
}

/// The renderer definition files that could not be read, and why.
pub fn definition_errors() -> &'static [String] { &loaded().1 }

fn loaded() -> &'static (Vec<Arc<Definition>>, Vec<String>) {
    static DEFS: std::sync::OnceLock<(Vec<Arc<Definition>>, Vec<String>)> = std::sync::OnceLock::new();
    DEFS.get_or_init(|| {
        let mut defs: Vec<Arc<Definition>> = EMBEDDED.iter()
            .map(|text| Arc::new(serde_json::from_str::<Definition>(text).expect("the renderer definitions Imago comes with read"))).collect();
        let mut errors = vec![];
        let mut files: Vec<std::path::PathBuf> = renderers_folder()
            .and_then(|dir| std::fs::read_dir(dir).ok())
            .map(|read| read.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("json"))).collect())
            .unwrap_or_default();
        files.sort();
        for f in files {
            match std::fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Definition>(&t).map_err(|e| e.to_string())) {
                Ok(d) if d.name.trim().is_empty() || d.name == CUSTOM => errors.push(format!("{}: needs a name other than \"{CUSTOM}\"", f.display())),
                Ok(d) => match defs.iter().position(|x| x.name == d.name) {
                    Some(k) => defs[k] = Arc::new(d),
                    None => defs.push(Arc::new(d)),
                },
                Err(e) => errors.push(format!("{}: {e}", f.display())),
            }
        }
        (defs, errors)
    })
}

/// A renderer's definition, by its key.
pub fn definition(name: &str) -> Option<Arc<Definition>> {
    definitions().iter().find(|d| d.name == name).cloned()
}

/// The key of the Custom renderer: no definition, its name typed in.
pub const CUSTOM: &str = "custom";

/// A type named as definitions name it.
pub fn value_type(ty: &str) -> ValueType {
    ValueType::ALL.iter().copied().find(|t| t.usd() == ty.trim()).unwrap_or(ValueType::Float)
}

/// A JSON value as a value of a type.
pub fn value_of(ty: ValueType, j: &serde_json::Value) -> Value {
    let n = |k: usize| -> f32 {
        match j { serde_json::Value::Array(a) => a.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32, other => other.as_f64().unwrap_or(0.0) as f32 }
    };
    let text = || match j { serde_json::Value::String(s) => s.clone(), serde_json::Value::Null => String::new(), other => other.to_string() };
    match ty {
        ValueType::Bool => Value::Bool(j.as_bool().unwrap_or_else(|| j.as_f64().is_some_and(|x| x != 0.0))),
        ValueType::Int => Value::Int(j.as_f64().unwrap_or(0.0).round() as i32),
        ValueType::Float => Value::Float(n(0)),
        ValueType::Double => Value::Double(j.as_f64().unwrap_or(0.0)),
        ValueType::String => Value::String(text()),
        ValueType::Token => Value::Token(text()),
        ValueType::Asset => Value::Asset(text()),
        ValueType::Float2 => Value::Float2([n(0), n(1)]),
        ValueType::Float3 => Value::Float3([n(0), n(1), n(2)]),
        ValueType::Float4 => Value::Float4([n(0), n(1), n(2), n(3)]),
        ValueType::Color3f => Value::Color3f([n(0), n(1), n(2)]),
    }
}

// ── Render Settings ────────────────────────────────────────────────────────

/// A setting typed in: its name and value.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Extra {
    pub name:  String,
    pub value: Value,
}

/// Render Settings: one renderer's settings, and its AOVs.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RendererSettings {
    /// Where the RenderSettings prim is made when none comes in.
    pub location:    String,
    /// The renderer's key (`karma`, `arnold`), or `custom`.
    pub renderer:    String,
    /// Custom: the renderer's name, the namespace of its settings (`rray`).
    pub custom_name: String,
    /// Values changed from a definition's defaults, by attribute name. Kept
    /// for every renderer chosen, so going back to one finds them again.
    pub values:      BTreeMap<String, Value>,
    /// Settings typed in, by renderer (Custom's one set): put under its
    /// namespace unless the name has one (`spp` is `rray:spp`;
    /// `karma:object:x` stays).
    pub extras:      BTreeMap<String, Vec<Extra>>,
    /// The AOVs ticked, by renderer. A renderer not here has its defaults.
    pub aovs:        BTreeMap<String, Vec<String>>,
}

impl Default for RendererSettings {
    fn default() -> Self {
        RendererSettings {
            location: "/Render/Settings".into(),
            renderer: "karma".into(),
            custom_name: String::new(),
            values: BTreeMap::new(),
            extras: BTreeMap::new(),
            aovs: BTreeMap::new(),
        }
    }
}

impl RendererSettings {
    /// A Render Settings operator for a renderer.
    pub fn for_renderer(name: &str) -> Self { RendererSettings { renderer: name.into(), ..Default::default() } }

    pub fn path(&self) -> String { prim_location(&self.location, "/Render/Settings") }

    /// The namespace of the renderer's settings: its definition's, or the
    /// Custom name made one word.
    pub fn namespace(&self) -> String {
        if self.renderer == CUSTOM {
            return self.custom_name.trim().chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == ':' { c } else { '_' }).collect::<String>()
                .trim_matches(':').to_string();
        }
        definition(&self.renderer).map(|d| d.namespace.clone()).unwrap_or_else(|| self.renderer.clone())
    }

    /// An extra row's attribute name: under the namespace unless it has one.
    pub fn full_name(&self, name: &str) -> String {
        let name = name.trim();
        let ns = self.namespace();
        if name.contains(':') || ns.is_empty() { name.to_string() } else { format!("{ns}:{name}") }
    }

    /// A setting's value: as changed, else the definition's default.
    pub fn value(&self, p: &Param) -> Value {
        match self.values.get(&p.name) {
            Some(v) if v.kind() == p.value_type() => v.clone(),
            Some(v) => p.value_type().convert(v),
            None => p.default_value(),
        }
    }

    /// The settings typed in for the renderer chosen.
    pub fn extras(&self) -> &[Extra] {
        self.extras.get(&self.extras_key()).map(|v| v.as_slice()).unwrap_or(&[])
    }
    pub fn extras_mut(&mut self) -> &mut Vec<Extra> {
        let key = self.extras_key();
        self.extras.entry(key).or_default()
    }
    /// By renderer: Custom's are one set, whatever its name, so typing or
    /// changing the name keeps the rows.
    fn extras_key(&self) -> String { self.renderer.clone() }

    /// The AOVs ticked for the renderer chosen.
    pub fn ticked(&self, def: &Definition) -> Vec<String> {
        match self.aovs.get(&def.name) {
            Some(list) => def.aovs.iter().filter(|a| list.contains(&a.name)).map(|a| a.name.clone()).collect(),
            None => def.aovs.iter().filter(|a| a.default).map(|a| a.name.clone()).collect(),
        }
    }

    /// Ticks or unticks an AOV of the renderer chosen.
    pub fn tick(&mut self, def: &Definition, aov: &str, on: bool) {
        let mut list = self.ticked(def);
        list.retain(|a| a != aov);
        if on { list.push(aov.to_string()); }
        self.aovs.insert(def.name.clone(), list);
    }
}

/// The scene coming in, with a renderer's settings on its RenderSettings
/// prim and the AOVs ticked added to its product.
pub fn renderer_settings(input: Option<&EvalResult>, r: &RendererSettings) -> EvalResult {
    let mut prims = prims_in(input);
    let found = find_settings(&prims, &r.path());
    let path = found.path().to_string();
    let mut st = take(&prims, &path, RenderKind::Settings);
    let def = (r.renderer != CUSTOM).then(|| definition(&r.renderer)).flatten();
    if let Some(def) = &def {
        for p in &def.params { st.set(&p.name, RAttr::of(&r.value(p))); }
    }
    // A Custom renderer's settings go under its name: none, none written.
    let named = r.renderer != CUSTOM || !r.namespace().is_empty();
    for e in r.extras().iter().filter(|_| named) {
        if e.name.trim().is_empty() { continue; }
        st.set(&r.full_name(&e.name), RAttr::of(&e.value));
    }
    // The AOVs ticked, as RenderVars of the product.
    let ticked: Vec<&Aov> = match &def {
        Some(d) => { let t = r.ticked(d); d.aovs.iter().filter(|a| t.contains(&a.name)).collect() }
        None => vec![],
    };
    if !ticked.is_empty() {
        let product = product_of(&prims, &st, &path);
        st.target("products", &product);
        let mut pr = take(&prims, &product, RenderKind::Product);
        let root = vars_root(&product);
        let d = def.as_ref().expect("AOVs come from a definition");
        for a in ticked {
            let var = format!("{root}/{}", var_name(&a.name));
            let v = var_prim(&prims, &var, &a.name, &a.source_name, &a.source_type, &a.data_type, &a.var_extras(d));
            put(&mut prims, &var, v);
            pr.target("orderedVars", &var);
        }
        put(&mut prims, &product, pr);
    }
    put(&mut prims, &path, st);
    EvalResult::Named(prims)
}

/// The product a RenderSettings prim renders to: its first, else a file's
/// first (for one of a file, whose `products` Imago does not read), else
/// `Products/beauty` beside it.
fn product_of(prims: &[Prim], st: &RenderPrim, settings: &str) -> String {
    if let Some(p) = st.first("products") { return p.to_string(); }
    if st.over {
        if let Some(p) = stage_paths(prims, "RenderProduct").into_iter().next() { return p; }
    }
    default_product(settings)
}

// ── Render Variables ───────────────────────────────────────────────────────

/// The `sourceType` values, as UsdRender has them.
pub const SOURCE_TYPES: [&str; 4] = ["raw", "lpe", "primvar", "intrinsic"];

/// The `dataType`s AOVs commonly have.
pub const DATA_TYPES: [&str; 12] = [
    "color4f", "color3f", "float", "float2", "float3", "float4", "int", "half", "half3", "normal3f", "point3f", "vector3f",
];

/// One RenderVar, as a row of Render Variables.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct VarRow {
    /// Off: not written, and taken off the product.
    pub on:          bool,
    /// The RenderVar prim's name, and the AOV's in the image.
    pub name:        String,
    pub source_name: String,
    pub source_type: String,
    pub data_type:   String,
    /// What the renderer reads on the RenderVar beyond those: a format
    /// (`driver:parameters:aov:format`), a filter (`arnold:filter`)…
    pub extras:      Vec<Extra>,
}

impl Default for VarRow {
    fn default() -> Self {
        VarRow { on: true, name: String::new(), source_name: String::new(), source_type: "raw".into(), data_type: "color3f".into(), extras: vec![] }
    }
}

/// Extras RenderVars commonly carry, to pick from: name, value, what it is.
pub fn var_extra_presets() -> Vec<(&'static str, Value, &'static str)> {
    vec![
        ("driver:parameters:aov:format", Value::Token("half".into()), "Karma, husk: the channels' format in the file (half, float, color3h, color4h…)"),
        ("driver:parameters:aov:name", Value::String(String::new()), "The channel name in the file: the row's name unless set here"),
        ("arnold:filter", Value::Token("gaussian_filter".into()), "Arnold: the pixel filter (box_filter, gaussian_filter, closest_filter…)"),
        ("arnold:width", Value::Float(2.0), "Arnold: the pixel filter's width"),
    ]
}

/// Render Variables: RenderVars of a product.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Variables {
    /// The product. Empty: the RenderSettings prim's first, else the first
    /// coming in, else `/Render/Products/beauty`.
    pub product: String,
    /// The product holds these alone, in this order: its other RenderVars
    /// are taken off it. Off, these are added to them (or replace those of
    /// the same name).
    pub replace: bool,
    pub rows:    Vec<VarRow>,
}

/// The product Render Variables works on, and where it was found.
pub fn variables_product(prims: &[Prim], v: &Variables) -> String {
    if !v.product.trim().is_empty() { return prim_location(&v.product, "/Render/Products/beauty"); }
    let found = find_settings(prims, "/Render/Settings");
    if let Some(p) = prims.iter().find(|p| p.path == found.path()).and_then(|p| p.render.as_ref()).and_then(|r| r.first("products")) {
        return p.to_string();
    }
    all_of(prims, RenderKind::Product).into_iter().next().unwrap_or_else(|| "/Render/Products/beauty".into())
}

/// The scene coming in, with these RenderVars on the product.
pub fn render_variables(input: Option<&EvalResult>, v: &Variables) -> EvalResult {
    let mut prims = prims_in(input);
    let product = variables_product(&prims, v);
    let mut pr = take(&prims, &product, RenderKind::Product);
    let root = vars_root(&product);
    let mut ordered: Vec<String> = vec![];
    for row in v.rows.iter().filter(|r| !r.name.trim().is_empty()) {
        let var = format!("{root}/{}", var_name(&row.name));
        if !row.on {
            // Off: taken off the product, and not written.
            if let Some(r) = pr.rels.get_mut("orderedVars") { r.targets.retain(|t| *t != var); }
            if let Some(k) = prims.iter().position(|p| p.path == var && p.render.is_some()) { prims.remove(k); }
            continue;
        }
        let source = if row.source_name.trim().is_empty() { row.name.trim() } else { row.source_name.trim() };
        let extras: Vec<(String, Value)> = row.extras.iter().map(|e| (e.name.clone(), e.value.clone())).collect();
        let prim = var_prim(&prims, &var, &row.name, source, &row.source_type, &row.data_type, &extras);
        put(&mut prims, &var, prim);
        ordered.push(var);
    }
    if v.replace {
        pr.targets("orderedVars", ordered);
    } else {
        for var in &ordered { pr.target("orderedVars", var); }
    }
    // A product made here: on the RenderSettings prim made in Imago, if there is one.
    let is_new = !prims.iter().any(|p| p.path == product) && !pr.over;
    put(&mut prims, &product, pr);
    if is_new {
        if let Found::Named(s) | Found::First(s) = find_settings(&prims, "") {
            let mut st = take(&prims, &s, RenderKind::Settings);
            st.target("products", &product);
            put(&mut prims, &s, st);
        }
    }
    EvalResult::Named(prims)
}

// ── Render Command ─────────────────────────────────────────────────────────

/// One environment variable a command runs with.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EnvVar {
    pub name:  String,
    pub value: String,
}

/// Render Command: a command line run on the system, to render the layer
/// the Write USD above it writes. See `core::command`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CommandSettings {
    /// With tokens: `$USD`, `$F4`, `$FSTART`, `$N`… (`command::TOKENS`).
    pub line:         String,
    /// Write the layer first, with the Write USD above, so the render sees
    /// the scene as it is now.
    pub write_first:  bool,
    /// One run per frame (`$F` each frame), else one for the whole range.
    pub per_frame:    bool,
    /// Its own frames, else the scene range.
    pub custom_range: bool,
    pub start:        i64,
    pub end:          i64,
    /// Where it runs. Empty: the layer's folder, else Imago's.
    pub folder:       String,
    pub env:          Vec<EnvVar>,
    /// Make the folders the scene's products write into, before running:
    /// renderers do not.
    pub make_folders: bool,
}

impl Default for CommandSettings {
    fn default() -> Self {
        CommandSettings {
            line: COMMAND_PRESETS[0].1.into(),
            write_first: true,
            per_frame: false,
            custom_range: false,
            start: 1,
            end: 1,
            folder: String::new(),
            env: vec![],
            make_folders: true,
        }
    }
}

impl CommandSettings {
    /// The frames it renders: its own, or the scene's (`start`, `end`).
    pub fn range(&self, scene: (i64, i64)) -> (i64, i64) {
        let (a, b) = if self.custom_range { (self.start, self.end) } else { scene };
        (a.min(b), a.max(b))
    }

    /// The command lines a run runs, tokens put in: one, or one per frame.
    pub fn lines(&self, usd: &str, name: &str, scene: (i64, i64)) -> Vec<String> {
        let (start, end) = self.range(scene);
        let t = crate::core::command::Tokens { usd: usd.to_string(), name: name.to_string(), start, end, frame: start };
        if self.per_frame {
            (start..=end).map(|frame| crate::core::command::expand(&self.line, &crate::core::command::Tokens { frame, ..t.clone() })).collect()
        } else {
            vec![crate::core::command::expand(&self.line, &t)]
        }
    }

    /// Where it runs: its folder, else the layer's.
    pub fn folder_for(&self, usd: &str) -> Option<std::path::PathBuf> {
        if !self.folder.trim().is_empty() { return Some(self.folder.trim().into()); }
        std::path::Path::new(usd.trim()).parent().filter(|p| !p.as_os_str().is_empty()).map(|p| p.to_path_buf())
    }
}

/// The folders the scene's products write into (`productName`'s folder),
/// a relative one taken from `base` (the layer's folder, as husk takes it).
/// A folder with a token in it (`$F`) is left out: the renderer's to make.
pub fn output_folders(prims: &[Prim], base: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out: Vec<std::path::PathBuf> = vec![];
    for r in prims.iter().filter_map(|p| p.render.as_ref()).filter(|r| r.kind == RenderKind::Product) {
        let Some(name) = r.attrs.get("productName").map(|a| a.display()) else { continue };
        let Some(dir) = std::path::Path::new(name.trim()).parent().filter(|d| !d.as_os_str().is_empty()) else { continue };
        if dir.to_string_lossy().contains('$') { continue; }
        let dir = if dir.is_absolute() { dir.to_path_buf() } else { base.join(dir) };
        if !out.contains(&dir) { out.push(dir); }
    }
    out
}

/// Command lines to start from: name, line, what it does. husk takes the
/// RenderSettings prim the layer names, so none is given.
pub const COMMAND_PRESETS: [(&str, &str, &str); 5] = [
    ("husk: Karma CPU", "husk --engine cpu -f $FSTART -n $N -V 2a \"$USD\"", "Karma's CPU engine, through husk (Houdini's bin folder on the PATH)"),
    ("husk: Karma XPU", "husk --engine xpu -f $FSTART -n $N -V 2a \"$USD\"", "Karma XPU, through husk"),
    ("husk: Arnold", "husk -R HdArnoldRendererPlugin -f $FSTART -n $N -V 2a \"$USD\"", "Arnold through husk, with arnold-usd's render delegate installed"),
    ("husk: one frame each", "husk -f $F -n 1 -V 2a \"$USD\"", "One husk per frame: turn on Run per frame"),
    ("Test: echo", "echo Rendering \"$USD\", frames $FSTART to $FEND", "Prints what it would render: to try the tokens"),
];

/// A RenderSettings prim of the scene, as the Image Viewer offers it.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsInfo {
    pub path:     String,
    /// Its camera, when Imago knows it (made in Imago: its `camera`).
    pub camera:   Option<String>,
    /// The renderer its settings are for, by their namespace: `arnold`
    /// when it has `arnold:` settings, else `karma`.
    pub renderer: &'static str,
    /// The one the layer names.
    pub named:    bool,
}

/// Every RenderSettings prim of the scene: those made in Imago (the named
/// one first), then the files'.
pub fn settings_list(prims: &[Prim]) -> Vec<SettingsInfo> {
    let named = layer_settings(prims);
    let mut out: Vec<SettingsInfo> = prims.iter().filter_map(|p| {
        let r = p.render.as_ref().filter(|r| r.kind == RenderKind::Settings)?;
        Some(SettingsInfo {
            path: p.path.clone(),
            camera: r.first("camera").map(str::to_string),
            renderer: if r.attrs.keys().any(|k| k.starts_with("arnold:")) { "arnold" } else { "karma" },
            named: named.as_deref() == Some(p.path.as_str()),
        })
    }).collect();
    out.sort_by_key(|s| !s.named);
    for path in stage_paths(prims, "RenderSettings") {
        if !out.iter().any(|s| s.path == path) { out.push(SettingsInfo { path, camera: None, renderer: "karma", named: false }); }
    }
    out
}

/// The path of the RenderSettings prim a layer names: the one named, else
/// the first made in Imago (not one of a file: that one's file names its own).
pub fn layer_settings(prims: &[Prim]) -> Option<String> {
    let made: Vec<&Prim> = prims.iter().filter(|p| p.render.as_ref().is_some_and(|r| r.kind == RenderKind::Settings && !r.over)).collect();
    made.iter().find(|p| p.render.as_ref().is_some_and(|r| r.primary)).or(made.first()).map(|p| p.path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(r: EvalResult) -> Vec<Prim> { match r { EvalResult::Named(p) => p, _ => panic!("primitives") } }
    fn at<'a>(prims: &'a [Prim], path: &str) -> &'a RenderPrim {
        prims.iter().find(|p| p.path == path).and_then(|p| p.render.as_deref()).unwrap_or_else(|| panic!("{path} is a render prim"))
    }

    #[test]
    fn the_definitions_imago_comes_with_read() {
        for name in ["karma", "arnold"] {
            let d = definition(name).unwrap_or_else(|| panic!("{name}"));
            assert!(!d.params.is_empty() && !d.aovs.is_empty(), "{name}");
            for p in &d.params {
                assert!(p.name.contains(':'), "{name}: {} is namespaced", p.name);
                assert_eq!(p.default_value().kind(), p.value_type(), "{name}: {}", p.name);
                if !p.choices.is_empty() { assert!(p.choices.contains(&p.default_value().display()), "{name}: {}", p.name); }
            }
            assert!(d.aovs.iter().any(|a| a.default), "{name} ticks a beauty");
            for a in &d.aovs { assert!(SOURCE_TYPES.contains(&a.source_type.as_str()), "{name}: {}", a.name); }
        }
        assert!(definition("arnold").unwrap().params.iter().all(|p| p.name.starts_with("arnold:global:")));
    }

    #[test]
    fn scene_settings_make_the_settings_and_a_product() {
        let cam = Prim { camera: Some(Arc::new(Default::default())), ..Prim::new("/cams/main".into(), MeshData::default()) };
        let s = SceneSettings { overscan: 0.1, motion_blur: false, ..Default::default() };
        let prims = named(scene_settings(Some(&EvalResult::Named(vec![cam])), &s));
        assert_eq!(prims.len(), 3, "the camera, the settings, the product");
        let st = at(&prims, "/Render/Settings");
        assert!(st.primary && !st.over);
        let lines = st.lines();
        for l in [
            "uniform int2 resolution = (1920, 1080)",
            "uniform float4 dataWindowNDC = (-0.1, -0.1, 1.1, 1.1)",
            "uniform bool disableMotionBlur = true",
            "uniform bool disableDepthOfField = false",
            "uniform token[] includedPurposes = [\"default\", \"render\"]",
            "uniform token aspectRatioConformPolicy = \"expandAperture\"",
            "rel camera = </cams/main>",
            "rel products = </Render/Products/beauty>",
        ] {
            assert!(lines.iter().any(|x| x == l), "{l} in {lines:#?}");
        }
        let pr = at(&prims, "/Render/Products/beauty");
        assert_eq!(pr.attrs["productName"].display(), "render/beauty.$F4.exr");
        assert_eq!(pr.attrs["productType"].line("productType"), "uniform token productType = \"raster\"");
        assert_eq!(layer_settings(&prims).as_deref(), Some("/Render/Settings"));
    }

    #[test]
    fn render_settings_find_the_settings_without_a_wire() {
        // Made by RenderScene Settings somewhere else in the scene.
        let s = SceneSettings { location: "/shot/rs".into(), product: "/shot/out/main".into(), ..Default::default() };
        let scene = scene_settings(None, &s);
        let mut r = RendererSettings::for_renderer("arnold");
        r.values.insert("arnold:global:AA_samples".into(), Value::Int(6));
        r.extras_mut().push(Extra { name: "abort_on_error".into(), value: Value::Bool(false) });
        r.tick(&definition("arnold").unwrap(), "N", true);
        let prims = named(renderer_settings(Some(&scene), &r));
        assert!(prims.iter().all(|p| p.path != "/Render/Settings"), "it found /shot/rs, made none");
        let st = at(&prims, "/shot/rs");
        assert_eq!(st.attrs["arnold:global:AA_samples"].line("arnold:global:AA_samples"), "int arnold:global:AA_samples = 6");
        assert_eq!(st.attrs["arnold:global:GI_diffuse_samples"].literal, "2", "a default is written");
        assert!(st.attrs.contains_key("arnold:global:abort_on_error"), "a typed name goes under the namespace");
        // RGBA by default, and N: Vars beside the product, in its orderedVars.
        let pr = at(&prims, "/shot/out/main");
        assert_eq!(pr.rels["orderedVars"].targets, vec!["/shot/out/Vars/RGBA".to_string(), "/shot/out/Vars/N".to_string()]);
        let n = at(&prims, "/shot/out/Vars/N");
        assert_eq!(n.kind, RenderKind::Var);
        assert!(n.lines().contains(&"uniform token dataType = \"normal3f\"".to_string()));
        assert!(n.lines().contains(&"uniform string sourceName = \"N\"".to_string()));
        assert!(n.lines().contains(&"string driver:parameters:aov:name = \"N\"".to_string()), "husk needs it: {:?}", n.lines());
    }

    /// Karma's RenderVars as Houdini 20.5 writes them: the beauty a light
    /// path expression, with format, filter and multi-sampling.
    #[test]
    fn karma_aovs_are_written_as_houdini_writes_them() {
        let mut r = RendererSettings::default();
        r.tick(&definition("karma").unwrap(), "P", true);
        let prims = named(renderer_settings(None, &r));
        let c = at(&prims, "/Render/Products/Vars/C").lines();
        for l in ["uniform string sourceName = \"C.*[LO]\"", "uniform token sourceType = \"lpe\"", "uniform token dataType = \"color4f\"",
            "token driver:parameters:aov:format = \"color4h\"", "bool driver:parameters:aov:multiSampled = true", "int driver:parameters:aov:clearValue = 0",
            "string driver:parameters:aov:karma:filter = \"[\\\"ubox\\\",{}]\"", "string driver:parameters:aov:name = \"C\""] {
            assert!(c.contains(&l.to_string()), "{l} in {c:#?}");
        }
        let p = at(&prims, "/Render/Products/Vars/P").lines();
        assert!(p.contains(&"uniform string sourceName = \"ray:hitP\"".to_string()) && p.contains(&"token driver:parameters:aov:format = \"float3\"".to_string()), "{p:#?}");
    }

    #[test]
    fn custom_settings_are_named_as_typed() {
        let mut r = RendererSettings { renderer: CUSTOM.into(), custom_name: "rray".into(), ..Default::default() };
        r.extras_mut().push(Extra { name: "spp".into(), value: Value::Int(64) });
        r.extras_mut().push(Extra { name: "bounce".into(), value: Value::Int(4) });
        r.extras_mut().push(Extra { name: "other:thing".into(), value: Value::Float(1.0) });
        // Typed in before the name: the rows stay when it is typed.
        let mut unnamed = r.clone();
        unnamed.custom_name.clear();
        let st = named(renderer_settings(None, &unnamed));
        assert!(at(&st, "/Render/Settings").attrs.is_empty(), "no name, nothing written (not a bare spp)");
        assert_eq!(unnamed.extras().len(), 3, "the rows are kept with no name");
        let prims = named(renderer_settings(None, &r));
        assert_eq!(prims.len(), 1, "a RenderSettings prim, no AOVs: Custom has none");
        let st = at(&prims, "/Render/Settings");
        let lines = st.lines();
        assert!(lines.contains(&"int rray:spp = 64".to_string()) && lines.contains(&"int rray:bounce = 4".to_string()), "{lines:?}");
        assert!(lines.contains(&"float other:thing = 1".to_string()), "a name with a namespace stays");
        assert!(!st.primary, "only RenderScene Settings names one");
    }

    #[test]
    fn render_variables_add_override_and_replace() {
        let scene = scene_settings(None, &SceneSettings::default());
        let karma = RendererSettings::default();
        let scene = renderer_settings(Some(&scene), &karma);
        let row = |name: &str, source: &str, ty: &str| VarRow { name: name.into(), source_name: source.into(), source_type: ty.into(), ..Default::default() };
        let mut v = Variables { rows: vec![row("C", "Ci", "raw"), row("spec", "C<RS>.*L", "lpe")], ..Default::default() };
        v.rows[1].extras.push(Extra { name: "driver:parameters:aov:format".into(), value: Value::Token("half".into()) });
        let prims = named(render_variables(Some(&scene), &v));
        let pr = at(&prims, "/Render/Products/beauty");
        assert_eq!(pr.rels["orderedVars"].targets, vec!["/Render/Products/Vars/C".to_string(), "/Render/Products/Vars/spec".to_string()], "C overridden in place, spec added");
        assert_eq!(at(&prims, "/Render/Products/Vars/C").attrs["sourceName"].display(), "Ci");
        assert!(at(&prims, "/Render/Products/Vars/spec").lines().contains(&"token driver:parameters:aov:format = \"half\"".to_string()));
        // Replacing: spec alone.
        let only = Variables { replace: true, rows: vec![row("spec", "C<RS>.*L", "lpe")], ..Default::default() };
        let prims = named(render_variables(Some(&scene), &only));
        assert_eq!(at(&prims, "/Render/Products/beauty").rels["orderedVars"].targets, vec!["/Render/Products/Vars/spec".to_string()]);
        // Off: taken off.
        let off = Variables { rows: vec![VarRow { on: false, ..row("C", "C", "raw") }], ..Default::default() };
        let prims = named(render_variables(Some(&scene), &off));
        assert!(at(&prims, "/Render/Products/beauty").rels["orderedVars"].targets.is_empty());
        assert!(prims.iter().all(|p| p.path != "/Render/Products/Vars/C"));
    }

    #[test]
    fn a_render_command_has_its_lines() {
        let c = CommandSettings { line: "husk -f $F -o out.$F4.exr \"$USD\"".into(), per_frame: true, custom_range: true, start: 3, end: 1, ..Default::default() };
        assert_eq!(c.lines("/s/a.usda", "R", (1, 100)), vec![
            "husk -f 1 -o out.0001.exr \"/s/a.usda\"".to_string(), "husk -f 2 -o out.0002.exr \"/s/a.usda\"".into(), "husk -f 3 -o out.0003.exr \"/s/a.usda\"".into(),
        ], "its own range, in order");
        let once = CommandSettings::default();
        assert_eq!(once.lines("/s/a.usda", "R", (1, 24)), vec!["husk --engine cpu -f 1 -n 24 -V 2a \"/s/a.usda\"".to_string()]);
        assert_eq!(once.folder_for("/s/a.usda").as_deref(), Some(std::path::Path::new("/s")));
        assert_eq!(once.folder_for("a.usda"), None);
    }

    #[test]
    fn the_folders_products_write_into() {
        let scene = scene_settings(None, &SceneSettings::default());
        let EvalResult::Named(prims) = scene else { panic!() };
        assert_eq!(output_folders(&prims, std::path::Path::new("/shots/a")), vec![std::path::PathBuf::from("/shots/a/render")]);
        let abs = scene_settings(None, &SceneSettings { output: "/out/$F4/x.exr".into(), ..Default::default() });
        let EvalResult::Named(prims) = abs else { panic!() };
        assert!(output_folders(&prims, std::path::Path::new("/shots")).is_empty(), "a token in the folder: the renderer's");
    }

    #[test]
    fn the_settings_of_a_scene_are_listed() {
        let cam = Prim { camera: Some(Arc::new(Default::default())), ..Prim::new("/cam".into(), MeshData::default()) };
        let one = scene_settings(Some(&EvalResult::Named(vec![cam])), &SceneSettings { location: "/a/rs".into(), ..Default::default() });
        let two = scene_settings(Some(&one), &SceneSettings { location: "/b/rs".into(), product: "/b/out".into(), camera: "/other".into(), ..Default::default() });
        let two = renderer_settings(Some(&two), &RendererSettings::for_renderer("arnold"));
        let EvalResult::Named(prims) = two else { panic!() };
        let list = settings_list(&prims);
        assert_eq!(list.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(), ["/b/rs", "/a/rs"], "the named one first");
        assert!(list[0].named && !list[1].named);
        assert_eq!((list[0].camera.as_deref(), list[0].renderer), (Some("/other"), "arnold"));
        assert_eq!((list[1].camera.as_deref(), list[1].renderer), (Some("/cam"), "karma"));
    }

    #[test]
    fn relationships_on_an_over_are_appended() {
        let mut r = RenderPrim::new(RenderKind::Product);
        r.over = true;
        r.target("orderedVars", "/Render/Vars/a");
        assert_eq!(r.lines(), vec!["append rel orderedVars = [</Render/Vars/a>]".to_string()]);
        r.targets("orderedVars", vec!["/Render/Vars/b".into()]);
        assert_eq!(r.lines(), vec!["rel orderedVars = </Render/Vars/b>".to_string()]);
    }
}
