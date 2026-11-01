// SPDX-License-Identifier: MIT
//! Measurement objects (mitcad#649) on the mock kernel
//! (`MockKernel::measures_selections`: the closest points of the picks'
//! boxes, planar faces' planes): kept with a default kind, name and
//! colour, measured again at the marker after a parameter change, renamed,
//! recoloured, hidden, changed and deleted as undo steps, saved and read
//! back, and broken while a pick is gone.

use std::f64::consts::{FRAC_PI_2, PI};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use mitcad_model_diff::diff_states;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn measurements(doc: &Document<MockKernel>) -> Vec<Value> {
    let answer: Value =
        serde_json::from_str(&doc.query(r#"{"query": "measurements"}"#).unwrap()).unwrap();
    answer.as_array().unwrap().clone()
}

/// The measurement of this name in the `measurements` query.
fn measurement(doc: &Document<MockKernel>, name: &str) -> Value {
    measurements(doc)
        .into_iter()
        .find(|m| m["name"] == name)
        .unwrap_or_else(|| panic!("no measurement {name}"))
}

/// The measurement of this name measured even while hidden (mitcad#716).
fn measured(doc: &Document<MockKernel>, name: &str) -> Value {
    let answer: Value = serde_json::from_str(
        &doc.query(r#"{"query": "measurements", "measure_hidden": true}"#)
            .unwrap(),
    )
    .unwrap();
    answer
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no measurement {name}"))
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    let answer: Value =
        serde_json::from_str(&doc.query(r#"{"query": "document"}"#).unwrap()).unwrap();
    answer["undo"].clone()
}

fn near(actual: &Value, expected: f64) {
    let value = actual
        .as_f64()
        .unwrap_or_else(|| panic!("{actual} is no number"));
    assert!(
        (value - expected).abs() < 1e-6,
        "{value} instead of {expected}"
    );
}

fn vector(value: &Value) -> [f64; 3] {
    std::array::from_fn(|i| value[i].as_f64().unwrap())
}

fn face(face: &str) -> Value {
    json!({"body": "F2.b0", "face": face})
}

fn top() -> Value {
    face(&format!("F2:end({R})"))
}

fn bottom() -> Value {
    face(&format!("F2:start({R})"))
}

fn side() -> Value {
    face("F2:side(c1[c4,c2])")
}

/// A 20 x 20 block (F2.b0) on XY, as high as the parameter `height`
/// (20 mm).
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "height", "value": "20 mm"},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": "height"}, "operation": "new_body"}},
        ]),
    );
    doc
}

#[test]
fn measurements_are_kept_with_a_default_kind_and_follow_the_model() {
    let mut doc = block();
    assert!(measurements(&doc).is_empty());
    // The top and the bottom: parallel planes, so their offset.
    let added = command(
        &mut doc,
        json!({"cmd": "add_measurement", "a": top(), "b": bottom()}),
    );
    assert_eq!(added["name"], "Offset1");
    assert_eq!(added["kind"], "offset");
    assert_eq!(undo_label(&doc), "Add Offset1");
    let offset = measurement(&doc, "Offset1");
    assert_eq!(offset["color"], mitcad_model::AUTO_COLOR);
    assert_eq!(offset["visible"], true);
    near(&offset["value"], 20.0);
    near(&offset["values"]["distance"], 20.0);
    near(&offset["values"]["angle"], PI);
    near(&offset["values"]["offset"], 20.0);
    // Drawn along the normal from the top to the bottom.
    let [p, q] = [vector(&offset["points"][0]), vector(&offset["points"][1])];
    near(&json!(p[2]), 20.0);
    near(&json!(q[2]), 0.0);
    near(&json!(p[0] - q[0]), 0.0);
    near(&json!(p[1] - q[1]), 0.0);
    assert!(offset.get("error").is_none(), "{offset}");

    // The top and a side meet at an edge: the angle between them, with its
    // corner and rays.
    let added = command(
        &mut doc,
        json!({"cmd": "add_measurement", "a": top(), "b": side()}),
    );
    assert_eq!(added["name"], "Angle1");
    let angle = measurement(&doc, "Angle1");
    assert_eq!(angle["kind"], "angle");
    assert_eq!(angle["color"], mitcad_model::AUTO_COLOR);
    near(&angle["value"], FRAC_PI_2);
    near(&angle["values"]["distance"], 0.0);
    let arc = &angle["arc"];
    let (from, to) = (vector(&arc["from"]), vector(&arc["to"]));
    let between = mitcad_vec3::dot(from, to).clamp(-1.0, 1.0).acos();
    near(&json!(between), FRAC_PI_2);
    near(&json!(vector(&arc["center"])[2]), 20.0);

    // A kind given, and a name and a colour.
    let added = command(
        &mut doc,
        json!({"cmd": "add_measurement", "a": top(), "b": bottom(), "kind": "distance",
               "name": "Gap", "color": "#3366FF"}),
    );
    assert_eq!(added["name"], "Gap");
    let gap = measurement(&doc, "Gap");
    assert_eq!(gap["kind"], "distance");
    assert_eq!(gap["color"], "#3366ff");
    near(&gap["value"], 20.0);

    // The block made higher: every measurement is measured again.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 35}),
    );
    near(&measurement(&doc, "Offset1")["value"], 35.0);
    near(&measurement(&doc, "Gap")["value"], 35.0);
    near(&measurement(&doc, "Angle1")["value"], FRAC_PI_2);
    near(
        &json!(vector(&measurement(&doc, "Offset1")["points"][0])[2]),
        35.0,
    );
    // And back with the undo.
    command(&mut doc, json!({"cmd": "undo"}));
    near(&measurement(&doc, "Offset1")["value"], 20.0);
}

#[test]
fn measurements_are_refused_when_their_picks_have_no_such_value() {
    let mut doc = block();
    // One pick without a circle has nothing to keep.
    let error = refused(&mut doc, json!({"cmd": "add_measurement", "a": top()}));
    assert!(
        error.contains("one pick keeps its radius or diameter"),
        "{error}"
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "add_measurement", "a": top(), "kind": "radius"}),
    );
    assert!(error.contains("the picks have no radius"), "{error}");
    // Two planes at an angle have no offset.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_measurement", "a": top(), "b": side(), "kind": "offset"}),
    );
    assert!(
        error.contains("the picks have no offset; they have distance, angle"),
        "{error}"
    );
    // A body that is not there.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_measurement", "a": top(), "b": {"body": "F9.b0"}}),
    );
    assert!(error.contains("the measurement's picks"), "{error}");
    // A pick that is no pick, and fields no command has.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_measurement", "a": top(), "b": {"face": "F2:end(r{c1})"}}),
    );
    assert!(!error.is_empty());
    let error = refused(
        &mut doc,
        json!({"cmd": "add_measurement", "a": top(), "b": bottom(), "depth": 1}),
    );
    assert!(error.contains("depth"), "{error}");
    // A colour that is none.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_measurement", "a": top(), "b": bottom(), "color": "red"}),
    );
    assert!(error.contains("#rrggbb"), "{error}");
    assert!(measurements(&doc).is_empty());
}

#[test]
fn measurements_are_renamed_recoloured_hidden_changed_and_deleted() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_measurement", "a": top(), "b": bottom()}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_measurement", "a": top(), "b": side()}),
    );
    command(
        &mut doc,
        json!({"cmd": "rename_measurement", "name": "Offset1", "new_name": " Height "}),
    );
    assert_eq!(undo_label(&doc), "Rename Offset1");
    near(&measurement(&doc, "Height")["value"], 20.0);
    let error = refused(
        &mut doc,
        json!({"cmd": "rename_measurement", "name": "Height", "new_name": "Angle1"}),
    );
    assert!(error.contains("an annotation Angle1 exists"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "rename_measurement", "name": "Offset1", "new_name": "X"}),
    );
    assert!(error.contains("there is no annotation Offset1"), "{error}");

    command(
        &mut doc,
        json!({"cmd": "set_measurement_color", "name": "Height", "color": "#00AA00"}),
    );
    assert_eq!(undo_label(&doc), "Color of Height");
    assert_eq!(measurement(&doc, "Height")["color"], "#00aa00");
    let error = refused(
        &mut doc,
        json!({"cmd": "set_measurement_color", "name": "Height", "color": "#00AA0"}),
    );
    assert!(error.contains("#rrggbb"), "{error}");

    command(
        &mut doc,
        json!({"cmd": "set_measurement_visible", "name": "Height", "visible": false}),
    );
    assert_eq!(undo_label(&doc), "Hide Height");
    assert_eq!(measurement(&doc, "Height")["visible"], false);
    // A hidden one is measured when asked for (mitcad#716).
    near(&measured(&doc, "Height")["value"], 20.0);

    command(
        &mut doc,
        json!({"cmd": "set_measurement_kind", "name": "Height", "kind": "angle"}),
    );
    assert_eq!(undo_label(&doc), "Kind of Height");
    near(&measured(&doc, "Height")["value"], PI);
    let error = refused(
        &mut doc,
        json!({"cmd": "set_measurement_kind", "name": "Height", "kind": "diameter"}),
    );
    assert!(
        error.contains("Height: the picks have no diameter"),
        "{error}"
    );

    let before = measurements(&doc);
    command(
        &mut doc,
        json!({"cmd": "delete_measurement", "name": "Height"}),
    );
    assert_eq!(undo_label(&doc), "Delete Height");
    assert_eq!(measurements(&doc).len(), 1);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(measurements(&doc), before);
    // Nothing changed is no undo step.
    command(
        &mut doc,
        json!({"cmd": "set_measurement_visible", "name": "Height", "visible": false}),
    );
    assert_eq!(undo_label(&doc), "Kind of Height");
}

#[test]
fn measurements_are_saved_and_read_back() {
    let mut doc = block();
    let plain = doc.to_json();
    assert!(!plain.contains("\"measurements\""), "none: left out");
    let before = doc.state().clone();
    command(
        &mut doc,
        json!({"cmd": "add_measurement", "a": top(), "b": bottom()}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_measurement", "a": top(), "b": {"datum": "xy"}, "kind": "distance",
               "color": "#112233"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_measurement_visible", "name": "Distance1", "visible": false}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["measurements"],
        json!([
            {"name": "Offset1", "kind": "offset", "color": mitcad_model::AUTO_COLOR,
             "a": top(), "b": bottom()},
            {"name": "Distance1", "kind": "distance", "color": "#112233", "visible": false,
             "a": top(), "b": {"datum": "xy"}}
        ])
    );
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.kernel().measures_selections.set(true);
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(measurements(&loaded), measurements(&doc));
    near(&measurement(&loaded, "Offset1")["value"], 20.0);
    near(&measured(&loaded, "Distance1")["value"], 20.0);
    // Reopened, it follows the model as before.
    command(
        &mut loaded,
        json!({"cmd": "set_parameter", "name": "height", "value": 12}),
    );
    near(&measurement(&loaded, "Offset1")["value"], 12.0);
    near(&measured(&loaded, "Distance1")["value"], 12.0);

    // Older files without measurements load as before.
    let old = Document::from_json(&plain, MockKernel::default()).unwrap();
    assert!(measurements(&old).is_empty());

    // The comparison of versions lists them.
    let diff = diff_states(&before, doc.state());
    let texts: Vec<&str> = diff.annotations.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, ["Offset1: added", "Distance1: added"]);
    assert!(diff.summary.contains("+2 annotations"), "{}", diff.summary);

    // Broken entries are refused with where they are.
    let twice = saved.replace("\"Distance1\"", "\"Offset1\"");
    let Err(error) = Document::from_json(&twice, MockKernel::default()) else {
        panic!("a name listed twice was read");
    };
    assert!(error.to_string().contains("measurements[1]"), "{error}");
    let unknown = saved.replace("\"kind\": \"offset\"", "\"kind\": \"offset\", \"depth\": 1");
    let Err(error) = Document::from_json(&unknown, MockKernel::default()) else {
        panic!("an unknown field was read");
    };
    assert!(error.to_string().contains("measurements[0]"), "{error}");
    let one_pick = saved.replace("\"kind\": \"distance\"", "\"kind\": \"radius\"");
    let Err(error) = Document::from_json(&one_pick, MockKernel::default()) else {
        panic!("a radius of two picks was read");
    };
    assert!(error.to_string().contains("measurements[1]"), "{error}");
}

#[test]
fn a_pick_that_is_gone_breaks_the_measurement() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_measurement", "a": top(), "b": bottom()}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_measurement", "a": {"datum": "xy"}, "b": {"datum": "yz"}}),
    );
    // The extrusion rolled back: the block is not there.
    command(&mut doc, json!({"cmd": "set_marker", "position": 1}));
    let broken = measurement(&doc, "Offset1");
    let error = broken["error"]
        .as_str()
        .unwrap_or_else(|| panic!("{broken}"));
    assert!(error.starts_with("a pick is not there: "), "{error}");
    assert!(broken.get("value").is_none(), "{broken}");
    assert!(broken.get("points").is_none(), "{broken}");
    // The kept definition stays as it was, and the origin's planes are
    // still measured.
    assert_eq!(broken["a"], top());
    assert_eq!(broken["kind"], "offset");
    let planes = measurement(&doc, "Angle1");
    near(&planes["value"], FRAC_PI_2);
    // Rolled forward, it is measured again.
    command(&mut doc, json!({"cmd": "set_marker", "position": 2}));
    near(&measurement(&doc, "Offset1")["value"], 20.0);

    // The extrusion deleted: broken until the undo.
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F2"}));
    assert!(measurement(&doc, "Offset1")["error"].is_string());
    assert_eq!(measurements(&doc).len(), 2, "a broken one stays");
    // Its kind cannot change while it is broken.
    let error = refused(
        &mut doc,
        json!({"cmd": "set_measurement_kind", "name": "Offset1", "kind": "distance"}),
    );
    assert!(error.starts_with("Offset1: "), "{error}");
    // It can be renamed, recoloured, hidden and deleted all the same.
    command(
        &mut doc,
        json!({"cmd": "rename_measurement", "name": "Offset1", "new_name": "Height"}),
    );
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    near(&measurement(&doc, "Offset1")["value"], 20.0);
    assert!(measurement(&doc, "Offset1").get("error").is_none());
}
