//! Rename and Duplicate: prims given new names, or copied beside themselves,
//! each with what is below it. Both move paths as Reparent does
//! (`core::reparent`): what comes after sees the new paths, and Write USD
//! writes a file's prim as a reference to it at its new path, deactivated at
//! the old one (a copy leaves the old one as it is).

use glam::{DMat4, DQuat, DVec3, EulerRot};

use crate::core::locations;
use crate::core::material::prim_name;
use crate::types::{EvalResult, Prim};

fn name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or("")
}

fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) | None => "/",
        Some(i) => &path[..i],
    }
}

fn child(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

fn within(path: &str, root: &str) -> bool {
    path == root || path.strip_prefix(root).is_some_and(|r| r.starts_with('/'))
}

/// A prim's new name: `find` replaced by `replace` (every time it is in the
/// name), then `prefix` before it and `suffix` after, made a name USD takes.
pub fn new_name(name: &str, find: &str, replace: &str, prefix: &str, suffix: &str) -> String {
    let replaced = if find.is_empty() {
        name.to_string()
    } else {
        name.replace(find, replace)
    };
    let whole = format!("{prefix}{replaced}{suffix}");
    if whole.trim().is_empty() {
        return name.to_string();
    }
    prim_name(&whole)
}

/// `name` made free among `taken`: as it is, else with a number after it.
fn free(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let mut n = name.to_string();
    let mut k = 1;
    while taken(&n) {
        n = format!("{name}{k}");
        k += 1;
    }
    n
}

/// Each prim the locations name and its new path, those whose name changes;
/// a name a sibling has gets a number.
pub fn plan(
    prims: &[Prim],
    list: &[String],
    find: &str,
    replace: &str,
    prefix: &str,
    suffix: &str,
) -> Vec<(String, String)> {
    let paths = crate::node_graph::scene_paths(prims);
    let mut out: Vec<(String, String)> = vec![];
    for o in paths.iter().filter(|p| locations::any_names(list, p)) {
        let wanted = new_name(name(o), find, replace, prefix, suffix);
        if wanted == name(o) {
            continue;
        }
        let parent = parent_of(o);
        let taken = |n: &str| {
            let at = child(parent, n);
            (paths.contains(&at) && !out.iter().any(|(from, _)| *from == at)) || out.iter().any(|(_, to)| *to == at)
        };
        let n = free(&wanted, &taken);
        out.push((o.clone(), child(parent, &n)));
    }
    out
}

/// Rename: the prims the locations name, renamed as `plan` says; the deepest
/// first, so a renamed group's renamed children are found.
pub fn rename(
    input: &EvalResult,
    list: &[String],
    find: &str,
    replace: &str,
    prefix: &str,
    suffix: &str,
) -> EvalResult {
    let EvalResult::Named(prims) = input else {
        return input.clone();
    };
    let mut moves = plan(prims, list, find, replace, prefix, suffix);
    if moves.is_empty() {
        return input.clone();
    }
    moves.sort_by_key(|(o, _)| std::cmp::Reverse(o.matches('/').count()));
    let mut prims = prims.clone();
    let mut k = 0;
    while k < moves.len() {
        let depth = moves[k].0.matches('/').count();
        let level: Vec<(String, String)> = moves[k..]
            .iter()
            .take_while(|(o, _)| o.matches('/').count() == depth)
            .cloned()
            .collect();
        k += level.len();
        prims = crate::core::reparent::moved(&prims, &level);
    }
    EvalResult::Named(prims)
}

/// How Duplicate copies.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Copies {
    pub count: u32,
    /// The copies' names: `{name}` the prim's, `{n}` the copy's number.
    pub name: String,
    /// Each copy moved from the one before: translated, turned (degrees, X
    /// then Y then Z) and scaled, in the prim's parent's space.
    pub translate: [f32; 3],
    pub rotate: [f32; 3],
    pub scale: [f32; 3],
}

impl Default for Copies {
    fn default() -> Self {
        Copies {
            count: 1,
            name: "{name}_{n}".into(),
            translate: [0.0; 3],
            rotate: [0.0; 3],
            scale: [1.0; 3],
        }
    }
}

impl Copies {
    /// Copy `n`'s move from the original, in its parent's space: the step
    /// made `n` times.
    pub fn step(&self, n: u32) -> DMat4 {
        let k = n as f64;
        let t = DVec3::from(self.translate.map(|x| x as f64)) * k;
        let r = self.rotate.map(|x| (x as f64).to_radians() * k);
        let s = DVec3::from(self.scale.map(|x| (x as f64).powf(k)));
        DMat4::from_scale_rotation_translation(s, DQuat::from_euler(EulerRot::ZYX, r[2], r[1], r[0]), t)
    }
}

/// Each copy Duplicate makes: the prim copied (the topmost of those the
/// locations name), the copy's path, and its number.
pub fn copies_plan(prims: &[Prim], list: &[String], c: &Copies) -> Vec<(String, String, u32)> {
    let paths = crate::node_graph::scene_paths(prims);
    let mut roots: Vec<&String> = vec![];
    for o in paths.iter().filter(|p| locations::any_names(list, p)) {
        if roots.iter().any(|r| within(o, r)) {
            continue;
        }
        roots.retain(|r| !within(r, o));
        roots.push(o);
    }
    let mut out: Vec<(String, String, u32)> = vec![];
    for o in roots {
        let parent = parent_of(o);
        for n in 1..=c.count.min(10_000) {
            let wanted = prim_name(&c.name.replace("{name}", name(o)).replace("{n}", &n.to_string()));
            let taken = |x: &str| {
                let at = child(parent, x);
                paths.contains(&at) || out.iter().any(|(_, to, _)| *to == at)
            };
            let fresh = free(&wanted, &taken);
            out.push((o.clone(), child(parent, &fresh), n));
        }
    }
    out
}

/// Duplicate: copies of the prims the locations name, each with what is below
/// it, beside it, each moved by its step in its parent's space.
pub fn duplicate(input: &EvalResult, list: &[String], c: &Copies) -> EvalResult {
    let EvalResult::Named(prims) = input else {
        return input.clone();
    };
    let plan = copies_plan(prims, list, c);
    if plan.is_empty() {
        return input.clone();
    }
    // The step in the world: through the copied prim's parent's place.
    let parent_world = |path: &str| -> DMat4 {
        let parent = parent_of(path);
        prims
            .iter()
            .filter_map(|p| p.stage.as_deref())
            .find(|t| t.node(parent).is_some())
            .map_or(DMat4::IDENTITY, |t| t.world(parent, true))
    };
    let copies: Vec<(String, String, DMat4)> = plan
        .into_iter()
        .map(|(o, to, n)| {
            let w = parent_world(&o);
            (o, to, w * c.step(n) * w.inverse())
        })
        .collect();
    EvalResult::Named(crate::core::reparent::copied(prims, &copies))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Placement;

    fn cube(path: &str, x: f32) -> Prim {
        Prim {
            place: Placement::One(glam::Mat4::from_translation(glam::Vec3::new(x, 0.0, 0.0))),
            ..Prim::new(path.into(), crate::node_graph::nodes::create_cube(1.0))
        }
    }

    fn paths(r: &EvalResult) -> Vec<String> {
        match r {
            EvalResult::Named(p) => p.iter().map(|p| p.path.clone()).collect(),
            _ => vec![],
        }
    }

    #[test]
    fn names_are_found_replaced_and_kept_clear_of_their_siblings() {
        assert_eq!(new_name("wheel_fl", "wheel", "tyre", "car_", ""), "car_tyre_fl");
        assert_eq!(new_name("lamp", "", "", "", ".hi"), "lamp_hi", "a name USD takes");
        let scene = EvalResult::Named(vec![
            cube("/set/a_old", 0.0),
            cube("/set/b_old", 1.0),
            cube("/set/a_new", 2.0),
        ]);
        let list = vec!["/set/*_old".to_string()];
        let out = rename(&scene, &list, "_old", "_new", "", "");
        assert_eq!(paths(&out), ["/set/a_new1", "/set/b_new", "/set/a_new"]);
    }

    #[test]
    fn copies_are_numbered_beside_and_stepped() {
        let scene = EvalResult::Named(vec![cube("/set/crate", 0.0)]);
        let c = Copies {
            count: 3,
            translate: [2.0, 0.0, 0.0],
            ..Default::default()
        };
        let out = duplicate(&scene, &["/set/crate".to_string()], &c);
        assert_eq!(
            paths(&out),
            ["/set/crate", "/set/crate_1", "/set/crate_2", "/set/crate_3"]
        );
        let EvalResult::Named(prims) = out else { panic!() };
        let x = |p: &Prim| p.place.matrices()[0].w_axis.x;
        assert_eq!(prims.iter().map(x).collect::<Vec<_>>(), [0.0, 2.0, 4.0, 6.0]);
        assert_eq!(c.step(2).transform_point3(DVec3::ZERO), DVec3::new(4.0, 0.0, 0.0));
    }
}
