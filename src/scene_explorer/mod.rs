//! The Scene Explorer (the stage's primitives as a hierarchy, in `ui`) and the
//! Operator Stack (the chain of operators feeding the viewed one). Not to be
//! confused with the scene graph, the network of operators.

pub mod ui;

use crate::node_graph::{Connection, GraphNode};
use crate::types::{node_type_icon, node_type_label, NodeId};
use std::collections::{HashMap, HashSet};

// OPERATOR STACK  (mirrors the node graph — unchanged logic, renamed)

#[derive(Clone, Debug)]
pub struct StackEntry {
    pub node_id: NodeId,
    pub name: String,
    pub type_label: &'static str,
    pub depth: usize,
    pub type_icon: &'static str,
    pub expanded: bool,
    pub has_children: bool,
}

#[derive(Default)]
pub struct OperatorStack {
    pub entries: Vec<StackEntry>,
    pub selected_entry: Option<NodeId>,
}

impl OperatorStack {
    /// The operators the scene is made by: from `root`, the operator with the
    /// view flag, up through everything wired into it.
    pub fn rebuild(&mut self, nodes: &[GraphNode], connections: &[Connection], root: Option<NodeId>) {
        let expansions: HashMap<NodeId, bool> = self.entries.iter().map(|e| (e.node_id, e.expanded)).collect();
        self.entries.clear();

        let Some(output) = root.and_then(|r| nodes.iter().find(|n| n.id == r)) else {
            return;
        };

        let mut children_of: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for conn in connections {
            children_of.entry(conn.to_node).or_default().push(conn.from_node);
        }

        let mut stack: Vec<(NodeId, usize)> = vec![(output.id, 0)];
        let mut visited: HashSet<NodeId> = HashSet::new();

        while let Some((id, depth)) = stack.pop() {
            if visited.contains(&id) {
                continue;
            }
            visited.insert(id);

            let node = match nodes.iter().find(|n| n.id == id) {
                Some(n) => n,
                None => continue,
            };

            let kids = children_of.get(&id).cloned().unwrap_or_default();
            let has_children = !kids.is_empty();
            let expanded = *expansions.get(&id).unwrap_or(&true);

            self.entries.push(StackEntry {
                node_id: id,
                name: node.name.clone(),
                type_label: node_type_label(&node.node_type),
                depth,
                type_icon: node_type_icon(&node.node_type),
                expanded,
                has_children,
            });

            if expanded {
                for child in kids.iter().rev() {
                    stack.push((*child, depth + 1));
                }
            }
        }
    }
}
