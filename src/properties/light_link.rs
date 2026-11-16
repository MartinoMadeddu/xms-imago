//! A Light Linker's parameters (`core::light_link`): a table, a row per link,
//! its lights and prims, then Light and Shadow as columns of toggles.

use crate::core::collections::{self, Held};
use crate::core::light_link::Link;
use crate::core::locations;

use super::form::{group, ink, Status};

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

pub fn rows(ui: &mut egui::Ui, links: &mut Vec<Link>, scene: &Scene) {
    group(ui, "Links", Some(HINT), |f| {
        table(&mut *f.ui, links, scene, None, 150.0);
        status(f, links);
    });
}

/// What the table's columns are, for its hint.
pub const HINT: &str = "A row per link: the lights it is for (a path takes the lights at and below it; a pattern, or %collection) and the prims (a prim with what is below it), then whether those lights light them (Light) and cast shadows on them (Shadow). Lights light and shadow everything until linked. Later rows win; the nearest prim set above another decides: /set off then /set/hero on lights the hero alone";

/// How many links say something.
fn status(f: &mut super::form::Rows, links: &[Link]) {
    let set = links.iter().filter(|l| l.says_something()).count();
    match (links.len(), set) {
        (0, _) => f.status(
            Status::Info,
            "No links: every light lights and shadows everything it did",
        ),
        (n, s) if s < n => f.status(
            Status::Info,
            format!("{s} of {n} links say something: each needs its lights and prims"),
        ),
        (n, _) => f.status(Status::Info, format!("{n} links")),
    }
}

/// Whether a row is shown for a light chosen (`focus`): its lights take it,
/// or it has none yet.
pub fn shows(l: &Link, focus: Option<&str>, held: &[Held]) -> bool {
    let Some(light) = focus else { return true };
    if l.light.trim().is_empty() {
        return true;
    }
    let read = collections::resolve(std::slice::from_ref(&l.light), held).into_owned();
    locations::matches(&read[0], light)
}

/// The table: a row per link (those of the light `focus` when one is
/// chosen), its lights and prims (fields `field_w` wide), Light and Shadow;
/// then + Add Link, new rows for `focus` when one is chosen.
pub fn table(ui: &mut egui::Ui, links: &mut Vec<Link>, scene: &Scene, focus: Option<&str>, field_w: f32) {
    let mut remove = None;
    if !links.is_empty() {
        egui::Grid::new("light_link_table")
            .num_columns(6)
            .spacing([8.0, 3.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label("");
                ui.label(egui::RichText::new("Lights").strong());
                ui.label(egui::RichText::new("Prims").strong());
                column_header(ui, "Light", links, lights_of);
                column_header(ui, "Shadow", links, shadows_of);
                ui.label("");
                ui.end_row();
                for (k, l) in links.iter_mut().enumerate() {
                    if !shows(l, focus, scene.held) {
                        continue;
                    }
                    ui.label(egui::RichText::new(format!("{}", k + 1)).small().color(ink::DIM()));
                    path_cell(ui, &mut l.light, scene.lights, scene.held, "/lights/key", field_w);
                    path_cell(ui, &mut l.prim, scene.paths, scene.held, "/set/hero", field_w);
                    toggle(ui, &mut l.lights, "Light", "these lights light these prims");
                    toggle(ui, &mut l.shadows, "Shadow", "these lights cast shadows on these prims");
                    if ui.small_button("🗑").on_hover_text("Remove this link").clicked() {
                        remove = Some(k);
                    }
                    ui.end_row();
                }
            });
    }
    if let Some(k) = remove {
        links.remove(k);
    }
    ui.horizontal(|ui| {
        let tip = "Add a link. Lights and prims selected in the Scene Explorer go in it: a row for each prim, for the lights selected (or the light chosen, or the last row's). Prims dragged from the Scene Explorer can be dropped here";
        let add = ui.button("+ Add Link").on_hover_text(tip);
        if add.clicked() {
            add_links(links, scene.selected, scene.lights, focus);
        }
        if let Some(p) = add.dnd_release_payload::<crate::types::DraggedPrims>() {
            add_links(links, &p.0, scene.lights, focus);
        }
    });
}

/// Rows for what is selected (or dropped): each prim with the lights among
/// them, else `light` or the last row's lights; an empty row when nothing is.
pub fn add_links(links: &mut Vec<Link>, paths: &[String], scene_lights: &[String], light: Option<&str>) {
    let (lights, prims): (Vec<&String>, Vec<&String>) = paths.iter().partition(|p| scene_lights.contains(p));
    let last = match light {
        Some(l) => l.to_string(),
        None => links.last().map(|l| l.light.clone()).unwrap_or_default(),
    };
    let lights: Vec<String> = if lights.is_empty() {
        vec![last]
    } else {
        lights.into_iter().cloned().collect()
    };
    if prims.is_empty() {
        for l in &lights {
            links.push(Link::new(l, ""));
        }
        return;
    }
    for l in &lights {
        for p in &prims {
            if links.iter().any(|x| x.light == *l && x.prim == **p) {
                continue;
            }
            // An empty row (the one a new operator has) is filled first.
            match links
                .iter_mut()
                .find(|x| x.prim.trim().is_empty() && (x.light.trim().is_empty() || x.light == *l))
            {
                Some(x) => {
                    x.light = l.clone();
                    x.prim = p.to_string();
                }
                None => links.push(Link::new(l, p)),
            }
        }
    }
}

fn lights_of(l: &mut Link) -> &mut Option<bool> {
    &mut l.lights
}

fn shadows_of(l: &mut Link) -> &mut Option<bool> {
    &mut l.shadows
}

/// A column's name: right-click sets the whole column.
fn column_header(ui: &mut egui::Ui, name: &str, links: &mut [Link], side: fn(&mut Link) -> &mut Option<bool>) {
    let r = ui
        .add(egui::Label::new(egui::RichText::new(name).strong()).sense(egui::Sense::click()))
        .on_hover_text("Right-click: the whole column");
    r.context_menu(|ui| {
        for (label, v) in [
            ("All on", Some(true)),
            ("All off", Some(false)),
            ("All as they come in", None),
        ] {
            if ui.button(label).clicked() {
                for l in links.iter_mut() {
                    *side(l) = v;
                }
                ui.close();
            }
        }
    });
}

/// A path, a pattern or `%collection`, typed or picked from `options` (and
/// the collections), with how many of them it takes. A prim dropped on it
/// is put in.
fn path_cell(ui: &mut egui::Ui, text: &mut String, options: &[String], held: &[Held], hint: &str, width: f32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let field = crate::properties::form::typed(ui, &mut *text, |ui, t| {
            use crate::properties::form::FieldExt;
            ui.field(
                egui::TextEdit::singleline(t)
                    .desired_width(width)
                    .hint_text(hint)
                    .font(egui::TextStyle::Monospace)
                    .background_color(super::ui::location_bg()),
            )
        });
        if let Some(p) = field.dnd_release_payload::<crate::types::DraggedPrims>() {
            if let Some(first) = p.0.first() {
                *text = first.clone();
            }
        }
        crate::menus::menu(ui, "⏷", |ui| {
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                let mut names: Vec<String> = vec![];
                for h in held {
                    let n = format!("%{}", h.collection.name);
                    if !names.contains(&n) {
                        names.push(n);
                    }
                }
                for p in names.iter().chain(options) {
                    if ui.selectable_label(text.trim() == p, p).clicked() {
                        *text = p.clone();
                        ui.close();
                    }
                }
            });
        })
        .response
        .on_hover_text("Pick one coming in, or a collection");
        if !text.trim().is_empty() && !options.is_empty() {
            let read = collections::resolve(std::slice::from_ref(text), held).into_owned();
            let n = options.iter().filter(|p| locations::matches(&read[0], p)).count();
            let label = egui::RichText::new(format!("{n}")).monospace().small();
            if n == 0 {
                ui.colored_label(ink::BAD(), label)
                    .on_hover_text("Takes none coming in");
            } else {
                ui.label(label).on_hover_text(format!("Takes {n} coming in"));
            }
        }
    });
}

/// A Light or Shadow cell: a toggle, on or off; right-click leaves it as it
/// comes in (—), and a click on that turns it on.
fn toggle(ui: &mut egui::Ui, v: &mut Option<bool>, name: &str, what: &str) {
    let r = if let Some(on) = v.as_mut() {
        let r = crate::properties::form::check(ui, on);
        let state = if *on { "on" } else { "off" };
        r.on_hover_text(format!("{name} {state}: {what}. Right-click: as it comes in"))
    } else {
        let r = ui.add(egui::Button::new(egui::RichText::new("—").color(ink::DIM())).small());
        let r = r.on_hover_text(format!("{name} as it comes in. Click: on"));
        if r.clicked() {
            *v = Some(true);
        }
        r
    };
    r.context_menu(|ui| {
        if ui.button("As it comes in").clicked() {
            *v = None;
            ui.close();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_lights_and_prims_make_a_row_each() {
        let lights = vec!["/lights/key".to_string(), "/lights/rim".to_string()];
        let mut links = vec![Link::new("", "")];
        let sel = vec!["/lights/key".to_string(), "/set/a".to_string(), "/set/b".to_string()];
        add_links(&mut links, &sel, &lights, None);
        assert_eq!(links.len(), 2, "the empty row filled, one more");
        assert!(links.iter().all(|l| l.light == "/lights/key" && l.lights == Some(true)));
        // No light selected: the last row's.
        add_links(&mut links, &["/set/c".to_string()], &lights, None);
        assert_eq!(links[2].light, "/lights/key");
        assert_eq!(links[2].prim, "/set/c");
        // Nothing selected: an empty row for the last row's lights.
        add_links(&mut links, &[], &lights, None);
        assert_eq!(links[3].prim, "");
        assert!(!links[3].says_something());
        // A light chosen: the new rows are its.
        add_links(&mut links, &["/set/d".to_string()], &lights, Some("/lights/rim"));
        assert_eq!(links[4].light, "/lights/rim");
    }
}
