//! A Light Linker's parameters (`core::light_link`): its lights and prims as
//! Prims Location lists, then the grid of their links.

use crate::core::collections::Held;
use crate::core::light_link::{cell, cell_mut, Link};

use super::form::{group, ink, Status};
use super::ui::{locations_tab, TabKind};

/// What comes into the operator.
pub struct Scene<'a> {
    /// Every prim's path.
    pub paths: &'a [String],
    /// The lights' paths.
    pub lights: &'a [String],
    /// The Scene Explorer's selection.
    pub selected: &'a [String],
    /// The collections.
    pub held: &'a [Held],
}

pub fn rows(
    ui: &mut egui::Ui,
    lights: &mut Vec<String>,
    prims: &mut Vec<String>,
    links: &mut Vec<Link>,
    scene: &Scene,
) {
    let (lights_were, prims_were) = (lights.clone(), prims.clone());
    locations_tab(
        ui,
        lights,
        scene.lights,
        scene.selected,
        TabKind::Collects,
        None,
        Some("Lights"),
        scene.held,
    );
    locations_tab(
        ui,
        prims,
        scene.paths,
        scene.selected,
        TabKind::Collects,
        None,
        Some("Prims"),
        scene.held,
    );
    keep_cells(links, &lights_were, lights, true);
    keep_cells(links, &prims_were, prims, false);
    grid(ui, lights, prims, links);
}

/// The cells follow a row or column whose text changed, and go with one
/// taken away. `light`: the light rows, else the prim columns.
fn keep_cells(links: &mut Vec<Link>, was: &[String], now: &[String], light: bool) {
    let side = |l: &Link| if light { l.light.clone() } else { l.prim.clone() };
    if was.len() == now.len() {
        for (a, b) in was.iter().zip(now).filter(|(a, b)| a != b) {
            for l in links.iter_mut().filter(|l| side(l) == *a) {
                if light {
                    l.light = b.clone();
                } else {
                    l.prim = b.clone();
                }
            }
        }
    }
    links.retain(|l| now.contains(&side(l)) && !l.is_empty());
}

/// A cell's state, shown: as it comes in, off, on.
fn mark(ui: &mut egui::Ui, letter: &str, v: Option<bool>, what: &str) -> egui::Response {
    let (text, colour, tip) = match v {
        None => (
            "·".to_string(),
            ink::DIM(),
            format!("{what}: as it comes in. Click: off"),
        ),
        Some(false) => (letter.to_string(), ink::BAD(), format!("{what}: off. Click: on")),
        Some(true) => (
            letter.to_string(),
            ink::OK(),
            format!("{what}: on. Click: as it comes in"),
        ),
    };
    let r = ui.add(
        egui::Button::new(egui::RichText::new(text).monospace().strong().color(colour))
            .min_size(egui::vec2(18.0, 16.0))
            .small(),
    );
    r.on_hover_text(tip)
}

/// The next state of a cell clicked: off, on, as it comes in.
fn next(v: Option<bool>) -> Option<bool> {
    match v {
        None => Some(false),
        Some(false) => Some(true),
        Some(true) => None,
    }
}

/// A row's or a column's text, short: its last name, or `%name`.
fn short(text: &str) -> String {
    let t = text.trim();
    if t.starts_with('%') {
        return t.to_string();
    }
    t.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or(t)
        .to_string()
}

/// Every cell of a light row, or of a prim column, set to one state.
fn set_all(
    links: &mut Vec<Link>,
    cells: &[(String, String)],
    lights: Option<Option<bool>>,
    shadows: Option<Option<bool>>,
) {
    for (l, p) in cells {
        let c = cell_mut(links, l, p);
        if let Some(v) = lights {
            c.lights = v;
        }
        if let Some(v) = shadows {
            c.shadows = v;
        }
    }
}

/// The menu on a row's or a column's name: all its cells at once.
fn all_menu(ui: &mut egui::Ui, links: &mut Vec<Link>, cells: &[(String, String)]) {
    for (label, l, s) in [
        ("Light and Shadow on", Some(Some(true)), Some(Some(true))),
        ("Light and Shadow off", Some(Some(false)), Some(Some(false))),
        ("Light off", Some(Some(false)), None),
        ("Shadow off", None, Some(Some(false))),
        ("As they come in", Some(None), Some(None)),
    ] {
        if ui.button(label).clicked() {
            set_all(links, cells, l, s);
            ui.close();
        }
    }
}

fn grid(ui: &mut egui::Ui, lights: &[String], prims: &[String], links: &mut Vec<Link>) {
    let lights: Vec<&String> = lights.iter().filter(|l| !l.trim().is_empty()).collect();
    let prims: Vec<&String> = prims.iter().filter(|p| !p.trim().is_empty()).collect();
    group(
        ui,
        "Links",
        Some("L: lights it, S: casts its shadows on it, each on, off or as it comes in (·). The nearest prim set above another decides: /set off and /set/hero on lights the hero alone. Right-click a light or a prim for all of its cells"),
        |f| {
            if lights.is_empty() || prims.is_empty() {
                f.status(Status::Info, "Add lights and prims above: a cell for each light and prim");
                return;
            }
            let set = links.iter().filter(|l| !l.is_empty()).count();
            f.status(
                Status::Info,
                format!("{} lights by {} prims, {set} cells set", lights.len(), prims.len()),
            );
            let ui = &mut *f.ui;
            egui::ScrollArea::horizontal().id_salt("light_link_grid").show(ui, |ui| {
                egui::Grid::new("light_link_cells").spacing([6.0, 2.0]).striped(true).show(ui, |ui| {
                    ui.label("");
                    for p in &prims {
                        let r = ui.add(egui::Label::new(egui::RichText::new(short(p)).small()).sense(egui::Sense::click()));
                        let cells: Vec<(String, String)> = lights.iter().map(|l| (l.to_string(), p.to_string())).collect();
                        r.on_hover_text(p.as_str()).context_menu(|ui| all_menu(ui, links, &cells));
                    }
                    ui.end_row();
                    for l in &lights {
                        let r = ui.add(egui::Label::new(egui::RichText::new(short(l)).small()).sense(egui::Sense::click()));
                        let cells: Vec<(String, String)> = prims.iter().map(|p| (l.to_string(), p.to_string())).collect();
                        r.on_hover_text(l.as_str()).context_menu(|ui| all_menu(ui, links, &cells));
                        for p in &prims {
                            let now = cell(links, l, p).cloned().unwrap_or_default();
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 1.0;
                                if mark(ui, "L", now.lights, "Light").clicked() {
                                    cell_mut(links, l, p).lights = next(now.lights);
                                }
                                if mark(ui, "S", now.shadows, "Shadow").clicked() {
                                    cell_mut(links, l, p).shadows = next(now.shadows);
                                }
                            });
                        }
                        ui.end_row();
                    }
                });
            });
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_follow_their_row_and_go_with_it() {
        let mut links = vec![
            Link {
                light: "/lights/key".into(),
                prim: "/set".into(),
                lights: Some(false),
                shadows: None,
            },
            Link {
                light: "/lights/rim".into(),
                prim: "/set".into(),
                lights: Some(true),
                shadows: None,
            },
        ];
        let was = vec!["/lights/key".to_string(), "/lights/rim".to_string()];
        let now = vec!["/lights/key1".to_string(), "/lights/rim".to_string()];
        keep_cells(&mut links, &was, &now, true);
        assert_eq!(links[0].light, "/lights/key1");
        keep_cells(&mut links, &now, &now[1..], true);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].light, "/lights/rim");
    }

    #[test]
    fn a_click_goes_off_on_and_back() {
        assert_eq!(next(None), Some(false));
        assert_eq!(next(Some(false)), Some(true));
        assert_eq!(next(Some(true)), None);
        assert_eq!(short("/set/props/lamp"), "lamp");
        assert_eq!(short("%keys"), "%keys");
    }
}
