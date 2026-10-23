//! Spatial queries: a BVH over boxes, and the closest point on triangles.
//!
//! The builder is rray's (`rray-render`'s `accel::bvh`, Martino's renderer):
//! binned SAH, 16 bins, up to four primitives a leaf, subtrees of more than
//! 4096 built in parallel with `rayon::join`, then flattened into 32-byte
//! nodes with children side by side. The ray traversal is replaced by a
//! point query: nodes nearest first, any node farther than the best found
//! so far (or the search radius) skipped.

use std::sync::atomic::{AtomicU32, Ordering};

use glam::Vec3;

/// Primitives per leaf, at most.
const MAX_LEAF: u32 = 4;
const BINS: usize = 16;
/// Subtrees up to this size are built one after the other, straight into
/// flat nodes.
const SMALL: usize = 4096;

/// An axis-aligned box.
#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb { min: [f32::INFINITY; 3], max: [f32::NEG_INFINITY; 3] };

    /// The box around points.
    pub fn around(points: &[Vec3]) -> Aabb {
        let mut b = Aabb::EMPTY;
        for p in points { b.grow_point(&p.to_array()); }
        b
    }

    fn grow(&mut self, b: &Aabb) {
        for a in 0..3 {
            self.min[a] = self.min[a].min(b.min[a]);
            self.max[a] = self.max[a].max(b.max[a]);
        }
    }

    fn grow_point(&mut self, p: &[f32; 3]) {
        for a in 0..3 {
            self.min[a] = self.min[a].min(p[a]);
            self.max[a] = self.max[a].max(p[a]);
        }
    }

    fn area(&self) -> f32 {
        let d = [self.max[0] - self.min[0], self.max[1] - self.min[1], self.max[2] - self.min[2]];
        if d[0] < 0.0 { return 0.0; }
        2.0 * (d[0] * d[1] + d[1] * d[2] + d[2] * d[0])
    }
}

/// A node: children at `first` and `first + 1` (`count` 0), or a leaf of
/// primitives `first..first + count` in `Bvh::order`.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct Node {
    min: [f32; 3],
    first: u32,
    max: [f32; 3],
    count: u32,
}

impl Node {
    const EMPTY: Node = Node { min: [0.0; 3], first: 0, max: [0.0; 3], count: 0 };

    /// The squared distance from `p` to the node's box (0 inside).
    #[inline]
    fn distance2(&self, p: Vec3) -> f32 {
        let lo = Vec3::from_array(self.min);
        let hi = Vec3::from_array(self.max);
        let d = (lo - p).max(p - hi).max(Vec3::ZERO);
        d.length_squared()
    }
}

/// A bounding volume hierarchy over boxes (triangles, points).
pub struct Bvh {
    nodes: Vec<Node>,
    /// The primitive at each leaf slot: leaves index this.
    order: Vec<u32>,
}

/// One primitive during the build, moved as nodes are split.
#[derive(Clone, Copy)]
#[repr(C)]
struct BuildPrim {
    min: [f32; 3],
    idx: u32,
    max: [f32; 3],
    _pad: u32,
}

impl BuildPrim {
    fn centroid(&self, a: usize) -> f32 { 0.5 * (self.min[a] + self.max[a]) }
    fn aabb(&self) -> Aabb { Aabb { min: self.min, max: self.max } }
}

/// Above the small-subtree size: built in parallel, then flattened.
enum BuildNode {
    Flat { nodes: Vec<Node> },
    Inner { bounds: Aabb, children: Box<[BuildNode; 2]> },
}

#[derive(Clone, Copy)]
struct Bin { b: Aabb, n: u32 }

impl Bin { const EMPTY: Bin = Bin { b: Aabb::EMPTY, n: 0 }; }

type Bins = [[Bin; BINS]; 3];

/// Binning per axis, over the centroids' bounds.
#[derive(Clone, Copy)]
struct Range { lo: [f32; 3], scale: [f32; 3] }

impl Range {
    fn from_bounds(cb: &Aabb) -> Range {
        Range {
            lo: cb.min,
            scale: std::array::from_fn(|a| {
                let ext = cb.max[a] - cb.min[a];
                if ext > 0.0 { BINS as f32 / ext } else { 0.0 }
            }),
        }
    }
    fn bin(&self, c: f32, a: usize) -> usize { (((c - self.lo[a]) * self.scale[a]) as usize).min(BINS - 1) }
    fn edge(&self, a: usize, k: usize) -> f32 { self.lo[a] + (k + 1) as f32 / self.scale[a] }
}

/// The records' bounds and their centroids' bounds.
fn slice_bounds(recs: &[BuildPrim]) -> (Aabb, Aabb) {
    let (mut b, mut c) = (Aabb::EMPTY, Aabb::EMPTY);
    for r in recs {
        b.grow(&r.aabb());
        c.grow_point(&[r.centroid(0), r.centroid(1), r.centroid(2)]);
    }
    (b, c)
}

fn fill_bins(recs: &[BuildPrim], range: &Range) -> Bins {
    let mut bins = [[Bin::EMPTY; BINS]; 3];
    for r in recs {
        let b = r.aabb();
        for a in 0..3 {
            let bin = &mut bins[a][range.bin(r.centroid(a), a)];
            bin.b.grow(&b);
            bin.n += 1;
        }
    }
    bins
}

/// The cheapest split by the surface area heuristic: cost, axis, last bin
/// of the left side.
fn best_split(bins: &Bins, range: &Range) -> Option<(f32, usize, usize)> {
    let mut best: Option<(f32, usize, usize)> = None;
    for a in 0..3 {
        if range.scale[a] <= 0.0 { continue; }
        let mut right_area = [0.0f32; BINS];
        let mut right_count = [0u32; BINS];
        let (mut acc, mut cnt) = (Aabb::EMPTY, 0u32);
        for k in (1..BINS).rev() {
            acc.grow(&bins[a][k].b);
            cnt += bins[a][k].n;
            right_area[k] = acc.area();
            right_count[k] = cnt;
        }
        let (mut acc, mut cnt) = (Aabb::EMPTY, 0u32);
        for k in 0..BINS - 1 {
            acc.grow(&bins[a][k].b);
            cnt += bins[a][k].n;
            if cnt == 0 || right_count[k + 1] == 0 { continue; }
            let cost = acc.area() * cnt as f32 + right_area[k + 1] * right_count[k + 1] as f32;
            if best.map_or(true, |b| cost < b.0) { best = Some((cost, a, k)); }
        }
    }
    best
}

/// Records for which `left` holds first; how many there are.
fn partition(recs: &mut [BuildPrim], left: impl Fn(&BuildPrim) -> bool) -> usize {
    let (mut i, mut j) = (0usize, recs.len());
    loop {
        while i < j && left(&recs[i]) { i += 1; }
        while i < j && !left(&recs[j - 1]) { j -= 1; }
        if i >= j { return i; }
        recs.swap(i, j - 1);
        i += 1;
        j -= 1;
    }
}

struct Split { mid: usize, left: (Aabb, Aabb), right: (Aabb, Aabb) }

/// How to split a node; `None`, a leaf.
fn split(recs: &mut [BuildPrim], bounds: &Aabb, cb: &Aabb) -> Option<Split> {
    let n = recs.len();
    if n <= MAX_LEAF as usize { return None; }
    let range = Range::from_bounds(cb);
    let bins = fill_bins(recs, &range);
    let best = best_split(&bins, &range);
    if n <= 2 * MAX_LEAF as usize && best.map_or(true, |b| b.0 >= bounds.area() * n as f32) { return None; }
    let halves = |recs: &mut [BuildPrim]| {
        let mid = n / 2;
        let (l, r) = recs.split_at(mid);
        Some(Split { mid, left: slice_bounds(l), right: slice_bounds(r) })
    };
    let Some((_, a, k)) = best else { return halves(recs) };
    let mid = partition(recs, |r| range.bin(r.centroid(a), a) <= k);
    if mid == 0 || mid == n { return halves(recs); }
    let (mut lb, mut rb) = (Aabb::EMPTY, Aabb::EMPTY);
    for bin in &bins[a][..=k] { lb.grow(&bin.b); }
    for bin in &bins[a][k + 1..] { rb.grow(&bin.b); }
    let plane = range.edge(a, k);
    let (mut lc, mut rc) = (*cb, *cb);
    lc.max[a] = plane.min(cb.max[a]);
    rc.min[a] = plane.max(cb.min[a]);
    Some(Split { mid, left: (lb, lc), right: (rb, rc) })
}

fn build_small(recs: &mut [BuildPrim], start: u32, bounds: Aabb, cb: Aabb, nodes: &mut Vec<Node>, at: usize) -> u32 {
    match split(recs, &bounds, &cb) {
        None => {
            nodes[at] = Node { min: bounds.min, first: start, max: bounds.max, count: recs.len() as u32 };
            0
        }
        Some(sp) => {
            let li = nodes.len();
            nodes.push(Node::EMPTY);
            nodes.push(Node::EMPTY);
            nodes[at] = Node { min: bounds.min, first: li as u32, max: bounds.max, count: 0 };
            let (l, r) = recs.split_at_mut(sp.mid);
            let dl = build_small(l, start, sp.left.0, sp.left.1, nodes, li);
            let dr = build_small(r, start + sp.mid as u32, sp.right.0, sp.right.1, nodes, li + 1);
            1 + dl.max(dr)
        }
    }
}

fn build_rec(recs: &mut [BuildPrim], start: u32, bounds: Aabb, cb: Aabb, depth: u32, max_depth: &AtomicU32) -> BuildNode {
    let n = recs.len();
    if n <= SMALL {
        let mut nodes = Vec::with_capacity(n.div_ceil(2) + 1);
        nodes.push(Node::EMPTY);
        let d = build_small(recs, start, bounds, cb, &mut nodes, 0);
        max_depth.fetch_max(depth + d, Ordering::Relaxed);
        return BuildNode::Flat { nodes };
    }
    let sp = split(recs, &bounds, &cb).unwrap_or_else(|| {
        let mid = n / 2;
        let (l, r) = recs.split_at(mid);
        Split { mid, left: slice_bounds(l), right: slice_bounds(r) }
    });
    let (l, r) = recs.split_at_mut(sp.mid);
    let (left, right) = rayon::join(
        || build_rec(l, start, sp.left.0, sp.left.1, depth + 1, max_depth),
        || build_rec(r, start + sp.mid as u32, sp.right.0, sp.right.1, depth + 1, max_depth),
    );
    BuildNode::Inner { bounds, children: Box::new([left, right]) }
}

/// Traversal stack depth: SAH trees stay far below it.
const STACK: usize = 64;

impl Bvh {
    /// Built over boxes, primitive `i` being `boxes[i]`. `None` for none.
    pub fn build(boxes: &[Aabb]) -> Option<Bvh> {
        if boxes.is_empty() { return None; }
        let mut recs: Vec<BuildPrim> = boxes.iter().enumerate()
            .map(|(i, b)| BuildPrim { min: b.min, idx: i as u32, max: b.max, _pad: 0 })
            .collect();
        let (bounds, cb) = slice_bounds(&recs);
        let max_depth = AtomicU32::new(0);
        let tree = build_rec(&mut recs, 0, bounds, cb, 0, &max_depth);
        let order: Vec<u32> = recs.iter().map(|r| r.idx).collect();
        // Flattened: children side by side; small subtrees spliced in.
        let mut nodes: Vec<Node> = Vec::with_capacity(boxes.len() / 2 + 2);
        nodes.push(Node::EMPTY);
        let mut stack: Vec<(BuildNode, usize)> = vec![(tree, 0)];
        while let Some((bn, at)) = stack.pop() {
            match bn {
                BuildNode::Flat { nodes: sub } => {
                    let base = nodes.len();
                    let remap = |nd: &Node| if nd.count == 0 { Node { first: (base + nd.first as usize - 1) as u32, ..*nd } } else { *nd };
                    nodes[at] = remap(&sub[0]);
                    nodes.extend(sub[1..].iter().map(remap));
                }
                BuildNode::Inner { bounds, children } => {
                    let li = nodes.len();
                    nodes.push(Node::EMPTY);
                    nodes.push(Node::EMPTY);
                    nodes[at] = Node { min: bounds.min, first: li as u32, max: bounds.max, count: 0 };
                    let [c0, c1] = *children;
                    stack.push((c1, li + 1));
                    stack.push((c0, li));
                }
            }
        }
        Some(Bvh { nodes, order })
    }

    /// The primitives near `p`, nearest boxes first: `test(primitive,
    /// best)` measures one and lowers `best` (a squared distance) when it
    /// is nearer. Boxes farther than `best` are skipped, so start `best` at
    /// the squared search radius (or infinity).
    pub fn nearest(&self, p: Vec3, best: &mut f32, mut test: impl FnMut(u32, &mut f32)) {
        let Some(root) = self.nodes.first() else { return };
        if root.distance2(p) > *best { return; }
        let mut stack = [(0u32, 0.0f32); STACK];
        let mut sp = 0usize;
        let mut idx = 0u32;
        loop {
            let n = &self.nodes[idx as usize];
            if n.count > 0 {
                for k in n.first..n.first + n.count { test(self.order[k as usize], best); }
            } else {
                let (l, r) = (n.first, n.first + 1);
                let (dl, dr) = (self.nodes[l as usize].distance2(p), self.nodes[r as usize].distance2(p));
                let (near, far, dnear, dfar) = if dl <= dr { (l, r, dl, dr) } else { (r, l, dr, dl) };
                if dnear <= *best {
                    if dfar <= *best && sp < STACK { stack[sp] = (far, dfar); sp += 1; }
                    idx = near;
                    continue;
                }
            }
            // The next node still nearer than the best found.
            loop {
                if sp == 0 { return; }
                sp -= 1;
                let (i, d) = stack[sp];
                if d <= *best { idx = i; break; }
            }
        }
    }
}

/// The point of triangle `a b c` nearest `p`, as weights on its corners
/// (they sum to 1). Ericson, Real-Time Collision Detection, 5.1.5.
pub fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> [f32; 3] {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 { return [1.0, 0.0, 0.0]; }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 { return [0.0, 1.0, 0.0]; }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return [1.0 - v, v, 0.0];
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 { return [0.0, 0.0, 1.0]; }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return [1.0 - w, 0.0, w];
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return [0.0, 1.0 - w, w];
    }
    let denom = 1.0 / (va + vb + vc);
    if !denom.is_finite() { return [1.0, 0.0, 0.0]; }
    let (v, w) = (vb * denom, vc * denom);
    [1.0 - v - w, v, w]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The nearest of many triangles, found through the tree, is the one a
    /// search over all of them finds; and nothing past the radius.
    #[test]
    fn nearest_matches_brute_force() {
        // A grid of small triangles, 60 by 60, in a wavy sheet.
        let mut tris: Vec<[Vec3; 3]> = vec![];
        for i in 0..60 {
            for j in 0..60 {
                let (x, z) = (i as f32 * 0.1, j as f32 * 0.1);
                let y = |x: f32, z: f32| (x * 2.0).sin() * 0.3 + (z * 3.0).cos() * 0.2;
                tris.push([Vec3::new(x, y(x, z), z), Vec3::new(x + 0.1, y(x + 0.1, z), z), Vec3::new(x, y(x, z + 0.1), z + 0.1)]);
            }
        }
        let boxes: Vec<Aabb> = tris.iter().map(|t| Aabb::around(t)).collect();
        let bvh = Bvh::build(&boxes).unwrap();
        let at = |t: &[Vec3; 3], w: [f32; 3]| t[0] * w[0] + t[1] * w[1] + t[2] * w[2];
        for k in 0..200 {
            let p = Vec3::new((k * 37 % 61) as f32 * 0.1, ((k * 13 % 11) as f32 - 5.0) * 0.2, (k * 17 % 59) as f32 * 0.1);
            let brute = tris.iter().map(|t| (p - at(t, closest_on_triangle(p, t[0], t[1], t[2]))).length_squared()).fold(f32::INFINITY, f32::min);
            let mut best = f32::INFINITY;
            bvh.nearest(p, &mut best, |i, best| {
                let t = &tris[i as usize];
                let d = (p - at(t, closest_on_triangle(p, t[0], t[1], t[2]))).length_squared();
                if d < *best { *best = d; }
            });
            assert!((best - brute).abs() < 1e-5, "{p}: {best} vs {brute}");
        }
        // Far above the sheet, with a small radius: nothing.
        let mut best = 0.5f32 * 0.5;
        let mut found = false;
        bvh.nearest(Vec3::new(3.0, 10.0, 3.0), &mut best, |_, _| found = true);
        assert!(!found);
    }

    #[test]
    fn closest_on_a_triangle() {
        let (a, b, c) = (Vec3::ZERO, Vec3::X, Vec3::Z);
        assert_eq!(closest_on_triangle(Vec3::new(0.25, 1.0, 0.25), a, b, c), [0.5, 0.25, 0.25]);
        assert_eq!(closest_on_triangle(Vec3::new(-1.0, 0.0, -1.0), a, b, c), [1.0, 0.0, 0.0]);
        let w = closest_on_triangle(Vec3::new(1.0, 0.0, 1.0), a, b, c);
        assert!((w[1] - 0.5).abs() < 1e-6 && (w[2] - 0.5).abs() < 1e-6, "{w:?}");
    }
}
