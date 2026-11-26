//! Where the Viewer's and the Selection bar's edits go: the Edit Session at
//! the display flag, or one put in after the operator viewed, in its wires
//! (`core::edit_session`).

use super::{Connection, NodeGraphState, AFTER};
use crate::core::edit_session;
use crate::types::{NodeId, NodeType};

impl NodeGraphState {
    /// The Edit Session edits go into, if there is one already: the viewed
    /// operator, when it is one.
    pub fn session_at_view(&self) -> Option<NodeId> {
        let v = self.view_flag?;
        self.nodes
            .iter()
            .find(|n| n.id == v && matches!(n.node_type, NodeType::EditSession { .. }))
            .map(|n| n.id)
    }

    /// The Edit Session edits go into: the viewed one, else a new one after
    /// the viewed operator, fed by the output viewed and feeding what that
    /// output fed, the display flag passed on to it. The selection stays.
    /// `None` with nothing viewed.
    pub fn session_for_edits(&mut self) -> Option<NodeId> {
        if let Some(s) = self.session_at_view() {
            return Some(s);
        }
        let from = self.view_flag?;
        let out = self.view_output;
        let at = self.nodes.iter().find(|n| n.id == from)?.position + egui::vec2(0.0, AFTER);
        let id = self.add_node("EditSession".into(), NodeType::EditSession { steps: vec![] }, at);
        let fed: Vec<Connection> = self
            .connections
            .iter()
            .filter(|c| c.from_node == from && c.from_output == out)
            .cloned()
            .collect();
        for c in &fed {
            self.remove_connection(c.id);
            self.add_connection(id, 0, c.to_node, c.to_input);
            if let Some(n) = self.nodes.iter_mut().find(|n| n.id == c.to_node) {
                if n.position.y < at.y + AFTER {
                    n.position.y += AFTER;
                }
            }
        }
        self.add_connection(from, out, id, 0);
        self.view_flag = Some(id);
        self.view_output = 0;
        Some(id)
    }

    /// Values (prim, name, value) written as `verb` into the Edit Session
    /// edits go into (`edit_session::record`), made if need be.
    pub fn record_edits(&mut self, verb: &str, values: &[(String, &str, serde_json::Value)]) -> Option<NodeId> {
        if values.is_empty() {
            return None;
        }
        let id = self.session_for_edits()?;
        let n = self.nodes.iter_mut().find(|n| n.id == id)?;
        if let NodeType::EditSession { steps } = &mut n.node_type {
            let before = steps.len();
            let was = steps.last().cloned();
            edit_session::record(steps, verb, values);
            if steps.len() != before || steps.last() != was.as_ref() {
                self.mark_dirty();
            }
        }
        Some(id)
    }

    /// An Edit Session's steps that are on, as Edit Primitives in its place,
    /// in order (`edit_session::explode`): fed by what fed it, feeding what
    /// it fed, the display flag and the selection on the last. None on: the
    /// session goes and its wires are joined.
    pub fn explode_session(&mut self, id: NodeId) {
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            return;
        };
        let NodeType::EditSession { steps } = &node.node_type else {
            return;
        };
        let parts = edit_session::explode(steps);
        let pos = node.position;
        let source = node.inputs.first().and_then(|i| i.connected_output);
        let fed: Vec<Connection> = self.connections.iter().filter(|c| c.from_node == id).cloned().collect();
        let viewed = self.view_flag == Some(id);
        let mut last = source;
        for (k, (name, paths, edits)) in parts.into_iter().enumerate() {
            let n = self.add_node(
                name.chars().filter(|c| c.is_alphanumeric()).collect::<String>(),
                NodeType::EditPrimitive {
                    kind: Default::default(),
                    edits,
                },
                pos + egui::vec2(0.0, k as f32 * AFTER),
            );
            if let Some(e) = self.nodes.iter_mut().find(|x| x.id == n) {
                e.locations = paths;
            }
            if let Some((src, out)) = last {
                self.add_connection(src, out, n, 0);
            }
            last = Some((n, 0));
        }
        let wires: Vec<_> = self
            .connections
            .iter()
            .filter(|c| c.from_node == id || c.to_node == id)
            .map(|c| c.id)
            .collect();
        for c in wires {
            self.remove_connection(c);
        }
        self.nodes.retain(|n| n.id != id);
        self.selected_nodes.retain(|n| *n != id);
        if let Some((src, out)) = last {
            for c in &fed {
                self.add_connection(src, out, c.to_node, c.to_input);
            }
            if viewed {
                self.view_flag = Some(src);
                self.view_output = out;
            }
            if self.selected_node == Some(id) {
                self.selected_node = Some(src);
                self.selected_nodes = vec![src];
            }
        } else {
            if self.selected_node == Some(id) {
                self.selected_node = None;
            }
            self.repair_view_flag(None);
        }
        self.mark_dirty();
    }

    /// The steps of an Edit Session, if `id` is one.
    #[cfg(test)]
    pub fn session_steps(&self, id: NodeId) -> Option<&[edit_session::Step]> {
        match &self.nodes.iter().find(|n| n.id == id)?.node_type {
            NodeType::EditSession { steps } => Some(steps),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn chain() -> (NodeGraphState, NodeId, NodeId) {
        let mut g = NodeGraphState::default();
        let cube = g.add_node(
            "Cube".into(),
            NodeType::CreateCube {
                size: 1.0,
                location: "/cube".into(),
                xform: Default::default(),
            },
            egui::pos2(0.0, 0.0),
        );
        let write = g.add_node("Write".into(), NodeType::PassThrough, egui::pos2(0.0, 50.0));
        g.add_connection(cube, 0, write, 0);
        g.view_flag = Some(cube);
        (g, cube, write)
    }
    fn mv(g: &mut NodeGraphState, path: &str, x: f64) -> Option<NodeId> {
        g.record_edits("Move", &[(path.into(), "translate", json!([x, 0.0, 0.0]))])
    }

    #[test]
    fn the_first_edit_puts_a_session_in_after_the_viewed_operator() {
        let (mut g, cube, write) = chain();
        g.selected_node = Some(write);
        let s = mv(&mut g, "/cube", 2.0).expect("a session");
        // In the wire, viewed, the selection left alone.
        assert!(g.connections.iter().any(|c| c.from_node == cube && c.to_node == s));
        assert!(g.connections.iter().any(|c| c.from_node == s && c.to_node == write));
        assert!(!g.connections.iter().any(|c| c.from_node == cube && c.to_node == write));
        assert_eq!((g.view_flag, g.selected_node), (Some(s), Some(write)));
        // The next goes into it: no other operator.
        assert_eq!(mv(&mut g, "/cube", 3.0), Some(s));
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.session_steps(s).unwrap().len(), 1);
        // Viewing another operator: a session of its own after it.
        g.view_flag = Some(write);
        let t = mv(&mut g, "/cube", 4.0).unwrap();
        assert_ne!(t, s);
        assert!(g.connections.iter().any(|c| c.from_node == write && c.to_node == t));
        // Nothing viewed: nowhere to go.
        let mut e = NodeGraphState::default();
        assert!(mv(&mut e, "/cube", 1.0).is_none());
    }

    #[test]
    fn the_scene_shows_the_session_and_explode_keeps_it() {
        let (mut g, _cube, write) = chain();
        let s = mv(&mut g, "/cube", 2.0).unwrap();
        g.record_edits("Rotate", &[("/cube".into(), "rotate", json!([0.0, 90.0, 0.0]))]);
        let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
        let at = |g: &NodeGraphState| {
            let Some(crate::types::EvalResult::Named(p)) = g.evaluate_for_viewport_packed(&pass) else {
                panic!()
            };
            match &p[0].place {
                crate::types::Placement::One(m) => m.to_cols_array(),
                _ => panic!(),
            }
        };
        let before = at(&g);
        assert!((before[12] - 2.0).abs() < 1e-5, "{before:?}");
        g.selected_node = Some(s);
        g.explode_session(s);
        assert!(g.session_steps(s).is_none());
        let ep = g.view_flag.unwrap();
        assert!(matches!(
            g.nodes.iter().find(|n| n.id == ep).unwrap().node_type,
            NodeType::EditPrimitive { .. }
        ));
        assert_eq!(g.selected_node, Some(ep));
        assert!(g.connections.iter().any(|c| c.from_node == ep && c.to_node == write));
        let after = at(&g);
        for k in 0..16 {
            assert!((before[k] - after[k]).abs() < 1e-5, "{before:?} {after:?}");
        }
    }
}
