// SPDX-License-Identifier: MIT
//! Wood Joint (mitcad#2604) in documents with the mock kernel: what reaches
//! the kernel for a grown joint (a dado), a through one (a box joint) and
//! loose pieces (a loose tenon, dowels), the parts changed under their ids,
//! the loose pieces new bodies, the refusals and a size edit. The volumes
//! are checked with OCCT in `cli.wood_joint`
//! (`tools/cli/tests/wood_joint.json`).

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
        FeatureStatus::Failed(message) => message.agent_text().to_owned(),
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn line(start: [f64; 3], end: [f64; 3]) -> Curve3 {
    Curve3::Line { start, end }
}

fn body(text: &str) -> BodyUid {
    text.parse().unwrap()
}

/// Mortise and Tenon's boards: the rail Box1 (F1.b0) 100 long to x = 0,
/// 20 across and 60 high, its end face F1:side1 at x = 0; the post Box2
/// (F2.b0) 40 x 40 x 100 from x = 0 on, across the rail's end.
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

/// A joint of `kind` from the rail's end into the post, with `more`
/// fields.
fn joint(kind: &str, more: Value) -> Value {
    let mut def = json!({"type": "wood_joint", "kind": kind, "body": "F1.b0", "face": "F1:side1",
                         "other": "F2.b0"});
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

fn body_uids(doc: &Document<MockKernel>) -> Vec<String> {
    query(doc, json!({"query": "bodies"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["uid"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_dado_grows_the_whole_end_and_cuts_its_housing() {
    let mut doc = boards();
    let uid = add(&mut doc, joint("dado", json!({"depth": 6}))).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    // The housing from the end face, 0.1 past the end; the end from 0.3
    // inside the rail (5 % of the depth).
    assert_extrusions(&doc, &[(0.0, 6.1), (-0.3, 6.3)]);
    assert!(history(&doc, "F1.b0").starts_with("join(box(F1"));
    assert!(history(&doc, "F2.b0").starts_with("cut(box(F2"));
    assert_eq!(body_uids(&doc), ["F1.b0", "F2.b0"]);
    // The rail's whole 20 x 60 end, 6 out.
    let bounds = doc.body_shape(body("F1.b0")).unwrap().bounds.unwrap();
    assert!((bounds.max[0] - 6.0).abs() < 1e-9, "{bounds:?}");
    // Stored as given: the defaults left out.
    let stored = query(&doc, json!({"query": "feature", "uid": uid}));
    assert_eq!(stored["def"]["type"], "wood_joint", "{stored}");
    assert_eq!(stored["def"]["kind"], "dado", "{stored}");
    for key in ["clearance", "flip", "count", "body_depth"] {
        assert!(stored["def"].get(key).is_none(), "{key} in {stored}");
    }
    assert_eq!(stored["name"], "WoodJoint1", "{stored}");
}

#[test]
fn a_box_joint_takes_its_fingers_through_the_other_part() {
    let mut doc = boards();
    let uid = add(&mut doc, joint("box_joint", json!({}))).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    // Two fingers of three, through the post's 40: their slots run 0.1
    // past it and 1 more, the fingers from 0.5 inside the rail; both of
    // one length, extruded together.
    assert_extrusions(&doc, &[(0.0, 41.1), (-0.5, 40.5)]);
    let bounds = doc.body_shape(body("F1.b0")).unwrap().bounds.unwrap();
    assert!((bounds.max[0] - 40.0).abs() < 1e-9, "{bounds:?}");
}

#[test]
fn loose_pieces_are_new_bodies_in_holes_of_both_parts() {
    let mut doc = boards();
    let uid = add(
        &mut doc,
        joint(
            "loose_tenon",
            json!({"width": 40, "thickness": 8, "depth": 20, "body_depth": 15}),
        ),
    )
    .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    // One hole through both parts from 15.1 inside the rail to 20.1 into
    // the post; the tenon from 15 inside the rail to 20 into the post.
    assert_extrusions(&doc, &[(-15.1, 35.2), (-15.0, 35.0)]);
    assert!(history(&doc, "F1.b0").starts_with("cut(box(F1"));
    assert!(history(&doc, "F2.b0").starts_with("cut(box(F2"));
    assert_eq!(body_uids(&doc), ["F1.b0", "F2.b0", "F3.b0"]);

    // Three dowels: three holes extruded together, each dowel apart.
    let mut doc = boards();
    let uid = add(
        &mut doc,
        joint(
            "dowel",
            json!({"diameter": 8, "depth": 20, "count": 3, "start_offset": 5, "end_offset": 5}),
        ),
    )
    .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_extrusions(
        &doc,
        &[(-20.1, 40.2), (-20.0, 40.0), (-20.0, 40.0), (-20.0, 40.0)],
    );
    assert_eq!(
        body_uids(&doc),
        ["F1.b0", "F2.b0", "F3.b0", "F3.b1", "F3.b2"]
    );
}

#[test]
fn a_joint_that_does_not_reach_the_other_part_is_refused() {
    let mut doc = boards();
    *doc.kernel().touch.borrow_mut() = Some(vec![false]);
    let uid = add(&mut doc, joint("dado", json!({"depth": 6}))).unwrap();
    assert_eq!(error(&doc, uid), "the joint does not reach the other part");
}

#[test]
fn inconsistent_definitions_are_refused_before_they_are_added() {
    let mut doc = boards();
    let refused = |doc: &mut Document<MockKernel>, kind: &str, more: Value| -> String {
        add(doc, joint(kind, more)).unwrap_err().to_string()
    };
    let same = refused(&mut doc, "dado", json!({"depth": 6, "other": "F1.b0"}));
    assert!(
        same.contains("the joint needs two different parts"),
        "{same}"
    );
    let depth = refused(&mut doc, "dado", json!({}));
    assert!(depth.contains("the joint needs a depth"), "{depth}");
    let unused = refused(&mut doc, "bridle", json!({"thickness": 8, "depth": 6}));
    assert!(
        unused.contains("this kind of joint does not use the field depth"),
        "{unused}"
    );
    let zero = refused(
        &mut doc,
        "tongue_and_groove",
        json!({"depth": 6, "thickness": 0}),
    );
    assert!(
        zero.contains("the joint's thickness must be greater than zero"),
        "{zero}"
    );
    let count = refused(&mut doc, "box_joint", json!({"count": 1}));
    assert!(
        count.contains("the joint's count must be a whole number from 2 to 1000"),
        "{count}"
    );
    // A kind there is none of is not read as a definition.
    let kind: Result<mitcad_model::features::FeatureDef<mitcad_model::features::ValueInput>, _> =
        serde_json::from_value(
            json!({"type": "wood_joint", "kind": "nail", "body": "F1.b0", "face": "F1:side1",
               "other": "F2.b0"}),
        );
    assert!(kind.is_err());
}

#[test]
fn a_tongue_thicker_than_the_end_is_refused_in_words() {
    let mut doc = boards();
    let uid = add(
        &mut doc,
        joint("tongue_and_groove", json!({"depth": 10, "thickness": 24})),
    )
    .unwrap();
    assert_eq!(
        error(&doc, uid),
        "the tongue (24 mm) is thicker than the end face (20 mm)"
    );
}

#[test]
fn editing_the_depth_recomputes_both_parts() {
    let mut doc = boards();
    add(&mut doc, joint("dado", json!({"depth": 6}))).unwrap();
    let parameters = query(&doc, json!({"query": "parameters"}));
    let depth = parameters
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["comment"] == "WoodJoint1 depth")
        .expect("the depth's parameter")["name"]
        .as_str()
        .unwrap()
        .to_owned();
    doc.kernel().specs.borrow_mut().clear();
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": depth, "value": 10}),
    );
    assert_extrusions(&doc, &[(0.0, 10.1), (-0.5, 10.5)]);
    let bounds = doc.body_shape(body("F1.b0")).unwrap().bounds.unwrap();
    assert!((bounds.max[0] - 10.0).abs() < 1e-9, "{bounds:?}");
}
