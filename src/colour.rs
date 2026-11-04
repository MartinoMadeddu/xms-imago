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

    /// What it shows until one is chosen: the Image Viewer a render as on a
    /// reference monitor, Rec.1886 (when the config has that display), the
    /// Viewer the config's default.
    fn default_view(self) -> View {
        match self {
            Pane::Viewer => View::default(),
            Pane::ImageViewer => View {
                display: "Rec.1886 Rec.709 - Display".into(),
                view: "ACES 1.0 - SDR Video".into(),
            },
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

/// The display and view a pane shows: the one chosen, else the pane's own
/// default, else the config's (when the config has neither).
pub fn view(pane: Pane) -> View {
    let mut chosen = CHOSEN.lock().unwrap_or_else(|e| e.into_inner());
    let chosen = chosen.get_or_insert_with(|| {
        file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|t| read(&t))
            .unwrap_or_default()
    });
    let v = chosen.get(&pane).cloned().unwrap_or_else(|| pane.default_view());
    Colour::get().valid(&v)
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

/// Where the config came from, for a tooltip or a settings page.
pub fn about() -> String {
    let colour = Colour::get();
    let from = match &colour.source {
        xms_colour::Source::Env(path) => format!("$OCIO: {path}"),
        xms_colour::Source::Imago => "Imago's own (set $OCIO for another)".into(),
    };
    format!(
        "Config: {} ({from}). The scene's colours (USD's, {}) go through the working space ({}) to the display",
        colour.name(),
        colour.scene,
        colour.rendering()
    )
}

/// A pane's two colour menus, View then Display, as Houdini and Nuke have
/// them, and a warning mark when `$OCIO`'s config was not used.
pub fn menus(ui: &mut egui::Ui, pane: Pane) {
    let colour = Colour::get();
    let now = view(pane);
    let about = about();
    let id = |what: &str| egui::Id::new(("colour", pane.key(), what));
    egui::ComboBox::from_id_salt(id("view"))
        .selected_text(&now.view)
        .show_ui(ui, |ui| {
            for v in colour.views(&now.display) {
                if ui.selectable_label(now.view == v, v.as_str()).clicked() {
                    set_view(pane, colour.on_display(&now.display, &v));
                }
            }
        })
        .response
        .on_hover_text(format!("View: how the scene's light is shown (OpenColorIO). {about}"));
    egui::ComboBox::from_id_salt(id("display"))
        .selected_text(&now.display)
        .show_ui(ui, |ui| {
            for d in colour.displays() {
                // The same view, if the display has it.
                if ui.selectable_label(now.display == d, d.as_str()).clicked() {
                    set_view(pane, colour.on_display(&d, &now.view));
                }
            }
        })
        .response
        .on_hover_text(format!(
            "Display: the screen it is shown on (sRGB for most monitors). {about}"
        ));
    if let Some(p) = &colour.problem {
        crate::properties::form::status_label(ui, crate::properties::form::Status::Bad, "OCIO").on_hover_text(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Until one is chosen, the Image Viewer shows ACES 1.0 SDR on Rec.1886,
    /// the Viewer the config's default (ACES 1.0 SDR on sRGB in Imago's).
    #[test]
    fn each_pane_has_its_default() {
        let c = Colour::load(None);
        let image = c.valid(&Pane::ImageViewer.default_view());
        assert_eq!(
            (image.display.as_str(), image.view.as_str()),
            ("Rec.1886 Rec.709 - Display", "ACES 1.0 - SDR Video")
        );
        let viewer = c.valid(&Pane::Viewer.default_view());
        assert_eq!(
            (viewer.display.as_str(), viewer.view.as_str()),
            ("sRGB - Display", "ACES 1.0 - SDR Video")
        );
    }

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
