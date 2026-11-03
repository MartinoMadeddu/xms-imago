//! Machine parts, animal legs and chains as guides: their locators, where a
//! reference puts them, and where they go fitted to primitives' bounds.
//!
//! A machine is not autofitted from a body as a human is: each part names the
//! primitives it is (its Fit parameter: words or regular expressions found
//! in their paths, comma-separated, as joint name patterns are) and its locators are
//! placed from their bounds. A wheel is a box thin along its axle: the centre,
//! the axle out through its thin side, the top at the radius. A hinge's line
//! runs along the box's longest side, on the side nearest what it hangs
//! from, from its pivot to its axis locator. A rotor or a gear is a box thin along its axis. A piston runs the
//! length of its box. A tread runs the length of its box round two wheels as
//! high as the box. A body is the centre of its box, facing +Z.

use std::collections::HashMap;

use glam::Vec3;

use super::guides::Locator;
use super::{Component, PartInfo};

fn loc(name: &str, parent: Option<&str>, bone: bool) -> Locator {
    Locator {
        name: name.into(),
        parent: parent.map(Into::into),
        bone,
    }
}

/// A part's locators, parents first. None for a human's.
pub fn locators(part: &PartInfo) -> Option<Vec<Locator>> {
    Some(match part.component {
        Component::AnimalLeg => {
            let n = ["hip", "knee", "hock", "fetlock", "hoof", "toe"];
            n.iter()
                .enumerate()
                .map(|(k, x)| loc(x, if k == 0 { None } else { Some(n[k - 1]) }, true))
                .collect()
        }
        Component::Chain => vec![
            loc("start", None, true),
            loc("mid", Some("start"), true),
            loc("end", Some("mid"), true),
            loc("pole", Some("mid"), false),
        ],
        Component::Body => vec![
            loc("centre", None, true),
            loc("front", Some("centre"), false),
            loc("side", Some("centre"), false),
            loc("top", Some("centre"), false),
        ],
        Component::Hinge => vec![
            loc("pivot", None, true),
            loc("axis", Some("pivot"), false),
            loc("tip", Some("pivot"), true),
        ],
        Component::Rotor | Component::Gear => vec![
            loc("centre", None, true),
            loc("axis", Some("centre"), false),
            loc("tip", Some("centre"), true),
        ],
        Component::Piston => vec![loc("base", None, true), loc("end", Some("base"), true)],
        Component::Wheel => vec![
            loc("centre", None, true),
            loc("axle", Some("centre"), false),
            loc("top", Some("centre"), false),
        ],
        Component::Tread => vec![
            loc("front", None, false),
            loc("rear", Some("front"), false),
            loc("top", Some("front"), false),
        ],
        _ => return None,
    })
}

/// Where a part's locators go without anything to fit to, metres, the left
/// side (mirrored to the right): a part a metre or so across, at a height
/// where it can be seen. Templates move them.
pub fn reference(part: &PartInfo) -> Option<HashMap<String, Vec3>> {
    let t: Vec<(&str, [f32; 3])> = match part.component {
        Component::AnimalLeg => vec![
            ("hip", [0.25, 1.1, -0.6]),
            ("knee", [0.27, 0.78, -0.45]),
            ("hock", [0.27, 0.5, -0.72]),
            ("fetlock", [0.27, 0.15, -0.68]),
            ("hoof", [0.27, 0.05, -0.6]),
            ("toe", [0.27, 0.0, -0.52]),
        ],
        Component::Chain => vec![
            ("start", [0.0, 1.0, -0.5]),
            ("mid", [0.0, 0.9, -0.9]),
            ("end", [0.0, 0.6, -1.2]),
            ("pole", [0.0, 1.4, -0.9]),
        ],
        Component::Body => vec![
            ("centre", [0.0, 0.6, 0.0]),
            ("front", [0.0, 0.6, 1.0]),
            ("side", [0.45, 0.6, 0.0]),
            ("top", [0.0, 0.9, 0.0]),
        ],
        Component::Hinge => vec![
            ("pivot", [0.4, 1.0, 0.0]),
            ("axis", [0.9, 1.0, 0.0]),
            ("tip", [0.4, 1.0, -0.4]),
        ],
        Component::Rotor | Component::Gear => {
            vec![
                ("centre", [0.0, 2.0, 0.0]),
                ("axis", [0.0, 2.5, 0.0]),
                ("tip", [0.0, 2.0, 1.0]),
            ]
        }
        Component::Piston => vec![("base", [0.3, 0.5, 0.0]), ("end", [0.3, 1.0, 0.3])],
        Component::Wheel => vec![
            ("centre", [0.8, 0.35, 1.2]),
            ("axle", [1.0, 0.35, 1.2]),
            ("top", [0.8, 0.7, 1.2]),
        ],
        Component::Tread => vec![
            ("front", [1.0, 0.4, 1.5]),
            ("rear", [1.0, 0.4, -1.5]),
            ("top", [1.0, 0.8, 1.5]),
        ],
        _ => return None,
    };
    let s = part.side.sign();
    Some(
        t.into_iter()
            .map(|(n, p)| (n.to_string(), Vec3::new(p[0] * s, p[1], p[2])))
            .collect(),
    )
}

/// The axes of a box, longest first.
fn axes(lo: Vec3, hi: Vec3) -> [(usize, f32); 3] {
    let e = hi - lo;
    let mut a = [(0, e.x), (1, e.y), (2, e.z)];
    a.sort_by(|x, y| y.1.total_cmp(&x.1));
    a
}

fn unit(k: usize) -> Vec3 {
    [Vec3::X, Vec3::Y, Vec3::Z][k]
}

/// A part's locators from the bounds of the primitives it names, the
/// locator it hangs from at `attach` (to tell which side of a hinge is
/// fixed, which way an axle points out). None when nothing matches.
pub fn fit(part: &PartInfo, bounds: &[(String, Vec3, Vec3)], attach: Option<Vec3>) -> Option<HashMap<String, Vec3>> {
    let pattern = part.mech.fit.trim();
    if pattern.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let names = crate::core::pattern::NamePattern::new(pattern);
    for (path, a, b) in bounds {
        if names.matches(path) {
            lo = lo.min(*a);
            hi = hi.max(*b);
        }
    }
    if lo.x > hi.x {
        return None;
    }
    let c = (lo + hi) * 0.5;
    let e = hi - lo;
    let ax = axes(lo, hi);
    let from = attach.unwrap_or(Vec3::new(0.0, c.y, 0.0));
    let mut out = HashMap::new();
    let mut put = |n: &str, p: Vec3| {
        out.insert(n.to_string(), p);
    };
    match part.component {
        Component::Body => {
            put("centre", c);
            put("front", c + Vec3::Z * e.z * 0.5);
            put("side", c + Vec3::X * e.x * 0.5);
            put("top", c + Vec3::Y * e.y * 0.5);
        }
        Component::Wheel => {
            // Thin along the axle; out away from what it hangs from.
            let (k, _) = ax[2];
            let mut out_dir = unit(k);
            if (c - from).dot(out_dir) < 0.0 {
                out_dir = -out_dir;
            }
            let r = ax[0].1 * 0.5;
            put("centre", c);
            put("axle", c + out_dir * e[k] * 0.5);
            put("top", c + Vec3::Y * r);
        }
        Component::Rotor | Component::Gear => {
            let (k, _) = ax[2];
            let mut axis = unit(k);
            if axis.dot(Vec3::Y) < -0.5 || (k != 1 && (c - from).dot(axis) < 0.0) {
                axis = -axis;
            }
            put("centre", c);
            put("axis", c + axis * e[k].max(ax[0].1 * 0.2));
            put("tip", c + unit(ax[0].0) * ax[0].1 * 0.5);
        }
        Component::Hinge => {
            // The fixed side: the box's point nearest what it hangs from,
            // on its longest side.
            let near = from.clamp(lo, hi);
            let long = unit(ax[0].0);
            let pivot = Vec3::new(
                if ax[0].0 == 0 { c.x } else { near.x },
                if ax[0].0 == 1 { c.y } else { near.y },
                if ax[0].0 == 2 { c.z } else { near.z },
            );
            // The hinge line along the whole side: from one end (the pivot)
            // to the other (the axis locator).
            let mid = pivot - long * long.dot(pivot - c);
            let pivot = mid - long * ax[0].1 * 0.5;
            put("pivot", pivot);
            put("axis", mid + long * ax[0].1 * 0.5);
            put("tip", pivot + (c - mid) * 2.0);
        }
        Component::Piston => {
            let long = unit(ax[0].0);
            let (a, b) = (c - long * ax[0].1 * 0.5, c + long * ax[0].1 * 0.5);
            let (base, end) = if a.distance(from) <= b.distance(from) {
                (a, b)
            } else {
                (b, a)
            };
            put("base", base);
            put("end", end);
        }
        Component::Tread => {
            let r = e.y * 0.5;
            let long = unit(ax[0].0);
            let front = if long.z.abs() > 0.5 { Vec3::Z } else { long };
            put("front", c + front * (ax[0].1 * 0.5 - r));
            put("rear", c - front * (ax[0].1 * 0.5 - r));
            put("top", c + front * (ax[0].1 * 0.5 - r) + Vec3::Y * r);
        }
        Component::AnimalLeg | Component::Chain => {
            // Down the box's length from the end nearest what it hangs from.
            let long = unit(ax[0].0);
            let (a, b) = (c - long * ax[0].1 * 0.5, c + long * ax[0].1 * 0.5);
            let (s, t) = if a.distance(from) <= b.distance(from) {
                (a, b)
            } else {
                (b, a)
            };
            if part.component == Component::Chain {
                put("start", s);
                put("mid", s.lerp(t, 0.5));
                put("end", t);
                put("pole", s.lerp(t, 0.5) + Vec3::Y * ax[1].1);
            } else {
                for (k, n) in ["hip", "knee", "hock", "fetlock", "hoof", "toe"].iter().enumerate() {
                    put(n, s.lerp(t, k as f32 / 5.0));
                }
            }
        }
        _ => return None,
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rig::{MechParams, Side};

    fn part(c: Component, side: Side, fit: &str) -> PartInfo {
        PartInfo {
            component: c,
            side,
            index: 0,
            attach: None,
            segments: 4,
            twists: 0,
            fingers: vec![],
            metacarpals: false,
            label: String::new(),
            mech: MechParams {
                fit: fit.into(),
                ..MechParams::of(c)
            },
        }
    }

    /// A wheel fitted to its bounds: centre, axle out, radius; a hinge on the
    /// side nearest its parent, along its longest side; nothing matched is
    /// no fit.
    #[test]
    fn parts_fit_to_their_bounds() {
        let b = vec![
            (
                "/car/wheelFL".to_string(),
                Vec3::new(80.0, 0.0, 120.0),
                Vec3::new(110.0, 80.0, 200.0),
            ),
            (
                "/car/shovel".to_string(),
                Vec3::new(-130.0, -80.0, -310.0),
                Vec3::new(130.0, 100.0, -110.0),
            ),
        ];
        let w = fit(
            &part(Component::Wheel, Side::Left, "wheelFL"),
            &b,
            Some(Vec3::new(0.0, 60.0, 0.0)),
        )
        .unwrap();
        assert_eq!(w["centre"], Vec3::new(95.0, 40.0, 160.0));
        assert_eq!(w["axle"], Vec3::new(110.0, 40.0, 160.0));
        assert_eq!(w["top"], Vec3::new(95.0, 80.0, 160.0));
        let h = fit(
            &part(Component::Hinge, Side::Centre, "shovel"),
            &b,
            Some(Vec3::new(0.0, 60.0, 0.0)),
        )
        .unwrap();
        assert_eq!(h["pivot"], Vec3::new(-130.0, 60.0, -110.0));
        assert_eq!(h["axis"], Vec3::new(130.0, 60.0, -110.0));
        assert!((h["axis"] - h["pivot"]).normalize().x.abs() > 0.99);
        assert!(h["tip"].z < -110.0);
        assert!(fit(&part(Component::Wheel, Side::Left, "nothing"), &b, None).is_none());
    }
}
