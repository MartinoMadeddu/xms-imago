//! Surfaces a graph queries: the meshes and curves on the operator's reference
//! inputs (Closest Point on Surface), moved into the space the graph runs in,
//! with a BVH over their triangles and curve segments; and the graph's own
//! geometry, for reading at its own samples.
//!
//! A curve is read as a face whose corners are its points, in order: a sample
//! on it is a segment's two points and their weights (`corners` `[a, b, b]`,
//! weights `[1 - t, t, 0]`), `face` the curve. Its segments are its control
//! points joined by straight lines, as a linear curve is drawn.

use std::sync::{Arc, OnceLock};

use glam::{Mat3, Mat4, Vec3};
use rayon::prelude::*;

use xms_geo::{Geo, Topology};

use super::spatial::{closest_on_triangle, Aabb, Bvh};
use super::types::Loc;

/// What one of the operator's reference inputs holds: the prims wired there by
/// path (groups too, for Get Reference), and the meshes among them.
#[derive(Clone, Debug, Default)]
pub struct Reference {
    pub paths: Vec<String>,
    pub meshes: Vec<RefMesh>,
}

/// A mesh on a reference input: its path, where it is in the world (one entry
/// per copy of an instanced one), and its geometry.
#[derive(Clone, Debug)]
pub struct RefMesh {
    pub path: String,
    pub world: Mat4,
    pub geo: RefGeo,
}

/// A reference mesh's geometry: given, or made the first time a graph needs it
/// (a mesh with no `Geo` yet is not converted for graphs that never query it).
/// Shared by its clones.
#[derive(Clone)]
pub struct RefGeo {
    cell: Arc<OnceLock<Arc<Geo>>>,
    make: Option<Arc<dyn Fn() -> Geo + Send + Sync>>,
}

impl RefGeo {
    pub fn ready(geo: Arc<Geo>) -> RefGeo {
        let cell = OnceLock::new();
        let _ = cell.set(geo);
        RefGeo {
            cell: Arc::new(cell),
            make: None,
        }
    }

    /// Made by `make` when first asked for.
    pub fn later(make: impl Fn() -> Geo + Send + Sync + 'static) -> RefGeo {
        RefGeo {
            cell: Arc::new(OnceLock::new()),
            make: Some(Arc::new(make)),
        }
    }

    pub fn get(&self) -> &Arc<Geo> {
        self.cell
            .get_or_init(|| Arc::new(self.make.as_ref().map(|m| m()).unwrap_or_default()))
    }
}

impl std::fmt::Debug for RefGeo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.cell.get().is_some() {
            "RefGeo(made)"
        } else {
            "RefGeo(later)"
        })
    }
}

/// A mesh or curves of a surface: its geometry, where each face's (curve's)
/// corners start, and the matrix into the graph's space.
pub(crate) struct Mesh {
    pub geo: Arc<Geo>,
    pub face_off: Vec<u32>,
    /// Curves: a corner is a point (`corner_points` is every point in order).
    curve_points: Vec<u32>,
    pub to_graph: Mat4,
    /// For normals: the inverse transpose of `to_graph`'s 3×3.
    pub normals: Mat3,
    /// Not the identity: values with a role are moved.
    pub moved: bool,
}

impl Mesh {
    fn new(geo: Arc<Geo>, to_graph: Mat4) -> Mesh {
        let mut face_off = vec![0u32];
        let mut curve_points = vec![];
        if let Topology::Mesh { counts, .. } | Topology::Curves { counts, .. } = &geo.topology {
            let mut total = 0u32;
            for &c in counts.iter() {
                total += c;
                face_off.push(total);
            }
            if matches!(geo.topology, Topology::Curves { .. }) {
                curve_points = (0..total).collect();
            }
        }
        let normals = Mat3::from_mat4(to_graph).inverse().transpose();
        Mesh {
            geo,
            face_off,
            curve_points,
            to_graph,
            normals,
            moved: to_graph != Mat4::IDENTITY,
        }
    }

    /// The point of each corner.
    pub fn corner_points(&self) -> &[u32] {
        match &self.geo.topology {
            Topology::Mesh { indices, .. } => indices.as_slice(),
            Topology::Curves { .. } => &self.curve_points,
            _ => &[],
        }
    }

    /// The corners of face `f`.
    pub fn corners(&self, f: u32) -> std::ops::Range<u32> {
        let f = f as usize;
        match (self.face_off.get(f), self.face_off.get(f + 1)) {
            (Some(&a), Some(&b)) => a..b,
            _ => 0..0,
        }
    }
}

/// A triangle of a surface, in the graph's space: the mesh, the face and the
/// corners it is from. A curve's segment is one too, its last corner its
/// second again.
struct Tri {
    mesh: u32,
    face: u32,
    corners: [u32; 3],
    p: [Vec3; 3],
}

/// Meshes to read (at samples, or element by element) and to search.
pub(crate) struct Surface {
    pub meshes: Vec<Mesh>,
    /// Its triangles and a BVH over them: made the first time it is searched,
    /// not for reading alone.
    index: OnceLock<(Vec<Tri>, Option<Bvh>)>,
}

impl Surface {
    /// The graph's own geometry, as its own samples (Scatter's) read it.
    pub fn own(geo: &Geo) -> Surface {
        Surface {
            meshes: vec![Mesh::new(Arc::new(geo.clone()), Mat4::IDENTITY)],
            index: OnceLock::new(),
        }
    }

    /// The reference prims `paths` name (a group names those below it), in the
    /// space of the graph (`space`: where its primitive is in the world).
    /// Polygons and curves are searched; points only read.
    pub fn of(paths: &[Arc<str>], references: &[Reference], space: Mat4) -> Surface {
        let back = space.inverse();
        let named = |path: &str| {
            paths.iter().any(|p| {
                let p: &str = p;
                path == p || (path.starts_with(p) && path[p.len()..].starts_with('/')) || p == "/"
            })
        };
        let meshes: Vec<Mesh> = references
            .iter()
            .flat_map(|r| &r.meshes)
            .filter(|m| named(&m.path))
            .map(|m| Mesh::new(m.geo.get().clone(), back * m.world))
            .collect();
        Surface {
            meshes,
            index: OnceLock::new(),
        }
    }

    /// Its triangles (and curve segments) and their BVH, made once.
    fn index(&self) -> &(Vec<Tri>, Option<Bvh>) {
        self.index.get_or_init(|| {
            let meshes = &self.meshes;
            // Every polygon as a fan of triangles from its first corner, as
            // Scatter on Surface cuts them.
            let tris: Vec<Tri> = meshes
                .par_iter()
                .enumerate()
                .flat_map_iter(|(k, m)| {
                    let pts = m.geo.points();
                    let cp = m.corner_points();
                    let at = |c: u32| {
                        m.to_graph.transform_point3(Vec3::from_array(
                            cp.get(c as usize)
                                .and_then(|&p| pts.get(p as usize))
                                .copied()
                                .unwrap_or_default(),
                        ))
                    };
                    let faces = m.face_off.len().saturating_sub(1);
                    // A curve's segments: corners c and c + 1, the second twice.
                    let curve = matches!(m.geo.topology, Topology::Curves { .. });
                    (0..faces)
                        .flat_map(move |f| {
                            let (first, end) = (m.face_off[f], m.face_off[f + 1]);
                            let from = if curve { first } else { first + 1 };
                            (from..end.saturating_sub(1)).map(move |c| {
                                let corners = if curve { [c, c + 1, c + 1] } else { [first, c, c + 1] };
                                Tri {
                                    mesh: k as u32,
                                    face: f as u32,
                                    corners,
                                    p: corners.map(at),
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            let boxes: Vec<Aabb> = tris.iter().map(|t| Aabb::around(&t.p)).collect();
            let bvh = Bvh::build(&boxes);
            (tris, bvh)
        })
    }

    /// The sample nearest `p` within `radius` (0 or less: anywhere), and how
    /// far it is. Found nothing: a sample on no mesh, and the radius.
    pub fn closest(&self, p: Vec3, radius: f32) -> (Loc, f32) {
        let limit = if radius > 0.0 { radius * radius } else { f32::INFINITY };
        let mut best = limit;
        let mut hit: Option<(u32, [f32; 3])> = None;
        let (tris, bvh) = self.index();
        if let Some(bvh) = bvh {
            bvh.nearest(p, &mut best, |i, best| {
                let t = &tris[i as usize];
                let w = if t.corners[1] == t.corners[2] {
                    closest_on_segment(p, t.p[0], t.p[1])
                } else {
                    closest_on_triangle(p, t.p[0], t.p[1], t.p[2])
                };
                let q = t.p[0] * w[0] + t.p[1] * w[1] + t.p[2] * w[2];
                let d = (p - q).length_squared();
                if d <= *best {
                    *best = d;
                    hit = Some((i, w));
                }
            });
        }
        match hit {
            Some((i, weights)) => {
                let t = &tris[i as usize];
                (
                    Loc {
                        face: t.face,
                        corners: t.corners,
                        weights,
                        mesh: t.mesh,
                    },
                    best.sqrt(),
                )
            }
            None => (
                Loc {
                    mesh: Loc::NONE,
                    ..Loc::default()
                },
                radius.max(0.0),
            ),
        }
    }
}

/// The weights of the place nearest `p` on the segment from `a` to `b`, as a
/// triangle's: `[1 - t, t, 0]`.
fn closest_on_segment(p: Vec3, a: Vec3, b: Vec3) -> [f32; 3] {
    let d = b - a;
    let len2 = d.length_squared();
    let t = if len2 > 0.0 {
        ((p - a).dot(d) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    [1.0 - t, t, 0.0]
}
