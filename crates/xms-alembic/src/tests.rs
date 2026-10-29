//! Read against files other programs wrote (`tests/data`, from OpenUSD's
//! `usdAbc` tests), checked against what `usdAbc` gives for them.

use super::*;

fn open(name: &str) -> Archive {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name);
    Archive::open(&path).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn mesh<'a>(a: &'a Archive, path: &str) -> Mesh<'a> {
    match geom(&a.object(path).unwrap().expect(path)).unwrap() {
        Geom::PolyMesh(m) | Geom::SubD(m) => m,
        other => panic!("{path}: {other:?}"),
    }
}

fn xform<'a>(a: &'a Archive, path: &str) -> Xform<'a> {
    match geom(&a.object(path).unwrap().expect(path)).unwrap() {
        Geom::Xform(x) => x,
        other => panic!("{path}: {other:?}"),
    }
}

/// A cube Blender wrote: its hierarchy, points, polygons and indexed texture
/// coordinates, as `usdAbc` reads them (its baseline layer).
#[test]
fn a_mesh_is_read_as_written() {
    let a = open("indexedTextureCoordinates.abc");
    assert_eq!(a.metadata().get("_ai_Application"), Some("Blender"));
    assert_eq!(a.time_range(), None, "nothing moves");
    let root = a.root().unwrap();
    assert_eq!(root.child_names(), vec!["Cube"]);
    let cube = a.object("/Cube").unwrap().unwrap();
    assert_eq!(cube.schema(), Some("AbcGeom_Xform_v3"));
    assert_eq!(cube.child_names(), vec!["CubeShape"]);
    let vis = visibility(&cube).unwrap().expect("visible");
    assert_eq!(visibility_at(&vis, 0).unwrap(), Visibility::Visible);
    let x = xform(&a, "/Cube");
    assert_eq!(x.ops, vec![XformOp::Matrix]);
    assert_eq!(x.matrix_at(0).unwrap(), IDENTITY);
    assert!(!x.inherits_at(0).unwrap());

    let m = mesh(&a, "/Cube/CubeShape");
    assert!(m.subd.is_none());
    let p = m.positions_at(0).unwrap();
    assert_eq!(p.len(), 8);
    assert_eq!((p[0], p[7]), ([-1.0, -1.0, 1.0], [1.0, 1.0, -1.0]));
    assert_eq!(m.face_counts_at(0).unwrap(), vec![4; 6]);
    assert_eq!(
        m.face_indices_at(0).unwrap(),
        vec![2, 3, 1, 0, 6, 7, 3, 2, 4, 5, 7, 6, 0, 1, 5, 4, 0, 4, 6, 2, 5, 1, 3, 7]
    );
    let uv = m.uvs.as_ref().expect("texture coordinates");
    assert_eq!((uv.scope, uv.extent), (Scope::FaceVarying, 2));
    let s = uv.sample(0).unwrap();
    let st = chunked::<f32, 2>(&s.values.to_f32());
    assert_eq!(st.len(), 14);
    assert!((st[0][0] - 0.9999134).abs() < 1e-6 && (st[0][1] - 0.25004348).abs() < 1e-6);
    assert_eq!(
        s.indices.unwrap(),
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 4, 11, 12, 13, 8, 11, 8, 4, 7, 9, 2, 1, 10]
    );
    let meshtype = m
        .extra
        .user
        .iter()
        .find(|p| p.header.name == "meshtype")
        .expect("a user property");
    assert_eq!(meshtype.sample(0).unwrap(), Values::Bool(vec![false]));
}

/// Points Houdini wrote at 30 fps that move a unit a frame: each sample at its
/// time, from the first frame's to the last's.
#[test]
fn moving_points_have_a_sample_each_frame() {
    let a = open("testusdtimecode.abc");
    assert_eq!(a.metadata().get("_ai_DCC_FPS"), Some("30.000000"));
    let (start, end) = a.time_range().expect("it moves");
    assert!((start - 1.0 / 30.0).abs() < 1e-12 && (end - 1.0).abs() < 1e-12);
    let m = mesh(&a, "/font1/transform1");
    assert_eq!(m.num_samples(), 30);
    assert!(!m.positions.is_constant());
    assert!(m.face_counts.is_constant(), "its polygons stay");
    let times = m.times();
    assert!((times[29] - 1.0).abs() < 1e-12);
    assert_eq!(m.positions_at(0).unwrap()[0], [0.5, -0.5, 0.5]);
    assert_eq!(m.positions_at(29).unwrap()[0], [29.5, -0.5, 0.5]);
    assert_eq!(
        m.positions_at(99).unwrap()[0],
        [29.5, -0.5, 0.5],
        "past the last: the last"
    );
    assert_eq!(m.positions.floor(0.5), 14);
    let x = xform(&a, "/font1");
    assert!(x.is_constant());
}

/// Instances stand for the object they name; their sources' transforms are
/// read with them.
#[test]
fn instances_name_their_source() {
    let a = open("nestedInstancing.abc");
    let src = |p: &str| a.object(p).unwrap().expect(p).instance_source().unwrap();
    assert_eq!(src("/rootNode/c/instance1"), Some("/rootNode/instance0".to_string()));
    assert_eq!(
        src("/rootNode/instance0/f/g"),
        Some("/rootNode/instance0/a".to_string())
    );
    assert_eq!(src("/rootNode/instance0/a"), None, "a source is no instance");
    let a_x = xform(&a, "/rootNode/instance0/a");
    assert_eq!(a_x.ops, vec![XformOp::Translate]);
    assert_eq!(a_x.matrix_at(0).unwrap()[3], [-2.0, 0.0, 0.0, 1.0]);
    let m = mesh(&a, "/rootNode/instance0/a/aMesh");
    assert_eq!(m.normals.as_ref().map(|n| n.scope), Some(Scope::FaceVarying));
}

/// A camera's lens over time, and the archive's time samplings.
#[test]
fn a_camera_is_read_with_its_lens() {
    let a = open("testAbcCamera.abc");
    assert_eq!(a.time_sampling_count(), 4);
    let (start, end) = a.time_range().unwrap();
    assert!(start.abs() < 1e-12 && (end - 20.0 / 24.0).abs() < 1e-9);
    let mut cameras = vec![];
    let mut stack = vec![a.root().unwrap()];
    while let Some(o) = stack.pop() {
        stack.extend(o.children().unwrap());
        if let Geom::Camera(c) = geom(&o).unwrap() {
            cameras.push((o.path.clone(), c.sample_at(0).unwrap()));
        }
    }
    assert!(!cameras.is_empty());
    let (_, c) = &cameras[0];
    assert_eq!((c.horizontal_aperture, c.vertical_aperture), (1000.0, 400.0));
    assert!((c.f_stop - 5.6).abs() < 1e-9 && (c.near - 0.1).abs() < 1e-9);
}

/// Arbitrary parameters: a colour indexed by face corner, a constant one, and
/// integers, by their names.
#[test]
fn arbitrary_parameters_are_read() {
    let a = open("hasColorSet.abc");
    let m = mesh(&a, "/hasColorSet/hasColorSetShape");
    let names: Vec<&str> = m.extra.arb.iter().map(|p| p.name.as_str()).collect();
    for n in ["colorSet", "vary", "rman_subdivScheme"] {
        assert!(names.contains(&n), "{n} in {names:?}");
    }
    let set = m.extra.arb.iter().find(|p| p.name == "colorSet").unwrap();
    assert_eq!(
        (set.scope, set.interpretation.as_str(), set.extent),
        (Scope::FaceVarying, "rgb", 3)
    );
    assert!(set.indices.is_some());
    let vary = m.extra.arb.iter().find(|p| p.name == "vary").unwrap();
    assert_eq!(vary.sample(0).unwrap().values.len(), 3);
}

/// Operations put together as Alembic does: the first outermost.
#[test]
fn transforms_compose_their_operations() {
    let m = matrix_of(&[XformOp::Translate, XformOp::RotateZ], &[1.0, 2.0, 3.0, 90.0]);
    // A point at (1, 0, 0) is turned to (0, 1, 0), then moved.
    let p = [1.0, 0.0, 0.0, 1.0];
    let q: Vec<f64> = (0..4).map(|j| (0..4).map(|k| p[k] * m[k][j]).sum()).collect();
    assert!(
        (q[0] - 1.0).abs() < 1e-12 && (q[1] - 3.0).abs() < 1e-12 && (q[2] - 3.0).abs() < 1e-12,
        "{q:?}"
    );
    let s = matrix_of(&[XformOp::Scale], &[2.0, 3.0, 4.0]);
    assert_eq!((s[0][0], s[1][1], s[2][2]), (2.0, 3.0, 4.0));
}

/// What is not an Ogawa Alembic file is refused, saying why.
#[test]
fn other_files_are_refused() {
    assert!(matches!(
        Archive::from_bytes(b"#usda 1.0\n".to_vec()),
        Err(Error::NotAlembic(_))
    ));
    let mut unfinished = b"Ogawa\x00\x00\x01".to_vec();
    unfinished.extend(16u64.to_le_bytes());
    assert!(matches!(Archive::from_bytes(unfinished), Err(Error::NotAlembic(_))));
    assert!(matches!(Archive::open("/nowhere/at/all.abc"), Err(Error::Io(_))));
}
