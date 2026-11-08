//! Live rendering: a renderer in Imago's process (`xms_render`), sent the
//! scene once, then what changes in it as you work. A [`Session`] belongs to no
//! pane: the Image Viewer drives one, and the Viewer can.
//!
//! Its edits: what changed in the scene between two cooks, by prim path, as
//! the renderer takes it (`xms_render::Change`). Primitives share what did not
//! change (`Arc`s), so most compare by pointer. What no live edit covers (a
//! prim added or removed, topology, a material's network, a camera's lens,
//! another purpose or binding) is a change of structure: the layer is written
//! and loaded again.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use glam::Mat4;
use xms_render::{Change, Edit, Value};

use crate::core::properties::Value as Prop;
use crate::types::{Placement, Prim};

/// How a renderer renders live, when its definition says so: Arnold's.
pub fn renderer_for(name: &str) -> Option<crate::core::render::LiveRenderer> {
    crate::core::render::definitions()
        .iter()
        .find(|d| d.name == name)
        .and_then(|d| d.live.clone())
        .filter(|l| l.backend == "arnold")
}

/// A live render: the renderer, the scene as last sent to it, and how it is
/// doing.
pub struct Session {
    live: xms_render::Live,
    /// Where the scene is written, when it must be.
    layer: PathBuf,
    sent: Vec<Prim>,
    /// The graph's revision and the frame the scene sent is of.
    key: (u64, u64),
    rendering: bool,
    /// IPR: in passes, sent each change. Else once, at the Render Settings'
    /// samples, bucket by bucket, the scene as it was.
    ipr: bool,
    /// Passes done of the render going, and how many it makes.
    pass: Option<(u32, u32)>,
    finished: bool,
    /// When it last started rendering: a first pass long in coming is Arnold
    /// making its `.tx` textures.
    since: Instant,
    /// The image's generation last taken.
    taken: u64,
}

impl Session {
    /// The renderer started, the scene to be written in `dir` when it must be.
    pub fn start(renderer: &crate::core::render::LiveRenderer, dir: &Path) -> Result<Session, String> {
        let sdk = crate::core::vars::expand(&renderer.sdk, &[]);
        if sdk.trim().is_empty() || sdk.contains('$') {
            return Err(
                "Where is Arnold? Set ARNOLD_PATH to its SDK's folder, in File > Variables or the environment".into(),
            );
        }
        let config = xms_render::arnold::Config {
            sdk: sdk.trim().into(),
            materialx: materialx_folder(dir),
            licence: renderer.licence,
        };
        let live = xms_render::Live::start(move |pixels| {
            Ok(Box::new(xms_render::arnold::Arnold::new(config, pixels)?) as Box<dyn xms_render::Renderer>)
        });
        Ok(Session {
            live,
            layer: dir.join("live.usda"),
            sent: vec![],
            key: (0, 0),
            rendering: false,
            ipr: true,
            pass: None,
            finished: false,
            since: Instant::now(),
            taken: 0,
        })
    }

    /// Render the scene: `prims`, of the graph's revision and frame `key`, at
    /// `frame`. Written and loaded; `ipr`, in passes, sent each change after.
    pub fn render(&mut self, prims: Vec<Prim>, key: (u64, u64), frame: f64, ipr: bool) -> Result<(), String> {
        crate::usd_write::write_override(&prims, &self.layer)
            .map_err(|e| format!("The scene could not be written: {e}"))?;
        self.live.load(&self.layer, frame, ipr);
        self.ipr = ipr;
        self.sent = prims;
        self.key = key;
        self.rendering = true;
        self.since = Instant::now();
        Ok(())
    }

    /// Stop rendering; the renderer keeps what it has loaded.
    pub fn stop(&mut self) {
        self.live.stop();
        self.rendering = false;
    }

    pub fn rendering(&self) -> bool {
        self.rendering
    }

    /// Its render is done (IPR: until the next change).
    pub fn done(&self) -> bool {
        self.finished
    }

    /// While it renders, each time it is looked at: the scene sent when the
    /// graph's revision or the frame (`key`) changed (`prims` gives it), what
    /// the renderer did taken in. What failed, if anything.
    pub fn update(&mut self, key: (u64, u64), frame: f64, prims: impl FnOnce() -> Option<Vec<Prim>>) -> Option<String> {
        if !self.rendering {
            return None;
        }
        let mut failed = None;
        if self.ipr && self.key != key && !crate::usd_scene::any_reading() {
            if let Some(prims) = prims() {
                if self.key.1 != key.1 {
                    // Another frame: loaded.
                    if let Err(e) = self.render(prims, key, frame, true) {
                        failed = Some(e);
                    }
                } else {
                    let edits = changes(&self.sent, &prims, crate::usd_write::layer_axes(&prims));
                    if !edits.is_empty() {
                        // Written only if the renderer cannot take them live.
                        let (now, layer) = (prims.clone(), self.layer.clone());
                        self.live.edit(edits, move || {
                            crate::usd_write::write_override(&now, &layer).map(|_| layer)
                        });
                    }
                    self.sent = prims;
                    self.key = key;
                }
            }
        }
        for e in self.live.events() {
            match e {
                xms_render::Event::Loaded { .. } => self.restarted(),
                xms_render::Event::Edited(_) => self.restarted(),
                xms_render::Event::Pass { index, total } => self.pass = Some((index + 1, total)),
                xms_render::Event::Finished => {
                    self.finished = true;
                    // A final render is done; IPR waits for changes.
                    self.rendering &= self.ipr;
                }
                xms_render::Event::Failed(why) => failed = Some(why),
            }
        }
        failed
    }

    fn restarted(&mut self) {
        self.pass = None;
        self.finished = false;
        self.since = Instant::now();
    }

    /// The image as it renders, in memory, bucket by bucket: for a texture
    /// kept up to date with what changed (`Pixels::take`).
    pub fn pixels(&self) -> &Arc<xms_render::Pixels> {
        self.live.pixels()
    }

    /// The images as they render, in memory, when buckets came in since they
    /// were last taken: an AOV each, the beauty first, refining pass by pass.
    pub fn layers(&mut self) -> Option<Vec<xms_render::Layer>> {
        let pixels = self.live.pixels();
        let now = pixels.generation();
        if now == self.taken {
            return None;
        }
        self.taken = now;
        Some(pixels.layers()).filter(|l| l.first().is_some_and(|l| l.frame.width > 0 && l.frame.height > 0))
    }

    /// How it is doing, in a few words.
    pub fn status(&self) -> String {
        let what = if self.ipr { "IPR" } else { "Render" };
        match (self.rendering, self.finished, self.pass) {
            (_, true, _) => format!("{what}: done"),
            (false, _, _) => format!("{what}: stopped"),
            (true, _, Some((done, total))) if self.ipr => format!("{what}: pass {done} of {total}"),
            _ if self.since.elapsed().as_secs() >= 3 && self.pass.is_none() && self.ipr => {
                format!("{what}: waiting for the first pass (Arnold makes .tx files of new textures)")
            }
            _ if self.ipr => format!("{what}: starting"),
            _ => format!("{what}: rendering, bucket by bucket"),
        }
    }
}

/// MaterialX's libraries as files, for Arnold's USD reader to know MaterialX
/// materials by: Imago's own, written once into `dir`.
fn materialx_folder(dir: &Path) -> Option<PathBuf> {
    let dir = dir.join("materialx");
    for (name, text) in xms_matx::FILES {
        let file = dir.join(name);
        if !file.exists() {
            std::fs::create_dir_all(file.parent()?).ok()?;
            std::fs::write(&file, text).ok()?;
        }
    }
    Some(dir)
}

/// The edits from `before` to `now`; one `Change::Structure` when any change
/// is not one a renderer takes live. `axes`: the layer's (`usd_write::layer_axes`).
pub fn changes(before: &[Prim], now: &[Prim], axes: Mat4) -> Vec<Edit> {
    let structure = || {
        vec![Edit {
            path: "/".into(),
            change: Change::Structure,
        }]
    };
    if before.len() != now.len() {
        return structure();
    }
    let was: HashMap<&str, &Prim> = before.iter().map(|p| (p.path.as_str(), p)).collect();
    let mut edits = vec![];
    for p in now {
        match was.get(p.path.as_str()).and_then(|b| prim_changes(b, p, axes)) {
            Some(e) => edits.extend(e),
            None => return structure(),
        }
    }
    edits
}

/// The same: shared, else equal (an operator cooked again makes it anew).
fn same<T: PartialEq>(a: &Option<Arc<T>>, b: &Option<Arc<T>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b) || a == b,
        (None, None) => true,
        _ => false,
    }
}

/// Shared: compared by pointer alone, as their values are big.
fn shared<T>(a: &Option<Arc<T>>, b: &Option<Arc<T>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

/// `p`'s edits since `b`; `None` for a change of structure.
fn prim_changes(b: &Prim, p: &Prim, axes: Mat4) -> Option<Vec<Edit>> {
    let mut out = vec![];
    let mut edit = |change| {
        out.push(Edit {
            path: p.path.clone(),
            change,
        })
    };
    // What only a load changes.
    if b.purpose != p.purpose
        || b.material != p.material
        || !shared(&b.stage, &p.stage)
        || !same(&b.instancing, &p.instancing)
        || !shared(&b.instanced_by, &p.instanced_by)
        || b.above != p.above
        || !same(&b.camera, &p.camera)
    {
        return None;
    }
    if b.place != p.place {
        let world = match &p.place {
            Placement::InPlace => Mat4::IDENTITY,
            Placement::One(m) => *m,
            Placement::Many(_) => return None,
        };
        edit(Change::Transform((axes.inverse() * world).to_cols_array_2d()));
    }
    // Geometry: a light, camera, material or render prim has none.
    let bare = p.light.is_some() || p.camera.is_some() || p.shading.is_some() || p.render.is_some();
    if !same(&b.geo, &p.geo) {
        edit(Change::Points(points_only(b.geo.as_deref()?, p.geo.as_deref()?)?));
    } else if !bare && p.geo.is_none() && !Arc::ptr_eq(&b.mesh, &p.mesh) {
        return None;
    }
    if !Arc::ptr_eq(&b.props, &p.props) && b.props != p.props {
        edit(Change::Visible(visible_only(&b.props, &p.props)?));
    }
    if !same(&b.light, &p.light) {
        let (l, m) = (b.light.as_deref()?, p.light.as_deref()?);
        if l.kind != m.kind {
            return None;
        }
        let names: BTreeSet<&String> = l.values.keys().chain(m.values.keys()).collect();
        let mut attrs = vec![];
        for name in names {
            let now = m.value(name);
            if l.value(name) != now {
                attrs.push((name.clone(), value(&now?)?));
            }
        }
        if !attrs.is_empty() {
            edit(Change::Attributes(attrs));
        }
    }
    if !same(&b.render, &p.render) {
        let (r, s) = (b.render.as_deref()?, p.render.as_deref()?);
        if r.kind != crate::core::render::RenderKind::Settings || r.kind != s.kind || r.rels != s.rels {
            return None;
        }
        let mut attrs = vec![];
        for (name, a) in &s.attrs {
            if r.attrs.get(name) != Some(a) {
                attrs.push((name.clone(), literal(&a.ty, &a.literal)?));
            }
        }
        if r.attrs.keys().any(|k| !s.attrs.contains_key(k)) {
            return None;
        }
        if !attrs.is_empty() {
            edit(Change::Settings(attrs));
        }
    }
    if !same(&b.shading, &p.shading) {
        out.extend(shader_changes(&p.path, b.shading.as_deref()?, p.shading.as_deref()?)?);
    }
    Some(out)
}

/// The points of `now`, when they are all that changed from `before`.
fn points_only(before: &crate::core::geo::Geo, now: &crate::core::geo::Geo) -> Option<Arc<Vec<[f32; 3]>>> {
    use crate::core::geo::Context;
    if before.topology != now.topology {
        return None;
    }
    // Its attributes but its points.
    fn others(g: &crate::core::geo::Geo, ctx: Context) -> Vec<(&str, &xms_geo::Attr)> {
        g.attrs(ctx).filter(|(n, _)| *n != xms_geo::POINTS).collect()
    }
    for ctx in [Context::Object, Context::Point, Context::Face, Context::Corner] {
        if others(before, ctx) != others(now, ctx) {
            return None;
        }
    }
    Some(Arc::new(now.points().to_vec()))
}

/// Shown or hidden, when `visibility` is the only property that changed.
fn visible_only(
    before: &std::collections::BTreeMap<String, Prop>,
    now: &std::collections::BTreeMap<String, Prop>,
) -> Option<bool> {
    let names: BTreeSet<&String> = before.keys().chain(now.keys()).collect();
    if names
        .iter()
        .any(|n| *n != "visibility" && before.get(*n) != now.get(*n))
    {
        return None;
    }
    Some(!matches!(now.get("visibility"), Some(Prop::Token(t)) | Some(Prop::String(t)) if t == "invisible"))
}

/// A MaterialX material's shaders' changed inputs, each shader at its prim
/// (`<material>/<node>`); `None` when its network changed, or a material that
/// is not MaterialX (Arnold's names for UsdPreviewSurface's are not its own).
fn shader_changes(
    path: &str,
    before: &crate::core::material::Material,
    now: &crate::core::material::Material,
) -> Option<Vec<Edit>> {
    let (g, h) = (before.graph.as_deref()?, now.graph.as_deref()?);
    if before.name != now.name || g.nodes.len() != h.nodes.len() {
        return None;
    }
    let mut out = vec![];
    for (a, b) in g.nodes.iter().zip(&h.nodes) {
        let names = |n: &xms_matx::Node| n.inputs.iter().map(|(i, _)| i.clone()).collect::<Vec<_>>();
        if a.id != b.id || a.name != b.name || a.node != b.node || a.def != b.def || names(a) != names(b) {
            return None;
        }
        let mut attrs = vec![];
        for ((name, x), (_, y)) in a.inputs.iter().zip(&b.inputs) {
            match (x, y) {
                (xms_matx::Binding::Value(x), xms_matx::Binding::Value(y)) if x != y => {
                    attrs.push((format!("inputs:{name}"), matx_value(y)?))
                }
                (xms_matx::Binding::Value(_), xms_matx::Binding::Value(_)) => {}
                (x, y) if x == y => {}
                _ => return None,
            }
        }
        if !attrs.is_empty() {
            out.push(Edit {
                path: format!("{path}/{}", b.name),
                change: Change::Attributes(attrs),
            });
        }
    }
    Some(out)
}

/// A property's value as a renderer takes it.
fn value(v: &Prop) -> Option<Value> {
    Some(match v {
        Prop::Bool(b) => Value::Bool(*b),
        Prop::Int(i) => Value::Int(*i),
        Prop::Float(f) => Value::Float(*f),
        Prop::Double(d) => Value::Float(*d as f32),
        Prop::Color3f(c) | Prop::Float3(c) => Value::Color(*c),
        Prop::Token(t) | Prop::String(t) => Value::Token(t.clone()),
        _ => return None,
    })
}

fn matx_value(v: &xms_matx::Value) -> Option<Value> {
    Some(match v {
        xms_matx::Value::Boolean(b) => Value::Bool(*b),
        xms_matx::Value::Integer(i) => Value::Int(*i),
        xms_matx::Value::Floats(f) if f.len() == 1 => Value::Float(f[0]),
        xms_matx::Value::Floats(f) if f.len() == 3 => Value::Color([f[0], f[1], f[2]]),
        xms_matx::Value::String(s) => Value::Token(s.clone()),
        _ => return None,
    })
}

/// A render prim's attribute, from its `.usda` text.
fn literal(ty: &str, text: &str) -> Option<Value> {
    let numbers = || -> Option<Vec<f64>> {
        text.trim_matches(|c| c == '(' || c == ')')
            .split(',')
            .map(|n| n.trim().parse().ok())
            .collect()
    };
    Some(match ty {
        "int" => Value::Int(text.trim().parse().ok()?),
        "float" | "double" => Value::Float(text.trim().parse().ok()?),
        "bool" => Value::Bool(matches!(text.trim(), "1" | "true")),
        "int2" => match numbers()?[..] {
            [x, y] => Value::Int2([x as i32, y as i32]),
            _ => return None,
        },
        "token" | "string" => Value::Token(text.trim().trim_matches('"').to_string()),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::light::{Light, LightType};

    fn light(path: &str, intensity: f32) -> Prim {
        let mut l = Light::new(LightType::Rect);
        l.set("inputs:intensity", Prop::Float(intensity));
        Prim {
            path: path.into(),
            light: Some(Arc::new(l)),
            place: Placement::One(Mat4::IDENTITY),
            ..Prim::new(path.into(), crate::types::MeshData::default())
        }
    }

    #[test]
    fn a_light_changed_is_its_attributes() {
        let before = vec![light("/lights/key", 1.0)];
        let now = vec![light("/lights/key", 4.0)];
        assert_eq!(
            changes(&before, &now, Mat4::IDENTITY),
            vec![Edit {
                path: "/lights/key".into(),
                change: Change::Attributes(vec![("inputs:intensity".into(), Value::Float(4.0))]),
            }]
        );
        // Nothing changed: no edits.
        assert!(changes(&before, &before.clone(), Mat4::IDENTITY).is_empty());
    }

    #[test]
    fn a_move_is_its_world_matrix_in_the_layer() {
        let before = vec![light("/lights/key", 1.0)];
        let mut moved = before[0].clone();
        moved.place = Placement::One(Mat4::from_translation(glam::vec3(0.0, 2.0, 0.0)));
        // A Z-up layer: Imago's Y is the layer's Z.
        let z_up = Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        let e = changes(&before, &[moved], z_up);
        let Change::Transform(m) = &e[0].change else {
            panic!("{e:?}")
        };
        // Rows as USD writes them: translation last.
        assert!((m[3][2] - 2.0).abs() < 1e-5 && m[3][1].abs() < 1e-5, "{m:?}");
    }

    #[test]
    fn a_prim_added_or_its_lens_changed_is_a_load() {
        let before = vec![light("/lights/key", 1.0)];
        let mut now = before.clone();
        now.push(light("/lights/fill", 1.0));
        assert_eq!(changes(&before, &now, Mat4::IDENTITY)[0].change, Change::Structure);
        let mut purpose = before[0].clone();
        purpose.purpose = crate::types::Purpose::Proxy;
        assert_eq!(
            changes(&before, &[purpose], Mat4::IDENTITY)[0].change,
            Change::Structure
        );
    }

    #[test]
    fn render_settings_read_from_their_text() {
        assert_eq!(literal("int", "6"), Some(Value::Int(6)));
        assert_eq!(literal("int2", "(1920, 1080)"), Some(Value::Int2([1920, 1080])));
        assert_eq!(literal("token", "\"raw\""), Some(Value::Token("raw".into())));
        assert_eq!(literal("rel", "</cam>"), None);
    }

    #[test]
    fn hidden_is_visibility_alone() {
        let mut b = std::collections::BTreeMap::new();
        let mut n = b.clone();
        n.insert("visibility".to_string(), Prop::Token("invisible".into()));
        assert_eq!(visible_only(&b, &n), Some(false));
        b.insert("kind".to_string(), Prop::Token("component".into()));
        assert_eq!(visible_only(&b, &n), None);
    }
}
