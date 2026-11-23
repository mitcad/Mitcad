// SPDX-License-Identifier: MIT
//! Two carriages on one rail that move together (mitcad#2274): a Rigid
//! joint between them, or a Rigid Group of the two, holds; the carriage of
//! side b (of the group, the members after the first) follows the other,
//! its stored offset rewritten from where it is. A drag or a Placement
//! edit of either moves both, each within its travel limits.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::assembly::inverse;
use mitcad_model::features::FeatureDef;
use mitcad_model::ids::FeatureUid;
use mitcad_model::joints::Motion;
use mitcad_model::testing::MockKernel;
use mitcad_model_tests_support::test_library::*;

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// The value a carriage's slide is driven to (its Placement Offset).
fn slide(doc: &Document<MockKernel>, carriage: &Value) -> f64 {
    let joint: FeatureUid = carriage["joint"].as_str().unwrap().parse().unwrap();
    let FeatureDef::Joint(j) = &doc.feature(joint).unwrap().def else {
        panic!("a joint");
    };
    doc.parameters().value(j.position[&Motion::Tx]).unwrap()
}

fn assert_slides(doc: &Document<MockKernel>, c1: &Value, c2: &Value, first: f64, second: f64) {
    let (s1, s2) = (slide(doc, c1), slide(doc, c2));
    assert!(
        near(s1, first) && near(s2, second),
        "slides {s1}, {s2} instead of {first}, {second}"
    );
}

/// An upright, a rail 400 long on it from 100, and two carriages on the
/// rail `at1` and `at2` from its end A (to each carriage's frame, its
/// middle: a carriage is 40 long, so its travel is 20 to 380).
fn rail_and_two_carriages(
    doc: &mut Document<MockKernel>,
    at1: f64,
    at2: f64,
) -> (Value, Value, Value) {
    let t = first(doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(doc, component(&t));
    let rail = place(
        doc,
        json!({"part": part("rail15"),
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(slot_x+_1)")},
                      "offset": 100},
               "length": 400}),
    );
    let rp = derived(doc, component(&rail));
    let carriage = |doc: &mut Document<MockKernel>, offset: f64| {
        place(
            doc,
            json!({"part": part("carriage15"),
                   "at": {"occurrence": rail["occurrence"], "geometry": {"mount": format!("{rp}:mount(rail)")},
                          "offset": offset}}),
        )
    };
    let c1 = carriage(doc, at1);
    let c2 = carriage(doc, at2);
    all_ok(doc);
    (rail, c1, c2)
}

/// A carriage's top frame as a joint origin.
fn top(doc: &Document<MockKernel>, carriage: &Value) -> Value {
    let part = derived(doc, component(carriage));
    json!({"occurrence": carriage["occurrence"], "geometry": {"mount": format!("{part}:mount(top)")}})
}

/// A Rigid joint of `c1`'s top (a) on `c2`'s top (b) where they are: b's
/// frame shifted along the rail to a's.
fn rigid_where_they_are(doc: &mut Document<MockKernel>, c1: &Value, c2: &Value) -> Value {
    let shift =
        inverse(&placed(doc, occurrence(c2))).apply_point(placed(doc, occurrence(c1)).translation);
    let mut b = top(doc, c2);
    b["frame_override"] = json!({"shift": shift});
    let a = top(doc, c1);
    command(
        doc,
        json!({"cmd": "add_joint", "kind": "rigid", "a": a, "b": b}),
    )
    .unwrap()
}

/// Drags a carriage `by` along its rail.
fn drag_along(doc: &mut Document<MockKernel>, carriage: &Value, by: f64) {
    let placement = placed(doc, occurrence(carriage));
    let at = placement.translation;
    let along = placement.apply_vector([1.0, 0.0, 0.0]);
    let target: [f64; 3] = std::array::from_fn(|i| at[i] + along[i] * by);
    command(
        doc,
        json!({"cmd": "drag_occurrence", "occurrence": carriage["occurrence"], "point": at,
               "target": target}),
    )
    .unwrap();
    all_ok(doc);
}

fn offset_to(
    doc: &mut Document<MockKernel>,
    carriage: &Value,
    offset: f64,
) -> Result<Value, String> {
    command(
        doc,
        json!({"cmd": "build_edit", "component": carriage["component"], "at": {"offset": offset}}),
    )
}

/// The distance between the carriages' frames.
fn apart(doc: &Document<MockKernel>, c1: &Value, c2: &Value) -> f64 {
    let (a, b) = (
        placed(doc, occurrence(c1)).translation,
        placed(doc, occurrence(c2)).translation,
    );
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

#[test]
fn a_rigid_joint_between_two_carriages_holds_and_both_move() {
    let mut doc = design();
    let (_, c1, c2) = rail_and_two_carriages(&mut doc, 100.0, 250.0);
    let added = rigid_where_they_are(&mut doc, &c1, &c2);
    assert_eq!(added["error"], Value::Null, "{added}");
    all_ok(&doc);
    assert_slides(&doc, &c1, &c2, 100.0, 250.0);
    // A drag of either moves both.
    drag_along(&mut doc, &c1, 50.0);
    assert_slides(&doc, &c1, &c2, 150.0, 300.0);
    assert!(near(apart(&doc, &c1, &c2), 150.0));
    drag_along(&mut doc, &c2, -100.0);
    assert_slides(&doc, &c1, &c2, 50.0, 200.0);
    // A drag past the first's travel (from 20) stops both there.
    drag_along(&mut doc, &c2, -100.0);
    assert_slides(&doc, &c1, &c2, 20.0, 170.0);
    // Past the second's (to 380), too.
    drag_along(&mut doc, &c1, 1000.0);
    assert_slides(&doc, &c1, &c2, 230.0, 380.0);
    assert!(near(apart(&doc, &c1, &c2), 150.0));
    // A Placement edit of either moves both.
    offset_to(&mut doc, &c1, 100.0).unwrap();
    all_ok(&doc);
    assert_slides(&doc, &c1, &c2, 100.0, 250.0);
    offset_to(&mut doc, &c2, 300.0).unwrap();
    all_ok(&doc);
    assert_slides(&doc, &c1, &c2, 150.0, 300.0);
    assert!(near(apart(&doc, &c1, &c2), 150.0));
    // One that would take the other beyond its travel is refused in words.
    let refused = offset_to(&mut doc, &c1, 300.0).unwrap_err();
    assert!(refused.contains("beyond its travel"), "{refused}");
    assert_slides(&doc, &c1, &c2, 150.0, 300.0);
}

#[test]
fn a_rigid_group_of_two_carriages_moves_both() {
    let mut doc = design();
    let (_, c1, c2) = rail_and_two_carriages(&mut doc, 100.0, 250.0);
    let group = command(
        &mut doc,
        json!({"cmd": "add_rigid_group", "occurrences": [c1["occurrence"], c2["occurrence"]]}),
    )
    .unwrap();
    assert_eq!(group["error"], Value::Null, "{group}");
    all_ok(&doc);
    assert_slides(&doc, &c1, &c2, 100.0, 250.0);
    drag_along(&mut doc, &c2, 50.0);
    assert_slides(&doc, &c1, &c2, 150.0, 300.0);
    // Past the second's travel (to 380): both stop there.
    drag_along(&mut doc, &c1, 1000.0);
    assert_slides(&doc, &c1, &c2, 230.0, 380.0);
    offset_to(&mut doc, &c1, 100.0).unwrap();
    all_ok(&doc);
    assert_slides(&doc, &c1, &c2, 100.0, 250.0);
    assert!(near(apart(&doc, &c1, &c2), 150.0));
    offset_to(&mut doc, &c2, 200.0).unwrap();
    all_ok(&doc);
    assert_slides(&doc, &c1, &c2, 50.0, 200.0);
    let refused = offset_to(&mut doc, &c2, 100.0).unwrap_err();
    assert!(refused.contains("beyond its travel"), "{refused}");
    assert_slides(&doc, &c1, &c2, 50.0, 200.0);
}

#[test]
fn a_rigid_joint_moves_side_b_s_carriage_and_rewrites_its_offset() {
    let mut doc = design();
    let (_, c1, c2) = rail_and_two_carriages(&mut doc, 100.0, 250.0);
    let c1_at = placed(&doc, occurrence(&c1)).translation;
    // Top on top at offset 0: the second goes where the first is.
    let (a, b) = (top(&doc, &c1), top(&doc, &c2));
    let added = command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "rigid", "a": a, "b": b}),
    )
    .unwrap();
    assert_eq!(added["error"], Value::Null, "{added}");
    all_ok(&doc);
    assert!(close(placed(&doc, occurrence(&c2)).translation, c1_at));
    assert!(close(placed(&doc, occurrence(&c1)).translation, c1_at));
    assert_slides(&doc, &c1, &c2, 100.0, 100.0);
    // Undone in one step: the second is back at its offset.
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    assert_slides(&doc, &c1, &c2, 100.0, 250.0);
}

#[test]
fn a_rigid_joint_whose_carriage_cannot_reach_fails_in_words() {
    let mut doc = design();
    let (_, c1, c2) = rail_and_two_carriages(&mut doc, 100.0, 250.0);
    // The second 300 past the first: beyond its travel (to 380).
    let mut b = top(&doc, &c2);
    // (Toward the first's end of the rail in the second's frame.)
    let shift_sign = inverse(&placed(&doc, occurrence(&c2)))
        .apply_point(placed(&doc, occurrence(&c1)).translation)[0]
        .signum();
    b["frame_override"] = json!({"shift": [300.0 * shift_sign, 0.0, 0.0]});
    let a = top(&doc, &c1);
    let added = command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "rigid", "a": a, "b": b}),
    )
    .unwrap();
    let error = added["error"].as_str().unwrap_or_default();
    assert!(error.contains("cannot"), "{added}");
    // The second stays where its offset puts it.
    assert_slides(&doc, &c1, &c2, 100.0, 250.0);
}
