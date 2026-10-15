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
///
/// Instanced geometry is listed once, in its own space, not once per copy:
/// a point instancer's prototypes (placed at its points), and the meshes of
/// native instances (shared by every instance of a prototype). A point
/// instancer itself is listed as its points, one per instance. So what is
/// listed grows with the geometry there is, not with the copies drawn.
pub fn scene_parts(scene: EvalResult, paths: &[String]) -> Vec<MeshData> {
    match scene {
        EvalResult::Named(prims) => {
            // A point instancer is listed as its instances, one point each.
            // Its prototypes are listed only when they are selected
            // themselves (a row below the instancer): selecting the
            // instancer, a group above it or nothing lists its points, not
            // the geometry drawn at them.
            let instancers: Vec<&str> = prims.iter().filter(|p| p.instancing.is_some()).map(|p| p.path.as_str()).collect();
            let below = |q: &str, p: &str| q.strip_prefix(p.trim_end_matches('/')).is_some_and(|rest| rest.starts_with('/'));
            let at_or_below = |q: &str, p: &str| q == p.trim_end_matches('/') || below(q, p);
            let wanted = |x: &Prim| {
                let instancer = x.instanced_by.as_ref().map(|b| b.instancer.as_str())
                    .or_else(|| instancers.iter().copied().find(|i| below(&x.path, i)));
                match instancer {
                    Some(i) => paths.iter().any(|p| at_or_below(&x.path, p) && below(p.trim_end_matches('/'), i)),
                    None => paths.is_empty() || paths.iter().any(|p| at_or_below(&x.path, p)),
                }
            };
            let mut seen: std::collections::HashSet<*const MeshData> = std::collections::HashSet::new();
            prims.iter().filter(|x| wanted(x)).filter_map(|p| {
                if !instanced(p) { return Some(part(p, true)); }
                seen.insert(std::sync::Arc::as_ptr(&p.mesh)).then(|| part(p, false))
            }).collect()
        }
        scene => scene.into_parts(),
    }
}

/// Whether a primitive is instanced geometry, or a point instancer: listed
/// once, in its own space.
pub fn instanced(p: &Prim) -> bool {
    matches!(p.place, crate::types::Placement::Many(_)) || p.instancing.is_some()
        || p.stage.as_ref().is_some_and(|t| t.node(&p.path).is_some_and(|n| n.proxy))
}

/// How many copies of a primitive's geometry the scene draws: a prototype's
/// points, or the instances sharing a native instance's mesh.
pub fn copies(p: &Prim, scene: &[Prim]) -> usize {
    match &p.place {
        crate::types::Placement::Many(m) => m.len(),
        _ => scene.iter().filter(|q| std::sync::Arc::ptr_eq(&q.mesh, &p.mesh)).count(),
    }
}

/// One primitive, where it is drawn (`placed`) or in its own space, with
/// the Object values it inherits from the prims above it (Set Attribute on
/// a group) among its Object attributes.
fn part(p: &Prim, placed: bool) -> MeshData {
    let mut m = if placed { p.world().into_owned() } else { (*p.mesh).clone() };
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
    /// The operator it comes from, for "Go to Operator": the Load USD that
    /// read the file, the shape that made the prim, or the Set Attribute
    /// that set the value (on the prim, or on the group it is inherited from).
    pub node:  Option<NodeId>,
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
    // The Set Attribute that set a value, by name, and its operator.
    let setter = |path: &str, name: &str, context: SetContext| match set_by(graph, view, path, name, context) {
        Some((id, n)) => (n, Some(id)),
        None => ("Set Attribute".to_string(), None),
    };
    // The group a value is inherited from: the operator that set it there.
    let inherited_from = |from: &str, name: &str| from.strip_prefix("inherited: ")
        .and_then(|group| set_by(graph, view, group, name, SetContext::Property).map(|(id, _)| id));

    let mut rows = vec![];
    for path in &listed {
        let node = node_of(path);
        // The operator a value read from the file, or made in Imago, comes from.
        let maker = match node {
            Some((_, tree)) => load_of(graph, view, tree),
            None => shape_of(graph, view, path),
        };
        let mut row = |name: &str, value: String, (from, op): (String, Option<NodeId>)| {
            rows.push(PropRow { path: path.clone(), name: name.into(), value, from, node: op });
        };
        let prim = prims.iter().find(|p| &p.path == path);
        let imago_xform = made.contains(path);
        if prim.is_none() && node.is_none() && !imago_xform { continue; }
        let origin = (if node.is_some() { "file" } else { "Imago" }).to_string();
        let made_here = || (origin.clone(), maker);

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
        row("type", ty, made_here());

        // Its own values: those of a primitive, of a stage prim, of an Xform Imago made.
        let own: Opinions = match (prim, node) {
            (Some(p), _) => Opinions { primvars: Default::default(), props: (*p.props).clone() },
            (None, Some((_, tree))) => tree.opinions.get(path).cloned().unwrap_or_default(),
            (None, None) => prims.iter().find_map(|p| p.above.get(path).cloned()).unwrap_or_default(),
        };
        // What the file authors on it.
        let file_values: Vec<(String, crate::core::props::Value)> = node.map(|(n, _)| n.authored.clone()).unwrap_or_default();
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
                // Its own, or the nearest above it: authored in the file or set by Set Attribute.
                let from = match props::purpose_from(p) {
                    props::PurposeFrom::Set => setter(path, "purpose", SetContext::Property),
                    props::PurposeFrom::File => ("file".into(), maker),
                    props::PurposeFrom::Above(group, true) => (format!("inherited: {group}"),
                        set_by(graph, view, &group, "purpose", SetContext::Property).map(|(id, _)| id)),
                    props::PurposeFrom::Above(group, false) => (format!("inherited: {group}"), maker),
                    props::PurposeFrom::Unset => made_here(),
                };
                row("purpose", purpose(props::purpose_of(p)).into(), from);
                // Instancing: a point instancer's instances and prototypes, a
                // prototype's or a native instance's copies.
                if let Some(protos) = &p.instancing {
                    row("instances", p.mesh.point_count().to_string(), made_here());
                    row("prototypes", protos.join(", "), made_here());
                } else if instanced(p) {
                    row("copies", copies(p, prims).to_string(), made_here());
                }
                let bound = p.material.clone().unwrap_or_else(|| "none".into());
                let rebound = p.source.as_ref().is_some_and(|s| s.material != p.material);
                row("material:binding", bound, if rebound { ("Imago".into(), None) } else { made_here() });
            }
            // A group: its purpose as its file authors it or Set Attribute set it
            // (listed below), else what it inherits from a group above.
            None => {
                let says = own.props.contains_key("purpose") || file_values.iter().any(|(k, _)| k == "purpose");
                if let (false, Some((v, from))) = (says, inherited.get("purpose")) {
                    row("purpose", v.clone(), (from.clone(), inherited_from(from, "purpose")));
                }
            }
        }
        match (own.props.get("visibility"), inherited.get("visibility"), node) {
            (Some(v), _, _) => row("visibility", v.display(), setter(path, "visibility", SetContext::Property)),
            (None, Some((v, from)), _) if v == "invisible" => row("visibility", v.clone(), (from.clone(), inherited_from(from, "visibility"))),
            (None, _, Some((n, _))) if n.hidden => row("visibility", "invisible".into(), ("file".into(), maker)),
            _ => row("visibility", "inherited".into(), made_here()),
        }
        if !own.props.contains_key("active") { row("active", "true".into(), made_here()); }

        // What the file authors on it (`kind`, `doubleSided`, custom
        // attributes), unless Set Attribute set it since.
        for (name, v) in &file_values {
            if name == "visibility" || (prim.is_some() && name == "purpose") || own.props.contains_key(name) { continue; }
            row(name, v.display(), ("file".into(), maker));
        }
        // Values Set Attribute set on it. Only visibility and purpose are
        // inherited in USD (and Object values, in the Object tab): `kind` or
        // `doubleSided` on a group say nothing about what is below it.
        for (name, v) in &own.props {
            if name == "visibility" { continue; }
            row(name, v.display(), setter(path, name, SetContext::Property));
        }
        for (name, v) in &own.primvars {
            row(&format!("primvars:{name}"), v.display(), setter(path, name, SetContext::Object));
        }
    }
    rows
}

/// The operator nearest upstream of `view` (itself first) that `is` says
/// is the one. Bypassed operators do nothing, so they are not it.
fn upstream<'a>(graph: &'a crate::node_graph::NodeGraphState, view: NodeId, is: impl Fn(&crate::node_graph::GraphNode) -> bool) -> Option<&'a crate::node_graph::GraphNode> {
    let mut queue = std::collections::VecDeque::from([view]);
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) { continue; }
        let Some(n) = graph.nodes.iter().find(|n| n.id == id) else { continue };
        if !n.bypassed && is(n) { return Some(n); }
        queue.extend(n.inputs.iter().filter_map(|i| i.connected_output.map(|(src, _)| src)));
    }
    None
}

/// The Set Attribute operator nearest upstream of `view` with a row setting
/// `name` in this context on `path`: one with no Primitive Locations, or
/// with one naming the path. Its id and name.
pub fn set_by(graph: &crate::node_graph::NodeGraphState, view: NodeId, path: &str, name: &str, context: crate::core::props::SetContext) -> Option<(NodeId, String)> {
    use crate::types::NodeType;
    upstream(graph, view, |n| match &n.node_type {
        NodeType::SetAttribute { rows } => {
            rows.iter().any(|r| r.context == context && r.name.trim() == name && r.is_usable())
                && (!crate::core::locations::is_set(&n.locations) || crate::core::locations::any_names(&n.locations, path))
        }
        _ => false,
    }).map(|n| (n.id, n.name.clone()))
}

/// The Load USD operator upstream of `view` that read this stage: the one
/// whose file gives a stage from the same layer, else the nearest one.
fn load_of(graph: &crate::node_graph::NodeGraphState, view: NodeId, tree: &crate::usd_scene::StageTree) -> Option<NodeId> {
    use crate::types::NodeType;
    let reads = |n: &crate::node_graph::GraphNode| matches!(&n.node_type, NodeType::LoadUsd { path }
        if crate::usd_scene::load_cached(path).is_ok_and(|s| s.tree.source == tree.source));
    upstream(graph, view, reads)
        .or_else(|| upstream(graph, view, |n| matches!(n.node_type, NodeType::LoadUsd { .. })))
        .map(|n| n.id)
}

/// The shape operator upstream of `view` that made the primitive at `path`,
/// or one below it (for an Xform Imago made above a shape).
fn shape_of(graph: &crate::node_graph::NodeGraphState, view: NodeId, path: &str) -> Option<NodeId> {
    use crate::types::{prim_location, NodeType};
    let at = |n: &crate::node_graph::GraphNode| match &n.node_type {
        NodeType::CreateCube { location, .. } => Some(prim_location(location, "/cube")),
        NodeType::CreateSphere { location, .. } => Some(prim_location(location, "/sphere")),
        NodeType::CreateGrid { location, .. } => Some(prim_location(location, "/grid")),
        _ => None,
    };
    upstream(graph, view, |n| at(n).is_some_and(|p| p == path || p.starts_with(&format!("{path}/")))).map(|n| n.id)
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
    /// Instanced geometry is listed once, in its own space; a point
    /// instancer as its points.
    #[test]
    fn instanced_geometry_is_listed_once() {
        use crate::types::Placement;
        use std::sync::Arc;
        let cube = crate::node_graph::nodes::create_cube(1.0);
        let shared = Arc::new(cube.clone());
        let at = |x: f32| glam::Mat4::from_translation(glam::Vec3::new(x, 0.0, 0.0));
        // A prototype at 1000 points, and a point instancer of 1000 points.
        let proto = Prim { mesh: shared.clone(), place: Placement::Many((0..1000).map(|k| at(k as f32)).collect()), ..Prim::new("/inst/proto".into(), Default::default()) };
        let points = crate::types::MeshData { points: (0..1000).map(|k| [k as f32, 0.0, 0.0]).collect(), ..Default::default() };
        let instancer = Prim { instancing: Some(Arc::new(vec!["/inst/proto".into()])), place: Placement::One(at(0.0)), ..Prim::new("/inst".into(), points) };
        let scene = || EvalResult::Named(vec![instancer.clone(), proto.clone()]);
        let parts = scene_parts(scene(), &["/inst".to_string()]);
        assert_eq!(parts.len(), 1, "the instancer selected: its instances alone");
        assert_eq!(parts[0].points.len(), 1000, "one point per instance");
        let parts = scene_parts(scene(), &["/inst/proto".to_string()]);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].vertices.len(), cube.vertices.len(), "the prototype once, not 1000 times");
        assert_eq!(scene_parts(scene(), &[]).len(), 1, "nothing selected: the instancer as its points");
        let parts = scene_parts(EvalResult::Named(vec![instancer.clone(), proto.clone()]), &["/".to_string()]);
        assert_eq!((parts.len(), parts[0].points.len()), (1, 1000), "a group above it: the instancer as its points");
        assert_eq!(copies(&proto, &[]), 1000);
        // Two prims sharing one mesh (native instances' proxies): listed once.
        let tree = Arc::new(crate::usd_scene::StageTree { nodes: ["/a", "/a/m", "/b", "/b/m"].iter().map(|p| crate::usd_scene::StageNode {
            path: p.to_string(), proxy: p.ends_with("/m"), ..Default::default() }).collect(), ..Default::default() });
        let proxy = |p: &str| Prim { mesh: shared.clone(), stage: Some(tree.clone()), place: Placement::One(at(3.0)), ..Prim::new(p.into(), Default::default()) };
        let prims = vec![proxy("/a/m"), proxy("/b/m")];
        assert_eq!(scene_parts(EvalResult::Named(prims.clone()), &[]).len(), 1);
        assert_eq!(copies(&prims[0], &prims), 2);
    }

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
        // Go to Operator: the shape that made the Xform, the Set Attribute that set a value.
        let op = |rows: &[PropRow], name: &str| rows.iter().find(|r| r.name == name).and_then(|r| r.node);
        assert_eq!((op(&group, "type"), op(&group, "visibility")), (Some(cube), Some(set)));
        assert_eq!(find(&group, "kind"), Some(("group".into(), "Hide".into())));

        let cube_rows = scene_properties(&g, view, &scene, &["/root/geo/cube".to_string()]);
        assert_eq!(find(&cube_rows, "type"), Some(("Mesh".into(), "Imago".into())));
        assert_eq!(find(&cube_rows, "visibility"), Some(("invisible".into(), "inherited: /root/geo".into())));
        assert_eq!(find(&cube_rows, "kind"), None, "kind is not inherited");
        assert_eq!(find(&cube_rows, "purpose"), Some(("default".into(), "Imago".into())));
        assert_eq!((op(&cube_rows, "purpose"), op(&cube_rows, "visibility")), (Some(cube), Some(set)), "inherited: the operator that set it on the group");

        // Nothing selected: the root prims.
        let roots = scene_properties(&g, view, &scene, &[]);
        assert!(roots.iter().all(|r| r.path == "/root") && !roots.is_empty());
    }
}
