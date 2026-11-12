//! The MaterialX materials of a scene checked before a renderer reads them
//! (`xms_matx::check`): what would make it fail is said, and the render not
//! started, where a broken network could stop Imago with the renderer.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::types::Prim;

/// What in the scene's MaterialX materials would make a renderer fail, one
/// line each (`material: node: what`). `renderer`: the MaterialX definitions
/// it has, by name, and its name.
pub fn render_problems(prims: &[Prim], renderer: Option<(&HashSet<String>, &str)>) -> Vec<String> {
    let lib = super::lib();
    let mut seen: Vec<*const xms_matx::Graph> = vec![];
    let mut out = vec![];
    for p in prims {
        let Some(m) = &p.shading else { continue };
        let Some(g) = &m.graph else { continue };
        if seen.contains(&Arc::as_ptr(g)) {
            continue;
        }
        seen.push(Arc::as_ptr(g));
        for problem in xms_matx::check::problems(g, lib, renderer) {
            out.push(format!("{}: {problem}", m.name));
        }
    }
    out
}

/// Why a scene is not rendered: its materials' problems, the first few.
pub fn refusal(problems: &[String], renderer: &str) -> Option<String> {
    const SHOWN: usize = 6;
    if problems.is_empty() {
        return None;
    }
    let mut text =
        format!("Not rendered: {renderer} would fail on a MaterialX material. Fix it in its MaterialX graph:");
    for p in problems.iter().take(SHOWN) {
        text.push_str(&format!("\n  {p}"));
    }
    if problems.len() > SHOWN {
        text.push_str(&format!("\n  and {} more", problems.len() - SHOWN));
    }
    Some(text)
}

/// The MaterialX definitions an SDK ships, by name: those of every `.mtlx`
/// file in its folders. Read once per SDK; `None` when it has none.
pub fn sdk_definitions(sdk: &Path) -> Option<Arc<HashSet<String>>> {
    static READ: Mutex<Option<HashMap<PathBuf, Option<Arc<HashSet<String>>>>>> = Mutex::new(None);
    let mut read = READ.lock().unwrap_or_else(|e| e.into_inner());
    let read = read.get_or_insert_with(HashMap::new);
    if let Some(d) = read.get(sdk) {
        return d.clone();
    }
    let mut files = vec![];
    mtlx_files(sdk, 0, &mut files);
    let texts: Vec<String> = files.iter().filter_map(|f| std::fs::read_to_string(f).ok()).collect();
    let defs = xms_matx::check::defined(&texts);
    let found = (!defs.is_empty()).then(|| Arc::new(defs));
    read.insert(sdk.to_path_buf(), found.clone());
    found
}

/// The `.mtlx` files below `dir`, a few folders down.
fn mtlx_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    const DEPTH: usize = 6;
    const MOST: usize = 2000;
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for e in read.flatten() {
        if out.len() >= MOST {
            return;
        }
        let p = e.path();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            if depth < DEPTH {
                mtlx_files(&p, depth + 1, out);
            }
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mtlx")) {
            out.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_broken_material_is_said_and_refused() {
        let lib = super::super::lib();
        let mut g = super::super::new_graph();
        let s = g.surface_shader().unwrap();
        g.node_mut(s).unwrap().inputs.push((
            "coat".into(),
            xms_matx::Binding::Connect {
                node: 999,
                output: "out".into(),
            },
        ));
        let m = crate::core::material::Material::from_graph("lion", &g);
        let p = Prim {
            shading: Some(Arc::new(m)),
            ..Prim::new("/materials/lion".into(), Default::default())
        };
        let problems = render_problems(&[p.clone(), p], None);
        assert_eq!(problems.len(), 1, "each material once: {problems:?}");
        assert!(problems[0].starts_with("lion: standard_surface"), "{problems:?}");
        let why = refusal(&problems, "Arnold").unwrap();
        assert!(why.contains("Not rendered") && why.contains("not in the graph"));
        assert!(refusal(&[], "Arnold").is_none());
        let _ = lib;
    }

    #[test]
    fn an_sdk_s_definitions_are_read_from_its_files() {
        let dir = std::env::temp_dir().join(format!("xms-mtlx-sdk-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("materialx/libraries/stdlib")).unwrap();
        std::fs::write(
            dir.join("materialx/libraries/stdlib/stdlib_defs.mtlx"),
            "<materialx><nodedef name=\"ND_add_float\" node=\"add\"/></materialx>",
        )
        .unwrap();
        let defs = sdk_definitions(&dir).unwrap();
        assert!(defs.contains("ND_add_float"));
        assert!(sdk_definitions(&dir.join("nothing")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
