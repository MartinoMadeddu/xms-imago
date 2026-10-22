use std::collections::HashMap;
use std::sync::Arc;

use xms_geo::{Attr, Column, Context, Geo, Role, POINTS};

use super::graph::{wire, ArrayOp, Graph, Input, Math, Op, SCATTER_NORMALS, SCATTER_POSITIONS, SCATTER_SAMPLES};
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
    let out = run(&plan, &geo, &no_params(), &[]).geo;
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
    assert_eq!(points(&run(&plan, &geo, &no_params(), &[]).geo)[2], [2.0, 2.0, 0.0]);
    let params = HashMap::from([("amount".to_string(), Value::Float(0.25))]);
    assert_eq!(points(&run(&plan, &geo, &params, &[]).geo)[2], [2.0, 0.5, 0.0]);
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
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &[]).geo;
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

    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &[]).geo;
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

    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &[]).geo;
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
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &[]).geo;
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
    for _ in 0..runs { std::hint::black_box(run(&plan, &geo, &no_params(), &[]).geo); }
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
    let run_on = |g: &Graph| run(&compile(g, &geo).unwrap(), &geo, &no_params(), &[]).geo;

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
    let out = run(&plan, &geo, &no_params(), &[]).geo;
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
        let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &[]).geo;
        let got: Vec<f32> = match &out.attr(ctx, "out").unwrap().column {
            Column::Float(v) => v.to_vec(),
            Column::Int(v) => v.iter().map(|&i| i as f32).collect(),
            other => panic!("{what}: {:?}", other.kind()),
        };
        assert_eq!(got, want, "{what}");
    }
    // The face normal: the quads wind counter-clockwise seen from +Z.
    let plan = compile(&reading(Context::Face, "n", "N", Context::Face, Reduce::Average), &geo).unwrap();
    let out = run(&plan, &geo, &no_params(), &[]).geo;
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
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &[]).geo;
    assert_eq!(out.attr(Context::Face, "x").unwrap().column.floats().unwrap(), &[0.5, 1.5]);
    assert!(out.attr(Context::Point, "x").is_none(), "written per face, not per point");
    // Per face, into each point: the most of the faces around it.
    let mut g = Graph::default();
    let heat = g.add(Op::get("heat"), vec![]);
    let g = writing(g, Context::Face, "hot", Context::Point, Reduce::Max, heat);
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &[]).geo;
    assert_eq!(out.attr(Context::Point, "hot").unwrap().column.floats().unwrap(), &[1.0, 3.0, 3.0, 1.0, 3.0, 3.0]);
    // Per object, into every corner.
    let mut g = Graph::default();
    let seven = g.add(Op::Constant(Value::Int(7)), vec![]);
    let g = writing(g, Context::Object, "seven", Context::Corner, Reduce::Average, seven);
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &[]).geo;
    assert_eq!(out.attr(Context::Corner, "seven").unwrap().column.ints().unwrap(), &[7; 8]);
    // Written per face, a name held per point goes: one attribute a name.
    let mut g = Graph::default();
    let one = g.add(Op::Constant(Value::Float(1.0)), vec![]);
    let mut geo2 = two_quads();
    geo2.set(Context::Point, "x", Attr::new(Column::Float(Arc::new(vec![0.0; 6])), Role::None));
    let g = writing(g, Context::Point, "x", Context::Face, Reduce::Sum, one);
    let out = run(&compile(&g, &geo2).unwrap(), &geo2, &no_params(), &[]).geo;
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
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &[]).geo;
    assert_eq!(out.attr(Context::Point, "test").unwrap().column.floats().unwrap(), &[2.5; 4]);
}

/// root(Object) <- Set Data `name` <- `value`; the one value set on the object.
fn object_value(mut g: Graph, value: usize, name: &str, geo: &Geo) -> Column {
    let set = g.add(Op::set(name), vec![wire(value)]);
    g.add(Op::Root { context: Context::Object }, vec![wire(set)]);
    let out = run(&compile(&g, geo).unwrap(), geo, &no_params(), &[]).geo;
    out.attr(Context::Object, name).unwrap().column.clone()
}

fn random(g: &mut Graph, seed: u32, min: Value, max: Value, count: i32) -> usize {
    g.add(Op::Random { seed }, vec![Input::Value(min), Input::Value(max), Input::Value(Value::Int(count)), Input::Value(Value::Int(0))])
}

/// A random value per point drawn for a seed per point: `seed` wired from
/// an attribute, or (None) unwired.
fn random_by(geo: &Geo, seed_from: Option<&str>) -> Vec<f32> {
    let mut g = Graph::default();
    let seed = match seed_from {
        Some(name) => wire(g.add(Op::get(name), vec![])),
        None => Input::Value(Value::Int(0)),
    };
    let r = g.add(Op::Random { seed: 3 }, vec![Input::Value(Value::Float(0.0)), Input::Value(Value::Float(1.0)), Input::Value(Value::Int(0)), seed]);
    let set = g.add(Op::set("r"), vec![wire(r)]);
    g.add(Op::Root { context: Context::Point }, vec![wire(set)]);
    let out = run(&compile(&g, geo).unwrap(), geo, &no_params(), &[]).geo;
    out.attr(Context::Point, "r").unwrap().column.floats().unwrap().to_vec()
}

/// The Seed: unwired, each point is drawn for its own number, as ptIndex
/// wired is; wired from a group, the points of one group get one value.
#[test]
fn a_seed_per_element_draws_each_for_its_seed() {
    let mut geo = cloud(6);
    geo.set(Context::Point, "group", Attr::new(Column::Int(Arc::new(vec![0, 0, 0, 1, 1, 1])), Role::None));
    assert_eq!(random_by(&geo, None), random_by(&geo, Some("ptIndex")), "unwired is the point's own number");
    let by_group = random_by(&geo, Some("group"));
    assert!(by_group[0] == by_group[1] && by_group[1] == by_group[2] && by_group[3] == by_group[5]);
    assert_ne!(by_group[0], by_group[3]);
}

/// Random Orientation per point: N per point, an orient per point.
#[test]
fn random_orientations_per_point() {
    let geo = cloud(500);
    let mut g = Graph::default();
    let n = g.add(Op::get("N"), vec![]);
    let turn = g.add(Op::RandomOrientation { seed: 2 }, vec![
        wire(n), Input::Value(Value::Vec3([0.0, 360.0, 0.0])), Input::Value(Value::Int(0)), Input::Value(Value::Int(0)),
    ]);
    let set = g.add(Op::set("orient"), vec![wire(turn)]);
    g.add(Op::Root { context: Context::Point }, vec![wire(set)]);
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &[]).geo;
    let q = out.attr(Context::Point, "orient").unwrap().column.quats().unwrap().to_vec();
    assert_eq!(q.len(), 500);
    // Up along N (+Y), turned about it: each its own turn.
    assert!(q.iter().all(|q| ((glam::Quat::from_array(*q) * glam::Vec3::Y).y - 1.0).abs() < 1e-4));
    assert_ne!(q[0], q[1]);
}

/// Concatenate Arrays: the paths of two reference inputs, one after the
/// other; arrays of two types do not join.
#[test]
fn arrays_are_concatenated_in_order() {
    let geo = cloud(1);
    let refs = vec![vec!["/a".to_string(), "/b".into()], vec!["/c".to_string()]];
    let mut g = Graph::default();
    let one = g.add(Op::GetReference { input: 1, paths: None }, vec![]);
    let two = g.add(Op::GetReference { input: 2, paths: None }, vec![]);
    // The free input (a value) is not joined.
    let all = g.add(Op::Concat, vec![wire(one), wire(two), Input::Value(Value::Float(0.0))]);
    let size = g.add(Op::Array(ArrayOp::Size), vec![wire(all)]);
    let set = g.add(Op::set("n"), vec![wire(size)]);
    g.add(Op::Root { context: Context::Object }, vec![wire(set)]);
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &refs).geo;
    assert_eq!(out.attr(Context::Object, "n").unwrap().column.ints().unwrap(), &[3]);
    // Numbers: an array then a single value.
    let mut g = Graph::default();
    let arr = random(&mut g, 0, Value::Float(5.0), Value::Float(5.0), 4);
    let one = g.add(Op::Constant(Value::Float(1.0)), vec![]);
    let all = g.add(Op::Concat, vec![wire(arr), wire(one)]);
    let sum = g.add(Op::Array(ArrayOp::Sum), vec![wire(all)]);
    assert_eq!(object_value(g, sum, "s", &geo).floats().unwrap(), &[21.0]);
    // A path and a number do not join.
    let mut g = Graph::default();
    let paths = g.add(Op::GetReference { input: 1, paths: None }, vec![]);
    let num = g.add(Op::Constant(Value::Float(1.0)), vec![]);
    let bad = g.add(Op::Concat, vec![wire(paths), wire(num)]);
    let size = g.add(Op::Array(ArrayOp::Size), vec![wire(bad)]);
    let set = g.add(Op::set("n"), vec![wire(size)]);
    g.add(Op::Root { context: Context::Object }, vec![wire(set)]);
    assert_eq!(compile(&g, &geo).unwrap_err().node, bad);
}

#[test]
fn random_values_are_in_range_different_per_element_and_the_same_every_run() {
    let geo = cloud(10_000);
    let run_with = |seed: u32| {
        let mut g = Graph::default();
        let r = random(&mut g, seed, Value::Float(2.0), Value::Float(3.0), 0);
        let set = g.add(Op::set("r"), vec![wire(r)]);
        g.add(Op::Root { context: Context::Point }, vec![wire(set)]);
        let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &[]).geo;
        out.attr(Context::Point, "r").unwrap().column.floats().unwrap().to_vec()
    };
    let a = run_with(7);
    assert!(a.iter().all(|x| (2.0..3.0).contains(x)));
    let mean = a.iter().sum::<f32>() / a.len() as f32;
    assert!((mean - 2.5).abs() < 0.02, "evenly spread: mean {mean}");
    assert_eq!(a, run_with(7), "the same seed, the same values");
    assert_ne!(a, run_with(8), "another seed, other values");
}

#[test]
fn random_ints_include_both_ends() {
    let geo = cloud(5_000);
    let mut g = Graph::default();
    let r = random(&mut g, 1, Value::Int(0), Value::Int(3), 0);
    let set = g.add(Op::set("pick"), vec![wire(r)]);
    g.add(Op::Root { context: Context::Point }, vec![wire(set)]);
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &[]).geo;
    let picks = out.attr(Context::Point, "pick").unwrap().column.ints().unwrap().to_vec();
    for k in 0..=3 { assert!(picks.contains(&k), "{k} is picked"); }
    assert!(picks.iter().all(|k| (0..=3).contains(k)));
}

#[test]
fn an_object_graph_makes_an_array_and_works_on_it_element_by_element() {
    let geo = cloud(3);
    // 10,000 values from 0 to 1, each times 4: their average is near 2.
    let mut g = Graph::default();
    let arr = random(&mut g, 3, Value::Float(0.0), Value::Float(1.0), 10_000);
    let times = g.add(Op::Math(Math::Multiply), vec![wire(arr), Input::Value(Value::Float(4.0))]);
    let avg = g.add(Op::Array(ArrayOp::Average), vec![wire(times)]);
    let mean = object_value(g, avg, "mean", &geo).floats().unwrap()[0];
    assert!((mean - 2.0).abs() < 0.05, "{mean}");

    let mut g = Graph::default();
    let arr = random(&mut g, 3, Value::Vec3([0.0; 3]), Value::Vec3([1.0; 3]), 250);
    let size = g.add(Op::Array(ArrayOp::Size), vec![wire(arr)]);
    assert_eq!(object_value(g, size, "n", &geo).ints().unwrap(), &[250]);

    // Two equal arrays (min = max) added: each element doubled.
    let mut g = Graph::default();
    let a = random(&mut g, 0, Value::Float(1.5), Value::Float(1.5), 4);
    let b = random(&mut g, 9, Value::Float(1.5), Value::Float(1.5), 4);
    let sum = g.add(Op::Math(Math::Add), vec![wire(a), wire(b)]);
    let total = g.add(Op::Array(ArrayOp::Sum), vec![wire(sum)]);
    assert_eq!(object_value(g, total, "total", &geo).floats().unwrap(), &[12.0]);
    let mut g = Graph::default();
    let a = random(&mut g, 0, Value::Float(1.5), Value::Float(1.5), 4);
    let third = g.add(Op::Array(ArrayOp::Element), vec![wire(a), Input::Value(Value::Int(2))]);
    assert_eq!(object_value(g, third, "third", &geo).floats().unwrap(), &[1.5]);
}

#[test]
fn arrays_stay_in_the_one_value_part_for_now() {
    let geo = cloud(3);
    // An array set as an attribute.
    let mut g = Graph::default();
    let arr = random(&mut g, 0, Value::Float(0.0), Value::Float(1.0), 8);
    let set = g.add(Op::set("a"), vec![wire(arr)]);
    g.add(Op::Root { context: Context::Object }, vec![wire(set)]);
    assert_eq!(compile(&g, &geo).unwrap_err().node, set);
    // An array meeting per-point values.
    let mut g = Graph::default();
    let arr = random(&mut g, 0, Value::Float(0.0), Value::Float(1.0), 8);
    let heat = g.add(Op::get("heat"), vec![]);
    let add = g.add(Op::Math(Math::Add), vec![wire(arr), wire(heat)]);
    let set = g.add(Op::set("b"), vec![wire(add)]);
    g.add(Op::Root { context: Context::Point }, vec![wire(set)]);
    assert_eq!(compile(&g, &geo).unwrap_err().node, add);
}

#[test]
fn rotations_from_euler_angles_and_from_an_axis_turn_vectors_alike() {
    let geo = cloud(3);
    let turned = |make: &dyn Fn(&mut Graph) -> usize| {
        let mut g = Graph::default();
        let q = make(&mut g);
        let v = g.add(Op::Math(Math::Rotate), vec![wire(q), Input::Value(Value::Vec3([1.0, 0.0, 0.0]))]);
        object_value(g, v, "v", &geo).vec3s().unwrap()[0]
    };
    let euler = turned(&|g| g.add(Op::Math(Math::Euler), vec![Input::Value(Value::Vec3([0.0, 90.0, 0.0]))]));
    let axis = turned(&|g| g.add(Op::Math(Math::AxisAngle), vec![Input::Value(Value::Vec3([0.0, 1.0, 0.0])), Input::Value(Value::Float(90.0))]));
    for v in [euler, axis] {
        assert!((v[0]).abs() < 1e-5 && (v[1]).abs() < 1e-5 && (v[2] + 1.0).abs() < 1e-5, "{v:?}");
    }
}

#[test]
fn a_rotation_attribute_turns_each_point() {
    let mut geo = cloud(2);
    let quarter = glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
    geo.set(Context::Point, "orient", Attr::new(Column::Quat(Arc::new(vec![quarter; 2])), Role::None));
    let mut g = Graph::default();
    let q = g.add(Op::get("orient"), vec![]);
    let p = g.add(Op::get("P"), vec![]);
    let v = g.add(Op::Math(Math::Rotate), vec![wire(q), wire(p)]);
    let g = setting_p(g, v);
    let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &[]).geo;
    let p1 = points(&out)[1];
    assert!(p1[0].abs() < 1e-5 && (p1[1] - 1.0).abs() < 1e-5, "{p1:?}");
}

/// Two squares side by side on the ground: one of area 1 (x 0 to 1), one of
/// area 3 (x 1 to 4), each with its number as a per-face `id`.
fn small_and_big() -> Geo {
    let pts = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0], [4.0, 0.0, 0.0], [4.0, 0.0, 1.0]];
    let mut g = Geo::from_polygons(pts, vec![4, 4], vec![0, 1, 2, 3, 1, 4, 5, 2]);
    g.set(Context::Face, "id", Attr::new(Column::Float(Arc::new(vec![0.0, 1.0])), Role::None));
    g
}

/// Scatter `count` places with `seed` (and `weight`), read `name` there,
/// and average it.
fn scattered_average(geo: &Geo, seed: u32, count: i32, weight: &str, name: &str) -> Column {
    let mut g = Graph::default();
    let at = g.add(Op::Scatter { seed, weight: weight.into() }, vec![Input::Value(Value::Int(count))]);
    let read = g.add(Op::GetDataAt { name: name.into() }, vec![Input::Wire { node: at, output: SCATTER_SAMPLES }]);
    let avg = g.add(Op::Array(ArrayOp::Average), vec![wire(read)]);
    object_value(g, avg, "avg", geo)
}

#[test]
fn scattered_places_spread_by_area_and_lie_on_the_surface() {
    let geo = small_and_big();
    // Three in four land on the big square.
    let share = scattered_average(&geo, 1, 20_000, "", "id").floats().unwrap()[0];
    assert!((share - 0.75).abs() < 0.02, "{share}");
    // On the ground, around the area-weighted centre (2, 0, 0.5).
    let c = scattered_average(&geo, 1, 20_000, "", "P").vec3s().unwrap()[0];
    assert!((c[0] - 2.0).abs() < 0.05 && c[1].abs() < 1e-6 && (c[2] - 0.5).abs() < 0.02, "{c:?}");
    // The face's normal where there are no normals: straight up or down.
    let n = scattered_average(&geo, 1, 100, "", "N").vec3s().unwrap()[0];
    assert!((n[1].abs() - 1.0).abs() < 1e-5, "{n:?}");
}

#[test]
fn a_weight_moves_where_places_land_and_a_seed_keeps_them() {
    let mut geo = small_and_big();
    geo.set(Context::Face, "w", Attr::new(Column::Float(Arc::new(vec![1.0, 0.0])), Role::None));
    assert_eq!(scattered_average(&geo, 1, 1_000, "w", "id").floats().unwrap(), &[0.0], "none on the big square");
    let a = scattered_average(&geo, 4, 50, "", "P");
    assert_eq!(a.vec3s(), scattered_average(&geo, 4, 50, "", "P").vec3s(), "the same seed, the same places");
    assert_ne!(a.vec3s(), scattered_average(&geo, 5, 50, "", "P").vec3s(), "another seed, other places");
}

#[test]
fn scattering_is_for_the_whole_primitive() {
    let geo = small_and_big();
    let mut g = Graph::default();
    let at = g.add(Op::Scatter { seed: 0, weight: String::new() }, vec![Input::Value(Value::Int(10))]);
    let read = g.add(Op::GetDataAt { name: "P".into() }, vec![wire(at)]);
    let size = g.add(Op::Array(ArrayOp::Size), vec![wire(read)]);
    let set = g.add(Op::set("n"), vec![wire(size)]);
    g.add(Op::Root { context: Context::Point }, vec![wire(set)]);
    assert_eq!(compile(&g, &geo).unwrap_err().node, at);
}

#[test]
fn a_reference_input_gives_its_paths_all_or_those_listed_in_order() {
    let geo = cloud(1);
    let refs = vec![vec!["/trees/pine_a".to_string(), "/trees/pine_b".into(), "/rocks/boulder".into()]];
    let listed = |input: usize, paths: Option<Vec<&str>>| {
        let mut g = Graph::default();
        let paths = g.add(Op::GetReference { input, paths: paths.map(|v| v.into_iter().map(String::from).collect()) }, vec![]);
        let size = g.add(Op::Array(ArrayOp::Size), vec![wire(paths)]);
        let set = g.add(Op::set("n"), vec![wire(size)]);
        g.add(Op::Root { context: Context::Object }, vec![wire(set)]);
        let out = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &refs).geo;
        out.attr(Context::Object, "n").unwrap().column.ints().unwrap()[0]
    };
    assert_eq!(listed(1, None), 3, "all of them");
    assert_eq!(listed(1, Some(vec!["/rocks/boulder", "/trees/pine_a", "/nowhere"])), 2, "those listed that are there");
    assert_eq!(listed(2, None), 0, "nothing wired there");
}

#[test]
fn which_outputs_are_arrays() {
    let mut g = Graph::default();
    let one = random(&mut g, 0, Value::Float(0.0), Value::Float(1.0), 0);
    let many = random(&mut g, 0, Value::Float(0.0), Value::Float(1.0), 5);
    let twice = g.add(Op::Math(Math::Multiply), vec![wire(many), Input::Value(Value::Float(2.0))]);
    let size = g.add(Op::Array(ArrayOp::Size), vec![wire(twice)]);
    let at = g.add(Op::Scatter { seed: 0, weight: String::new() }, vec![Input::Value(Value::Int(3))]);
    let paths = g.add(Op::GetReference { input: 1, paths: None }, vec![]);
    let arrays = super::output_arrays(&g);
    assert_eq!([arrays[one], arrays[many], arrays[twice], arrays[size], arrays[at], arrays[paths]], [false, true, true, false, true, true]);
}

/// Scatter's three outputs: its positions and normals are Get Data at
/// Samples of `P` and `N` at its samples.
#[test]
fn scatter_gives_positions_normals_and_samples() {
    let geo = small_and_big();
    let read = |output: usize, name: Option<&str>| {
        let mut g = Graph::default();
        let at = g.add(Op::Scatter { seed: 3, weight: String::new() }, vec![Input::Value(Value::Int(200))]);
        let v = match name {
            Some(n) => g.add(Op::GetDataAt { name: n.into() }, vec![Input::Wire { node: at, output }]),
            None => g.add(Op::Array(ArrayOp::Average), vec![Input::Wire { node: at, output }]),
        };
        let v = if name.is_some() { g.add(Op::Array(ArrayOp::Average), vec![wire(v)]) } else { v };
        object_value(g, v, "v", &geo).vec3s().unwrap()[0]
    };
    assert_eq!(read(SCATTER_POSITIONS, None), read(SCATTER_SAMPLES, Some("P")), "positions are P at the samples");
    assert_eq!(read(SCATTER_NORMALS, None), read(SCATTER_SAMPLES, Some("N")), "normals are N at the samples");
    // A position is not a sample: Get Data at Samples refuses it.
    let mut g = Graph::default();
    let at = g.add(Op::Scatter { seed: 3, weight: String::new() }, vec![Input::Value(Value::Int(5))]);
    let bad = g.add(Op::GetDataAt { name: "P".into() }, vec![Input::Wire { node: at, output: SCATTER_POSITIONS }]);
    let set = g.add(Op::set("v"), vec![wire(bad)]);
    g.add(Op::Root { context: Context::Object }, vec![wire(set)]);
    assert_eq!(compile(&g, &geo).unwrap_err().node, bad);
}

/// Random Orientation: one per element of the normals it is given, +Y along
/// each normal, turned about it at random; the same for the same seed.
#[test]
fn random_orientations_follow_the_normals() {
    let geo = small_and_big();
    let make = |seed: u32| {
        let mut g = Graph::default();
        let at = g.add(Op::Scatter { seed: 1, weight: String::new() }, vec![Input::Value(Value::Int(300))]);
        let turn = g.add(Op::RandomOrientation { seed }, vec![
            Input::Wire { node: at, output: SCATTER_NORMALS }, Input::Value(Value::Vec3([0.0, 360.0, 0.0])),
            Input::Value(Value::Int(0)), Input::Value(Value::Int(0)),
        ]);
        let protos = g.add(Op::GetReference { input: 1, paths: None }, vec![]);
        let inst = g.add(Op::Instancer { path: "scatter".into(), seed: 0 }, vec![
            Input::Wire { node: at, output: SCATTER_POSITIONS }, wire(protos), Input::Value(Value::Int(0)),
            wire(turn), Input::Value(Value::Float(1.0)),
        ]);
        g.add(Op::Root { context: Context::Object }, vec![wire(inst)]);
        run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &[vec!["/p".to_string()]]).instances.remove(0)
    };
    let a = make(4);
    assert_eq!(a.orientations.len(), 300);
    for q in &a.orientations {
        let up = glam::Quat::from_array(*q) * glam::Vec3::Y;
        assert!((up.y.abs() - 1.0).abs() < 1e-4, "{up:?}");
    }
    assert_eq!(a, make(4), "the same seed, the same turns");
    assert_ne!(a.orientations, make(5).orientations);
}

#[test]
fn the_instancer_makes_usds_point_instancer_from_its_inputs() {
    let geo = small_and_big();
    let refs = vec![vec!["/assets/pine".to_string(), "/assets/rock".into()]];
    let mut g = Graph::default();
    let at = g.add(Op::Scatter { seed: 2, weight: String::new() }, vec![Input::Value(Value::Int(500))]);
    let protos = g.add(Op::GetReference { input: 1, paths: None }, vec![]);
    // Scales: an array of random numbers, one per instance.
    let scales = random(&mut g, 9, Value::Float(0.5), Value::Float(2.0), 500);
    let inst = g.add(Op::Instancer { path: "/set/scatter".into(), seed: 5 }, vec![
        Input::Wire { node: at, output: SCATTER_POSITIONS }, wire(protos), Input::Value(Value::Int(-1)),
        Input::Value(Value::Quat([0.0, 0.0, 0.0, 1.0])), wire(scales),
    ]);
    g.add(Op::Root { context: Context::Object }, vec![wire(inst)]);
    let ran = run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &refs);
    let i = &ran.instances[0];
    assert_eq!((i.path.as_str(), i.positions.len(), i.prototypes.len()), ("/set/scatter", 500, 2));
    assert_eq!((i.proto_indices.len(), i.orientations.len(), i.scales.len()), (500, 500, 500), "every array as long as the positions");
    // Both prototypes picked at random; every one on the ground; a number scales evenly.
    assert!(i.proto_indices.contains(&0) && i.proto_indices.contains(&1));
    assert!(i.positions.iter().all(|p| p[1].abs() < 1e-6));
    assert!(i.scales.iter().all(|s| (0.5..=2.0).contains(&s[0]) && s[0] == s[1] && s[1] == s[2]));
    assert!(i.orientations.iter().all(|q| *q == [0.0, 0.0, 0.0, 1.0]), "one rotation for all");
    assert_eq!(run(&compile(&g, &geo).unwrap(), &geo, &no_params(), &refs).instances, ran.instances, "the same seeds, the same instances");
}

#[test]
fn the_instancer_is_for_object_graphs_and_wants_positions_and_paths() {
    let geo = small_and_big();
    let make = |context: Context, output: usize, protos_are_paths: bool| {
        let mut g = Graph::default();
        let at = g.add(Op::Scatter { seed: 0, weight: String::new() }, vec![Input::Value(Value::Int(5))]);
        let protos = if protos_are_paths { g.add(Op::GetReference { input: 1, paths: None }, vec![]) } else { at };
        let inst = g.add(Op::Instancer { path: "i".into(), seed: 0 }, vec![
            Input::Wire { node: at, output }, wire(protos), Input::Value(Value::Int(-1)),
            Input::Value(Value::Quat([0.0, 0.0, 0.0, 1.0])), Input::Value(Value::Float(1.0)),
        ]);
        g.add(Op::Root { context }, vec![wire(inst)]);
        (compile(&g, &geo).err().map(|e| e.node), inst)
    };
    let (err, inst) = make(Context::Object, SCATTER_POSITIONS, false);
    assert_eq!(err, Some(inst), "positions are not prototypes");
    assert_eq!(make(Context::Object, SCATTER_SAMPLES, true).0, Some(inst), "samples are not positions");
    assert!(make(Context::Object, SCATTER_POSITIONS, true).0.is_none());
}

/// A per-face graph's value of `out` (a node), set as `name`.
fn face_value(mut g: Graph, out: usize, geo: &Geo) -> Column {
    let set = g.add(Op::set("v"), vec![wire(out)]);
    g.add(Op::Root { context: Context::Face }, vec![wire(set)]);
    let out = run(&compile(&g, geo).unwrap(), geo, &no_params(), &[]).geo;
    out.attr(Context::Face, "v").unwrap().column.clone()
}

/// Get Array Data per face: its point numbers (ptIndices), how many, the
/// first; its points' positions, worked on item by item, then averaged.
#[test]
fn get_array_data_keeps_what_each_face_touches() {
    let geo = small_and_big();
    let list = |g: &mut Graph, name: &str, from: Option<Context>| g.add(Op::GetArrayData { name: name.into(), context: from }, vec![]);
    // How many, and the first of each: faces 0 1 2 3 and 1 4 5 2.
    let mut g = Graph::default();
    let ix = list(&mut g, "ptIndices", None);
    let size = g.add(Op::Array(ArrayOp::Size), vec![wire(ix)]);
    assert_eq!(face_value(g, size, &geo).ints().unwrap(), &[4, 4]);
    let mut g = Graph::default();
    let ix = list(&mut g, "ptIndices", None);
    let second = g.add(Op::Array(ArrayOp::Element), vec![wire(ix), Input::Value(Value::Int(1))]);
    assert_eq!(face_value(g, second, &geo).ints().unwrap(), &[1, 4]);
    // The points' positions doubled, item by item, then averaged: twice
    // the face's centre.
    let mut g = Graph::default();
    let p = list(&mut g, "P", Some(Context::Point));
    let twice = g.add(Op::Math(Math::Multiply), vec![wire(p), Input::Value(Value::Float(2.0))]);
    let avg = g.add(Op::Array(ArrayOp::Average), vec![wire(twice)]);
    let c = face_value(g, avg, &geo).vec3s().unwrap().to_vec();
    assert!((c[0][0] - 1.0).abs() < 1e-5 && (c[0][2] - 1.0).abs() < 1e-5, "{c:?}");
    assert!((c[1][0] - 5.0).abs() < 1e-5, "{c:?}");
    // A per-face value meets each item: the points' x plus the face's id.
    let mut g = Graph::default();
    let p = list(&mut g, "P", Some(Context::Point));
    let id = g.add(Op::get("id"), vec![]);
    let x = g.add(Op::Math(Math::Dot), vec![wire(p), Input::Value(Value::Vec3([1.0, 0.0, 0.0]))]);
    let plus = g.add(Op::Math(Math::Add), vec![wire(x), wire(id)]);
    let sum = g.add(Op::Array(ArrayOp::Sum), vec![wire(plus)]);
    assert_eq!(face_value(g, sum, &geo).floats().unwrap(), &[2.0, 14.0], "0+1+1+0, then 1+4+4+1 plus 4 x 1");
    // A list is not written as an attribute.
    let mut g = Graph::default();
    let ix = list(&mut g, "ptIndices", None);
    let set = g.add(Op::set("v"), vec![wire(ix)]);
    g.add(Op::Root { context: Context::Face }, vec![wire(set)]);
    assert_eq!(compile(&g, &geo).unwrap_err().node, set);
}

/// In an Object graph, Get Array Data gives every value as the graph's
/// array; a Count takes an array's length.
#[test]
fn an_object_graph_reads_every_value_and_a_count_takes_an_arrays_length() {
    let geo = small_and_big();
    let mut g = Graph::default();
    let p = g.add(Op::GetArrayData { name: "P".into(), context: None }, vec![]);
    let size = g.add(Op::Array(ArrayOp::Size), vec![wire(p)]);
    assert_eq!(object_value(g, size, "n", &geo).ints().unwrap(), &[6]);
    let mut g = Graph::default();
    let at = g.add(Op::Scatter { seed: 0, weight: String::new() }, vec![Input::Value(Value::Int(37))]);
    let r = g.add(Op::Random { seed: 0 }, vec![Input::Value(Value::Float(0.0)), Input::Value(Value::Float(1.0)),
        Input::Wire { node: at, output: SCATTER_POSITIONS }, Input::Value(Value::Int(0))]);
    let size = g.add(Op::Array(ArrayOp::Size), vec![wire(r)]);
    assert_eq!(object_value(g, size, "n", &geo).ints().unwrap(), &[37], "as many as the positions");
}

