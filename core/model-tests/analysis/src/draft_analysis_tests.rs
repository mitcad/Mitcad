// SPDX-License-Identifier: MIT
//! Draft Analysis kept in the document (mitcad#2166), with the mock
//! kernel: its definition with the defaults and `Draft<n>` names, one shown
//! at a time with the other analyses, refused fields, the pull direction
//! from a plane, a line or a flip, a face reference followed when a sketch
//! change renames its face, a lost one said in words, saved and read back,
//! and the `draft_analysis` query's classes on the mock kernel's box
//! meshes.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::notice::FeatureError;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
/// The front face of the block: line c1 between c4 and c2.
const FRONT: &str = "F2:side(c1[c4,c2])";
const TOL: f64 = 1e-6;

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

fn shown(doc: &Document<MockKernel>) -> Vec<bool> {
    analyses(doc)
        .iter()
        .map(|a| a["visible"].as_bool().unwrap())
        .collect()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn close(value: &Value, expected: f64) -> bool {
    value
        .as_f64()
        .is_some_and(|actual| (actual - expected).abs() <= TOL)
}

/// A `width` x `depth` x `height` block (F2.b0) on a rectangle on XY.
fn block(width: f64, depth: f64, height: f64) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": width, "height": depth},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": height}, "operation": "new_body"}},
        ]),
    );
    doc
}

fn front() -> Value {
    json!({"body": "F2.b0", "face": FRONT})
}

/// The name of the block's face that starts with `prefix`.
fn face_named(doc: &Document<MockKernel>, prefix: &str) -> String {
    doc.body_shape(BodyUid::new(FeatureUid(2), 0))
        .expect("the block")
        .faces
        .iter()
        .map(ToString::to_string)
        .find(|f| f.starts_with(prefix))
        .unwrap_or_else(|| panic!("no face {prefix}…"))
}

/// The mock kernel has the front face's plane (outward -Y) under `names`.
fn front_plane(doc: &Document<MockKernel>, names: &[&str]) {
    for name in names {
        doc.kernel().surfaces.borrow_mut().insert(
            (*name).to_owned(),
            SurfaceGeometry::Plane {
                origin: [0.0, 0.0, 0.0],
                normal: [0.0, -1.0, 0.0],
            },
        );
    }
}

#[test]
fn a_draft_analysis_takes_the_defaults_and_draft_names() {
    let mut doc = block(20.0, 20.0, 20.0);
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "draft", "bodies": ["F2.b0"]}}),
    );
    assert_eq!(added["name"], "Draft1");
    assert_eq!(added["recomputed"], 0, "an analysis changes no geometry");
    assert_eq!(undo_label(&doc), "Add Draft1");
    let first = &analyses(&doc)[0];
    assert_eq!(first["type"], "draft");
    assert_eq!(first["visible"], true);
    assert_eq!(first["bodies"], json!(["F2.b0"]));
    assert_eq!(first["pull"], Value::Null);
    assert_eq!(first["angle"], 1.0);
    assert!(first.get("flip").is_none(), "{first}");
    // Without a pull reference: the design's +Z.
    assert_eq!(first["pull_direction"], json!([0.0, 0.0, 1.0]));
    assert!(first.get("error").is_none(), "{first}");

    // A plane's normal flipped; numbering is per type, one shown at a time.
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "draft", "bodies": ["F2.b0"],
               "pull": "xy", "flip": true, "angle": 2.5}}),
    );
    assert_eq!(added["name"], "Draft2");
    assert_eq!(analyses(&doc)[1]["pull_direction"], json!([0.0, 0.0, -1.0]));
    assert_eq!(analyses(&doc)[1]["flip"], true);
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "print", "bodies": ["F2.b0"]}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": "xy"}}),
    );
    assert_eq!(shown(&doc), [false, false, false, true]);

    // An axis' direction; the end face's outward normal.
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Draft1", "def": {"type": "draft",
               "bodies": ["F2.b0"], "pull": "x"}, "visible": true}),
    );
    assert_eq!(undo_label(&doc), "Edit Draft1");
    assert_eq!(analyses(&doc)[0]["pull_direction"], json!([1.0, 0.0, 0.0]));
    assert_eq!(shown(&doc), [true, false, false, false]);
    let top = json!({"body": "F2.b0", "face": format!("F2:end({R})")});
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Draft2", "def": {"type": "draft",
               "bodies": ["F2.b0"], "pull": top}}),
    );
    assert_eq!(analyses(&doc)[1]["pull_direction"], json!([0.0, 0.0, 1.0]));
    assert_eq!(analyses(&doc)[1]["pull"], top);

    command(
        &mut doc,
        json!({"cmd": "rename_analysis", "name": "Draft1", "new_name": "Mould pull"}),
    );
    command(
        &mut doc,
        json!({"cmd": "delete_analysis", "name": "Draft2"}),
    );
    let names: Vec<Value> = analyses(&doc).iter().map(|a| a["name"].clone()).collect();
    assert_eq!(names, ["Mould pull", "Print1", "Section1"]);
    command(
        &mut doc,
        json!({"cmd": "set_analysis_visible", "name": "Mould pull", "visible": false}),
    );
    assert_eq!(undo_label(&doc), "Hide Mould pull");
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(analyses(&doc)[0]["name"], "Draft1");
    assert_eq!(analyses(&doc)[1]["name"], "Draft2");
}

#[test]
fn a_draft_analysis_refuses_what_it_cannot_keep() {
    let mut doc = block(20.0, 20.0, 20.0);
    let add = |def: Value| json!({"cmd": "add_analysis", "def": def});
    for (def, says) in [
        (
            json!({"type": "draft", "bodies": ["F2.b0"], "depth": 3}),
            "depth",
        ),
        (json!({"type": "draft", "bodies": []}), "needs a body"),
        (
            json!({"type": "draft", "bodies": ["F2.b0"], "angle": 90}),
            "draft angle",
        ),
        (
            json!({"type": "draft", "bodies": ["F2.b0"], "angle": -1}),
            "draft angle",
        ),
        (json!({"type": "draft", "bodies": ["F9.b0"]}), "F9.b0"),
        (
            json!({"type": "draft", "bodies": ["F2.b0"], "pull": "origin"}),
            "neither a plane nor a line",
        ),
        (
            json!({"type": "draft", "bodies": ["F2.b0"],
                   "pull": {"body": "F2.b0", "face": "F2:end(r{c9})"}}),
            "pull direction",
        ),
    ] {
        let error = refused(&mut doc, add(def.clone()));
        assert!(error.contains(says), "{def}: {error}");
    }
    assert!(analyses(&doc).is_empty());
}

#[test]
fn the_pull_face_follows_a_renamed_face_and_a_lost_one_is_said_in_words() {
    let mut doc = block(60.0, 40.0, 10.0);
    // The front face's name after a rounded corner, from a first try.
    command(
        &mut doc,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": "c1", "b": "c2", "radius": 3}),
    );
    let renamed = face_named(&doc, "F2:side(c1[");
    assert_ne!(renamed, FRONT);
    command(&mut doc, json!({"cmd": "undo"}));
    front_plane(&doc, &[FRONT, &renamed]);

    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "draft", "bodies": ["F2.b0"], "pull": front()}}),
    );
    assert_eq!(analyses(&doc)[0]["pull_direction"], json!([0.0, -1.0, 0.0]));

    // The corner rounded again: the face has another name and is found.
    command(
        &mut doc,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": "c1", "b": "c2", "radius": 3}),
    );
    let followed = &analyses(&doc)[0];
    assert!(followed.get("error").is_none(), "{followed}");
    assert_eq!(followed["pull_direction"], json!([0.0, -1.0, 0.0]));
    // The definition keeps the name it was given.
    assert_eq!(followed["pull"], front());
    let answer = query(
        &doc,
        json!({"query": "draft_analysis", "bodies": ["F2.b0"], "pull": front(), "triangles": false}),
    );
    assert_eq!(answer["pull"], json!([0.0, -1.0, 0.0]));
    assert!(answer.get("error").is_none(), "{answer}");

    // Rolled back before the block: the analysis says why, and stays.
    command(&mut doc, json!({"cmd": "set_marker", "position": 1}));
    let rolled = &analyses(&doc)[0];
    assert!(rolled.get("pull_direction").is_none(), "{rolled}");
    let error = rolled["error"].as_str().unwrap();
    assert!(error.contains("F2.b0"), "{rolled}");
    assert!(
        FeatureError::from_text(error)
            .message
            .contains("does not exist at the timeline marker"),
        "{rolled}"
    );
    let answer = query(
        &doc,
        json!({"query": "draft_analysis", "bodies": ["F2.b0"]}),
    );
    assert!(
        answer["error"].as_str().unwrap().contains("F2.b0"),
        "{answer}"
    );
    command(&mut doc, json!({"cmd": "set_marker", "position": 2}));
    assert!(analyses(&doc)[0].get("error").is_none());

    // Pulled along the top face, which a shell then takes away: the pull
    // face is lost, and the analysis says so in words.
    let top = face_named(&doc, "F2:end(");
    let pull_top = json!({"body": "F2.b0", "face": top});
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Draft1", "def": {"type": "draft",
               "bodies": ["F2.b0"], "pull": pull_top}}),
    );
    assert_eq!(analyses(&doc)[0]["pull_direction"], json!([0.0, 0.0, 1.0]));
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "shell", "body": "F2.b0",
               "faces": [top], "inside": 2}}),
    );
    let lost = &analyses(&doc)[0];
    let error = lost["error"].as_str().unwrap_or_else(|| panic!("{lost}"));
    let words = FeatureError::from_text(error).message;
    assert!(words.starts_with("the pull direction: "), "{words}");
    assert!(words.contains("face"), "{words}");
    // The analysis stays, with its definition, and is found again undone.
    assert_eq!(lost["pull"], pull_top);
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(analyses(&doc)[0].get("error").is_none());
}

#[test]
fn draft_analyses_are_saved_and_read_back() {
    let mut doc = block(20.0, 20.0, 20.0);
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "draft", "bodies": ["F2.b0"],
               "pull": {"body": "F2.b0", "face": format!("F2:end({R})")}, "flip": true, "angle": 3}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "draft", "bodies": ["F2.b0"]}}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["analyses"],
        json!([
            {"name": "Draft1", "visible": false, "type": "draft", "bodies": ["F2.b0"],
             "pull": {"body": "F2.b0", "face": format!("F2:end({R})")}, "flip": true, "angle": 3.0},
            {"name": "Draft2", "type": "draft", "bodies": ["F2.b0"], "pull": null, "angle": 1.0}
        ])
    );
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(analyses(&loaded), analyses(&doc));
    assert_eq!(
        analyses(&loaded)[0]["pull_direction"],
        json!([0.0, 0.0, -1.0])
    );
    let unknown = saved.replace("\"angle\": 3.0", "\"draft\": 3.0");
    assert_ne!(unknown, saved);
    let Err(error) = Document::from_json(&unknown, MockKernel::default()) else {
        panic!("an unknown field was read");
    };
    assert!(error.to_string().contains("analyses[0]"), "{error}");
}

#[test]
fn the_query_classes_the_box_by_its_draft() {
    let doc = block(20.0, 20.0, 20.0);
    // Pulled along +Z: the top releases, the four sides need draft, the
    // bottom releases only downward.
    let answer = query(
        &doc,
        json!({"query": "draft_analysis", "bodies": ["F2.b0"]}),
    );
    assert_eq!(answer["pull"], json!([0.0, 0.0, 1.0]));
    assert_eq!(answer["angle"], 1.0);
    assert!(close(&answer["area"], 2400.0), "{answer}");
    assert!(close(&answer["positive_area"], 400.0), "{answer}");
    assert!(close(&answer["under_area"], 1600.0), "{answer}");
    assert!(close(&answer["negative_area"], 400.0), "{answer}");
    assert!(close(&answer["under_share"], 2.0 / 3.0), "{answer}");
    let body = &answer["bodies"][0];
    assert_eq!(body["body"], "F2.b0");
    assert!(body["name"].is_string());
    assert!(close(&body["positive_share"], 1.0 / 6.0), "{body}");
    let count = |class: &str| body["triangles"][class].as_array().unwrap().len();
    assert_eq!(
        (count("positive"), count("under"), count("negative")),
        (2, 8, 2)
    );
    for triangle in body["triangles"]["positive"].as_array().unwrap() {
        let numbers = triangle.as_array().unwrap();
        assert_eq!(numbers.len(), 9);
        for z in [2, 5, 8] {
            assert!(close(&numbers[z], 20.0), "{triangle}");
        }
    }

    // Flipped: the classes trade places. Along X: the +X side releases.
    let flipped = query(
        &doc,
        json!({"query": "draft_analysis", "bodies": ["F2.b0"], "flip": true, "triangles": false}),
    );
    assert_eq!(flipped["pull"], json!([0.0, 0.0, -1.0]));
    assert!(close(&flipped["positive_area"], 400.0));
    assert!(flipped["bodies"][0].get("triangles").is_none(), "{flipped}");
    let along_x = query(&doc, json!({"query": "draft_analysis", "pull": "x"}));
    assert_eq!(along_x["pull"], json!([1.0, 0.0, 0.0]));
    assert!(close(&along_x["positive_area"], 400.0));
    assert!(close(&along_x["under_area"], 1600.0));

    // Inputs out of range are refused as other queries' are.
    let error = query_error(&doc, json!({"query": "draft_analysis", "angle": 90}));
    assert!(error.contains("draft angle"), "{error}");
    let error = query_error(&doc, json!({"query": "draft_analysis", "deviation": 0}));
    assert!(error.contains("deviation"), "{error}");
    let error = query_error(&doc, json!({"query": "draft_analysis", "angel": 1}));
    assert!(error.contains("angel"), "{error}");
}
