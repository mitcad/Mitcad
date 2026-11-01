// SPDX-License-Identifier: MIT
//! The Center of Mass marker (mitcad#1094): the `center_of_mass` query over
//! the bodies as placed, with their materials, and the marker's display
//! state (add, show, hide, delete, undo, saved in the design file).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::test_support::{block, extrude};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("{value} is no number"))
}

fn assert_point(value: &Value, expected: [f64; 3], what: &str) {
    for (i, e) in expected.iter().enumerate() {
        let got = number(&value[i]);
        assert!(
            (got - e).abs() <= 1e-9,
            "{what}: {value}, expected {expected:?}"
        );
    }
}

/// Body1 (F2.b0) of aluminum in the root, and Body2 (F3.b0), steel for
/// lack of a material, in Component1:1 (O1) at the design's origin.
fn two_bodies() -> Document<MockKernel> {
    let mut b = block();
    // The mock kernel's mass properties: of the bounding boxes.
    b.doc.kernel().physical.set(true);
    b.doc
        .add_feature(
            &extrude(b.sketch, &[&b.region], 5.0, "new_component", &[]),
            None,
        )
        .unwrap();
    command(
        &mut b.doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "aluminum"}),
    );
    b.doc
}

#[test]
fn the_center_of_mass_is_the_placed_bodies_total() {
    let mut doc = two_bodies();
    let properties = query(&doc, json!({"query": "properties"}));
    let found = query(&doc, json!({"query": "center_of_mass"}));
    // At the origin the placed total is the `properties` query's.
    let total = &properties["total"];
    let center: Vec<f64> = (0..3)
        .map(|i| number(&total["center_of_mass"][i]))
        .collect();
    assert_point(
        &found["center_of_mass"],
        [center[0], center[1], center[2]],
        "at the origin",
    );
    assert!((number(&found["mass"]) - number(&total["mass"])).abs() <= 1e-12);
    assert!((number(&found["volume"]) - number(&total["volume"])).abs() <= 1e-9);
    assert_eq!(found["bodies"], 2);
    assert_eq!(found["default_material"], 1, "{found}");
    assert_eq!(found["marker"], Value::Null);
    let aluminum = number(&properties["bodies"][0]["mass"]);
    let steel = number(&properties["bodies"][1]["mass"]);
    assert_eq!(properties["bodies"][1]["material"], "steel");
    assert!(aluminum > 0.0 && steel > 0.0 && (aluminum - steel).abs() > 1e-9);

    // Moving the component moves the center by its share of the mass.
    command(
        &mut doc,
        json!({"cmd": "ground_occurrence", "occurrence": "Component1:1", "grounded": false}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_transform", "occurrence": "Component1:1",
               "transform": {"translation": [100, 0, 0]}}),
    );
    let moved = query(&doc, json!({"query": "center_of_mass"}));
    let shift = 100.0 * steel / (aluminum + steel);
    assert_point(
        &moved["center_of_mass"],
        [center[0] + shift, center[1], center[2]],
        "moved",
    );

    // A material of its own counts: none is steel by default any more.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F3.b0", "material": "brass"}),
    );
    let brass = query(&doc, json!({"query": "center_of_mass"}));
    assert_eq!(brass["default_material"], 0);
    assert!(number(&brass["mass"]) > number(&moved["mass"]));
}

#[test]
fn the_marker_is_display_state_kept_in_the_design() {
    let mut doc = two_bodies();
    let display =
        |doc: &Document<MockKernel>| query(doc, json!({"query": "document"}))["display"].clone();
    let undo =
        |doc: &Document<MockKernel>| query(doc, json!({"query": "document"}))["undo"].clone();
    assert_eq!(display(&doc).get("center_of_mass"), None);

    command(&mut doc, json!({"cmd": "add_center_of_mass"}));
    assert_eq!(display(&doc)["center_of_mass"], json!({"visible": true}));
    assert_eq!(undo(&doc), "Add Center of Mass");
    assert_eq!(
        query(&doc, json!({"query": "center_of_mass"}))["marker"],
        json!({"visible": true})
    );
    // Added again while shown: nothing changes.
    let depth = query(&doc, json!({"query": "document"}))["undo_depth"].clone();
    command(&mut doc, json!({"cmd": "add_center_of_mass"}));
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo_depth"],
        depth
    );

    command(
        &mut doc,
        json!({"cmd": "set_center_of_mass_visible", "visible": false}),
    );
    assert_eq!(display(&doc)["center_of_mass"], json!({"visible": false}));
    assert_eq!(undo(&doc), "Hide Center of Mass");

    // Saved in the design file and read back.
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(file["display"]["center_of_mass"], json!({"visible": false}));
    let opened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(
        display(&opened)["center_of_mass"],
        json!({"visible": false})
    );

    // Add on a hidden marker shows it; undo hides it again.
    command(&mut doc, json!({"cmd": "add_center_of_mass"}));
    assert_eq!(undo(&doc), "Show Center of Mass");
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(display(&doc)["center_of_mass"], json!({"visible": false}));

    command(&mut doc, json!({"cmd": "delete_center_of_mass"}));
    assert_eq!(undo(&doc), "Delete Center of Mass");
    assert_eq!(display(&doc).get("center_of_mass"), None);
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(file["display"].get("center_of_mass"), None);
    for refused in [
        json!({"cmd": "delete_center_of_mass"}),
        json!({"cmd": "set_center_of_mass_visible", "visible": true}),
    ] {
        let error = doc.command(&refused.to_string()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "the design has no Center of Mass marker",
            "{refused}"
        );
    }
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(display(&doc)["center_of_mass"], json!({"visible": false}));
}
