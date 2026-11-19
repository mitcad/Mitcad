// SPDX-License-Identifier: MIT
//! Laser sheet's joints (mitcad#1939; commands.md "Finger Joint", "T-Nut
//! Slot", "Flat Pack") in documents with the mock kernel: the plates keep
//! their ids, the refusals in words, a flat pack's six plates. The
//! volumes are checked with OCCT in `cli.sheet_laser_joints`
//! (`tools/cli/tests/sheet_laser_joints.json`); the joints' plans in the
//! features' own unit tests.

use serde_json::{Value, json};

use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Curve3;
use mitcad_model::test_support::def;
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
        FeatureStatus::Failed(message) => message.agent_text().to_owned(),
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn body(text: &str) -> BodyUid {
    text.parse().unwrap()
}

fn has(doc: &Document<MockKernel>, uid: BodyUid) -> bool {
    doc.bodies().iter().any(|b| b.uid == uid)
}

fn plate(doc: &mut Document<MockKernel>, corner: [f64; 2], size: [f64; 3]) {
    add(
        doc,
        json!({"type": "box", "plane": {"type": "origin", "plane": "xy"},
               "corner": corner, "length": size[0], "width": size[1], "height": size[2],
               "operation": "new_body"}),
    )
    .unwrap();
}

/// A front plate 100 x 6 x 60 (F1.b0) and a side plate 6 x 80 x 60
/// (F2.b0) overlapping at the corner.
fn corner() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    plate(&mut doc, [0.0, 0.0], [100.0, 6.0, 60.0]);
    plate(&mut doc, [0.0, 0.0], [6.0, 80.0, 60.0]);
    doc
}

#[test]
fn a_finger_joint_keeps_both_plates() {
    let mut doc = corner();
    let joint = add(
        &mut doc,
        json!({"type": "finger_joint", "bodies": ["F1.b0", "F2.b0"], "finger_width": 12}),
    )
    .unwrap();
    assert!(
        matches!(status(&doc, joint), FeatureStatus::Ok),
        "{:?}",
        status(&doc, joint)
    );
    assert_eq!(doc.bodies().len(), 2);
    assert!(has(&doc, body("F1.b0")));
    assert!(has(&doc, body("F2.b0")));
}

#[test]
fn finger_joint_refusals_are_in_words() {
    let mut doc = corner();
    plate(&mut doc, [300.0, 0.0], [6.0, 80.0, 60.0]);
    let apart = add(
        &mut doc,
        json!({"type": "finger_joint", "bodies": ["F1.b0", "F3.b0"]}),
    )
    .unwrap();
    assert!(error(&doc, apart).starts_with("the two plates do not meet"));
    let one = add(
        &mut doc,
        json!({"type": "finger_joint", "bodies": ["F1.b0"]}),
    );
    assert!(one.is_err() || matches!(status(&doc, one.unwrap()), FeatureStatus::Failed(_)));
    let narrow = add(
        &mut doc,
        json!({"type": "finger_joint", "bodies": ["F1.b0", "F2.b0"], "count": 300,
               "clearance": 0.2}),
    )
    .unwrap();
    assert!(error(&doc, narrow).contains("too narrow for the clearance"));
}

#[test]
fn a_t_nut_slot_cuts_the_plate_and_its_mate() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_faces.set(true);
    // The plate's end face at x = 100: 6 across (y) and 60 high (z).
    *doc.kernel().curves.borrow_mut() = vec![
        Curve3::Line {
            start: [100.0, 0.0, 0.0],
            end: [100.0, 6.0, 0.0],
        },
        Curve3::Line {
            start: [100.0, 6.0, 0.0],
            end: [100.0, 6.0, 60.0],
        },
        Curve3::Line {
            start: [100.0, 6.0, 60.0],
            end: [100.0, 0.0, 60.0],
        },
        Curve3::Line {
            start: [100.0, 0.0, 60.0],
            end: [100.0, 0.0, 0.0],
        },
    ];
    plate(&mut doc, [0.0, 0.0], [100.0, 6.0, 60.0]);
    plate(&mut doc, [100.0, -20.0], [6.0, 46.0, 60.0]);
    let slot = add(
        &mut doc,
        json!({"type": "t_nut_slot", "body": "F1.b0", "face": "F1:side1", "mating_body": "F2.b0",
               "screw_diameter": 3, "slot_length": 12, "nut_width": 5.5, "nut_thickness": 2.4,
               "nut_distance": 6}),
    )
    .unwrap();
    assert!(
        matches!(status(&doc, slot), FeatureStatus::Ok),
        "{:?}",
        status(&doc, slot)
    );
    assert_eq!(doc.bodies().len(), 2);
    // Sizes that make no slot are refused when the feature is added.
    let short = add(
        &mut doc,
        json!({"type": "t_nut_slot", "body": "F1.b0", "face": "F1:side1",
               "screw_diameter": 3, "slot_length": 5, "nut_width": 5.5, "nut_thickness": 2.4,
               "nut_distance": 6}),
    );
    let refusal = match short {
        Err(e) => e.to_string(),
        Ok(uid) => error(&doc, uid),
    };
    assert!(refusal.contains("past the slot's end"), "{refusal}");
}

#[test]
fn a_flat_pack_turns_a_box_into_six_plates() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    plate(&mut doc, [0.0, 0.0], [200.0, 120.0, 80.0]);
    let pack = add(
        &mut doc,
        json!({"type": "flat_pack", "body": "F1.b0", "thickness": 6}),
    )
    .unwrap();
    assert!(
        matches!(status(&doc, pack), FeatureStatus::Ok),
        "{:?}",
        status(&doc, pack)
    );
    assert_eq!(doc.bodies().len(), 6);
    assert!(!has(&doc, body("F1.b0")));
    for k in 0..6 {
        assert!(has(&doc, body(&format!("F2.b{k}"))));
    }
    let thick = add(
        &mut doc,
        json!({"type": "flat_pack", "body": "F2.b0", "thickness": 50}),
    )
    .unwrap();
    assert!(matches!(status(&doc, thick), FeatureStatus::Failed(_)));
}
