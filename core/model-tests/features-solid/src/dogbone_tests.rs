// SPDX-License-Identifier: MIT
//! Dogbone (mitcad#1011) in documents with the mock kernel: what reaches the
//! kernel, the cut that keeps the body, edits that recompute it, and the
//! checks of values and picks.

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::{Block, block, corner, def, extrude, num, side};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, ModelError};

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

fn specs(doc: &Document<MockKernel>) -> Vec<String> {
    doc.kernel().specs.borrow().clone()
}

fn last_dogbone(doc: &Document<MockKernel>) -> String {
    specs(doc)
        .into_iter()
        .rev()
        .find(|s| s.starts_with("dogbone "))
        .expect("a dogbone call")
}

fn faces(doc: &Document<MockKernel>, body: BodyUid) -> Vec<String> {
    doc.body_shape(body)
        .unwrap()
        .faces
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn cuts(doc: &Document<MockKernel>) -> usize {
    doc.kernel().calls.borrow().get("cut").copied().unwrap_or(0)
}

/// A 20 x 10 pocket 10 deep cut into the block's top (Sketch2 with c1..c4,
/// Extrude2): its four corner edges.
fn pocket(b: &mut Block) -> (FeatureUid, Vec<String>) {
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_rectangle(sketch, [20.0, 15.0], &num(20.0), &num(10.0))
        .unwrap()
        .region;
    let cut = extrude(sketch, &[&region], 10.0, "cut", &[b.body]);
    let uid = b.doc.add_feature(&cut, None).unwrap().uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let corners = (0..4).map(|i| corner(uid, 1, i).to_string()).collect();
    (uid, corners)
}

#[test]
fn a_dogbone_cuts_its_reliefs_from_the_body_which_keeps_its_id() {
    let mut b = block();
    let (_, corners) = pocket(&mut b);
    let before = cuts(&b.doc);
    let dogbone = add(
        &mut b.doc,
        json!({"type": "dogbone", "body": b.body, "edges": corners,
               "diameter": 6.35, "extra": 0.1}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, dogbone), FeatureStatus::Ok);
    assert_eq!(
        last_dogbone(&b.doc),
        format!(
            "dogbone {dogbone} diameter 6.35 extra 0.1 Diagonal axis [0.0, 0.0, 1.0] edges [{}] faces []",
            corners.join(", ")
        )
    );
    // One cut of the body by the reliefs; the body keeps its id and gets
    // the reliefs' faces, named by their edges.
    assert_eq!(cuts(&b.doc), before + 1);
    assert!(
        b.history(b.body).starts_with("cut("),
        "{}",
        b.history(b.body)
    );
    assert!(
        b.history(b.body)
            .contains(&format!("dogbone({dogbone}:3.175)"))
    );
    let names = faces(&b.doc, b.body);
    for edge in &corners {
        for role in ["dogbone", "dogbone_start", "dogbone_end"] {
            let face = format!("{dogbone}:{role}({edge})");
            assert!(names.contains(&face), "{face} in {names:?}");
        }
    }
    // The faces the pocket made are still there.
    assert!(names.iter().any(|n| n.contains(":side(c1")), "{names:?}");
    assert_eq!(b.doc.feature(dogbone).unwrap().name, "Dogbone1");
}

#[test]
fn edits_of_the_diameter_and_the_type_recompute_the_reliefs() {
    let mut b = block();
    let (_, corners) = pocket(&mut b);
    let dogbone = add(
        &mut b.doc,
        json!({"type": "dogbone", "body": b.body, "edges": corners,
               "diameter": 6.35, "extra": 0.1}),
    )
    .unwrap();
    b.doc
        .edit_feature(
            dogbone,
            &def(json!({"type": "dogbone", "body": b.body, "edges": corners,
                        "diameter": 8, "extra": 0, "dogbone_type": "along_side"})),
        )
        .unwrap();
    assert_eq!(status(&b.doc, dogbone), FeatureStatus::Ok);
    let spec = last_dogbone(&b.doc);
    assert!(
        spec.starts_with(&format!(
            "dogbone {dogbone} diameter 8 extra 0 AlongSide axis"
        )),
        "{spec}"
    );
    assert!(b.history(b.body).contains(&format!("dogbone({dogbone}:4)")));
}

#[test]
fn a_body_or_faces_go_to_the_kernel_with_the_tool_axis_and_the_defaults() {
    let mut b = block();
    let (pocket, _) = pocket(&mut b);
    // The whole body, without values: the defaults, along Z.
    let whole = add(&mut b.doc, json!({"type": "dogbone", "body": b.body})).unwrap();
    assert_eq!(status(&b.doc, whole), FeatureStatus::Ok);
    assert_eq!(
        last_dogbone(&b.doc),
        format!(
            "dogbone {whole} diameter 6.35 extra 0.1 Diagonal axis [0.0, 0.0, 1.0] edges [] faces []"
        )
    );
    assert!(faces(&b.doc, b.body).contains(&format!("{whole}:dogbone")));
    // A face of the pocket and an axis of its own (not a unit vector).
    let wall = side(pocket, 1, 0).to_string();
    let along = add(
        &mut b.doc,
        json!({"type": "dogbone", "body": b.body, "faces": [wall],
               "dogbone_type": "along_side", "axis": [0, 0, 2]}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, along), FeatureStatus::Ok);
    assert_eq!(
        last_dogbone(&b.doc),
        format!(
            "dogbone {along} diameter 6.35 extra 0.1 AlongSide axis [0.0, 0.0, 1.0] edges [] faces [{wall}]"
        )
    );
    assert!(faces(&b.doc, b.body).contains(&format!("{along}:dogbone({wall})")));
}

#[test]
fn values_picks_and_the_kernel_s_refusal_are_reported() {
    let mut b = block();
    let (_, corners) = pocket(&mut b);
    let edge = &corners[0];
    let e = add(
        &mut b.doc,
        json!({"type": "dogbone", "body": b.body, "edges": [edge], "diameter": 0}),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("tool diameter must be greater than zero"), "{e}");
    let e = add(
        &mut b.doc,
        json!({"type": "dogbone", "body": b.body, "edges": [edge], "extra": -0.5}),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("extra clearance must not be negative"), "{e}");
    let e = add(
        &mut b.doc,
        json!({"type": "dogbone", "body": b.body, "axis": [0, 0, 0]}),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("tool axis must be a direction"), "{e}");
    let e = add(
        &mut b.doc,
        json!({"type": "dogbone", "body": b.body, "edges": [edge, edge]}),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("listed more than once"), "{e}");

    // An edge the body does not have.
    let missing = corner(FeatureUid(40), 1, 0).to_string();
    let e = add(
        &mut b.doc,
        json!({"type": "dogbone", "body": b.body, "edges": [missing]}),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("F40"), "{e}");

    // The kernel's refusal (an edge that is no inside corner) is the
    // feature's error; the body stays as it was.
    let history = b.history(b.body);
    b.doc.kernel().fail.borrow_mut().insert("dogbone");
    let refused = add(
        &mut b.doc,
        json!({"type": "dogbone", "body": b.body, "edges": [edge]}),
    )
    .unwrap();
    assert_eq!(error(&b.doc, refused), "dogbone failed");
    assert_eq!(b.history(b.body), history);
}
