//! The Light Linker pane: a Light Linker operator's table, big, with the
//! lights coming into it beside it.
//!
//! - It edits the Light Linker selected, else the last one it edited, else the
//!   nearest above the viewed operator, else the first of the graph
//!   (`NodeGraphState::light_linker`). The menu at its top picks another;
//!   **+ New** adds one after the viewed operator.
//! - **Lights**: the lights coming in, each with how many rows are for it.
//!   Clicking one shows its rows alone, and new rows are for it; **All
//!   lights** shows every row. Prims dropped on a light get a row each for it.
//! - The table is the operator's (`properties::light_link::table`).

use crate::core::light_link::Link;
use crate::node_graph::NodeGraphState;
use crate::properties::light_link::{add_links, shows, table, Scene, HINT};
use crate::types::{NodeId, NodeType};

/// What the pane keeps from one frame to the next.
#[derive(Clone, Default)]
struct Kept {
    /// The Light Linker it last edited.
    linker: Option<usize>,
    /// The light chosen in its list; none: every row.
    focus: Option<String>,
}

/// A path's last name.
fn short(path: &str) -> &str {
    path.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or(path)
}

pub fn draw(ui: &mut egui::Ui, graph: &mut NodeGraphState, selected: &[String]) {
    let kept_id = egui::Id::new("light_linker_pane");
    let mut kept: Kept = ui.data(|d| d.get_temp(kept_id)).unwrap_or_default();
    let linker = graph.light_linker(kept.linker.map(NodeId));

    // Which operator: its name, a menu of the others, + New.
    let linkers: Vec<(NodeId, String)> = graph
        .nodes
        .iter()
        .filter(|n| matches!(n.node_type, NodeType::LightLinker { .. }))
        .map(|n| (n.id, n.name.clone()))
        .collect();
    let mut pick: Option<NodeId> = None;
    let mut add = false;
    ui.horizontal(|ui| {
        let name = linker
            .and_then(|id| linkers.iter().find(|(i, _)| *i == id))
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| "No Light Linker".into());
        crate::menus::menu(ui, format!("🔦 {name}"), |ui| {
            for (id, n) in &linkers {
                if ui.selectable_label(Some(*id) == linker, n.as_str()).clicked() {
                    pick = Some(*id);
                    ui.close();
                }
            }
            if linkers.is_empty() {
                ui.label("None in the graph");
            }
        })
        .response
        .on_hover_text("The Light Linker edited here: the one selected, else the last edited, else the nearest above the viewed operator");
        add = ui
            .button("+ New")
            .on_hover_text("A Light Linker after the viewed operator, viewed and selected")
            .clicked();
        ui.label(egui::RichText::new("?").weak()).on_hover_text(HINT);
    });
    if add {
        pick = graph.add_light_linker();
    }
    if let Some(id) = pick {
        graph.selected_node = Some(id);
        graph.selected_nodes = vec![id];
        kept.linker = Some(id.0);
        ui.data_mut(|d| d.insert_temp(kept_id, kept));
        return;
    }
    ui.separator();
    let Some(id) = linker else {
        ui.label("No Light Linker in the graph: + New adds one after the viewed operator.");
        ui.data_mut(|d| d.insert_temp(kept_id, kept));
        return;
    };
    if kept.linker != Some(id.0) {
        kept.focus = None;
    }
    kept.linker = Some(id.0);

    // What comes into it.
    let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
    let input = graph
        .nodes
        .iter()
        .find(|n| n.id == id)
        .and_then(|n| n.inputs.first())
        .and_then(|i| i.connected_output);
    let (paths, lights, held) = match input {
        Some((src, out)) => (
            graph.eval_scene_paths(src, out, &pass).unwrap_or_default(),
            graph.eval_light_paths(src, out, &pass),
            graph.eval_collections(src, out, &pass),
        ),
        None => Default::default(),
    };
    if kept.focus.as_ref().is_some_and(|f| !lights.contains(f)) {
        kept.focus = None;
    }
    let Some(links) = graph
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .and_then(|n| match &mut n.node_type {
            NodeType::LightLinker { links } => Some(links),
            _ => None,
        })
    else {
        return;
    };
    let scene = Scene {
        paths: &paths,
        lights: &lights,
        selected,
        held: &held,
    };

    // The lights at the left, the table beside them.
    let all = ui.available_rect_before_wrap();
    let side = egui::Rect::from_min_size(all.min, egui::vec2(220.0_f32.min(all.width() * 0.35), all.height()));
    let body = egui::Rect::from_min_max(egui::pos2(side.max.x + 10.0, all.min.y), all.max);
    let mut left = ui.new_child(egui::UiBuilder::new().max_rect(side).layout(*ui.layout()));
    egui::ScrollArea::vertical()
        .id_salt("light_linker_lights")
        .auto_shrink([false, false])
        .show(&mut left, |ui| lights_list(ui, links, &scene, &mut kept.focus));
    let mut right = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(*ui.layout()));
    egui::ScrollArea::both()
        .id_salt("light_linker_table")
        .auto_shrink([false, false])
        .show(&mut right, |ui| {
            match &kept.focus {
                Some(f) => ui.label(egui::RichText::new(format!("Links of {f}")).strong()),
                None => ui.label(egui::RichText::new("Every link").strong()),
            };
            table(ui, links, &scene, kept.focus.as_deref());
        });
    ui.allocate_rect(all, egui::Sense::hover());
    ui.data_mut(|d| d.insert_temp(kept_id, kept));
}

/// The lights coming in: All lights, then each with how many rows are for it.
/// A click chooses one (again, all); prims dropped on one get rows for it.
fn lights_list(ui: &mut egui::Ui, links: &mut Vec<Link>, scene: &Scene, focus: &mut Option<String>) {
    ui.label(egui::RichText::new("Lights").strong());
    if ui
        .selectable_label(focus.is_none(), format!("All lights ({})", links.len()))
        .on_hover_text("Every row")
        .clicked()
    {
        *focus = None;
    }
    if scene.lights.is_empty() {
        ui.label(egui::RichText::new("No lights coming in").weak());
    }
    for light in scene.lights {
        let n = links
            .iter()
            .filter(|l| !l.light.trim().is_empty() && shows(l, Some(light.as_str()), scene.held))
            .count();
        let text = if n == 0 {
            format!("💡 {}", short(light))
        } else {
            format!("💡 {}  ({n})", short(light))
        };
        let on = focus.as_deref() == Some(light.as_str());
        let r = ui.selectable_label(on, text).on_hover_text(format!(
            "{light}: {n} rows for it. Click to show them alone; drop prims here for rows of it"
        ));
        if r.clicked() {
            *focus = if on { None } else { Some(light.clone()) };
        }
        if let Some(p) = r.dnd_release_payload::<crate::types::DraggedPrims>() {
            add_links(links, &p.0, scene.lights, Some(light.as_str()));
            *focus = Some(light.clone());
        }
    }
}
