//! Variant: choosing variants of a stage's variant sets (Select). A stage is
//! composed when it loads, with the variants its file selects; the variant
//! sets and their variants are listed then, by name only
//! (`StageNode::variants`). Choosing another variant composes the stage again
//! with the choice in a session layer (`usd_scene::load_cached_with`, cached by
//! choice), and puts the prims it changed in place of those coming in:
//!
//! - prims outside the subtree of the prim whose variant changed, and those
//!   inside it the variant leaves as they were, pass through with every edit
//!   made above;
//! - prims the variant changed take what was made of them above where it still
//!   fits: their placement, material, purpose, properties and Object values;
//!   what rebuilt their geometry cannot be carried and is said
//!   (`Selected::notes`);
//! - prims the variant brings are added, unless the path was pruned above.
//!
//! Write USD writes the choices as `variants` on `over`s (`StageTree::variants`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::core::geo::Context;
use crate::types::{EvalResult, NodeId, Placement, Prim};
use crate::usd_scene::{LoadMode, StageTree, Variants};

/// What a Variant does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum VariantMode {
    /// Choose variants of the variant sets a stage has.
    #[default]
    Select,
    /// Make a variant set on a prim, its variants from the inputs.
    Create,
}

/// One choice: the variant `variant` of the set `set` on the prim at `prim`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Choice {
    pub prim: String,
    pub set: String,
    pub variant: String,
}

/// The scene with the variants chosen, and what could not be done or carried
/// over.
pub struct Selected {
    pub result: EvalResult,
    pub notes: Vec<String>,
}

/// What each Variant's choices could not do when it last cooked, by operator:
/// for its panel.
static NOTES: std::sync::Mutex<Vec<(NodeId, Vec<String>)>> = std::sync::Mutex::new(Vec::new());

/// Keep what a Variant's cook could not do (`Selected::notes`).
pub fn noted(id: NodeId, notes: Vec<String>) {
    if let Ok(mut all) = NOTES.lock() {
        all.retain(|(n, _)| *n != id);
        if !notes.is_empty() {
            all.push((id, notes));
        }
    }
}

/// What a Variant's choices could not do when it last cooked.
pub fn notes(id: NodeId) -> Vec<String> {
    NOTES
        .lock()
        .ok()
        .and_then(|all| all.iter().find(|(n, _)| *n == id).map(|(_, v)| v.clone()))
        .unwrap_or_default()
}

/// The variant sets of the prims coming in, by path, for choosing from.
pub fn variant_prims(prims: &[Prim]) -> Vec<(String, Vec<xms_scene::VariantSet>)> {
    let mut seen: HashSet<*const StageTree> = HashSet::new();
    let mut out = vec![];
    for t in prims.iter().filter_map(|p| p.stage.as_ref()) {
        if seen.insert(Arc::as_ptr(t)) {
            out.extend(
                t.nodes
                    .iter()
                    .filter(|n| !n.variants.is_empty())
                    .map(|n| (n.path.clone(), n.variants.clone())),
            );
        }
    }
    out
}

/// The variants `choices` choose, on the stages coming in.
pub fn select(input: &EvalResult, choices: &[Choice]) -> Selected {
    let EvalResult::Named(prims) = input else {
        return Selected {
            result: input.clone(),
            notes: vec![],
        };
    };
    let mut notes = vec![];
    // Each stage asked, with its choices by the prim's path in the file, and
    // the prims (scene paths) whose variants change.
    let mut stages: Vec<(Arc<StageTree>, Variants, Vec<String>)> = vec![];
    for c in choices {
        let path = crate::types::prim_location(&c.prim, "");
        if path.is_empty() || c.set.trim().is_empty() || c.variant.trim().is_empty() {
            continue;
        }
        let Some(tree) = prims
            .iter()
            .filter_map(|p| p.stage.as_ref())
            .find(|t| t.node(&path).is_some())
        else {
            notes.push(format!("{path}: no prim of a stage coming in"));
            continue;
        };
        let has = tree.node(&path).is_some_and(|n| {
            n.variants
                .iter()
                .any(|s| s.name == c.set && s.variants.contains(&c.variant))
        });
        if !has {
            notes.push(format!("{path} has no variant {} in a set {}", c.variant, c.set));
            continue;
        }
        let Some(file) = tree.file_path(&path) else { continue };
        let k = match stages.iter().position(|(t, _, _)| Arc::ptr_eq(t, tree)) {
            Some(k) => k,
            None => {
                stages.push((tree.clone(), tree.variants.clone(), vec![]));
                stages.len() - 1
            }
        };
        let (_, chosen, switched) = &mut stages[k];
        chosen.entry(file).or_default().insert(c.set.clone(), c.variant.clone());
        switched.push(path);
    }

    let mut prims = prims.clone();
    for (old, chosen, switched) in stages {
        if chosen == old.variants {
            continue;
        }
        if !old.moves.is_empty() {
            notes.push(format!(
                "{}: a Reparent above moved prims of this stage; put Variant before it",
                old.source.display()
            ));
            continue;
        }
        let (mode, location, prim) = match &old.arc {
            None => (LoadMode::Sublayer, "", ""),
            Some(a) if a.payload => (LoadMode::Payload, a.at.as_str(), a.prim.as_str()),
            Some(a) => (LoadMode::Reference, a.at.as_str(), a.prim.as_str()),
        };
        let source = old.source.to_string_lossy().into_owned();
        let Some(fresh) = crate::node_graph::nodes::load_stage(&source, old.convert, &chosen, mode, location, prim)
        else {
            notes.push(format!("{source}: could not be read with the variants chosen"));
            continue;
        };
        prims = swapped(prims, &old, fresh, &switched, &mut notes);
    }
    Selected {
        result: EvalResult::Named(prims),
        notes,
    }
}

/// `prims` with the prims of the stage `old` taken from `fresh` (the stage
/// composed with other variants) where the variants switched at `switched`
/// changed them, and edits carried over.
fn swapped(
    prims: Vec<Prim>,
    old: &Arc<StageTree>,
    fresh: Vec<Prim>,
    switched: &[String],
    notes: &mut Vec<String>,
) -> Vec<Prim> {
    let inside = |p: &str| {
        switched
            .iter()
            .any(|s| p == s || p.strip_prefix(s.as_str()).is_some_and(|r| r.starts_with('/')))
    };
    // The stage as composed now, with what operators above did to its prims
    // without geometry.
    let tree = {
        let mut t = fresh
            .iter()
            .find_map(|p| p.stage.as_deref().cloned())
            .unwrap_or_else(|| (**old).clone());
        t.opinions = old.opinions.clone();
        t.made = old.made.clone();
        Arc::new(t)
    };
    // Pruned above: not brought back by the variant.
    let removed: HashSet<String> = crate::edits::edit_set(&prims, old).removed.into_iter().collect();
    let order: Vec<String> = fresh.iter().map(|p| p.path.clone()).collect();
    let mut fresh: HashMap<String, Prim> = fresh.into_iter().map(|p| (p.path.clone(), p)).collect();
    let on = |p: Prim| Prim {
        stage: Some(tree.clone()),
        ..p
    };
    let mut out = vec![];
    for p in prims {
        if !p.stage.as_ref().is_some_and(|t| Arc::ptr_eq(t, old)) {
            out.push(p);
            continue;
        }
        let new = fresh.remove(&p.path);
        if !inside(&p.path) {
            out.push(on(p));
            continue;
        }
        match new {
            None => {
                if crate::edits::prim_edit(&p).is_some_and(|e| !e.is_empty()) {
                    notes.push(format!(
                        "{}: not in the variant chosen; what was done to it above goes",
                        p.path
                    ));
                }
            }
            Some(n) if unchanged(&p, &n) => out.push(on(p)),
            Some(n) => out.push(on(carried(&p, n, notes))),
        }
    }
    // What the variant brings.
    for path in order {
        if let Some(n) = fresh.remove(&path) {
            if inside(&path) && !removed.contains(&path) {
                out.push(on(n));
            }
        }
    }
    out
}

/// Whether the variant left a prim as it was: the same geometry, placement,
/// material and purpose as loaded.
fn unchanged(p: &Prim, n: &Prim) -> bool {
    match (&p.source, &n.source) {
        (Some(a), Some(b)) => {
            a.geo == b.geo && a.place.same(&b.place) && a.material == b.material && a.purpose == b.purpose
        }
        _ => false,
    }
}

/// The prim the variant made, `n`, with what was done above to the prim it
/// replaces, `p`, where it still fits.
fn carried(p: &Prim, mut n: Prim, notes: &mut Vec<String>) -> Prim {
    let Some(e) = crate::edits::prim_edit(p) else { return n };
    if e.is_empty() {
        return n;
    }
    if e.place.is_some() {
        n.place = moved(p, &n);
    }
    if let Some(m) = e.material {
        n.material = m;
        n.look = p.look.clone();
    }
    if let Some(purpose) = e.purpose {
        n.purpose = purpose;
    }
    n.props = p.props.clone();
    n.above = p.above.clone();
    // Object values (Set Attribute's) go on the new geometry; what reshaped
    // the old one has nothing to go on.
    let mut lost = e.topology || e.replaced || !e.removed.is_empty();
    if let (Some(from), Some(geo)) = (&p.geo, &mut n.geo) {
        for (ctx, name) in &e.attrs {
            match (ctx, from.attr(*ctx, name)) {
                (Context::Object, Some(a)) => Arc::make_mut(geo).set(Context::Object, name, a.clone()),
                _ => lost = true,
            }
        }
    }
    if lost {
        notes.push(format!(
            "{}: the variant chosen changed it; what reshaped it above is not carried over",
            p.path
        ));
    }
    n
}

/// Where `n` is placed once moved as `p` was: by the same transform, when
/// both are one copy; else where `p` is.
fn moved(p: &Prim, n: &Prim) -> Placement {
    let one = |pl: &Placement| match pl {
        Placement::InPlace => Some(glam::Mat4::IDENTITY),
        Placement::One(m) => Some(*m),
        Placement::Many(_) => None,
    };
    let from = p.source.as_ref().and_then(|s| one(&s.place));
    match (one(&p.place), from, one(&n.place)) {
        (Some(now), Some(was), Some(new)) => Placement::One(now * was.inverse() * new),
        _ => p.place.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usd_scene::variants_layer;

    fn quad(name: &str, size: f32) -> String {
        format!(
            "def Mesh \"{name}\" {{
    int[] faceVertexCounts = [4]
    int[] faceVertexIndices = [0, 1, 2, 3]
    point3f[] points = [(0, 0, 0), ({size}, 0, 0), ({size}, {size}, 0), (0, {size}, 0)]
}}
"
        )
    }

    /// A car whose paint is a variant set: red has a body, blue a bigger body
    /// and a stripe. Its wheel and the ground are outside the set.
    fn car() -> String {
        let dir = std::env::temp_dir().join("xms_variant_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("car.usda");
        let text = format!(
            "#usda 1.0
def Xform \"car\" (
    prepend variantSets = \"paint\"
    variants = {{ string paint = \"red\" }}
)
{{
    variantSet \"paint\" = {{
        \"red\" {{
{}        }}
        \"blue\" {{
{}{}        }}
    }}
{}}}
{}",
            quad("body", 1.0),
            quad("body", 2.0),
            quad("stripe", 1.0),
            quad("wheel", 1.0),
            quad("ground", 5.0),
        );
        std::fs::write(&path, text).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn loaded(path: &str) -> Vec<Prim> {
        crate::node_graph::nodes::load_stage(path, false, &Variants::new(), LoadMode::Sublayer, "", "").unwrap()
    }

    fn at<'a>(prims: &'a [Prim], path: &str) -> Option<&'a Prim> {
        prims.iter().find(|p| p.path == path)
    }

    fn blue() -> Vec<Choice> {
        vec![Choice {
            prim: "/car".into(),
            set: "paint".into(),
            variant: "blue".into(),
        }]
    }

    #[test]
    fn the_variant_sets_are_listed_as_the_stage_loads() {
        let prims = loaded(&car());
        let sets = variant_prims(&prims);
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].0, "/car");
        assert_eq!(sets[0].1[0].name, "paint");
        assert_eq!(sets[0].1[0].selection, "red");
        assert_eq!(sets[0].1[0].variants, vec!["blue".to_string(), "red".to_string()]);
    }

    /// Choosing blue: the body is the bigger one and the stripe comes in;
    /// what was moved above stays moved, the moved body moved the same way.
    #[test]
    fn a_choice_composes_the_stage_again_and_keeps_what_was_done_above() {
        let mut prims = loaded(&car());
        let shift = glam::Mat4::from_translation(glam::Vec3::new(0.0, 3.0, 0.0));
        for p in prims
            .iter_mut()
            .filter(|p| p.path == "/ground" || p.path == "/car/body")
        {
            p.place = p.place.moved(shift);
        }
        let ground = at(&prims, "/ground").unwrap().clone();
        let wheel = at(&prims, "/car/wheel").unwrap().clone();
        let out = select(&EvalResult::Named(prims), &blue());
        assert!(out.notes.is_empty(), "{:?}", out.notes);
        let EvalResult::Named(after) = out.result else { panic!() };
        // Outside the set, and inside it but unchanged: as they came, edits
        // and all.
        let g = at(&after, "/ground").unwrap();
        assert!(
            g.place.same(&ground.place) && Arc::ptr_eq(g.source.as_ref().unwrap(), ground.source.as_ref().unwrap())
        );
        let w = at(&after, "/car/wheel").unwrap();
        assert!(Arc::ptr_eq(w.source.as_ref().unwrap(), wheel.source.as_ref().unwrap()));
        // The body the variant changed: the blue one, moved as the red one was.
        let body = at(&after, "/car/body").unwrap();
        let points = body.geo.as_ref().unwrap().points();
        assert_eq!(points[2], [2.0, 2.0, 0.0]);
        let Placement::One(m) = &body.place else {
            panic!("{:?}", body.place)
        };
        assert!((m.w_axis.y - 3.0).abs() < 1e-5);
        assert!(at(&after, "/car/stripe").is_some());
        // One stage, as composed now, with the choice.
        let tree = body.stage.as_ref().unwrap();
        assert!(after
            .iter()
            .all(|p| p.stage.as_ref().is_some_and(|t| Arc::ptr_eq(t, tree))));
        assert_eq!(tree.variants["/car"]["paint"], "blue");
        assert_eq!(tree.node("/car").unwrap().variants[0].selection, "blue");
        // Choosing what it is already changes nothing.
        let again = select(&EvalResult::Named(after.clone()), &blue());
        let EvalResult::Named(again) = again.result else {
            panic!()
        };
        assert!(Arc::ptr_eq(again[0].stage.as_ref().unwrap(), tree));
    }

    /// What was pruned above stays pruned; an edit to geometry the variant
    /// replaced is said.
    #[test]
    fn pruned_stays_pruned_and_lost_edits_are_said() {
        let mut prims = loaded(&car());
        prims.retain(|p| p.path != "/car/wheel");
        // The body reshaped: new points.
        let body = prims.iter_mut().find(|p| p.path == "/car/body").unwrap();
        let mut geo = (**body.geo.as_ref().unwrap()).clone();
        geo.points_mut()[0] = [0.0, 0.0, 1.0];
        body.geo = Some(Arc::new(geo));
        let out = select(&EvalResult::Named(prims), &blue());
        let EvalResult::Named(after) = out.result else { panic!() };
        assert!(at(&after, "/car/wheel").is_none(), "pruned above");
        assert!(out.notes.iter().any(|n| n.starts_with("/car/body")), "{:?}", out.notes);
    }

    /// Write USD writes the choice on the prim holding the set, and the layer
    /// read back has the blue car.
    #[test]
    fn write_usd_writes_the_choice() {
        let prims = loaded(&car());
        let EvalResult::Named(after) = select(&EvalResult::Named(prims), &blue()).result else {
            panic!()
        };
        let out = std::env::temp_dir().join("xms_variant_test/written.usda");
        crate::usd_write::write_override(&after, &out).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.contains(r#"variants = { string "paint" = "blue" }"#), "{text}");
        let back = crate::usd_scene::load(&out).unwrap();
        assert!(back.meshes.iter().any(|m| m.path == "/car/stripe"));
    }

    #[test]
    fn a_choice_is_an_over_with_its_variants() {
        let mut v = Variants::new();
        v.entry("/car".into()).or_default().insert("paint".into(), "red".into());
        v.entry("/car/front".into())
            .or_default()
            .insert("size".into(), "small".into());
        v.entry("/house".into())
            .or_default()
            .insert("roof".into(), "flat".into());
        assert_eq!(
            variants_layer(&v),
            "#usda 1.0
over \"car\" (
    variants = { string \"paint\" = \"red\" }
)
{
    over \"front\" (
        variants = { string \"size\" = \"small\" }
    )
    {
    }
}
over \"house\" (
    variants = { string \"roof\" = \"flat\" }
)
{
}
"
        );
    }
}
