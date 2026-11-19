// SPDX-License-Identifier: MIT
//! `build_remove` of a BUILD part that a free part of a flexible
//! occurrence is joined to (mitcad#2017, mitcad#2025), with the mock
//! kernel: an upright profile grounded in the root, and a Gantry with a
//! Carriage and a Joiner and no joints of its own, placed once and made
//! flexible. A joint of the root puts the Joiner (a free part of Gantry:1)
//! on a point of the upright; removing the upright takes the joint along,
//! and the Joiner stays where the joint left it, kept as Gantry:1's own
//! placement of it, while the Gantry's design does not change. Undo brings
//! the upright and the joint back and takes the kept placement back.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_tests_support::test_library::*;

/// Points that pin a placement down: its origin and one along each axis.
const PROBES: [[f64; 3]; 4] = [
    [0.0; 3],
    [10.0, 0.0, 0.0],
    [0.0, 10.0, 0.0],
    [0.0, 0.0, 10.0],
];

fn same(a: &Transform, b: &Transform) -> bool {
    PROBES
        .iter()
        .all(|p| close(a.apply_point(*p), b.apply_point(*p)))
}

/// Where Gantry:1 keeps the Joiner of its own, if it does.
fn kept(doc: &Document<MockKernel>, g1: OccurrenceUid, joiner: OccurrenceUid) -> Option<Transform> {
    doc.assembly()
        .occurrence(g1)
        .and_then(|o| o.flexible.as_ref())
        .and_then(|f| f.placement(&[joiner]))
}

#[test]
fn a_free_part_joined_to_a_removed_build_part_stays() {
    let mut doc = design();
    let upright = first(&mut doc, "p30", [0.0; 3], 500.0);
    // The Gantry 1000 along x: the Carriage 150 along its y, the Joiner at
    // its origin.
    let gantry_at = Transform::translation([1000.0, 0.0, 0.0]);
    let (_, g1) = doc
        .create_component(Some("Gantry"), gantry_at, true)
        .unwrap();
    let (_, carriage) = doc
        .create_component(
            Some("Carriage"),
            Transform::translation([0.0, 150.0, 0.0]),
            false,
        )
        .unwrap();
    let (_, joiner) = doc
        .create_component(Some("Joiner"), Transform::IDENTITY, false)
        .unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.set_occurrence_flexible(g1, true).unwrap();

    // The Joiner on a point 400 up the upright's own z.
    let joined = command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"occurrence": format!("{g1}/{joiner}"), "geometry": {"point": [0, 0, 0]}},
               "b": {"occurrence": upright["occurrence"], "geometry": {"point": [0, 0, 400]}}}),
    )
    .unwrap();
    assert_eq!(joined["solved"], true, "{joined}");
    let joint: FeatureUid = joined["uid"].as_str().unwrap().parse().unwrap();
    all_ok(&doc);
    let on_upright = placed(&doc, occurrence(&upright)).apply_point([0.0, 0.0, 400.0]);
    let before = doc.path_transform(&[g1, joiner]);
    assert!(
        close(before.apply_point([0.0; 3]), on_upright),
        "the Joiner on the upright: {before:?}"
    );
    assert!(kept(&doc, g1, joiner).is_none(), "nothing kept yet");

    let removed = command(
        &mut doc,
        json!({"cmd": "build_remove", "component": upright["component"]}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(
        doc.assembly().component(component(&upright)).is_none(),
        "{removed}"
    );
    assert!(doc.feature(joint).is_none(), "the joint went: {removed}");
    // The Joiner stayed, kept by Gantry:1 in the Gantry's coordinates.
    let after = doc.path_transform(&[g1, joiner]);
    assert!(
        same(&after, &before),
        "{after:?}, was {before:?}: {removed}"
    );
    let held = kept(&doc, g1, joiner).expect("kept for Gantry:1");
    assert!(same(&gantry_at.after(&held), &before), "{held:?}");
    // The rest of Gantry:1 and the Gantry's design are as they were.
    assert!(close(
        doc.path_transform(&[g1, carriage]).apply_point([0.0; 3]),
        [1000.0, 150.0, 0.0]
    ));
    assert!(same(&placed(&doc, g1), &gantry_at));
    assert!(same(&placed(&doc, joiner), &Transform::IDENTITY));
    // Saved: the kept placement is in the file.
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let flexible = file["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["uid"] == g1.to_string())
        .map(|o| o["flexible"].clone())
        .unwrap_or_else(|| panic!("Gantry:1 in the file: {file}"));
    assert!(flexible.get("placements").is_some(), "{flexible}");

    // Undo: the upright and the joint are back, the kept placement is
    // taken back, and the joint holds the Joiner where it was.
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    all_ok(&doc);
    assert!(doc.assembly().component(component(&upright)).is_some());
    assert!(doc.feature(joint).is_some());
    assert!(kept(&doc, g1, joiner).is_none(), "undo drops it");
    assert!(same(&doc.path_transform(&[g1, joiner]), &before));
}
