// SPDX-License-Identifier: MIT
//! The reader on files of its test writer: both forms, every kind of
//! shape, layers and colours, versions, and damaged files.

use serde_json::json;

use crate::testfile::{self, TestBlock, TestLayer, TestMesh, TestObject, TestScene, TestShape};
use crate::{NEWEST_VERSION, Other, Shape, read};

fn sample() -> TestScene {
    testfile::sample(b"box", b"face", b"part", b"part moved", b"part turned")
}

#[test]
fn the_sample_reads_in_both_forms() {
    let zipped = testfile::write(&sample());
    assert!(zipped.starts_with(b"PK"));
    let mut bare_scene = sample();
    bare_scene.version = 1;
    let bare = testfile::write(&bare_scene);
    assert!(bare.starts_with(b"{"));
    for (bytes, container, version) in [(zipped, true, 6), (bare, false, 1)] {
        let serp = read(&bytes).unwrap();
        assert_eq!(serp.container, container);
        assert_eq!(serp.version, version);
        assert_eq!(
            (serp.units.symbol, serp.units.mm, serp.units.assumed),
            ("cm", 10.0, false)
        );
        assert_eq!(serp.layers.len(), 4);
        assert_eq!(serp.objects.len(), 7);
        // B-rep data as it is, for the kernel.
        assert_eq!(serp.objects[0].shape, Shape::Brep(b"box".to_vec()));
        assert_eq!(serp.objects[0].name.as_deref(), Some("Block"));
        // The mesh in the file's units.
        let Shape::Mesh(mesh) = &serp.objects[2].shape else {
            panic!("a mesh: {:?}", serp.objects[2].shape)
        };
        assert_eq!((mesh.vertices.len(), mesh.triangles.len()), (8, 12));
        assert_eq!(mesh.vertices[7], [1.0, 1.0, 1.0]);
        // The hidden object, the text.
        assert!(!serp.shown(&serp.objects[3]));
        assert_eq!(serp.objects[4].shape, Shape::Other(Other::Text));
        // The block placed twice.
        assert_eq!(serp.blocks.len(), 1);
        assert_eq!(serp.blocks[0].name, "Bolt");
        assert_eq!(serp.blocks[0].shapes, [Shape::Brep(b"part".to_vec())]);
        assert_eq!(serp.objects[5].block, Some(0));
        assert_eq!(serp.objects[6].block, Some(0));
        assert_eq!(serp.objects[0].block, None);
        // What is left out, in words.
        assert_eq!(
            serp.skipped.parts(),
            ["1 point cloud", "1 text", "1 layout", "1 history record"]
        );
        assert!(serp.notes.is_empty(), "{:?}", serp.notes);
    }
}

#[test]
fn layers_give_paths_colours_and_visibility() {
    let mut scene = sample();
    scene.layers[1].visible = false;
    let serp = read(&testfile::write(&scene)).unwrap();
    let bolts = serp.objects[5].layer.unwrap();
    assert_eq!(serp.layer_path(bolts), "Solids::Bolts");
    // Hidden by its parent.
    assert!(!serp.layer_shown(bolts));
    assert!(!serp.shown(&serp.objects[5]));
    assert!(serp.shown(&serp.objects[1]));
    // Its own colour, else its layer's.
    assert_eq!(serp.color(&serp.objects[1]), Some([0.1, 0.2, 0.9]));
    assert_eq!(serp.color(&serp.objects[0]), Some([0.8, 0.1, 0.1]));
    assert_eq!(serp.color(&serp.objects[5]), Some([0.2, 0.7, 0.2]));
}

#[test]
fn a_parent_loop_ends() {
    let mut a = TestLayer::named("a", "A", [0.0; 3]);
    a.parent = Some("b".to_owned());
    let mut b = TestLayer::named("b", "B", [0.0; 3]);
    b.parent = Some("a".to_owned());
    let scene = TestScene {
        layers: vec![a, b],
        ..TestScene::default()
    };
    let serp = read(&testfile::write(&scene)).unwrap();
    assert!(serp.layer_path(0).contains("A"));
    assert!(serp.layer_shown(0));
}

#[test]
fn block_meshes_and_meshes_far_out_read() {
    let mut cube = TestMesh::cube([0.0; 3], [2.0, 2.0, 2.0]);
    let mut far = cube.clone();
    far.origin = Some([1.0e6, 0.0, 0.0]);
    let blocks = vec![TestBlock {
        id: "b".to_owned(),
        name: "Mesh block".to_owned(),
        shapes: vec![cube.tagged(), far.tagged()],
    }];
    cube.origin = Some([0.0, 0.0, 5.0]);
    let scene = TestScene {
        blocks,
        objects: vec![TestObject::new("Moved", "default", TestShape::Mesh(cube))],
        ..TestScene::default()
    };
    let serp = read(&testfile::write(&scene)).unwrap();
    let Shape::Mesh(near) = &serp.blocks[0].shapes[0] else {
        panic!("{:?}", serp.blocks[0].shapes[0])
    };
    assert_eq!(near.vertices[7], [2.0, 2.0, 2.0]);
    let Shape::Mesh(far) = &serp.blocks[0].shapes[1] else {
        panic!("{:?}", serp.blocks[0].shapes[1])
    };
    assert_eq!(far.vertices[7], [1.0e6 + 2.0, 2.0, 2.0]);
    let Shape::Mesh(moved) = &serp.objects[0].shape else {
        panic!("{:?}", serp.objects[0].shape)
    };
    assert_eq!(moved.vertices[0], [0.0, 0.0, 5.0]);
}

#[test]
fn every_version_up_to_the_newest_reads_and_a_newer_one_is_refused() {
    for version in 1..=NEWEST_VERSION {
        let mut scene = sample();
        scene.version = version;
        let serp = read(&testfile::write(&scene)).unwrap();
        assert_eq!(serp.version, version);
    }
    let mut scene = sample();
    scene.version = NEWEST_VERSION + 1;
    let error = read(&testfile::write(&scene)).unwrap_err();
    assert!(
        error
            .words
            .contains("made by a newer version of its program"),
        "{error:?}"
    );
    // A bare scene too.
    let bare = json!({"format": "serp", "version": 99, "objects": []}).to_string();
    let error = read(bare.as_bytes()).unwrap_err();
    assert!(
        error
            .words
            .contains("made by a newer version of its program")
    );
}

#[test]
fn other_files_are_refused_in_words() {
    let not_serp = json!({"format": "something else", "objects": []}).to_string();
    let error = read(not_serp.as_bytes()).unwrap_err();
    assert!(error.words.contains("not a .serp file"), "{error:?}");
    assert!(read(b"not json at all").is_err());
    assert!(read(b"[1, 2]").is_err());
    let zip = mitcad_zip::stored_zip(&[("other.txt", b"hello")]);
    let error = read(&zip).unwrap_err();
    assert!(error.words.contains("no scene"), "{error:?}");
    // A container cut short.
    let mut cut = testfile::write(&sample());
    cut.truncate(cut.len() / 2);
    assert!(read(&cut).is_err());
}

#[test]
fn damaged_objects_are_unreadable_and_the_rest_is_read() {
    let mut scene = sample();
    let mut bad_mesh = TestMesh::cube([0.0; 3], [1.0; 3]);
    bad_mesh.triangles.push([0, 1, 99]);
    scene
        .objects
        .push(TestObject::new("Bad mesh", "l1", TestShape::Mesh(bad_mesh)));
    scene
        .objects
        .push(TestObject::new("Empty", "l1", TestShape::Bytes(Vec::new())));
    let mut short = TestMesh::cube([0.0; 3], [1.0; 3]).tagged();
    short.truncate(30);
    scene.blocks.push(TestBlock {
        id: "b2".to_owned(),
        name: String::new(),
        shapes: vec![short],
    });
    let mut doc = testfile::scene_json(&scene);
    // An object whose B-rep is not base64.
    doc["objects"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "Garbled", "layer": "l1", "brep": "!!!"}));
    let serp = read(doc.to_string().as_bytes()).unwrap();
    let last = serp.objects.len() - 1;
    for i in [last - 2, last - 1, last] {
        assert!(
            matches!(serp.objects[i].shape, Shape::Unreadable(_)),
            "{:?}",
            serp.objects[i]
        );
    }
    let short = serp.blocks.iter().find(|b| b.id == "b2").unwrap();
    assert_eq!(short.name, "Block 2");
    assert!(matches!(short.shapes[0], Shape::Unreadable(_)));
    assert_eq!(serp.objects[0].shape, Shape::Brep(b"box".to_vec()));
}

#[test]
fn units_missing_or_unknown_are_millimetres() {
    let mut scene = sample();
    scene.units = None;
    let serp = read(&testfile::write(&scene)).unwrap();
    assert_eq!((serp.units.mm, serp.units.assumed), (1.0, true));
    assert!(serp.notes.is_empty());
    scene.units = Some("yards".to_owned());
    let serp = read(&testfile::write(&scene)).unwrap();
    assert_eq!((serp.units.mm, serp.units.assumed), (1.0, true));
    assert_eq!(serp.notes.len(), 1);
    assert!(!serp.notes[0].words.contains("yards"));
    scene.units = Some("in".to_owned());
    assert_eq!(read(&testfile::write(&scene)).unwrap().units.mm, 25.4);
}

#[test]
fn colours_of_bytes_are_scaled() {
    let mut scene = sample();
    scene.objects[0].color = Some([255.0, 0.0, 127.5]);
    let serp = read(&testfile::write(&scene)).unwrap();
    assert_eq!(serp.objects[0].color, Some([1.0, 0.0, 0.5]));
}
