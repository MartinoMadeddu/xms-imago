//! Materials from a MaterialX document made anywhere: Substance, Houdini, Maya,
//! the MaterialX examples.
//!
//! A document holds materials (`surfacematerial` nodes) at its top level, the
//! shaders they read, and node graphs (`nodegraph`) the shaders read from
//! through the graphs' outputs. Each material becomes a graph of its own,
//! holding the material and every node it reads, with the node graphs laid out
//! flat: their nodes are in the graph with the rest, a wire into a graph's
//! output goes to the node behind it, and an input reading the graph's
//! interface (`interfacename`) takes what the interface input is set to. A file
//! name takes the `fileprefix` in force where it is written, and, relative, the
//! folder of the document.
//!
//! Documents it includes (`xi:include`) are read as part of it, from its
//! folder. A document with no `surfacematerial` gives a material for each
//! surface shader nothing reads. Looks, material assignments, and nodes the
//! document defines itself (a `nodedef` with its `nodegraph`) are not read.

use std::collections::HashMap;
use std::path::Path;

use crate::graph::{Binding, Graph};
use crate::library::{Library, NodeDef};
use crate::types::{Type, Value};
use crate::xml::{self, Element};

/// The materials read from a document, by name, and what was not read.
#[derive(Clone, Debug, Default)]
pub struct Imported {
    pub materials: Vec<(String, Graph)>,
    pub notes: Vec<String>,
}

/// Elements at the top of a document that are not nodes.
const NOT_NODES: [&str; 15] = [
    "typedef",
    "nodedef",
    "nodegraph",
    "implementation",
    "look",
    "lookgroup",
    "collection",
    "geominfo",
    "unitdef",
    "unittypedef",
    "targetdef",
    "backdrop",
    "output",
    "input",
    "token",
];

/// A node of the document: the node graph it is in (None: the top level), and
/// its name.
type Key = (Option<String>, String);

/// What an input reads, once followed through node graphs.
enum Source {
    /// A value as written, and the file prefix in force where it is.
    Value(String, String),
    /// A node's output.
    Node(Key, String),
    Nothing,
}

struct Reader<'a> {
    lib: &'a Library,
    root: &'a Element,
    dir: Option<&'a Path>,
    top: HashMap<&'a str, &'a Element>,
    graphs: HashMap<&'a str, &'a Element>,
}

/// The materials of a MaterialX document. `dir`: the document's folder, which
/// relative file names are taken from. Refused only when the text is not a
/// MaterialX document; a material that cannot be read is left out, and said why
/// in `notes`.
pub fn import(text: &str, lib: &Library, dir: Option<&Path>) -> Result<Imported, String> {
    let root = xml::parse(text).map_err(|e| e.to_string())?;
    if root.name != "materialx" {
        return Err(format!("root is <{}>, not <materialx>", root.name));
    }
    let mut include_notes = vec![];
    let root = expand(root, dir, 0, &mut include_notes);
    let mut r = Reader {
        lib,
        root: &root,
        dir,
        top: HashMap::new(),
        graphs: HashMap::new(),
    };
    for el in &root.children {
        let Some(name) = el.attr("name") else { continue };
        match el.name.as_str() {
            // A graph that implements a definition is not one of the document's.
            "nodegraph" if el.attr("nodedef").is_none() => {
                r.graphs.insert(name, el);
            }
            n if !NOT_NODES.contains(&n) => {
                r.top.insert(name, el);
            }
            _ => {}
        }
    }
    let mut out = Imported {
        notes: include_notes,
        ..Default::default()
    };
    // The materials, in document order; else the surface shaders nothing reads.
    let mut materials: Vec<&Element> = root
        .children
        .iter()
        .filter(|e| e.name == "surfacematerial" && e.attr("name").is_some())
        .collect();
    let mut wrapped = false;
    if materials.is_empty() {
        let read: Vec<&str> = root
            .children
            .iter()
            .flat_map(|e| e.children_named("input"))
            .filter_map(|i| i.attr("nodename"))
            .collect();
        materials = root
            .children
            .iter()
            .filter(|e| e.attr("type") == Some("surfaceshader") && !NOT_NODES.contains(&e.name.as_str()))
            .filter(|e| e.attr("name").is_some_and(|n| !read.contains(&n)))
            .collect();
        wrapped = true;
    }
    for m in materials {
        let name = m.attr("name").unwrap_or("material").to_string();
        let mut g = Graph::new();
        let mut ids = HashMap::new();
        let made = r.visit(&mut g, &mut ids, (None, name.clone()), 0, &mut out.notes);
        match made {
            Ok(id) => {
                if wrapped {
                    // A shader on its own: put into a material of its name.
                    if let Some(mat) = g.add(lib, "surfacematerial", [0.0, 0.0]) {
                        let _ = g.connect(lib, id, "out", mat, "surfaceshader");
                    }
                }
                g.arrange();
                out.materials.push((name, g));
            }
            Err(why) => out.notes.push(format!("{name}: not read: {why}")),
        }
    }
    if out.materials.is_empty() && out.notes.is_empty() {
        out.notes.push("No material in the document".into());
    }
    let looks = root.children.iter().filter(|e| e.name == "look").count();
    if looks > 0 {
        out.notes.push(format!(
            "Looks ({looks}) and their material assignments are left out: bind the materials with Material Binding"
        ));
    }
    Ok(out)
}

/// The document with what it includes (`xi:include href="..."`) in place of the
/// includes: their elements, each given the included document's file prefix,
/// its folder in front, where it has none of its own.
fn expand(mut root: Element, dir: Option<&Path>, depth: usize, notes: &mut Vec<String>) -> Element {
    if !root.children.iter().any(|c| c.name == "xi:include") {
        return root;
    }
    let mut children = vec![];
    for c in std::mem::take(&mut root.children) {
        if c.name != "xi:include" {
            children.push(c);
            continue;
        }
        let Some(href) = c.attr("href") else { continue };
        let Some(dir) = dir.filter(|_| depth < 8) else {
            notes.push(format!("{href}: included, not read"));
            continue;
        };
        let path = dir.join(href);
        let read = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| xml::parse(&t).map_err(|e| e.to_string()));
        let mut inc = match read {
            Ok(inc) => inc,
            Err(e) => {
                notes.push(format!("{href}: included, not read: {e}"));
                continue;
            }
        };
        let inc_dir = path.parent().map(Path::to_path_buf);
        inc = expand(inc, inc_dir.as_deref(), depth + 1, notes);
        // Its file names are relative to its own folder, after its prefix.
        let folder = Path::new(href)
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .filter(|p| !p.is_empty());
        let prefix = format!(
            "{}{}",
            folder.map(|f| format!("{f}/")).unwrap_or_default(),
            inc.attr("fileprefix").unwrap_or("")
        );
        for mut e in inc.children {
            if !prefix.is_empty() && e.attr("fileprefix").is_none() {
                e.attrs.push(("fileprefix".into(), prefix.clone()));
            }
            children.push(e);
        }
    }
    root.children = children;
    root
}

impl<'a> Reader<'a> {
    /// The element of a node.
    fn element(&self, key: &Key) -> Option<&'a Element> {
        match &key.0 {
            None => self.top.get(key.1.as_str()).copied(),
            Some(g) => self
                .graphs
                .get(g.as_str())?
                .children
                .iter()
                .find(|c| c.name != "input" && c.name != "output" && c.attr("name") == Some(key.1.as_str())),
        }
    }

    /// The file prefix in force on a node: its own, its graph's, the
    /// document's.
    fn prefix(&self, scope: &Option<String>, el: &Element) -> String {
        el.attr("fileprefix")
            .or_else(|| {
                scope
                    .as_deref()
                    .and_then(|g| self.graphs.get(g))
                    .and_then(|g| g.attr("fileprefix"))
            })
            .or_else(|| self.root.attr("fileprefix"))
            .unwrap_or("")
            .to_string()
    }

    /// The definition a node element is: the one it names, else the one its
    /// types give.
    fn def_of(&self, el: &Element) -> Result<&'a NodeDef, String> {
        let name = el.attr("name").unwrap_or("?");
        // A node the document defines itself.
        let own = |node: &str| {
            self.root
                .children
                .iter()
                .any(|c| c.name == "nodedef" && (c.attr("node") == Some(node) || c.attr("name") == Some(node)))
        };
        if let Some(d) = el.attr("nodedef") {
            if own(d) {
                return Err(format!(
                    "{name} is a {d}, which the document defines itself: not read yet"
                ));
            }
            return self
                .lib
                .def(d)
                .ok_or_else(|| format!("{name} is a {d}, which is not in the MaterialX library"));
        }
        if own(&el.name) && self.lib.default_def(&el.name).is_none() {
            return Err(format!(
                "{name} is a {}, which the document defines itself: not read yet",
                el.name
            ));
        }
        let inputs: Vec<(&str, Type)> = el
            .children_named("input")
            .filter_map(|i| Some((i.attr("name")?, Type::from_name(i.attr("type")?)?)))
            .collect();
        let out = el.attr("type").and_then(Type::from_name);
        if self.lib.default_def(&el.name).is_none() {
            return Err(format!(
                "{name} is a {}, which is not in the MaterialX library",
                el.name
            ));
        }
        self.lib
            .resolve(&el.name, &inputs, out)
            .or_else(|| self.lib.resolve(&el.name, &[], out))
            .or_else(|| self.lib.default_def(&el.name))
            .ok_or_else(|| format!("{name}: no {} takes these types", el.name))
    }

    /// What an input of a node in `scope` reads.
    fn source(&self, scope: &Option<String>, node: &Element, input: &Element, depth: usize) -> Source {
        if depth > 32 {
            return Source::Nothing;
        }
        if let Some(n) = input.attr("nodename") {
            return Source::Node(
                (scope.clone(), n.to_string()),
                input.attr("output").unwrap_or("out").to_string(),
            );
        }
        if let (None, Some(ng)) = (scope, input.attr("nodegraph")) {
            let Some(graph) = self.graphs.get(ng) else {
                return Source::Nothing;
            };
            let outputs: Vec<&Element> = graph.children_named("output").collect();
            let out = match input.attr("output") {
                Some(o) => outputs.iter().find(|e| e.attr("name") == Some(o)).copied(),
                None => outputs.first().copied(),
            };
            let Some(out) = out else { return Source::Nothing };
            // An output is read as an input of the graph.
            return self.source(&Some(ng.to_string()), graph, out, depth + 1);
        }
        if let (Some(g), Some(iface)) = (scope, input.attr("interfacename")) {
            let Some(graph) = self.graphs.get(g.as_str()) else {
                return Source::Nothing;
            };
            let Some(port) = graph.children_named("input").find(|i| i.attr("name") == Some(iface)) else {
                return Source::Nothing;
            };
            // The interface input is set at the top level: a value, or a node there.
            return self.source(&None, graph, port, depth + 1);
        }
        match input.attr("value") {
            Some(v) => Source::Value(v.to_string(), self.prefix(scope, node)),
            None => Source::Nothing,
        }
    }

    /// Add a node and everything it reads to `g`, once. Its id.
    fn visit(
        &self,
        g: &mut Graph,
        ids: &mut HashMap<Key, u32>,
        key: Key,
        depth: usize,
        notes: &mut Vec<String>,
    ) -> Result<u32, String> {
        if let Some(&id) = ids.get(&key) {
            return Ok(id);
        }
        if depth > 512 {
            return Err("the graph is too deep, or loops".into());
        }
        let el = self.element(&key).ok_or_else(|| match &key.0 {
            Some(ng) => format!("{} is not in {ng}", key.1),
            None => format!("{} is not in the document", key.1),
        })?;
        let def = self.def_of(el)?;
        let name = g.unique_name(&key.1);
        let id = g.push(name, def.node.clone(), def.name.clone(), [0.0, 0.0]);
        ids.insert(key.clone(), id);
        for input in el.children_named("input") {
            let Some(iname) = input.attr("name") else { continue };
            let Some(idef) = def.input(iname) else {
                notes.push(format!("{}: {} has no input {iname}: left out", key.1, def.name));
                continue;
            };
            match self.source(&key.0, el, input, 0) {
                Source::Value(text, prefix) => {
                    let text = if idef.ty == Type::Filename {
                        self.file(&prefix, &text)
                    } else {
                        text
                    };
                    match Value::parse(idef.ty, &text) {
                        Some(v) => g.set_value(id, iname, v),
                        None => notes.push(format!(
                            "{}.{iname}: {text:?} is not a {}: left at its default",
                            key.1, idef.ty
                        )),
                    }
                }
                Source::Node(src, output) => {
                    let from = self.visit(g, ids, src, depth + 1, notes)?;
                    // An output name the definition does not have: its first.
                    let output = match g.def(self.lib, from) {
                        Some(d) if d.output(&output).is_some() => output,
                        Some(d) => d.outputs.first().map(|o| o.name.clone()).unwrap_or(output),
                        None => output,
                    };
                    g.bind(id, iname, Binding::Connect { node: from, output });
                }
                Source::Nothing => {}
            }
        }
        Ok(id)
    }

    /// A file name as written, with its prefix, from the document's folder when
    /// relative.
    fn file(&self, prefix: &str, name: &str) -> String {
        if name.trim().is_empty() {
            return String::new();
        }
        let joined = format!("{prefix}{name}");
        match self.dir {
            Some(dir) if Path::new(&joined).is_relative() => dir.join(&joined).to_string_lossy().replace('\\', "/"),
            _ => joined,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> &'static Library {
        Library::standard()
    }

    #[test]
    fn node_graphs_are_laid_out_flat() {
        let doc = r#"<?xml version="1.0"?>
<materialx version="1.39" colorspace="lin_rec709">
  <nodegraph name="NG_wood1" fileprefix="../Images/">
    <tiledimage name="image_color" type="color3">
      <input name="file" type="filename" value="wood_color.jpg" colorspace="srgb_texture" />
      <input name="uvtiling" type="vector2" value="4.0, 4.0" />
    </tiledimage>
    <tiledimage name="image_roughness" type="float">
      <input name="file" type="filename" value="wood_roughness.jpg" />
    </tiledimage>
    <output name="out_color" type="color3" nodename="image_color" />
    <output name="out_roughness" type="float" nodename="image_roughness" />
  </nodegraph>
  <standard_surface name="SR_wood1" type="surfaceshader">
    <input name="base_color" type="color3" nodegraph="NG_wood1" output="out_color" />
    <input name="specular_roughness" type="float" nodegraph="NG_wood1" output="out_roughness" />
    <input name="coat" type="float" value="0.1" />
  </standard_surface>
  <surfacematerial name="Tiled_Wood" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR_wood1" />
  </surfacematerial>
</materialx>"#;
        let got = import(doc, lib(), Some(Path::new("/mats"))).unwrap();
        assert_eq!(got.materials.len(), 1, "{:?}", got.notes);
        let (name, g) = &got.materials[0];
        assert_eq!(name, "Tiled_Wood");
        assert_eq!(g.nodes.len(), 4);
        let s = g.surface_shader().unwrap();
        assert_eq!(g.node(s).unwrap().name, "SR_wood1");
        let (img, _) = g.source(s, "base_color").unwrap();
        assert_eq!(g.node(img).unwrap().def, "ND_tiledimage_color3");
        assert_eq!(
            g.value(lib(), img, "file"),
            Some(&Value::String("/mats/../Images/wood_color.jpg".into()))
        );
        assert_eq!(g.value(lib(), img, "uvtiling"), Some(&Value::Floats(vec![4.0, 4.0])));
        assert_eq!(g.value(lib(), s, "coat"), Some(&Value::Floats(vec![0.1])));
        assert!(g
            .nodes
            .iter()
            .all(|n| n.inputs.iter().all(|(i, _)| g.wire_ok(lib(), n.id, i))));
        // Laid out right to left: the material right of its shader, the shader right of the images.
        let x = |id: u32| g.node(id).unwrap().pos[0];
        assert!(x(g.material().unwrap()) > x(s) && x(s) > x(img));
    }

    #[test]
    fn interface_inputs_give_their_values_and_wires() {
        let doc = r#"<materialx version="1.39">
  <constant name="tint" type="color3"><input name="value" type="color3" value="1, 0, 0" /></constant>
  <nodegraph name="NG">
    <input name="amount" type="float" value="0.25" />
    <input name="colour" type="color3" nodename="tint" />
    <multiply name="m" type="color3">
      <input name="in1" type="color3" interfacename="colour" />
      <input name="in2" type="float" interfacename="amount" />
    </multiply>
    <output name="out" type="color3" nodename="m" />
  </nodegraph>
  <standard_surface name="S" type="surfaceshader">
    <input name="base_color" type="color3" nodegraph="NG" />
  </standard_surface>
</materialx>"#;
        let got = import(doc, lib(), None).unwrap();
        // No surfacematerial: the shader nothing reads, in a material of its name.
        assert_eq!(got.materials.len(), 1, "{:?}", got.notes);
        let (name, g) = &got.materials[0];
        assert_eq!(name, "S");
        let s = g.surface_shader().unwrap();
        let (m, _) = g.source(s, "base_color").unwrap();
        assert_eq!(g.node(m).unwrap().def, "ND_multiply_color3FA");
        assert_eq!(g.value(lib(), m, "in2"), Some(&Value::Floats(vec![0.25])));
        let (tint, _) = g.source(m, "in1").unwrap();
        assert_eq!(g.node(tint).unwrap().name, "tint");
    }

    #[test]
    fn what_is_not_read_is_said() {
        let doc = r#"<materialx version="1.39">
  <mystery name="x" type="color3" />
  <standard_surface name="S" type="surfaceshader"><input name="base_color" type="color3" nodename="x" /></standard_surface>
  <surfacematerial name="M" type="material"><input name="surfaceshader" type="surfaceshader" nodename="S" /></surfacematerial>
  <look name="L" />
</materialx>"#;
        let got = import(doc, lib(), None).unwrap();
        assert!(got.materials.is_empty());
        assert!(
            got.notes
                .iter()
                .any(|n| n.contains("M: not read") && n.contains("mystery")),
            "{:?}",
            got.notes
        );
        assert!(got.notes.iter().any(|n| n.contains("Looks")), "{:?}", got.notes);
        assert!(import("<nope/>", lib(), None).is_err());
    }
}
