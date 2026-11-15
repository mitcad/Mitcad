// SPDX-License-Identifier: MIT
//! Press Pull continued Straight or Along the walls, tapered (mitcad#1850),
//! in documents with the mock kernel: the feature's fields as they reach
//! the kernel and come back from a file, its edit and undo, the moved end's
//! names after a parameter change, the checks of its values, and the
//! `face_run` query with the feature-end case Press Pull opens instead.

use serde_json::{Value, json};

use mitcad_model::test_support::{Block, block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{FaceName, RoleKey};
use mitcad_model::{Document, FeatureStatus, ModelError};
use mitcad_model_api::DocumentApi;

use mitcad_model::ids::{BodyUid, FeatureUid};

fn add(doc: &mut Document<MockKernel>, value: Value) -> Result<FeatureUid, ModelError> {
    doc.add_feature(&def(value), None).map(|added| added.uid)
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn last_press_pull(doc: &Document<MockKernel>) -> String {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .rev()
        .find(|s| s.starts_with("press_pull "))
        .cloned()
        .expect("a press pull reached the kernel")
}

fn top(b: &Block) -> FaceName {
    FaceName::end(b.extrude, b.region.clone())
}

/// The moved end of press pull `uid` on face `face`.
fn moved_end(uid: FeatureUid, face: &FaceName) -> FaceName {
    FaceName::new(uid, "end", Some(RoleKey::Face(Box::new(face.clone()))))
}

fn query(doc: &Document<MockKernel>, query: Value) -> Result<Value, String> {
    doc.query(&query.to_string())
        .map(|text| serde_json::from_str(&text).unwrap())
        .map_err(|e| e.to_string())
}

fn parameter(doc: &Document<MockKernel>, comment: &str) -> String {
    doc.parameters()
        .iter()
        .find(|p| p.comment() == comment)
        .unwrap_or_else(|| panic!("no parameter {comment}"))
        .name()
        .to_owned()
}

#[test]
fn a_press_pull_reaches_the_kernel_with_its_fields_and_comes_back_from_a_file() {
    let mut b = block();
    let face = top(&b);
    let pull = add(
        &mut b.doc,
        json!({"type": "press_pull", "body": b.body, "faces": [face], "continue": "along",
               "distance": 5.0, "taper_type": "draft", "taper_angle": "3 deg"}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, pull), FeatureStatus::Ok);
    let spec = last_press_pull(&b.doc);
    assert!(
        spec.starts_with(&format!("press_pull {pull} {face} along 5 draft 0.05")),
        "{spec}"
    );
    // The new walls and the moved end; the face keeps its name.
    let faces = &b.doc.body_shape(b.body).unwrap().faces;
    assert!(faces.contains(&moved_end(pull, &face)), "{faces:?}");
    assert!(faces.contains(&face));
    assert!(
        faces.iter().any(|f| f.feature == pull && f.role == "side"),
        "{faces:?}"
    );
    // Its values are parameters of their own, the taper an angle.
    let angle = parameter(&b.doc, "PressPull1 taper_angle");
    let distance = parameter(&b.doc, "PressPull1 distance");
    assert_ne!(angle, distance);
    // A file keeps the definition: `continue`, the taper type and angle.
    let saved = b.doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let text = file.to_string();
    assert!(text.contains("\"continue\":\"along\""), "{text}");
    assert!(text.contains("\"taper_type\":\"draft\""), "{text}");
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.state(), b.doc.state());
    assert_eq!(loaded.to_json(), saved);
}

#[test]
fn a_press_pull_is_edited_and_undone() {
    let mut b = block();
    let face = top(&b);
    let pull = add(
        &mut b.doc,
        json!({"type": "press_pull", "body": b.body, "faces": [face], "continue": "straight",
               "distance": 5.0}),
    )
    .unwrap();
    assert!(last_press_pull(&b.doc).ends_with("straight 5"));
    // Straight without a taper to Along with a scale to 110 %.
    b.doc
        .edit_feature(
            pull,
            &def(
                json!({"type": "press_pull", "body": b.body, "faces": [face], "continue": "along",
                        "distance": 8.0, "taper_type": "scale", "end_scale": 1.1}),
            ),
        )
        .unwrap();
    assert_eq!(status(&b.doc, pull), FeatureStatus::Ok);
    assert!(
        last_press_pull(&b.doc).ends_with("along 8 scale 1.1"),
        "{}",
        last_press_pull(&b.doc)
    );
    let history = b.history(b.body);
    assert!(history.starts_with("press_pull(prism("), "{history}");
    assert!(history.ends_with(",along,8 scale 1.1)"), "{history}");
    // Undone: as it was.
    b.doc.undo();
    assert!(
        b.history(b.body).ends_with(",straight,5)"),
        "{}",
        b.history(b.body)
    );
    // Into the material: a cut, recorded with its sign.
    b.doc
        .edit_feature(
            pull,
            &def(json!({"type": "press_pull", "body": b.body, "faces": [face], "continue": "straight",
                        "distance": -4.0, "taper_type": "draft", "taper_angle": "-2 deg"})),
        )
        .unwrap();
    assert_eq!(status(&b.doc, pull), FeatureStatus::Ok);
    assert!(last_press_pull(&b.doc).contains("straight -4 draft -0.03"));
}

#[test]
fn the_moved_end_keeps_its_names_when_the_distance_changes() {
    let mut b = block();
    let face = top(&b);
    let pull = add(
        &mut b.doc,
        json!({"type": "press_pull", "body": b.body, "faces": [face], "continue": "along",
               "distance": 5.0}),
    )
    .unwrap();
    // A later feature on the moved end, by the press pull's name for it.
    let end = moved_end(pull, &face);
    let offset = add(
        &mut b.doc,
        json!({"type": "offset_face", "body": b.body, "faces": [end], "distance": 1.0}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, offset), FeatureStatus::Ok);
    let distance = parameter(&b.doc, "PressPull1 distance");
    b.doc.set_parameter(&distance, 12.0).unwrap();
    assert_eq!(status(&b.doc, pull), FeatureStatus::Ok);
    assert_eq!(status(&b.doc, offset), FeatureStatus::Ok);
    assert!(last_press_pull(&b.doc).ends_with("along 12"));
    let faces = &b.doc.body_shape(b.body).unwrap().faces;
    assert!(faces.contains(&end) && faces.contains(&face), "{faces:?}");
}

#[test]
fn the_values_and_fields_are_checked() {
    let mut b = block();
    let face = top(&b);
    for (fields, error) in [
        (json!({"distance": 0.0}), "must not be zero"),
        (
            json!({"distance": 5.0, "taper_type": "draft"}),
            "a draft taper needs its taper angle",
        ),
        (
            json!({"distance": 5.0, "taper_type": "draft", "taper_angle": "90 deg"}),
            "the taper angle must be between -85 and 85 degrees",
        ),
        (
            json!({"distance": 5.0, "taper_type": "scale"}),
            "a scale taper needs its end scale",
        ),
        (
            json!({"distance": 5.0, "taper_type": "scale", "end_scale": 0.0}),
            "the end scale must be more than 0 %",
        ),
    ] {
        let mut value = json!({"type": "press_pull", "body": b.body, "faces": [face],
                               "continue": "straight"});
        for (key, field) in fields.as_object().unwrap() {
            value[key.as_str()] = field.clone();
        }
        let message = match add(&mut b.doc, value.clone()) {
            Err(e) => e.to_string(),
            Ok(uid) => match status(&b.doc, uid) {
                FeatureStatus::Failed(m) => {
                    b.doc.delete_feature(uid, false).unwrap();
                    m.agent_text().to_owned()
                }
                other => panic!("{value}: expected a refusal, got {other:?}"),
            },
        };
        assert!(message.contains(error), "{value}: {message}");
    }
    // Walls that continue neither way: Along fails in words, Straight works.
    b.doc.kernel().fail.borrow_mut().insert("press_pull_along");
    let along = add(
        &mut b.doc,
        json!({"type": "press_pull", "body": b.body, "faces": [face], "continue": "along",
               "distance": 5.0}),
    )
    .unwrap();
    let FeatureStatus::Failed(message) = status(&b.doc, along) else {
        panic!("expected a failure");
    };
    assert_eq!(
        message.message,
        "the walls do not continue as a turn or a straight run; use Offset or Straight"
    );
    b.doc.delete_feature(along, false).unwrap();
    let straight = add(
        &mut b.doc,
        json!({"type": "press_pull", "body": b.body, "faces": [face], "continue": "straight",
               "distance": 5.0}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, straight), FeatureStatus::Ok);
}

#[test]
fn face_run_tells_how_the_walls_continue_and_which_feature_ends_there() {
    let b = block();
    let face = top(&b);
    let run = query(
        &b.doc,
        json!({"query": "face_run", "body": b.body, "face": face}),
    )
    .unwrap();
    assert_eq!(run["along"]["kind"], "straight", "{run}");
    assert_eq!(run["along"]["direction"], json!([0.0, 0.0, 1.0]));
    assert_eq!(run["normal"], json!([0.0, 0.0, 1.0]));
    assert!(run["reach"].as_f64().unwrap() > 0.0);
    // The top is Extrude1's moving end: Press Pull may open its edit at
    // its Distance.
    assert_eq!(run["feature_end"]["uid"], json!(b.extrude.to_string()));
    assert_eq!(run["feature_end"]["name"], "Extrude1");
    assert_eq!(run["feature_end"]["type"], "extrude");
    assert_eq!(run["feature_end"]["input"], "distance");
    // A one-sided extrusion's start is no moving end.
    let start = FaceName::start(b.extrude, b.region.clone());
    let run = query(
        &b.doc,
        json!({"query": "face_run", "body": b.body, "face": start}),
    )
    .unwrap();
    assert_eq!(run["feature_end"], Value::Null, "{run}");
    // Walls that continue neither way: the refusal in words.
    b.doc.kernel().fail.borrow_mut().insert("press_pull_along");
    let run = query(
        &b.doc,
        json!({"query": "face_run", "body": b.body, "face": face}),
    )
    .unwrap();
    assert_eq!(run["along"]["kind"], "none");
    assert_eq!(
        run["along"]["refusal"]["message"],
        "the walls do not continue as a turn or a straight run; use Offset or Straight"
    );
    // A face the body lacks.
    assert!(
        query(
            &b.doc,
            json!({"query": "face_run", "body": b.body, "face": "F2:top"}),
        )
        .is_err()
    );
}

#[test]
fn face_run_opens_a_revolves_angle_at_its_end() {
    let mut b = block();
    // Revolve1 (F3): the block's sketch turned a quarter about the X axis.
    let revolve = add(
        &mut b.doc,
        json!({"type": "revolve", "profiles": [{"sketch": b.sketch, "region": b.region}],
               "axis": "x", "extent": {"type": "angle", "angle": "90 deg"},
               "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, revolve), FeatureStatus::Ok);
    let body = BodyUid::new(revolve, 0);
    // Its far end moves by its Angle: Press Pull opens Revolve1 there.
    let end = FaceName::end(revolve, b.region.clone());
    let run = query(
        &b.doc,
        json!({"query": "face_run", "body": body, "face": end}),
    )
    .unwrap();
    assert_eq!(
        run["feature_end"]["uid"],
        json!(revolve.to_string()),
        "{run}"
    );
    assert_eq!(run["feature_end"]["name"], "Revolve1");
    assert_eq!(run["feature_end"]["type"], "revolve");
    assert_eq!(run["feature_end"]["input"], "angle");
    // A one-sided revolution's start stays where the profile is.
    let start = FaceName::start(revolve, b.region.clone());
    let run = query(
        &b.doc,
        json!({"query": "face_run", "body": body, "face": start}),
    )
    .unwrap();
    assert_eq!(run["feature_end"], Value::Null, "{run}");
}
