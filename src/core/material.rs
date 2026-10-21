//! Materials made in Imago: a UsdPreviewSurface with fixed inputs.
//!
//! The Material operator puts out one `Material`: the surface's inputs and,
//! for some of them, a texture file feeding the input. It is named by its
//! operator. Material Binding puts it in the scene, under its Material
//! Library Location, and binds it to prims.
//!
//! In USD it is a `Material` prim holding a `UsdPreviewSurface` shader, one
//! `UsdUVTexture` per texture file and one `UsdPrimvarReader_float2` reading
//! `st` for them (`network`). The viewport shows it as a `Look`; viewed on
//! its own, on a sphere (`preview`).
//!
//! In the scene, a material Material Binding put there is a primitive with
//! no geometry whose `Prim::shading` is the material: listed, inspected and
//! written as any other prim (`bind`).
//!
//! The MaterialX operator puts out the same `Material`, holding its graph
//! (`graph`): written as a MaterialX network, shown by what the Viewer can
//! read of its surface shader (`materialx::surface_of`). Material Binding
//! takes either.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

use crate::types::{EvalResult, Look, MeshData, Prim};

/// The inputs of a UsdPreviewSurface the Material operator sets. Defaults
/// are the UsdPreviewSurface ones.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PreviewSurface {
    pub diffuse_color:       [f32; 3],
    pub emissive_color:      [f32; 3],
    pub metallic:            f32,
    pub roughness:           f32,
    pub clearcoat:           f32,
    pub clearcoat_roughness: f32,
    pub opacity:             f32,
    /// Below this opacity the surface is cut out; 0: see-through instead.
    pub opacity_threshold:   f32,
    pub ior:                 f32,
    /// Texture files by the input they feed (`TEXTURED`). An input fed by a
    /// texture takes its value from it.
    pub textures:            BTreeMap<String, String>,
}

impl Default for PreviewSurface {
    fn default() -> Self {
        Self {
            diffuse_color: [0.18; 3], emissive_color: [0.0; 3], metallic: 0.0, roughness: 0.5,
            clearcoat: 0.0, clearcoat_roughness: 0.01, opacity: 1.0, opacity_threshold: 0.0, ior: 1.5,
            textures: BTreeMap::new(),
        }
    }
}

/// The inputs a texture can feed, with their labels: the colour inputs read
/// its colour, the others its first channel.
pub const TEXTURED: [(&str, &str); 7] = [
    ("diffuseColor", "Diffuse"), ("emissiveColor", "Emissive"), ("metallic", "Metallic"),
    ("roughness", "Roughness"), ("opacity", "Opacity"), ("normal", "Normal"), ("occlusion", "Occlusion"),
];

impl PreviewSurface {
    /// The texture feeding an input, if one is set.
    pub fn texture(&self, input: &str) -> Option<&str> {
        self.textures.get(input).map(|s| s.trim()).filter(|s| !s.is_empty())
    }

    /// The scalar inputs, by name, with their values and defaults.
    fn floats(&self) -> [(&'static str, f32, f32); 7] {
        [
            ("metallic", self.metallic, 0.0), ("roughness", self.roughness, 0.5),
            ("clearcoat", self.clearcoat, 0.0), ("clearcoatRoughness", self.clearcoat_roughness, 0.01),
            ("opacity", self.opacity, 1.0), ("opacityThreshold", self.opacity_threshold, 0.0), ("ior", self.ior, 1.5),
        ]
    }
}

/// A material as operators pass it on: its name and its surface.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    /// Its prim name: the operator's name, as USD allows it (`prim_name`).
    pub name:    String,
    /// The UsdPreviewSurface; for a MaterialX material, what the Viewer
    /// shows of its graph.
    pub surface: PreviewSurface,
    /// How the viewport shows it.
    pub look:    Arc<Look>,
    /// A MaterialX operator's graph: what is written, in place of `surface`.
    pub graph:   Option<Arc<xms_matx::Graph>>,
}

impl Material {
    pub fn new(name: &str, surface: &PreviewSurface) -> Self {
        let name = prim_name(name);
        Self { look: Arc::new(look(&name, surface)), name, surface: surface.clone(), graph: None }
    }

    /// A MaterialX operator's material: its graph, shown as its surface.
    pub fn from_graph(name: &str, graph: &xms_matx::Graph) -> Self {
        let surface = crate::materialx::surface_of(graph);
        Self { graph: Some(Arc::new(graph.clone())), ..Material::new(name, &surface) }
    }
}

/// A name USD takes as a prim name: letters, digits and `_`, not starting
/// with a digit. Anything else becomes `_`.
pub fn prim_name(name: &str) -> String {
    let mut out: String = name.trim().chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if out.is_empty() { out = "material".into(); }
    if out.starts_with(|c: char| c.is_ascii_digit()) { out.insert(0, '_'); }
    out
}

/// The surface as the viewport shows it. A textured colour is white under
/// its texture, as USD gives it.
fn look(name: &str, s: &PreviewSurface) -> Look {
    let file = |input: &str| s.texture(input).map(std::path::PathBuf::from);
    Look {
        name: name.to_string(),
        color: if file("diffuseColor").is_some() { [1.0; 3] } else { s.diffuse_color },
        roughness: s.roughness,
        metallic: s.metallic,
        opacity: if file("opacity").is_some() { 1.0 } else { s.opacity },
        color_map: file("diffuseColor"),
        emissive_map: file("emissiveColor"),
        cutout: file("opacity").is_some(),
    }
}

/// A material viewed on its own: a sphere with its look, at the origin.
pub fn preview(m: &Material) -> EvalResult {
    static SPHERE: OnceLock<Arc<MeshData>> = OnceLock::new();
    let mesh = SPHERE.get_or_init(|| Arc::new(sphere(48))).clone();
    EvalResult::Named(vec![Prim { mesh, look: Some(m.look.clone()), ..Prim::new(format!("/{}", m.name), MeshData::default()) }])
}

/// A sphere of radius 0.5 with texture coordinates: around it, then from
/// the bottom to the top.
fn sphere(segments: u32) -> MeshData {
    let mut m = crate::node_graph::nodes::create_sphere(0.5, segments);
    let row = segments as usize + 1;
    let uv = |k: u32| {
        let (lat, lon) = (k as usize / row, k as usize % row);
        [lon as f32 / segments as f32, 1.0 - lat as f32 / segments as f32]
    };
    m.uvs = m.indices.iter().map(|&k| uv(k)).collect();
    m
}

// ── USD ──────────────────────────────────────────────────────────────────────

/// A shader prim of a material's network: its name under the material, and
/// its attribute lines.
#[derive(Clone, Debug, PartialEq)]
pub struct Shader {
    pub name:  String,
    pub lines: Vec<String>,
}

/// The material at `path` as USD has it: the Material's own lines (its
/// `outputs:surface`) and its shaders. The surface shader has every input
/// that is not its default (colour, metallic, roughness and opacity always);
/// a texture node per file, and the reader of `st` they share. An input fed
/// by a texture is connected, with no value of its own. Opacity from the
/// diffuse file reads that texture's alpha.
pub fn network(m: &Material, path: &str) -> (Vec<String>, Vec<Shader>) {
    // A MaterialX graph: its own network. One with nothing going into its
    // material is written as the grey it shows.
    if let Some(g) = &m.graph {
        if let Some((lines, shaders)) = xms_matx::usd::network(g, crate::materialx::lib(), path) {
            return (lines, shaders.into_iter().map(|s| Shader { name: s.name, lines: s.lines }).collect());
        }
    }
    let s = &m.surface;
    let surface = format!("{path}/PreviewSurface");
    let texture = |input: &str| format!("{path}/{input}Texture");
    // Opacity read from the diffuse texture's alpha.
    let alpha = s.texture("opacity").is_some() && s.texture("opacity") == s.texture("diffuseColor");
    let fed = |input: &str| -> Option<String> {
        s.texture(input)?;
        Some(match input {
            "opacity" if alpha => format!("{}.outputs:a", texture("diffuseColor")),
            "diffuseColor" | "emissiveColor" | "normal" => format!("{}.outputs:rgb", texture(input)),
            _ => format!("{}.outputs:r", texture(input)),
        })
    };

    let mut lines = vec!["uniform token info:id = \"UsdPreviewSurface\"".to_string()];
    let colour = |c: [f32; 3]| format!("({}, {}, {})", num(c[0]), num(c[1]), num(c[2]));
    for (input, value, default) in [("diffuseColor", s.diffuse_color, None), ("emissiveColor", s.emissive_color, Some([0.0; 3]))] {
        match fed(input) {
            Some(src) => lines.push(format!("color3f inputs:{input}.connect = <{src}>")),
            None if Some(value) != default => lines.push(format!("color3f inputs:{input} = {}", colour(value))),
            None => {}
        }
    }
    for (input, value, default) in s.floats() {
        let always = matches!(input, "metallic" | "roughness" | "opacity");
        match fed(input) {
            Some(src) => lines.push(format!("float inputs:{input}.connect = <{src}>")),
            None if always || value != default => lines.push(format!("float inputs:{input} = {}", num(value))),
            None => {}
        }
    }
    if let Some(src) = fed("normal") { lines.push(format!("normal3f inputs:normal.connect = <{src}>")); }
    if let Some(src) = fed("occlusion") { lines.push(format!("float inputs:occlusion.connect = <{src}>")); }
    lines.push("token outputs:surface".into());
    let mut shaders = vec![Shader { name: "PreviewSurface".into(), lines }];

    let files: Vec<(&str, &str)> = TEXTURED.iter()
        .filter(|(input, _)| !(*input == "opacity" && alpha))
        .filter_map(|(input, _)| s.texture(input).map(|f| (*input, f)))
        .collect();
    if !files.is_empty() {
        shaders.push(Shader { name: "stReader".into(), lines: vec![
            "uniform token info:id = \"UsdPrimvarReader_float2\"".into(),
            "string inputs:varname = \"st\"".into(),
            "float2 outputs:result".into(),
        ] });
    }
    for (input, file) in files {
        // Colours are stored in sRGB; values (and normals) as they are.
        let srgb = matches!(input, "diffuseColor" | "emissiveColor");
        let mut lines = vec![
            "uniform token info:id = \"UsdUVTexture\"".to_string(),
            format!("asset inputs:file = @{file}@"),
            format!("float2 inputs:st.connect = <{path}/stReader.outputs:result>"),
            format!("token inputs:sourceColorSpace = \"{}\"", if srgb { "sRGB" } else { "raw" }),
        ];
        if input == "normal" {
            // From 0..1 in the file to -1..1.
            lines.push("float4 inputs:scale = (2, 2, 2, 1)".into());
            lines.push("float4 inputs:bias = (-1, -1, -1, 0)".into());
        }
        lines.push(match input {
            "diffuseColor" | "emissiveColor" | "normal" => "float3 outputs:rgb".into(),
            _ => "float outputs:r".into(),
        });
        if input == "diffuseColor" && alpha { lines.push("float outputs:a".into()); }
        shaders.push(Shader { name: format!("{input}Texture"), lines });
    }
    (vec![format!("token outputs:surface.connect = <{surface}.outputs:surface>")], shaders)
}

/// The surface's inputs as the Primitive Inspector lists them: a value, or
/// the texture file feeding it.
pub fn inputs(m: &Material) -> Vec<(String, String)> {
    // A MaterialX graph: its surface shader's definition, and what is set
    // or connected on it.
    if let Some(g) = &m.graph {
        if let Some(node) = g.surface_shader().and_then(|s| g.node(s)) {
            let mut out = vec![("info:id".to_string(), node.def.clone())];
            for (input, b) in &node.inputs {
                let value = match b {
                    xms_matx::Binding::Value(v) => v.to_text(),
                    xms_matx::Binding::Connect { node: src, output } => {
                        format!("← {}.{output}", g.node(*src).map(|n| n.name.as_str()).unwrap_or("?"))
                    }
                };
                out.push((format!("inputs:{input}"), value));
            }
            out.push(("MaterialX graph".into(), format!("{} nodes", g.nodes.len())));
            return out;
        }
    }
    let s = &m.surface;
    let colour = |c: [f32; 3]| format!("({}, {}, {})", num(c[0]), num(c[1]), num(c[2]));
    let mut out = vec![("info:id".to_string(), "UsdPreviewSurface".to_string())];
    let mut add = |name: &str, value: String| out.push((format!("inputs:{name}"), match s.texture(name) {
        Some(file) => format!("texture {file}"),
        None => value,
    }));
    add("diffuseColor", colour(s.diffuse_color));
    add("emissiveColor", colour(s.emissive_color));
    for (name, value, _) in s.floats() { add(name, num(value)); }
    for name in ["normal", "occlusion"] {
        if let Some(file) = s.texture(name) { out.push((format!("inputs:{name}"), format!("texture {file}"))); }
    }
    out
}

fn num(x: f32) -> String { if x.is_finite() { format!("{x}") } else { "0".into() } }

// ── Binding ──────────────────────────────────────────────────────────────────

/// What a Material Binding row binds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum BindTo {
    /// The material input of the row's number.
    #[default]
    Same,
    /// A material input, by number (from 1).
    Input(u32),
    /// A material already in the scene, by path.
    Path(String),
}

/// The Material Library Location as a USD path: `/materials` when blank.
pub fn library_path(text: &str) -> String { crate::types::prim_location(text, "/materials") }

/// Where each material input's material goes: `library`/its name. `None`
/// for an input with no material, and for a name an earlier input has
/// (the first is used).
pub fn placed(library: &str, materials: &[Option<Arc<Material>>]) -> Vec<Option<String>> {
    let library = library_path(library);
    let mut seen = std::collections::HashSet::new();
    materials.iter().map(|m| m.as_ref().and_then(|m| {
        let path = format!("{library}/{}", m.name);
        seen.insert(path.clone()).then_some(path)
    })).collect()
}

/// The material a row binds: its path, if it has one.
pub fn chosen(row: usize, to: &[BindTo], placed: &[Option<String>]) -> Option<String> {
    match to.get(row).cloned().unwrap_or_default() {
        BindTo::Same => placed.get(row).cloned().flatten(),
        BindTo::Input(n) => (n as usize).checked_sub(1).and_then(|k| placed.get(k).cloned().flatten()),
        BindTo::Path(p) => Some(p.trim().to_string()).filter(|p| !p.is_empty()),
    }
}

/// Material Binding: the materials put in the scene, each at its place under
/// the Material Library Location (after the primitives; one already on that
/// path is replaced), and each row's material bound to the primitives its
/// location takes: the one at that path and every one below it. On a
/// primitive two rows take, the later row wins. Rows with no location, or
/// no material, bind nothing. A point instancer is not bound itself: its
/// prototypes, below it, are.
pub fn bind(scene: Option<&EvalResult>, materials: &[Option<Arc<Material>>], library: &str, rows: &[String], to: &[BindTo]) -> EvalResult {
    let mut prims = match scene {
        Some(EvalResult::Named(p)) => p.clone(),
        Some(EvalResult::Single(m)) => vec![Prim::new("/mesh".into(), m.clone())],
        _ => vec![],
    };
    let places = placed(library, materials);
    // How each material is drawn, by path: those primitives wear already,
    // those in the scene, those put in now.
    let mut looks: HashMap<String, Arc<Look>> = HashMap::new();
    for p in &prims {
        if let (Some(m), Some(l)) = (&p.material, &p.look) { looks.entry(m.clone()).or_insert_with(|| l.clone()); }
        if let Some(m) = &p.shading { looks.insert(p.path.clone(), m.look.clone()); }
    }
    for (path, m) in places.iter().zip(materials) {
        if let (Some(path), Some(m)) = (path, m) { looks.insert(path.clone(), m.look.clone()); }
    }
    let wanted: Vec<(&str, String)> = rows.iter().enumerate()
        .filter(|(_, row)| !row.trim().is_empty())
        .filter_map(|(k, row)| chosen(k, to, &places).map(|m| (row.as_str(), m)))
        .collect();
    for p in prims.iter_mut().filter(|p| p.is_geometry() && p.instancing.is_none()) {
        if let Some((_, m)) = wanted.iter().rev().find(|(row, _)| crate::core::locations::matches(row, &p.path)) {
            p.material = Some(m.clone());
            p.look = looks.get(m).cloned();
        }
    }
    for (path, m) in places.into_iter().zip(materials) {
        let (Some(path), Some(m)) = (path, m) else { continue };
        let prim = Prim { shading: Some(m.clone()), ..Prim::new(path, MeshData::default()) };
        match prims.iter().position(|p| p.path == prim.path) {
            Some(k) => prims[k] = prim,
            None => prims.push(prim),
        }
    }
    EvalResult::Named(prims)
}

/// The materials a scene has, by path: those of its stages and those put in
/// by Material Binding. For a row's Material column.
pub fn scene_materials(scene: &EvalResult) -> Vec<String> {
    let EvalResult::Named(prims) = scene else { return vec![] };
    let mut out: Vec<String> = vec![];
    let mut trees: Vec<&Arc<crate::usd_scene::StageTree>> = vec![];
    for p in prims {
        if let Some(t) = &p.stage { if !trees.iter().any(|x| Arc::ptr_eq(x, t)) { trees.push(t); } }
    }
    for t in trees {
        out.extend(t.nodes.iter().filter(|n| n.type_name == "Material").map(|n| n.path.clone()));
    }
    for p in prims {
        if p.shading.is_some() && !out.contains(&p.path) { out.push(p.path.clone()); }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_usd_prim_names() {
        assert_eq!(prim_name("Material"), "Material");
        assert_eq!(prim_name("car paint 2"), "car_paint_2");
        assert_eq!(prim_name("2tone"), "_2tone");
        assert_eq!(prim_name("  "), "material");
    }

    /// A MaterialX operator's material is written as its MaterialX
    /// network, and shown by its surface shader's values.
    #[test]
    fn a_materialx_material_is_its_network() {
        let mut g = crate::materialx::new_graph();
        let sh = g.surface_shader().unwrap();
        g.set_value(sh, "base_color", xms_matx::Value::Floats(vec![0.1, 0.2, 0.9]));
        let m = Material::from_graph("blue glass", &g);
        assert_eq!(m.name, "blue_glass");
        assert_eq!(m.look.color, [0.1, 0.2, 0.9]);
        let (lines, shaders) = network(&m, "/materials/blue_glass");
        assert_eq!(lines, ["token outputs:mtlx:surface.connect = </materials/blue_glass/standard_surface1.outputs:out>"]);
        assert_eq!(shaders.len(), 1);
        assert!(shaders[0].lines.contains(&"color3f inputs:base_color = (0.1, 0.2, 0.9)".to_string()), "{:?}", shaders[0].lines);
        let listed = inputs(&m);
        assert_eq!(listed[0], ("info:id".to_string(), "ND_standard_surface_surfaceshader".to_string()));
        // Nothing into its material: written as the grey it shows.
        g.remove(sh);
        let (lines, shaders) = network(&Material::from_graph("empty", &g), "/m");
        assert!(lines[0].contains("PreviewSurface"), "{lines:?}");
        assert_eq!(shaders[0].name, "PreviewSurface");
    }

    #[test]
    fn the_look_follows_the_surface() {
        let mut s = PreviewSurface { diffuse_color: [0.8, 0.1, 0.1], roughness: 0.3, ..Default::default() };
        let m = Material::new("red", &s);
        assert_eq!((m.look.color, m.look.roughness, m.look.color_map.is_none()), ([0.8, 0.1, 0.1], 0.3, true));
        s.textures.insert("diffuseColor".into(), "/tex/red.png".into());
        let m = Material::new("red", &s);
        assert_eq!(m.look.color, [1.0; 3], "white under its texture");
        assert_eq!(m.look.color_map.as_deref(), Some(std::path::Path::new("/tex/red.png")));
        // A viewed material is a sphere wearing it.
        let EvalResult::Named(p) = preview(&m) else { panic!() };
        assert!(Arc::ptr_eq(p[0].look.as_ref().unwrap(), &m.look));
        assert_eq!(p[0].mesh.uvs.len(), p[0].mesh.indices.len());
    }

    #[test]
    fn the_network_connects_textures() {
        let mut s = PreviewSurface { ior: 1.33, ..Default::default() };
        s.textures.insert("diffuseColor".into(), "paint.png".into());
        s.textures.insert("opacity".into(), "paint.png".into());
        let (own, shaders) = network(&Material::new("paint", &s), "/materials/paint");
        assert_eq!(own, vec!["token outputs:surface.connect = </materials/paint/PreviewSurface.outputs:surface>".to_string()]);
        let names: Vec<&str> = shaders.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["PreviewSurface", "stReader", "diffuseColorTexture"], "opacity reads the diffuse file's alpha");
        let surface = &shaders[0].lines;
        assert!(surface.contains(&"float inputs:ior = 1.33".to_string()) && !surface.iter().any(|l| l.contains("clearcoat")));
        assert!(surface.contains(&"float inputs:opacity.connect = </materials/paint/diffuseColorTexture.outputs:a>".to_string()));
        assert!(!surface.iter().any(|l| l.starts_with("color3f inputs:diffuseColor =")), "a textured input has no value");
    }

    fn mat(name: &str) -> Option<Arc<Material>> { Some(Arc::new(Material::new(name, &PreviewSurface::default()))) }

    #[test]
    fn rows_bind_their_materials_and_the_later_row_wins() {
        let prims: Vec<Prim> = ["/car/body", "/car/wheels/fl", "/car/wheels/fr", "/lamp"].iter()
            .map(|p| Prim::new(p.to_string(), MeshData::default())).collect();
        let scene = EvalResult::Named(prims);
        let rows = vec!["/car".to_string(), "/car/wheels".to_string(), String::new(), "/lamp".to_string()];
        // Row 4 picks input 1 by number; input 3 has nothing plugged.
        let to = vec![BindTo::Same, BindTo::Same, BindTo::Same, BindTo::Input(1)];
        let EvalResult::Named(out) = bind(Some(&scene), &[mat("paint"), mat("chrome"), None], "", &rows, &to) else { panic!() };
        let bound = |path: &str| out.iter().find(|p| p.path == path).and_then(|p| p.material.clone());
        assert_eq!(bound("/car/body").as_deref(), Some("/materials/paint"));
        assert_eq!(bound("/car/wheels/fl").as_deref(), Some("/materials/chrome"), "the later row wins");
        assert_eq!(bound("/lamp").as_deref(), Some("/materials/paint"));
        assert!(out.iter().find(|p| p.path == "/car/body").unwrap().look.is_some());
        // The materials, after the primitives, under the library.
        let made: Vec<&str> = out.iter().filter(|p| p.shading.is_some()).map(|p| p.path.as_str()).collect();
        assert_eq!(made, vec!["/materials/paint", "/materials/chrome"]);
        assert_eq!(scene_materials(&EvalResult::Named(out)), vec!["/materials/paint".to_string(), "/materials/chrome".to_string()]);
    }

    #[test]
    fn a_name_used_twice_is_placed_once() {
        let places = placed("/looks/", &[mat("red"), mat("red"), mat("blue")]);
        assert_eq!(places, vec![Some("/looks/red".to_string()), None, Some("/looks/blue".to_string())]);
        // A material of the scene, by path.
        assert_eq!(chosen(0, &[BindTo::Path("/file/looks/glass".into())], &places).as_deref(), Some("/file/looks/glass"));
        assert_eq!(chosen(5, &[], &places), None);
    }
}
