//! Going inside a compound instance (see `docs/compounds.md`).
//!
//! Entering takes the graph the instance is in, with its ICE graphs, its undo
//! history and which ICE or MaterialX graph was open, out of the program's
//! state, and puts the graph inside the instance in their place: every pane
//! (the Node Graph, the Viewer, the Scene Explorer, the Parameters) then works
//! on it as on any graph. Its Compound Input operators are fed what comes into
//! the instance, as it was when entered. Leaving puts the outer graph back: an
//! editable instance takes the graph inside as its definition (its ports from
//! its Compound Input and Compound Output operators, its promoted values from
//! their targets), a locked one is as it was. Inside a locked instance a
//! change is put back at once.

use std::sync::Arc;

use crate::compounds::Definition;
use crate::history::History;
use crate::ice::{GraphNavigation, IceGraphStore};
use crate::node_graph::compound::{self, Fed, FedPort, Request};
use crate::node_graph::{EvalCache, NodeGraphState};
use crate::types::{IceGraphId, NodeId, NodeType};

/// The graphs waiting while an instance's inside is looked at, the outermost
/// first.
#[derive(Default)]
pub struct Inside {
    frames: Vec<Frame>,
}

/// A graph waiting while the inside of one of its instances is looked at.
struct Frame {
    instance: NodeId,
    graph: NodeGraphState,
    subnets: IceGraphStore,
    history: History,
    nav: GraphNavigation,
    /// Locked: the inside as it was entered, put back when it is changed.
    pristine: Option<String>,
    /// The inside's content hash when last compared with `pristine`.
    seen: u64,
}

impl Inside {
    /// Whether an instance's inside is being looked at.
    pub fn is_inside(&self) -> bool {
        !self.frames.is_empty()
    }
}

/// What the panes see: the graph, its ICE graphs, its history and which graph
/// inside an operator is open. Inside a compound, the compound's.
pub struct Panes<'a> {
    pub graph: &'a mut NodeGraphState,
    pub subnets: &'a mut IceGraphStore,
    pub history: &'a mut History,
    pub nav: &'a mut GraphNavigation,
    pub inside: &'a mut Inside,
}

/// Carry out what the panes asked of compounds this frame (going in, out,
/// making the instance editable), and put back a change made inside a locked
/// one. Returns what to tell.
pub fn frame(p: Panes) -> Option<String> {
    let told = match p.graph.inside.request.take() {
        Some(Request::Enter(op)) => {
            Some(enter(p.graph, p.subnets, p.history, p.nav, p.inside, op).unwrap_or_else(|e| e))
        }
        Some(Request::Leave) => leave(p.graph, p.subnets, p.history, p.nav, p.inside),
        Some(Request::LeaveAll) => leave_all(p.graph, p.subnets, p.history, p.nav, p.inside),
        Some(Request::MakeEditable) => make_editable(p.graph, p.inside),
        None => None,
    };
    let Some(top) = p.inside.frames.last_mut() else {
        return told;
    };
    let Some(pristine) = &top.pristine else { return told };
    let now = p.graph.content_hash();
    if now == top.seen {
        return told;
    }
    top.seen = now;
    if crate::graph_io::to_json(p.graph, p.subnets) == *pristine {
        return told;
    }
    // Locked: as it was, its feed, path and the view kept.
    let (fed, inside, scene, variables) = (
        p.graph.fed.clone(),
        p.graph.inside.clone(),
        p.graph.scene,
        p.graph.variables.clone(),
    );
    let mut fresh = IceGraphStore::default();
    if crate::graph_io::from_json(p.graph, &mut fresh, pristine).is_ok() {
        *p.subnets = fresh;
    }
    p.graph.fed = fed;
    p.graph.inside = inside;
    p.graph.scene = scene;
    p.graph.variables = variables;
    top.seen = p.graph.content_hash();
    Some("Locked: it follows its definition. Make Editable to change it here".into())
}

/// Go inside instance `op` of `graph`. Returns what to tell.
pub fn enter(
    graph: &mut NodeGraphState,
    subnets: &mut IceGraphStore,
    history: &mut History,
    nav: &mut GraphNavigation,
    inside: &mut Inside,
    op: NodeId,
) -> Result<String, String> {
    let node = graph.nodes.iter().find(|n| n.id == op).ok_or("That operator is gone")?;
    let NodeType::Compound {
        definition,
        values,
        editable,
        ..
    } = &node.node_type
    else {
        return Err(format!("{} is not a compound", node.name));
    };
    let locked = editable.is_none();
    let d: Definition = compound::with_definition(&node.node_type, |d| d.clone()).ok_or_else(|| {
        format!("The compound {definition} was not found in the library: there is nothing inside to show")
    })?;
    let mut inner = compound::read_inner(&d)?;
    compound::apply_values(&mut inner.graph, &d, values);

    // What comes into the instance, cooked now.
    let outer_subnets = &*subnets;
    let eval = |sid: IceGraphId, mesh: &crate::core::geo::Geo, refs: &[crate::ice::Reference]| {
        outer_subnets
            .get(sid)
            .map(|sg| sg.evaluate(mesh, refs))
            .unwrap_or_else(|| mesh.clone())
    };
    let mut cache = EvalCache::new();
    let ports: Vec<FedPort> = node
        .inputs
        .iter()
        .map(|s| FedPort {
            name: s.name.clone(),
            result: s
                .connected_output
                .and_then(|(src, out)| graph.eval_node_out(src, out, &mut cache, &eval)),
            key: s.connected_output.and_then(|(src, out)| graph.cook_key(src, out)),
        })
        .collect();
    let mut above: Vec<(String, String)> = vec![("instance".into(), node.name.clone())];
    above.extend(
        graph
            .from_above(op)
            .into_iter()
            .filter(|a| a.name != "instance")
            .map(|a| (a.name, a.value)),
    );
    // Inside a compound inside a compound, what the outer instance gives too.
    if let Some(fed) = &graph.fed {
        for (name, value) in &fed.above {
            if !above.iter().any(|(n, _)| n == name) {
                above.push((name.clone(), value.clone()));
            }
        }
    }

    let g = &mut inner.graph;
    g.fed = Some(Arc::new(Fed { ports, above }));
    g.cooked = graph.cooked.clone();
    g.scene = graph.scene;
    g.variables = graph.variables.clone();
    g.time = graph.time;
    g.inside.path = graph
        .inside
        .path
        .iter()
        .cloned()
        .chain(std::iter::once(node.name.clone()))
        .collect();
    g.inside.locked = locked;
    g.inside.parameters = d.parameters.clone();
    g.frame_request = true;
    let pristine = locked.then(|| crate::graph_io::to_json(g, &inner.subnets));
    let seen = g.content_hash();
    let name = node.name.clone();

    inside.frames.push(Frame {
        instance: op,
        graph: std::mem::replace(graph, inner.graph),
        subnets: std::mem::replace(subnets, inner.subnets),
        history: std::mem::take(history),
        nav: std::mem::take(nav),
        pristine,
        seen,
    });
    Ok(if locked {
        format!("Inside {name}: locked, it follows its definition {}", d.name)
    } else {
        format!("Inside {name}: ↑ goes back out")
    })
}

/// Back out of the instance being looked at. An editable one takes the graph
/// inside as its definition. Returns what to tell; None when not inside.
pub fn leave(
    graph: &mut NodeGraphState,
    subnets: &mut IceGraphStore,
    history: &mut History,
    nav: &mut GraphNavigation,
    inside: &mut Inside,
) -> Option<String> {
    let frame = inside.frames.pop()?;
    let mut g = std::mem::replace(graph, frame.graph);
    let inner_subnets = std::mem::replace(subnets, frame.subnets);
    *history = frame.history;
    *nav = frame.nav;
    graph.inside.request = None;
    graph.frame_request = true;
    if frame.pristine.is_some() {
        return Some("Back out of the compound".into());
    }
    let node = graph.nodes.iter_mut().find(|n| n.id == frame.instance)?;
    let name = node.name.clone();
    let NodeType::Compound {
        editable: Some(d),
        values,
        ..
    } = &mut node.node_type
    else {
        return None;
    };
    // Its parameters as promoted inside; the definition keeps its defaults,
    // the instance its values.
    d.parameters = std::mem::take(&mut g.inside.parameters);
    compound::take_values(&mut g, d, values);
    g.scene = Default::default();
    g.variables = Default::default();
    let (ins, outs) = compound::ports_of(&g, (d.inputs.as_slice(), d.outputs.as_slice()));
    d.inputs = ins.clone();
    d.outputs = outs.clone();
    d.graph = serde_json::from_str(&crate::graph_io::to_json(&g, &inner_subnets)).unwrap_or_default();
    graph.rewire(
        frame.instance,
        ins.into_iter().map(|p| p.name).collect(),
        outs.into_iter().map(|p| p.name).collect(),
    );
    Some(format!("Back out of {name}: its graph is kept with it"))
}

/// Back out of every instance entered, to the root scene graph: before the
/// graph is saved, replaced or closed. Returns what to tell.
pub fn leave_all(
    graph: &mut NodeGraphState,
    subnets: &mut IceGraphStore,
    history: &mut History,
    nav: &mut GraphNavigation,
    inside: &mut Inside,
) -> Option<String> {
    let mut told = None;
    while inside.is_inside() {
        told = leave(graph, subnets, history, nav, inside).or(told);
    }
    told
}

/// Make the instance being looked at editable, from inside it: its own copy of
/// the definition, changed as the graph inside is.
fn make_editable(graph: &mut NodeGraphState, inside: &mut Inside) -> Option<String> {
    let top = inside.frames.last_mut()?;
    let node = top.graph.nodes.iter_mut().find(|n| n.id == top.instance)?;
    let d = compound::with_definition(&node.node_type, |d| d.clone())?;
    if let NodeType::Compound { editable, .. } = &mut node.node_type {
        *editable = Some(Box::new(d));
    }
    top.pristine = None;
    graph.inside.locked = false;
    Some(format!(
        "{} is editable: its own copy of the definition, no longer the library's",
        node.name
    ))
}
