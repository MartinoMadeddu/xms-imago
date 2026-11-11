//! Delete Attribute and Rename Attribute: attributes (primvars) of the
//! primitives' geometry taken away, or given new names, by name. On the
//! primitives the Prims Location tab names (picked), or every one.
//!
//! Write USD writes an attribute taken away as blocked (`= None`), so the
//! file's value is not seen; one renamed as the new name written and the old
//! one blocked. `points` is never taken away or renamed: it is the geometry.

use std::sync::Arc;

use crate::core::geo::{Context, Geo, POINTS};
use crate::core::geo_mesh::GeoMesh;
use crate::types::{EvalResult, Prim};

/// Whether a name matches one word of a pattern: `*` any run of characters,
/// `?` any one.
fn wild(pattern: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) = (pattern.chars().collect(), name.chars().collect());
    // Positions of the pattern reachable after each character of the name.
    let mut at = vec![false; p.len() + 1];
    at[0] = true;
    let close = |at: &mut Vec<bool>| {
        for i in 0..p.len() {
            if at[i] && p[i] == '*' {
                at[i + 1] = true;
            }
        }
    };
    close(&mut at);
    for c in n {
        let mut next = vec![false; p.len() + 1];
        for i in 0..p.len() {
            if at[i] {
                match p[i] {
                    '*' => next[i] = true,
                    '?' => next[i + 1] = true,
                    x if x == c => next[i + 1] = true,
                    _ => {}
                }
            }
        }
        close(&mut next);
        at = next;
    }
    at[p.len()]
}

/// The words of a list of names: split on spaces and commas.
pub fn words(text: &str) -> Vec<&str> {
    text.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty())
        .collect()
}

/// Whether a name is one the pattern names: any word matches it (`uv*`), and
/// no word after `^` does (`^uvMask`).
pub fn names(pattern: &str, name: &str) -> bool {
    let w = words(pattern);
    w.iter().any(|x| !x.starts_with('^') && wild(x, name))
        && !w.iter().any(|x| x.strip_prefix('^').is_some_and(|x| wild(x, name)))
}

fn in_context(only: Option<Context>, ctx: Context) -> bool {
    only.map_or(true, |c| c == ctx)
}

/// The geometry without the attributes the pattern names (in `only`, or any
/// context); `None` when none goes.
pub fn deleted(geo: &Geo, pattern: &str, only: Option<Context>) -> Option<Geo> {
    let gone: Vec<(Context, String)> = Context::ALL
        .into_iter()
        .filter(|c| in_context(only, *c))
        .flat_map(|c| geo.attrs(c).map(move |(n, _)| (c, n.to_string())))
        .filter(|(_, n)| n != POINTS && names(pattern, n))
        .collect();
    if gone.is_empty() {
        return None;
    }
    let mut g = geo.clone();
    for (c, n) in gone {
        g.remove(c, &n);
    }
    Some(g)
}

/// The pairs of a rename: each word of `from` and the word of `to` in its
/// place. A `*` in both keeps what it matched (`uv*` to `st*`: `uv1` is
/// `st1`).
pub fn pairs<'a>(from: &'a str, to: &'a str) -> Vec<(&'a str, &'a str)> {
    words(from).into_iter().zip(words(to)).collect()
}

/// A name's new name by a pair, when the pair names it.
fn renamed(from: &str, to: &str, name: &str) -> Option<String> {
    match (from.split_once('*'), to.split_once('*')) {
        (Some((a, b)), Some((c, d))) if !b.contains('*') && !d.contains('*') => {
            let mid = name.strip_prefix(a)?.strip_suffix(b)?;
            (name.len() >= a.len() + b.len()).then(|| format!("{c}{mid}{d}"))
        }
        _ => wild(from, name).then(|| to.to_string()),
    }
}

/// The geometry with the attributes `from` names given `to`'s names (in
/// `only`, or any context). One already of the new name is replaced, in any
/// context: USD has one attribute of a name. `None` when none is renamed.
pub fn renamed_geo(geo: &Geo, from: &str, to: &str, only: Option<Context>) -> Option<Geo> {
    let pairs = pairs(from, to);
    let mut moves: Vec<(Context, String, String)> = vec![];
    for c in Context::ALL.into_iter().filter(|c| in_context(only, *c)) {
        for (n, _) in geo.attrs(c) {
            if n == POINTS {
                continue;
            }
            let new = pairs.iter().find_map(|(f, t)| renamed(f, t, n));
            if let Some(new) = new.filter(|x| !x.is_empty() && x != n && x != POINTS) {
                moves.push((c, n.to_string(), new));
            }
        }
    }
    if moves.is_empty() {
        return None;
    }
    let mut g = geo.clone();
    let taken: Vec<(Context, String, crate::core::geo::Attr)> = moves
        .iter()
        .filter_map(|(c, n, new)| g.remove(*c, n).map(|a| (*c, new.clone(), a)))
        .collect();
    for (c, new, a) in taken {
        for other in Context::ALL {
            g.remove(other, &new);
        }
        g.set(c, &new, a);
    }
    Some(g)
}

/// `edit` made on each primitive's geometry (the picked ones, when some are).
fn on_prims(input: &EvalResult, edit: &dyn Fn(&Geo) -> Option<Geo>) -> EvalResult {
    match input {
        EvalResult::Named(prims) => {
            let any_picked = prims.iter().any(|p| p.picked);
            EvalResult::Named(
                prims
                    .iter()
                    .map(|p| {
                        if any_picked && !p.picked {
                            return p.clone();
                        }
                        let geo = p.geo.as_deref().cloned().unwrap_or_else(|| Geo::from_mesh(&p.mesh));
                        match edit(&geo) {
                            Some(g) => Prim {
                                mesh: Arc::new(g.to_mesh()),
                                geo: Some(Arc::new(g)),
                                ..p.clone()
                            },
                            None => p.clone(),
                        }
                    })
                    .collect(),
            )
        }
        EvalResult::Single(m) => match edit(&Geo::from_mesh(m)) {
            Some(g) => EvalResult::Single(g.to_mesh()),
            None => input.clone(),
        },
        other => other.clone(),
    }
}

/// Delete Attribute.
pub fn delete(input: &EvalResult, pattern: &str, only: Option<Context>) -> EvalResult {
    on_prims(input, &|g| deleted(g, pattern, only))
}

/// Rename Attribute.
pub fn rename(input: &EvalResult, from: &str, to: &str, only: Option<Context>) -> EvalResult {
    on_prims(input, &|g| renamed_geo(g, from, to, only))
}

/// The attributes of the primitives coming in that `edit` would change, by
/// name (the primitives' count each): what the panels list.
pub fn found(input: &EvalResult, edit: &dyn Fn(&Geo) -> Vec<String>) -> Vec<(String, usize)> {
    let mut out: std::collections::BTreeMap<String, usize> = Default::default();
    if let EvalResult::Named(prims) = input {
        let any_picked = prims.iter().any(|p| p.picked);
        for g in prims
            .iter()
            .filter(|p| !any_picked || p.picked)
            .filter_map(|p| p.geo.as_deref())
        {
            for n in edit(g) {
                *out.entry(n).or_default() += 1;
            }
        }
    }
    out.into_iter().collect()
}

/// What Delete Attribute takes from one geometry.
pub fn would_delete(g: &Geo, pattern: &str, only: Option<Context>) -> Vec<String> {
    Context::ALL
        .into_iter()
        .filter(|c| in_context(only, *c))
        .flat_map(|c| g.attrs(c).map(|(n, _)| n.to_string()).collect::<Vec<_>>())
        .filter(|n| n != POINTS && names(pattern, n))
        .collect()
}

/// What Rename Attribute renames in one geometry: `old → new`.
pub fn would_rename(g: &Geo, from: &str, to: &str, only: Option<Context>) -> Vec<String> {
    let pairs = pairs(from, to);
    Context::ALL
        .into_iter()
        .filter(|c| in_context(only, *c))
        .flat_map(|c| g.attrs(c).map(|(n, _)| n.to_string()).collect::<Vec<_>>())
        .filter(|n| n != POINTS)
        .filter_map(|n| {
            let new = pairs.iter().find_map(|(f, t)| renamed(f, t, &n))?;
            (!new.is_empty() && new != n && new != POINTS).then(|| format!("{n} → {new}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geo::{Attr, Column, Role};

    fn geo() -> Geo {
        let mut g = Geo::from_points(vec![[0.0; 3], [1.0, 0.0, 0.0]]);
        let floats = |n: usize| Attr::new(Column::Float(Arc::new(vec![0.5; n])), Role::None);
        g.set(Context::Point, "uv1", floats(2));
        g.set(Context::Point, "uvMask", floats(2));
        g.set(Context::Point, "Cd", floats(2));
        g.set(Context::Object, "Cd", floats(1));
        g
    }

    fn has(g: &Geo, ctx: Context, name: &str) -> bool {
        g.attr(ctx, name).is_some()
    }

    #[test]
    fn names_are_words_with_wildcards_and_exceptions() {
        assert!(names("uv*", "uv1") && names("uv*", "uvMask"));
        assert!(!names("uv* ^uvMask", "uvMask") && names("uv*,^uvMask", "uv1"));
        assert!(!names("uv", "uv1"), "a whole name, not a part");
        assert!(names("u?1", "uv1"));
    }

    #[test]
    fn deleting_takes_what_is_named_never_the_points() {
        let g = deleted(&geo(), "* ^uvMask", None).unwrap();
        assert!(has(&g, Context::Point, POINTS) && has(&g, Context::Point, "uvMask"));
        assert!(!has(&g, Context::Point, "uv1") && !has(&g, Context::Object, "Cd"));
        let g = deleted(&geo(), "Cd", Some(Context::Object)).unwrap();
        assert!(has(&g, Context::Point, "Cd") && !has(&g, Context::Object, "Cd"));
        assert!(deleted(&geo(), "nothing", None).is_none());
    }

    #[test]
    fn renaming_pairs_names_and_keeps_what_a_star_matched() {
        let g = renamed_geo(&geo(), "uv*", "st*", None).unwrap();
        assert!(has(&g, Context::Point, "st1") && has(&g, Context::Point, "stMask"));
        let g = renamed_geo(&geo(), "uv1 Cd", "map1 displayColor", Some(Context::Point)).unwrap();
        assert!(has(&g, Context::Point, "map1") && has(&g, Context::Point, "displayColor"));
        assert!(has(&g, Context::Object, "Cd"), "another context's stays");
        // The new name replaces one there was, in any context.
        let g = renamed_geo(&geo(), "uv1", "Cd", Some(Context::Point)).unwrap();
        assert!(has(&g, Context::Point, "Cd") && !has(&g, Context::Object, "Cd"));
        assert!(renamed_geo(&geo(), "points", "P", None).is_none());
    }

    #[test]
    fn only_the_picked_change_when_some_are() {
        let p = |path: &str, picked: bool| Prim {
            mesh: Arc::new(geo().to_mesh()),
            geo: Some(Arc::new(geo())),
            picked,
            ..Prim::new(path.into(), Default::default())
        };
        let EvalResult::Named(out) = delete(&EvalResult::Named(vec![p("/a", true), p("/b", false)]), "uv1", None)
        else {
            panic!()
        };
        assert!(!has(out[0].geo.as_ref().unwrap(), Context::Point, "uv1"));
        assert!(has(out[1].geo.as_ref().unwrap(), Context::Point, "uv1"));
    }
}
