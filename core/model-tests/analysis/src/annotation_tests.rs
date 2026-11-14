// SPDX-License-Identifier: MIT
//! Annotations (mitcad#702) on the mock kernel (`MockKernel::
//! measures_selections`: the closest points of the picks' boxes, planar
//! faces' planes): added with a group, a tolerance, a format and the
//! places a view suggests; their stored form, older files' measurements
//! read unchanged, the geometry drawn in their planes, ISO 286 fit codes,
//! several edited and deleted as one undo step, and leader notes; lengths
//! placed outside the body and stacked, the old default colours read as
//! Auto and the texts' height (mitcad#894); hole callouts written from
//! their holes' features, a frame under one (mitcad#902); previews of
//! `add_annotation` (mitcad#1097); rolled back with the features their
//! picks are on, not broken (mitcad#1756).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

/// A command's own answer, without the recompute's status fields that every
/// command's answer carries.
fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    let mut answer: Value = serde_json::from_str(&result).unwrap();
    if let Some(fields) = answer.as_object_mut() {
        fields.remove("recomputed");
        if fields.get("error").is_some_and(Value::is_null) {
            fields.remove("error");
        }
    }
    answer
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn annotations(doc: &Document<MockKernel>) -> Vec<Value> {
    let answer: Value =
        serde_json::from_str(&doc.query(r#"{"query": "annotations"}"#).unwrap()).unwrap();
    answer.as_array().unwrap().clone()
}

fn annotation(doc: &Document<MockKernel>, name: &str) -> Value {
    annotations(doc)
        .into_iter()
        .find(|m| m["name"] == name)
        .unwrap_or_else(|| panic!("no annotation {name}"))
}

/// The annotation of this name measured even while hidden (mitcad#716).
fn measured(doc: &Document<MockKernel>, name: &str) -> Value {
    let answer: Value = serde_json::from_str(
        &doc.query(r#"{"query": "annotations", "measure_hidden": true}"#)
            .unwrap(),
    )
    .unwrap();
    answer
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {name}"))
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

fn near_point(actual: &Value, expected: [f64; 3]) {
    for (i, e) in expected.into_iter().enumerate() {
        let value = actual[i]
            .as_f64()
            .unwrap_or_else(|| panic!("{actual} is no point"));
        assert!((value - e).abs() < 1e-6, "{actual} instead of {expected:?}");
    }
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
fn annotations_are_kept_with_their_format_tolerance_group_and_places() {
    let mut doc = block();
    let added = command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(), "group": " Shop ",
               "format": {"decimals": 1, "before": "2 x"},
               "tolerance": {"type": "symmetric", "value": 0.1},
               "line_offset": -4, "text_offset": [1, 2]}),
    );
    assert_eq!(added, json!({"name": "Offset1", "kind": "offset"}));
    assert_eq!(undo_label(&doc), "Add Offset1");
    let offset = annotation(&doc, "Offset1");
    assert_eq!(offset["group"], "Shop");
    near(&offset["value"], 20.0);
    assert_eq!(offset["label"]["text"], "2 x 20.0 ±0.1");
    assert_eq!(offset["label"]["tolerance"], "±0.1");
    near(&offset["deviations"]["upper"], 0.1);
    near(&offset["deviations"]["lower"], -0.1);

    // The stored form: only what is set.
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(
        file["annotations"],
        json!([{"name": "Offset1", "kind": "offset", "color": mitcad_model::AUTO_COLOR,
                "group": "Shop", "a": top(), "b": bottom(), "line_offset": -4.0,
                "text_offset": [1.0, 2.0],
                "format": {"decimals": 1, "before": "2 x"},
                "tolerance": {"type": "symmetric", "value": 0.1}}])
    );
    assert!(file.get("measurements").is_none());
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.kernel().measures_selections.set(true);
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(annotations(&loaded), annotations(&doc));

    // Refused: a fit code on an angle, a note's text on a dimension, a
    // plane on a circle's kind, unknown fields.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": side(), "kind": "angle",
               "tolerance": {"type": "fit", "code": "H7"}}),
    );
    assert!(
        error.contains("a fit code is a length's tolerance"),
        "{error}"
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(), "kind": "distance",
               "tolerance": {"type": "fit", "code": "X7"}}),
    );
    assert!(error.contains("no fit code covered"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(), "depth": 1}),
    );
    assert!(error.contains("depth"), "{error}");
    assert_eq!(annotations(&doc).len(), 1);
    // The measurements query is the same list.
    let answer: Value =
        serde_json::from_str(&doc.query(r#"{"query": "measurements"}"#).unwrap()).unwrap();
    assert_eq!(answer.as_array().unwrap(), &annotations(&doc));
}

#[test]
fn older_files_measurements_open_as_annotations_unchanged_in_value() {
    let mut doc = block();
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
    // With only what the measurement objects of mitcad#649 had, they are
    // written as those were, which older versions read.
    let saved = doc.to_json();
    assert!(saved.contains("\"measurements\""), "{saved}");
    assert!(!saved.contains("\"annotations\""), "{saved}");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.kernel().measures_selections.set(true);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(annotations(&loaded), annotations(&doc));
    near(&annotation(&loaded, "Offset1")["value"], 20.0);
    assert_eq!(annotation(&loaded, "Offset1")["label"]["text"], "20");
    assert_eq!(annotation(&loaded, "Distance1")["visible"], false);
    assert_eq!(loaded.to_json(), saved);
    // One with a new field: all of them as annotations, read back the same.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1", "group": "Shop"}),
    );
    let newer = doc.to_json();
    assert!(newer.contains("\"annotations\""), "{newer}");
    assert!(!newer.contains("\"measurements\""), "{newer}");
    let mut loaded = Document::from_json(&newer, MockKernel::default()).unwrap();
    loaded.kernel().measures_selections.set(true);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(annotations(&loaded), annotations(&doc));
    assert_eq!(loaded.to_json(), newer);
    // Taken away again: back to measurements.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1", "group": null}),
    );
    assert_eq!(doc.to_json(), saved);
    // Both lists are read; a name in both is refused.
    let both = saved.replace(
        "\"measurements\": [",
        "\"annotations\": [{\"name\": \"Offset1\", \"kind\": \"distance\", \
         \"color\": \"#000000\", \"a\": {\"datum\": \"xy\"}, \"b\": {\"datum\": \"yz\"}}],\n  \
         \"measurements\": [",
    );
    let Err(error) = Document::from_json(&both, MockKernel::default()) else {
        panic!("a name listed twice was read");
    };
    assert!(
        error.to_string().contains("listed more than once"),
        "{error}"
    );
    let other = both.replace(
        "\"name\": \"Offset1\", \"kind\": \"distance\"",
        "\"name\": \"Gap\", \"kind\": \"distance\"",
    );
    let read = Document::from_json(&other, MockKernel::default()).unwrap();
    let names: Vec<&str> = read.annotations().iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["Offset1", "Distance1", "Gap"]);
}

/// A root block F2.b0 and a Block (F4.b0, 20 mm cubed) inside an
/// Assembly: the Assembly at (100, 20, 30) turned 90 degrees about Z, the
/// Block at (5, 7, 11) in it. In the design the Block spans x 73 to 93, y
/// 25 to 45 and z 41 to 61 (as `api::measure_tests` places it).
fn nested() -> Document<MockKernel> {
    let mut doc = block();
    command(
        &mut doc,
        json!([
            {"cmd": "create_component", "name": "Assembly", "transform": {
                "translation": [100, 20, 30], "rotation": {"axis": [0, 0, 1], "angle": std::f64::consts::FRAC_PI_2}}},
            {"cmd": "create_component", "name": "Block", "transform": {"translation": [5, 7, 11]}},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"}
        ]),
    );
    doc
}

#[test]
fn a_note_on_a_component_follows_its_occurrence() {
    let mut doc = nested();
    let block_top = json!({"body": "F4.b0", "face": format!("F4:end({R})"), "occurrence": "O1/O2"});
    // Picked at (3, 4, 25) in the Block's coordinates, seen from above:
    // its point and its plane are kept in them.
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": block_top, "text": "Deburr", "at": [3, 4, 25],
               "view": {"direction": [0, 0, -1], "up": [0, 1, 0], "pixel": 0.5}}),
    );
    let note = annotation(&doc, "Note1");
    // The arrow on the Block's top in the design: (3, 4) of the Block is
    // (89, 28), its top at 61.
    near_point(&note["drawing"]["arrows"][0]["tip"], [89.0, 28.0, 61.0]);
    near_point(&note["plane"]["normal"], [0.0, 0.0, 1.0]);
    // The view's right (X) in the Block's coordinates, turned with it.
    near_point(&note["plane"]["x"], [0.0, -1.0, 0.0]);
    near_point(&note["drawing"]["plane"]["x"], [1.0, 0.0, 0.0]);
    // Its text out of the Block the shortest way on an oblique leader
    // (mitcad#902), 2 text heights clear, after a landing of 1.5; no pixels
    // of the view kept.
    near_point(
        &note["drawing"]["text"]["at"],
        [99.785_533_905_932_74, 20.964_466_094_067_262, 61.0],
    );
    assert!(note.get("text_offset").is_none(), "{note}");
    // The Block moved in its Assembly: the note goes with it.
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_transform", "occurrence": "O2",
               "transform": {"translation": [5, 17, 11]}}),
    );
    let note = annotation(&doc, "Note1");
    near_point(&note["drawing"]["arrows"][0]["tip"], [79.0, 28.0, 61.0]);
    // Pointed at a root face instead: the plane and point kept in the
    // Block's coordinates go; a body without a point is refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Note1", "a": {"body": "F2.b0"}}),
    );
    assert!(error.contains("no point to point at"), "{error}");
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Note1", "a": top(), "at": [5, 6, 30]}),
    );
    let note = annotation(&doc, "Note1");
    assert!(note.get("plane").is_none(), "{note}");
    near_point(&note["at"], [5.0, 6.0, 30.0]);
    near_point(&note["drawing"]["arrows"][0]["tip"], [5.0, 6.0, 20.0]);
    // A body picked without a point has nothing to point at.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_annotation", "a": {"body": "F2.b0"}, "text": "Steel"}),
    );
    assert!(error.contains("no point to point at"), "{error}");
}

#[test]
fn seen_along_its_ends_a_length_takes_the_plane_of_the_views_up() {
    for direction in [json!([0, 0, -1]), json!([1e-9, 0, -1])] {
        let mut doc = block();
        // Looking down the top-to-bottom offset: no plane square to the
        // view holds it; the view's up decides.
        command(
            &mut doc,
            json!({"cmd": "add_annotation", "a": top(), "b": bottom(),
                   "view": {"direction": direction, "up": [0, 1, 0], "pixel": 0.5}}),
        );
        let offset = annotation(&doc, "Offset1");
        near_point(&offset["plane"]["normal"], [0.0, 1.0, 0.0]);
        near_point(&offset["drawing"]["plane"]["normal"], [0.0, 1.0, 0.0]);
    }
}

#[test]
fn a_dimension_is_drawn_in_its_plane_relative_to_its_picks() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom()}),
    );
    let offset = annotation(&doc, "Offset1");
    let drawing = &offset["drawing"];
    let [p, q] = [vector(&offset["points"][0]), vector(&offset["points"][1])];
    // Along -Z, so in the plane square to Y (the axis least along it, a
    // plane of the block's sides), the line outside the block along -X
    // (mitcad#894): the ends slid within both faces to their edge there and
    // to their far edge along the plane's normal (mitcad#902: the block's
    // corner at x 0, y 0), the line 6 text heights (2.5 mm, the block being
    // small) past the block; the extension lines from the faces' corners,
    // not across the top.
    near_point(&drawing["plane"]["normal"], [0.0, 1.0, 0.0]);
    near_point(&drawing["plane"]["x"], [0.0, 0.0, -1.0]);
    near_point(&drawing["plane"]["y"], [-1.0, 0.0, 0.0]);
    near(&drawing["text_height"], 2.5);
    near(&drawing["line_offset"], 15.0);
    near_point(&drawing["extension_lines"][0][0], [0.0, 0.0, p[2]]);
    near_point(&drawing["extension_lines"][0][1], [-15.0, 0.0, p[2]]);
    near_point(&drawing["dimension_lines"][0][1], [-15.0, 0.0, q[2]]);
    near_point(&drawing["arrows"][1]["direction"], [0.0, 0.0, -1.0]);
    near_point(&drawing["text"]["at"], [-15.0, 0.0, 10.0]);
    near_point(&drawing["text"]["direction"], [0.0, 0.0, -1.0]);

    // The text dragged along and off the line, the line to the other side.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1", "text_offset": [3, 1],
               "line_offset": 7}),
    );
    assert_eq!(undo_label(&doc), "Edit Offset1");
    let drawing = annotation(&doc, "Offset1")["drawing"].clone();
    near_point(&drawing["text"]["at"], [-7.0 - 1.0, 0.0, 10.0 - 3.0]);
    // The block made higher: the places follow the picks.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 30}),
    );
    let drawing = annotation(&doc, "Offset1")["drawing"].clone();
    near_point(&drawing["text"]["at"], [-8.0, 0.0, 15.0 - 3.0]);
    // Back to the defaults with null.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1", "text_offset": null,
               "line_offset": null}),
    );
    let stored = annotation(&doc, "Offset1");
    assert!(stored.get("text_offset").is_none(), "{stored}");
    near(&stored["drawing"]["line_offset"], 15.0);

    // A plane picked: the XZ plane, which the length lies in already.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1",
               "plane": {"pick": {"datum": "xz"}}}),
    );
    let normal = vector(&annotation(&doc, "Offset1")["drawing"]["plane"]["normal"]);
    near(&json!(normal[1].abs()), 1.0);
    // A plane whose pick is no plane breaks it with the reason.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1",
               "plane": {"pick": {"datum": "x"}}}),
    );
    let broken = annotation(&doc, "Offset1");
    let error = broken["error"]
        .as_str()
        .unwrap_or_else(|| panic!("{broken}"));
    assert!(error.starts_with("the plane: "), "{error}");
    assert!(broken.get("drawing").is_none());
    // A broken annotation is a warning about it (mitcad#1040).
    assert_eq!(broken["level"], "warning", "{broken}");
    assert_eq!(
        broken["notices"],
        json!([{"level": "warning", "message": error, "target": "Offset1"}]),
        "{broken}"
    );
}

#[test]
fn a_view_suggests_the_plane_and_the_places() {
    let mut doc = block();
    // Seen from the front (looking along +Y), Z up, half a millimetre a
    // pixel.
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(),
               "view": {"direction": [0, 1, 0], "up": [0, 0, 1], "pixel": 0.5}}),
    );
    let offset = annotation(&doc, "Offset1");
    // The block's front face, the plane of the part most facing the view;
    // the line outside the block (mitcad#894), 1.5 text heights past its
    // side, on +X (both sides as clear: neither is the view's upper one).
    assert_eq!(offset["plane"], json!({"normal": [0.0, -1.0, 0.0]}));
    near(&offset["line_offset"], 15.0);
    near_point(&offset["drawing"]["plane"]["y"], [1.0, 0.0, 0.0]);
    // A note there: its text along the view's right, four and three text
    // heights off (mitcad#902: no pixels of the view kept).
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "at": [3, 4, 25], "text": "Deburr\nall edges",
               "view": {"direction": [0, 1, 0], "up": [0, 0, 1], "pixel": 0.5}}),
    );
    let note = annotation(&doc, "Note1");
    assert_eq!(note["kind"], "note");
    assert!(note.get("text_offset").is_none(), "{note}");
    near_point(&note["drawing"]["plane"]["x"], [1.0, 0.0, 0.0]);
    near_point(&note["drawing"]["plane"]["y"], [0.0, 0.0, 1.0]);
    near_point(&note["drawing"]["arrows"][0]["tip"], [3.0, 4.0, 20.0]);
    // Its text out of the part the shortest way on an oblique leader
    // (mitcad#902): up and out to its side (-X), 2 text heights past the
    // block, after a landing of 1.5.
    near_point(
        &note["drawing"]["text"]["at"],
        [-3.586_067_977_499_79, 4.0, 25.672_135_954_999_58],
    );
}

#[test]
fn notes_point_at_their_picks() {
    let mut doc = block();
    let added = command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "at": [3, 4, 25],
               "text": "Deburr\nall edges "}),
    );
    assert_eq!(added, json!({"name": "Note1", "kind": "note"}));
    let note = annotation(&doc, "Note1");
    assert_eq!(note["label"]["lines"], json!(["Deburr", "all edges"]));
    assert_eq!(note["label"]["text"], "Deburr / all edges");
    assert!(note.get("value").is_none());
    // In the top face's plane, the point brought onto it.
    let drawing = &note["drawing"];
    near_point(&drawing["plane"]["normal"], [0.0, 0.0, 1.0]);
    near_point(&drawing["arrows"][0]["tip"], [3.0, 4.0, 20.0]);
    // Out of the part the shortest way on an oblique leader (mitcad#902):
    // from its corner's side (-X, -Y) at 45 degrees, 2 text heights
    // (2.5 mm each) past the block, after a landing of 1.5.
    near_point(
        &drawing["text"]["at"],
        [-7.785_533_905_932_736, -3.035_533_905_932_736, 20.0],
    );
    near_point(&drawing["dimension_lines"][0][1], [3.0, 4.0, 20.0]);
    // Its text edited; it follows the face.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Note1", "text": "Polish"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 25}),
    );
    let note = annotation(&doc, "Note1");
    assert_eq!(note["label"]["text"], "Polish");
    near_point(&note["drawing"]["arrows"][0]["tip"], [3.0, 4.0, 25.0]);
    // Refused: a note without text, a tolerance, a dimension's kind.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "kind": "note"}),
    );
    assert!(error.contains("a note needs a text"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Note1",
               "tolerance": {"type": "symmetric", "value": 1}}),
    );
    assert!(error.contains("a note has a text"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Note1", "kind": "distance"}),
    );
    assert!(error.contains("a note stays a note"), "{error}");
    // Its pick gone (rolled back): broken, its text still listed.
    command(&mut doc, json!({"cmd": "set_marker", "position": 1}));
    let broken = annotation(&doc, "Note1");
    assert!(
        broken["error"]
            .as_str()
            .unwrap()
            .starts_with("a pick is not there")
    );
    assert_eq!(broken["label"]["text"], "Polish");
}

#[test]
fn fit_codes_give_their_deviations() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(),
               "tolerance": {"type": "fit", "code": "H7", "deviations": true}}),
    );
    let offset = annotation(&doc, "Offset1");
    assert_eq!(offset["label"]["text"], "20 H7 (+0.021/0)");
    assert_eq!(offset["label"]["main"], "20 H7");
    assert_eq!(offset["label"]["upper"], "+0.021");
    assert_eq!(offset["label"]["lower"], "0");
    assert_eq!(offset["label"]["brackets"], true);
    near(&offset["deviations"]["upper"], 0.021);
    near(&offset["deviations"]["lower"], 0.0);
    // A shaft's, and the size grows into the next range.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1",
               "tolerance": {"type": "fit", "code": "g6"}}),
    );
    assert_eq!(annotation(&doc, "Offset1")["label"]["text"], "20 g6");
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 40}),
    );
    let offset = annotation(&doc, "Offset1");
    near(&offset["deviations"]["upper"], -0.009);
    near(&offset["deviations"]["lower"], -0.025);
    // Over 500 mm: the code, with why there are no deviations.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 600}),
    );
    let offset = annotation(&doc, "Offset1");
    assert_eq!(offset["label"]["text"], "600 g6");
    assert!(
        offset["label"]["warning"]
            .as_str()
            .unwrap()
            .contains("500 mm")
    );
    assert!(offset.get("deviations").is_none());
}

#[test]
fn several_are_edited_and_deleted_as_one_step() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom()}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": side()}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(), "kind": "distance",
               "group": "Assembly"}),
    );
    let before = annotations(&doc);
    let edited = command(
        &mut doc,
        json!({"cmd": "edit_annotation", "names": ["Offset1", "Angle1"], "color": "#00AA00",
               "group": "Shop", "visible": false, "format": {"decimals": 2},
               "tolerance": {"type": "symmetric", "value": 0.5}}),
    );
    assert_eq!(edited, json!({"changed": true}));
    assert_eq!(undo_label(&doc), "Edit 2 annotations");
    for name in ["Offset1", "Angle1"] {
        let a = annotation(&doc, name);
        assert_eq!(a["color"], "#00aa00");
        assert_eq!(a["group"], "Shop");
        assert_eq!(a["visible"], false);
        assert_eq!(a["format"], json!({"decimals": 2}));
        assert_eq!(a["measured"], false);
        assert!(a.get("label").is_none() && a.get("values").is_none(), "{a}");
    }
    // Hidden: measured only when asked for (mitcad#716).
    assert_eq!(measured(&doc, "Offset1")["label"]["text"], "20.00 ±0.5");
    assert_eq!(measured(&doc, "Angle1")["label"]["text"], "90.00° ±0.5°");
    assert_eq!(annotation(&doc, "Distance1")["group"], "Assembly");
    // Nothing changed is no step.
    let again = command(
        &mut doc,
        json!({"cmd": "edit_annotation", "names": ["Offset1", "Angle1"], "visible": false}),
    );
    assert_eq!(again, json!({"changed": false}));
    // One undo brings both back.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(annotations(&doc), before);
    // A group taken away, the decimals back to the default.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Distance1", "group": null,
               "format": {"decimals": 3}}),
    );
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Distance1", "format": {"decimals": null},
               "new_name": "Height"}),
    );
    let height = annotation(&doc, "Height");
    assert!(height.get("group").is_none() && height.get("format").is_none());
    // Refused: a new name or picks for several, a name taken, a name twice.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_annotation", "names": ["Offset1", "Angle1"], "new_name": "X"}),
    );
    assert!(error.contains("without a new name or picks"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1", "new_name": "Angle1"}),
    );
    assert!(error.contains("an annotation Angle1 exists"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_annotation", "names": ["Offset1", "Offset1"], "visible": true}),
    );
    assert!(error.contains("named twice"), "{error}");
    // Other picks: the kind they have first when the old one is gone.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1", "b": side()}),
    );
    assert_eq!(annotation(&doc, "Offset1")["kind"], "angle");

    let before = annotations(&doc);
    command(
        &mut doc,
        json!({"cmd": "delete_annotation", "names": ["Offset1", "Height"]}),
    );
    assert_eq!(undo_label(&doc), "Delete 2 annotations");
    let left: Vec<Value> = annotations(&doc)
        .iter()
        .map(|a| a["name"].clone())
        .collect();
    assert_eq!(left, [json!("Angle1")]);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(annotations(&doc), before);
    command(
        &mut doc,
        json!({"cmd": "delete_annotation", "name": "Angle1"}),
    );
    assert_eq!(undo_label(&doc), "Delete Angle1");
    let error = refused(&mut doc, json!({"cmd": "delete_annotation"}));
    assert!(error.contains("`names`"), "{error}");
}

#[test]
fn datums_and_feature_control_frames_are_kept_on_their_picks() {
    let mut doc = block();
    // A datum on the bottom face: the first free letter, named by it, its
    // leader ending in a datum triangle.
    let added = command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "datum", "a": bottom(), "at": [5, 5, -1]}),
    );
    assert_eq!(added, json!({"name": "DatumA", "kind": "datum"}));
    let datum = annotation(&doc, "DatumA");
    assert_eq!(datum["letter"], "A");
    assert_eq!(datum["label"], json!({"text": "A", "datum": "A"}));
    assert_eq!(datum["drawing"]["terminator"], "datum");
    near_point(&datum["drawing"]["arrows"][0]["tip"], [5.0, 5.0, 0.0]);
    assert!(datum.get("value").is_none());
    // A letter given (a datum B on the side), and refused letters.
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": side(), "letter": "B"}),
    );
    for (letter, why) in [
        ("A", "datum A exists"),
        ("I", "I, O and Q"),
        ("ab", "capital"),
    ] {
        let error = refused(
            &mut doc,
            json!({"cmd": "add_annotation", "a": top(), "letter": letter}),
        );
        assert!(error.contains(why), "{letter}: {error}");
    }

    // The top perpendicular to A within Ø0.05 at maximum material, on a
    // leader to the top.
    let added = command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "at": [10, 10, 20],
               "frame": {"characteristic": "perpendicularity", "value": 0.05,
                         "diameter": true, "modifier": "M", "datums": ["A", "B"]}}),
    );
    assert_eq!(added, json!({"name": "Frame1", "kind": "frame"}));
    let frame = annotation(&doc, "Frame1");
    assert_eq!(
        frame["label"],
        json!({"text": "⟂ | Ø0.05 Ⓜ | A | B", "frame": ["⟂", "Ø0.05 Ⓜ", "A", "B"],
               "tolerance": "Ø0.05 Ⓜ"})
    );
    near_point(&frame["drawing"]["arrows"][0]["tip"], [10.0, 10.0, 20.0]);
    // Its leader on a face ends in a dot (mitcad#902: a dot on a face, an
    // arrow on an edge or an outline, as on the reference drawing).
    assert_eq!(frame["drawing"]["terminator"], "dot");
    // Refused: the rules of ISO 1101.
    for (tolerance, why) in [
        (
            json!({"characteristic": "flatness", "value": 0.1, "datums": ["A"]}),
            "refers to no datum",
        ),
        (
            json!({"characteristic": "parallelism", "value": 0.1}),
            "refers to a datum",
        ),
        (
            json!({"characteristic": "flatness", "value": 0.1, "diameter": true}),
            "no cylinder",
        ),
        (
            json!({"characteristic": "circular_runout", "value": 0.1, "modifier": "L",
                "datums": ["A"]}),
            "no material condition",
        ),
        (
            json!({"characteristic": "position", "value": 0.0}),
            "more than 0",
        ),
        (
            json!({"characteristic": "position", "value": 0.1, "datums": ["A", "A"]}),
            "twice",
        ),
        (
            json!({"characteristic": "position", "value": 0.1, "datums": ["A", "B", "C", "D"]}),
            "three datums",
        ),
    ] {
        let error = refused(
            &mut doc,
            json!({"cmd": "add_annotation", "a": top(), "frame": tolerance}),
        );
        assert!(error.contains(why), "{tolerance}: {error}");
    }
    // A flatness of the top: no datum; one referred to that has no label
    // says so.
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "at": [3, 3, 20],
               "frame": {"characteristic": "flatness", "value": 0.02}}),
    );
    assert_eq!(annotation(&doc, "Frame2")["label"]["text"], "⏥ | 0.02");
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "at": [3, 3, 20],
               "frame": {"characteristic": "position", "value": 0.1, "datums": ["C"]}}),
    );
    assert_eq!(
        annotation(&doc, "Frame3")["label"]["warning"],
        "no datum label C"
    );

    // A frame under a dimension: in that dimension's text's place.
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(), "name": "Height"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "on": "Height", "name": "HeightFrame",
               "frame": {"characteristic": "parallelism", "value": 0.03, "datums": ["A"]}}),
    );
    let height = annotation(&doc, "Height");
    let under = annotation(&doc, "HeightFrame");
    assert_eq!(under["drawing"]["attached"], "Height");
    assert_eq!(under["drawing"]["text"], height["drawing"]["text"]);
    assert_eq!(under["drawing"]["arrows"], json!([]));
    // Its dimension deleted: a leader to its own pick again, in the same
    // step, so that a later Height does not take it.
    command(
        &mut doc,
        json!({"cmd": "delete_annotation", "name": "Height"}),
    );
    let leader = annotation(&doc, "HeightFrame");
    assert!(
        leader.get("on").is_none() && leader.get("error").is_none(),
        "{leader}"
    );
    assert_eq!(
        leader["drawing"]["arrows"].as_array().map(Vec::len),
        Some(1)
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(), "name": "Height"}),
    );
    assert!(annotation(&doc, "HeightFrame").get("on").is_none());
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(annotation(&doc, "HeightFrame")["on"], "Height");
    // A frame named under a dimension that is not there (added so by the
    // document, an older file): on its leader instead.
    let mut orphan = doc.annotation("HeightFrame").unwrap().clone();
    orphan.name = "Orphan".to_owned();
    orphan.on = Some("Gone".to_owned());
    doc.add_annotation(orphan).unwrap();
    let orphan = annotation(&doc, "Orphan");
    assert!(
        orphan.get("on").is_none() && orphan.get("error").is_none(),
        "{orphan}"
    );
    command(&mut doc, json!({"cmd": "undo"}));

    // Edited: another value and datum order; a frame stays a frame, a
    // datum's letter is its own.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Frame1",
               "frame": {"characteristic": "perpendicularity", "value": 0.1, "datums": ["B", "A"]}}),
    );
    assert_eq!(
        annotation(&doc, "Frame1")["label"]["text"],
        "⟂ | 0.1 | B | A"
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Frame1", "kind": "distance"}),
    );
    assert!(error.contains("a frame stays a frame"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "DatumB", "letter": "A"}),
    );
    assert!(error.contains("datum A exists"), "{error}");
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "DatumB", "letter": "C"}),
    );
    assert!(annotation(&doc, "Frame3")["label"].get("warning").is_none());

    // Saved and read back the same, as annotations.
    let saved = doc.to_json();
    assert!(saved.contains("\"annotations\""), "{saved}");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.kernel().measures_selections.set(true);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(annotations(&loaded), annotations(&doc));
    assert_eq!(loaded.to_json(), saved);
}

#[test]
fn the_characteristics_of_frames_are_listed_with_their_rules() {
    let doc = block();
    let answer: Value = serde_json::from_str(
        &doc.query(r#"{"query": "geometric_characteristics"}"#)
            .unwrap(),
    )
    .unwrap();
    let list = answer.as_array().unwrap();
    assert_eq!(list.len(), 14);
    assert_eq!(
        list[1],
        json!({"key": "flatness", "symbol": "⏥", "datums": "none",
               "diameter": false, "modifier": false})
    );
    let position = list.iter().find(|c| c["key"] == "position").unwrap();
    assert_eq!(position["symbol"], "⌖");
    assert_eq!(position["datums"], "optional");
    assert_eq!(position["diameter"], true);
    assert_eq!(position["modifier"], true);
    let runout = list.iter().find(|c| c["key"] == "total_runout").unwrap();
    assert_eq!(runout["datums"], "required");
    assert_eq!(runout["diameter"], false);
}

#[test]
fn frames_follow_their_dimension_and_datums_their_letters() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "datum", "a": bottom()}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(), "name": "Height"}),
    );
    // Datum letters trimmed, as they are stored.
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "on": "Height", "name": "HeightFrame",
               "frame": {"characteristic": "parallelism", "value": 0.03, "datums": [" A "]}}),
    );
    assert_eq!(
        annotation(&doc, "HeightFrame")["frame"]["datums"],
        json!(["A"])
    );

    // Under a leader, under itself or under nothing: refused.
    for (on, why) in [
        ("DatumA", "DatumA is no dimension"),
        ("HeightFrame", "HeightFrame is no dimension"),
        ("Width", "there is no dimension Width"),
    ] {
        let error = refused(
            &mut doc,
            json!({"cmd": "edit_annotation", "name": "HeightFrame", "on": on}),
        );
        assert!(error.contains(why), "{on}: {error}");
    }

    // Height renamed: its frame stays under it, in the same undo step.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Height", "new_name": "Thickness"}),
    );
    let under = annotation(&doc, "HeightFrame");
    assert_eq!(under["on"], "Thickness");
    assert_eq!(under["drawing"]["attached"], "Thickness");
    assert!(under.get("error").is_none(), "{under}");
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(annotation(&doc, "HeightFrame")["on"], "Height");

    // Its dimension hidden: the frame is not drawn either.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Height", "visible": false}),
    );
    let hidden = annotation(&doc, "HeightFrame");
    assert!(
        hidden.get("drawing").is_none() && hidden.get("error").is_none(),
        "{hidden}"
    );
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Height", "visible": true}),
    );

    // `on: null`: a leader to its pick again.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "HeightFrame", "on": null}),
    );
    let leader = annotation(&doc, "HeightFrame");
    assert!(leader.get("on").is_none(), "{leader}");
    assert!(leader["drawing"].get("attached").is_none());
    assert_eq!(
        leader["drawing"]["arrows"].as_array().map(Vec::len),
        Some(1)
    );

    // A datum still named by its letter follows a new letter; one named
    // otherwise keeps its name.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "DatumA", "letter": "D"}),
    );
    assert_eq!(annotation(&doc, "DatumD")["letter"], "D");
    // The frame referring to it follows its letter.
    assert_eq!(
        annotation(&doc, "HeightFrame")["frame"]["datums"],
        json!(["D"])
    );
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "DatumD", "new_name": "Base"}),
    );
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Base", "letter": "E"}),
    );
    assert_eq!(annotation(&doc, "Base")["letter"], "E");
    assert_eq!(
        annotation(&doc, "HeightFrame")["frame"]["datums"],
        json!(["E"])
    );
    assert_eq!(
        annotation(&doc, "HeightFrame")["label"]["text"],
        "∥ | 0.03 | E"
    );

    // On a point (a datum point): no datum label, no frame.
    for new in [
        json!({"cmd": "add_annotation", "kind": "datum", "a": {"datum": "origin"}}),
        json!({"cmd": "add_annotation", "a": {"datum": "origin"},
               "frame": {"characteristic": "flatness", "value": 0.1}}),
    ] {
        let error = refused(&mut doc, new);
        assert!(error.contains("not on a point"), "{error}");
    }
}

#[test]
fn datum_letters_go_on_in_pairs_after_z() {
    let mut doc = block();
    // A to Z without I, O and Q: 23 letters, then AA.
    for _ in 0..23 {
        command(
            &mut doc,
            json!({"cmd": "add_annotation", "kind": "datum", "a": top()}),
        );
    }
    assert_eq!(annotation(&doc, "DatumZ")["letter"], "Z");
    let added = command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "datum", "a": top()}),
    );
    assert_eq!(added, json!({"name": "DatumAA", "kind": "datum"}));
}

#[test]
fn a_frame_stands_under_a_dimension_from_a_point() {
    let mut doc = block();
    // A dimension whose first pick is a point (the origin): a frame under
    // it is no frame on a point; its pick is only an anchor there.
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": {"datum": "origin"}, "b": top(), "name": "Rise"}),
    );
    let added = command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": {"datum": "origin"}, "on": "Rise",
               "frame": {"characteristic": "flatness", "value": 0.1}}),
    );
    assert_eq!(added["name"], "Frame1");
    let under = annotation(&doc, "Frame1");
    assert!(under.get("error").is_none(), "{under}");
    assert_eq!(under["drawing"]["attached"], "Rise");
    // Taken from under it, it would stand on the point: refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Frame1", "on": null}),
    );
    assert!(error.contains("not on a point"), "{error}");

    // The letter a new datum label takes, from the model.
    let next: Value =
        serde_json::from_str(&doc.query(r#"{"query": "next_datum_letter"}"#).unwrap()).unwrap();
    assert_eq!(next, json!({"letter": "A"}));
}

#[test]
fn a_new_letter_never_leaves_a_frame_invalid_and_files_open_again() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "datum", "a": bottom()}),
    );
    // Frame1 refers to A and to C, which no label has yet.
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(),
               "frame": {"characteristic": "perpendicularity", "value": 0.05, "datums": ["A", "C"]}}),
    );
    // C is not offered for a new label while a frame refers to it.
    let next: Value =
        serde_json::from_str(&doc.query(r#"{"query": "next_datum_letter"}"#).unwrap()).unwrap();
    assert_eq!(next, json!({"letter": "B"}));
    // A lettered C would leave Frame1 referring to C twice: refused, named.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "DatumA", "letter": "C"}),
    );
    assert!(
        error.contains("Frame1: datum C is referred to twice"),
        "{error}"
    );
    assert_eq!(
        annotation(&doc, "Frame1")["frame"]["datums"],
        json!(["A", "C"])
    );

    // What the commands allow opens again: lettered, renamed, deleted.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "DatumA", "letter": "D"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(), "name": "Height"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "on": "Height", "name": "Under",
               "frame": {"characteristic": "parallelism", "value": 0.02, "datums": ["D"]}}),
    );
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Height", "new_name": "Thickness"}),
    );
    command(
        &mut doc,
        json!({"cmd": "delete_annotation", "name": "Thickness"}),
    );
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.kernel().measures_selections.set(true);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(annotations(&loaded), annotations(&doc));
}

#[test]
fn a_frame_stands_under_a_dimension_from_a_body() {
    let mut doc = block();
    // A dimension whose first pick is a whole body (no point of its own).
    let body: mitcad_model::MeasurePick = serde_json::from_value(json!({"body": "F2.b0"})).unwrap();
    let mut between =
        mitcad_model::Annotation::new("Between", mitcad_model::MeasureKind::Distance, "", body);
    between.b = Some(serde_json::from_value(bottom()).unwrap());
    doc.add_annotation(between).unwrap();
    // A frame under it: its pick is only an anchor, so no point is needed.
    let added = command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": {"body": "F2.b0"}, "on": "Between",
               "frame": {"characteristic": "flatness", "value": 0.1}}),
    );
    assert_eq!(added["name"], "Frame1");
    assert_eq!(annotation(&doc, "Frame1")["on"], "Between");
    // Edited under it, its pick is not checked either.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Frame1", "a": {"body": "F2.b0"}}),
    );
    // On a leader it would need a point: refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Frame1", "on": null}),
    );
    assert!(error.contains("no point"), "{error}");
    // Moved onto a leader with a face picked in the same edit: accepted,
    // as the editor sends it (mitcad#713).
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Frame1", "on": null, "a": top(),
               "at": [5, 6, 20]}),
    );
    let leader = annotation(&doc, "Frame1");
    assert!(leader.get("on").is_none(), "{leader}");
    assert!(leader.get("error").is_none(), "{leader}");
    assert_eq!(leader["a"]["face"], top()["face"]);
}

#[test]
fn a_frame_under_a_dimension_from_a_point_moves_onto_a_leader_with_a_new_pick() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": {"datum": "origin"}, "b": top(), "name": "Rise"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": {"datum": "origin"}, "on": "Rise",
               "frame": {"characteristic": "flatness", "value": 0.1}}),
    );
    // A point picked again is still refused on a leader.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Frame1", "on": null,
               "a": {"datum": "origin"}}),
    );
    assert!(error.contains("not on a point"), "{error}");
    // `on: null` with a face in one edit: the check takes the new pick.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Frame1", "on": null, "a": top(),
               "at": [5, 6, 20]}),
    );
    let leader = annotation(&doc, "Frame1");
    assert!(leader.get("on").is_none(), "{leader}");
    assert!(leader.get("error").is_none(), "{leader}");
    assert_eq!(
        leader["drawing"]["arrows"].as_array().map(Vec::len),
        Some(1)
    );
}

#[test]
fn a_file_with_a_datum_letter_twice_is_refused() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "datum", "a": top()}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "datum", "a": bottom()}),
    );
    let saved = doc.to_json();
    assert!(Document::from_json(&saved, MockKernel::default()).is_ok());
    // DatumB lettered A by hand: two labels of A.
    let mut file: Value = serde_json::from_str(&saved).unwrap();
    let list = file["annotations"].as_array_mut().expect("annotations");
    let second = list
        .iter_mut()
        .find(|a| a["letter"] == "B")
        .expect("datum B");
    second["letter"] = json!("A");
    let edited = serde_json::to_string(&file).unwrap();
    let Err(error) = Document::from_json(&edited, MockKernel::default()) else {
        panic!("a datum letter on two labels was read");
    };
    assert!(
        error
            .to_string()
            .contains("annotations[1]: datum A is lettered on DatumA and on DatumB"),
        "{error}"
    );
}

#[test]
fn kept_annotations_are_measured_again_only_when_their_bodies_change() {
    // mitcad#716: the application asks for the annotations after every
    // command; picks whose bodies kept their shapes are not measured again.
    let mut doc = block();
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "width", "value": "10 mm"},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [40, 0], "width": 10, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": R}],
              "extent": {"type": "distance", "distance": "width"}, "operation": "new_body"}},
        ]),
    );
    assert!(doc.body_shape("F4.b0".parse().unwrap()).is_some());
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom()}),
    );
    let measures = |doc: &Document<MockKernel>| doc.kernel().count("measure_between");
    near(&annotation(&doc, "Offset1")["value"], 20.0);
    let once = measures(&doc);
    assert!(once >= 1);
    near(&annotation(&doc, "Offset1")["value"], 20.0);
    assert_eq!(measures(&doc), once, "asked again");
    for change in [
        json!({"cmd": "edit_annotation", "name": "Offset1", "new_name": "Height"}),
        json!({"cmd": "edit_annotation", "name": "Height", "color": "#00AA00"}),
        json!({"cmd": "edit_annotation", "name": "Height", "visible": false}),
        json!({"cmd": "edit_annotation", "name": "Height", "visible": true}),
        json!({"cmd": "set_body_visible", "uid": "F2.b0", "visible": false}),
        json!({"cmd": "set_body_visible", "uid": "F2.b0", "visible": true}),
        json!({"cmd": "set_parameter", "name": "width", "value": 15}),
    ] {
        command(&mut doc, change.clone());
        let height = annotation(&doc, "Height");
        assert!(height.get("error").is_none(), "{change}: {height}");
        if height["visible"] == true {
            near(&height["value"], 20.0);
        }
        assert_eq!(measures(&doc), once, "{change} measured it again");
    }

    // The picked body changed: measured again, once.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 12}),
    );
    near(&annotation(&doc, "Height")["value"], 12.0);
    assert_eq!(measures(&doc), once + 1);
    near(&annotation(&doc, "Height")["value"], 12.0);
    assert_eq!(measures(&doc), once + 1);
    // Undone: the earlier shape's measure is still kept.
    command(&mut doc, json!({"cmd": "undo"}));
    near(&annotation(&doc, "Height")["value"], 20.0);
    assert_eq!(measures(&doc), once + 1);

    // Hidden: not measured for the answer, measured when asked for.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Height", "visible": false}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 14}),
    );
    let hidden = annotation(&doc, "Height");
    assert_eq!(hidden["measured"], false);
    assert_eq!(hidden["visible"], false);
    assert!(
        hidden.get("value").is_none()
            && hidden.get("values").is_none()
            && hidden.get("error").is_none(),
        "{hidden}"
    );
    assert_eq!(measures(&doc), once + 1);
    let asked = measured(&doc, "Height");
    near(&asked["value"], 14.0);
    assert!(asked.get("measured").is_none(), "{asked}");
    assert_eq!(asked["values"]["offset"], asked["value"]);
    assert_eq!(measures(&doc), once + 2);
    // Its body gone: broken all the same, without measuring.
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F2"}));
    let broken = annotation(&doc, "Height");
    assert!(broken["error"].is_string(), "{broken}");
    assert_eq!(measures(&doc), once + 2);
}

#[test]
fn more_annotations_than_the_measure_cache_holds_are_not_measured_again() {
    // mitcad#758: the annotations are asked for in the same order after
    // every command; a cache smaller than the keys they use would evict
    // each before it is asked for again and measure all of them each time.
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    let mut faces = Vec::new();
    for i in 0..12 {
        let sketch = format!("F{}", 2 * i + 1);
        let extrude = format!("F{}", 2 * i + 2);
        command(
            &mut doc,
            json!([
                {"cmd": "sketch.create"},
                {"cmd": "sketch.add_rectangle", "sketch": sketch, "corner": [40 * i, 0],
                 "width": 20, "height": 20},
                {"cmd": "add_feature", "def": {"type": "extrude",
                  "profiles": [{"sketch": sketch, "region": R}],
                  "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            ]),
        );
        let body = format!("{extrude}.b0");
        faces.push(json!({"body": body, "face": format!("{extrude}:end({R})")}));
        faces.push(json!({"body": body, "face": format!("{extrude}:start({R})")}));
    }
    // 300 distances between distinct ordered pairs of faces apart: as many
    // distinct measure keys.
    let pairs: Vec<(&Value, &Value)> = faces
        .iter()
        .flat_map(|a| faces.iter().map(move |b| (a, b)))
        .filter(|(a, b)| a != b)
        .take(300)
        .collect();
    assert_eq!(pairs.len(), 300);
    for (a, b) in pairs {
        command(
            &mut doc,
            json!({"cmd": "add_annotation", "a": a, "b": b, "kind": "distance"}),
        );
    }
    let measures = |doc: &Document<MockKernel>| doc.kernel().count("measure_between");
    let listed = annotations(&doc);
    assert_eq!(listed.len(), 300);
    for item in &listed {
        assert!(item.get("error").is_none(), "{item}");
        assert!(item["value"].as_f64().is_some(), "{item}");
    }
    let first = measures(&doc);
    assert_eq!(annotations(&doc).len(), 300);
    assert_eq!(measures(&doc), first, "measured again");
}

/// mitcad#894: a length placed automatically on the synthetic 20 mm block
/// lies outside the block's extent in its plane, on the side away from it,
/// and a second parallel one stacks outside the first; with a view the
/// places are kept, stacked the same way.
#[test]
fn lengths_are_placed_outside_the_body_and_stack_outside_each_other() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom()}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(), "kind": "distance"}),
    );
    // Both in the plane square to Y, along -Z, offset along -X: the block
    // spans x 0 to 20, the ends are at x 10.
    let offset = annotation(&doc, "Offset1")["drawing"].clone();
    let distance = annotation(&doc, "Distance1")["drawing"].clone();
    for drawing in [&offset, &distance] {
        near_point(&drawing["plane"]["y"], [-1.0, 0.0, 0.0]);
        for end in drawing["dimension_lines"][0].as_array().unwrap() {
            let x = end[0].as_f64().unwrap();
            assert!(x < 0.0, "inside the block's extent: {drawing}");
        }
    }
    // 6 text heights (2.5 mm) past the block, their ends slid to the
    // faces' edge at x 0 (mitcad#902); the second 2.5 text heights further
    // out.
    near(&offset["line_offset"], 15.0);
    near(&distance["line_offset"], 21.25);
    near(&distance["dimension_lines"][0][0][0], -21.25);
    // Hidden, the first leaves its place to the second.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1", "visible": false}),
    );
    near(
        &annotation(&doc, "Distance1")["drawing"]["line_offset"],
        15.0,
    );
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1", "visible": true}),
    );

    // Added in a view from the front, Z up: each one's place kept, the
    // second outside the first.
    let mut doc = block();
    let view = json!({"direction": [0, 1, 0], "up": [0, 0, 1], "pixel": 0.5});
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(), "view": view.clone()}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(), "kind": "distance",
               "view": view}),
    );
    let first = annotation(&doc, "Offset1");
    let second = annotation(&doc, "Distance1");
    assert_eq!(second["plane"], json!({"normal": [0.0, -1.0, 0.0]}));
    near(&first["line_offset"], 15.0);
    near(&second["line_offset"], 21.25);
    near_point(&second["drawing"]["plane"]["y"], [1.0, 0.0, 0.0]);
    // Kept: the first moved by hand stays where it was put.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1", "line_offset": 30}),
    );
    near(&annotation(&doc, "Offset1")["drawing"]["line_offset"], 30.0);
    near(
        &annotation(&doc, "Distance1")["drawing"]["line_offset"],
        21.25,
    );
}

/// mitcad#894: the six colours new annotations took in turn are read from
/// a file as Auto; a colour the user chose stays; new ones are Auto.
#[test]
fn old_default_colours_are_read_as_auto() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom()}),
    );
    assert_eq!(annotation(&doc, "Offset1")["color"], "auto");
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1", "color": "#1971c2"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(), "kind": "distance",
               "color": "#112233"}),
    );
    // As an older version wrote it: Blue, one of the six.
    let saved = doc.to_json();
    assert!(saved.contains("#1971c2"), "{saved}");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.kernel().measures_selections.set(true);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(annotation(&loaded, "Offset1")["color"], "auto");
    assert_eq!(annotation(&loaded, "Distance1")["color"], "#112233");
    // Saved again: Auto.
    let again = loaded.to_json();
    assert!(!again.contains("#1971c2"), "{again}");
    assert!(again.contains("\"auto\""), "{again}");
    // Auto given by name, in any case.
    command(
        &mut loaded,
        json!({"cmd": "edit_annotation", "name": "Distance1", "color": "AUTO"}),
    );
    assert_eq!(annotation(&loaded, "Distance1")["color"], "auto");
    let error = refused(
        &mut loaded,
        json!({"cmd": "edit_annotation", "name": "Distance1", "color": "grey"}),
    );
    assert!(error.contains("\"auto\""), "{error}");
}

/// mitcad#894: the height of the annotations' texts, automatic (of the ISO
/// 3098 series by the size of what is annotated) or set for the design;
/// the automatic line keeps 1.5 of it clear of the body.
#[test]
fn the_text_height_is_automatic_or_set_for_the_design() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom()}),
    );
    let height = |doc: &Document<MockKernel>| -> Value {
        serde_json::from_str(&doc.query(r#"{"query": "annotation_text_height"}"#).unwrap()).unwrap()
    };
    // The 20 mm block: the smallest of the series.
    assert_eq!(
        height(&doc),
        json!({"text_height": null, "in_use": 2.5, "automatic": 2.5,
               "series": [2.5, 3.5, 5.0, 7.0, 10.0]})
    );
    command(
        &mut doc,
        json!({"cmd": "set_annotation_text_height", "text_height": 5}),
    );
    assert_eq!(undo_label(&doc), "Annotation Text Height");
    assert_eq!(height(&doc)["in_use"], 5.0);
    let drawing = annotation(&doc, "Offset1")["drawing"].clone();
    near(&drawing["text_height"], 5.0);
    near(&drawing["line_offset"], 30.0);
    // Kept in the file, read back.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["annotation_text_height"], 5.0);
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.annotation_text_height(), Some(5.0));
    assert_eq!(loaded.to_json(), saved);
    // Back to the automatic one; refused out of range.
    command(
        &mut doc,
        json!({"cmd": "set_annotation_text_height", "text_height": null}),
    );
    assert_eq!(height(&doc)["text_height"], Value::Null);
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(file.get("annotation_text_height").is_none());
    let error = refused(
        &mut doc,
        json!({"cmd": "set_annotation_text_height", "text_height": 0}),
    );
    assert!(error.contains("text height"), "{error}");
    let bad = saved.replace(
        "\"annotation_text_height\": 5.0",
        "\"annotation_text_height\": -1",
    );
    assert_ne!(bad, saved);
    assert!(Document::from_json(&bad, MockKernel::default()).is_err());
}

/// mitcad#902: the block with a hole feature (F3) on its top face at
/// `points`, its drill the parameter `drill` (6.6 mm), with `extra` fields
/// (its extent, kind, thread).
fn holed(points: Value, extra: Value) -> Document<MockKernel> {
    let mut doc = block();
    let mut def = json!({"type": "hole",
        "placement": {"type": "face", "body": "F2.b0", "face": format!("F2:end({R})"),
                      "points": points},
        "diameter": "drill", "extent": {"type": "distance", "depth": 10}});
    for (key, field) in extra.as_object().unwrap() {
        def[key] = field.clone();
    }
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "drill", "value": "6.6 mm"},
            {"cmd": "add_feature", "def": def},
        ]),
    );
    doc
}

/// A hole callout on wall `index` of the hole feature F3, picked at
/// `at`; its lines.
fn callout(doc: &mut Document<MockKernel>, index: usize, at: [f64; 3]) -> Value {
    let added = command(
        doc,
        json!({"cmd": "add_annotation", "kind": "hole_callout",
               "a": {"body": "F2.b0", "face": format!("F3:hole{index}.wall")}, "at": at}),
    );
    let name = added["name"].as_str().unwrap().to_owned();
    annotation(doc, &name)["label"]["lines"].clone()
}

#[test]
fn hole_callouts_write_their_holes_from_the_feature() {
    // A simple hole to a depth: its drill and depth (ISO 129-1 symbols).
    let mut doc = holed(json!([[10, 10, 20]]), json!({}));
    assert_eq!(
        callout(&mut doc, 0, [10.0, 13.3, 20.0]),
        json!(["Ø6.6 ↧ 10"])
    );
    let simple = annotation(&doc, "HoleCallout1");
    assert_eq!(simple["kind"], "hole_callout");
    assert_eq!(simple["label"]["text"], "Ø6.6 ↧ 10");
    assert!(simple.get("value").is_none());
    // On a leader with an arrow to the hole's edge, not a dot; its text
    // runs from the leader's end.
    let drawing = &simple["drawing"];
    assert_eq!(drawing["arrows"].as_array().unwrap().len(), 1, "{drawing}");
    assert!(drawing.get("terminator").is_none(), "{drawing}");
    assert!(
        drawing["align"] == "start" || drawing["align"] == "end",
        "{drawing}"
    );
    // Associative: the drill's size followed.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "drill", "value": 8}),
    );
    assert_eq!(
        annotation(&doc, "HoleCallout1")["label"]["lines"],
        json!(["Ø8 ↧ 10"])
    );
    // Kept in the design file as the other annotations.
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(
        saved["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["name"] == "HoleCallout1" && a["kind"] == "hole_callout"),
        "{saved}"
    );

    // Four through holes with a counterbore: counted, THRU, ⌴ Ø ↧.
    let mut doc = holed(
        json!([[4, 4, 20], [16, 4, 20], [4, 16, 20], [16, 16, 20]]),
        json!({"extent": {"type": "through_all"},
               "kind": {"type": "counterbore", "diameter": 11, "depth": 6.8}}),
    );
    assert_eq!(
        callout(&mut doc, 2, [4.0, 16.0, 20.0]),
        json!(["4x Ø6.6 THRU", "⌴ Ø11 ↧ 6.8"])
    );

    // A countersink: ⌵ Ø × the full angle.
    let mut doc = holed(
        json!([[10, 10, 20]]),
        json!({"kind": {"type": "countersink", "diameter": 12,
                        "angle": std::f64::consts::FRAC_PI_2}}),
    );
    assert_eq!(
        callout(&mut doc, 0, [10.0, 10.0, 20.0]),
        json!(["Ø6.6 ↧ 10", "⌵ Ø12 × 90°"])
    );

    // A tapped hole by its thread and class; a thread along part of the
    // wall has its depth and the drill's below it.
    let mut doc = holed(
        json!([[10, 10, 20]]),
        json!({"extent": {"type": "distance", "depth": 12},
               "thread": {"designation": "M6x1", "class": "6H"}}),
    );
    assert_eq!(
        callout(&mut doc, 0, [10.0, 10.0, 20.0]),
        json!(["M6x1 - 6H ↧ 12"])
    );
    let mut doc = holed(
        json!([[10, 10, 20]]),
        json!({"extent": {"type": "distance", "depth": 12},
               "thread": {"designation": "M6x1", "class": "6H", "length": 8}}),
    );
    assert_eq!(
        callout(&mut doc, 0, [10.0, 10.0, 20.0]),
        json!(["M6x1 - 6H ↧ 8", "Ø4.92 ↧ 12"])
    );

    // A custom thread: its size and starts.
    let mut doc = holed(
        json!([[10, 10, 20]]),
        json!({"extent": {"type": "distance", "depth": 12},
               "thread": {"standard": "custom", "custom": {"major": 10, "pitch": 1.5},
                          "starts": 2}}),
    );
    assert_eq!(
        callout(&mut doc, 0, [10.0, 10.0, 20.0]),
        json!(["10x1.5 2 STARTS ↧ 12"])
    );
}

#[test]
fn a_hole_callout_takes_a_frame_and_breaks_with_its_hole() {
    let mut doc = holed(
        json!([[5, 5, 20], [15, 15, 20]]),
        json!({"extent": {"type": "through_all"}}),
    );
    assert_eq!(
        callout(&mut doc, 0, [5.0, 5.0, 20.0]),
        json!(["2x Ø6.6 THRU"])
    );
    // A position frame under it, on the same leader (mitcad#902).
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": {"body": "F2.b0", "face": "F3:hole0.wall"},
               "on": "HoleCallout1", "name": "Position",
               "frame": {"characteristic": "position", "value": 0.1, "diameter": true}}),
    );
    let frame = annotation(&doc, "Position");
    assert_eq!(frame["on"], "HoleCallout1");
    let drawing = &frame["drawing"];
    assert_eq!(drawing["attached"], "HoleCallout1", "{frame}");
    assert_eq!(drawing["under_lines"], 1, "{frame}");
    assert_eq!(
        drawing["align"],
        annotation(&doc, "HoleCallout1")["drawing"]["align"]
    );
    // A hole callout on a pick that is no face or edge is refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "hole_callout", "a": {"body": "F2.b0"}}),
    );
    assert!(error.contains("a hole callout points at a hole"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "hole_callout", "a": {"datum": "xy"}}),
    );
    assert!(error.contains("a hole callout points at a hole"), "{error}");
    // The hole deleted: broken with the reason, as other annotations are;
    // its frame with it.
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F3"}));
    let broken = annotation(&doc, "HoleCallout1");
    assert!(broken["error"].is_string(), "{broken}");
    assert!(broken.get("drawing").is_none(), "{broken}");
    assert!(annotation(&doc, "Position")["error"].is_string());
}

/// The `document` query: the undo and redo steps among the rest.
fn document(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(&doc.query(r#"{"query": "document"}"#).unwrap()).unwrap()
}

/// Two answers alike, their numbers within the baseline tolerance (1e-6
/// mm).
fn alike(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => {
            let (a, e) = (a.as_f64().unwrap(), e.as_f64().unwrap());
            assert!((a - e).abs() < 1e-6, "{path}: {a} instead of {e}");
        }
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len(), "{path}: {actual} instead of {expected}");
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                alike(a, e, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(e)) => {
            let mut keys: Vec<&String> = a.keys().chain(e.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                alike(
                    &actual[key.as_str()],
                    &expected[key.as_str()],
                    &format!("{path}.{key}"),
                );
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

#[test]
fn a_preview_is_what_add_annotation_adds_and_changes_nothing() {
    let mut doc = block();
    let view = json!({"direction": [-1, -1, -1], "up": [0, 0, 1], "pixel": 0.5});
    // Annotations there before: the one previewed is laid out after them
    // (stacked outside a parallel line, its text clear of a note's).
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(), "view": view}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "datum", "a": bottom(), "view": view}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "text": "Deburr", "at": [15, 15, 20],
               "view": view}),
    );
    for new in [
        json!({"cmd": "add_annotation", "a": top(), "text": "Break edges\n0.5 x 45°",
               "at": [5, 6, 20], "view": view}),
        json!({"cmd": "add_annotation", "a": top(), "b": bottom(), "view": view}),
        json!({"cmd": "add_annotation", "kind": "datum", "a": side(), "view": view}),
        json!({"cmd": "add_annotation", "a": side(), "view": view,
               "frame": {"characteristic": "flatness", "value": 0.02}}),
        json!({"cmd": "add_annotation", "a": top(), "on": "Offset1",
               "frame": {"characteristic": "parallelism", "value": 0.03, "datums": ["A"]}}),
    ] {
        let annotations_before = annotations(&doc);
        let document_before = document(&doc);
        let mut asked = new.clone();
        asked["preview"] = json!(true);
        let preview = command(&mut doc, asked);
        assert_eq!(preview["preview"], true, "{preview}");
        assert!(preview.get("error").is_none(), "{preview}");
        // Nothing stored, no undo step.
        assert_eq!(annotations(&doc), annotations_before, "{new}");
        assert_eq!(document(&doc), document_before, "{new}");
        // Added with the same inputs: the same annotation where the preview
        // showed it.
        let added = command(&mut doc, new.clone());
        assert_eq!(preview["name"], added["name"], "{new}");
        assert_eq!(preview["kind"], added["kind"], "{new}");
        let kept = annotation(&doc, added["name"].as_str().unwrap());
        assert!(kept.get("drawing").is_some(), "{kept}");
        let mut shown = preview.clone();
        shown.as_object_mut().unwrap().remove("preview");
        alike(&shown, &kept, added["name"].as_str().unwrap());
        command(&mut doc, json!({"cmd": "undo"}));
        assert_eq!(annotations(&doc), annotations_before, "{new}");
    }
    // What add_annotation refuses, the preview refuses with the same reason.
    for new in [
        json!({"cmd": "add_annotation", "a": {"body": "F2.b0"}, "text": "Steel"}),
        json!({"cmd": "add_annotation", "a": side(), "letter": "A"}),
    ] {
        let mut asked = new.clone();
        asked["preview"] = json!(true);
        assert_eq!(refused(&mut doc, asked), refused(&mut doc, new));
    }
}

#[test]
fn an_annotation_on_a_feature_after_the_marker_is_rolled_back_not_a_warning() {
    // mitcad#1756: editing a sketch rolls the timeline back to it; an
    // annotation on a body a later feature makes is rolled back with that
    // feature, not broken.
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top(), "b": bottom()}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "material", "a": top(), "at": [5, 5, 20]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": {"datum": "xy"}, "b": {"datum": "yz"}}),
    );
    for name in ["Offset1", "Material1", "Angle1"] {
        let shown = annotation(&doc, name);
        assert!(shown.get("error").is_none(), "{shown}");
    }
    // The sketch's edit: the marker right after it, the extrusion rolled
    // back. Its picks are not there, but no warning: rolled back.
    command(&mut doc, json!({"cmd": "set_marker", "position": 1}));
    for name in ["Offset1", "Material1"] {
        let rolled = annotation(&doc, name);
        assert!(rolled["error"].is_string(), "{rolled}");
        assert_eq!(rolled["rolled_back"], true, "{rolled}");
        assert!(rolled.get("level").is_none(), "{rolled}");
        assert!(rolled.get("notices").is_none(), "{rolled}");
        assert!(rolled.get("drawing").is_none(), "{rolled}");
    }
    // The origin's planes are there before any feature: measured as ever.
    let planes = annotation(&doc, "Angle1");
    assert!(
        planes.get("error").is_none() && planes.get("rolled_back").is_none(),
        "{planes}"
    );
    // The marker at the end again (the sketch finished): as before.
    command(&mut doc, json!({"cmd": "set_marker", "position": 2}));
    for name in ["Offset1", "Material1"] {
        let back = annotation(&doc, name);
        assert!(
            back.get("error").is_none()
                && back.get("rolled_back").is_none()
                && back.get("level").is_none(),
            "{back}"
        );
    }
    // A pick that is gone with the marker at the end stays a warning
    // (mitcad#1040): the extrusion deleted.
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F2"}));
    for name in ["Offset1", "Material1"] {
        let broken = annotation(&doc, name);
        assert!(broken["error"].is_string(), "{broken}");
        assert!(broken.get("rolled_back").is_none(), "{broken}");
        assert_eq!(broken["level"], "warning", "{broken}");
    }
}
