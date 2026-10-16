//! A MaterialX graph: nodes of the library, their values and connections.
//!
//! As in a MaterialX document, a connection belongs to the input it goes
//! into: an input is either a value or the output of another node. A node
//! keeps only what is set on it; anything else is its definition's default.
//! Nodes are known by a number that stays the same when they are renamed.
//!
//! A node is one node whatever types it takes: connecting may change which
//! of its definitions it is (`connect`), as MaterialX's own editors do.

use crate::library::{Library, NodeDef};
use crate::types::{Type, Value};

/// What an input of a node is set to.
#[derive(Clone, Debug, PartialEq)]
pub enum Binding {
    Value(Value),
    /// The output `output` of the node numbered `node`.
    Connect { node: u32, output: String },
}

/// A node of a graph.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Its number in the graph: stays when it is renamed.
    pub id: u32,
    /// Its name, unique in the graph; what a document and USD call it.
    pub name: String,
    /// The MaterialX node it is: `add`, `image`, `standard_surface`.
    pub node: String,
    /// The definition it is (`ND_add_color3`), from what is connected.
    pub def: String,
    /// Where it is drawn: its top-left corner, in canvas units.
    pub pos: [f32; 2],
    /// Its inputs that are set, in the order they were set.
    pub inputs: Vec<(String, Binding)>,
}

impl Node {
    /// What an input is set to, if it is.
    pub fn binding(&self, input: &str) -> Option<&Binding> {
        self.inputs.iter().find(|(n, _)| n == input).map(|(_, b)| b)
    }

    fn set(&mut self, input: &str, b: Binding) {
        match self.inputs.iter_mut().find(|(n, _)| n == input) {
            Some(slot) => slot.1 = b,
            None => self.inputs.push((input.to_string(), b)),
        }
    }

    fn unset(&mut self, input: &str) { self.inputs.retain(|(n, _)| n != input); }
}

/// A graph of MaterialX nodes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    pub nodes: Vec<Node>,
    next: u32,
}

/// A name MaterialX and USD take: letters, digits and `_`, not starting
/// with a digit.
pub fn identifier(name: &str) -> String {
    let mut out: String = name.trim().chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if out.is_empty() {
        out = "node".into();
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

impl Graph {
    pub fn new() -> Graph { Graph::default() }

    /// A new material: a `standard_surface` into a `surfacematerial`.
    pub fn standard_surface(lib: &Library) -> Graph {
        let mut g = Graph::new();
        let surface = g.add(lib, "standard_surface", [0.0, 0.0]).expect("standard_surface is in the library");
        let material = g.add(lib, "surfacematerial", [320.0, 0.0]).expect("surfacematerial is in the library");
        g.connect(lib, surface, "out", material, "surfaceshader").expect("a surface shader goes into a material");
        g
    }

    pub fn node(&self, id: u32) -> Option<&Node> { self.nodes.iter().find(|n| n.id == id) }
    pub fn node_mut(&mut self, id: u32) -> Option<&mut Node> { self.nodes.iter_mut().find(|n| n.id == id) }
    pub fn by_name(&self, name: &str) -> Option<&Node> { self.nodes.iter().find(|n| n.name == name) }

    /// The definition a node is.
    pub fn def<'l>(&self, lib: &'l Library, id: u32) -> Option<&'l NodeDef> { lib.def(&self.node(id)?.def) }

    /// `base` if no node has it, else `base` followed by the first number
    /// that makes it unique.
    pub fn unique_name(&self, base: &str) -> String {
        let base = identifier(base);
        if self.by_name(&base).is_none() {
            return base;
        }
        let stem = base.trim_end_matches(|c: char| c.is_ascii_digit());
        (1..).map(|k| format!("{stem}{k}")).find(|n| self.by_name(n).is_none()).unwrap()
    }

    /// Add a node of the library, as its default definition, named after
    /// it and numbered (`image1`). None when the library has no such node.
    pub fn add(&mut self, lib: &Library, node: &str, pos: [f32; 2]) -> Option<u32> {
        let def = lib.default_def(node)?;
        let id = self.next;
        self.next += 1;
        // `image1`, `image2`; `separate3_1` for a node ending in a digit.
        let stem = if node.ends_with(|c: char| c.is_ascii_digit()) { format!("{node}_") } else { node.to_string() };
        let name = self.unique_name(&format!("{stem}1"));
        self.nodes.push(Node { id, name, node: node.to_string(), def: def.name.clone(), pos, inputs: vec![] });
        Some(id)
    }

    /// Add a node as read from a document: its definition given.
    pub(crate) fn push(&mut self, name: String, node: String, def: String, pos: [f32; 2]) -> u32 {
        let id = self.next;
        self.next += 1;
        self.nodes.push(Node { id, name, node, def, pos, inputs: vec![] });
        id
    }

    /// Remove a node, and what it fed.
    pub fn remove(&mut self, id: u32) -> bool {
        let before = self.nodes.len();
        self.nodes.retain(|n| n.id != id);
        for n in &mut self.nodes {
            n.inputs.retain(|(_, b)| !matches!(b, Binding::Connect { node, .. } if *node == id));
        }
        self.nodes.len() != before
    }

    /// Rename a node: made an identifier, and unique. Returns the name it got.
    pub fn rename(&mut self, id: u32, name: &str) -> String {
        let name = identifier(name);
        let taken = self.nodes.iter().any(|n| n.id != id && n.name == name);
        let name = if taken { self.unique_name(&name) } else { name };
        if let Some(n) = self.node_mut(id) {
            n.name = name.clone();
        }
        name
    }

    /// What an input is, set or by default: its value, or None when it is
    /// connected or has no value.
    pub fn value<'a>(&'a self, lib: &'a Library, id: u32, input: &str) -> Option<&'a Value> {
        let node = self.node(id)?;
        match node.binding(input) {
            Some(Binding::Value(v)) => Some(v),
            Some(Binding::Connect { .. }) => None,
            None => lib.def(&node.def)?.input(input)?.value.as_ref(),
        }
    }

    pub fn set_value(&mut self, id: u32, input: &str, v: Value) {
        if let Some(n) = self.node_mut(id) {
            n.set(input, Binding::Value(v));
        }
    }

    /// Back to the definition's default, and not connected.
    pub fn reset(&mut self, id: u32, input: &str) {
        if let Some(n) = self.node_mut(id) {
            n.unset(input);
        }
    }

    /// The node and output an input is connected to.
    pub fn source(&self, id: u32, input: &str) -> Option<(u32, &str)> {
        match self.node(id)?.binding(input)? {
            Binding::Connect { node, output } => Some((*node, output.as_str())),
            Binding::Value(_) => None,
        }
    }

    /// The type of a node's output.
    pub fn output_type(&self, lib: &Library, id: u32, output: &str) -> Option<Type> {
        self.def(lib, id)?.output(output).map(|o| o.ty)
    }

    /// The type each connected input of a node receives.
    pub fn connected_types(&self, lib: &Library, id: u32) -> Vec<(String, Type)> {
        let Some(node) = self.node(id) else { return vec![] };
        node.inputs
            .iter()
            .filter_map(|(input, b)| match b {
                Binding::Connect { node, output } => self.output_type(lib, *node, output).map(|t| (input.clone(), t)),
                Binding::Value(_) => None,
            })
            .collect()
    }

    /// Every node `id` reads from, and `id`: each once, sources first.
    pub fn upstream(&self, id: u32) -> Vec<u32> {
        let mut out = vec![];
        self.visit(id, &mut out, 0);
        out
    }

    fn visit(&self, id: u32, out: &mut Vec<u32>, depth: usize) {
        if out.contains(&id) || depth > 256 {
            return;
        }
        let Some(node) = self.node(id) else { return };
        for (_, b) in &node.inputs {
            if let Binding::Connect { node, .. } = b {
                self.visit(*node, out, depth + 1);
            }
        }
        out.push(id);
    }

    /// Connecting `from` into `to` would make a loop.
    pub fn would_loop(&self, from: u32, to: u32) -> bool { from == to || self.upstream(from).contains(&to) }

    /// The definition `id` would be with `input` receiving `ty`, keeping
    /// what else is connected: the one it is when that takes it, else the
    /// first that does, with its output type kept if it can be.
    fn retyped_for_input<'l>(&self, lib: &'l Library, id: u32, input: &str, ty: Type) -> Option<&'l NodeDef> {
        let node = self.node(id)?;
        let current = lib.def(&node.def)?;
        let mut connected: Vec<(String, Type)> =
            self.connected_types(lib, id).into_iter().filter(|(n, _)| n != input).collect();
        connected.push((input.to_string(), ty));
        let refs: Vec<(&str, Type)> = connected.iter().map(|(n, t)| (n.as_str(), *t)).collect();
        if Library::fits(current, &refs, None) {
            return Some(current);
        }
        lib.resolve(&node.node, &refs, Some(current.out_type())).or_else(|| lib.resolve(&node.node, &refs, None))
    }

    /// The definitions that make `id`'s output `output` give `ty`: `id`
    /// made the first definition of itself that does, and, where what is
    /// connected to it then no longer fits, the nodes it reads made other
    /// definitions too, up the graph (an `image` into a `multiply` into a
    /// colour makes both colour nodes). None when no such definitions are.
    fn plan_output(&self, lib: &Library, id: u32, output: &str, ty: Type, depth: usize) -> Option<Vec<(u32, String)>> {
        let node = self.node(id)?;
        if depth > 8 {
            return None;
        }
        'defs: for d in lib.variants(&node.node) {
            if d.output(output).is_none_or(|o| o.ty != ty) {
                continue;
            }
            let mut plan = vec![(id, d.name.clone())];
            for (input, b) in &node.inputs {
                let Binding::Connect { node: src, output: src_out } = b else { continue };
                let Some(want) = d.input(input).map(|i| i.ty) else { continue 'defs };
                if self.output_type(lib, *src, src_out) == Some(want) {
                    continue;
                }
                match self.plan_output(lib, *src, src_out, want, depth + 1) {
                    Some(up) => plan.extend(up),
                    None => continue 'defs,
                }
            }
            return Some(plan);
        }
        None
    }

    /// Make a node another definition of itself: values set on inputs it
    /// no longer has, or of another type, are dropped.
    fn retype(&mut self, lib: &Library, id: u32, def: &NodeDef) {
        let Some(node) = self.node_mut(id) else { return };
        if node.def == def.name {
            return;
        }
        node.def = def.name.clone();
        node.inputs.retain(|(name, b)| match (def.input(name), b) {
            (None, _) => false,
            (Some(i), Binding::Value(v)) => Value::parse(i.ty, &v.to_text()).as_ref() == Some(v),
            (Some(_), Binding::Connect { .. }) => true,
        });
        let _ = lib;
    }

    /// Connect the output `output` of `from` into `input` of `to`. When the
    /// types differ, `to` becomes the definition of itself that takes the
    /// type (an `add` given a colour becomes `ND_add_color3`); failing
    /// that, `from` becomes the definition that gives the type (an `image`
    /// into a colour reads a colour), and the nodes it reads with it where
    /// they must (`plan_output`). Refused, with why, when neither can,
    /// when it would make a loop, or when there is no such port.
    pub fn connect(&mut self, lib: &Library, from: u32, output: &str, to: u32, input: &str) -> Result<(), String> {
        let (Some(src), Some(dst)) = (self.node(from), self.node(to)) else {
            return Err("no such node".into());
        };
        if self.would_loop(from, to) {
            return Err(format!("{} already reads from {}: that would make a loop", src.name, dst.name));
        }
        let dst_def = lib.def(&dst.def).ok_or_else(|| format!("{} is not in the library", dst.def))?;
        let src_def = lib.def(&src.def).ok_or_else(|| format!("{} is not in the library", src.def))?;
        let src_ty = src_def.output(output).map(|o| o.ty).ok_or_else(|| format!("{} has no output {output}", src.name))?;
        // The input must be one the node has in some definition.
        if !lib.variants(&dst.node).any(|d| d.input(input).is_some()) {
            return Err(format!("{} has no input {input}", dst.name));
        }
        let (src_name, dst_name) = (src.name.clone(), dst.name.clone());
        let want = dst_def.input(input).map(|i| i.ty);
        if want != Some(src_ty) {
            if let Some(def) = self.retyped_for_input(lib, to, input, src_ty) {
                self.retype(lib, to, def);
            } else if let Some(plan) = want.and_then(|t| self.plan_output(lib, from, output, t, 0)) {
                for (id, def) in plan {
                    if let Some(def) = lib.def(&def) {
                        self.retype(lib, id, def);
                    }
                }
            } else {
                let into = want.map(|t| format!(" ({t})")).unwrap_or_default();
                return Err(format!("{src_name}.{output} is a {src_ty}: {dst_name}.{input}{into} does not take it"));
            }
        }
        if let Some(n) = self.node_mut(to) {
            n.set(input, Binding::Connect { node: from, output: output.to_string() });
        }
        Ok(())
    }

    /// Take the connection off an input: it goes back to its default.
    pub fn disconnect(&mut self, to: u32, input: &str) {
        if self.source(to, input).is_some() {
            self.reset(to, input);
        }
    }

    /// Whether what is connected to an input is of its type (a wire made
    /// wrong by a later change is drawn red).
    pub fn wire_ok(&self, lib: &Library, to: u32, input: &str) -> bool {
        let Some((from, output)) = self.source(to, input) else { return true };
        let want = self.def(lib, to).and_then(|d| d.input(input)).map(|i| i.ty);
        want.is_some() && self.output_type(lib, from, output) == want
    }

    /// The inputs of `to` that the output `output` of `from` can go into,
    /// as they are or with `to` or `from` made another definition: what a
    /// wire dropped on the node offers.
    pub fn inputs_taking(&self, lib: &Library, from: u32, output: &str, to: u32) -> Vec<String> {
        let (Some(src_ty), Some(dst)) = (self.output_type(lib, from, output), self.node(to)) else { return vec![] };
        if self.would_loop(from, to) {
            return vec![];
        }
        let mut names: Vec<String> = vec![];
        for d in lib.variants(&dst.node) {
            for i in &d.inputs {
                if names.contains(&i.name) {
                    continue;
                }
                let fits = self.retyped_for_input(lib, to, &i.name, src_ty).is_some()
                    || d.name == dst.def && self.plan_output(lib, from, output, i.ty, 0).is_some();
                if fits {
                    names.push(i.name.clone());
                }
            }
        }
        // In the order the node's current definition lists them.
        if let Some(cur) = self.def(lib, to) {
            names.sort_by_key(|n| cur.inputs.iter().position(|i| &i.name == n).unwrap_or(usize::MAX));
        }
        names
    }

    /// The material the graph puts out: its first `surfacematerial` node.
    pub fn material(&self) -> Option<u32> { self.nodes.iter().find(|n| n.node == "surfacematerial").map(|n| n.id) }

    /// The node feeding a material's surface: its `surfaceshader` input.
    pub fn surface_shader(&self) -> Option<u32> { self.source(self.material()?, "surfaceshader").map(|(n, _)| n) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> &'static Library { Library::standard() }

    #[test]
    fn a_new_material_is_a_standard_surface() {
        let g = Graph::standard_surface(lib());
        assert_eq!(g.nodes.len(), 2);
        let m = g.material().unwrap();
        assert_eq!(g.node(m).unwrap().name, "surfacematerial1");
        let s = g.surface_shader().unwrap();
        assert_eq!(g.node(s).unwrap().name, "standard_surface1");
        assert_eq!(g.value(lib(), s, "base_color"), Some(&Value::Floats(vec![0.8, 0.8, 0.8])));
        assert!(g.wire_ok(lib(), m, "surfaceshader"));
    }

    #[test]
    fn names_are_unique_identifiers() {
        let mut g = Graph::new();
        let a = g.add(lib(), "add", [0.0, 0.0]).unwrap();
        let b = g.add(lib(), "add", [0.0, 0.0]).unwrap();
        assert_eq!(g.node(a).unwrap().name, "add1");
        assert_eq!(g.node(b).unwrap().name, "add2");
        assert_eq!(g.rename(b, "my node"), "my_node");
        assert_eq!(g.rename(a, "my_node"), "my_node1");
        assert_eq!(g.rename(a, "2x"), "_2x");
        assert!(g.add(lib(), "no_such_node", [0.0, 0.0]).is_none());
    }

    #[test]
    fn connecting_picks_the_definitions() {
        let lib = lib();
        let mut g = Graph::standard_surface(lib);
        let s = g.surface_shader().unwrap();
        // An image starts as a float image; into a colour it reads a colour.
        let img = g.add(lib, "image", [-300.0, 0.0]).unwrap();
        assert_eq!(g.node(img).unwrap().def, "ND_image_float");
        g.connect(lib, img, "out", s, "base_color").unwrap();
        assert_eq!(g.node(img).unwrap().def, "ND_image_color3");
        assert!(g.wire_ok(lib, s, "base_color"));
        assert_eq!(g.value(lib, s, "base_color"), None);
        // An add given the colour becomes a colour add.
        let add = g.add(lib, "add", [-150.0, 0.0]).unwrap();
        g.connect(lib, img, "out", add, "in1").unwrap();
        assert_eq!(g.node(add).unwrap().def, "ND_add_color3");
        // A float into in2 then: the colour-plus-float add.
        let f = g.add(lib, "constant", [-300.0, 100.0]).unwrap();
        assert_eq!(g.node(f).unwrap().def, "ND_constant_float");
        g.connect(lib, f, "out", add, "in2").unwrap();
        assert_eq!(g.node(add).unwrap().def, "ND_add_color3FA");
        g.connect(lib, add, "out", s, "base_color").unwrap();
        assert!(g.wire_ok(lib, s, "base_color"));
        assert_eq!(g.upstream(s).len(), 4);
        // Up the graph: a float image into a float multiply, then into a
        // colour: both become colour nodes.
        let img2 = g.add(lib, "image", [-450.0, 0.0]).unwrap();
        let mul = g.add(lib, "multiply", [-300.0, 0.0]).unwrap();
        g.connect(lib, img2, "out", mul, "in1").unwrap();
        assert_eq!(g.node(mul).unwrap().def, "ND_multiply_float");
        g.connect(lib, mul, "out", s, "specular_color").unwrap();
        assert_eq!(g.node(mul).unwrap().def, "ND_multiply_color3");
        assert_eq!(g.node(img2).unwrap().def, "ND_image_color3");
        assert!(g.wire_ok(lib, mul, "in1") && g.wire_ok(lib, s, "specular_color"));
    }

    #[test]
    fn wrong_connections_are_refused() {
        let lib = lib();
        let mut g = Graph::standard_surface(lib);
        let s = g.surface_shader().unwrap();
        let m = g.material().unwrap();
        // A material is not a colour.
        let err = g.connect(lib, m, "out", s, "base_color").unwrap_err();
        assert!(err.contains("material"), "{err}");
        // No loops.
        assert!(g.connect(lib, m, "out", m, "surfaceshader").is_err());
        let add = g.add(lib, "add", [0.0, 0.0]).unwrap();
        let mul = g.add(lib, "multiply", [0.0, 0.0]).unwrap();
        g.connect(lib, add, "out", mul, "in1").unwrap();
        assert!(g.connect(lib, mul, "out", add, "in1").unwrap_err().contains("loop"));
        assert!(g.connect(lib, add, "out", mul, "nope").is_err());
    }

    #[test]
    fn values_and_defaults() {
        let lib = lib();
        let mut g = Graph::standard_surface(lib);
        let s = g.surface_shader().unwrap();
        g.set_value(s, "base", Value::Floats(vec![0.5]));
        assert_eq!(g.value(lib, s, "base"), Some(&Value::Floats(vec![0.5])));
        g.reset(s, "base");
        assert_eq!(g.value(lib, s, "base"), Some(&Value::Floats(vec![1.0])));
        // Removing a node takes its wires with it.
        g.remove(s);
        let m = g.material().unwrap();
        assert!(g.source(m, "surfaceshader").is_none());
        assert!(g.surface_shader().is_none());
    }

    #[test]
    fn a_wire_dropped_on_a_node_offers_the_inputs_that_take_it() {
        let lib = lib();
        let mut g = Graph::standard_surface(lib);
        let s = g.surface_shader().unwrap();
        let img = g.add(lib, "image", [0.0, 0.0]).unwrap();
        let offered = g.inputs_taking(lib, img, "out", s);
        // Float inputs as the image is, colour inputs by making it a colour image.
        assert!(offered.iter().any(|n| n == "base"));
        assert!(offered.iter().any(|n| n == "base_color"));
        // A normal, by making it a vector image.
        assert!(offered.iter().any(|n| n == "normal"));
        assert_eq!(offered[0], "base");
        // A material takes the surface shader only.
        let m = g.material().unwrap();
        assert_eq!(g.inputs_taking(lib, s, "out", m), ["surfaceshader", "backsurfaceshader"]);
    }
}
