// SPDX-License-Identifier: MIT
//! The `parameter_range` query (mitcad#1730): the design evaluated at a
//! parameter's limits on a copy, with the bodies' volumes and boxes, the
//! body count, failures and new warnings against the current value, and
//! the document unchanged after.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::ProfileRef;
use mitcad_model::ids::FeatureUid;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const F1: FeatureUid = FeatureUid(1);

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn refused(doc: &Document<MockKernel>, query: Value) -> String {
    match doc.query(&query.to_string()) {
        Ok(result) => panic!("{query} was answered: {result}"),
        Err(error) => error.to_string(),
    }
}

/// Even-odd point in polygon.
fn inside(polygon: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    for (a, b) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// The region of Sketch1 whose area (its outline less its holes) holds
/// the point.
fn region_at(doc: &Document<MockKernel>, at: [f64; 2]) -> ProfileRef {
    let output = doc.sketch_output(F1).expect("Sketch1 evaluated");
    let region = output
        .region_info
        .iter()
        .find(|r| inside(&r.outline, at) && !r.holes.iter().any(|h| inside(h, at)))
        .unwrap_or_else(|| panic!("no region at {at:?}"));
    ProfileRef {
        sketch: F1,
        region: region.profile.key.clone(),
    }
}

fn parameter(doc: &Document<MockKernel>, name: &str) -> Value {
    query(doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no parameter {name}"))
}

/// A 40 x 20 plate with a hole of 10 at its middle (d1, the hole's
/// diameter, limited to 4 to 24 mm), extruded 10 mm (F2). At 24 the hole
/// is wider than the plate: it cuts the plate's region in two.
fn plate() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    // The mock kernel measures a body as its bounding box.
    doc.kernel().measures.set(true);
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [20, 10],
               "diameter": 10}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 40, "height": 20}),
    );
    assert_eq!(parameter(&doc, "d1")["value"], json!(10.0));
    let plate = region_at(&doc, [5.0, 5.0]);
    let def = json!({"type": "extrude", "profiles": [plate],
                     "extent": {"type": "distance", "distance": 10}, "operation": "new_body"});
    command(&mut doc, json!({"cmd": "add_feature", "def": def}));
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d1", "min": 4, "max": 24}),
    );
    doc
}

/// What must stay as it was: the document's state and undo history, the
/// parameter, the bodies and the timeline.
fn snapshot(doc: &Document<MockKernel>) -> Value {
    let document = query(doc, json!({"query": "document"}));
    json!({
        "revision": document["revision"],
        "modified": document["modified"],
        "undo": document["undo"],
        "redo": document["redo"],
        "undo_depth": document["undo_depth"],
        "d1": parameter(doc, "d1"),
        "bodies": query(doc, json!({"query": "bodies", "volumes": true, "bbox": true})),
        "timeline": query(doc, json!({"query": "timeline"})),
        "regions": doc.sketch_output(F1).unwrap().regions.len(),
    })
}

fn total_volume(answer: &Value) -> f64 {
    answer["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["volume"].as_f64().expect("a measured volume"))
        .sum()
}

#[test]
fn the_maximum_that_splits_a_profile_reports_the_volume_and_the_warning() {
    let doc = plate();
    let before = snapshot(&doc);
    assert_eq!(before["bodies"].as_array().unwrap().len(), 1, "{before}");

    let answer = query(&doc, json!({"query": "parameter_range", "parameter": "d1"}));

    // The current value: one body of the plate's box, nothing failed.
    let current = &answer["current"];
    assert_eq!(answer["parameter"], json!("d1"));
    assert_eq!(current["value"], json!(10.0));
    assert_eq!(current["body_count"], json!(1));
    assert!((total_volume(current) - 8000.0).abs() < 1e-6, "{current}");
    assert_eq!(current["failed_features"], json!([]));
    assert_eq!(answer["timed_out"], json!(false));

    // Its min and max by default.
    let values = answer["values"].as_array().unwrap();
    assert_eq!(values.len(), 2, "{answer}");
    let (at_min, at_max) = (&values[0], &values[1]);
    assert_eq!(at_min["value"], json!(4.0));
    assert_eq!(at_min["text"], json!("4 mm"));
    assert_eq!(at_max["value"], json!(24.0));
    assert_eq!(at_max["text"], json!("24 mm"));

    // At the min the hole is smaller: the plate's box, so its measured
    // volume, is the same, and nothing warns.
    assert_eq!(at_min["changed"], json!(false), "{at_min}");
    assert_eq!(at_min["body_count"], json!(1));
    assert_eq!(at_min["new_warnings"], json!([]));
    assert!(at_min.get("applied").is_none(), "{at_min}");

    // At the max the plate is in two, and the extrusion warns that its
    // region was found as several. (It is now found as three: the piece
    // inside the hole comes in too, mitcad#1740; assert exactly two bodies
    // and a total volume below the plate's once that is fixed.)
    assert_eq!(at_max["changed"], json!(true), "{at_max}");
    let count = at_max["body_count"].as_u64().unwrap();
    assert!(count >= 2, "{at_max}");
    assert_eq!(at_max["body_count_change"], json!(count - 1));
    // A body that is there now gives its change; one that is not is new.
    let bodies = at_max["bodies"].as_array().unwrap();
    for body in bodies {
        match body["volume_change"].as_f64() {
            Some(change) => {
                assert!(change < -1.0, "{body}");
                assert_eq!(body["changed"], json!(true), "{body}");
            }
            None => assert_eq!(body["new"], json!(true), "{body}"),
        }
    }
    assert!(bodies.iter().any(|b| b["new"] == json!(true)), "{at_max}");
    assert_eq!(at_max["failed_features"], json!([]));
    let warnings = at_max["new_warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "{at_max}");
    assert_eq!(warnings[0]["feature"], json!("F2"));
    let warning = warnings[0]["warning"].as_str().unwrap();
    assert!(warning.starts_with("region r{"), "{warning}");
    assert!(warning.contains(" found as r{"), "{warning}");
    assert!(warning.contains("} + r{"), "{warning}");
    assert!(warning.ends_with("after a change in Sketch1"), "{warning}");
    assert!(warnings[0]["message"].is_string());

    // The document is as it was: the same state, undo history, value,
    // bodies and timeline.
    assert_eq!(snapshot(&doc), before);
}

#[test]
fn steps_values_and_limits_of_the_range() {
    let doc = plate();
    let before = snapshot(&doc);

    // Evenly from the min to the max.
    let answer = query(
        &doc,
        json!({"query": "parameter_range", "parameter": "d1", "steps": 3}),
    );
    let values: Vec<f64> = answer["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["value"].as_f64().unwrap())
        .collect();
    assert_eq!(values, [4.0, 14.0, 24.0]);

    // Values of one's own; one beyond the max is held at it, as
    // set_parameter holds it.
    let answer = query(
        &doc,
        json!({"query": "parameter_range", "parameter": "d1", "values": [12, 30],
               "time_limit_s": 60}),
    );
    assert_eq!(answer["time_limit_s"], json!(60.0));
    let beyond = &answer["values"][1];
    assert_eq!(beyond["value"], json!(30.0));
    assert_eq!(beyond["applied"], json!(24.0), "{beyond}");
    assert_eq!(beyond["applied_text"], json!("24 mm"));
    assert_eq!(beyond["limited"], json!("max"));
    assert!(beyond["body_count"].as_u64().unwrap() >= 2, "{beyond}"); // mitcad#1740

    // Refused queries.
    for (query, expected) in [
        (
            json!({"query": "parameter_range", "parameter": "d9"}),
            "unknown parameter d9",
        ),
        (
            json!({"query": "parameter_range", "parameter": "d2"}),
            "d2 has no min and no max; give values",
        ),
        (
            json!({"query": "parameter_range", "parameter": "d1", "values": [5],
                   "steps": 3}),
            "parameter_range takes values or steps, not both",
        ),
        (
            json!({"query": "parameter_range", "parameter": "d1", "steps": 1}),
            "steps must be 2 to 50, got 1",
        ),
        (
            json!({"query": "parameter_range", "parameter": "d1", "values": []}),
            "values must have 1 to 50 numbers, got 0",
        ),
        (
            json!({"query": "parameter_range", "parameter": "d1", "time_limit_s": 0.5}),
            "time_limit_s must be 1 to 600 seconds, got 0.5",
        ),
    ] {
        let error = refused(&doc, query.clone());
        assert!(error.contains(expected), "{query}: {error}");
    }

    assert_eq!(snapshot(&doc), before);
}
