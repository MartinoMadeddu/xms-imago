//! Writing what a scene network changed as a USD override layer.
//!
//! The layer sublayers the file each stage was read from (or, for a stage
//! Load USD referenced or payloaded, writes that arc on its prim) and holds
//! only opinions over it: `over` prims with the attributes that changed. Opened on
//! its own (in usdview, rray, Imago), it composes to what Imago shows. What
//! changed comes from `crate::edits`: attributes no longer shared with the
//! loaded source.
//!
//! - Moved prims get their own transform again (`xformOp:transform`), so
//!   they keep following their parents.
//! - Changed geometry writes the changed attributes: `points` (and
//!   `extent`), topology, normals, widths, and primvars with their
//!   interpolation and indices. Attributes a prim lost are blocked (`None`).
//! - Geometry that can no longer be traced to its source (several prims
//!   merged into one) is written whole, in the prim's own space.
//! - Pruned prims are deactivated (`active = false`).
//! - Prims that did not come from a stage are written as new prims (`def`).
//! - Material binding and purpose changes are written as such. Materials
//!   Material Binding put in the scene are written as `def Material`s with
//!   their shaders, under `def Scope` parents.
//! - Render prims the render operators made are written as
//!   `def RenderSettings`, `def RenderProduct` and `def RenderVar` under
//!   `def Scope` parents (an `over` where the prim is a file's), and the
//!   layer names its RenderSettings prim (`renderSettingsPrimPath`).
//!
//! Prims inside a native instance cannot carry opinions in USD (instance
//! proxies are read-only), and point-instancer prototypes are changed
//! through their instancer: their edits are reported, not written, for now.
//!
//! Values are written at the current frame, as single values.

use crate::core::geo_mesh::GeoMesh;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::{DMat4, Mat4};

use crate::core::geo::{Attr, Column, Context, Geo, Role, Topology, NORMALS, POINTS, WIDTHS};
use crate::core::properties::{written, Opinions, Value, Written};
use crate::edits::{edit_set, PrimEdit};
use crate::types::{CurveBasis, CurveWrap, Prim, Placement, Purpose};
use crate::usd_scene::{StageNode, StageTree};

/// The last result of each Write USD node, for its properties panel.
static LAST: std::sync::Mutex<Vec<(crate::types::NodeId, Result<WriteReport, String>)>> = std::sync::Mutex::new(Vec::new());

pub fn remember(node: crate::types::NodeId, result: Result<WriteReport, String>) {
    let mut last = LAST.lock().unwrap();
    last.retain(|(n, _)| *n != node);
    last.push((node, result));
}

pub fn last(node: crate::types::NodeId) -> Option<Result<WriteReport, String>> {
    LAST.lock().unwrap().iter().find(|(n, _)| *n == node).map(|(_, r)| r.clone())
}

/// What a write did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WriteReport {
    /// Prims with opinions over their stage.
    pub changed:     usize,
    /// Prims deactivated.
    pub deactivated: usize,
    /// Prims written as new.
    pub added:       usize,
    /// Stages written as a reference or a payload on a prim.
    pub referenced:  usize,
    /// Edits that could not be written, and why.
    pub skipped:     Vec<String>,
}

/// Write the override layer for these primitives to `out`.
pub fn write_override(prims: &[Prim], out: &Path) -> Result<WriteReport, String> {
    if let Some(dir) = out.parent() { std::fs::create_dir_all(dir).map_err(|e| e.to_string())?; }
    let (text, report) = override_layer(prims, out)?;
    std::fs::write(out, text).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(report)
}

/// The override layer as text, for a layer to be written at `out` (paths to
/// the sources are relative to it).
pub fn override_layer(prims: &[Prim], out: &Path) -> Result<(String, WriteReport), String> {
    // Each stage once.
    let mut trees: Vec<Arc<StageTree>> = vec![];
    for p in prims {
        if let Some(t) = &p.stage {
            if !trees.iter().any(|x| Arc::ptr_eq(x, t)) { trees.push(t.clone()); }
        }
    }
    let mut root = Spec::default();
    let mut report = WriteReport::default();
    // The stages the layer sublayers: their files at their own paths. The
    // others come in by a reference or a payload on a prim.
    let sublayered: Vec<&Arc<StageTree>> = trees.iter().filter(|t| t.arc.is_none()).collect();
    // The layer's axes and unit: those of the first sublayered stage, since
    // stage metadata only comes from the root layer; Imago's (Y up, metres)
    // when there is none.
    let layer_root = sublayered.first().map(|t| t.root).unwrap_or(Mat4::IDENTITY);
    let in_a_sublayer = |path: &str| sublayered.iter().any(|t| t.node(path).is_some());
    let dir = out.parent().unwrap_or(Path::new("."));

    for tree in &trees {
        if let Some(arc) = &tree.arc {
            arc_prim(&mut root, tree, arc, layer_root, dir, &in_a_sublayer);
            report.referenced += 1;
        }
        let nodes: BTreeMap<&str, &StageNode> = tree.nodes.iter().map(|n| (n.path.as_str(), n)).collect();
        let set = edit_set(prims, tree);
        let by_path: BTreeMap<&str, &Prim> = prims.iter()
            .filter(|p| p.stage.as_ref().is_some_and(|t| Arc::ptr_eq(t, tree)))
            .map(|p| (p.path.as_str(), p)).collect();

        for e in &set.prims {
            let (Some(p), Some(node)) = (by_path.get(e.path.as_str()), nodes.get(e.path.as_str())) else { continue };
            if let Some(why) = read_only(node) {
                report.skipped.push(format!("{}: {why}", e.path));
                continue;
            }
            let spec = root.at(&e.path);
            prim_opinions(spec, p, e, node, tree);
            report.changed += 1;
        }
        for path in &set.removed {
            if let Some(why) = nodes.get(path.as_str()).and_then(|n| read_only(n)) {
                report.skipped.push(format!("{path}: removed, but {why}"));
                continue;
            }
            root.at(path).meta.push("active = false".into());
            report.deactivated += 1;
        }
        for path in &set.added {
            if let Some(p) = by_path.get(path.as_str()) {
                new_prim(root.at(path), p, tree.root);
                report.added += 1;
            }
        }
        // Values set on prims without geometry (groups, cameras, lights).
        for (path, o) in &set.opinions {
            if let Some(why) = nodes.get(path.as_str()).and_then(|n| read_only(n)) {
                report.skipped.push(format!("{path}: {why}"));
                continue;
            }
            opinions(root.at(path), o);
            report.changed += 1;
        }
    }
    // Prims that came from no stage at all (shapes, meshes made by nodes,
    // materials), in the layer's axes and unit: those of the first stage
    // when there is one, Imago's (Y up, metres) when not. Their parents are
    // Xforms, except where a stage already has a prim on that path; those
    // with materials alone below them (the Material Library Location) are
    // Scopes.
    let in_a_stage = |path: &str| trees.iter().any(|t| t.nodes.iter().any(|n| n.path == path));
    // Values set on those Xforms, carried by each primitive below them.
    let mut above: BTreeMap<String, Opinions> = BTreeMap::new();
    let mut made: Vec<&Prim> = prims.iter().filter(|p| p.stage.is_none()).collect();
    made.sort_by_key(|p| p.shading.is_some() || p.render.is_some());
    for p in made {
        if p.path.is_empty() || !p.path.starts_with('/') || p.path == "/" {
            report.skipped.push(format!("{}: not a primitive path", if p.path.is_empty() { "(no path)" } else { p.path.as_str() }));
            continue;
        }
        new_prim(root.at(&p.path), p, layer_root);
        // A render prim of a file, changed: an `over`.
        if p.render.as_ref().is_some_and(|r| r.over) { report.changed += 1; } else { report.added += 1; }
        let segments: Vec<&str> = p.path.split('/').filter(|s| !s.is_empty()).collect();
        for n in 1..segments.len() {
            let parent = format!("/{}", segments[..n].join("/"));
            let spec = root.at(&parent);
            if spec.def.is_none() && !in_a_stage(&parent) {
                spec.def = Some(if p.shading.is_some() || p.render.is_some() { "Scope" } else { "Xform" });
            }
        }
        for (path, o) in p.above.iter() { above.entry(path.clone()).or_default().merge(o); }
    }
    for (path, o) in &above {
        opinions(root.at(path), o);
        report.changed += 1;
    }

    // The prim a reference to this layer gets when it names none: the first
    // stage's first root prim, else the first prim made here (Imago does not
    // read the stage's own defaultPrim yet).
    let default_prim = sublayered.iter().flat_map(|t| t.nodes.iter()).find(|n| n.depth == 1).map(|n| n.name.clone())
        .or_else(|| trees.iter().find_map(|t| t.arc.as_ref()).and_then(|a| a.at.split('/').find(|s| !s.is_empty()).map(str::to_string)))
        .or_else(|| prims.iter().filter(|p| p.stage.is_none())
            .find_map(|p| p.path.split('/').find(|s| !s.is_empty()).map(str::to_string)));

    // The layer: the stages' files as sublayers, their axis and unit (stage
    // metadata only comes from the root layer), then the opinions.
    let mut text = String::from("#usda 1.0\n(\n    doc = \"Imago override layer\"\n");
    if let Some(name) = &default_prim { let _ = writeln!(text, "    defaultPrim = \"{name}\""); }
    // The RenderSettings prim a renderer uses when told none: the one
    // RenderScene Settings made.
    if let Some(path) = crate::core::render::layer_settings(prims) { let _ = writeln!(text, "    renderSettingsPrimPath = \"{path}\""); }
    match sublayered.first() {
        Some(t) => {
            if let Some(mpu) = t.meters_per_unit { let _ = writeln!(text, "    metersPerUnit = {}", num64(mpu)); }
            if !t.up_axis.is_empty() { let _ = writeln!(text, "    upAxis = \"{}\"", t.up_axis); }
        }
        // Only prims made in Imago: its own axes and unit.
        None if !root.children.is_empty() => text.push_str("    metersPerUnit = 1\n    upAxis = \"Y\"\n"),
        None => {}
    }
    if !sublayered.is_empty() {
        let layers: Vec<String> = sublayered.iter().map(|t| format!("@{}@", relative(&t.source, dir).display())).collect();
        let _ = writeln!(text, "    subLayers = [\n        {}\n    ]", layers.join(",\n        "));
    }
    text.push_str(")\n");
    for (name, child) in &root.children {
        text.push('\n');
        child.write(&mut text, name, 0);
    }
    Ok((text, report))
}

/// The prim a referenced or payloaded stage is at: `def` with no type (the
/// file's prim gives it one), the arc to the file's prim, and, where the
/// file's axes or unit differ from the layer's, the transform that turns
/// one into the other, on top of the prim's own (which it replaces). Its
/// parents are Xforms, unless a sublayered stage has them.
fn arc_prim(root: &mut Spec, tree: &StageTree, arc: &crate::usd_scene::StageArc, layer_root: Mat4, dir: &Path, in_a_sublayer: &dyn Fn(&str) -> bool) {
    let segments: Vec<&str> = arc.at.split('/').filter(|s| !s.is_empty()).collect();
    for n in 1..segments.len() {
        let parent = format!("/{}", segments[..n].join("/"));
        let spec = root.at(&parent);
        if spec.def.is_none() && !in_a_sublayer(&parent) { spec.def = Some("Xform"); }
    }
    let spec = root.at(&arc.at);
    if spec.def.is_none() && !in_a_sublayer(&arc.at) { spec.def = Some(""); }
    let file = relative(&tree.source, dir);
    spec.meta.push(format!("prepend {} = @{}@<{}>", if arc.payload { "payload" } else { "references" }, file.display(), arc.prim));
    let fix = layer_root.as_dmat4().inverse() * tree.root.as_dmat4();
    if !fix.abs_diff_eq(DMat4::IDENTITY, 1e-9) {
        let (own, reset) = tree.node(&arc.at).map(|n| (n.local, n.reset_xform)).unwrap_or((DMat4::IDENTITY, false));
        spec.lines.insert(0, format!("matrix4d xformOp:transform = {}", matrix(&(fix * own))));
        let order = if reset { "[\"!resetXformStack!\", \"xformOp:transform\"]" } else { "[\"xformOp:transform\"]" };
        spec.lines.insert(1, format!("uniform token[] xformOpOrder = {order}"));
    }
}

/// Why a prim cannot carry opinions, if it cannot.
fn read_only(node: &StageNode) -> Option<&'static str> {
    if node.proxy {
        Some("inside a native instance, which USD keeps read-only (edit the prototype, or de-instance it)")
    } else if node.prototype {
        Some("a point instancer's prototype (changing it through its instancer comes later)")
    } else {
        None
    }
}

// ── Prim specs ──────────────────────────────────────────────────────────────

/// A prim in the layer: `over` unless given a type (`def`), its metadata and
/// attribute lines, and its children by name.
#[derive(Default)]
struct Spec {
    def:      Option<&'static str>,
    meta:     Vec<String>,
    lines:    Vec<String>,
    children: BTreeMap<String, Spec>,
}

impl Spec {
    /// The spec at a path, made (as `over`s) on the way.
    fn at(&mut self, path: &str) -> &mut Spec {
        path.split('/').filter(|s| !s.is_empty()).fold(self, |s, name| s.children.entry(name.to_string()).or_default())
    }

    fn write(&self, out: &mut String, name: &str, depth: usize) {
        let pad = "    ".repeat(depth);
        match self.def {
            // No type: what the prim references gives it one.
            Some("") => { let _ = write!(out, "{pad}def \"{name}\""); }
            Some(ty) => { let _ = write!(out, "{pad}def {ty} \"{name}\""); }
            None => { let _ = write!(out, "{pad}over \"{name}\""); }
        }
        if !self.meta.is_empty() {
            let _ = write!(out, " (\n{}\n{pad})", self.meta.iter().map(|m| format!("{pad}    {m}")).collect::<Vec<_>>().join("\n"));
        }
        let _ = writeln!(out, "\n{pad}{{");
        for l in &self.lines { let _ = writeln!(out, "{pad}    {l}"); }
        for (i, (n, c)) in self.children.iter().enumerate() {
            if i > 0 || !self.lines.is_empty() { out.push('\n'); }
            c.write(out, n, depth + 1);
        }
        let _ = writeln!(out, "{pad}}}");
    }
}

/// The opinions for one edited prim of a stage.
fn prim_opinions(spec: &mut Spec, p: &Prim, e: &PrimEdit, node: &StageNode, tree: &StageTree) {
    let src = p.source.as_ref().expect("edits come from prims with a source");
    // Its placement when loaded: root · parents · its own transform.
    let loaded = match &src.place { Placement::One(m) => m.as_dmat4(), _ => DMat4::IDENTITY };

    if let Some(Placement::One(now)) = &e.place {
        // Same parents, new own transform: L' = L · loaded⁻¹ · now.
        let local = node.local * loaded.inverse() * now.as_dmat4();
        spec.lines.push(format!("matrix4d xformOp:transform = {}", matrix(&local)));
        let order = if node.reset_xform { "[\"!resetXformStack!\", \"xformOp:transform\"]" } else { "[\"xformOp:transform\"]" };
        spec.lines.push(format!("uniform token[] xformOpOrder = {order}"));
    }
    if let Some(m) = &e.material {
        bind(spec, m.as_deref());
    }
    if let Some(purpose) = e.purpose {
        spec.lines.push(format!("uniform token purpose = \"{}\"", purpose_token(purpose)));
    }
    for (name, value) in &e.props { property(spec, name, value); }

    if e.replaced {
        // The whole geometry, from the mesh as drawn, back in the prim's own space.
        let drawn = match &p.place { Placement::One(m) => m.as_dmat4(), _ => DMat4::IDENTITY };
        let to_local = (loaded.inverse() * drawn).as_mat4();
        let local = crate::node_graph::nodes::place_mesh(&p.mesh, &to_local);
        let geo = Geo::from_mesh(&local);
        geometry(spec, &geo, true);
        // What the source had and this does not: blocked.
        for ctx in Context::ALL {
            for (name, attr) in src.geo.attrs(ctx) {
                if geo.attr(ctx, name).is_none() { block(spec, name, attr); }
            }
        }
        return;
    }
    let Some(geo) = &p.geo else { return };
    if e.topology { topology(spec, &geo.topology, true); }
    // A point instancer: its points and their attributes are its arrays.
    let instancer = p.instancing.is_some();
    for (ctx, name) in &e.attrs {
        if let Some(attr) = geo.attr(*ctx, name) {
            if instancer { instancer_array(spec, *ctx, name, attr); } else { attribute(spec, *ctx, name, attr); }
        }
        if &**name == POINTS && !instancer { extent(spec, geo); }
    }
    for (ctx, name) in &e.removed {
        if let Some(attr) = src.geo.attr(*ctx, name) { block(spec, name, attr); }
    }
    let _ = tree;
}

/// A prim that is not in the stage (or not in any): written whole, as new.
fn new_prim(spec: &mut Spec, p: &Prim, root: Mat4) {
    // A material: its shaders below it, and no place of its own.
    if let Some(m) = &p.shading {
        spec.def = Some("Material");
        let (lines, shaders) = crate::core::material::network(m, &p.path);
        spec.lines.extend(lines);
        for s in shaders {
            let child = spec.children.entry(s.name).or_default();
            child.def = Some("Shader");
            child.lines = s.lines;
        }
        for (name, value) in p.props.iter() { property(spec, name, value); }
        return;
    }
    // A render prim: its attributes and relationships. One of a file is
    // an `over`.
    if let Some(r) = &p.render {
        spec.def = if r.over { None } else { Some(r.kind.usd()) };
        spec.lines.extend(r.lines());
        for (name, value) in p.props.iter() { property(spec, name, value); }
        return;
    }
    // A camera: where it is, and its lens.
    if let Some(c) = &p.camera {
        spec.def = Some("Camera");
        let place = match &p.place { Placement::One(m) => root.as_dmat4().inverse() * m.as_dmat4(), _ => root.as_dmat4().inverse() };
        if place != DMat4::IDENTITY {
            spec.lines.push(format!("matrix4d xformOp:transform = {}", matrix(&place)));
            spec.lines.push("uniform token[] xformOpOrder = [\"xformOp:transform\"]".into());
        }
        if p.purpose != Purpose::Default {
            spec.lines.push(format!("uniform token purpose = \"{}\"", purpose_token(p.purpose)));
        }
        spec.lines.extend(c.usda_lines());
        for (name, value) in p.props.iter() { property(spec, name, value); }
        return;
    }
    let geo = p.geo.as_deref().cloned().unwrap_or_else(|| Geo::from_mesh(&p.mesh));
    spec.def = Some(match geo.topology {
        Topology::Mesh { .. } => "Mesh",
        Topology::Curves { .. } => "BasisCurves",
        Topology::Points => "Points",
    });
    // Its placement, without the root correction the file's axes do not have.
    let place = match &p.place { Placement::One(m) => root.as_dmat4().inverse() * m.as_dmat4(), _ => root.as_dmat4().inverse() };
    if place != DMat4::IDENTITY {
        spec.lines.push(format!("matrix4d xformOp:transform = {}", matrix(&place)));
        spec.lines.push("uniform token[] xformOpOrder = [\"xformOp:transform\"]".into());
    }
    if let Some(m) = &p.material { bind(spec, Some(m)); }
    if p.purpose != Purpose::Default {
        spec.lines.push(format!("uniform token purpose = \"{}\"", purpose_token(p.purpose)));
    }
    for (name, value) in p.props.iter() { property(spec, name, value); }
    geometry(spec, &geo, false);
}

/// A Property set by Set Attribute: the prim's attribute (`uniform` where
/// the schema has it so) or its metadata, by name.
fn property(spec: &mut Spec, name: &str, value: &Value) {
    match written(name) {
        Written::Metadata => spec.meta.push(format!("{name} = {}", value.usda())),
        Written::Uniform => spec.lines.push(format!("uniform {} {name} = {}", value.kind().usd(), value.usda())),
        Written::Attribute => spec.lines.push(format!("{} {name} = {}", value.kind().usd(), value.usda())),
    }
}

/// The values set on a prim without geometry: Object ones as constant
/// primvars, Property ones as they are written on any prim.
fn opinions(spec: &mut Spec, o: &Opinions) {
    for (name, value) in &o.primvars {
        spec.lines.push(format!("{} primvars:{name} = {} (\n    interpolation = \"constant\"\n)", value.kind().usd(), value.usda()));
    }
    for (name, value) in &o.props { property(spec, name, value); }
}

/// Every attribute and the topology of a geometry.
fn geometry(spec: &mut Spec, geo: &Geo, over: bool) {
    topology(spec, &geo.topology, over);
    for ctx in Context::ALL {
        for (name, attr) in geo.attrs(ctx) { attribute(spec, ctx, name, attr); }
    }
    extent(spec, geo);
}

fn topology(spec: &mut Spec, t: &Topology, over: bool) {
    match t {
        Topology::Points => {}
        Topology::Mesh { counts, indices, left_handed, .. } => {
            spec.lines.push(format!("int[] faceVertexCounts = {}", list(counts.iter(), |c| c.to_string())));
            spec.lines.push(format!("int[] faceVertexIndices = {}", list(indices.iter(), |i| i.to_string())));
            if *left_handed || over {
                let o = if *left_handed { "leftHanded" } else { "rightHanded" };
                spec.lines.push(format!("uniform token orientation = \"{o}\""));
            }
        }
        Topology::Curves { counts, basis, wrap } => {
            let (ty, b) = match basis {
                CurveBasis::Linear => ("linear", "bezier"),
                CurveBasis::Bezier => ("cubic", "bezier"),
                CurveBasis::Bspline => ("cubic", "bspline"),
                CurveBasis::CatmullRom => ("cubic", "catmullRom"),
            };
            let w = match wrap { CurveWrap::Nonperiodic => "nonperiodic", CurveWrap::Periodic => "periodic", CurveWrap::Pinned => "pinned" };
            spec.lines.push(format!("int[] curveVertexCounts = {}", list(counts.iter(), |c| c.to_string())));
            spec.lines.push(format!("uniform token type = \"{ty}\""));
            spec.lines.push(format!("uniform token basis = \"{b}\""));
            spec.lines.push(format!("uniform token wrap = \"{w}\""));
        }
    }
}

/// The USD attribute name of an attribute: its own for the schema's
/// (`points`, `normals`, `widths`), `primvars:` for the rest.
fn usd_name(name: &str) -> String {
    match name {
        POINTS | NORMALS | WIDTHS => name.to_string(),
        _ => format!("primvars:{name}"),
    }
}

/// An attribute, with its interpolation and indices.
fn attribute(spec: &mut Spec, ctx: Context, name: &str, attr: &Attr) {
    let Some((ty, values)) = typed_values(&attr.column, attr.role) else { return };
    let usd = usd_name(name);
    if name == POINTS {
        spec.lines.push(format!("point3f[] points = {values}"));
        return;
    }
    spec.lines.push(format!("{ty}[] {usd} = {values} (\n    interpolation = \"{}\"\n)", ctx.usd_interpolation()));
    if let Some(ix) = &attr.indices {
        spec.lines.push(format!("int[] {usd}:indices = {}", list(ix.iter(), |i| i.to_string())));
    }
}

/// A point instancer's attribute as the instancer's own array: `points`
/// as `positions`, `orient` as `orientations`, `scale` as `scales`,
/// `protoIndex` as `protoIndices`, `id` as `ids`. Others as primvars.
fn instancer_array(spec: &mut Spec, ctx: Context, name: &str, attr: &Attr) {
    let (usd, ty) = match name {
        POINTS => ("positions", "point3f"),
        "orient" => ("orientations", "quath"),
        "scale" => ("scales", "float3"),
        "protoIndex" => ("protoIndices", "int"),
        "id" => ("ids", "int64"),
        _ => return attribute(spec, ctx, name, attr),
    };
    let Some((_, values)) = typed_values(&attr.column, attr.role) else { return };
    spec.lines.push(format!("{ty}[] {usd} = {values}"));
}

/// An attribute the prim no longer has: blocked, so the weaker opinion is
/// not seen through.
fn block(spec: &mut Spec, name: &str, attr: &Attr) {
    let Some((ty, _)) = typed_values(&attr.column, attr.role) else { return };
    let usd = usd_name(name);
    spec.lines.push(format!("{ty}[] {usd} = None"));
    if attr.indices.is_some() { spec.lines.push(format!("int[] {usd}:indices = None")); }
}

/// The bounds of the points, which USD wants to match them.
fn extent(spec: &mut Spec, geo: &Geo) {
    if let Some((lo, hi)) = geo.bounds() {
        spec.lines.push(format!("float3[] extent = [{}, {}]", v3(&lo), v3(&hi)));
    }
}

fn bind(spec: &mut Spec, material: Option<&str>) {
    spec.meta.push("prepend apiSchemas = [\"MaterialBindingAPI\"]".into());
    match material {
        Some(m) => spec.lines.push(format!("rel material:binding = <{m}>")),
        None => spec.lines.push("rel material:binding = None".into()),
    }
}

fn purpose_token(p: Purpose) -> &'static str {
    match p {
        Purpose::Default => "default",
        Purpose::Render { .. } => "render",
        Purpose::Proxy => "proxy",
        Purpose::Guide => "guide",
    }
}

/// The USD array type of a column, and its values as text.
fn typed_values(c: &Column, role: Role) -> Option<(&'static str, String)> {
    Some(match c {
        Column::Float(v) => ("float", list(v.iter(), |x| num(*x))),
        Column::Int(v) => ("int", list(v.iter(), |x| x.to_string())),
        Column::Bool(v) => ("bool", list(v.iter(), |x| x.to_string())),
        Column::Vec2(v) => (if role == Role::TexCoord { "texCoord2f" } else { "float2" }, list(v.iter(), |p| format!("({}, {})", num(p[0]), num(p[1])))),
        Column::Vec3(v) => (match role {
            Role::Point => "point3f",
            Role::Normal => "normal3f",
            Role::Vector => "vector3f",
            Role::Color => "color3f",
            _ => "float3",
        }, list(v.iter(), v3)),
        Column::Vec4(v) => (if role == Role::Color { "color4f" } else { "float4" },
            list(v.iter(), |p| format!("({}, {}, {}, {})", num(p[0]), num(p[1]), num(p[2]), num(p[3])))),
        Column::Quat(v) => ("quatf", list(v.iter(), |q| format!("({}, {}, {}, {})", num(q[3]), num(q[0]), num(q[1]), num(q[2])))),
        Column::Token(v) => ("token", list(v.iter(), |t| format!("\"{}\"", t.replace('"', "\\\"")))),
        Column::Mat4(_) => return None,
    })
}

// ── Text ────────────────────────────────────────────────────────────────────

fn list<T>(items: impl Iterator<Item = T>, f: impl Fn(T) -> String) -> String {
    format!("[{}]", items.map(f).collect::<Vec<_>>().join(", "))
}

/// A number as USD reads it: finite, shortest form.
fn num(x: f32) -> String { if x.is_finite() { format!("{x}") } else { "0".into() } }
fn num64(x: f64) -> String { if x.is_finite() { format!("{x}") } else { "0".into() } }

fn v3(p: &[f32; 3]) -> String { format!("({}, {}, {})", num(p[0]), num(p[1]), num(p[2])) }

/// A matrix as USD writes it: rows, with the translation in the last row.
/// glam's columns are USD's rows.
fn matrix(m: &DMat4) -> String {
    let c = m.to_cols_array_2d();
    let row = |r: &[f64; 4]| format!("({}, {}, {}, {})", num64(r[0]), num64(r[1]), num64(r[2]), num64(r[3]));
    format!("( {}, {}, {}, {} )", row(&c[0]), row(&c[1]), row(&c[2]), row(&c[3]))
}

/// `path` relative to `dir` when they share a start, else as it is.
fn relative(path: &Path, dir: &Path) -> PathBuf {
    let (Ok(path), Ok(dir)) = (path.canonicalize(), dir.canonicalize()) else { return path.to_path_buf() };
    let (p, d): (Vec<_>, Vec<_>) = (path.components().collect(), dir.components().collect());
    let common = p.iter().zip(&d).take_while(|(a, b)| a == b).count();
    if common == 0 { return path; }
    let mut out = PathBuf::from(".");
    for _ in common..d.len() { out.push(".."); }
    for c in &p[common..] { out.push(c); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EvalResult, MeshData, Source};

    /// A stage on disk, loaded as Load USD gives it: packed primitives with
    /// their sources.
    fn load(name: &str, text: &str) -> (PathBuf, Vec<Prim>) {
        let dir = std::env::temp_dir().join("xms_usd_write_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        (path.clone(), prims_of(&path))
    }

    fn prims_of(path: &Path) -> Vec<Prim> {
        let scene = crate::usd_scene::load(path).unwrap();
        scene.meshes.iter().map(|m| Prim {
            mesh: m.mesh.clone(), geo: m.geo.clone(), place: m.place.clone(), stage: Some(scene.tree.clone()),
            material: m.material.clone(), purpose: m.purpose,
            source: m.geo.clone().map(|geo| Arc::new(Source { geo, place: m.place.clone(), material: m.material.clone(), purpose: m.purpose })),
            ..Prim::new(m.path.clone(), MeshData::default())
        }).collect()
    }

    const QUAD: &str = r#"
        int[] faceVertexCounts = [4]
        int[] faceVertexIndices = [0, 1, 2, 3]
        point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)]
"#;

    fn stage_text() -> String {
        format!(r#"#usda 1.0
(
    upAxis = "Z"
    metersPerUnit = 0.01
)
def Xform "set" {{
    double3 xformOp:translate = (0, 0, 100)
    uniform token[] xformOpOrder = ["xformOp:translate"]
    def Mesh "a" {{{QUAD}    }}
    def Mesh "b" {{{QUAD}    }}
    def Mesh "c" {{{QUAD}    }}
}}
"#)
    }

    #[test]
    fn an_untouched_stage_writes_no_opinions() {
        let (_, prims) = load("untouched.usda", &stage_text());
        let out = std::env::temp_dir().join("xms_usd_write_test/untouched_over.usda");
        let (text, report) = override_layer(&prims, &out).unwrap();
        assert_eq!(report, WriteReport::default());
        assert!(!text.contains("over "), "{text}");
        assert!(text.contains("upAxis = \"Z\"") && text.contains("metersPerUnit = 0.01") && text.contains("@./untouched.usda@"), "{text}");
    }

    #[test]
    fn edits_written_and_read_back_compose_to_what_was_shown() {
        let (_, mut prims) = load("edited.usda", &stage_text());
        // a: points moved. b: moved by a Transform. c: pruned.
        let mut g = (**prims[0].geo.as_ref().unwrap()).clone();
        g.points_mut()[2] = [1.0, 1.0, 5.0];
        prims[0].geo = Some(Arc::new(g));
        prims[1].place = prims[1].place.moved(Mat4::from_translation(glam::Vec3::new(2.0, 0.0, 0.0)));
        let shown_b = prims[1].place.clone();
        prims.truncate(2);

        let out = std::env::temp_dir().join("xms_usd_write_test/edited_over.usda");
        let report = write_override(&prims, &out).unwrap();
        assert_eq!((report.changed, report.deactivated, report.added), (2, 1, 0), "{report:?}");

        // The override, opened on its own, composes over the original.
        let back = crate::usd_scene::load(&out).unwrap();
        let find = |p: &str| back.meshes.iter().find(|m| m.path == p);
        let a = find("/set/a").expect("a is there");
        assert_eq!(a.geo.as_ref().unwrap().points()[2], [1.0, 1.0, 5.0]);
        let b = find("/set/b").expect("b is there");
        let (Placement::One(now), Placement::One(was)) = (&b.place, &shown_b) else { panic!("placed") };
        assert!(now.abs_diff_eq(*was, 1e-4), "{now} vs {was}");
        assert!(find("/set/c").is_none(), "c is deactivated");
        // The same axis and unit as the original.
        assert_eq!((back.up_axis.as_str(), back.meters_per_unit), ("Z", Some(0.01)));
    }

    /// A cube made in Imago, as the Cube operator gives it.
    fn cube_at(path: &str) -> Prim {
        Prim::new(path.into(), crate::node_graph::nodes::create_cube(1.0))
    }

    #[test]
    fn a_shape_is_written_as_a_mesh_under_xforms_and_is_the_default_prim() {
        let out = std::env::temp_dir().join("xms_usd_write_test/shape_over.usda");
        let report = write_override(&[cube_at("/root/asset/cube1")], &out).unwrap();
        assert_eq!(report.added, 1, "{report:?}");
        let text = std::fs::read_to_string(&out).unwrap();
        for line in ["def Xform \"root\"", "def Xform \"asset\"", "def Mesh \"cube1\"", "defaultPrim = \"root\"", "upAxis = \"Y\"", "metersPerUnit = 1"] {
            assert!(text.contains(line), "{line} missing in\n{text}");
        }
        assert!(!text.contains("over "), "{text}");
        // Read back: the cube is there, where it was made.
        let back = crate::usd_scene::load(&out).unwrap();
        let cube = back.meshes.iter().find(|m| m.path == "/root/asset/cube1").expect("the cube is read back");
        assert_eq!(cube.geo.as_ref().unwrap().points().len(), 8);

        // On its own at the default location, the cube is the default prim.
        let (text, _) = override_layer(&[cube_at("/cube")], &out).unwrap();
        assert!(text.contains("defaultPrim = \"cube\"") && text.contains("def Mesh \"cube\""), "{text}");
    }

    /// Material Binding's result written: the materials as `def Material`s
    /// under a Scope, the bindings as `material:binding`, and read back by
    /// the USD reader Imago loads stages with.
    #[test]
    fn bound_materials_are_written_and_read_back() {
        use crate::core::material::{bind, BindTo, Material, PreviewSurface};
        let (_, mut prims) = load("bound.usda", &stage_text());
        prims.push(cube_at("/root/cube"));
        let mut paint = PreviewSurface { roughness: 0.25, metallic: 1.0, ..Default::default() };
        paint.textures.insert("diffuseColor".into(), "textures/paint.png".into());
        paint.textures.insert("normal".into(), "textures/paint_n.png".into());
        let materials = [Some(Arc::new(Material::new("paint", &paint))), Some(Arc::new(Material::new("chrome", &PreviewSurface::default())))];
        let rows = vec!["/set".to_string(), "/root".to_string()];
        let EvalResult::Named(bound) = bind(Some(&EvalResult::Named(prims)), &materials, "/materials", &rows, &[BindTo::Same, BindTo::Same]) else { panic!() };

        let out = std::env::temp_dir().join("xms_usd_write_test/bound_over.usda");
        let report = write_override(&bound, &out).unwrap();
        assert_eq!((report.changed, report.added), (3, 3), "a, b and c bound; the cube and two materials added: {report:?}");
        let text = std::fs::read_to_string(&out).unwrap();
        for line in ["def Scope \"materials\"", "def Material \"paint\"", "def Shader \"PreviewSurface\"", "def Shader \"diffuseColorTexture\"",
                     "rel material:binding = </materials/paint>", "rel material:binding = </materials/chrome>", "def Xform \"root\""] {
            assert!(text.contains(line), "{line} missing in\n{text}");
        }
        let back = crate::usd_scene::load(&out).unwrap();
        let material = |p: &str| back.meshes.iter().find(|m| m.path == p).and_then(|m| m.material.clone());
        assert_eq!(material("/set/a").as_deref(), Some("/materials/paint"));
        assert_eq!(material("/root/cube").as_deref(), Some("/materials/chrome"));
        let read = back.materials.iter().find(|m| m.path == "/materials/paint").expect("paint is read back");
        assert_eq!((read.roughness, read.metallic), (Some(0.25), Some(1.0)));
        assert!(read.textures.contains(&("diffuseColor".to_string(), "textures/paint.png".to_string())), "{:?}", read.textures);
    }

    /// Load USD in Reference mode: the file's prim at /root, its paths
    /// rebased; written as a reference on /root (with the transform from the
    /// file's axes and unit to the layer's) and edits under it, which,
    /// opened, give the prims where Imago showed them.
    #[test]
    fn a_referenced_stage_is_written_as_a_reference_and_composes_back() {
        use crate::usd_scene::LoadMode;
        let (path, _) = load("referenced.usda", &stage_text());
        let scene = crate::usd_scene::load(&path).unwrap();
        let mut prims = crate::node_graph::nodes::stage_prims(&scene, LoadMode::Reference, "/root", "").unwrap();
        let paths: Vec<&str> = prims.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, vec!["/root/a", "/root/b", "/root/c"], "the file names no default prim: its first root prim, /set");
        let b = prims.iter().position(|p| p.path == "/root/b").unwrap();
        prims[b].place = prims[b].place.moved(Mat4::from_translation(glam::Vec3::new(2.0, 0.0, 0.0)));
        let shown: Vec<(String, Placement)> = prims.iter().map(|p| (p.path.clone(), p.place.clone())).collect();

        let out = std::env::temp_dir().join("xms_usd_write_test/referencing.usda");
        let report = write_override(&prims, &out).unwrap();
        assert_eq!((report.referenced, report.changed), (1, 1), "{report:?}");
        let text = std::fs::read_to_string(&out).unwrap();
        for line in ["def \"root\"", "prepend references = @./referenced.usda@</set>", "matrix4d xformOp:transform", "upAxis = \"Y\"", "defaultPrim = \"root\"", "over \"b\""] {
            assert!(text.contains(line), "{line} missing in\n{text}");
        }
        assert!(!text.contains("subLayers"), "{text}");
        // Read back: every prim where Imago showed it.
        let back = crate::usd_scene::load(&out).unwrap();
        for (path, place) in shown {
            let m = back.meshes.iter().find(|m| m.path == path).unwrap_or_else(|| panic!("{path} is read back"));
            let (Placement::One(now), Placement::One(was)) = (&m.place, &place) else { panic!("placed") };
            assert!(now.abs_diff_eq(*was, 1e-3), "{path}: {now} vs {was}");
        }

        // Payload: the same, written as a payload.
        let prims = crate::node_graph::nodes::stage_prims(&scene, LoadMode::Payload, "/shot/set", "").unwrap();
        let (text, _) = override_layer(&prims, &out).unwrap();
        assert!(text.contains("prepend payload = @./referenced.usda@</set>") && text.contains("def Xform \"shot\""), "{text}");
    }

    /// A camera made by the Camera operator is written as a `def Camera`
    /// with its lens; a file's camera moved by a Transform as its new
    /// transform. Read back, both are where Imago showed them.
    #[test]
    fn cameras_are_written_and_read_back() {
        let pass = |_: crate::types::IceGraphId, g: &Geo, _: Option<&Geo>| g.clone();
        let Some(EvalResult::Named(made)) = crate::node_graph::nodes::evaluate_node_type(&crate::types::NodeType::camera(), &[], &pass, 0) else { panic!() };
        let out = std::env::temp_dir().join("xms_usd_write_test/camera_over.usda");
        let report = write_override(&made, &out).unwrap();
        assert_eq!(report.added, 1);
        let text = std::fs::read_to_string(&out).unwrap();
        for line in ["def Camera \"camera\"", "token projection = \"perspective\"", "float focalLength = 50", "matrix4d xformOp:transform"] {
            assert!(text.contains(line), "{line} missing in\n{text}");
        }
        let back = crate::usd_scene::load(&out).unwrap();
        let cam = back.cameras.iter().find(|c| c.path == "/camera").expect("the camera is read back");
        assert_eq!(cam.camera.focal_length, 50.0);
        let shown = made[0].place.matrices()[0].w_axis.truncate().to_array();
        assert!(cam.position().iter().zip(shown).all(|(a, b)| (a - b).abs() < 1e-4), "{:?} vs {shown:?}", cam.position());

        // A file's camera, moved.
        let (path, _) = load("with_camera.usda", &format!(r#"#usda 1.0
def Xform "shot" {{
    def Mesh "card" {{{QUAD}    }}
    def Camera "cam" {{
        float focalLength = 35
        double3 xformOp:translate = (0, 1, 8)
        uniform token[] xformOpOrder = ["xformOp:translate"]
    }}
}}
"#));
        let scene = crate::usd_scene::load(&path).unwrap();
        let mut prims = crate::node_graph::nodes::stage_prims(&scene, crate::usd_scene::LoadMode::Sublayer, "", "").unwrap();
        let k = prims.iter().position(|p| p.path == "/shot/cam").expect("the file's camera is a primitive");
        assert_eq!(prims[k].camera.as_ref().map(|c| c.focal_length), Some(35.0));
        prims[k].place = prims[k].place.moved(Mat4::from_translation(glam::Vec3::new(3.0, 0.0, 0.0)));
        let out = std::env::temp_dir().join("xms_usd_write_test/camera_moved.usda");
        let report = write_override(&prims, &out).unwrap();
        assert_eq!((report.changed, report.added), (1, 0), "only its transform: {report:?}");
        let back = crate::usd_scene::load(&out).unwrap();
        let moved = back.cameras.iter().find(|c| c.path == "/shot/cam").unwrap();
        assert!((moved.position()[0] - 3.0).abs() < 1e-4 && (moved.position()[2] - 8.0).abs() < 1e-4, "{:?}", moved.position());
    }

    /// The render operators' prims are written as `def RenderSettings`,
    /// `RenderProduct` and `RenderVar` under Scopes, with their
    /// relationships, and the layer names the RenderSettings prim. Read back,
    /// they are prims of those types.
    #[test]
    fn render_prims_are_written_and_read_back() {
        use crate::core::render::{self, RendererSettings, SceneSettings};
        let cube = cube_at("/set/cube");
        let cam = Prim { camera: Some(Arc::new(Default::default())), ..Prim::new("/cam".into(), MeshData::default()) };
        let scene = render::scene_settings(Some(&EvalResult::Named(vec![cube, cam])), &SceneSettings::default());
        let EvalResult::Named(prims) = render::renderer_settings(Some(&scene), &RendererSettings::for_renderer("karma")) else { panic!() };
        let out = std::env::temp_dir().join("xms_usd_write_test/render.usda");
        let report = write_override(&prims, &out).unwrap();
        assert_eq!(report.added, 5, "cube, camera, settings, product, the C var: {report:?}");
        let text = std::fs::read_to_string(&out).unwrap();
        for line in [
            "renderSettingsPrimPath = \"/Render/Settings\"",
            "def Scope \"Render\"", "def RenderSettings \"Settings\"", "def Scope \"Products\"", "def RenderProduct \"beauty\"",
            "def Scope \"Vars\"", "def RenderVar \"C\"",
            "uniform int2 resolution = (1920, 1080)", "rel camera = </cam>", "rel products = </Render/Products/beauty>",
            "rel orderedVars = </Render/Products/Vars/C>", "uniform string sourceName = \"C\"", "token driver:parameters:aov:name = \"C\"", "int karma:global:bucketsize = 32",
        ] {
            assert!(text.contains(line), "{line} missing in\n{text}");
        }
        let back = crate::usd_scene::load(&out).unwrap();
        for (path, ty) in [("/Render/Settings", "RenderSettings"), ("/Render/Products/beauty", "RenderProduct"), ("/Render/Products/Vars/C", "RenderVar")] {
            assert!(back.tree.nodes.iter().any(|n| n.path == path && n.type_name == ty), "{path} read back as a {ty}");
        }
    }

    /// A file's RenderSettings prim is found without a wire and changed with
    /// an `over`; the AOVs go on the file's product, appended to its own.
    #[test]
    fn a_files_render_settings_are_changed_with_overs() {
        use crate::core::render::{self, RendererSettings};
        let (path, _) = load("with_render.usda", &format!(r#"#usda 1.0
def Xform "set" {{
    def Mesh "card" {{{QUAD}    }}
}}
def Scope "Render" {{
    def RenderSettings "rs" {{
        uniform int2 resolution = (640, 480)
    }}
    def Scope "Products" {{
        def RenderProduct "main" {{
        }}
    }}
}}
"#));
        let scene = crate::usd_scene::load(&path).unwrap();
        let prims = crate::node_graph::nodes::stage_prims(&scene, crate::usd_scene::LoadMode::Sublayer, "", "").unwrap();
        let EvalResult::Named(prims) = render::renderer_settings(Some(&EvalResult::Named(prims)), &RendererSettings::for_renderer("arnold")) else { panic!() };
        let out = std::env::temp_dir().join("xms_usd_write_test/render_over.usda");
        let (text, report) = override_layer(&prims, &out).unwrap();
        assert_eq!(report.changed, 2, "the settings and the product: {report:?}");
        for line in ["over \"rs\"", "int arnold:global:AA_samples = 3", "over \"main\"",
            "append rel orderedVars = [</Render/Products/Vars/RGBA>]", "def Scope \"Vars\"", "def RenderVar \"RGBA\""] {
            assert!(text.contains(line), "{line} missing in\n{text}");
        }
        assert!(!text.contains("def RenderSettings") && !text.contains("renderSettingsPrimPath"), "the file's prim is not made again:\n{text}");
    }

    #[test]
    fn a_shape_beside_a_stage_is_added_in_the_stage_axes() {
        let (_, mut prims) = load("with_shape.usda", &stage_text());
        prims.push(cube_at("/props/cube"));
        let out = std::env::temp_dir().join("xms_usd_write_test/with_shape_over.usda");
        let (text, report) = override_layer(&prims, &out).unwrap();
        assert_eq!((report.changed, report.added), (0, 1), "{report:?}");
        assert!(text.contains("defaultPrim = \"set\"") && text.contains("def Xform \"props\""), "{text}");
        assert!(!text.contains("over \"set\""), "the stage is untouched: {text}");
        // Written in the stage's Z-up centimetres, it reads back where Imago drew it.
        std::fs::write(&out, &text).unwrap();
        let back = crate::usd_scene::load(&out).unwrap();
        let cube = back.meshes.iter().find(|m| m.path == "/props/cube").expect("the cube is read back");
        let world = Prim { place: cube.place.clone(), ..Prim::new(cube.path.clone(), (*cube.mesh).clone()) }.world().into_owned();
        let bounds = |vs: &[[f32; 3]]| vs.iter().fold((glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN)),
            |(lo, hi), v| (lo.min(glam::Vec3::from(*v)), hi.max(glam::Vec3::from(*v))));
        let (lo, hi) = bounds(&world.vertices);
        assert!(lo.abs_diff_eq(glam::Vec3::splat(-0.5), 1e-3) && hi.abs_diff_eq(glam::Vec3::splat(0.5), 1e-3), "{lo} {hi}");
    }

    #[test]
    fn a_prim_without_a_path_is_listed_as_not_written() {
        let out = std::env::temp_dir().join("xms_usd_write_test/no_path_over.usda");
        let (_, report) = override_layer(&[cube_at("")], &out).unwrap();
        assert_eq!((report.added, report.skipped.len()), (0, 1), "{report:?}");
    }

    #[test]
    fn set_attribute_values_are_written_on_their_prims() {
        use crate::core::properties::SetContext;
        use crate::core::set_attribute::{set_attributes, SetRow};
        use crate::types::EvalResult;
        let (_, prims) = load("set_attribute.usda", &stage_text());
        let row = |context, name: &str, value| SetRow { context, name: name.into(), value };
        let at = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let set = |input: Vec<Prim>, rows: &[SetRow], l: &[&str]| match set_attributes(&EvalResult::Named(input), rows, &at(l)) {
            EvalResult::Named(p) => p, _ => panic!("primitives"),
        };
        // On a mesh: a constant primvar and properties. On the group: kind.
        let prims = set(prims, &[
            row(SetContext::Object, "displayColor", Value::Color3f([1.0, 0.0, 0.0])),
            row(SetContext::Property, "doubleSided", Value::Bool(true)),
            row(SetContext::Property, "userProperties:assetId", Value::String("a-1".into())),
        ], &["/set/a"]);
        let prims = set(prims, &[row(SetContext::Property, "kind", Value::Token("assembly".into()))], &["/set"]);
        let prims = set(prims, &[row(SetContext::Property, "visibility", Value::Token("invisible".into()))], &["/set/b"]);

        let out = std::env::temp_dir().join("xms_usd_write_test/set_attribute_over.usda");
        let (text, report) = override_layer(&prims, &out).unwrap();
        assert_eq!((report.changed, report.deactivated, report.added), (3, 0, 0), "{report:?}\n{text}");
        for line in [
            "over \"set\" (\n    kind = \"assembly\"\n)",
            "color3f[] primvars:displayColor = [(1, 0, 0)] (\n    interpolation = \"constant\"",
            "uniform bool doubleSided = true",
            "string userProperties:assetId = \"a-1\"",
            "token visibility = \"invisible\"",
        ] {
            assert!(text.contains(line), "{line} missing in\n{text}");
        }
        // Read back: b is invisible, so not drawn; a carries its colour.
        std::fs::write(&out, &text).unwrap();
        let back = crate::usd_scene::load(&out).unwrap();
        assert!(back.meshes.iter().all(|m| m.path != "/set/b"), "b is invisible");
        let a = back.meshes.iter().find(|m| m.path == "/set/a").expect("a is there");
        assert!(a.geo.as_ref().unwrap().attr(Context::Object, "displayColor").is_some());

        // A group deactivated: one opinion on it, nothing below it written.
        let two = format!("#usda 1.0\ndef Xform \"set\" {{\n    def Mesh \"a\" {{{QUAD}    }}\n}}\ndef Xform \"props\" {{\n    def Mesh \"b\" {{{QUAD}    }}\n}}\n");
        let (_, prims) = load("set_attribute_off.usda", &two);
        let prims = set(prims, &[row(SetContext::Property, "active", Value::Bool(false))], &["/props"]);
        assert_eq!(prims.iter().map(|p| p.path.as_str()).collect::<Vec<_>>(), vec!["/set/a"], "what is below /props left the scene");
        let (text, report) = override_layer(&prims, &out).unwrap();
        assert_eq!((report.changed, report.deactivated), (1, 0), "{report:?}\n{text}");
        assert!(text.contains("over \"props\" (\n    active = false\n)") && !text.contains("\"b\""), "{text}");
    }

    #[test]
    fn values_on_the_xforms_above_a_shape_are_written_there() {
        use crate::core::properties::SetContext;
        use crate::core::set_attribute::{set_attributes, SetRow};
        use crate::types::EvalResult;
        let rows = [
            SetRow { context: SetContext::Object, name: "displayColor".into(), value: Value::Color3f([0.0, 1.0, 0.0]) },
            SetRow { context: SetContext::Property, name: "visibility".into(), value: Value::Token("invisible".into()) },
        ];
        let shapes = vec![cube_at("/root/asset/geo/cube1"), cube_at("/root/asset/geo/cube2"), cube_at("/root/other")];
        let EvalResult::Named(prims) = set_attributes(&EvalResult::Named(shapes), &rows, &["/root/asset/geo".to_string()]) else { panic!() };
        let out = std::env::temp_dir().join("xms_usd_write_test/above_over.usda");
        let (text, report) = override_layer(&prims, &out).unwrap();
        assert_eq!((report.added, report.changed), (3, 1), "once, on the Xform: {report:?}");
        assert_eq!(text.matches("primvars:displayColor").count(), 1, "{text}");
        for line in ["def Xform \"geo\"", "color3f primvars:displayColor = (0, 1, 0) (\n    interpolation = \"constant\"", "token visibility = \"invisible\""] {
            assert!(text.contains(line), "{line} missing in\n{text}");
        }
        // Read back: what is below the invisible Xform is not drawn.
        std::fs::write(&out, &text).unwrap();
        let back = crate::usd_scene::load(&out).unwrap();
        let paths: Vec<&str> = back.meshes.iter().map(|m| m.path.as_str()).collect();
        assert_eq!(paths, vec!["/root/other"], "{text}");
    }

    /// A point instancer whose points moved: its positions are written, its
    /// prototypes' copies are not edits of their own, and read back the
    /// instances are where they were moved.
    #[test]
    fn a_point_instancer_writes_its_arrays() {
        use crate::types::EvalResult;
        let text = format!(r#"#usda 1.0
def PointInstancer "inst" {{
    rel prototypes = [</inst/protos/box>]
    int[] protoIndices = [0, 0]
    point3f[] positions = [(0, 0, 0), (5, 0, 0)]
    def Scope "protos" {{
        def Mesh "box" {{{QUAD}    }}
    }}
}}
"#);
        let path = std::env::temp_dir().join("xms_usd_write_test/instancer.usda");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        let scene = crate::usd_scene::load(&path).unwrap();
        assert_eq!(scene.instancers.len(), 1);
        // As Load USD gives it: the prototype mesh, and the instancer.
        let inst = &scene.instancers[0];
        let mut prims = prims_of(&path);
        prims.iter_mut().for_each(|p| {
            p.instanced_by = inst.members.iter().find(|(m, _, _)| *m == p.path).map(|(_, index, inside)| Arc::new(crate::types::InstancedBy {
                instancer: inst.path.clone(), index: *index, inside: *inside, hidden: inst.hidden.clone(), geo: inst.geo.clone(), place: inst.place.clone(),
            }));
        });
        let mut moved = (*inst.geo).clone();
        moved.points_mut()[1] = [7.0, 0.0, 0.0];
        prims.push(Prim {
            // The same stage as the prototype mesh, as Load USD gives them.
            geo: Some(Arc::new(moved)), place: inst.place.clone(), stage: prims[0].stage.clone(), instancing: Some(inst.prototypes.clone()),
            source: Some(Arc::new(Source { geo: inst.geo.clone(), place: inst.place.clone(), material: None, purpose: inst.purpose })),
            ..Prim::new(inst.path.clone(), (*inst.mesh).clone())
        });
        let EvalResult::Named(prims) = crate::core::instancing::sync(EvalResult::Named(prims)) else { panic!() };
        let box_ = prims.iter().find(|p| p.path.ends_with("box")).unwrap();
        assert_eq!(box_.place.matrices()[1].w_axis.x, 7.0, "the copies follow the points");

        let out = std::env::temp_dir().join("xms_usd_write_test/instancer_over.usda");
        let (text, report) = override_layer(&prims, &out).unwrap();
        assert!(text.contains("point3f[] positions = [(0, 0, 0), (7, 0, 0)]"), "{text}");
        assert_eq!(text.matches("instancer.usda@").count(), 1, "one stage, one sublayer: {text}");
        assert!(report.skipped.is_empty() && report.changed == 1, "only the instancer: {report:?}\n{text}");
        std::fs::write(&out, &text).unwrap();
        let back = crate::usd_scene::load(&out).unwrap();
        let b = back.meshes.iter().find(|m| m.path.ends_with("box")).expect("the prototype is read back");
        assert_eq!(b.place.matrices().len(), 2);
        assert!((b.place.matrices()[1].w_axis.x - 7.0).abs() < 1e-4);
    }

    #[test]
    fn opinions_on_a_prim_without_geometry() {
        let mut spec = Spec::default();
        let mut o = Opinions::default();
        o.primvars.insert("variant".into(), Value::Int(3));
        o.props.insert("active".into(), Value::Bool(false));
        o.props.insert("purpose".into(), Value::Token("guide".into()));
        opinions(&mut spec, &o);
        assert_eq!(spec.lines[0], "int primvars:variant = 3 (\n    interpolation = \"constant\"\n)");
        assert_eq!(spec.lines[1], "uniform token purpose = \"guide\"");
        assert_eq!(spec.meta, vec!["active = false".to_string()]);
    }

    #[test]
    fn primvars_keep_their_interpolation_indices_and_type() {
        let mut spec = Spec::default();
        let st = Attr::indexed(Column::Vec2(Arc::new(vec![[0.0, 0.0], [1.0, 1.0]])), vec![0, 1, 1, 0], Role::TexCoord);
        attribute(&mut spec, Context::Corner, "st", &st);
        assert!(spec.lines[0].starts_with("texCoord2f[] primvars:st = [(0, 0), (1, 1)]"), "{}", spec.lines[0]);
        assert!(spec.lines[0].contains("interpolation = \"faceVarying\""));
        assert_eq!(spec.lines[1], "int[] primvars:st:indices = [0, 1, 1, 0]");
        block(&mut spec, "displayColor", &Attr::new(Column::Vec3(Arc::new(vec![[1.0, 0.0, 0.0]])), Role::Color));
        assert_eq!(spec.lines[2], "color3f[] primvars:displayColor = None");
    }
}
