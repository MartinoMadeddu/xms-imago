//! What comes down the wires: the `{name}` variables operators publish for the
//! operators below them (see `core::vars`).
//!
//! - **Published:** Write USD's `{usd}` (the layer it writes), Render Commons'
//!   `{settings}` and `{camera}`, Render Settings' `{renderer}` and
//!   `{delegate}` (and `{settings}` when none comes in), a Camera's `{camera}`,
//!   and a Set Variable's own rows.
//! - **Down the wires:** every operator below reads them, along its inputs. The
//!   nearest wins: an operator's own, then the one just above, then further up;
//!   of several inputs, the first input's first, as Merge orders them. A
//!   bypassed operator publishes nothing; what comes in goes on.
//! - **Read from parameters, never by cooking:** finding what reaches an
//!   operator walks up the wires, so a Render Command runs, and a panel lists
//!   them, without cooking the graph. A published value can hold variables: the
//!   built-ins, and what comes down to its operator.

use std::collections::HashMap;

use super::{GraphNode, NodeGraphState};
use crate::types::{NodeId, NodeType};

/// A variable coming down the wires: its name (lower case, without the braces),
/// its value, and the operator that published it.
#[derive(Clone, Debug, PartialEq)]
pub struct FromAbove {
    pub name: String,
    pub value: String,
    pub from: NodeId,
}

/// Where a variable the compound instance gives its inside is from: no
/// operator of that graph.
pub const INSTANCE: NodeId = NodeId(usize::MAX);

/// How far up the wires is walked.
const DEPTH: usize = 256;

impl NodeGraphState {
    /// What comes down the wires to operator `id`, nearest first.
    pub fn from_above(&self, id: NodeId) -> Vec<FromAbove> {
        self.above_in(id, &mut HashMap::new(), 0)
    }

    /// What leaves operator `id` for the operators below: what it publishes,
    /// then what comes down to it. What a pane looking at its output (the Image
    /// Viewer) reads.
    pub fn going_out(&self, id: NodeId) -> Vec<FromAbove> {
        self.going_down(id, &mut HashMap::new(), 0)
    }

    /// While it lives, text read by `reader` (a pane, not an operator) has
    /// `{name}` as it leaves operator `id`: `$OPNAME` is `reader`.
    pub fn reading_out_of(&self, id: Option<NodeId>, reader: &str) -> crate::core::vars::Operator {
        let above = id
            .map(|id| self.going_out(id).into_iter().map(|a| (a.name, a.value)).collect())
            .unwrap_or_default();
        crate::core::vars::operator_with(reader, above)
    }

    /// While it lives, `id`'s text is read as its own: `$OPNAME` is its name,
    /// `{name}` reads what comes down the wires to it.
    pub fn operator(&self, id: NodeId) -> crate::core::vars::Operator {
        match self.nodes.iter().find(|n| n.id == id) {
            Some(n) => self.reading(n),
            None => crate::core::vars::operator(""),
        }
    }

    /// `text` with its variables put in as operator `id`'s.
    pub fn expand_at(&self, id: NodeId, text: &str) -> String {
        let name = self
            .nodes
            .iter()
            .find(|n| n.id == id)
            .map_or(String::new(), |n| n.name.clone());
        let above = if has_braces(text) {
            self.from_above(id).into_iter().map(|a| (a.name, a.value)).collect()
        } else {
            vec![]
        };
        let _o = crate::core::vars::operator_with(&name, above);
        crate::core::vars::expand(text, &[])
    }

    /// `node`'s text read as its own; what comes down the wires is looked for
    /// only when its parameters hold a `{name}` (cooking asks often).
    pub(crate) fn reading(&self, node: &GraphNode) -> crate::core::vars::Operator {
        // Edit Poly's operations can be long and hold no text.
        let braces = !matches!(node.node_type, NodeType::EditPoly { .. })
            && serde_json::to_string(&node.node_type).is_ok_and(|t| has_braces(&t));
        let above = if braces {
            self.from_above(node.id)
                .into_iter()
                .map(|a| (a.name, a.value))
                .collect()
        } else {
            vec![]
        };
        crate::core::vars::operator_with(&node.name, above)
    }

    /// What reaches `id` along its inputs, in order: the first input's first, a
    /// name only once.
    fn above_in(&self, id: NodeId, memo: &mut HashMap<NodeId, Vec<FromAbove>>, depth: usize) -> Vec<FromAbove> {
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            return vec![];
        };
        if depth > DEPTH {
            return vec![];
        }
        let mut out: Vec<FromAbove> = vec![];
        for src in node
            .inputs
            .iter()
            .filter_map(|i| i.connected_output.map(|(src, _)| src))
        {
            for a in self.going_down(src, memo, depth + 1) {
                if !out.iter().any(|o| o.name == a.name) {
                    out.push(a);
                }
            }
        }
        // Inside a compound, what the instance gives every operator in it
        // (`{instance}`, and what comes down to the instance), after what
        // comes down the wires inside.
        if let Some(fed) = &self.fed {
            for (name, value) in &fed.above {
                if !out.iter().any(|o| o.name == *name) {
                    out.push(FromAbove {
                        name: name.clone(),
                        value: value.clone(),
                        from: INSTANCE,
                    });
                }
            }
        }
        out
    }

    /// What leaves `id` for the operators below: what it publishes, then what
    /// comes to it that it does not.
    fn going_down(&self, id: NodeId, memo: &mut HashMap<NodeId, Vec<FromAbove>>, depth: usize) -> Vec<FromAbove> {
        if let Some(known) = memo.get(&id) {
            return known.clone();
        }
        let above = self.above_in(id, memo, depth);
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            return above;
        };
        let mut out: Vec<FromAbove> = if node.bypassed { vec![] } else { publishes(node, &above) };
        for a in above {
            if !out.iter().any(|o| o.name == a.name) {
                out.push(a);
            }
        }
        memo.insert(id, out.clone());
        out
    }
}

/// What `node` publishes, its values read as its own with `above` coming down
/// to it.
fn publishes(node: &GraphNode, above: &[FromAbove]) -> Vec<FromAbove> {
    let _o = crate::core::vars::operator_with(
        &node.name,
        above.iter().map(|a| (a.name.clone(), a.value.clone())).collect(),
    );
    let put = |text: &str| crate::core::vars::expand(text.trim(), &[]);
    let mut out: Vec<(String, String)> = vec![];
    match &node.node_type {
        NodeType::WriteUsd { path } => out.push(("usd".into(), put(path.as_str()))),
        NodeType::RenderCommons { scene } => {
            out.push(("settings".into(), put(scene.location.as_str())));
            // None: the scene's first camera, so one from above still counts.
            if !scene.camera.trim().is_empty() {
                out.push(("camera".into(), put(scene.camera.as_str())));
            }
        }
        NodeType::RenderSettings { renderer } => {
            // It works on the RenderSettings prim that comes in: its own
            // location only when none does.
            if !above.iter().any(|a| a.name == "settings") {
                out.push(("settings".into(), put(renderer.location.as_str())));
            }
            let name = if renderer.renderer == "custom" {
                renderer.custom_name.trim().to_string()
            } else {
                renderer.renderer.clone()
            };
            out.push(("renderer".into(), name));
            // husk's -R: Arnold's delegate; Karma's is husk's own, so none.
            out.push((
                "delegate".into(),
                if renderer.renderer == "arnold" {
                    "HdArnoldRendererPlugin".into()
                } else {
                    String::new()
                },
            ));
        }
        NodeType::Camera { location, .. } => out.push(("camera".into(), put(location.as_str()))),
        NodeType::SetVariable { vars } => {
            // A row reads the rows before it, then what comes from above.
            for v in vars {
                let name = v.name.trim().to_lowercase();
                if name.is_empty() || out.iter().any(|(n, _)| *n == name) {
                    continue;
                }
                let earlier: Vec<(&str, String)> = out.iter().map(|(n, v)| (n.as_str(), v.clone())).collect();
                let value = crate::core::vars::expand(v.value.trim(), &earlier);
                out.push((name, value));
            }
        }
        _ => {}
    }
    out.into_iter()
        .map(|(name, value)| FromAbove {
            name,
            value,
            from: node.id,
        })
        .collect()
}

/// Whether text (a node's parameters as JSON) holds a `{name}`: in JSON a `{`
/// of its own is followed by `"` or `}`, so one before a letter, a digit or `_`
/// is in a string.
pub(crate) fn has_braces(text: &str) -> bool {
    let b = text.as_bytes();
    b.windows(2)
        .any(|w| w[0] == b'{' && (w[1].is_ascii_alphanumeric() || w[1] == b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::vars::Variable;

    fn p() -> egui::Pos2 {
        egui::pos2(0.0, 0.0)
    }

    #[test]
    fn they_come_down_the_wires_nearest_first() {
        let mut g = NodeGraphState::default();
        let cube = g.add_node("Cube".into(), NodeType::cube(1.0), p());
        let shot = g.add_node(
            "Shot".into(),
            NodeType::SetVariable {
                vars: vec![Variable::new("shot", "sh010"), Variable::new("Seq", "{shot}_a")],
            },
            p(),
        );
        let write = g.add_node(
            "Write".into(),
            NodeType::WriteUsd {
                path: "/out/{shot}.usda".into(),
            },
            p(),
        );
        let over = g.add_node(
            "Over".into(),
            NodeType::SetVariable {
                vars: vec![Variable::new("shot", "sh020")],
            },
            p(),
        );
        let run = g.add_node(
            "Run".into(),
            NodeType::RenderCommand {
                command: Default::default(),
            },
            p(),
        );
        g.add_connection(cube, 0, shot, 0);
        g.add_connection(shot, 0, write, 0);
        g.add_connection(write, 0, over, 0);
        g.add_connection(over, 0, run, 0);

        let at = |g: &NodeGraphState, id, name: &str| {
            g.from_above(id)
                .into_iter()
                .find(|a| a.name == name)
                .map(|a| (a.value, a.from))
        };
        assert_eq!(at(&g, cube, "shot"), None, "nothing above the first");
        assert_eq!(at(&g, shot, "shot"), None, "its own are for below");
        assert_eq!(
            at(&g, write, "seq"),
            Some(("sh010_a".into(), shot)),
            "names in lower case; a row reads the rows before it"
        );
        assert_eq!(
            at(&g, over, "usd"),
            Some(("/out/sh010.usda".into(), write)),
            "the writer's path reads what comes to it"
        );
        assert_eq!(at(&g, run, "shot"), Some(("sh020".into(), over)), "the nearest wins");
        assert_eq!(
            at(&g, run, "usd"),
            Some(("/out/sh010.usda".into(), write)),
            "and the rest goes on"
        );
        assert_eq!(
            g.expand_at(run, "husk \"{usd}\" # {shot} {nope}"),
            "husk \"/out/sh010.usda\" # sh020 {nope}"
        );

        // Bypassed: it publishes nothing; what comes in goes on.
        g.nodes.iter_mut().find(|n| n.id == over).unwrap().bypassed = true;
        assert_eq!(at(&g, run, "shot"), Some(("sh010".into(), shot)));
    }

    #[test]
    fn the_first_input_first_and_render_settings() {
        let mut g = NodeGraphState::default();
        let a = g.add_node(
            "A".into(),
            NodeType::SetVariable {
                vars: vec![Variable::new("look", "a")],
            },
            p(),
        );
        let b = g.add_node(
            "B".into(),
            NodeType::SetVariable {
                vars: vec![Variable::new("look", "b"), Variable::new("only_b", "1")],
            },
            p(),
        );
        let merge = g.add_node("Merge".into(), NodeType::merge(), p());
        g.add_connection(a, 0, merge, 0);
        g.add_connection(b, 0, merge, 1);
        let names: Vec<(String, String)> = g.from_above(merge).into_iter().map(|x| (x.name, x.value)).collect();
        assert_eq!(names, vec![("look".into(), "a".into()), ("only_b".into(), "1".into())]);

        let commons = g.add_node(
            "Commons".into(),
            NodeType::RenderCommons {
                scene: crate::core::render::SceneSettings {
                    location: "/Render/Shot".into(),
                    camera: "/cam".into(),
                    ..Default::default()
                },
            },
            p(),
        );
        let mut arnold = crate::core::render::RendererSettings::default();
        arnold.renderer = "arnold".into();
        let settings = g.add_node("Settings".into(), NodeType::RenderSettings { renderer: arnold }, p());
        let run = g.add_node(
            "Run".into(),
            NodeType::RenderCommand {
                command: Default::default(),
            },
            p(),
        );
        g.add_connection(commons, 0, settings, 0);
        g.add_connection(settings, 0, run, 0);
        assert_eq!(
            g.expand_at(run, "-R {delegate} -s {settings} -c {camera} {renderer}"),
            "-R HdArnoldRendererPlugin -s /Render/Shot -c /cam arnold",
            "Render Settings works on the prim that comes in: its own location only without one"
        );
    }

    #[test]
    fn braces_in_parameters_not_in_json() {
        assert!(!has_braces(r#"{"WriteUsd":{"path":"/a.usda"}}"#));
        assert!(has_braces(r#"{"WriteUsd":{"path":"/{shot}.usda"}}"#));
    }
}
