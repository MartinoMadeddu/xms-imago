//! Paint (docs/paint.md): values painted on prims' points in the Viewer, kept
//! by the Paint operator and put on its output as point attributes, which ICE
//! and the renderers read as any other (USD `vertex` primvars).
//!
//! The operator keeps the values, not the strokes that made them: per prim it
//! painted, its points' positions at rest (in the prim's own space) and, per
//! channel, a value per point. A float channel (density, weights) paints
//! values from 0 to 1; an ID channel (biomes, variants) paints whole numbers,
//! never blended or smoothed. When the prim comes in with other points (it was
//! remodelled), the values are carried to the new points from the nearest
//! point at rest.
//!
//! The values live in memory, shared (`Arc`) with the undo history; the graph
//! file holds only a name and a hash of them. On save they are written beside
//! the graph, to `paint/<stem>.<hash>.mcol`: named by their content, so the
//! same paint is the same file and a file being read is never written over.
//!
//! The brush (`dab`) works on the points' positions in the world: the points
//! within its radius, weighted by a smooth falloff, optionally only where the
//! surface is no steeper than an angle, optionally mirrored across X in the
//! prim's own space.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use glam::{Mat4, Vec3};

use crate::core::geo::{Attr, Column, Context, Geo, Role, Topology};
use crate::core::geo_mesh::GeoMesh;
use crate::types::{EvalResult, Prim};

/// What a channel holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ChannelKind {
    /// 0 to 1, blended and smoothed: density, weights, length.
    #[default]
    Float,
    /// Whole numbers: regions, biomes, variants. Painted whole, never blended.
    Id,
}

/// A channel: the point attribute it puts on the prims.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Channel {
    pub name: String,
    #[serde(default)]
    pub kind: ChannelKind,
    /// Where nothing was painted.
    #[serde(default)]
    pub default: f32,
    /// An ID channel's names, by ID from 1 (`grass`, `fern`, `rock`): shown
    /// in the Parameters and the Viewer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}

impl Channel {
    pub fn float(name: &str) -> Channel {
        Channel {
            name: name.into(),
            kind: ChannelKind::Float,
            default: 0.0,
            labels: vec![],
        }
    }

    pub fn id(name: &str, labels: &[&str]) -> Channel {
        Channel {
            name: name.into(),
            kind: ChannelKind::Id,
            default: 0.0,
            labels: labels.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// What was painted on one prim.
#[derive(Clone, Debug, PartialEq)]
pub struct PrimPaint {
    /// Its points at rest, in its own space, when it was painted.
    pub rest: Arc<Vec<[f32; 3]>>,
    /// A hash of its topology (the polygons' counts and corners), to tell
    /// when it comes in remodelled.
    pub topology: u64,
    /// Per channel (by name), a value per point.
    pub values: BTreeMap<String, Arc<Vec<f32>>>,
}

/// The values a Paint operator keeps. Saved as `stem` and `hash` only; the
/// values go to their own file (`file_name`).
#[derive(Clone, Debug, Default)]
pub struct PaintData {
    /// The start of its file's name, set when the operator was made: renaming
    /// the operator keeps its file.
    pub stem: String,
    /// A hash of every value: what names the file, and what the undo history
    /// sees change.
    pub hash: u64,
    pub prims: BTreeMap<String, PrimPaint>,
    /// Read from its file: false when the file named was missing.
    pub missing: bool,
}

impl PartialEq for PaintData {
    fn eq(&self, other: &Self) -> bool {
        self.stem == other.stem && self.hash == other.hash
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Saved {
    stem: String,
    #[serde(default)]
    hash: String,
}

impl serde::Serialize for PaintData {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        Saved {
            stem: self.stem.clone(),
            hash: format!("{:016x}", self.hash),
        }
        .serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for PaintData {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = Saved::deserialize(d)?;
        Ok(PaintData {
            stem: s.stem,
            hash: u64::from_str_radix(&s.hash, 16).unwrap_or(0),
            prims: BTreeMap::new(),
            missing: false,
        })
    }
}

impl PaintData {
    pub fn new(stem: &str) -> PaintData {
        PaintData {
            stem: stem.to_string(),
            ..Default::default()
        }
    }

    /// Its file, relative to the graph's folder. None while nothing is painted.
    pub fn file_name(&self) -> Option<String> {
        (!self.prims.is_empty()).then(|| format!("paint/{}.{:016x}.mcol", self.stem, self.hash))
    }

    /// The hash made again from the values: after every change.
    pub fn rehash(&mut self) {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for (path, p) in &self.prims {
            path.hash(&mut h);
            p.topology.hash(&mut h);
            p.rest.len().hash(&mut h);
            for (name, v) in &p.values {
                name.hash(&mut h);
                // The bits of each value, as bytes: fast for millions.
                let bytes: &[u8] =
                    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(&v[..])) };
                bytes.hash(&mut h);
            }
        }
        self.hash = h.finish();
    }

    /// The values of `channel` on `path`, made (at the channel's default) for
    /// a prim of `geo`'s points if it was never painted, or carried to its
    /// points if it was painted with others.
    pub fn values_mut(&mut self, path: &str, geo: &Geo, channel: &Channel) -> &mut Vec<f32> {
        let points = geo.points();
        let topology = topology_hash(geo);
        let entry = self.prims.entry(path.to_string()).or_insert_with(|| PrimPaint {
            rest: Arc::new(points.to_vec()),
            topology,
            values: BTreeMap::new(),
        });
        if entry.topology != topology || entry.rest.len() != points.len() {
            let carried = carry(entry, points, topology);
            *entry = carried;
        }
        let n = points.len();
        let v = entry
            .values
            .entry(channel.name.clone())
            .or_insert_with(|| Arc::new(vec![channel.default; n]));
        Arc::make_mut(v)
    }
}

/// A hash of a geometry's topology: its polygons' (or curves') counts and
/// corners, and its number of points; not where they are.
pub fn topology_hash(geo: &Geo) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    geo.point_count().hash(&mut h);
    match &geo.topology {
        Topology::Points => 0u8.hash(&mut h),
        Topology::Mesh { counts, indices, .. } => {
            1u8.hash(&mut h);
            counts.hash(&mut h);
            indices.hash(&mut h);
        }
        Topology::Curves { counts, .. } => {
            2u8.hash(&mut h);
            counts.hash(&mut h);
        }
    }
    h.finish()
}

/// What was painted on a prim carried to new points: each takes the values of
/// the nearest point at rest.
fn carry(old: &PrimPaint, points: &[[f32; 3]], topology: u64) -> PrimPaint {
    let tree = xms_ice::spatial::KdPointTree::build(&old.rest);
    let mut nearest = Vec::with_capacity(points.len());
    let mut found = vec![];
    for p in points {
        found.clear();
        tree.near(*p, 1, 0.0, &mut found);
        nearest.push(found.first().map(|(i, _)| *i as usize));
    }
    let values = old
        .values
        .iter()
        .map(|(name, v)| {
            let moved: Vec<f32> = nearest.iter().map(|i| i.and_then(|i| v.get(i).copied()).unwrap_or(0.0)).collect();
            (name.clone(), Arc::new(moved))
        })
        .collect();
    PrimPaint {
        rest: Arc::new(points.to_vec()),
        topology,
        values,
    }
}

/// The geometry of a prim, its own or made from its mesh.
pub fn geo_of(p: &Prim) -> Geo {
    p.geo.as_deref().cloned().unwrap_or_else(|| Geo::from_mesh(&p.mesh))
}

/// The scene with what was painted put on it: on each prim painted, a point
/// attribute per channel (an ID channel as integers). Prims never painted pass
/// through untouched; a channel never painted on a painted prim is its default
/// everywhere.
pub fn apply(input: &EvalResult, channels: &[Channel], data: &PaintData) -> EvalResult {
    let EvalResult::Named(prims) = input else {
        return input.clone();
    };
    if data.prims.is_empty() || channels.is_empty() {
        return input.clone();
    }
    let out = prims
        .iter()
        .map(|p| {
            let Some(painted) = data.prims.get(&p.path) else {
                return p.clone();
            };
            let mut geo = geo_of(p);
            let n = geo.point_count();
            // Remodelled since: the values carried to its points.
            let carried;
            let painted = if painted.topology != topology_hash(&geo) || painted.rest.len() != n {
                carried = carry(painted, geo.points(), topology_hash(&geo));
                &carried
            } else {
                painted
            };
            for c in channels {
                let values = painted.values.get(&c.name);
                let column = match c.kind {
                    ChannelKind::Float => Column::Float(
                        values
                            .cloned()
                            .filter(|v| v.len() == n)
                            .unwrap_or_else(|| Arc::new(vec![c.default; n])),
                    ),
                    ChannelKind::Id => Column::Int(Arc::new(match values.filter(|v| v.len() == n) {
                        Some(v) => v.iter().map(|x| x.round() as i32).collect(),
                        None => vec![c.default.round() as i32; n],
                    })),
                };
                geo.set(Context::Point, &c.name, Attr::new(column, Role::None));
            }
            let mut q = p.clone();
            q.mesh = Arc::new(geo.to_mesh());
            q.geo = Some(Arc::new(geo));
            q
        })
        .collect();
    EvalResult::Named(out)
}

// The brush

/// What a dab does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Op {
    #[default]
    Add,
    Subtract,
    /// Towards the brush's value (an ID: to its ID).
    Set,
    /// Towards the average of the points around (floats only).
    Smooth,
    /// Back to the channel's default.
    Erase,
    /// The whole prim clicked set to the brush's value (an ID: its ID), as
    /// far as the slope limit allows: a click, not a stroke.
    Flood,
}

/// The brush: interface state, not the document's (not undone, not saved
/// with the graph).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Brush {
    pub op: Op,
    /// In the world's units.
    pub radius: f32,
    /// 0 to 1: how much of the change one dab makes.
    pub strength: f32,
    /// What Set paints: a float's value, an ID's number.
    pub value: f32,
    /// Also on the other side of the prim's own X = 0.
    pub mirror: bool,
    /// Only where the surface is no steeper than this, in degrees (90: off).
    pub max_slope: f32,
}

impl Default for Brush {
    fn default() -> Self {
        Brush {
            op: Op::Add,
            radius: 1.0,
            strength: 0.5,
            value: 1.0,
            mirror: false,
            max_slope: 90.0,
        }
    }
}

/// The brush, shared by the Viewer and the Paint operator's Parameters.
pub static BRUSH: std::sync::Mutex<Brush> = std::sync::Mutex::new(Brush {
    op: Op::Add,
    radius: 1.0,
    strength: 0.5,
    value: 1.0,
    mirror: false,
    max_slope: 90.0,
});

pub fn brush() -> Brush {
    BRUSH.lock().map(|b| *b).unwrap_or_default()
}

pub fn set_brush(b: Brush) {
    if let Ok(mut x) = BRUSH.lock() {
        *x = b;
    }
}

/// A prim's points as a brush sees them, made once per stroke: in the
/// world, with their normals, searchable.
pub struct Surface {
    pub world: Vec<[f32; 3]>,
    pub normals: Vec<Vec3>,
    pub tree: xms_ice::spatial::KdPointTree,
    /// Its own space to the world, and back.
    pub to_world: Mat4,
    pub to_own: Mat4,
}

impl Surface {
    pub fn new(geo: &Geo, to_world: Mat4) -> Surface {
        let world: Vec<[f32; 3]> =
            geo.points().iter().map(|p| to_world.transform_point3(Vec3::from(*p)).to_array()).collect();
        let normals = point_normals(geo, &world);
        let tree = xms_ice::spatial::KdPointTree::build(&world);
        Surface {
            world,
            normals,
            tree,
            to_world,
            to_own: to_world.inverse(),
        }
    }
}

/// Each point's normal in the world: its polygons' normals, weighted by their
/// area. Up (+Y) where it has none (points, curves).
pub fn point_normals(geo: &Geo, world: &[[f32; 3]]) -> Vec<Vec3> {
    let mut n = vec![Vec3::ZERO; world.len()];
    if let Topology::Mesh { counts, indices, .. } = &geo.topology {
        let mut at = 0usize;
        for &c in counts.iter() {
            let c = c as usize;
            let corners = &indices[at..at + c];
            at += c;
            if c < 3 {
                continue;
            }
            // Newell's normal: the polygon's area times its normal.
            let mut f = Vec3::ZERO;
            for k in 0..c {
                let a = Vec3::from(world[corners[k] as usize]);
                let b = Vec3::from(world[corners[(k + 1) % c] as usize]);
                f += Vec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
            }
            for &i in corners {
                n[i as usize] += f;
            }
        }
    }
    n.into_iter().map(|v| v.try_normalize().unwrap_or(Vec3::Y)).collect()
}

/// How much a point at `d` from the centre of a brush of radius `r` is
/// painted: 1 at the centre, 0 at the edge, smoothly.
pub fn falloff(d: f32, r: f32) -> f32 {
    if r <= 0.0 || d >= r {
        return 0.0;
    }
    let x = d / r;
    let t = 1.0 - x * x;
    t * t
}

/// One dab of the brush at `hit` (in the world) on `values` (one per point
/// of `s`), and at its mirror across the prim's own X when the brush says.
/// The points it changed, for the caller to show.
pub fn dab(values: &mut [f32], s: &Surface, hit: Vec3, b: &Brush, channel: &Channel) -> usize {
    let mut centres = vec![hit];
    if b.mirror {
        let own = s.to_own.transform_point3(hit);
        let other = s.to_world.transform_point3(Vec3::new(-own.x, own.y, own.z));
        if other.distance(hit) > b.radius * 0.25 {
            centres.push(other);
        }
    }
    let cos_max = b.max_slope.clamp(0.0, 90.0).to_radians().cos();
    let mut changed = 0;
    let mut near = vec![];
    for c in centres {
        near.clear();
        s.tree.near(c.to_array(), 0, b.radius, &mut near);
        // Smoothing reads the values before this dab.
        let before: Vec<(u32, f32)> = if b.op == Op::Smooth {
            near.iter().map(|(i, _)| (*i, values[*i as usize])).collect()
        } else {
            vec![]
        };
        for &(i, d) in &near {
            let i = i as usize;
            if b.max_slope < 90.0 && s.normals[i].y < cos_max {
                continue;
            }
            let w = falloff(d, b.radius) * b.strength.clamp(0.0, 1.0);
            if w <= 0.0 {
                continue;
            }
            let v = &mut values[i];
            let old = *v;
            *v = match (channel.kind, b.op) {
                (ChannelKind::Float, Op::Add) => (*v + w).min(1.0),
                (ChannelKind::Float, Op::Subtract) => (*v - w).max(0.0),
                (ChannelKind::Float, Op::Set) => *v + (b.value - *v) * w,
                (ChannelKind::Float, Op::Erase) => *v + (channel.default - *v) * w,
                (ChannelKind::Float, Op::Smooth) => {
                    // The average of the points within a third of the radius.
                    let r = b.radius / 3.0;
                    let p = Vec3::from(s.world[i]);
                    let (mut sum, mut n) = (0.0, 0usize);
                    for &(j, x) in &before {
                        if Vec3::from(s.world[j as usize]).distance(p) <= r {
                            sum += x;
                            n += 1;
                        }
                    }
                    if n == 0 {
                        *v
                    } else {
                        *v + (sum / n as f32 - *v) * w
                    }
                }
                // An ID is painted whole where the brush is more than half on.
                (ChannelKind::Id, Op::Set | Op::Add) if w >= 0.5 * b.strength.max(1e-6) => b.value.round(),
                (ChannelKind::Id, Op::Erase | Op::Subtract) if w >= 0.5 * b.strength.max(1e-6) => channel.default,
                (ChannelKind::Id, _) | (ChannelKind::Float, Op::Flood) => *v,
            };
            if *v != old {
                changed += 1;
            }
        }
    }
    changed
}

/// Every point set to the brush's value (Flood), where the slope allows.
pub fn flood(values: &mut [f32], s: &Surface, b: &Brush, channel: &Channel) {
    let cos_max = b.max_slope.clamp(0.0, 90.0).to_radians().cos();
    let to = match channel.kind {
        ChannelKind::Float => b.value.clamp(0.0, 1.0),
        ChannelKind::Id => b.value.round(),
    };
    for (i, v) in values.iter_mut().enumerate() {
        if b.max_slope < 90.0 && s.normals[i].y < cos_max {
            continue;
        }
        *v = to;
    }
}

// Its file

const MAGIC: &[u8; 8] = b"XMSMCOL1";

/// Its values as the bytes of a `.mcol` file: a header, then per prim its
/// path, topology hash, points at rest and channels; compressed.
pub fn to_bytes(data: &PaintData) -> Vec<u8> {
    use std::io::Write;
    let mut raw: Vec<u8> = vec![];
    let put_u64 = |b: &mut Vec<u8>, x: u64| b.extend_from_slice(&x.to_le_bytes());
    let put_str = |b: &mut Vec<u8>, s: &str| {
        b.extend_from_slice(&(s.len() as u64).to_le_bytes());
        b.extend_from_slice(s.as_bytes());
    };
    put_u64(&mut raw, data.prims.len() as u64);
    for (path, p) in &data.prims {
        put_str(&mut raw, path);
        put_u64(&mut raw, p.topology);
        put_u64(&mut raw, p.rest.len() as u64);
        for q in p.rest.iter() {
            for x in q {
                raw.extend_from_slice(&x.to_le_bytes());
            }
        }
        put_u64(&mut raw, p.values.len() as u64);
        for (name, v) in &p.values {
            put_str(&mut raw, name);
            put_u64(&mut raw, v.len() as u64);
            for x in v.iter() {
                raw.extend_from_slice(&x.to_le_bytes());
            }
        }
    }
    let mut out = MAGIC.to_vec();
    let mut z = flate2::write::DeflateEncoder::new(&mut out, flate2::Compression::fast());
    let _ = z.write_all(&raw);
    let _ = z.finish();
    out
}

/// The prims of a `.mcol` file's bytes.
pub fn from_bytes(bytes: &[u8]) -> Result<BTreeMap<String, PrimPaint>, String> {
    use std::io::Read;
    if bytes.len() < 8 || &bytes[..8] != MAGIC {
        return Err("not a paint file of Imago's".into());
    }
    let mut raw = vec![];
    flate2::read::DeflateDecoder::new(&bytes[8..]).read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let mut at = 0usize;
    let bad = || "a paint file cut short".to_string();
    let u64_ = |at: &mut usize| -> Result<u64, String> {
        let b = raw.get(*at..*at + 8).ok_or_else(bad)?;
        *at += 8;
        Ok(u64::from_le_bytes(b.try_into().unwrap()))
    };
    let mut prims = BTreeMap::new();
    let count = u64_(&mut at)?;
    for _ in 0..count {
        let n = u64_(&mut at)? as usize;
        let path = String::from_utf8(raw.get(at..at + n).ok_or_else(bad)?.to_vec()).map_err(|e| e.to_string())?;
        at += n;
        let topology = u64_(&mut at)?;
        let points = u64_(&mut at)? as usize;
        let f = |at: &mut usize| -> Result<f32, String> {
            let b = raw.get(*at..*at + 4).ok_or_else(bad)?;
            *at += 4;
            Ok(f32::from_le_bytes(b.try_into().unwrap()))
        };
        let mut rest = Vec::with_capacity(points);
        for _ in 0..points {
            rest.push([f(&mut at)?, f(&mut at)?, f(&mut at)?]);
        }
        let channels = u64_(&mut at)?;
        let mut values = BTreeMap::new();
        for _ in 0..channels {
            let n = u64_(&mut at)? as usize;
            let name = String::from_utf8(raw.get(at..at + n).ok_or_else(bad)?.to_vec()).map_err(|e| e.to_string())?;
            at += n;
            let len = u64_(&mut at)? as usize;
            let mut v = Vec::with_capacity(len);
            for _ in 0..len {
                v.push(f(&mut at)?);
            }
            values.insert(name, Arc::new(v));
        }
        prims.insert(
            path,
            PrimPaint {
                rest: Arc::new(rest),
                topology,
                values,
            },
        );
    }
    Ok(prims)
}

/// Its file written beside the graph (`dir`), if it is not there already:
/// to a temporary file, then renamed, so it is never seen half written.
pub fn write_file(data: &PaintData, dir: &std::path::Path) -> Result<(), String> {
    let Some(name) = data.file_name() else {
        return Ok(());
    };
    let path = dir.join(&name);
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("mcol.tmp");
    std::fs::write(&tmp, to_bytes(data)).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Its values read from beside the graph (`dir`), by its stem and hash.
pub fn read_file(data: &mut PaintData, dir: &std::path::Path) {
    if data.hash == 0 {
        return;
    }
    let name = format!("paint/{}.{:016x}.mcol", data.stem, data.hash);
    match std::fs::read(dir.join(&name)).map_err(|e| e.to_string()).and_then(|b| from_bytes(&b)) {
        Ok(prims) => {
            data.prims = prims;
            data.missing = false;
        }
        Err(_) => data.missing = true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: usize) -> Geo {
        let mut pts = vec![];
        for j in 0..n {
            for i in 0..n {
                pts.push([i as f32, 0.0, j as f32]);
            }
        }
        let (mut counts, mut idx) = (vec![], vec![]);
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let a = (j * n + i) as u32;
                counts.push(4);
                idx.extend([a, a + n as u32, a + n as u32 + 1, a + 1]);
            }
        }
        Geo::from_polygons(pts, counts, idx)
    }

    /// A dab adds most at its centre, nothing past its radius; Set and Erase
    /// go towards their values; an ID is painted whole; the slope limit keeps
    /// paint off a steep surface.
    #[test]
    fn a_dab_paints_where_it_should() {
        let g = grid(11);
        let s = Surface::new(&g, Mat4::IDENTITY);
        let f = Channel::float("density");
        let mut v = vec![0.0; g.point_count()];
        let b = Brush {
            radius: 2.0,
            strength: 1.0,
            ..Default::default()
        };
        dab(&mut v, &s, Vec3::new(5.0, 0.0, 5.0), &b, &f);
        let at = |v: &[f32], x: usize, z: usize| v[z * 11 + x];
        assert!((at(&v, 5, 5) - 1.0).abs() < 1e-6);
        assert!(at(&v, 6, 5) > 0.0 && at(&v, 6, 5) < 1.0);
        assert_eq!(at(&v, 8, 5), 0.0, "past the radius");
        dab(&mut v, &s, Vec3::new(5.0, 0.0, 5.0), &Brush { op: Op::Erase, ..b }, &f);
        assert_eq!(at(&v, 5, 5), 0.0);

        let id = Channel::id("biome", &["grass", "fern"]);
        let mut ids = vec![0.0; g.point_count()];
        dab(&mut ids, &s, Vec3::new(5.0, 0.0, 5.0), &Brush { op: Op::Set, value: 2.0, ..b }, &id);
        assert_eq!(at(&ids, 5, 5), 2.0);
        assert!(ids.iter().all(|x| *x == 0.0 || *x == 2.0), "never between");

        // A flat grid is no steeper than 10°; turned on its side it is.
        let side = Surface::new(&g, Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2));
        let mut w = vec![0.0; g.point_count()];
        let hit = side.to_world.transform_point3(Vec3::new(5.0, 0.0, 5.0));
        dab(&mut w, &side, hit, &Brush { max_slope: 10.0, ..b }, &f);
        assert!(w.iter().all(|x| *x == 0.0));
        let mut w = vec![0.0; g.point_count()];
        dab(&mut w, &s, Vec3::new(5.0, 0.0, 5.0), &Brush { max_slope: 10.0, ..b }, &f);
        assert!(at(&w, 5, 5) > 0.9);
    }

    /// Mirrored across the prim's own X, the other side is painted the same.
    #[test]
    fn a_mirrored_dab_paints_both_sides() {
        let mut g = grid(11);
        // Centred on X = 0.
        for p in g.points_mut() {
            p[0] -= 5.0;
        }
        let s = Surface::new(&g, Mat4::IDENTITY);
        let f = Channel::float("w");
        let mut v = vec![0.0; g.point_count()];
        let b = Brush {
            radius: 1.5,
            strength: 1.0,
            mirror: true,
            ..Default::default()
        };
        dab(&mut v, &s, Vec3::new(3.0, 0.0, 5.0), &b, &f);
        assert!(v[5 * 11 + 8] > 0.9 && (v[5 * 11 + 2] - v[5 * 11 + 8]).abs() < 1e-5);
    }

    /// Saved and read back the same; the hash changes with the values; a
    /// remodelled prim gets the values of its nearest points at rest.
    #[test]
    fn its_file_and_its_points() {
        let g = grid(5);
        let f = Channel::float("density");
        let mut d = PaintData::new("ground");
        d.values_mut("/ground", &g, &f)[12] = 0.75;
        d.rehash();
        let h = d.hash;
        let back = from_bytes(&to_bytes(&d)).unwrap();
        assert_eq!(back, d.prims);
        d.values_mut("/ground", &g, &f)[0] = 1.0;
        d.rehash();
        assert_ne!(d.hash, h);

        // The same grid with its points in another order: carried by place.
        let mut pts = g.points().to_vec();
        pts.reverse();
        let moved = Geo::from_points(pts);
        let painted = d.values_mut("/ground", &moved, &f).clone();
        assert_eq!(painted[24 - 12], 0.75);
        assert_eq!(painted[24], 1.0);
    }

    /// The scene: painted prims get the channels as point attributes (an ID as
    /// integers); others pass through.
    #[test]
    fn the_channels_go_on_the_points() {
        let g = grid(3);
        let mesh = g.to_mesh();
        let prims = vec![
            Prim {
                geo: Some(Arc::new(g.clone())),
                ..Prim::new("/ground".into(), mesh.clone())
            },
            Prim::new("/other".into(), mesh),
        ];
        let chans = vec![Channel::float("density"), Channel::id("biome", &["a"])];
        let mut d = PaintData::new("p");
        d.values_mut("/ground", &g, &chans[0])[4] = 0.5;
        d.values_mut("/ground", &g, &chans[1])[4] = 1.0;
        d.rehash();
        let EvalResult::Named(out) = apply(&EvalResult::Named(prims), &chans, &d) else {
            panic!()
        };
        let geo = out[0].geo.as_ref().unwrap();
        assert_eq!(geo.attr(Context::Point, "density").unwrap().column.floats().unwrap()[4], 0.5);
        assert_eq!(geo.attr(Context::Point, "biome").unwrap().column.ints().unwrap()[4], 1);
        assert!(out[1].geo.is_none(), "not painted: untouched");
    }
}
