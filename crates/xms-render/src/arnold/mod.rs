//! The Arnold backend: `libai` from the user's SDK, opened at run time, loading
//! the layer with its own USD reader (`AiSceneLoad`) and rendering it in an
//! interactive session. Its nodes are named by prim path, so an edit is a few
//! calls on the node of that name; what has no node, or no parameter Arnold's
//! reader would have set from that attribute, is a reload.

mod driver;
mod ffi;

use std::ffi::{c_char, c_int, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::{with_frame, Change, Edit, Event, Pixels, Renderer, Value};
use ffi::{Api, Matrix, Ptr, UpdateInfo};

/// Where Arnold is, and how it renders.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// The SDK's folder (`$ARNOLD_PATH`), with `bin/` and `plugins/`.
    pub sdk: PathBuf,
    /// MaterialX's libraries, for Arnold's USD reader to know MaterialX
    /// materials by (`PXR_MTLX_STDLIB_SEARCH_PATHS`, unless set already).
    pub materialx: Option<PathBuf>,
    /// Look for a licence; without one, renders are watermarked and start at
    /// once.
    pub licence: bool,
    /// The folder relative products are written from; `None`, the layer's.
    pub outputs: Option<PathBuf>,
}

impl Config {
    /// The SDK at `$ARNOLD_PATH`.
    pub fn from_env() -> Result<Config, String> {
        let sdk = std::env::var_os("ARNOLD_PATH").ok_or("ARNOLD_PATH is not set: the Arnold SDK's folder")?;
        Ok(Config {
            sdk: sdk.into(),
            ..Default::default()
        })
    }
}

/// Arnold as installed: its version, and the parameters of its `options`
/// node (what Render Settings can set), as this version has them.
#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub version: String,
    pub options: Vec<ArnoldOption>,
}

/// One parameter of Arnold's `options` node, and its default.
#[derive(Clone, Debug, PartialEq)]
pub struct ArnoldOption {
    pub name: String,
    pub default: OptionDefault,
}

/// A parameter's type and default, of the types a setting can be.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionDefault {
    Bool(bool),
    Int(i32),
    Float(f32),
    Rgb([f32; 3]),
    Vector([f32; 3]),
    Vector2([f32; 2]),
    Text(String),
    /// An enum: its default and every value it takes.
    Choice(String, Vec<String>),
}

/// Whether Arnold renders in Imago from the SDK at `sdk`: its library loaded,
/// as it is to render, and started; its version and options, else why not.
/// It stays loaded and started (the system's loader is never asked to close
/// it), so rendering later starts at no cost.
pub fn installed(sdk: &Path) -> Result<Installed, String> {
    let ai = Api::load(sdk)?;
    let version = ai.version();
    supported(&version)?;
    begin(&ai, sdk);
    Ok(Installed {
        version,
        options: ai.options(),
    })
}

/// `AiBegin`, once while Arnold is started: by whichever comes first, a
/// render or `installed`.
static BEGUN: Mutex<bool> = Mutex::new(false);

fn begin(ai: &Api, sdk: &Path) {
    let mut begun = BEGUN.lock().unwrap_or_else(|e| e.into_inner());
    if !*begun {
        environment(sdk);
        // SAFETY: before any call that needs it, once until `AiEnd`; the
        // callback is a function of this crate, its data none.
        unsafe {
            (ai.AiBegin)(ffi::SESSION_INTERACTIVE);
            (ai.AiMsgRegisterCallback)(Some(logged), ffi::LOG_WARNINGS_ERRORS, std::ptr::null_mut());
        }
        *begun = true;
    }
}

/// Arnold's environment, as its own programs (kick) have it from a shell,
/// for an SDK set in Imago's Settings rather than the environment:
/// `ARNOLD_PATH` the SDK unless set, its `plugins` folder first on
/// `ARNOLD_PLUGIN_PATH` (the USD reader among them), before `AiBegin`.
fn environment(sdk: &Path) {
    if std::env::var_os("ARNOLD_PATH").is_none_or(|v| v.is_empty()) {
        std::env::set_var("ARNOLD_PATH", sdk);
    }
    let plugins = sdk.join("plugins");
    if !plugins.is_dir() {
        return;
    }
    let now = std::env::var_os("ARNOLD_PLUGIN_PATH").unwrap_or_default();
    let mut paths: Vec<PathBuf> = std::env::split_paths(&now)
        .filter(|p| !p.as_os_str().is_empty())
        .collect();
    if !paths.contains(&plugins) {
        paths.insert(0, plugins);
        if let Ok(joined) = std::env::join_paths(paths) {
            std::env::set_var("ARNOLD_PLUGIN_PATH", joined);
        }
    }
}

/// Arnold's warnings and errors, as it logs them: how many there were, and
/// the last of them (`LOG_KEPT`).
static LOG: Mutex<(u64, std::collections::VecDeque<(bool, String)>)> =
    Mutex::new((0, std::collections::VecDeque::new()));
const LOG_KEPT: usize = 2000;

/// Arnold's log callback, on whichever thread logs.
unsafe extern "C" fn logged(_mask: c_int, severity: c_int, text: *const c_char, _: Ptr, _: Ptr) {
    if text.is_null() {
        return;
    }
    // SAFETY: Arnold's text, for the call.
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy().trim().to_string();
    log(severity == ffi::SEVERITY_ERROR, text);
}

fn log(error: bool, text: String) {
    if let Ok(mut l) = LOG.lock() {
        l.0 += 1;
        if l.1.len() >= LOG_KEPT {
            l.1.pop_front();
        }
        l.1.push_back((error, text));
    }
}

/// The lines logged after the first `seen`, as Imago shows them
/// (`Arnold error: …`), and how many there are now.
fn log_after(seen: u64) -> (Vec<(bool, String)>, u64) {
    let Ok(l) = LOG.lock() else {
        return (vec![], seen);
    };
    let new = (l.0.saturating_sub(seen) as usize).min(l.1.len());
    let lines = l.1.iter().skip(l.1.len() - new).cloned().collect();
    (lines, l.0)
}

fn shown((error, text): &(bool, String)) -> String {
    format!("Arnold {}: {text}", if *error { "error" } else { "warning" })
}

/// The image files the scene at `layer` reads at `frame`, as Arnold loads
/// it (its `image` shaders' files, MaterialX's among them), a `<UDIM>`'s
/// tiles found beside it: what `maketx` makes `.tx` files of. Arnold started
/// as for `installed`; the scene loaded in a universe of its own, let go.
pub fn textures(config: &Config, layer: &Path, frame: f64) -> Result<Vec<PathBuf>, String> {
    let ai = Api::load(&config.sdk)?;
    supported(&ai.version())?;
    if let Some(dir) = &config.materialx {
        if std::env::var_os(MTLX).is_none() {
            std::env::set_var(MTLX, dir);
        }
    }
    begin(&ai, &config.sdk);
    let file = CString::new(layer.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let before = log_after(u64::MAX).1;
    let mut names = vec![];
    // SAFETY: a universe of its own, destroyed once its nodes are read.
    unsafe {
        let universe = (ai.AiUniverse)();
        let params = (ai.AiParamValueMap)();
        (ai.AiParamValueMapSetFlt)(params, ai.string("frame"), frame as f32);
        let loaded = (ai.AiSceneLoad)(universe, file.as_ptr(), params);
        (ai.AiParamValueMapDestroy)(params);
        if loaded {
            for (entry, _, node) in ai.nodes(universe, ffi::NODE_SHADER as u32) {
                if entry == "image" {
                    names.push(ai.str_param(node, "filename"));
                }
            }
        }
        (ai.AiUniverseDestroy)(universe);
        if !loaded {
            return Err(could_not_load(layer, before));
        }
    }
    let folder = layer.parent().unwrap_or(Path::new("."));
    let mut out: Vec<PathBuf> = names
        .iter()
        .filter(|n| !n.trim().is_empty())
        .flat_map(|n| tiles(&folder.join(n.trim())))
        .filter(|p| p.is_file() && !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("tx")))
        .collect();
    out.sort();
    out.dedup();
    Ok(out)
}

/// A texture's files: itself, or its `<UDIM>` tiles on disk (`1001`…).
fn tiles(file: &Path) -> Vec<PathBuf> {
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some(at) = name.find("<UDIM>").or_else(|| name.find("<udim>")) else {
        return vec![file.to_path_buf()];
    };
    let (before, after) = (&name[..at], &name[at + 6..]);
    let dir = file.parent().unwrap_or(Path::new("."));
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|read| {
            read.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| {
                    n.strip_prefix(before)
                        .and_then(|r| r.strip_suffix(after))
                        .is_some_and(|tile| tile.len() == 4 && tile.bytes().all(|b| b.is_ascii_digit()))
                })
                .map(|n| dir.join(n))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// Arnold's `maketx`, in the SDK's `bin`.
pub fn maketx(config: &Config) -> PathBuf {
    let exe = if cfg!(windows) { "maketx.exe" } else { "maketx" };
    config.sdk.join("bin").join(exe)
}

/// Why Arnold did not load a layer: the errors it logged since `before`,
/// else its warnings, else that it said no more.
fn could_not_load(layer: &Path, before: u64) -> String {
    let (lines, _) = log_after(before);
    let errors: Vec<&str> = lines.iter().filter(|(e, _)| *e).map(|(_, t)| t.as_str()).collect();
    let said: Vec<&str> = match errors.is_empty() {
        true => lines.iter().map(|(_, t)| t.as_str()).collect(),
        false => errors,
    };
    match said.is_empty() {
        true => format!("Arnold could not load {} (it logged no error)", layer.display()),
        false => format!("Arnold could not load {}: {}", layer.display(), said.join("; ")),
    }
}

/// Imago renders with Arnold 7 or later.
fn supported(version: &str) -> Result<(), String> {
    let major = version.split('.').next().and_then(|m| m.trim().parse::<u32>().ok());
    match major {
        Some(m) if m >= 7 => Ok(()),
        _ => Err(format!("Arnold {version}: Imago renders with Arnold 7 or later")),
    }
}

/// Where Arnold's USD reader finds MaterialX's libraries.
const MTLX: &str = "PXR_MTLX_STDLIB_SEARCH_PATHS";

/// Arnold is started once per process.
static STARTED: AtomicBool = AtomicBool::new(false);

/// What the render callback, on Arnold's threads, tells the backend.
#[derive(Default)]
struct Progress {
    passes: AtomicU32,
    total: AtomicU32,
    finished: AtomicBool,
    failed: AtomicBool,
    /// `AiRenderGetStatus`, for updates that do not change the status.
    status: Option<unsafe extern "C" fn(Ptr) -> c_int>,
}

pub struct Arnold {
    ai: Api,
    config: Config,
    universe: Ptr,
    session: Ptr,
    progress: Box<Progress>,
    /// Passes already told.
    told: u32,
    /// What it could not do as asked, to tell.
    notes: Vec<String>,
    /// The products' drivers and their file names as the layer has them
    /// (frame tokens left in), and the layer's folder: for another frame.
    products: Vec<(Ptr, String)>,
    folder: PathBuf,
    /// The products' files after another frame, to tell.
    images: Option<Vec<PathBuf>>,
    /// Arnold's log lines told.
    logged: u64,
}

impl Arnold {
    /// Start Arnold, Arnold 7 or later, its image going into `pixels` bucket by
    /// bucket as it renders.
    pub fn new(config: Config, pixels: Arc<Pixels>) -> Result<Arnold, String> {
        if STARTED.swap(true, Ordering::SeqCst) {
            return Err("Arnold is already rendering in Imago".into());
        }
        let started = (|| {
            if let Some(dir) = &config.materialx {
                if std::env::var_os(MTLX).is_none() {
                    std::env::set_var(MTLX, dir);
                }
            }
            let ai = Api::load(&config.sdk)?;
            let version = ai.version();
            supported(&version)?;
            begin(&ai, &config.sdk);
            // SAFETY: once per process (STARTED), Arnold started; the driver's
            // table is a static, as Arnold keeps it.
            unsafe {
                driver::prepare(
                    pixels,
                    driver::Calls {
                        driver_initialize: ai.AiDriverInitialize,
                        output_next: ai.AiOutputIteratorGetNext,
                    },
                );
                let version = CString::new(version.as_str()).unwrap_or_default();
                (ai.AiNodeEntryInstall)(
                    ffi::NODE_DRIVER as c_int,
                    ffi::TYPE_NONE,
                    driver::NAME.as_ptr(),
                    c"".as_ptr(),
                    &driver::METHODS as *const driver::NodeMethods as *const std::ffi::c_void,
                    version.as_ptr(),
                );
            }
            let progress = Box::new(Progress {
                status: Some(ai.AiRenderGetStatus),
                ..Default::default()
            });
            Ok(Arnold {
                ai,
                config,
                universe: std::ptr::null_mut(),
                session: std::ptr::null_mut(),
                progress,
                told: 0,
                notes: vec![],
                products: vec![],
                folder: PathBuf::new(),
                images: None,
                logged: log_after(u64::MAX).1,
            })
        })();
        if started.is_err() {
            STARTED.store(false, Ordering::SeqCst);
        }
        started
    }

    fn status(&self) -> c_int {
        if self.session.is_null() {
            return ffi::NOT_STARTED;
        }
        // SAFETY: a live session.
        unsafe { (self.ai.AiRenderGetStatus)(self.session) }
    }

    /// Nodes are changed only while the render is paused.
    fn pause(&mut self) {
        if matches!(self.status(), ffi::RENDERING | ffi::RESTARTING) {
            // SAFETY: a live session; returns once paused.
            unsafe { (self.ai.AiRenderInterrupt)(self.session, ffi::BLOCKING) };
        }
    }

    /// What is loaded, let go.
    /// The products' file names at `frame`, their folders made: relative
    /// ones from the layer's folder.
    fn at_frame(&self, frame: f64) -> Vec<PathBuf> {
        let ai = &self.ai;
        let mut images = vec![];
        for (driver, name) in &self.products {
            let mut file = PathBuf::from(with_frame(name, frame));
            if file.is_relative() {
                file = self.folder.join(file);
            }
            if let Some(dir) = file.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            // SAFETY: a driver of the loaded universe (`products` is made
            // again with each load).
            unsafe { (ai.AiNodeSetStr)(*driver, ai.string("filename"), ai.string(&file.to_string_lossy())) };
            images.push(file);
        }
        images
    }

    fn unload(&mut self) {
        self.products.clear();
        // SAFETY: the session and universe made by `load`, ended once.
        unsafe {
            if !self.session.is_null() {
                self.pause();
                (self.ai.AiRenderEnd)(self.session);
                (self.ai.AiRenderSessionDestroy)(self.session);
                self.session = std::ptr::null_mut();
            }
            if !self.universe.is_null() {
                (self.ai.AiUniverseDestroy)(self.universe);
                self.universe = std::ptr::null_mut();
            }
        }
    }

    /// Every output (each AOV, once) to Imago's driver too: in memory, bucket
    /// by bucket, beside the products' files.
    fn pixels_out(&self, options: Ptr) {
        let ai = &self.ai;
        // SAFETY: the loaded universe, before its session renders.
        unsafe {
            let outputs = (ai.AiNodeGetArray)(options, ai.string("outputs"));
            if outputs.is_null() {
                return;
            }
            let n = (ai.AiArrayGetNumElements)(outputs);
            let lines: Vec<String> = (0..n).map(|i| ffi::text((ai.AiArrayGetStr)(outputs, i).0)).collect();
            let mut ours: Vec<String> = vec![];
            for line in lines.iter().filter_map(|l| pixels_line(l)) {
                let aov = |l: &str| l.split_whitespace().next().map(str::to_string);
                if !ours.iter().any(|o| aov(o) == aov(&line)) {
                    ours.push(line);
                }
            }
            if ours.is_empty() {
                return;
            }
            (ai.AiNode)(
                self.universe,
                ai.string(&driver_name()),
                ai.string(PIXELS_NODE),
                std::ptr::null_mut(),
            );
            let all = (ai.AiArrayAllocate)(n + ours.len() as u32, 1, ffi::TYPE_STRING);
            for (i, l) in lines.iter().chain(&ours).enumerate() {
                (ai.AiArraySetStr)(all, i as u32, ai.string(l));
            }
            (ai.AiNodeSetArray)(options, ai.string("outputs"), all);
        }
    }

    /// Render through `v`, the Viewer's view: a camera of Imago's own in the
    /// universe, the options' camera, at the view's size.
    fn view(&self, v: &crate::View) -> bool {
        let ai = &self.ai;
        // SAFETY: the loaded universe, changed while paused.
        unsafe {
            let options = (ai.AiUniverseGetOptions)(self.universe);
            let mut camera = (ai.AiNodeLookUpByName)(self.universe, ai.string(VIEW_NODE), std::ptr::null_mut());
            if camera.is_null() {
                camera = (ai.AiNode)(
                    self.universe,
                    ai.string("persp_camera"),
                    ai.string(VIEW_NODE),
                    std::ptr::null_mut(),
                );
            }
            if camera.is_null() {
                return false;
            }
            (ai.AiNodeSetMatrix)(camera, ai.string("matrix"), Matrix(v.matrix));
            (ai.AiNodeSetFlt)(camera, ai.string("fov"), v.fov);
            (ai.AiNodeSetPtr)(options, ai.string("camera"), camera);
            (ai.AiNodeSetInt)(options, ai.string("xres"), v.size[0].max(1) as c_int);
            (ai.AiNodeSetInt)(options, ai.string("yres"), v.size[1].max(1) as c_int);
        }
        true
    }

    fn apply(&self, e: &Edit) -> bool {
        let ai = &self.ai;
        // SAFETY: nodes of the loaded universe, changed while paused.
        unsafe {
            if let Change::View(v) = &e.change {
                return self.view(v);
            }
            if let Change::Frame(f) = e.change {
                // The products' files: in `edit`.
                let options = (ai.AiUniverseGetOptions)(self.universe);
                (ai.AiNodeSetFlt)(options, ai.string("frame"), f as f32);
                return true;
            }
            let node = match &e.change {
                Change::Settings(_) => (ai.AiUniverseGetOptions)(self.universe),
                _ => (ai.AiNodeLookUpByName)(self.universe, ai.string(&e.path), std::ptr::null_mut()),
            };
            if node.is_null() {
                return false;
            }
            let kind = (ai.AiNodeEntryGetType)((ai.AiNodeGetNodeEntry)(node));
            match &e.change {
                Change::Transform(m) => {
                    let has = [ffi::NODE_SHAPE, ffi::NODE_LIGHT, ffi::NODE_CAMERA].contains(&kind);
                    // Motion blur's keys are the layer's to give again.
                    let keys = (ai.AiNodeGetArray)(node, ai.string("matrix"));
                    if !has || (!keys.is_null() && (ai.AiArrayGetNumKeys)(keys) > 1) {
                        return false;
                    }
                    (ai.AiNodeSetMatrix)(node, ai.string("matrix"), Matrix(*m));
                    true
                }
                Change::Visible(shown) => {
                    if param_type(ai, node, "visibility") != Some(ffi::TYPE_BYTE) {
                        return false;
                    }
                    (ai.AiNodeSetByte)(node, ai.string("visibility"), if *shown { 255 } else { 0 });
                    true
                }
                Change::Points(points) => {
                    let vlist = (ai.AiNodeGetArray)(node, ai.string("vlist"));
                    if kind != ffi::NODE_SHAPE
                        || vlist.is_null()
                        || (ai.AiArrayGetNumElements)(vlist) as usize != points.len()
                        || (ai.AiArrayGetNumKeys)(vlist) != 1
                    {
                        return false;
                    }
                    let array =
                        (ai.AiArrayConvert)(points.len() as u32, 1, ffi::TYPE_VECTOR, points.as_ptr() as *const _);
                    (ai.AiNodeSetArray)(node, ai.string("vlist"), array);
                    true
                }
                Change::Attributes(attributes) => attributes.iter().all(|(name, v)| {
                    let param = match kind {
                        ffi::NODE_LIGHT => light_param(name),
                        ffi::NODE_SHADER => name.strip_prefix("inputs:"),
                        _ => None,
                    };
                    param.is_some_and(|p| set(ai, node, p, v))
                }),
                Change::Settings(settings) => settings.iter().all(|(name, v)| {
                    settings_params(name, v).is_some_and(|ps| ps.iter().all(|(p, v)| set(ai, node, p, v)))
                }),
                Change::Structure | Change::View(_) | Change::Frame(_) => false,
            }
        }
    }
}

/// The node of Imago's driver in each universe.
const PIXELS_NODE: &str = "/__xms_imago_pixels";
/// The camera of the Viewer's view, when it renders through it.
const VIEW_NODE: &str = "/__xms_imago_view";

fn driver_name() -> String {
    driver::NAME.to_string_lossy().into_owned()
}

/// An output line (`AOV type filter driver`, maybe a layer after) sent to
/// Imago's driver, of the same type.
fn pixels_line(line: &str) -> Option<String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    match words[..] {
        [aov, ty, filter, _, ..] => Some(format!("{aov} {ty} {filter} {PIXELS_NODE}")),
        _ => None,
    }
}

/// The Arnold parameter Arnold's USD reader sets from a light's attribute.
fn light_param(attribute: &str) -> Option<&'static str> {
    Some(match attribute {
        "inputs:intensity" => "intensity",
        "inputs:exposure" => "exposure",
        "inputs:color" => "color",
        "inputs:diffuse" => "diffuse",
        "inputs:specular" => "specular",
        "inputs:normalize" => "normalize",
        _ => return None,
    })
}

/// The options' parameters a Render Settings attribute sets.
fn settings_params(attribute: &str, v: &Value) -> Option<Vec<(String, Value)>> {
    if let Some(p) = attribute.strip_prefix("arnold:global:") {
        return Some(vec![(p.to_string(), v.clone())]);
    }
    match (attribute, v) {
        ("resolution", Value::Int2([x, y])) => {
            Some(vec![("xres".into(), Value::Int(*x)), ("yres".into(), Value::Int(*y))])
        }
        _ => None,
    }
}

/// The type of `node`'s parameter `name`, if it has it.
fn param_type(ai: &Api, node: Ptr, name: &str) -> Option<u8> {
    // SAFETY: a live node and its entry.
    unsafe {
        let p = (ai.AiNodeEntryLookUpParameter)((ai.AiNodeGetNodeEntry)(node), ai.string(name));
        (!p.is_null()).then(|| (ai.AiParamGetType)(p))
    }
}

/// Set `node`'s parameter `name` to `v`, if it has it, of that type.
fn set(ai: &Api, node: Ptr, name: &str, v: &Value) -> bool {
    let Some(ty) = param_type(ai, node, name) else {
        return false;
    };
    let p = ai.string(name);
    // SAFETY: a live node, the parameter of this type.
    unsafe {
        match (v, ty) {
            (Value::Float(x), ffi::TYPE_FLOAT) => (ai.AiNodeSetFlt)(node, p, *x),
            (Value::Int(x), ffi::TYPE_FLOAT) => (ai.AiNodeSetFlt)(node, p, *x as f32),
            (Value::Int(x), ffi::TYPE_INT) => (ai.AiNodeSetInt)(node, p, *x),
            (Value::Bool(x), ffi::TYPE_BOOLEAN) => (ai.AiNodeSetBool)(node, p, *x),
            (Value::Color([r, g, b]), ffi::TYPE_RGB) => (ai.AiNodeSetRGB)(node, p, *r, *g, *b),
            (Value::Token(s), ffi::TYPE_STRING) => (ai.AiNodeSetStr)(node, p, ai.string(s)),
            _ => return false,
        }
    }
    true
}

/// Called by Arnold as a render goes: counts the passes done. Returns the
/// status the render goes on with.
unsafe extern "C" fn update(data: Ptr, kind: c_int, info: *const UpdateInfo) -> c_int {
    // SAFETY: `data` is the backend's `Progress`, alive while the session is.
    let progress = unsafe { &*(data as *const Progress) };
    if let Some(info) = unsafe { info.as_ref() } {
        progress.total.store(info.total_passes, Ordering::Relaxed);
    }
    match kind {
        ffi::UPDATE_INTERRUPT => ffi::PAUSED,
        ffi::UPDATE_AFTER_PASS => {
            progress.passes.fetch_add(1, Ordering::Relaxed);
            ffi::RENDERING
        }
        ffi::UPDATE_FINISHED => {
            progress.passes.fetch_add(1, Ordering::Relaxed);
            progress.finished.store(true, Ordering::Relaxed);
            ffi::FINISHED
        }
        ffi::UPDATE_ERROR => {
            progress.failed.store(true, Ordering::Relaxed);
            ffi::FAILED
        }
        // Before or during a pass, imagers, direct outputs: the status as it is.
        _ => match (progress.status, unsafe { info.as_ref() }) {
            (Some(status), Some(info)) if !info.session.is_null() => unsafe { status(info.session) },
            _ => ffi::RENDERING,
        },
    }
}

impl Renderer for Arnold {
    fn load(&mut self, layer: &Path, frame: f64) -> Result<Vec<PathBuf>, String> {
        self.unload();
        let before = log_after(u64::MAX).1;
        let ai = &self.ai;
        let file = CString::new(layer.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        // SAFETY: a new universe, loaded before its session renders.
        unsafe {
            self.universe = (ai.AiUniverse)();
            let options = (ai.AiUniverseGetOptions)(self.universe);
            let params = (ai.AiParamValueMap)();
            (ai.AiParamValueMapSetFlt)(params, ai.string("frame"), frame as f32);
            let loaded = (ai.AiSceneLoad)(self.universe, file.as_ptr(), params);
            (ai.AiParamValueMapDestroy)(params);
            if !loaded {
                return Err(could_not_load(layer, before));
            }
            // After the load: the layer's Render Settings set the options.
            (ai.AiNodeSetBool)(options, ai.string("skip_license_check"), !self.config.licence);
            // The products' frame tokens, left for the renderer, put in; the
            // folders made. Relative paths are from the layer's folder.
            self.folder = match &self.config.outputs {
                Some(f) => f.clone(),
                None => layer.parent().unwrap_or(Path::new(".")).to_path_buf(),
            };
            self.products = ai
                .nodes(self.universe, ffi::NODE_DRIVER)
                .into_iter()
                .map(|(_, _, driver)| (driver, ai.str_param(driver, "filename")))
                .filter(|(_, name)| !name.is_empty())
                .collect();
            let images = self.at_frame(frame);
            self.pixels_out(options);
            self.session = (ai.AiRenderSession)(self.universe, ffi::SESSION_INTERACTIVE);
            Ok(images)
        }
    }

    fn render(&mut self, progressive: bool) -> Result<(), String> {
        let ai = &self.ai;
        self.told = 0;
        self.progress.passes.store(0, Ordering::Relaxed);
        self.progress.finished.store(false, Ordering::Relaxed);
        self.progress.failed.store(false, Ordering::Relaxed);
        // SAFETY: a live session, before it starts again.
        let taken = unsafe { (ai.AiRenderSetHintBool)(self.session, ai.string("progressive"), progressive) };
        if !taken && !progressive {
            self.notes
                .push("Arnold did not take the hint to render without passes: it renders in passes".into());
        }
        // SAFETY: a live session; the callback's data lives as long.
        unsafe {
            match self.status() {
                ffi::PAUSED => (ai.AiRenderRestart)(self.session),
                ffi::RENDERING | ffi::RESTARTING => {}
                status => {
                    if status == ffi::FINISHED {
                        (ai.AiRenderEnd)(self.session);
                    }
                    let data = &*self.progress as *const Progress as Ptr;
                    let code = (ai.AiRenderBegin)(self.session, ffi::RENDER_MODE_CAMERA, Some(update), data);
                    if code != 0 {
                        let why = ffi::ERRORS.get(code as usize).copied().unwrap_or("an error");
                        return Err(format!("Arnold did not start: {why}"));
                    }
                }
            }
        }
        Ok(())
    }

    fn live(&self, change: &Change) -> bool {
        !matches!(change, Change::Structure)
    }

    fn edit(&mut self, edits: &[Edit]) -> Result<bool, String> {
        if self.session.is_null() {
            return Ok(false);
        }
        self.pause();
        let applied = edits.iter().all(|e| self.apply(e));
        if let Some(f) = edits.iter().rev().find_map(|e| match e.change {
            Change::Frame(f) => Some(f),
            _ => None,
        }) {
            let images = self.at_frame(f);
            self.images = Some(images);
        }
        Ok(applied)
    }

    fn poll(&mut self) -> Vec<Event> {
        let mut events = vec![];
        let done = self.progress.passes.load(Ordering::Relaxed);
        let total = self.progress.total.load(Ordering::Relaxed);
        while self.told < done {
            events.push(Event::Pass {
                index: self.told,
                total,
            });
            self.told += 1;
        }
        if self.progress.finished.swap(false, Ordering::Relaxed) {
            events.push(Event::Finished);
        }
        events.extend(self.notes.drain(..).map(Event::Note));
        let (lines, seen) = log_after(self.logged);
        self.logged = seen;
        events.extend(lines.iter().map(|l| Event::Log(shown(l))));
        events.extend(self.images.take().map(Event::Images));
        if self.progress.failed.swap(false, Ordering::Relaxed) {
            events.push(Event::Failed("the render failed: Arnold's log says why".into()));
        }
        events
    }

    fn stop(&mut self) {
        self.pause();
    }
}

impl Drop for Arnold {
    fn drop(&mut self) {
        self.unload();
        let mut begun = BEGUN.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: once, after everything Arnold made is let go.
        unsafe { (self.ai.AiEnd)() };
        *begun = false;
        STARTED.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_udim_texture_is_its_tiles_on_disk() {
        let dir = std::env::temp_dir().join(format!("xms_tiles_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in [
            "wood.1001.exr",
            "wood.1002.exr",
            "wood.10x3.exr",
            "wood.exr",
            "stone.1001.exr",
        ] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        let tiles = tiles(&dir.join("wood.<UDIM>.exr"));
        assert_eq!(tiles, [dir.join("wood.1001.exr"), dir.join("wood.1002.exr")]);
        assert_eq!(super::tiles(&dir.join("wood.exr")), [dir.join("wood.exr")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Arnold's log: the lines after those seen, and a failed load says its
    /// errors (else its warnings).
    #[test]
    fn arnolds_log_is_told_and_says_why_a_load_failed() {
        let seen = log_after(u64::MAX).1;
        log(false, "[usd] no default prim".into());
        log(true, "can't load plugin usd_proc.so: libGLU.so.1: cannot open".into());
        let (lines, now) = log_after(seen);
        assert!(now >= seen + 2);
        let ours: Vec<String> = lines.iter().filter(|(_, t)| t.contains("usd")).map(shown).collect();
        assert_eq!(
            ours,
            [
                "Arnold warning: [usd] no default prim",
                "Arnold error: can't load plugin usd_proc.so: libGLU.so.1: cannot open"
            ]
        );
        let why = could_not_load(Path::new("/tmp/live.usda"), seen);
        assert!(why.contains("/tmp/live.usda: ") && why.contains("libGLU.so.1"), "{why}");
        assert!(!why.contains("no default prim"), "errors only, when there are: {why}");
        assert!(could_not_load(Path::new("/x.usda"), log_after(u64::MAX).1).contains("logged no error"));
    }

    /// An SDK set in Settings: Arnold's variables as a shell would have them.
    #[test]
    fn an_sdk_from_settings_sets_arnolds_environment() {
        let sdk = std::env::temp_dir().join(format!("xms_sdk_{}", std::process::id()));
        std::fs::create_dir_all(sdk.join("plugins")).unwrap();
        let had = std::env::var_os("ARNOLD_PATH");
        environment(&sdk);
        let plugins: Vec<PathBuf> = std::env::split_paths(&std::env::var_os("ARNOLD_PLUGIN_PATH").unwrap()).collect();
        assert_eq!(plugins[0], sdk.join("plugins"));
        environment(&sdk);
        let again: Vec<PathBuf> = std::env::split_paths(&std::env::var_os("ARNOLD_PLUGIN_PATH").unwrap()).collect();
        assert_eq!(again.len(), plugins.len(), "once");
        if had.is_none() {
            assert_eq!(std::env::var_os("ARNOLD_PATH"), Some(sdk.clone().into()));
        }
        let _ = std::fs::remove_dir_all(&sdk);
    }

    #[test]
    fn arnold_7_or_later_is_supported() {
        assert!(supported("7.5.0.0").is_ok() && supported("8.0.1").is_ok());
        assert!(supported("6.2.1.1").unwrap_err().contains("7 or later"));
        assert!(supported("").is_err());
    }

    #[test]
    fn no_sdk_says_where_it_looked() {
        let e = installed(Path::new("/nowhere/arnold")).unwrap_err();
        assert!(e.contains("/nowhere/arnold"), "{e}");
    }

    #[test]
    fn lights_and_settings_map_to_arnold_parameters() {
        assert_eq!(light_param("inputs:intensity"), Some("intensity"));
        assert_eq!(light_param("inputs:color"), Some("color"));
        // Shaping, width, colour temperature: a reload.
        assert_eq!(light_param("inputs:width"), None);
        assert_eq!(
            settings_params("arnold:global:AA_samples", &Value::Int(6)),
            Some(vec![("AA_samples".into(), Value::Int(6))])
        );
        assert_eq!(
            settings_params("resolution", &Value::Int2([1280, 720])),
            Some(vec![
                ("xres".into(), Value::Int(1280)),
                ("yres".into(), Value::Int(720))
            ])
        );
        assert_eq!(settings_params("pixelAspectRatio", &Value::Float(1.0)), None);
    }

    #[test]
    fn each_aov_goes_to_imagos_driver_too() {
        assert_eq!(
            pixels_line("RGBA RGBA /Render/Products/Vars/RGBA/filter /Render/Products/beauty").as_deref(),
            Some("RGBA RGBA /Render/Products/Vars/RGBA/filter /__xms_imago_pixels")
        );
        assert_eq!(
            pixels_line("Z FLOAT /Render/Products/Vars/Z/filter /Render/Products/beauty layer").as_deref(),
            Some("Z FLOAT /Render/Products/Vars/Z/filter /__xms_imago_pixels")
        );
        assert_eq!(pixels_line("RGBA RGBA"), None);
    }

    #[test]
    fn without_the_sdk_it_says_where_it_looked() {
        let e = Arnold::new(
            Config {
                sdk: "/no/arnold/here".into(),
                ..Default::default()
            },
            Default::default(),
        )
        .err()
        .unwrap();
        assert!(e.contains("/no/arnold/here"), "{e}");
        // And may be started again once it is there.
        assert!(!STARTED.load(Ordering::SeqCst));
    }
}
