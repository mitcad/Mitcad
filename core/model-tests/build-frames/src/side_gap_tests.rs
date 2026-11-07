// SPDX-License-Identifier: MIT
//! The middle of a free stretch of a side (mitcad#1401), with the mock
//! kernel: on a side with a cross member at its middle, a click near the
//! middle of either half snaps to the middle of that free stretch, and the
//! part placed there stays centred between the cross member and the
//! side's end when the cross member moves or the side's length changes;
//! when the cross member goes, each keeps its place, bound to the nearer
//! end of the side. An edit to Middle of Gap keeps a part in place and
//! binds it the same way.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::FeatureDef;
use mitcad_model::testing::MockKernel;
use mitcad_model_tests_support::test_library::*;

/// A 30 profile 1000 long standing at the origin (the side), and its slot
/// `slot_y+_1` as `at.geometry`.
fn target(doc: &mut Document<MockKernel>) -> (Value, Value) {
    let t = first(doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(doc, component(&t));
    (t, json!({"mount": format!("{tp}:mount(slot_y+_1)")}))
}

fn side_at(t: &Value, slot: &Value, at: Value) -> Value {
    let mut fields = json!({"occurrence": t["occurrence"], "geometry": slot});
    for (key, value) in at.as_object().unwrap() {
        fields[key] = value.clone();
    }
    fields
}

/// A 30 profile 200 long on the side's y+ slot with `at` merged in.
fn on_side(doc: &mut Document<MockKernel>, t: &Value, slot: &Value, at: Value) -> Value {
    place(
        doc,
        json!({"part": {"source": "S1"}, "at": side_at(t, slot, at), "length": 200}),
    )
}

/// What `build_preview` says of a part 200 long on the side.
fn preview(doc: &Document<MockKernel>, t: &Value, slot: &Value, at: Value) -> Value {
    query(
        doc,
        json!({"query": "build_preview", "part": {"source": "S1"}, "length": 200,
               "at": side_at(t, slot, at)}),
    )
}

/// Where a part on the side stands along it (its centre's z).
fn along(doc: &Document<MockKernel>, answer: &Value) -> f64 {
    placed(doc, occurrence(answer)).translation[2]
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn assert_at(doc: &Document<MockKernel>, answer: &Value, expected: f64) {
    let at = along(doc, answer);
    assert!(near(at, expected), "{at} instead of {expected}");
}

/// Sets the side's length.
fn lengthen(doc: &mut Document<MockKernel>, t: &Value, length: f64) {
    let id = doc.part_info(component(t)).unwrap().length.unwrap().0;
    let name = doc.parameters().name(id);
    command(
        doc,
        json!({"cmd": "set_parameter", "name": name, "value": length}),
    )
    .unwrap();
    all_ok(doc);
}

/// The record's reference of a placed part.
fn from_of(doc: &Document<MockKernel>, answer: &Value) -> Option<&'static str> {
    doc.part_info(component(answer))
        .and_then(|i| i.record.from)
        .map(|f| f.as_str())
}

/// The name and the expression of the parameter a placed part's joint
/// slides by.
fn slide(doc: &Document<MockKernel>, answer: &Value) -> (String, String) {
    let joint = answer["joint"].as_str().unwrap().parse().unwrap();
    let FeatureDef::Joint(j) = &doc.feature(joint).unwrap().def else {
        panic!("a joint");
    };
    let id = j.position[&mitcad_model::joints::Motion::Tx];
    let p = doc.parameters().get(id).unwrap();
    (p.name().to_owned(), p.expression().to_owned())
}

#[test]
fn a_profile_in_the_middle_of_a_free_stretch_stays_centred_between_the_profiles() {
    let mut doc = design();
    let (t, slot) = target(&mut doc);
    // A cross member at the side's middle: 485 to 515.
    let m = on_side(&mut doc, &t, &slot, json!({"point": [0.0, 15.0, 503.0]}));
    assert_at(&doc, &m, 500.0);
    // The free stretches either side of it, 0 to 485 and 515 to 1000, are
    // snaps at their middles (242.5 and 757.5, about a quarter and three
    // quarters of the side; the snaps' offsets are the near faces).
    let shown = preview(&doc, &t, &slot, json!({"point": [0.0, 15.0, 246.0]}));
    let gaps: Vec<f64> = shown["snaps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["kind"] == "gap")
        .map(|s| s["offset"].as_f64().unwrap())
        .collect();
    assert_eq!(gaps.len(), 2, "{}", shown["snaps"]);
    assert!(near(gaps[0], 227.5) && near(gaps[1], 742.5), "{gaps:?}");
    // A click near the first one's middle takes it: from the middle of the
    // gap, offset 0, between end A and the cross member.
    let side = &shown["side"];
    assert_eq!(side["kind"], "gap", "{side}");
    assert_eq!(side["from"], "gap");
    assert!(near(side["offset"].as_f64().unwrap(), 0.0), "{side}");
    assert!(near(side["gap"]["start"].as_f64().unwrap(), 0.0), "{side}");
    assert!(near(side["gap"]["end"].as_f64().unwrap(), 485.0), "{side}");
    assert!(side["gap"]["a"].is_null(), "{side}");
    assert_eq!(side["gap"]["b"], m["component"], "{side}");
    let a = on_side(&mut doc, &t, &slot, json!({"point": [0.0, 15.0, 246.0]}));
    let b = on_side(&mut doc, &t, &slot, json!({"point": [0.0, 15.0, 760.0]}));
    assert_at(&doc, &a, 242.5);
    assert_at(&doc, &b, 757.5);
    assert_eq!(from_of(&doc, &a), Some("gap"));
    assert_eq!(from_of(&doc, &b), Some("gap"));
    let gap_of = |doc: &Document<MockKernel>, answer: &Value| {
        doc.part_info(component(answer)).unwrap().record.gap
    };
    assert_eq!(gap_of(&doc, &a), Some([None, Some(component(&m))]));
    assert_eq!(gap_of(&doc, &b), Some([Some(component(&m)), None]));
    all_ok(&doc);
    // Their slides are expressions of the cross member's slide (and of the
    // side's length towards end B).
    let (m_slide, _) = slide(&doc, &m);
    let length = doc.part_info(component(&t)).unwrap().length.unwrap().0;
    let length = doc.parameters().name(length);
    assert_eq!(slide(&doc, &a).1, format!("{m_slide} / 2 - 7.5"));
    assert_eq!(
        slide(&doc, &b).1,
        format!("({m_slide} + {length}) / 2 + 7.5")
    );
    // The cross member moves 100 towards end B: both stay centred.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": m["component"],
               "at": {"offset": 100, "from": "middle"}}),
    )
    .unwrap();
    all_ok(&doc);
    assert_at(&doc, &m, 600.0);
    assert_at(&doc, &a, 292.5);
    assert_at(&doc, &b, 807.5);
    // The side grows to 1200: the cross member at 700, the one towards end
    // B follows that end too.
    lengthen(&mut doc, &t, 1200.0);
    assert_at(&doc, &m, 700.0);
    assert_at(&doc, &a, 342.5);
    assert_at(&doc, &b, 957.5);
    // The cross member goes: no error, each keeps its place, bound to the
    // nearer end of the side, and its slide's parameter goes with it.
    command(
        &mut doc,
        json!({"cmd": "build_remove", "component": m["component"]}),
    )
    .unwrap();
    all_ok(&doc);
    assert_at(&doc, &a, 342.5);
    assert_at(&doc, &b, 957.5);
    assert_eq!(from_of(&doc, &a), Some("a"));
    assert_eq!(from_of(&doc, &b), Some("b"));
    assert_eq!(gap_of(&doc, &a), None);
    assert_eq!(gap_of(&doc, &b), None);
    assert!(
        doc.parameters().find(&m_slide).is_none(),
        "{m_slide} is still a parameter"
    );
    lengthen(&mut doc, &t, 1400.0);
    assert_at(&doc, &a, 342.5);
    assert_at(&doc, &b, 1157.5);
}

#[test]
fn an_edit_to_middle_of_gap_keeps_the_part_in_place_and_binds_it() {
    let mut doc = design();
    let (t, slot) = target(&mut doc);
    let m = on_side(&mut doc, &t, &slot, json!({"point": [0.0, 15.0, 503.0]}));
    // 100 from end A: its centre at 115, 127.5 before the middle of the
    // free stretch from end A to the cross member (242.5).
    let p = on_side(&mut doc, &t, &slot, json!({"offset": 100}));
    assert_at(&doc, &p, 115.0);
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": p["component"], "at": {"from": "gap"}}),
    )
    .unwrap();
    all_ok(&doc);
    assert_at(&doc, &p, 115.0);
    assert_eq!(from_of(&doc, &p), Some("gap"));
    // The cross member moves 100 towards end B: the stretch's middle at
    // 292.5, the part 127.5 before it.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": m["component"],
               "at": {"offset": 100, "from": "middle"}}),
    )
    .unwrap();
    all_ok(&doc);
    assert_at(&doc, &p, 165.0);
    // An offset alone is from the middle of the gap it has.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": p["component"], "at": {"offset": 0}}),
    )
    .unwrap();
    assert_at(&doc, &p, 292.5);
    // The cross member is never centred between parts bound to it (a
    // cycle): with p left out its free stretch is the whole side, and it
    // stays where it is, 100 past the middle.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": m["component"], "at": {"from": "gap"}}),
    )
    .unwrap();
    all_ok(&doc);
    assert_at(&doc, &m, 600.0);
    assert_at(&doc, &p, 292.5);
}
