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
        table(&mut *f.ui, links, scene, None);
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

const ROW_H: f32 = 24.0;
const HEADER_H: f32 = 22.0;
const IDX_W: f32 = 30.0;
const TOGGLE_W: f32 = 62.0;
const DEL_W: f32 = 34.0;
/// What a path cell gives its ▾ and its count, beside the field.
const PATH_EXTRA_W: f32 = 66.0;

/// The table, as the Primitive Inspector's: a header, then a row per link
/// (those of the light `focus` when one is chosen) in full-width bands, its
/// lights and prims sharing the width left by Light, Shadow and remove; then
/// + Add Link, new rows for `focus` when one is chosen.
pub fn table(ui: &mut egui::Ui, links: &mut Vec<Link>, scene: &Scene, focus: Option<&str>) {
    use crate::prim_inspector::ui::xsi;
    let shown: Vec<usize> = (0..links.len())
        .filter(|k| shows(&links[*k], focus, scene.held))
        .collect();
    let fixed = IDX_W + 2.0 * TOGGLE_W + DEL_W;
    let path_w = ((ui.available_width() - fixed) / 2.0).max(140.0);
    let widths = [IDX_W, path_w, path_w, TOGGLE_W, TOGGLE_W, DEL_W];
    let width: f32 = widths.iter().sum();
    let (outer, _) = ui.allocate_exact_size(
        egui::vec2(width, HEADER_H + ROW_H * shown.len() as f32),
        egui::Sense::hover(),
    );
    // A cell: row `None` the header.
    let cell = |row: Option<usize>, c: usize| {
        let x = outer.min.x + widths[..c].iter().sum::<f32>();
        let (y, h) = match row {
            None => (outer.min.y, HEADER_H),
            Some(r) => (outer.min.y + HEADER_H + r as f32 * ROW_H, ROW_H),
        };
        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(widths[c], h))
    };
    let painter = ui.painter_at(outer);
    painter.rect_filled(
        egui::Rect::from_min_size(outer.min, egui::vec2(width, HEADER_H)),
        0.0,
        xsi::HEADER_BG(),
    );
    for r in 0..shown.len() {
        let band = egui::Rect::from_min_size(
            egui::pos2(outer.min.x, outer.min.y + HEADER_H + r as f32 * ROW_H),
            egui::vec2(width, ROW_H),
        );
        painter.rect_filled(band, 0.0, if r % 2 == 0 { xsi::ROW_EVEN() } else { xsi::ROW_ODD() });
    }
    let border = egui::Stroke::new(1.0, xsi::BORDER());
    for row in std::iter::once(None).chain((0..shown.len()).map(Some)) {
        for c in 0..widths.len() {
            painter.rect_stroke(cell(row, c), 0.0, border, egui::StrokeKind::Middle);
        }
    }
    for (c, name) in ["", "Lights", "Prims", "Light", "Shadow", ""].iter().enumerate() {
        painter.text(
            cell(None, c).center(),
            egui::Align2::CENTER_CENTER,
            *name,
            egui::FontId::proportional(11.0),
            xsi::TEXT(),
        );
    }
    let sides: [(usize, fn(&mut Link) -> &mut Option<bool>); 2] = [(3, lights_of), (4, shadows_of)];
    for (c, side) in sides {
        let r = ui
            .interact(
                cell(None, c),
                ui.id().with(("light_link_header", c)),
                egui::Sense::click(),
            )
            .on_hover_text("Right-click: the whole column");
        r.context_menu(|ui| column_menu(ui, links, &shown, side));
    }

    // The rows' widgets, each in its cell.
    let mut remove = None;
    let left = egui::Layout::left_to_right(egui::Align::Center);
    for (r, &k) in shown.iter().enumerate() {
        let inside = |ui: &mut egui::Ui, c: usize| {
            ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(cell(Some(r), c).shrink2(egui::vec2(4.0, 1.0)))
                    .id_salt(("light_link_cell", k, c))
                    .layout(left),
            )
        };
        let l = &mut links[k];
        let mut u = inside(ui, 0);
        u.label(egui::RichText::new(format!("{}", k + 1)).small().color(xsi::TEXT_IDX()));
        let field = path_w - PATH_EXTRA_W;
        path_cell(
            &mut inside(ui, 1),
            &mut l.light,
            scene.lights,
            scene.held,
            "/lights/key",
            field,
        );
        path_cell(
            &mut inside(ui, 2),
            &mut l.prim,
            scene.paths,
            scene.held,
            "/set/hero",
            field,
        );
        let mut u = inside(ui, 3);
        u.add_space((TOGGLE_W - 26.0) / 2.0);
        toggle(&mut u, &mut l.lights, "Light", "these lights light these prims");
        let mut u = inside(ui, 4);
        u.add_space((TOGGLE_W - 26.0) / 2.0);
        toggle(
            &mut u,
            &mut l.shadows,
            "Shadow",
            "these lights cast shadows on these prims",
        );
        let mut u = inside(ui, 5);
        u.add_space((DEL_W - 30.0) / 2.0);
        if u.small_button("🗑").on_hover_text("Remove this link").clicked() {
            remove = Some(k);
        }
    }
    if let Some(k) = remove {
        links.remove(k);
    }
    ui.add_space(4.0);
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

/// The menu on Light's or Shadow's name: the column, for the rows shown.
fn column_menu(ui: &mut egui::Ui, links: &mut [Link], shown: &[usize], side: fn(&mut Link) -> &mut Option<bool>) {
    for (label, v) in [
        ("All on", Some(true)),
        ("All off", Some(false)),
        ("All as they come in", None),
    ] {
        if ui.button(label).clicked() {
            for &k in shown {
                *side(&mut links[k]) = v;
            }
            ui.close();
        }
    }
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
        crate::menus::menu(ui, "▾", |ui| {
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
