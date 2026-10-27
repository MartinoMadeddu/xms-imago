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
//!   A reference reads the number at the end of its chain (`root_of`): each
//!   number is looked up once, however many references an operator has.
//! - **Refused:** a number following itself, one that is keyed (and keying one
//!   that follows: take the reference away first), one that is not a number,
//!   and a loop (a number following one that follows it, on the same operator
//!   or another).
//! - **Editing** a number that follows, in the Parameters pane or with a
//!   handle, changes the number it follows (at the end of its chain), so both
//!   change and the reference stays (`keying::end_edit`). Remove Reference
//!   takes it away.

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

impl NodeGraphState {
    /// An operator's parameters as it cooks at `frame`: its keyed numbers from
    /// their curves, its referenced ones from their sources.
    pub fn shown_at<'a>(&self, node: &'a GraphNode, frame: f64) -> Cow<'a, NodeType> {
        let keyed = animated_at(node, frame);
        if node.links.is_empty() {
            return keyed;
        }
        let Ok(mut v) = serde_json::to_value(&*keyed) else {
            return keyed;
        };
        let mut any = false;
        for l in &node.links {
            if let Some(x) = self.followed(l) {
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

    /// The number a reference reads: the one at the end of its chain, as its
    /// operator cooks. None when an operator on the way is gone, the chain
    /// loops, or there is no number there.
    fn followed(&self, l: &Link) -> Option<f64> {
        let (id, path) = self.root_of(l.node, &l.from)?;
        let src = self.nodes.iter().find(|n| n.id == id)?;
        keys::get(
            &serde_json::to_value(&*animated_at(src, self.node_frame(src))).ok()?,
            &path,
        )
    }

    /// The numbers number `path` of operator `id` follows, one after the
    /// other, itself first; stopping where a number is its own, or where the
    /// chain would come back on itself.
    pub fn chain(&self, id: NodeId, path: &str) -> Vec<(NodeId, String)> {
        let mut cells = vec![(id, path.to_string())];
        while let Some(l) = cells.last().and_then(|(id, p)| self.link_of(*id, p)) {
            let next = (l.node, l.from.clone());
            if cells.contains(&next) {
                break;
            }
            cells.push(next);
        }
        cells
    }

    /// Where number `path` of operator `id` takes its value from: the end of
    /// its chain of references, itself when it follows none. None when the
    /// chain loops or reaches an operator that is gone.
    pub fn root_of(&self, id: NodeId, path: &str) -> Option<(NodeId, String)> {
        let cells = self.chain(id, path);
        let (end, p) = cells.last()?.clone();
        let loops = self.link_of(end, &p).is_some();
        let there = self.nodes.iter().any(|n| n.id == end);
        (!loops && there).then_some((end, p))
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
        if self.chain(node, from).contains(&(to, path.to_string())) {
            return Err(format!(
                "That would make a loop: {source} {} follows {target} {} already",
                keys::label(from),
                keys::label(path)
            ));
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
            .link(a, "translation/0", b, "translation/1")
            .unwrap_err()
            .contains("loop"));
        // Another number of the operator it follows is not a loop.
        assert!(g.link(a, "translation/2", b, "translation/0").is_ok());
        // Another number of the same operator is fine.
        assert!(g.link(a, "translation/2", a, "translation/0").is_ok());
        g.set_key_value(b, "scale/0", 1.0, 2.0);
        assert!(g.link(b, "scale/0", a, "scale/0").unwrap_err().contains("keyed"));
    }

    /// In the Parameters pane a number that follows shows its source's value;
    /// left alone, or changed, it keeps following: changed, its source takes
    /// the change, so both change. Through a chain, the number at its end does.
    #[test]
    fn editing_either_end_of_a_reference_changes_both() {
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
        // Typed over: the source takes it, the reference stays.
        g.edit_animated(b, |t| {
            if let NodeType::Transform { translation, .. } = t {
                translation.y = 9.0;
            }
        });
        assert!(g.link_of(b, "translation/1").is_some());
        assert_eq!((y_of(&g, a), y_of(&g, b)), (9.0, 9.0));
        // The source changed: the number following it too.
        g.edit_animated(a, |t| {
            if let NodeType::Transform { translation, .. } = t {
                translation.y = 2.0;
            }
        });
        assert_eq!((y_of(&g, a), y_of(&g, b)), (2.0, 2.0));
        // A keyed source is keyed.
        g.set_key_value(a, "translation/1", 1.0, 2.0);
        g.edit_animated(b, |t| {
            if let NodeType::Transform { translation, .. } = t {
                translation.y = 6.0;
            }
        });
        assert_eq!(g.nodes[0].channels[0].keys[0].value, 6.0);
        assert_eq!(y_of(&g, b), 6.0);
        // Keying all leaves a number that follows alone.
        g.link(b, "translation/0", a, "translation/0").unwrap();
        g.key_all(b);
        assert!(!g.nodes[1].channels.iter().any(|c| c.path == "translation/0"));
    }

    /// Two numbers of one Transform, one following the other: changing either
    /// changes both. More references on it are each looked up once, so many
    /// of them on one operator are quick.
    #[test]
    fn numbers_of_one_operator_follow_each_other() {
        let mut g = NodeGraphState::default();
        let a = g.add_node("A".into(), transform(0.0), egui::pos2(0.0, 0.0));
        g.link(a, "scale/2", a, "translation/0").unwrap();
        let get = |g: &NodeGraphState| match &*g.animated_type(&g.nodes[0]) {
            NodeType::Transform { translation, scale, .. } => (translation.x, scale.z),
            _ => (f32::NAN, f32::NAN),
        };
        assert_eq!(get(&g), (0.0, 0.0));
        g.edit_animated(a, |t| {
            if let NodeType::Transform { scale, .. } = t {
                scale.z = 3.0;
            }
        });
        assert_eq!(get(&g), (3.0, 3.0));
        g.edit_animated(a, |t| {
            if let NodeType::Transform { translation, .. } = t {
                translation.x = -1.0;
            }
        });
        assert_eq!(get(&g), (-1.0, -1.0));
        assert!(g.link(a, "translation/0", a, "scale/2").unwrap_err().contains("loop"));
        // Every other number following translate X, each through the one
        // before: looked up once each.
        let paths = [
            "translation/1",
            "translation/2",
            "rotation/0",
            "rotation/1",
            "rotation/2",
            "scale/0",
            "scale/1",
        ];
        let mut before = "scale/2";
        for p in paths {
            g.link(a, p, a, before).unwrap();
            before = p;
        }
        let start = std::time::Instant::now();
        let shown = g.animated_type(&g.nodes[0]).into_owned();
        assert!(start.elapsed() < std::time::Duration::from_millis(100));
        assert!(matches!(shown, NodeType::Transform { scale, .. } if scale.y == -1.0));
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
        assert_eq!(g.root_of(c, "translation/1"), Some((a, "translation/1".to_string())));
        assert_eq!(g.chain(a, "translation/1").len(), 1);
    }
}
