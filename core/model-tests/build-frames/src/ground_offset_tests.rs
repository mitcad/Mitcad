// SPDX-License-Identifier: MIT
//! Offset and Rotate on a plane or a face (mitcad#2046, GitHub
//! mitcad/Mitcad#446): a part placed on the leg of `face_normal_tests`
//! (its bottom face, normal -Z, centre (120, 230, 0)) stands `at.offset`
//! off the face along its normal, positive out of it, negative into it,
//! turned about its axis by any angle; a part lying on the XY plane is
//! lifted by its offset; on a profile's side the turn stays a quarter
//! turn; `build_edit` changes a ground part's offset and turn in one undo
//! step and `build_parts` reports them; the part follows its face when the
//! face's body moves; a design placed without them opens unchanged.

use serde_json::{Value, json};

use mitcad_vec3::dot;

use mitcad_model::Document;
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_tests_support::test_library::*;

use crate::face_normal_tests::{CENTRE, leg};

fn axis(t: &Transform) -> [f64; 3] {
    t.apply_vector([0.0, 0.0, 1.0])
}

fn x_of(t: &Transform) -> [f64; 3] {
    t.apply_vector([1.0, 0.0, 0.0])
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// Whether the part's x makes `degrees` with `along` (or its opposite),
/// either way round.
fn turned_from(t: &Transform, along: [f64; 3], degrees: f64) -> bool {
    near(dot(x_of(t), along).abs(), degrees.to_radians().cos().abs())
}

fn numbers(v: &Value) -> [f64; 3] {
    [0, 1, 2].map(|i| v[i].as_f64().unwrap_or_else(|| panic!("no number in {v}")))
}

/// The part's entry of `build_parts`.
fn entry(doc: &Document<MockKernel>, answer: &Value) -> Value {
    query(doc, json!({"query": "build_parts"}))["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["component"] == answer["component"])
        .cloned()
        .unwrap_or_else(|| panic!("no part {answer}"))
}

fn number(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("no number: {v}"))
}

#[test]
fn a_profile_on_a_face_stands_off_it_turned_at_any_angle() {
    let mut doc = design();
    let bottom = leg(&mut doc);
    // Face normal, Offset 20 mm, Rotate 30 deg: shown so before Place.
    let fields = json!({"part": part("p30"), "at": {"geometry": bottom, "offset": "20 mm"},
                        "length": 300, "rotate": "30 deg"});
    let mut probe = fields.clone();
    probe["query"] = json!("build_preview");
    let shown = query(&doc, probe);
    assert_eq!(shown["problems"], json!([]), "{shown}");
    assert!(
        close(numbers(&shown["frame"]["origin"]), [120.0, 230.0, -20.0]),
        "{shown}"
    );
    let t = place(&mut doc, fields);
    all_ok(&doc);
    assert_eq!(t["place"], "geometry", "{t}");
    let at = placed(&doc, occurrence(&t));
    // 20 out of the face's centre along its normal (down), turned 30 deg
    // from the face's longest edge (Y) about its axis.
    assert!(close(at.translation, [120.0, 230.0, -20.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, -1.0]), "{at:?}");
    assert!(turned_from(&at, [0.0, 1.0, 0.0], 30.0), "{at:?}");
    let e = entry(&doc, &t);
    assert_eq!(e["ground"], true, "{e}");
    assert!(near(number(&e["offset"]), 20.0), "{e}");
    assert!(near(number(&e["rotate"]), 30f64.to_radians()), "{e}");

    // Offset -20 mm goes into the face (the leg).
    let into = place(
        &mut doc,
        json!({"part": {"source": "S1"}, "at": {"geometry": bottom, "offset": -20},
               "length": 300}),
    );
    all_ok(&doc);
    let at = placed(&doc, occurrence(&into));
    assert!(close(at.translation, [120.0, 230.0, 20.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, -1.0]), "{at:?}");
    assert!(near(number(&entry(&doc, &into)["offset"]), -20.0));
}

#[test]
fn a_part_lying_on_the_xy_plane_is_lifted_by_its_offset() {
    let mut doc = design();
    let lying = |point: [f64; 3], offset: f64| {
        json!({"part": part("p30"),
               "at": {"geometry": "xy", "point": point, "direction": [1.0, 0.0, 0.0],
                      "offset": offset},
               "length": 300})
    };
    let resting = place(&mut doc, lying([0.0, 0.0, 0.0], 0.0));
    let lifted = place(&mut doc, lying([0.0, 500.0, 0.0], 50.0));
    all_ok(&doc);
    let rest = placed(&doc, occurrence(&resting));
    let lift = placed(&doc, occurrence(&lifted));
    assert!(close(axis(&lift), [1.0, 0.0, 0.0]), "{lift:?}");
    assert!(close(x_of(&lift), x_of(&rest)), "{lift:?} {rest:?}");
    assert!(
        close(
            lift.translation,
            [
                rest.translation[0],
                rest.translation[1] + 500.0,
                rest.translation[2] + 50.0
            ]
        ),
        "{lift:?} {rest:?}"
    );
    assert!(near(number(&entry(&doc, &lifted)["offset"]), 50.0));
    // Offset 0 writes no shift: the joint as before mitcad#2046.
    let resting_joint = entry(&doc, &resting)["joint"].clone();
    let joint = query(&doc, json!({"query": "feature", "uid": resting_joint}));
    assert!(
        joint["def"]["b"]["frame_override"].get("shift").is_none(),
        "{joint}"
    );
}

#[test]
fn rotate_30_on_a_profile_s_side_is_still_refused() {
    let mut doc = design();
    doc.kernel().measures_selections.set(true);
    let t = first(&mut doc, "p20", [0.0; 3], 500.0);
    let tp = derived(&doc, component(&t));
    let slot = json!({"mount": format!("{tp}:mount(slot_y+_1)")});
    let refused = command(
        &mut doc,
        json!({"cmd": "build_place", "part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"], "geometry": slot}, "length": 200,
               "rotate": "30 deg"}),
    )
    .unwrap_err();
    assert!(refused.contains("0, 90, 180 or 270"), "{refused}");
    // Placed square, it is not turned 30 deg by an edit either.
    let side = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"], "geometry": slot}, "length": 200}),
    );
    let refused = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": side["component"], "rotate": "30 deg"}),
    )
    .unwrap_err();
    assert!(refused.contains("0, 90, 180 or 270"), "{refused}");
    assert_eq!(entry(&doc, &side)["ground"], false);
}

#[test]
fn build_edit_changes_a_ground_part_s_offset_and_rotate_in_one_undo_step() {
    let mut doc = design();
    let bottom = leg(&mut doc);
    let t = place(
        &mut doc,
        json!({"part": part("p30"), "at": {"geometry": bottom}, "length": 300}),
    );
    all_ok(&doc);
    let e = entry(&doc, &t);
    assert_eq!(e["ground"], true, "{e}");
    assert!(near(number(&e["offset"]), 0.0), "{e}");

    let answer = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": t["component"], "at": {"offset": "20 mm"},
               "rotate": "30 deg"}),
    )
    .unwrap();
    let changed = answer["changed"].as_array().unwrap();
    assert!(changed.iter().any(|v| v == "offset"), "{answer}");
    assert!(changed.iter().any(|v| v == "rotate"), "{answer}");
    all_ok(&doc);
    let at = placed(&doc, occurrence(&t));
    assert!(close(at.translation, [120.0, 230.0, -20.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, -1.0]), "{at:?}");
    assert!(turned_from(&at, [0.0, 1.0, 0.0], 30.0), "{at:?}");
    let e = entry(&doc, &t);
    assert!(near(number(&e["offset"]), 20.0), "{e}");
    assert!(near(number(&e["rotate"]), 30f64.to_radians()), "{e}");

    // A From has no meaning there.
    let refused = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": t["component"], "at": {"offset": 5, "from": "a"}}),
    )
    .unwrap_err();
    assert!(refused.contains("plane or a face"), "{refused}");

    // One undo step takes both back.
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    let at = placed(&doc, occurrence(&t));
    assert!(close(at.translation, CENTRE), "{at:?}");
    assert!(turned_from(&at, [0.0, 1.0, 0.0], 0.0), "{at:?}");
    let e = entry(&doc, &t);
    assert!(near(number(&e["offset"]), 0.0), "{e}");
    assert!(near(number(&e["rotate"]), 0.0), "{e}");
}

#[test]
fn the_offset_part_follows_its_face_when_the_face_s_body_moves() {
    let mut doc = design();
    let bottom = leg(&mut doc);
    let t = place(
        &mut doc,
        json!({"part": part("p30"), "at": {"geometry": bottom, "offset": 20}, "length": 300}),
    );
    all_ok(&doc);
    let feature = bottom["body"]
        .as_str()
        .unwrap()
        .split('.')
        .next()
        .unwrap()
        .to_owned();
    // The leg moved 200 along X: its bottom face's centre to (320, 230, 0).
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": feature,
               "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
                       "corner": [300, 200], "length": 40, "width": 60, "height": 500,
                       "operation": "new_body"}}),
    )
    .unwrap_or_else(|e| panic!("moving the leg: {e}"));
    all_ok(&doc);
    let at = placed(&doc, occurrence(&t));
    assert!(close(at.translation, [320.0, 230.0, -20.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, -1.0]), "{at:?}");
    assert!(near(number(&entry(&doc, &t)["offset"]), 20.0));
}

#[test]
fn a_design_placed_without_offset_opens_unchanged() {
    let mut doc = design();
    let bottom = leg(&mut doc);
    let t = place(
        &mut doc,
        json!({"part": part("p30"), "at": {"geometry": bottom}, "length": 300,
               "rotate": "90 deg"}),
    );
    all_ok(&doc);
    let before = placed(&doc, occurrence(&t));
    // Its joint has no shift: as before mitcad#2046.
    let joint = query(
        &doc,
        json!({"query": "feature", "uid": entry(&doc, &t)["joint"]}),
    );
    assert!(
        joint["def"]["b"]["frame_override"].get("shift").is_none(),
        "{joint}"
    );
    let text = doc.to_json();
    // Opened with the same mock faces as the leg's.
    let kernel = doc.kernel().fork().expect("the mock kernel forks");
    let mut opened = Document::from_json(&text, kernel).unwrap();
    opened.recompute();
    all_ok(&opened);
    let after = placed(&opened, occurrence(&t));
    assert!(close(after.translation, before.translation), "{after:?}");
    assert!(close(axis(&after), axis(&before)), "{after:?}");
    assert!(close(x_of(&after), x_of(&before)), "{after:?}");
    let e = entry(&opened, &t);
    assert_eq!(e["ground"], true, "{e}");
    assert!(near(number(&e["offset"]), 0.0), "{e}");
    assert!(near(number(&e["rotate"]), 90f64.to_radians()), "{e}");
}

/// Rotate 90° on a part on a face (mitcad#2046): the app sends the turn a
/// quarter further, 30 deg then 120 deg, which the model takes as it is,
/// in one undo step.
#[test]
fn rotate_90_turns_a_ground_part_a_quarter_from_any_angle() {
    let mut doc = design();
    let bottom = leg(&mut doc);
    let t = place(
        &mut doc,
        json!({"part": part("p30"), "at": {"geometry": bottom}, "length": 300,
               "rotate": "30 deg"}),
    );
    all_ok(&doc);
    assert!(near(number(&entry(&doc, &t)["rotate"]), 30f64.to_radians()));
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": t["component"], "rotate": "120 deg"}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(near(
        number(&entry(&doc, &t)["rotate"]),
        120f64.to_radians()
    ));
    let at = placed(&doc, occurrence(&t));
    assert!(close(at.translation, CENTRE), "{at:?}");
    assert!(turned_from(&at, [0.0, 1.0, 0.0], 120.0), "{at:?}");
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    assert!(near(number(&entry(&doc, &t)["rotate"]), 30f64.to_radians()));
    assert!(turned_from(
        &placed(&doc, occurrence(&t)),
        [0.0, 1.0, 0.0],
        30.0
    ));
}
