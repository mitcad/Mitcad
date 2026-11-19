// SPDX-License-Identifier: MIT
//! Members along an edge (mitcad#1814): a member laid along a straight edge
//! of a plate on the face beside it, flush with the edge, moved by Inset,
//! Start, End and Lift, and following the plate when it changes.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::CurveGeometry;
use mitcad_model::testing::MockKernel;
use mitcad_model_tests_support::test_library::{
    all_ok, close, command, component, design, occurrence, part, place, placed, query,
};

/// A plate 750 wide and 9 thick on XY from the origin, its length the
/// parameter `plate_length` (1800 mm): its box feature.
fn plate(doc: &mut Document<MockKernel>) -> String {
    doc.kernel().box_faces.set(true);
    doc.kernel().box_edges.set(true);
    command(
        doc,
        json!({"cmd": "add_parameter", "name": "plate_length", "expression": "1800 mm"}),
    )
    .unwrap();
    let added = command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "box",
               "plane": {"type": "origin", "plane": "xy"}, "corner": [0.0, 0.0],
               "length": "plate_length", "width": 750, "height": 9,
               "operation": "new_body"}}),
    )
    .unwrap_or_else(|e| panic!("box: {e}"));
    added["uid"]
        .as_str()
        .or_else(|| added["feature"].as_str())
        .unwrap_or_else(|| panic!("no feature in {added}"))
        .to_owned()
}

/// The plate's long edge at y = 0 on its top face, and that face.
fn long_edge(box_feature: &str) -> (Value, Value) {
    let body = format!("{box_feature}.b0");
    (
        json!({"body": body, "edge": format!("E{{{box_feature}:side0|{box_feature}:top}}")}),
        json!({"body": body, "face": format!("{box_feature}:top")}),
    )
}

fn length_of(doc: &Document<MockKernel>, answer: &Value) -> f64 {
    query(doc, json!({"query": "build_parts"}))["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["component"] == answer["component"])
        .unwrap()["length"]
        .as_f64()
        .unwrap()
}

#[test]
fn a_member_along_an_edge_runs_it_flush_on_the_face() {
    let mut doc = design();
    let plate = plate(&mut doc);
    all_ok(&doc);
    let (edge, face) = long_edge(&plate);
    // A 28 x 120 board, flat: 120 across the face, 28 up.
    let member = place(
        &mut doc,
        json!({"part": part("deck120"), "along": {"edge": edge, "face": face}}),
    );
    assert_eq!(member["place"], "edge", "{member}");
    all_ok(&doc);
    assert!((length_of(&doc, &member) - 1800.0).abs() < 1e-6);
    let t = placed(&doc, occurrence(&member));
    // End A at the edge's start, its axis 60 in from the edge and 14 up
    // from the top (z = 9): its side flush with the edge, on the face.
    assert!(
        close(t.translation, [0.0, 60.0, 23.0]),
        "{:?}",
        t.translation
    );
    assert!(close(t.apply_vector([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]));
    assert!(close(t.apply_vector([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]));
    // It is held by its joint on the edge frame, which goes with it.
    let record = doc
        .assembly()
        .component(component(&member))
        .and_then(|c| c.build.clone())
        .unwrap();
    assert!(record.joint.is_some() && record.bound.len() == 1);
    assert_eq!(record.adds.len(), 1, "{:?}", record.adds);
}

#[test]
fn the_offsets_move_it_and_it_follows_the_plate() {
    let mut doc = design();
    let plate = plate(&mut doc);
    let (edge, face) = long_edge(&plate);
    let member = place(
        &mut doc,
        json!({"part": part("deck120"),
               "along": {"edge": edge, "face": face,
                         "inset": 10, "start": 50, "end": 20, "lift": 5}}),
    );
    all_ok(&doc);
    assert!((length_of(&doc, &member) - 1730.0).abs() < 1e-6);
    let t = placed(&doc, occurrence(&member));
    assert!(
        close(t.translation, [50.0, 70.0, 28.0]),
        "{:?}",
        t.translation
    );
    // The plate 1500 long: the member follows, its offsets kept.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "plate_length", "value": 1500}),
    )
    .unwrap();
    all_ok(&doc);
    assert!((length_of(&doc, &member) - 1430.0).abs() < 1e-6);
    let t = placed(&doc, occurrence(&member));
    assert!(
        close(t.translation, [50.0, 70.0, 28.0]),
        "{:?}",
        t.translation
    );
    assert!(close(t.apply_vector([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]));
}

#[test]
fn on_edge_it_stands_on_the_face_turned_a_quarter() {
    let mut doc = design();
    let plate = plate(&mut doc);
    let (edge, face) = long_edge(&plate);
    let member = place(
        &mut doc,
        json!({"part": part("deck120"), "lie": "edge",
               "along": {"edge": edge, "face": face}}),
    );
    all_ok(&doc);
    let t = placed(&doc, occurrence(&member));
    // 28 across the face, 120 up.
    assert!(
        close(t.translation, [0.0, 14.0, 69.0]),
        "{:?}",
        t.translation
    );
    assert!(close(t.apply_vector([1.0, 0.0, 0.0]), [0.0, 0.0, 1.0]));
}

#[test]
fn a_curved_edge_or_offsets_that_leave_nothing_are_refused_in_words() {
    let mut doc = design();
    let plate = plate(&mut doc);
    let (edge, face) = long_edge(&plate);
    let name = edge["edge"].as_str().unwrap().to_owned();
    doc.kernel().edge_curves.borrow_mut().insert(
        name,
        CurveGeometry::Circle {
            center: [900.0, 0.0, 9.0],
            normal: [0.0, 0.0, 1.0],
            radius: 900.0,
            start: [0.0, 0.0, 9.0],
            end: [1800.0, 0.0, 9.0],
        },
    );
    let refused = command(
        &mut doc,
        json!({"cmd": "build_place", "part": part("deck120"),
               "along": {"edge": edge, "face": face}}),
    )
    .unwrap_err();
    assert!(refused.contains("needs a straight edge"), "{refused}");
    assert!(!refused.lines().next().unwrap().contains("E{"), "{refused}");
    doc.kernel().edge_curves.borrow_mut().clear();
    let refused = command(
        &mut doc,
        json!({"cmd": "build_place", "part": part("deck120"),
               "along": {"edge": edge, "face": face, "start": 1000, "end": 800}}),
    )
    .unwrap_err();
    assert!(refused.contains("leave nothing of the edge"), "{refused}");
}

#[test]
fn its_edit_changes_the_offsets_and_the_lie_in_one_undo_step() {
    let mut doc = design();
    let plate = plate(&mut doc);
    let (edge, face) = long_edge(&plate);
    let member = place(
        &mut doc,
        json!({"part": part("deck120"),
               "along": {"edge": edge, "face": face, "start": 50, "end": 20}}),
    );
    all_ok(&doc);
    let parts = query(&doc, json!({"query": "build_parts"}));
    let along = &parts["parts"][0]["along"];
    assert_eq!(along["lie"], "flat", "{parts}");
    assert!(along["feature"].is_string(), "{parts}");
    let edited = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": member["component"],
               "along": {"inset": 30}, "lie": "edge"}),
    )
    .unwrap();
    assert_eq!(edited["changed"], json!(["along", "lie"]), "{edited}");
    all_ok(&doc);
    // On edge, 30 in: 28 across the face, 120 up, from x 50.
    let t = placed(&doc, occurrence(&member));
    assert!(
        close(t.translation, [50.0, 44.0, 69.0]),
        "{:?}",
        t.translation
    );
    assert!(close(t.apply_vector([1.0, 0.0, 0.0]), [0.0, 0.0, 1.0]));
    assert!((length_of(&doc, &member) - 1730.0).abs() < 1e-6);
    let parts = query(&doc, json!({"query": "build_parts"}));
    assert_eq!(parts["parts"][0]["along"]["lie"], "edge", "{parts}");
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    all_ok(&doc);
    let t = placed(&doc, occurrence(&member));
    assert!(
        close(t.translation, [50.0, 60.0, 23.0]),
        "{:?}",
        t.translation
    );
}

#[test]
fn an_edge_offset_change_renames_the_member_to_its_new_length() {
    // Its automatic name follows its designation (mitcad#2030).
    let mut doc = design();
    let plate = plate(&mut doc);
    let (edge, face) = long_edge(&plate);
    let member = place(
        &mut doc,
        json!({"part": part("deck120"), "along": {"edge": edge, "face": face}}),
    );
    all_ok(&doc);
    let name = |doc: &Document<MockKernel>| doc.assembly().name(component(&member));
    let placed_name = name(&doc);
    assert!(placed_name.starts_with("Planed 28x120 L="), "{placed_name}");
    let depth = doc.undo_depth();
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": member["component"],
               "along": {"start": 50, "end": 20}}),
    )
    .unwrap();
    all_ok(&doc);
    assert!((length_of(&doc, &member) - 1730.0).abs() < 1e-6);
    assert_eq!(name(&doc), "Planed 28x120 L=1730");
    assert_eq!(doc.undo_depth(), depth + 1);
    // The plate shorter: the member follows, renamed in that edit.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "plate_length", "value": 1500}),
    )
    .unwrap();
    all_ok(&doc);
    assert_eq!(name(&doc), "Planed 28x120 L=1430");
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    all_ok(&doc);
    assert_eq!(name(&doc), placed_name);
}
