//! Compounds: definitions, their `.icd` files and the library they are found
//! in (see `docs/compounds.md`).
//!
//! - **A definition** is a piece of graph and its interface: ports, promoted
//!   parameters, a name, a category. It records its **context**: a scene
//!   compound is operators inside, an ICE compound ICE nodes, and each goes
//!   only in a graph of its own context.
//! - **An `.icd` file** holds one definition as JSON, with the file layout's
//!   version (`format_version`) beside the definition's own (`version`).
//! - **The library** is the folders definitions are found in, in order: the
//!   project's (`$JOB/compounds`), the user's (`compounds` in the config
//!   folder), those in `XMS_COMPOUNDS`, then the ones Imago ships (`compounds`
//!   beside the program, `assets/compounds` in the repository). The first
//!   definition of a name wins.
//!
//! Instances (the Compound operator) and their cooking are
//! `node_graph::compound`; going inside one, `inside_compound`.
#![cfg_attr(not(test), allow(dead_code))]

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The extension of a definition's file.
pub const EXTENSION: &str = "icd";

/// What an `.icd` file says it is.
pub const FORMAT: &str = "imago-compound";

/// The layout of `.icd` files this Imago writes; it reads this one and those
/// before it.
pub const FORMAT_VERSION: u32 = 1;

/// How deep compounds can be inside one another.
pub const MAX_DEPTH: usize = 32;

/// A compound's definition: its interface, and the graph inside.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    /// `FORMAT`.
    pub format: String,
    /// The file layout's version (`FORMAT_VERSION` when written).
    pub format_version: u32,
    /// The graph it goes in, and is made of.
    pub context: Context,
    /// Its name: what instances find it by, and its add-menu entry.
    pub name: String,
    /// The definition's own version, raised by Save as Definition.
    #[serde(default = "first_version")]
    pub version: u32,
    /// Its group in the add menu.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    /// Its operators' colour in the graph, when not the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<[u8; 3]>,
    /// What it does, for the add menu and the Parameters pane.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub help: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default)]
    pub inputs: Vec<Port>,
    #[serde(default)]
    pub outputs: Vec<Port>,
    /// Its promoted parameters, in the order the Parameters pane shows them.
    #[serde(default)]
    pub parameters: Vec<Parameter>,
    /// The inner operator the Viewer's tools act on, by name.
    #[serde(default)]
    pub tool: Option<String>,
    /// The graph inside: a scene compound's as a `.xms` graph holds it
    /// (`graph_io::to_json`, its ICE graphs with it), an ICE compound's as an
    /// ICE graph.
    #[serde(default)]
    pub graph: serde_json::Value,
}

/// The graph a compound goes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Context {
    /// The scene graph: operators inside.
    Scene,
    /// An ICE graph: ICE nodes inside.
    Ice,
}

/// A port of a compound's interface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Port {
    pub name: String,
    /// What an ICE compound's port carries (`Float`, `Vector3`…); a scene
    /// compound's ports carry a scene, or what their operators give.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// A promoted parameter: an inner parameter (or several) on the compound's
/// interface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    /// What instances keep its value by.
    pub name: String,
    /// What the Parameters pane calls it; its name when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(rename = "type")]
    pub kind: ParameterType,
    pub default: serde_json::Value,
    /// The slider's range, for numbers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<[f64; 2]>,
    /// The group (a section of the Parameters pane) it is shown in.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub help: String,
    /// Where its value goes: one or several inner parameters.
    #[serde(default)]
    pub targets: Vec<Target>,
}

/// The kinds of value a promoted parameter holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ParameterType {
    Float,
    Integer,
    Boolean,
    Vector2,
    Vector3,
    Vector4,
    Colour,
    String,
    Ramp,
}

/// An inner parameter a promoted one drives: the inner node by its name in the
/// definition, and the parameter's serde path in it, as keys name theirs
/// (`translation/0`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Target {
    pub node: String,
    pub path: String,
}

impl Definition {
    /// An empty definition of `context`, named `name`, at version 1.
    pub fn new(context: Context, name: &str) -> Definition {
        Definition {
            format: FORMAT.into(),
            format_version: FORMAT_VERSION,
            context,
            name: name.into(),
            version: first_version(),
            category: String::new(),
            icon: String::new(),
            colour: None,
            help: String::new(),
            author: String::new(),
            inputs: vec![],
            outputs: vec![],
            parameters: vec![],
            tool: None,
            graph: serde_json::Value::Null,
        }
    }

    /// A definition from an `.icd` file's text. Refused: what is not a
    /// definition, one written by a newer Imago, one with no name.
    pub fn from_json(text: &str) -> Result<Definition, String> {
        let d: Definition = serde_json::from_str(text).map_err(|e| format!("not a compound definition: {e}"))?;
        if d.format != FORMAT {
            return Err(format!("not a compound definition (its format is \"{}\")", d.format));
        }
        if d.format_version > FORMAT_VERSION {
            return Err(format!(
                "written by a newer Imago (file layout {}, this one reads up to {FORMAT_VERSION})",
                d.format_version
            ));
        }
        if d.name.trim().is_empty() {
            return Err("a compound definition with no name".into());
        }
        Ok(d)
    }

    /// As an `.icd` file holds it: indented JSON, its keys in a stable order,
    /// written in today's file layout.
    pub fn to_json(&self) -> String {
        let d = Definition {
            format: FORMAT.into(),
            format_version: FORMAT_VERSION,
            ..self.clone()
        };
        serde_json::to_string_pretty(&d).unwrap_or_default()
    }

    /// Read an `.icd` file.
    pub fn read(path: &Path) -> Result<Definition, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        Definition::from_json(&text)
    }

    /// Write it as an `.icd` file, making its folder.
    pub fn write(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, self.to_json()).map_err(|e| e.to_string())
    }

    /// A hash of the definition as written, the same on every machine and
    /// version of Imago: what an instance keeps to know its definition
    /// changed.
    pub fn hash(&self) -> u64 {
        fnv1a(self.to_json().as_bytes())
    }

    /// Whether it can go in a graph of context `into`; the message says why
    /// not.
    pub fn fits(&self, into: Context) -> Result<(), String> {
        if self.context == into {
            return Ok(());
        }
        let (is, goes, not) = match self.context {
            Context::Scene => ("a scene compound", "the scene graph", "an ICE graph"),
            Context::Ice => ("an ICE compound", "an ICE graph", "the scene graph"),
        };
        Err(format!("{} is {is}: it goes in {goes}, not {not}", self.name))
    }

    /// The name of its file in a library folder: its name, with what a file
    /// name cannot hold made `_` (`Groom Clump` is `Groom Clump.icd`).
    pub fn file_name(&self) -> String {
        let safe: String = self
            .name
            .trim()
            .chars()
            .map(|c| {
                if r#"/\:*?"<>|"#.contains(c) || c.is_control() {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        format!("{safe}.{EXTENSION}")
    }
}

/// Where a definition was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// `$JOB/compounds`.
    Project,
    /// `compounds` in the config folder.
    User,
    /// A folder in `XMS_COMPOUNDS`.
    Path,
    /// Shipped with Imago.
    Shipped,
}

impl Source {
    /// How the add menu and the Parameters pane say it.
    pub fn label(self) -> &'static str {
        match self {
            Source::Project => "project",
            Source::User => "user",
            Source::Path => "XMS_COMPOUNDS",
            Source::Shipped => "shipped",
        }
    }
}

/// A definition found in the library, and its file.
#[derive(Clone, Debug)]
pub struct Found {
    pub definition: Definition,
    pub path: PathBuf,
    pub source: Source,
}

/// The definitions of the library's folders, the first of each name; and the
/// files that could not be read, with why.
#[derive(Clone, Debug, Default)]
pub struct Library {
    pub found: Vec<Found>,
    pub problems: Vec<String>,
}

impl Library {
    /// Read the `.icd` files of `folders`, in order. A name already found is
    /// left to the first; a folder that is not there is passed over.
    pub fn scan(folders: &[(PathBuf, Source)]) -> Library {
        let mut library = Library::default();
        for (folder, source) in folders {
            let Ok(entries) = std::fs::read_dir(folder) else {
                continue;
            };
            let mut files: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case(EXTENSION)))
                .collect();
            // Within a folder, by file name: the same on every system.
            files.sort();
            for path in files {
                match Definition::read(&path) {
                    Ok(definition) if library.get(&definition.name).is_none() => library.found.push(Found {
                        definition,
                        path,
                        source: *source,
                    }),
                    Ok(_) => {}
                    Err(e) => library.problems.push(format!("{}: {e}", path.display())),
                }
            }
        }
        library
    }

    /// The definition of a name.
    pub fn get(&self, name: &str) -> Option<&Found> {
        self.found.iter().find(|f| f.definition.name == name)
    }

    /// The definition of a name, for a graph of context `into`: missing, or
    /// of the other context, the message says so.
    pub fn for_context(&self, name: &str, into: Context) -> Result<&Found, String> {
        let found = self
            .get(name)
            .ok_or_else(|| format!("The compound {name} was not found in the library"))?;
        found.definition.fits(into)?;
        Ok(found)
    }

    /// The definitions that go in a graph of context `context`, by category
    /// then name: the add menu's.
    pub fn of_context(&self, context: Context) -> Vec<&Found> {
        let mut out: Vec<&Found> = self.found.iter().filter(|f| f.definition.context == context).collect();
        out.sort_by(|a, b| {
            (&a.definition.category, &a.definition.name).cmp(&(&b.definition.category, &b.definition.name))
        });
        out
    }
}

static LIBRARY: std::sync::RwLock<Option<std::sync::Arc<Library>>> = std::sync::RwLock::new(None);

/// The library, scanned from `folders()` the first time it is asked for and
/// kept until `rescan`.
pub fn library() -> std::sync::Arc<Library> {
    if let Some(l) = LIBRARY.read().ok().and_then(|l| l.clone()) {
        return l;
    }
    rescan()
}

/// Scan the library's folders again: after a definition is saved, a graph is
/// opened or `$JOB` changed.
pub fn rescan() -> std::sync::Arc<Library> {
    let l = std::sync::Arc::new(Library::scan(&folders()));
    if let Ok(mut kept) = LIBRARY.write() {
        *kept = Some(l.clone());
    }
    l
}

/// Put `x` at `path` in a node's parameters as JSON (`translation/0`), the
/// variant's name above them skipped, as keys' paths are. False when the path
/// is not there.
pub fn put(value: &mut serde_json::Value, path: &str, x: serde_json::Value) -> bool {
    let mut v = match value {
        serde_json::Value::Object(o) if o.len() == 1 => match o.values_mut().next() {
            Some(v) => v,
            None => return false,
        },
        v => v,
    };
    for part in path.split('/').filter(|p| !p.is_empty()) {
        v = match v {
            serde_json::Value::Object(o) => match o.get_mut(part) {
                Some(v) => v,
                None => return false,
            },
            serde_json::Value::Array(a) => match part.parse::<usize>().ok().and_then(|i| a.get_mut(i)) {
                Some(v) => v,
                None => return false,
            },
            _ => return false,
        };
    }
    *v = x;
    true
}

/// The value at `path` in a node's parameters as JSON, as `put` finds it.
pub fn get<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut v = match value {
        serde_json::Value::Object(o) if o.len() == 1 => o.values().next()?,
        v => v,
    };
    for part in path.split('/').filter(|p| !p.is_empty()) {
        v = match v {
            serde_json::Value::Object(o) => o.get(part)?,
            serde_json::Value::Array(a) => a.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(v)
}

/// The library's folders, in the order they are searched: the project's (when
/// `$JOB` is known), the user's, those in `XMS_COMPOUNDS`, the shipped ones.
pub fn folders() -> Vec<(PathBuf, Source)> {
    let mut out = vec![];
    let job = crate::core::vars::expand("$JOB", &[]);
    if !job.trim().is_empty() && job != "$JOB" {
        out.push((Path::new(job.trim()).join("compounds"), Source::Project));
    }
    if let Some(user) = crate::file_browser::config_path("compounds") {
        out.push((user, Source::User));
    }
    if let Some(list) = std::env::var_os("XMS_COMPOUNDS") {
        out.extend(
            std::env::split_paths(&list)
                .filter(|p| !p.as_os_str().is_empty())
                .map(|p| (p, Source::Path)),
        );
    }
    if let Some(shipped) = shipped() {
        out.push((shipped, Source::Shipped));
    }
    out
}

/// The compounds Imago ships: `compounds` beside the program, as a release
/// has them, else the repository's `assets/compounds` for a build run from
/// it (`target/release/xms-imago`).
pub fn shipped() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let mut places = vec![dir.join("compounds")];
    places.extend(
        dir.ancestors()
            .skip(1)
            .take(3)
            .map(|root| root.join("assets").join("compounds")),
    );
    if let Ok(cwd) = std::env::current_dir() {
        places.push(cwd.join("assets").join("compounds"));
    }
    places.into_iter().find(|p| p.is_dir())
}

/// The first version of a definition.
fn first_version() -> u32 {
    1
}

/// 64-bit FNV-1a: a hash that stays the same from one build to the next.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own under the system's temporary folder, for one test.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xms_compounds_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn clump() -> Definition {
        let mut d = Definition::new(Context::Scene, "Groom Clump");
        d.version = 3;
        d.category = "Groom".into();
        d.help = "Pulls each strand toward its clump guide".into();
        d.inputs = vec![
            Port {
                name: "Scene".into(),
                kind: None,
            },
            Port {
                name: "Clump Guides".into(),
                kind: None,
            },
        ];
        d.outputs = vec![Port {
            name: "Scene".into(),
            kind: None,
        }];
        d.parameters = vec![Parameter {
            name: "strength".into(),
            label: "Strength".into(),
            kind: ParameterType::Float,
            default: serde_json::json!(0.8),
            range: Some([0.0, 1.0]),
            group: "Clump".into(),
            help: String::new(),
            targets: vec![Target {
                node: "Clump".into(),
                path: "params/strength".into(),
            }],
        }];
        d.graph = serde_json::json!({ "nodes": [] });
        d
    }

    /// Written and read back, a definition is the same; the file is the
    /// documented layout.
    #[test]
    fn definitions_round_trip_through_their_files() {
        let dir = folder("round_trip");
        let d = clump();
        let path = dir.join("sub").join(d.file_name());
        d.write(&path).unwrap();
        assert_eq!(Definition::read(&path).unwrap(), d);
        let text = std::fs::read_to_string(&path).unwrap();
        for key in [
            r#""format": "imago-compound""#,
            r#""format_version": 1"#,
            r#""context": "Scene""#,
            r#""type": "Float""#,
        ] {
            assert!(text.contains(key), "{key} in {text}");
        }
        assert!(!text.contains("icon"), "empty metadata is left out");
        assert_eq!(
            d.hash(),
            Definition::read(&path).unwrap().hash(),
            "the hash is of what is written"
        );
        let mut v4 = d.clone();
        v4.version = 4;
        assert_ne!(d.hash(), v4.hash());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file that is not a definition, one from a newer Imago, one without a
    /// name: refused, saying why. A file without the optional keys reads.
    #[test]
    fn what_is_not_a_definition_is_refused() {
        assert!(Definition::from_json("{ nonsense").is_err());
        assert!(Definition::from_json(r#"{"nodes": []}"#).is_err());
        let other = r#"{"format":"something","format_version":1,"context":"Scene","name":"A"}"#;
        assert!(Definition::from_json(other).unwrap_err().contains("format"));
        let newer = r#"{"format":"imago-compound","format_version":99,"context":"Scene","name":"A"}"#;
        assert!(Definition::from_json(newer).unwrap_err().contains("newer"));
        let nameless = r#"{"format":"imago-compound","format_version":1,"context":"Ice","name":" "}"#;
        assert!(Definition::from_json(nameless).is_err());
        let least = r#"{"format":"imago-compound","format_version":1,"context":"Ice","name":"Wiggle"}"#;
        let d = Definition::from_json(least).unwrap();
        assert_eq!((d.version, d.context, d.parameters.len()), (1, Context::Ice, 0));
    }

    /// A scene compound goes in the scene graph only, an ICE compound in ICE
    /// graphs only.
    #[test]
    fn a_compound_goes_in_its_own_context() {
        let scene = clump();
        assert!(scene.fits(Context::Scene).is_ok());
        assert_eq!(
            scene.fits(Context::Ice).unwrap_err(),
            "Groom Clump is a scene compound: it goes in the scene graph, not an ICE graph"
        );
        let ice = Definition::new(Context::Ice, "Wiggle");
        assert!(ice.fits(Context::Ice).is_ok() && ice.fits(Context::Scene).is_err());
        let mut library = Library::default();
        library.found.push(Found {
            definition: ice,
            path: PathBuf::from("w.icd"),
            source: Source::User,
        });
        assert!(library.for_context("Wiggle", Context::Ice).is_ok());
        assert!(library
            .for_context("Wiggle", Context::Scene)
            .unwrap_err()
            .contains("an ICE compound"));
        assert!(library
            .for_context("Nope", Context::Ice)
            .unwrap_err()
            .contains("not found"));
    }

    /// The first folder with a name wins; files that do not read are listed,
    /// not fatal; a folder that is not there is passed over.
    #[test]
    fn the_library_is_searched_in_order() {
        let (project, user, shipped) = (folder("project"), folder("user"), folder("shipped"));
        let mut mine = clump();
        mine.help = "the project's".into();
        mine.write(&project.join(mine.file_name())).unwrap();
        clump().write(&user.join("clump.icd")).unwrap();
        Definition::new(Context::Ice, "Wiggle")
            .write(&shipped.join("wiggle.icd"))
            .unwrap();
        Definition::new(Context::Scene, "Light Rig")
            .write(&shipped.join("rig.ICD"))
            .unwrap();
        std::fs::write(shipped.join("broken.icd"), "{").unwrap();
        std::fs::write(shipped.join("notes.txt"), "not a definition").unwrap();
        let library = Library::scan(&[
            (project.clone(), Source::Project),
            (project.join("not_there"), Source::Path),
            (user.clone(), Source::User),
            (shipped.clone(), Source::Shipped),
        ]);
        let clump = library.get("Groom Clump").unwrap();
        assert_eq!(
            (clump.source, clump.definition.help.as_str()),
            (Source::Project, "the project's")
        );
        assert_eq!(library.found.len(), 3);
        assert_eq!(library.get("Wiggle").unwrap().source, Source::Shipped);
        assert_eq!(library.problems.len(), 1);
        assert!(library.problems[0].contains("broken.icd"));
        let scene: Vec<&str> = library
            .of_context(Context::Scene)
            .iter()
            .map(|f| f.definition.name.as_str())
            .collect();
        assert_eq!(scene, ["Light Rig", "Groom Clump"], "by category, then name");
        for dir in [project, user, shipped] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// File names keep the definition's name, with what a file name cannot
    /// hold made `_`.
    #[test]
    fn file_names_come_from_names() {
        assert_eq!(clump().file_name(), "Groom Clump.icd");
        assert_eq!(Definition::new(Context::Scene, "a/b:c?").file_name(), "a_b_c_.icd");
    }

    /// A value put at a path in parameters is read back there; a path that is
    /// not there changes nothing.
    #[test]
    fn values_go_to_paths() {
        let mut v = serde_json::json!({ "Transform": { "translation": [0.0, 1.0, 2.0], "name": "a" } });
        assert!(put(&mut v, "translation/1", serde_json::json!(5.0)));
        assert!(put(&mut v, "name", serde_json::json!("b")));
        assert!(!put(&mut v, "scale/0", serde_json::json!(1.0)));
        assert!(!put(&mut v, "translation/9", serde_json::json!(1.0)));
        assert_eq!(get(&v, "translation/1"), Some(&serde_json::json!(5.0)));
        assert_eq!(get(&v, "name"), Some(&serde_json::json!("b")));
        assert_eq!(v["Transform"]["translation"][2], serde_json::json!(2.0));
    }

    /// The FNV-1a hash of known text: the same in every build.
    #[test]
    fn the_hash_is_stable() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
