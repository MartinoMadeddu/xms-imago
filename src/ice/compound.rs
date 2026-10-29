//! ICE compounds (see `docs/compounds.md`): an ICE graph's Compound node, which
//! runs the nodes of its definition in its place, and the Compound Input and
//! Compound Output nodes inside it.
//!
//! - **Flattened, never run as such:** before a graph goes to the engine, each
//!   instance is replaced by its definition's nodes (`flatten`): what reads a
//!   Compound Input reads what is wired to the instance's port of its name (or
//!   the port's value), and what reads the instance's output reads what feeds
//!   the Compound Output of its name. Instances inside an instance are
//!   flattened first, `compounds::MAX_DEPTH` deep; a definition inside itself
//!   is refused. The engine never sees a compound.
//! - **Ports:** an instance's inputs are its definition's Compound Inputs, top
//!   to bottom, each with the value it holds while nothing is wired in; its
//!   outputs the Compound Outputs. An output fed by an Execute (Set Data) is an
//!   Execute, for the Execution Ports.
//! - **Going inside** puts the definition's graph in the editor in place of the
//!   graph the instance is in, which is kept with it (`IceGraph::outer`): what
//!   is run, saved and hashed is the outer graph with the inside folded back
//!   into its instance (`IceGraph::composed`), so the result follows the
//!   edits. A locked inside is put back when changed (`keep_locked`).
//! - **Make Compound** packs the selected nodes into an editable instance, the
//!   wires crossing the selection its ports.
//! - **Settings promoted** (a seed, a path: `promote_setting`) are a graph's
//!   `parameters`: at the top of an ICE operator's graph the operator's, set
//!   in its parameters; inside a compound the compound's, set on each
//!   instance. Their values are put in the settings before the graph runs
//!   (`set_values`), a promoted setting following its parameter as a
//!   reference follows its source.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

use super::{IceGraph, IceGraphConnection, IceGraphInputSocket, IceGraphNode, IceGraphOutputSocket};
use crate::compounds::{self, Context, Definition, Parameter, ParameterType, Port, Target};
use crate::types::{ConnectionId, IceGraphId, IceGraphNodeType, NodeId};
use xms_ice::Value;

/// The graph an instance is in, kept while its inside is looked at.
#[derive(Clone)]
pub struct Outer {
    /// The graph, its own `outer` empty.
    pub graph: Box<IceGraph>,
    pub instance: NodeId,
    pub locked: bool,
    /// Locked: the inside as entered, put back when changed.
    pub pristine: Option<serde_json::Value>,
}

/// An instance's definition: its own once editable, else the library's ICE
/// compound of its name. Why not, for another node or a definition not found.
pub fn definition(t: &IceGraphNodeType) -> Result<Cow<'_, Definition>, String> {
    match t {
        IceGraphNodeType::Compound { editable: Some(d), .. } => Ok(Cow::Borrowed(&**d)),
        IceGraphNodeType::Compound { definition, .. } => {
            let library = compounds::library();
            let found = library.for_context(definition, Context::Ice)?;
            Ok(Cow::Owned(found.definition.clone()))
        }
        _ => Err("Not an ICE compound".into()),
    }
}

/// An instance's ports, from its definition; the names it was saved with
/// (each a Scalar) while that cannot be found.
pub fn sockets(t: &IceGraphNodeType) -> (Vec<IceGraphInputSocket>, Vec<IceGraphOutputSocket>) {
    let IceGraphNodeType::Compound { inputs, outputs, .. } = t else {
        return (vec![], vec![]);
    };
    let named = |names: &[String]| -> Vec<Port> {
        names
            .iter()
            .map(|n| Port {
                name: n.clone(),
                kind: None,
                value: None,
            })
            .collect()
    };
    let (ins, outs) = match definition(t) {
        Ok(d) => (d.inputs.clone(), d.outputs.clone()),
        Err(_) => (named(inputs), named(outputs)),
    };
    (
        ins.into_iter()
            .map(|p| IceGraphInputSocket {
                name: p.name,
                value_hint: "Any",
                connected_output: None,
                value: Some(p.value.unwrap_or(Value::Float(0.0))),
            })
            .collect(),
        outs.into_iter()
            .map(|p| IceGraphOutputSocket {
                value_hint: if p.kind.as_deref() == Some("Execute") {
                    "Execute"
                } else {
                    "Any"
                },
                name: p.name,
            })
            .collect(),
    )
}

/// The ports a compound's inside makes: its Compound Inputs, then its
/// Compound Outputs, each top to bottom, a name once.
pub fn ports_of(inside: &IceGraph) -> (Vec<Port>, Vec<Port>) {
    let mut nodes: Vec<&IceGraphNode> = inside.nodes.iter().collect();
    nodes.sort_by(|a, b| {
        (a.position.y, a.position.x)
            .partial_cmp(&(b.position.y, b.position.x))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let (mut ins, mut outs): (Vec<Port>, Vec<Port>) = (vec![], vec![]);
    for n in nodes {
        match &n.node_type {
            IceGraphNodeType::CompoundInput { name, value } if !ins.iter().any(|p| p.name == *name) => ins.push(Port {
                name: name.clone(),
                kind: None,
                value: Some(*value),
            }),
            IceGraphNodeType::CompoundOutput { name } if !outs.iter().any(|p| p.name == *name) => {
                let execute = n.inputs.first().and_then(|i| i.connected_output).is_some_and(|(a, o)| {
                    inside
                        .nodes
                        .iter()
                        .find(|m| m.id == a)
                        .and_then(|m| m.outputs.get(o))
                        .is_some_and(|s| s.value_hint == "Execute")
                });
                outs.push(Port {
                    name: name.clone(),
                    kind: execute.then(|| "Execute".to_string()),
                    value: None,
                })
            }
            _ => {}
        }
    }
    (ins, outs)
}

/// Set each of `parameters` at its targets (a node by name, the setting's
/// path) in `g`: its value in `values` (by name), else its default.
pub fn set_values(g: &mut IceGraph, parameters: &[Parameter], values: &BTreeMap<String, serde_json::Value>) {
    for p in parameters {
        let v = values.get(&p.name).unwrap_or(&p.default);
        for t in &p.targets {
            let Some(n) = g.nodes.iter_mut().find(|n| n.name == t.node) else {
                continue;
            };
            let Ok(mut j) = serde_json::to_value(&n.node_type) else {
                continue;
            };
            if compounds::put(&mut j, &t.path, v.clone()) {
                if let Ok(set) = serde_json::from_value(j) {
                    n.node_type = set;
                }
            }
        }
    }
}

/// `g` with `parameters` set (`set_values`); `g` itself when there are none.
pub fn with_values<'a>(
    g: &'a IceGraph,
    parameters: &[Parameter],
    values: &BTreeMap<String, serde_json::Value>,
) -> Cow<'a, IceGraph> {
    if parameters.is_empty() {
        return Cow::Borrowed(g);
    }
    let mut out = g.clone();
    set_values(&mut out, parameters, values);
    Cow::Owned(out)
}

/// The Parameter nodes of a graph, its compounds' included: each name once,
/// with its default.
pub fn parameters_of(g: &IceGraph) -> Vec<(String, Value)> {
    let flat = flatten(g).ok();
    let nodes = flat.as_ref().map_or(&g.nodes, |f| &f.graph.nodes);
    let mut out: Vec<(String, Value)> = vec![];
    for n in nodes {
        if let IceGraphNodeType::Parameter { name, default } = &n.node_type {
            if !out.iter().any(|(m, _)| m == name) {
                out.push((name.clone(), *default));
            }
        }
    }
    out
}

/// Each ICE operator of `graph` keeps a value for every Parameter node of its
/// graph (in `subnets`), its default until set, so each can be referenced and
/// promoted: values for nodes gone are dropped, those of another type set
/// back to the default; and the values set of its settings promoted, those of
/// settings no longer promoted dropped. Done when the graphs change.
pub fn keep_params(graph: &mut crate::node_graph::NodeGraphState, subnets: &crate::ice::IceGraphStore) {
    use std::hash::{Hash, Hasher};
    thread_local! {
        static SEEN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (subnets.content_hash(), graph.nodes.len()).hash(&mut h);
    for n in &graph.nodes {
        if let crate::types::NodeType::Ice { id, .. } = n.node_type {
            (n.id.0, id.0).hash(&mut h);
        }
    }
    let now = h.finish();
    // Inside a locked compound nothing changes.
    if SEEN.with(|s| s.replace(now)) == now || graph.inside.locked {
        return;
    }
    for n in graph.nodes.iter_mut() {
        let crate::types::NodeType::Ice { id, params, .. } = &mut n.node_type else {
            continue;
        };
        let Some(sg) = subnets.get(*id) else { continue };
        let g = sg.composed();
        let mut kept: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        for (name, default) in parameters_of(&g) {
            let same = |v: &&serde_json::Value| {
                serde_json::from_value::<Value>((*v).clone()).is_ok_and(|x| x.ty() == default.ty())
            };
            let v = params
                .get(&name)
                .filter(same)
                .cloned()
                .unwrap_or_else(|| serde_json::to_value(default).unwrap_or_default());
            kept.insert(name, v);
        }
        // Its settings promoted: those set.
        for p in &g.parameters {
            if let Some(v) = params.get(&p.name) {
                kept.insert(p.name.clone(), v.clone());
            }
        }
        if kept != *params {
            *params = kept;
        }
    }
}

/// A graph with its compounds flattened, as the engine takes it.
pub struct Flat<'a> {
    pub graph: Cow<'a, IceGraph>,
    /// The way to each node put in an instance's place: the instance, then
    /// the node inside it (and inside the compounds within, one id each).
    trails: HashMap<NodeId, Vec<NodeId>>,
    /// What gives each output of an instance, in `graph`.
    pub ports: HashMap<(NodeId, usize), (NodeId, usize)>,
}

impl Flat<'_> {
    /// The way to the node that node `id` of `graph` stands for: itself, or
    /// the instance it came from, then the node inside it, and so on.
    pub fn trail(&self, id: NodeId) -> Vec<NodeId> {
        self.trails.get(&id).cloned().unwrap_or_else(|| vec![id])
    }
}

/// `g` with every instance replaced by its definition's nodes (see the
/// module's notes). A Compound Input left (a compound's inside, run on its own
/// while it is looked at) gives its value, a Compound Output nothing. Refused:
/// an instance whose definition cannot be found, one inside itself, or too
/// deep; the way to the instance (the outer instance first) and why.
pub fn flatten(g: &IceGraph) -> Result<Flat<'_>, (Vec<NodeId>, String)> {
    let plain = !g.nodes.iter().any(|n| {
        matches!(
            n.node_type,
            IceGraphNodeType::Compound { .. }
                | IceGraphNodeType::CompoundInput { .. }
                | IceGraphNodeType::CompoundOutput { .. }
        )
    });
    let mut flat = Flat {
        graph: Cow::Borrowed(g),
        trails: HashMap::new(),
        ports: HashMap::new(),
    };
    if plain {
        return Ok(flat);
    }
    let mut out = g.clone();
    out.outer.clear();
    expand(&mut out, &mut flat.trails, &mut flat.ports, &mut vec![])?;
    for n in out.nodes.iter_mut() {
        if let IceGraphNodeType::CompoundInput { value, .. } = n.node_type {
            n.node_type = IceGraphNodeType::Constant { value };
        }
    }
    out.nodes
        .retain(|n| !matches!(n.node_type, IceGraphNodeType::CompoundOutput { .. }));
    rewire(&mut out);
    flat.graph = Cow::Owned(out);
    Ok(flat)
}

/// Replace every instance of `g` by its definition's nodes, those of
/// definitions `within` (the ones being flattened, outermost first) refused.
fn expand(
    g: &mut IceGraph,
    trails: &mut HashMap<NodeId, Vec<NodeId>>,
    ports: &mut HashMap<(NodeId, usize), (NodeId, usize)>,
    within: &mut Vec<String>,
) -> Result<(), (Vec<NodeId>, String)> {
    let instances: Vec<NodeId> = g
        .nodes
        .iter()
        .filter(|n| matches!(n.node_type, IceGraphNodeType::Compound { .. }))
        .map(|n| n.id)
        .collect();
    for id in instances {
        let Some(node) = g.nodes.iter().find(|n| n.id == id).cloned() else {
            continue;
        };
        let d = definition(&node.node_type).map_err(|e| (vec![id], e))?;
        if within.contains(&d.name) {
            return Err((
                vec![id],
                format!("{} is inside itself: a compound cannot hold itself", d.name),
            ));
        }
        if within.len() >= compounds::MAX_DEPTH {
            return Err((
                vec![id],
                format!(
                    "Compounds are more than {} deep inside one another",
                    compounds::MAX_DEPTH
                ),
            ));
        }
        let mut inner =
            crate::graph_io::ice_from_value(&d.graph).map_err(|e| (vec![id], format!("{}: {e}", d.name)))?;
        if let IceGraphNodeType::Compound { values, .. } = &node.node_type {
            set_values(&mut inner, &d.parameters, values);
        }
        within.push(d.name.to_string());
        let mut inner_trails = HashMap::new();
        let nested = expand(&mut inner, &mut inner_trails, &mut HashMap::new(), within);
        within.pop();
        nested.map_err(|(way, e)| {
            let mut to = vec![id];
            to.extend(way);
            (to, format!("{}: {e}", d.name))
        })?;
        splice(g, &node, &inner, &inner_trails, trails, ports);
    }
    rewire(g);
    Ok(())
}

/// What an output of a compound's inside stands for in the graph its instance
/// is in: a wire from there, or a value (none: the input keeps its own).
#[derive(Clone, Copy)]
enum Source {
    Wire(NodeId, usize),
    Value(Option<Value>),
}

fn take(input: &mut IceGraphInputSocket, s: Source) {
    match s {
        Source::Wire(a, b) => input.connected_output = Some((a, b)),
        Source::Value(v) => {
            input.connected_output = None;
            if v.is_some() {
                input.value = v;
            }
        }
    }
}

/// Put the nodes of `inner` (flattened, `inner_trails` the ways to its nodes)
/// in the place of instance `inst` of `g`, with new ids, and wire them as the
/// module's notes say.
fn splice(
    g: &mut IceGraph,
    inst: &IceGraphNode,
    inner: &IceGraph,
    inner_trails: &HashMap<NodeId, Vec<NodeId>>,
    trails: &mut HashMap<NodeId, Vec<NodeId>>,
    ports: &mut HashMap<(NodeId, usize), (NodeId, usize)>,
) {
    let offset = g.next_node_id;
    g.next_node_id += inner.next_node_id;
    let shift = |id: NodeId| NodeId(id.0 + offset);
    let way_in = trails.get(&inst.id).cloned().unwrap_or_else(|| vec![inst.id]);
    let resolve = |(s, o): (NodeId, usize)| -> Source {
        match inner.nodes.iter().find(|n| n.id == s).map(|n| &n.node_type) {
            Some(IceGraphNodeType::CompoundInput { name, value }) => {
                match inst.inputs.iter().find(|i| i.name == *name) {
                    Some(i) => match i.connected_output {
                        Some((a, b)) => Source::Wire(a, b),
                        None => Source::Value(i.value.or(Some(*value))),
                    },
                    None => Source::Value(Some(*value)),
                }
            }
            Some(_) => Source::Wire(shift(s), o),
            None => Source::Value(None),
        }
    };
    for n in &inner.nodes {
        if matches!(
            n.node_type,
            IceGraphNodeType::Root { .. }
                | IceGraphNodeType::CompoundInput { .. }
                | IceGraphNodeType::CompoundOutput { .. }
        ) {
            continue;
        }
        let mut m = n.clone();
        m.id = shift(n.id);
        for input in &mut m.inputs {
            if let Some(w) = input.connected_output {
                take(input, resolve(w));
            }
        }
        let mut way = way_in.clone();
        way.extend(inner_trails.get(&n.id).cloned().unwrap_or_else(|| vec![n.id]));
        trails.insert(m.id, way);
        g.nodes.push(m);
    }
    let outs: Vec<Source> = inst
        .outputs
        .iter()
        .map(|o| {
            inner
                .nodes
                .iter()
                .find(|n| matches!(&n.node_type, IceGraphNodeType::CompoundOutput { name } if *name == o.name))
                .and_then(|n| n.inputs.first())
                .map_or(Source::Value(None), |i| match i.connected_output {
                    Some(w) => resolve(w),
                    None => Source::Value(i.value),
                })
        })
        .collect();
    for m in g.nodes.iter_mut() {
        for input in &mut m.inputs {
            if let Some((a, k)) = input.connected_output {
                if a == inst.id {
                    take(input, outs.get(k).copied().unwrap_or(Source::Value(None)));
                }
            }
        }
    }
    for (k, s) in outs.iter().enumerate() {
        if let Source::Wire(a, b) = s {
            ports.insert((inst.id, k), (*a, *b));
        }
    }
    g.nodes.retain(|n| n.id != inst.id);
}

/// `g`'s wires made again from what its inputs are wired to.
fn rewire(g: &mut IceGraph) {
    let wires: Vec<(NodeId, usize, NodeId, usize)> = g
        .nodes
        .iter()
        .flat_map(|n| {
            n.inputs
                .iter()
                .enumerate()
                .filter_map(move |(k, i)| i.connected_output.map(|(f, o)| (f, o, n.id, k)))
        })
        .collect();
    g.connections = wires
        .into_iter()
        .enumerate()
        .map(|(c, (f, o, t, k))| IceGraphConnection {
            id: ConnectionId(c),
            from_node: f,
            from_output: o,
            to_node: t,
            to_input: k,
        })
        .collect();
    g.next_connection_id = g.connections.len();
    g.selected_wire = None;
}

/// Instance `id`'s ports made again from its definition: the wires into its
/// inputs and out of its outputs kept by port name, and its values.
pub fn refresh_ports(g: &mut IceGraph, id: NodeId) {
    let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) else {
        return;
    };
    let (mut ins, outs) = sockets(&n.node_type);
    for s in ins.iter_mut() {
        if let Some(old) = n.inputs.iter().find(|i| i.name == s.name) {
            s.connected_output = old.connected_output;
            s.value = old.value.or(s.value);
        }
    }
    let map: Vec<Option<usize>> = n
        .outputs
        .iter()
        .map(|o| outs.iter().position(|p| p.name == o.name))
        .collect();
    if let IceGraphNodeType::Compound { inputs, outputs, .. } = &mut n.node_type {
        *inputs = ins.iter().map(|s| s.name.clone()).collect();
        *outputs = outs.iter().map(|s| s.name.clone()).collect();
    }
    n.inputs = ins;
    n.outputs = outs;
    for m in g.nodes.iter_mut() {
        for input in &mut m.inputs {
            if let Some((a, k)) = input.connected_output {
                if a == id {
                    input.connected_output = map.get(k).copied().flatten().map(|j| (id, j));
                }
            }
        }
    }
    rewire(g);
}

/// Put `inside` in instance `id` of `g` as its own definition's graph, its
/// ports and promoted parameters made from it. The instance keeps the
/// promoted settings' values (read at their first targets), the definition
/// their defaults.
fn put_inside(g: &mut IceGraph, id: NodeId, inside: &IceGraph) {
    let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) else {
        return;
    };
    let IceGraphNodeType::Compound {
        editable: Some(d),
        values,
        ..
    } = &mut n.node_type
    else {
        return;
    };
    let (ins, outs) = ports_of(inside);
    d.inputs = ins;
    d.outputs = outs;
    for p in &inside.parameters {
        let now = p.targets.first().and_then(|t| {
            let n = inside.nodes.iter().find(|n| n.name == t.node)?;
            compounds::get(&serde_json::to_value(&n.node_type).ok()?, &t.path).cloned()
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
    values.retain(|k, _| inside.parameters.iter().any(|p| p.name == *k));
    d.parameters = inside.parameters.clone();
    let mut kept = inside.clone();
    kept.outer.clear();
    set_values(&mut kept, &inside.parameters, &BTreeMap::new());
    d.graph = crate::graph_io::ice_to_value(&kept);
    refresh_ports(g, id);
}

impl IceGraph {
    /// The graph as it is run, saved and hashed: itself, or, while an
    /// instance's inside is looked at, the outermost graph with each inside
    /// entered folded back into its instance (a locked one's as it is).
    pub fn composed(&self) -> Cow<'_, IceGraph> {
        if self.outer.is_empty() {
            return Cow::Borrowed(self);
        }
        let mut inside = self.clone();
        inside.outer.clear();
        for o in self.outer.iter().rev() {
            let mut g = (*o.graph).clone();
            if !o.locked {
                put_inside(&mut g, o.instance, &inside);
            }
            inside = g;
        }
        Cow::Owned(inside)
    }

    /// The instances entered to look at this graph, outermost first, with
    /// whether the innermost is locked.
    pub fn entered(&self) -> (Vec<String>, bool) {
        let names = self
            .outer
            .iter()
            .map(|o| {
                o.graph
                    .nodes
                    .iter()
                    .find(|n| n.id == o.instance)
                    .map_or("(gone)".to_string(), |n| n.name.clone())
            })
            .collect();
        (names, self.outer.last().is_some_and(|o| o.locked))
    }
}

/// Go inside instance `id` of the graph `g` holds: `g` holds the inside, the
/// graph it was kept with it. Returns what to tell.
pub fn enter(g: &mut IceGraph, id: NodeId) -> Result<String, String> {
    let node = g.nodes.iter().find(|n| n.id == id).ok_or("That node is gone")?;
    let d = definition(&node.node_type)?;
    let locked = !matches!(node.node_type, IceGraphNodeType::Compound { editable: Some(_), .. });
    let mut inside = crate::graph_io::ice_from_value(&d.graph)?;
    // Its promoted settings, at the instance's values.
    inside.parameters = d.parameters.clone();
    if let IceGraphNodeType::Compound { values, .. } = &node.node_type {
        set_values(&mut inside, &d.parameters, values);
    }
    inside.id = g.id;
    inside.name = g.name.clone();
    // Get Data's list: the geometry the graph runs on.
    inside.last_input = g.last_input.clone();
    inside.zoom = g.zoom;
    let pristine = locked.then(|| crate::graph_io::ice_to_value(&inside));
    let told = if locked {
        format!("Inside {}: locked, it follows its definition {}", node.name, d.name)
    } else {
        format!("Inside {}: ↑ goes back out", node.name)
    };
    let mut outer = std::mem::take(&mut g.outer);
    let parent = std::mem::replace(g, inside);
    outer.push(Outer {
        graph: Box::new(parent),
        instance: id,
        locked,
        pristine,
    });
    g.outer = outer;
    Ok(told)
}

/// Back out of the instance whose inside `g` holds: an editable one takes the
/// inside as its definition's graph. Returns what to tell; None when not
/// inside one.
pub fn leave(g: &mut IceGraph) -> Option<String> {
    let o = g.outer.pop()?;
    let rest = std::mem::take(&mut g.outer);
    let inside = std::mem::replace(g, *o.graph);
    g.outer = rest;
    g.selected_node = Some(o.instance);
    g.selected_nodes = vec![o.instance];
    let name = g
        .nodes
        .iter()
        .find(|n| n.id == o.instance)
        .map_or(String::new(), |n| n.name.clone());
    if o.locked {
        return Some(format!("Back out of {name}"));
    }
    put_inside(g, o.instance, &inside);
    Some(format!("Back out of {name}: its graph is kept with it"))
}

/// Inside a locked instance: a change is put back. Returns whether one was.
pub fn keep_locked(g: &mut IceGraph) -> bool {
    let Some(pristine) = g.outer.last().and_then(|o| o.pristine.clone()) else {
        return false;
    };
    if crate::graph_io::ice_to_value(g) == pristine {
        return false;
    }
    let Ok(mut fresh) = crate::graph_io::ice_from_value(&pristine) else {
        return false;
    };
    fresh.id = g.id;
    fresh.name = g.name.clone();
    fresh.outer = std::mem::take(&mut g.outer);
    fresh.last_input = g.last_input.clone();
    fresh.pan_offset = g.pan_offset;
    fresh.zoom = g.zoom;
    *g = fresh;
    true
}

/// Make the instance being looked at editable, from inside it: its own copy
/// of the definition. Returns what to tell.
pub fn make_editable(g: &mut IceGraph) -> Option<String> {
    let o = g.outer.last_mut()?;
    let instance = o.instance;
    let node = o.graph.nodes.iter_mut().find(|n| n.id == instance)?;
    let d = definition(&node.node_type).ok()?.into_owned();
    if let IceGraphNodeType::Compound { editable, .. } = &mut node.node_type {
        *editable = Some(Box::new(d));
    }
    let told = format!(
        "{} is editable: its own copy of the definition, no longer the library's",
        node.name
    );
    o.locked = false;
    o.pristine = None;
    Some(told)
}

/// A name not among `taken`: `name`, else `name2`, `name3`…
fn unique(name: &str, taken: &[String]) -> String {
    (1..)
        .map(|k| if k == 1 { name.to_string() } else { format!("{name}{k}") })
        .find(|n| !taken.contains(n))
        .unwrap_or_default()
}

/// Input `k` of node `id`, holding a value, promoted: inside a compound, a
/// port of it (a Compound Input); at the top of an ICE operator's graph, a
/// parameter of the operator (a Parameter node). Named after the node and the
/// input, holding its value, beside it and wired in. Returns what to tell.
pub fn promote_input(g: &mut IceGraph, id: NodeId, k: usize) -> Option<String> {
    let in_compound = !g.outer.is_empty();
    let n = g.nodes.iter().find(|n| n.id == id)?;
    let input = n.inputs.get(k).filter(|i| i.connected_output.is_none())?;
    let taken: Vec<String> = g
        .nodes
        .iter()
        .filter_map(|n| match &n.node_type {
            IceGraphNodeType::CompoundInput { name, .. } if in_compound => Some(name.clone()),
            IceGraphNodeType::Parameter { name, .. } if !in_compound => Some(name.clone()),
            _ => None,
        })
        .collect();
    let name = unique(&format!("{} {}", n.name, input.name), &taken);
    let value = input.value.unwrap_or(Value::Float(0.0));
    let at = n.position + egui::vec2(-220.0, 40.0 * k as f32);
    let (t, told) = if in_compound {
        (
            IceGraphNodeType::CompoundInput {
                name: name.clone(),
                value,
            },
            format!("{name} is a port of the compound, once you go back out"),
        )
    } else {
        (
            IceGraphNodeType::Parameter {
                name: name.clone(),
                default: value,
            },
            format!("{name} is a parameter of the ICE operator: set it in its parameters"),
        )
    };
    let made = g.add_node(name, t, at);
    g.add_connection(made, 0, id, k);
    Some(told)
}

/// Setting `path` of node `id` (a seed, a path, a count) promoted: a parameter
/// of `g` (`IceGraph::parameters`), its default the setting's value now. At
/// the top of an ICE operator's graph a parameter of the operator; inside a
/// compound, of the compound. The setting then follows it. Returns what to
/// tell, or why not.
pub fn promote_setting(
    g: &mut IceGraph,
    id: NodeId,
    path: &str,
    group: &str,
    choices: Vec<(String, serde_json::Value)>,
) -> Result<String, String> {
    // Its settings are promoted by its name: another node called the same
    // (from a graph made before names were kept apart), it is renamed.
    let named = g
        .nodes
        .iter()
        .find(|n| n.id == id)
        .ok_or("That node is gone")?
        .name
        .clone();
    let renamed = if g.nodes.iter().filter(|m| m.name == named).count() > 1 {
        let taken: Vec<String> = g.nodes.iter().map(|m| m.name.clone()).collect();
        let name = unique(&named, &taken);
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) {
            n.name = name.clone();
        }
        Some(name)
    } else {
        None
    };
    let n = g.nodes.iter().find(|n| n.id == id).ok_or("That node is gone")?;
    // A row of a list stands for the list (a Primitive Locations).
    let json = serde_json::to_value(&n.node_type).unwrap_or_default();
    let path = compounds::whole_list(&json, path);
    let path = path.as_str();
    let target = Target {
        node: n.name.clone(),
        path: path.to_string(),
    };
    if g.parameters.iter().any(|p| p.targets.contains(&target)) {
        return Err("Promoted already".into());
    }
    let default = compounds::get(&json, path)
        .cloned()
        .ok_or("Not a setting of the node")?;
    let label = format!("{} {}", n.name, crate::core::keys::label(path));
    // A name no parameter of its graph has, a Parameter node's included.
    let mut taken: Vec<String> = g.parameters.iter().map(|p| p.name.clone()).collect();
    taken.extend(parameters_of(g).into_iter().map(|(name, _)| name));
    let base: String = label
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let name = unique(&base, &taken);
    g.parameters.push(Parameter {
        name,
        label: label.clone(),
        kind: if choices.is_empty() {
            ParameterType::of(&default)
        } else {
            ParameterType::Choice
        },
        default,
        range: None,
        group: group.to_string(),
        help: String::new(),
        targets: vec![target],
        choices,
    });
    let told = if g.outer.is_empty() {
        format!("{label} is a parameter of the ICE operator: set it in its parameters")
    } else {
        format!("{label} is a parameter of the compound, once you go back out")
    };
    Ok(match renamed {
        Some(name) => format!("Renamed {named} to {name}, its name its own. {told}"),
        None => told,
    })
}

/// Setting `path` of node `node` its own again: off the graph's parameters, a
/// parameter left with none going.
pub fn remove_promotion(g: &mut IceGraph, node: &str, path: &str) {
    for p in &mut g.parameters {
        p.targets.retain(|t| !(t.node == node && t.path == path));
    }
    g.parameters.retain(|p| !p.targets.is_empty());
}

/// Pack nodes `ids` of `g` into an ICE compound named `name`: an editable
/// instance in their place, its graph the nodes with their wires, a Compound
/// Input for each output wired into them from outside (holding the value of
/// the first input it feeds) and a Compound Output for each of theirs wired
/// out. The instance is selected. The Execution Ports stay.
pub fn make_compound(g: &mut IceGraph, ids: &[NodeId], name: &str) -> Result<NodeId, String> {
    let picked: Vec<NodeId> = g
        .nodes
        .iter()
        .filter(|n| ids.contains(&n.id) && !matches!(n.node_type, IceGraphNodeType::Root { .. }))
        .map(|n| n.id)
        .collect();
    if picked.is_empty() {
        return Err("Select the nodes to make a compound of".into());
    }
    if g.nodes.iter().any(|n| {
        picked.contains(&n.id)
            && matches!(
                n.node_type,
                IceGraphNodeType::CompoundInput { .. } | IceGraphNodeType::CompoundOutput { .. }
            )
    }) {
        return Err("Compound Input and Compound Output nodes stay where they are".into());
    }
    let inside = |id: NodeId| picked.contains(&id);
    let chosen: Vec<IceGraphNode> = g.nodes.iter().filter(|n| inside(n.id)).cloned().collect();
    let (left, top, right) = chosen.iter().fold((f32::MAX, f32::MAX, f32::MIN), |(l, t, r), n| {
        (l.min(n.position.x), t.min(n.position.y), r.max(n.position.x))
    });

    let mut inner = IceGraph::new(IceGraphId(0), name.into());
    inner.nodes.clear();
    inner.next_node_id = g.next_node_id;
    // Wires coming in, by what they come from: a Compound Input each.
    let mut ins: Vec<((NodeId, usize), NodeId, String)> = vec![];
    let mut taken: Vec<String> = vec![];
    for n in &chosen {
        let mut n = n.clone();
        for input in n.inputs.iter_mut() {
            let Some(src) = input.connected_output.filter(|(a, _)| !inside(*a)) else {
                continue;
            };
            let cin = match ins.iter().find(|i| i.0 == src) {
                Some(i) => i.1,
                None => {
                    let port = unique(&input.name, &taken);
                    taken.push(port.clone());
                    let at = egui::pos2(left - 220.0, top + 70.0 * ins.len() as f32);
                    let value = input.value.unwrap_or(Value::Float(0.0));
                    let cin = inner.add_node(
                        port.clone(),
                        IceGraphNodeType::CompoundInput {
                            name: port.clone(),
                            value,
                        },
                        at,
                    );
                    ins.push((src, cin, port));
                    cin
                }
            };
            input.connected_output = Some((cin, 0));
        }
        inner.nodes.push(n);
    }
    // Wires going out, by what they come from: a Compound Output each.
    let mut outs: Vec<((NodeId, usize), Vec<(NodeId, usize)>, String)> = vec![];
    let mut taken: Vec<String> = vec![];
    for c in g
        .connections
        .iter()
        .filter(|c| inside(c.from_node) && !inside(c.to_node))
    {
        match outs.iter_mut().find(|o| o.0 == (c.from_node, c.from_output)) {
            Some(o) => o.1.push((c.to_node, c.to_input)),
            None => {
                let socket = g
                    .nodes
                    .iter()
                    .find(|n| n.id == c.from_node)
                    .and_then(|n| n.outputs.get(c.from_output))
                    .map_or("Value".to_string(), |s| s.name.clone());
                let port = unique(&socket, &taken);
                taken.push(port.clone());
                outs.push(((c.from_node, c.from_output), vec![(c.to_node, c.to_input)], port));
            }
        }
    }
    for (k, (src, _, port)) in outs.iter().enumerate() {
        let at = egui::pos2(right + 260.0, top + 70.0 * k as f32);
        let cout = inner.add_node(
            port.clone(),
            IceGraphNodeType::CompoundOutput { name: port.clone() },
            at,
        );
        if let Some(n) = inner.nodes.iter_mut().find(|n| n.id == cout) {
            if let Some(i) = n.inputs.first_mut() {
                i.connected_output = Some(*src);
            }
        }
    }
    rewire(&mut inner);

    let mut d = Definition::new(Context::Ice, name);
    let (pi, po) = ports_of(&inner);
    d.inputs = pi;
    d.outputs = po;
    d.graph = crate::graph_io::ice_to_value(&inner);
    let in_names: Vec<String> = d.inputs.iter().map(|p| p.name.clone()).collect();
    let out_names: Vec<String> = d.outputs.iter().map(|p| p.name.clone()).collect();

    for id in &picked {
        g.remove_node(*id);
    }
    let id = g.add_node(
        name.into(),
        IceGraphNodeType::Compound {
            definition: name.into(),
            version: d.version,
            hash: 0,
            inputs: in_names.clone(),
            outputs: out_names.clone(),
            values: Default::default(),
            editable: Some(Box::new(d)),
        },
        egui::pos2(left, top),
    );
    for (src, _, port) in &ins {
        if let Some(k) = in_names.iter().position(|p| p == port) {
            g.add_connection(src.0, src.1, id, k);
        }
    }
    for (_, targets, port) in &outs {
        if let Some(k) = out_names.iter().position(|p| p == port) {
            for (to, input) in targets {
                g.add_connection(id, k, *to, *input);
            }
        }
    }
    g.selected_node = Some(id);
    g.selected_nodes = vec![id];
    Ok(id)
}

/// Write editable instance `id`'s definition to `path`, a version after the
/// library's of its name. In a library folder, the instance then follows the
/// file, locked, and the other instances of the name in `g` are brought up to
/// date; elsewhere it stays editable. Returns what to tell.
pub fn save_definition(g: &mut IceGraph, id: NodeId, path: &std::path::Path) -> Result<String, String> {
    let node = g.nodes.iter_mut().find(|n| n.id == id).ok_or("That node is gone")?;
    let IceGraphNodeType::Compound {
        editable,
        definition,
        version,
        hash,
        ..
    } = &mut node.node_type
    else {
        return Err("Not an ICE compound".into());
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
    if !in_library {
        return Ok(format!(
            "Saved {name} v{v} to {}: not in a library folder, so this instance keeps its own copy",
            path.display()
        ));
    }
    *editable = None;
    let others: Vec<NodeId> = g
        .nodes
        .iter()
        .filter(|n| matches!(&n.node_type, IceGraphNodeType::Compound { definition, editable: None, .. } if *definition == name))
        .map(|n| n.id)
        .collect();
    for other in others {
        refresh_ports(g, other);
    }
    Ok(format!(
        "Saved {name} v{v} to {}: this instance follows it, locked",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geo::Geo;
    use xms_ice::Math;

    /// A graph adding 1 to every point's Y: Get P, Add (B a vector), Set P.
    fn lifting() -> (IceGraph, NodeId, NodeId) {
        let mut g = IceGraph::new(IceGraphId(0), "Lift".into());
        let root = g.root().unwrap();
        let p = g.add_node("P".into(), IceGraphNodeType::get("P"), egui::pos2(0.0, 0.0));
        let add = g.add_node(
            "Add".into(),
            IceGraphNodeType::Math { op: Math::Add },
            egui::pos2(200.0, 0.0),
        );
        let set = g.add_node("Set".into(), IceGraphNodeType::set("P"), egui::pos2(400.0, 0.0));
        g.add_connection(p, 0, add, 0);
        g.add_connection(add, 0, set, 0);
        g.add_connection(set, 0, root, 0);
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == add) {
            n.inputs[1].value = Some(Value::Vec3([0.0, 1.0, 0.0]));
        }
        (g, add, set)
    }

    fn ys(g: &Geo) -> Vec<f32> {
        g.points().iter().map(|p| p[1]).collect()
    }

    /// A triangle.
    fn triangle() -> Geo {
        Geo::from_polygons(
            vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            vec![3],
            vec![0, 1, 2],
        )
    }

    /// Packed into a compound, the nodes run as they did: the graph gives the
    /// same, and the wires that crossed the selection are its ports, an
    /// Execute among them.
    #[test]
    fn a_compound_runs_as_its_nodes_did() {
        let (mut g, add, set) = lifting();
        let before = ys(&g.evaluate(&triangle(), &[]));
        let id = make_compound(&mut g, &[add, set], "Lift").unwrap();
        let node = g.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(node.inputs.len(), 1, "P comes in");
        assert_eq!(node.outputs[0].value_hint, "Execute");
        assert!(g.nodes.iter().all(|n| n.id != add && n.id != set));
        assert_eq!(ys(&g.evaluate(&triangle(), &[])), before);
        assert!(g.error.lock().unwrap().is_none());
    }

    /// Inside, an edit is run at once and kept when leaving; a locked inside
    /// puts a change back.
    #[test]
    fn going_inside_changes_what_runs() {
        let (mut g, add, set) = lifting();
        let id = make_compound(&mut g, &[add, set], "Lift").unwrap();
        let base = ys(&triangle());
        enter(&mut g, id).unwrap();
        assert_eq!(g.entered(), (vec!["Lift".to_string()], false));
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == add) {
            n.inputs[1].value = Some(Value::Vec3([0.0, 3.0, 0.0]));
        }
        let lifted = |g: &IceGraph| ys(&g.evaluate(&triangle(), &[]));
        let three: Vec<f32> = base.iter().map(|y| y + 3.0).collect();
        assert_eq!(lifted(&g), three, "run with the inside as edited");
        assert!(leave(&mut g).is_some());
        assert!(g.outer.is_empty());
        assert_eq!(lifted(&g), three, "kept");

        // Locked: what is changed inside is put back.
        enter(&mut g, id).unwrap();
        g.outer.last_mut().unwrap().locked = true;
        g.outer.last_mut().unwrap().pristine = Some(crate::graph_io::ice_to_value(&g));
        g.nodes.retain(|n| n.id != add);
        assert!(keep_locked(&mut g));
        assert!(g.nodes.iter().any(|n| n.id == add), "put back");
        assert!(!keep_locked(&mut g));
    }

    /// Saved and read back, a graph keeps its compound; an inside being looked
    /// at is saved folded into its instance.
    #[test]
    fn a_compound_is_saved_with_its_graph() {
        let (mut g, add, set) = lifting();
        let id = make_compound(&mut g, &[add, set], "Lift").unwrap();
        enter(&mut g, id).unwrap();
        let text = crate::graph_io::ice_to_value(&g.composed());
        let back = crate::graph_io::ice_from_value(&text).unwrap();
        assert!(back
            .nodes
            .iter()
            .any(|n| matches!(n.node_type, IceGraphNodeType::Compound { .. })));
        assert_eq!(ys(&back.evaluate(&triangle(), &[])), ys(&g.evaluate(&triangle(), &[])));
    }

    /// Inside, an input promoted is a port of the compound, its value the
    /// input's; set on the instance, it is what runs.
    #[test]
    fn an_input_promoted_is_a_port() {
        let (mut g, add, set) = lifting();
        let id = make_compound(&mut g, &[add, set], "Lift").unwrap();
        enter(&mut g, id).unwrap();
        let told = promote_input(&mut g, add, 1).unwrap();
        assert!(told.contains("Add B"), "{told}");
        leave(&mut g).unwrap();
        let node = g.nodes.iter_mut().find(|n| n.id == id).unwrap();
        let k = node.inputs.iter().position(|i| i.name == "Add B").unwrap();
        assert_eq!(node.inputs[k].value, Some(Value::Vec3([0.0, 1.0, 0.0])));
        node.inputs[k].value = Some(Value::Vec3([0.0, 4.0, 0.0]));
        let base = ys(&triangle());
        assert_eq!(
            ys(&g.evaluate(&triangle(), &[])),
            base.iter().map(|y| y + 4.0).collect::<Vec<_>>()
        );
    }

    /// At the top of an ICE operator's graph, an input promoted is a parameter
    /// of the operator: what the operator sets is what runs.
    #[test]
    fn an_input_promoted_at_the_top_is_a_parameter() {
        let (mut g, add, _) = lifting();
        let told = promote_input(&mut g, add, 1).unwrap();
        assert!(told.contains("parameter"), "{told}");
        assert_eq!(
            parameters_of(&g),
            vec![("Add B".to_string(), Value::Vec3([0.0, 1.0, 0.0]))]
        );
        crate::ice::set_params(
            [(
                "Add B".to_string(),
                serde_json::to_value(Value::Vec3([0.0, 6.0, 0.0])).unwrap(),
            )]
            .into_iter()
            .collect(),
        );
        let base = ys(&triangle());
        assert_eq!(
            ys(&g.evaluate(&triangle(), &[])),
            base.iter().map(|y| y + 6.0).collect::<Vec<_>>()
        );
    }

    /// A setting promoted at the top of a graph is a parameter of the ICE
    /// operator: its value is the setting's; the setting follows it.
    #[test]
    fn a_setting_promoted_follows_its_parameter() {
        let mut g = IceGraph::new(IceGraphId(0), "g".into());
        let r = g.add_node(
            "Random".into(),
            IceGraphNodeType::Random { seed: 3 },
            egui::pos2(0.0, 0.0),
        );
        let told = promote_setting(&mut g, r, "seed", "", vec![]).unwrap();
        assert!(told.contains("ICE operator"), "{told}");
        assert!(promote_setting(&mut g, r, "seed", "", vec![]).is_err(), "once");
        let p = &g.parameters[0];
        assert_eq!(
            (p.label.as_str(), p.kind, p.default.clone()),
            ("Random Seed", ParameterType::Integer, serde_json::json!(3))
        );
        let values: BTreeMap<String, serde_json::Value> = [(p.name.clone(), serde_json::json!(7))].into();
        let set = with_values(&g, &g.parameters, &values);
        assert!(set
            .nodes
            .iter()
            .any(|n| matches!(n.node_type, IceGraphNodeType::Random { seed: 7 })));
        remove_promotion(&mut g, "Random", "seed");
        assert!(g.parameters.is_empty());
    }

    /// A row of Get Reference's paths promotes them all, a list, in the group
    /// it was promoted with; a switch's options come with it.
    #[test]
    fn a_list_setting_is_promoted_whole() {
        let mut g = IceGraph::new(IceGraphId(0), "g".into());
        let r = g.add_node(
            "Ref".into(),
            IceGraphNodeType::GetReference {
                input: 1,
                paths: vec!["/a".into(), "/b".into()],
            },
            egui::pos2(0.0, 0.0),
        );
        promote_setting(&mut g, r, "paths/1", "References", vec![]).unwrap();
        let p = &g.parameters[0];
        assert_eq!((p.kind, p.group.as_str()), (ParameterType::List, "References"));
        assert_eq!(p.targets[0].path, "paths");
        assert_eq!(p.default, serde_json::json!(["/a", "/b"]));
        let choices = vec![
            ("One".to_string(), serde_json::json!(1)),
            ("Two".to_string(), serde_json::json!(2)),
        ];
        promote_setting(&mut g, r, "input", "References", choices).unwrap();
        assert_eq!(g.parameters[1].kind, ParameterType::Choice);
        assert_eq!(g.parameters[1].choices.len(), 2);
    }

    /// Nodes are named apart; two of one name (a graph from before) and one
    /// promoted, that one is renamed so its promotion is its own.
    #[test]
    fn a_setting_of_a_node_named_as_another_promotes() {
        let mut g = IceGraph::new(IceGraphId(0), "g".into());
        let paths = |p: &str| IceGraphNodeType::GetReference {
            input: 1,
            paths: vec![p.into()],
        };
        let a = g.add_node("Ref".into(), paths("/a"), egui::pos2(0.0, 0.0));
        let b = g.add_node("Ref".into(), paths("/b"), egui::pos2(0.0, 0.0));
        let name = |g: &IceGraph, id: NodeId| g.nodes.iter().find(|n| n.id == id).unwrap().name.clone();
        assert_eq!((name(&g, a), name(&g, b)), ("Ref".to_string(), "Ref2".to_string()));
        g.nodes.iter_mut().find(|n| n.id == b).unwrap().name = "Ref".into();
        let told = promote_setting(&mut g, b, "paths/0", "", vec![]).unwrap();
        assert!(told.starts_with("Renamed Ref to Ref2"), "{told}");
        assert_eq!(g.parameters[0].targets[0].node, "Ref2");
        assert_eq!(g.parameters[0].default, serde_json::json!(["/b"]));
    }

    /// Inside a compound, a setting promoted is a parameter of the compound:
    /// each instance's value is what runs, the definition keeping the default.
    #[test]
    fn a_setting_promoted_inside_is_the_compounds() {
        let (mut g, add, set) = lifting();
        let r = g.add_node(
            "Random".into(),
            IceGraphNodeType::Random { seed: 3 },
            egui::pos2(0.0, 300.0),
        );
        let id = make_compound(&mut g, &[add, set, r], "Lift").unwrap();
        enter(&mut g, id).unwrap();
        promote_setting(&mut g, r, "seed", "", vec![]).unwrap();
        leave(&mut g).unwrap();
        let d = definition(&g.nodes.iter().find(|n| n.id == id).unwrap().node_type)
            .unwrap()
            .into_owned();
        assert_eq!(d.parameters.len(), 1);
        let name = d.parameters[0].name.clone();
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) {
            if let IceGraphNodeType::Compound { values, .. } = &mut n.node_type {
                values.insert(name, serde_json::json!(9));
            }
        }
        let flat = flatten(&g).unwrap();
        assert!(flat
            .graph
            .nodes
            .iter()
            .any(|n| matches!(n.node_type, IceGraphNodeType::Random { seed: 9 })));
    }

    /// An error inside a compound is kept with the way to the node that
    /// broke: the compound, then the node inside it.
    #[test]
    fn an_error_inside_is_found_inside() {
        let (mut g, add, set) = lifting();
        let id = make_compound(&mut g, &[add, set], "Lift").unwrap();
        enter(&mut g, id).unwrap();
        // Set Data of a name no context has, from a boolean: it cannot run.
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == add) {
            n.inputs[1].value = Some(Value::Bool(true));
        }
        g.evaluate(&triangle(), &[]);
        let top = &g.outer[0].graph;
        let way = top.broke.lock().unwrap().clone();
        assert!(top.error.lock().unwrap().is_some());
        assert_eq!(way.first(), Some(&id));
        assert_eq!(way.len(), 2);
    }

    /// A compound inside itself is refused, the error on the instance.
    #[test]
    fn a_compound_cannot_hold_itself() {
        let (mut g, add, set) = lifting();
        let id = make_compound(&mut g, &[add, set], "Lift").unwrap();
        let me = g.nodes.iter().find(|n| n.id == id).unwrap().node_type.clone();
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) {
            if let IceGraphNodeType::Compound { editable: Some(d), .. } = &mut n.node_type {
                let mut inner = crate::graph_io::ice_from_value(&d.graph).unwrap();
                inner.add_node("Again".into(), me, egui::pos2(0.0, 300.0));
                d.graph = crate::graph_io::ice_to_value(&inner);
            }
        }
        let err = flatten(&g).err().unwrap();
        assert_eq!(err.0[0], id);
        assert!(err.1.contains("inside itself"), "{}", err.1);
    }

    /// A Parameter node gives what its operator sets, else its default.
    #[test]
    fn a_parameter_takes_the_operators_value() {
        let (mut g, add, _) = lifting();
        let lift = g.add_node(
            "Lift".into(),
            IceGraphNodeType::Parameter {
                name: "lift".into(),
                default: Value::Vec3([0.0, 2.0, 0.0]),
            },
            egui::pos2(0.0, 200.0),
        );
        g.add_connection(lift, 0, add, 1);
        let base = ys(&triangle());
        assert_eq!(
            ys(&g.evaluate(&triangle(), &[])),
            base.iter().map(|y| y + 2.0).collect::<Vec<_>>()
        );
        crate::ice::set_params(
            [(
                "lift".to_string(),
                serde_json::to_value(Value::Vec3([0.0, 5.0, 0.0])).unwrap(),
            )]
            .into_iter()
            .collect(),
        );
        assert_eq!(
            ys(&g.evaluate(&triangle(), &[])),
            base.iter().map(|y| y + 5.0).collect::<Vec<_>>()
        );
    }
}
