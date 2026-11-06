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
//!
//! Create makes a variant set: on the prim at its Primitive Location, one
//! variant per input after the scene, each holding what that input has below
//! the location: prims made in Imago, and stages referenced or payloaded there
//! (Load USD's Primitive Location below it). The scene goes on with the selected
//! variant's prims; every prim of it carries the whole set (`MadeSet`, in
//! `Prim::above` at the location), so Write USD writes the `variantSet` with
//! all its variants, and Select can choose another of them. A variant whose
//! input has nothing below the location makes no prims, so when it is shown
//! nothing carries the set.

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

/// A variant set Create made: on the prim at `location`, the variants by name
/// with their prims (at their paths in the scene), the one `selected`.
#[derive(Debug)]
pub struct Made {
    pub location: String,
    pub set: String,
    pub selected: String,
    pub variants: Vec<(String, Vec<Prim>)>,
}

/// A made variant set, shared by the prims of its selected variant: equal when
/// it is the same one.
#[derive(Clone, Debug)]
pub struct MadeSet(pub Arc<Made>);

impl PartialEq for MadeSet {
    fn eq(&self, other: &MadeSet) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Made {
    /// As a stage's variant set is listed.
    pub fn listed(&self) -> xms_scene::VariantSet {
        xms_scene::VariantSet {
            name: self.set.clone(),
            variants: self.variants.iter().map(|(n, _)| n.clone()).collect(),
            selection: self.selected.clone(),
        }
    }
}

/// The made variant set a prim is in: one on a prim above it.
pub fn made_set_of(p: &Prim) -> Option<MadeSet> {
    p.above.iter().find_map(|(at, o)| {
        let m = o.variant_set.as_ref()?;
        let below = p.path.strip_prefix(at.as_str()).is_some_and(|r| r.starts_with('/'));
        below.then(|| m.clone())
    })
}

/// The variant sets made in the scene (Create), by the prim holding each.
pub fn made_sets(prims: &[Prim]) -> Vec<MadeSet> {
    let mut out: Vec<MadeSet> = vec![];
    for p in prims {
        for o in p.above.values() {
            if let Some(m) = &o.variant_set {
                if !out.contains(m) {
                    out.push(m.clone());
                }
            }
        }
    }
    out
}

/// What a Create's inputs give: the variant set `set` on the prim at
/// `location`, a variant per input in `variants` (named by `names`, else
/// `variant1`, `variant2`…), each holding the prims below the location made in
/// Imago or of a stage referenced or payloaded below it; the scene with
/// `selected`'s (the first, when no variant is called that).
pub fn create(
    scene: Option<&EvalResult>,
    variants: &[Option<EvalResult>],
    location: &str,
    set: &str,
    names: &[String],
    selected: &str,
) -> Selected {
    let mut prims = match scene {
        Some(EvalResult::Named(p)) => p.clone(),
        _ => vec![],
    };
    let mut notes = vec![];
    let location = crate::types::prim_location(location, "");
    let set = set.trim();
    if location.is_empty() || set.is_empty() {
        notes.push("Name the Primitive Location and the Variant Set".into());
        return Selected {
            result: EvalResult::Named(prims),
            notes,
        };
    }
    let below = |p: &str| p.strip_prefix(location.as_str()).is_some_and(|r| r.starts_with('/'));
    let mut made = Made {
        location: location.clone(),
        set: set.to_string(),
        selected: String::new(),
        variants: vec![],
    };
    for (k, input) in variants.iter().enumerate() {
        let Some(input) = input else { continue };
        let name = names
            .get(k)
            .map(|n| n.trim())
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("variant{}", k + 1));
        let EvalResult::Named(content) = input else {
            notes.push(format!("{name}: primitives only, at paths below {location}"));
            continue;
        };
        let mut kept = vec![];
        // Each stage said once.
        let mut said: Vec<*const StageTree> = vec![];
        for p in content {
            let fits = match p.stage.as_ref() {
                None => below(&p.path),
                // The whole stage goes in, as its arc.
                Some(t) if t.arc.as_ref().is_some_and(|a| below(&a.at)) => true,
                Some(t) => {
                    if !said.contains(&Arc::as_ptr(t)) {
                        said.push(Arc::as_ptr(t));
                        notes.push(format!(
                            "{name}: {} goes in a variant as a Reference or Payload with its Primitive Location below {location}",
                            t.source.display()
                        ));
                    }
                    continue;
                }
            };
            if fits {
                kept.push(p.clone());
            } else {
                notes.push(format!("{name}: {} is not below {location}", p.path));
            }
        }
        made.variants.push((name, kept));
    }
    if made.variants.is_empty() {
        notes.push("Wire a variant into the inputs after the scene".into());
        return Selected {
            result: EvalResult::Named(prims),
            notes,
        };
    }
    made.selected = made
        .variants
        .iter()
        .find(|(n, _)| n == selected)
        .unwrap_or(&made.variants[0])
        .0
        .clone();
    // What the scene has below the location gives way to the variant: its
    // prims made in Imago, and the stages referenced or payloaded there.
    prims.retain(|p| match p.stage.as_ref() {
        None => !below(&p.path),
        Some(t) => !t.arc.as_ref().is_some_and(|a| below(&a.at)),
    });
    prims.extend(shown(&MadeSet(Arc::new(made))));
    Selected {
        result: EvalResult::Named(prims),
        notes,
    }
}

/// The selected variant's prims of a made set, each carrying the set.
fn shown(m: &MadeSet) -> Vec<Prim> {
    let Some((_, prims)) = m.0.variants.iter().find(|(n, _)| *n == m.0.selected) else {
        return vec![];
    };
    prims.iter().map(|p| in_set(p.clone(), m)).collect()
}

/// A prim of a variant shown of the made set `m`: carrying it.
fn in_set(mut p: Prim, m: &MadeSet) -> Prim {
    if made_set_of(&p).as_ref() != Some(m) {
        Arc::make_mut(&mut p.above)
            .entry(m.0.location.clone())
            .or_default()
            .variant_set = Some(m.clone());
    }
    p
}

/// A prim of a variant as written inside it: out of the set at `location`.
pub fn out_of_set(p: &Prim, location: &str) -> Prim {
    let mut p = p.clone();
    let above = Arc::make_mut(&mut p.above);
    if let Some(o) = above.get_mut(location) {
        o.variant_set = None;
        if o.is_empty() {
            above.remove(location);
        }
    }
    p
}

/// The scene with variant `variant` of the made set `m` shown in place of the
/// one selected.
fn reselected(mut prims: Vec<Prim>, m: &MadeSet, variant: &str) -> Vec<Prim> {
    prims.retain(|p| !p.above.values().any(|o| o.variant_set.as_ref() == Some(m)));
    let made = Made {
        location: m.0.location.clone(),
        set: m.0.set.clone(),
        selected: variant.to_string(),
        variants: m.0.variants.clone(),
    };
    prims.extend(shown(&MadeSet(Arc::new(made))));
    prims
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

/// The variant sets of the prims coming in, by path, for choosing from: the
/// stages' and those made in Imago (Create).
pub fn variant_prims(prims: &[Prim]) -> Vec<(String, Vec<xms_scene::VariantSet>)> {
    let mut seen: HashSet<*const StageTree> = HashSet::new();
    let mut out: Vec<(String, Vec<xms_scene::VariantSet>)> = vec![];
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
    for m in made_sets(prims) {
        match out.iter_mut().find(|(p, _)| *p == m.0.location) {
            Some((_, sets)) => sets.push(m.0.listed()),
            None => out.push((m.0.location.clone(), vec![m.0.listed()])),
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
    let mut prims = prims.clone();
    // Each stage asked, with its choices by the prim's path in the file, and
    // the prims (scene paths) whose variants change.
    let mut stages: Vec<(Arc<StageTree>, Variants, Vec<String>)> = vec![];
    for c in choices {
        let path = crate::types::prim_location(&c.prim, "");
        if path.is_empty() || c.set.trim().is_empty() || c.variant.trim().is_empty() {
            continue;
        }
        // A set made in Imago (Create): its other variant shown.
        if let Some(m) = made_sets(&prims)
            .into_iter()
            .find(|m| m.0.location == path && m.0.set == c.set)
        {
            if m.0.variants.iter().any(|(n, _)| *n == c.variant) {
                if m.0.selected != c.variant {
                    prims = reselected(prims, &m, &c.variant);
                }
            } else {
                notes.push(format!("{path} has no variant {} in a set {}", c.variant, c.set));
            }
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
    // A stage in a variant shown of a made set: what it brings is in the set.
    let set = prims
        .iter()
        .filter(|p| p.stage.as_ref().is_some_and(|t| Arc::ptr_eq(t, old)))
        .find_map(made_set_of);
    let on = |p: Prim| {
        let p = Prim {
            stage: Some(tree.clone()),
            ..p
        };
        match &set {
            Some(m) => in_set(p, m),
            None => p,
        }
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

    /// The "Variant set: cube or sphere" template's scene: the ground, and
    /// /asset's shape set with the cube shown.
    fn made() -> Vec<Prim> {
        let t = crate::templates::TEMPLATES
            .iter()
            .find(|t| t.name == "Variant set: cube or sphere")
            .unwrap();
        let mut g = crate::node_graph::NodeGraphState::default();
        (t.build)(&mut g, &mut Default::default());
        let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
        let Some((_, _, EvalResult::Named(prims))) = g.evaluate_scene(&pass) else {
            panic!()
        };
        prims
    }

    #[test]
    fn create_shows_the_variant_selected_and_lists_the_set() {
        let prims = made();
        assert!(at(&prims, "/set/ground").is_some());
        assert!(at(&prims, "/asset/cube").is_some());
        assert!(at(&prims, "/asset/sphere").is_none());
        let sets = variant_prims(&prims);
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].0, "/asset");
        assert_eq!(sets[0].1[0].name, "shape");
        assert_eq!(sets[0].1[0].selection, "cube");
        assert_eq!(sets[0].1[0].variants, vec!["cube".to_string(), "sphere".to_string()]);
    }

    /// Select chooses another variant of a made set: its prims in place of
    /// the cube's, the set selecting it.
    #[test]
    fn select_chooses_a_variant_of_a_made_set() {
        let sphere = vec![Choice {
            prim: "/asset".into(),
            set: "shape".into(),
            variant: "sphere".into(),
        }];
        let out = select(&EvalResult::Named(made()), &sphere);
        assert!(out.notes.is_empty(), "{:?}", out.notes);
        let EvalResult::Named(after) = out.result else { panic!() };
        assert!(at(&after, "/asset/cube").is_none());
        assert!(at(&after, "/set/ground").is_some());
        let s = at(&after, "/asset/sphere").unwrap();
        assert_eq!(made_set_of(s).unwrap().0.selected, "sphere");
    }

    /// Write USD writes the set on /asset with both variants; read back, the
    /// file shows the cube, and choosing the sphere gives the sphere.
    #[test]
    fn write_usd_writes_a_made_set_with_every_variant() {
        let out = std::env::temp_dir().join("xms_variant_test/made.usda");
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        crate::usd_write::write_override(&made(), &out).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        for line in [
            "def Xform \"asset\"",
            "prepend variantSets = \"shape\"",
            "variants = { string \"shape\" = \"cube\" }",
            "variantSet \"shape\" = {",
            "\"cube\" {",
            "\"sphere\" {",
        ] {
            assert!(text.contains(line), "{line} in\n{text}");
        }
        let path = out.to_string_lossy();
        let back = loaded(&path);
        assert!(at(&back, "/asset/cube").is_some() && at(&back, "/asset/sphere").is_none());
        assert_eq!(variant_prims(&back)[0].1[0].variants.len(), 2);
        let mut v = Variants::new();
        v.entry("/asset".into())
            .or_default()
            .insert("shape".into(), "sphere".into());
        let sphere = crate::node_graph::nodes::load_stage(&path, false, &v, LoadMode::Sublayer, "", "").unwrap();
        assert!(at(&sphere, "/asset/sphere").is_some() && at(&sphere, "/asset/cube").is_none());
    }

    /// The car file's prim `prim` referenced at `at`.
    fn referenced(prim: &str, at: &str) -> Option<EvalResult> {
        crate::node_graph::nodes::load_stage(&car(), false, &Variants::new(), LoadMode::Reference, at, prim)
            .map(EvalResult::Named)
    }

    /// /asset's set `model`: the car or the ground, each referenced below it.
    fn of_stages() -> Selected {
        create(
            None,
            &[referenced("/car", "/asset/car"), referenced("/ground", "/asset/ground")],
            "/asset",
            "model",
            &["car".into(), "ground".into()],
            "car",
        )
    }

    /// Stages go in a variant whole, by their arcs; Select chooses both among
    /// the made set's variants and in a stage's own sets inside one, and what
    /// that stage brings stays in the made set.
    #[test]
    fn stages_are_variants_too() {
        let out = of_stages();
        assert!(out.notes.is_empty(), "{:?}", out.notes);
        let EvalResult::Named(prims) = out.result else { panic!() };
        assert!(at(&prims, "/asset/car/body").is_some() && at(&prims, "/asset/ground").is_none());
        let sets = variant_prims(&prims);
        assert!(sets.iter().any(|(p, s)| p == "/asset" && s[0].name == "model"));
        assert!(sets.iter().any(|(p, s)| p == "/asset/car" && s[0].name == "paint"));
        let blue = vec![Choice {
            prim: "/asset/car".into(),
            set: "paint".into(),
            variant: "blue".into(),
        }];
        let EvalResult::Named(blue) = select(&EvalResult::Named(prims.clone()), &blue).result else {
            panic!()
        };
        let stripe = at(&blue, "/asset/car/stripe").unwrap();
        assert_eq!(made_set_of(stripe).unwrap().0.set, "model");
        let ground = vec![Choice {
            prim: "/asset".into(),
            set: "model".into(),
            variant: "ground".into(),
        }];
        let EvalResult::Named(ground) = select(&EvalResult::Named(prims), &ground).result else {
            panic!()
        };
        assert!(at(&ground, "/asset/ground").is_some() && at(&ground, "/asset/car/body").is_none());
        // A stage at its own paths cannot go in.
        let sublayered = create(
            None,
            &[Some(EvalResult::Named(loaded(&car())))],
            "/asset",
            "model",
            &[],
            "",
        );
        assert!(
            sublayered.notes.iter().any(|n| n.contains("Reference or Payload")),
            "{:?}",
            sublayered.notes
        );
    }

    /// Write USD writes each variant's stage as its reference inside the
    /// variant; read back, the file switches between them.
    #[test]
    fn write_usd_writes_stages_in_a_variant_as_references() {
        let EvalResult::Named(prims) = of_stages().result else {
            panic!()
        };
        let out = std::env::temp_dir().join("xms_variant_test/stages.usda");
        crate::usd_write::write_override(&prims, &out).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        for line in [
            "variantSet \"model\" = {",
            "prepend references = @car.usda@</car>",
            "prepend references = @car.usda@</ground>",
        ] {
            assert!(text.contains(line), "{line} in\n{text}");
        }
        let path = out.to_string_lossy();
        let back = loaded(&path);
        assert!(at(&back, "/asset/car/body").is_some() && at(&back, "/asset/ground").is_none());
        let mut v = Variants::new();
        v.entry("/asset".into())
            .or_default()
            .insert("model".into(), "ground".into());
        let ground = crate::node_graph::nodes::load_stage(&path, false, &v, LoadMode::Sublayer, "", "").unwrap();
        assert!(at(&ground, "/asset/ground").is_some() && at(&ground, "/asset/car/body").is_none());
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
