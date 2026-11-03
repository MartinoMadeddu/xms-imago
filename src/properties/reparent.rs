//! Reparent's parameters: the Parent, what moves, Keep World Position, and
//! what that does to the scene coming in (`core::reparent::plan`).

use crate::core::reparent::{plan, Plan, ReparentMode};
use crate::node_graph::NodeGraphState;
use crate::types::{EvalResult, NodeId, NodeType};

use super::form::{group, Status};

/// What reaches the selected Reparent: what it would move, and the scene's
/// paths to pick a Parent from.
#[derive(Default)]
pub struct ReparentIn {
    pub plan: Plan,
    pub paths: Vec<String>,
    /// Whether anything comes in.
    pub wired: bool,
}

pub fn reparent_in(graph: &NodeGraphState, id: NodeId) -> ReparentIn {
    let Some(n) = graph.nodes.iter().find(|n| n.id == id) else {
        return ReparentIn::default();
    };
    let NodeType::Reparent { parent, mode, .. } = &n.node_type else {
        return ReparentIn::default();
    };
    let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
    let scene = n
        .inputs
        .first()
        .and_then(|i| i.connected_output)
        .and_then(|(src, out)| graph.eval_node_out(src, out, &mut Default::default(), &pass));
    let Some(EvalResult::Named(prims)) = scene else {
        return ReparentIn::default();
    };
    ReparentIn {
        plan: plan(&prims, &n.locations, parent, *mode),
        paths: crate::node_graph::scene_paths(&prims),
        wired: true,
    }
}

pub fn rows(
    ui: &mut egui::Ui,
    parent: &mut String,
    mode: &mut ReparentMode,
    keep_world: &mut bool,
    found: &ReparentIn,
    located: bool,
) {
    group(
        ui,
        "Reparent",
        Some("Prims moved under another, with what is below them. What comes after sees their new paths; Write USD writes a file's prim as a reference to it at its new path, deactivated at the old one"),
        |f| {
            super::render::path_pick(
                f,
                "Primitive Location",
                "The Parent: a prim of the scene (Pick), or a new path, made as an Xform",
                parent,
                "/world",
                &found.paths,
            );
            f.choice(
                "Move",
                Some("Primitives: those the Prims Location tab names, each with its children. Whole scene: every root prim"),
                mode,
                &[(ReparentMode::Primitives, "Primitives"), (ReparentMode::WholeScene, "Whole scene")],
            );
            f.toggle(
                "Keep World Position",
                Some("On: what moves stays where it was, its own transform making up for the Parent's. Off: it keeps its own transform and follows the Parent"),
                keep_world,
            );
            let p = &found.plan;
            if p.parent.is_empty() {
                f.status(Status::Info, "Name the Parent");
            } else if *mode == ReparentMode::Primitives && !located {
                f.status(Status::Info, "Name the prims to move in the Prims Location tab");
            } else if !found.wired {
                f.status(Status::Info, "No scene coming in");
            } else if p.moves.is_empty() && p.skipped.is_empty() {
                f.status(Status::Bad, "Nothing to move: no prim named, or already under the Parent");
            } else if !p.moves.is_empty() {
                f.status(
                    Status::Ok,
                    format!(
                        "{} prim{} moved under {}",
                        p.moves.len(),
                        if p.moves.len() == 1 { "" } else { "s" },
                        p.parent
                    ),
                );
                if !found.paths.contains(&p.parent) {
                    f.status(Status::Info, format!("{} is new: made as an Xform", p.parent));
                }
            }
            for (o, n) in p.renamed() {
                f.status(
                    Status::Info,
                    format!("{o} is {}: the Parent has a {}", n.rsplit('/').next().unwrap_or(""), o.rsplit('/').next().unwrap_or("")),
                );
            }
            for s in &p.skipped {
                f.status(Status::Bad, format!("{s}: not moved"));
            }
        },
    );
}
