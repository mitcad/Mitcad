// SPDX-License-Identifier: MIT
//! A side placement's Offset checked with Measure in the design's
//! coordinates (mitcad#646), on the mock kernel (its measure of the
//! selections' boxes, `MockKernel::measures_selections`): a 20 profile
//! placed on the side of an upright one, 500 long, from end A or end B has
//! its face nearer that end Offset from the end's plane (0: flush), and
//! from the middle its middle Offset from the target's middle, also turned
//! about its axis; a typed number without a unit is millimetres. Both
//! profiles are BUILD parts, each placed by its own occurrence.
//!
//! A tilted part's Offset is to its foot's long point on the slot, not to
//! a face (mitcad#444, `tilt_tests`).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::BodyUid;
use mitcad_model::testing::MockKernel;
use mitcad_model_tests_support::test_library::*;

/// The upright profile's length, as in the report.
const LENGTH: f64 = 500.0;

/// A 20 profile 500 long standing at the origin along +Z (x and y from -10
/// to 10), and its slot `slot_y+_1` as `at.geometry`; the mock measures
/// selections from the start (its extrusions keep their side planes).
fn upright(doc: &mut Document<MockKernel>) -> (Value, Value) {
    doc.kernel().measures_selections.set(true);
    let t = first(doc, "p20", [0.0; 3], LENGTH);
    let tp = derived(doc, component(&t));
    (t, json!({"mount": format!("{tp}:mount(slot_y+_1)")}))
}

/// A 20 profile 200 long on the upright one's y+ side, `at` merged into
/// its `at` and `fields` into the command.
fn on_side(
    doc: &mut Document<MockKernel>,
    t: &Value,
    slot: &Value,
    at: Value,
    fields: Value,
) -> Value {
    let mut place_at = json!({"occurrence": t["occurrence"], "geometry": slot});
    for (key, value) in at.as_object().unwrap() {
        place_at[key] = value.clone();
    }
    let mut command = json!({"part": {"source": "S1"}, "at": place_at, "length": 200});
    for (key, value) in fields.as_object().unwrap() {
        command[key] = value.clone();
    }
    place(doc, command)
}

/// A placed part's occurrence path from the root (`"O1"`).
fn path(doc: &Document<MockKernel>, answer: &Value) -> String {
    let paths = doc.assembly().paths_to(component(answer));
    assert_eq!(paths.len(), 1, "{answer}");
    paths[0]
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

/// A placed part's body and the names of its faces.
fn faces(doc: &Document<MockKernel>, answer: &Value) -> (String, Vec<String>) {
    let body = format!("{}.b0", derived(doc, component(answer)));
    let shape = doc.body_shape(body.parse::<BodyUid>().unwrap()).unwrap();
    (body, shape.faces.iter().map(ToString::to_string).collect())
}

/// The upright profile's end face (`end_a` or `end_b`) as a selection with
/// its occurrence.
fn end_face(doc: &Document<MockKernel>, t: &Value, end: &str) -> Value {
    let (body, names) = faces(doc, t);
    let name = names
        .into_iter()
        .find(|n| n.ends_with(&format!(":{end}")))
        .unwrap_or_else(|| panic!("no face {end}"));
    json!({"body": body, "face": name, "occurrence": path(doc, t)})
}

/// Measure from `reference` to each face of a placed part: `between` for
/// the faces parallel to it (those with an offset), nearest first.
fn parallel_faces(doc: &Document<MockKernel>, reference: &Value, answer: &Value) -> Vec<Value> {
    let (body, names) = faces(doc, answer);
    let occurrence = path(doc, answer);
    let mut found: Vec<Value> = names
        .into_iter()
        .filter_map(|name| {
            let b = json!({"body": body, "face": name, "occurrence": occurrence});
            let between =
                query(doc, json!({"query": "measure", "a": reference, "b": b}))["between"].clone();
            between.get("offset").is_some().then_some(between)
        })
        .collect();
    found.sort_by(|a, b| offset(a).total_cmp(&offset(b)));
    found
}

fn offset(between: &Value) -> f64 {
    between["offset"].as_f64().unwrap()
}

fn near(actual: &Value, expected: f64) {
    let value = actual.as_f64().unwrap();
    assert!(
        (value - expected).abs() < 1e-6,
        "{value} instead of {expected}"
    );
}

fn near_point(actual: &Value, expected: [f64; 3]) {
    for (i, expected) in expected.into_iter().enumerate() {
        assert!(
            (actual[i].as_f64().unwrap() - expected).abs() < 1e-6,
            "{actual} instead of {expected:?}"
        );
    }
}

/// The part's face nearer the end is `expected` from the end's plane: the
/// offset and the closest distance (the faces overlap across the target's
/// width), from the end face's edge at y = 10 to the part's face.
fn assert_near_face(near_face: &Value, end_z: f64, face_z: f64) {
    near(&near_face["offset"], (end_z - face_z).abs());
    near(&near_face["distance"], (end_z - face_z).abs());
    near_point(&near_face["on_a"], [0.0, 10.0, end_z]);
    near_point(&near_face["on_b"], [0.0, 10.0, face_z]);
}

#[test]
fn a_side_placement_s_offset_is_its_near_face_from_the_end() {
    let mut doc = design();
    let (t, slot) = upright(&mut doc);
    let end_a = end_face(&doc, &t, "end_a");
    let end_b = end_face(&doc, &t, "end_b");

    // The report's case: from end B, Offset 100. The part reaches from
    // z = 380 to 400: its face nearer end B is 100 from it, the other 120.
    let b100 = on_side(
        &mut doc,
        &t,
        &slot,
        json!({"offset": 100, "from": "b"}),
        json!({}),
    );
    let found = parallel_faces(&doc, &end_b, &b100);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_near_face(&found[0], LENGTH, 400.0);
    near(&found[1]["offset"], 120.0);
    // Offset 0 is flush with end B.
    let flush = on_side(
        &mut doc,
        &t,
        &slot,
        json!({"offset": 0, "from": "b"}),
        json!({}),
    );
    assert_near_face(&parallel_faces(&doc, &end_b, &flush)[0], LENGTH, LENGTH);

    // From end A: the face nearer end A is 100 from it.
    let a100 = on_side(
        &mut doc,
        &t,
        &slot,
        json!({"offset": 100, "from": "a"}),
        json!({}),
    );
    let found = parallel_faces(&doc, &end_a, &a100);
    assert_near_face(&found[0], 0.0, 100.0);
    near(&found[1]["offset"], 120.0);

    // From the middle: the part's middle is 100 past the target's (towards
    // end B), measured from a construction plane through the middle.
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {
            "type": "offset", "plane": "xy", "distance": LENGTH / 2.0}}}),
    )
    .unwrap();
    let middle = json!({"datum": added["uid"]});
    let m100 = on_side(
        &mut doc,
        &t,
        &slot,
        json!({"offset": 100, "from": "middle"}),
        json!({}),
    );
    let found = parallel_faces(&doc, &middle, &m100);
    assert_eq!(found.len(), 2, "{found:?}");
    near(&found[0]["offset"], 90.0);
    near(&found[1]["offset"], 110.0);
    all_ok(&doc);
}

#[test]
fn a_turned_part_keeps_its_offset() {
    let mut doc = design();
    let (t, slot) = upright(&mut doc);
    let end_a = end_face(&doc, &t, "end_a");
    let end_b = end_face(&doc, &t, "end_b");
    for rotate in ["90 deg", "180 deg", "270 deg"] {
        let b = on_side(
            &mut doc,
            &t,
            &slot,
            json!({"offset": 100, "from": "b"}),
            json!({"rotate": rotate}),
        );
        assert_near_face(&parallel_faces(&doc, &end_b, &b)[0], LENGTH, 400.0);
        let a = on_side(
            &mut doc,
            &t,
            &slot,
            json!({"offset": 100, "from": "a"}),
            json!({"rotate": rotate}),
        );
        assert_near_face(&parallel_faces(&doc, &end_a, &a)[0], 0.0, 100.0);
    }
    all_ok(&doc);
}

#[test]
fn a_typed_offset_without_a_unit_is_millimetres() {
    let mut doc = design();
    let (t, slot) = upright(&mut doc);
    let end_b = end_face(&doc, &t, "end_b");
    // As typed ("100"), as the panel gives it with the unit ("100 mm"),
    // and in another unit.
    for typed in ["100", "100 mm", "10 cm"] {
        let b = on_side(
            &mut doc,
            &t,
            &slot,
            json!({"offset": typed, "from": "b"}),
            json!({}),
        );
        assert_near_face(&parallel_faces(&doc, &end_b, &b)[0], LENGTH, 400.0);
    }
    let end_a = end_face(&doc, &t, "end_a");
    let a = on_side(
        &mut doc,
        &t,
        &slot,
        json!({"offset": "100", "from": "a"}),
        json!({}),
    );
    assert_near_face(&parallel_faces(&doc, &end_a, &a)[0], 0.0, 100.0);
    all_ok(&doc);
}

/// The part's entry of `build_parts`.
fn part_entry(doc: &Document<MockKernel>, answer: &Value) -> Value {
    query(doc, json!({"query": "build_parts"}))["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["component"] == answer["component"])
        .cloned()
        .unwrap_or_else(|| panic!("no part {answer}"))
}

/// The BUILD Placement panel's edit (mitcad#1885): a part placed from the
/// middle of a side, measured from end A instead without moving, then
/// placed 100 from end A; each edit one undo step, and `build_parts` gives
/// the offset from the part's reference throughout.
#[test]
fn a_side_placement_s_from_and_offset_are_edited() {
    let mut doc = design();
    let (t, slot) = upright(&mut doc);
    let end_a = end_face(&doc, &t, "end_a");
    // From the middle, 100 past it: the part reaches from z = 340 to 360.
    let m100 = on_side(
        &mut doc,
        &t,
        &slot,
        json!({"offset": 100, "from": "middle"}),
        json!({}),
    );
    let entry = part_entry(&doc, &m100);
    assert_eq!(entry["from"], "middle", "{entry}");
    near(&entry["offset"], 100.0);
    assert_near_face(&parallel_faces(&doc, &end_a, &m100)[0], 0.0, 340.0);

    // From end A: where it is, now 340 from end A.
    let c = m100["component"].clone();
    let answer = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": c, "at": {"from": "a"}}),
    )
    .unwrap();
    assert!(
        answer["changed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "from"),
        "{answer}"
    );
    let entry = part_entry(&doc, &m100);
    assert_eq!(entry["from"], "a", "{entry}");
    near(&entry["offset"], 340.0);
    assert_near_face(&parallel_faces(&doc, &end_a, &m100)[0], 0.0, 340.0);

    // Offset and From together: 100 from end A.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": c, "at": {"offset": "100 mm", "from": "a"}}),
    )
    .unwrap();
    near(&part_entry(&doc, &m100)["offset"], 100.0);
    assert_near_face(&parallel_faces(&doc, &end_a, &m100)[0], 0.0, 100.0);
    all_ok(&doc);

    // One undo step takes each edit back.
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    near(&part_entry(&doc, &m100)["offset"], 340.0);
    assert_near_face(&parallel_faces(&doc, &end_a, &m100)[0], 0.0, 340.0);
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    let entry = part_entry(&doc, &m100);
    assert_eq!(entry["from"], "middle", "{entry}");
    near(&entry["offset"], 100.0);

    // A part placed otherwise has no offset: the upright one.
    assert!(part_entry(&doc, &t)["offset"].is_null());
}
