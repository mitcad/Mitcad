// SPDX-License-Identifier: MIT
//! Mortise and Tenon (mitcad#1012) in documents with the mock kernel: what
//! reaches the kernel, both bodies changed under their ids, the refusals
//! and a size edit recomputing both. The volumes are checked with OCCT in
//! `cli.mortise_tenon` (`tools/cli/tests/mortise_tenon.json`).

use serde_json::{Value, json};

use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Curve3;
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, ModelError};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn add(doc: &mut Document<MockKernel>, value: Value) -> Result<FeatureUid, ModelError> {
    doc.add_feature(&def(value), None).map(|added| added.uid)
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn error(doc: &Document<MockKernel>, uid: FeatureUid) -> String {
    match status(doc, uid) {
        FeatureStatus::Failed(message) => message,
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn line(start: [f64; 3], end: [f64; 3]) -> Curve3 {
    Curve3::Line { start, end }
}

fn body(text: &str) -> BodyUid {
    text.parse().unwrap()
}

/// Two crossing boards: the rail Box1 (F1, F1.b0) 100 long to x = 0, 20
/// across and 60 high, its end face F1:side1 at x = 0; the post Box2 (F2,
/// F2.b0) 40 x 40 x 100 from x = 0 on, across the rail's end. The kernel's
/// box faces lie on their bounds; the rail's end face has the edges of its
/// 20 x 60 rectangle.
fn boards() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_faces.set(true);
    *doc.kernel().curves.borrow_mut() = vec![
        line([0.0, 0.0, 0.0], [0.0, 20.0, 0.0]),
        line([0.0, 20.0, 0.0], [0.0, 20.0, 60.0]),
        line([0.0, 20.0, 60.0], [0.0, 0.0, 60.0]),
        line([0.0, 0.0, 60.0], [0.0, 0.0, 0.0]),
    ];
    for (corner, size) in [([-100, 0], [100, 20, 60]), ([0, -10], [40, 40, 100])] {
        add(
            &mut doc,
            json!({"type": "box", "plane": {"type": "origin", "plane": "xy"}, "corner": corner,
                   "length": size[0], "width": size[1], "height": size[2],
                   "operation": "new_body"}),
        )
        .unwrap();
    }
    doc
}

/// A tenon 40 wide (along the rail's height), 8 thick and 25 long on the
/// rail's end, its mortise in the post, with `more` fields.
fn joint(more: Value) -> Value {
    let mut def = json!({"type": "mortise_tenon", "body": "F1.b0", "face": "F1:side1",
                         "mortise_body": "F2.b0", "width": 40, "thickness": 8, "length": 25});
    for (key, value) in more.as_object().unwrap() {
        def[key] = value.clone();
    }
    def
}

/// The extrusions that reached the kernel: (start offset, distance) each.
fn extrusions(doc: &Document<MockKernel>) -> Vec<(f64, f64)> {
    let number = |text: &str, after: &str, until: &str| -> f64 {
        let from = text.find(after).unwrap() + after.len();
        let to = from + text[from..].find(until).unwrap();
        text[from..to].parse().unwrap()
    };
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .map(|spec| {
            assert!(
                spec.contains("along [1.0, 0.0, 0.0]"),
                "out of the rail's end: {spec}"
            );
            (
                number(spec, "from offset ", ":"),
                number(spec, "distance ", " taper"),
            )
        })
        .collect()
}

fn assert_extrusions(doc: &Document<MockKernel>, expected: &[(f64, f64)]) {
    let got = extrusions(doc);
    assert_eq!(got.len(), expected.len(), "{got:?}");
    for (g, e) in got.iter().zip(expected) {
        assert!(
            (g.0 - e.0).abs() < 1e-9 && (g.1 - e.1).abs() < 1e-9,
            "{got:?} against {expected:?}"
        );
    }
}

fn history(doc: &Document<MockKernel>, uid: &str) -> String {
    doc.body_shape(body(uid)).unwrap().history.clone()
}

fn has_face(doc: &Document<MockKernel>, uid: &str, face: &str) -> bool {
    doc.body_shape(body(uid))
        .unwrap()
        .faces
        .iter()
        .any(|f| f.to_string() == face)
}

fn body_names(doc: &Document<MockKernel>) -> Vec<(String, String)> {
    query(doc, json!({"query": "bodies"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["uid"].as_str().unwrap().to_owned(),
                b["name"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn a_blind_tenon_joins_its_body_and_its_mortise_cuts_the_other() {
    let mut doc = boards();
    let names = body_names(&doc);
    let uid = add(&mut doc, joint(json!({}))).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    // The mortise from the end face, 0.1 past the tenon's tip; the tenon
    // from 0.5 inside the rail (5 % of its length, at most 0.5).
    assert_extrusions(&doc, &[(0.0, 25.1), (-0.5, 25.5)]);
    // Both bodies changed under their ids and names.
    assert!(history(&doc, "F1.b0").starts_with("join(box(F1"));
    assert!(history(&doc, "F2.b0").starts_with("cut(box(F2"));
    assert_eq!(body_names(&doc), names);
    // The tenon's sides and tip on the rail, the mortise's walls in the
    // post.
    for face in ["F3:side(c1[c4,c2])", "F3:side(c3[c2,c4])"] {
        assert!(has_face(&doc, "F1.b0", face), "{face}");
    }
    for face in ["F3:side(c11[c14,c12])", "F3:side(c13[c12,c14])"] {
        assert!(has_face(&doc, "F2.b0", face), "{face}");
    }
    // The tenon stands 25 out of the rail's end: 40 along its height, 8
    // across its 20 mm, centred.
    let bounds = doc.body_shape(body("F1.b0")).unwrap().bounds.unwrap();
    assert!((bounds.max[0] - 25.0).abs() < 1e-9, "{bounds:?}");
    // The definition as it is stored: the defaults left out.
    let stored = query(&doc, json!({"query": "feature", "uid": uid}));
    assert_eq!(stored["def"]["type"], "mortise_tenon", "{stored}");
    for key in ["through", "turned", "clearance", "corners", "tool_diameter"] {
        assert!(stored["def"].get(key).is_none(), "{key} in {stored}");
    }
}

#[test]
fn a_through_tenon_reaches_the_far_face_of_the_post() {
    let mut doc = boards();
    let mut through = joint(json!({"through": true}));
    through.as_object_mut().unwrap().remove("length");
    let uid = add(&mut doc, through).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    // 40 through the post; the mortise runs on 0.1 + 1 past it.
    assert_extrusions(&doc, &[(0.0, 41.1), (-0.5, 40.5)]);
    let bounds = doc.body_shape(body("F1.b0")).unwrap().bounds.unwrap();
    assert!((bounds.max[0] - 40.0).abs() < 1e-9, "{bounds:?}");
}

#[test]
fn rounded_corners_round_the_mortise_and_the_tenon_to_match() {
    let mut doc = boards();
    let uid = add(
        &mut doc,
        joint(json!({"corners": "rounded", "tool_diameter": 6, "clearance": 0.2})),
    )
    .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_extrusions(&doc, &[(0.0, 25.2), (-0.5, 25.5)]);
    // Lines and quarter rounds round both: the tenon's c1 … c8, the
    // mortise's c11 … c18.
    for face in [
        "F3:side(c1[c8,c2])",
        "F3:side(c2[c1,c3])",
        "F3:side(c8[c7,c1])",
    ] {
        assert!(has_face(&doc, "F1.b0", face), "{face}");
    }
    for face in ["F3:side(c12[c11,c13])", "F3:side(c18[c17,c11])"] {
        assert!(has_face(&doc, "F2.b0", face), "{face}");
    }
}

#[test]
fn a_turned_tenon_runs_its_width_across_the_longest_edge() {
    let mut doc = boards();
    let wide = add(&mut doc, joint(json!({"turned": true}))).unwrap();
    assert_eq!(
        error(&doc, wide),
        "the tenon (40 x 8 mm) is larger than the end face (20 x 60 mm)"
    );
    let mut doc = boards();
    let turned = add(
        &mut doc,
        joint(json!({"turned": true, "width": 16, "thickness": 30})),
    )
    .unwrap();
    assert_eq!(status(&doc, turned), FeatureStatus::Ok);
}

#[test]
fn a_tenon_larger_than_the_face_or_off_it_is_refused() {
    let mut doc = boards();
    let large = add(&mut doc, joint(json!({"width": 70}))).unwrap();
    assert_eq!(
        error(&doc, large),
        "the tenon (70 x 8 mm) is larger than the end face (60 x 20 mm)"
    );
    let off = add(&mut doc, joint(json!({"offset_width": 15}))).unwrap();
    assert_eq!(
        error(&doc, off),
        "the tenon's offset takes it past the end face's edge (shoulders 25, -5, 6, 6 mm)"
    );
    // Offset as far as the face's edges: shoulders of 0 on two sides.
    let fits = add(
        &mut doc,
        joint(json!({"offset_width": 10, "offset_thickness": -6})),
    )
    .unwrap();
    assert_eq!(status(&doc, fits), FeatureStatus::Ok);
}

#[test]
fn a_tenon_that_does_not_reach_the_other_body_is_refused() {
    // Blind: the mortise touches nothing.
    let mut doc = boards();
    *doc.kernel().touch.borrow_mut() = Some(vec![false]);
    let blind = add(&mut doc, joint(json!({}))).unwrap();
    assert_eq!(error(&doc, blind), "the tenon does not reach body F2.b0");
    // Through: nothing of a block beside the rail lies ahead of it.
    let mut doc = boards();
    add(
        &mut doc,
        json!({"type": "box", "plane": {"type": "origin", "plane": "xy"}, "corner": [0, 200],
               "length": 40, "width": 40, "height": 100, "operation": "new_body"}),
    )
    .unwrap();
    let mut through = joint(json!({"through": true, "mortise_body": "F3.b0"}));
    through.as_object_mut().unwrap().remove("length");
    let uid = add(&mut doc, through).unwrap();
    assert_eq!(error(&doc, uid), "the tenon does not reach body F3.b0");
}

#[test]
fn inconsistent_definitions_are_refused_before_they_are_added() {
    let mut doc = boards();
    let refused = |doc: &mut Document<MockKernel>, more: Value| -> String {
        add(doc, joint(more)).unwrap_err().to_string()
    };
    let same = refused(&mut doc, json!({"mortise_body": "F1.b0"}));
    assert!(
        same.contains("the mortise must go into another body than the tenon's"),
        "{same}"
    );
    let zero = refused(&mut doc, json!({"width": 0}));
    assert!(
        zero.contains("tenon's width must be greater than zero"),
        "{zero}"
    );
    let tool = refused(&mut doc, json!({"corners": "rounded"}));
    assert!(
        tool.contains("rounded corners need a tool diameter"),
        "{tool}"
    );
    let large = refused(&mut doc, json!({"corners": "rounded", "tool_diameter": 9}));
    assert!(
        large.contains(
            "the tool diameter (9 mm) must be smaller than the mortise's thickness (8.2 mm)"
        ),
        "{large}"
    );
    let mut blind = joint(json!({}));
    blind.as_object_mut().unwrap().remove("length");
    let length = add(&mut doc, blind).unwrap_err().to_string();
    assert!(
        length.contains("a tenon that is not through needs a length"),
        "{length}"
    );
}

#[test]
fn editing_a_size_recomputes_both_bodies() {
    let mut doc = boards();
    let uid = add(&mut doc, joint(json!({}))).unwrap();
    let parameters = query(&doc, json!({"query": "parameters"}));
    let length = parameters
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["comment"] == "MortiseTenon1 length")
        .expect("the length's parameter")["name"]
        .as_str()
        .unwrap()
        .to_owned();
    doc.kernel().specs.borrow_mut().clear();
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": length, "value": 30}),
    );
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_extrusions(&doc, &[(0.0, 30.1), (-0.5, 30.5)]);
    assert!(history(&doc, "F1.b0").starts_with("join(box(F1"));
    assert!(history(&doc, "F2.b0").starts_with("cut(box(F2"));
    let bounds = doc.body_shape(body("F1.b0")).unwrap().bounds.unwrap();
    assert!((bounds.max[0] - 30.0).abs() < 1e-9, "{bounds:?}");
}
