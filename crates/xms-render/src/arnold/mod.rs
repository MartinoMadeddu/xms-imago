//! The Arnold backend: `libai` from the user's SDK, opened at run time, loading
//! the layer with its own USD reader (`AiSceneLoad`) and rendering it in an
//! interactive session. Its nodes are named by prim path, so an edit is a few
//! calls on the node of that name; what has no node, or no parameter Arnold's
//! reader would have set from that attribute, is a reload.

mod driver;
mod ffi;

use std::ffi::{c_int, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

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
}

impl Arnold {
    /// Start Arnold, Arnold 7 or later, its image going into `pixels` bucket by
    /// bucket as it renders.
    pub fn new(config: Config, pixels: Arc<Pixels>) -> Result<Arnold, String> {
        if STARTED.swap(true, Ordering::SeqCst) {
            return Err("Arnold is already rendering in Imago".into());
        }
        let started = (|| {
            const MTLX: &str = "PXR_MTLX_STDLIB_SEARCH_PATHS";
            if let Some(dir) = &config.materialx {
                if std::env::var_os(MTLX).is_none() {
                    std::env::set_var(MTLX, dir);
                }
            }
            let ai = Api::load(&config.sdk)?;
            let version = ai.version();
            if version
                .split('.')
                .next()
                .and_then(|m| m.parse::<u32>().ok())
                .unwrap_or(0)
                < 7
            {
                return Err(format!("Arnold {version}: Imago renders with Arnold 7 or later"));
            }
            // SAFETY: once per process (STARTED), before any other call; the
            // driver's table is a static, as Arnold keeps it.
            unsafe {
                (ai.AiBegin)(ffi::SESSION_INTERACTIVE);
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
    fn unload(&mut self) {
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

    /// The first output (the beauty) to Imago's driver too: in memory, bucket
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
            let Some(line) = lines.first().and_then(|l| pixels_line(l)) else {
                return;
            };
            (ai.AiNode)(
                self.universe,
                ai.string(&driver_name()),
                ai.string(PIXELS_NODE),
                std::ptr::null_mut(),
            );
            let all = (ai.AiArrayAllocate)(n + 1, 1, ffi::TYPE_STRING);
            for (i, l) in lines.iter().chain([&line]).enumerate() {
                (ai.AiArraySetStr)(all, i as u32, ai.string(l));
            }
            (ai.AiNodeSetArray)(options, ai.string("outputs"), all);
        }
    }

    fn apply(&self, e: &Edit) -> bool {
        let ai = &self.ai;
        // SAFETY: nodes of the loaded universe, changed while paused.
        unsafe {
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
                Change::Structure => false,
            }
        }
    }
}

/// The node of Imago's driver in each universe.
const PIXELS_NODE: &str = "/__xms_imago_pixels";

fn driver_name() -> String {
    driver::NAME.to_string_lossy().into_owned()
}

/// An output line (`AOV type filter driver`, maybe a layer after) sent to
/// Imago's driver as RGBA.
fn pixels_line(line: &str) -> Option<String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    match words[..] {
        [aov, _, filter, _, ..] => Some(format!("{aov} RGBA {filter} {PIXELS_NODE}")),
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
                return Err(format!("Arnold could not load {}", layer.display()));
            }
            // After the load: the layer's Render Settings set the options.
            (ai.AiNodeSetBool)(options, ai.string("skip_license_check"), !self.config.licence);
            // The products' frame tokens, left for the renderer, put in; the
            // folders made. Relative paths are from the layer's folder.
            let mut images = vec![];
            for (_, _, driver) in ai.nodes(self.universe, ffi::NODE_DRIVER) {
                let name = ai.str_param(driver, "filename");
                if name.is_empty() {
                    continue;
                }
                let mut file = PathBuf::from(with_frame(&name, frame));
                if file.is_relative() {
                    file = layer.parent().unwrap_or(Path::new(".")).join(file);
                }
                if let Some(dir) = file.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                (ai.AiNodeSetStr)(driver, ai.string("filename"), ai.string(&file.to_string_lossy()));
                images.push(file);
            }
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
        Ok(edits.iter().all(|e| self.apply(e)))
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
        events.extend(self.notes.drain(..).map(Event::Failed));
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
        // SAFETY: once, after everything Arnold made is let go.
        unsafe { (self.ai.AiEnd)() };
        STARTED.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_beauty_goes_to_imagos_driver_too() {
        assert_eq!(
            pixels_line("RGBA RGBA /Render/Products/Vars/RGBA/filter /Render/Products/beauty").as_deref(),
            Some("RGBA RGBA /Render/Products/Vars/RGBA/filter /__xms_imago_pixels")
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
