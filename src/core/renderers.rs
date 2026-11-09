//! Where renderer definitions come from, and whether each renderer is on this
//! machine (see `docs/renderer-definitions.md`).
//!
//! - **Definitions** (`definitions`): the ones Imago is built with
//!   (`assets/renderers`), then yours, from your renderers folder (`folder`),
//!   read at launch and on `reload`. One of yours replaces the built-in one of
//!   its name, or adds a renderer; a file that does not read is listed
//!   (`errors`), the built-in one kept.
//! - **Yours from a built-in one:** `make_copy` writes it into your folder
//!   with the version it is from; `lacks` says what a later built-in one has
//!   that yours does not; `revert` renames yours aside.
//! - **Found** (`found`): each way Imago runs a renderer looked for on its own,
//!   in Imago (the `live` block: the renderer loaded, as it is to render, and
//!   its version) and as a command line (the `detect` block). A renderer is
//!   never hidden, only marked.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use super::render::{Definition, LiveRenderer, CUSTOM};

const BUILT_IN: [&str; 2] = [
    include_str!("../../assets/renderers/karma.json"),
    include_str!("../../assets/renderers/arnold.json"),
];

/// Where a definition was read from.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Source {
    #[default]
    BuiltIn,
    Yours(PathBuf),
}

impl Source {
    pub fn describe(&self) -> String {
        match self {
            Source::BuiltIn => format!("Built in (Imago {})", env!("CARGO_PKG_VERSION")),
            Source::Yours(p) => format!("Yours: {}", p.display()),
        }
    }
}

/// How a renderer run as a command line is looked for: any one found is
/// enough. Paths may hold variables (`$HFS`, `$HOME`, `$JOB`).
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct Detect {
    /// Environment variables, set and not empty.
    #[serde(default)]
    pub env: Vec<String>,
    /// Programs on the `PATH`.
    #[serde(default)]
    pub programs: Vec<String>,
    /// Files.
    #[serde(default)]
    pub files: Vec<String>,
}

impl Detect {
    pub fn is_empty(&self) -> bool {
        self.env.is_empty() && self.programs.is_empty() && self.files.is_empty()
    }
}

/// Where a renderer is installed, set in File > Settings > Renderers: the
/// variable it sets inside Imago, over the environment's (for its `live` and
/// `detect` blocks, and command lines, which also find its `bin` on the
/// `PATH`).
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Folder {
    /// `Houdini Folder`.
    pub label: String,
    /// `HFS`.
    pub variable: String,
}

/// What looking for a renderer, one way it runs, found.
#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    Looking,
    /// What was found.
    Yes(String),
    /// Why not.
    No(String),
}

/// Each way Imago runs a renderer, looked for: `None`, not a way it runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Finding {
    /// In Imago's process (a definition with `live`).
    pub in_imago: Option<Found>,
    /// As a command line (a definition with `detect`).
    pub command: Option<Found>,
}

impl Finding {
    /// One line: "Arnold 7.5.0.0 in Imago · husk on the PATH".
    pub fn describe(&self) -> String {
        let part = |what: &str, f: &Found| match f {
            Found::Looking => format!("{what}: looking…"),
            Found::Yes(s) => s.clone(),
            Found::No(s) => format!("{what}: {s}"),
        };
        let parts: Vec<String> = [("In Imago", &self.in_imago), ("Command line", &self.command)]
            .into_iter()
            .filter_map(|(what, f)| f.as_ref().map(|f| part(what, f)))
            .collect();
        match parts.is_empty() {
            true => "Not looked for: its definition says neither how it runs in Imago nor how to find it".into(),
            false => parts.join(" · "),
        }
    }

    /// Found some way, or still looking.
    pub fn any(&self) -> bool {
        [&self.in_imago, &self.command]
            .into_iter()
            .flatten()
            .any(|f| !matches!(f, Found::No(_)))
    }
}

/// The folder of your renderer definitions: `$XMS_RENDERERS`, else
/// `renderers` in Imago's config folder.
pub fn folder() -> Option<PathBuf> {
    std::env::var_os("XMS_RENDERERS")
        .map(PathBuf::from)
        .or_else(|| crate::file_browser::config_path("renderers"))
}

struct Loaded {
    defs: Vec<Arc<Definition>>,
    errors: Vec<String>,
}

static LOADED: RwLock<Option<Arc<Loaded>>> = RwLock::new(None);

fn loaded() -> Arc<Loaded> {
    if let Some(l) = LOADED.read().unwrap_or_else(|e| e.into_inner()).clone() {
        return l;
    }
    let mut slot = LOADED.write().unwrap_or_else(|e| e.into_inner());
    if let Some(l) = slot.clone() {
        return l;
    }
    let l = Arc::new(read(folder().as_deref()));
    *slot = Some(l.clone());
    drop(slot);
    find_all(&l.defs);
    l
}

/// Every renderer definition: the built-in ones, then yours, by file name.
pub fn definitions() -> Vec<Arc<Definition>> {
    loaded().defs.clone()
}

/// A renderer's definition, by its key.
pub fn definition(name: &str) -> Option<Arc<Definition>> {
    loaded().defs.iter().find(|d| d.name == name).cloned()
}

/// Your definition files that could not be read, and why.
pub fn errors() -> Vec<String> {
    loaded().errors.clone()
}

/// Your folder read again, and the renderers looked for again.
pub fn reload() {
    let l = Arc::new(read(folder().as_deref()));
    *LOADED.write().unwrap_or_else(|e| e.into_inner()) = Some(l.clone());
    find_all(&l.defs);
}

/// The built-in definitions, each with its text.
fn built_in() -> impl Iterator<Item = (Definition, &'static str)> {
    BUILT_IN.iter().map(|text| {
        let d = serde_json::from_str::<Definition>(text).expect("the renderer definitions Imago is built with read");
        (d, *text)
    })
}

/// Whether Imago is built with a definition of this name.
pub fn is_built_in(name: &str) -> bool {
    built_in().any(|(d, _)| d.name == name)
}

/// The built-in definitions, then the `.json` files of `folder`, by name.
fn read(folder: Option<&Path>) -> Loaded {
    let mut defs: Vec<Arc<Definition>> = built_in().map(|(d, _)| Arc::new(d)).collect();
    let mut errors = vec![];
    let mut files: Vec<PathBuf> = folder
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .map(|read| {
            read.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("json")))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    for f in files {
        match std::fs::read_to_string(&f)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str::<Definition>(&t).map_err(|e| e.to_string()))
        {
            Ok(d) if d.name.trim().is_empty() || d.name == CUSTOM => {
                errors.push(format!("{}: needs a name other than \"{CUSTOM}\"", f.display()))
            }
            Ok(mut d) => {
                d.source = Source::Yours(f);
                match defs.iter().position(|x| x.name == d.name) {
                    Some(k) => defs[k] = Arc::new(d),
                    None => defs.push(Arc::new(d)),
                }
            }
            Err(e) => errors.push(format!("{}: {e} (the built-in definition is used)", f.display())),
        }
    }
    Loaded { defs, errors }
}

/// The built-in definition of a renderer, as a file of yours in `folder`,
/// `from` this version of Imago. Where it went.
fn copy_into(folder: &Path, name: &str) -> Result<PathBuf, String> {
    let (_, text) = built_in()
        .find(|(d, _)| d.name == name)
        .ok_or_else(|| format!("No built-in definition named \"{name}\""))?;
    let file = folder.join(format!("{name}.json"));
    if file.exists() {
        return Err(format!("Yours is there already: {}", file.display()));
    }
    // The version first, the built-in text as it is after it.
    let body = text
        .trim_start()
        .strip_prefix('{')
        .ok_or("The built-in definition is not an object")?;
    let text = format!("{{\n  \"from\": \"{}\",{body}", env!("CARGO_PKG_VERSION"));
    std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(file)
}

/// Make editable copy: the built-in definition into your folder, read in its
/// place from now on, and the folder opened. Where it went.
pub fn make_copy(name: &str) -> Result<PathBuf, String> {
    let dir = folder().ok_or("No config folder to put your renderer definitions in")?;
    let file = copy_into(&dir, name)?;
    reload();
    let _ = crate::docs::show(&dir);
    Ok(file)
}

/// Revert to built-in: your file renamed aside (not loaded, not deleted).
/// What it is called now.
pub fn revert(def: &Definition) -> Result<PathBuf, String> {
    let Source::Yours(file) = &def.source else {
        return Err("It is the built-in definition already".into());
    };
    let aside = aside(file, std::time::SystemTime::now());
    std::fs::rename(file, &aside).map_err(|e| format!("{}: {e}", file.display()))?;
    reload();
    Ok(aside)
}

/// `arnold.json` as `arnold.json.reverted-<seconds>`: no longer `.json`.
fn aside(file: &Path, at: std::time::SystemTime) -> PathBuf {
    let secs = at
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    file.with_file_name(format!("{name}.reverted-{secs}"))
}

/// What the built-in definition of the same name has that yours does not: its
/// settings and its AOVs, by name. Nothing for a built-in one.
pub fn lacks(def: &Definition) -> (Vec<String>, Vec<String>) {
    let Some((built, _)) = built_in()
        .find(|(d, _)| d.name == def.name)
        .filter(|_| def.source != Source::BuiltIn)
    else {
        return (vec![], vec![]);
    };
    let params = built
        .params
        .iter()
        .filter(|p| !def.params.iter().any(|q| q.name == p.name))
        .map(|p| p.name.clone())
        .collect();
    let aovs = built
        .aovs
        .iter()
        .filter(|a| !def.aovs.iter().any(|b| b.name == a.name))
        .map(|a| a.name.clone())
        .collect();
    (params, aovs)
}

// Finding renderers

static FINDINGS: Mutex<BTreeMap<String, Finding>> = Mutex::new(BTreeMap::new());

/// How a renderer was found, each way it runs.
pub fn found(name: &str) -> Finding {
    loaded();
    FINDINGS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(name)
        .cloned()
        .unwrap_or_default()
}

/// Every renderer looked for, off the main thread (loading a renderer takes a
/// moment): `Looking` until then.
fn find_all(defs: &[Arc<Definition>]) {
    let mut findings = FINDINGS.lock().unwrap_or_else(|e| e.into_inner());
    findings.clear();
    for d in defs {
        findings.insert(
            d.name.clone(),
            Finding {
                in_imago: d.live.as_ref().map(|_| Found::Looking),
                command: (!d.detect.is_empty()).then_some(Found::Looking),
            },
        );
    }
    drop(findings);
    let defs = defs.to_vec();
    std::thread::spawn(move || {
        for d in defs {
            let installed = d.live.as_ref().map(in_imago);
            let finding = Finding {
                in_imago: installed.as_ref().map(|i| match i {
                    Ok(a) => Found::Yes(format!("Arnold {} in Imago ({} options)", a.version, a.options.len())),
                    Err(e) => Found::No(e.clone()),
                }),
                command: (!d.detect.is_empty())
                    .then(|| on_machine(&d.detect, &|s: &str| crate::core::vars::expand(s, &[]))),
            };
            // The definition against the renderer as installed, and the
            // renderer against the one found here last.
            if let Some(Ok(a)) = &installed {
                let options = text_options(&a.options);
                let last = options_file(&d.name).and_then(|f| std::fs::read_to_string(f).ok());
                let last = last.as_deref().and_then(read_options);
                let params: Vec<(String, String)> = d
                    .params
                    .iter()
                    .map(|p| (p.name.clone(), p.default_value().display()))
                    .collect();
                let check = check(&d.namespace, &params, &a.version, &options, last.as_ref());
                if let Some(f) = options_file(&d.name) {
                    if let Some(dir) = f.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(f, write_options(&a.version, &options));
                }
                CHECKS
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(d.name.clone(), check);
            }
            FINDINGS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(d.name.clone(), finding);
        }
    });
}

/// A renderer run in Imago, loaded as it is to render: as installed.
fn in_imago(live: &LiveRenderer) -> Result<xms_render::arnold::Installed, String> {
    let sdk = sdk(live).ok_or_else(|| where_is(live))?;
    match live.backend.as_str() {
        "arnold" => xms_render::arnold::installed(&sdk),
        other => Err(format!("Imago has no backend named \"{other}\"")),
    }
}

/// A renderer's options as text: name, type and default, in its order.
type Options = Vec<(String, String, String)>;

fn text_options(options: &[xms_render::arnold::ArnoldOption]) -> Options {
    use xms_render::arnold::OptionDefault as D;
    let floats = |v: &[f32]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ");
    options
        .iter()
        .map(|o| {
            let (ty, v) = match &o.default {
                D::Bool(b) => ("bool", b.to_string()),
                D::Int(i) => ("int", i.to_string()),
                D::Float(f) => ("float", f.to_string()),
                D::Rgb(c) => ("rgb", floats(c)),
                D::Vector(v) => ("vector", floats(v)),
                D::Vector2(v) => ("vector2", floats(v)),
                D::Text(t) => ("string", t.clone()),
                D::Choice(at, all) => ("enum", format!("{at} [{}]", all.join(" "))),
            };
            (o.name.clone(), ty.to_string(), v)
        })
        .collect()
}

/// Where the options of a renderer found here last are kept, with its
/// version: to say what a new version changed.
fn options_file(renderer: &str) -> Option<PathBuf> {
    crate::file_browser::config_path(&format!("{renderer}_options.txt"))
}

/// The version on the first line, then a line per option: name, type and
/// default, tab separated.
fn write_options(version: &str, options: &Options) -> String {
    let lines = options.iter().map(|(n, t, d)| format!("{n}\t{t}\t{d}\n"));
    std::iter::once(format!("{version}\n")).chain(lines).collect()
}

fn read_options(text: &str) -> Option<(String, Options)> {
    let mut lines = text.lines();
    let version = lines.next()?.trim().to_string();
    let options = lines
        .filter_map(|l| {
            let mut p = l.splitn(3, '\t');
            Some((
                p.next()?.to_string(),
                p.next()?.to_string(),
                p.next().unwrap_or("").to_string(),
            ))
        })
        .collect();
    Some((version, options))
}

/// A renderer's definition against the renderer as installed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Check {
    /// The renderer's version.
    pub version: String,
    /// Settings the definition lists that the renderer does not have.
    pub unknown: Vec<String>,
    /// Settings whose default is not the renderer's: name, the definition's,
    /// the renderer's.
    pub defaults: Vec<(String, String, String)>,
    /// A new version since the one found here last: the last's, and the
    /// options added, gone, and whose type or default changed.
    pub since: Option<(String, Vec<String>, Vec<String>, Vec<String>)>,
}

static CHECKS: Mutex<BTreeMap<String, Check>> = Mutex::new(BTreeMap::new());

/// A renderer's definition against the renderer as installed, once it was
/// found (in Imago).
pub fn checked(name: &str) -> Option<Check> {
    CHECKS.lock().unwrap_or_else(|e| e.into_inner()).get(name).cloned()
}

/// The same default: numbers alike, else the same text (an enum's value,
/// ignoring case).
fn same(a: &str, b: &str) -> bool {
    // A bool as a number: `.usda` writes 1, Arnold says true.
    let number = |s: &str| match s.trim() {
        "true" => Ok(1.0),
        "false" => Ok(0.0),
        s => s.parse::<f64>(),
    };
    match (number(a), number(b)) {
        (Ok(x), Ok(y)) => (x - y).abs() <= 1e-6 * x.abs().max(1.0),
        _ => a.trim().eq_ignore_ascii_case(b.trim()),
    }
}

/// The definition's settings (name and default) under `namespace` against
/// the renderer's options; the options against those found here `last`.
fn check(
    namespace: &str,
    params: &[(String, String)],
    version: &str,
    options: &Options,
    last: Option<&(String, Options)>,
) -> Check {
    let prefix = format!("{namespace}:");
    let mut out = Check {
        version: version.to_string(),
        ..Default::default()
    };
    for (name, default) in params {
        let short = name.strip_prefix(&prefix).unwrap_or(name);
        match options.iter().find(|(n, _, _)| n == short) {
            None => out.unknown.push(short.to_string()),
            Some((_, _, theirs)) => {
                // An enum's default, without its values.
                let theirs = theirs.split(" [").next().unwrap_or(theirs);
                if !same(default, theirs) {
                    out.defaults
                        .push((short.to_string(), default.clone(), theirs.to_string()));
                }
            }
        }
    }
    if let Some((was, before)) = last.filter(|(was, _)| was != version) {
        let names = |o: &Options| o.iter().map(|(n, _, _)| n.clone()).collect::<Vec<_>>();
        let (now, then) = (names(options), names(before));
        let added = now.iter().filter(|n| !then.contains(n)).cloned().collect();
        let gone = then.iter().filter(|n| !now.contains(n)).cloned().collect();
        let changed = options
            .iter()
            .filter(|o| before.iter().any(|b| b.0 == o.0 && (b.1 != o.1 || b.2 != o.2)))
            .map(|o| o.0.clone())
            .collect();
        out.since = Some((was.clone(), added, gone, changed));
    }
    out
}

/// What to say when where a renderer is installed is not known.
pub fn where_is(live: &LiveRenderer) -> String {
    format!(
        "Where is it? Set its folder in File > Settings > Renderers, or {}",
        live.sdk.trim()
    )
}

/// A command-line renderer on this machine: the first of `detect` found.
/// `expand`: a path's variables put in.
fn on_machine(detect: &Detect, expand: &dyn Fn(&str) -> String) -> Found {
    if let Some(e) = detect
        .env
        .iter()
        .find(|e| std::env::var_os(e).is_some_and(|v| !v.is_empty()))
    {
        return Found::Yes(format!("${e} set"));
    }
    let path = search_path();
    let exe = |p: &str| {
        if cfg!(windows) {
            format!("{p}.exe")
        } else {
            p.to_string()
        }
    };
    if let Some(p) = detect
        .programs
        .iter()
        .find(|p| std::env::split_paths(&path).any(|dir| dir.join(exe(p)).is_file()))
    {
        return Found::Yes(format!("{p} on the PATH"));
    }
    if let Some(f) = detect
        .files
        .iter()
        .map(|f| expand(f))
        .find(|f| !f.contains('$') && Path::new(f).is_file())
    {
        return Found::Yes(f);
    }
    let looked: Vec<String> = (detect.env.iter().map(|e| format!("${e}")))
        .chain(detect.programs.iter().map(|p| format!("{p} on the PATH")))
        .chain(detect.files.iter().cloned())
        .collect();
    Found::No(format!("not found on this machine (looked for {})", looked.join(", ")))
}

// Where renderers are installed

fn folders_file() -> Option<PathBuf> {
    crate::file_browser::config_path("renderer_folders.txt")
}

/// One line per variable: its name and folder, tab separated. (`arnold`, as
/// the first version keyed Arnold's: `ARNOLD_PATH`.)
fn read_folders(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(|(_, f)| !f.trim().is_empty())
        .map(|(v, f)| match v.trim() {
            "arnold" => ("ARNOLD_PATH".to_string(), f.trim().to_string()),
            v => (v.to_string(), f.trim().to_string()),
        })
        .collect()
}

static FOLDERS: RwLock<Option<BTreeMap<String, String>>> = RwLock::new(None);

/// Every folder set, by variable, read once.
pub fn folders() -> BTreeMap<String, String> {
    if let Some(f) = FOLDERS.read().unwrap_or_else(|e| e.into_inner()).clone() {
        return f;
    }
    let read = folders_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|t| read_folders(&t))
        .unwrap_or_default();
    *FOLDERS.write().unwrap_or_else(|e| e.into_inner()) = Some(read.clone());
    read
}

/// The folder set for a variable (`HFS`), if one is.
pub fn folder_var(variable: &str) -> Option<String> {
    folders().remove(variable)
}

/// Set (or with an empty one clear) the folder of a variable, and look for
/// the renderers again.
pub fn set_folder(variable: &str, folder: &str) {
    let mut all = folders();
    match folder.trim() {
        "" => all.remove(variable),
        f => all.insert(variable.to_string(), f.to_string()),
    };
    if let Some(file) = folders_file() {
        let text: String = all.iter().map(|(v, f)| format!("{v}\t{f}\n")).collect();
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(file, text);
    }
    *FOLDERS.write().unwrap_or_else(|e| e.into_inner()) = Some(all);
    find_all(&definitions());
}

/// Where programs are looked for: each folder's `bin` set in Settings, then
/// the `PATH` (programs opened from the Finder or the Dock on macOS have a
/// short one).
pub fn search_path() -> std::ffi::OsString {
    let set = folders().into_values().map(|f| PathBuf::from(f).join("bin"));
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(set.chain(std::env::split_paths(&path))).unwrap_or(path)
}

/// Where a renderer run in Imago is installed: its definition's `sdk`, its
/// variables put in (`$ARNOLD_PATH`: the folder set in Settings, else the
/// environment's); `None` while one is not set.
pub fn sdk(live: &LiveRenderer) -> Option<PathBuf> {
    let path = crate::core::vars::expand(&live.sdk, &[]);
    let path = path.trim();
    (!path.is_empty() && !path.contains('$')).then(|| PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("xms-renderers-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn yours_replace_built_in_ones_or_add_renderers() {
        let dir = temp("yours");
        std::fs::write(
            dir.join("arnold.json"),
            r#"{"name": "arnold", "label": "Mine", "namespace": "arnold:global"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("rray.json"),
            r#"{"name": "rray", "label": "rray", "namespace": "rray"}"#,
        )
        .unwrap();
        std::fs::write(dir.join("broken.json"), "{ not json").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a definition").unwrap();
        let l = read(Some(&dir));
        let names: Vec<&str> = l.defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["karma", "arnold", "rray"]);
        let arnold = &l.defs[1];
        assert_eq!(arnold.label, "Mine");
        assert_eq!(arnold.source, Source::Yours(dir.join("arnold.json")));
        assert_eq!(l.defs[0].source, Source::BuiltIn);
        assert_eq!(l.errors.len(), 1);
        assert!(l.errors[0].contains("broken.json") && l.errors[0].contains("built-in definition is used"));
        // Yours lacks every setting and AOV of the built-in one.
        let (params, aovs) = lacks(arnold);
        assert!(params.contains(&"arnold:global:AA_samples".to_string()) && !aovs.is_empty());
        assert_eq!(lacks(&l.defs[0]), (vec![], vec![]), "a built-in one lacks nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_editable_copy_is_the_built_in_one_from_this_version() {
        let dir = temp("copy");
        let file = copy_into(&dir, "arnold").unwrap();
        assert_eq!(file, dir.join("arnold.json"));
        let l = read(Some(&dir));
        let mine = l.defs.iter().find(|d| d.name == "arnold").unwrap();
        assert_eq!(mine.from, env!("CARGO_PKG_VERSION"));
        assert_eq!(mine.source, Source::Yours(file.clone()));
        let built = built_in().find(|(d, _)| d.name == "arnold").unwrap().0;
        assert_eq!((&mine.params, &mine.aovs), (&built.params, &built.aovs));
        assert_eq!(lacks(mine), (vec![], vec![]));
        assert!(copy_into(&dir, "arnold").unwrap_err().contains("already"));
        assert!(copy_into(&dir, "nothing").is_err());
        // Reverting: renamed aside, no longer read.
        let away = aside(&file, std::time::UNIX_EPOCH + std::time::Duration::from_secs(7));
        assert_eq!(away, dir.join("arnold.json.reverted-7"));
        std::fs::rename(&file, &away).unwrap();
        let l = read(Some(&dir));
        assert_eq!(
            l.defs.iter().find(|d| d.name == "arnold").unwrap().source,
            Source::BuiltIn
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_command_line_renderer_is_found_by_any_of_its_signs() {
        let dir = temp("detect");
        let none = |s: &str| s.to_string();
        // A variable set.
        std::env::set_var("XMS_TEST_DETECT_SET", "/opt/thing");
        let by_env = Detect {
            env: vec!["XMS_TEST_DETECT_UNSET".into(), "XMS_TEST_DETECT_SET".into()],
            ..Default::default()
        };
        assert_eq!(
            on_machine(&by_env, &none),
            Found::Yes("$XMS_TEST_DETECT_SET set".into())
        );
        // A file, its variable put in.
        let husk = dir.join("husk");
        std::fs::write(&husk, "").unwrap();
        let by_file = Detect {
            files: vec!["$HERE/husk".into()],
            ..Default::default()
        };
        let here = dir.display().to_string();
        assert_eq!(
            on_machine(&by_file, &|s: &str| s.replace("$HERE", &here)),
            Found::Yes(husk.display().to_string())
        );
        // Nothing: what was looked for, said.
        let missing = Detect {
            env: vec!["XMS_TEST_DETECT_UNSET".into()],
            programs: vec!["xms-no-such-program".into()],
            files: vec!["$HERE/nothing".into()],
        };
        match on_machine(&missing, &none) {
            Found::No(why) => assert!(why.contains("xms-no-such-program on the PATH") && why.contains("$HERE/nothing")),
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A program on the `PATH`: here the PATH of this test's own process.
    #[test]
    fn a_program_on_the_path_is_found() {
        let dir = temp("path");
        let exe = if cfg!(windows) {
            "xms-test-husk.exe"
        } else {
            "xms-test-husk"
        };
        std::fs::write(dir.join(exe), "").unwrap();
        let mut paths: Vec<PathBuf> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
        paths.push(dir.clone());
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
        let d = Detect {
            programs: vec!["xms-test-husk".into()],
            ..Default::default()
        };
        assert_eq!(
            on_machine(&d, &|s: &str| s.to_string()),
            Found::Yes("xms-test-husk on the PATH".into())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_finding_says_each_way_on_one_line() {
        let f = Finding {
            in_imago: Some(Found::No("no Arnold library at /x/bin/libai.dylib".into())),
            command: Some(Found::Yes("husk on the PATH".into())),
        };
        assert_eq!(
            f.describe(),
            "In Imago: no Arnold library at /x/bin/libai.dylib · husk on the PATH"
        );
        assert!(f.any());
        let lost = Finding {
            in_imago: Some(Found::No("x".into())),
            command: None,
        };
        assert!(!lost.any());
        assert!(Finding::default().describe().starts_with("Not looked for"));
    }

    #[test]
    fn the_folders_set_are_read_back_by_variable() {
        let m = read_folders("HFS\t/opt/hfs21.0\narnold\t/Applications/Arnold-7.5\nrray\t  \njunk\n");
        assert_eq!(m.len(), 2);
        assert_eq!(m["HFS"], "/opt/hfs21.0");
        assert_eq!(
            m["ARNOLD_PATH"], "/Applications/Arnold-7.5",
            "as the first version keyed it"
        );
    }

    fn options(list: &[(&str, &str, &str)]) -> Options {
        list.iter()
            .map(|(n, t, d)| (n.to_string(), t.to_string(), d.to_string()))
            .collect()
    }

    /// The definition's settings against Arnold's options: one Arnold does
    /// not have, defaults that differ (numbers alike, an enum's value).
    #[test]
    fn a_definition_is_checked_against_the_renderer() {
        let arnold = options(&[
            ("AA_samples", "int", "1"),
            ("AA_adaptive_threshold", "float", "0.015"),
            ("render_device", "enum", "CPU [CPU GPU]"),
            ("ignore_sss", "bool", "false"),
        ]);
        let params = [
            ("arnold:global:AA_samples".to_string(), "3".to_string()),
            (
                "arnold:global:AA_adaptive_threshold".to_string(),
                "0.0150000".to_string(),
            ),
            ("arnold:global:render_device".to_string(), "cpu".to_string()),
            ("arnold:global:ignore_sss".to_string(), "0".to_string()),
            ("arnold:global:not_an_option".to_string(), "0".to_string()),
        ];
        let c = check("arnold:global", &params, "7.5.0.0", &arnold, None);
        assert_eq!(c.unknown, ["not_an_option"]);
        assert_eq!(
            c.defaults,
            [("AA_samples".to_string(), "3".to_string(), "1".to_string())]
        );
        assert_eq!(c.since, None, "nothing found here before");
        // The same version again: nothing new.
        let same = check(
            "arnold:global",
            &params,
            "7.5.0.0",
            &arnold,
            Some(&("7.5.0.0".into(), arnold.clone())),
        );
        assert_eq!(same.since, None);
    }

    /// A new Arnold: the options added, gone and changed since the last.
    #[test]
    fn a_new_version_says_what_changed() {
        let before = options(&[
            ("AA_samples", "int", "1"),
            ("old", "bool", "false"),
            ("bucket_size", "int", "64"),
        ]);
        let now = options(&[
            ("AA_samples", "int", "1"),
            ("bucket_size", "int", "32"),
            ("new_one", "float", "1"),
        ]);
        let text = write_options("7.4.0.0", &before);
        let last = read_options(&text).unwrap();
        assert_eq!(last, ("7.4.0.0".to_string(), before.clone()));
        let c = check("arnold:global", &[], "7.5.0.0", &now, Some(&last));
        let (was, added, gone, changed) = c.since.unwrap();
        assert_eq!(was, "7.4.0.0");
        assert_eq!(
            (added, gone, changed),
            (
                vec!["new_one".to_string()],
                vec!["old".to_string()],
                vec!["bucket_size".to_string()]
            )
        );
    }
}
