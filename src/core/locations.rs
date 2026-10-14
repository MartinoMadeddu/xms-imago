//! Primitive Locations: the primitives an operator works on, as a list of
//! paths in the operator's Prims Location tab (as a Solaris LOP's
//! Primitives parameter).
//!
//! - A location matches the primitive at that path and every primitive
//!   below it: `/DeLorean/wheels` takes every wheel.
//! - `*` stands for any run of characters within one name, `?` for one
//!   character, and `**` for any number of levels: `/set/**/lamp*`.
//! - `/` matches everything. The leading `/` may be left out.
//! - An empty list (or one of empty rows only) means every primitive: the
//!   operator works as it did without a list, on what an upstream Pick
//!   Primitives chose, or on everything.

use std::collections::HashMap;

use crate::types::{EvalResult, NodeType};

/// Whether an operator has a Prims Location tab.
pub fn takes_locations(t: &NodeType) -> bool {
    matches!(t, NodeType::Transform { .. } | NodeType::EditPoly { .. } | NodeType::PrunePrims { .. })
}

/// The rows that say something: trimmed, blank ones left out.
pub fn rows(list: &[String]) -> impl Iterator<Item = &str> {
    list.iter().map(|s| s.trim()).filter(|s| !s.is_empty())
}

/// Whether the list names any location. Empty: every primitive.
pub fn is_set(list: &[String]) -> bool { rows(list).next().is_some() }

/// Whether one location matches a primitive path: the primitive at it, or
/// any below it.
pub fn matches(location: &str, path: &str) -> bool {
    let pattern: Vec<&str> = location.split('/').map(str::trim).filter(|s| !s.is_empty()).collect();
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    prefix(&pattern, &segments)
}

/// Whether any location of the list matches the path.
pub fn any_matches(list: &[String], path: &str) -> bool {
    rows(list).any(|l| matches(l, path))
}

/// The pattern consumes a leading part of the path: what is left of the path
/// is below the location, and taken with it.
fn prefix(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => true,
        Some((&"**", rest)) => (0..=path.len()).any(|k| prefix(rest, &path[k..])),
        Some((first, rest)) => !path.is_empty() && glob(first, path[0]) && prefix(rest, &path[1..]),
    }
}

/// One name against one pattern name: `*` any run, `?` one character.
fn glob(pattern: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) = (pattern.chars().collect(), name.chars().collect());
    // Positions of the pattern reachable after each character of the name.
    let mut at = vec![false; p.len() + 1];
    at[0] = true;
    let star_closure = |at: &mut Vec<bool>| for i in 0..p.len() { if at[i] && p[i] == '*' { at[i + 1] = true; } };
    star_closure(&mut at);
    for c in n {
        let mut next = vec![false; p.len() + 1];
        for i in 0..p.len() {
            if !at[i] { continue; }
            match p[i] {
                '*' => next[i] = true,
                '?' => next[i + 1] = true,
                x if x == c => next[i + 1] = true,
                _ => {}
            }
        }
        star_closure(&mut next);
        at = next;
    }
    at[p.len()]
}

/// The primitives an operator with this list works on: the ones the list
/// matches are picked, the others not. Without a list, or for what is not
/// primitives, the input as it is.
pub fn target(input: &EvalResult, list: &[String]) -> EvalResult {
    match input {
        EvalResult::Named(prims) if is_set(list) => EvalResult::Named(prims.iter()
            .map(|p| crate::types::Prim { picked: any_matches(list, &p.path), ..p.clone() })
            .collect()),
        other => other.clone(),
    }
}

/// The picks of `input` put back on what an operator gave: the list of one
/// operator does not reach the operators after it.
pub fn restore_picks(output: EvalResult, input: &EvalResult) -> EvalResult {
    let EvalResult::Named(before) = input else { return output };
    let picked: HashMap<&str, bool> = before.iter().map(|p| (p.path.as_str(), p.picked)).collect();
    match output {
        EvalResult::Named(prims) => EvalResult::Named(prims.into_iter()
            .map(|p| crate::types::Prim { picked: picked.get(p.path.as_str()).copied().unwrap_or(false), ..p })
            .collect()),
        other => other,
    }
}

/// Prune by a list: the primitives it matches removed, or kept alone.
pub fn prune(input: &EvalResult, list: &[String], keep: bool) -> EvalResult {
    match input {
        EvalResult::Named(prims) => EvalResult::Named(prims.iter()
            .filter(|p| any_matches(list, &p.path) == keep).cloned().collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MeshData, Prim};

    #[test]
    fn a_location_takes_its_primitive_and_everything_below() {
        assert!(matches("/DeLorean/wheels", "/DeLorean/wheels"));
        assert!(matches("/DeLorean/wheels", "/DeLorean/wheels/front_L"));
        assert!(!matches("/DeLorean/wheels", "/DeLorean/wheelsCover"), "a whole name, not a start");
        assert!(!matches("/DeLorean/wheels/front_L", "/DeLorean/wheels"), "not what is above");
        assert!(matches("DeLorean/wheels/", "/DeLorean/wheels/front_L"), "leading and trailing / optional");
        assert!(matches("/", "/anything/at/all"));
    }

    #[test]
    fn wildcards_stay_in_a_name_and_two_stars_cross_levels() {
        assert!(matches("/DeLorean/wheel_*", "/DeLorean/wheel_front_L"));
        assert!(!matches("/DeLorean/wheel_*", "/DeLorean/body/wheel_front_L"), "* stays within one name");
        assert!(matches("/DeLorean/**/wheel_*", "/DeLorean/body/wheel_front_L"));
        assert!(matches("/DeLorean/**/wheel_*", "/DeLorean/wheel_front_L"), "** may be no level at all");
        assert!(matches("/**/lamp?", "/street/a/b/lamp1/bulb"));
        assert!(!matches("/**/lamp?", "/street/lamp12"));
        assert!(matches("/set/*", "/set/a"));
        assert!(!matches("/set/*", "/set"), "* is one name: nothing to match at /set itself");
    }

    #[test]
    fn a_list_picks_what_it_matches_and_the_picks_go_back_after() {
        let prims = |paths: &[&str]| EvalResult::Named(paths.iter().map(|p| Prim::new(p.to_string(), MeshData::default())).collect());
        let input = prims(&["/car/body", "/car/wheel_L", "/car/wheel_R", "/cube"]);
        let list = vec!["/car/wheel_*".to_string(), "  ".to_string()];
        let EvalResult::Named(t) = target(&input, &list) else { panic!() };
        assert_eq!(t.iter().map(|p| p.picked).collect::<Vec<_>>(), vec![false, true, true, false]);
        // Empty: as it came.
        assert!(!is_set(&[" ".to_string()]));
        let EvalResult::Named(same) = target(&input, &[]) else { panic!() };
        assert!(same.iter().all(|p| !p.picked));
        // After the operator, the picks are those that came in.
        let EvalResult::Named(back) = restore_picks(EvalResult::Named(t), &input) else { panic!() };
        assert!(back.iter().all(|p| !p.picked));
        // Prune by the list.
        assert_eq!(prune(&input, &list, false).prim_paths(), vec!["/car/body", "/cube"]);
        assert_eq!(prune(&input, &list, true).prim_paths(), vec!["/car/wheel_L", "/car/wheel_R"]);
    }
}
