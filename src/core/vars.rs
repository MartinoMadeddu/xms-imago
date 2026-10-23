//! Variables: `$NAME` in the text Imago reads (file paths, command lines),
//! put in when the text is used.
//!
//! - **Local** variables belong to what is reading the text (a Render
//!   Command's `$USD`, the Image Viewer's `$IMAGE`), and come first.
//! - **Global** variables are the same everywhere in the graph: the graph's
//!   file, the project, the frames (`GLOBALS` lists them).
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
pub const GLOBALS_LIST: [(&str, &str); 11] = [
    ("$GNAME", "the graph's name: its file without .xms"),
    ("$GDIR", "the folder of the graph's file"),
    ("$JOB", "the project folder: $JOB from the environment, else $GDIR"),
    ("$F", "the frame (the playhead's; a render's own, in its command)"),
    ("$F4", "the frame, four digits (0001); $F2 to $F9 likewise"),
    ("$FSTART", "the scene's first frame"),
    ("$FEND", "the scene's last frame"),
    ("$N", "how many frames"),
    ("$FPS", "frames per second"),
    ("$T", "the time in seconds"),
    ("$HOME, …", "anything else set in the environment"),
];

/// `text` with its variables put in: `locals` first (names without the
/// `$`), then the globals, then the environment.
pub fn expand(text: &str, locals: &[(&str, String)]) -> String {
    let g = globals();
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
        match (len > 1).then(|| value(&name, locals, &g)).flatten() {
            Some(v) => { out.push_str(&v); i += len; }
            None => { out.push('$'); i += 1; }
        }
    }
    out
}

/// A variable's value, or `None` for a name that is not one.
fn value(name: &str, locals: &[(&str, String)], g: &Globals) -> Option<String> {
    if let Some((_, v)) = locals.iter().find(|(n, _)| *n == name) { return Some(v.clone()); }
    // The frame, in the reader's own when it has one.
    let frame = locals.iter().find(|(n, _)| *n == "F").and_then(|(_, v)| v.parse().ok()).unwrap_or(g.frame);
    let (start, end) = (
        locals.iter().find(|(n, _)| *n == "FSTART").and_then(|(_, v)| v.parse().ok()).unwrap_or(g.start),
        locals.iter().find(|(n, _)| *n == "FEND").and_then(|(_, v)| v.parse().ok()).unwrap_or(g.end),
    );
    Some(match name {
        "GNAME" => g.gname.clone(),
        "GDIR" => g.gdir.clone(),
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
        Globals { gname: "street_v001".into(), gdir: "/shows/a".into(), frame: 7, start: 1, end: 24, fps: 24.0 }
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
}
