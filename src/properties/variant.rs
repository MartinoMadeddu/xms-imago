//! Variant's parameters: its Mode and, in Select, its choices, each a prim, one
//! of its variant sets and the variant chosen, picked from what the stages
//! coming in have (`core::variant`).

use crate::core::variant::{notes, variant_prims, Choice, VariantMode};
use crate::node_graph::NodeGraphState;
use crate::types::{EvalResult, NodeId, NodeType};

use super::form::{group, group_with, Status};

/// What reaches the selected Variant: the prims with variant sets coming in,
/// and what its choices could not do.
#[derive(Default)]
pub struct VariantIn {
    pub sets: Vec<(String, Vec<xms_scene::VariantSet>)>,
    pub notes: Vec<String>,
    /// Whether anything comes in.
    pub wired: bool,
}

pub fn variant_in(graph: &NodeGraphState, id: NodeId) -> VariantIn {
    let Some(n) = graph.nodes.iter().find(|n| n.id == id) else {
        return VariantIn::default();
    };
    if !matches!(n.node_type, NodeType::Variant { .. }) {
        return VariantIn::default();
    }
    let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
    let scene = n
        .inputs
        .first()
        .and_then(|i| i.connected_output)
        .and_then(|(src, out)| graph.eval_node_out(src, out, &mut Default::default(), &pass));
    let Some(EvalResult::Named(prims)) = scene else {
        return VariantIn::default();
    };
    VariantIn {
        sets: variant_prims(&prims),
        notes: notes(id),
        wired: true,
    }
}

pub fn rows(ui: &mut egui::Ui, mode: &mut VariantMode, choices: &mut Vec<Choice>, found: &VariantIn) {
    group(
        ui,
        "Variant",
        Some("Select: choose variants of the variant sets the stages coming in have. The stage is composed again with them; what was done above is carried over where it still fits. Write USD writes the choices"),
        |f| {
            f.choice(
                "Mode",
                Some("Select: choose variants a stage has. Create: make a variant set, its variants from the inputs"),
                mode,
                &[(VariantMode::Select, "Select"), (VariantMode::Create, "Create")],
            );
            if *mode == VariantMode::Create {
                f.status(Status::Info, "Create is not made yet: the scene passes through");
            } else if !found.wired {
                f.status(Status::Info, "Nothing comes in");
            } else if found.sets.is_empty() {
                f.status(Status::Info, "No prim coming in has variant sets");
            }
            for note in &found.notes {
                f.status(Status::Bad, note.as_str());
            }
        },
    );
    if *mode != VariantMode::Select {
        return;
    }
    let mut remove = None;
    for (k, c) in choices.iter_mut().enumerate() {
        let title = if c.prim.trim().is_empty() {
            format!("{}  (no prim)", k + 1)
        } else {
            format!("{}  {}", k + 1, c.prim.trim())
        };
        group_with(
            ui,
            &title,
            Some("Where two choices set the same variant set of a prim, the later one wins"),
            |ui| {
                if ui.small_button("🗑").on_hover_text("Remove this choice").clicked() {
                    remove = Some(k);
                }
            },
            |f| {
                let paths: Vec<String> = found.sets.iter().map(|(p, _)| p.clone()).collect();
                super::render::path_pick(
                    f,
                    "Prim",
                    "The prim with the variant set: one of the stage's (Pick lists those with variant sets)",
                    &mut c.prim,
                    "/asset",
                    &paths,
                );
                let sets = found
                    .sets
                    .iter()
                    .find(|(p, _)| *p == crate::types::prim_location(&c.prim, ""))
                    .map(|(_, s)| s.as_slice())
                    .unwrap_or_default();
                if sets.is_empty() {
                    if !c.prim.trim().is_empty() {
                        f.status(Status::Bad, "No variant sets on this prim");
                    }
                    return;
                }
                if !sets.iter().any(|s| s.name == c.set) {
                    c.set = sets[0].name.clone();
                }
                let names: Vec<(String, &str)> = sets.iter().map(|s| (s.name.clone(), s.name.as_str())).collect();
                f.dropdown(
                    "Variant Set",
                    Some("One of the prim's variant sets"),
                    &mut c.set,
                    &names,
                );
                if let Some(set) = sets.iter().find(|s| s.name == c.set) {
                    if !set.variants.contains(&c.variant) {
                        c.variant = set.selection.clone();
                    }
                    let variants: Vec<(String, &str)> = set.variants.iter().map(|v| (v.clone(), v.as_str())).collect();
                    f.dropdown(
                        "Variant",
                        Some("The variant chosen. The file's own choice is selected to start with"),
                        &mut c.variant,
                        &variants,
                    );
                    if !set.selection.is_empty() {
                        f.value("In the file", set.selection.as_str());
                    }
                }
            },
        );
    }
    if let Some(k) = remove {
        choices.remove(k);
    }
    ui.horizontal(|ui| {
        if ui
            .button("+ Choice")
            .on_hover_text("Choose a variant of another prim or variant set")
            .clicked()
        {
            choices.push(Choice::default());
        }
    });
}
