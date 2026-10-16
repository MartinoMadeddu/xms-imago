//! Tests on the vendored libraries as a whole.

use crate::{Library, Type, Value};

fn lib() -> &'static Library { Library::standard() }

#[test]
fn the_libraries_read() {
    let lib = lib();
    // MaterialX 1.39: 824 definitions of 226 nodes.
    assert!(lib.defs().len() >= 800, "{} definitions", lib.defs().len());
    assert!(lib.nodes().len() >= 200, "{} nodes", lib.nodes().len());
    // Every data type the libraries declare is one Imago knows.
    for t in &lib.typedefs {
        assert!(Type::from_name(t).is_some(), "unknown typedef {t}");
    }
}

#[test]
fn every_node_has_a_current_definition_with_an_output() {
    let lib = lib();
    for node in lib.nodes() {
        let def = lib.default_def(node).unwrap_or_else(|| panic!("{node} has no current definition"));
        assert!(!def.outputs.is_empty(), "{} has no output", def.name);
    }
}

#[test]
fn every_definition_is_consistent() {
    for def in lib().defs() {
        for o in &def.outputs {
            if let Some(i) = &o.default_input {
                assert!(def.input(i).is_some(), "{}: output {} passes through {i}, not an input", def.name, o.name);
            }
        }
        for i in &def.inputs {
            // A value, when there is one, is of the input's type.
            if let Some(v) = &i.value {
                assert_eq!(Value::parse(i.ty, &v.to_text()).as_ref(), Some(v), "{}.{}", def.name, i.name);
            }
            // Closures and shaders are connected, never set.
            if i.ty.is_shading() {
                assert!(i.value.is_none(), "{}.{} has a value", def.name, i.name);
            }
        }
        let mut names: Vec<_> = def.inputs.iter().map(|i| &i.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), def.inputs.len(), "{} has an input twice", def.name);
    }
}

#[test]
fn standard_surface_is_version_1_0_1_with_its_inputs() {
    let lib = lib();
    let def = lib.default_def("standard_surface").unwrap();
    assert_eq!(def.name, "ND_standard_surface_surfaceshader");
    assert_eq!(def.version.as_deref(), Some("1.0.1"));
    assert_eq!(def.group, "pbr");
    assert_eq!(def.out_type(), Type::SurfaceShader);
    // 1.0.1 inherits 1.0.0's inputs and changes two defaults.
    assert!(def.inputs.len() >= 40, "{} inputs", def.inputs.len());
    let base = def.input("base").unwrap();
    assert_eq!(base.value, Some(Value::Floats(vec![1.0])));
    assert_eq!(base.label(), "Base");
    assert_eq!(base.ui.folder.as_deref(), Some("Base"));
    assert_eq!(base.ui.min, Some(Value::Floats(vec![0.0])));
    let color = def.input("base_color").unwrap();
    assert_eq!((color.ty, color.value.clone()), (Type::Color3, Some(Value::Floats(vec![0.8, 0.8, 0.8]))));
    assert_eq!(def.input("specular_roughness").unwrap().value, Some(Value::Floats(vec![0.2])));
    // In 1.0.0's order: base, base_color, then the rest.
    assert_eq!(def.inputs[0].name, "base");
    assert_eq!(def.inputs[1].name, "base_color");
    // 1.0.0 is there, but not current.
    let old = lib.def("ND_standard_surface_surfaceshader_100").unwrap();
    assert!(!lib.is_current(old));
    assert_eq!(old.input("base").unwrap().value, Some(Value::Floats(vec![0.8])));
    assert_eq!(lib.variants("standard_surface").count(), 1);
}

#[test]
fn the_surface_shaders_and_material_are_there() {
    let lib = lib();
    for (node, def) in [
        ("open_pbr_surface", "ND_open_pbr_surface_surfaceshader"),
        ("UsdPreviewSurface", "ND_UsdPreviewSurface_surfaceshader"),
        ("gltf_pbr", "ND_gltf_pbr_surfaceshader"),
        ("surfacematerial", "ND_surfacematerial"),
    ] {
        assert_eq!(lib.default_def(node).map(|d| d.name.as_str()), Some(def), "{node}");
    }
    let usd = lib.default_def("UsdPreviewSurface").unwrap();
    assert_eq!(usd.input("diffuseColor").unwrap().value, Some(Value::Floats(vec![0.18, 0.18, 0.18])));
    let material = lib.default_def("surfacematerial").unwrap();
    assert_eq!(material.out_type(), Type::Material);
    assert_eq!(material.input("surfaceshader").map(|i| i.ty), Some(Type::SurfaceShader));
    // UsdUVTexture 2.3 is current; 2.2 inherits it.
    let tex = lib.default_def("UsdUVTexture").unwrap();
    assert_eq!(tex.version.as_deref(), Some("2.3"));
    assert_eq!(lib.def("ND_UsdUVTexture").unwrap().inputs.len(), tex.inputs.len());
}

#[test]
fn image_has_its_ui() {
    let def = lib().def("ND_image_color3").unwrap();
    assert_eq!(def.group, "texture2d");
    let file = def.input("file").unwrap();
    assert_eq!((file.ty, file.uniform, file.label()), (Type::Filename, true, "Filename"));
    let texcoord = def.input("texcoord").unwrap();
    assert_eq!(texcoord.default_geomprop.as_deref(), Some("UV0"));
    assert!(texcoord.value.is_none());
    let address = def.input("uaddressmode").unwrap();
    assert_eq!(address.choices, ["constant", "clamp", "periodic", "mirror"]);
    assert_eq!(address.value, Some(Value::String("periodic".into())));
}

#[test]
fn a_node_takes_the_types_connected_to_it() {
    let lib = lib();
    // Nothing connected: the first definition.
    assert_eq!(lib.resolve("add", &[], None).unwrap().name, "ND_add_float");
    // A colour into in1: colour in, colour out.
    assert_eq!(lib.resolve("add", &[("in1", Type::Color3)], None).unwrap().name, "ND_add_color3");
    // A colour plus a float: the mixed definition.
    let mixed = lib.resolve("add", &[("in1", Type::Color3), ("in2", Type::Float)], None).unwrap();
    assert_eq!(mixed.name, "ND_add_color3FA");
    assert_eq!(mixed.out_type(), Type::Color3);
    // Asked for by what it feeds: a vector3 out.
    assert_eq!(lib.resolve("add", &[], Some(Type::Vector3)).unwrap().name, "ND_add_vector3");
    // Types the node does not join.
    assert!(lib.resolve("add", &[("in1", Type::Color3), ("in2", Type::Vector2)], None).is_none());
    assert!(lib.resolve("add", &[("nope", Type::Float)], None).is_none());
    // Convert is chosen by both ends.
    assert_eq!(lib.resolve("convert", &[("in", Type::Float)], Some(Type::Color3)).unwrap().name, "ND_convert_float_color3");
    // An image read as a colour, or as a float.
    assert_eq!(lib.resolve("image", &[], Some(Type::Float)).unwrap().name, "ND_image_float");
    // A surface shader into a material.
    assert_eq!(lib.resolve("surfacematerial", &[("surfaceshader", Type::SurfaceShader)], None).unwrap().name,
               "ND_surfacematerial");
}

#[test]
fn nodes_are_grouped() {
    let lib = lib();
    assert_eq!(lib.group("add"), Some("math"));
    assert_eq!(lib.group("image"), Some("texture2d"));
    assert_eq!(lib.group("standard_surface"), Some("pbr"));
    assert_eq!(lib.group("noise3d"), Some("procedural3d"));
    assert_eq!(lib.group("no_such_node"), None);
}

#[test]
fn a_broken_document_names_its_file() {
    let err = Library::read(&[("bad.mtlx", "<materialx><nodedef name=\"ND_x\" node=\"x\"><input name=\"a\" type=\"nope\"/></nodedef></materialx>")]).unwrap_err();
    assert_eq!(err.file, "bad.mtlx");
    assert!(err.what.contains("unknown type nope"), "{}", err.what);
    let err = Library::read(&[("b.mtlx", "<materialx><nodedef name=\"ND_y\" node=\"y\" inherit=\"ND_z\"/></materialx>")]).unwrap_err();
    assert!(err.what.contains("not defined"), "{}", err.what);
}
