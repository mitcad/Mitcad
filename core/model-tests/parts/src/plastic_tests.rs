// SPDX-License-Identifier: MIT
//! Plastic part features and plastic rules (mitcad#459) in documents with
//! the mock kernel: what reaches the kernel, the rules' defaults and
//! warnings, checks and references.

use serde_json::{Value, json};

use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::notice::{FeatureError, NoticePlace};
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

fn failure(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureError {
    match status(doc, uid) {
        FeatureStatus::Failed(error) => error,
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[test]
fn lips_and_grooves_follow_the_face_beside_them_and_lean_one_side() {
    // mitcad#2511.
    let mut b = block();
    let face = top(&b);
    let rule = add(
        &mut b.doc,
        json!({"type": "plastic_rule", "rule_name": "ABS", "thickness": 1.5,
               "draft_angle": "1.5 deg"}),
    )
    .unwrap();
    // Following the face beside the edge, the lip starts at the rim face;
    // its other side leans by its own draft, narrowing it as it rises.
    let lip = add(
        &mut b.doc,
        json!({"type": "lip", "body": b.body, "face": face, "width": 1, "height": 2,
               "tangent": true, "draft_angle": "3 deg"}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, lip), FeatureStatus::Ok);
    assert_eq!(
        specs(&b.doc).last().unwrap(),
        &format!(
            "rim_band {lip} on {face} inside width 1 free 0 from 0 to 2 tangent side_taper {}",
            degrees(3.0)
        )
    );
    // The side draft of the rule: a lip leans one way, a groove the other,
    // so that a groove of a lip's draft takes the lip.
    let lip2 = add(
        &mut b.doc,
        json!({"type": "lip", "body": b.body, "face": face, "width": 1, "height": 2,
               "side_draft": true, "rule": rule}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, lip2), FeatureStatus::Ok);
    assert_eq!(
        specs(&b.doc).last().unwrap(),
        &format!(
            "rim_band {lip2} on {face} inside width 1 free 0 from -0.1 to 2 side_taper {}",
            degrees(1.5)
        )
    );
    let groove = add(
        &mut b.doc,
        json!({"type": "groove", "body": b.body, "face": face, "width": 1, "depth": 2,
               "outside": true, "tangent": true, "side_draft": true, "rule": rule}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, groove), FeatureStatus::Ok);
    assert_eq!(
        specs(&b.doc).last().unwrap(),
        &format!(
            "rim_band {groove} on {face} outside width 1.1 free 1.1 from -2.1 to 0.1 tangent side_taper {}",
            -degrees(1.5)
        )
    );
    // The rule's draft below a draft of its own warns as the boss's does.
    let low = add(
        &mut b.doc,
        json!({"type": "lip", "body": b.body, "face": face, "width": 1, "height": 2,
               "draft_angle": "0.2 deg", "rule": rule}),
    )
    .unwrap();
    let warnings = b.doc.warnings(low).join("; ");
    assert!(warnings.contains("below the smallest draft"), "{warnings}");

    // Refused in words: a draft that closes the lip before its top (when
    // added), a side draft with neither an angle nor a rule.
    let e = add(
        &mut b.doc,
        json!({"type": "lip", "body": b.body, "face": face, "width": 0.2, "height": 5,
               "draft_angle": "10 deg"}),
    )
    .unwrap_err()
    .to_string();
    assert!(
        e.contains("the lip's draft closes it before its top"),
        "{e}"
    );
    let e = add(
        &mut b.doc,
        json!({"type": "groove", "body": b.body, "face": face, "width": 0.2, "depth": 5,
               "draft_angle": "10 deg"}),
    )
    .unwrap_err()
    .to_string();
    assert!(
        e.contains("the groove's draft closes it before its bottom"),
        "{e}"
    );
    let bare = add(
        &mut b.doc,
        json!({"type": "lip", "body": b.body, "face": face, "width": 1, "height": 2,
               "side_draft": true}),
    )
    .unwrap();
    assert!(
        error(&b.doc, bare).contains("needs a draft angle, or a plastic rule"),
        "{}",
        error(&b.doc, bare)
    );
    // The rule's draft known only when evaluated: a steep one closes the
    // groove, refused with the rim face painted and named for agents only.
    let steep = add(
        &mut b.doc,
        json!({"type": "plastic_rule", "rule_name": "Steep", "thickness": 1.5,
               "draft_angle": "40 deg"}),
    )
    .unwrap();
    let closed = add(
        &mut b.doc,
        json!({"type": "groove", "body": b.body, "face": face, "width": 1, "depth": 2,
               "side_draft": true, "rule": steep}),
    )
    .unwrap();
    let failed = failure(&b.doc, closed);
    assert!(
        failed
            .message
            .contains("the groove's draft closes it before its bottom"),
        "{failed:?}"
    );
    assert!(!failed.message.contains(&face), "{failed:?}");
    assert!(
        failed.places.contains(&NoticePlace::Face {
            body: Some(b.body.to_string()),
            name: face.clone(),
        }),
        "{:?}",
        failed.places
    );
}

#[test]
fn a_boss_drafts_its_ribs_and_hole_turns_its_ribs_and_ends_them_at_walls() {
    // mitcad#2511.
    let mut b = block();
    let face = top(&b);
    let rule = add(
        &mut b.doc,
        json!({"type": "plastic_rule", "rule_name": "ABS", "thickness": 1.5,
               "draft_angle": "1.5 deg"}),
    )
    .unwrap();
    let boss = add(
        &mut b.doc,
        json!({"type": "boss", "body": b.body, "face": face, "point": [30, 20, 20],
               "diameter": 10, "height": 12, "hole_diameter": 4, "hole_draft": true,
               "ribs": 3, "rib_thickness": 1.5, "rib_length": 6, "rib_angle": "30 deg",
               "rib_draft_angle": "1 deg",
               "rib_ends": [{"end": "wall", "short": 1}, {"end": "wall"}], "rule": rule}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, boss), FeatureStatus::Ok);
    assert!(
        b.doc.warnings(boss).is_empty(),
        "{:?}",
        b.doc.warnings(boss)
    );
    let specs = specs(&b.doc);
    let has = |spec: String| assert!(specs.contains(&spec), "{spec} in {specs:?}");
    // The post by the rule's draft, and run on past the ribs' tops for the
    // ribs that end at a wall.
    let post = -degrees(1.5);
    has(format!(
        "extrude {boss} along [0.0, 0.0, 1.0] from offset -0.5: distance 12.5 taper {post}"
    ));
    has(format!(
        "extrude {boss} along [0.0, 0.0, 1.0] from offset -0.5: distance 13 taper {post}"
    ));
    // The ribs up to the wall: the kernel's rib from their top edges down
    // to the face (To Next), their ends continued, the first 1 short; as
    // thick at the face as asked, so thinner at the top by the draft.
    let draft = degrees(1.0);
    let top_thickness = 1.5 - 2.0 * 12.0 * draft.tan();
    for (curve, offset) in [("c11", "-1"), ("c15", "0")] {
        has(format!(
            "rib {boss} chains {curve} thickness {top_thickness} Symmetric depth None flip true \
             bodies 2 extend_ends offset {offset} draft {draft}"
        ));
    }
    // The third rib to its length, leaning by its own draft.
    has(format!(
        "extrude {boss} along [0.0, 0.0, 1.0] from offset -0.5: distance 12.5 taper {}",
        -draft
    ));
    // The hole by the rule's draft: as wide as asked at the top.
    has(format!(
        "extrude {boss} along [0.0, 0.0, 1.0] from offset 0: distance 12.5 taper {}",
        degrees(1.5)
    ));
    let names = faces(&b.doc, b.body);
    for face in [
        format!("{boss}:rib0({boss}:side(c11))"),
        format!("{boss}:rib1({boss}:side(c15))"),
        format!("{boss}:side(c18[c21,c19])"),
        format!("{boss}:side(c2)"),
    ] {
        assert!(names.contains(&face), "{face} in {names:?}");
    }

    // Refused in words when added: drafts that close the hole or the ribs,
    // a negative distance short of the wall.
    let base = json!({"type": "boss", "body": b.body, "face": face, "point": [30, 20, 20],
                      "diameter": 10, "height": 12});
    let refused = |changes: Value| {
        let mut def = base.clone();
        for (key, value) in changes.as_object().unwrap() {
            def[key] = value.clone();
        }
        def
    };
    let e = add(
        &mut b.doc,
        refused(json!({"hole_diameter": 2, "hole_draft_angle": "10 deg"})),
    )
    .unwrap_err()
    .to_string();
    assert!(
        e.contains("the hole's draft closes it before its bottom"),
        "{e}"
    );
    let e = add(
        &mut b.doc,
        refused(json!({"hole_diameter": 8, "hole_depth": 30, "hole_draft_angle": "-10 deg"})),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("takes it through the boss's wall"), "{e}");
    let e = add(
        &mut b.doc,
        refused(json!({"ribs": 2, "rib_thickness": 1, "rib_draft_angle": "5 deg"})),
    )
    .unwrap_err()
    .to_string();
    assert!(
        e.contains("the ribs' draft closes them before their tops"),
        "{e}"
    );
    let e = add(
        &mut b.doc,
        refused(json!({"ribs": 2, "rib_ends": [{"end": "wall", "short": -1}]})),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("rib 1's distance short of the wall"), "{e}");
    // The rule's draft asked for without a rule.
    let bare = add(&mut b.doc, refused(json!({"ribs": 2, "rib_draft": true}))).unwrap();
    assert!(
        error(&b.doc, bare).contains("the ribs' draft needs a draft angle"),
        "{}",
        error(&b.doc, bare)
    );
}

#[test]
fn plastic_features_saved_before_the_new_options_are_built_as_before() {
    // mitcad#2511: no new field is written for them, and the kernel gets
    // what it got.
    let mut b = block();
    let face = top(&b);
    let lip = add(
        &mut b.doc,
        json!({"type": "lip", "body": b.body, "face": face, "width": 1, "height": 2}),
    )
    .unwrap();
    assert_eq!(
        specs(&b.doc).last().unwrap(),
        &format!("rim_band {lip} on {face} inside width 1 free 0 from -0.1 to 2")
    );
    let boss = add(
        &mut b.doc,
        json!({"type": "boss", "body": b.body, "face": face, "point": [30, 20, 20],
               "diameter": 10, "height": 12, "hole_diameter": 4, "ribs": 2}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, boss), FeatureStatus::Ok);
    let calls = b.doc.kernel().calls.borrow().clone();
    assert_eq!(calls.get("rib"), None);
    for uid in [lip, boss] {
        let feature = query(&b.doc, json!({"query": "feature", "uid": uid}));
        let keys: Vec<&String> = feature["def"].as_object().unwrap().keys().collect();
        for new in [
            "tangent",
            "side_draft",
            "draft_angle",
            "rib_angle",
            "rib_draft",
            "rib_draft_angle",
            "hole_draft",
            "hole_draft_angle",
            "rib_ends",
        ] {
            assert!(!keys.iter().any(|k| k.as_str() == new), "{new} in {keys:?}");
        }
    }
    // And they open again as they were.
    let text = b.doc.to_json();
    let mut again = Document::from_json(&text, MockKernel::default()).unwrap();
    again.recompute();
    assert_eq!(status(&again, lip), FeatureStatus::Ok);
    assert_eq!(status(&again, boss), FeatureStatus::Ok);
}
