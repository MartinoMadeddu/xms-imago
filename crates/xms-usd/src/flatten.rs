//! A stage written as one layer, as USD's `UsdStage::Flatten`: what Write USD's
//! Flatten gives.

use std::path::Path;

use openusd::usd::Stage;

/// The stage `layer` opens (its sublayers, references and payloads composed),
/// written into one layer at `out`: `.usda` or `.usdc` by its extension. Every
/// prim with its composed values and time samples, no arcs left; asset paths
/// anchored to where their layer was, so the file can move.
pub fn flatten(layer: &Path, out: &Path) -> Result<(), String> {
    crate::register_formats();
    let text = |p: &Path| {
        p.to_str()
            .map(str::to_string)
            .ok_or(format!("{}: not UTF-8", p.display()))
    };
    let stage = Stage::open(text(layer)?.as_str()).map_err(|e| format!("{}: {e}", layer.display()))?;
    let flat = stage
        .flatten()
        .map_err(|e| format!("Flattening {}: {e}", layer.display()))?;
    flat.export(text(out)?).map_err(|e| format!("{}: {e}", out.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An override layer over a file, flattened: one layer with the file's
    /// prims, the override's values and time samples, and no sublayer.
    #[test]
    fn an_override_layer_and_its_file_become_one() {
        let dir = std::env::temp_dir().join("xms_usd_flatten_test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("base.usda"),
            "#usda 1.0\ndef Xform \"set\" {\n    def Sphere \"ball\" {\n        double radius = 1\n    }\n}\n",
        )
        .unwrap();
        let over = dir.join("over.usda");
        std::fs::write(
            &over,
            "#usda 1.0\n(\n    subLayers = [@./base.usda@]\n)\nover \"set\" {\n    over \"ball\" {\n        double radius.timeSamples = { 1: 2, 10: 3 }\n    }\n}\n",
        )
        .unwrap();
        let out = dir.join("flat.usda");
        flatten(&over, &out).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(!text.contains("subLayers"), "{text}");
        assert!(text.contains("def Sphere \"ball\""), "{text}");
        assert!(text.contains("radius.timeSamples"), "{text}");
    }
}
