pub mod ui;

use crate::types::{EvalResult, MeshData, NodeId, Prim, PrimVar, PrimVarInterp};

/// What the Primitive Inspector shows: the scene at the operator with the
/// view flag, all of it, or the rows selected in the Scene Explorer and what
/// is below them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Source {
    pub node:  Option<NodeId>,
    pub paths: Vec<String>,
}

/// The primitives of a scene at or below any of `paths` (all of them with
/// none), each where it is drawn. `/set` takes `/set` and `/set/a`, not
/// `/settle`.
pub fn scene_parts(scene: EvalResult, paths: &[String]) -> Vec<MeshData> {
    match scene {
        EvalResult::Named(prims) => {
            let under = |q: &str| paths.is_empty() || paths.iter().any(|p| {
                let p = p.trim_end_matches('/');
                q == p || q.strip_prefix(p).is_some_and(|rest| rest.starts_with('/'))
            });
            prims.iter().filter(|x| under(&x.path)).map(part).collect()
        }
        scene => scene.into_parts(),
    }
}

/// One primitive where it is drawn, with the Object values it inherits
/// from the prims above it (Set Attribute on a group) among its Object
/// attributes.
fn part(p: &Prim) -> MeshData {
    let mut m = p.world().into_owned();
    for (name, value) in crate::core::props::inherited_primvars(p) {
        if let Some(n) = value.numbers() {
            m.primvars.push(PrimVar { name, interp: PrimVarInterp::Constant, values: vec![n] });
        }
    }
    m
}

// ── Properties ──────────────────────────────────────────────────────────────

/// One row of the Properties tab.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropRow {
    pub path:  String,
    pub name:  String,
    pub value: String,
    /// Where the value comes from: `file`, `Imago`, the Set Attribute
    /// operator that set it, or `inherited: /path` from a prim above.
    pub from:  String,
}

/// The Properties tab: the selected prims themselves (a group, a camera or
/// a light as well as a mesh), or the root prims with nothing selected.
/// `view`: the operator with the view flag, whose upstream operators are
/// searched for the Set Attribute that set a value.
pub fn scene_properties(graph: &crate::node_graph::NodeGraphState, view: NodeId, scene: &EvalResult, paths: &[String]) -> Vec<PropRow> {
    use crate::core::props::{self, Opinions, SetContext};
    let EvalResult::Named(prims) = scene else { return vec![] };
    let mut trees: Vec<&std::sync::Arc<crate::usd_scene::StageTree>> = vec![];
    for p in prims {
        if let Some(t) = &p.stage { if !trees.iter().any(|x| std::sync::Arc::ptr_eq(x, t)) { trees.push(t); } }
    }
    let node_of = |path: &str| trees.iter().copied()
        .find_map(|t| t.nodes.iter().find(|n| n.path == path).map(|n| (n, &**t)));
    // The Xforms Imago makes above its own primitives.
    let made: Vec<String> = {
        let mut v: Vec<String> = vec![];
        for p in prims.iter().filter(|p| p.stage.is_none()) {
            for a in props::parents(&p.path) { if !v.contains(&a) && node_of(&a).is_none() { v.push(a); } }
        }
        v
    };
    // What to list: the selection, or the root prims.
    let listed: Vec<String> = if !paths.is_empty() { paths.to_vec() } else {
        let mut roots: Vec<String> = trees.iter().flat_map(|t| t.nodes.iter().filter(|n| n.depth == 1).map(|n| n.path.clone())).collect();
        for p in prims {
            let root = props::parents(&p.path).into_iter().next().unwrap_or_else(|| p.path.clone());
            if !roots.contains(&root) { roots.push(root); }
        }
        roots
    };
    let setter = |path: &str, name: &str, context: SetContext| set_by(graph, view, path, name, context).unwrap_or_else(|| "Set Attribute".into());

    let mut rows = vec![];
    for path in &listed {
        let mut row = |name: &str, value: String, from: String| rows.push(PropRow { path: path.clone(), name: name.into(), value, from });
        let prim = prims.iter().find(|p| &p.path == path);
        let node = node_of(path);
        let imago_xform = made.contains(path);
        if prim.is_none() && node.is_none() && !imago_xform { continue; }
        let origin = if node.is_some() { "file" } else { "Imago" };

        // What the prim is.
        let ty = match (node, prim) {
            (Some((n, _)), _) => n.type_name.clone(),
            (None, Some(p)) => match p.geo.as_ref().map(|g| &g.topology) {
                Some(crate::core::geo::Topology::Curves { .. }) => "BasisCurves".into(),
                Some(crate::core::geo::Topology::Points) => "Points".into(),
                _ => "Mesh".into(),
            },
            (None, None) => "Xform".into(),
        };
        row("type", ty, origin.into());

        // Its own values: those of a primitive, of a stage prim, of an Xform Imago made.
        let own: Opinions = match (prim, node) {
            (Some(p), _) => Opinions { primvars: Default::default(), props: (*p.props).clone() },
            (None, Some((_, tree))) => tree.opinions.get(path).cloned().unwrap_or_default(),
            (None, None) => prims.iter().find_map(|p| p.above.get(path).cloned()).unwrap_or_default(),
        };
        // What the prims above it set, nearest winning.
        let mut inherited: std::collections::BTreeMap<String, (String, String)> = Default::default();
        let mut above: Vec<(String, Opinions)> = vec![];
        if let Some((_, tree)) = node {
            above.extend(tree.opinions.iter().filter(|(k, _)| *k != path && path.starts_with(&format!("{k}/"))).map(|(k, o)| (k.clone(), o.clone())));
        }
        if let Some(p) = prims.iter().find(|p| p.stage.is_none() && (p.path == *path || p.path.starts_with(&format!("{path}/")))) {
            above.extend(p.above.iter().filter(|(k, _)| *k != path && path.starts_with(&format!("{k}/"))).map(|(k, o)| (k.clone(), o.clone())));
        }
        above.sort_by_key(|(k, _)| k.matches('/').count());
        for (k, o) in &above {
            for (name, v) in &o.props { inherited.insert(name.clone(), (v.display(), format!("inherited: {k}"))); }
        }

        // The basics every prim has.
        match prim {
            Some(p) => {
                let purpose = |x: crate::types::Purpose| match x {
                    crate::types::Purpose::Default => "default", crate::types::Purpose::Render { .. } => "render",
                    crate::types::Purpose::Proxy => "proxy", crate::types::Purpose::Guide => "guide",
                };
                let set = p.source.as_ref().map_or(p.purpose != crate::types::Purpose::Default, |s| s.purpose != p.purpose);
                match (p.purpose, inherited.get("purpose")) {
                    (crate::types::Purpose::Default, Some((v, from))) => row("purpose", v.clone(), from.clone()),
                    (x, _) => row("purpose", purpose(x).into(), if set { setter(path, "purpose", SetContext::Property) } else { origin.into() }),
                }
                let bound = p.material.clone().unwrap_or_else(|| "none".into());
                let rebound = p.source.as_ref().is_some_and(|s| s.material != p.material);
                row("material:binding", bound, if rebound { "Imago".into() } else { origin.into() });
            }
            None => if let Some((v, from)) = inherited.get("purpose") { row("purpose", v.clone(), from.clone()); },
        }
        match (own.props.get("visibility"), inherited.get("visibility"), node) {
            (Some(v), _, _) => row("visibility", v.display(), setter(path, "visibility", SetContext::Property)),
            (None, Some((v, from)), _) if v == "invisible" => row("visibility", v.clone(), from.clone()),
            (None, _, Some((n, _))) if n.hidden => row("visibility", "invisible".into(), "file".into()),
            _ => row("visibility", "inherited".into(), origin.into()),
        }
        if !own.props.contains_key("active") { row("active", "true".into(), origin.into()); }

        // Values Set Attribute set on it, then those it inherits.
        for (name, v) in &own.props {
            if name == "visibility" { continue; }
            row(name, v.display(), setter(path, name, SetContext::Property));
        }
        for (name, v) in &own.primvars {
            row(&format!("primvars:{name}"), v.display(), setter(path, name, SetContext::Object));
        }
        for (name, (v, from)) in &inherited {
            if matches!(name.as_str(), "visibility" | "purpose" | "active") || own.props.contains_key(name) { continue; }
            row(name, v.clone(), from.clone());
        }
    }
    rows
}

/// The Set Attribute operator nearest upstream of `view` with a row setting
/// `name` in this context on `path`: one with no Primitive Locations, or
/// with one naming the path.
pub fn set_by(graph: &crate::node_graph::NodeGraphState, view: NodeId, path: &str, name: &str, context: crate::core::props::SetContext) -> Option<String> {
    use crate::types::NodeType;
    let mut queue = std::collections::VecDeque::from([view]);
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) { continue; }
        let Some(n) = graph.nodes.iter().find(|n| n.id == id) else { continue };
        if let NodeType::SetAttribute { rows } = &n.node_type {
            let sets = rows.iter().any(|r| r.context == context && r.name.trim() == name && r.is_usable());
            let here = !crate::core::locations::is_set(&n.locations) || crate::core::locations::any_names(&n.locations, path);
            if sets && here && !n.bypassed { return Some(n.name.clone()); }
        }
        queue.extend(n.inputs.iter().filter_map(|i| i.connected_output.map(|(src, _)| src)));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Prim;

    #[test]
    fn a_selected_row_shows_what_is_at_or_below_it() {
        let cube = || crate::node_graph::nodes::create_cube(1.0);
        let scene = || EvalResult::Named(["/set/a", "/set/grp/b", "/settle", "/other"].iter()
            .map(|p| Prim::new(p.to_string(), cube())).collect());
        let sel = |p: &[&str]| p.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(scene_parts(scene(), &[]).len(), 4, "nothing selected: the whole scene");
        assert_eq!(scene_parts(scene(), &sel(&["/set"])).len(), 2, "a group: what is below it, not /settle");
        assert_eq!(scene_parts(scene(), &sel(&["/set/grp/b"])).len(), 1, "a prim: itself");
        assert_eq!(scene_parts(scene(), &sel(&["/set/a", "/other"])).len(), 2, "several rows: all of them");
        assert!(scene_parts(scene(), &sel(&["/gone"])).is_empty());
        // A plain mesh has no path: always all of it.
        assert_eq!(scene_parts(EvalResult::Single(cube()), &sel(&["/set"])).len(), 1);
    }

    /// The Properties tab of a group Imago made, and of a cube below it:
    /// what each has, and where it comes from.
    #[test]
    fn properties_say_what_a_prim_has_and_where_from() {
        use crate::core::props::{SetContext, SetRow, Value};
        use crate::types::NodeType;
        let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: Option<&crate::core::geo::Geo>| m.clone();
        let mut g = crate::node_graph::NodeGraphState::default();
        let cube = g.add_node("Cube".into(), NodeType::CreateCube { size: 1.0, location: "/root/geo/cube".into() }, egui::pos2(0.0, 0.0));
        let set = g.add_node("Hide".into(), NodeType::SetAttribute { rows: vec![
            SetRow { context: SetContext::Property, name: "visibility".into(), value: Value::Token("invisible".into()) },
            SetRow { context: SetContext::Property, name: "kind".into(), value: Value::Token("group".into()) },
        ] }, egui::pos2(0.0, 100.0));
        g.add_connection(cube, 0, set, 0);
        g.nodes.iter_mut().find(|n| n.id == set).unwrap().locations = vec!["/root/geo".into()];
        g.set_view_flag(set);
        let (view, _, scene) = g.evaluate_scene(&pass).unwrap();
        let find = |rows: &[PropRow], name: &str| rows.iter().find(|r| r.name == name).map(|r| (r.value.clone(), r.from.clone()));

        let group = scene_properties(&g, view, &scene, &["/root/geo".to_string()]);
        assert_eq!(find(&group, "type"), Some(("Xform".into(), "Imago".into())));
        assert_eq!(find(&group, "visibility"), Some(("invisible".into(), "Hide".into())));
        assert_eq!(find(&group, "kind"), Some(("group".into(), "Hide".into())));

        let cube_rows = scene_properties(&g, view, &scene, &["/root/geo/cube".to_string()]);
        assert_eq!(find(&cube_rows, "type"), Some(("Mesh".into(), "Imago".into())));
        assert_eq!(find(&cube_rows, "visibility"), Some(("invisible".into(), "inherited: /root/geo".into())));
        assert_eq!(find(&cube_rows, "kind"), Some(("group".into(), "inherited: /root/geo".into())));
        assert_eq!(find(&cube_rows, "purpose"), Some(("default".into(), "Imago".into())));

        // Nothing selected: the root prims.
        let roots = scene_properties(&g, view, &scene, &[]);
        assert!(roots.iter().all(|r| r.path == "/root") && !roots.is_empty());
    }
}
