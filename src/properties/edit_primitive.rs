//! Edit Primitive's parameters: its Type, then the form of that type with the
//! target's values (`core::edit_primitive::params`). A value changed is an
//! override, marked ✎ with ↺ beside it to give it back to the target; the
//! Overrides group lists them all.

use std::collections::BTreeMap;

use crate::core::edit_primitive::{params, plain, survey, targets, typed, EditKind, Param};
use crate::core::properties::Value;
use crate::node_graph::NodeGraphState;
use crate::types::{EvalResult, NodeId, NodeType};

use super::form::{group, Rows, Status};

/// What reaches the selected Edit Primitive: how many prims its locations
/// name, the kind of the first, how many are of another kind than its Type,
/// and the form of its Type with the values of the first of that kind.
#[derive(Default)]
pub struct EditIn {
    pub targets: usize,
    pub first: Option<EditKind>,
    pub other: usize,
    pub form: Vec<Param>,
    /// The first target of its Type, whose values the form shows.
    pub shown: Option<String>,
}

pub fn edit_in(graph: &NodeGraphState, id: NodeId) -> EditIn {
    let Some(n) = graph.nodes.iter().find(|n| n.id == id) else {
        return EditIn::default();
    };
    let NodeType::EditPrimitive { kind, .. } = &n.node_type else {
        return EditIn::default();
    };
    let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
    let scene = n
        .inputs
        .first()
        .and_then(|i| i.connected_output)
        .and_then(|(src, out)| graph.eval_node_out(src, out, &mut Default::default(), &pass));
    let Some(EvalResult::Named(prims)) = scene else {
        return EditIn::default();
    };
    let (count, first, other) = survey(&prims, &n.locations, *kind);
    let all = targets(&prims, &n.locations);
    let shown = all.iter().find(|t| kind.takes(t.kind()));
    EditIn {
        targets: count,
        first,
        other,
        form: shown.map(|t| params(*kind, t)).unwrap_or_default(),
        shown: shown.map(|t| t.path().to_string()),
    }
}

pub fn rows(
    ui: &mut egui::Ui,
    kind: &mut EditKind,
    edits: &mut BTreeMap<String, serde_json::Value>,
    channels: &mut Vec<crate::core::keys::Channel>,
    found: &EditIn,
    located: bool,
) {
    // The Type follows the targets until something is overridden.
    if let Some(first) = found.first {
        if edits.is_empty() && first != *kind {
            *kind = first;
        }
    }
    let mut back: Vec<String> = vec![];
    group(
        ui,
        "Edit",
        Some("The prims the Prims Location tab names, edited as what they are. The form shows the first one's values; what you change overrides them, on every prim named of this type"),
        |f| {
            let options: Vec<(EditKind, &str)> = EditKind::ALL.iter().map(|k| (*k, k.label())).collect();
            f.choice(
                "Type",
                Some("What the prims are edited as: set from the first prim named. Xform's values (transform, visibility, purpose, active, kind) are every prim's"),
                kind,
                &options,
            );
            if !located {
                f.status(Status::Info, "Name the prims in the Prims Location tab");
            } else if found.targets == 0 {
                f.status(Status::Bad, "No prim of the scene coming in is named");
            } else {
                if let Some(path) = &found.shown {
                    f.value("Showing", path.clone());
                }
                if found.other > 0 {
                    f.status(
                        Status::Bad,
                        format!(
                            "{} of {} named {} not {}: left as they are",
                            found.other,
                            found.targets,
                            if found.other == 1 { "is" } else { "are" },
                            kind.label()
                        ),
                    );
                }
            }
        },
    );
    // The form, by its groups, in their order.
    let mut groups: Vec<&str> = vec![];
    for p in &found.form {
        if !groups.contains(&p.group.as_str()) {
            groups.push(&p.group);
        }
    }
    for g in groups {
        group(ui, g, None, |f| {
            for p in found.form.iter().filter(|p| p.group == g) {
                if let Some(name) = row(f, p, edits) {
                    back.push(name);
                }
            }
        });
    }
    if !edits.is_empty() {
        group(
            ui,
            "Overrides",
            Some("The values this operator sets; every other one is the prim's own"),
            |f| {
                for (name, j) in edits.iter() {
                    let like = found
                        .form
                        .iter()
                        .find(|p| p.name == *name)
                        .map(|p| typed(&p.value, j).display())
                        .unwrap_or_else(|| j.to_string());
                    f.buttons(name, None, |ui| {
                        ui.label(like);
                        if ui.small_button("↺").on_hover_text("Back to the prim's own").clicked() {
                            back.push(name.clone());
                        }
                    });
                }
                f.buttons("", None, |ui| {
                    if ui
                        .button("Revert all")
                        .on_hover_text("Every value back to the prim's own")
                        .clicked()
                    {
                        back.extend(edits.keys().cloned());
                    }
                });
            },
        );
    }
    for name in back {
        edits.remove(&name);
        // Its keys go with it.
        let path = format!("edits/{name}");
        channels.retain(|c| c.path != path && !c.path.starts_with(&format!("{path}/")));
    }
}

/// One value: the override when there is one, else the target's. A change
/// makes it an override. The name, when ↺ gives it back.
fn row(f: &mut Rows, p: &Param, edits: &mut BTreeMap<String, serde_json::Value>) -> Option<String> {
    let overridden = edits.contains_key(&p.name);
    let mut v = edits
        .get(&p.name)
        .map(|j| typed(&p.value, j))
        .unwrap_or_else(|| p.value.clone());
    let before = v.clone();
    let label = if overridden {
        format!("{} ✎", p.label)
    } else {
        p.label.clone()
    };
    let mut hint = p.help.clone();
    if overridden {
        if !hint.is_empty() {
            hint.push_str("\n\n");
        }
        hint.push_str(&format!("Overridden. The prim's own: {}", p.value.display()));
    }
    let hint = (!hint.is_empty()).then_some(hint.as_str());
    let mut back = false;
    match &mut v {
        Value::Token(t) if !p.choices.is_empty() => {
            let mut options: Vec<(String, &str)> = p
                .choices
                .iter()
                .map(|c| (c.clone(), if c.is_empty() { "(none)" } else { c.as_str() }))
                .collect();
            if !p.choices.contains(t) {
                options.push((t.clone(), t.as_str()));
            }
            let mut picked = t.clone();
            f.choice(&label, hint, &mut picked, &options);
            *t = picked;
        }
        Value::Bool(b) => {
            f.toggle(&label, hint, b);
        }
        Value::Float3(xyz) if p.group == "Xform" => {
            let speed = if p.name == "rotate" { 0.5 } else { 0.05 };
            let unit = if p.name == "rotate" { "°" } else { "" };
            f.xyz(&label, hint, xyz, speed, unit);
        }
        other => {
            f.row(&label, hint, |ui| {
                ui.horizontal(|ui| {
                    super::render::value_field(ui, other, p.range);
                })
            });
        }
    }
    if overridden {
        f.buttons("", None, |ui| {
            back = ui.small_button("↺").on_hover_text("Back to the prim's own").clicked();
        });
    }
    if v != before {
        edits.insert(p.name.clone(), plain(&v));
    }
    back.then(|| p.name.clone())
}
