//! How the Viewer shows skin weights while a Hard Bind, Envelope, Delta Mush
//! or Bake Weights operator is viewed: each joint's region in a colour of its
//! own (the joint each point weighs most to), or, with a joint chosen in the
//! operator's Parameters, that joint's weights from dark blue (none) through
//! red to yellow (all). The choice is the Viewer's, not the operator's: it
//! cooks nothing again.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::surface::Surface;
use crate::fbx_loader::{SKIN_JOINTS, SKIN_WEIGHTS};
use crate::types::{MeshData, Prim};

static SHOWN: Mutex<Option<String>> = Mutex::new(None);
static REVISION: AtomicU64 = AtomicU64::new(0);

/// The joint whose weights are shown; None shows the regions.
pub fn shown() -> Option<String> {
    SHOWN.lock().ok().and_then(|s| s.clone())
}

pub fn show(joint: Option<String>) {
    if let Ok(mut s) = SHOWN.lock() {
        if *s != joint {
            *s = joint;
            REVISION.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Changes whenever what is shown does: the Viewer builds its scene again.
pub fn revision() -> u64 {
    REVISION.load(Ordering::Relaxed)
}

/// A colour of its own for a joint, from its name: hues spread by the
/// golden angle, light enough to read the shape.
pub fn joint_colour(name: &str) -> [f32; 3] {
    let mut h: u64 = 1469598103934665603;
    for b in name.bytes() {
        h = (h ^ b as u64).wrapping_mul(1099511628211);
    }
    let hue = ((h % 4096) as f32 * 0.618_034).fract();
    let sat = 0.55 + ((h >> 40) % 30) as f32 / 100.0;
    hsv(hue, sat, 0.85)
}

fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - f * s), v * (1.0 - (1.0 - f) * s));
    match i as i32 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// A weight from 0 to 1 as dark blue, red, yellow.
pub fn heat(w: f32) -> [f32; 3] {
    let w = w.clamp(0.0, 1.0);
    if w < 0.5 {
        let t = w * 2.0;
        [0.08 + 0.82 * t, 0.1 * (1.0 - t), 0.35 * (1.0 - t)]
    } else {
        let t = (w - 0.5) * 2.0;
        [0.9 + 0.1 * t, 0.85 * t, 0.1 * t]
    }
}

/// Pieces of a weighted primitive, as `painted`, kept while the primitive and
/// the choice stay the same: the Viewer asks every time it builds its scene.
pub fn painted_kept(p: &Prim, joint: Option<&str>) -> Option<Arc<Vec<(Arc<MeshData>, [f32; 3])>>> {
    type Kept = Vec<((usize, Option<String>), Option<Arc<Vec<(Arc<MeshData>, [f32; 3])>>>)>;
    static KEPT: Mutex<Kept> = Mutex::new(vec![]);
    let key = (Arc::as_ptr(&p.mesh) as usize, joint.map(str::to_string));
    if let Ok(k) = KEPT.lock() {
        if let Some(x) = k.iter().find(|x| x.0 == key) {
            return x.1.clone();
        }
    }
    let made = painted(p, joint).map(Arc::new);
    if let Ok(mut k) = KEPT.lock() {
        k.push((key, made.clone()));
        if k.len() > 16 {
            k.remove(0);
        }
    }
    made
}

/// A weighted primitive cut into pieces of one colour each, in the world:
/// by region, or by `joint`'s weight in tenths. None without weights.
pub fn painted(p: &Prim, joint: Option<&str>) -> Option<Vec<(Arc<MeshData>, [f32; 3])>> {
    let geo = p.geo.as_ref()?;
    let tokens: Vec<(&[Arc<str>], &[u32])> = SKIN_JOINTS
        .iter()
        .map(|n| {
            let a = geo.attr(crate::core::geo::Context::Point, n)?;
            Some((a.column.tokens()?, a.indices.as_deref()?.as_slice()))
        })
        .collect::<Option<Vec<_>>>()?;
    let weights = geo
        .attr(crate::core::geo::Context::Point, SKIN_WEIGHTS)?
        .column
        .vec4s()?;
    let s = Surface::of(p)?;
    let n = s.points.len();
    // Per point: the joint it weighs most to, and the shown joint's weight.
    let mut top: Vec<Arc<str>> = Vec::with_capacity(n);
    let mut mine: Vec<f32> = vec![0.0; n];
    for v in 0..n {
        let w = weights.get(v).copied().unwrap_or([1.0, 0.0, 0.0, 0.0]);
        let mut best = (0usize, f32::MIN);
        for k in 0..4 {
            if w[k] > best.1 {
                best = (k, w[k]);
            }
            if let Some(j) = joint {
                let (names, ix) = tokens[k];
                if ix
                    .get(v)
                    .and_then(|i| names.get(*i as usize))
                    .is_some_and(|nm| &**nm == j)
                {
                    mine[v] += w[k];
                }
            }
        }
        let (names, ix) = tokens[best.0];
        top.push(
            ix.get(v)
                .and_then(|i| names.get(*i as usize))
                .cloned()
                .unwrap_or_else(|| Arc::from("")),
        );
    }
    let normals: Vec<[f32; 3]> = s.normals(&s.points).iter().map(|v| v.to_array()).collect();
    let vertices: Vec<[f32; 3]> = s.points.iter().map(|v| v.to_array()).collect();
    let mut groups: HashMap<String, Vec<u32>> = HashMap::new();
    for poly in &s.polys {
        if poly.len() < 3 {
            continue;
        }
        let key = match joint {
            Some(_) => {
                let w = poly.iter().map(|i| mine[*i as usize]).sum::<f32>() / poly.len() as f32;
                format!("{}", (w * 10.0).round() as i32)
            }
            None => top[poly[0] as usize].to_string(),
        };
        let g = groups.entry(key).or_default();
        for k in 1..poly.len() - 1 {
            g.extend([poly[0], poly[k], poly[k + 1]]);
        }
    }
    let mut out = vec![];
    for (key, indices) in groups {
        let colour = match joint {
            Some(_) => heat(key.parse::<f32>().unwrap_or(0.0) / 10.0),
            None => joint_colour(&key),
        };
        // Only the points its polygons use.
        let mut map: HashMap<u32, u32> = HashMap::new();
        let mut md = MeshData::default();
        for i in indices {
            let k = *map.entry(i).or_insert_with(|| {
                md.vertices.push(vertices[i as usize]);
                md.normals.push(normals[i as usize]);
                md.vertices.len() as u32 - 1
            });
            md.indices.push(k);
        }
        out.push((Arc::new(md), colour));
    }
    out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    Some(out)
}
