//! Running a command line on the operating system, from the Render Command
//! operator: a render with husk, say, of the layer the Write USD above it
//! writes.
//!
//! - **Tokens:** `$USD` (the layer), `$NAME` (the operator), `$F` and `$F4`
//!   (the frame, padded to that many digits), `$FSTART`, `$FEND` and `$N`
//!   (the range and its frame count), also as `${USD}`; `$$` is a `$`. Any
//!   other `$` is left as it is, for the shell (`$HOME`).
//! - **Running:** in the background, through the system's shell (`sh -c`;
//!   `cmd /C` on Windows), one command line or one per frame, one after the
//!   other. What it prints is kept, line by line, and a percentage in it is
//!   its progress (`ALF_PROGRESS 45%`, `45%`). Stop ends it and what it
//!   started. A command only runs when asked: never while the graph cooks.
//!
//! Pure std: no part of it knows the graph.

use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

// ── Tokens ─────────────────────────────────────────────────────────────────

/// What the tokens of a command line stand for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tokens {
    pub usd:   String,
    pub name:  String,
    pub start: i64,
    pub end:   i64,
    /// The frame `$F` is: the run's, one per frame, else the first.
    pub frame: i64,
    /// Tokens of the caller's own (the Image Viewer's `$IMAGE`, `$CAMERA`…),
    /// by name without the `$`.
    pub more:  Vec<(&'static str, String)>,
}

/// The tokens Render Command knows, for its panel: token, what it is.
pub const TOKENS: [(&str, &str); 7] = [
    ("$USD", "the layer the Write USD above writes"),
    ("$NAME", "this operator's name"),
    ("$F", "the frame"),
    ("$F4", "the frame, four digits (0001); $F2 to $F9 likewise"),
    ("$FSTART", "the first frame"),
    ("$FEND", "the last frame"),
    ("$N", "how many frames"),
];

/// A command line with its tokens put in.
pub fn expand(line: &str, t: &Tokens) -> String {
    let mut out = String::with_capacity(line.len() + t.usd.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '$' { out.push(c); i += 1; continue; }
        // `$$`: a dollar.
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
        match (len > 0).then(|| value(&name, t)).flatten() {
            Some(v) => { out.push_str(&v); i += len; }
            None => { out.push('$'); i += 1; }
        }
    }
    out
}

/// A token's value, or `None` for a name that is not one.
fn value(name: &str, t: &Tokens) -> Option<String> {
    if let Some((_, v)) = t.more.iter().find(|(n, _)| *n == name) { return Some(v.clone()); }
    Some(match name {
        "USD" => t.usd.clone(),
        "NAME" => t.name.clone(),
        "FSTART" => t.start.to_string(),
        "FEND" => t.end.to_string(),
        "N" => (t.end - t.start + 1).max(1).to_string(),
        "F" => t.frame.to_string(),
        _ => {
            let digits = name.strip_prefix('F')?;
            let width: usize = digits.parse().ok().filter(|w| (1..=9).contains(w))?;
            if t.frame < 0 { format!("-{:0width$}", -t.frame, width = width) } else { format!("{:0width$}", t.frame, width = width) }
        }
    })
}

/// A command line without the options whose value is one of `t.more`'s
/// tokens standing for nothing: `-c "$CAMERA"`, `-camera $CAMERA`, with no
/// camera, are left out (the flag too), so the renderer takes its own.
pub fn without_empty(line: &str, t: &Tokens) -> String {
    let mut out = line.to_string();
    for (name, _) in t.more.iter().filter(|(_, v)| v.is_empty()) {
        for form in [format!("\"${name}\""), format!("\"${{{name}}}\""), format!("${{{name}}}"), format!("${name}")] {
            while let Some(k) = find_token(&out, &form) {
                let mut start = k;
                // The spaces before, and the flag before them.
                let before = out[..start].trim_end();
                let flag = before.rsplit(char::is_whitespace).next().unwrap_or("");
                if flag.starts_with('-') && flag.len() > 1 { start = before.len() - flag.len(); } else { start = before.len(); }
                let start = out[..start].trim_end().len();
                out.replace_range(start..k + form.len(), "");
            }
        }
    }
    out
}

/// Where `form` is in `s` as a whole token: not followed by a letter, a
/// digit or `_` (`$CAMERA` is not in `$CAMERAS`).
fn find_token(s: &str, form: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(k) = s[from..].find(form).map(|k| k + from) {
        let next = s[k + form.len()..].chars().next();
        if !next.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') { return Some(k); }
        from = k + form.len();
    }
    None
}

/// A percentage a line of output gives: `ALF_PROGRESS 45%`, `45.5%`,
/// `[ 45%]`. The last one when there are several.
pub fn progress(line: &str) -> Option<f32> {
    let bytes = line.as_bytes();
    let mut found = None;
    for (k, b) in bytes.iter().enumerate() {
        if *b != b'%' { continue; }
        let mut s = k;
        while s > 0 && (bytes[s - 1].is_ascii_digit() || bytes[s - 1] == b'.') { s -= 1; }
        if s == k { continue; }
        if let Ok(v) = line[s..k].trim_start_matches('.').parse::<f32>() {
            if (0.0..=100.0).contains(&v) { found = Some(v); }
        }
    }
    found
}

// ── Running ────────────────────────────────────────────────────────────────

/// Where a run is.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Running,
    /// Every command ran and ended with 0.
    Done,
    /// A command ended with this code (`None`: by a signal), or could not start.
    Failed(String),
    Stopped,
}

/// A run, as its panel shows it.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub state:    State,
    /// The last lines it printed, oldest first.
    pub lines:    Vec<String>,
    pub progress: Option<f32>,
    /// Which command of how many: 1 of 1, or frame by frame.
    pub at:       (usize, usize),
    pub elapsed:  Duration,
    /// The command line running now, tokens put in.
    pub command:  String,
}

/// How many lines a run keeps.
const KEEP: usize = 4000;

struct Shared {
    state:    State,
    lines:    VecDeque<String>,
    progress: Option<f32>,
    at:       (usize, usize),
    started:  Instant,
    ended:    Option<Instant>,
    command:  String,
    stop:     bool,
}

impl Shared {
    fn push(&mut self, line: String) {
        if let Some(p) = progress(&line) { self.progress = Some(p); }
        if self.lines.len() >= KEEP { self.lines.pop_front(); }
        self.lines.push_back(line);
    }
}

fn runs() -> &'static Mutex<HashMap<usize, Arc<Mutex<Shared>>>> {
    static RUNS: OnceLock<Mutex<HashMap<usize, Arc<Mutex<Shared>>>>> = OnceLock::new();
    RUNS.get_or_init(Default::default)
}

/// What a command line is run through: the system's shell.
fn shell(line: &str) -> std::process::Command {
    #[cfg(windows)]
    { let mut c = std::process::Command::new("cmd"); c.args(["/C", line]); c }
    #[cfg(not(windows))]
    {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", line]);
        // Its own process group, so Stop ends what it starts too.
        use std::os::unix::process::CommandExt;
        c.process_group(0);
        c
    }
}

/// Ends a process and everything it started.
fn kill_tree(child: &mut std::process::Child) {
    let pid = child.id();
    #[cfg(windows)]
    { let _ = std::process::Command::new("taskkill").args(["/T", "/F", "/PID", &pid.to_string()]).output(); }
    #[cfg(not(windows))]
    { let _ = std::process::Command::new("kill").args(["-TERM", "--", &format!("-{pid}")]).output(); }
    let _ = child.kill();
}

/// Starts running `commands`, one after the other, under `key` (the
/// operator's id): in `folder` when given, with `env` added to the
/// environment. A run already going under that key is stopped first.
pub fn start(key: usize, commands: Vec<String>, folder: Option<PathBuf>, env: Vec<(String, String)>) {
    stop(key);
    let shared = Arc::new(Mutex::new(Shared {
        state: State::Running, lines: VecDeque::new(), progress: None, at: (0, commands.len()),
        started: Instant::now(), ended: None, command: String::new(), stop: false,
    }));
    if let Ok(mut r) = runs().lock() { r.insert(key, shared.clone()); }
    std::thread::spawn(move || {
        let end = |state: State| if let Ok(mut s) = shared.lock() { s.state = state; s.ended = Some(Instant::now()); };
        for (k, line) in commands.iter().enumerate() {
            if let Ok(mut s) = shared.lock() {
                if s.stop { break; }
                s.at = (k + 1, commands.len());
                s.command = line.clone();
                s.progress = None;
                let shown = format!("$ {line}");
                s.push(shown);
            }
            let mut cmd = shell(line);
            cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
            if let Some(dir) = folder.as_ref().filter(|d| !d.as_os_str().is_empty()) { cmd.current_dir(dir); }
            for (name, value) in &env { if !name.trim().is_empty() { cmd.env(name.trim(), value); } }
            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => { end(State::Failed(format!("Could not start: {e}"))); return; }
            };
            let readers: Vec<_> = [child.stdout.take().map(|o| Box::new(o) as Box<dyn Read + Send>), child.stderr.take().map(|e| Box::new(e) as Box<dyn Read + Send>)]
                .into_iter().flatten().map(|pipe| { let s = shared.clone(); std::thread::spawn(move || read_lines(pipe, &s)) }).collect();
            // Waits, looking for Stop.
            let status = loop {
                if shared.lock().map(|s| s.stop).unwrap_or(false) {
                    kill_tree(&mut child);
                    let _ = child.wait();
                    for r in readers { let _ = r.join(); }
                    end(State::Stopped);
                    return;
                }
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => std::thread::sleep(Duration::from_millis(40)),
                    Err(e) => { end(State::Failed(e.to_string())); return; }
                }
            };
            for r in readers { let _ = r.join(); }
            if !status.success() {
                end(State::Failed(match status.code() { Some(c) => format!("Ended with code {c}"), None => "Ended by a signal".into() }));
                return;
            }
        }
        let stopped = shared.lock().map(|s| s.stop).unwrap_or(false);
        end(if stopped { State::Stopped } else { State::Done });
    });
}

/// What a pipe prints, as lines: a line ends at a newline, or at a
/// carriage return (progress printed over itself).
fn read_lines(mut pipe: Box<dyn Read + Send>, shared: &Arc<Mutex<Shared>>) {
    let mut buf = [0u8; 8192];
    let mut line: Vec<u8> = vec![];
    let flush = |line: &mut Vec<u8>| {
        if line.is_empty() { return; }
        let text = String::from_utf8_lossy(line).trim_end().to_string();
        line.clear();
        if text.is_empty() { return; }
        if let Ok(mut s) = shared.lock() { s.push(text); }
    };
    loop {
        match pipe.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => for b in &buf[..n] {
                if *b == b'\n' || *b == b'\r' { flush(&mut line); } else { line.push(*b); }
            },
        }
    }
    flush(&mut line);
}

/// Records a run that failed before it started (its layer could not be
/// written, say), for its panel.
pub fn fail(key: usize, why: String) {
    stop(key);
    let now = Instant::now();
    let mut lines = VecDeque::new();
    lines.push_back(why.clone());
    let shared = Arc::new(Mutex::new(Shared {
        state: State::Failed(why), lines, progress: None, at: (0, 0), started: now, ended: Some(now), command: String::new(), stop: false,
    }));
    if let Ok(mut r) = runs().lock() { r.insert(key, shared); }
}

/// Stops the run under `key`, if one is going.
pub fn stop(key: usize) {
    let shared = runs().lock().ok().and_then(|r| r.get(&key).cloned());
    if let Some(s) = shared { if let Ok(mut s) = s.lock() { s.stop = true; } }
}

/// Stops every run: when the program closes.
pub fn stop_all() {
    let keys: Vec<usize> = runs().lock().map(|r| r.keys().copied().collect()).unwrap_or_default();
    for k in keys { stop(k); }
}

/// The run under `key`, as it is now.
pub fn snapshot(key: usize, lines: usize) -> Option<Snapshot> {
    let shared = runs().lock().ok()?.get(&key).cloned()?;
    let s = shared.lock().ok()?;
    let skip = s.lines.len().saturating_sub(lines);
    Some(Snapshot {
        state: s.state.clone(),
        lines: s.lines.iter().skip(skip).cloned().collect(),
        progress: s.progress,
        at: s.at,
        elapsed: s.ended.unwrap_or_else(Instant::now).duration_since(s.started),
        command: s.command.clone(),
    })
}

/// Whether any run is going: the window keeps drawing to show it.
pub fn any_running() -> bool {
    let all: Vec<Arc<Mutex<Shared>>> = runs().lock().map(|r| r.values().cloned().collect()).unwrap_or_default();
    all.iter().any(|s| s.lock().map(|s| s.state == State::Running).unwrap_or(false))
}

/// Forgets the run under `key` (its lines too). A run going is stopped.
pub fn clear(key: usize) {
    stop(key);
    if let Ok(mut r) = runs().lock() { r.remove(&key); }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tokens { Tokens { usd: "/shots/a b.usda".into(), name: "Render".into(), start: 1, end: 24, frame: 7, more: vec![] } }

    #[test]
    fn a_callers_own_tokens_and_options_with_nothing() {
        let t = Tokens { more: vec![("IMAGE", "/r/a.exr".into()), ("CAMERA", String::new()), ("SETTINGS", "/Render/S".into())], ..t() };
        assert_eq!(expand("-o \"$IMAGE\" -s $SETTINGS -f $F", &t), "-o \"/r/a.exr\" -s /Render/S -f 7");
        let line = "husk -s \"$SETTINGS\" -c \"$CAMERA\" -f $F \"$USD\"";
        assert_eq!(without_empty(line, &t), "husk -s \"$SETTINGS\" -f $F \"$USD\"", "no camera: no -c");
        assert_eq!(without_empty("kick -i x -camera ${CAMERA} -dw", &t), "kick -i x -dw");
        assert_eq!(without_empty("echo $CAMERAS $CAMERA", &t), "echo $CAMERAS", "a whole token only");
        let with = Tokens { more: vec![("CAMERA", "/cam".into())], ..t.clone() };
        assert_eq!(without_empty(line, &with), line, "a camera: kept");
    }

    #[test]
    fn tokens_are_put_in() {
        assert_eq!(expand("husk -f $FSTART -n $N \"$USD\"", &t()), "husk -f 1 -n 24 \"/shots/a b.usda\"");
        assert_eq!(expand("out.$F4.exr $F ${F2}x $FEND", &t()), "out.0007.exr 7 07x 24");
        assert_eq!(expand("$HOME/$NAME costs $$5", &t()), "$HOME/Render costs $5", "unknown tokens are the shell's");
        assert_eq!(expand("${USD}! $ end $", &t()), "/shots/a b.usda! $ end $");
        assert_eq!(expand("$F0 $F10 $Fx", &t()), "$F0 $F10 $Fx", "not tokens");
        assert_eq!(expand("$F3", &Tokens { frame: -2, ..t() }), "-002");
        assert_eq!(expand("naïve $NAME", &t()), "naïve Render");
    }

    #[test]
    fn progress_is_read_from_lines() {
        assert_eq!(progress("ALF_PROGRESS 45%"), Some(45.0));
        assert_eq!(progress("[ 12.5%] rendering"), Some(12.5));
        assert_eq!(progress("10% then 20%"), Some(20.0));
        assert_eq!(progress("no percent here"), None);
        assert_eq!(progress("120% is not progress"), None);
        assert_eq!(progress("%"), None);
    }

    fn wait(key: usize) -> Snapshot {
        let t0 = Instant::now();
        loop {
            let s = snapshot(key, 100).unwrap();
            if s.state != State::Running || t0.elapsed() > Duration::from_secs(10) { return s; }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn commands_run_one_after_the_other() {
        start(9001, vec!["echo one; printf '50%%\\r60%%\\n' 1>&2".into(), "echo \"two $GREETING\"".into()], None, vec![("GREETING".into(), "hi".into())]);
        let s = wait(9001);
        assert_eq!(s.state, State::Done, "{:?}", s.lines);
        assert!(s.lines.iter().any(|l| l == "one") && s.lines.iter().any(|l| l == "two hi"), "{:?}", s.lines);
        assert!(s.lines.iter().any(|l| l == "50%"), "a carriage return ends a line: {:?}", s.lines);
        assert_eq!(s.at, (2, 2));
        start(9002, vec!["exit 3".into(), "echo never".into()], None, vec![]);
        let s = wait(9002);
        assert_eq!(s.state, State::Failed("Ended with code 3".into()));
        assert!(!s.lines.iter().any(|l| l == "never"), "a failure ends the run");
        fail(9001, "No layer".into());
        assert_eq!(snapshot(9001, 10).unwrap().state, State::Failed("No layer".into()));
        clear(9001);
        clear(9002);
        assert!(snapshot(9001, 10).is_none());
    }

    #[cfg(not(windows))]
    #[test]
    fn stop_ends_what_it_started() {
        let marker = std::env::temp_dir().join(format!("xms_command_stop_{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        // The shell starts a child that would write the marker later.
        start(9003, vec![format!("sh -c 'sleep 1; touch {}' ; echo after", marker.display())], None, vec![]);
        std::thread::sleep(Duration::from_millis(200));
        stop(9003);
        let s = wait(9003);
        assert_eq!(s.state, State::Stopped);
        std::thread::sleep(Duration::from_millis(1300));
        assert!(!marker.exists(), "the child was ended with the shell");
        clear(9003);
    }
}
