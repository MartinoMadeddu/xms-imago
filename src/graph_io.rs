//! Saving and loading the node graph as JSON, copying and pasting nodes,
//! adding one graph to another, and ready-made graphs.
//!
//! The contents of ICE graphs are not saved in a graph file yet: a loaded
//! ICE node starts with a fresh, empty graph. Copied nodes carry their ICE
//! graphs.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ice::IceGraphStore;
use crate::node_graph::NodeGraphState;
use crate::types::{NodeId, NodeType, SplitPick, IceGraphId, DEFAULT_WRITE_PATH};

fn is_false(b: &bool) -> bool { !*b }

#[derive(Serialize, Deserialize)]
struct SavedNode {
    id:        usize,
    name:      String,
    position:  [f32; 2],
    node_type: NodeType,
    #[serde(default)]
    bypassed:  bool,
    /// Frozen and ghosted (see `GraphNode`). Left out when off.
    #[serde(default, skip_serializing_if = "is_false")]
    frozen:    bool,
    #[serde(default, skip_serializing_if = "is_false")]
    ghost:     bool,
    /// Keys on its parameters. Left out when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    channels:  Vec<crate::core::keys::Channel>,
    /// Its Prims Location tab. Left out of the file when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    locations: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct SavedGraph {
    version:     u32,
    nodes:       Vec<SavedNode>,
    /// from node, from output, to node, to input
    connections: Vec<[usize; 4]>,
    view_flag:   Option<usize>,
    /// The output of the viewed operator that is viewed (see
    /// `NodeGraphState::view_output`).
    #[serde(default)]
    view_output: usize,
    /// The scene's frame range and rate. Graphs saved before it have the
    /// default (1 to 120 at 24 fps).
    #[serde(default)]
    scene:       crate::node_graph::SceneRange,
}

pub fn to_json(graph: &NodeGraphState) -> String {
    let saved = SavedGraph {
        version: 1,
        nodes: graph.nodes.iter().map(|n| SavedNode {
            id:        n.id.0,
            name:      n.name.clone(),
            position:  [n.position.x, n.position.y],
            node_type: n.node_type.clone(),
            bypassed:  n.bypassed,
            frozen:    n.frozen,
            ghost:     n.ghost,
            channels:  n.channels.clone(),
            locations: n.locations.clone(),
        }).collect(),
        connections: graph.connections.iter()
            .map(|c| [c.from_node.0, c.from_output, c.to_node.0, c.to_input])
            .collect(),
        view_flag: graph.view_flag.map(|n| n.0),
        view_output: graph.view_output,
        scene: graph.scene,
    };
    serde_json::to_string_pretty(&saved).unwrap_or_default()
}

/// Replace the contents of `graph` with a saved graph. View settings (pan,
/// zoom) are kept.
pub fn from_json(graph: &mut NodeGraphState, json: &str) -> Result<(), String> {
    let mut value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    upgrade_json(&mut value);
    let mut saved: SavedGraph = serde_json::from_value(value).map_err(|e| e.to_string())?;
    upgrade(&mut saved);
    clear(graph);
    for n in saved.nodes {
        let mut node_type = n.node_type;
        if let NodeType::Ice { id, .. } = &mut node_type { *id = IceGraphId(usize::MAX); }
        let id = graph.add_node(n.name, node_type, egui::pos2(n.position[0], n.position[1]));
        // Keep the saved ids so connections can be restored as written.
        if let Some(node) = graph.nodes.iter_mut().find(|x| x.id == id) { node.id = NodeId(n.id); node.bypassed = n.bypassed; node.frozen = n.frozen; node.ghost = n.ghost; node.channels = n.channels; node.locations = n.locations; }
        graph.next_node_id = graph.next_node_id.max(n.id + 1);
    }
    for [from, out, to, input] in saved.connections {
        let ok = graph.nodes.iter().any(|n| n.id.0 == from && out < n.outputs.len())
              && graph.nodes.iter().any(|n| n.id.0 == to && input < n.inputs.len());
        if ok { graph.add_connection(NodeId(from), out, NodeId(to), input); }
    }
    // Merge and Material Binding keep an input free.
    let ids: Vec<NodeId> = graph.nodes.iter().map(|n| n.id).collect();
    for id in ids { graph.keep_an_input_free(id); }
    graph.view_flag = saved.view_flag.map(NodeId).filter(|v| graph.nodes.iter().any(|n| n.id == *v));
    graph.view_output = saved.view_output;
    graph.scene = saved.scene;
    // A graph saved with nothing viewed views the end of its last chain.
    graph.repair_view_flag(None);
    Ok(())
}

/// Operators that changed shape, brought up to date before the graph is
/// read, in the text of a saved graph or of copied nodes (the scene
/// graph's own nodes only: ICE graphs have operators of the same names):
/// - Merge, saved before it had a mode, gets Auto.
/// - Scatter Points and Copy to Points are gone: ICE graphs do both. Each
///   becomes a Placeholder with the same inputs and outputs and a note, so
///   the graph opens with its wires in place.
/// - Output is gone: the view flag says what the scene is. What was wired
///   into Output gets the flag, unless another operator had it.
fn upgrade_json(value: &mut serde_json::Value) {
    use serde_json::{json, Value};
    remove_output(value);
    let Some(nodes) = value.get_mut("nodes").and_then(Value::as_array_mut) else { return };
    for node in nodes {
        let Some(t) = node.get_mut("node_type") else { continue };
        let name = match t {
            Value::String(s) => s.clone(),
            Value::Object(o) if o.len() == 1 => o.keys().next().cloned().unwrap_or_default(),
            _ => continue,
        };
        let removed = |inputs: &[&str], outputs: &[&str], what: &str| json!({ "Placeholder": {
            "inputs": inputs, "outputs": outputs,
            "description": format!("{what} was removed from the scene graph: do it in an ICE graph, which has {what} as a node. Until then this node passes its inputs through"),
        }});
        *t = match name.as_str() {
            "Merge" if t.is_string() => json!({ "Merge": { "mode": "Auto" } }),
            "ScatterPoints" => removed(&["Surface"], &["Points"], "Scatter Points"),
            "CopyToPoints" => removed(&["Template", "Points"], &["Geo"], "Copy to Points"),
            _ => continue,
        };
    }
}

/// Take the Output node out of a saved graph (or copied nodes). The view
/// flag goes to the operator and output wired into it, when no other
/// operator had the flag.
fn remove_output(value: &mut serde_json::Value) {
    use serde_json::{json, Value};
    let ids: Vec<Value> = value.get("nodes").and_then(Value::as_array).map(|nodes| nodes.iter()
        .filter(|n| n.get("node_type").and_then(Value::as_str) == Some("Output"))
        .filter_map(|n| n.get("id").cloned()).collect()).unwrap_or_default();
    for id in ids {
        let wired = value.get("connections").and_then(Value::as_array)
            .and_then(|cs| cs.iter().find(|c| c.get(2) == Some(&id)))
            .map(|c| (c[0].clone(), c[1].clone()));
        if let Some(cs) = value.get_mut("connections").and_then(Value::as_array_mut) {
            cs.retain(|c| c.get(0) != Some(&id) && c.get(2) != Some(&id));
        }
        if let Some(nodes) = value.get_mut("nodes").and_then(Value::as_array_mut) {
            nodes.retain(|n| n.get("id") != Some(&id));
        }
        if let Some(obj) = value.as_object_mut() {
            let unset = obj.get("view_flag").is_none_or(|f| f.is_null() || *f == id);
            if unset && obj.contains_key("view_flag") {
                match wired {
                    Some((src, out)) => { obj.insert("view_flag".into(), src); obj.insert("view_output".into(), out); }
                    None => { obj.insert("view_flag".into(), Value::Null); obj.insert("view_output".into(), json!(0)); }
                }
            }
        }
    }
}

/// Bring a graph saved by an older version up to date:
/// - Transform Clip becomes Transform, which now moves clips too. The turn
///   is the same rotation, written in Transform's convention.
/// - Calamari goes. Its display moves onto the Body Collide node it came
///   after, and the wires through it are joined up.
/// - Load FBX Mesh becomes Load FBX, which reads every mesh too: its wires
///   leave from Load FBX's Geometry output.
fn upgrade(saved: &mut SavedGraph) {
    for n in saved.nodes.iter_mut() {
        if let NodeType::LoadFbxMesh { path } = &n.node_type {
            n.node_type = NodeType::LoadFbx { path: path.clone(), take: 0 };
            for c in saved.connections.iter_mut().filter(|c| c[0] == n.id && c[1] == 0) { c[1] = 1; }
        }
    }
    // The example take and car were `.xmsclip` and `.xmsmesh` files; they
    // are FBX now. The car is the same mesh, polygon for polygon, so picks
    // made on it still hold.
    for n in saved.nodes.iter_mut() {
        if let NodeType::LoadFbx { path, .. } = &mut n.node_type {
            for (old, new) in [("ragdoll/car.xmsmesh", "ragdoll/car.fbx"), ("ragdoll/into_the_car.xmsclip", "ragdoll/split/into_the_car_.fbx")] {
                if path.replace('\\', "/").ends_with(old) { *path = format!("{}{new}", &path[..path.len() - old.len()]); }
            }
        }
    }
    use glam::{EulerRot, Quat, Vec3};
    for n in saved.nodes.iter_mut() {
        if let NodeType::TransformClip { translate, rotate, scale } = n.node_type.clone() {
            let q = Quat::from_euler(EulerRot::YXZ, rotate[1].to_radians(), rotate[0].to_radians(), rotate[2].to_radians());
            let (x, y, z) = q.to_euler(EulerRot::XYZ);
            n.node_type = NodeType::Transform { translation: Vec3::from_array(translate), rotation: Vec3::new(x, y, z), scale: Vec3::splat(scale) };
        }
    }
    let calamari: Vec<(usize, bool)> = saved.nodes.iter()
        .filter_map(|n| match n.node_type { NodeType::Calamari { hulls, .. } => Some((n.id, hulls)), _ => None }).collect();
    for (id, hulls) in calamari {
        let from = saved.connections.iter().find(|c| c[2] == id && c[3] == 0).map(|c| (c[0], c[1]));
        let to: Vec<(usize, usize)> = saved.connections.iter().filter(|c| c[0] == id).map(|c| (c[2], c[3])).collect();
        saved.connections.retain(|c| c[0] != id && c[2] != id);
        if let Some((src, out)) = from {
            for (dst, input) in to { saved.connections.push([src, out, dst, input]); }
            if let Some(n) = saved.nodes.iter_mut().find(|n| n.id == src) {
                if let NodeType::Ragdoll { view, .. } = &mut n.node_type {
                    *view = if hulls { crate::types::BodyView::Hulls } else { crate::types::BodyView::Pieces };
                }
            }
            if saved.view_flag == Some(id) { saved.view_flag = Some(src); }
        } else if saved.view_flag == Some(id) {
            saved.view_flag = None;
        }
        saved.nodes.retain(|n| n.id != id);
    }
}

pub fn save(graph: &NodeGraphState, path: &Path) -> Result<(), String> {
    std::fs::write(path, to_json(graph)).map_err(|e| e.to_string())
}

pub fn load(graph: &mut NodeGraphState, path: &Path) -> Result<(), String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    from_json(graph, &json)
}

// ── Copy, paste, and adding one graph to another ─────────────────────────────

/// Nodes copied from a scene network: their parameters, the wires between
/// them and the trees of their ICE nodes. Held as JSON text in the system
/// clipboard and in `clipboard.json` in the config folder, so it can be
/// pasted in another window of the program, or after it was closed.
#[derive(Serialize, Deserialize)]
pub struct Fragment {
    /// Marks the text as XMS nodes: other text in the clipboard is ignored.
    xms_nodes:   u32,
    nodes:       Vec<SavedNode>,
    /// from node, from output, to node, to input
    connections: Vec<[usize; 4]>,
    #[serde(default)]
    trees:       Vec<SavedTree>,
}

/// The tree of an ICE node.
#[derive(Serialize, Deserialize)]
struct SavedTree {
    /// The ICE node it belongs to.
    node:        usize,
    name:        String,
    nodes:       Vec<SavedTreeNode>,
    connections: Vec<[usize; 4]>,
}

#[derive(Serialize, Deserialize)]
struct SavedTreeNode {
    id:        usize,
    name:      String,
    position:  [f32; 2],
    /// Kept as JSON and read node by node, so a node of an older ICE (which
    /// the new engine replaced) is left out instead of failing the paste.
    node_type: serde_json::Value,
    /// What each input holds while nothing is wired to it.
    #[serde(default)]
    values:    Vec<Option<xms_ice::Value>>,
}

/// What pasting placed: the new id of each node, by the id it was copied with.
pub type Placed = std::collections::HashMap<usize, NodeId>;

impl Fragment {
    pub fn len(&self) -> usize { self.nodes.len() }

    pub fn to_text(&self) -> String { serde_json::to_string_pretty(self).unwrap_or_default() }

    /// The fragment in a text, if the text is one.
    pub fn from_text(text: &str) -> Option<Self> {
        let mut value: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
        upgrade_json(&mut value);
        let f: Self = serde_json::from_value(value).ok()?;
        (f.xms_nodes >= 1 && !f.nodes.is_empty()).then_some(f)
    }
}

/// Copy nodes, with the wires between them.
pub fn copy(graph: &NodeGraphState, subnets: &IceGraphStore, ids: &[NodeId]) -> Option<Fragment> {
    let picked: Vec<&crate::node_graph::GraphNode> = graph.nodes.iter()
        .filter(|n| ids.contains(&n.id)).collect();
    if picked.is_empty() { return None; }
    let inside = |id: NodeId| picked.iter().any(|n| n.id == id);
    let trees = picked.iter().filter_map(|n| match &n.node_type {
        NodeType::Ice { id, .. } => subnets.get(*id).map(|sg| SavedTree {
            node: n.id.0,
            name: sg.name.clone(),
            nodes: sg.nodes.iter().map(|t| SavedTreeNode {
                id: t.id.0, name: t.name.clone(), position: [t.position.x, t.position.y],
                node_type: serde_json::to_value(&t.node_type).unwrap_or_default(),
                values: t.inputs.iter().map(|i| i.value).collect(),
            }).collect(),
            connections: sg.connections.iter().map(|c| [c.from_node.0, c.from_output, c.to_node.0, c.to_input]).collect(),
        }),
        _ => None,
    }).collect();
    Some(Fragment {
        xms_nodes: 1,
        nodes: picked.iter().map(|n| SavedNode {
            id: n.id.0, name: n.name.clone(), position: [n.position.x, n.position.y],
            node_type: n.node_type.clone(), bypassed: n.bypassed, frozen: n.frozen, ghost: n.ghost, channels: n.channels.clone(), locations: n.locations.clone(),
        }).collect(),
        connections: graph.connections.iter()
            .filter(|c| inside(c.from_node) && inside(c.to_node))
            .map(|c| [c.from_node.0, c.from_output, c.to_node.0, c.to_input]).collect(),
        trees,
    })
}

/// Paste a fragment: new nodes with the same parameters and wires, the
/// top-left one at `at` (or beside where they were copied from). Each ICE
/// node gets a tree of its own. The pasted nodes are selected.
pub fn paste(graph: &mut NodeGraphState, subnets: &mut IceGraphStore, fragment: Fragment, at: Option<egui::Pos2>) -> Placed {
    let Fragment { nodes, connections, trees, .. } = fragment;
    // Nodes from an older version are brought up to date as a file is.
    let mut saved = SavedGraph { version: 1, nodes, connections, view_flag: None, view_output: 0, scene: Default::default() };
    upgrade(&mut saved);
    let corner = saved.nodes.iter().fold(egui::pos2(f32::MAX, f32::MAX), |c, n| egui::pos2(c.x.min(n.position[0]), c.y.min(n.position[1])));
    let shift = match at { Some(at) => at - corner, None => egui::vec2(40.0, 40.0) };
    let mut placed = Placed::new();
    for n in saved.nodes {
        let mut node_type = n.node_type;
        if let NodeType::Ice { id, name } = &mut node_type {
            *id = trees.iter().find(|t| t.node == n.id)
                .map(|t| restore_tree(t, name, subnets))
                .unwrap_or(IceGraphId(usize::MAX));
        }
        let new = graph.add_node(n.name, node_type, egui::pos2(n.position[0], n.position[1]) + shift);
        if let Some(node) = graph.nodes.iter_mut().find(|x| x.id == new) { node.bypassed = n.bypassed; node.frozen = n.frozen; node.ghost = n.ghost; node.channels = n.channels.clone(); node.locations = n.locations.clone(); }
        placed.insert(n.id, new);
    }
    for [from, out, to, input] in saved.connections {
        let (Some(&a), Some(&b)) = (placed.get(&from), placed.get(&to)) else { continue };
        let ok = graph.nodes.iter().any(|n| n.id == a && out < n.outputs.len())
              && graph.nodes.iter().any(|n| n.id == b && input < n.inputs.len());
        if ok { graph.add_connection(a, out, b, input); }
    }
    for id in placed.values().copied().collect::<Vec<_>>() { graph.keep_an_input_free(id); }
    let mut new: Vec<NodeId> = placed.values().copied().collect();
    new.sort_by_key(|id| id.0);
    graph.selected_node = new.first().copied();
    graph.selected_nodes = new;
    graph.selected_connection = None;
    placed
}

fn restore_tree(t: &SavedTree, name: &str, subnets: &mut IceGraphStore) -> IceGraphId {
    let sid = subnets.create_subnet(name.to_string());
    let Some(sg) = subnets.get_mut(sid) else { return sid };
    sg.name = t.name.clone();
    sg.nodes.clear();
    sg.connections.clear();
    sg.next_node_id = 0;
    for n in &t.nodes {
        let Ok(node_type) = serde_json::from_value::<crate::types::IceGraphNodeType>(n.node_type.clone()) else { continue };
        // The root's old names: ICE Tree, then ICEExectPorts.
        let name = if matches!(n.name.as_str(), "ICE Tree" | "ICEExectPorts") && matches!(node_type, crate::types::IceGraphNodeType::Root { .. }) { crate::ice::ROOT_NAME.to_string() } else { n.name.clone() };
        let id = sg.add_node(name, node_type, egui::pos2(n.position[0], n.position[1]));
        if let Some(node) = sg.nodes.iter_mut().find(|x| x.id == id) {
            node.id = NodeId(n.id);
            for (input, value) in node.inputs.iter_mut().zip(&n.values) {
                if value.is_some() { input.value = *value; }
            }
        }
        sg.next_node_id = sg.next_node_id.max(n.id + 1);
    }
    // A tree from before the new engine has no root: it gets an empty one.
    if sg.root().is_none() {
        sg.add_node(crate::ice::ROOT_NAME.into(), crate::types::IceGraphNodeType::Root { context: crate::core::geo::Context::Point }, egui::pos2(500.0, 200.0));
    }
    // The root's ports, as many as the wires into it need.
    if let Some(root) = sg.nodes.iter_mut().find(|n| matches!(n.node_type, crate::types::IceGraphNodeType::Root { .. })) {
        let wanted = t.connections.iter().filter(|c| c[2] == root.id.0).map(|c| c[3] + 1).max().unwrap_or(0);
        while root.inputs.len() < wanted { let n = root.inputs.len() + 1; root.inputs.push(crate::ice::execute_port(n)); }
    }
    for &[from, out, to, input] in &t.connections {
        let ok = sg.nodes.iter().any(|n| n.id.0 == from && out < n.outputs.len())
              && sg.nodes.iter().any(|n| n.id.0 == to && input < n.inputs.len());
        if ok { sg.add_connection(NodeId(from), out, NodeId(to), input); }
    }
    sid
}

/// Add every node of `other` to `graph`, to the right of what is there.
/// When `graph` had nothing to view, the viewed operator of `other` is
/// viewed. Returns how many nodes were added.
pub fn add_graph(graph: &mut NodeGraphState, subnets: &mut IceGraphStore, other: &NodeGraphState) -> usize {
    let had_view = graph.view_flag.is_some();
    let ids: Vec<NodeId> = other.nodes.iter().map(|n| n.id).collect();
    let Some(fragment) = copy(other, subnets, &ids) else { return 0 };
    let count = fragment.len();
    let w = crate::node_graph::ui::NODE_WIDTH;
    let at = graph.nodes.iter()
        .fold(None::<egui::Rect>, |r, n| {
            let b = egui::Rect::from_min_size(n.position, egui::vec2(w, crate::node_graph::ui::NODE_HEIGHT));
            Some(r.map_or(b, |r| r.union(b)))
        })
        .map(|r| egui::pos2(r.right() + 120.0, r.top()));
    let placed = paste(graph, subnets, fragment, at.or(Some(egui::pos2(40.0, 20.0))));
    if !had_view {
        if let Some(&v) = other.view_flag.and_then(|v| placed.get(&v.0)) {
            graph.view_flag = Some(v);
            graph.view_output = other.view_output;
        }
    }
    graph.repair_view_flag(None);
    graph.frame_request = true;
    count
}

/// Keep copied nodes for a later session: the system clipboard can lose
/// them when the program that copied them closes.
pub fn remember(text: &str) {
    if let Some(file) = crate::file_browser::config_path("clipboard.json") {
        if let Some(dir) = file.parent() { let _ = std::fs::create_dir_all(dir); }
        let _ = std::fs::write(file, text);
    }
}

/// The nodes copied last, from this session or an earlier one.
pub fn remembered() -> Option<Fragment> {
    let file = crate::file_browser::config_path("clipboard.json")?;
    Fragment::from_text(&std::fs::read_to_string(file).ok()?)
}

/// Remove every operator.
pub(crate) fn clear(graph: &mut NodeGraphState) {
    graph.connections.clear();
    graph.nodes.clear();
    graph.selected_node = None;
    graph.selected_nodes.clear();
    graph.selected_connection = None;
    graph.view_flag = None;
    graph.view_output = 0;
    graph.scene = Default::default();
    graph.dragging_node = None;
    graph.connecting_from = None;
}

/// Graph that splits a two-character mocap take for Unreal: per character,
/// one FBX with the animation and one skinned T-pose FBX.
pub fn mocap_split_template(graph: &mut NodeGraphState) {
    clear(graph);
    let p = |x: f32, y: f32| egui::pos2(x, y);

    let load = graph.add_node("Takes".into(),
        NodeType::LoadFbxDir { dir: String::new(), index: 0, take: 0 }, p(330.0, 20.0));
    let split = graph.add_node("Split".into(),
        NodeType::SplitSkeleton { picks: vec![SplitPick::Character(0), SplitPick::Character(1)] }, p(330.0, 120.0));
    graph.add_connection(load, 0, split, 0);

    let tpose_path = DEFAULT_WRITE_PATH.replace(".fbx", "_tpose.fbx");
    for c in 0..2usize {
        let x = 20.0 + c as f32 * 420.0;
        let n = c + 1;
        let anim = graph.add_node(format!("WriteAnim{n}"),
            NodeType::WriteFbx { path: DEFAULT_WRITE_PATH.into(), mesh: false }, p(x, 240.0));
        let tpose = graph.add_node(format!("TPose{n}"),
            NodeType::AutoTPose { set_hip_height: false, hip_height: 90.0 }, p(x + 200.0, 240.0));
        let fix = graph.add_node(format!("FixPose{n}"),
            NodeType::FixPose { edits: vec![] }, p(x + 200.0, 340.0));
        let skin = graph.add_node(format!("ProxySkin{n}"),
            NodeType::ProxySkin { thickness: 1.0 }, p(x + 200.0, 440.0));
        let write = graph.add_node(format!("WriteTPose{n}"),
            NodeType::WriteFbx { path: tpose_path.clone(), mesh: true }, p(x + 200.0, 540.0));
        graph.add_connection(split, c, anim, 0);
        graph.add_connection(split, c, tpose, 0);
        graph.add_connection(tpose, 0, fix, 0);
        graph.add_connection(fix, 0, skin, 0);
        graph.add_connection(skin, 0, write, 0);
    }

    graph.view_flag     = Some(split);
    graph.view_output   = 0;
    graph.selected_node = Some(load);
    graph.selected_nodes = vec![load];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_passes_the_first_input_through() {
        let mut g = NodeGraphState::default();
        let cube = g.add_node("Cube".into(), NodeType::cube(2.0), egui::pos2(0.0, 0.0));
        let xf = g.add_node("Xf".into(), NodeType::Transform {
            translation: glam::Vec3::new(5.0, 0.0, 0.0),
            rotation: glam::Vec3::ZERO, scale: glam::Vec3::ONE,
        }, egui::pos2(0.0, 0.0));
        g.add_connection(cube, 0, xf, 0);
        let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: Option<&crate::core::geo::Geo>| m.clone();
        let max_x = |g: &NodeGraphState| g.eval_node(xf, &mut Default::default(), &pass).unwrap().into_mesh()
            .vertices.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
        assert!((max_x(&g) - 6.0).abs() < 1e-5);
        let before = g.content_hash();
        g.toggle_bypass(xf);
        assert_ne!(before, g.content_hash());
        assert!((max_x(&g) - 1.0).abs() < 1e-5);
        // A bypassed node with nothing wired gives nothing.
        g.toggle_bypass(cube);
        assert!(g.eval_node(cube, &mut Default::default(), &pass).is_none());
    }

    #[test]
    fn graph_survives_save_and_load() {
        let mut g = NodeGraphState::default();
        mocap_split_template(&mut g);
        let cube = g.add_node("Cube".into(), NodeType::cube(2.0), egui::pos2(1.0, 2.0));
        let xf = g.add_node("Xf".into(), NodeType::Transform {
            translation: glam::Vec3::new(1.0, 2.0, 3.0),
            rotation: glam::Vec3::ZERO, scale: glam::Vec3::ONE,
        }, egui::pos2(5.0, 6.0));
        g.add_connection(cube, 0, xf, 0);
        g.toggle_bypass(xf);
        // An Edit Poly node with an operation, a rule-based and a picked selection.
        use crate::core::poly::{ExtrudeMode, PolyOp, PolyOpKind, PolySelection, SelSource, SubLevel};
        let ep = g.add_node("EditPoly".into(), NodeType::EditPoly {
            ops: vec![
                PolyOp {
                    enabled: true,
                    selection: PolySelection { source: SelSource::ByNormal { dir: [0.0, 1.0, 0.0], angle: 5.0 }, grow: 1, ..Default::default() },
                    kind: PolyOpKind::Bevel { height: 0.5, outline: -0.1, mode: ExtrudeMode::LocalNormal },
                    collapsed: true,
                },
                PolyOp::new(
                    PolySelection { level: SubLevel::Border, ..Default::default() },
                    PolyOpKind::Transform { translate: [0.5, 0.0, 0.25], rotate: [0.0, 0.0, 0.0, 1.0], scale: [1.0, 2.0, 1.0], falloff: 0.0 }),
                PolyOp::new(Default::default(), PolyOpKind::MakePlanar { axis: None }),
            ],
            pending: PolySelection { level: SubLevel::Edge, edges: vec![[0, 1], [2, 3]], ..Default::default() },
            edit: Some(0),
            auto_collapse: true,
        }, egui::pos2(9.0, 9.0));
        g.add_connection(xf, 0, ep, 0);
        let json = to_json(&g);

        let mut h = NodeGraphState::default();
        from_json(&mut h, &json).unwrap();
        assert_eq!(h.nodes.len(), g.nodes.len());
        assert_eq!(h.connections.len(), g.connections.len());
        assert_eq!(h.view_flag, g.view_flag);
        assert!(h.nodes.iter().any(|n| n.bypassed));
        assert_eq!(to_json(&h), json);
        // Split keeps both outputs and their wires.
        let split = h.nodes.iter().find(|n| matches!(n.node_type, NodeType::SplitSkeleton { .. })).unwrap();
        assert_eq!(split.outputs.len(), 2);
        assert_eq!(h.connections.iter().filter(|c| c.from_node == split.id && c.from_output == 1).count(), 2);
        // New nodes do not collide with restored ids.
        let fresh = h.add_node("New".into(), NodeType::merge(), egui::pos2(0.0, 0.0));
        assert_eq!(h.nodes.iter().filter(|n| n.id == fresh).count(), 1);
    }

    /// What a Characterize node sets is saved with the graph and reaches
    /// the nodes after it.
    #[test]
    fn characterize_picks_are_saved_and_reach_the_clip() {
        use crate::core::human::{Human, Slot};
        let mut g = NodeGraphState::default();
        let take = g.add_node("Take".into(), NodeType::TestClip { seconds: 1.0, fps_num: 30, fps_den: 1 }, egui::pos2(0.0, 0.0));
        let who = g.add_node("Characterize".into(), NodeType::Characterize { picks: vec![(Slot::LeftHand, "Take01:LeftForeArm".into())] }, egui::pos2(0.0, 80.0));
        g.add_connection(take, 0, who, 0);
        let json = to_json(&g);
        let mut back = NodeGraphState::default();
        from_json(&mut back, &json).unwrap();
        let clip = back.eval_anim(who).unwrap();
        let h = Human::of(&clip);
        assert_eq!(h.get(Slot::LeftHand).map(|j| clip.joints[j].name.as_str()), Some("Take01:LeftForeArm"));
        assert_eq!(h.get(Slot::RightHand).map(|j| clip.joints[j].name.as_str()), Some("Take01:RightHand"));
        assert_eq!(h.picked, vec![Slot::LeftHand]);
    }

    /// Graphs saved when the example take and car were `.xmsclip` and
    /// `.xmsmesh` open on the FBX files that replaced them.
    #[test]
    fn old_example_files_open_as_fbx() {
        let mut g = NodeGraphState::default();
        g.add_node("Car".into(), NodeType::LoadFbx { path: "/home/x/examples/ragdoll/car.xmsmesh".into(), take: 0 }, egui::pos2(0.0, 0.0));
        g.add_node("Take".into(), NodeType::LoadFbx { path: "/home/x/examples/ragdoll/into_the_car.xmsclip".into(), take: 0 }, egui::pos2(0.0, 80.0));
        g.add_node("Other".into(), NodeType::LoadFbx { path: "/home/x/takes/car.fbx".into(), take: 0 }, egui::pos2(0.0, 160.0));
        let mut back = NodeGraphState::default();
        from_json(&mut back, &to_json(&g)).unwrap();
        let paths: Vec<String> = back.nodes.iter().filter_map(|n| match &n.node_type { NodeType::LoadFbx { path, .. } => Some(path.clone()), _ => None }).collect();
        assert_eq!(paths, ["/home/x/examples/ragdoll/car.fbx", "/home/x/examples/ragdoll/split/into_the_car_.fbx", "/home/x/takes/car.fbx"]);
    }

    /// Copied nodes come back through text (the clipboard) with their
    /// parameters, the wires between them and their ICE tree, as new nodes.
    #[test]
    fn copied_nodes_paste_with_their_wires_and_trees() {
        use crate::types::IceGraphNodeType;
        let mut subnets = IceGraphStore::default();
        let mut g = NodeGraphState::default();
        let cube = g.add_node("Cube".into(), NodeType::cube(2.0), egui::pos2(100.0, 0.0));
        let sid = subnets.create_subnet("Spherify".into());
        {
            // Get P → Normalize → Multiply (by 1.5) → Set P → Execution Ports.
            let sg = subnets.get_mut(sid).unwrap();
            let root = sg.root().unwrap();
            let p = sg.add_node("P".into(), IceGraphNodeType::get("P"), egui::pos2(0.0, 200.0));
            let norm = sg.add_node("Normalize".into(), IceGraphNodeType::Math { op: xms_ice::Math::Normalize }, egui::pos2(200.0, 200.0));
            let grow = sg.add_node("Multiply".into(), IceGraphNodeType::Math { op: xms_ice::Math::Multiply }, egui::pos2(300.0, 200.0));
            let set = sg.add_node("Set P".into(), IceGraphNodeType::set("P"), egui::pos2(400.0, 200.0));
            sg.nodes.iter_mut().find(|n| n.id == grow).unwrap().inputs[1].value = Some(xms_ice::Value::Float(1.5));
            sg.add_connection(p, 0, norm, 0);
            sg.add_connection(norm, 0, grow, 0);
            sg.add_connection(grow, 0, set, 0);
            sg.add_connection(set, 0, root, 0);
        }
        let ice = g.add_node("ICreationEngine".into(), NodeType::Ice { id: sid, name: "Spherify".into() }, egui::pos2(100.0, 100.0));
        g.add_connection(cube, 0, ice, 0);
        g.toggle_bypass(cube);
        g.set_view_flag(ice);

        let text = copy(&g, &subnets, &[cube, ice]).unwrap().to_text();
        let fragment = Fragment::from_text(&text).unwrap();
        assert_eq!(fragment.len(), 2);

        let placed = paste(&mut g, &mut subnets, fragment, Some(egui::pos2(500.0, 300.0)));
        let (new_cube, new_ice) = (placed[&cube.0], placed[&ice.0]);
        assert_eq!(g.nodes.len(), 4);
        assert_eq!(g.selected_nodes.len(), 2);
        let node = |id: NodeId| g.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(node(new_cube).position, egui::pos2(500.0, 300.0));
        assert_eq!(node(new_ice).position, egui::pos2(500.0, 400.0));
        assert!(node(new_cube).bypassed);
        assert_eq!(node(new_ice).inputs[0].connected_output, Some((new_cube, 0)));
        assert_eq!(g.view_flag, Some(ice), "pasting leaves the view where it was");
        // A tree of its own, with the same nodes and wires.
        let NodeType::Ice { id: new_sid, .. } = node(new_ice).node_type else { panic!() };
        assert_ne!(new_sid, sid);
        let (a, b) = (subnets.get(sid).unwrap(), subnets.get(new_sid).unwrap());
        assert_eq!(a.nodes.len(), b.nodes.len());
        assert_eq!(a.connections.len(), b.connections.len());
        let root = b.nodes.iter().find(|n| matches!(n.node_type, IceGraphNodeType::Root { .. })).unwrap();
        assert!(root.inputs[0].connected_output.is_some());
        assert_eq!(root.inputs.len(), 2, "its free port too");
        // The value on Multiply's B came along.
        let grow = b.nodes.iter().find(|n| n.node_type == IceGraphNodeType::Math { op: xms_ice::Math::Multiply }).unwrap();
        assert_eq!(grow.inputs[1].value, Some(xms_ice::Value::Float(1.5)));
        subnets.get_mut(sid).unwrap().nodes.retain(|n| n.node_type != IceGraphNodeType::Math { op: xms_ice::Math::Normalize });
        assert_eq!(subnets.get(new_sid).unwrap().nodes.len(), 5, "editing one tree leaves the other");
    }

    #[test]
    fn other_text_is_not_nodes() {
        assert!(Fragment::from_text("hello").is_none());
        assert!(Fragment::from_text(r#"{"version":1,"nodes":[],"connections":[],"view_flag":null}"#).is_none());
        let g = NodeGraphState::default();
        assert!(copy(&g, &IceGraphStore::default(), &[]).is_none(), "nothing picked copies nothing");
    }

    /// A graph added to another lands to the right of its nodes. Its view
    /// comes with it only when nothing was viewed.
    #[test]
    fn a_graph_added_sits_beside_and_brings_its_view_to_an_empty_graph() {
        let mut subnets = IceGraphStore::default();
        let mut other = NodeGraphState::default();
        mocap_split_template(&mut other);
        let cube = other.add_node("Cube".into(), NodeType::cube(1.0), egui::pos2(0.0, 0.0));
        other.set_view_flag(cube);
        let added = other.nodes.len();

        // Into an empty graph: the added graph's view comes with it.
        let mut e = NodeGraphState::default();
        assert_eq!(add_graph(&mut e, &mut subnets, &other), added);
        let viewed = e.nodes.iter().find(|n| Some(n.id) == e.view_flag).unwrap();
        assert!(matches!(viewed.node_type, NodeType::CreateCube { .. }));

        // Beside a viewed sphere: the sphere stays viewed.
        let mut g = NodeGraphState::default();
        let mine = g.add_node("Sphere".into(), NodeType::sphere(1.0, 8), egui::pos2(0.0, 0.0));
        assert_eq!(g.view_flag, Some(mine), "the first operator of a graph is viewed");
        assert_eq!(add_graph(&mut g, &mut subnets, &other), added);
        assert_eq!(g.nodes.len(), 1 + added);
        let right = g.nodes.iter().find(|n| n.id == mine).unwrap().position.x + crate::node_graph::ui::NODE_WIDTH;
        assert!(g.nodes.iter().filter(|n| g.selected_nodes.contains(&n.id)).all(|n| n.position.x > right));
        assert_eq!(g.view_flag, Some(mine));
    }

    /// One operator is always viewed: deleting it moves the view to what was
    /// wired into it; the flag clicked again moves to the next output.
    #[test]
    fn one_operator_is_always_viewed() {
        let mut g = NodeGraphState::default();
        assert_eq!(g.view_flag, None, "an empty graph views nothing");
        let cube = g.add_node("Cube".into(), NodeType::cube(1.0), egui::pos2(0.0, 0.0));
        let xf = g.add_node("Xf".into(), NodeType::Transform { translation: glam::Vec3::ZERO, rotation: glam::Vec3::ZERO, scale: glam::Vec3::ONE }, egui::pos2(0.0, 100.0));
        g.add_connection(cube, 0, xf, 0);
        assert_eq!(g.view_flag, Some(cube));
        g.set_view_flag(xf);
        g.set_view_flag(xf);
        assert_eq!((g.view_flag, g.view_output), (Some(xf), 0), "clicked again, one output: still viewed");
        g.selected_nodes = vec![xf];
        g.delete_selected();
        assert_eq!(g.view_flag, Some(cube), "the view goes to what came in");
        g.selected_nodes = vec![cube];
        g.delete_selected();
        assert_eq!(g.view_flag, None);

        // Several outputs: the flag clicked again goes round them.
        let fbx = g.add_node("Load".into(), NodeType::LoadFbx { path: String::new(), take: 0 }, egui::pos2(0.0, 0.0));
        assert_eq!(g.view_flag, Some(fbx));
        g.set_view_flag(fbx);
        assert_eq!(g.viewed_output(), 1);
        g.set_view_flag(fbx);
        g.set_view_flag(fbx);
        assert_eq!(g.viewed_output(), 0);
    }

    #[test]
    fn bad_json_leaves_graph_alone() {
        let mut g = NodeGraphState::default();
        mocap_split_template(&mut g);
        let before = to_json(&g);
        assert!(from_json(&mut g, "{ not json").is_err());
        assert!(from_json(&mut g, r#"{"version":1}"#).is_err());
        assert_eq!(to_json(&g), before);
    }

    /// A graph saved before Body Collide had its display and Transform took
    /// clips: Calamari folds into Body Collide, Transform Clip becomes
    /// Transform with the same rotation.
    /// A placeholder hands each input to the output of the same number,
    /// keeps its wires as its sockets are renamed or added, and goes into
    /// the file with its description.
    #[test]
    fn a_placeholder_passes_its_inputs_through_and_is_saved() {
        let mut g = NodeGraphState::default();
        let cube = g.add_node("Cube".into(), NodeType::cube(2.0), egui::pos2(0.0, 0.0));
        let walk = g.add_node("Walk".into(), NodeType::TestClip { seconds: 1.0, fps_num: 30, fps_den: 1 }, egui::pos2(200.0, 0.0));
        let ph = g.add_node("FootLocker".into(), NodeType::Placeholder {
            inputs: vec!["Mesh".into(), "Clip".into()], outputs: vec!["Mesh".into(), "Locked clip".into()],
            description: "Locks feet that should not slide.".into(),
        }, egui::pos2(100.0, 100.0));
        g.add_connection(cube, 0, ph, 0);
        g.add_connection(walk, 0, ph, 1);
        let pass = |_: IceGraphId, m: &crate::core::geo::Geo, _: Option<&crate::core::geo::Geo>| m.clone();
        let mesh = g.eval_node_out(ph, 0, &mut Default::default(), &pass).unwrap().into_mesh();
        assert_eq!(mesh.vertices.len(), 8);
        let clip = g.eval_anim_out(ph, 1).unwrap();
        assert!(std::sync::Arc::ptr_eq(&clip, &g.eval_anim(walk).unwrap()), "the very clip, unchanged");
        // Only the first input wired: the second output gives nothing.
        let c = g.connections.iter().find(|c| c.to_node == ph && c.to_input == 1).unwrap().id;
        g.remove_connection(c);
        assert!(g.eval_anim_out(ph, 1).is_none());
        g.add_connection(walk, 0, ph, 1);

        // Renamed and given a third input: the wires stay.
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == ph) {
            if let NodeType::Placeholder { inputs, .. } = &mut n.node_type { inputs[0] = "Body".into(); inputs.push("Ground".into()); }
        }
        g.sync_sockets(ph);
        let node = g.nodes.iter().find(|n| n.id == ph).unwrap();
        assert_eq!(node.inputs.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Body", "Clip", "Ground"]);
        assert_eq!(node.inputs[0].connected_output, Some((cube, 0)));
        assert_eq!(node.inputs[1].connected_output, Some((walk, 0)));
        // One output removed: wires from it go.
        let down = g.add_node("Trim".into(), NodeType::TrimClip { head: 0, tail: 0 }, egui::pos2(100.0, 200.0));
        g.add_connection(ph, 1, down, 0);
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == ph) {
            if let NodeType::Placeholder { outputs, .. } = &mut n.node_type { outputs.pop(); }
        }
        g.sync_sockets(ph);
        assert!(!g.connections.iter().any(|c| c.from_node == ph && c.from_output == 1));

        let json = to_json(&g);
        assert!(json.contains("Locks feet that should not slide."));
        let mut h = NodeGraphState::default();
        from_json(&mut h, &json).unwrap();
        let back = h.nodes.iter().find(|n| n.name == "FootLocker").unwrap();
        assert!(matches!(&back.node_type, NodeType::Placeholder { inputs, outputs, description }
            if inputs.len() == 3 && outputs == &vec!["Mesh".to_string()] && description.starts_with("Locks")));
        assert_eq!(back.inputs.len(), 3);
        assert_eq!(back.inputs[1].connected_output, Some((walk, 0)));
    }

    #[test]
    fn a_load_fbx_mesh_node_becomes_load_fbx() {
        let json = r#"{"version":1,"nodes":[
            {"id":0,"name":"Output","position":[0,500],"node_type":"Output"},
            {"id":2,"name":"Car","position":[0,0],"node_type":{"LoadFbxMesh":{"path":"car.fbx"}}},
            {"id":3,"name":"Move","position":[0,100],"node_type":{"Transform":{"translation":[0.0,0.0,0.0],"rotation":[0.0,0.0,0.0],"scale":[1.0,1.0,1.0]}}}
        ],"connections":[[2,0,3,0],[3,0,0,0]],"view_flag":null}"#;
        let mut g = NodeGraphState::default();
        from_json(&mut g, json).unwrap();
        let car = g.nodes.iter().find(|n| n.id == NodeId(2)).unwrap();
        assert!(matches!(&car.node_type, NodeType::LoadFbx { path, take: 0 } if path == "car.fbx"));
        assert_eq!(car.outputs.len(), 3);
        // Its wire leaves from the Geometry output now.
        let mv = g.nodes.iter().find(|n| n.id == NodeId(3)).unwrap();
        assert_eq!(mv.inputs[0].connected_output, Some((NodeId(2), 1)));
        // Output is gone; what was wired into it is viewed.
        assert_eq!(g.nodes.len(), 2);
        assert_eq!((g.view_flag, g.view_output), (Some(NodeId(3)), 0));
    }

    #[test]
    fn older_graphs_are_brought_up_to_date() {
        let json = r#"{"version":1,"nodes":[
            {"id":0,"name":"Output","position":[0,500],"node_type":"Output"},
            {"id":2,"name":"Take","position":[0,0],"node_type":{"TestClip":{"seconds":1.0,"fps_num":30,"fps_den":1}}},
            {"id":3,"name":"Turn","position":[0,50],"node_type":{"TransformClip":{"translate":[1.0,0.0,0.0],"rotate":[0.0,90.0,0.0],"scale":2.0}}},
            {"id":4,"name":"Ragdoll","position":[0,100],"node_type":{"Ragdoll":{"settings":{"margin":1.2,"friction":0.5,"self_collision":true,"self_slack":3.0,"stiffness":1.0,"release":60.0,"ghost_depth":12.0,"sink":0.0,"fade_out":6,"fade_in":10,"limb_limit":45.0,"smooth":2,"detail":1}}}},
            {"id":5,"name":"Hulls","position":[0,200],"node_type":{"Calamari":{"hulls":true,"detail":1}}}
        ],"connections":[[2,0,3,0],[3,0,4,0],[4,0,5,0],[5,0,0,0]],"view_flag":5}"#;
        let mut g = NodeGraphState::default();
        from_json(&mut g, json).unwrap();
        assert!(!g.nodes.iter().any(|n| matches!(n.node_type, NodeType::Calamari { .. } | NodeType::TransformClip { .. })));
        let rag = g.nodes.iter().find(|n| n.id == NodeId(4)).unwrap();
        assert!(matches!(rag.node_type, NodeType::Ragdoll { view: crate::types::BodyView::Hulls, .. }));
        assert_eq!(g.view_flag, Some(NodeId(4)), "viewed: Body Collide, which took over the old display node's view");
        assert_eq!(g.nodes.len(), 3, "Output and Calamari are gone");
        // Same motion as the old Transform Clip.
        let old = crate::core::anim::create_test_clip(1.0, crate::core::anim::FrameRate::new(30, 1))
            .transformed(glam::Vec3::new(1.0, 0.0, 0.0), glam::Vec3::new(0.0, 90.0, 0.0), 2.0);
        let new = g.eval_anim(NodeId(3)).unwrap();
        for f in [0, 11] {
            let (a, b) = (old.world_pose(f), new.world_pose(f));
            for j in 0..a.len() { assert!((a[j].w_axis - b[j].w_axis).length() < 1e-4); }
        }
    }

    #[test]
    fn graphs_from_before_primitive_locations_open() {
        // A cube without a location, a Merge without a mode, and the
        // Scatter Points and Copy to Points operators that are gone.
        let json = r#"{"version":1,"nodes":[
            {"id":0,"name":"Output","position":[0,500],"node_type":"Output"},
            {"id":1,"name":"Ground","position":[0,0],"node_type":{"CreateGrid":{"rows":4,"cols":4,"size":2.0}}},
            {"id":2,"name":"Scatter","position":[0,100],"node_type":{"ScatterPoints":{"count":10,"seed":1}}},
            {"id":3,"name":"Pebble","position":[200,0],"node_type":{"CreateCube":{"size":0.2}}},
            {"id":4,"name":"Copy","position":[0,200],"node_type":"CopyToPoints"},
            {"id":5,"name":"Merge","position":[0,300],"node_type":"Merge"}
        ],"connections":[[1,0,2,0],[3,0,4,0],[2,0,4,1],[4,0,5,0],[1,0,5,1],[5,0,0,0]],"view_flag":null}"#;
        let mut g = NodeGraphState::default();
        from_json(&mut g, json).unwrap();
        let node = |id: usize| g.nodes.iter().find(|n| n.id == NodeId(id)).unwrap();
        assert_eq!(g.view_flag, Some(NodeId(5)), "what reached Output is viewed");
        assert!(matches!(&node(3).node_type, NodeType::CreateCube { location, .. } if location == "/cube"));
        assert!(matches!(&node(1).node_type, NodeType::CreateGrid { location, .. } if location == "/grid"));
        assert!(matches!(node(5).node_type, NodeType::Merge { mode: crate::types::MergeMode::Auto, .. }));
        assert!(matches!(&node(2).node_type, NodeType::Placeholder { inputs, outputs, description }
            if inputs == &["Surface"] && outputs == &["Points"] && description.contains("Scatter Points")));
        assert!(matches!(&node(4).node_type, NodeType::Placeholder { inputs, .. } if inputs.len() == 2));
        // Every wire is still there.
        assert_eq!(node(4).inputs[1].connected_output, Some((NodeId(2), 0)));
        assert_eq!(node(5).inputs[0].connected_output, Some((NodeId(4), 0)));
        // Saved again, it reads back the same.
        let mut again = NodeGraphState::default();
        from_json(&mut again, &to_json(&g)).unwrap();
        assert_eq!(again.nodes.len(), g.nodes.len());
    }

    /// A graph from before the view flag was always set: an Output wired from
    /// Load FBX's Geometry output, nothing flagged. That output is viewed.
    #[test]
    fn output_wired_from_a_second_output_keeps_that_output_viewed() {
        let json = r#"{"version":1,"nodes":[
            {"id":0,"name":"Output","position":[0,500],"node_type":"Output"},
            {"id":1,"name":"Studio","position":[0,0],"node_type":{"LoadFbx":{"path":"studio.fbx","take":0}}}
        ],"connections":[[1,1,0,0]],"view_flag":null}"#;
        let mut g = NodeGraphState::default();
        from_json(&mut g, json).unwrap();
        assert_eq!(g.nodes.len(), 1);
        assert_eq!((g.view_flag, g.viewed_output()), (Some(NodeId(1)), 1));
        // Saved again, the output viewed is kept.
        let mut h = NodeGraphState::default();
        from_json(&mut h, &to_json(&g)).unwrap();
        assert_eq!((h.view_flag, h.viewed_output()), (Some(NodeId(1)), 1));
    }

    /// An operator's Prims Location tab: it works on what the list matches,
    /// the operators after it do not inherit the list, and the list is saved.
    #[test]
    fn an_operator_works_on_its_primitive_locations() {
        let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: Option<&crate::core::geo::Geo>| m.clone();
        let mut g = NodeGraphState::default();
        let a = g.add_node("A".into(), NodeType::CreateCube { size: 1.0, location: "/set/a".into() }, egui::pos2(0.0, 0.0));
        let b = g.add_node("B".into(), NodeType::CreateCube { size: 1.0, location: "/set/b".into() }, egui::pos2(200.0, 0.0));
        let merge = g.add_node("Merge".into(), NodeType::merge(), egui::pos2(100.0, 100.0));
        let lift = g.add_node("Lift".into(), NodeType::Transform { translation: glam::Vec3::new(0.0, 10.0, 0.0), rotation: glam::Vec3::ZERO, scale: glam::Vec3::ONE }, egui::pos2(100.0, 200.0));
        let after = g.add_node("After".into(), NodeType::Transform { translation: glam::Vec3::new(5.0, 0.0, 0.0), rotation: glam::Vec3::ZERO, scale: glam::Vec3::ONE }, egui::pos2(100.0, 300.0));
        g.add_connection(a, 0, merge, 0);
        g.add_connection(b, 0, merge, 1);
        g.add_connection(merge, 0, lift, 0);
        g.add_connection(lift, 0, after, 0);
        g.nodes.iter_mut().find(|n| n.id == lift).unwrap().locations = vec!["/set/b".into()];

        let prims = |g: &NodeGraphState, id| match g.eval_node(id, &mut Default::default(), &pass).unwrap() {
            crate::types::EvalResult::Named(p) => p, _ => panic!("primitives"),
        };
        let low = |p: &crate::types::Prim| p.mesh.vertices.iter().map(|v| v[1]).fold(f32::MAX, f32::min);
        let left = |p: &crate::types::Prim| p.mesh.vertices.iter().map(|v| v[0]).fold(f32::MAX, f32::min);
        let lifted = prims(&g, lift);
        assert!(low(&lifted[0]) < 0.0 && low(&lifted[1]) > 9.0, "only /set/b is lifted");
        assert!(lifted.iter().all(|p| !p.picked), "the list does not leave the operator");
        // The next operator, with no list, works on everything.
        let moved = prims(&g, after);
        assert!(moved.iter().all(|p| left(p) > 4.0));

        // Prune by the list.
        let prune = g.add_node("Prune".into(), NodeType::PrunePrims { pattern: String::new(), keep: false }, egui::pos2(300.0, 300.0));
        g.add_connection(lift, 0, prune, 0);
        assert_eq!(prims(&g, prune).len(), 2, "an empty list prunes nothing");
        g.nodes.iter_mut().find(|n| n.id == prune).unwrap().locations = vec!["/set/a".into()];
        assert_eq!(prims(&g, prune).iter().map(|p| p.path.as_str()).collect::<Vec<_>>(), vec!["/set/b"]);

        // Saved and opened: the lists are there; a node without one writes none.
        let json = to_json(&g);
        assert_eq!(json.matches("\"locations\"").count(), 2);
        let mut h = NodeGraphState::default();
        from_json(&mut h, &json).unwrap();
        assert_eq!(h.nodes.iter().find(|n| n.id == lift).unwrap().locations, vec!["/set/b".to_string()]);
        // Copied and pasted: the list comes along.
        let text = copy(&g, &IceGraphStore::default(), &[lift]).unwrap().to_text();
        let placed = paste(&mut h, &mut IceGraphStore::default(), Fragment::from_text(&text).unwrap(), None);
        assert_eq!(h.nodes.iter().find(|n| n.id == placed[&lift.0]).unwrap().locations, vec!["/set/b".to_string()]);
    }

    /// Set Attribute: its rows set values on the prims its list names, and
    /// are saved with the graph.
    #[test]
    fn set_attribute_sets_its_rows_on_the_prims_it_names() {
        use crate::core::properties::{SetContext, Value};
        use crate::core::set_attribute::SetRow;
        let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: Option<&crate::core::geo::Geo>| m.clone();
        let mut g = NodeGraphState::default();
        let a = g.add_node("A".into(), NodeType::CreateCube { size: 1.0, location: "/set/a".into() }, egui::pos2(0.0, 0.0));
        let b = g.add_node("B".into(), NodeType::CreateCube { size: 1.0, location: "/set/b".into() }, egui::pos2(200.0, 0.0));
        let merge = g.add_node("Merge".into(), NodeType::merge(), egui::pos2(100.0, 100.0));
        let rows = vec![
            SetRow { context: SetContext::Object, name: "displayColor".into(), value: Value::Color3f([0.0, 1.0, 0.0]) },
            SetRow { context: SetContext::Property, name: "purpose".into(), value: Value::Token("proxy".into()) },
        ];
        let set = g.add_node("Set".into(), NodeType::SetAttribute { rows }, egui::pos2(100.0, 200.0));
        g.add_connection(a, 0, merge, 0);
        g.add_connection(b, 0, merge, 1);
        g.add_connection(merge, 0, set, 0);
        let prims = |g: &NodeGraphState| match g.eval_node(set, &mut Default::default(), &pass).unwrap() {
            crate::types::EvalResult::Named(p) => p, _ => panic!("primitives"),
        };
        let coloured = |p: &crate::types::Prim| p.geo.as_ref().is_some_and(|g| g.attr(crate::core::geo::Context::Object, "displayColor").is_some());
        // No list: every primitive.
        assert!(prims(&g).iter().all(|p| coloured(p) && p.purpose == crate::types::Purpose::Proxy));
        // `/set` names the Xform above both cubes: set there, inherited by both.
        g.nodes.iter_mut().find(|n| n.id == set).unwrap().locations = vec!["/set".into()];
        let out = prims(&g);
        assert!(out.iter().all(|p| !coloured(p) && p.above.contains_key("/set")));
        assert!(out.iter().all(|p| crate::core::properties::inherited_primvars(p).len() == 1 && crate::core::properties::purpose_of(p) == crate::types::Purpose::Proxy));
        g.nodes.iter_mut().find(|n| n.id == set).unwrap().locations = vec!["/set/b".into()];
        let out = prims(&g);
        assert!(!coloured(&out[0]) && coloured(&out[1]));

        // Saved and opened: the rows and the list are there.
        let mut h = NodeGraphState::default();
        from_json(&mut h, &to_json(&g)).unwrap();
        let node = h.nodes.iter().find(|n| n.id == set).unwrap();
        assert_eq!(node.locations, vec!["/set/b".to_string()]);
        let NodeType::SetAttribute { rows } = &node.node_type else { panic!("Set Attribute") };
        assert_eq!((rows.len(), rows[1].value.clone()), (2, Value::Token("proxy".into())));
    }

    /// A root saved under an old name (ICE Tree, ICEExectPorts) is named
    /// ExecutionPorts when read; any other node keeps its name.
    #[test]
    fn an_ice_root_saved_under_an_old_name_is_renamed() {
        let root = serde_json::to_value(crate::types::IceGraphNodeType::Root { context: crate::core::geo::Context::Point }).unwrap();
        let get = serde_json::to_value(crate::types::IceGraphNodeType::get("P")).unwrap();
        let node = |id: usize, name: &str, t: &serde_json::Value| SavedTreeNode { id, name: name.into(), position: [0.0, 0.0], node_type: t.clone(), values: vec![] };
        for old in ["ICE Tree", "ICEExectPorts"] {
            let t = SavedTree { node: 0, name: "Spherify".into(), nodes: vec![node(0, old, &root), node(1, "ICEExectPorts", &get)], connections: vec![] };
            let mut subnets = IceGraphStore::default();
            let sid = restore_tree(&t, "Spherify", &mut subnets);
            let names: Vec<String> = subnets.get(sid).unwrap().nodes.iter().map(|n| n.name.clone()).collect();
            assert_eq!(names, vec!["ExecutionPorts".to_string(), "ICEExectPorts".to_string()], "{old}");
        }
    }
}
