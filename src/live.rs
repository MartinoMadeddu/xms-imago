//! Live rendering: a renderer in Imago's process (`xms_render`), sent the
//! scene once, then what changes in it as you work. One [`Session`] for the
//! program (Arnold is started once per process), in no pane: the Image Viewer
//! renders with it through the layer's camera, the Viewer through its own
//! view (`in_viewer`), the last to start it having it.
//!
//! Its edits: what changed in the scene between two cooks, by prim path, as
//! the renderer takes it (`xms_render::Change`). Primitives share what did not
//! change (`Arc`s), so most compare by pointer. What no live edit covers (a
//! prim added or removed, topology, a material's network, a camera's lens,
//! another purpose or binding) is a change of structure: the layer is written
//! and loaded again.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use glam::Mat4;
use xms_render::{Change, Edit, Value};

use crate::core::properties::Value as Prop;
use crate::node_graph::SceneRange;
use crate::types::{Placement, Prim};

/// The program's live render, once started.
static SESSION: Mutex<Option<Session>> = Mutex::new(None);

/// Work on the program's live render (`None` until one is started).
pub fn with<R>(f: impl FnOnce(&mut Option<Session>) -> R) -> R {
    let mut s = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut s)
}

/// Before the program exits: the live render ended (its thread joined, the
/// renderer let go), then Arnold, started to render or to be asked about.
pub fn end() {
    // Dropped outside the lock: the render's thread is joined.
    let session = with(|s| s.take());
    drop(session);
    xms_render::arnold::end();
}

/// The Viewer renders (with Arnold) in place of drawing the scene.
static IN_VIEWER: AtomicBool = AtomicBool::new(false);

pub fn in_viewer() -> bool {
    IN_VIEWER.load(Ordering::Relaxed)
}

/// The Viewer rendering or not; off, its render stops.
pub fn set_in_viewer(on: bool) {
    if IN_VIEWER.swap(on, Ordering::Relaxed) && !on {
        with(|s| {
            if let Some(s) = s.as_mut().filter(|s| s.through.is_some()) {
                s.stop();
            }
        });
    }
}

/// The scene the view flag shows, as primitives.
pub fn scene(graph: &crate::node_graph::NodeGraphState, subnets: &crate::ice::IceGraphStore) -> Option<Vec<Prim>> {
    let eval = |sid: crate::types::IceGraphId,
                mesh: &crate::core::geo::Geo,
                template: &[crate::ice::Reference]|
     -> crate::core::geo::Geo {
        subnets
            .get(sid)
            .map(|sg| sg.evaluate(mesh, template))
            .unwrap_or_else(|| mesh.clone())
    };
    match graph.evaluate_scene(&eval)?.2 {
        crate::types::EvalResult::Named(prims) => Some(prims),
        _ => None,
    }
}

/// A view to render `scene` through: a camera placed at `world` (Imago's
/// space, looking down -Z), `vertical_fov` (radians), `size` pixels.
pub fn view(scene: &SceneRange, world: Mat4, vertical_fov: f32, size: [u32; 2]) -> xms_render::View {
    let aspect = size[0] as f32 / size[1].max(1) as f32;
    let horizontal = 2.0 * ((vertical_fov * 0.5).tan() * aspect).atan();
    xms_render::View {
        matrix: (scene.axes().inverse() * world).to_cols_array_2d(),
        fov: horizontal.to_degrees(),
        size,
    }
}

/// How a renderer renders live, when its definition says so (Arnold's) and
/// it was not found not to load here (still looking is a yes: a render then
/// says what went wrong).
pub fn renderer_for(name: &str) -> Option<crate::core::render::LiveRenderer> {
    let found = crate::core::renderers::found(name);
    crate::core::renderers::definition(name)
        .and_then(|d| d.live.clone())
        .filter(|l| l.backend == "arnold")
        .filter(|_| !matches!(found.in_imago, Some(crate::core::renderers::Found::No(_))))
}

/// Why a renderer does not render in Imago, when it is one that would.
pub fn why_not(name: &str) -> Option<String> {
    match crate::core::renderers::found(name).in_imago {
        Some(crate::core::renderers::Found::No(why)) => Some(why),
        _ => None,
    }
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
    /// Its up axis, unit and rate: what it is written in.
    scene: SceneRange,
    rendering: bool,
    /// IPR: in passes, sent each change. Else once, at the Render Settings'
    /// samples, bucket by bucket, the scene as it was.
    ipr: bool,
    /// The view it renders through, the Viewer's; `None`, the layer's camera.
    through: Option<xms_render::View>,
    /// Passes done of the render going, and how many it makes.
    pass: Option<(u32, u32)>,
    finished: bool,
    /// When it last started rendering: a first pass long in coming is Arnold
    /// making its `.tx` textures.
    since: Instant,
    /// The image's generation last taken.
    taken: u64,
    /// The renderer's own log, its warnings and errors: the last `LOG_KEPT`.
    log: std::collections::VecDeque<String>,
}

/// How many of the renderer's log lines a session keeps.
const LOG_KEPT: usize = 400;

/// How Arnold is started: from where it is installed, MaterialX's libraries
/// written in `dir`, relative products from `outputs`.
pub fn arnold_config(
    renderer: &crate::core::render::LiveRenderer,
    dir: &Path,
    outputs: Option<PathBuf>,
) -> Result<xms_render::arnold::Config, String> {
    let sdk = crate::core::renderers::sdk(renderer).ok_or_else(|| crate::core::renderers::where_is(renderer))?;
    Ok(xms_render::arnold::Config {
        sdk,
        materialx: materialx_folder(dir),
        licence: renderer.licence,
        outputs,
    })
}

impl Session {
    /// The renderer started, the scene to be written in `dir` when it must be;
    /// relative products written from `outputs`, else from `dir`.
    pub fn start(
        renderer: &crate::core::render::LiveRenderer,
        dir: &Path,
        outputs: Option<PathBuf>,
    ) -> Result<Session, String> {
        let config = arnold_config(renderer, dir, outputs)?;
        let live = xms_render::Live::start(move |pixels| {
            Ok(Box::new(xms_render::arnold::Arnold::new(config, pixels)?) as Box<dyn xms_render::Renderer>)
        });
        Ok(Session {
            live,
            layer: dir.join("live.usda"),
            sent: vec![],
            key: (0, 0),
            scene: SceneRange::default(),
            rendering: false,
            ipr: true,
            through: None,
            pass: None,
            finished: false,
            since: Instant::now(),
            taken: 0,
            log: Default::default(),
        })
    }

    /// Render the scene: `prims` of `scene`, of the graph's revision and
    /// frame `key`, at `frame`, through `through` (the Viewer's view), else
    /// the layer's camera. Written and loaded; `ipr`, in passes, sent each
    /// change after.
    pub fn render(
        &mut self,
        prims: Vec<Prim>,
        scene: &SceneRange,
        key: (u64, u64),
        frame: f64,
        ipr: bool,
        through: Option<xms_render::View>,
    ) -> Result<(), String> {
        // Light: the points deformers moved left out, sent in memory before
        // the first bucket; the whole layer written only if they cannot be.
        let (light, heavier) = lighter(&prims);
        let written = crate::usd_write::write_override(&light, &self.layer, scene)
            .map_err(|e| format!("The scene could not be written: {e}"))?;
        // What the layer has otherwise than the stages: in the Log too, once.
        for n in heavier.into_iter().chain(written.notes) {
            let line = format!("Imago: {n}");
            if !self.log.contains(&line) {
                self.logged(line);
            }
        }
        let moved = changes(&light, &prims, scene.axes());
        let (all, layer, at) = (prims.clone(), self.layer.clone(), *scene);
        self.live.load_then(&self.layer, frame, ipr, through, moved, move || {
            crate::usd_write::write_override(&all, &layer, &at).map(|_| layer)
        });
        self.scene = *scene;
        self.ipr = ipr;
        self.through = through;
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

    /// It renders through the Viewer's view.
    pub fn in_viewer(&self) -> bool {
        self.through.is_some()
    }

    /// The Viewer's view (as `view` takes it), sent if it moved, when it
    /// renders through it.
    pub fn look(&mut self, world: Mat4, vertical_fov: f32, size: [u32; 2]) {
        let v = view(&self.scene, world, vertical_fov, size);
        if self.rendering && self.through.is_some() && self.through != Some(v) {
            self.through = Some(v);
            let layer = self.layer.clone();
            self.live.edit(
                vec![xms_render::Edit {
                    path: String::new(),
                    change: Change::View(v),
                }],
                move || Ok(layer),
            );
        }
    }

    /// Its render is done (IPR: until the next change).
    pub fn done(&self) -> bool {
        self.finished
    }

    /// While it renders, each time it is looked at: the scene sent when the
    /// graph's revision or the frame (`key`) changed (`prims` gives it), what
    /// the renderer did taken in. What failed, if anything.
    pub fn update(
        &mut self,
        scene: &SceneRange,
        key: (u64, u64),
        frame: f64,
        prims: impl FnOnce() -> Option<Vec<Prim>>,
    ) -> Option<String> {
        if !self.rendering {
            return None;
        }
        let mut failed = None;
        if self.ipr && self.key != key && !crate::usd_scene::any_reading() {
            if let Some(prims) = prims() {
                if self.key.1 != key.1 || self.scene != *scene {
                    // Another frame, or other axes, unit or rate: loaded.
                    if let Err(e) = self.render(prims, scene, key, frame, true, self.through) {
                        failed = Some(e);
                    }
                } else {
                    let edits = changes(&self.sent, &prims, scene.axes());
                    if !edits.is_empty() {
                        // Written only if the renderer cannot take them live.
                        let (now, layer, at) = (prims.clone(), self.layer.clone(), *scene);
                        self.live.edit(edits, move || {
                            crate::usd_write::write_override(&now, &layer, &at).map(|_| layer)
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
                xms_render::Event::Failed(why) | xms_render::Event::Note(why) => failed = Some(why),
                xms_render::Event::Images(_) => {}
                xms_render::Event::Log(line) => self.logged(line),
            }
        }
        failed
    }

    /// Another frame of a range rendered without the window (headless): what
    /// changed sent live with the frame, Arnold loaded once; the layer written
    /// again only when the renderer cannot take a change live (`Structure`,
    /// motion blur's keys).
    pub fn frame(&mut self, prims: Vec<Prim>, key: (u64, u64), frame: f64) {
        let mut edits = changes(&self.sent, &prims, self.scene.axes());
        edits.push(xms_render::Edit {
            path: String::new(),
            change: Change::Frame(frame),
        });
        let (now, layer, at) = (prims.clone(), self.layer.clone(), self.scene);
        self.live.edit(edits, move || {
            crate::usd_write::write_override(&now, &layer, &at).map(|_| layer)
        });
        self.sent = prims;
        self.key = key;
        self.rendering = true;
        self.restarted();
    }

    /// Until the render going is done, at most `timeout`: the files it
    /// writes, each note and line of the renderer's log on the way told to
    /// `note`.
    pub fn finish(&mut self, timeout: std::time::Duration, note: impl Fn(&str)) -> Result<Vec<PathBuf>, String> {
        let end = Instant::now() + timeout;
        let mut images = vec![];
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            for e in self.live.wait(left, |e| {
                matches!(e, xms_render::Event::Finished | xms_render::Event::Failed(_))
            }) {
                match e {
                    xms_render::Event::Loaded { images: i, .. } | xms_render::Event::Images(i) => images = i,
                    xms_render::Event::Note(n) => note(&n),
                    xms_render::Event::Log(line) => {
                        note(&line);
                        self.logged(line);
                    }
                    xms_render::Event::Failed(why) => {
                        self.rendering = false;
                        return Err(why);
                    }
                    xms_render::Event::Finished => {
                        self.finished = true;
                        self.rendering = false;
                        return Ok(images);
                    }
                    _ => {}
                }
            }
        }
        Err(format!("Not done after {} s", timeout.as_secs()))
    }

    fn logged(&mut self, line: String) {
        if self.log.len() >= LOG_KEPT {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }

    /// Its log as the Image Viewer's console shows a run: the renderer's
    /// warnings and errors.
    pub fn log(&self) -> crate::core::command::Snapshot {
        use crate::core::command::State;
        crate::core::command::Snapshot {
            state: if self.rendering { State::Running } else { State::Done },
            lines: self.log.iter().cloned().collect(),
            progress: None,
            at: (1, 1),
            elapsed: self.since.elapsed(),
            command: "Arnold in Imago".into(),
        }
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
/// is not one a renderer takes live. `axes`: the scene's (`SceneRange::axes`).
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

/// The scene as written for the renderer to load: each prim whose points
/// moved from its file (a deformer, animation) with its file's points, so the
/// layer holds no points of them; they go to the renderer in memory
/// (`changes` against this, `Change::Points`). What else changed of it (an
/// attribute set, its topology unchanged) is still written, and named in the
/// notes: the layer's weight. What a deformer copied unchanged is the file's
/// again, so it is not written (the layer writes what is not the file's
/// array).
fn lighter(prims: &[Prim]) -> (Vec<Prim>, Vec<String>) {
    use crate::core::geo::Context;
    let mut notes = vec![];
    let light = prims
        .iter()
        .map(|p| {
            let (Some(geo), Some(source)) = (&p.geo, &p.source) else {
                return p.clone();
            };
            let file = &source.geo;
            let moved = !Arc::ptr_eq(geo, file)
                && geo.topology == file.topology
                && geo.points().len() == file.points().len()
                && geo.points() != file.points();
            if !moved {
                return p.clone();
            }
            let mut light = p.clone();
            if points_only(file, geo).is_some() {
                light.geo = Some(file.clone());
            } else {
                let mut kept = (**geo).clone();
                kept.topology = file.topology.clone();
                for ctx in Context::ALL {
                    for (name, a) in file.attrs(ctx) {
                        if name == xms_geo::POINTS || kept.attr(ctx, name) == Some(a) {
                            kept.set(ctx, name, a.clone());
                        }
                    }
                }
                notes.push(format!(
                    "{}: points in memory; written too, changed from its file: {}",
                    p.path,
                    changed_besides_points(file, &kept).join(", ")
                ));
                light.geo = Some(Arc::new(kept));
            }
            light
        })
        .collect();
    (light, notes)
}

/// The attributes of `now` but its points that are not `before`'s, as
/// `context name`.
fn changed_besides_points(before: &crate::core::geo::Geo, now: &crate::core::geo::Geo) -> Vec<String> {
    use crate::core::geo::Context;
    let mut out = vec![];
    for ctx in Context::ALL {
        for (name, attr) in now.attrs(ctx).filter(|(n, _)| *n != xms_geo::POINTS) {
            if before.attr(ctx, name) != Some(attr) {
                out.push(format!("{ctx:?} {name}").to_lowercase());
            }
        }
        for (name, _) in before.attrs(ctx).filter(|(n, _)| now.attr(ctx, n).is_none()) {
            out.push(format!("{ctx:?} {name} (removed)").to_lowercase());
        }
    }
    out
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

    /// A prim whose points alone moved from its file (a deformer): its file's
    /// points in the layer written, its own sent in memory; one whose
    /// topology changed keeps its geometry to be written.
    #[test]
    fn the_layer_written_leaves_out_points_that_only_moved() {
        use crate::core::geo::Geo;
        let quad = |y: f32| {
            let points = vec![[0.0, y, 0.0], [1.0, y, 0.0], [1.0, y, 1.0], [0.0, y, 1.0]];
            Arc::new(Geo::from_polygons(points, vec![4], vec![0, 1, 2, 3]))
        };
        let from_file = |path: &str, now: Arc<Geo>| Prim {
            path: path.into(),
            geo: Some(now),
            source: Some(Arc::new(crate::types::Source {
                geo: quad(0.0),
                place: Placement::One(Mat4::IDENTITY),
                material: None,
                purpose: Default::default(),
            })),
            place: Placement::One(Mat4::IDENTITY),
            ..Prim::new(path.into(), crate::types::MeshData::default())
        };
        let triangle = Arc::new(Geo::from_polygons(
            vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            vec![3],
            vec![0, 1, 2],
        ));
        let prims = vec![
            from_file("/deformed", quad(2.0)),
            from_file("/remeshed", triangle.clone()),
        ];
        let (light, notes) = lighter(&prims);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(
            light[0].geo.as_deref().unwrap().points()[0],
            [0.0, 0.0, 0.0],
            "as its file"
        );
        assert!(
            Arc::ptr_eq(light[1].geo.as_ref().unwrap(), &triangle),
            "written as it is"
        );
        let sent = changes(&light, &prims, Mat4::IDENTITY);
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert_eq!(sent[0].path, "/deformed");
        assert!(matches!(&sent[0].change, Change::Points(p) if p[0] == [0.0, 2.0, 0.0]));
    }

    /// A deformer that also set an attribute (a width): its points still in
    /// memory, the attribute alone written, and named.
    #[test]
    fn an_attribute_set_with_the_points_moved_keeps_them_in_memory() {
        use crate::core::geo::{Attr, Column, Context, Geo, Role};
        let quad = |y: f32| {
            Geo::from_polygons(
                vec![[0.0, y, 0.0], [1.0, y, 0.0], [1.0, y, 1.0]],
                vec![3],
                vec![0, 1, 2],
            )
        };
        let mut now = quad(2.0);
        now.set(
            Context::Point,
            "widths",
            Attr::new(Column::Float(Arc::new(vec![0.1; 3])), Role::None),
        );
        let p = Prim {
            path: "/hair".into(),
            geo: Some(Arc::new(now)),
            source: Some(Arc::new(crate::types::Source {
                geo: Arc::new(quad(0.0)),
                place: Placement::One(Mat4::IDENTITY),
                material: None,
                purpose: Default::default(),
            })),
            place: Placement::One(Mat4::IDENTITY),
            ..Prim::new("/hair".into(), crate::types::MeshData::default())
        };
        let (light, notes) = lighter(std::slice::from_ref(&p));
        let geo = light[0].geo.as_deref().unwrap();
        assert_eq!(geo.points()[0], [0.0, 0.0, 0.0], "the file's points");
        assert!(geo.attr(Context::Point, "widths").is_some(), "the width written");
        let file = &p.source.as_ref().unwrap().geo;
        assert!(
            geo.topology.same(&file.topology),
            "its topology the file's: not written"
        );
        assert_eq!(notes.len(), 1);
        assert!(notes[0].ends_with("changed from its file: point widths"), "{notes:?}");
        let sent = changes(&light, &[p], Mat4::IDENTITY);
        assert!(
            matches!(&sent[..], [e] if matches!(&e.change, Change::Points(p) if p[0] == [0.0, 2.0, 0.0])),
            "{sent:?}"
        );
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
    fn a_view_is_its_placement_and_horizontal_field_of_view() {
        let world = Mat4::from_translation(glam::Vec3::new(1.0, 2.0, 3.0));
        let fov = 50f32.to_radians();
        let square = view(&SceneRange::default(), world, fov, [400, 400]);
        assert_eq!(square.matrix, world.to_cols_array_2d());
        assert!((square.fov - 50.0).abs() < 1e-4);
        // Twice as wide: the tangent of half the angle twice as large.
        let wide = view(&SceneRange::default(), world, fov, [800, 400]);
        let half = |deg: f32| (deg.to_radians() * 0.5).tan();
        assert!((half(wide.fov) - 2.0 * half(50.0)).abs() < 1e-5);
        assert_eq!(wide.size, [800, 400]);
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
