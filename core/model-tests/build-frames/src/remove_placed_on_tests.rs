// SPDX-License-Identifier: MIT
//! Removing a profile that others are placed on (mitcad#1811), with the
//! mock kernel: a frame of an upright on the ground, a cross profile on
//! its side and a second upright on the cross profile's end, and the same
//! three built from the cross profile out. Removing the cross profile
//! takes neither upright along: each stays where it was (within 1e-6 mm),
//! with no joint left to the removed one, the free ones listed and named
//! in words; undo brings the cross profile and its joints back. In a frame
//! of its own with a shaft joined to the second upright from outside it
//! (the arrangement of the report), the shaft stays too, also when the
//! upright it is joined to goes after the cross profile, its own move on
//! the timeline taken into account. Deleting the cross profile's
//! occurrence (`delete_occurrence`) keeps the second upright where it is
//! too. Delete Component on a BUILD part goes as a member (mitcad#1820):
//! a connector so removed takes its hole and fasteners along.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::FeatureDef;
use mitcad_model::ids::{ComponentUid, OccurrenceUid};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_tests_support::test_library::*;

/// A profile of the design's source placed on a slot of `host`'s side,
/// `offset` from its end A.
fn on_side(
    doc: &mut Document<MockKernel>,
    host: &Value,
    slot: &str,
    offset: f64,
    length: f64,
) -> Value {
    let hp = derived(doc, component(host));
    let placed = place(
        doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": host["occurrence"],
                      "geometry": {"mount": format!("{hp}:mount({slot})")},
                      "offset": offset},
               "length": length}),
    );
    all_ok(doc);
    placed
}

/// A profile of the design's source placed on `host`'s end B, along
/// `direction` (a corner).
fn on_end(doc: &mut Document<MockKernel>, host: &Value, direction: [f64; 3], length: f64) -> Value {
    let hp = derived(doc, component(host));
    let placed = place(
        doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": host["occurrence"],
                      "geometry": {"mount": format!("{hp}:mount(end_b)")},
                      "direction": direction},
               "length": length}),
    );
    all_ok(doc);
    placed
}

fn same(a: &Transform, b: &Transform) -> bool {
    close(a.translation, b.translation) && (0..3).all(|r| close(a.linear[r], b.linear[r]))
}

/// The joints with a side in `occurrence` (anywhere on its path).
fn joints_on(doc: &Document<MockKernel>, occurrence: OccurrenceUid) -> usize {
    doc.features()
        .filter(|e| {
            matches!(&e.def, FeatureDef::Joint(j)
                if j.a.occurrence.0.contains(&occurrence) || j.b.occurrence.0.contains(&occurrence))
        })
        .count()
}

fn uids(value: &Value) -> Vec<String> {
    let mut list: Vec<String> = value
        .as_array()
        .unwrap_or_else(|| panic!("no list: {value}"))
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    list.sort();
    list
}

/// Whether a notice of `answer` says in words that the part of
/// `occurrence` stayed, painting it.
fn said_stays(answer: &Value, occurrence: OccurrenceUid) -> bool {
    let tail = occurrence.to_string();
    answer["notices"].as_array().is_some_and(|notices| {
        notices.iter().any(|n| {
            n["level"] == "info"
                && n["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("stays where it was"))
                && n["places"].as_array().is_some_and(|places| {
                    places.iter().any(|p| {
                        p["kind"] == "occurrence"
                            && p["path"]
                                .as_str()
                                .is_some_and(|path| path.rsplit('/').next() == Some(tail.as_str()))
                    })
                })
        })
    })
}

/// Removes `cross` (Delete Member) and checks: the parts of `stay` stay
/// where they were, none with a joint to it; `freed` are listed and said
/// in words, and lose their joint; undo brings it and its joints back,
/// the parts where they were.
fn remove_cross(doc: &mut Document<MockKernel>, cross: &Value, stay: &[&Value], freed: &[&Value]) {
    let x: ComponentUid = component(cross);
    let xo = occurrence(cross);
    let before: Vec<Transform> = stay.iter().map(|p| placed(doc, occurrence(p))).collect();
    let joints_before = joints_on(doc, xo);
    assert!(joints_before > 0, "nothing is joined to the cross profile");
    let removed = command(
        doc,
        json!({"cmd": "build_remove", "component": cross["component"]}),
    )
    .unwrap();
    all_ok(doc);
    assert!(doc.assembly().component(x).is_none());
    assert_eq!(joints_on(doc, xo), 0, "a joint to the removed part is left");
    for (p, was) in stay.iter().zip(&before) {
        let c = component(p);
        assert!(
            doc.assembly().component(c).is_some(),
            "{c} went with the cross profile: {removed}"
        );
        let now = placed(doc, occurrence(p));
        assert!(same(&now, was), "{c} moved: {now:?}, was {was:?}");
    }
    let wanted = uids(&json!(
        freed
            .iter()
            .map(|p| p["component"].clone())
            .collect::<Vec<_>>()
    ));
    assert_eq!(uids(&removed["removed"]["freed"]), wanted, "{removed}");
    for p in freed {
        let info = doc.part_info(component(p)).unwrap();
        assert!(info.record.joint.is_none(), "{:?}", info.record);
        assert!(said_stays(&removed, occurrence(p)), "{removed}");
    }
    // Undo: the cross profile and its joints, the parts where they were.
    command(doc, json!({"cmd": "undo"})).unwrap();
    all_ok(doc);
    assert!(doc.assembly().component(x).is_some());
    assert_eq!(joints_on(doc, xo), joints_before);
    for p in freed {
        let joint = doc.part_info(component(p)).unwrap().record.joint;
        assert!(joint.is_some_and(|j| doc.feature(j).is_some()));
    }
    for (p, was) in stay.iter().zip(&before) {
        assert!(same(&placed(doc, occurrence(p)), was));
    }
}

#[test]
fn removing_the_cross_profile_leaves_the_upright_placed_on_its_end() {
    let mut doc = design();
    // An upright on the ground, the cross profile on its side, the second
    // upright on the cross profile's end, standing up.
    let u1 = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let x = on_side(&mut doc, &u1, "slot_x+_1", 500.0, 460.0);
    let u2 = on_end(&mut doc, &x, [0.0, 0.0, 1.0], 1000.0);
    remove_cross(&mut doc, &x, &[&u1, &u2], &[&u2]);
}

#[test]
fn removing_the_cross_profile_built_first_leaves_both_uprights() {
    let mut doc = design();
    // The reverse order: the cross profile on the ground, an upright on
    // its side and one on its end.
    let x = first(&mut doc, "p30", [0.0; 3], 460.0);
    let u1 = on_side(&mut doc, &x, "slot_x+_1", 100.0, 1000.0);
    let u2 = on_end(&mut doc, &x, [0.0, 1.0, 0.0], 1000.0);
    remove_cross(&mut doc, &x, &[&u1, &u2], &[&u1, &u2]);
}

#[test]
fn a_shaft_on_the_second_upright_stays_and_stays_when_the_upright_goes() {
    let mut doc = design();
    // The frame in a component of its own, the BUILD host while active.
    let (_, frame) = doc
        .create_component(Some("Frame"), Transform::IDENTITY, true)
        .unwrap();
    let u1 = first(&mut doc, "p30", [0.0; 3], 500.0);
    let x = on_side(&mut doc, &u1, "slot_x+_1", 250.0, 460.0);
    let u2 = on_end(&mut doc, &x, [0.0, 0.0, 1.0], 500.0);
    // A shaft beside the frame, moved down on the timeline and then joined
    // to the second upright's end from outside the frame.
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (_, shaft) = doc
        .create_component(Some("Shaft"), Transform::IDENTITY, false)
        .unwrap();
    command(
        &mut doc,
        json!({"cmd": "add_feature",
               "def": {"type": "move_occurrence", "occurrences": [shaft.to_string()],
                       "transform": {"type": "translate_xyz", "x": 0, "y": 0, "z": -21}}}),
    )
    .unwrap();
    let u2p = derived(&doc, component(&u2));
    let joint = command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"occurrence": shaft.to_string(), "geometry": "xy"},
               "b": {"occurrence": format!("{frame}/{}", occurrence(&u2)),
                     "geometry": {"mount": format!("{u2p}:mount(end_b)")}}}),
    )
    .unwrap();
    assert_eq!(joint["error"], Value::Null, "{joint}");
    all_ok(&doc);
    let shaft_at = placed(&doc, shaft);
    // The cross profile goes: the uprights and the shaft stay.
    let u2_at = placed(&doc, occurrence(&u2));
    let removed = command(
        &mut doc,
        json!({"cmd": "build_remove", "component": x["component"]}),
    )
    .unwrap();
    all_ok(&doc);
    assert_eq!(
        uids(&removed["removed"]["freed"]),
        uids(&json!([u2["component"]]))
    );
    assert!(same(&placed(&doc, occurrence(&u2)), &u2_at));
    assert!(same(&placed(&doc, shaft), &shaft_at), "{removed}");
    // The second upright goes too: the shaft joined to it stays where it
    // was, freed of it, its move still on the timeline.
    let removed = command(
        &mut doc,
        json!({"cmd": "build_remove", "component": u2["component"]}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(doc.assembly().occurrence(shaft).is_some());
    assert_eq!(joints_on(&doc, shaft), 0);
    let now = placed(&doc, shaft);
    assert!(
        same(&now, &shaft_at),
        "{now:?}, was {shaft_at:?}: {removed}"
    );
    assert!(said_stays(&removed, shaft), "{removed}");
    // Undo twice: all as it was.
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    all_ok(&doc);
    assert!(doc.assembly().component(component(&x)).is_some());
    assert!(same(&placed(&doc, shaft), &shaft_at));
    assert!(same(&placed(&doc, occurrence(&u2)), &u2_at));
    assert_eq!(joints_on(&doc, shaft), 1);
}

#[test]
fn deleting_the_cross_profiles_occurrence_leaves_the_upright_placed_on_it() {
    let mut doc = design();
    let u1 = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let x = on_side(&mut doc, &u1, "slot_x+_1", 500.0, 460.0);
    let u2 = on_end(&mut doc, &x, [0.0, 0.0, 1.0], 1000.0);
    let was = placed(&doc, occurrence(&u2));
    command(
        &mut doc,
        json!({"cmd": "delete_occurrence", "occurrence": x["occurrence"]}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(doc.assembly().component(component(&u2)).is_some());
    assert_eq!(joints_on(&doc, occurrence(&x)), 0);
    let now = placed(&doc, occurrence(&u2));
    assert!(same(&now, &was), "{now:?}, was {was:?}");
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    all_ok(&doc);
    assert!(same(&placed(&doc, occurrence(&u2)), &was));
    assert!(joints_on(&doc, occurrence(&x)) > 0);
}

/// Delete Component on a BUILD part (mitcad#1820): the view's context
/// menu takes the occurrence's component as a member, as the Delete key
/// does (`BrowserController::deleteOccurrences`: a component that
/// `build_parts` lists is a `build_remove`). A connector at a T so removed
/// takes along the hole it machined in a profile and the nut and screw it
/// added; the two profiles stay where they were; undo brings all back.
#[test]
fn delete_component_on_a_connector_takes_its_holes_and_fasteners_along() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let p = on_side(&mut doc, &t, "slot_x+_1", 500.0, 300.0);
    let junctions = query(&doc, json!({"query": "build_junctions"}));
    let id = junctions["junctions"][0]["id"].clone();
    let answer = command(
        &mut doc,
        json!({"cmd": "build_connect", "junction": id, "part": part("bracket30")}),
    )
    .unwrap();
    all_ok(&doc);
    let connector = component(&answer);
    let hole: mitcad_model::ids::FeatureUid = answer["adds"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|a| a.as_str().unwrap().parse().ok())
        .unwrap_or_else(|| panic!("no hole: {answer}"));
    let fasteners: Vec<ComponentUid> = doc
        .assembly()
        .components
        .iter()
        .filter(|c| {
            c.build
                .as_ref()
                .is_some_and(|b| b.added_by == Some(connector))
        })
        .map(|c| c.uid)
        .collect();
    assert_eq!(fasteners.len(), 2, "{answer}");
    let t_at = placed(&doc, occurrence(&t));
    let p_at = placed(&doc, occurrence(&p));
    // What the menu does with the right-clicked occurrence: its component
    // is a BUILD part, so it goes as one.
    let picked = occurrence(&answer);
    let of = doc.assembly().occurrence(picked).unwrap().component;
    let parts = query(&doc, json!({"query": "build_parts"}));
    assert!(
        parts["parts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|part| part["component"] == of.to_string()),
        "{parts}"
    );
    let removed = command(
        &mut doc,
        json!({"cmd": "build_remove", "component": of.to_string()}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(doc.assembly().component(connector).is_none());
    assert!(doc.feature(hole).is_none(), "the hole is left: {removed}");
    for c in &fasteners {
        assert!(
            doc.assembly().component(*c).is_none(),
            "a fastener is left: {removed}"
        );
    }
    assert!(same(&placed(&doc, occurrence(&t)), &t_at));
    assert!(same(&placed(&doc, occurrence(&p)), &p_at));
    // Undo: the connector, its hole and its fasteners back.
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    all_ok(&doc);
    assert!(doc.assembly().component(connector).is_some());
    assert!(doc.feature(hole).is_some());
    for c in &fasteners {
        assert!(doc.assembly().component(*c).is_some());
    }
    assert!(same(&placed(&doc, occurrence(&t)), &t_at));
    assert!(same(&placed(&doc, occurrence(&p)), &p_at));
}
