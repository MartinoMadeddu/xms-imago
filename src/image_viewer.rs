//! The Image Viewer pane: renders and images, on `xms_image`.
//!
//! Its top bar renders the scene the view flag shows, without a Write USD
//! or Render Command in the graph: choose the RenderSettings prim (the one
//! the scene names to start with) and the camera (the settings' own, or
//! any camera of the scene), press Render. The scene is written as a layer
//! in a folder of its own under the system's temporary folder and its
//! command line (⚙: husk, kick or your own, with tokens) renders the
//! current frame of it there, in the background (`core::command`); husk
//! saves a snapshot every two seconds; each snapshot,
//! and the finished frame, comes into the view as it is written. Stop ends
//! it. Under the bar: the layer (AOV), channel, exposure and gamma of the
//! view, then the image, zoomed and panned, with the values under the
//! pointer; under it the snapshots bar (Snap keeps the image, to go back
//! and forth between it and the next render). Its log shows what the command prints.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use crate::core::command::{self, State};
use crate::core::render::SettingsInfo;
use crate::node_graph::NodeGraphState;
use crate::types::{EvalResult, IceGraphId};

/// The key its renders run under in `core::command`: no operator has it.
const RUN_KEY: usize = usize::MAX - 7;

/// A render going, or the last one: where it writes, and what of it is shown.
struct Run {
    image:    PathBuf,
    /// The file as last read, by when it was written.
    seen:     Option<SystemTime>,
    /// What the view's corner says.
    title:    String,
    finished: bool,
}

/// An image being read off the main thread.
type Reading = Arc<Mutex<Option<(Result<xms_image::Image, String>, String, Option<SystemTime>)>>>;

/// The pane's state.
#[derive(Default)]
pub struct ImageViewerState {
    pub view:  xms_image::ImageView,
    /// The RenderSettings prim chosen. Empty: the one the scene names.
    pub settings: String,
    /// The camera chosen. Empty: the RenderSettings prim's.
    pub camera: String,
    /// The command line rendering a frame, with tokens (`VIEWER_TOKENS`).
    /// Kept in Imago's config folder.
    line:      Option<String>,
    /// The scene's RenderSettings prims and cameras, by the graph's revision.
    lists:     Option<(u64, Vec<SettingsInfo>, Vec<String>)>,
    run:       Option<Run>,
    reading:   Reading,
    busy:      bool,
    message:   String,
    show_log:  bool,
    /// The ⚙ window is open.
    show_settings: bool,
    /// A file to open, typed in the ⚙ window.
    open_path: String,
}

/// The command lines the ⚙ menu fills in: name, line, what it does.
/// Custom is any other line.
pub const VIEWER_PRESETS: [(&str, &str, &str); 2] = [
    ("husk", "husk -R $DELEGATE -s \"$SETTINGS\" -c \"$CAMERA\" -f $F -n 1 -V 2a --snapshot 2 -o \"$IMAGE\" \"$USD\"",
        "Houdini's husk: Karma, or Arnold with its delegate for Arnold settings; a snapshot every two seconds"),
    ("kick", "kick -i \"$USD\" -frame $F -c \"$CAMERA\" -o \"$IMAGE\" -dw -dp -v 2",
        "Arnold's kick, reading the layer (arnold-usd's procedural): the image comes in when the frame is done"),
];

/// The tokens of its command line: token, what it is.
pub const VIEWER_TOKENS: [(&str, &str); 7] = [
    ("$USD", "the scene, written as a layer in the render folder"),
    ("$IMAGE", "the image to write, in the render folder"),
    ("$F", "the frame; $F4 four digits (0001)"),
    ("$SETTINGS", "the RenderSettings prim chosen"),
    ("$CAMERA", "the camera chosen, else the settings' own; with none, its option is left out"),
    ("$DELEGATE", "husk's -R for the settings' renderer: HdArnoldRendererPlugin for Arnold; for Karma none, its option left out"),
    ("$$", "a $; any other $ is the shell's ($HOME)"),
];

fn line_file() -> Option<PathBuf> { crate::file_browser::config_path("image_viewer_command.txt") }
/// Where husk's path was kept before the command line was: read once, to
/// start the line from it.
fn husk_file() -> Option<PathBuf> { crate::file_browser::config_path("husk_path.txt") }

impl ImageViewerState {
    /// Its command line: the one saved, else husk's preset (with husk's
    /// path, when one was given before).
    fn line(&mut self) -> &mut String {
        self.line.get_or_insert_with(|| {
            let saved = line_file().and_then(|f| std::fs::read_to_string(f).ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            saved.unwrap_or_else(|| {
                let husk = husk_file().and_then(|f| std::fs::read_to_string(f).ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && s != "husk");
                let preset = VIEWER_PRESETS[0].1;
                match husk { Some(h) => format!("\"{h}\"{}", &preset["husk".len()..]), None => preset.to_string() }
            })
        })
    }

    fn save_line(&mut self) {
        let value = self.line().clone();
        if let Some(f) = line_file() {
            if let Some(dir) = f.parent() { let _ = std::fs::create_dir_all(dir); }
            let _ = std::fs::write(f, value);
        }
    }

    /// Reads an image file in the background; it is shown when read.
    fn read(&mut self, path: PathBuf, title: String, written: Option<SystemTime>) {
        if self.busy { return; }
        self.busy = true;
        let slot = self.reading.clone();
        std::thread::spawn(move || {
            let result = xms_image::read(&path);
            if let Ok(mut s) = slot.lock() { *s = Some((result, title, written)); }
        });
    }
}

/// The scene the view flag shows, as primitives.
fn scene(graph: &NodeGraphState, subnets: &crate::ice::IceGraphStore) -> Option<Vec<crate::types::Prim>> {
    let eval = |sid: IceGraphId, mesh: &crate::core::geo::Geo, template: Option<&crate::core::geo::Geo>| -> crate::core::geo::Geo {
        subnets.get(sid).map(|sg| sg.evaluate(mesh, template)).unwrap_or_else(|| mesh.clone())
    };
    match graph.evaluate_scene(&eval)?.2 {
        EvalResult::Named(prims) => Some(prims),
        _ => None,
    }
}

/// What the Image Viewer's render folders are called, before the program's id.
const RENDER_DIR: &str = "xms-imago-render-";

/// Where the Image Viewer's renders go: a folder of this program's own,
/// deleted when it closes (`clean_up`).
fn render_dir() -> PathBuf { std::env::temp_dir().join(format!("{RENDER_DIR}{}", std::process::id())) }

/// Deletes this program's render folder (the layers written, the images
/// rendered): when it closes. A render going is stopped first, and given
/// two seconds to end, so its files can go.
pub fn clean_up() {
    command::stop(RUN_KEY);
    let t0 = std::time::Instant::now();
    while command::snapshot(RUN_KEY, 0).is_some_and(|s| s.state == State::Running) && t0.elapsed() < std::time::Duration::from_secs(2) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let _ = std::fs::remove_dir_all(render_dir());
}

/// Deletes the render folders of programs no longer running (one that
/// crashed, or was killed): when it starts. Another Imago running keeps its own.
pub fn clean_up_left_behind() {
    let Ok(read) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for e in read.filter_map(|e| e.ok()) {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(pid) = name.strip_prefix(RENDER_DIR).and_then(|p| p.parse::<u32>().ok()) else { continue };
        if pid != std::process::id() && !running(pid) { let _ = std::fs::remove_dir_all(e.path()); }
    }
}

/// Whether a program with this id is running. On Windows not asked: taken as running.
fn running(pid: u32) -> bool {
    #[cfg(not(windows))]
    { std::process::Command::new("kill").args(["-0", &pid.to_string()]).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(true) }
    #[cfg(windows)]
    { let _ = pid; true }
}

pub fn draw(ui: &mut egui::Ui, st: &mut ImageViewerState, graph: &NodeGraphState, subnets: &crate::ice::IceGraphStore, revision: u64) {
    // The scene's RenderSettings prims and cameras, when the graph changed.
    if st.lists.as_ref().is_none_or(|(r, ..)| *r != revision) {
        let (settings, cameras) = match scene(graph, subnets) {
            Some(prims) => (crate::core::render::settings_list(&prims), crate::core::render::cameras(&prims)),
            None => (vec![], vec![]),
        };
        st.lists = Some((revision, settings, cameras));
    }
    let (settings, cameras) = st.lists.as_ref().map(|(_, s, c)| (s.clone(), c.clone())).unwrap_or_default();
    if !st.settings.is_empty() && !settings.iter().any(|s| s.path == st.settings) { st.settings.clear(); }
    if !st.camera.is_empty() && !cameras.contains(&st.camera) { st.camera.clear(); }
    let chosen = settings.iter().find(|s| s.path == st.settings).or(settings.first()).cloned();

    // What has been read comes into the view.
    let done = st.reading.lock().ok().and_then(|mut s| s.take());
    if let Some((result, title, written)) = done {
        st.busy = false;
        let running = command::snapshot(RUN_KEY, 0).is_some_and(|s| s.state == State::Running);
        match result {
            Ok(img) => { st.view.set(Arc::new(img), title); if let Some(r) = &mut st.run { r.seen = written; } }
            // A snapshot read while it was being written: the next one will do.
            Err(_) if running => {}
            Err(e) => st.message = e,
        }
    }
    poll(st);

    let run = command::snapshot(RUN_KEY, 400);
    let running = run.as_ref().is_some_and(|r| r.state == State::Running);
    if running || st.busy { ui.ctx().request_repaint_after(std::time::Duration::from_millis(200)); }

    // ── Top bar: what to render, Render and Stop ───────────────────────────
    ui.horizontal(|ui| {
        let current = match &chosen { Some(s) => s.path.clone(), None => "No RenderSettings".into() };
        ui.add_enabled_ui(!settings.is_empty(), |ui| {
            egui::ComboBox::from_id_salt("image_viewer_settings").width(170.0).selected_text(current).show_ui(ui, |ui| {
                for s in &settings {
                    let label = if s.named { format!("{}   (the scene's)", s.path) } else { s.path.clone() };
                    let picked = chosen.as_ref().is_some_and(|c| c.path == s.path);
                    if ui.selectable_label(picked, label).clicked() { st.settings = if s.named { String::new() } else { s.path.clone() }; }
                }
            }).response.on_hover_text("The RenderSettings prim rendered");
        });
        let own = chosen.as_ref().and_then(|s| s.camera.clone());
        let camera_text = if st.camera.is_empty() {
            match &own { Some(c) => format!("Its camera: {c}"), None => "Its camera".into() }
        } else { st.camera.clone() };
        ui.add_enabled_ui(chosen.is_some(), |ui| {
            egui::ComboBox::from_id_salt("image_viewer_camera").width(190.0).selected_text(camera_text).show_ui(ui, |ui| {
                let label = match &own { Some(c) => format!("The RenderSettings' ({c})"), None => "The RenderSettings'".into() };
                if ui.selectable_label(st.camera.is_empty(), label).clicked() { st.camera.clear(); }
                for c in &cameras {
                    if ui.selectable_label(st.camera == *c, c.as_str()).clicked() { st.camera = c.clone(); }
                }
            }).response.on_hover_text("The camera rendered through");
        });
        ui.separator();
        let can = chosen.is_some() && !running;
        if ui.add_enabled(can, egui::Button::new("▶ Render")).on_hover_text("The current frame of the scene the view flag shows, with the command line in ⚙, in the background").clicked() {
            if let Some(s) = &chosen { start(st, graph, subnets, s); }
        }
        if ui.add_enabled(running, egui::Button::new("⏹ Stop")).clicked() { command::stop(RUN_KEY); }
        match &run {
            Some(r) if r.state == State::Running => {
                match r.progress {
                    Some(p) => { ui.add(egui::ProgressBar::new(p / 100.0).desired_width(120.0).show_percentage()); }
                    None => { ui.spinner(); }
                }
            }
            Some(r) => {
                let secs = r.elapsed.as_secs_f64();
                let (text, colour) = match &r.state {
                    State::Done => (format!("Done in {secs:.1} s"), crate::properties::form::ink::OK()),
                    State::Stopped => ("Stopped".to_string(), egui::Color32::from_gray(200)),
                    State::Failed(why) => (why.clone(), crate::properties::form::ink::BAD()),
                    State::Running => (String::new(), egui::Color32::WHITE),
                };
                ui.label(egui::RichText::new(text).color(colour));
            }
            None => {}
        }
        if !st.message.is_empty() { ui.label(egui::RichText::new(st.message.clone()).color(crate::properties::form::ink::BAD())); }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.toggle_value(&mut st.show_settings, "⚙").on_hover_text("Its command line (husk, kick or your own), and opening an image: in a window of its own");
            ui.toggle_value(&mut st.show_log, "Log").on_hover_text("What the command prints");
        });
    });
    settings_window(ui.ctx(), st, graph, chosen.as_ref());
    // ── The view's own controls, then the image ───────────────────────────
    ui.horizontal(|ui| st.view.controls(ui));
    ui.separator();
    // The image, the snapshots bar under it, the log under that.
    let w = ui.available_width();
    let below = 34.0 + if st.show_log { 210.0 } else { 0.0 };
    let h = (ui.available_height() - below).max(80.0);
    ui.allocate_ui(egui::vec2(w, h), |ui| st.view.canvas(ui));
    ui.horizontal(|ui| st.view.snapshots(ui));
    if st.show_log {
        crate::properties::render::console(ui, "image_viewer_log", run.as_ref(), 150.0, "Nothing yet: press Render");
    }
}

/// ⚙: a floating window, so its menus (the presets) open inside it rather
/// than closing it as a menu would. Its command line, and an image to open.
fn settings_window(ctx: &egui::Context, st: &mut ImageViewerState, graph: &NodeGraphState, chosen: Option<&SettingsInfo>) {
    let mut open = st.show_settings;
    egui::Window::new("Image Viewer: render command").id(egui::Id::new("image_viewer_settings_window"))
        .open(&mut open).collapsible(false).resizable(true).default_width(460.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Command");
                let line = st.line().clone();
                let current = VIEWER_PRESETS.iter().find(|(_, l, _)| *l == line).map(|(n, _, _)| *n).unwrap_or("Custom");
                egui::ComboBox::from_id_salt("image_viewer_command").width(100.0).selected_text(current).show_ui(ui, |ui| {
                    for (name, l, about) in VIEWER_PRESETS {
                        if ui.selectable_label(current == name, name).on_hover_text(about).clicked() { *st.line() = l.to_string(); st.save_line(); }
                    }
                    // Custom: the line as it is, to change.
                    let _ = ui.selectable_label(current == "Custom", "Custom").on_hover_text("Your own line: change it below. Any line that is not a preset's is Custom");
                }).response.on_hover_text("husk, kick, or your own line");
            });
            let tokens = VIEWER_TOKENS.iter().map(|(t, what)| format!("{t}  {what}")).collect::<Vec<_>>().join("\n");
            let r = ui.add(egui::TextEdit::multiline(st.line()).code_editor().desired_rows(3).desired_width(420.0))
                .on_hover_text(format!("Run through the system's shell, in the render folder. Tokens:\n{tokens}"));
            if r.changed() { st.save_line(); }
            if let Some(s) = chosen {
                let (template, dir) = (st.line().clone(), render_dir());
                let frame = graph.frame().round() as i64;
                let shown = line_for(&template, s, &st.camera, frame, &dir.join(format!("render.{frame:04}.exr")), &dir.join("scene.usda"));
                ui.add(egui::Label::new(egui::RichText::new(shown).monospace().small().color(crate::properties::form::ink::DIM())).wrap());
            }
            ui.label(egui::RichText::new("On macOS a program started from the Dock does not see your shell's PATH: give the renderer's full path (…/Houdini.framework/Versions/Current/Resources/bin/husk, …/Arnold/bin/kick)").small());
            ui.separator();
            ui.label("Open an image");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut st.open_path).desired_width(300.0).hint_text("/path/to/image.exr"));
                if ui.button("Open").clicked() && !st.open_path.trim().is_empty() {
                    let p = PathBuf::from(st.open_path.trim());
                    let title = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    st.message.clear();
                    st.read(p, title, None);
                }
            });
        });
    st.show_settings = open;
}

/// Starts rendering the current frame of the scene with these settings.
fn start(st: &mut ImageViewerState, graph: &NodeGraphState, subnets: &crate::ice::IceGraphStore, settings: &SettingsInfo) {
    st.message.clear();
    if crate::usd_scene::any_reading() { st.message = "A stage is still being read: render when it is done".into(); return; }
    let Some(prims) = scene(graph, subnets) else { st.message = "The view flag shows no scene".into(); return };
    let dir = render_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) { st.message = format!("{}: {e}", dir.display()); return; }
    let layer = dir.join("scene.usda");
    if let Err(e) = crate::usd_write::write_override(&prims, &layer) { st.message = format!("The scene could not be written: {e}"); return; }
    let frame = graph.frame().round() as i64;
    let image = dir.join(format!("render.{frame:04}.exr"));
    let _ = std::fs::remove_file(&image);
    let line = line_for(&st.line().clone(), settings, &st.camera, frame, &image, &layer);
    command::start(RUN_KEY, vec![line], Some(dir), vec![]);
    // What comes in is shown, whatever snapshot was.
    st.view.show_active();
    let camera = if st.camera.is_empty() { settings.camera.clone().unwrap_or_default() } else { st.camera.clone() };
    st.run = Some(Run { image, seen: None, title: format!("frame {frame}   {}   {camera}", settings.path), finished: false });
}

/// The command line rendering one frame of a layer to an image: its
/// template with the tokens put in, an option whose token stands for
/// nothing (no camera, Karma's delegate) left out.
fn line_for(template: &str, settings: &SettingsInfo, camera: &str, frame: i64, image: &Path, layer: &Path) -> String {
    let camera = if camera.is_empty() { settings.camera.clone().unwrap_or_default() } else { camera.to_string() };
    let delegate = if settings.renderer == "arnold" { "HdArnoldRendererPlugin" } else { "" };
    let t = command::Tokens {
        usd: layer.display().to_string(), name: "ImageViewer".into(), start: frame, end: frame, frame,
        more: vec![("IMAGE", image.display().to_string()), ("SETTINGS", settings.path.clone()), ("CAMERA", camera), ("DELEGATE", delegate.into())],
    };
    command::expand(&command::without_empty(template, &t), &t)
}

/// A render's image read again when the file changes, until it is finished.
fn poll(st: &mut ImageViewerState) {
    let Some(r) = &st.run else { return };
    if r.finished || st.busy { return; }
    let running = command::snapshot(RUN_KEY, 0).is_some_and(|s| s.state == State::Running);
    let written = std::fs::metadata(&r.image).and_then(|m| m.modified()).ok();
    match written {
        Some(w) if r.seen != Some(w) => {
            let (path, title) = (r.image.clone(), r.title.clone());
            st.read(path, title, Some(w));
        }
        // Up to date: done once the render is.
        _ if !running => { if let Some(r) = &mut st.run { r.finished = true; } }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn render_folders_left_behind_are_deleted() {
        // An id no program has (beyond the usual pid range).
        let gone = std::env::temp_dir().join(format!("{RENDER_DIR}{}", 4_000_000_000u32));
        let mine = render_dir();
        std::fs::create_dir_all(&gone).unwrap();
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::write(mine.join("scene.usda"), "#usda 1.0").unwrap();
        clean_up_left_behind();
        assert!(!gone.exists() && mine.exists(), "only folders of programs not running");
        clean_up();
        assert!(!mine.exists(), "its own when it closes");
    }

    #[test]
    fn a_frame_is_rendered_with_the_settings_and_camera_chosen() {
        let (husk, kick) = (VIEWER_PRESETS[0].1, VIEWER_PRESETS[1].1);
        let s = SettingsInfo { path: "/Render/Settings".into(), camera: None, renderer: "karma", named: true };
        let line = line_for(husk, &s, "", 12, Path::new("/tmp/r/render.0012.exr"), Path::new("/tmp/r/scene.usda"));
        assert_eq!(line, "husk -s \"/Render/Settings\" -f 12 -n 1 -V 2a --snapshot 2 -o \"/tmp/r/render.0012.exr\" \"/tmp/r/scene.usda\"", "Karma, no camera known: no -R, no -c");
        let arnold = SettingsInfo { renderer: "arnold", camera: Some("/cam".into()), ..s };
        let line = line_for(husk, &arnold, "/shot/cam2", 1, Path::new("o.exr"), Path::new("l.usda"));
        assert!(line.starts_with("husk -R HdArnoldRendererPlugin -s \"/Render/Settings\" -c \"/shot/cam2\" -f 1"), "{line}");
        let line = line_for(kick, &arnold, "", 3, Path::new("o.exr"), Path::new("l.usda"));
        assert_eq!(line, "kick -i \"l.usda\" -frame 3 -c \"/cam\" -o \"o.exr\" -dw -dp -v 2", "the settings' camera when none is chosen");
        let line = line_for("/opt/my/render $USD $F4 > $HOME/log", &arnold, "", 7, Path::new("o.exr"), Path::new("l.usda"));
        assert_eq!(line, "/opt/my/render l.usda 0007 > $HOME/log", "a custom line");
    }
}
