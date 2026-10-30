//! A mesh as the rig's skinning steps see it: positions in world space, the
//! polygons, and which point touches which. One per primitive.

use std::collections::BinaryHeap;

use glam::Vec3;

use crate::core::geo::{Geo, Topology};
use crate::core::geo_mesh::GeoMesh;
use crate::types::{Placement, Prim};

/// A primitive's points in world space and their neighbours.
#[derive(Clone, Debug, Default)]
pub struct Surface {
    pub points: Vec<Vec3>,
    /// Each polygon's points, in order.
    pub polys: Vec<Vec<u32>>,
    /// Each point's neighbours along polygon edges, each once.
    pub near: Vec<Vec<u32>>,
}

impl Surface {
    /// The surface of a primitive, placed in the world. None for a primitive
    /// with no polygons, or one drawn many times (an instancer's).
    pub fn of(p: &Prim) -> Option<Surface> {
        let place = match &p.place {
            Placement::InPlace => glam::Mat4::IDENTITY,
            Placement::One(m) => *m,
            Placement::Many(_) => return None,
        };
        let geo = p
            .geo
            .clone()
            .unwrap_or_else(|| std::sync::Arc::new(Geo::from_mesh(&p.mesh)));
        let points: Vec<Vec3> = geo
            .points()
            .iter()
            .map(|q| place.transform_point3(Vec3::from_array(*q)))
            .collect();
        let Topology::Mesh { counts, indices, .. } = &geo.topology else {
            return None;
        };
        let mut polys = Vec::with_capacity(counts.len());
        let mut at = 0usize;
        for &n in counts.iter() {
            let n = n as usize;
            polys.push(indices[at..at + n].to_vec());
            at += n;
        }
        if polys.is_empty() {
            return None;
        }
        Some(Surface::new(points, polys))
    }

    pub fn new(points: Vec<Vec3>, polys: Vec<Vec<u32>>) -> Surface {
        let mut near: Vec<Vec<u32>> = vec![vec![]; points.len()];
        for poly in &polys {
            for k in 0..poly.len() {
                let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                if a == b || a as usize >= points.len() || b as usize >= points.len() {
                    continue;
                }
                if !near[a as usize].contains(&b) {
                    near[a as usize].push(b);
                }
                if !near[b as usize].contains(&a) {
                    near[b as usize].push(a);
                }
            }
        }
        Surface { points, polys, near }
    }

    /// Normals at the points of `pos` (this surface's polygons, other
    /// positions): area-weighted face normals averaged.
    pub fn normals(&self, pos: &[Vec3]) -> Vec<Vec3> {
        let mut n = vec![Vec3::ZERO; pos.len()];
        for poly in &self.polys {
            if poly.len() < 3 {
                continue;
            }
            let p0 = pos[poly[0] as usize];
            for k in 1..poly.len() - 1 {
                let f = (pos[poly[k] as usize] - p0).cross(pos[poly[k + 1] as usize] - p0);
                for &i in poly {
                    n[i as usize] += f;
                }
            }
        }
        n.iter().map(|v| v.normalize_or(Vec3::Y)).collect()
    }

    /// Geodesic flood: from `seeds` (point, label, starting distance), each
    /// point takes the label of the seed nearest to it along the surface.
    /// Points no seed reaches (separate pieces) keep `None`.
    pub fn flood(&self, seeds: &[(u32, u32, f32)]) -> (Vec<Option<u32>>, Vec<f32>) {
        #[derive(PartialEq)]
        struct Item(f32, u32, u32);
        impl Eq for Item {}
        impl PartialOrd for Item {
            fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Item {
            fn cmp(&self, o: &Self) -> std::cmp::Ordering {
                o.0.total_cmp(&self.0)
            }
        }
        let n = self.points.len();
        let mut dist = vec![f32::MAX; n];
        let mut label: Vec<Option<u32>> = vec![None; n];
        let mut heap = BinaryHeap::new();
        for &(p, l, d) in seeds {
            if (p as usize) < n && d < dist[p as usize] {
                dist[p as usize] = d;
                label[p as usize] = Some(l);
                heap.push(Item(d, p, l));
            }
        }
        while let Some(Item(d, p, l)) = heap.pop() {
            if d > dist[p as usize] || label[p as usize] != Some(l) {
                continue;
            }
            for &q in &self.near[p as usize] {
                let nd = d + self.points[p as usize].distance(self.points[q as usize]);
                if nd < dist[q as usize] {
                    dist[q as usize] = nd;
                    label[q as usize] = Some(l);
                    heap.push(Item(nd, q, l));
                }
            }
        }
        (label, dist)
    }

    /// Positions smoothed `iterations` times: each point moved `step` of the
    /// way to the mean of its neighbours (Laplacian smoothing).
    pub fn smooth(&self, pos: &[Vec3], iterations: u32, step: f32) -> Vec<Vec3> {
        let mut a = pos.to_vec();
        let mut b = a.clone();
        for _ in 0..iterations {
            for (i, near) in self.near.iter().enumerate() {
                if near.is_empty() {
                    b[i] = a[i];
                    continue;
                }
                let mean = near.iter().map(|q| a[*q as usize]).sum::<Vec3>() / near.len() as f32;
                b[i] = a[i] + (mean - a[i]) * step;
            }
            std::mem::swap(&mut a, &mut b);
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A strip of quads: neighbours, flood by path length, smoothing.
    #[test]
    fn a_strip_floods_and_smooths() {
        // 0-1-2-3 over 4-5-6-7.
        let points: Vec<Vec3> = (0..8).map(|i| Vec3::new((i % 4) as f32, (i / 4) as f32, 0.0)).collect();
        let polys = vec![vec![0, 1, 5, 4], vec![1, 2, 6, 5], vec![2, 3, 7, 6]];
        let s = Surface::new(points.clone(), polys);
        assert_eq!(s.near[1].len(), 3);
        let (label, dist) = s.flood(&[(0, 7, 0.0), (3, 9, 0.0)]);
        assert_eq!(label[1], Some(7));
        assert_eq!(label[2], Some(9));
        assert!((dist[5] - 2.0).abs() < 1e-5);
        let sm = s.smooth(&points, 5, 0.5);
        assert!(sm[0].x > 0.0 && sm[3].x < 3.0);
        let n = s.normals(&points);
        assert!(n.iter().all(|v| v.z.abs() > 0.99));
    }
}
