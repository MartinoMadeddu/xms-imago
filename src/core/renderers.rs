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
            let finding = Finding {
                in_imago: d.live.as_ref().map(in_imago),
                command: (!d.detect.is_empty())
                    .then(|| on_machine(&d.detect, &|s: &str| crate::core::vars::expand(s, &[]))),
            };
            FINDINGS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(d.name.clone(), finding);
        }
    });
}

/// A renderer run in Imago, loaded as it is to render.
fn in_imago(live: &LiveRenderer) -> Found {
    let Some(sdk) = sdk(live) else {
        return Found::No(where_is(live));
    };
    match live.backend.as_str() {
        "arnold" => match xms_render::arnold::version(&sdk) {
            Ok(v) => Found::Yes(format!("Arnold {v} in Imago")),
            Err(e) => Found::No(e),
        },
        other => Found::No(format!("Imago has no backend named \"{other}\"")),
    }
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
    let path = std::env::var_os("PATH").unwrap_or_default();
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

// Where a renderer run in Imago is installed

fn folders_file() -> Option<PathBuf> {
    crate::file_browser::config_path("renderer_folders.txt")
}

/// One line per backend: its name and folder, tab separated.
fn read_folders(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(|(_, f)| !f.trim().is_empty())
        .map(|(b, f)| (b.trim().to_string(), f.trim().to_string()))
        .collect()
}

/// The folder set for a backend in Settings (empty: none).
pub fn folder_set(backend: &str) -> String {
    folders_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|t| read_folders(&t))
        .and_then(|m| m.get(backend).cloned())
        .unwrap_or_default()
}

/// Set (or with an empty one clear) a backend's folder, and look again.
pub fn set_folder(backend: &str, folder: &str) {
    let Some(file) = folders_file() else { return };
    let mut all = std::fs::read_to_string(&file)
        .map(|t| read_folders(&t))
        .unwrap_or_default();
    match folder.trim() {
        "" => all.remove(backend),
        f => all.insert(backend.to_string(), f.to_string()),
    };
    let text: String = all.iter().map(|(b, f)| format!("{b}\t{f}\n")).collect();
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(file, text);
    find_all(&definitions());
}

/// Where a renderer run in Imago is installed: the folder set in Settings,
/// else its definition's `sdk` with its variables put in (`$ARNOLD_PATH`);
/// `None` while a variable is not set.
pub fn sdk(live: &LiveRenderer) -> Option<PathBuf> {
    let set = folder_set(&live.backend);
    let path = match set.is_empty() {
        false => crate::core::vars::expand(&set, &[]),
        true => crate::core::vars::expand(&live.sdk, &[]),
    };
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
    fn the_folders_set_are_read_back_by_backend() {
        let m = read_folders("arnold\t/Applications/Arnold-7.5\nrray\t  \njunk\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m["arnold"], "/Applications/Arnold-7.5");
    }
}
