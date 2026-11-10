// SPDX-License-Identifier: MIT
use super::*;
use crate::compress::{
    compress_ints, decompress_ints, fast_compress, fast_decompress, lz4_compress, lz4_decompress,
};
use crate::sdf::{Path, SpecType, Value, Variability};

/// A unit cube's six faces, each with its own four points and normals.
fn cube(name: &str, material: usize) -> Mesh {
    let mut points = Vec::new();
    let mut normals = Vec::new();
    let mut triangles = Vec::new();
    for axis in 0..3 {
        for side in [0.0, 1.0] {
            let first = points.len() as u32;
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            for (a, b) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                let mut p = [0.0; 3];
                p[axis] = side;
                p[u] = a;
                p[v] = b;
                points.push(p);
                let mut n = [0.0; 3];
                n[axis] = if side > 0.0 { 1.0 } else { -1.0 };
                normals.push(n);
            }
            if side > 0.0 {
                triangles.extend([[first, first + 1, first + 2], [first, first + 2, first + 3]]);
            } else {
                triangles.extend([[first, first + 2, first + 1], [first, first + 3, first + 2]]);
            }
        }
    }
    Mesh {
        name: name.to_owned(),
        points,
        normals,
        triangles,
        tex_coords: None,
        double_sided: false,
        material,
        subsets: Vec::new(),
        samples: Vec::new(),
    }
}

/// A 2 x 2 RGB PNG image (a checker of brown and beige), its pixels in a
/// stored deflate block.
fn png() -> Vec<u8> {
    let chunk = |out: &mut Vec<u8>, kind: &[u8], data: &[u8]| {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend(&body);
        out.extend(mitcad_zip::crc32(&body).to_be_bytes());
    };
    let mut header = Vec::new();
    header.extend(2u32.to_be_bytes());
    header.extend(2u32.to_be_bytes());
    header.extend([8, 2, 0, 0, 0]); // 8 bits, RGB, no interlace
    let (a, b) = ([120u8, 80, 40], [230u8, 210, 170]);
    let raw: Vec<u8> = [[0].as_slice(), &a, &b, &[0], &b, &a].concat();
    let adler = {
        let (mut s1, mut s2) = (1u32, 0u32);
        for &byte in &raw {
            s1 = (s1 + u32::from(byte)) % 65521;
            s2 = (s2 + s1) % 65521;
        }
        (s2 << 16) | s1
    };
    let mut zlib = vec![0x78, 0x01, 1];
    zlib.extend((raw.len() as u16).to_le_bytes());
    zlib.extend((!(raw.len() as u16)).to_le_bytes());
    zlib.extend(&raw);
    zlib.extend(adler.to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn scene() -> Scene {
    let mut textured = cube("Pin (2)", 1);
    textured.tex_coords = Some(TexCoords::FaceVarying(vec![[0.5, 0.25]; 36]));
    let mut painted = cube("Body1", 0);
    painted.subsets.push(Subset {
        name: "Faces paint_red".to_owned(),
        triangles: vec![10, 11],
        material: 2,
    });
    let mut arm = Node::new("Arm:1");
    arm.placement = Placement {
        linear: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [10.0, 20.0, 30.5],
    };
    let mut pin = Node::new("Pin:1");
    pin.placement.translation = [0.0, 0.0, 5.0];
    pin.meshes.push(textured);
    arm.children.push(pin);
    arm.meshes.push(cube("Body1", 0));
    let mut root = Node::new("bracket assembly");
    root.meshes.push(painted);
    root.children.push(arm);
    Scene {
        root,
        materials: vec![
            Material {
                name: "default".to_owned(),
                ..Material::default()
            },
            Material {
                name: "wood_oak".to_owned(),
                texture: Some(0),
                ..Material::default()
            },
            Material {
                name: "paint_red".to_owned(),
                diffuse_color: [0.58, 0.02, 0.02],
                metallic: 0.0,
                roughness: 0.3,
                clearcoat: 0.5,
                ..Material::default()
            },
        ],
        images: vec![Image {
            path: "textures/wood_oak.png".to_owned(),
            data: png(),
        }],
        documentation: "Mitcad test".to_owned(),
        animation: None,
    }
}

/// A turn of `degrees` about Z and a move.
fn turned(degrees: f64, translation: [f64; 3]) -> Placement {
    let (s, c) = degrees.to_radians().sin_cos();
    Placement {
        linear: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
        translation,
    }
}

/// The test scene animated (mitcad#1597) at 24 frames per second over the
/// time codes 0, 1, 2: the arm turns 0, 45 and 90 degrees about Z at its
/// place, the pin slides 0, 5 and 10 mm up in the arm, and the root's
/// painted body moves 0, 10 and 20 mm along X in its node.
fn animated_scene() -> Scene {
    let mut scene = scene();
    scene.animation = Some(Animation {
        fps: 24.0,
        times: vec![0.0, 1.0, 2.0],
    });
    let arm = &mut scene.root.children[0];
    let at = arm.placement.translation;
    arm.samples = [0.0, 45.0, 90.0].iter().map(|d| turned(*d, at)).collect();
    let pin = &mut arm.children[0];
    pin.samples = [0.0, 5.0, 10.0]
        .iter()
        .map(|z| turned(0.0, [0.0, 0.0, 5.0 + z]))
        .collect();
    scene.root.meshes[0].samples = [0.0, 10.0, 20.0]
        .iter()
        .map(|x| turned(0.0, [*x, 0.0, 0.0]))
        .collect();
    scene
}

#[test]
fn writes_time_samples_that_read_back() {
    let animated = animated_scene();
    let data = write(&animated).unwrap();
    let layer = read_package(&data).unwrap().layer;
    // The time: from 0 to 2 at 24 frames per second.
    let root = layer.spec("/").unwrap();
    assert_eq!(root.field("startTimeCode"), Some(&Value::Double(0.0)));
    assert_eq!(root.field("endTimeCode"), Some(&Value::Double(2.0)));
    assert_eq!(root.field("timeCodesPerSecond"), Some(&Value::Double(24.0)));
    assert_eq!(root.field("framesPerSecond"), Some(&Value::Double(24.0)));
    let samples = |path: &str| match layer.field(path, "timeSamples") {
        Some(Value::TimeSamples(samples)) => samples.clone(),
        other => panic!("{path}: {other:?}"),
    };
    let matrix = |value: &Value| match value {
        Value::Matrix4d(m) => *m,
        other => panic!("{other:?}"),
    };
    // The arm: first, middle and last frames, USD's row-vector matrices.
    let arm = samples("/bracket_assembly/Arm_1.xformOp:transform");
    assert_eq!(
        arm.iter().map(|(t, _)| *t).collect::<Vec<_>>(),
        [0.0, 1.0, 2.0]
    );
    for ((_, value), degrees) in arm.iter().zip([0.0_f64, 45.0, 90.0]) {
        let m = matrix(value);
        let (s, c) = degrees.to_radians().sin_cos();
        assert!((m[0][0] - c).abs() < 1e-12 && (m[0][1] - s).abs() < 1e-12);
        assert!((m[1][0] + s).abs() < 1e-12 && (m[1][1] - c).abs() < 1e-12);
        assert_eq!(m[3], [10.0, 20.0, 30.5, 1.0]);
    }
    // The static placement stays the default.
    assert_eq!(
        layer.field("/bracket_assembly/Arm_1.xformOp:transform", "default"),
        Some(&Value::Matrix4d(
            animated.root.children[0].placement.to_usd()
        ))
    );
    let pin = samples("/bracket_assembly/Arm_1/Pin_1.xformOp:transform");
    let z: Vec<f64> = pin.iter().map(|(_, v)| matrix(v)[3][2]).collect();
    assert_eq!(z, [5.0, 10.0, 15.0]);
    // The moved mesh: its own transform, identity by default, then its
    // samples; its points stay as they are.
    let body = "/bracket_assembly/Body1";
    assert_eq!(
        layer.field(&format!("{body}.xformOpOrder"), "default"),
        Some(&Value::TokenArray(vec!["xformOp:transform".to_owned()]))
    );
    assert_eq!(
        layer.field(&format!("{body}.xformOp:transform"), "default"),
        Some(&Value::Matrix4d(Placement::IDENTITY.to_usd()))
    );
    let x: Vec<f64> = samples(&format!("{body}.xformOp:transform"))
        .iter()
        .map(|(_, v)| matrix(v)[3][0])
        .collect();
    assert_eq!(x, [0.0, 10.0, 20.0]);
    let Some(Value::Vec3fArray(points)) = layer.field(&format!("{body}.points"), "default") else {
        panic!("no points");
    };
    assert_eq!(points.len(), 24);
    // A mesh and a node that do not move have no samples.
    assert!(
        layer
            .field("/bracket_assembly/Arm_1/Body1.xformOp:transform", "default")
            .is_none()
    );
    assert!(
        layer
            .field(
                "/bracket_assembly/Arm_1/Pin_1/Pin_2.xformOpOrder",
                "default"
            )
            .is_none()
    );
    // Each mesh is written once: as many meshes as in the still scene.
    let still = read_package(&write(&scene()).unwrap()).unwrap().layer;
    let meshes = |layer: &Layer| {
        layer
            .specs
            .iter()
            .filter(|s| s.field("typeName") == Some(&Value::Token("Mesh".to_owned())))
            .count()
    };
    assert_eq!(meshes(&layer), meshes(&still));
    // A still scene declares no time.
    assert!(still.spec("/").unwrap().field("startTimeCode").is_none());
    assert!(
        still
            .field("/bracket_assembly/Arm_1.xformOp:transform", "timeSamples")
            .is_none()
    );
}

#[test]
fn checks_the_animation() {
    let error = |scene: Scene| write(&scene).unwrap_err().0;
    let mut bad = animated_scene();
    bad.animation.as_mut().unwrap().fps = 0.0;
    assert!(error(bad).contains("positive number of frames per second"));
    let mut bad = animated_scene();
    bad.animation.as_mut().unwrap().fps = f64::NAN;
    assert!(error(bad).contains("positive number of frames per second"));
    let mut bad = animated_scene();
    bad.animation.as_mut().unwrap().times = vec![0.0, 2.0, 1.0];
    assert!(error(bad).contains("must increase"));
    let mut bad = animated_scene();
    bad.animation.as_mut().unwrap().times = vec![0.0, f64::INFINITY, 3.0];
    assert!(error(bad).contains("finite"));
    let mut bad = animated_scene();
    bad.root.children[0].samples.pop();
    assert!(error(bad).contains("Arm:1 has 2 samples for 3 time codes"));
    let mut bad = animated_scene();
    bad.root.children[0].samples[1].translation[0] = f64::NAN;
    assert!(error(bad).contains("Arm:1: the placement at time code 1 is not finite"));
    let mut bad = scene();
    bad.root.children[0].samples = vec![Placement::IDENTITY];
    assert!(error(bad).contains("the scene has no animation"));
    let mut bad = animated_scene();
    bad.root.samples = vec![Placement::IDENTITY; 3];
    assert!(error(bad).contains("the root does not move"));
}

/// `MITCAD_USDZ_ANIMATED_SAMPLE=<file>` writes the animated test scene
/// there, for OpenUSD's own reading of the time samples
/// (`tools/usdz-check/check-animation.py` with
/// `tools/usdz-check/animated-sample.json`; `docs/development.md`, *USDZ
/// files*).
#[test]
fn animated_sample_for_validators() {
    if let Ok(path) = std::env::var("MITCAD_USDZ_ANIMATED_SAMPLE") {
        std::fs::write(path, write(&animated_scene()).unwrap()).unwrap();
    }
}

#[test]
fn lz4_round_trips() {
    let mut inputs: Vec<Vec<u8>> = vec![
        Vec::new(),
        b"short".to_vec(),
        b"abcabcabcabcabcabcabcabcabcabcabc".to_vec(),
        vec![7; 100_000],
    ];
    // Bytes without repeats, and a mix of both with long literals.
    let mut state = 12345u32;
    let noise: Vec<u8> = (0..70_000)
        .map(|_| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (state >> 16) as u8
        })
        .collect();
    inputs.push(noise.clone());
    inputs.push([noise[..300].to_vec(), vec![1; 5000], noise[..300].to_vec()].concat());
    for input in inputs {
        let mut block = Vec::new();
        lz4_compress(&input, &mut block);
        assert_eq!(lz4_decompress(&block, input.len()).unwrap(), input);
        let framed = fast_compress(&input);
        assert_eq!(framed[0], 0, "one chunk");
        assert_eq!(fast_decompress(&framed, input.len()).unwrap(), input);
        if input.len() > 1000 && input.iter().all(|&b| b == input[0]) {
            assert!(block.len() < input.len() / 100, "repeats compress");
        }
    }
    // A bad offset is an error, not a panic.
    assert!(lz4_decompress(&[0x10, b'a', 9, 0], 100).is_err());
}

#[test]
fn integer_coding_round_trips() {
    let lists: Vec<Vec<i32>> = vec![
        Vec::new(),
        vec![5],
        (0..1000).collect(),
        vec![
            0,
            1,
            -1,
            300,
            -300,
            70_000,
            -70_000,
            i32::MAX,
            i32::MIN,
            3,
            3,
            3,
        ],
        (0..999).map(|i| (i * 7919) % 1000 - 500).collect(),
    ];
    for list in lists {
        let compressed = compress_ints(&list);
        assert_eq!(decompress_ints(&compressed, list.len()).unwrap(), list);
    }
    // Consecutive integers: one common difference, two bits each.
    let run: Vec<i32> = (0..4000).collect();
    assert!(compress_ints(&run).len() < 100);
}

#[test]
fn identifiers_are_valid_and_unique() {
    assert_eq!(stage::identifier("Pin (2)"), "Pin_2");
    assert_eq!(stage::identifier("Arm:1"), "Arm_1");
    assert_eq!(stage::identifier("a  b"), "a_b");
    assert_eq!(stage::identifier("_"), "_");
    assert_eq!(stage::identifier("x_"), "x_");
    assert_eq!(stage::identifier("2nd"), "_2nd");
    assert_eq!(stage::identifier(""), "_");
    assert_eq!(stage::identifier("Bracket_1"), "Bracket_1");
    assert_eq!(stage::identifier("Ab\u{e4}"), "Ab");
}

#[test]
fn writes_a_package_that_reads_back() {
    let scene = scene();
    let data = write(&scene).unwrap();
    let package = read_package(&data).unwrap();
    // The layer first, then the image; both 64-byte aligned (checked by
    // the reader) and stored.
    let names: Vec<&str> = package.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["bracket_assembly.usdc", "textures/wood_oak.png"]);
    assert!(package.files.iter().all(|f| f.offset % ALIGNMENT == 0));
    assert_eq!(package.files[1].data, png());
    let layer = &package.layer;
    assert_eq!(layer.version, [0, 8, 0]);
    // Stage metadata.
    let root = layer.spec("/").unwrap();
    assert_eq!(root.spec_type, SpecType::PseudoRoot);
    assert_eq!(
        root.field("defaultPrim"),
        Some(&Value::Token("bracket_assembly".to_owned()))
    );
    assert_eq!(root.field("metersPerUnit"), Some(&Value::Double(0.001)));
    assert_eq!(root.field("upAxis"), Some(&Value::Token("Y".to_owned())));
    assert_eq!(
        root.field("documentation"),
        Some(&Value::String("Mitcad test".to_owned()))
    );
    // The hierarchy: root meshes, the occurrences as Xforms.
    let type_of = |path: &str| match layer.field(path, "typeName") {
        Some(Value::Token(t)) => t.clone(),
        other => panic!("{path}: {other:?}"),
    };
    assert_eq!(type_of("/bracket_assembly"), "Xform");
    assert_eq!(type_of("/bracket_assembly/Body1"), "Mesh");
    assert_eq!(type_of("/bracket_assembly/Arm_1"), "Xform");
    assert_eq!(type_of("/bracket_assembly/Arm_1/Body1"), "Mesh");
    assert_eq!(type_of("/bracket_assembly/Arm_1/Pin_1/Pin_2"), "Mesh");
    assert_eq!(type_of("/bracket_assembly/Materials"), "Scope");
    assert_eq!(
        type_of("/bracket_assembly/Materials/wood_oak/Texture"),
        "Shader"
    );
    assert_eq!(
        layer.field("/bracket_assembly", "primChildren"),
        Some(&Value::TokenVector(vec![
            "Materials".to_owned(),
            "Body1".to_owned(),
            "Arm_1".to_owned()
        ]))
    );
    assert_eq!(
        layer.field("/bracket_assembly.xformOp:rotateX", "default"),
        Some(&Value::Float(-90.0))
    );
    // The occurrence's placement: USD's row-vector matrix.
    let Some(Value::Matrix4d(m)) =
        layer.field("/bracket_assembly/Arm_1.xformOp:transform", "default")
    else {
        panic!("no transform");
    };
    assert_eq!(m[3], [10.0, 20.0, 30.5, 1.0]);
    assert_eq!(m[0], [0.0, 1.0, 0.0, 0.0], "X goes to Y");
    // Points and indices round trip (as 32-bit floats and ints).
    let mesh = &scene.root.meshes[0];
    let Some(Value::Vec3fArray(points)) = layer.field("/bracket_assembly/Body1.points", "default")
    else {
        panic!("no points");
    };
    assert_eq!(points.len(), mesh.points.len());
    for (a, b) in points.iter().zip(&mesh.points) {
        assert_eq!(*a, b.map(|c| c as f32));
    }
    let Some(Value::IntArray(indices)) =
        layer.field("/bracket_assembly/Body1.faceVertexIndices", "default")
    else {
        panic!("no indices");
    };
    let expected: Vec<i32> = mesh.triangles.iter().flatten().map(|&i| i as i32).collect();
    assert_eq!(*indices, expected);
    assert_eq!(
        layer.field("/bracket_assembly/Body1.faceVertexCounts", "default"),
        Some(&Value::IntArray(vec![3; 12]))
    );
    assert_eq!(
        layer.field("/bracket_assembly/Body1.normals", "interpolation"),
        Some(&Value::Token("vertex".to_owned()))
    );
    assert_eq!(
        layer.field("/bracket_assembly/Body1.extent", "default"),
        Some(&Value::Vec3fArray(vec![[0.0; 3], [1.0; 3]]))
    );
    assert_eq!(
        layer.field("/bracket_assembly/Body1.subdivisionScheme", "variability"),
        Some(&Value::Variability(Variability::Uniform))
    );
    // Materials: bindings, the subset of painted faces, the texture.
    let target = |path: &str| match layer.field(path, "targetPaths") {
        Some(Value::PathListOp(op)) => op.explicit_items[0].to_string(),
        other => panic!("{path}: {other:?}"),
    };
    assert_eq!(
        target("/bracket_assembly/Body1.material:binding"),
        "/bracket_assembly/Materials/default"
    );
    assert_eq!(
        target("/bracket_assembly/Body1/Faces_paint_red.material:binding"),
        "/bracket_assembly/Materials/paint_red"
    );
    assert_eq!(
        layer.field("/bracket_assembly/Body1/Faces_paint_red.indices", "default"),
        Some(&Value::IntArray(vec![10, 11]))
    );
    assert_eq!(
        layer.field(
            "/bracket_assembly/Body1.subsetFamily:materialBind:familyType",
            "default"
        ),
        Some(&Value::Token("nonOverlapping".to_owned()))
    );
    assert_eq!(
        target("/bracket_assembly/Arm_1/Pin_1/Pin_2.material:binding"),
        "/bracket_assembly/Materials/wood_oak"
    );
    assert_eq!(
        layer.field(
            "/bracket_assembly/Arm_1/Pin_1/Pin_2.primvars:st",
            "interpolation"
        ),
        Some(&Value::Token("faceVarying".to_owned()))
    );
    assert_eq!(
        layer.field(
            "/bracket_assembly/Materials/wood_oak/Texture.inputs:file",
            "default"
        ),
        Some(&Value::Asset("textures/wood_oak.png".to_owned()))
    );
    let connection = |path: &str| match layer.field(path, "connectionPaths") {
        Some(Value::PathListOp(op)) => op.explicit_items[0].to_string(),
        other => panic!("{path}: {other:?}"),
    };
    assert_eq!(
        connection("/bracket_assembly/Materials/wood_oak/Surface.inputs:diffuseColor"),
        "/bracket_assembly/Materials/wood_oak/Texture.outputs:rgb"
    );
    assert_eq!(
        connection("/bracket_assembly/Materials/wood_oak.outputs:surface"),
        "/bracket_assembly/Materials/wood_oak/Surface.outputs:surface"
    );
    assert_eq!(
        layer.field(
            "/bracket_assembly/Materials/paint_red/Surface.inputs:diffuseColor",
            "default"
        ),
        Some(&Value::Vec3f([0.58, 0.02, 0.02]))
    );
    // Every prim's children and properties have specs.
    for spec in layer.specs.iter().filter(|s| s.spec_type == SpecType::Prim) {
        if let Some(Value::TokenVector(children)) = spec.field("primChildren") {
            for child in children {
                assert!(layer.specs.iter().any(|s| s.path == spec.path.child(child)));
            }
        }
        if let Some(Value::TokenVector(properties)) = spec.field("properties") {
            for property in properties {
                assert!(
                    layer
                        .specs
                        .iter()
                        .any(|s| s.path == spec.path.property(property))
                );
            }
        }
    }
}

#[test]
fn checks_the_scene() {
    let mut bad = scene();
    bad.root.meshes[0].triangles[0] = [0, 1, 99];
    assert!(write(&bad).unwrap_err().0.contains("beyond its 24 points"));
    let mut bad = scene();
    bad.root.meshes[0].material = 7;
    assert!(write(&bad).unwrap_err().0.contains("material 7"));
    let mut bad = scene();
    bad.root.meshes[0].normals.pop();
    assert!(write(&bad).unwrap_err().0.contains("normals"));
    let mut bad = scene();
    bad.root.children[0].children[0].meshes[0].tex_coords = Some(TexCoords::Vertex(vec![]));
    assert!(write(&bad).unwrap_err().0.contains("texture coordinates"));
}

#[test]
fn reader_rejects_broken_packages() {
    let data = write(&scene()).unwrap();
    // A deflated package.
    let mut zip = mitcad_zip::Writer::new();
    zip.add("a.usdc", &data, mitcad_zip::Method::Deflate)
        .unwrap();
    assert!(
        read_package(&zip.finish().unwrap())
            .unwrap_err()
            .0
            .contains("compressed")
    );
    // Unaligned data.
    let unaligned = mitcad_zip::stored_zip(&[("a.usdc", &data)]);
    assert!(
        read_package(&unaligned)
            .unwrap_err()
            .0
            .contains("multiple of 64")
    );
    // A layer cut short.
    let package = read_package(&data).unwrap();
    let layer = &package.files[0].data;
    assert!(read_layer(&layer[..layer.len() - 20]).is_err());
    assert!(read_layer(b"PXR-USDA").is_err());
}

#[test]
fn paths_parse_and_print() {
    let path = Path::parse("/A/B.points").unwrap();
    assert_eq!(path.to_string(), "/A/B.points");
    assert_eq!(path.parent().unwrap().to_string(), "/A/B");
    assert_eq!(Path::root().to_string(), "/");
    assert!(Path::parse("A").is_none());
    assert!(Path::parse("/.x").is_none());
}

/// `MITCAD_USDZ_SAMPLE=<file>` writes the test scene there, for USD's own
/// validators (`docs/development.md`).
#[test]
fn sample_for_validators() {
    if let Ok(path) = std::env::var("MITCAD_USDZ_SAMPLE") {
        std::fs::write(path, write(&scene()).unwrap()).unwrap();
    }
}
