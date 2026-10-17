use std::collections::HashMap;
use std::sync::Arc;

use crate::core::geo::{Attr, Column, Context, Geo, Role, POINTS};

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
    let set = g.add(Op::SetData { name: "P".into() }, vec![wire(value)]);
    g.add(Op::Root { context: Context::Point }, vec![wire(set)]);
    g
}

#[test]
fn p_doubled_and_every_other_column_still_shared() {
    let geo = cloud(10);
    let mut g = Graph::default();
    let p = g.add(Op::GetData { name: "P".into() }, vec![]);
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
    let p = g.add(Op::GetData { name: "P".into() }, vec![]);
    let n = g.add(Op::GetData { name: "N".into() }, vec![]);
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
    let p = g.add(Op::GetData { name: "P".into() }, vec![]);
    let lift = g.add(Op::GetData { name: "lift".into() }, vec![]);
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
    let p = g.add(Op::GetData { name: "P".into() }, vec![]);
    let bad = g.add(Op::Math(Math::Add), vec![wire(p), Input::Value(Value::Bool(true))]);
    let g = setting_p(g, bad);
    assert_eq!(compile(&g, &geo).unwrap_err().node, bad);

    // A per-face attribute in a per-point graph.
    let mut g = Graph::default();
    let id = g.add(Op::GetData { name: "id".into() }, vec![]);
    let set = g.add(Op::SetData { name: "copy".into() }, vec![wire(id)]);
    g.add(Op::Root { context: Context::Point }, vec![wire(set)]);
    let e = compile(&g, &geo).unwrap_err();
    assert_eq!(e.node, id);
    assert!(e.message.contains("Face"), "{}", e.message);
}

#[test]
fn only_what_reaches_the_root_is_compiled() {
    let geo = cloud(3);
    let mut g = Graph::default();
    let p = g.add(Op::GetData { name: "P".into() }, vec![]);
    // Reads heat, but feeds nothing.
    let heat = g.add(Op::GetData { name: "heat".into() }, vec![]);
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
    let heat = g.add(Op::GetData { name: "heat".into() }, vec![]);
    let plus = g.add(Op::Math(Math::Add), vec![wire(heat), Input::Value(Value::Float(1.0))]);
    let set_a = g.add(Op::SetData { name: "a".into() }, vec![wire(plus)]);
    let a = g.add(Op::GetData { name: "a".into() }, vec![]);
    let times = g.add(Op::Math(Math::Multiply), vec![wire(a), Input::Value(Value::Float(10.0))]);
    let set_b = g.add(Op::SetData { name: "b".into() }, vec![wire(times)]);
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
    let heat = g.add(Op::GetData { name: "heat".into() }, vec![]);
    let pick = g.add(Op::GetData { name: "pick".into() }, vec![]);
    let sum = g.add(Op::Math(Math::Add), vec![wire(heat), wire(pick)]);
    let set = g.add(Op::SetData { name: "sum".into() }, vec![wire(sum)]);
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
    let p = g.add(Op::GetData { name: "P".into() }, vec![]);
    let n = g.add(Op::GetData { name: "N".into() }, vec![]);
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
/// `cargo test --release ice_engine_speed -- --ignored --nocapture`
#[test]
#[ignore]
fn ice_engine_speed() {
    let n = 1_000_000;
    let geo = cloud(n);
    let mut g = Graph::default();
    let p = g.add(Op::GetData { name: "P".into() }, vec![]);
    let nrm = g.add(Op::GetData { name: "N".into() }, vec![]);
    let heat = g.add(Op::GetData { name: "heat".into() }, vec![]);
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
