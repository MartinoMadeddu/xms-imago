//! A graph as a MaterialX document (`.mtlx`), and back.
//!
//! Each node is an element named after its node, with its name, the type of its
//! output (`multioutput` for nodes with several), its definition, and where it
//! is drawn (`xpos`, `ypos`, in canvas units). Under it, the inputs set on it:
//! a value, or the node (and output, for nodes with several) it reads. This is
//! how Imago saves a MaterialX operator's graph, and how it writes `.mtlx`
//! files.

use crate::graph::{Binding, Graph};
use crate::library::Library;
use crate::types::Type;
use crate::xml::{self, escape};

/// The graph as a MaterialX document.
pub fn write(graph: &Graph, lib: &Library) -> String {
    let mut out = String::from("<?xml version=\"1.0\"?>\n");
    out.push_str(&format!("<materialx version=\"{}\">\n", crate::MATERIALX_VERSION));
    for n in &graph.nodes {
        let def = graph.find_def(lib, &n.def);
        let ty = match def {
            Some(d) if d.outputs.len() > 1 => "multioutput".to_string(),
            Some(d) => d.out_type().name().to_string(),
            None => "float".to_string(),
        };
        out.push_str(&format!(
            "  <{} name=\"{}\" type=\"{}\" nodedef=\"{}\" xpos=\"{}\" ypos=\"{}\"",
            n.node,
            escape(&n.name),
            ty,
            escape(&n.def),
            n.pos[0],
            n.pos[1]
        ));
        if n.inputs.is_empty() {
            out.push_str(" />\n");
            continue;
        }
        out.push_str(">\n");
        for (input, b) in &n.inputs {
            let ity = def.and_then(|d| d.input(input)).map(|i| i.ty.name()).unwrap_or("float");
            match b {
                Binding::Value(v) => out.push_str(&format!(
                    "    <input name=\"{}\" type=\"{ity}\" value=\"{}\" />\n",
                    escape(input),
                    escape(&v.to_text())
                )),
                Binding::Connect { node, output } => {
                    let Some(src) = graph.node(*node) else { continue };
                    let several = graph.find_def(lib, &src.def).is_some_and(|d| d.outputs.len() > 1);
                    let out_attr = if several {
                        format!(" output=\"{}\"", escape(output))
                    } else {
                        String::new()
                    };
                    out.push_str(&format!(
                        "    <input name=\"{}\" type=\"{ity}\" nodename=\"{}\"{out_attr} />\n",
                        escape(input),
                        escape(&src.name)
                    ));
                }
            }
        }
        out.push_str(&format!("  </{}>\n", n.node));
    }
    out.push_str("</materialx>\n");
    out
}

/// The graph a MaterialX document holds: the nodes at its top level.
/// Definitions, node graphs, looks and other elements are not read. A node
/// without a `nodedef` takes the definition its output and input types give.
/// A `nodedef` the library does not have is kept as the document describes it
/// (`Graph::missing`: its inputs and output by their types), so its node is
/// written out again as it was read. Refused, with why, on a node the library
/// does not have and that names no definition, a value that does not read as
/// its type, or a connection to a node that is not there.
pub fn read(text: &str, lib: &Library) -> Result<Graph, String> {
    let root = xml::parse(text).map_err(|e| e.to_string())?;
    if root.name != "materialx" {
        return Err(format!("root is <{}>, not <materialx>", root.name));
    }
    const NOT_NODES: [&str; 12] = [
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
    ];
    let mut g = Graph::new();
    let mut wires: Vec<(u32, String, String, String)> = vec![];
    for el in root.children.iter().filter(|e| !NOT_NODES.contains(&e.name.as_str())) {
        let name = el.attr("name").ok_or_else(|| format!("a <{}> has no name", el.name))?;
        if let Some(d) = el.attr("nodedef").filter(|d| lib.def(d).is_none()) {
            if !g.missing.iter().any(|m| m.name == d) {
                g.missing.push(described(el, d, lib));
            }
        }
        let def = match el.attr("nodedef") {
            Some(d) => g
                .find_def(lib, d)
                .cloned()
                .ok_or_else(|| format!("{name}: {d} is not in the library"))?,
            None => {
                let inputs: Vec<(&str, Type)> = el
                    .children_named("input")
                    .filter_map(|i| Some((i.attr("name")?, Type::from_name(i.attr("type")?)?)))
                    .collect();
                let out = el.attr("type").and_then(Type::from_name);
                lib.resolve(&el.name, &inputs, out)
                    .or_else(|| lib.resolve(&el.name, &[], out))
                    .cloned()
                    .ok_or_else(|| format!("{name}: no {} node takes these types", el.name))?
            }
        };
        let num = |k: &str| el.attr(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        let id = g.push(
            name.to_string(),
            def.node.clone(),
            def.name.clone(),
            [num("xpos"), num("ypos")],
        );
        for i in el.children_named("input") {
            let input = i.attr("name").ok_or_else(|| format!("{name}: an input has no name"))?;
            let Some(idef) = def.input(input) else {
                return Err(format!("{name}: {} has no input {input}", def.name));
            };
            if let Some(src) = i.attr("nodename") {
                let output = i.attr("output").unwrap_or("out");
                wires.push((id, input.to_string(), src.to_string(), output.to_string()));
                // Its place among the inputs kept; the node is found below.
                if let Some(n) = g.node_mut(id) {
                    n.inputs.push((
                        input.to_string(),
                        Binding::Connect {
                            node: u32::MAX,
                            output: output.to_string(),
                        },
                    ));
                }
            } else if let Some(text) = i.attr("value") {
                let v = idef
                    .parse(text)
                    .ok_or_else(|| format!("{name}.{input}: {text:?} is not a {}", idef.ty))?;
                g.set_value(id, input, v);
            }
        }
    }
    for (to, input, src, output) in wires {
        let from = g
            .by_name(&src)
            .map(|n| n.id)
            .ok_or_else(|| format!("{input} reads {src}, which is not in the document"))?;
        // A missing definition of several outputs has those its readers name.
        let from_def = g.node(from).map(|n| n.def.clone()).unwrap_or_default();
        let ty = g
            .node(to)
            .and_then(|n| g.find_def(lib, &n.def))
            .and_then(|d| d.input(&input))
            .map(|i| i.ty);
        if let (Some(m), Some(ty)) = (g.missing.iter_mut().find(|m| m.name == from_def), ty) {
            if m.output(&output).is_none() && !m.outputs.iter().any(|o| o.name == "out") {
                m.outputs.push(crate::library::Output {
                    name: output.clone(),
                    ty,
                    default_input: None,
                    attrs: vec![],
                });
            }
        }
        // Connected as written: the definitions are those the document gives.
        let out = match g.node(from).and_then(|n| g.find_def(lib, &n.def)) {
            Some(d) if d.output(&output).is_some() => output,
            Some(d) => d.outputs.first().map(|o| o.name.clone()).unwrap_or(output),
            None => output,
        };
        if let Some(slot) = g
            .node_mut(to)
            .and_then(|n| n.inputs.iter_mut().find(|(i, _)| *i == input))
        {
            slot.1 = Binding::Connect {
                node: from,
                output: out,
            };
        }
    }
    Ok(g)
}

/// A definition the library does not have, as a node of it in a document
/// describes it: its node, its output's type, its inputs' types. A node of a
/// renderer the library knows (by its prefix, `ai_`) is that renderer's
/// shader.
fn described(el: &xml::Element, def: &str, lib: &Library) -> crate::library::NodeDef {
    use crate::library::{Input, NodeDef, Output, Target, Ui};
    let ty = |t: Option<&str>| t.and_then(Type::from_name).unwrap_or(Type::String);
    let inputs = el
        .children_named("input")
        .filter_map(|i| {
            Some(Input {
                name: i.attr("name")?.to_string(),
                ty: ty(i.attr("type")),
                value: None,
                uniform: false,
                default_geomprop: None,
                choices: vec![],
                choice_values: None,
                doc: None,
                ui: Ui {
                    visible: true,
                    ..Default::default()
                },
                attrs: vec![],
            })
        })
        .collect();
    let outputs = match el.attr("type") {
        Some("multioutput") => vec![],
        t => vec![Output {
            name: "out".into(),
            ty: ty(t),
            default_input: None,
            attrs: vec![],
        }],
    };
    NodeDef {
        name: def.to_string(),
        node: el.name.clone(),
        group: String::new(),
        ui_name: None,
        version: None,
        is_default_version: false,
        inherit: None,
        doc: Some("Its definition is not loaded: kept as it was read".into()),
        inputs,
        outputs,
        file: String::new(),
        target: lib.renderer_of(&el.name).map(|(r, shader)| Target {
            context: r.context.clone(),
            shader: shader.to_string(),
        }),
        hidden: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Value;

    #[test]
    fn a_graph_reads_back_as_written() {
        let lib = Library::standard();
        let mut g = Graph::standard_surface(lib);
        let s = g.surface_shader().unwrap();
        let img = g.add(lib, "image", [-300.0, 40.0]).unwrap();
        g.set_value(img, "file", Value::String("tex/wood <1>.png".into()));
        g.connect(lib, img, "out", s, "base_color").unwrap();
        g.set_value(s, "specular_roughness", Value::Floats(vec![0.35]));
        let sep = g.add(lib, "separate3", [-500.0, 0.0]).unwrap();
        g.connect(lib, sep, "outg", s, "metalness").unwrap();

        let text = write(&g, lib);
        assert!(text.contains("<standard_surface name=\"standard_surface1\" type=\"surfaceshader\" nodedef=\"ND_standard_surface_surfaceshader\""), "{text}");
        assert!(
            text.contains("<input name=\"base_color\" type=\"color3\" nodename=\"image1\" />"),
            "{text}"
        );
        assert!(text.contains("value=\"tex/wood &lt;1&gt;.png\""), "{text}");
        assert!(text.contains("type=\"multioutput\""), "{text}");
        assert!(
            text.contains("<input name=\"metalness\" type=\"float\" nodename=\"separate3_1\" output=\"outg\" />"),
            "{text}"
        );

        let back = read(&text, lib).unwrap();
        assert_eq!(back.nodes.len(), g.nodes.len());
        for (a, b) in g.nodes.iter().zip(&back.nodes) {
            assert_eq!((&a.name, &a.node, &a.def, a.pos), (&b.name, &b.node, &b.def, b.pos));
        }
        let s2 = back.surface_shader().unwrap();
        assert_eq!(back.node(s2).unwrap().name, "standard_surface1");
        assert_eq!(
            back.value(lib, s2, "specular_roughness"),
            Some(&Value::Floats(vec![0.35]))
        );
        let (src, _) = back.source(s2, "base_color").unwrap();
        assert_eq!(
            back.value(lib, src, "file"),
            Some(&Value::String("tex/wood <1>.png".into()))
        );
        assert_eq!(back.source(s2, "metalness").map(|(_, o)| o), Some("outg"));
        // Written again, the same document.
        assert_eq!(write(&back, lib), text);
    }

    #[test]
    fn a_document_without_nodedefs_reads_by_its_types() {
        let lib = Library::standard();
        let doc = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <nodedef name="ND_mine" node="mine"><output name="out" type="float"/></nodedef>
  <image name="tex" type="color3">
    <input name="file" type="filename" value="wood.png" />
  </image>
  <standard_surface name="SR_wood" type="surfaceshader">
    <input name="base_color" type="color3" nodename="tex" />
  </standard_surface>
  <surfacematerial name="M_wood" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR_wood" />
  </surfacematerial>
</materialx>"#;
        let g = read(doc, lib).unwrap();
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.by_name("tex").unwrap().def, "ND_image_color3");
        assert_eq!(g.node(g.surface_shader().unwrap()).unwrap().name, "SR_wood");
        assert!(g.wire_ok(lib, g.by_name("SR_wood").unwrap().id, "base_color"));
    }

    /// A node whose definition the library does not have (a renderer's
    /// shader, its SDK's definitions not loaded) is kept: written again as it
    /// was read, its values and wires, and to USD as its renderer's.
    #[test]
    fn a_node_of_a_missing_definition_is_kept() {
        let lib = Library::standard();
        let doc = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <ai_flat name="flat1" type="surfaceshader" nodedef="ND_ai_flat" xpos="0" ypos="0">
    <input name="color" type="color3" value="1.0, 0.5, 0.0" />
    <input name="mode" type="integer" value="2" />
  </ai_flat>
  <surfacematerial name="surfacematerial1" type="material" nodedef="ND_surfacematerial" xpos="300" ypos="0">
    <input name="surfaceshader" type="surfaceshader" nodename="flat1" />
  </surfacematerial>
</materialx>
"#;
        let g = read(doc, lib).unwrap();
        assert_eq!(g.missing.len(), 1);
        assert_eq!(g.missing[0].out_type(), Type::SurfaceShader);
        assert_eq!(
            g.value(lib, g.by_name("flat1").unwrap().id, "mode"),
            Some(&Value::Integer(2))
        );
        assert_eq!(write(&g, lib), doc);
        assert!(lib.def("ND_nope").is_none());
    }

    #[test]
    fn broken_documents_say_why() {
        let lib = Library::standard();
        let bad = |body: &str| read(&format!("<materialx>{body}</materialx>"), lib).unwrap_err();
        assert!(bad("<nope name=\"a\" type=\"float\"/>").contains("no nope node"));
        assert!(
            bad("<add name=\"a\" type=\"float\"><input name=\"in1\" type=\"float\" value=\"x\"/></add>")
                .contains("is not a float")
        );
        assert!(
            bad("<add name=\"a\" type=\"float\"><input name=\"in1\" type=\"float\" nodename=\"b\"/></add>")
                .contains("not in the document")
        );
    }
}
