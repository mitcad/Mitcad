// SPDX-License-Identifier: MIT
//! Make Driven and Make Driving on an existing sketch dimension
//! (mitcad#2055): `sketch.set_driven` is one undo step named by the switch,
//! driving again keeps the measured value, and a refusal says why in words.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string())
        .map(|r| panic!("{command} must fail, got {r}"))
        .unwrap_err()
        .to_string()
}

/// The sketch query's entry of dimension `id`.
fn dimension(doc: &Document<MockKernel>, id: &str) -> Value {
    let s: Value = serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap();
    s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no {id} in {s}"))
}

fn set_driven(doc: &mut Document<MockKernel>, id: &str, driven: bool) -> Value {
    command(
        doc,
        json!({"cmd": "sketch.set_driven", "sketch": "F1", "dimension": id, "driven": driven}),
    )
}

/// A sketch with a free line from (0, 0) to (30, 40) and its length as a
/// driving dimension of 50.
fn line_with_length() -> (Document<MockKernel>, String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [30, 40]}),
    );
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "value": 50,
               "dimension": {"type": "length", "line": "c1"}}),
    );
    let k = added["dimensions"][0].as_str().unwrap().to_owned();
    (d, k)
}

#[test]
fn make_driven_and_make_driving_are_named_undo_steps() {
    let (mut d, k) = line_with_length();
    assert_ne!(dimension(&d, &k)["driven"], true);

    set_driven(&mut d, &k, true);
    assert_eq!(d.undo_label(), Some("Make Driven in Sketch1"));
    assert_eq!(dimension(&d, &k)["driven"], true);
    // One Undo takes the switch back.
    command(&mut d, json!({"cmd": "undo"}));
    assert_ne!(dimension(&d, &k)["driven"], true);

    set_driven(&mut d, &k, true);
    let measured = dimension(&d, &k)["measured"].as_f64().unwrap();
    assert!((measured - 50.0).abs() < 1e-6, "{measured}");
    set_driven(&mut d, &k, false);
    assert_eq!(d.undo_label(), Some("Make Driving in Sketch1"));
    let driving = dimension(&d, &k);
    assert_ne!(driving["driven"], true, "{driving}");
    // Driving again at the measured value.
    let value = driving["measured"].as_f64().unwrap();
    assert!((value - 50.0).abs() < 1e-6, "{driving}");
    command(&mut d, json!({"cmd": "undo"}));
    assert_eq!(dimension(&d, &k)["driven"], true);
}

#[test]
fn make_driving_that_over_constrains_is_refused_in_words() {
    let (mut d, k) = line_with_length();
    // A second length on the same line: driven, it only measures.
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "driven": true,
               "dimension": {"type": "length", "line": "c1"}}),
    );
    let second = added["dimensions"][0].as_str().unwrap().to_owned();
    assert_ne!(second, k);
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.set_driven", "sketch": "F1", "dimension": second, "driven": false}),
    );
    assert!(error.contains("over-constrain"), "{error}");
    assert_eq!(dimension(&d, &second)["driven"], true);
}
