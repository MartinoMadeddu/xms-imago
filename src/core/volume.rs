//! Volumes: OpenVDB files (`.vdb`) in the scene, each a USD `Volume` with a
//! field per grid. Imago reads what a file says of its grids (names, types,
//! voxel sizes, bounds), not their voxels: renderers read the file itself.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use glam::{Mat4, Vec3};

/// Where Load VDB puts its volume unless told: under `/volumes`, named after
/// the operator (`/volumes/LoadVdb`).
pub const DEFAULT_LOCATION: &str = "/volumes/$OPNAME";

/// A volume: the file of the frame cooked and the grids it holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Volume {
    /// The file, its variables and frame put in: what renderers read.
    pub file: String,
    pub grids: Vec<Grid>,
}

/// A grid of a VDB file, as the file describes it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grid {
    pub name: String,
    /// Its field's name in USD: its name as an identifier, `_1`, `_2`… after
    /// it when an earlier grid has the same name (`field_name`).
    pub field: String,
    /// Among the grids of its name, which (USD's `fieldIndex`): 0 for the
    /// first.
    pub index: usize,
    /// Its values' type: `float`, `vec3s`, `int32`…
    pub value_type: String,
    /// `level set`, `fog volume`, `staggered`, or `unknown`.
    pub class: String,
    /// A voxel's size, in the file's units (metres in Imago).
    pub voxel_size: [f64; 3],
    /// The box around its active voxels (their whole extent, not their
    /// centres), in the file's space; `None` when the file does not say (it
    /// was written without OpenVDB's statistics) or the grid is empty.
    pub bounds: Option<([f64; 3], [f64; 3])>,
    /// How many voxels are active, when the file says.
    pub active_voxels: Option<u64>,
}

impl Grid {
    /// Whether renderers read it as a field: not one of points (OpenVDB's
    /// point data, `ptdataidx32`).
    pub fn is_field(&self) -> bool {
        !self.value_type.starts_with("ptdata")
    }
}

impl Volume {
    /// The box around every grid's, when any has one.
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        self.grids.iter().filter_map(|g| g.bounds).reduce(|(a, b), (c, d)| {
            (
                [a[0].min(c[0]), a[1].min(c[1]), a[2].min(c[2])],
                [b[0].max(d[0]), b[1].max(d[1]), b[2].max(d[2])],
            )
        })
    }

    /// The lines the Viewer draws for it, placed at `world`: the edges of its
    /// box. None when its bounds are not known.
    pub fn glyph(&self, world: Mat4) -> Vec<[Vec3; 2]> {
        let Some((lo, hi)) = self.bounds() else {
            return vec![];
        };
        let corner = |k: usize| {
            let pick = |axis: usize| if k & (1 << axis) == 0 { lo[axis] } else { hi[axis] };
            world.transform_point3(Vec3::new(pick(0) as f32, pick(1) as f32, pick(2) as f32))
        };
        // Corners differing in one axis: the twelve edges.
        let mut out = vec![];
        for k in 0..8 {
            for axis in 0..3 {
                if k & (1 << axis) == 0 {
                    out.push([corner(k), corner(k | 1 << axis)]);
                }
            }
        }
        out
    }

    /// What the Primitive Inspector lists of it: its file, then each grid.
    pub fn properties(&self) -> Vec<(String, String)> {
        let mut out = vec![("filePath".to_string(), self.file.clone())];
        for g in &self.grids {
            let size = g.voxel_size.map(|x| format!("{x:.4}")).join(" × ");
            let mut text = format!("{} {}, voxels {size}", g.class, g.value_type);
            if let Some(n) = g.active_voxels {
                text.push_str(&format!(", {n} active"));
            }
            if !g.is_field() {
                text.push_str(", points: not a field");
            }
            out.push((format!("field:{}", g.field), text));
        }
        out
    }
}

/// A grid's name as a USD identifier, for its field (`density`, `vel`):
/// letters, digits and `_`, not starting with a digit.
pub fn field_name(grid: &str) -> String {
    let mut out: String = grid
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// The grids of the VDB file `file`, from its header and each grid's
/// metadata (no voxels are read).
pub fn read(file: &str) -> Result<Volume, String> {
    use vdb::math::bbox::BBoxd;
    use vdb::math::vec3::Vec3;
    let metas = vdb::io::read_grid_metadata(file).map_err(|e| format!("{file}: {e}"))?;
    let mut grids: Vec<Grid> = metas
        .iter()
        .map(|m| {
            let meta = |name: &str| m.metadata.get(name);
            let stored = meta("file_bbox_min")
                .and_then(|v| v.as_vec3i())
                .zip(meta("file_bbox_max").and_then(|v| v.as_vec3i()));
            // The voxels' whole extent: from half a voxel below the first
            // centre to half a voxel above the last.
            let bounds = stored
                .zip(m.transform.as_ref())
                .filter(|((lo, hi), _)| (0..3).all(|i| lo.0[i] <= hi.0[i]))
                .map(|((lo, hi), t)| {
                    let corner = |c: [i32; 3], d: f64| Vec3(c.map(|x| x as f64 + d));
                    let b = t.index_to_world_bboxd(&BBoxd::new(corner(lo.0, -0.5), corner(hi.0, 0.5)));
                    (b.min().0, b.max().0)
                });
            Grid {
                name: m.descriptor.grid_name.clone(),
                field: String::new(),
                index: 0,
                value_type: m.descriptor.value_type().unwrap_or(&m.descriptor.grid_type).to_string(),
                class: m.metadata.get_str("class").unwrap_or("unknown").to_string(),
                voxel_size: m.transform.as_ref().map_or([0.0; 3], |t| t.voxel_size().0),
                bounds,
                active_voxels: meta("file_voxel_count")
                    .and_then(|v| v.as_i64())
                    .map(|n| n.max(0) as u64),
            }
        })
        .collect();
    // Fields named apart: a second `density` is `density_1`.
    for k in 0..grids.len() {
        let index = grids[..k].iter().filter(|g| g.name == grids[k].name).count();
        let field = field_name(&grids[k].name);
        grids[k].field = if index == 0 { field } else { format!("{field}_{index}") };
        grids[k].index = index;
    }
    Ok(Volume {
        file: file.to_string(),
        grids,
    })
}

/// The file read, kept until it changes on disk: the cook and the panel ask
/// for it often, and a sequence reads a file per frame.
pub fn cached(file: &str) -> Result<Arc<Volume>, String> {
    type Read = Result<Arc<Volume>, String>;
    static READ: Mutex<Option<HashMap<String, (Option<std::time::SystemTime>, Read)>>> = Mutex::new(None);
    let stamp = std::fs::metadata(file).and_then(|m| m.modified()).ok();
    let mut read = READ.lock().unwrap_or_else(|e| e.into_inner());
    let read = read.get_or_insert_with(HashMap::new);
    if let Some((at, done)) = read.get(file) {
        if *at == stamp {
            return done.clone();
        }
    }
    let done = read_file(file);
    read.insert(file.to_string(), (stamp, done.clone()));
    done
}

fn read_file(file: &str) -> Result<Arc<Volume>, String> {
    if !std::path::Path::new(file).is_file() {
        return Err(format!("{file}: no such file"));
    }
    read(file).map(Arc::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_names_become_field_names() {
        assert_eq!(field_name("density"), "density");
        assert_eq!(field_name("vel.x"), "vel_x");
        assert_eq!(field_name("2d"), "_2d");
        assert_eq!(field_name(""), "_");
    }

    #[test]
    fn its_box_is_every_grid_s() {
        let grid = |lo: [f64; 3], hi: [f64; 3]| Grid {
            bounds: Some((lo, hi)),
            ..Default::default()
        };
        let v = Volume {
            file: "smoke.vdb".into(),
            grids: vec![
                grid([-1.0, 0.0, 0.0], [1.0, 2.0, 1.0]),
                grid([0.0, -3.0, 0.0], [4.0, 0.0, 0.5]),
                Grid::default(),
            ],
        };
        assert_eq!(v.bounds(), Some(([-1.0, -3.0, 0.0], [4.0, 2.0, 1.0])));
        let edges = v.glyph(Mat4::IDENTITY);
        assert_eq!(edges.len(), 12);
        assert!(edges
            .iter()
            .all(|[a, b]| (*a - *b).abs().cmpgt(Vec3::splat(0.0)).bitmask().count_ones() == 1));
        assert!(Volume::default().glyph(Mat4::IDENTITY).is_empty());
    }

    #[test]
    fn a_missing_file_says_so() {
        assert!(cached("/no/such/smoke.vdb").unwrap_err().contains("no such file"));
    }
}
