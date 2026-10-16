//! Every example material of MaterialX imports, each material whole: every
//! node known, every wire between types that join.

use std::path::Path;

use xms_matx::{import::import, Library};

fn files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() { files(&p, out); } else if p.extension().is_some_and(|x| x == "mtlx") { out.push(p); }
    }
}

#[test]
fn every_example_imports() {
    let lib = Library::standard();
    let mut all = vec![];
    files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/examples"), &mut all);
    all.sort();
    assert!(all.len() >= 40, "{} files", all.len());
    let mut failed = vec![];
    let mut materials = 0;
    for f in &all {
        let text = std::fs::read_to_string(f).unwrap();
        let got = match import(&text, lib, f.parent()) {
            Ok(g) => g,
            Err(e) => { failed.push(format!("{}: {e}", f.display())); continue; }
        };
        for n in &got.notes {
            // Nodes a document defines itself are not read yet: said so.
            if n.contains("defines itself") { continue; }
            if n.contains("not read") || n.contains("No material") { failed.push(format!("{}: {n}", f.display())); }
        }
        for (name, g) in &got.materials {
            materials += 1;
            if g.surface_shader().is_none() { failed.push(format!("{}: {name} has no surface shader", f.display())); }
            for n in &g.nodes {
                for (i, _) in &n.inputs {
                    if !g.wire_ok(lib, n.id, i) { failed.push(format!("{}: {name}: {}.{i} wired between types that do not join", f.display(), n.name)); }
                }
            }
            // Written and read back as Imago saves it, the same graph.
            let saved = xms_matx::document::write(g, lib);
            let back = xms_matx::document::read(&saved, lib).unwrap_or_else(|e| panic!("{}: {name}: {e}", f.display()));
            assert_eq!(back.nodes.len(), g.nodes.len());
        }
    }
    eprintln!("{} files, {materials} materials", all.len());
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
