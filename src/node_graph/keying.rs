//! Keys on an operator's parameters, as the graph holds them: setting and
//! removing keys, the parameters at the playhead, and editing a keyed parameter
//! (which keys it). The curves themselves are `core::keys`.

use std::borrow::Cow;

use super::{GraphNode, NodeGraphState};
use crate::core::keys::{self, Channel};
use crate::types::{NodeId, NodeType};

/// A node's parameters at `frame`: its keyed numbers set from their curves. The
/// node's own when nothing is keyed.
pub fn animated_at(node: &GraphNode, frame: f64) -> Cow<'_, NodeType> {
    if node.channels.is_empty() {
        return Cow::Borrowed(&node.node_type);
    }
    let Ok(mut v) = serde_json::to_value(&node.node_type) else {
        return Cow::Borrowed(&node.node_type);
    };
    for c in &node.channels {
        if !c.keys.is_empty() {
            keys::set(&mut v, &c.path, c.value_at(frame));
        }
    }
    match serde_json::from_value(v) {
        Ok(t) => Cow::Owned(t),
        Err(_) => Cow::Borrowed(&node.node_type),
    }
}

/// The operator's kind as saved: "Transform".
pub fn kind_of(t: &NodeType) -> String {
    match serde_json::to_value(t) {
        Ok(serde_json::Value::Object(o)) => o.keys().next().cloned().unwrap_or_default(),
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}

impl NodeGraphState {
    /// The frame the playhead is on, at the scene's rate (fractional while
    /// playing a clip of another rate).
    pub fn frame(&self) -> f64 {
        self.scene.frame(self.now())
    }

    /// A node's parameters at the playhead (at its own local frame, for a Local
    /// Animation): keyed numbers from their curves, referenced ones from their
    /// sources (`links`).
    pub fn animated_type<'a>(&self, node: &'a GraphNode) -> Cow<'a, NodeType> {
        self.shown_at(node, self.node_frame(node))
    }

    fn node_mut(&mut self, id: NodeId) -> Option<&mut GraphNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// The numbers of a node that can be keyed, with their values at the
    /// playhead, in the order the parameters are saved.
    pub fn keyable_values(&self, id: NodeId) -> Vec<(String, f64)> {
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            return vec![];
        };
        let all = serde_json::to_value(&*self.animated_type(node))
            .map(|v| keys::keyable(&v))
            .unwrap_or_default();
        // A Local Animation keys the parameters it inherits, not its range.
        if matches!(node.node_type, NodeType::LocalAnimation { .. }) {
            return all.into_iter().filter(|(p, _)| p.starts_with("inherited/")).collect();
        }
        all
    }

    /// Set a key on `path` at `frame` with `value`.
    /// A number following another (a reference) is not keyed.
    pub fn set_key_value(&mut self, id: NodeId, path: &str, frame: f64, value: f64) {
        let Some(node) = self.node_mut(id) else { return };
        if node.links.iter().any(|l| l.path == path) {
            return;
        }
        let i = match node.channels.iter().position(|c| c.path == path) {
            Some(i) => i,
            None => {
                node.channels.push(Channel::new(path));
                node.channels.len() - 1
            }
        };
        node.channels[i].set(frame, value);
    }

    /// Set a key on `path` at `frame`, with the value it has at the playhead.
    pub fn set_key(&mut self, id: NodeId, path: &str, frame: f64) {
        let Some(value) = self
            .keyable_values(id)
            .into_iter()
            .find(|(p, _)| p == path)
            .map(|(_, v)| v)
        else {
            return;
        };
        self.set_key_value(id, path, frame, value);
    }

    /// Key every keyable number of a node at the playhead. How many.
    pub fn key_all(&mut self, id: NodeId) -> usize {
        let frame = self
            .nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| self.node_frame(n))
            .unwrap_or(0.0)
            .round();
        // Not the numbers that follow others.
        let values: Vec<(String, f64)> = self
            .keyable_values(id)
            .into_iter()
            .filter(|(p, _)| self.link_of(id, p).is_none())
            .collect();
        for (p, v) in &values {
            self.set_key_value(id, p, frame, *v);
        }
        values.len()
    }

    /// Take the key of `path` at `frame` away. A channel left with no keys
    /// goes, its number keeping the value it had there.
    pub fn remove_key(&mut self, id: NodeId, path: &str, frame: f64) {
        let Some(node) = self.node_mut(id) else { return };
        let Some(ci) = node.channels.iter().position(|c| c.path == path) else {
            return;
        };
        if let Some(k) = node.channels[ci].key_at(frame) {
            let value = node.channels[ci].keys[k].value;
            node.channels[ci].keys.remove(k);
            if node.channels[ci].keys.is_empty() {
                node.channels.remove(ci);
                if let Ok(mut v) = serde_json::to_value(&node.node_type) {
                    keys::set(&mut v, path, value);
                    if let Ok(t) = serde_json::from_value(v) {
                        node.node_type = t;
                    }
                }
            }
        }
    }

    /// The frames a node has keys on, in order, once each.
    pub fn key_frames(&self, id: NodeId) -> Vec<f64> {
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            return vec![];
        };
        let mut f: Vec<f64> = node
            .channels
            .iter()
            .flat_map(|c| c.keys.iter().map(|k| k.frame))
            .collect();
        f.sort_by(f64::total_cmp);
        f.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        f
    }

    /// Move every key of a node on frame `from` to frame `to`. A key already on
    /// `to` is replaced.
    pub fn move_keys(&mut self, id: NodeId, from: f64, to: f64) {
        let Some(node) = self.node_mut(id) else { return };
        for c in &mut node.channels {
            let Some(i) = c.key_at(from) else { continue };
            let mut k = c.keys.remove(i);
            if let Some(j) = c.key_at(to) {
                c.keys.remove(j);
            }
            k.frame = to;
            c.keys.push(k);
            c.sort();
        }
    }

    /// Take away every key of a node on `frame`.
    pub fn delete_keys_at(&mut self, id: NodeId, frame: f64) {
        let paths: Vec<String> = self
            .nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| {
                n.channels
                    .iter()
                    .filter(|c| c.key_at(frame).is_some())
                    .map(|c| c.path.clone())
                    .collect()
            })
            .unwrap_or_default();
        for p in paths {
            self.remove_key(id, &p, frame);
        }
    }

    /// Before the Parameters pane or a handle edits a node: its parameters as
    /// at the playhead, so what is shown is what is cooked. Returns its own,
    /// for `end_edit`.
    pub fn begin_edit(&mut self, id: NodeId) -> Option<NodeType> {
        let node = self.nodes.iter().find(|n| n.id == id)?;
        let own = node.node_type.clone();
        if node.channels.is_empty() && node.links.is_empty() {
            return Some(own);
        }
        let shown = self.animated_type(node).into_owned();
        self.node_mut(id)?.node_type = shown;
        Some(own)
    }

    /// After the edit: a keyed number that was changed is keyed at the playhead
    /// with its new value; a number that follows another, changed, changes the
    /// number at the end of its chain, and keeps following it; everything else
    /// changed is kept as the node's own. Nothing changed: the node is as it
    /// was.
    pub fn end_edit(&mut self, id: NodeId, own: NodeType) {
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            return;
        };
        if node.channels.is_empty() && node.links.is_empty() {
            return;
        }
        let frame = self.node_frame(node);
        // What was shown: its own parameters as they cook.
        let as_it_was = GraphNode {
            node_type: own.clone(),
            ..node.clone()
        };
        let shown = self.shown_at(&as_it_was, frame).into_owned();
        let (Ok(edited), Ok(shown_v), Ok(own_v)) = (
            serde_json::to_value(&node.node_type),
            serde_json::to_value(&shown),
            serde_json::to_value(&own),
        ) else {
            return;
        };
        if edited == shown_v {
            if let Some(node) = self.node_mut(id) {
                node.node_type = own;
            }
            return;
        }
        let changed = |p: &str| match (keys::get(&edited, p), keys::get(&shown_v, p)) {
            // Parameters are f32: a row converting (degrees) moves the others
            // by no more than rounding.
            (Some(now), Some(was)) => (now - was).abs() > 1e-6 * (1.0 + was.abs()),
            _ => false,
        };
        let mut kept = edited.clone();
        let key = frame.round();
        let mut keyed = vec![];
        for c in &node.channels {
            if changed(&c.path) {
                keyed.push((c.path.clone(), keys::get(&edited, &c.path).unwrap_or_default()));
            }
            // The node's own number stays as it was: the curve gives it.
            if let Some(o) = keys::get(&own_v, &c.path) {
                keys::set(&mut kept, &c.path, o);
            }
        }
        // A number that follows another keeps its own; changed, the number it
        // follows takes the change.
        let mut through = vec![];
        for l in &node.links {
            if changed(&l.path) {
                if let (Some(root), Some(x)) = (self.root_of(id, &l.path), keys::get(&edited, &l.path)) {
                    through.push((root, x));
                }
            }
            if let Some(o) = keys::get(&own_v, &l.path) {
                keys::set(&mut kept, &l.path, o);
            }
        }
        let Some(node) = self.node_mut(id) else { return };
        if let Ok(t) = serde_json::from_value(kept) {
            node.node_type = t;
        }
        for (p, v) in keyed {
            self.set_key_value(id, &p, key, v);
        }
        for ((root, p), v) in through {
            self.set_number(root, &p, v);
        }
    }

    /// Set number `path` of a node to `value`: keyed at its playhead when it
    /// is keyed, its own otherwise.
    fn set_number(&mut self, id: NodeId, path: &str, value: f64) {
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            return;
        };
        if node.channels.iter().any(|c| c.path == path) {
            let frame = self.node_frame(node).round();
            self.set_key_value(id, path, frame, value);
            return;
        }
        let Some(node) = self.node_mut(id) else { return };
        let Ok(mut v) = serde_json::to_value(&node.node_type) else {
            return;
        };
        if keys::set(&mut v, path, value) {
            if let Ok(t) = serde_json::from_value(v) {
                node.node_type = t;
            }
        }
    }

    /// Change a node's parameters as at the playhead: `begin_edit`, the change,
    /// `end_edit`. For handles in the Viewer.
    pub fn edit_animated(&mut self, id: NodeId, f: impl FnOnce(&mut NodeType)) {
        let Some(own) = self.begin_edit(id) else { return };
        if let Some(n) = self.node_mut(id) {
            f(&mut n.node_type);
        }
        self.end_edit(id, own);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EvalResult;

    fn pass(
        _: crate::types::IceGraphId,
        m: &crate::core::geo::Geo,
        _: &[crate::ice::Reference],
    ) -> crate::core::geo::Geo {
        m.clone()
    }

    /// Two keys on a Transform's translate X: the result moves with the
    /// playhead, the graph's content too; editing at the playhead keys the
    /// number; a frozen copy of the node is not touched. Saved and read back.
    #[test]
    fn keyed_parameters_cook_at_the_playhead() {
        let mut g = NodeGraphState::default();
        let cube = g.add_node(
            "Cube".into(),
            NodeType::CreateCube {
                size: 1.0,
                location: "/cube".into(),
            },
            egui::pos2(0.0, 0.0),
        );
        let mv = g.add_node(
            "Move".into(),
            NodeType::Transform {
                translation: glam::Vec3::ZERO,
                rotation: glam::Vec3::ZERO,
                scale: glam::Vec3::ONE,
            },
            egui::pos2(0.0, 100.0),
        );
        g.add_connection(cube, 0, mv, 0);
        g.set_key_value(mv, "translation/0", 1.0, 0.0);
        g.set_key_value(mv, "translation/0", 25.0, 4.0);
        for k in &mut g.nodes.iter_mut().find(|n| n.id == mv).unwrap().channels[0].keys {
            k.in_type = keys::Tangent::Linear;
            k.out_type = keys::Tangent::Linear;
        }
        let x = |g: &NodeGraphState| match g.eval_node(mv, &mut Default::default(), &pass) {
            Some(EvalResult::Named(p)) => {
                let b = crate::viewport::bounds::world_bounds(&p[0]).unwrap();
                (b.0.x + b.1.x) * 0.5
            }
            _ => panic!(),
        };
        g.time = g.scene.seconds(1.0);
        assert!(x(&g).abs() < 1e-5);
        let h0 = g.content_hash();
        g.time = g.scene.seconds(13.0);
        assert!((x(&g) - 2.0).abs() < 1e-4, "{}", x(&g));
        assert_ne!(h0, g.content_hash(), "the playhead moves a keyed graph");
        assert_eq!(g.key_frames(mv), [1.0, 25.0]);

        // The Parameters pane shows 2 at frame 13; typing 3 keys it there.
        let own = g.begin_edit(mv).unwrap();
        let shown = g.nodes.iter().find(|n| n.id == mv).unwrap().node_type.clone();
        assert!(matches!(shown, NodeType::Transform { translation, .. } if (translation.x - 2.0).abs() < 1e-4));
        g.end_edit(mv, own.clone());
        let json = |t: &NodeType| serde_json::to_string(t).unwrap();
        assert_eq!(
            json(&g.nodes.iter().find(|n| n.id == mv).unwrap().node_type),
            json(&own),
            "nothing edited, nothing changed"
        );
        g.edit_animated(mv, |t| {
            if let NodeType::Transform { translation, scale, .. } = t {
                translation.x = 3.0;
                scale.y = 2.0;
            }
        });
        assert_eq!(g.key_frames(mv), [1.0, 13.0, 25.0]);
        assert!((x(&g) - 3.0).abs() < 1e-4);
        // The unkeyed scale is the node's own; the keyed X is not.
        assert!(
            matches!(g.nodes.iter().find(|n| n.id == mv).unwrap().node_type, NodeType::Transform { translation, scale, .. } if translation.x == 0.0 && scale.y == 2.0)
        );

        // Moved and deleted.
        g.move_keys(mv, 13.0, 10.0);
        assert_eq!(g.key_frames(mv), [1.0, 10.0, 25.0]);
        g.delete_keys_at(mv, 10.0);
        assert_eq!(g.key_frames(mv), [1.0, 25.0]);

        // Saved and read back.
        let mut h = NodeGraphState::default();
        crate::graph_io::from_json(
            &mut h,
            &mut Default::default(),
            &crate::graph_io::to_json(&g, &Default::default()),
        )
        .unwrap();
        let n = h.nodes.iter().find(|n| n.name == "Move").unwrap();
        assert_eq!(n.channels, g.nodes.iter().find(|n| n.id == mv).unwrap().channels);
        assert_eq!(h.scene, g.scene);

        // S: every keyable number of the cube keyed at the playhead.
        assert!(g.key_all(cube) >= 1);
        assert_eq!(g.key_frames(cube), [13.0]);
        // The last key of a channel taken away: the number keeps its value.
        g.remove_key(cube, "size", 13.0);
        assert!(g
            .nodes
            .iter()
            .find(|n| n.id == cube)
            .unwrap()
            .channels
            .iter()
            .all(|c| c.path != "size"));
    }

    /// The bouncing ball template: high on frame 13, down on 25, and a metre on
    /// by the next bounce (cycle with offset).
    #[test]
    fn the_bouncing_ball_bounces() {
        let t = crate::templates::TEMPLATES
            .iter()
            .find(|t| t.name == "Keys: a bouncing ball")
            .unwrap();
        let mut g = NodeGraphState::default();
        (t.build)(&mut g, &mut Default::default());
        let mv = g.nodes.iter().find(|n| n.name == "Bounce").unwrap().id;
        let at = |g: &mut NodeGraphState, f: f64| {
            g.time = g.scene.seconds(f);
            match g.eval_node(mv, &mut Default::default(), &pass) {
                Some(EvalResult::Named(p)) => {
                    let b = crate::viewport::bounds::world_bounds(&p[0]).unwrap();
                    (b.0 + b.1) * 0.5
                }
                _ => panic!(),
            }
        };
        let (a, top, b, next) = (at(&mut g, 1.0), at(&mut g, 13.0), at(&mut g, 25.0), at(&mut g, 37.0));
        assert!(top.y > a.y + 1.5 && (b.y - a.y).abs() < 1e-3, "{a} {top} {b}");
        assert!((next.y - top.y).abs() < 1e-3, "cycled");
        assert!((b.x - a.x - 1.0).abs() < 1e-3 && next.x > b.x, "{a} {b} {next}");
    }
}
