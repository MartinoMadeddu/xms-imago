//! Set Attribute: one value per primitive, set by the scene graph. The
//! values and how they are inherited are `core::properties`.
//!
//! An operator holds rows; each sets one value on the primitives its Prims
//! Location tab names. A row's context says what the value is in USD:
//!
//! | Context    | Written as                                       |
//! |------------|--------------------------------------------------|
//! | `Object`   | `primvars:name`, `interpolation = "constant"`    |
//! | `Property` | the prim's own attribute or metadata, by name    |
//!
//! Where a value lands in Imago:
//!
//! - On a geometry primitive (a `Prim`), an Object value is an Object
//!   attribute of its `Geo`, as one read from a file is: the Primitive
//!   Inspector and ICE see it, and Write USD writes it as any changed
//!   attribute. A Property value is kept in the primitive's `props`, apart
//!   from `purpose`, which is the primitive's purpose, and `active = off`,
//!   which takes the primitive out of the scene (written as deactivated).
//! - On a prim of a stage without geometry (an Xform, a camera, a light, a
//!   material), values are opinions in its stage tree (`StageTree::opinions`),
//!   written as `over`s on that path. `active = off` there takes what is
//!   below it out of the scene too, and `visibility = invisible` hides it in
//!   the Viewer, as USD inherits them.
//! - On a prim Imago made above its primitives (the Xforms of a Primitive
//!   Location, `/root/asset` above `/root/asset/geo/cube`), values are
//!   opinions each primitive below carries (`Prim::above`), written by Write
//!   USD on that Xform.
//!
//! What is set on a prim is inherited by the primitives below it, in Imago
//! as in USD: `visibility` invisible hides them, `purpose` comes from the
//! nearest prim that has one (authored in the file or set here,
//! `purpose_from`), `active` off takes them out, and Object
//! values reach them where they have no attribute of that name (`above`,
//! `purpose_of`, `inherited_primvars`). The Primitive Inspector shows the
//! inherited Object values; ICE reads only a primitive's own.
//!
//! Unlike the other operators' Primitive Locations, a location here names
//! the prim at that path, not everything below it (`locations::names`): a
//! value on a group is one opinion on the group. `/set/**` names `/set` and
//! everything below it, `/set/*` its children.
//!
//! Values per point, face or corner are not set here: they are ICE's.

use crate::core::geo_mesh::GeoMesh;
use std::sync::Arc;

use crate::core::geo::{Context, Geo};
use crate::core::locations;
use crate::core::properties::{is_below, parents, purpose, Opinions, Preset, SetContext, Value, ValueType};
use crate::types::{EvalResult, Prim};

/// One row of a Set Attribute operator: one value.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SetRow {
    pub context: SetContext,
    pub name:    String,
    pub value:   Value,
}

impl SetRow {
    /// A row for a preset, with its first value.
    pub fn from_preset(context: SetContext, p: &Preset) -> SetRow {
        let value = match (p.ty, p.tokens.first()) {
            (ValueType::Token, Some(t)) => Value::Token(t.to_string()),
            (ValueType::Bool, _) => Value::Bool(p.name != "active"),
            (ValueType::Color3f, _) => Value::Color3f([0.8, 0.8, 0.8]),
            (ValueType::Float, _) if p.name == "displayOpacity" => Value::Float(1.0),
            (ty, _) => ty.convert(&Value::default()),
        };
        SetRow { context, name: p.name.to_string(), value }
    }

    fn name(&self) -> &str { self.name.trim() }

    /// Whether the row sets anything: a name, and for Object a type a column holds.
    pub fn is_usable(&self) -> bool {
        !self.name().is_empty() && (self.context == SetContext::Property || self.value.attr().is_some())
    }

    /// Whether it takes what it is set on out of the scene.
    fn deactivates(&self) -> bool {
        self.context == SetContext::Property && self.name() == "active" && self.value == Value::Bool(false)
    }
}

/// The values of some rows, as opinions on a prim.
fn opinions_of(rows: &[&SetRow]) -> Opinions {
    let mut o = Opinions::default();
    for r in rows {
        let map = match r.context { SetContext::Object => &mut o.primvars, SetContext::Property => &mut o.props };
        map.insert(r.name().to_string(), r.value.clone());
    }
    o
}

// ── The operator ────────────────────────────────────────────────────────────

/// What a Set Attribute operator puts out: the primitives coming in with
/// its rows set on those `locations` names (or, with no locations, on the
/// picked primitives, or every one).
pub fn set_attributes(input: &EvalResult, rows: &[SetRow], locations: &[String]) -> EvalResult {
    let EvalResult::Named(prims) = input else { return input.clone() };
    // Names and text with their variables put in (`$OPNAME`, `$JOB/…`).
    let put: Vec<SetRow> = rows.iter().map(|r| SetRow {
        name: if r.name.contains('$') { crate::core::vars::expand(&r.name, &[]) } else { r.name.clone() },
        value: r.value.with_variables_put_in(false),
        ..r.clone()
    }).collect();
    let rows: Vec<&SetRow> = put.iter().filter(|r| r.is_usable()).collect();
    if rows.is_empty() { return input.clone(); }

    let named = locations::is_set(locations);
    let any_picked = prims.iter().any(|p| p.picked);
    let targeted = |p: &Prim| if named { locations::any_names(locations, &p.path) } else { !any_picked || p.picked };

    // Geometry primitives.
    let mut out: Vec<Prim> = Vec::with_capacity(prims.len());
    for p in prims {
        // A point instancer is set as the stage prim it is (below), so what
        // is set on it reaches its prototypes as on any group.
        if !targeted(p) || p.instancing.is_some() { out.push(p.clone()); continue; }
        if let Some(q) = set_on_prim(p, &rows) { out.push(q); }
    }
    if !named { return EvalResult::Named(out); }

    let values = opinions_of(&rows);
    let deactivates = rows.iter().any(|r| r.deactivates());

    // Prims of a stage without geometry: opinions in its tree. Each stage
    // once; every primitive of it then shares the new tree.
    let present: std::collections::HashSet<String> = prims.iter().filter(|p| p.instancing.is_none()).map(|p| p.path.clone()).collect();
    let mut trees: Vec<Arc<crate::usd_scene::StageTree>> = vec![];
    for p in &out {
        if let Some(t) = &p.stage { if !trees.iter().any(|x| Arc::ptr_eq(x, t)) { trees.push(t.clone()); } }
    }
    let mut staged: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut gone: Vec<String> = vec![];
    for old in trees {
        staged.extend(old.nodes.iter().map(|n| n.path.clone()));
        let mut tree = (*old).clone();
        let mut changed = false;
        for node in &old.nodes {
            if present.contains(&node.path) || !locations::any_names(locations, &node.path) { continue; }
            tree.opinions.entry(node.path.clone()).or_default().merge(&values);
            if deactivates { gone.push(node.path.clone()); }
            changed = true;
        }
        if !changed { continue; }
        let new = Arc::new(tree);
        for p in out.iter_mut() {
            if p.stage.as_ref().is_some_and(|t| Arc::ptr_eq(t, &old)) { p.stage = Some(new.clone()); }
        }
    }

    // Prims Imago made above its own primitives (the Xforms of a Primitive
    // Location) where no stage has a prim: each primitive below carries the
    // opinions on them.
    for p in out.iter_mut().filter(|p| p.stage.is_none()) {
        for parent in parents(&p.path) {
            if present.contains(&parent) || staged.contains(&parent) || !locations::any_names(locations, &parent) { continue; }
            if deactivates && !gone.contains(&parent) { gone.push(parent.clone()); }
            Arc::make_mut(&mut p.above).entry(parent).or_default().merge(&values);
        }
    }
    // What is below a deactivated prim is not part of the scene.
    if !gone.is_empty() {
        out.retain(|p| !gone.iter().any(|g| is_below(&p.path, g)));
    }
    EvalResult::Named(out)
}

/// One geometry primitive with the rows set on it. `None` when a row takes
/// it out of the scene.
fn set_on_prim(p: &Prim, rows: &[&SetRow]) -> Option<Prim> {
    let mut q = p.clone();
    let mut geo: Option<Geo> = None;
    for r in rows {
        match r.context {
            SetContext::Object => {
                let Some(attr) = r.value.attr() else { continue };
                // Geometry merged from several prims has no `Geo` left: one
                // from its mesh, which is then written whole.
                let g = geo.get_or_insert_with(|| p.geo.as_deref().cloned().unwrap_or_else(|| Geo::from_mesh(&p.mesh)));
                g.set(Context::Object, r.name(), attr);
            }
            SetContext::Property => match r.name() {
                "active" => if r.deactivates() { return None; },
                "purpose" => if let Some(t) = r.value.token() { q.purpose = purpose(t, p.purpose); },
                name => { Arc::make_mut(&mut q.props).insert(name.to_string(), r.value.clone()); }
            },
        }
    }
    if let Some(g) = geo {
        q.mesh = Arc::new(g.to_mesh());
        q.geo = Some(Arc::new(g));
    }
    Some(q)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geo::Role;
    use crate::core::properties::*;
    use crate::types::Purpose;
    use crate::types::MeshData;
    use crate::usd_scene::{StageNode, StageTree};

    fn node(path: &str, ty: &str) -> StageNode {
        StageNode { path: path.into(), name: path.rsplit('/').next().unwrap().into(), type_name: ty.into(),
            depth: path.matches('/').count(), ..Default::default() }
    }

    /// A stage `/car` (Xform) with `/car/body` and `/car/wheels/w1` (meshes),
    /// and a camera, as primitives.
    fn car() -> Vec<Prim> {
        let tree = Arc::new(StageTree { nodes: vec![
            node("/car", "Xform"), node("/car/body", "Mesh"), node("/car/wheels", "Xform"),
            node("/car/wheels/w1", "Mesh"), node("/cam", "Camera"),
        ], ..Default::default() });
        ["/car/body", "/car/wheels/w1"].iter().map(|path| {
            let mesh = crate::node_graph::nodes::create_cube(1.0);
            let geo = Arc::new(Geo::from_mesh(&mesh));
            Prim { geo: Some(geo), stage: Some(tree.clone()), ..Prim::new(path.to_string(), mesh) }
        }).collect()
    }

    fn row(context: SetContext, name: &str, value: Value) -> SetRow { SetRow { context, name: name.into(), value } }
    fn locs(l: &[&str]) -> Vec<String> { l.iter().map(|s| s.to_string()).collect() }
    fn prims(r: EvalResult) -> Vec<Prim> { match r { EvalResult::Named(p) => p, _ => panic!("primitives") } }

    #[test]
    fn an_object_value_is_an_object_attribute_of_the_named_prims() {
        let colour = row(SetContext::Object, "displayColor", Value::Color3f([1.0, 0.0, 0.0]));
        let out = prims(set_attributes(&EvalResult::Named(car()), &[colour], &locs(&["/car/body"])));
        let body = out[0].geo.as_ref().unwrap();
        let attr = body.attr(Context::Object, "displayColor").expect("set on /car/body");
        assert_eq!((attr.column.vec3s().unwrap(), attr.role), (&[[1.0, 0.0, 0.0]][..], Role::Color));
        assert!(out[0].mesh.primvars.iter().any(|v| v.name == "displayColor"), "the mesh view follows");
        assert!(out[1].geo.as_ref().unwrap().attr(Context::Object, "displayColor").is_none(), "/car/wheels/w1 not named");
    }

    #[test]
    fn a_location_names_its_prim_not_what_is_below() {
        let kind = row(SetContext::Property, "kind", Value::Token("component".into()));
        let out = prims(set_attributes(&EvalResult::Named(car()), &[kind.clone()], &locs(&["/car"])));
        // On the group, as an opinion in the stage: not on each mesh.
        assert!(out.iter().all(|p| p.props.is_empty()));
        let tree = out[0].stage.as_ref().unwrap();
        assert_eq!(tree.opinions["/car"].props["kind"], Value::Token("component".into()));
        assert!(Arc::ptr_eq(tree, out[1].stage.as_ref().unwrap()), "every prim of the stage shares the new tree");
        // `**` names everything below.
        let out = prims(set_attributes(&EvalResult::Named(car()), &[kind.clone()], &locs(&["/car/**"])));
        assert!(out.iter().all(|p| p.props.contains_key("kind")));
        let tree = out[0].stage.as_ref().unwrap();
        assert!(tree.opinions.contains_key("/car/wheels") && tree.opinions.contains_key("/car"), "groups too, /car itself among them");
        // `*` its children only.
        let kind = row(SetContext::Property, "kind", Value::Token("group".into()));
        let out = prims(set_attributes(&EvalResult::Named(car()), &[kind], &locs(&["/car/*"])));
        assert!(out[0].props.contains_key("kind") && !out[1].props.contains_key("kind"));
        let tree = out[0].stage.as_ref().unwrap();
        assert!(tree.opinions.contains_key("/car/wheels") && !tree.opinions.contains_key("/car"));
    }

    #[test]
    fn properties_purpose_active_and_visibility() {
        let set = |r: SetRow, l: &[&str]| prims(set_attributes(&EvalResult::Named(car()), &[r], &locs(l)));
        // Purpose is the primitive's purpose.
        let out = set(row(SetContext::Property, "purpose", Value::Token("guide".into())), &["/car/body"]);
        assert_eq!(out[0].purpose, Purpose::Guide);
        assert!(out[0].props.is_empty());
        // Active off takes the prim out, or what is below a group.
        let off = row(SetContext::Property, "active", Value::Bool(false));
        assert_eq!(set(off.clone(), &["/car/body"]).len(), 1);
        let out = set(off, &["/car/wheels"]);
        assert_eq!(out.iter().map(|p| p.path.as_str()).collect::<Vec<_>>(), vec!["/car/body"]);
        assert_eq!(out[0].stage.as_ref().unwrap().opinions["/car/wheels"].props["active"], Value::Bool(false));
        // Invisible on a group hides what is below it.
        let out = set(row(SetContext::Property, "visibility", Value::Token("invisible".into())), &["/car/wheels"]);
        assert!(!invisible(&out[0]) && invisible(&out[1]));
        let out = set(row(SetContext::Property, "visibility", Value::Token("invisible".into())), &["/car/body"]);
        assert!(invisible(&out[0]) && !invisible(&out[1]));
    }

    #[test]
    fn without_locations_every_prim_or_the_picked_ones() {
        let r = row(SetContext::Property, "doubleSided", Value::Bool(true));
        let out = prims(set_attributes(&EvalResult::Named(car()), &[r.clone()], &[]));
        assert!(out.iter().all(|p| p.props.get("doubleSided") == Some(&Value::Bool(true))));
        assert!(out[0].stage.as_ref().unwrap().opinions.is_empty(), "no locations: geometry only");
        let mut input = car();
        input[1].picked = true;
        let out = prims(set_attributes(&EvalResult::Named(input), &[r], &[]));
        assert!(!out[0].props.contains_key("doubleSided") && out[1].props.contains_key("doubleSided"));
    }

    #[test]
    fn rows_that_say_nothing_change_nothing() {
        let input = EvalResult::Named(car());
        let blank = row(SetContext::Property, "  ", Value::Bool(true));
        // A string has no column: not an Object value.
        let string = row(SetContext::Object, "note", Value::String("x".into()));
        let out = prims(set_attributes(&input, &[blank, string], &locs(&["/car/body"])));
        assert!(Arc::ptr_eq(out[0].geo.as_ref().unwrap(), car_geo(&input)));
        let mesh = EvalResult::Single(MeshData::default());
        assert!(matches!(set_attributes(&mesh, &[row(SetContext::Object, "a", Value::Float(1.0))], &[]), EvalResult::Single(_)));
    }

    fn car_geo(r: &EvalResult) -> &Arc<Geo> {
        match r { EvalResult::Named(p) => p[0].geo.as_ref().unwrap(), _ => panic!() }
    }

    /// Shapes made in Imago, under Xforms no stage has.
    fn shapes() -> Vec<Prim> {
        ["/root/asset/geo/cube1", "/root/asset/geo/cube2", "/root/other/cube3"].iter()
            .map(|p| Prim::new(p.to_string(), crate::node_graph::nodes::create_cube(1.0))).collect()
    }

    #[test]
    fn values_on_the_xforms_above_shapes_are_inherited() {
        let rows = [
            row(SetContext::Object, "displayColor", Value::Color3f([0.0, 1.0, 0.0])),
            row(SetContext::Property, "visibility", Value::Token("invisible".into())),
        ];
        let out = prims(set_attributes(&EvalResult::Named(shapes()), &rows, &locs(&["/root/asset/geo"])));
        // Carried by each primitive below, on the Xform's path; not set on the cubes themselves.
        assert!(out[0].above.contains_key("/root/asset/geo") && out[1].above.contains_key("/root/asset/geo"));
        assert!(out[2].above.is_empty() && out.iter().all(|p| p.props.is_empty() && p.geo.is_none()));
        // Inherited.
        assert!(invisible(&out[0]) && invisible(&out[1]) && !invisible(&out[2]));
        assert_eq!(inherited_primvars(&out[0]), vec![("displayColor".to_string(), Value::Color3f([0.0, 1.0, 0.0]))]);
        assert!(inherited_primvars(&out[2]).is_empty());
        // The nearest wins; a primitive's own attribute wins over both.
        let blue = [row(SetContext::Object, "displayColor", Value::Color3f([0.0, 0.0, 1.0]))];
        let out = prims(set_attributes(&EvalResult::Named(out), &blue, &locs(&["/root"])));
        assert_eq!(inherited_primvars(&out[0])[0].1, Value::Color3f([0.0, 1.0, 0.0]));
        assert_eq!(inherited_primvars(&out[2])[0].1, Value::Color3f([0.0, 0.0, 1.0]));
        let red = [row(SetContext::Object, "displayColor", Value::Color3f([1.0, 0.0, 0.0]))];
        let out = prims(set_attributes(&EvalResult::Named(out), &red, &locs(&["/root/asset/geo/cube1"])));
        assert!(inherited_primvars(&out[0]).is_empty());
        // Purpose, from the nearest prim above that has one.
        let guide = [row(SetContext::Property, "purpose", Value::Token("guide".into()))];
        let out = prims(set_attributes(&EvalResult::Named(shapes()), &guide, &locs(&["/root/asset"])));
        assert_eq!((purpose_of(&out[0]), purpose_of(&out[2])), (Purpose::Guide, Purpose::Default));
        assert_eq!(out[0].purpose, Purpose::Default, "the cube's own purpose is unset");
        // Active off: what is below the Xform leaves the scene.
        let off = [row(SetContext::Property, "active", Value::Bool(false))];
        let out = prims(set_attributes(&EvalResult::Named(shapes()), &off, &locs(&["/root/asset"])));
        assert_eq!(out.iter().map(|p| p.path.as_str()).collect::<Vec<_>>(), vec!["/root/other/cube3"]);
        assert_eq!(parents("/a/b/c"), vec!["/a".to_string(), "/a/b".to_string()]);
    }

    #[test]
    fn a_purpose_set_above_beats_one_only_inherited_from_the_file() {
        // /car authors `render` in the file; its meshes inherit it.
        let mut input = car();
        let tree = {
            let mut t = (**input[0].stage.as_ref().unwrap()).clone();
            t.nodes[0].authored = vec![("purpose".into(), Value::Token("render".into()))];
            Arc::new(t)
        };
        for p in input.iter_mut() {
            p.stage = Some(tree.clone());
            p.purpose = Purpose::Render { has_proxy: false };
            p.source = Some(Arc::new(crate::types::Source { geo: p.geo.clone().unwrap(), place: p.place.clone(), material: None, purpose: p.purpose }));
        }
        assert_eq!(purpose_from(&input[1]), PurposeFrom::Above("/car".into(), false));
        // Guide on /car/wheels, nearer than /car: the wheel is a guide, the body still render.
        let guide = [row(SetContext::Property, "purpose", Value::Token("guide".into()))];
        let out = prims(set_attributes(&EvalResult::Named(input.clone()), &guide, &locs(&["/car/wheels"])));
        assert_eq!((purpose_of(&out[0]), purpose_of(&out[1])), (Purpose::Render { has_proxy: false }, Purpose::Guide));
        assert_eq!(purpose_from(&out[1]), PurposeFrom::Above("/car/wheels".into(), true));
        // On /car itself, where the file authors render: Set Attribute's opinion is the stronger.
        let out = prims(set_attributes(&EvalResult::Named(input.clone()), &guide, &locs(&["/car"])));
        assert_eq!(purpose_of(&out[1]), Purpose::Guide);
        // A purpose the file authors on the mesh itself wins over the group's.
        let mut t = (**out[1].stage.as_ref().unwrap()).clone();
        t.nodes[3].authored = vec![("purpose".into(), Value::Token("render".into()))];
        let mut w1 = out[1].clone();
        w1.stage = Some(Arc::new(t));
        assert_eq!((purpose_from(&w1), purpose_of(&w1)), (PurposeFrom::File, Purpose::Render { has_proxy: false }));
    }

    #[test]
    fn values_convert_and_write() {
        assert_eq!(ValueType::Float3.convert(&Value::Float(2.0)), Value::Float3([2.0, 0.0, 0.0]));
        assert_eq!(ValueType::Token.convert(&Value::Int(3)), Value::Token("3".into()));
        assert_eq!(ValueType::Int.convert(&Value::String(" 7 ".into())), Value::Int(7));
        assert_eq!(Value::Color3f([1.0, 0.5, 0.0]).usda(), "(1, 0.5, 0)");
        assert_eq!(Value::Token("a\"b".into()).usda(), "\"a\\\"b\"");
        assert_eq!(Value::Asset("tex/a.png".into()).usda(), "@tex/a.png@");
        assert_eq!(Value::Bool(false).usda(), "false");
        assert_eq!(written("kind"), Written::Metadata);
        assert_eq!(written("subdivisionScheme"), Written::Uniform);
        assert_eq!(written("userProperties:assetId"), Written::Attribute);
    }
}
