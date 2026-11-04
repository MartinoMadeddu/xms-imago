//! Which display and view each pane shows the scene in (`xms_colour`), and
//! the menu choosing it. Remembered between sessions, by pane; a view the
//! config does not have is its default.

use std::collections::BTreeMap;
use std::sync::Mutex;

pub use xms_colour::{Colour, View};

/// The panes that show colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Pane {
    Viewer,
    ImageViewer,
}

impl Pane {
    fn key(self) -> &'static str {
        match self {
            Pane::Viewer => "viewer",
            Pane::ImageViewer => "image_viewer",
        }
    }
}

static CHOSEN: Mutex<Option<BTreeMap<Pane, View>>> = Mutex::new(None);

fn file() -> Option<std::path::PathBuf> {
    crate::file_browser::config_path("colour.txt")
}

/// One line per pane: its key, display and view, tab separated.
fn read(text: &str) -> BTreeMap<Pane, View> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        let pane = [Pane::Viewer, Pane::ImageViewer]
            .into_iter()
            .find(|p| parts.first() == Some(&p.key()));
        if let (Some(pane), [_, display, view]) = (pane, parts.as_slice()) {
            out.insert(
                pane,
                View {
                    display: display.to_string(),
                    view: view.to_string(),
                },
            );
        }
    }
    out
}

fn write(chosen: &BTreeMap<Pane, View>) -> String {
    chosen
        .iter()
        .map(|(p, v)| format!("{}\t{}\t{}\n", p.key(), v.display, v.view))
        .collect()
}

/// The display and view a pane shows: the one chosen, if the config has it,
/// else the config's default.
pub fn view(pane: Pane) -> View {
    let mut chosen = CHOSEN.lock().unwrap_or_else(|e| e.into_inner());
    let chosen = chosen.get_or_insert_with(|| {
        file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|t| read(&t))
            .unwrap_or_default()
    });
    Colour::get().valid(&chosen.get(&pane).cloned().unwrap_or_default())
}

/// Choose a pane's display and view, and remember it.
pub fn set_view(pane: Pane, v: View) {
    let mut chosen = CHOSEN.lock().unwrap_or_else(|e| e.into_inner());
    let chosen = chosen.get_or_insert_with(BTreeMap::new);
    chosen.insert(pane, v);
    if let Some(f) = file() {
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(f, write(chosen));
    }
}

/// The menu choosing a pane's display and view: each display's views, and
/// which config they come from.
pub fn menu(ui: &mut egui::Ui, pane: Pane) {
    let colour = Colour::get();
    let now = view(pane);
    crate::menus::menu(ui, format!("{} ⏷", now.view), |ui| {
        for display in colour.displays() {
            ui.label(egui::RichText::new(&display).small().weak());
            for v in colour.views(&display) {
                let this = View {
                    display: display.clone(),
                    view: v.clone(),
                };
                if ui.radio(now == this, v.as_str()).clicked() {
                    set_view(pane, this);
                    ui.close();
                }
            }
        }
        ui.separator();
        let from = match &colour.source {
            xms_colour::Source::Env(path) => format!("$OCIO: {path}"),
            xms_colour::Source::Imago => "Imago's own (set $OCIO for another)".into(),
        };
        ui.weak(format!("Config: {} ({from})", colour.name()));
        ui.weak(format!("The scene is in {}", colour.scene));
        if let Some(p) = &colour.problem {
            crate::properties::form::status_label(ui, crate::properties::form::Status::Bad, p);
        }
    })
    .response
    .on_hover_text(format!(
        "How the scene's colours are shown: {} on {} (OpenColorIO)",
        now.view, now.display
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choices_are_read_back_as_written() {
        let mut chosen = BTreeMap::new();
        chosen.insert(
            Pane::Viewer,
            View {
                display: "sRGB - Display".into(),
                view: "ACES 1.0 - SDR Video".into(),
            },
        );
        chosen.insert(
            Pane::ImageViewer,
            View {
                display: "Display P3 - Display".into(),
                view: "Raw".into(),
            },
        );
        assert_eq!(read(&write(&chosen)), chosen);
        assert!(read("junk\nviewer\tonly two").is_empty());
    }
}
