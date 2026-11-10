// SPDX-License-Identifier: MIT
//! Plastic part features and plastic rules (mitcad#459) in documents with
//! the mock kernel: what reaches the kernel, the rules' defaults and
//! warnings, checks and references.

use serde_json::{Value, json};

use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::{Block, block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::FaceName;
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

fn specs(doc: &Document<MockKernel>) -> Vec<String> {
    doc.kernel().specs.borrow().clone()
}

/// The block's top face (planar, z = 20, out of the body along +Z).
fn top(b: &Block) -> String {
    FaceName::end(b.extrude, b.region.clone()).to_string()
}

fn faces(doc: &Document<MockKernel>, body: BodyUid) -> Vec<String> {
    doc.body_shape(body)
        .unwrap()
        .faces
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn degrees(d: f64) -> f64 {
    d.to_radians()
}

#[test]
fn a_boss_takes_its_draft_from_its_rule_and_builds_post_ribs_and_hole() {
    let mut b = block();
    let face = top(&b);
    let rule = add(
        &mut b.doc,
        json!({"type": "plastic_rule", "rule_name": "ABS (1.5mm)", "material": "ABS Plastic",
               "thickness": 1.5, "draft_angle": "1.5 deg", "min_thickness": 1.2}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, rule), FeatureStatus::Ok);
    let boss = add(
        &mut b.doc,
        json!({"type": "boss", "body": b.body, "face": face, "point": [30, 20, 25],
               "diameter": 10, "height": 12, "hole_diameter": 4, "ribs": 4,
               "rib_thickness": 1.5, "rib_length": 6, "rule": rule}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, boss), FeatureStatus::Ok);
    assert!(
        b.doc.warnings(boss).is_empty(),
        "{:?}",
        b.doc.warnings(boss)
    );
    // The post: from 0.5 below the face (5 % of the height, at most 0.5),
    // narrowing by the rule's draft.
    let taper = -degrees(1.5);
    assert!(
        specs(&b.doc).contains(&format!(
            "extrude {boss} along [0.0, 0.0, 1.0] from offset -0.5: distance 12.5 taper {taper}"
        )),
        "{:?}",
        specs(&b.doc)
    );
    let calls = b.doc.kernel().calls.borrow().clone();
    assert_eq!(calls.get("join"), Some(&1));
    assert_eq!(calls.get("cut"), Some(&1));
    // The post, one of its ribs and the hole are faces of the body.
    let names = faces(&b.doc, b.body);
    for face in [
        format!("{boss}:side(c1)"),
        format!("{boss}:side(c2)"),
        format!("{boss}:side(c10[c13,c11])"),
        format!("{boss}:side(c22[c25,c23])"),
    ] {
        assert!(names.contains(&face), "{face} in {names:?}");
    }
    // A thinner wall than the rule's smallest is a warning.
    b.doc
        .edit_feature(
            boss,
            &def(json!({"type": "boss", "body": b.body, "face": face,
                "point": [30, 20, 25], "diameter": 10, "height": 12, "hole_diameter": 8,
                "draft_angle": "0.2 deg", "rule": rule})),
        )
        .unwrap();
    let warnings = b.doc.warnings(boss).join("; ");
    assert!(
        warnings.contains("below the smallest draft of PlasticRule1"),
        "{warnings}"
    );
    assert!(
        warnings.contains("boss's wall 1 mm is thinner"),
        "{warnings}"
    );
}

#[test]
fn a_rule_edit_rebuilds_its_features_and_a_suppressed_rule_is_not_used() {
    let mut b = block();
    let face = top(&b);
    let rule = add(
        &mut b.doc,
        json!({"type": "plastic_rule", "rule_name": "Thick ABS", "thickness": 3}),
    )
    .unwrap();
    let rest = add(
        &mut b.doc,
        json!({"type": "rest", "body": b.body, "face": face, "point": [30, 20, 20],
               "width": 8, "length": 12, "height": 3, "thickness": 1, "rule": rule}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, rest), FeatureStatus::Ok);
    // The rule's default draft, 2 degrees.
    let last = specs(&b.doc).last().cloned().unwrap();
    assert!(
        last.ends_with(&format!("taper {}", -degrees(2.0))),
        "{last}"
    );
    // The rule's draft changed: the rest is built again with it.
    let param = query(&b.doc, json!({"query": "feature", "uid": rule}));
    assert_eq!(param["def"]["thickness"], "d4", "{param}");
    b.doc
        .edit_feature(
            rule,
            &def(
                json!({"type": "plastic_rule", "rule_name": "Thick ABS", "thickness": 3,
                        "draft_angle": "1 deg"}),
            ),
        )
        .unwrap();
    let last = specs(&b.doc).last().cloned().unwrap();
    assert!(
        last.ends_with(&format!("taper {}", -degrees(1.0))),
        "{last}"
    );
    // Suppressed: no draft, and a warning says so.
    command(
        &mut b.doc,
        json!({"cmd": "suppress_feature", "uid": rule, "suppressed": true}),
    );
    assert!(specs(&b.doc).last().unwrap().ends_with("taper 0"));
    let warnings = b.doc.warnings(rest).join("; ");
    assert!(warnings.contains("is suppressed"), "{warnings}");
}

#[test]
fn lips_and_grooves_follow_the_rim_and_a_groove_takes_the_clearance() {
    let mut b = block();
    let face = top(&b);
    let rule = add(
        &mut b.doc,
        json!({"type": "plastic_rule", "rule_name": "ABS", "thickness": 1.5}),
    )
    .unwrap();
    let lip = add(
        &mut b.doc,
        json!({"type": "lip", "body": b.body, "face": face, "width": 1, "height": 2}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, lip), FeatureStatus::Ok);
    assert_eq!(
        specs(&b.doc).last().unwrap(),
        &format!(
            "rim_band {lip} on {} inside width 1 free 0 from -0.1 to 2",
            face
        )
    );
    assert!(faces(&b.doc, b.body).contains(&format!("{lip}:band_top")));
    let groove = add(
        &mut b.doc,
        json!({"type": "groove", "body": b.body, "face": face, "width": 1, "depth": 2,
               "outside": true, "rule": rule}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, groove), FeatureStatus::Ok);
    // Widened and deepened by the rule's clearance, 0.1 mm, and reaching
    // as far past the edge.
    assert_eq!(
        specs(&b.doc).last().unwrap(),
        &format!(
            "rim_band {groove} on {} outside width 1.1 free 1.1 from -2.1 to 0.1",
            face
        )
    );
    assert_eq!(b.doc.kernel().calls.borrow().get("cut"), Some(&1));
    // A gap of its own wins.
    b.doc
        .edit_feature(
            groove,
            &def(
                json!({"type": "groove", "body": b.body, "face": face, "width": 1,
                        "depth": 2, "gap": 0.3, "rule": rule}),
            ),
        )
        .unwrap();
    assert_eq!(
        specs(&b.doc).last().unwrap(),
        &format!(
            "rim_band {groove} on {} inside width 1.3 free 1.3 from -2.3 to 0.1",
            face
        )
    );
}

#[test]
fn snap_fits_and_rests_check_their_sizes() {
    let mut b = block();
    let face = top(&b);

    let snap = json!({"type": "snap_fit", "body": b.body, "face": face, "point": [30, 20, 20],
        "hook": [1, 0, 0], "length": 12, "thickness": 1.5, "width": 6, "catch_depth": 1,
        "catch_length": 2});
    let uid = add(&mut b.doc, snap.clone()).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let names = faces(&b.doc, b.body);
    assert!(
        names.contains(&format!("{uid}:side(c4[c3,c5])")),
        "{names:?}"
    );
    // The catch must be shorter than the arm; the hook across it.
    let mut long = snap.clone();
    long["catch_length"] = json!(12);
    let e = add(&mut b.doc, long).unwrap_err().to_string();
    assert!(e.contains("must be shorter than its arm"), "{e}");
    let mut along = snap.clone();
    along["hook"] = json!([0, 0, 1]);
    let bad = add(&mut b.doc, along).unwrap();
    assert!(error(&b.doc, bad).contains("must point across the arm"));
    command(&mut b.doc, json!({"cmd": "delete_feature", "uid": bad}));

    let rest = json!({"type": "rest", "body": b.body, "face": face, "point": [30, 20, 20],
        "width": 4, "height": 4});
    let mut thick = rest.clone();
    thick["thickness"] = json!(2);
    let e = add(&mut b.doc, thick).unwrap_err().to_string();
    assert!(e.contains("thinner than half its width"), "{e}");
    let mut closed = rest.clone();
    closed["height"] = json!(40);
    closed["draft_angle"] = json!("10 deg");
    let e = add(&mut b.doc, closed).unwrap_err().to_string();
    assert!(e.contains("closes the rest before its top"), "{e}");
    let round = add(&mut b.doc, rest).unwrap();
    assert_eq!(status(&b.doc, round), FeatureStatus::Ok);
    assert!(faces(&b.doc, b.body).contains(&format!("{round}:side(c1)")));

    // A boss whose hole does not fit, or whose draft closes it.
    let e = add(
        &mut b.doc,
        json!({"type": "boss", "body": b.body, "face": face, "point": [30, 20, 20],
               "diameter": 10, "height": 12, "hole_diameter": 20}),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("must fit inside its top"), "{e}");
    let e = add(
        &mut b.doc,
        json!({"type": "boss", "body": b.body, "face": face, "point": [30, 20, 20],
               "diameter": 2, "height": 30, "draft_angle": "10 deg"}),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("closes the boss"), "{e}");
}

#[test]
fn a_rule_must_be_a_plastic_rule_before_the_feature() {
    let mut b = block();
    let face = top(&b);
    let e = add(
        &mut b.doc,
        json!({"type": "groove", "body": b.body, "face": face, "width": 1, "depth": 1,
               "rule": b.extrude}),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("is not a plastic rule"), "{e}");
    let e = add(
        &mut b.doc,
        json!({"type": "plastic_rule", "rule_name": "Bad", "thickness": 1, "min_thickness": 2,
               "max_thickness": 1}),
    )
    .unwrap_err()
    .to_string();
    assert!(
        e.contains("smallest thickness 2 is above its largest 1"),
        "{e}"
    );
    // The root fillet needs the edges where the boss meets the face, which
    // the mock kernel's joins do not make.
    let boss = add(
        &mut b.doc,
        json!({"type": "boss", "body": b.body, "face": face, "point": [30, 20, 20],
               "diameter": 6, "height": 10, "fillet": 1}),
    )
    .unwrap();
    assert!(error(&b.doc, boss).contains("root fillet found no edge"));
}

#[test]
fn plastic_features_round_trip_through_the_design_file() {
    let mut b = block();
    let face = top(&b);
    let rule = add(
        &mut b.doc,
        json!({"type": "plastic_rule", "rule_name": "PP", "thickness": 3, "clearance": 0.2}),
    )
    .unwrap();
    add(
        &mut b.doc,
        json!({"type": "boss", "body": b.body, "face": face, "point": [30, 20, 20],
               "direction": [0, 0, 1], "diameter": 6, "height": 10, "rule": rule}),
    )
    .unwrap();
    add(
        &mut b.doc,
        json!({"type": "rest", "body": b.body, "face": face, "point": [10, 10, 20],
               "along": [0, 1, 0], "width": 4, "length": 6, "height": 2}),
    )
    .unwrap();
    let text = b.doc.to_json();
    let mut again = Document::from_json(&text, MockKernel::default()).unwrap();
    again.recompute();
    let timeline = query(&again, json!({"query": "timeline"}));
    let types: Vec<&str> = timeline["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["type"].as_str().unwrap())
        .collect();
    assert_eq!(types, ["sketch", "extrude", "plastic_rule", "boss", "rest"]);
    assert!(
        timeline["features"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["status"] == "ok"),
        "{timeline}"
    );
}
