// SPDX-License-Identifier: MIT
//! Revolve with a planar body face as the profile (mitcad#539), with the
//! mock kernel: what reaches the kernel (the face as found, its key, the
//! axis and the angles), Join and New Body, the face followed through an
//! upstream edit and a saved design, and the refusals: a face that is gone
//! or not planar fails the feature without another profile taken, and
//! options a face's revolution has not are refused.

use serde_json::{Value, json};

use crate::datum::SurfaceGeometry;
use crate::features::SketchPlane;
use crate::ids::{BodyUid, FeatureUid};
use crate::test_support::{block, def};
use crate::testing::MockKernel;
use crate::{Document, FeatureStatus};

/// The block's side at y = 0 (line c1 of its sketch, from (0, 0) to
/// (60, 0)): a face in the XZ plane with an edge on the X axis.
const SIDE: &str = "F2:side(c1[c4,c2])";

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn error(doc: &Document<MockKernel>, uid: FeatureUid) -> String {
    match status(doc, uid) {
        FeatureStatus::Failed(message) => message.agent_text().to_owned(),
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn last_spec(doc: &Document<MockKernel>) -> String {
    doc.kernel()
        .specs
        .borrow()
        .last()
        .cloned()
        .unwrap_or_default()
}

fn history(doc: &Document<MockKernel>, body: BodyUid) -> String {
    doc.body_shape(body)
        .map_or_else(|| "none".to_owned(), |s| s.history.clone())
}

fn faces(doc: &Document<MockKernel>, body: BodyUid) -> Vec<String> {
    doc.body_shape(body)
        .map(|s| s.faces.iter().map(ToString::to_string).collect())
        .unwrap_or_default()
}

/// A revolution of `face` of `body` about the X axis.
fn face_revolution(body: BodyUid, face: &str, extent: Value, operation: &str) -> Value {
    json!({"type": "revolve", "profile_face": {"body": body, "face": face}, "axis": "x",
           "extent": extent, "operation": operation})
}

fn angle(a: f64) -> Value {
    json!({"type": "angle", "angle": a})
}

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None).unwrap().uid
}

#[test]
fn a_box_side_turns_about_an_axis_without_a_sketch_and_joins() {
    let mut b = block();
    let uid = add(
        &mut b.doc,
        face_revolution(b.body, SIDE, angle(1.0), "join"),
    );
    assert_eq!(uid, FeatureUid(3));
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!(
            "revolve_face F3 {SIDE} key {SIDE} axis [0.0, 0.0, 0.0] [1.0, 0.0, 0.0] angles 1 None"
        )
    );
    assert_eq!(
        history(&b.doc, b.body),
        format!("join(prism(F2:0..20),face_revolution(F3:{SIDE}:1))")
    );
    // The caps are keyed by the face, the sides by the face's edges.
    let names = faces(&b.doc, b.body);
    for name in [format!("F3:start({SIDE})"), format!("F3:end({SIDE})")] {
        assert!(names.contains(&name), "{name} not in {names:?}");
    }
    let sides = names
        .iter()
        .filter(|n| n.starts_with("F3:side(E{") && n.contains(SIDE))
        .count();
    assert_eq!(sides, 4, "{names:?}");
    assert_eq!(b.doc.kernel().count("revolve_face"), 1);
    assert_eq!(
        b.doc.kernel().count("revolve"),
        0,
        "no sketch region turned"
    );
}

#[test]
fn angles_turn_as_a_sketch_profile_s_do() {
    let mut b = block();
    let new_body = BodyUid::new(FeatureUid(3), 0);
    let uid = add(
        &mut b.doc,
        face_revolution(b.body, SIDE, json!({"type": "full"}), "new_body"),
    );
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert!(
        last_spec(&b.doc).ends_with("angles 6.283185307179586 None"),
        "{}",
        last_spec(&b.doc)
    );
    // A full turn has no caps.
    let names = faces(&b.doc, new_body);
    assert!(
        !names.iter().any(|n| n.starts_with("F3:start(")),
        "{names:?}"
    );
    let cases = [
        (
            json!({"type": "symmetric", "angle": 0.5}),
            "[1.0, 0.0, 0.0] angles 0.5 Some(0.5)",
        ),
        // A negative angle turns about the reversed axis.
        (angle(-1.0), "[-1.0, -0.0, -0.0] angles 1 None"),
        (
            json!({"type": "two_sides", "angle1": 0.25, "angle2": 0.5}),
            "[1.0, 0.0, 0.0] angles 0.25 Some(0.5)",
        ),
        // A side of nothing leaves the other side alone.
        (
            json!({"type": "two_sides", "angle1": 0, "angle2": 0.5}),
            "[-1.0, -0.0, -0.0] angles 0.5 None",
        ),
    ];
    for (extent, expected) in cases {
        b.doc
            .edit_feature(uid, &def(face_revolution(b.body, SIDE, extent, "new_body")))
            .unwrap();
        assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
        let spec = last_spec(&b.doc);
        assert!(spec.ends_with(expected), "{expected}: {spec}");
    }
    assert_eq!(
        history(&b.doc, new_body),
        format!("face_revolution(F3:{SIDE}:0.5)")
    );
    // The block stays as it was.
    assert_eq!(history(&b.doc, b.body), "prism(F2:0..20)");
}

#[test]
fn the_face_reference_follows_an_edit_and_a_saved_design() {
    let mut b = block();
    let before = b.doc.to_json();
    assert!(!before.contains("profile_face"), "{before}");
    let uid = add(
        &mut b.doc,
        face_revolution(b.body, SIDE, angle(1.0), "join"),
    );
    // The block 30 deep: the face is found where it is now.
    b.doc.set_parameter("d3", 30.0).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        history(&b.doc, b.body),
        format!("join(prism(F2:0..30),face_revolution(F3:{SIDE}:1))")
    );
    let saved = b.doc.to_json();
    assert!(saved.contains("\"profile_face\""), "{saved}");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(status(&loaded, uid), FeatureStatus::Ok);
    assert_eq!(history(&loaded, b.body), history(&b.doc, b.body));
}

#[test]
fn a_face_that_is_gone_or_not_planar_fails_without_taking_another_profile() {
    let mut b = block();
    // An unused sketch profile beside it, which must not stand in.
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    b.doc
        .add_rectangle(
            sketch,
            [100.0, 0.0],
            &crate::test_support::num(10.0),
            &crate::test_support::num(10.0),
        )
        .unwrap();
    let uid = add(
        &mut b.doc,
        face_revolution(b.body, "F2:top", angle(1.0), "join"),
    );
    let why = error(&b.doc, uid);
    assert!(why.contains("F2:top"), "{why}");
    assert_eq!(b.doc.kernel().count("revolve_face"), 0);
    assert_eq!(b.doc.kernel().count("revolve"), 0);
    assert_eq!(history(&b.doc, b.body), "prism(F2:0..20)");

    b.doc.kernel().surfaces.borrow_mut().insert(
        SIDE.to_owned(),
        SurfaceGeometry::Cylinder {
            origin: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 5.0,
        },
    );
    b.doc
        .edit_feature(uid, &def(face_revolution(b.body, SIDE, angle(1.0), "join")))
        .unwrap();
    let why = error(&b.doc, uid);
    assert!(why.contains("is not planar"), "{why}");
    assert_eq!(history(&b.doc, b.body), "prism(F2:0..20)");
}

#[test]
fn options_a_face_revolution_has_not_are_refused() {
    let mut b = block();
    let refused = |doc: &mut Document<MockKernel>, value: Value, text: &str| {
        let why = doc.add_feature(&def(value), None).unwrap_err().to_string();
        assert!(why.contains(text), "{text}: {why}");
    };
    let base = face_revolution(b.body, SIDE, angle(1.0), "join");
    let mut with_profiles = base.clone();
    with_profiles["profiles"] = json!([{"sketch": b.sketch, "region": b.region}]);
    refused(&mut b.doc, with_profiles, "takes no sketch profiles");
    let mut surface = base.clone();
    surface["surface"] = json!(true);
    refused(&mut b.doc, surface, "no surface or curves");
    let mut projected = base.clone();
    projected["project_axis"] = json!(true);
    refused(&mut b.doc, projected, "does not project its axis");
    let up_to = face_revolution(
        b.body,
        SIDE,
        json!({"type": "to_object", "object": {"type": "plane", "plane": "yz"}}),
        "join",
    );
    refused(&mut b.doc, up_to, "turns by angles");
    // A body that is not there.
    let missing = face_revolution(BodyUid::new(FeatureUid(9), 0), "F9:top", angle(1.0), "join");
    assert!(b.doc.add_feature(&def(missing), None).is_err());
}
