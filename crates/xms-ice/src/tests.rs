use std::collections::HashMap;
use std::sync::Arc;

use xms_geo::{Attr, Column, Context, Geo, Role, POINTS};

use super::graph::{wire, Graph, Input, Math, Op};
use super::types::Value;
use super::{compile, run};

/// A cloud of `n` points along X, with normals up and a per-point float.
fn cloud(n: usize) -> Geo {
    let mut g = Geo::from_points((0..n).map(|i| [i as f32, 0.0, 0.0]).collect());
    g.set(Context::Point, "N", Attr::new(Column::Vec3(Arc::new(vec![[0.0, 1.0, 0.0]; n])), Role::Normal));
    g.set(Context::Point, "heat", Attr::new(Column::Float(Arc::new((0..n).map(|i| i as f32).collect())), Role::None));
    g
}

fn points(g: &Geo) -> &[[f32; 3]] { g.points() }

fn no_params() -> HashMap<String, Value> { HashMap::new() }

/// root(Point) <- Set Data P <- …
fn setting_p(mut g: Graph, value: usize) -> Graph {
    let set = g.add(Op::set("P"), vec![wire(value)]);
    g.add(Op::Root { context: Context::Point }, vec![wire(set)]);
    g
}

#[test]
fn p_doubled_and_every_other_column_still_shared() {
    let geo = cloud(10);
    let mut g = Graph::default();
    let p = g.add(Op::get("P"), vec![]);
    let doubled = g.add(Op::Math(Math::Multiply), vec![wire(p), Input::Value(Value::Float(2.0))]);
    let g = setting_p(g, doubled);

    let plan = compile(&g, &geo).unwrap();
    let out = run(&plan, &geo, &no_params());
    assert_eq!(points(&out)[3], [6.0, 0.0, 0.0]);
    // Only P was read and only P written: N and heat are the same columns.
    assert_eq!(plan.reads().map(|(n, _)| n).collect::<Vec<_>>(), vec![POINTS]);
    assert!(out.attr(Context::Point, "N").unwrap().same(geo.attr(Context::Point, "N").unwrap()));
    assert!(out.attr(Context::Point, "heat").unwrap().same(geo.attr(Context::Point, "heat").unwrap()));
    assert!(out.topology.same(&geo.topology));
    // The input is untouched.
    assert_eq!(points(&geo)[3], [3.0, 0.0, 0.0]);
}

#[test]
fn parameters_change_without_compiling_again_and_uniforms_are_computed_once() {
    let geo = cloud(5);
    let mut g = Graph::default();
    let p = g.add(Op::get("P"), vec![]);
    let n = g.add(Op::get("N"), vec![]);
    let amount = g.add(Op::Parameter { name: "amount".into(), default: Value::Float(1.0) }, vec![]);
    let twice = g.add(Op::Math(Math::Multiply), vec![wire(amount), Input::Value(Value::Float(2.0))]);
    let push = g.add(Op::Math(Math::Multiply), vec![wire(n), wire(twice)]);
    let moved = g.add(Op::Math(Math::Add), vec![wire(p), wire(push)]);
    let g = setting_p(g, moved);

    let plan = compile(&g, &geo).unwrap();
    assert_eq!(points(&run(&plan, &geo, &no_params()))[2], [2.0, 2.0, 0.0]);
    let params = HashMap::from([("amount".to_string(), Value::Float(0.25))]);
    assert_eq!(points(&run(&plan, &geo, &params))[2], [2.0, 0.5, 0.0]);
    // amount × 2 does not vary per point: per batch it is one broadcast, not a multiply.
    assert_eq!(plan.batch_len(), 6, "load P, load N, broadcast, multiply, add, store");
}

#[test]
fn an_object_attribute_is_one_value_for_every_point() {
    let mut geo = cloud(4);
    geo.set(Context::Object, "lift", Attr::new(Column::Float(Arc::new(vec![3.0])), Role::None));
    let mut g = Graph::default();
    let p = g.add(Op::get("P"), vec![]);
    let lift = g.add(Op::get("lift"), vec![]);
    let up = g.add(Op::Math(Math::Multiply), vec![Input::Value(Value::Vec3([0.0, 1.0, 0.0])), wire(lift)]);
    let moved = g.add(Op::Math(Math::Add), vec![wire(p), wire(up)]);
    let g = setting_p(g, moved);
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params());
    assert_eq!(points(&out)[1], [1.0, 3.0, 0.0]);
}

#[test]
fn types_and_contexts_are_checked_at_the_node_that_breaks_them() {
    let mut geo = cloud(3);
    geo.set(Context::Face, "id", Attr::new(Column::Int(Arc::new(vec![])), Role::None));

    // A vector plus a bool.
    let mut g = Graph::default();
    let p = g.add(Op::get("P"), vec![]);
    let bad = g.add(Op::Math(Math::Add), vec![wire(p), Input::Value(Value::Bool(true))]);
    let g = setting_p(g, bad);
    assert_eq!(compile(&g, &geo).unwrap_err().node, bad);

    // A per-face attribute in a per-point graph.
    let mut g = Graph::default();
    let id = g.add(Op::get("id"), vec![]);
    let set = g.add(Op::set("copy"), vec![wire(id)]);
    g.add(Op::Root { context: Context::Point }, vec![wire(set)]);
    let e = compile(&g, &geo).unwrap_err();
    assert_eq!(e.node, id);
    assert!(e.message.contains("Face"), "{}", e.message);
}

#[test]
fn only_what_reaches_the_root_is_compiled() {
    let geo = cloud(3);
    let mut g = Graph::default();
    let p = g.add(Op::get("P"), vec![]);
    // Reads heat, but feeds nothing.
    let heat = g.add(Op::get("heat"), vec![]);
    g.add(Op::Math(Math::Sin), vec![wire(heat)]);
    let g = setting_p(g, p);
    let plan = compile(&g, &geo).unwrap();
    assert_eq!(plan.reads().count(), 1);
    assert_eq!(plan.batch_len(), 2, "load P, store P");
}

#[test]
fn execute_ports_run_in_order() {
    let geo = cloud(3);
    let mut g = Graph::default();
    // First: a = heat + 1. Then: b = a * 10, reading the a just set.
    let heat = g.add(Op::get("heat"), vec![]);
    let plus = g.add(Op::Math(Math::Add), vec![wire(heat), Input::Value(Value::Float(1.0))]);
    let set_a = g.add(Op::set("a"), vec![wire(plus)]);
    let a = g.add(Op::get("a"), vec![]);
    let times = g.add(Op::Math(Math::Multiply), vec![wire(a), Input::Value(Value::Float(10.0))]);
    let set_b = g.add(Op::set("b"), vec![wire(times)]);
    g.add(Op::Root { context: Context::Point }, vec![wire(set_a), wire(set_b)]);

    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params());
    let b = out.attr(Context::Point, "b").unwrap().column.floats().unwrap();
    assert_eq!(b, &[10.0, 20.0, 30.0]);
    assert_eq!(out.attr(Context::Point, "a").unwrap().column.floats().unwrap(), &[1.0, 2.0, 3.0]);
}

#[test]
fn batches_cover_every_element_and_indexed_attributes_are_read_through_their_indices() {
    let n = super::exec::BATCH * 2 + 17;
    let mut geo = cloud(n);
    // Two values, every other point taking the second.
    let ix: Vec<u32> = (0..n as u32).map(|i| i % 2).collect();
    geo.set(Context::Point, "pick", Attr::indexed(Column::Float(Arc::new(vec![0.0, 1.0])), ix, Role::None));
    let mut g = Graph::default();
    let heat = g.add(Op::get("heat"), vec![]);
    let pick = g.add(Op::get("pick"), vec![]);
    let sum = g.add(Op::Math(Math::Add), vec![wire(heat), wire(pick)]);
    let set = g.add(Op::set("sum"), vec![wire(sum)]);
    g.add(Op::Root { context: Context::Point }, vec![wire(set)]);

    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params());
    let sum = out.attr(Context::Point, "sum").unwrap().column.floats().unwrap();
    assert_eq!(sum.len(), n);
    for (i, s) in sum.iter().enumerate() { assert_eq!(*s, i as f32 + (i % 2) as f32, "element {i}"); }
    assert!(out.validate().is_ok());
}

#[test]
fn vector_maths() {
    let geo = cloud(2);
    let mut g = Graph::default();
    let p = g.add(Op::get("P"), vec![]);
    let n = g.add(Op::get("N"), vec![]);
    let side = g.add(Op::Math(Math::Cross), vec![wire(n), Input::Value(Value::Vec3([0.0, 0.0, 1.0]))]);
    let len = g.add(Op::Math(Math::Length), vec![wire(p)]);
    let scaled = g.add(Op::Math(Math::Multiply), vec![wire(side), wire(len)]);
    let mixed = g.add(Op::Math(Math::Lerp), vec![wire(p), wire(scaled), Input::Value(Value::Float(0.5))]);
    let g = setting_p(g, mixed);
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params());
    // Point 1: P = (1,0,0), N × Z = (1,0,0), length 1: halfway between equals (1,0,0).
    assert_eq!(points(&out)[1], [1.0, 0.0, 0.0]);
}

/// P += N × (heat × amount) over a million points.
/// `cargo test -p xms-ice --release ice_engine_speed -- --ignored --nocapture`
#[test]
#[ignore]
fn ice_engine_speed() {
    let n = 1_000_000;
    let geo = cloud(n);
    let mut g = Graph::default();
    let p = g.add(Op::get("P"), vec![]);
    let nrm = g.add(Op::get("N"), vec![]);
    let heat = g.add(Op::get("heat"), vec![]);
    let amount = g.add(Op::Parameter { name: "amount".into(), default: Value::Float(0.001) }, vec![]);
    let k = g.add(Op::Math(Math::Multiply), vec![wire(heat), wire(amount)]);
    let push = g.add(Op::Math(Math::Multiply), vec![wire(nrm), wire(k)]);
    let moved = g.add(Op::Math(Math::Add), vec![wire(p), wire(push)]);
    let g = setting_p(g, moved);
    let plan = compile(&g, &geo).unwrap();
    let t = std::time::Instant::now();
    let runs = 20;
    for _ in 0..runs { std::hint::black_box(run(&plan, &geo, &no_params())); }
    let each = t.elapsed().as_secs_f64() / runs as f64;
    println!("{n} points: {:.2} ms a run, {:.1} M points/s", each * 1e3, n as f64 / each / 1e6);
}

/// Every node's output type, wired into the root or not, for the editor's
/// ports and wires.
#[test]
fn output_types_follow_the_geometry_and_the_maths() {
    use super::output_types;
    use super::types::Ty;
    let geo = cloud(3);
    let mut g = Graph::default();
    let p = g.add(Op::get("P"), vec![]);
    let heat = g.add(Op::get("heat"), vec![]);
    let scaled = g.add(Op::Math(Math::Multiply), vec![wire(p), wire(heat)]);
    let length = g.add(Op::Math(Math::Length), vec![wire(scaled)]);
    let one = g.add(Op::Math(Math::Add), vec![Input::Value(Value::Float(1.0)), wire(p)]);
    let missing = g.add(Op::get("nowhere"), vec![]);
    let set = g.add(Op::set("P"), vec![wire(scaled)]);
    let root = g.add(Op::Root { context: Context::Point }, vec![wire(set)]);
    let t = output_types(&g, &geo);
    assert_eq!(t[p], Some(Ty::Vec3));
    assert_eq!(t[heat], Some(Ty::Float));
    assert_eq!(t[scaled], Some(Ty::Vec3));
    // Not wired into the root, typed all the same.
    assert_eq!(t[length], Some(Ty::Float));
    assert_eq!(t[one], Some(Ty::Vec3), "a value takes the type of what it meets");
    assert_eq!((t[missing], t[set], t[root]), (None, None, None));
}
/// Two quads side by side: points 0 to 5, faces 0 (points 0 1 4 3) and 1
/// (points 1 2 5 4), with a per-face heat of 1 and 3.
fn two_quads() -> Geo {
    let pts = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [2.0, 1.0, 0.0]];
    let mut g = Geo::from_polygons(pts, vec![4, 4], vec![0, 1, 4, 3, 1, 2, 5, 4]);
    g.set(Context::Face, "heat", Attr::new(Column::Float(Arc::new(vec![1.0, 3.0])), Role::None));
    g
}

/// root(`context`) <- Set Data `name` <- Get Data `what` read from `from`.
fn reading(context: Context, name: &str, what: &str, from: Context, reduce: super::graph::Reduce) -> Graph {
    let mut g = Graph::default();
    let get = g.add(Op::GetData { name: what.into(), context: Some(from), reduce }, vec![]);
    let set = g.add(Op::set(name), vec![wire(get)]);
    g.add(Op::Root { context }, vec![wire(set)]);
    g
}

/// Reading across contexts: a face its points, a corner its point, a point
/// the faces around it, the object everything.
#[test]
fn get_data_reads_across_contexts() {
    use super::graph::Reduce;
    let geo = two_quads();
    let run_on = |g: &Graph| run(&compile(g, &geo).unwrap(), &geo, &no_params());

    // Each face: the average of its points, its centre.
    let out = run_on(&reading(Context::Face, "centre", "P", Context::Point, Reduce::Average));
    let centre = out.attr(Context::Face, "centre").unwrap().column.vec3s().unwrap().to_vec();
    assert_eq!(centre, vec![[0.5, 0.5, 0.0], [1.5, 0.5, 0.0]]);

    // Each corner: its point.
    let out = run_on(&reading(Context::Corner, "at", "P", Context::Point, Reduce::Average));
    let at = out.attr(Context::Corner, "at").unwrap().column.vec3s().unwrap().to_vec();
    assert_eq!(at[2], [1.0, 1.0, 0.0]);
    assert_eq!(at[5], [2.0, 0.0, 0.0]);

    // Each point: the faces around it, summed, or the hottest.
    let out = run_on(&reading(Context::Point, "warm", "heat", Context::Face, Reduce::Sum));
    let warm = out.attr(Context::Point, "warm").unwrap().column.floats().unwrap().to_vec();
    assert_eq!(warm, vec![1.0, 4.0, 3.0, 1.0, 4.0, 3.0]);
    let out = run_on(&reading(Context::Point, "hot", "heat", Context::Face, Reduce::Max));
    assert_eq!(out.attr(Context::Point, "hot").unwrap().column.floats().unwrap()[1], 3.0);

    // The object: every point's average.
    let out = run_on(&reading(Context::Object, "middle", "P", Context::Point, Reduce::Average));
    assert_eq!(out.attr(Context::Object, "middle").unwrap().column.vec3s().unwrap()[0], [1.0, 0.5, 0.0]);

    // Read in the graph's own context without saying so: told where it is.
    let mut g = Graph::default();
    let get = g.add(Op::get("heat"), vec![]);
    let set = g.add(Op::set("x"), vec![wire(get)]);
    g.add(Op::Root { context: Context::Point }, vec![wire(set)]);
    let err = compile(&g, &geo).unwrap_err();
    assert!(err.message.contains("per Face") && err.message.contains("context"), "{}", err.message);
    // And the types follow the context read.
    assert_eq!(super::output_types(&reading(Context::Face, "c", "P", Context::Point, Reduce::Average), &geo)[0], Some(super::types::Ty::Vec3));
}

#[test]
fn what_the_inspector_shows_reads_from_the_topology() {
    use super::graph::Reduce;
    let geo = two_quads();
    // Per point, the lowest first corner of its faces: points 1 and 4 are on
    // both quads, so 0; points 2 and 5 only on the second, so 4.
    let plan = compile(&reading(Context::Point, "fc", "firstCorner", Context::Face, Reduce::Min), &geo).unwrap();
    let out = run(&plan, &geo, &no_params());
    assert_eq!(out.attr(Context::Point, "fc").unwrap().column.ints().unwrap(), &[0, 0, 4, 0, 0, 4]);
    // A face's corners, area and normal; a corner's point, face and place.
    for (ctx, what, from, want) in [
        (Context::Face, "corners", Context::Face, vec![4.0, 4.0]),
        (Context::Face, "area", Context::Face, vec![1.0, 1.0]),
        (Context::Corner, "ptIndex", Context::Corner, vec![0.0, 1.0, 4.0, 3.0, 1.0, 2.0, 5.0, 4.0]),
        (Context::Corner, "faceIndex", Context::Corner, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]),
        (Context::Corner, "localCornerIndex", Context::Corner, vec![0.0, 1.0, 2.0, 3.0, 0.0, 1.0, 2.0, 3.0]),
    ] {
        let mut g = Graph::default();
        let get = g.add(Op::GetData { name: what.into(), context: Some(from), reduce: Reduce::Average }, vec![]);
        let f = g.add(Op::Math(Math::Add), vec![wire(get), Input::Value(Value::Float(0.0))]);
        let set = g.add(Op::set("out"), vec![wire(f)]);
        g.add(Op::Root { context: ctx }, vec![wire(set)]);
        let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params());
        let got: Vec<f32> = match &out.attr(ctx, "out").unwrap().column {
            Column::Float(v) => v.to_vec(),
            Column::Int(v) => v.iter().map(|&i| i as f32).collect(),
            other => panic!("{what}: {:?}", other.kind()),
        };
        assert_eq!(got, want, "{what}");
    }
    // The face normal: the quads wind counter-clockwise seen from +Z.
    let plan = compile(&reading(Context::Face, "n", "N", Context::Face, Reduce::Average), &geo).unwrap();
    let out = run(&plan, &geo, &no_params());
    assert_eq!(out.attr(Context::Face, "n").unwrap().column.vec3s().unwrap(), &[[0.0, 0.0, 1.0]; 2]);
    // Unset, the context finds it and says which to set.
    let mut g = Graph::default();
    let get = g.add(Op::get("firstCorner"), vec![]);
    let set = g.add(Op::set("fc"), vec![wire(get)]);
    g.add(Op::Root { context: Context::Point }, vec![wire(set)]);
    let e = compile(&g, &geo).unwrap_err();
    assert!(e.message.contains("per Face"), "{}", e.message);
    // Points have no faces: nothing derived.
    assert!(compile(&reading(Context::Point, "a", "area", Context::Face, Reduce::Sum), &cloud(3)).is_err());
}

/// root(`context`) <- Set Data `name` into `to` <- `value`.
fn writing(mut g: Graph, context: Context, name: &str, to: Context, reduce: super::graph::Reduce, value: usize) -> Graph {
    let set = g.add(Op::SetData { name: name.into(), context: Some(to), reduce }, vec![wire(value)]);
    g.add(Op::Root { context }, vec![wire(set)]);
    g
}

#[test]
fn set_data_writes_across_contexts() {
    use super::graph::Reduce;
    let geo = two_quads();
    // Per point, into each face: the average of its points' x.
    let mut g = Graph::default();
    let p = g.add(Op::get("P"), vec![]);
    let x = g.add(Op::Math(Math::Dot), vec![wire(p), Input::Value(Value::Vec3([1.0, 0.0, 0.0]))]);
    let g = writing(g, Context::Point, "x", Context::Face, Reduce::Average, x);
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params());
    assert_eq!(out.attr(Context::Face, "x").unwrap().column.floats().unwrap(), &[0.5, 1.5]);
    assert!(out.attr(Context::Point, "x").is_none(), "written per face, not per point");
    // Per face, into each point: the most of the faces around it.
    let mut g = Graph::default();
    let heat = g.add(Op::get("heat"), vec![]);
    let g = writing(g, Context::Face, "hot", Context::Point, Reduce::Max, heat);
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params());
    assert_eq!(out.attr(Context::Point, "hot").unwrap().column.floats().unwrap(), &[1.0, 3.0, 3.0, 1.0, 3.0, 3.0]);
    // Per object, into every corner.
    let mut g = Graph::default();
    let seven = g.add(Op::Constant(Value::Int(7)), vec![]);
    let g = writing(g, Context::Object, "seven", Context::Corner, Reduce::Average, seven);
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params());
    assert_eq!(out.attr(Context::Corner, "seven").unwrap().column.ints().unwrap(), &[7; 8]);
    // Written per face, a name held per point goes: one attribute a name.
    let mut g = Graph::default();
    let one = g.add(Op::Constant(Value::Float(1.0)), vec![]);
    let mut geo2 = two_quads();
    geo2.set(Context::Point, "x", Attr::new(Column::Float(Arc::new(vec![0.0; 6])), Role::None));
    let g = writing(g, Context::Point, "x", Context::Face, Reduce::Sum, one);
    let out = run(&compile(&g, &geo2).unwrap(), &geo2, &no_params());
    assert_eq!(out.attr(Context::Face, "x").unwrap().column.floats().unwrap(), &[4.0, 4.0]);
    assert!(out.attr(Context::Point, "x").is_none());
    // The points stay per point.
    let mut g = Graph::default();
    let p = g.add(Op::get("P"), vec![]);
    assert!(compile(&writing(g, Context::Point, "P", Context::Face, Reduce::Average, p), &geo).is_err());
}

#[test]
fn a_new_attribute_is_made_beside_p() {
    // Two ports: P moved, and a new per-point attribute from a constant.
    let geo = cloud(4);
    let mut g = Graph::default();
    let p = g.add(Op::get("P"), vec![]);
    let set_p = g.add(Op::set("P"), vec![wire(p)]);
    let k = g.add(Op::Constant(Value::Float(2.5)), vec![]);
    let set_t = g.add(Op::set("test"), vec![wire(k)]);
    g.add(Op::Root { context: Context::Point }, vec![wire(set_p), wire(set_t)]);
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params());
    assert_eq!(out.attr(Context::Point, "test").unwrap().column.floats().unwrap(), &[2.5; 4]);
}
