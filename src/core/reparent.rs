//! Reparent: prims moved under another prim, its Parent (an Xform Imago makes
//! when the scene has none there), each with what is below it and keeping its
//! name, a number added when the Parent already has a child of that name.
//! Primitives moves the prims the Prims Location tab names; Whole scene, every
//! root prim. With Keep World Position, what moves stays where it was in the
//! world: its own transform changes to make up for the new parent's.
//!
//! What comes after sees the new paths. Write USD writes a file's prim as a
//! reference to it at its new path, deactivated at its old one
//! (`StageTree::moves`); a prim Imago made is written whole where it now is.
//! A material binding from inside a moved prim to a material outside it is
//! written again at the new path, since a reference drops targets outside the
//! prim it brings in.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use glam::DMat4;

use crate::core::locations;
use crate::types::{EvalResult, Placement, Prim};
use crate::usd_scene::{Moved, StageNode, StageTree};

/// What a Reparent moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ReparentMode {
    /// The prims its Prims Location tab names.
    #[default]
    Primitives,
    /// Every root prim of the scene.
    WholeScene,
}

/// What a Reparent does to the scene coming in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// The Parent's path; empty when it names none.
    pub parent: String,
    /// Each prim moved: its path, and its new one.
    pub moves: Vec<(String, String)>,
    /// Prims named that stay where they are, and why.
    pub skipped: Vec<String>,
}

impl Plan {
    /// The moves whose prim got a number added to its name.
    pub fn renamed(&self) -> impl Iterator<Item = &(String, String)> {
        self.moves.iter().filter(|(o, n)| name(o) != name(n))
    }
}

fn name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or("")
}

fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) | None => "/",
        Some(i) => &path[..i],
    }
}

/// Whether `path` is `root` or below it.
fn within(path: &str, root: &str) -> bool {
    path == root || path.strip_prefix(root).is_some_and(|r| r.starts_with('/'))
}

/// Where the moves put a path, when one does.
pub fn moved_path(moves: &[(String, String)], path: &str) -> Option<String> {
    moves
        .iter()
        .find(|(o, _)| within(path, o))
        .map(|(o, n)| format!("{n}{}", &path[o.len()..]))
}

fn renamed(moves: &[(String, String)], path: &str) -> String {
    moved_path(moves, path).unwrap_or_else(|| path.to_string())
}

fn trees_of(prims: &[Prim]) -> Vec<Arc<StageTree>> {
    let mut trees: Vec<Arc<StageTree>> = vec![];
    for t in prims.iter().filter_map(|p| p.stage.as_ref()) {
        if !trees.iter().any(|x| Arc::ptr_eq(x, t)) {
            trees.push(t.clone());
        }
    }
    trees
}

/// The prims a Reparent moves and where to: the topmost of those chosen, the
/// Parent's children's names kept clear of each other.
pub fn plan(prims: &[Prim], list: &[String], parent: &str, mode: ReparentMode) -> Plan {
    let dest = crate::types::prim_location(parent, "");
    let mut plan = Plan {
        parent: dest.clone(),
        ..Default::default()
    };
    if dest.is_empty() {
        return plan;
    }
    let paths = crate::node_graph::scene_paths(prims);
    let chosen = paths.iter().filter(|p| match mode {
        ReparentMode::Primitives => locations::any_names(list, p),
        ReparentMode::WholeScene => parent_of(p) == "/",
    });
    let mut roots: Vec<&String> = vec![];
    for o in chosen {
        if roots.iter().any(|r| within(o, r)) {
            continue;
        }
        roots.retain(|r| !within(r, o));
        roots.push(o);
    }
    let taken: BTreeSet<&str> = paths.iter().filter(|p| parent_of(p) == dest).map(|p| name(p)).collect();
    let mut names: Vec<String> = vec![];
    for o in roots {
        if within(&dest, o) {
            plan.skipped.push(format!(
                "{o}: the Parent is {}",
                if *o == dest { "this prim" } else { "below it" }
            ));
            continue;
        }
        if parent_of(o) == dest {
            continue;
        }
        let base = name(o);
        let mut n = base.to_string();
        let mut k = 1;
        while taken.contains(n.as_str()) || names.contains(&n) {
            n = format!("{base}{k}");
            k += 1;
        }
        names.push(n.clone());
        plan.moves.push((o.clone(), format!("{dest}/{n}")));
    }
    plan
}

/// One move, with what its prims need: the new and old parents' places in the
/// world (edited, as loaded).
struct Move {
    from: String,
    to: String,
    dest: [DMat4; 2],
    was: [DMat4; 2],
    /// It ignores its parents' transforms: nothing moves in the world.
    reset: bool,
    /// A file's prim, written as a reference to it: not a prim Imago made, an
    /// arc's prim or one already moved.
    by_reference: bool,
}

impl Move {
    /// What moving it does to where things are in the world.
    fn delta(&self, edited: bool) -> DMat4 {
        let k = if edited { 0 } else { 1 };
        if self.reset {
            DMat4::IDENTITY
        } else {
            self.dest[k] * self.was[k].inverse()
        }
    }
}

/// The scene with the Plan's moves made.
pub fn reparent(input: &EvalResult, list: &[String], parent: &str, mode: ReparentMode, keep_world: bool) -> EvalResult {
    let EvalResult::Named(prims) = input else {
        return input.clone();
    };
    let plan = plan(prims, list, parent, mode);
    if plan.moves.is_empty() {
        return input.clone();
    }
    EvalResult::Named(apply(prims, &plan, keep_world))
}

fn apply(prims: &[Prim], plan: &Plan, keep_world: bool) -> Vec<Prim> {
    let trees = trees_of(prims);
    let dest = plan.parent.as_str();
    // Where a path is in the world: in the stage that has it, else in `home`.
    let world = |path: &str, edited: bool, home: Option<&StageTree>| -> DMat4 {
        trees
            .iter()
            .map(|t| &**t)
            .find(|t| t.node(path).is_some() && !t.made.contains(path))
            .or(home)
            .map_or(DMat4::IDENTITY, |t| t.world(path, edited))
    };
    let moves: Vec<Move> = plan
        .moves
        .iter()
        .map(|(o, n)| {
            let home = trees.iter().map(|t| &**t).find(|t| t.node(o).is_some());
            let at = |path: &str| [world(path, true, home), world(path, false, home)];
            Move {
                from: o.clone(),
                to: n.clone(),
                dest: at(dest),
                was: at(parent_of(o)),
                reset: home.and_then(|t| t.node(o)).is_some_and(|n| n.reset_xform),
                by_reference: home.is_some_and(|t| {
                    !t.made.contains(o)
                        && !t.arc.as_ref().is_some_and(|a| within(&a.at, o))
                        && !t.moves.iter().any(|m| m.to == *o)
                        && t.file_path(o).is_some()
                }),
            }
        })
        .collect();
    let pairs = &plan.moves;

    // Each stage with something moved, its paths changed.
    let new_trees: Vec<Option<Arc<StageTree>>> = trees
        .iter()
        .map(|t| {
            let here: Vec<&Move> = moves.iter().filter(|m| t.node(&m.from).is_some()).collect();
            if here.is_empty() {
                return None;
            }
            let is_prim = |path: &str| {
                prims
                    .iter()
                    .any(|p| p.path == path && p.stage.as_ref().is_some_and(|s| Arc::ptr_eq(s, t)))
            };
            Some(Arc::new(moved_tree(
                t, &here, pairs, dest, keep_world, &trees, &is_prim,
            )))
        })
        .collect();

    prims
        .iter()
        .map(|p| {
            let mut p = p.clone();
            let i = p
                .stage
                .as_ref()
                .and_then(|s| trees.iter().position(|t| Arc::ptr_eq(t, s)));
            if let Some(Some(t)) = i.map(|i| &new_trees[i]) {
                p.stage = Some(t.clone());
            }
            let mut src = p.source.as_deref().cloned();
            if let Some(m) = moves.iter().find(|m| within(&p.path, &m.from)) {
                p.path = renamed(pairs, &p.path);
                if let Some(s) = src.as_mut() {
                    if keep_world {
                        s.place = shift(&s.place, m.delta(false));
                    } else {
                        let same = p.place.same(&s.place);
                        p.place = shift(&p.place, m.delta(true));
                        s.place = if same {
                            p.place.clone()
                        } else {
                            shift(&s.place, m.delta(false))
                        };
                    }
                    // A reference keeps the targets inside what it brings in
                    // (at their new paths) and drops the rest.
                    s.material = match s.material.take() {
                        Some(mat) if within(&mat, &m.from) => Some(renamed(pairs, &mat)),
                        Some(_) if m.by_reference => None,
                        other => other,
                    };
                } else if !keep_world {
                    p.place = shift(&p.place, m.delta(true));
                }
                if let Some(by) = p.instanced_by.as_deref() {
                    let mut by = by.clone();
                    if !keep_world {
                        by.place = shift(&by.place, m.delta(true));
                    }
                    p.instanced_by = Some(Arc::new(by));
                }
            }
            // Paths to what moved, wherever they are.
            p.material = p.material.map(|mat| renamed(pairs, &mat));
            if let Some(by) = p.instanced_by.as_deref() {
                if moved_path(pairs, &by.instancer).is_some() {
                    let mut by = by.clone();
                    by.instancer = renamed(pairs, &by.instancer);
                    p.instanced_by = Some(Arc::new(by));
                }
            }
            if let Some(list) = p.instancing.as_deref() {
                if list.iter().any(|x| moved_path(pairs, x).is_some()) {
                    p.instancing = Some(Arc::new(list.iter().map(|x| renamed(pairs, x)).collect()));
                }
            }
            if p.above.keys().any(|k| moved_path(pairs, k).is_some()) {
                p.above = Arc::new(p.above.iter().map(|(k, o)| (renamed(pairs, k), o.clone())).collect());
            }
            if let Some(r) = p.render.as_deref() {
                if r.rels
                    .values()
                    .flat_map(|x| &x.targets)
                    .any(|x| moved_path(pairs, x).is_some())
                {
                    let mut r = r.clone();
                    for rel in r.rels.values_mut() {
                        for x in rel.targets.iter_mut() {
                            *x = renamed(pairs, x);
                        }
                    }
                    p.render = Some(Arc::new(r));
                }
            }
            if let Some(s) = src {
                let before = p.source.as_deref().expect("cloned from it");
                if !s.place.same(&before.place) || s.material != before.material {
                    p.source = Some(Arc::new(s));
                }
            }
            p
        })
        .collect()
}

/// A placement moved by `d` in the world.
fn shift(place: &Placement, d: DMat4) -> Placement {
    if d.abs_diff_eq(DMat4::IDENTITY, 1e-12) {
        return place.clone();
    }
    let by = |m: &glam::Mat4| (d * m.as_dmat4()).as_mat4();
    match place {
        Placement::InPlace => Placement::One(d.as_mat4()),
        Placement::One(m) => Placement::One(by(m)),
        Placement::Many(ms) => Placement::Many(ms.iter().map(by).collect()),
    }
}

/// A stage with the moves of its prims made: its prims' paths, its opinions
/// and earlier moves follow them; each file's prim moved is a new entry of
/// `moves`; the Parent and what is above it are added where it has none.
fn moved_tree(
    t: &StageTree,
    here: &[&Move],
    pairs: &[(String, String)],
    dest: &str,
    keep_world: bool,
    trees: &[Arc<StageTree>],
    is_prim: &dyn Fn(&str) -> bool,
) -> StageTree {
    let mut out = t.clone();
    for m in here {
        // A group keeping its place: L' = W(parent)⁻¹ · W(old parent) · L. A
        // primitive's own transform is its placement's, written from it.
        if keep_world && !m.reset && !is_prim(&m.from) {
            if let Some(l) = t.local(&m.from, true) {
                let l = m.dest[0].inverse() * m.was[0] * l;
                out.opinions.entry(m.from.clone()).or_default().transform = Some(l.to_cols_array());
            }
        }
    }
    let new: Vec<Moved> = here
        .iter()
        .filter(|m| m.by_reference)
        .filter_map(|m| {
            Some(Moved {
                file: t.file_path(&m.from)?,
                from: m.from.clone(),
                to: m.to.clone(),
            })
        })
        .collect();

    // Every path the stage keeps, where the moves put it.
    let moving = |path: &str| here.iter().any(|m| within(path, &m.from));
    let (mut gone, mut nodes): (Vec<StageNode>, Vec<StageNode>) = std::mem::take(&mut out.nodes)
        .into_iter()
        .partition(|n| moving(&n.path));
    for n in gone.iter_mut() {
        n.path = renamed(pairs, &n.path);
        n.name = name(&n.path).to_string();
        n.depth = n.path.split('/').filter(|s| !s.is_empty()).count();
    }
    out.opinions = std::mem::take(&mut out.opinions)
        .into_iter()
        .map(|(k, o)| (renamed(pairs, &k), o))
        .collect();
    out.made = std::mem::take(&mut out.made)
        .into_iter()
        .map(|k| renamed(pairs, &k))
        .collect();
    for m in out.moves.iter_mut() {
        m.from = renamed(pairs, &m.from);
        m.to = renamed(pairs, &m.to);
    }
    out.moves.extend(new);
    if let Some(a) = out.arc.as_mut() {
        a.at = renamed(pairs, &a.at);
    }
    out.render_settings = out.render_settings.map(|r| renamed(pairs, &r));

    // The Parent and the prims above it, where the stage has none: as another
    // stage has them, else Xforms.
    let names: Vec<&str> = dest.split('/').filter(|s| !s.is_empty()).collect();
    for k in 1..=names.len() {
        let path = format!("/{}", names[..k].join("/"));
        if nodes.iter().any(|n| n.path == path) {
            continue;
        }
        let like = trees
            .iter()
            .find_map(|x| x.node(&path).filter(|_| !x.made.contains(&path)));
        nodes.push(StageNode {
            path: path.clone(),
            name: names[k - 1].to_string(),
            type_name: like.map_or("Xform".into(), |n| n.type_name.clone()),
            depth: k,
            local: like.map_or(DMat4::IDENTITY, |n| n.local),
            reset_xform: like.is_some_and(|n| n.reset_xform),
            ..Default::default()
        });
        out.made.insert(path);
    }
    // Parents before their children; what moved after the Parent's own.
    nodes.extend(gone);
    out.nodes = depth_first(nodes);
    out.index = out.nodes.iter().enumerate().map(|(i, n)| (n.path.clone(), i)).collect();
    out
}

/// Nodes in depth-first order, siblings in the order given.
fn depth_first(nodes: Vec<StageNode>) -> Vec<StageNode> {
    let mut kids: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        kids.entry(parent_of(&n.path)).or_default().push(i);
    }
    let mut order: Vec<usize> = vec![];
    let mut seen = vec![false; nodes.len()];
    let mut stack: Vec<usize> = kids
        .get("/")
        .map(|v| v.iter().rev().copied().collect())
        .unwrap_or_default();
    while let Some(i) = stack.pop() {
        if std::mem::replace(&mut seen[i], true) {
            continue;
        }
        order.push(i);
        if let Some(v) = kids.get(nodes[i].path.as_str()) {
            stack.extend(v.iter().rev());
        }
    }
    // Any whose parent the stage does not list, as they were.
    order.extend((0..nodes.len()).filter(|i| !seen[*i]));
    let mut slots: Vec<Option<StageNode>> = nodes.into_iter().map(Some).collect();
    order.into_iter().filter_map(|i| slots[i].take()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MeshData, Source};

    fn node(path: &str, local: DMat4) -> StageNode {
        StageNode {
            path: path.into(),
            name: name(path).into(),
            type_name: "Xform".into(),
            depth: path.matches('/').count(),
            local,
            ..Default::default()
        }
    }

    /// `/set` at x 2 holding the mesh `/set/table` (its own x 1), and `/world`
    /// at y 5: a file's stage.
    fn scene() -> Vec<Prim> {
        let at = |x: f64, y: f64| DMat4::from_translation(glam::DVec3::new(x, y, 0.0));
        let mut table = node("/set/table", at(1.0, 0.0));
        table.type_name = "Mesh".into();
        let nodes = vec![node("/set", at(2.0, 0.0)), table, node("/world", at(0.0, 5.0))];
        let tree = Arc::new(StageTree {
            index: nodes.iter().enumerate().map(|(i, n)| (n.path.clone(), i)).collect(),
            nodes,
            ..Default::default()
        });
        let place = Placement::One(glam::Mat4::from_translation(glam::Vec3::new(3.0, 0.0, 0.0)));
        let geo = Arc::new(crate::core::geo::Geo::default());
        vec![Prim {
            stage: Some(tree),
            place: place.clone(),
            material: Some("/mtl/wood".into()),
            source: Some(Arc::new(Source {
                geo: geo.clone(),
                place,
                material: Some("/mtl/wood".into()),
                purpose: Default::default(),
            })),
            geo: Some(geo),
            ..Prim::new("/set/table".into(), MeshData::default())
        }]
    }

    fn x_y(p: &Prim) -> (f32, f32) {
        let m = p.place.matrices()[0];
        (m.w_axis.x, m.w_axis.y)
    }

    #[test]
    fn a_group_moves_with_its_children_keeping_its_place() {
        let EvalResult::Named(out) = reparent(
            &EvalResult::Named(scene()),
            &["/set".into()],
            "/world",
            ReparentMode::Primitives,
            true,
        ) else {
            panic!()
        };
        assert_eq!(out[0].path, "/world/set/table");
        assert_eq!(x_y(&out[0]), (3.0, 0.0), "where it was");
        let tree = out[0].stage.as_deref().unwrap();
        let paths: Vec<&str> = tree.nodes.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(paths, ["/world", "/world/set", "/world/set/table"]);
        assert_eq!(
            tree.moves,
            [Moved {
                file: "/set".into(),
                from: "/set".into(),
                to: "/world/set".into()
            }]
        );
        // The group's own transform makes up for its new parent's.
        let w = tree.world("/world/set/table", true);
        assert!((w.w_axis.truncate() - glam::DVec3::new(3.0, 0.0, 0.0)).length() < 1e-9);
        // The binding to a material outside it is the prim's again: written.
        assert_eq!(out[0].material.as_deref(), Some("/mtl/wood"));
        assert_eq!(out[0].source.as_ref().unwrap().material, None);
        assert_eq!(tree.file_path("/world/set/table").as_deref(), Some("/set/table"));
    }

    #[test]
    fn without_keeping_its_place_it_follows_the_parent() {
        let EvalResult::Named(out) = reparent(
            &EvalResult::Named(scene()),
            &["/set".into()],
            "/world",
            ReparentMode::Primitives,
            false,
        ) else {
            panic!()
        };
        assert_eq!(x_y(&out[0]), (3.0, 5.0));
        let src = out[0].source.as_ref().unwrap();
        assert!(out[0].place.same(&src.place), "nothing edited but the move");
    }

    #[test]
    fn the_whole_scene_goes_under_a_new_parent_names_kept_clear() {
        let mut prims = scene();
        prims.push(Prim::new("/world".into(), MeshData::default()));
        let plan = plan(&prims, &[], "/world/shot", ReparentMode::WholeScene);
        assert_eq!(plan.parent, "/world/shot");
        assert_eq!(plan.moves, [("/set".to_string(), "/world/shot/set".to_string())]);
        assert_eq!(plan.skipped, ["/world: the Parent is below it"]);
        let plan = super::plan(&prims, &[], "/shot", ReparentMode::WholeScene);
        assert_eq!(plan.moves.len(), 2);
        let EvalResult::Named(out) = reparent(&EvalResult::Named(prims), &[], "/shot", ReparentMode::WholeScene, true)
        else {
            panic!()
        };
        let tree = out[0].stage.as_deref().unwrap();
        assert!(tree.made.contains("/shot"));
        assert_eq!(out[1].path, "/shot/world");
    }

    #[test]
    fn a_clash_gets_a_number() {
        let mut prims = scene();
        prims.push(Prim::new("/world/set".into(), MeshData::default()));
        let plan = plan(&prims, &["/set".into()], "/world", ReparentMode::Primitives);
        assert_eq!(plan.moves, [("/set".to_string(), "/world/set1".to_string())]);
        assert_eq!(plan.renamed().count(), 1);
    }
}
