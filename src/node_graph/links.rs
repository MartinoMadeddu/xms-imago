//! References: a number of an operator's parameters following another
//! operator's (see `docs/compounds.md`, Promotion and references).
//!
//! - **Kept on the operator that follows,** beside its keys (`GraphNode::links`):
//!   a reference copies, pastes, deletes and undoes with it.
//! - **Applied where keys are:** an operator's parameters as it cooks
//!   (`shown_at`, which `animated_type` is) are its own, with its keyed numbers
//!   from their curves and its referenced ones from their sources, as those
//!   cook. The graph is not written to, so a reference adds no step to the
//!   history, and the cook key, made from those parameters, follows the
//!   source. A reference to a keyed number follows its animation.
//! - **Refused:** a number following itself, one that is keyed (and keying one
//!   that follows: take the reference away first), one that is not a number,
//!   and a loop (an operator following one that follows it).
//! - **Editing** a number that follows, in the Parameters pane, takes the
//!   reference away and keeps what was typed (`keying::end_edit`).

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use super::keying::animated_at;
use super::{GraphNode, NodeGraphState};
use crate::core::keys;
use crate::types::{NodeId, NodeType};

/// Number `path` of an operator's parameters (`translation/1`), following
/// number `from` of operator `node`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Link {
    pub path: String,
    pub node: NodeId,
    pub from: String,
}

/// How many references are followed one after the other.
const DEPTH: usize = 32;

impl NodeGraphState {
    /// An operator's parameters as it cooks at `frame`: its keyed numbers from
    /// their curves, its referenced ones from their sources.
    pub fn shown_at<'a>(&self, node: &'a GraphNode, frame: f64) -> Cow<'a, NodeType> {
        self.shown_depth(node, frame, 0)
    }

    fn shown_depth<'a>(&self, node: &'a GraphNode, frame: f64, depth: usize) -> Cow<'a, NodeType> {
        let keyed = animated_at(node, frame);
        if node.links.is_empty() || depth > DEPTH {
            return keyed;
        }
        let Ok(mut v) = serde_json::to_value(&*keyed) else {
            return keyed;
        };
        let mut any = false;
        for l in &node.links {
            if let Some(x) = self.followed(l, depth) {
                any |= keys::set(&mut v, &l.path, x);
            }
        }
        if !any {
            return keyed;
        }
        match serde_json::from_value(v) {
            Ok(t) => Cow::Owned(t),
            Err(_) => keyed,
        }
    }

    /// The number a reference reads: its source's, as the source cooks. None
    /// when the source is gone or has no number there.
    fn followed(&self, l: &Link, depth: usize) -> Option<f64> {
        let src = self.nodes.iter().find(|n| n.id == l.node)?;
        let shown = self.shown_depth(src, self.node_frame(src), depth + 1);
        keys::get(&serde_json::to_value(&*shown).ok()?, &l.from)
    }

    /// The reference number `path` of operator `id` follows, if any.
    pub fn link_of(&self, id: NodeId, path: &str) -> Option<&Link> {
        self.nodes
            .iter()
            .find(|n| n.id == id)?
            .links
            .iter()
            .find(|l| l.path == path)
    }

    /// Make number `path` of operator `to` follow number `from` of operator
    /// `node`, in place of a reference it had. Refused, saying why: a number
    /// following itself, one that is keyed, what is not a number, a loop.
    pub fn link(&mut self, to: NodeId, path: &str, node: NodeId, from: &str) -> Result<(), String> {
        let name = |id: NodeId| self.nodes.iter().find(|n| n.id == id).map(|n| n.name.clone());
        let (Some(target), Some(source)) = (name(to), name(node)) else {
            return Err("That operator is gone".into());
        };
        if to == node && path == from {
            return Err("A number cannot follow itself".into());
        }
        let number = |id: NodeId, p: &str| self.keyable_values(id).iter().any(|(k, _)| k == p);
        if !number(to, path) {
            return Err(format!("{path} is not a number of {target}"));
        }
        if !number(node, from) {
            return Err(format!("{from} is not a number of {source}"));
        }
        let keyed = self
            .nodes
            .iter()
            .find(|n| n.id == to)
            .is_some_and(|n| n.channels.iter().any(|c| c.path == path));
        if keyed {
            return Err(format!(
                "{target}'s {} is keyed: take its keys away to make it follow another",
                keys::label(path)
            ));
        }
        if to != node && self.follows(node, to) {
            return Err(format!("That would make a loop: {source} follows {target} already"));
        }
        let Some(n) = self.nodes.iter_mut().find(|n| n.id == to) else {
            return Err("That operator is gone".into());
        };
        n.links.retain(|l| l.path != path);
        n.links.push(Link {
            path: path.to_string(),
            node,
            from: from.to_string(),
        });
        self.mark_dirty();
        Ok(())
    }

    /// Take away the reference number `path` of operator `id` follows; its
    /// own value is what it had before. Whether there was one.
    pub fn unlink(&mut self, id: NodeId, path: &str) -> bool {
        let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) else {
            return false;
        };
        let had = n.links.len();
        n.links.retain(|l| l.path != path);
        let gone = n.links.len() != had;
        if gone {
            self.mark_dirty();
        }
        gone
    }

    /// Whether operator `from` follows operator `to`, through its references
    /// and theirs.
    pub fn follows(&self, from: NodeId, to: NodeId) -> bool {
        let mut stack = vec![from];
        let mut seen = vec![];
        while let Some(id) = stack.pop() {
            if id == to {
                return true;
            }
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
            if let Some(n) = self.nodes.iter().find(|n| n.id == id) {
                stack.extend(n.links.iter().map(|l| l.node));
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn transform(y: f32) -> NodeType {
        NodeType::Transform {
            translation: Vec3::new(0.0, y, 0.0),
            rotation: Vec3::ZERO,
            scale: Vec3::ONE,
        }
    }

    fn y_of(g: &NodeGraphState, id: NodeId) -> f32 {
        let node = g.nodes.iter().find(|n| n.id == id).unwrap();
        match &*g.animated_type(node) {
            NodeType::Transform { translation, .. } => translation.y,
            _ => f32::NAN,
        }
    }

    /// A number follows another as it cooks, the operator's own unchanged;
    /// taken away, it is its own again.
    #[test]
    fn a_number_follows_another() {
        let mut g = NodeGraphState::default();
        let a = g.add_node("A".into(), transform(3.0), egui::pos2(0.0, 0.0));
        let b = g.add_node("B".into(), transform(1.0), egui::pos2(0.0, 100.0));
        g.link(b, "translation/1", a, "translation/1").unwrap();
        assert_eq!(y_of(&g, b), 3.0);
        assert!(matches!(&g.nodes[1].node_type, NodeType::Transform { translation, .. } if translation.y == 1.0));
        // The source changing: it follows; and so does the cook key.
        let before = g.cook_key(b, 0);
        if let NodeType::Transform { translation, .. } = &mut g.nodes[0].node_type {
            translation.y = 5.0;
        }
        assert_eq!(y_of(&g, b), 5.0);
        assert_ne!(g.cook_key(b, 0), before);
        assert!(g.unlink(b, "translation/1"));
        assert_eq!(y_of(&g, b), 1.0);
    }

    /// Following itself, a keyed number, what is not a number and a loop are
    /// refused.
    #[test]
    fn what_cannot_follow_is_refused() {
        let mut g = NodeGraphState::default();
        let a = g.add_node("A".into(), transform(0.0), egui::pos2(0.0, 0.0));
        let b = g.add_node("B".into(), transform(0.0), egui::pos2(0.0, 100.0));
        assert!(g.link(a, "translation/1", a, "translation/1").is_err());
        assert!(g.link(a, "nothing/0", b, "translation/1").is_err());
        g.link(b, "translation/1", a, "translation/0").unwrap();
        assert!(g
            .link(a, "translation/2", b, "translation/0")
            .unwrap_err()
            .contains("loop"));
        // Another number of the same operator is fine.
        assert!(g.link(a, "translation/2", a, "translation/0").is_ok());
        g.set_key_value(b, "scale/0", 1.0, 2.0);
        assert!(g.link(b, "scale/0", a, "scale/0").unwrap_err().contains("keyed"));
    }

    /// In the Parameters pane a number that follows shows its source's value;
    /// left alone it keeps following, typed over it is its own.
    #[test]
    fn typing_over_a_reference_takes_it_away() {
        let mut g = NodeGraphState::default();
        let a = g.add_node("A".into(), transform(4.0), egui::pos2(0.0, 0.0));
        let b = g.add_node("B".into(), transform(1.0), egui::pos2(0.0, 100.0));
        g.link(b, "translation/1", a, "translation/1").unwrap();
        let own = g.begin_edit(b).unwrap();
        assert!(matches!(&g.nodes[1].node_type, NodeType::Transform { translation, .. } if translation.y == 4.0));
        g.end_edit(b, own);
        assert!(
            g.link_of(b, "translation/1").is_some(),
            "nothing typed: it still follows"
        );
        assert!(matches!(&g.nodes[1].node_type, NodeType::Transform { translation, .. } if translation.y == 1.0));
        let own = g.begin_edit(b).unwrap();
        if let NodeType::Transform { translation, .. } = &mut g.nodes[1].node_type {
            translation.y = 9.0;
        }
        g.end_edit(b, own);
        assert!(g.link_of(b, "translation/1").is_none());
        assert_eq!(y_of(&g, b), 9.0);
        // Keying all leaves a number that follows alone.
        g.link(b, "translation/0", a, "translation/0").unwrap();
        g.key_all(b);
        assert!(!g.nodes[1].channels.iter().any(|c| c.path == "translation/0"));
    }

    /// A graph saved and opened keeps its references; pasted together, the
    /// copies follow each other, pasted alone, the copy follows the source.
    #[test]
    fn references_are_saved_and_pasted() {
        let mut g = NodeGraphState::default();
        let mut subnets = crate::ice::IceGraphStore::default();
        let a = g.add_node("A".into(), transform(2.0), egui::pos2(0.0, 0.0));
        let b = g.add_node("B".into(), transform(0.0), egui::pos2(0.0, 100.0));
        g.link(b, "translation/1", a, "translation/1").unwrap();
        let text = crate::graph_io::to_json(&g, &subnets);
        let mut back = NodeGraphState::default();
        crate::graph_io::from_json(&mut back, &mut subnets, &text).unwrap();
        assert_eq!(y_of(&back, b), 2.0);

        let both = crate::graph_io::copy(&g, &subnets, &[a, b]).unwrap();
        let placed = crate::graph_io::paste(&mut g, &mut subnets, both, None);
        let (a2, b2) = (placed[&a.0], placed[&b.0]);
        assert_eq!(g.link_of(b2, "translation/1").map(|l| l.node), Some(a2));
        let alone = crate::graph_io::copy(&g, &subnets, &[b]).unwrap();
        let placed = crate::graph_io::paste(&mut g, &mut subnets, alone, None);
        assert_eq!(g.link_of(placed[&b.0], "translation/1").map(|l| l.node), Some(a));
    }

    /// A chain of references: each follows the one before.
    #[test]
    fn references_follow_through_a_chain() {
        let mut g = NodeGraphState::default();
        let a = g.add_node("A".into(), transform(7.0), egui::pos2(0.0, 0.0));
        let b = g.add_node("B".into(), transform(0.0), egui::pos2(0.0, 100.0));
        let c = g.add_node("C".into(), transform(0.0), egui::pos2(0.0, 200.0));
        g.link(b, "translation/1", a, "translation/1").unwrap();
        g.link(c, "translation/1", b, "translation/1").unwrap();
        assert_eq!(y_of(&g, c), 7.0);
        assert!(g.follows(c, a) && !g.follows(a, c));
    }
}
