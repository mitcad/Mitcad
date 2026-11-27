// SPDX-License-Identifier: MIT
//! Minimum Radius kept in the document (mitcad#2518), with the mock
//! kernel: its definition with `MinRadius<n>` names, one shown at a time
//! with the other analyses, refused fields, a lost face said in words,
//! saved and read back, and the `min_radius_analysis` query on a block
//! whose side faces the mock kernel takes for roundings of 2, 5 and 10 mm:
//! 2 mm at the 2 mm one, a limit of 4 mm marking only that one, a block of
//! planes reporting none. The exact curvature of a real body is the bridge
//! test's (`core/tests/test_bridge.cpp`,
//! `test_minimum_radius_of_a_filleted_block`).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const TOL: f64 = 1e-6;
/// The area the mock kernel gives the 2 mm rounding: a quarter of a
/// cylinder of radius 2, 20 mm high.
const ROUND2_AREA: f64 = std::f64::consts::PI * 20.0;

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

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn query_error(doc: &Document<MockKernel>, query: Value) -> String {
    match doc.query(&query.to_string()) {
        Ok(answer) => panic!("{query} was answered: {answer}"),
        Err(e) => e.to_string(),
    }
}

fn analyses(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "analyses"}))
        .as_array()
        .unwrap()
        .clone()
}

fn close(value: &Value, expected: f64) -> bool {
    value
        .as_f64()
        .is_some_and(|actual| (actual - expected).abs() <= TOL)
}

/// A 40 x 30 x 20 block (F2.b0) on a rectangle on XY: its faces all
/// planes.
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    doc
}

/// The block's faces, in the order the kernel lists them.
fn faces(doc: &Document<MockKernel>) -> Vec<String> {
    doc.body_shape(BodyUid::new(FeatureUid(2), 0))
        .expect("the block")
        .faces
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// The name of the block's face that starts with `prefix`.
fn face_named(doc: &Document<MockKernel>, prefix: &str) -> String {
    faces(doc)
        .into_iter()
        .find(|f| f.starts_with(prefix))
        .unwrap_or_else(|| panic!("no face {prefix}…"))
}

/// The block with three side faces taken for roundings of 2, 5 and
/// 10 mm (cylinders), the 2 mm one with its area; returns their names.
fn rounded_block() -> (Document<MockKernel>, [String; 3]) {
    let doc = block();
    let names = ["F2:side(c1", "F2:side(c2", "F2:side(c3"].map(|prefix| face_named(&doc, prefix));
    for (name, radius) in names.iter().zip([2.0, 5.0, 10.0]) {
        doc.kernel().surfaces.borrow_mut().insert(
            name.clone(),
            SurfaceGeometry::Cylinder {
                origin: [0.0, 0.0, 0.0],
                axis: [0.0, 0.0, 1.0],
                radius,
            },
        );
    }
    doc.kernel()
        .face_areas
        .borrow_mut()
        .insert(names[0].clone(), ROUND2_AREA);
    (doc, names)
}

fn face_ref(name: &str) -> Value {
    json!({"body": "F2.b0", "face": name})
}

#[test]
fn a_minimum_radius_analysis_is_kept_with_its_names() {
    let (mut doc, names) = rounded_block();
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "min_radius", "bodies": ["F2.b0"]}}),
    );
    assert_eq!(added["name"], "MinRadius1");
    assert_eq!(added["recomputed"], 0, "an analysis changes no geometry");
    let first = &analyses(&doc)[0];
    assert_eq!(first["type"], "min_radius");
    assert_eq!(first["visible"], true);
    assert_eq!(first["bodies"], json!(["F2.b0"]));
    assert!(first.get("limit").is_none(), "{first}");
    assert!(first.get("faces").is_none(), "{first}");
    assert!(first.get("error").is_none(), "{first}");

    // A face of the body with a limit; one shown at a time.
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "min_radius",
               "faces": [face_ref(&names[2])], "limit": 3}}),
    );
    assert_eq!(added["name"], "MinRadius2");
    let all = analyses(&doc);
    assert_eq!(all[1]["faces"], json!([face_ref(&names[2])]));
    assert_eq!(all[1]["limit"], 3.0);
    assert_eq!(all[1]["bodies"], json!([]));
    assert_eq!(
        all.iter().map(|a| a["visible"].clone()).collect::<Vec<_>>(),
        [false, true]
    );
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "MinRadius1", "def": {"type": "min_radius",
               "bodies": ["F2.b0"], "limit": 4}, "visible": true}),
    );
    assert_eq!(analyses(&doc)[0]["limit"], 4.0);
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(analyses(&doc)[0].get("limit").is_none());
}

#[test]
fn a_minimum_radius_analysis_refuses_what_it_cannot_keep() {
    let (mut doc, names) = rounded_block();
    let add = |def: Value| json!({"cmd": "add_analysis", "def": def});
    let error = refused(&mut doc, add(json!({"type": "min_radius"})));
    assert!(error.contains("needs a body or a face"), "{error}");
    let error = refused(
        &mut doc,
        add(json!({"type": "min_radius", "bodies": ["F2.b0"], "limit": 0})),
    );
    assert!(error.contains("radius limit"), "{error}");
    let error = refused(
        &mut doc,
        add(json!({"type": "min_radius", "faces": ["xy"]})),
    );
    assert!(error.contains("not a face"), "{error}");
    let error = refused(
        &mut doc,
        add(json!({"type": "min_radius", "bodies": ["F9.b0"]})),
    );
    assert!(error.contains("does not exist"), "{error}");
    let error = refused(
        &mut doc,
        add(json!({"type": "min_radius", "faces": [face_ref("F2:side(c9[c8,c7])")]})),
    );
    assert!(error.contains("has no face"), "{error}");
    let error = refused(
        &mut doc,
        add(json!({"type": "min_radius", "bodies": ["F2.b0"], "radius": 2})),
    );
    assert!(error.contains("radius"), "{error}");
    assert!(analyses(&doc).is_empty());

    // A face kept and then lost (the analysis stays, saying why).
    command(
        &mut doc,
        add(json!({"type": "min_radius", "faces": [face_ref(&names[0])]})),
    );
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F2"}));
    let lost = &analyses(&doc)[0];
    assert!(
        lost["error"].as_str().is_some_and(|e| e.contains("F2.b0")),
        "{lost}"
    );
    let answer = query(
        &doc,
        json!({"query": "min_radius_analysis", "faces": [face_ref(&names[0])]}),
    );
    assert!(answer["error"].is_string(), "{answer}");
}

#[test]
fn minimum_radius_analyses_are_saved_and_read_back() {
    let (mut doc, names) = rounded_block();
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "min_radius", "bodies": ["F2.b0"], "limit": 4}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "min_radius", "faces": [face_ref(&names[1])]}}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["analyses"],
        json!([
            {"name": "MinRadius1", "visible": false, "type": "min_radius", "bodies": ["F2.b0"], "limit": 4.0},
            {"name": "MinRadius2", "type": "min_radius", "bodies": [], "faces": [face_ref(&names[1])]}
        ])
    );
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
}

#[test]
fn the_query_finds_the_smallest_radius_and_marks_the_faces_under_the_limit() {
    let (doc, names) = rounded_block();
    let count = faces(&doc).len();
    let two = faces(&doc).iter().position(|f| *f == names[0]).unwrap();

    // Without a limit: the smallest radius, at the 2 mm rounding, nothing
    // marked.
    let answer = query(
        &doc,
        json!({"query": "min_radius_analysis", "bodies": ["F2.b0"]}),
    );
    assert!(close(&answer["smallest"], 2.0), "{answer}");
    assert_eq!(answer["face"], names[0]);
    assert_eq!(answer["body"], "F2.b0");
    assert!(answer["body_name"].is_string(), "{answer}");
    assert_eq!(answer["limit"], Value::Null);
    assert_eq!(answer["under_count"], 0);
    let body = &answer["bodies"][0];
    assert_eq!(body["body"], "F2.b0");
    assert_eq!(body["faces"], count);
    assert!(close(&body["smallest"], 2.0), "{body}");
    assert_eq!(body["triangles"], json!([]));
    // Where: on the side of the box the mock kernel puts that face on.
    let at: Vec<f64> = answer["at"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    let side = two % 6;
    let axis = 2 - side / 2;
    let edge = [[0.0, 40.0], [0.0, 30.0], [0.0, 20.0]][axis][side % 2];
    assert!((at[axis] - edge).abs() <= TOL, "{at:?} on side {side}");

    // A limit of 4 mm marks the 2 mm rounding only: its area and its two
    // triangles; 6 mm the 5 mm one too.
    let answer = query(
        &doc,
        json!({"query": "min_radius_analysis", "bodies": ["F2.b0"], "limit": 4}),
    );
    assert_eq!(answer["under_count"], 1, "{answer}");
    assert!(close(&answer["under_area"], ROUND2_AREA), "{answer}");
    let body = &answer["bodies"][0];
    assert_eq!(body["under_faces"], json!([names[0]]));
    let triangles = body["triangles"].as_array().unwrap();
    assert_eq!(triangles.len(), 2, "{body}");
    for triangle in triangles {
        let numbers = triangle.as_array().unwrap();
        assert_eq!(numbers.len(), 9);
        for k in [axis, axis + 3, axis + 6] {
            assert!(close(&numbers[k], edge), "{triangle}");
        }
    }
    let wider = query(
        &doc,
        json!({"query": "min_radius_analysis", "bodies": ["F2.b0"], "limit": 6, "triangles": false}),
    );
    assert_eq!(wider["under_count"], 2, "{wider}");
    assert_eq!(
        wider["bodies"][0]["under_faces"],
        json!([names[0], names[1]])
    );
    assert!(wider["bodies"][0].get("triangles").is_none(), "{wider}");
    // A radius equal to the limit is not under it.
    let equal = query(
        &doc,
        json!({"query": "min_radius_analysis", "bodies": ["F2.b0"], "limit": 2}),
    );
    assert_eq!(equal["under_count"], 0, "{equal}");

    // A face picked alone: its own radius.
    let one = query(
        &doc,
        json!({"query": "min_radius_analysis", "faces": [face_ref(&names[2])], "limit": 4}),
    );
    assert!(close(&one["smallest"], 10.0), "{one}");
    assert_eq!(one["bodies"][0]["faces"], 1);
    assert_eq!(one["under_count"], 0);

    // Inputs out of range are refused as other queries' are.
    let error = query_error(&doc, json!({"query": "min_radius_analysis", "limit": -1}));
    assert!(error.contains("radius limit"), "{error}");
    let error = query_error(
        &doc,
        json!({"query": "min_radius_analysis", "faces": ["xy"]}),
    );
    assert!(error.contains("not a face"), "{error}");
    let error = query_error(&doc, json!({"query": "min_radius_analysis", "limt": 1}));
    assert!(error.contains("limt"), "{error}");
}

#[test]
fn a_body_of_planes_reports_no_radius() {
    let doc = block();
    // Every body at the marker when none is named.
    let answer = query(&doc, json!({"query": "min_radius_analysis", "limit": 100}));
    assert_eq!(answer["smallest"], Value::Null, "{answer}");
    assert_eq!(answer["at"], Value::Null);
    assert_eq!(answer["face"], Value::Null);
    assert_eq!(answer["under_count"], 0);
    let body = &answer["bodies"][0];
    assert_eq!(body["smallest"], Value::Null, "{body}");
    assert_eq!(body["faces"], faces(&doc).len());
}
