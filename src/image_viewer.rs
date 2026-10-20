//! The Image Viewer pane: renders and images, on `xms_image`.
//!
//! Its top bar renders the scene the view flag shows, without a Write USD
//! or Render Command in the graph: choose the RenderSettings prim (the one
//! the scene names to start with) and the camera (the settings' own, or
//! any camera of the scene), press Render. The scene is written as a layer
//! in a folder of its own under the system's temporary folder and husk
//! renders the current frame of it there, in the background
//! (`core::command`), saving a snapshot every two seconds; each snapshot,
//! and the finished frame, comes into the view as it is written. Stop ends
//! it. Under the bar: the layer (AOV), channel, exposure and gamma of the
//! view, then the image, zoomed and panned, with the values under the
//! pointer; under it the snapshots bar (Snap keeps the image, to go back
//! and forth between it and the next render). Its log shows what husk prints.

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
    /// husk: its name on the PATH, or its full path. Kept in Imago's config folder.
    husk:      Option<String>,
    /// The scene's RenderSettings prims and cameras, by the graph's revision.
    lists:     Option<(u64, Vec<SettingsInfo>, Vec<String>)>,
    run:       Option<Run>,
    reading:   Reading,
    busy:      bool,
    message:   String,
    show_log:  bool,
    /// A file to open, typed in the ⚙ menu.
    open_path: String,
}

fn husk_file() -> Option<PathBuf> { crate::file_browser::config_path("husk_path.txt") }

impl ImageViewerState {
    fn husk(&mut self) -> &mut String {
        self.husk.get_or_insert_with(|| {
            husk_file().and_then(|f| std::fs::read_to_string(f).ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
                .unwrap_or_else(|| "husk".into())
        })
    }

    fn save_husk(&mut self) {
        let value = self.husk().clone();
        if let Some(f) = husk_file() {
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

/// Where the Image Viewer's renders go: a folder of this program's own.
fn render_dir() -> PathBuf { std::env::temp_dir().join(format!("xms-imago-render-{}", std::process::id())) }

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
        if ui.add_enabled(can, egui::Button::new("▶ Render")).on_hover_text("The current frame of the scene the view flag shows, with husk, in the background").clicked() {
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
                    State::Done => (format!("Done in {secs:.1} s"), egui::Color32::from_rgb(150, 210, 150)),
                    State::Stopped => ("Stopped".to_string(), egui::Color32::from_gray(200)),
                    State::Failed(why) => (why.clone(), egui::Color32::from_rgb(235, 140, 120)),
                    State::Running => (String::new(), egui::Color32::WHITE),
                };
                ui.label(egui::RichText::new(text).color(colour));
            }
            None => {}
        }
        if !st.message.is_empty() { ui.label(egui::RichText::new(st.message.clone()).color(egui::Color32::from_rgb(235, 140, 120))); }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            crate::menus::menu(ui, "⚙", |ui| {
                ui.label("husk");
                let husk = st.husk();
                let r = ui.add(egui::TextEdit::singleline(husk).desired_width(360.0).hint_text("husk, or its full path"));
                if r.lost_focus() || r.changed() { st.save_husk(); }
                ui.label(egui::RichText::new("On macOS a program started from the Dock does not see your shell's PATH: give husk's full path (…/Houdini.framework/Versions/Current/Resources/bin/husk)").small());
                ui.separator();
                ui.label("Open an image");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut st.open_path).desired_width(300.0).hint_text("/path/to/image.exr"));
                    if ui.button("Open").clicked() && !st.open_path.trim().is_empty() {
                        let p = PathBuf::from(st.open_path.trim());
                        let title = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        st.message.clear();
                        st.read(p, title, None);
                        ui.close();
                    }
                });
            });
            ui.toggle_value(&mut st.show_log, "Log").on_hover_text("What husk prints");
        });
    });
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
    let line = husk_line(&st.husk().clone(), settings, &st.camera, frame, &image, &layer);
    command::start(RUN_KEY, vec![line], Some(dir), vec![]);
    // What comes in is shown, whatever snapshot was.
    st.view.show_active();
    let camera = if st.camera.is_empty() { settings.camera.clone().unwrap_or_default() } else { st.camera.clone() };
    st.run = Some(Run { image, seen: None, title: format!("frame {frame}   {}   {camera}", settings.path), finished: false });
}

/// The husk command line rendering one frame of a layer to an image,
/// saving a snapshot every two seconds.
fn husk_line(husk: &str, settings: &SettingsInfo, camera: &str, frame: i64, image: &Path, layer: &Path) -> String {
    let renderer = if settings.renderer == "arnold" { " -R HdArnoldRendererPlugin" } else { "" };
    let camera = if camera.is_empty() { String::new() } else { format!(" -c \"{camera}\"") };
    format!("\"{husk}\"{renderer} -s \"{}\"{camera} -f {frame} -n 1 -V 2a --snapshot 2 -o \"{}\" \"{}\"",
        settings.path, image.display(), layer.display())
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

    #[test]
    fn a_frame_is_rendered_with_the_settings_and_camera_chosen() {
        let s = SettingsInfo { path: "/Render/Settings".into(), camera: Some("/cam".into()), renderer: "karma", named: true };
        let line = husk_line("/opt/hfs/bin/husk", &s, "", 12, Path::new("/tmp/r/render.0012.exr"), Path::new("/tmp/r/scene.usda"));
        assert_eq!(line, "\"/opt/hfs/bin/husk\" -s \"/Render/Settings\" -f 12 -n 1 -V 2a --snapshot 2 -o \"/tmp/r/render.0012.exr\" \"/tmp/r/scene.usda\"");
        let arnold = SettingsInfo { renderer: "arnold", ..s };
        let line = husk_line("husk", &arnold, "/shot/cam2", 1, Path::new("o.exr"), Path::new("l.usda"));
        assert!(line.starts_with("\"husk\" -R HdArnoldRendererPlugin -s \"/Render/Settings\" -c \"/shot/cam2\" -f 1"), "{line}");
    }
}
