//! Autofit: guide locators placed on a body mesh from what can be measured on
//! it. The mesh stands Y up on the floor, facing +Z, its left at +X: an A or a
//! T pose. It is cut in horizontal slices; each slice falls apart into pieces
//! (legs, arms, the torso) where there are gaps across X. From those:
//! - the crotch: the lowest slice whose middle piece spans X = 0;
//! - each leg's centre line, from the floor to the crotch; the foot's toe and
//!   heel;
//! - the armpit: above the last slice where the arm is a piece of its own (an
//!   A pose), or where the torso suddenly widens (a T pose);
//! - the neck: where the torso narrows above the shoulders; a mesh without a
//!   head ends there;
//! - each arm's points, and its centre line out from the shoulder.
//!
//! Joints are then placed along those lines in the proportions of a reference
//! body (see `REFERENCE`), and the hand's locators in the frame of the hand's
//! own points. Fingers are placed from the reference hand, relaxed, in that
//! frame: close on a hand posed like it, rough on others.

use glam::{Mat3, Vec3};

use super::Side;

/// Slices from the floor to the top.
const SLICES: usize = 220;

/// A piece of a slice: its extent across X and front to back, its points.
#[derive(Clone, Debug, Default)]
struct Piece {
    lo: f32,
    hi: f32,
    zlo: f32,
    zhi: f32,
    points: Vec<u32>,
}

impl Piece {
    /// The middle of its extent (not its points' mean: meshes are denser in
    /// some places than others).
    fn centre(&self, y: f32) -> Vec3 {
        Vec3::new((self.lo + self.hi) * 0.5, y, (self.zlo + self.zhi) * 0.5)
    }
    fn mid_x(&self) -> f32 {
        (self.lo + self.hi) * 0.5
    }
    fn spans_middle(&self) -> bool {
        self.lo <= 0.0 && self.hi >= 0.0
    }
    /// Wholly on the side of `s` (+1 left, -1 right).
    fn on(&self, s: f32) -> bool {
        !self.spans_middle() && self.mid_x() * s > 0.0
    }
    fn width(&self) -> f32 {
        self.hi - self.lo
    }
}

/// What was measured on a body.
#[derive(Clone, Debug, Default)]
pub struct Body {
    pub floor: f32,
    pub top: f32,
    pub crotch: f32,
    /// Per side (left, right): the leg's centre line, low to high.
    pub leg_line: [Vec<Vec3>; 2],
    /// The foot's front and back points, on the floor.
    pub toe: [Vec3; 2],
    pub heel: [Vec3; 2],
    pub armpit: [f32; 2],
    /// The arms hang apart from the torso (an A pose), or not (a T pose).
    pub arms_apart: [bool; 2],
    /// Where the neck is narrowest above the shoulders (the top of a mesh
    /// with no head).
    pub neck: f32,
    pub has_head: bool,
    /// The torso's centre front to back, by height: (y, z), low to high.
    pub torso_z: Vec<(f32, f32)>,
    /// The torso's half width under the armpit, per side.
    pub chest_half: [f32; 2],
    /// Each arm's points.
    pub arm: [Vec<Vec3>; 2],
}

fn side_index(s: Side) -> usize {
    if s == Side::Right {
        1
    } else {
        0
    }
}

/// The pieces of one slice: points sorted across X, split where the gap is
/// wider than `gap`.
fn pieces(points: &[Vec3], ids: &[u32], gap: f32) -> Vec<Piece> {
    let mut ids = ids.to_vec();
    ids.sort_by(|a, b| points[*a as usize].x.total_cmp(&points[*b as usize].x));
    let mut out: Vec<Piece> = vec![];
    for i in ids {
        let p = points[i as usize];
        match out.last_mut() {
            Some(c) if p.x - c.hi <= gap => {
                c.hi = p.x;
                c.zlo = c.zlo.min(p.z);
                c.zhi = c.zhi.max(p.z);
                c.points.push(i);
            }
            _ => out.push(Piece {
                lo: p.x,
                hi: p.x,
                zlo: p.z,
                zhi: p.z,
                points: vec![i],
            }),
        }
    }
    out
}

/// From piece `at` of slice `band`, the pieces that continue it slice by
/// slice in direction `dir` (+1 up, -1 down): the piece of the next slice on
/// the same side overlapping it most. Stops where nothing continues it, or
/// where it joins something much wider (the torso, a leg).
fn track(slices: &[Vec<Piece>], band: usize, at: usize, dir: i32, s: f32, gap: f32) -> Vec<(usize, usize)> {
    let mut out = vec![(band, at)];
    let (mut b, mut cur) = (band as i32, at);
    loop {
        let next = b + dir;
        if next < 0 || next as usize >= slices.len() {
            break;
        }
        let here = &slices[b as usize][cur];
        let best = slices[next as usize]
            .iter()
            .enumerate()
            .filter(|(_, p)| p.on(s))
            .map(|(k, p)| (k, p.hi.min(here.hi + gap) - p.lo.max(here.lo - gap)))
            .filter(|(_, o)| *o > 0.0)
            .max_by(|a, c| a.1.total_cmp(&c.1));
        let Some((k, _)) = best else { break };
        if slices[next as usize][k].width() > here.width() * 2.5 + gap {
            break;
        }
        out.push((next as usize, k));
        b = next;
        cur = k;
    }
    out
}

/// Measure a body. None when there is too little to measure.
pub fn measure(points: &[Vec3]) -> Option<Body> {
    if points.len() < 200 {
        return None;
    }
    let (mut floor, mut top) = (f32::MAX, f32::MIN);
    for p in points {
        floor = floor.min(p.y);
        top = top.max(p.y);
    }
    let height = top - floor;
    if height <= 1e-4 {
        return None;
    }
    let step = height / SLICES as f32;
    let mut bands: Vec<Vec<u32>> = vec![vec![]; SLICES];
    for (i, p) in points.iter().enumerate() {
        let b = (((p.y - floor) / step) as usize).min(SLICES - 1);
        bands[b].push(i as u32);
    }
    // Each slice is a little thicker than its band: edge loops are rarely
    // level, and a thin slice catches a loop's front and not its back.
    const THICK: usize = 2;
    let gap = height * 0.015;
    let slices: Vec<Vec<Piece>> = (0..SLICES)
        .map(|b| {
            let ids: Vec<u32> = (b.saturating_sub(THICK)..=(b + THICK).min(SLICES - 1))
                .flat_map(|k| bands[k].iter().copied())
                .collect();
            pieces(points, &ids, gap)
        })
        .collect();
    let y_of = |b: usize| floor + (b as f32 + 0.5) * step;
    let band_of = |y: f32| (((y - floor) / step).max(0.0) as usize).min(SLICES - 1);
    let middle = |b: usize| -> Option<&Piece> {
        slices[b].iter().find(|p| p.spans_middle()).or_else(|| {
            slices[b]
                .iter()
                .min_by(|a, c| a.mid_x().abs().total_cmp(&c.mid_x().abs()))
        })
    };

    // The crotch: the first of four slices in a row whose middle piece spans
    // X = 0, from a quarter of the height up.
    let crotch_band = (band_of(floor + height * 0.25)..band_of(floor + height * 0.8))
        .find(|b| (*b..*b + 4).all(|k| slices[k].iter().any(|p| p.spans_middle())))?;
    let crotch = y_of(crotch_band);
    let mut body = Body {
        floor,
        top,
        crotch,
        ..Default::default()
    };
    for b in crotch_band..SLICES {
        if let Some(m) = middle(b) {
            body.torso_z.push((y_of(b), (m.zlo + m.zhi) * 0.5));
        }
    }

    for (k, s) in [1.0f32, -1.0].into_iter().enumerate() {
        // The leg: below the crotch, the biggest piece on this side.
        for (b, slice) in slices.iter().enumerate().take(crotch_band) {
            if let Some(p) = slice.iter().filter(|p| p.on(s)).max_by_key(|p| p.points.len()) {
                body.leg_line[k].push(p.centre(y_of(b)));
            }
        }
        // The foot: the leg's points within a twentieth of the height of the
        // floor, front and back.
        let near_x = body.leg_line[k].first().map(|c| c.x).unwrap_or(0.1 * s);
        let foot = points
            .iter()
            .filter(|p| p.y < floor + height * 0.05 && p.x * s > 0.0 && (p.x - near_x).abs() < height * 0.12);
        let (mut toe, mut heel) = (None::<Vec3>, None::<Vec3>);
        for p in foot {
            if toe.is_none_or(|t| p.z > t.z) {
                toe = Some(*p);
            }
            if heel.is_none_or(|h| p.z < h.z) {
                heel = Some(*p);
            }
        }
        body.toe[k] = toe.unwrap_or_default();
        body.heel[k] = heel.unwrap_or_default();

        // The arm, hanging apart (an A pose): a piece of its own about the
        // middle of the torso's height, followed up to where it joins the
        // torso (the armpit) and down to the fingertips.
        let seed_band = band_of(crotch + (top - crotch) * 0.45);
        let seed = (0..12)
            .flat_map(|d| [seed_band + d, seed_band.saturating_sub(d)])
            .find_map(|b| {
                slices[b]
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.on(s) && p.mid_x() * s > 0.0)
                    .max_by(|a, c| (a.1.mid_x() * s).total_cmp(&(c.1.mid_x() * s)))
                    .map(|(i, _)| (b, i))
            });
        let half = |b: usize| middle(b).map(|m| if s > 0.0 { m.hi } else { -m.lo }).unwrap_or(0.0);
        let mut taken = vec![false; points.len()];
        let armpit_band = match seed {
            Some((b, i)) => {
                let up = track(&slices, b, i, 1, s, gap);
                let down = track(&slices, b, i, -1, s, gap);
                for (band, piece) in up.iter().chain(down.iter()) {
                    for q in &slices[*band][*piece].points {
                        taken[*q as usize] = true;
                    }
                }
                body.arms_apart[k] = true;
                up.last().map(|x| x.0 + 1).unwrap_or(b)
            }
            None => {
                // A T pose: the torso's half width jumps where the arms join.
                let from = (crotch_band + (SLICES - crotch_band) / 5).max(crotch_band + 5);
                (from..SLICES)
                    .find(|b| half(*b) > (b - 5..*b).map(half).sum::<f32>() / 5.0 * 1.6)
                    .unwrap_or(band_of(crotch + (top - crotch) * 0.6))
            }
        };
        body.armpit[k] = y_of(armpit_band.min(SLICES - 1));
        let chest = (armpit_band.saturating_sub(6)..armpit_band.saturating_sub(2).max(1))
            .map(half)
            .fold(0.0f32, f32::max);
        body.chest_half[k] = chest;
        // Above the armpit, the torso's points past the chest are the arm's
        // (the shoulder; the whole arm in a T pose).
        for b in armpit_band.min(SLICES)..SLICES {
            if let Some(m) = middle(b) {
                for q in &m.points {
                    if points[*q as usize].x * s > chest * 1.05 {
                        taken[*q as usize] = true;
                    }
                }
            }
        }
        body.arm[k] = taken
            .iter()
            .enumerate()
            .filter(|(_, t)| **t)
            .map(|(i, _)| points[i])
            .collect();
    }

    // The neck: above the armpits, the first slice narrower than 40% of the
    // widest slice below it. A mesh ending there has no head.
    let armpit_band = band_of(body.armpit[0].min(body.armpit[1]));
    let mut widest = 0.0f32;
    let mut neck = None;
    for b in armpit_band..SLICES {
        let Some(m) = middle(b) else { continue };
        widest = widest.max(m.width());
        if b > armpit_band + 3 && m.width() < widest * 0.4 {
            neck = Some(b);
            break;
        }
    }
    match neck {
        Some(b) if top - y_of(b) > height * 0.06 => {
            // The narrowest slice of the neck, before the head widens.
            let (mut best, mut w_best) = (b, f32::MAX);
            for k in b..SLICES {
                let Some(m) = middle(k) else { continue };
                if m.width() < w_best {
                    w_best = m.width();
                    best = k;
                }
                if m.width() > w_best * 1.3 {
                    break;
                }
            }
            body.neck = y_of(best);
            body.has_head = true;
        }
        _ => {
            body.neck = top;
            body.has_head = false;
        }
    }
    Some(body)
}

impl Body {
    /// The torso's centre front to back at height `y`.
    pub fn torso_z_at(&self, y: f32) -> f32 {
        self.torso_z
            .iter()
            .min_by(|a, b| (a.0 - y).abs().total_cmp(&(b.0 - y).abs()))
            .map(|t| t.1)
            .unwrap_or(0.0)
    }

    /// The leg's centre at height `y`.
    pub fn leg_at(&self, side: Side, y: f32) -> Vec3 {
        let line = &self.leg_line[side_index(side)];
        line.iter()
            .min_by(|a, b| (a.y - y).abs().total_cmp(&(b.y - y).abs()))
            .map(|c| Vec3::new(c.x, y, c.z))
            .unwrap_or(Vec3::new(0.1 * side.sign(), y, 0.0))
    }

    pub fn arm_points(&self, side: Side) -> &[Vec3] {
        &self.arm[side_index(side)]
    }

    pub fn armpit(&self, side: Side) -> f32 {
        self.armpit[side_index(side)]
    }

    pub fn chest_half(&self, side: Side) -> f32 {
        self.chest_half[side_index(side)]
    }

    pub fn toe(&self, side: Side) -> Vec3 {
        self.toe[side_index(side)]
    }

    pub fn heel(&self, side: Side) -> Vec3 {
        self.heel[side_index(side)]
    }
}

/// An arm's centre line out from `shoulder`: the arm's points binned by their
/// distance from it, each bin's centroid, nearest first. The last is the
/// tip of the hand.
pub fn arm_line(arm: &[Vec3], shoulder: Vec3) -> Vec<Vec3> {
    if arm.is_empty() {
        return vec![];
    }
    let far = arm.iter().map(|p| p.distance(shoulder)).fold(0.0f32, f32::max);
    const BINS: usize = 40;
    let mut sums = vec![(Vec3::ZERO, 0usize); BINS];
    for p in arm {
        let k = ((p.distance(shoulder) / far * BINS as f32) as usize).min(BINS - 1);
        sums[k].0 += *p;
        sums[k].1 += 1;
    }
    let mut line = vec![shoulder];
    line.extend(sums.iter().filter(|s| s.1 > 0).map(|s| s.0 / s.1 as f32));
    // The tip: the farthest point itself, not a centroid.
    if let Some(tip) = arm
        .iter()
        .max_by(|a, b| a.distance(shoulder).total_cmp(&b.distance(shoulder)))
    {
        line.push(*tip);
    }
    line
}

/// The point at arc length `t` (0..1 of the whole) along a polyline.
pub fn along(line: &[Vec3], t: f32) -> Vec3 {
    let total: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut left = total * t.clamp(0.0, 1.0);
    for w in line.windows(2) {
        let d = w[0].distance(w[1]);
        if left <= d && d > 0.0 {
            return w[0].lerp(w[1], left / d);
        }
        left -= d;
    }
    line.last().copied().unwrap_or(Vec3::ZERO)
}

/// A hand's frame from its points beyond the wrist: X out along the hand, Y
/// the back of the hand (away from the palm), Z = X×Y. `thumb`: a direction
/// the thumb side is toward (forward in an A or T pose).
pub fn hand_frame(hand: &[Vec3], wrist: Vec3, side: Side, thumb: Vec3) -> Option<Mat3> {
    if hand.len() < 10 {
        return None;
    }
    let c = hand.iter().copied().sum::<Vec3>() / hand.len() as f32;
    let x = (c - wrist).normalize_or_zero();
    if x == Vec3::ZERO {
        return None;
    }
    // The palm's normal: the direction across the hand, at right angles to
    // X, it is thinnest along (the least of the spread about the centroid).
    let mut cov = Mat3::ZERO;
    for p in hand {
        let d = *p - c;
        let d = d - x * d.dot(x);
        cov += Mat3::from_cols(d * d.x, d * d.y, d * d.z);
    }
    // Power iteration for the widest; the thinnest is across both.
    let mut wide = thumb - x * thumb.dot(x);
    if wide.length_squared() < 1e-8 {
        wide = x.any_orthonormal_vector();
    }
    for _ in 0..30 {
        wide = (cov * wide).normalize_or(wide);
        wide = (wide - x * wide.dot(x)).normalize_or(wide);
    }
    if wide.dot(thumb) < 0.0 {
        wide = -wide;
    }
    // `wide` runs from the little finger to the thumb. The back of the hand:
    // on the left, thumb side × X; on the right, mirrored.
    let back = if side == Side::Right {
        x.cross(wide)
    } else {
        wide.cross(x)
    }
    .normalize_or_zero();
    let z = x.cross(back);
    Some(Mat3::from_cols(x, back, z))
}
