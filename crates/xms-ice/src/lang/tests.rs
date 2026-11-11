//! Snippets parsed, lowered, compiled and run on geometry: what they set is
//! what the same nodes would.

use std::collections::HashMap;
use std::sync::Arc;

use xms_geo::{Attr, Column, Context, Geo, Role};

use super::{default_value, lower, parse};
use crate::graph::{wire, Graph, Input, Op};
use crate::types::Value;
use crate::{compile, run};

/// `n` points along X, normals up, `heat` the point's number.
fn cloud(n: usize) -> Geo {
    let mut g = Geo::from_points((0..n).map(|i| [i as f32, 0.0, 0.0]).collect());
    g.set(
        Context::Point,
        "N",
        Attr::new(Column::Vec3(Arc::new(vec![[0.0, 1.0, 0.0]; n])), Role::Normal),
    );
    g.set(
        Context::Point,
        "heat",
        Attr::new(Column::Float(Arc::new((0..n).map(|i| i as f32).collect())), Role::None),
    );
    g
}

/// Two unit squares side by side.
fn two_quads() -> Geo {
    let pts = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [2.0, 1.0, 0.0],
    ];
    Geo::from_polygons(pts, vec![4, 4], vec![0, 1, 4, 3, 1, 2, 5, 4])
}

/// `text` run over `context` of `geo`, its parameters given `inputs`: the
/// geometry it gives.
fn ran(text: &str, context: Context, geo: &Geo, inputs: &[(String, Input)]) -> Geo {
    let s = parse(text).unwrap_or_else(|e| panic!("{e}"));
    let mut g = Graph::default();
    let lowered = lower(&s, context, inputs, Some(geo), &mut g).unwrap_or_else(|e| panic!("{e}"));
    let sets = lowered.sets.iter().map(|n| wire(*n)).collect();
    g.add(Op::Root { context }, sets);
    let plan = compile(&g, geo).unwrap_or_else(|e| panic!("{e:?} at {:?}", lowered.pos_of(e.node)));
    run(&plan, geo, &HashMap::new(), &[]).geo
}

fn floats(geo: &Geo, ctx: Context, name: &str) -> Vec<f32> {
    match &geo.attr(ctx, name).expect("set").column {
        Column::Float(v) => v.to_vec(),
        Column::Int(v) => v.iter().map(|&i| i as f32).collect(),
        other => panic!("{name} is {other:?}"),
    }
}

fn error_of(text: &str, context: Context, geo: &Geo) -> String {
    let s = match parse(text) {
        Ok(s) => s,
        Err(e) => return e.to_string(),
    };
    lower(&s, context, &[], Some(geo), &mut Graph::default())
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default()
}

#[test]
fn a_point_snippet_moves_points_along_their_normals() {
    let geo = cloud(10);
    let text = r#"
        snippet inflate (
            float amount = 0.1  [[ float min = 0, float max = 2, string label = "Amount" ]],
            output float pushed = 0
        )
        {
            pushed = amount * smoothstep(2, 6, @heat);
            @P += normalize(@N) * pushed;
        }
    "#;
    let out = ran(text, Context::Point, &geo, &[]);
    let ys: Vec<f32> = out.points().iter().map(|p| p[1]).collect();
    // Below 2 nothing, above 6 all of it, halfway half.
    assert_eq!(ys[1], 0.0);
    assert!((ys[4] - 0.05).abs() < 1e-6, "{ys:?}");
    assert_eq!(ys[8], 0.1);
    // A parameter given: twice the push.
    let out = ran(
        text,
        Context::Point,
        &geo,
        &[("amount".into(), Input::Value(Value::Float(0.2)))],
    );
    assert_eq!(out.points()[8][1], 0.2);
}

#[test]
fn an_if_sets_only_where_it_holds() {
    let geo = cloud(6);
    let text = r#"
        snippet s (float h = 2.5)
        {
            float @mask;
            if (@heat > h) {
                @P.y = 1;
                @mask = 1;
            } else
                @P *= 2;
        }
    "#;
    let out = ran(text, Context::Point, &geo, &[]);
    assert_eq!(out.points()[1], [2.0, 0.0, 0.0]);
    assert_eq!(out.points()[4], [4.0, 1.0, 0.0]);
    // Made in one side only: zero on the other.
    assert_eq!(floats(&out, Context::Point, "mask"), [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
}

#[test]
fn constant_loops_are_worked_through_and_functions_inlined() {
    let geo = cloud(3);
    let text = r#"
        float twice(float x) { return x * 2; }
        void scale(output vector v, float k) { v = v * k; }
        snippet s ()
        {
            float total = 0;
            for (int i = 0; i < 4; i++)
                total += i;
            vector up = @N;
            scale(up, twice(total));
            @P += up;
        }
    "#;
    let out = ran(text, Context::Point, &geo, &[]);
    // 0 + 1 + 2 + 3 = 6, twice is 12.
    assert_eq!(out.points()[2], [2.0, 12.0, 0.0]);
}

#[test]
fn a_face_snippet_reads_its_points_and_its_intrinsics() {
    let geo = two_quads();
    let text = r#"
        snippet f [[ string context = "face" ]] (output int n = 0)
        {
            point @centre;
            @centre = get_data(@P, "point");
            point pts[] = get_array_data(@P, "point");
            n = arraylength(pts);
            float @second = @intrinsic:faceIndex == 1;
        }
    "#;
    let out = ran(text, Context::Face, &geo, &[]);
    let Column::Vec3(c) = &out.attr(Context::Face, "centre").expect("set").column else {
        panic!("a vector")
    };
    assert_eq!(c[1], [1.5, 0.5, 0.0]);
    assert_eq!(floats(&out, Context::Face, "second"), [0.0, 1.0]);
}

#[test]
fn near_points_and_reads_at_their_numbers() {
    // Points 0..5 along X: each one's two nearest are itself and a neighbour;
    // their average x, half a unit off (towards the inside at the ends).
    let geo = cloud(5);
    let text = r#"
        snippet relax (int count = 2)
        {
            int near[];
            float d[];
            int found = near_points(@P, count, 0, near, d);
            float @found = found;
            @P = average(get_data(@P, near));
        }
    "#;
    let out = ran(text, Context::Point, &geo, &[]);
    assert_eq!(floats(&out, Context::Point, "found"), [2.0; 5]);
    assert_eq!(out.points()[0][0], 0.5);
}

#[test]
fn errors_name_their_line_and_what_is_wrong() {
    let geo = cloud(3);
    let e = error_of("snippet s () {\n  @kept = 1;\n}", Context::Point, &geo);
    assert!(e.starts_with("line 2") && e.contains("int @kept"), "{e}");
    let e = error_of("snippet s () { float x = @nothing; }", Context::Point, &geo);
    assert!(e.contains("No attribute @nothing"), "{e}");
    let e = error_of("snippet s () { @intrinsic:ptIndex = 1; }", Context::Point, &geo);
    assert!(e.contains("read only"), "{e}");
    let e = error_of("snippet s () { vector @heat; }", Context::Point, &geo);
    assert!(e.contains("declared a vector"), "{e}");
    let e = error_of(
        "snippet s () { float x = get_data(@P, \"points\"); }",
        Context::Point,
        &geo,
    );
    assert!(e.contains("not a context"), "{e}");
    let e = error_of(
        "snippet s () { float x = 0; while (x < 1) x += 1; }",
        Context::Point,
        &geo,
    );
    assert!(e.contains("not yet"), "{e}");
    let e = error_of(
        "snippet s () { int n = 3; for (int i = 0; i < @heat; i++) n += 1; }",
        Context::Point,
        &geo,
    );
    assert!(e.contains("known when compiling"), "{e}");
    // Per face, a point attribute: by get_data.
    let e = error_of("snippet s () { vector n = @N; }", Context::Face, &two_quads());
    assert!(e.contains("get_data(@N, \"point\")"), "{e}");
}

#[test]
fn defaults_become_the_ports_values() {
    let s = parse("snippet s (float a = 0.5, color c = color(1, 0, 0), int k = 2 * 3) { }").unwrap();
    let values: Vec<Option<Value>> = s.params.iter().map(default_value).collect();
    assert_eq!(
        values,
        [
            Some(Value::Float(0.5)),
            Some(Value::Vec3([1.0, 0.0, 0.0])),
            Some(Value::Int(6))
        ]
    );
}

#[test]
fn noise_is_osl_s_signed_or_from_zero_to_one() {
    let geo = cloud(5);
    let text = r#"
        snippet n ()
        {
            float @s = noise("perlin", @P * 0.3 + point(0.1, 0.2, 0.3));
            float @u = noise(@P * 0.3 + point(0.1, 0.2, 0.3));
            float @c = cellnoise(@P);
        }
    "#;
    let out = ran(text, Context::Point, &geo, &[]);
    let (s, u, c) = (
        floats(&out, Context::Point, "s"),
        floats(&out, Context::Point, "u"),
        floats(&out, Context::Point, "c"),
    );
    for i in 0..5 {
        let p = [i as f32 * 0.3 + 0.1, 0.2, 0.3];
        assert_eq!(s[i], crate::noise::perlin(p));
        assert!((u[i] - 0.5 * (s[i] + 1.0)).abs() < 1e-6);
        assert_eq!(c[i], crate::noise::cell([i as f32, 0.0, 0.0]));
    }
    let e = error_of("snippet n () { float x = noise(\"gabor\", @P); }", Context::Point, &geo);
    assert!(e.contains("\"gabor\" noise"), "{e}");
}

#[test]
fn a_geometry_s_default_is_its_reference_input() {
    let s = parse("snippet s (geometry ground = 2, geometry rest, geometry rocks = get_reference(1, \"/rocks*\")) { }")
        .unwrap();
    let refs: Vec<Option<usize>> = super::ports(&s).iter().map(|p| p.reference).collect();
    assert_eq!(refs, [Some(2), Some(1), None]);
    let text = "snippet s (geometry ground = 2) { vector d = @P - get_data(@P, ground); }";
    let mut g = Graph::default();
    lower(&parse(text).unwrap(), Context::Point, &[], Some(&cloud(2)), &mut g).unwrap();
    assert!(g
        .nodes
        .iter()
        .any(|n| matches!(n.op, Op::GetReference { input: 2, paths: None })));
    let e = error_of(
        "snippet s (geometry ground = 0) { vector d = get_data(@P, ground); }",
        Context::Point,
        &cloud(2),
    );
    assert!(e.contains("numbered from 1"), "{e}");
}
