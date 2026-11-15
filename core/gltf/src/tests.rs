// SPDX-License-Identifier: MIT
//! The reader on files of the test writer (`testfile`), cut short and
//! damaged, and on the committed public-domain sample.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::testfile::{self, Form, TestMaterial, TestMesh, TestNode, TestPrimitive, TestScene};
use crate::{Gltf, read};

fn one_box(primitive: TestPrimitive) -> TestScene {
    TestScene {
        nodes: vec![TestNode::named("Box", Some(0), &[])],
        roots: vec![0],
        meshes: vec![TestMesh {
            name: Some("box".into()),
            primitives: vec![primitive],
        }],
        materials: vec![TestMaterial {
            name: Some("Blue".into()),
            base_color: [0.1, 0.2, 0.9, 1.0],
            ..TestMaterial::default()
        }],
        ..TestScene::default()
    }
}

fn words(gltf: &Gltf) -> Vec<String> {
    gltf.notes.iter().map(|n| n.words.clone()).collect()
}

/// A folder of the test's own (as the other crates' unit tests make
/// theirs).
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mitcad-gltf-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_binary_file_gives_its_nodes_meshes_and_materials() {
    let scene = testfile::assembly();
    let (bytes, files) = testfile::write(&scene, &Form::Glb);
    assert!(files.is_empty());
    assert!(crate::is_glb(&bytes));
    let gltf = read(&bytes, None).unwrap();
    assert_eq!(gltf.generator.as_deref(), Some("mitcad-gltf test writer"));
    assert_eq!(gltf.nodes.len(), 8);
    assert_eq!(gltf.roots, vec![0]);
    assert_eq!(gltf.nodes[0].children, vec![1, 2, 4, 5]);
    assert_eq!(gltf.node_name(1), "Bracket");
    // Translations in metres, as the file keeps them.
    assert!((gltf.nodes[2].matrix[0][3] - 0.3).abs() < 1e-12);
    assert!((gltf.nodes[5].matrix[1][1] - 2.0).abs() < 1e-12);
    assert_eq!(gltf.meshes.len(), 2);
    let plate = &gltf.meshes[0].primitives[0];
    assert_eq!(plate.positions.len(), 8);
    assert_eq!(plate.triangles.len(), 12);
    assert_eq!(plate.material, Some(0));
    assert!((plate.positions[6][0] - 0.02).abs() < 1e-7);
    assert_eq!(gltf.materials[0].name.as_deref(), Some("Red"));
    assert!((gltf.materials[0].base_color[0] - 0.8).abs() < 1e-12);
    assert!((gltf.materials[0].roughness - 0.4).abs() < 1e-12);
    // The label's image, a PNG in the binary chunk.
    assert_eq!(gltf.materials[1].base_color_image, Some(0));
    assert_eq!(gltf.images[0].format, Some("png"));
    assert_eq!(
        gltf.images[0].bytes.as_deref(),
        Some(testfile::png().as_slice())
    );
    assert_eq!(gltf.images[0].file_name, "image1.png");
    // The textured box has no texture coordinates, and the reader says so.
    assert_eq!(gltf.notes.len(), 1, "{:?}", gltf.notes);
    assert_eq!(
        words(&gltf)[0],
        "a textured mesh has no texture coordinates: its texture is projected as MitCad \
         projects textures"
    );
    assert!(gltf.meshes[1].primitives[0].uvs.is_none());
    assert!(plate.uvs.is_none());
}

/// A square of two triangles with texture coordinates (glTF's: v down
/// the image), its material textured by image 0.
fn textured_square(material: TestMaterial) -> TestScene {
    let mut scene = one_box(TestPrimitive {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        indices: Some(vec![0, 1, 2, 0, 2, 3]),
        material: Some(0),
        uvs: Some(vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]),
        ..TestPrimitive::default()
    });
    scene.materials = vec![TestMaterial {
        image: Some(0),
        ..material
    }];
    scene.images = vec![testfile::png()];
    scene
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
}

#[test]
fn texture_coordinates_are_read_with_v_up_the_image() {
    let scene = textured_square(TestMaterial::default());
    let gltf = read(&testfile::write(&scene, &Form::Glb).0, None).unwrap();
    assert!(gltf.notes.is_empty(), "{:?}", gltf.notes);
    let uvs = gltf.meshes[0].primitives[0].uvs.as_ref().expect("the uvs");
    // glTF's (0, 1), the image's bottom left corner, is MitCad's (0, 0).
    for (uv, expected) in uvs
        .iter()
        .zip([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
    {
        assert!(near(*uv, expected), "{uvs:?}");
    }
    let summary = gltf.summary();
    assert_eq!(summary["nodes"][0]["texture_coordinates"], true);
    assert_eq!(
        summary["nodes"][0]["uv_bounds"],
        json!([[0.0, 0.0], [1.0, 1.0]])
    );
    assert_eq!(summary["notes"], json!([]));

    // A texture transform (scaled twice, moved by a quarter in u, turned
    // a quarter turn) moves them as glTF says: offset + rotation x scale.
    let mut file = textured_square(TestMaterial {
        texture_transform: Some(json!({
            "offset": [0.25, 0.0], "scale": [2.0, 2.0], "rotation": std::f64::consts::FRAC_PI_2
        })),
        ..TestMaterial::default()
    });
    file.extra
        .insert("extensionsUsed".into(), json!(["KHR_texture_transform"]));
    let gltf = read(&testfile::write(&file, &Form::Glb).0, None).unwrap();
    assert!(gltf.notes.is_empty(), "{:?}", gltf.notes);
    let uvs = gltf.meshes[0].primitives[0].uvs.as_ref().unwrap();
    // glTF's (1, 1): u' = 0.25 + 2 v = 2.25, v' = -2 u = -2; MitCad's v is
    // 1 - v' = 3.
    assert!(near(uvs[1], [2.25, 3.0]), "{uvs:?}");
    // glTF's (0, 1): u' = 2.25, v' = 0, v = 1.
    assert!(near(uvs[0], [2.25, 1.0]), "{uvs:?}");
}

#[test]
fn texture_coordinates_that_do_not_fit_are_left_out_in_words() {
    let mut scene = textured_square(TestMaterial::default());
    scene.meshes[0].primitives[0]
        .uvs
        .as_mut()
        .unwrap()
        .truncate(3);
    let gltf = read(&testfile::write(&scene, &Form::Glb).0, None).unwrap();
    // The triangles are read; their texture is projected.
    let p = &gltf.meshes[0].primitives[0];
    assert_eq!(p.triangles.len(), 2);
    assert!(p.uvs.is_none());
    assert_eq!(
        words(&gltf),
        vec![
            "texture coordinates that cannot be read are left out: their texture is projected \
             as MitCad projects textures"
        ]
    );
    assert!(gltf.notes[0].detail.contains("3 texture coordinates for 4"));
    // A mesh without a textured material reads none and says nothing.
    let mut plain = textured_square(TestMaterial::default());
    plain.materials[0].image = None;
    let gltf = read(&testfile::write(&plain, &Form::Glb).0, None).unwrap();
    assert!(gltf.meshes[0].primitives[0].uvs.is_none());
    assert!(gltf.notes.is_empty(), "{:?}", gltf.notes);
}

#[test]
fn images_of_metalness_roughness_and_occlusion_are_read() {
    let mut scene = textured_square(TestMaterial {
        metallic_roughness: Some(1),
        occlusion: Some(1),
        ..TestMaterial::default()
    });
    scene.images.push(testfile::png());
    let gltf = read(&testfile::write(&scene, &Form::Glb).0, None).unwrap();
    assert!(gltf.notes.is_empty(), "{:?}", gltf.notes);
    let m = &gltf.materials[0];
    assert_eq!(m.base_color_image, Some(0));
    assert_eq!(m.metallic_roughness_image, Some(1));
    assert_eq!(m.occlusion_image, Some(1));
    assert_eq!(gltf.images[1].format, Some("png"));
    assert_eq!(gltf.images[1].file_name, "image2.png");
    let summary = gltf.summary();
    assert_eq!(
        summary["nodes"][0]["materials"][0]["metallic_roughness_texture"],
        true
    );
}

#[test]
fn the_json_form_reads_the_same_embedded_or_beside_it() {
    let scene = testfile::assembly();
    let glb = read(&testfile::write(&scene, &Form::Glb).0, None).unwrap();
    let (text, files) = testfile::write(&scene, &Form::Embedded);
    assert!(files.is_empty());
    let embedded = read(&text, None).unwrap();
    assert_eq!(embedded.nodes, glb.nodes);
    assert_eq!(embedded.meshes, glb.meshes);
    assert_eq!(embedded.materials, glb.materials);
    assert_eq!(embedded.images[0].bytes, glb.images[0].bytes);

    let dir = scratch("external");
    let (text, files) = testfile::write(&scene, &Form::External("assembly data.bin".into()));
    assert_eq!(files.len(), 2);
    for (name, bytes) in &files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    let path = dir.join("assembly.gltf");
    std::fs::write(&path, &text).unwrap();
    let beside = crate::read_file(&path).unwrap();
    assert_eq!(beside.meshes, glb.meshes);
    assert_eq!(beside.images[0].bytes, glb.images[0].bytes);
    // Without the folder the buffer beside it is not read: the meshes are
    // left out, with a note.
    let alone = read(&text, None).unwrap();
    assert!(alone.meshes.iter().all(|m| m.primitives.is_empty()));
    assert!(
        words(&alone)
            .iter()
            .any(|w| w == "a mesh whose data cannot be read is left out"),
        "{:?}",
        alone.notes
    );
}

#[test]
fn a_percent_encoded_buffer_name_is_found() {
    let scene = one_box(testfile::box_primitive([1.0, 1.0, 1.0], Some(0)));
    let dir = scratch("percent");
    let (text, files) = testfile::write(&scene, &Form::External("box data.bin".into()));
    std::fs::write(dir.join(&files[0].0), &files[0].1).unwrap();
    let text = String::from_utf8(text)
        .unwrap()
        .replace("box data.bin", "box%20data.bin");
    let gltf = read(text.as_bytes(), Some(dir.as_path())).unwrap();
    assert_eq!(gltf.meshes[0].primitives[0].triangles.len(), 12);
}

#[test]
fn files_outside_the_folder_are_not_read() {
    for uri in [
        "../box.bin",
        "/etc/box.bin",
        "https://example.invalid/box.bin",
    ] {
        let scene = one_box(testfile::box_primitive([1.0, 1.0, 1.0], Some(0)));
        let (text, _) = testfile::write(&scene, &Form::External("box.bin".into()));
        let text = String::from_utf8(text)
            .unwrap()
            .replace("\"box.bin\"", &format!("\"{uri}\""));
        let gltf = read(text.as_bytes(), Some(Path::new("."))).unwrap();
        assert!(gltf.meshes[0].primitives.is_empty(), "{uri}");
        let detail = &gltf.notes[0].detail;
        assert!(detail.contains("not read"), "{uri}: {detail}");
    }
}

#[test]
fn placements_come_in_millimetres_with_z_up() {
    let scene = testfile::assembly();
    let gltf = read(&testfile::write(&scene, &Form::Glb).0, None).unwrap();
    let placed = gltf.placed_nodes();
    assert_eq!(placed.len(), 8);
    // The root turns Y up into Z up: its matrix is the turn.
    assert_eq!(placed[0].node, 0);
    assert_eq!(placed[0].local, crate::Y_UP_TO_Z_UP);
    // The second bracket: 300 mm along x in the root.
    let second = placed.iter().find(|p| p.node == 2).unwrap();
    assert!((second.local[0][3] - 300.0).abs() < 1e-9);
    // The label: 50 mm along glTF's z is -50 mm along MitCad's y.
    let label = placed.iter().find(|p| p.node == 4).unwrap();
    assert!((label.world[1][3] + 50.0).abs() < 1e-9, "{:?}", label.world);
    assert!(label.world[2][3].abs() < 1e-9);

    let summary = gltf.summary();
    let nodes = summary["nodes"].as_array().unwrap();
    assert_eq!(nodes[1]["path"], "Assembly/Bracket");
    assert_eq!(nodes[2]["path"], "Assembly/Bracket/Plate");
    assert_eq!(nodes[2]["triangles"], 12);
    // sRGB of the linear 0.8.
    let red = nodes[2]["materials"][0]["base_color"][0].as_f64().unwrap();
    assert!((red - crate::srgb(0.8)).abs() < 1e-12 && red > 0.9, "{red}");
    assert_eq!(summary["triangles"], 24);
    assert_eq!(summary["images"], 1);
}

#[test]
fn strips_fans_short_indices_and_strides_are_read() {
    let square = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    let strip = TestPrimitive {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ],
        mode: Some(5),
        ..TestPrimitive::default()
    };
    let fan = TestPrimitive {
        positions: square.clone(),
        mode: Some(6),
        short_indices: true,
        indices: Some(vec![0, 1, 2, 3]),
        padding: 8,
        ..TestPrimitive::default()
    };
    let gltf = read(&testfile::write(&one_box(strip), &Form::Glb).0, None).unwrap();
    assert_eq!(
        gltf.meshes[0].primitives[0].triangles,
        vec![[0, 1, 2], [2, 1, 3]]
    );
    let gltf = read(&testfile::write(&one_box(fan), &Form::Glb).0, None).unwrap();
    let p = &gltf.meshes[0].primitives[0];
    assert_eq!(p.triangles, vec![[0, 1, 2], [0, 2, 3]]);
    assert_eq!(p.positions[2], [1.0, 1.0, 0.0]);
}

#[test]
fn what_is_not_read_is_said_in_words() {
    let mut scene = one_box(TestPrimitive {
        target: true,
        ..testfile::box_primitive([1.0, 1.0, 1.0], Some(0))
    });
    scene.meshes[0].primitives.push(TestPrimitive {
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
        mode: Some(1),
        ..TestPrimitive::default()
    });
    scene.extra.insert(
        "animations".into(),
        json!([{"channels": [], "samplers": []}]),
    );
    scene.extra.insert(
        "extensionsUsed".into(),
        json!(["KHR_lights_punctual", "KHR_materials_ior", "VENDOR_thing"]),
    );
    scene
        .extra
        .insert("extensionsRequired".into(), json!(["VENDOR_thing"]));
    let gltf = read(&testfile::write(&scene, &Form::Glb).0, None).unwrap();
    assert_eq!(gltf.meshes[0].primitives.len(), 1);
    let words = words(&gltf);
    for expected in [
        "morph targets are not read: the meshes come in in their base shape",
        "points and lines are left out: MitCad imports triangles",
        "animations are not read: the bodies come in as the file places them",
        "the file's extension KHR_lights_punctual is not read",
        "the file's extension VENDOR_thing is not read, although the file needs it",
    ] {
        assert!(words.iter().any(|w| w == expected), "{expected}: {words:?}");
    }
    assert!(!words.iter().any(|w| w.contains("KHR_materials_ior")));
    // Words only: no indexes in what a person reads.
    assert!(words.iter().all(|w| !w.contains("mesh 0")), "{words:?}");
}

#[test]
fn damaged_files_are_refused_or_their_meshes_left_out() {
    let scene = testfile::assembly();
    let (bytes, _) = testfile::write(&scene, &Form::Glb);
    // Cut anywhere: an error or fewer meshes, never a panic.
    for cut in (0..bytes.len()).step_by(37) {
        let _ = read(&bytes[..cut], None);
    }
    let error = read(&bytes[..10], None).unwrap_err();
    assert_eq!(error.words, "the glTF binary file is damaged");
    let mut version = bytes.clone();
    version[4] = 1;
    assert!(
        read(&version, None)
            .unwrap_err()
            .words
            .contains("version 1")
    );
    assert!(
        read(b"{\"asset\": {\"version\": \"1.0\"}}", None)
            .unwrap_err()
            .words
            .contains("glTF 1.0")
    );
    assert!(read(b"not json", None).is_err());

    // An index past the positions leaves that mesh out.
    let mut bad = one_box(testfile::box_primitive([1.0, 1.0, 1.0], Some(0)));
    bad.meshes[0].primitives[0].indices.as_mut().unwrap()[5] = 99;
    let gltf = read(&testfile::write(&bad, &Form::Glb).0, None).unwrap();
    assert!(gltf.meshes[0].primitives.is_empty());
    assert_eq!(
        words(&gltf),
        vec!["a mesh whose data cannot be read is left out"]
    );
    assert!(gltf.notes[0].detail.contains("index 99"));
}

#[test]
fn a_node_tree_with_a_cycle_is_walked_once() {
    let mut scene = one_box(testfile::box_primitive([1.0, 1.0, 1.0], Some(0)));
    scene.nodes = vec![
        TestNode::named("A", None, &[1]),
        TestNode::named("B", Some(0), &[0]),
    ];
    let gltf = read(&testfile::write(&scene, &Form::Glb).0, None).unwrap();
    assert_eq!(gltf.placed_nodes().len(), 2);
}

#[test]
fn the_public_domain_sample_is_read() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/two-blocks.gltf");
    let gltf = crate::read_file(&path).unwrap();
    let summary = gltf.summary();
    let names: Vec<&str> = summary["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["path"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Stand", "Stand/Base", "Stand/Post", "Marker"]);
    assert_eq!(summary["triangles"], 24);
    // The post 40 mm along x, 10 mm up (glTF's y) and 40 mm toward the
    // viewer (glTF's z, MitCad's -y) in the design; in the stand, whose
    // placement turns glTF's axes into MitCad's, it is where the file has
    // it.
    let local = &summary["nodes"][2]["transform"];
    assert!(
        (local[2][3].as_f64().unwrap() - 40.0).abs() < 1e-6,
        "{local}"
    );
    let post = &summary["nodes"][2]["world"];
    for (row, expected) in [40.0, -40.0, 10.0].iter().enumerate() {
        let value = post[row][3].as_f64().unwrap();
        assert!((value - expected).abs() < 1e-6, "{post}");
    }
    let orange = &summary["nodes"][2]["materials"][0];
    assert_eq!(orange["name"], "Orange");
    assert!(gltf.notes.is_empty(), "{:?}", gltf.notes);
}
