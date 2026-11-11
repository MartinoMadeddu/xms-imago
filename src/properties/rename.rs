//! Rename's and Duplicate's parameters, and what they would do to the scene
//! coming in (`core::rename`).

use crate::core::rename::{copies_plan, plan, Copies};
use crate::node_graph::NodeGraphState;
use crate::types::{EvalResult, NodeId, NodeType};

use super::form::{group, Status};

/// What reaches the selected Rename or Duplicate: each prim's new path (a
/// copy's, for Duplicate).
#[derive(Default)]
pub struct RenameIn {
    pub moves: Vec<(String, String)>,
    /// Whether anything comes in.
    pub wired: bool,
}

pub fn rename_in(graph: &NodeGraphState, id: NodeId) -> RenameIn {
    let Some(n) = graph.nodes.iter().find(|n| n.id == id) else {
        return RenameIn::default();
    };
    if !matches!(n.node_type, NodeType::Rename { .. } | NodeType::Duplicate { .. }) {
        return RenameIn::default();
    }
    let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
    let scene = n
        .inputs
        .first()
        .and_then(|i| i.connected_output)
        .and_then(|(src, out)| graph.eval_node_out(src, out, &mut Default::default(), &pass));
    let Some(EvalResult::Named(prims)) = scene else {
        return RenameIn::default();
    };
    let moves = match &n.node_type {
        NodeType::Rename {
            find,
            replace,
            prefix,
            suffix,
        } => plan(&prims, &n.locations, find, replace, prefix, suffix),
        NodeType::Duplicate { copies } => copies_plan(&prims, &n.locations, copies)
            .into_iter()
            .map(|(o, to, _)| (o, to))
            .collect(),
        _ => vec![],
    };
    RenameIn { moves, wired: true }
}

/// The first few of what happens, one line each.
fn listed(f: &mut super::form::Rows<'_>, moves: &[(String, String)]) {
    const SHOWN: usize = 8;
    for (o, n) in moves.iter().take(SHOWN) {
        f.status(Status::Info, format!("{o} → {}", n.rsplit('/').next().unwrap_or("")));
    }
    if moves.len() > SHOWN {
        f.status(Status::Info, format!("and {} more", moves.len() - SHOWN));
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

pub fn rename_rows(
    ui: &mut egui::Ui,
    find: &mut String,
    replace: &mut String,
    prefix: &mut String,
    suffix: &mut String,
    found: &RenameIn,
    located: bool,
) {
    group(
        ui,
        "Rename",
        Some("The prims the Prims Location tab names, renamed. What comes after sees their new paths; Write USD writes a file's prim as a reference to it at its new path, deactivated at the old one"),
        |f| {
            f.text("Find", Some("Replaced in each name wherever it is. Empty: the name as it is"), find, "");
            f.text("Replace", Some("What Find becomes"), replace, "");
            f.text("Prefix", Some("Put before the name"), prefix, "");
            f.text("Suffix", Some("Put after the name"), suffix, "");
            if !located {
                f.status(Status::Info, "Name the prims to rename in the Prims Location tab");
            } else if !found.wired {
                f.status(Status::Info, "No scene coming in");
            } else if found.moves.is_empty() {
                f.status(Status::Bad, "Nothing renamed: no prim named, or no name changes");
            } else {
                f.status(Status::Ok, format!("{} renamed", count(found.moves.len(), "prim", "prims")));
                listed(f, &found.moves);
            }
        },
    );
}

pub fn duplicate_rows(ui: &mut egui::Ui, c: &mut Copies, found: &RenameIn, located: bool) {
    group(
        ui,
        "Duplicate",
        Some("Copies of the prims the Prims Location tab names, beside them, each with what is below it. Write USD writes a file's prim copied as a reference to it"),
        |f| {
            f.drag_u32("Copies", Some("How many copies of each"), &mut c.count, 0..=10_000);
            f.text(
                "Name",
                Some("Each copy's name: {name} the prim's, {n} the copy's number. A name taken gets a number"),
                &mut c.name,
                "{name}_{n}",
            );
            f.xyz("Translate", Some("Each copy moved this far from the one before, in the prim's parent's space"), &mut c.translate, 0.1, "");
            f.xyz("Rotate", Some("Each copy turned this far from the one before (degrees, X then Y then Z)"), &mut c.rotate, 1.0, "°");
            f.xyz("Scale", Some("Each copy scaled this much from the one before"), &mut c.scale, 0.01, "");
            if !located {
                f.status(Status::Info, "Name the prims to copy in the Prims Location tab");
            } else if !found.wired {
                f.status(Status::Info, "No scene coming in");
            } else if found.moves.is_empty() {
                f.status(Status::Bad, "Nothing copied: no prim named, or no copies");
            } else {
                f.status(Status::Ok, count(found.moves.len(), "copy", "copies"));
                listed(f, &found.moves);
            }
        },
    );
}

/// What reaches the selected Delete Attribute or Rename Attribute: the
/// attributes of the primitives it works on, and what it would change (each
/// with how many primitives have it).
#[derive(Default)]
pub struct AttrsIn {
    pub names: Vec<String>,
    pub changed: Vec<(String, usize)>,
    pub wired: bool,
}

pub fn attrs_in(graph: &NodeGraphState, id: NodeId) -> AttrsIn {
    use crate::core::attribute_edit::{found, would_delete, would_rename};
    let Some(n) = graph.nodes.iter().find(|n| n.id == id) else {
        return AttrsIn::default();
    };
    if !matches!(
        n.node_type,
        NodeType::DeleteAttribute { .. } | NodeType::RenameAttribute { .. }
    ) {
        return AttrsIn::default();
    }
    let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
    let Some(scene) = n
        .inputs
        .first()
        .and_then(|i| i.connected_output)
        .and_then(|(src, out)| graph.eval_node_out(src, out, &mut Default::default(), &pass))
    else {
        return AttrsIn::default();
    };
    let scene = crate::core::locations::target(&scene, &n.locations);
    let names = found(&scene, &|g| {
        crate::core::geo::Context::ALL
            .into_iter()
            .flat_map(|c| g.attrs(c).map(|(n, _)| n.to_string()).collect::<Vec<_>>())
            .filter(|n| n != crate::core::geo::POINTS)
            .collect()
    });
    let changed = match &n.node_type {
        NodeType::DeleteAttribute { names, context } => found(&scene, &|g| would_delete(g, names, *context)),
        NodeType::RenameAttribute { from, to, context } => found(&scene, &|g| would_rename(g, from, to, *context)),
        _ => vec![],
    };
    AttrsIn {
        names: names.into_iter().map(|(n, _)| n).collect(),
        changed,
        wired: true,
    }
}

const CONTEXTS: [(Option<crate::core::geo::Context>, &str); 5] = [
    (None, "Any"),
    (Some(crate::core::geo::Context::Point), "Point"),
    (Some(crate::core::geo::Context::Face), "Face"),
    (Some(crate::core::geo::Context::Corner), "Corner"),
    (Some(crate::core::geo::Context::Object), "Object"),
];

/// The attributes there are, and what the operator changes, one line each.
fn attr_status(f: &mut super::form::Rows<'_>, found: &AttrsIn, what: &str) {
    if !found.wired {
        f.status(Status::Info, "No scene coming in");
        return;
    }
    if found.names.is_empty() {
        f.status(
            Status::Info,
            "The primitives coming in have no attributes but their points",
        );
        return;
    }
    f.value("Attributes", found.names.join(" "));
    if found.changed.is_empty() {
        f.status(Status::Bad, format!("Nothing {what}: no attribute named"));
    }
    const SHOWN: usize = 8;
    for (n, k) in found.changed.iter().take(SHOWN) {
        f.status(Status::Ok, format!("{n} ({})", count(*k, "prim", "prims")));
    }
    if found.changed.len() > SHOWN {
        f.status(Status::Info, format!("and {} more", found.changed.len() - SHOWN));
    }
}

pub fn delete_attribute_rows(
    ui: &mut egui::Ui,
    names: &mut String,
    context: &mut Option<crate::core::geo::Context>,
    found: &AttrsIn,
) {
    group(
        ui,
        "Delete Attribute",
        Some("Attributes (primvars) taken away from the geometry of the primitives the Prims Location tab names, or every one. Write USD writes each as blocked (= None). The points stay"),
        |f| {
            f.text(
                "Names",
                Some("Names, by spaces or commas: * any run of characters, ? any one, ^ before a name keeps it (* ^st)"),
                names,
                "Cd uv*",
            );
            f.choice("Context", Some("Only attributes of this context, or any"), context, &CONTEXTS);
            attr_status(f, found, "deleted");
        },
    );
}

pub fn rename_attribute_rows(
    ui: &mut egui::Ui,
    from: &mut String,
    to: &mut String,
    context: &mut Option<crate::core::geo::Context>,
    found: &AttrsIn,
) {
    group(
        ui,
        "Rename Attribute",
        Some("Attributes (primvars) of the primitives' geometry given new names. One already of a new name is replaced. Write USD writes the new name and blocks the old"),
        |f| {
            f.text(
                "From",
                Some("Names, by spaces or commas; each is renamed to the name in its place in To. A * in both keeps what it matched (uv* to st*)"),
                from,
                "Cd",
            );
            f.text("To", Some("The new names, in From's order"), to, "displayColor");
            f.choice("Context", Some("Only attributes of this context, or any"), context, &CONTEXTS);
            attr_status(f, found, "renamed");
        },
    );
}
