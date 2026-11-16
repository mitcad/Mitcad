// SPDX-License-Identifier: MIT
//! `.serp` files imported as bodies by layer and blocks as components
//! (mitcad#1870), with the mock kernel: files of the reader's test writer
//! whose B-reps are the mock kernel's data (`<faces> <history>`). The
//! import of OCCT's own B-rep data is `cli.import_serp`.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use mitcad_serp::testfile::{
    self, TestBlock, TestLayer, TestMesh, TestObject, TestScene, TestShape,
};

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|r| serde_json::from_str(&r).unwrap())
        .map_err(|e| e.to_string())
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// `scene` written as `name` into a folder of the test's own.
fn written(scene: &TestScene, name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("mitcad-serp-import-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, testfile::write(scene)).unwrap();
    path.to_string_lossy().into_owned()
}

fn component_named(doc: &Document<MockKernel>, name: &str) -> Option<ComponentUid> {
    doc.assembly()
        .components
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.uid)
}

fn body_named(doc: &Document<MockKernel>, name: &str) -> BodyUid {
    doc.bodies()
        .iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("a body {name}"))
        .uid
}

/// The mock kernel's B-rep data of a solid of six faces.
fn solid(history: &str) -> TestShape {
    TestShape::Bytes(format!("6 {history}").into_bytes())
}

/// Two layers (Solids, red, with its child Bolts; Surfaces), a solid in
/// its layer's colour, a blue sheet, a mesh, a hidden solid, a curve, a
/// text, and a point cloud, a layout and a history record; millimetres.
fn layered() -> TestScene {
    let mut bolts = TestLayer::named("l2", "Bolts", [0.2, 0.7, 0.2]);
    bolts.parent = Some("l1".to_owned());
    let mut panel = TestObject::new("Panel", "l3", TestShape::Bytes(b"1 sheet".to_vec()));
    panel.color = Some([0.1, 0.2, 0.9]);
    let mut spare = TestObject::new("Spare", "l2", solid("spare"));
    spare.visible = false;
    let mut extra = serde_json::Map::new();
    extra.insert(
        "pointclouds".to_owned(),
        json!([{"id": "c1", "name": "Scan", "count": 3}]),
    );
    extra.insert("layouts".to_owned(), json!([{"name": "Sheet 1"}]));
    extra.insert(
        "history_records".to_owned(),
        json!([{"command": "extrude"}]),
    );
    TestScene {
        version: 6,
        layers: vec![
            TestLayer::named("default", "Default", [0.85, 0.85, 0.85]),
            TestLayer::named("l1", "Solids", [0.8, 0.1, 0.1]),
            bolts,
            TestLayer::named("l3", "Surfaces", [0.5, 0.5, 0.5]),
        ],
        objects: vec![
            TestObject::new("Block", "l1", solid("box")),
            panel,
            TestObject::new(
                "Scan mesh",
                "l3",
                TestShape::Mesh(TestMesh::cube([0.0; 3], [1.0; 3])),
            ),
            spare,
            TestObject::new("Edge", "l1", TestShape::Bytes(b"0 curve".to_vec())),
            TestObject::new("Note", "l3", TestShape::Bytes(b"STXT\x01note".to_vec())),
        ],
        extra,
        ..TestScene::default()
    }
}

#[test]
fn objects_come_in_as_bodies_with_names_colours_and_layer_folders() {
    for (name, version) in [("layers.serp", 6), ("layers-v1.serp", 1)] {
        let mut scene = layered();
        scene.version = version;
        let path = written(&scene, name);
        let mut doc = Document::new(MockKernel::default());
        let result = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
        // One base feature: no blocks, no components.
        assert!(result.get("structure").is_none(), "{name}: {result}");
        assert!(doc.assembly().components.is_empty());
        let names: Vec<&str> = result["bodies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Block", "Panel", "Scan mesh", "Spare"], "{name}");
        let kinds: Vec<&str> = result["bodies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["solid", "sheet", "mesh", "solid"], "{name}");

        // Colours: the object's own, else its layer's.
        let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
        let bodies = &file["features"][0]["bodies"];
        assert_eq!(bodies[0]["color"], json!([0.8, 0.1, 0.1]), "{name}");
        assert_eq!(bodies[1]["color"], json!([0.1, 0.2, 0.9]), "{name}");
        assert_eq!(bodies[2]["color"], json!([0.5, 0.5, 0.5]), "{name}");
        assert_eq!(file["features"][0]["source"], name);

        // Each body in its layer's folder; the hidden one hidden.
        let folder = |body: &str| doc.body_attributes(body_named(&doc, body)).folder;
        assert_eq!(folder("Block").as_deref(), Some("Solids"));
        assert_eq!(folder("Panel").as_deref(), Some("Surfaces"));
        assert_eq!(folder("Spare").as_deref(), Some("Solids::Bolts"));
        assert!(!doc.body_attributes(body_named(&doc, "Spare")).visible);
        assert!(doc.body_attributes(body_named(&doc, "Block")).visible);

        // What is left out, in one line, in words.
        let warnings = result["warnings"].as_array().unwrap();
        let line = warnings
            .iter()
            .filter_map(Value::as_str)
            .find(|w| w.starts_with("not imported:"))
            .unwrap_or_else(|| panic!("{name}: {warnings:?}"));
        assert_eq!(
            line,
            "not imported: 1 curve or point, 1 point cloud, 1 text, 1 layout, 1 history record"
        );

        // One undo step takes it all back.
        assert_eq!(doc.undo_label(), Some(format!("Import {name}").as_str()));
        doc.undo();
        assert!(doc.bodies().is_empty());
    }
}

#[test]
fn one_layer_needs_no_folders() {
    let mut scene = layered();
    scene.objects.retain(|o| o.layer == "l1");
    let path = written(&scene, "one layer.serp");
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    assert_eq!(doc.bodies().len(), 1);
    assert_eq!(doc.body_attributes(body_named(&doc, "Block")).folder, None);
}

#[test]
fn the_file_s_unit_scales_the_bodies_and_becomes_the_design_s() {
    let mut scene = layered();
    scene.units = Some("cm".to_owned());
    let path = written(&scene, "centimetres.serp");
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    // Each B-rep scaled by 10 (the mesh's points are scaled as read).
    assert_eq!(doc.kernel().count("transform"), 3);
    let document = query(&doc, json!({"query": "document"}));
    assert_eq!(document["units"]["length"], "cm");
    // Into a design with features the unit stays the design's.
    let mut doc = Document::new(MockKernel::default());
    let first = written(&layered(), "first.serp");
    command(
        &mut doc,
        json!({"cmd": "import_file", "path": first, "wrap": false}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "import_file", "path": path, "wrap": false}),
    )
    .unwrap();
    let document = query(&doc, json!({"query": "document"}));
    assert_eq!(document["units"]["length"], "mm");
}

/// A block placed three times: twice moved (copies of its shape), once
/// changed after it was placed (no copy), and a block placed nowhere.
fn blocks() -> TestScene {
    let mut scene = TestScene::default();
    scene
        .layers
        .push(TestLayer::named("l1", "Hardware", [0.3, 0.3, 0.9]));
    scene.blocks = vec![
        TestBlock {
            id: "b1".to_owned(),
            name: "Bolt".to_owned(),
            shapes: vec![b"6 part(bolt;0,0,0)".to_vec()],
        },
        TestBlock {
            id: "b2".to_owned(),
            name: "Unused".to_owned(),
            shapes: vec![b"6 part(unused;0,0,0)".to_vec()],
        },
    ];
    let placed = |name: &str, history: &str| {
        let mut o = TestObject::new(name, "l1", solid(history));
        o.block = Some("b1".to_owned());
        o
    };
    scene.objects = vec![
        TestObject::new("Plate", "default", solid("plate")),
        placed("Bolt 01", "part(bolt;0,0,0)"),
        placed("Bolt 02", "part(bolt;100,0,0)"),
        placed("Bolt 03", "part(nut;0,0,0)"),
    ];
    scene
}

#[test]
fn placed_blocks_become_occurrences_of_a_component() {
    let path = written(&blocks(), "bolts.serp");
    let mut doc = Document::new(MockKernel::default());
    let result = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    assert_eq!(result["structure"], "components", "{result}");
    let components = result["components"].as_array().unwrap();
    assert_eq!(components.len(), 1, "{result}");
    assert_eq!(components[0]["name"], "Bolt");
    assert_eq!(components[0]["bodies"], 1);
    assert_eq!(result["occurrences"], 2);
    let bolt = component_named(&doc, "Bolt").unwrap();
    let placements: Vec<[f64; 3]> = doc
        .assembly()
        .occurrences_of(bolt)
        .map(|o| o.transform.translation)
        .collect();
    assert_eq!(placements, [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]]);
    // The component's body named after the block, in the colour of its
    // first placement's layer.
    let body = body_named(&doc, "Bolt");
    assert_eq!(doc.body_component(body), Some(bolt));
    // The plate and the changed placement are bodies of the design.
    for name in ["Plate", "Bolt 03"] {
        assert_ne!(
            doc.body_component(body_named(&doc, name)),
            Some(bolt),
            "{name}"
        );
    }
    assert_eq!(doc.bodies().len(), 3);
    let warnings: Vec<&str> = result["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        warnings.contains(
            &"1 placed block does not match the shapes of its block (scaled, mirrored or changed after placing): each came in as a body where the file has it"
        ),
        "{warnings:?}"
    );
    assert!(
        warnings.contains(&"the block Unused is placed nowhere and is left out"),
        "{warnings:?}"
    );
    // No ids or indexes in what a person reads.
    assert!(
        warnings
            .iter()
            .all(|w| !w.contains("b1") && !w.contains("object 3")),
        "{warnings:?}"
    );
    // One undo step takes the components back too.
    doc.undo();
    assert!(doc.assembly().components.is_empty());
    assert!(doc.bodies().is_empty());
}

#[test]
fn a_newer_version_is_refused_in_words_and_nothing_changes() {
    let mut scene = layered();
    scene.version = mitcad_serp::NEWEST_VERSION + 1;
    let path = written(&scene, "future.serp");
    let mut doc = Document::new(MockKernel::default());
    let error = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap_err();
    assert!(
        error.contains("made by a newer version of its program"),
        "{error}"
    );
    assert!(error.contains("future.serp"), "{error}");
    assert!(doc.bodies().is_empty());
    assert_eq!(doc.undo_label(), None);
}

#[test]
fn unreadable_objects_are_named_in_words_and_the_rest_comes_in() {
    let mut scene = layered();
    scene.objects.push(TestObject::new(
        "Broken",
        "l1",
        TestShape::Bytes(b"not mock data".to_vec()),
    ));
    let path = written(&scene, "broken.serp");
    let mut doc = Document::new(MockKernel::default());
    let result = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    assert_eq!(result["bodies"].as_array().unwrap().len(), 4);
    let warnings = result["warnings"].as_array().unwrap();
    assert!(
        warnings.contains(&json!(
            "the object \"Broken\" on the layer Solids cannot be read: left out"
        )),
        "{warnings:?}"
    );
    // A file of nothing to import is refused.
    let mut empty = layered();
    empty.objects.retain(|o| o.name.as_deref() == Some("Note"));
    let path = written(&empty, "notes only.serp");
    let error = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap_err();
    assert!(error.contains("no solids, surfaces or meshes"), "{error}");
}

#[test]
fn into_a_design_with_content_a_serp_file_comes_in_its_own_component() {
    let path = written(&layered(), "vase.serp");
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    assert!(doc.import_wraps_by_default(&path));
    let answer = query(&doc, json!({"query": "file_structure", "path": path}));
    assert_eq!(
        answer,
        json!({"assembly": false, "content": true, "wrap": true})
    );
    let result = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    assert_eq!(result["component"]["name"], "vase", "{result}");
}
