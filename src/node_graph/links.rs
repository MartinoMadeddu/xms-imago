//! References: a parameter of an operator following another operator's (see
//! `docs/compounds.md`, Promotion and references): a number, a part of a
//! vector, text, a switch, a choice, a vector or a list, each following one
//! of its own kind.
//!
//! - **Kept on the operator that follows,** beside its keys (`GraphNode::links`):
//!   a reference copies, pastes, deletes and undoes with it.
//! - **Applied where keys are:** an operator's parameters as it cooks
//!   (`shown_at`, which `animated_type` is) are its own, with its keyed numbers
//!   from their curves and its referenced values from their sources, as those
//!   cook. The graph is not written to, so a reference adds no step to the
//!   history, and the cook key, made from those parameters, follows the
//!   source. A reference to a keyed number follows its animation.
//!   A reference reads the value at the end of its chain (`root_of`).
//!   Values kept beside the parameters (a spare parameter, the Prims Location
//!   tab) are set from their source once a frame instead (`follow_kept`).
//! - **Refused:** a value following itself, a number that is keyed (and
//!   keying one that follows: take the reference away first), values of two
//!   kinds (text and a number, a 3D and a 2D vector), one the parameter cannot
//!   hold (text that is none of a choice's options), and a loop.
//! - **Editing** a value that follows, in the Parameters pane or with a
//!   handle, changes the value it follows (at the end of its chain), so both
//!   change and the reference stays (`keying::end_edit`). Remove Reference
//!   takes it away.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use super::keying::animated_at;
use super::{GraphNode, NodeGraphState};
use crate::compounds;
use crate::core::keys;
use crate::types::{NodeId, NodeType};

/// Parameter `path` of an operator's parameters (`translation/1`), following
/// parameter `from` of operator `node`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Link {
    pub path: String,
    pub node: NodeId,
    pub from: String,
}

/// What each value kept beside the parameters that follows another followed,
/// by its operator and path (`NodeGraphState::follow_kept`).
pub type Followed = std::collections::HashMap<(NodeId, String), serde_json::Value>;

/// Whether `path` is of a value kept beside an operator's parameters: a spare
/// parameter, the Prims Location tab.
pub fn kept(path: &str) -> bool {
    path.starts_with(super::spare::PREFIX) || path == compounds::LOCATIONS
}

/// Whether value `b` can stand for `a`: both numbers, text, switches, text
/// rows (a list, any length), or numbers side by side as many.
pub fn same_kind(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value as V;
    let text = |x: &[V]| x.iter().all(V::is_string);
    let numbers = |x: &[V]| x.iter().all(V::is_number);
    match (a, b) {
        (V::Number(_), V::Number(_)) | (V::String(_), V::String(_)) | (V::Bool(_), V::Bool(_)) => true,
        (V::Array(x), V::Array(y)) if text(x) && text(y) => true,
        (V::Array(x), V::Array(y)) => x.len() == y.len() && numbers(x) && numbers(y),
        _ => false,
    }
}

impl GraphNode {
    /// The value of parameter `path` of its own (not as keyed or referenced):
    /// of its parameters, a spare parameter's, its Prims Location rows.
    pub fn value_at(&self, path: &str) -> Option<serde_json::Value> {
        if path == compounds::LOCATIONS {
            return Some(serde_json::json!(self.locations));
        }
        if path.starts_with(super::spare::PREFIX) {
            return self.spare.get(path);
        }
        compounds::get(&serde_json::to_value(&self.node_type).ok()?, path).cloned()
    }

    /// Set value `path` of its own to `x` when it can hold it. Whether it did.
    pub fn put_value(&mut self, path: &str, x: serde_json::Value) -> bool {
        if path == compounds::LOCATIONS {
            let Some(rows) = x.as_array() else { return false };
            self.locations = rows.iter().filter_map(|r| r.as_str().map(str::to_string)).collect();
            return true;
        }
        if path.starts_with(super::spare::PREFIX) {
            return self.spare.put(path, x);
        }
        let Ok(mut v) = serde_json::to_value(&self.node_type) else {
            return false;
        };
        match put_checked(&mut v, path, x) {
            Some(t) => {
                self.node_type = t;
                true
            }
            None => false,
        }
    }
}

/// `x` put at `path` of parameters `v` when it is of the kind there and the
/// parameters still read as an operator's: those parameters.
fn put_checked(v: &mut serde_json::Value, path: &str, x: serde_json::Value) -> Option<NodeType> {
    let was = compounds::get(v, path)?;
    if !same_kind(was, &x) {
        return None;
    }
    let mut next = v.clone();
    if !compounds::put(&mut next, path, x) {
        return None;
    }
    let t = serde_json::from_value(next.clone()).ok()?;
    *v = next;
    Some(t)
}

impl NodeGraphState {
    /// An operator's parameters as it cooks at `frame`: its keyed numbers from
    /// their curves, its referenced values from their sources (each that it
    /// can hold).
    pub fn shown_at<'a>(&self, node: &'a GraphNode, frame: f64) -> Cow<'a, NodeType> {
        let keyed = animated_at(node, frame);
        if node.links.iter().all(|l| kept(&l.path)) {
            return keyed;
        }
        let Ok(mut v) = serde_json::to_value(&*keyed) else {
            return keyed;
        };
        let mut shown = None;
        for l in node.links.iter().filter(|l| !kept(&l.path)) {
            if let Some(t) = self.followed(l).and_then(|x| put_checked(&mut v, &l.path, x)) {
                shown = Some(t);
            }
        }
        match shown {
            Some(t) => Cow::Owned(t),
            None => keyed,
        }
    }

    /// The value a reference reads: the one at the end of its chain, as its
    /// operator cooks. None when an operator on the way is gone, the chain
    /// loops, or there is no value there.
    pub(crate) fn followed(&self, l: &Link) -> Option<serde_json::Value> {
        let (id, path) = self.root_of(l.node, &l.from)?;
        let src = self.nodes.iter().find(|n| n.id == id)?;
        if kept(&path) {
            return src.value_at(&path);
        }
        let v = serde_json::to_value(&*animated_at(src, self.node_frame(src))).ok()?;
        compounds::get(&v, &path).cloned()
    }

    /// The value of parameter `path` of operator `id` as it is shown: keyed
    /// at the playhead, referenced from its source.
    pub fn shown_value(&self, id: NodeId, path: &str) -> Option<serde_json::Value> {
        let n = self.nodes.iter().find(|n| n.id == id)?;
        if kept(path) {
            return n.value_at(path);
        }
        compounds::get(&serde_json::to_value(&*self.animated_type(n)).ok()?, path).cloned()
    }

    /// Values kept beside the parameters (a spare parameter, the Prims
    /// Location tab) that follow another take its value, once a frame; one
    /// changed by hand changes the value it follows (as any reference does)
    /// and keeps following it. `last` is what each followed last time.
    pub fn follow_kept(&mut self, last: &mut Followed) {
        let mut set: Vec<(NodeId, String, serde_json::Value)> = vec![];
        let mut through: Vec<((NodeId, String), serde_json::Value)> = vec![];
        for n in &self.nodes {
            for l in n.links.iter().filter(|l| kept(&l.path)) {
                let key = (n.id, l.path.clone());
                let now = n.value_at(&l.path);
                let Some(src) = self.followed(l) else { continue };
                let was = last.get(&key).cloned();
                match (now, was) {
                    // Changed here, not at the source: the source takes it.
                    (Some(x), Some(w)) if x != w && src == w => {
                        if let Some(root) = self.root_of(n.id, &l.path) {
                            through.push((root, x.clone()));
                        }
                        last.insert(key, x);
                    }
                    (now, _) => {
                        if now.as_ref() != Some(&src) {
                            set.push((n.id, l.path.clone(), src.clone()));
                        }
                        last.insert(key, src);
                    }
                }
            }
        }
        let any = !set.is_empty() || !through.is_empty();
        for (id, path, x) in set {
            self.set_value(id, &path, x);
        }
        for ((id, path), x) in through {
            self.set_value(id, &path, x);
        }
        if any {
            self.mark_dirty();
        }
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
            return Err("A parameter cannot follow itself".into());
        }
        let (Some(here), Some(there)) = (self.shown_value(to, path), self.shown_value(node, from)) else {
            let (whose, what) = match self.shown_value(to, path) {
                None => (&target, path),
                Some(_) => (&source, from),
            };
            return Err(format!("{whose} has no parameter {}", keys::label(what)));
        };
        if !same_kind(&here, &there) {
            return Err(format!(
                "{target} {} and {source} {} are not of one kind: a reference follows a value of its own kind",
                keys::label(path),
                keys::label(from)
            ));
        }
        // One it cannot hold (text that is none of a choice's options).
        if !kept(path) {
            let mut t = self.nodes.iter().find(|n| n.id == to).cloned();
            if !t.as_mut().is_some_and(|t| t.put_value(path, there.clone())) {
                return Err(format!(
                    "{target} {} cannot take {source} {}",
                    keys::label(path),
                    keys::label(from)
                ));
            }
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

    /// Every kind of value follows one of its kind: a vector, text, the Prims
    /// Location rows (set once a frame); two kinds are refused.
    #[test]
    fn every_kind_of_value_follows_its_own_kind() {
        let at = egui::pos2(0.0, 0.0);
        let mut g = NodeGraphState::default();
        let a = g.add_node("A".into(), transform(2.0), at);
        let b = g.add_node("B".into(), transform(0.0), at);
        g.link(b, "translation", a, "translation").unwrap();
        assert_eq!(y_of(&g, b), 2.0, "a vector follows a vector");
        assert!(g.link(b, "scale", a, "translation/1").unwrap_err().contains("kind"));
        let c = g.add_node("C".into(), NodeType::cube(1.0), at);
        let d = g.add_node("D".into(), NodeType::cube(1.0), at);
        assert!(g.nodes[2].put_value("location", serde_json::json!("/set/box")));
        assert!(
            !g.nodes[2].put_value("location", serde_json::json!(3.0)),
            "not a number in text"
        );
        g.link(d, "location", c, "location").unwrap();
        assert_eq!(g.shown_value(d, "location"), Some(serde_json::json!("/set/box")));
        assert!(g.link(d, "size", c, "location").is_err());
        g.nodes[0].locations = vec!["/a".into(), "/b".into()];
        g.link(b, "@locations", a, "@locations").unwrap();
        let mut last = Followed::new();
        g.follow_kept(&mut last);
        assert_eq!(g.nodes[1].locations, vec!["/a".to_string(), "/b".to_string()]);
    }

    /// A parameter follows a spare parameter; a spare parameter follows a
    /// parameter, and changed by hand changes it.
    #[test]
    fn spare_parameters_follow_and_are_followed() {
        use crate::compounds::ParameterType;
        let mut g = NodeGraphState::default();
        let a = g.add_node("A".into(), transform(0.0), egui::pos2(0.0, 0.0));
        let b = g.add_node("B".into(), transform(1.0), egui::pos2(0.0, 100.0));
        let spare = &mut g.nodes[0].spare;
        spare.add(ParameterType::Float, "Height", "", None, &[]).unwrap();
        spare.put("spare/height", serde_json::json!(4.0));
        g.link(b, "translation/1", a, "spare/height").unwrap();
        assert_eq!(y_of(&g, b), 4.0, "a parameter follows a spare one");
        assert!(g.link(b, "translation/1", a, "spare/nothing").is_err());
        // A spare parameter of B follows A's Y.
        g.nodes[1]
            .spare
            .add(ParameterType::Float, "Lift", "", None, &[])
            .unwrap();
        g.nodes[0].node_type = transform(2.0);
        g.link(b, "spare/lift", a, "translation/1").unwrap();
        let mut last = Followed::new();
        g.follow_kept(&mut last);
        assert_eq!(g.nodes[1].spare.get("spare/lift"), Some(serde_json::json!(2.0)));
        // Changed by hand: A's Y takes it.
        g.nodes[1].spare.put("spare/lift", serde_json::json!(5.0));
        g.follow_kept(&mut last);
        assert!(matches!(&g.nodes[0].node_type, NodeType::Transform { translation, .. } if translation.y == 5.0));
        assert_eq!(g.nodes[1].spare.get("spare/lift"), Some(serde_json::json!(5.0)));
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
        assert_eq!(get(&g), (-1.0, -1.0), "the number followed, changed, keeps its change");
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
