//! Collections: named sets of prims, as USD's `CollectionAPI` has them. A
//! Collection operator makes one on a prim (by default `/collections`), and
//! collections in the files loaded are read too. Any Prims Location row
//! `%name` then takes the collection's members.
//!
//! - Includes and excludes are paths or patterns (`*`, `?`, `**`). Patterns
//!   are matched against the scene coming in when the collection is made, so
//!   the collection holds paths, as USD writes it (`rel …:includes`).
//! - Expand Prims (the default): an include takes its prim and everything
//!   below it, an exclude takes its prim and everything below it out. Explicit
//!   Only: the prims named, and only those.
//! - `%other` in a collection's rows takes the other collection's includes and
//!   excludes into this one.
//! - Two collections of one name (on different prims): `%name` takes both.
//!
//! In a Prims Location list, `%name` stands for a row of its own form,
//! `{expand: /a /b -/a/c}` (`locations::matches`): made here from the scene's
//! collections before an operator reads its list (`resolved`).

use std::borrow::Cow;
use std::sync::Arc;

use crate::core::locations;
use crate::types::{EvalResult, MeshData, Prim};

/// The Collection Location: where a Collection operator puts its collection
/// unless told.
pub const DEFAULT_LOCATION: &str = "/collections";

/// One collection: its name, and the paths it includes and excludes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Collection {
    pub name: String,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    /// Explicit Only: the prims named alone, not what is below them.
    pub explicit_only: bool,
}

impl Collection {
    /// Its `expansionRule` token.
    pub fn rule(&self) -> &'static str {
        if self.explicit_only {
            "explicitOnly"
        } else {
            "expandPrims"
        }
    }

    /// The row a `%name` stands for in a Prims Location list.
    pub fn row(&self) -> String {
        let mut out = String::from(if self.explicit_only { "{explicit:" } else { "{expand:" });
        for p in &self.includes {
            out.push(' ');
            out.push_str(p);
        }
        for p in &self.excludes {
            out.push_str(" -");
            out.push_str(p);
        }
        out.push('}');
        out
    }

    /// Whether the prim at `path` is a member.
    pub fn has(&self, path: &str) -> bool {
        member(self.explicit_only, &self.includes, &self.excludes, path)
    }
}

/// A collection and the prim that holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Held {
    pub holder: String,
    pub collection: Collection,
}

// Rows of the `%name` form

/// A row `{expand: …}` or `{explicit: …}`, read: whether explicit only, its
/// includes and its excludes. `None` for any other row.
pub fn parse_row(row: &str) -> Option<(bool, Vec<&str>, Vec<&str>)> {
    let inner = row.trim().strip_prefix('{')?.strip_suffix('}')?;
    let (mode, rest) = inner.split_once(':')?;
    let explicit = match mode.trim() {
        "expand" => false,
        "explicit" => true,
        _ => return None,
    };
    let (mut inc, mut exc) = (vec![], vec![]);
    for word in rest.split_whitespace() {
        match word.strip_prefix('-') {
            Some(p) => exc.push(p),
            None => inc.push(word),
        }
    }
    Some((explicit, inc, exc))
}

/// Whether a path is a member: under an include (Expand Prims) and under no
/// exclude, or (Explicit Only) named by an include and by no exclude.
fn member<S: AsRef<str>>(explicit: bool, includes: &[S], excludes: &[S], path: &str) -> bool {
    let at_or_below = |p: &str| {
        let p = p.trim_end_matches('/');
        p.is_empty() || path == p || path.strip_prefix(p).is_some_and(|r| r.starts_with('/'))
    };
    if explicit {
        includes.iter().any(|i| i.as_ref() == path) && !excludes.iter().any(|e| e.as_ref() == path)
    } else {
        includes.iter().any(|i| at_or_below(i.as_ref())) && !excludes.iter().any(|e| at_or_below(e.as_ref()))
    }
}

/// Whether a row of the `{…}` form takes a path: its members. Others: `None`.
pub fn row_has(row: &str, path: &str) -> Option<bool> {
    let (explicit, inc, exc) = parse_row(row)?;
    Some(!path.is_empty() && member(explicit, &inc, &exc, path))
}

// The scene's collections

/// Every collection of the scene: those of the files loaded (their stages'
/// prims), then those Imago made, which win over a file's of the same name on
/// the same prim.
pub fn gather(prims: &[Prim]) -> Vec<Held> {
    let mut out: Vec<Held> = vec![];
    let mut put = |holder: &str, c: &Collection| match out
        .iter_mut()
        .find(|h| h.holder == holder && h.collection.name == c.name)
    {
        Some(h) => h.collection = c.clone(),
        None => out.push(Held {
            holder: holder.to_string(),
            collection: c.clone(),
        }),
    };
    let mut trees: Vec<&Arc<crate::usd_scene::StageTree>> = vec![];
    for p in prims {
        if let Some(t) = &p.stage {
            if !trees.iter().any(|x| Arc::ptr_eq(x, t)) {
                trees.push(t);
            }
        }
    }
    for n in trees.iter().flat_map(|t| t.nodes.iter()) {
        for c in &n.collections {
            put(&n.path, c);
        }
    }
    for p in prims {
        for c in p.collections.iter().flat_map(|v| v.iter()) {
            put(&p.path, c);
        }
    }
    out
}

/// The scene's collections, from what an operator gets.
pub fn gather_in(input: Option<&EvalResult>) -> Vec<Held> {
    match input {
        Some(EvalResult::Named(prims)) => gather(prims),
        _ => vec![],
    }
}

/// Whether a row is `%name`: the name.
pub fn reference(row: &str) -> Option<&str> {
    row.trim().strip_prefix('%').map(str::trim).filter(|n| !n.is_empty())
}

/// The row `%name` stands for: every collection of that name as one (none:
/// a row taking nothing).
pub fn row_for(name: &str, held: &[Held]) -> String {
    let found: Vec<&Collection> = held.iter().map(|h| &h.collection).filter(|c| c.name == name).collect();
    match found.as_slice() {
        [] => "{expand:}".to_string(),
        [one] => one.row(),
        // Several, as one: every include, and every exclude for all of them.
        many => Collection {
            name: name.to_string(),
            includes: many.iter().flat_map(|c| c.includes.iter().cloned()).collect(),
            excludes: many.iter().flat_map(|c| c.excludes.iter().cloned()).collect(),
            explicit_only: many.iter().all(|c| c.explicit_only),
        }
        .row(),
    }
}

/// A Prims Location list with each `%name` row put in as the row it stands
/// for. Borrowed when it has none.
pub fn resolve<'a>(list: &'a [String], held: &[Held]) -> Cow<'a, [String]> {
    if !list.iter().any(|r| reference(r).is_some()) {
        return Cow::Borrowed(list);
    }
    Cow::Owned(
        list.iter()
            .map(|r| match reference(r) {
                Some(name) => row_for(name, held),
                None => r.clone(),
            })
            .collect(),
    )
}

/// A list with its `%name` rows put in from the scene coming in.
pub fn resolved<'a>(list: &'a [String], prims: &[Prim]) -> Cow<'a, [String]> {
    if !list.iter().any(|r| reference(r).is_some()) {
        return Cow::Borrowed(list);
    }
    resolve(list, &gather(prims))
}

/// The same, from what an operator gets.
pub fn resolved_in<'a>(list: &'a [String], input: Option<&EvalResult>) -> Cow<'a, [String]> {
    if !list.iter().any(|r| reference(r).is_some()) {
        return Cow::Borrowed(list);
    }
    resolve(list, &gather_in(input))
}

// The operator

/// What a Collection operator's rows say, before they are matched.
pub struct Made<'a> {
    pub name: &'a str,
    /// Its Collection Location: the prim holding it.
    pub location: &'a str,
    pub includes: &'a [String],
    pub excludes: &'a [String],
    pub explicit_only: bool,
}

/// A collection name as USD takes it: letters, digits and `_`, not starting
/// with a digit.
pub fn clean_name(name: &str) -> String {
    let mut out: String = name
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// Rows as paths: a path as it is, a pattern as every prim of the scene it
/// names (`known`), `%other` as that collection's rows (`own` takes its
/// includes, else its excludes). For Expand Prims, a path below another is
/// left out.
fn paths(rows: &[String], known: &[String], held: &[Held], explicit: bool, own: bool) -> (Vec<String>, Vec<String>) {
    let (mut main, mut other) = (vec![], vec![]);
    let push = |v: &mut Vec<String>, p: String| {
        if !v.contains(&p) {
            v.push(p);
        }
    };
    for row in locations::rows(rows) {
        if let Some(name) = reference(row) {
            for c in held.iter().map(|h| &h.collection).filter(|c| c.name == name) {
                for p in &c.includes {
                    push(if own { &mut main } else { &mut other }, p.clone());
                }
                for p in &c.excludes {
                    push(if own { &mut other } else { &mut main }, p.clone());
                }
            }
        } else if row.contains(['*', '?']) {
            for p in known.iter().filter(|p| locations::names(row, p)) {
                push(&mut main, p.clone());
            }
        } else {
            let p = row.trim_start_matches('/').trim_end_matches('/');
            push(&mut main, format!("/{p}"));
        }
    }
    if !explicit {
        main = topmost(main);
    }
    (main, other)
}

/// Paths without those below another of them.
fn topmost(paths: Vec<String>) -> Vec<String> {
    let below = |p: &str, of: &str| of == "/" || p.strip_prefix(of).is_some_and(|r| r.starts_with('/'));
    paths
        .iter()
        .filter(|p| !paths.iter().any(|o| o != *p && below(p, o)))
        .cloned()
        .collect()
}

/// The collection a Collection operator makes, from the scene coming in.
pub fn collection(m: &Made, prims: &[Prim]) -> Option<Collection> {
    let name = clean_name(&expand(m.name));
    if name.is_empty() {
        return None;
    }
    let known = crate::node_graph::scene_paths(prims);
    let held = gather(prims);
    let (mut includes, from_inc) = paths(m.includes, &known, &held, m.explicit_only, true);
    let (mut excludes, from_exc) = paths(m.excludes, &known, &held, m.explicit_only, false);
    // What `%other` brings: its excludes go with this one's, and the reverse.
    for p in from_inc {
        if !excludes.contains(&p) {
            excludes.push(p);
        }
    }
    for p in from_exc {
        if !includes.contains(&p) {
            includes.push(p);
        }
    }
    if !m.explicit_only {
        includes = topmost(includes);
        excludes = topmost(excludes);
    }
    Some(Collection {
        name,
        includes,
        excludes,
        explicit_only: m.explicit_only,
    })
}

/// Its variables put in (`$OPNAME`).
fn expand(text: &str) -> String {
    if crate::core::vars::has_vars(text) {
        crate::core::vars::expand(text, &[])
    } else {
        text.to_string()
    }
}

/// The prim holding collections a Collection operator makes, where no prim of
/// the scene is: a Scope (an `over` where a stage has the prim).
fn holder_prim(path: &str) -> Prim {
    Prim::new(path.to_string(), MeshData::default())
}

/// A Collection Location as used: its variables put in, absolute, without a
/// trailing `/`.
pub fn holder_path(holder: &str) -> String {
    let h = expand(holder);
    let h = h.trim().trim_end_matches('/');
    match h {
        "" => DEFAULT_LOCATION.to_string(),
        h if h.starts_with('/') => h.to_string(),
        h => format!("/{h}"),
    }
}

/// What a Collection operator puts out: the scene coming in with the
/// collection on its holder, the prim there (one made where there is none).
/// One of the same name on it is replaced.
pub fn make(input: &EvalResult, m: &Made) -> EvalResult {
    let EvalResult::Named(prims) = input else {
        return input.clone();
    };
    let Some(c) = collection(m, prims) else {
        return input.clone();
    };
    let holder = holder_path(m.location);
    let mut out = prims.clone();
    let k = match out.iter().position(|p| p.path == holder && p.instanced_by.is_none()) {
        Some(k) => k,
        None => {
            out.push(holder_prim(&holder));
            out.len() - 1
        }
    };
    let p = &mut out[k];
    let mut list: Vec<Collection> = p.collections.as_deref().cloned().unwrap_or_default();
    match list.iter_mut().find(|x| x.name == c.name) {
        Some(x) => *x = c,
        None => list.push(c),
    }
    p.collections = Some(Arc::new(list));
    EvalResult::Named(out)
}

// USD

/// A prim's collections as `.usda`: the API schema each applies, and each
/// one's expansion rule, includes and excludes. The whole root as an include
/// is `includeRoot`.
pub fn usda_lines(c: &Collection) -> Vec<String> {
    let n = &c.name;
    let mut lines = vec![format!("uniform token collection:{n}:expansionRule = \"{}\"", c.rule())];
    let root = c.includes.iter().any(|p| p == "/" || p.is_empty());
    if root {
        lines.push(format!("uniform bool collection:{n}:includeRoot = 1"));
    }
    let targets = |v: &[String]| {
        let t: Vec<String> = v
            .iter()
            .filter(|p| !p.is_empty() && *p != "/")
            .map(|p| format!("<{p}>"))
            .collect();
        match t.len() {
            0 => "None".to_string(),
            1 => t[0].clone(),
            _ => format!("[{}]", t.join(", ")),
        }
    };
    lines.push(format!("rel collection:{n}:includes = {}", targets(&c.includes)));
    lines.push(format!("rel collection:{n}:excludes = {}", targets(&c.excludes)));
    lines
}

/// Its API schema's name.
pub fn api(c: &Collection) -> String {
    format!("CollectionAPI:{}", c.name)
}

/// A collection read from a file: its rule's token, includes, excludes and
/// whether it includes the root.
pub fn from_file(
    name: &str,
    rule: &str,
    includes: Vec<String>,
    excludes: Vec<String>,
    include_root: bool,
) -> Collection {
    let mut includes = includes;
    if include_root && !includes.iter().any(|p| p == "/") {
        includes.insert(0, "/".into());
    }
    Collection {
        name: name.to_string(),
        includes,
        excludes,
        explicit_only: rule == "explicitOnly",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prim(path: &str) -> Prim {
        Prim::new(path.to_string(), MeshData::default())
    }

    fn scene(paths: &[&str]) -> Vec<Prim> {
        paths.iter().map(|p| prim(p)).collect()
    }

    fn rows(r: &[&str]) -> Vec<String> {
        r.iter().map(|s| s.to_string()).collect()
    }

    fn made<'a>(name: &'a str, inc: &'a [String], exc: &'a [String]) -> Made<'a> {
        Made {
            name,
            location: DEFAULT_LOCATION,
            includes: inc,
            excludes: exc,
            explicit_only: false,
        }
    }

    #[test]
    fn a_collection_holds_paths_its_patterns_matched() {
        let prims = scene(&["/set/lamp_a", "/set/lamp_b", "/set/table", "/hero/body"]);
        let inc = rows(&["/set/lamp*", "/hero"]);
        let exc = rows(&["/set/lamp_b"]);
        let c = collection(&made("lamps", &inc, &exc), &prims).unwrap();
        assert_eq!(c.includes, rows(&["/set/lamp_a", "/set/lamp_b", "/hero"]));
        assert_eq!(c.excludes, rows(&["/set/lamp_b"]));
        assert!(c.has("/set/lamp_a") && c.has("/hero/body"));
        assert!(!c.has("/set/lamp_b") && !c.has("/set/table"));
    }

    #[test]
    fn expand_prims_takes_what_is_below_explicit_only_does_not() {
        let mut c = Collection {
            name: "c".into(),
            includes: rows(&["/a"]),
            excludes: rows(&["/a/b"]),
            explicit_only: false,
        };
        assert!(c.has("/a") && c.has("/a/x/y") && !c.has("/a/b/z") && !c.has("/ab"));
        c.explicit_only = true;
        assert!(c.has("/a") && !c.has("/a/x"));
    }

    #[test]
    fn a_collection_goes_on_its_holder_made_where_there_is_none() {
        let prims = scene(&["/set/lamp"]);
        let inc = rows(&["/set"]);
        let EvalResult::Named(out) = make(&EvalResult::Named(prims), &made("all", &inc, &[])) else {
            panic!()
        };
        let h = out.iter().find(|p| p.path == "/collections").unwrap();
        assert!(h.is_holder() && !h.is_geometry());
        assert_eq!(h.collections.as_ref().unwrap()[0].includes, rows(&["/set"]));
        // A second of the same name replaces it; another name joins it.
        let inc2 = rows(&["/set/lamp"]);
        let EvalResult::Named(out) = make(&EvalResult::Named(out), &made("all", &inc2, &[])) else {
            panic!()
        };
        let EvalResult::Named(out) = make(&EvalResult::Named(out), &made("more", &inc, &[])) else {
            panic!()
        };
        let h = out.iter().find(|p| p.path == "/collections").unwrap();
        let names: Vec<&str> = h
            .collections
            .as_ref()
            .unwrap()
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, ["all", "more"]);
        assert_eq!(h.collections.as_ref().unwrap()[0].includes, inc2);
    }

    #[test]
    fn percent_name_in_a_location_list_takes_the_members() {
        let prims = scene(&["/set/a", "/set/b", "/other"]);
        let inc = rows(&["/set"]);
        let exc = rows(&["/set/b"]);
        let EvalResult::Named(out) = make(&EvalResult::Named(prims), &made("pick", &inc, &exc)) else {
            panic!()
        };
        let list = rows(&["%pick"]);
        let r = resolved(&list, &out);
        assert!(locations::is_set(&r));
        assert!(locations::any_matches(&r, "/set/a"));
        assert!(!locations::any_matches(&r, "/set/b"));
        assert!(!locations::any_matches(&r, "/other"));
        assert!(locations::any_names(&r, "/set"));
        // An unknown collection takes nothing, and still is a list.
        let none = rows(&["%nothing"]);
        let r = resolved(&none, &out);
        assert!(locations::is_set(&r) && !locations::any_matches(&r, "/set/a"));
        // A list without one is borrowed.
        let plain = rows(&["/set"]);
        assert!(matches!(resolved(&plain, &out), Cow::Borrowed(_)));
    }

    #[test]
    fn a_collection_takes_another_by_percent_name() {
        let prims = scene(&["/set/a", "/set/b", "/x"]);
        let inc = rows(&["/set"]);
        let exc = rows(&["/set/b"]);
        let EvalResult::Named(out) = make(&EvalResult::Named(prims), &made("one", &inc, &exc)) else {
            panic!()
        };
        let inc2 = rows(&["%one", "/x"]);
        let EvalResult::Named(out) = make(&EvalResult::Named(out), &made("two", &inc2, &[])) else {
            panic!()
        };
        let two = gather(&out)
            .into_iter()
            .find(|h| h.collection.name == "two")
            .unwrap()
            .collection;
        assert!(two.has("/set/a") && two.has("/x") && !two.has("/set/b"));
    }

    #[test]
    fn names_are_made_usd_names() {
        assert_eq!(clean_name(" hero lights "), "hero_lights");
        assert_eq!(clean_name("2d"), "_2d");
    }

    #[test]
    fn written_with_its_rule_includes_and_excludes() {
        let c = Collection {
            name: "lamps".into(),
            includes: rows(&["/set/a", "/set/b"]),
            excludes: vec![],
            explicit_only: false,
        };
        assert_eq!(
            usda_lines(&c),
            [
                "uniform token collection:lamps:expansionRule = \"expandPrims\"",
                "rel collection:lamps:includes = [</set/a>, </set/b>]",
                "rel collection:lamps:excludes = None",
            ]
        );
        let all = from_file("everything", "expandPrims", vec![], rows(&["/x"]), true);
        assert!(all.has("/anything") && !all.has("/x/y"));
        assert!(usda_lines(&all).contains(&"uniform bool collection:everything:includeRoot = 1".to_string()));
    }
}
