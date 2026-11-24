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
use mitcad_model::parameters::ParamId;
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
    try_drag_along(doc, carriage, by).unwrap();
    all_ok(doc);
}

/// [`drag_along`], its refusal kept.
fn try_drag_along(
    doc: &mut Document<MockKernel>,
    carriage: &Value,
    by: f64,
) -> Result<Value, String> {
    let along = placed(doc, occurrence(carriage)).apply_vector([1.0, 0.0, 0.0]);
    drag_by(doc, carriage, along, by)
}

/// Drags a part `by` along the direction `along`.
fn drag_by(
    doc: &mut Document<MockKernel>,
    part: &Value,
    along: [f64; 3],
    by: f64,
) -> Result<Value, String> {
    let at = placed(doc, occurrence(part)).translation;
    let target: [f64; 3] = std::array::from_fn(|i| at[i] + along[i] * by);
    command(
        doc,
        json!({"cmd": "drag_occurrence", "occurrence": part["occurrence"], "point": at,
               "target": target}),
    )
}

/// The parameter a carriage's slide is driven by (its Placement Offset).
fn slide_parameter(doc: &Document<MockKernel>, carriage: &Value) -> ParamId {
    let joint: FeatureUid = carriage["joint"].as_str().unwrap().parse().unwrap();
    let FeatureDef::Joint(j) = &doc.feature(joint).unwrap().def else {
        panic!("a joint");
    };
    j.position[&Motion::Tx]
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

/// A part's Placement Offset as `build_parts` gives it (BUILD Placement
/// fills its Offset from it).
fn build_offset(doc: &Document<MockKernel>, part: &Value) -> Value {
    query(doc, json!({"query": "build_parts"}))["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["component"] == part["component"])
        .unwrap_or_else(|| panic!("no part {part}"))["offset"]
        .clone()
}

fn assert_offsets(doc: &Document<MockKernel>, b1: &Value, b2: &Value, first: f64, second: f64) {
    let (o1, o2) = (build_offset(doc, b1), build_offset(doc, b2));
    let at = |o: &Value, want: f64| o.as_f64().is_some_and(|v| (v - want).abs() < 1e-4);
    assert!(
        at(&o1, first) && at(&o2, second),
        "offsets {o1}, {o2} instead of {first}, {second}"
    );
}

/// Two bearings on a shaft (GitHub mitcad/Mitcad#575): a bearing slides
/// along its shaft frame's z, and BUILD records no `from` for it, yet
/// `build_parts` gives its Placement Offset (from end A, as `build_edit`
/// takes it), before and after a Rigid Group joins the two; an Offset
/// edit of one moves the other with it.
#[test]
fn bearings_on_a_shaft_have_placement_offsets_and_move_together() {
    let mut doc = design();
    let shaft = first(&mut doc, "shaft8", [0.0; 3], 100.0);
    let sp = derived(&doc, component(&shaft));
    let bearing = |doc: &mut Document<MockKernel>, offset: f64| {
        place(
            doc,
            json!({"part": part("bearing8"),
                   "at": {"occurrence": shaft["occurrence"], "geometry": {"mount": format!("{sp}:mount(shaft)")},
                          "offset": offset}}),
        )
    };
    let b1 = bearing(&mut doc, 20.0);
    let b2 = bearing(&mut doc, 80.0);
    all_ok(&doc);
    assert_offsets(&doc, &b1, &b2, 20.0, 80.0);
    // The Offset edited and read back.
    offset_to(&mut doc, &b1, 30.0).unwrap();
    all_ok(&doc);
    assert_offsets(&doc, &b1, &b2, 30.0, 80.0);
    // A Rigid Group keeps them where they are; an edit of one moves both.
    let group = command(
        &mut doc,
        json!({"cmd": "add_rigid_group", "occurrences": [b1["occurrence"], b2["occurrence"]]}),
    )
    .unwrap();
    assert_eq!(group["error"], Value::Null, "{group}");
    all_ok(&doc);
    assert_offsets(&doc, &b1, &b2, 30.0, 80.0);
    offset_to(&mut doc, &b1, 20.0).unwrap();
    all_ok(&doc);
    assert_offsets(&doc, &b1, &b2, 20.0, 70.0);
}

/// Sets a carriage's offset to an expression (a parameter's name or a
/// formula) with BUILD's Placement.
fn offset_expression(doc: &mut Document<MockKernel>, carriage: &Value, expression: &str) {
    command(
        doc,
        json!({"cmd": "build_edit", "component": carriage["component"],
               "at": {"offset": expression}}),
    )
    .unwrap();
    all_ok(doc);
}

fn add_parameter(doc: &mut Document<MockKernel>, name: &str, expression: &str) -> ParamId {
    command(
        doc,
        json!({"cmd": "add_parameter", "name": name, "expression": expression}),
    )
    .unwrap();
    doc.parameters().find(name).unwrap()
}

/// A follower whose offset is a user parameter that only it uses keeps
/// the parameter (mitcad#2317): a drag or a Placement edit of the other
/// carriage moves it, and the parameter takes its new offset.
#[test]
fn a_follower_s_offset_as_a_user_parameter_keeps_the_parameter() {
    let mut doc = design();
    let (_, c1, c2) = rail_and_two_carriages(&mut doc, 100.0, 250.0);
    let second_at = add_parameter(&mut doc, "second_at", "250 mm");
    offset_expression(&mut doc, &c2, "second_at");
    assert_eq!(slide_parameter(&doc, &c2), second_at);
    let added = rigid_where_they_are(&mut doc, &c1, &c2);
    assert_eq!(added["error"], Value::Null, "{added}");
    all_ok(&doc);
    assert_slides(&doc, &c1, &c2, 100.0, 250.0);
    // A drag of the first moves both; the second's offset is still the
    // parameter, now where the second is.
    drag_along(&mut doc, &c1, 50.0);
    assert_slides(&doc, &c1, &c2, 150.0, 300.0);
    assert_eq!(slide_parameter(&doc, &c2), second_at);
    let value = doc.parameters().value(second_at).unwrap();
    assert!(near(value, 300.0), "second_at is {value}");
    // So does a Placement edit of the first.
    offset_to(&mut doc, &c1, 120.0).unwrap();
    all_ok(&doc);
    assert_slides(&doc, &c1, &c2, 120.0, 270.0);
    assert_eq!(slide_parameter(&doc, &c2), second_at);
    assert!(near(doc.parameters().value(second_at).unwrap(), 270.0));
    // Undone in one step: the parameter is back too.
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    assert_slides(&doc, &c1, &c2, 150.0, 300.0);
    assert!(near(doc.parameters().value(second_at).unwrap(), 300.0));
}

/// A follower placed from End B keeps BUILD's form of its Offset (an
/// expression of the rail's length): only its number changes, and it
/// stays measured from End B (mitcad#2317).
#[test]
fn a_follower_placed_from_end_b_stays_measured_from_there() {
    let mut doc = design();
    let (_, c1, c2) = rail_and_two_carriages(&mut doc, 100.0, 250.0);
    // 150 from End B of the rail 400 long: where it is.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": c2["component"],
               "at": {"offset": 150, "from": "b"}}),
    )
    .unwrap();
    all_ok(&doc);
    assert_slides(&doc, &c1, &c2, 100.0, 250.0);
    let own = slide_parameter(&doc, &c2);
    let formula = |doc: &Document<MockKernel>| {
        let p = doc.parameters().get(own).unwrap();
        (p.expr().is_plain_number(), p.expression().to_owned())
    };
    assert!(!formula(&doc).0, "{}", formula(&doc).1);
    let added = rigid_where_they_are(&mut doc, &c1, &c2);
    assert_eq!(added["error"], Value::Null, "{added}");
    all_ok(&doc);
    drag_along(&mut doc, &c1, 50.0);
    assert_slides(&doc, &c1, &c2, 150.0, 300.0);
    assert_eq!(slide_parameter(&doc, &c2), own);
    let (plain, text) = formula(&doc);
    assert!(!plain, "{text}");
    assert_offsets(&doc, &c1, &c2, 150.0, 100.0);
}

/// A follower whose offset is by formula, or a parameter something else
/// uses as well, cannot take where the other carriage would move it: the
/// move is refused in words, and the expression stays (mitcad#2317).
#[test]
fn a_follower_s_offset_by_formula_refuses_the_move_in_words() {
    let mut doc = design();
    let (_, c1, c2) = rail_and_two_carriages(&mut doc, 100.0, 250.0);
    add_parameter(&mut doc, "base", "100 mm");
    offset_expression(&mut doc, &c2, "base + 150 mm");
    let own = slide_parameter(&doc, &c2);
    let expression = |doc: &Document<MockKernel>| {
        doc.parameters()
            .get(own)
            .map(|p| p.expression().to_owned())
            .unwrap_or_default()
    };
    assert!(expression(&doc).contains("base"), "{}", expression(&doc));
    let added = rigid_where_they_are(&mut doc, &c1, &c2);
    assert_eq!(added["error"], Value::Null, "{added}");
    all_ok(&doc);
    assert_slides(&doc, &c1, &c2, 100.0, 250.0);
    // A drag of the first is refused, in words; nothing moves.
    let refused = try_drag_along(&mut doc, &c1, 50.0).unwrap_err();
    assert!(
        refused.contains("cannot move with") && refused.contains("by formula"),
        "{refused}"
    );
    assert_slides(&doc, &c1, &c2, 100.0, 250.0);
    assert!(expression(&doc).contains("base"), "{}", expression(&doc));
    // So is a Placement edit.
    let refused = offset_to(&mut doc, &c1, 120.0).unwrap_err();
    assert!(refused.contains("by formula"), "{refused}");
    assert_slides(&doc, &c1, &c2, 100.0, 250.0);
    // A user parameter that another parameter uses as well.
    let shared = add_parameter(&mut doc, "shared", "250 mm");
    add_parameter(&mut doc, "twice", "shared * 2");
    offset_expression(&mut doc, &c2, "shared");
    assert_eq!(slide_parameter(&doc, &c2), shared);
    let refused = try_drag_along(&mut doc, &c1, 50.0).unwrap_err();
    assert!(
        refused.contains("cannot move with") && refused.contains("twice"),
        "{refused}"
    );
    assert_slides(&doc, &c1, &c2, 100.0, 250.0);
    assert!(near(doc.parameters().value(shared).unwrap(), 250.0));
}

/// One `build_parts` entry.
fn build_part(doc: &Document<MockKernel>, part: &Value) -> Value {
    query(doc, json!({"query": "build_parts"}))["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["component"] == part["component"])
        .unwrap_or_else(|| panic!("no part {part}"))
        .clone()
}

/// Bearings on a shaft have travel limits as carriages on a rail do
/// (mitcad#2317): their slide is along the shaft frame's z, from flush
/// with the shaft's end A to flush with its end B (a bearing is 24 long,
/// its frame in its middle: 12 to 88 on a shaft 100 long). A coupled drag
/// stops both where the first reaches a limit; an Offset past one is
/// refused.
#[test]
fn bearings_on_a_shaft_have_travel_limits_and_a_coupled_drag_stops_at_the_first() {
    let mut doc = design();
    let shaft = first(&mut doc, "shaft8", [0.0; 3], 100.0);
    let sp = derived(&doc, component(&shaft));
    let bearing = |doc: &mut Document<MockKernel>, offset: f64| {
        place(
            doc,
            json!({"part": part("bearing8"),
                   "at": {"occurrence": shaft["occurrence"], "geometry": {"mount": format!("{sp}:mount(shaft)")},
                          "offset": offset}}),
        )
    };
    let b1 = bearing(&mut doc, 20.0);
    let b2 = bearing(&mut doc, 80.0);
    all_ok(&doc);
    for b in [&b1, &b2] {
        let entry = build_part(&doc, b);
        assert_eq!(entry["slides"], json!(true), "{entry}");
        let limits = &entry["limits"];
        let is = |key: &str, want: f64| limits[key].as_f64().is_some_and(|v| near(v, want));
        assert!(
            is("min", 12.0)
                && is("max", 88.0)
                && is("default_min", 12.0)
                && is("default_max", 88.0),
            "{limits}"
        );
    }
    let group = command(
        &mut doc,
        json!({"cmd": "add_rigid_group", "occurrences": [b1["occurrence"], b2["occurrence"]]}),
    )
    .unwrap();
    assert_eq!(group["error"], Value::Null, "{group}");
    all_ok(&doc);
    assert_offsets(&doc, &b1, &b2, 20.0, 80.0);
    // Along the shaft.
    let along = placed(&doc, occurrence(&shaft)).apply_vector([0.0, 0.0, 1.0]);
    // A drag of either moves both.
    drag_by(&mut doc, &b1, along, -5.0).unwrap();
    all_ok(&doc);
    assert_offsets(&doc, &b1, &b2, 15.0, 75.0);
    // Past the second's Max (88): both stop there.
    drag_by(&mut doc, &b1, along, 100.0).unwrap();
    all_ok(&doc);
    assert_offsets(&doc, &b1, &b2, 28.0, 88.0);
    // Past the first's Min (12), dragging the second: both stop there.
    drag_by(&mut doc, &b2, along, -100.0).unwrap();
    all_ok(&doc);
    assert_offsets(&doc, &b1, &b2, 12.0, 72.0);
    // An Offset past a limit is refused in words.
    let refused = offset_to(&mut doc, &b2, 95.0).unwrap_err();
    assert!(refused.contains("beyond its travel"), "{refused}");
    assert_offsets(&doc, &b1, &b2, 12.0, 72.0);
}
