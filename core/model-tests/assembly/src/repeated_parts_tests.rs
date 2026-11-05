// SPDX-License-Identifier: MIT
//! Find Repeated Parts (mitcad#1277) with the mock kernel, whose shapes
//! `part(<kind>;<x>,<y>,<z>)` are the part `<kind>` moved to (x, y, z):
//! the copies among a base feature's bodies become occurrences of one
//! component, the bodies left keep their ids and face names, colours
//! follow, and the command and the import's option undo in one step.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::exchange::{ImportAssembly, ImportBody, ImportInstance, ImportPart};
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid};
use mitcad_model::testing::{MockKernel, MockShape};
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|r| serde_json::from_str(&r).unwrap())
        .map_err(|e| e.to_string())
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn body(name: &str, history: &str, faces: usize, color: Option<[f64; 3]>) -> ImportBody<MockShape> {
    ImportBody {
        name: Some(name.to_owned()),
        color,
        shape: MockShape::imported(history, faces),
    }
}

/// A flat file of seven bodies: three bolts (the last red), a plate, two
/// nuts and a washer.
fn parts_file(doc: &Document<MockKernel>) {
    let grey = Some([0.5, 0.5, 0.5]);
    doc.kernel().files.borrow_mut().insert(
        "parts.step".to_owned(),
        vec![
            body("Bolt", "part(bolt;0,0,0)", 6, grey),
            body("Plate", "plate", 6, None),
            body("Bolt", "part(bolt;10,0,0)", 6, grey),
            body("Nut", "part(nut;0,0,5)", 4, None),
            body("Bolt", "part(bolt;20,5,0)", 6, Some([1.0, 0.0, 0.0])),
            body("Nut", "part(nut;10,0,5)", 4, None),
            body("Washer", "part(washer;0,0,0)", 3, None),
        ],
    );
}

fn histories(doc: &Document<MockKernel>) -> Vec<(String, String)> {
    let mut all: Vec<(String, String)> = doc
        .bodies()
        .into_iter()
        .map(|b| (b.uid.to_string(), b.shape.history.clone()))
        .collect();
    all.sort();
    all
}

/// The placements of each occurrence of a component, as translations.
fn placed(doc: &Document<MockKernel>, component: ComponentUid) -> Vec<[f64; 3]> {
    doc.instances()
        .into_iter()
        .filter(|i| i.component == component)
        .map(|i| i.transform.translation)
        .collect()
}

#[test]
fn find_repeated_parts_makes_occurrences_and_undoes_in_one_step() {
    let mut doc = Document::new(MockKernel::default());
    parts_file(&doc);
    command(
        &mut doc,
        json!({"cmd": "import_file", "path": "parts.step"}),
    )
    .unwrap();
    let before = histories(&doc);
    assert_eq!(before.len(), 7);
    let depth = doc.undo_depth();

    let found = command(
        &mut doc,
        json!({"cmd": "find_repeated_parts", "feature": "F1"}),
    )
    .unwrap();
    assert_eq!(
        found["components"],
        json!([
            {"component": "C1", "name": "Bolt", "feature": "F2", "body": "F1.b0",
             "occurrences": ["O1", "O2"], "copies": ["F1.b2"]},
            {"component": "C2", "name": "Nut", "feature": "F3", "body": "F1.b3",
             "occurrences": ["O3", "O4"], "copies": ["F1.b5"]},
        ])
    );
    // The plate, the red bolt (another colour) and the washer copy none.
    assert_eq!(found["kept"], json!(["F1.b1", "F1.b4", "F1.b6"]));
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(doc.undo_label(), Some("Find Repeated Parts in Base1"));

    // The bodies left keep their ids and face names; the copies are gone.
    assert_eq!(
        histories(&doc),
        vec![
            (
                "F1.b0".to_owned(),
                "import(F1:0..6,part(bolt;0,0,0))".to_owned()
            ),
            ("F1.b1".to_owned(), "import(F1:6..12,plate)".to_owned()),
            (
                "F1.b3".to_owned(),
                "import(F1:18..22,part(nut;0,0,5))".to_owned()
            ),
            (
                "F1.b4".to_owned(),
                "import(F1:22..28,part(bolt;20,5,0))".to_owned()
            ),
            (
                "F1.b6".to_owned(),
                "import(F1:32..35,part(washer;0,0,0))".to_owned()
            ),
        ]
    );
    assert_eq!(doc.body_name(BodyUid::new(FeatureUid(1), 6)), "Washer");
    assert_eq!(doc.body_name(BodyUid::new(FeatureUid(1), 4)), "Bolt (3)");
    let feature = query(&doc, json!({"query": "feature", "uid": "F1"}));
    let bodies = feature["def"]["bodies"].as_array().unwrap();
    assert_eq!(bodies.len(), 5);
    assert!(bodies[0].get("number").is_none());
    assert_eq!(bodies[2]["number"], 3);
    assert_eq!(bodies[2]["first_face"], 18);
    assert_eq!(bodies[3]["number"], 4);
    assert!(bodies[3].get("first_face").is_none());
    assert_eq!(bodies[4]["number"], 6);
    assert_eq!(bodies[4]["first_face"], 32);
    // The bolt's colour stays the imported one.
    assert_eq!(bodies[0]["color"], json!([0.5, 0.5, 0.5]));

    // Each component placed where its bodies were.
    let bolt = ComponentUid(1);
    let nut = ComponentUid(2);
    assert_eq!(doc.assembly().name(bolt), "Bolt");
    assert_eq!(
        doc.component_bodies(bolt),
        vec![BodyUid::new(FeatureUid(1), 0)]
    );
    assert_eq!(placed(&doc, bolt), vec![[0.0; 3], [10.0, 0.0, 0.0]]);
    assert_eq!(placed(&doc, nut), vec![[0.0; 3], [10.0, 0.0, 0.0]]);

    // Again: the bodies the components' features use stay; the others
    // copy none.
    let depth_now = doc.undo_depth();
    let again = command(
        &mut doc,
        json!({"cmd": "find_repeated_parts", "feature": "F1"}),
    )
    .unwrap();
    assert_eq!(again["components"], json!([]));
    assert_eq!(doc.undo_depth(), depth_now);
    // Not a base feature.
    let other = command(
        &mut doc,
        json!({"cmd": "find_repeated_parts", "feature": "F2"}),
    )
    .unwrap_err();
    assert!(other.contains("is not a base feature"), "{other}");

    // One undo step takes it all back.
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    assert_eq!(histories(&doc), before);
    assert!(doc.assembly().components.is_empty());
    assert_eq!(doc.undo_depth(), depth);
}

/// A combine after the import (a design whose bodies were joined into one,
/// mitcad#1277's reported design): its tools become the components' bodies
/// where the occurrences show them, its target stays in the base feature.
#[test]
fn a_combine_of_the_bodies_takes_them_from_the_occurrences() {
    for suppressed in [false, true] {
        let mut doc = Document::new(MockKernel::default());
        parts_file(&doc);
        command(
            &mut doc,
            json!({"cmd": "import_file", "path": "parts.step"}),
        )
        .unwrap();
        command(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": "combine", "target": "F1.b1",
                   "tools": ["F1.b0", "F1.b2", "F1.b3", "F1.b5", "F1.b6"],
                   "operation": "join"}}),
        )
        .unwrap();
        if suppressed {
            command(&mut doc, json!({"cmd": "suppress_feature", "uid": "F2"})).unwrap();
        }
        let found = command(
            &mut doc,
            json!({"cmd": "find_repeated_parts", "feature": "F1"}),
        )
        .unwrap();
        let names: Vec<&str> = found["components"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Bolt", "Nut"], "suppressed {suppressed}");
        assert_eq!(found["components"][0]["feature"], "F3");
        assert_eq!(found["kept"], json!(["F1.b1", "F1.b4", "F1.b6"]));
        assert_eq!(found["combines"], json!(["F2"]));
        // The components' features come right after the base feature.
        let order: Vec<String> = doc.features().map(|f| f.uid.to_string()).collect();
        assert_eq!(order, ["F1", "F3", "F4", "F2"]);
        let combine = query(&doc, json!({"query": "feature", "uid": "F2"}));
        assert_eq!(
            combine["def"]["tools"],
            json!([{"body": "F1.b0", "occurrence": "O1"},
                   {"body": "F1.b0", "occurrence": "O2"},
                   {"body": "F1.b3", "occurrence": "O3"},
                   {"body": "F1.b3", "occurrence": "O4"},
                   "F1.b6"])
        );
        assert_eq!(combine["def"]["target"], "F1.b1");
        if !suppressed {
            assert!(
                doc.status(FeatureUid(2)).unwrap().is_ok(),
                "{:?}",
                doc.status(FeatureUid(2))
            );
            let joined = doc
                .body_shape(BodyUid::new(FeatureUid(1), 1))
                .unwrap()
                .history
                .clone();
            assert!(joined.contains("part(bolt;0,0,0)"), "{joined}");
            assert!(
                joined.contains(",10,0,0)"),
                "the copy where O2 is: {joined}"
            );
        }
        // One undo step.
        command(&mut doc, json!({"cmd": "undo"})).unwrap();
        let combine = query(&doc, json!({"query": "feature", "uid": "F2"}));
        assert_eq!(combine["def"]["tools"][1], "F1.b2");
    }
}

#[test]
fn import_file_finds_repeated_parts_in_its_own_undo_step() {
    let mut doc = Document::new(MockKernel::default());
    parts_file(&doc);
    let imported = command(
        &mut doc,
        json!({"cmd": "import_file", "path": "parts.step", "repeated_parts": true}),
    )
    .unwrap();
    assert_eq!(imported["uid"], "F1");
    let found = imported["repeated_parts"]["components"].as_array().unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0]["name"], "Bolt");
    assert_eq!(doc.undo_depth(), 1);
    assert_eq!(doc.undo_label(), Some("Import parts.step"));
    assert_eq!(doc.bodies().len(), 5);
    assert_eq!(doc.instances().len(), 7);
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    assert!(doc.bodies().is_empty());
    assert!(doc.assembly().components.is_empty());
}

#[test]
fn a_base_feature_without_copies_is_left_as_it_is() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().files.borrow_mut().insert(
        "plates.step".to_owned(),
        vec![
            body("Plate", "plate", 6, None),
            body("Bolt", "part(bolt;0,0,0)", 6, None),
        ],
    );
    command(
        &mut doc,
        json!({"cmd": "import_file", "path": "plates.step"}),
    )
    .unwrap();
    let depth = doc.undo_depth();
    let found = command(
        &mut doc,
        json!({"cmd": "find_repeated_parts", "feature": "F1", "tolerance": 0.001}),
    )
    .unwrap();
    assert_eq!(found["components"], json!([]));
    assert_eq!(found["kept"], json!(["F1.b0", "F1.b1"]));
    assert_eq!(doc.undo_depth(), depth);
    let bad = command(
        &mut doc,
        json!({"cmd": "find_repeated_parts", "feature": "F1", "tolerance": 0}),
    )
    .unwrap_err();
    assert!(bad.contains("greater than zero"), "{bad}");
}

/// The import's option with mitcad#1274's choice: "One base feature" finds
/// the copies among its bodies; "Components" finds them inside each part,
/// whose base feature holds copies the file did not share.
#[test]
fn import_file_finds_repeated_parts_with_either_structure() {
    // One base feature of a flat file.
    let mut doc = Document::new(MockKernel::default());
    parts_file(&doc);
    let imported = command(
        &mut doc,
        json!({"cmd": "import_file", "path": "parts.step", "structure": "base",
               "repeated_parts": true}),
    )
    .unwrap();
    let names: Vec<&str> = imported["repeated_parts"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Bolt", "Nut"]);
    assert_eq!(doc.undo_depth(), 1);

    // An assembly: a rail part holding two bolts baked in, placed twice,
    // and a plate.
    let mut doc = Document::new(MockKernel::default());
    let instance = |part: usize, x: f64| ImportInstance {
        part,
        name: None,
        transform: Transform::translation([x, 0.0, 0.0]),
    };
    let assembly = ImportAssembly {
        parts: vec![
            ImportPart {
                name: Some("Rail".to_owned()),
                bodies: vec![
                    body("Bolt", "part(bolt;0,0,0)", 6, None),
                    body("Bolt", "part(bolt;10,0,0)", 6, None),
                    body("Rail", "rail", 6, None),
                ],
                instances: Vec::new(),
            },
            ImportPart {
                name: Some("Plate".to_owned()),
                bodies: vec![body("Plate", "plate", 6, None)],
                instances: Vec::new(),
            },
        ],
        roots: vec![instance(0, 0.0), instance(0, 100.0), instance(1, 0.0)],
    };
    doc.kernel()
        .assemblies
        .borrow_mut()
        .insert("rails.step".to_owned(), assembly);
    let imported = command(
        &mut doc,
        json!({"cmd": "import_file", "path": "rails.step", "repeated_parts": true}),
    )
    .unwrap();
    assert_eq!(imported["structure"], "components");
    let found = imported["repeated_parts"]["components"].as_array().unwrap();
    assert_eq!(found.len(), 1, "{imported}");
    assert_eq!(found[0]["name"], "Bolt");
    assert_eq!(found[0]["occurrences"].as_array().unwrap().len(), 2);
    // The bolt's component is inside the rail's, so both rails show both
    // bolts: 2 rails x (2 bolts + the rail) + the plate.
    assert_eq!(doc.instances().len(), 7);
    assert_eq!(doc.undo_depth(), 1);
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    assert!(doc.assembly().components.is_empty());
}
