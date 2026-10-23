//! Variables: `$NAME` in the text Imago reads (file paths, command lines),
//! put in when the text is used.
//!
//! - **Local** variables belong to what is reading the text (a Render
//!   Command's `$USD`, the Image Viewer's `$IMAGE`), and come first.
//! - **Global** variables are the same everywhere in the graph: the graph's
//!   file, the project, the frames (`GLOBALS` lists them). `$OPNAME` is
//!   the operator cooking (or whose panel is drawn), wherever its text is
//!   read: see `operator`.
//! - The graph's **own** variables (File > Variables), saved with it; a
//!   value can hold variables too (`ASSETS` = `$JOB/assets`).
//! - Then the **environment** (`$HOME`, `$USER`, anything set).
//!
//! `${NAME}` sets a name apart from what follows (`${F4}_v2`); `$$` is a
//! `$`. A name that is none of these is left as it is, so a command line
//! keeps the shell's own (`$1`). Text passed on to a renderer as it is
//! (Render Settings' Output Image, which the renderer expands) is never
//! read through here.

use std::sync::RwLock;

/// What the global variables are, set from the program's state each frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Globals {
    /// The graph's file without its extension (`street_v001`), and its
    /// folder. Empty for a graph never saved.
    pub gname: String,
    pub gdir:  String,
    /// The playhead's frame, and the scene's range and rate.
    pub frame: i64,
    pub start: i64,
    pub end:   i64,
    pub fps:   f64,
    /// The graph's own variables and `$JOB`, as File > Variables has them.
    pub user:  Variables,
}

/// The graph's own variables, saved in its file (File > Variables).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Variables {
    /// `$JOB`, the project folder. Empty: `JOB` from the environment, else
    /// `$GDIR`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub job:  String,
    /// In the order they were made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub list: Vec<Variable>,
}

impl Variables {
    pub fn is_empty(&self) -> bool { self.job.is_empty() && self.list.is_empty() }
}

/// One of the graph's own variables.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(from = "SavedVariable")]
pub struct Variable {
    /// Without the `$`.
    pub name:        String,
    /// What it is; it can hold variables (`$JOB/assets`).
    pub value:       String,
    /// What it is for, said in the window and the list on `$`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl Variable {
    pub fn new(name: &str, value: &str) -> Self { Variable { name: name.into(), value: value.into(), description: String::new() } }
}

/// A variable as saved: a name and value alone, as the first graphs with
/// variables saved them, or with its description.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum SavedVariable {
    Full { name: String, value: String, #[serde(default)] description: String },
    Pair(String, String),
}

impl From<SavedVariable> for Variable {
    fn from(s: SavedVariable) -> Self {
        match s {
            SavedVariable::Full { name, value, description } => Variable { name, value, description },
            SavedVariable::Pair(name, value) => Variable { name, value, description: String::new() },
        }
    }
}

/// The built-in names: `$F2` to `$F9` too.
pub fn is_builtin(name: &str) -> bool {
    matches!(name, "GNAME" | "GDIR" | "JOB" | "OPNAME" | "F" | "FSTART" | "FEND" | "N" | "FPS" | "T")
        || name.strip_prefix('F').and_then(|d| d.parse::<usize>().ok()).is_some_and(|w| (1..=9).contains(&w))
}

/// Whether `name` can be a variable of the graph's: letters, digits and
/// `_`, not starting with a digit, and not a built-in name. What is wrong
/// with it, else.
pub fn check_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() { return Err("a name is needed"); }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') { return Err("letters, digits and _ only"); }
    if name.starts_with(|c: char| c.is_ascii_digit()) { return Err("cannot start with a digit"); }
    if is_builtin(name) { return Err("a built-in variable has this name"); }
    Ok(())
}

static GLOBALS: RwLock<Option<Globals>> = RwLock::new(None);

/// Set the global variables. Returns whether any but the frame changed:
/// what reads paths cooks again then (not on every frame of playback, so a
/// path with `$F` follows the frame when the graph next cooks).
pub fn set_globals(g: Globals) -> bool {
    let Ok(mut now) = GLOBALS.write() else { return false };
    let moved = now.as_ref().map(|n| Globals { frame: g.frame, ..n.clone() }) != Some(g.clone());
    *now = Some(g);
    moved
}

/// The global variables as last set.
pub fn globals() -> Globals {
    GLOBALS.read().ok().and_then(|g| g.clone()).unwrap_or_default()
}

/// The global variables, for the interface: name, what it is.
pub const GLOBALS_LIST: [(&str, &str); 13] = [
    ("$OPNAME", "the operator's name: the one whose text it is"),
    ("$GNAME", "the graph's name: its file without .xms"),
    ("$GDIR", "the folder of the graph's file"),
    ("$JOB", "the project folder: as File > Variables sets it, else $JOB from the environment, else $GDIR"),
    ("$F", "the frame (the playhead's; a render's own, in its command)"),
    ("$F4", "the frame, four digits (0001); $F2 to $F9 likewise"),
    ("$FSTART", "the scene's first frame"),
    ("$FEND", "the scene's last frame"),
    ("$N", "how many frames"),
    ("$FPS", "frames per second"),
    ("$T", "the time in seconds"),
    ("$ASSETS, …", "the graph's own, from File > Variables"),
    ("$HOME, …", "anything else set in the environment"),
];

/// The built-in global variables, by name without the `$`: what each is.
pub const BUILTINS: [(&str, &str); 11] = [
    ("OPNAME", "this operator's name"),
    ("GNAME", "the graph's name"),
    ("GDIR", "the graph's folder"),
    ("JOB", "the project folder"),
    ("F", "the frame"),
    ("F4", "the frame, four digits"),
    ("FSTART", "the first frame"),
    ("FEND", "the last frame"),
    ("N", "how many frames"),
    ("FPS", "frames per second"),
    ("T", "the time in seconds"),
];

/// Every variable a field can take, for the list shown on typing `$`:
/// name (without the `$`), value now, what it is. `locals` (name, what it
/// is, value) first, then the graph's own, then the built-ins.
pub fn available(locals: &[(&str, &str, String)]) -> Vec<(String, String, String)> {
    let g = globals();
    let values: Vec<(&str, String)> = locals.iter().map(|(n, _, v)| (*n, v.clone())).collect();
    let mut out: Vec<(String, String, String)> = locals.iter().map(|(n, what, v)| (n.to_string(), v.clone(), what.to_string())).collect();
    for v in &g.user.list {
        let n = &v.name;
        if out.iter().any(|o| o.0 == *n) { continue; }
        let what = if v.description.trim().is_empty() { "the graph's".to_string() } else { v.description.clone() };
        out.push((n.clone(), expand_in(&format!("${{{n}}}"), &values, &g, 0, false), what));
    }
    for (n, what) in BUILTINS {
        if out.iter().any(|o| o.0 == n) { continue; }
        // One with no value here (`$OPNAME` with no operator) shows none.
        let token = format!("${{{n}}}");
        let v = expand_in(&token, &values, &g, 0, false);
        out.push((n.to_string(), if v == token { String::new() } else { v }, what.to_string()));
    }
    out
}

/// Environment variables whose name starts with `prefix`, sorted: name, value.
pub fn environment(prefix: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::env::vars().filter(|(n, _)| n.starts_with(prefix)).collect();
    out.sort();
    out
}

/// How deep a variable's value is read for the variables in it: a value
/// naming itself stops there.
const DEPTH: usize = 8;

/// `text` with its variables put in: `locals` first (names without the
/// `$`), then the globals, then the graph's own, then the environment.
pub fn expand(text: &str, locals: &[(&str, String)]) -> String {
    expand_in(text, locals, &globals(), 0, false)
}

/// `expand`, the frame's variables left as they are (`$F`, `$F4`, `$T`):
/// text passed on to a renderer, which puts the frame in for each frame
/// it renders (Render Settings' Output Image).
pub fn expand_keeping_frames(text: &str) -> String {
    if !text.contains('$') { return text.to_string(); }
    expand_in(text, &[], &globals(), 0, true)
}

thread_local! {
    /// The operators whose text is being read, innermost last.
    static OPERATORS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// While it lives, `$OPNAME` is `name`: taken while an operator cooks,
/// while its panel is drawn, or while what it writes is written.
pub struct Operator(());

impl Drop for Operator {
    fn drop(&mut self) { OPERATORS.with(|o| { o.borrow_mut().pop(); }); }
}

/// `$OPNAME` is `name` until what this returns is dropped.
pub fn operator(name: &str) -> Operator {
    OPERATORS.with(|o| o.borrow_mut().push(name.to_string()));
    Operator(())
}

/// `text` with its variables put in as operator `name`'s: `$OPNAME` is it.
pub fn expand_as(name: &str, text: &str) -> String {
    let _o = operator(name);
    expand(text, &[])
}

/// The operator whose text is being read, if any.
fn current_operator() -> Option<String> { OPERATORS.with(|o| o.borrow().last().cloned()) }

fn expand_in(text: &str, locals: &[(&str, String)], g: &Globals, depth: usize, keep: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '$' { out.push(c); i += 1; continue; }
        if chars.get(i + 1) == Some(&'$') { out.push('$'); i += 2; continue; }
        // `${NAME}` or `$NAME`.
        let (name, len) = if chars.get(i + 1) == Some(&'{') {
            match chars[i + 2..].iter().position(|c| *c == '}') {
                Some(k) => (chars[i + 2..i + 2 + k].iter().collect::<String>(), k + 3),
                None => (String::new(), 0),
            }
        } else {
            let k = chars[i + 1..].iter().take_while(|c| c.is_ascii_alphanumeric() || **c == '_').count();
            (chars[i + 1..i + 1 + k].iter().collect::<String>(), k + 1)
        };
        match (len > 1).then(|| value(&name, locals, g, depth, keep)).flatten() {
            Some(v) => { out.push_str(&v); i += len; }
            None => { out.push('$'); i += 1; }
        }
    }
    out
}

/// A variable's value, or `None` for a name that is not one.
fn value(name: &str, locals: &[(&str, String)], g: &Globals, depth: usize, keep: bool) -> Option<String> {
    if let Some((_, v)) = locals.iter().find(|(n, _)| *n == name) { return Some(v.clone()); }
    // The frame's, for whoever renders: left.
    if keep && (name == "F" || name == "T" || name.strip_prefix('F').and_then(|d| d.parse::<usize>().ok()).is_some_and(|w| (1..=9).contains(&w))) { return None; }
    // A value of the graph's, its own variables put in.
    let nested = |v: &str| if depth < DEPTH { expand_in(v, locals, g, depth + 1, keep) } else { v.to_string() };
    // The frame, in the reader's own when it has one.
    let frame = locals.iter().find(|(n, _)| *n == "F").and_then(|(_, v)| v.parse().ok()).unwrap_or(g.frame);
    let (start, end) = (
        locals.iter().find(|(n, _)| *n == "FSTART").and_then(|(_, v)| v.parse().ok()).unwrap_or(g.start),
        locals.iter().find(|(n, _)| *n == "FEND").and_then(|(_, v)| v.parse().ok()).unwrap_or(g.end),
    );
    Some(match name {
        "OPNAME" => current_operator()?,
        "GNAME" => g.gname.clone(),
        "GDIR" => g.gdir.clone(),
        "JOB" if !g.user.job.trim().is_empty() => nested(g.user.job.trim()),
        "JOB" => std::env::var("JOB").unwrap_or_else(|_| g.gdir.clone()),
        "F" => frame.to_string(),
        "FSTART" => start.to_string(),
        "FEND" => end.to_string(),
        "N" => (end - start + 1).max(1).to_string(),
        "FPS" => format!("{}", g.fps),
        "T" => format!("{}", if g.fps > 0.0 { frame as f64 / g.fps } else { 0.0 }),
        _ => {
            if let Some(width) = name.strip_prefix('F').and_then(|d| d.parse::<usize>().ok()).filter(|w| (1..=9).contains(w)) {
                return Some(if frame < 0 { format!("-{:0width$}", -frame, width = width) } else { format!("{:0width$}", frame, width = width) });
            }
            if let Some(v) = g.user.list.iter().find(|v| v.name == name) { return Some(nested(v.value.as_str())); }
            return std::env::var(name).ok();
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(g: Globals, f: impl FnOnce()) {
        // Tests set the globals one after the other.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_globals(g);
        f();
    }

    fn shot() -> Globals {
        Globals { gname: "street_v001".into(), gdir: "/shows/a".into(), frame: 7, start: 1, end: 24, fps: 24.0, user: Variables::default() }
    }

    #[test]
    fn globals_frames_and_padding() {
        with(shot(), || {
            assert_eq!(expand("$GDIR/usd/$GNAME.$F4.usda", &[]), "/shows/a/usd/street_v001.0007.usda");
            assert_eq!(expand("$FSTART-$FEND ($N) at $FPS, ${F2}x", &[]), "1-24 (24) at 24, 07x");
            assert_eq!(expand("$T", &[]), format!("{}", 7.0 / 24.0));
        });
    }

    #[test]
    fn locals_come_first_and_unknown_names_stay() {
        with(shot(), || {
            let locals = [("F", "12".to_string()), ("OPNAME", "Render".to_string())];
            assert_eq!(expand("$OPNAME $F3 $GNAME", &locals), "Render 012 street_v001");
            assert_eq!(expand("costs $$5, $1 and $NOT_SET_ANYWHERE_XMS", &[]), "costs $5, $1 and $NOT_SET_ANYWHERE_XMS");
            assert_eq!(expand("$F0 $F10 $Fx $", &[]), "$F0 $F10 $Fx $");
            assert_eq!(expand("naïve $GNAME", &[]), "naïve street_v001");
        });
    }

    #[test]
    fn the_environment_comes_last() {
        with(shot(), || {
            std::env::set_var("XMS_VARS_TEST", "from the environment");
            assert_eq!(expand("${XMS_VARS_TEST}!", &[]), "from the environment!");
        });
    }

    /// The graph's own variables: after the built-ins, before the
    /// environment; a value's variables put in, `$JOB` as File > Variables
    /// sets it.
    #[test]
    fn the_graphs_own_variables() {
        let user = Variables {
            job: "$HOME_XMS_TEST/shows/a".into(),
            list: vec![Variable::new("ASSETS", "$JOB/assets"), Variable::new("SHOT", "sh010_$F4"), Variable::new("LOOP", "$LOOP")],
        };
        std::env::set_var("HOME_XMS_TEST", "/home/me");
        with(Globals { user, ..shot() }, || {
            assert_eq!(expand("$ASSETS/tree.usda", &[]), "/home/me/shows/a/assets/tree.usda");
            assert_eq!(expand("${SHOT}.exr", &[]), "sh010_0007.exr");
            assert_eq!(expand("$SHOT", &[("F", "3".to_string())]), "sh010_0003", "a render's own frame");
            assert_eq!(expand("$LOOP", &[]), "$LOOP", "a value naming itself stops");
            let names: Vec<String> = available(&[]).into_iter().map(|a| a.0).collect();
            assert_eq!(&names[..3], &["ASSETS", "SHOT", "LOOP"]);
            assert!(names.contains(&"F4".to_string()));
        });
        assert_eq!(check_name("ASSETS"), Ok(()));
        assert!(check_name("F4").is_err() && check_name("JOB").is_err() && check_name("1A").is_err() && check_name("A-B").is_err());
    }

    /// `$OPNAME` is the operator whose text is read, innermost first; the
    /// frame's variables can be kept for a renderer.
    #[test]
    fn the_operator_and_the_frames_kept() {
        with(shot(), || {
            assert_eq!(expand("/out/$OPNAME", &[]), "/out/$OPNAME", "no operator: left");
            {
                let _outer = operator("write_set");
                {
                    let _inner = operator("load_tree");
                    assert_eq!(expand("$OPNAME", &[]), "load_tree");
                }
                assert_eq!(expand("$GNAME/$OPNAME", &[]), "street_v001/write_set");
                assert_eq!(expand("$OPNAME", &[("OPNAME", "Render".to_string())]), "Render", "a local comes first");
            }
            assert_eq!(expand_keeping_frames("$GDIR/render/$GNAME.$F4.exr ${F} $T $FEND"), "/shows/a/render/street_v001.$F4.exr ${F} $T 24");
        });
    }

    /// A variable saves its description when it has one, and the first
    /// graphs' name-and-value pairs still read.
    #[test]
    fn variables_as_saved() {
        let v = Variables { job: String::new(), list: vec![Variable { description: "set dressing".into(), ..Variable::new("ASSETS", "$JOB/assets") }, Variable::new("SHOT", "sh010")] };
        let text = serde_json::to_string(&v).unwrap();
        assert!(text.contains("set dressing") && !text.contains("\"description\":\"\""), "{text}");
        assert_eq!(serde_json::from_str::<Variables>(&text).unwrap(), v);
        let old: Variables = serde_json::from_str(r#"{"list":[["ASSETS","$JOB/assets"]]}"#).unwrap();
        assert_eq!(old.list, vec![Variable::new("ASSETS", "$JOB/assets")]);
    }
}
