//! What in a graph's material would make a renderer fail on it, found before
//! it is rendered: a renderer reading a broken network can stop the program
//! with it (Arnold's OSL does), where a message says what to fix.

use std::collections::HashSet;

use crate::graph::{Binding, Graph};
use crate::library::Library;

/// One thing wrong, for the node it is on.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    /// The node's name.
    pub node: String,
    pub text: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.node, self.text)
    }
}

/// The nodes the material reads (what is written), from its terminals.
fn read(graph: &Graph) -> Vec<u32> {
    let mut out: Vec<u32> = vec![];
    for (material, input) in [
        ("surfacematerial", "surfaceshader"),
        ("surfacematerial", "displacementshader"),
        ("volumematerial", "volumeshader"),
    ] {
        let Some(m) = graph.first(material) else { continue };
        if let Some((src, _)) = graph.source(m, input) {
            for id in graph.upstream(src) {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
    }
    out
}

/// A node of a loop the material reads: what reads itself never ends.
fn in_loop(graph: &Graph, start: &[u32]) -> Option<u32> {
    // Depth first: a node met again while it is on the path is in a loop.
    fn walk(graph: &Graph, id: u32, path: &mut Vec<u32>, done: &mut HashSet<u32>) -> Option<u32> {
        if path.contains(&id) {
            return Some(id);
        }
        if !done.insert(id) {
            return None;
        }
        path.push(id);
        let sources: Vec<u32> = graph
            .node(id)
            .map(|n| {
                n.inputs
                    .iter()
                    .filter_map(|(_, b)| match b {
                        Binding::Connect { node, .. } => Some(*node),
                        Binding::Value(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        for s in sources {
            if let Some(x) = walk(graph, s, path, done) {
                return Some(x);
            }
        }
        path.pop();
        None
    }
    let mut done = HashSet::new();
    start.iter().find_map(|id| walk(graph, *id, &mut vec![], &mut done))
}

/// What in the material would make a renderer fail: a node whose definition
/// is neither MaterialX's nor a renderer's (one the file defined itself), a
/// wire from a node not in the graph or from an output its node does not
/// have, a wire between types that do not join (drawn red), a loop. With
/// `renderer` (the definitions a renderer has, by name, and its name), also
/// a MaterialX node it does not define: a newer MaterialX's than its own.
/// Only the nodes the material reads.
pub fn problems(graph: &Graph, lib: &Library, renderer: Option<(&HashSet<String>, &str)>) -> Vec<Problem> {
    let mut out = vec![];
    let ids = read(graph);
    let say = |out: &mut Vec<Problem>, node: &str, text: String| {
        out.push(Problem {
            node: node.into(),
            text,
        })
    };
    for &id in &ids {
        let Some(n) = graph.node(id) else { continue };
        let def = graph.find_def(lib, &n.def);
        match def {
            None => say(&mut out, &n.name, format!("its definition {} is in no library", n.def)),
            Some(d) if d.target.is_none() && lib.def(&d.name).is_none() => say(
                &mut out,
                &n.name,
                format!(
                    "its definition {} is the file's own, which renderers do not have",
                    d.name
                ),
            ),
            Some(d) => {
                if let Some((known, name)) = renderer {
                    let family = d.target.is_some() || n.node.starts_with("Usd");
                    if !family && !known.contains(&d.name) {
                        say(
                            &mut out,
                            &n.name,
                            format!("{name}'s MaterialX has no {} (a newer MaterialX's)", d.name),
                        );
                    }
                }
            }
        }
        for (input, b) in &n.inputs {
            let Binding::Connect { node: src, output } = b else {
                continue;
            };
            let Some(s) = graph.node(*src) else {
                say(
                    &mut out,
                    &n.name,
                    format!("{input} reads a node that is not in the graph"),
                );
                continue;
            };
            if def.is_some() && graph.output_type(lib, *src, output).is_none() {
                say(
                    &mut out,
                    &n.name,
                    format!("{input} reads {}.{output}, which {} does not have", s.name, s.name),
                );
            } else if def.is_some() && !graph.wire_ok(lib, id, input) {
                say(
                    &mut out,
                    &n.name,
                    format!("{input} reads {}.{output}, of a type it does not take", s.name),
                );
            }
        }
    }
    if let Some(id) = in_loop(graph, &ids) {
        let name = graph.node(id).map_or(String::new(), |n| n.name.clone());
        say(&mut out, &name, "it reads itself, through a loop".into());
    }
    out
}

/// The node definitions in MaterialX documents, by name: a renderer's own
/// libraries (`<nodedef name="ND_…"`).
pub fn defined(texts: &[String]) -> HashSet<String> {
    let mut out = HashSet::new();
    for t in texts {
        for part in t.split("<nodedef").skip(1) {
            let head = part.split('>').next().unwrap_or("");
            if let Some(rest) = head.split("name=\"").nth(1) {
                if let Some(name) = rest.split('"').next() {
                    out.insert(name.to_string());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> &'static Library {
        Library::standard()
    }

    #[test]
    fn a_good_material_has_no_problems() {
        let g = Graph::standard_surface(lib());
        assert!(problems(&g, lib(), None).is_empty());
    }

    #[test]
    fn wrong_wires_missing_nodes_and_loops_are_found() {
        let lib = lib();
        let mut g = Graph::standard_surface(lib);
        let s = g.surface_shader().unwrap();
        let a = g.add(lib, "add", [0.0, 0.0]).unwrap();
        let b = g.add(lib, "add", [0.0, 0.0]).unwrap();
        g.connect(lib, a, "out", s, "base").unwrap();
        g.connect(lib, b, "out", a, "in1").unwrap();
        assert!(problems(&g, lib, None).is_empty());
        // A wire made by hand, as a file can have it: a loop, and a colour into
        // a float.
        g.node_mut(b).unwrap().inputs.push((
            "in1".into(),
            Binding::Connect {
                node: a,
                output: "out".into(),
            },
        ));
        let p = problems(&g, lib, None);
        assert!(p.iter().any(|p| p.text.contains("loop")), "{p:?}");
        let c = g.add(lib, "constant", [0.0, 0.0]).unwrap();
        g.connect(lib, c, "out", s, "base_color").unwrap();
        let n = g.node_mut(c).unwrap();
        n.def = "ND_constant_vector2".into();
        let p = problems(&g, lib, None);
        assert!(
            p.iter()
                .any(|p| p.node.starts_with("standard_surface") && p.text.contains("type")),
            "{p:?}"
        );
        g.node_mut(s).unwrap().inputs.push((
            "coat".into(),
            Binding::Connect {
                node: 999,
                output: "out".into(),
            },
        ));
        assert!(problems(&g, lib, None)
            .iter()
            .any(|p| p.text.contains("not in the graph")));
    }

    #[test]
    fn a_node_the_renderer_does_not_define_is_found() {
        let lib = lib();
        let g = Graph::standard_surface(lib);
        let s = g.surface_shader().unwrap();
        let shader = g.node(s).unwrap().def.clone();
        let known = defined(&[format!(
            "<materialx><nodedef name=\"{shader}\" node=\"standard_surface\"></nodedef>"
        )]);
        assert!(problems(&g, lib, Some((&known, "Arnold"))).is_empty());
        let p = problems(&g, lib, Some((&HashSet::new(), "Arnold")));
        assert_eq!(p.len(), 1);
        assert!(
            p[0].to_string()
                .contains("Arnold's MaterialX has no ND_standard_surface"),
            "{}",
            p[0]
        );
    }
}
