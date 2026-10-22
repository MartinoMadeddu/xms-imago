//! The regression suite: every template loaded, cooked and measured, the
//! numbers compared with the baseline checked in at
//! `tests/regression/templates.json`.
//!
//! A template is a working graph on real files, so the suite covers the
//! loaders, the operators and the examples at once. What is measured is what
//! a template puts out at its view flag: how many primitives, points,
//! triangles, copies, materials, cameras; for a clip, its frames, joints,
//! rate and skin; which example files were missing; and how long the cook
//! took (recorded, not compared). A template whose files are fetched on
//! demand (`examples::SOURCES`) is skipped when they are not there, so the
//! suite passes offline and in full once `download_example_files` has run.
//! A number that moves fails the test, with
//! the template, the field and both values in the message, so the change is
//! either a regression to fix or a new baseline to write:
//!
//!     cargo test --release regression            # compare
//!     XMS_REGRESS_UPDATE=1 cargo test --release regression   # write the baseline again
//!
//! The GitHub workflow in `.github/workflows/tests.yml` runs it on every
//! push and pull request and puts the differences in the job summary.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ice::IceGraphStore;
use crate::node_graph::NodeGraphState;
use crate::types::{EvalResult, IceGraphId};

/// What one template puts out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub nodes:      usize,
    /// The operator with the view flag, and which of its outputs.
    pub viewed:     String,
    pub output:     usize,
    /// The kind of result at the view flag: prims, mesh, clip, material, none.
    pub kind:       String,
    pub prims:      usize,
    /// Copies of primitives drawn (an instancer's count), over all prims.
    pub copies:     usize,
    pub points:     usize,
    pub triangles:  usize,
    pub materials:  usize,
    pub cameras:    usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip:       Option<ClipSnapshot>,
    /// Example files the template names that were not found.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing:    Vec<String>,
    /// Cook time in milliseconds: recorded for the report, never compared.
    #[serde(default)]
    pub cook_ms:    u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClipSnapshot {
    pub frames:   usize,
    pub joints:   usize,
    pub rate:     String,
    pub skin:     usize,
    /// Hips position on the last frame, rounded to the millimetre, so a
    /// solver or a retarget that moves is seen.
    pub hips_end: [f32; 3],
}

/// Build and measure one template.
pub fn snapshot(t: &crate::templates::Template) -> Snapshot {
    let (mut g, mut store) = (NodeGraphState::default(), IceGraphStore::default());
    (t.build)(&mut g, &mut store);
    measure(&g, &store)
}

/// Measure a graph at its view flag.
pub fn measure(g: &NodeGraphState, store: &IceGraphStore) -> Snapshot {
    let eval = |sid: IceGraphId, mesh: &crate::core::geo::Geo, template: &[crate::ice::Reference]| -> crate::core::geo::Geo {
        store.get(sid).map(|sg| sg.evaluate(mesh, template)).unwrap_or_else(|| mesh.clone())
    };
    let mut s = Snapshot { nodes: g.nodes.len(), ..Default::default() };
    s.missing = crate::downloads::file_paths(g).into_iter()
        .filter(|p| !std::path::Path::new(p).is_file())
        .map(|p| relative_to_examples(&p)).collect();
    s.missing.sort();
    let Some(id) = g.view_flag else { s.kind = "none".into(); return s };
    s.viewed = g.nodes.iter().find(|n| n.id == id).map(|n| n.name.clone()).unwrap_or_default();
    s.output = g.viewed_output();
    let t0 = std::time::Instant::now();
    let result = g.eval_node_out(id, s.output, &mut Default::default(), &eval);
    s.cook_ms = t0.elapsed().as_millis() as u64;
    match result {
        None => s.kind = "none".into(),
        Some(EvalResult::Single(m)) => {
            s.kind = "mesh".into();
            s.prims = 1; s.copies = 1;
            s.points = m.vertices.len(); s.triangles = m.indices.len() / 3;
        }
        Some(EvalResult::Named(prims)) => {
            s.kind = "prims".into();
            s.prims = prims.len();
            for p in &prims {
                let n = p.place.copies().max(1);
                s.copies += n;
                s.points += p.mesh.vertices.len() * n;
                s.triangles += p.mesh.indices.len() / 3 * n;
                if p.camera.is_some() { s.cameras += 1; }
            }
            let mut materials: Vec<&str> = prims.iter().filter_map(|p| p.material.as_deref()).collect();
            materials.sort(); materials.dedup();
            s.materials = materials.len();
        }
        Some(EvalResult::Anim(c)) => {
            s.kind = "clip".into();
            let h = crate::core::human::Human::of(&c);
            let hips = h.get(crate::core::human::Slot::Hips).or_else(|| c.hip_joint());
            let hips_end = hips.map(|j| {
                let p = c.world_pose(c.frames.max(1) - 1)[j].w_axis;
                [round_mm(p.x), round_mm(p.y), round_mm(p.z)]
            }).unwrap_or_default();
            s.clip = Some(ClipSnapshot {
                frames: c.frames, joints: c.joints.len(),
                rate: format!("{}/{}", c.rate.num, c.rate.den),
                skin: c.skin.as_ref().map(|k| k.positions.len()).unwrap_or(0),
                hips_end,
            });
        }
        Some(EvalResult::Material(_)) => { s.kind = "material".into(); s.materials = 1; }
    }
    s
}

fn round_mm(v: f32) -> f32 { (v * 1000.0).round() / 1000.0 }

/// A path under the examples folder, as the baseline names it, so the
/// baseline is the same on every machine.
fn relative_to_examples(p: &str) -> String {
    let p = p.replace('\\', "/");
    match crate::examples::dir() {
        Some(d) => {
            let d = d.to_string_lossy().replace('\\', "/");
            p.strip_prefix(&format!("{d}/")).map(|r| r.to_string()).unwrap_or(p)
        }
        None => p,
    }
}

/// Where the baseline lives: `tests/regression/templates.json` at the root
/// of the source tree (XMS_REGRESS_BASELINE overrides it).
pub fn baseline_path() -> std::path::PathBuf {
    if let Some(p) = std::env::var_os("XMS_REGRESS_BASELINE") { return p.into(); }
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("regression").join("templates.json")
}

/// The fields of two snapshots that differ, as "field: was X, now Y".
pub fn differences(was: &Snapshot, now: &Snapshot) -> Vec<String> {
    let mut out = vec![];
    let mut cmp = |name: &str, a: String, b: String| if a != b { out.push(format!("{name}: was {a}, now {b}")); };
    cmp("nodes", was.nodes.to_string(), now.nodes.to_string());
    cmp("viewed", was.viewed.clone(), now.viewed.clone());
    cmp("output", was.output.to_string(), now.output.to_string());
    cmp("kind", was.kind.clone(), now.kind.clone());
    cmp("prims", was.prims.to_string(), now.prims.to_string());
    cmp("copies", was.copies.to_string(), now.copies.to_string());
    cmp("points", was.points.to_string(), now.points.to_string());
    cmp("triangles", was.triangles.to_string(), now.triangles.to_string());
    cmp("materials", was.materials.to_string(), now.materials.to_string());
    cmp("cameras", was.cameras.to_string(), now.cameras.to_string());
    cmp("missing", format!("{:?}", was.missing), format!("{:?}", now.missing));
    match (&was.clip, &now.clip) {
        (Some(a), Some(b)) => {
            cmp("clip.frames", a.frames.to_string(), b.frames.to_string());
            cmp("clip.joints", a.joints.to_string(), b.joints.to_string());
            cmp("clip.rate", a.rate.clone(), b.rate.clone());
            cmp("clip.skin", a.skin.to_string(), b.skin.to_string());
            // A millimetre either way is the same pose.
            let moved = a.hips_end.iter().zip(&b.hips_end).any(|(x, y)| (x - y).abs() > 0.0015);
            if moved { cmp("clip.hips_end", format!("{:?}", a.hips_end), format!("{:?}", b.hips_end)); }
        }
        (None, None) => {}
        (a, b) => cmp("clip", format!("{a:?}"), format!("{b:?}")),
    }
    out
}

/// Snapshots of every template, by name.
pub fn all() -> BTreeMap<String, Snapshot> {
    crate::templates::TEMPLATES.iter().map(|t| (t.name.to_string(), snapshot(t))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every template, cooked and compared with the baseline. Set
    /// XMS_REGRESS_UPDATE=1 to write the baseline from what the program does
    /// now. The report of the run goes to XMS_REGRESS_REPORT when set.
    #[test]
    fn regression_every_template_against_the_baseline() {
        let now = all();
        let path = baseline_path();
        if std::env::var_os("XMS_REGRESS_UPDATE").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, serde_json::to_string_pretty(&now).unwrap() + "\n").unwrap();
            println!("baseline written: {}", path.display());
        }
        let was: BTreeMap<String, Snapshot> = std::fs::read_to_string(&path).ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| panic!("no baseline at {}: run with XMS_REGRESS_UPDATE=1 to write it", path.display()));
        let mut lines: Vec<String> = vec![];
        let mut failed = 0;
        for (name, s) in &now {
            // A template whose files are fetched on demand, run without
            // them, is skipped, not failed: the baseline holds the numbers
            // with the files there.
            if !s.missing.is_empty() && s.missing.iter().all(|m| crate::examples::downloadable(m).is_some()) {
                lines.push(format!("SKIPPED  {name}: not downloaded: {} (cargo test --release download_example_files -- --ignored)", s.missing.join(", ")));
                continue;
            }
            match was.get(name) {
                None => { lines.push(format!("NEW      {name}: no baseline yet ({} prims, {} triangles, {} ms)", s.prims, s.triangles, s.cook_ms)); failed += 1; }
                Some(w) => {
                    let diff = differences(w, s);
                    if diff.is_empty() { lines.push(format!("ok       {name} ({} ms)", s.cook_ms)); }
                    else { lines.push(format!("CHANGED  {name}: {}", diff.join("; "))); failed += 1; }
                }
            }
        }
        for name in was.keys().filter(|n| !now.contains_key(*n)) { lines.push(format!("GONE     {name}: in the baseline, no such template now")); failed += 1; }
        let report = lines.join("\n");
        println!("{report}");
        if let Some(p) = std::env::var_os("XMS_REGRESS_REPORT") {
            let _ = std::fs::write(p, format!("{report}\n"));
        }
        assert!(failed == 0, "{failed} template(s) differ from the baseline:\n{}\nIf the change is intended, run with XMS_REGRESS_UPDATE=1 and commit tests/regression/templates.json.",
            lines.iter().filter(|l| !l.starts_with("ok")).cloned().collect::<Vec<_>>().join("\n"));
    }

    /// The differences a change makes are named field by field.
    #[test]
    fn differences_are_named() {
        let a = Snapshot { prims: 3, triangles: 12, kind: "prims".into(), ..Default::default() };
        let mut b = a.clone();
        assert!(differences(&a, &b).is_empty());
        b.triangles = 14;
        b.missing = vec!["usd/x.usd".into()];
        assert_eq!(differences(&a, &b), ["triangles: was 12, now 14", "missing: was [], now [\"usd/x.usd\"]"]);
        // Timing is not a difference.
        b = a.clone(); b.cook_ms = 999;
        assert!(differences(&a, &b).is_empty());
    }
}
