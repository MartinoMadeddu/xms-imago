//! What hides what the Viewer draws over the picture (Display Attribute's
//! labels): triangles in the world, in a bounding volume hierarchy, and
//! whether any lies between the eye and a point.

use glam::Vec3;

/// Triangles that hide what is behind them.
#[derive(Default)]
pub struct Occluders {
    tris: Vec<[Vec3; 3]>,
    nodes: Vec<Node>,
}

/// A box of the hierarchy: a leaf holds `count` triangles from `first`; a
/// branch its children, the next node and `second`.
struct Node {
    lo: Vec3,
    hi: Vec3,
    first: u32,
    count: u32,
    second: u32,
}

/// Triangles in a leaf at most.
const LEAF: usize = 4;

impl Occluders {
    pub fn new(mut tris: Vec<[Vec3; 3]>) -> Occluders {
        let mut nodes = Vec::with_capacity(tris.len().max(1) * 2 / LEAF + 1);
        if !tris.is_empty() {
            let n = tris.len();
            build(&mut tris, 0, n, &mut nodes);
        }
        Occluders { tris, nodes }
    }

    pub fn len(&self) -> usize {
        self.tris.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    /// Whether a triangle lies between `eye` and `at`: not the surface `at` is
    /// on, a little before it (a thousandth of the way, so a point on a
    /// surface, or a face's centre, is not hidden by its own).
    pub fn hides(&self, eye: Vec3, at: Vec3) -> bool {
        let d = at - eye;
        let far = 1.0 - 1e-3;
        if self.nodes.is_empty() || d == Vec3::ZERO {
            return false;
        }
        let inv = d.recip();
        let mut stack = vec![0u32];
        while let Some(i) = stack.pop() {
            let n = &self.nodes[i as usize];
            if !slab(eye, inv, n.lo, n.hi, far) {
                continue;
            }
            if n.count > 0 {
                let tris = &self.tris[n.first as usize..(n.first + n.count) as usize];
                if tris.iter().any(|t| hit(eye, d, t).is_some_and(|s| s < far)) {
                    return true;
                }
            } else {
                stack.push(i + 1);
                stack.push(n.second);
            }
        }
        false
    }
}

/// Make the node of `tris[a..b]`, and those below it: split at the middle of
/// its longest side, by the triangles' centres.
fn build(tris: &mut [[Vec3; 3]], a: usize, b: usize, nodes: &mut Vec<Node>) -> u32 {
    let (lo, hi) = tris[a..b]
        .iter()
        .flatten()
        .fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| {
            (lo.min(*p), hi.max(*p))
        });
    let me = nodes.len() as u32;
    nodes.push(Node {
        lo,
        hi,
        first: a as u32,
        count: (b - a) as u32,
        second: 0,
    });
    if b - a <= LEAF {
        return me;
    }
    let size = hi - lo;
    let axis = if size.x >= size.y && size.x >= size.z {
        0
    } else if size.y >= size.z {
        1
    } else {
        2
    };
    let centre = |t: &[Vec3; 3]| (t[0][axis] + t[1][axis] + t[2][axis]) / 3.0;
    let mid = (b - a) / 2;
    tris[a..b].select_nth_unstable_by(mid, |x, y| centre(x).total_cmp(&centre(y)));
    nodes[me as usize].count = 0;
    build(tris, a, a + mid, nodes);
    let second = build(tris, a + mid, b, nodes);
    nodes[me as usize].second = second;
    me
}

/// Whether the segment from `o` (`inv`: one over its direction) to `far` of
/// the way along crosses the box.
fn slab(o: Vec3, inv: Vec3, lo: Vec3, hi: Vec3, far: f32) -> bool {
    let (a, b) = ((lo - o) * inv, (hi - o) * inv);
    let (near, out) = (a.min(b), a.max(b));
    let enter = near.max_element().max(0.0);
    let leave = out.min_element().min(far);
    // A direction along an axis gives infinities, and NaN where the box's
    // side is on the ray: both treated as crossing.
    enter <= leave || enter.is_nan() || leave.is_nan()
}

/// Where along `d` from `o` the ray meets triangle `t` (0 at `o`, 1 at `o +
/// d`), from either side.
fn hit(o: Vec3, d: Vec3, t: &[Vec3; 3]) -> Option<f32> {
    let (e1, e2) = (t[1] - t[0], t[2] - t[0]);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let s = o - t[0];
    let u = s.dot(p) / det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) / det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let along = e2.dot(q) / det;
    (along > 0.0).then_some(along)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid of `n` by `n` unit squares on the plane y = `y`, two triangles
    /// each.
    fn grid(n: usize, y: f32) -> Vec<[Vec3; 3]> {
        let mut out = vec![];
        for i in 0..n {
            for j in 0..n {
                let p = |a: usize, b: usize| Vec3::new(a as f32, y, b as f32);
                out.push([p(i, j), p(i + 1, j), p(i + 1, j + 1)]);
                out.push([p(i, j), p(i + 1, j + 1), p(i, j + 1)]);
            }
        }
        out
    }

    #[test]
    fn what_is_in_front_hides_and_what_is_on_does_not() {
        let o = Occluders::new(grid(20, 0.0));
        assert_eq!(o.len(), 800);
        let eye = Vec3::new(5.5, 10.0, 5.5);
        // Below the floor: hidden. On it: not. Above it: not.
        assert!(o.hides(eye, Vec3::new(7.3, -1.0, 3.1)));
        assert!(!o.hides(eye, Vec3::new(7.3, 0.0, 3.1)));
        assert!(!o.hides(eye, Vec3::new(7.3, 2.0, 3.1)));
        // Past the floor's edge: not.
        assert!(!o.hides(eye, Vec3::new(30.0, -1.0, 3.0)));
        // Looked at from below, the floor hides what is above it.
        assert!(o.hides(Vec3::new(5.5, -10.0, 5.5), Vec3::new(5.5, 3.0, 5.5)));
        // Along an axis, straight down.
        assert!(o.hides(Vec3::new(3.5, 5.0, 3.5), Vec3::new(3.5, -5.0, 3.5)));
        assert!(!Occluders::new(vec![]).hides(eye, Vec3::ZERO));
    }

    #[test]
    fn the_far_side_of_a_closed_shape_is_hidden() {
        // A box's top and bottom: a point on the bottom, seen from above.
        let mut tris = grid(4, 0.0);
        tris.extend(grid(4, 2.0));
        let o = Occluders::new(tris);
        let eye = Vec3::new(2.0, 10.0, 2.0);
        assert!(o.hides(eye, Vec3::new(1.5, 0.0, 2.5)));
        assert!(!o.hides(eye, Vec3::new(1.5, 2.0, 2.5)));
    }
}
