//! Compounds in the scene graph (see `docs/compounds.md`): the Compound
//! operator, which cooks the graph of its definition, and the Compound Input
//! and Compound Output operators inside it.
//!
//! - **Cooking an instance:** the graph of its definition (the library's, or
//!   its own once editable) is read once per definition and kept
//!   (`inner_of`). Each cook takes a copy of it with the instance's promoted
//!   values put at their targets and what comes into the instance's ports
//!   (`Fed`), and cooks its Compound Output. The copy shares the outer graph's
//!   cache of cooked results, which is keyed by content, so two instances with
//!   the same values and inputs cook once.
//! - **Variables:** inside, `$OPNAME` is each operator's own name; every
//!   operator inside reads `{instance}`, the instance's name, and the `{name}`
//!   variables that come down the wires to the instance.
//! - **Ports** are the definition's, kept on the instance as last seen; when
//!   they change, wires follow them by name, then by number (`rewire`).
//! - **Make Compound** packs operators into an editable instance, the wires
//!   crossing the selection becoming its ports.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Arc;

use super::{EvalCache, GraphNode, NodeGraphState};
use crate::compounds::{self, Context, Definition, Port};
use crate::ice::IceGraphStore;
use crate::types::{EvalResult, IceGraphId, NodeId, NodeType};

/// What comes into a compound instance, for the graph inside it: each input
/// port's result and its cook key, by the port's name; and the `{name}`
/// variables every operator inside reads, `{instance}` first.
#[derive(Clone, Default)]
pub struct Fed {
    pub ports: Vec<FedPort>,
    pub above: Vec<(String, String)>,
}

/// What comes into one input port of a compound instance.
#[derive(Clone)]
pub struct FedPort {
    pub name: String,
    pub result: Option<EvalResult>,
    /// The cook key of what is wired there; None when nothing is, or when it
    /// cannot be keyed.
    pub key: Option<u64>,
}

impl Fed {
    /// The input port of a name.
    pub fn port(&self, name: &str) -> Option<&FedPort> {
        self.ports.iter().find(|p| p.name == name)
    }
}

/// For a graph inside a compound while it is looked at: the instances entered
/// to reach it, whether it is locked (read-only), and what the Node Graph pane
/// asked for, carried out by `inside_compound` after the frame.
#[derive(Clone, Default)]
pub struct Inside {
    /// The names of the instances entered, the outermost first.
    pub path: Vec<String>,
    pub locked: bool,
    pub request: Option<Request>,
    /// The instance's promoted parameters, changed by Promote inside; the
    /// definition takes them when an editable instance is left.
    pub parameters: Vec<compounds::Parameter>,
}

/// What the panes ask of the compounds entered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// Go inside this instance.
    Enter(NodeId),
    /// Back out to the graph the instance is in.
    Leave,
    /// Back out to the root scene graph.
    LeaveAll,
    /// Make the instance being looked at editable, from inside it.
    MakeEditable,
}

/// A definition's graph as cooked: its operators, and their ICE graphs.
pub struct Inner {
    pub graph: NodeGraphState,
    pub subnets: IceGraphStore,
}

thread_local! {
    /// Definitions' graphs read so far, by the definition's hash.
    static INNER: RefCell<HashMap<u64, Rc<Inner>>> = RefCell::new(HashMap::new());
    /// The definitions being cooked, outermost first: for the depth limit and
    /// to stop a definition cooking itself.
    static COOKING: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// While it lives, definition `name` is being cooked (see `COOKING`).
struct Cooking(());

impl Cooking {
    /// None when compounds are already `compounds::MAX_DEPTH` deep, or `name`
    /// is being cooked further out: it would cook itself.
    fn start(name: &str) -> Option<Cooking> {
        COOKING.with(|c| {
            let mut c = c.borrow_mut();
            if c.len() >= compounds::MAX_DEPTH || c.iter().any(|n| n == name) {
                return None;
            }
            c.push(name.to_string());
            Some(Cooking(()))
        })
    }
}

impl Drop for Cooking {
    fn drop(&mut self) {
        COOKING.with(|c| {
            c.borrow_mut().pop();
        });
    }
}

/// `f` of a compound instance's definition: its own when made editable, else
/// the library's of its name. None for another operator, or a definition that
/// cannot be found (or is not a scene compound).
pub fn with_definition<R>(t: &NodeType, f: impl FnOnce(&Definition) -> R) -> Option<R> {
    match t {
        NodeType::Compound { editable: Some(d), .. } => Some(f(d)),
        NodeType::Compound { definition, .. } => {
            let library = compounds::library();
            let found = library.for_context(definition, Context::Scene).ok()?;
            Some(f(&found.definition))
        }
        _ => None,
    }
}

/// A definition's graph, read into operators and ICE graphs.
pub fn read_inner(d: &Definition) -> Result<Inner, String> {
    let mut graph = NodeGraphState::default();
    let mut subnets = IceGraphStore::default();
    if !d.graph.is_null() {
        crate::graph_io::from_json(&mut graph, &mut subnets, &d.graph.to_string())?;
    }
    Ok(Inner { graph, subnets })
}

/// A definition's graph, read the first time it is asked for and kept.
// TODO(perf): the hash writes the definition out on every call; key by the
// definition's place instead when compounds are large.
fn inner_of(d: &Definition) -> Option<Rc<Inner>> {
    let hash = d.hash();
    if let Some(i) = INNER.with(|m| m.borrow().get(&hash).cloned()) {
        return Some(i);
    }
    let inner = Rc::new(read_inner(d).ok()?);
    INNER.with(|m| m.borrow_mut().insert(hash, inner.clone()));
    Some(inner)
}

/// Put each promoted parameter's value (the instance's, else its default) at
/// its targets in `g`.
pub fn apply_values(g: &mut NodeGraphState, d: &Definition, values: &BTreeMap<String, serde_json::Value>) {
    for p in &d.parameters {
        let value = values.get(&p.name).unwrap_or(&p.default);
        for t in &p.targets {
            let Some(node) = g.nodes.iter_mut().find(|n| n.name == t.node) else {
                continue;
            };
            let Ok(mut v) = serde_json::to_value(&node.node_type) else {
                continue;
            };
            if compounds::put(&mut v, &t.path, value.clone()) {
                if let Ok(changed) = serde_json::from_value(v) {
                    node.node_type = changed;
                }
            }
        }
    }
}

/// The promoted values `g` holds at each parameter's first target, kept in
/// `values` where they differ from the default; then every target set back to
/// its default, so the definition keeps its defaults.
pub fn take_values(g: &mut NodeGraphState, d: &Definition, values: &mut BTreeMap<String, serde_json::Value>) {
    for p in &d.parameters {
        let now = p.targets.first().and_then(|t| {
            let node = g.nodes.iter().find(|n| n.name == t.node)?;
            let v = serde_json::to_value(&node.node_type).ok()?;
            compounds::get(&v, &t.path).cloned()
        });
        match now {
            Some(v) if v != p.default => {
                values.insert(p.name.clone(), v);
            }
            Some(_) => {
                values.remove(&p.name);
            }
            None => {}
        }
    }
    let defaults: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    apply_values(g, d, &defaults);
}

/// The ports a graph inside a compound has: its Compound Input and Compound
/// Output operators' names, in the order of `before` (the definition's ports
/// as they were), then the new ones from left to right.
pub fn ports_of(g: &NodeGraphState, before: (&[Port], &[Port])) -> (Vec<Port>, Vec<Port>) {
    let named = |input: bool| -> Vec<(String, f32)> {
        let mut v: Vec<(String, f32)> = g
            .nodes
            .iter()
            .filter_map(|n| match &n.node_type {
                NodeType::CompoundInput { name } if input => Some((name.clone(), n.position.x)),
                NodeType::CompoundOutput { name } if !input => Some((name.clone(), n.position.x)),
                _ => None,
            })
            .collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1));
        v.dedup_by(|a, b| a.0 == b.0);
        v
    };
    let order = |now: Vec<(String, f32)>, was: &[Port]| -> Vec<Port> {
        let mut out: Vec<Port> = was
            .iter()
            .filter(|p| now.iter().any(|(n, _)| *n == p.name))
            .cloned()
            .collect();
        for (name, _) in now {
            if !out.iter().any(|p| p.name == name) {
                out.push(Port {
                    name,
                    kind: None,
                    value: None,
                });
            }
        }
        out
    };
    (order(named(true), before.0), order(named(false), before.1))
}

/// For each old port, the new port it goes to: the one of its name, else the
/// one of its number when that is not taken.
fn remap(old: &[String], new: &[String]) -> Vec<Option<usize>> {
    let mut taken = vec![false; new.len()];
    let mut out = vec![None; old.len()];
    for (i, name) in old.iter().enumerate() {
        if let Some(j) = new.iter().position(|n| n == name).filter(|j| !taken[*j]) {
            out[i] = Some(j);
            taken[j] = true;
        }
    }
    for (i, o) in out.iter_mut().enumerate() {
        if o.is_none() && i < new.len() && !taken[i] {
            *o = Some(i);
            taken[i] = true;
        }
    }
    out
}

/// Ports named `base`, `base2`, `base3`… none taken twice.
fn unique(base: &str, taken: &[String]) -> String {
    (1..)
        .map(|k| if k == 1 { base.to_string() } else { format!("{base}{k}") })
        .find(|n| !taken.contains(n))
        .unwrap_or_default()
}

impl NodeGraphState {
    /// Cook output `output` of compound instance `node`: its definition's
    /// graph, fed what comes into its ports. Its definition missing, or
    /// cooking itself, what comes into its first input goes on.
    pub(crate) fn cook_compound(
        &self,
        node: &GraphNode,
        node_type: &NodeType,
        output: usize,
        cache: &mut EvalCache,
        eval_subnet: &impl Fn(IceGraphId, &crate::core::geo::Geo, &[crate::ice::Reference]) -> crate::core::geo::Geo,
    ) -> Option<EvalResult> {
        let NodeType::Compound { values, .. } = node_type else {
            return None;
        };
        let ports: Vec<FedPort> = node
            .inputs
            .iter()
            .map(|s| match s.connected_output {
                Some((src, out)) => FedPort {
                    name: s.name.clone(),
                    result: self.eval_node_out(src, out, cache, eval_subnet),
                    key: self.cook_key(src, out),
                },
                None => FedPort {
                    name: s.name.clone(),
                    result: None,
                    key: None,
                },
            })
            .collect();
        let first = ports.first().and_then(|p| p.result.clone());
        let Some((inner, name, mut g)) = with_definition(node_type, |d| {
            let inner = inner_of(d)?;
            let mut g = inner.graph.clone();
            apply_values(&mut g, d, values);
            Some((inner, d.name.clone(), g))
        })
        .flatten() else {
            return first;
        };
        let Some(_cooking) = Cooking::start(&name) else {
            return first;
        };
        g.cooked = self.cooked.clone();
        g.time = self.time;
        g.eval_time = self.eval_time.clone();
        g.scene = self.scene;
        g.variables = self.variables.clone();
        let mut above: Vec<(String, String)> = vec![("instance".into(), node.name.clone())];
        above.extend(
            self.from_above(node.id)
                .into_iter()
                .filter(|a| a.name != "instance")
                .map(|a| (a.name, a.value)),
        );
        g.fed = Some(Arc::new(Fed { ports, above }));
        let port = &node.outputs.get(output)?.name;
        let out = g
            .nodes
            .iter()
            .find(|n| matches!(&n.node_type, NodeType::CompoundOutput { name } if name == port))?
            .id;
        let subnets = &inner.subnets;
        let eval_inner = |sid: IceGraphId, mesh: &crate::core::geo::Geo, refs: &[crate::ice::Reference]| {
            subnets
                .get(sid)
                .map(|sg| sg.evaluate(mesh, refs))
                .unwrap_or_else(|| mesh.clone())
        };
        // One type for the ICE graphs of every compound inside this one.
        let eval_inner: &dyn Fn(IceGraphId, &crate::core::geo::Geo, &[crate::ice::Reference]) -> crate::core::geo::Geo =
            &eval_inner;
        let mut inner_cache = EvalCache::new();
        g.eval_node_out(out, 0, &mut inner_cache, &eval_inner)
    }

    /// What a Compound Input operator gives: what comes into the instance's
    /// port of its name.
    pub(crate) fn fed_port(&self, name: &str) -> Option<EvalResult> {
        self.fed.as_ref()?.port(name)?.result.clone()
    }

    /// Bring every compound instance up to date with its definition: its
    /// ports (wires following them), version and hash; values of parameters it
    /// no longer has dropped. Returns what changed, to tell.
    pub fn refresh_compounds(&mut self) -> Vec<String> {
        let mut told = vec![];
        let ids: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|n| matches!(n.node_type, NodeType::Compound { .. }))
            .map(|n| n.id)
            .collect();
        for id in ids {
            let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
                continue;
            };
            let Some((ins, outs, version, hash, names)) = with_definition(&node.node_type, |d| {
                (
                    d.inputs.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                    d.outputs.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                    d.version,
                    d.hash(),
                    d.parameters.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                )
            }) else {
                continue;
            };
            let name = node.name.clone();
            let mut ports_changed = false;
            if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                if let NodeType::Compound {
                    version: v,
                    hash: h,
                    inputs,
                    outputs,
                    values,
                    definition,
                    editable,
                } = &mut node.node_type
                {
                    if editable.is_none() && *h != 0 && (*v != version || *h != hash) {
                        told.push(if *v != version {
                            format!("{name}: {definition} updated from v{v} to v{version}")
                        } else {
                            format!("{name}: its definition {definition} changed")
                        });
                    }
                    let dropped: Vec<String> = values.keys().filter(|k| !names.contains(k)).cloned().collect();
                    for k in &dropped {
                        values.remove(k);
                    }
                    if !dropped.is_empty() {
                        told.push(format!("{name}: its definition no longer has {}", dropped.join(", ")));
                    }
                    *v = version;
                    *h = hash;
                    ports_changed = *inputs != ins || *outputs != outs;
                }
            }
            if ports_changed {
                self.rewire(id, ins, outs);
            }
        }
        told
    }

    /// Give compound instance `id` these ports, its wires following them by
    /// name, then by number; a wire whose port is gone is taken off.
    pub fn rewire(&mut self, id: NodeId, ins: Vec<String>, outs: Vec<String>) {
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            return;
        };
        let old_in: Vec<String> = node.inputs.iter().map(|s| s.name.clone()).collect();
        let old_out: Vec<String> = node.outputs.iter().map(|s| s.name.clone()).collect();
        let (map_in, map_out) = (remap(&old_in, &ins), remap(&old_out, &outs));
        let wires: Vec<super::Connection> = self
            .connections
            .iter()
            .filter(|c| c.to_node == id || c.from_node == id)
            .cloned()
            .collect();
        for c in &wires {
            self.remove_connection(c.id);
        }
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
            if let NodeType::Compound { inputs, outputs, .. } = &mut node.node_type {
                *inputs = ins;
                *outputs = outs;
            }
            let (i, o) = Self::create_sockets(&node.node_type);
            node.inputs = i;
            node.outputs = o;
        }
        for c in wires {
            let from_out = if c.from_node == id {
                map_out.get(c.from_output).copied().flatten()
            } else {
                Some(c.from_output)
            };
            let to_in = if c.to_node == id {
                map_in.get(c.to_input).copied().flatten()
            } else {
                Some(c.to_input)
            };
            if let (Some(o), Some(i)) = (from_out, to_in) {
                self.add_connection(c.from_node, o, c.to_node, i);
            }
        }
        if self.view_flag == Some(id) {
            let n = self.nodes.iter().find(|n| n.id == id).map_or(0, |n| n.outputs.len());
            self.view_output = self.view_output.min(n.saturating_sub(1));
        }
        self.mark_dirty();
    }

    /// Write editable instance `id`'s definition to `path`, a version after
    /// the library's of its name. In a library folder, the instance then
    /// follows the file, locked, and other instances of the name are brought
    /// up to date; elsewhere it stays editable. Returns what to tell.
    pub fn save_definition(&mut self, id: NodeId, path: &std::path::Path) -> Result<String, String> {
        let node = self
            .nodes
            .iter_mut()
            .find(|n| n.id == id)
            .ok_or("That operator is gone")?;
        let NodeType::Compound {
            editable,
            definition,
            version,
            hash,
            ..
        } = &mut node.node_type
        else {
            return Err("Not a compound".into());
        };
        let d = editable
            .as_deref_mut()
            .ok_or("Make it editable first: a locked compound is its library's")?;
        if d.name.trim().is_empty() {
            return Err("Give the definition a name first".into());
        }
        if let Some(f) = compounds::library().get(&d.name) {
            d.version = d.version.max(f.definition.version + 1);
        }
        d.write(path)?;
        let same = |a: &std::path::Path| a.canonicalize().ok() == path.canonicalize().ok();
        let in_library = compounds::rescan().get(&d.name).is_some_and(|f| same(&f.path));
        let (name, v) = (d.name.clone(), d.version);
        *definition = name.clone();
        *version = v;
        *hash = d.hash();
        let mut told = if in_library {
            *editable = None;
            format!(
                "Saved {name} v{v} to {}: this instance follows it, locked",
                path.display()
            )
        } else {
            format!(
                "Saved {name} v{v} to {}: not in a library folder, so this instance keeps its own copy",
                path.display()
            )
        };
        for m in self.refresh_compounds() {
            told.push_str(". ");
            told.push_str(&m);
        }
        Ok(told)
    }

    /// Pack operators `ids` into a compound: an editable instance in their
    /// place, its graph the operators with their wires, a Compound Input for
    /// each output wired into them from outside and a Compound Output for each
    /// of theirs wired out (the viewed one's, or the last's, when none is).
    /// The instance is selected.
    pub fn make_compound(&mut self, subnets: &mut IceGraphStore, ids: &[NodeId], name: &str) -> Result<NodeId, String> {
        let picked: Vec<NodeId> = self.nodes.iter().map(|n| n.id).filter(|id| ids.contains(id)).collect();
        if picked.is_empty() {
            return Err("Select the operators to make a compound of".into());
        }
        if self.nodes.iter().any(|n| {
            picked.contains(&n.id)
                && matches!(
                    n.node_type,
                    NodeType::CompoundInput { .. } | NodeType::CompoundOutput { .. }
                )
        }) {
            return Err("Compound Input and Compound Output operators stay where they are".into());
        }
        let inside = |id: NodeId| picked.contains(&id);
        let fragment = crate::graph_io::copy(self, subnets, &picked).ok_or("Nothing to make a compound of")?;
        let corner = self
            .nodes
            .iter()
            .filter(|n| inside(n.id))
            .fold(egui::pos2(f32::MAX, f32::MAX), |c, n| {
                egui::pos2(c.x.min(n.position.x), c.y.min(n.position.y))
            });
        let bottom = self
            .nodes
            .iter()
            .filter(|n| inside(n.id))
            .map(|n| n.position.y)
            .fold(f32::MIN, f32::max);
        let mut g = NodeGraphState::default();
        let mut inner_subnets = IceGraphStore::default();
        let placed = crate::graph_io::paste(&mut g, &mut inner_subnets, fragment, Some(corner));
        let new = |id: NodeId| placed.get(&id.0).copied();
        // References to operators left outside cannot reach in.
        let kept: Vec<NodeId> = placed.values().copied().collect();
        for n in &mut g.nodes {
            n.links.retain(|l| kept.contains(&l.node));
        }

        // Wires coming in, by what they come from: one port each.
        let mut ins: Vec<((NodeId, usize), Vec<(NodeId, usize)>, String)> = vec![];
        for c in self
            .connections
            .iter()
            .filter(|c| inside(c.to_node) && !inside(c.from_node))
        {
            let socket = self
                .nodes
                .iter()
                .find(|n| n.id == c.to_node)
                .and_then(|n| n.inputs.get(c.to_input))
                .map_or("Input".to_string(), |s| s.name.clone());
            match ins.iter_mut().find(|i| i.0 == (c.from_node, c.from_output)) {
                Some(i) => i.1.push((c.to_node, c.to_input)),
                None => ins.push(((c.from_node, c.from_output), vec![(c.to_node, c.to_input)], socket)),
            }
        }
        // Wires going out, by what they come from: one port each.
        let mut outs: Vec<((NodeId, usize), Vec<(NodeId, usize)>, String)> = vec![];
        for c in self
            .connections
            .iter()
            .filter(|c| inside(c.from_node) && !inside(c.to_node))
        {
            let socket = self
                .nodes
                .iter()
                .find(|n| n.id == c.from_node)
                .and_then(|n| n.outputs.get(c.from_output))
                .map_or("Output".to_string(), |s| s.name.clone());
            match outs.iter_mut().find(|o| o.0 == (c.from_node, c.from_output)) {
                Some(o) => o.1.push((c.to_node, c.to_input)),
                None => outs.push(((c.from_node, c.from_output), vec![(c.to_node, c.to_input)], socket)),
            }
        }
        if outs.is_empty() {
            let last = self.view_flag.filter(|v| inside(*v)).or_else(|| {
                self.nodes
                    .iter()
                    .filter(|n| inside(n.id) && !n.outputs.is_empty())
                    .max_by(|a, b| a.position.y.total_cmp(&b.position.y))
                    .map(|n| n.id)
            });
            if let Some(last) = last {
                let out = if self.view_flag == Some(last) {
                    self.view_output
                } else {
                    0
                };
                let socket = self
                    .nodes
                    .iter()
                    .find(|n| n.id == last)
                    .and_then(|n| n.outputs.get(out))
                    .map_or("Output".into(), |s| s.name.clone());
                outs.push(((last, out), vec![], socket));
            }
        }

        // The ports inside: names made unique, the operators placed above
        // and below what they feed and take.
        let mut in_names: Vec<String> = vec![];
        for ((_, _), to, socket) in &ins {
            let port = unique(socket, &in_names);
            in_names.push(port.clone());
            let at = to
                .iter()
                .filter_map(|(n, _)| new(*n))
                .filter_map(|n| g.nodes.iter().find(|x| x.id == n))
                .map(|n| n.position)
                .next()
                .unwrap_or(corner);
            let input = g.add_node(
                port.clone(),
                NodeType::CompoundInput { name: port.clone() },
                at - egui::vec2(0.0, 110.0),
            );
            for (n, k) in to {
                if let Some(n) = new(*n) {
                    g.add_connection(input, 0, n, *k);
                }
            }
        }
        let mut out_names: Vec<String> = vec![];
        let mut first_out = None;
        for ((from, k), _, socket) in &outs {
            let port = unique(socket, &out_names);
            out_names.push(port.clone());
            let Some(src) = new(*from) else { continue };
            let at = g
                .nodes
                .iter()
                .find(|n| n.id == src)
                .map_or(egui::pos2(corner.x, bottom), |n| n.position);
            let output = g.add_node(
                port.clone(),
                NodeType::CompoundOutput { name: port.clone() },
                at + egui::vec2(0.0, 110.0),
            );
            g.add_connection(src, *k, output, 0);
            first_out.get_or_insert(output);
        }
        if let Some(o) = first_out {
            g.set_view_flag(o);
        }

        let mut d = Definition::new(Context::Scene, name);
        d.inputs = in_names
            .iter()
            .map(|n| Port {
                name: n.clone(),
                kind: None,
                value: None,
            })
            .collect();
        d.outputs = out_names
            .iter()
            .map(|n| Port {
                name: n.clone(),
                kind: None,
                value: None,
            })
            .collect();
        d.graph = serde_json::from_str(&crate::graph_io::to_json(&g, &inner_subnets)).map_err(|e| e.to_string())?;

        // The instance in their place, wired as they were.
        let viewed = self.view_flag.is_some_and(inside);
        let centre = {
            let ps: Vec<egui::Pos2> = self.nodes.iter().filter(|n| inside(n.id)).map(|n| n.position).collect();
            let n = ps.len().max(1) as f32;
            egui::pos2(
                ps.iter().map(|p| p.x).sum::<f32>() / n,
                ps.iter().map(|p| p.y).sum::<f32>() / n,
            )
        };
        self.selected_nodes = picked.clone();
        self.selected_connection = None;
        self.delete_selected();
        let instance = self.add_node(
            crate::node_graph::node_name(name),
            NodeType::Compound {
                definition: d.name.clone(),
                version: d.version,
                hash: 0,
                inputs: in_names,
                outputs: out_names,
                values: BTreeMap::new(),
                editable: Some(Box::new(d)),
            },
            centre,
        );
        for (k, ((from, out), _, _)) in ins.iter().enumerate() {
            self.add_connection(*from, *out, instance, k);
        }
        for (k, (_, to, _)) in outs.iter().enumerate() {
            for (n, i) in to {
                self.add_connection(instance, k, *n, *i);
            }
        }
        if viewed {
            self.set_view_flag(instance);
        }
        self.selected_node = Some(instance);
        self.selected_nodes = vec![instance];
        Ok(instance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compounds::{Parameter, ParameterType, Target};

    fn pass(_: IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]) -> crate::core::geo::Geo {
        m.clone()
    }

    fn p(x: f32, y: f32) -> egui::Pos2 {
        egui::pos2(x, y)
    }

    /// The primitives' paths and how many points each has, from a result.
    fn prims(r: &Option<EvalResult>) -> Vec<(String, usize)> {
        match r {
            Some(EvalResult::Named(prims)) => prims.iter().map(|p| (p.path.clone(), p.mesh.vertices.len())).collect(),
            _ => vec![],
        }
    }

    fn cook(g: &NodeGraphState, id: NodeId) -> Option<EvalResult> {
        g.eval_node_out(id, 0, &mut EvalCache::new(), &pass)
    }

    /// A promoted parameter whose target is an inner compound's promoted
    /// parameter, its value not saved there yet: the value reaches it, and so
    /// goes on down.
    #[test]
    fn promoted_values_reach_nested_compounds() {
        let mut g = NodeGraphState::default();
        g.add_node(
            "Inner".into(),
            NodeType::Compound {
                definition: "Inner".into(),
                version: 1,
                hash: 0,
                inputs: vec![],
                outputs: vec![],
                values: Default::default(),
                editable: None,
            },
            p(0.0, 0.0),
        );
        let mut d = Definition::new(Context::Scene, "Outer");
        d.parameters = vec![Parameter {
            name: "size".into(),
            label: String::new(),
            kind: ParameterType::Float,
            default: serde_json::json!(1.0),
            range: None,
            group: String::new(),
            help: String::new(),
            targets: vec![Target {
                node: "Inner".into(),
                path: "values/size".into(),
            }],
        }];
        let values: BTreeMap<String, serde_json::Value> = [("size".to_string(), serde_json::json!(3.0))].into();
        apply_values(&mut g, &d, &values);
        let NodeType::Compound { values, .. } = &g.nodes[0].node_type else {
            panic!("not a compound");
        };
        assert_eq!(values.get("size"), Some(&serde_json::json!(3.0)));
    }

    /// A cube moved by a Transform, made a compound: the instance cooks to
    /// what the operators did, and keeps their wires.
    #[test]
    fn a_compound_cooks_as_the_operators_it_was_made_of() {
        let mut g = NodeGraphState::default();
        let mut subnets = IceGraphStore::default();
        let cube = g.add_node("Cube".into(), NodeType::cube(1.0), p(0.0, 0.0));
        let tr = g.add_node(
            "Move".into(),
            NodeType::Transform {
                translation: glam::Vec3::new(0.0, 2.0, 0.0),
                rotation: glam::Vec3::ZERO,
                scale: glam::Vec3::ONE,
            },
            p(0.0, 120.0),
        );
        let after = g.add_node("After".into(), NodeType::PassThrough, p(0.0, 240.0));
        g.add_connection(cube, 0, tr, 0);
        g.add_connection(tr, 0, after, 0);
        g.set_view_flag(after);
        let before = prims(&cook(&g, after));
        assert!(!before.is_empty());

        let inst = g.make_compound(&mut subnets, &[tr], "Lift").unwrap();
        assert!(!g.nodes.iter().any(|n| n.id == tr), "the operators are inside now");
        let n = g.nodes.iter().find(|n| n.id == inst).unwrap();
        assert_eq!(n.inputs.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Scene"]);
        assert_eq!(n.outputs.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Scene"]);
        assert_eq!(n.inputs[0].connected_output, Some((cube, 0)));
        assert!(g.connections.iter().any(|c| c.from_node == inst && c.to_node == after));
        assert_eq!(prims(&cook(&g, after)), before, "the same scene comes out");

        // Its graph inside: the Transform between a Compound Input and a
        // Compound Output.
        let NodeType::Compound { editable: Some(d), .. } = &n.node_type else {
            panic!("editable")
        };
        let inner = read_inner(d).unwrap();
        let kinds: Vec<String> = inner
            .graph
            .nodes
            .iter()
            .map(|n| crate::types::node_type_label(&n.node_type).to_string())
            .collect();
        assert!(
            kinds.contains(&"Transform".into())
                && kinds.contains(&"Compound Input".into())
                && kinds.contains(&"Compound Output".into())
        );
    }

    /// Two instances of one definition with their own promoted values give
    /// their own results; with no value, the default.
    #[test]
    fn instances_keep_their_own_values() {
        let mut g = NodeGraphState::default();
        let mut subnets = IceGraphStore::default();
        let cube = g.add_node("Cube".into(), NodeType::cube(1.0), p(0.0, 0.0));
        let tr = g.add_node(
            "Move".into(),
            NodeType::Transform {
                translation: glam::Vec3::ZERO,
                rotation: glam::Vec3::ZERO,
                scale: glam::Vec3::ONE,
            },
            p(0.0, 120.0),
        );
        g.add_connection(cube, 0, tr, 0);
        let a = g.make_compound(&mut subnets, &[tr], "Lift").unwrap();
        // Its translation's y promoted as Height.
        let NodeType::Compound { editable: Some(d), .. } =
            &mut g.nodes.iter_mut().find(|n| n.id == a).unwrap().node_type
        else {
            panic!("editable")
        };
        d.parameters.push(Parameter {
            name: "height".into(),
            label: "Height".into(),
            kind: ParameterType::Float,
            default: serde_json::json!(1.0),
            range: None,
            group: String::new(),
            help: String::new(),
            targets: vec![Target {
                node: "Move".into(),
                path: "translation/1".into(),
            }],
        });
        let t = g.nodes.iter().find(|n| n.id == a).unwrap().node_type.clone();
        let b = g.add_node("Lift2".into(), t, p(200.0, 120.0));
        g.add_connection(cube, 0, b, 0);
        if let NodeType::Compound { values, .. } = &mut g.nodes.iter_mut().find(|n| n.id == b).unwrap().node_type {
            values.insert("height".into(), serde_json::json!(5.0));
        }
        // The lowest point of what comes out, where it is drawn.
        let low = |r: Option<EvalResult>| match r {
            Some(EvalResult::Named(prims)) => prims
                .iter()
                .flat_map(|p| {
                    let m = p.place.matrices().first().copied().unwrap_or(glam::Mat4::IDENTITY);
                    p.mesh
                        .vertices
                        .iter()
                        .map(move |v| m.transform_point3(glam::Vec3::from(*v)).y)
                        .collect::<Vec<_>>()
                })
                .fold(f32::MAX, f32::min),
            _ => f32::NAN,
        };
        let (at_default, at_five) = (low(cook(&g, a)), low(cook(&g, b)));
        assert!((at_five - at_default - 4.0).abs() < 1e-4, "{at_default} {at_five}");
    }

    /// Wires follow ports by name, then by number; a port gone takes its wire.
    #[test]
    fn wires_follow_ports_by_name() {
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            remap(&names(&["Scene", "Guides"]), &names(&["Guides", "Scene"])),
            [Some(1), Some(0)]
        );
        assert_eq!(
            remap(&names(&["Scene", "Old"]), &names(&["Scene", "New"])),
            [Some(0), Some(1)]
        );
        assert_eq!(remap(&names(&["Scene", "Old"]), &names(&["Scene"])), [Some(0), None]);
        assert_eq!(unique("Scene", &names(&["Scene", "Scene2"])), "Scene3");

        let mut g = NodeGraphState::default();
        let a = g.add_node("A".into(), NodeType::cube(1.0), p(0.0, 0.0));
        let b = g.add_node("B".into(), NodeType::sphere(1.0, 8), p(100.0, 0.0));
        let c = g.add_node(
            "C".into(),
            NodeType::Compound {
                definition: "Missing".into(),
                version: 1,
                hash: 0,
                inputs: names(&["Scene", "Guides"]),
                outputs: names(&["Scene"]),
                values: BTreeMap::new(),
                editable: None,
            },
            p(0.0, 120.0),
        );
        g.add_connection(a, 0, c, 0);
        g.add_connection(b, 0, c, 1);
        g.rewire(c, names(&["Guides", "Scene"]), names(&["Scene"]));
        let n = g.nodes.iter().find(|n| n.id == c).unwrap();
        assert_eq!(
            (n.inputs[0].connected_output, n.inputs[1].connected_output),
            (Some((b, 0)), Some((a, 0)))
        );
        // Its definition missing: what comes into its first input goes on.
        assert_eq!(prims(&cook(&g, c)).len(), 1);
    }

    /// Every operator inside reads `{instance}`, wired or not.
    #[test]
    fn inside_every_operator_reads_the_instance_name() {
        let mut inner = NodeGraphState::default();
        let light = inner.add_node("Key".into(), NodeType::PassThrough, p(0.0, 0.0));
        inner.fed = Some(Arc::new(Fed {
            ports: vec![],
            above: vec![("instance".into(), "Rig1".into()), ("shot".into(), "sh010".into())],
        }));
        assert_eq!(
            inner.expand_at(light, "/lights/{instance}/{shot}"),
            "/lights/Rig1/sh010"
        );
    }

    /// A definition inside itself is not cooked again: what comes in goes on.
    #[test]
    fn a_compound_does_not_cook_itself() {
        let _outer = Cooking::start("Loop").unwrap();
        assert!(Cooking::start("Loop").is_none());
        assert!(Cooking::start("Other").is_some());
    }
}
