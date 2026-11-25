//! Edit Session: what is done to prims from the Viewer and the Selection bar,
//! kept in one operator as steps. A step is a set of prims and, for each, its
//! overrides by name (`translate`, `rotate`, `scale`), as an Edit Primitive
//! keeps them: absolute values, so a later step on the same prim wins, and
//! switching an earlier one off changes only what no later step set.
//!
//! Edits gather into the last step while they go to the same prims (a cube
//! dragged ten times is one step); another selection starts a new one. Steps
//! can be switched off, renamed, moved and deleted in its Parameters. Freeze
//! makes them one step; Explode puts them in the graph as Edit Primitives.

use std::collections::BTreeMap;

use glam::Mat4;

use crate::core::edit_primitive::{self, EditKind};
use crate::types::{EvalResult, Prim};

/// One prim's overrides, by name, as plain JSON.
pub type Edits = BTreeMap<String, serde_json::Value>;

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Step {
    pub name: String,
    #[serde(default = "yes")]
    pub on: bool,
    /// By prim path, its overrides.
    #[serde(default)]
    pub prims: BTreeMap<String, Edits>,
}

/// The steps that are on, in order, on what comes in.
pub fn apply(input: &EvalResult, steps: &[Step]) -> EvalResult {
    let mut out = input.clone();
    for s in steps.iter().filter(|s| s.on) {
        for (path, edits) in &s.prims {
            out = edit_primitive::edit(&out, std::slice::from_ref(path), EditKind::Xform, edits);
        }
    }
    out
}

/// "/sphere", or "3 prims".
pub fn what(paths: &[String]) -> String {
    match paths {
        [one] => one.clone(),
        many => format!("{} prims", many.len()),
    }
}

/// Write `values` (path, name, value) into the steps, as `verb` ("Move",
/// "Rotate"): into the last step when it is on and has these prims and no
/// others, else into a new one. Nothing when the steps already give every
/// value so.
pub fn record(steps: &mut Vec<Step>, verb: &str, values: &[(String, &str, serde_json::Value)]) {
    if values.is_empty() {
        return;
    }
    let mut paths: Vec<String> = values.iter().map(|(p, _, _)| p.clone()).collect();
    paths.sort();
    paths.dedup();
    let same = steps
        .last()
        .is_some_and(|s| s.on && s.prims.len() == paths.len() && paths.iter().all(|p| s.prims.contains_key(p)));
    // What the steps that are on set last, for each value: the same again
    // changes nothing.
    let now = |p: &str, n: &str| {
        steps
            .iter()
            .rev()
            .filter(|s| s.on)
            .find_map(|s| s.prims.get(p).and_then(|e| e.get(n)))
    };
    if values.iter().all(|(p, n, v)| now(p, n) == Some(v)) {
        return;
    }
    if !same {
        steps.push(Step {
            name: format!("{verb} {}", what(&paths)),
            on: true,
            prims: paths.iter().map(|p| (p.clone(), Edits::new())).collect(),
        });
    }
    let step = steps.last_mut().expect("a step");
    // Another kind of edit on the same prims: the step is an edit of them.
    if !step.name.starts_with(&format!("{verb} ")) {
        step.name = format!("Edit {}", what(&paths));
    }
    for (p, n, v) in values {
        let e = step.prims.entry(p.clone()).or_default();
        if e.get(*n) != Some(v) {
            e.insert(n.to_string(), v.clone());
        }
    }
}

/// The steps that are on, as one: each prim's values as the last step to set
/// them left them. Steps that are off are dropped.
pub fn freeze(steps: &[Step]) -> Vec<Step> {
    let mut prims: BTreeMap<String, Edits> = BTreeMap::new();
    for s in steps.iter().filter(|s| s.on) {
        for (path, edits) in &s.prims {
            prims.entry(path.clone()).or_default().extend(edits.clone());
        }
    }
    if prims.is_empty() {
        return vec![];
    }
    vec![Step {
        name: format!("Frozen {}", what(&prims.keys().cloned().collect::<Vec<_>>())),
        on: true,
        prims,
    }]
}

/// The steps that are on as Edit Primitives, in order: one per set of prims
/// of a step with the same overrides (its name, the prims, the overrides).
pub fn explode(steps: &[Step]) -> Vec<(String, Vec<String>, Edits)> {
    let mut out = vec![];
    for s in steps.iter().filter(|s| s.on) {
        let mut groups: Vec<(Vec<String>, Edits)> = vec![];
        for (path, edits) in s.prims.iter().filter(|(_, e)| !e.is_empty()) {
            match groups.iter_mut().find(|(_, e)| e == edits) {
                Some((paths, _)) => paths.push(path.clone()),
                None => groups.push((vec![path.clone()], edits.clone())),
            }
        }
        for (paths, edits) in groups {
            out.push((s.name.clone(), paths, edits));
        }
    }
    out
}

/// A prim's place: where its parents put it, and its own translate, rotate
/// (degrees, X then Y then Z) and scale, as the scene has them.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub path: String,
    pub parents: Mat4,
    pub trs: [[f32; 3]; 3],
}

/// The places of `paths` in a scene, those that have one (an instance's
/// copies are placed by their instancer).
pub fn places(prims: &[Prim], paths: &[String]) -> Vec<Place> {
    paths
        .iter()
        .filter_map(|p| {
            let (parents, trs) =
                edit_primitive::handles(prims, std::slice::from_ref(p), EditKind::Xform, &Edits::new())?;
            Some(Place {
                path: p.clone(),
                parents,
                trs,
            })
        })
        .collect()
}

/// The verb of a transform part: 0 translate, 1 rotate, 2 scale.
pub fn verb(part: usize) -> &'static str {
    ["Move", "Rotate", "Scale"][part]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MeshData, Placement};
    use glam::Vec3;
    use serde_json::json;

    fn scene() -> EvalResult {
        let cube = crate::node_graph::nodes::create_cube(1.0);
        let at = |x: f32| Placement::One(Mat4::from_translation(Vec3::new(x, 0.0, 0.0)));
        EvalResult::Named(vec![
            Prim {
                place: at(1.0),
                ..Prim::new("/a".into(), cube.clone())
            },
            Prim {
                place: at(2.0),
                ..Prim::new("/b".into(), cube)
            },
            Prim::new("/c".into(), MeshData::default()),
        ])
    }
    fn x_of(r: &EvalResult, path: &str) -> f32 {
        let EvalResult::Named(p) = r else { panic!() };
        match &p.iter().find(|p| p.path == path).unwrap().place {
            Placement::One(m) => m.w_axis.x,
            _ => panic!(),
        }
    }
    /// Every prim's matrix, by path.
    fn placed(r: &EvalResult) -> Vec<(String, [f32; 16])> {
        let EvalResult::Named(p) = r else { panic!() };
        p.iter()
            .map(|p| match &p.place {
                Placement::One(m) => (p.path.clone(), m.to_cols_array()),
                _ => (p.path.clone(), Mat4::IDENTITY.to_cols_array()),
            })
            .collect()
    }
    fn mv(steps: &mut Vec<Step>, path: &str, x: f32) {
        record(steps, "Move", &[(path.into(), "translate", json!([x, 0.0, 0.0]))]);
    }

    #[test]
    fn edits_to_the_same_prims_gather_into_one_step() {
        let mut steps = vec![];
        mv(&mut steps, "/a", 3.0);
        mv(&mut steps, "/a", 4.0);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].name, "Move /a");
        // Another kind on the same prims: still one step, an edit of them.
        record(
            &mut steps,
            "Rotate",
            &[("/a".into(), "rotate", json!([0.0, 45.0, 0.0]))],
        );
        assert_eq!((steps.len(), steps[0].name.as_str()), (1, "Edit /a"));
        // Another prim: a new step; the first again: another.
        mv(&mut steps, "/b", 5.0);
        mv(&mut steps, "/a", 6.0);
        assert_eq!(steps.len(), 3);
        // What the steps already give: no step, even for another selection.
        mv(&mut steps, "/a", 6.0);
        mv(&mut steps, "/b", 5.0);
        assert_eq!(steps.len(), 3);
        mv(&mut steps, "/b", 8.0);
        assert_eq!(steps.len(), 4);
        mv(&mut steps, "/a", 6.0);
        assert_eq!(steps.len(), 4);
        // Two prims at once: one step naming both.
        record(
            &mut steps,
            "Move",
            &[
                ("/a".into(), "translate", json!([1.0, 0.0, 0.0])),
                ("/b".into(), "translate", json!([1.0, 0.0, 0.0])),
            ],
        );
        assert_eq!(steps.last().unwrap().name, "Move 2 prims");
    }

    #[test]
    fn steps_are_absolute_and_the_last_wins() {
        let mut steps = vec![];
        mv(&mut steps, "/a", 3.0);
        mv(&mut steps, "/b", 7.0);
        mv(&mut steps, "/a", 5.0);
        let out = apply(&scene(), &steps);
        assert_eq!((x_of(&out, "/a"), x_of(&out, "/b")), (5.0, 7.0));
        // The first step off: /a is still where the third put it.
        steps[0].on = false;
        assert_eq!(x_of(&apply(&scene(), &steps), "/a"), 5.0);
        // The third off: the first's again; both off, the prim's own.
        steps[0].on = true;
        steps[2].on = false;
        assert_eq!(x_of(&apply(&scene(), &steps), "/a"), 3.0);
        steps[0].on = false;
        assert_eq!(x_of(&apply(&scene(), &steps), "/a"), 1.0);
    }

    #[test]
    fn freeze_and_explode_keep_what_is_shown() {
        let mut steps = vec![];
        mv(&mut steps, "/a", 3.0);
        mv(&mut steps, "/b", 7.0);
        mv(&mut steps, "/a", 5.0);
        record(
            &mut steps,
            "Rotate",
            &[("/a".into(), "rotate", json!([0.0, 0.0, 90.0]))],
        );
        steps[1].on = false;
        let frozen = freeze(&steps);
        assert_eq!(frozen.len(), 1);
        assert!(!frozen[0].prims.contains_key("/b"), "a step off is dropped");
        let a = &frozen[0].prims["/a"];
        assert_eq!(
            (a["translate"].clone(), a["rotate"].clone()),
            (json!([5.0, 0.0, 0.0]), json!([0.0, 0.0, 90.0]))
        );
        assert_eq!(placed(&apply(&scene(), &frozen)), placed(&apply(&scene(), &steps)));
        // Explode: one Edit Primitive per step that is on, prims with the
        // same overrides together.
        let parts = explode(&steps);
        assert_eq!(parts.len(), 2);
        let mut out = scene();
        for (_, paths, edits) in &parts {
            out = edit_primitive::edit(&out, paths, EditKind::Xform, edits);
        }
        assert_eq!(placed(&out), placed(&apply(&scene(), &steps)));
        record(
            &mut steps,
            "Move",
            &[
                ("/a".into(), "translate", json!([1.0, 0.0, 0.0])),
                ("/b".into(), "translate", json!([1.0, 0.0, 0.0])),
            ],
        );
        assert_eq!(
            explode(&steps).last().unwrap().1,
            vec!["/a".to_string(), "/b".to_string()]
        );
    }

    #[test]
    fn places_are_read_from_the_scene() {
        let EvalResult::Named(prims) = scene() else { panic!() };
        let p = places(&prims, &["/b".into(), "/nope".into()]);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].trs[0], [2.0, 0.0, 0.0]);
    }
}
