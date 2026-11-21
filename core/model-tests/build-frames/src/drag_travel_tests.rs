// SPDX-License-Identifier: MIT
//! Drags leave placed parts at their Offset, travel limits for a carriage
//! on its rail, and joints named after the parts they join (mitcad#2088).
//! A frame's parts do not drag: their joints' driven values change only
//! through Placement (`build_edit`), whose edits the parts above follow. A
//! carriage drags along its rail within its limits, which by default are
//! the rail's ends and follow the rail's length; the rail stays.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::FeatureDef;
use mitcad_model::ids::FeatureUid;
use mitcad_model::joints::Motion;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use mitcad_model_tests_support::test_library::*;

const P30: &str = "Profile 30x30 B8";

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn joint_of(answer: &Value) -> FeatureUid {
    answer["joint"].as_str().unwrap().parse().unwrap()
}

fn joint_name(doc: &Document<MockKernel>, answer: &Value) -> String {
    doc.feature(joint_of(answer)).unwrap().name.clone()
}

/// The value a joint's slide is driven to.
fn slide(doc: &Document<MockKernel>, answer: &Value) -> f64 {
    let FeatureDef::Joint(j) = &doc.feature(joint_of(answer)).unwrap().def else {
        panic!("a joint");
    };
    let id = j.position[&Motion::Tx];
    doc.parameters().value(id).unwrap()
}

/// A grounded upright 1000 long and a profile 500 long on its x+ slot,
/// `offset` from end A, along +X.
fn upright_and_arm(doc: &mut Document<MockKernel>, offset: f64) -> (Value, Value) {
    let t = first(doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(doc, component(&t));
    let arm = place(
        doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(slot_x+_1)")},
                      "offset": offset},
               "length": 500}),
    );
    all_ok(doc);
    (t, arm)
}

/// A part on another's slot `slot`, `offset` from `from`.
fn on_slot(
    doc: &mut Document<MockKernel>,
    target: &Value,
    slot: &str,
    offset: f64,
    from: &str,
    length: f64,
) -> Value {
    let tp = derived(doc, component(target));
    let p = place(
        doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": target["occurrence"], "geometry": {"mount": format!("{tp}:mount({slot})")},
                      "offset": offset, "from": from},
               "length": length}),
    );
    all_ok(doc);
    p
}

fn parts(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "build_parts"}))["parts"]
        .as_array()
        .unwrap()
        .clone()
}

fn part_row(doc: &Document<MockKernel>, answer: &Value) -> Value {
    parts(doc)
        .into_iter()
        .find(|p| p["component"] == answer["component"])
        .unwrap()
}

#[test]
fn a_profile_placed_on_a_side_does_not_drag() {
    let mut doc = design();
    let (t, arm) = upright_and_arm(&mut doc, 100.0);
    let before = placed(&doc, occurrence(&arm)).translation;
    let at = before;
    let target = [at[0], at[1], at[2] + 200.0];
    for request in [
        json!({"query": "joint_drag", "occurrence": arm["occurrence"], "point": at, "target": target}),
        json!({"cmd": "drag_occurrence", "occurrence": arm["occurrence"], "point": at, "target": target}),
    ] {
        let refused = if request.get("query").is_some() {
            doc.query(&request.to_string()).unwrap_err().to_string()
        } else {
            command(&mut doc, request.clone()).unwrap_err()
        };
        assert!(refused.contains("Placement"), "{request}: {refused}");
    }
    assert!(close(placed(&doc, occurrence(&arm)).translation, before));
    assert!(near(slide(&doc, &arm), 115.0));
    // The grounded upright says so too.
    let refused = command(
        &mut doc,
        json!({"cmd": "drag_occurrence", "occurrence": t["occurrence"], "target": [0, 0, 50]}),
    )
    .unwrap_err();
    assert!(refused.contains("Placement"), "{refused}");
}

#[test]
fn a_placement_offset_edit_moves_the_lower_profile_and_the_parts_above() {
    let mut doc = design();
    let (_, arm) = upright_and_arm(&mut doc, 100.0);
    // A post on the arm's top slot, 200 from its end A.
    let post = on_slot(&mut doc, &arm, "slot_y+_1", 200.0, "a", 300.0);
    let arm_at = placed(&doc, occurrence(&arm)).translation;
    let post_at = placed(&doc, occurrence(&post)).translation;
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": arm["component"], "at": {"offset": 300}}),
    )
    .unwrap();
    all_ok(&doc);
    let up = |p: [f64; 3]| [p[0], p[1], p[2] + 200.0];
    assert!(close(
        placed(&doc, occurrence(&arm)).translation,
        up(arm_at)
    ));
    assert!(close(
        placed(&doc, occurrence(&post)).translation,
        up(post_at)
    ));
    // Nothing was grounded to keep them.
    for answer in [&arm, &post] {
        assert!(
            !doc.assembly()
                .occurrence(occurrence(answer))
                .unwrap()
                .grounded
        );
    }
}

#[test]
fn a_length_change_keeps_a_part_offset_from_end_b_and_the_parts_on_it() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    // An arm 50 from the upright's end B, a post on the arm.
    let arm = on_slot(&mut doc, &t, "slot_x+_1", 50.0, "b", 500.0);
    let post = on_slot(&mut doc, &arm, "slot_y+_1", 200.0, "a", 300.0);
    let arm_at = placed(&doc, occurrence(&arm)).translation;
    let post_at = placed(&doc, occurrence(&post)).translation;
    // Its far face 50 from end B.
    assert!(near(arm_at[2], 1000.0 - 50.0 - 15.0), "{arm_at:?}");
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": t["component"], "length": 1300}),
    )
    .unwrap();
    all_ok(&doc);
    let up = |p: [f64; 3]| [p[0], p[1], p[2] + 300.0];
    assert!(close(
        placed(&doc, occurrence(&arm)).translation,
        up(arm_at)
    ));
    assert!(close(
        placed(&doc, occurrence(&post)).translation,
        up(post_at)
    ));
    let row = part_row(&doc, &arm);
    assert_eq!(row["from"], "b");
    assert!(near(row["offset"].as_f64().unwrap(), 50.0), "{row}");
}

/// An upright, a rail 400 long on it from 100, and a carriage on the rail
/// `offset` from its end A (to the carriage's frame, its middle: the
/// carriage is 40 long).
fn rail_and_carriage(doc: &mut Document<MockKernel>, offset: f64) -> (Value, Value, Value) {
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
    let carriage = place(
        doc,
        json!({"part": part("carriage15"),
               "at": {"occurrence": rail["occurrence"], "geometry": {"mount": format!("{rp}:mount(rail)")},
                      "offset": offset}}),
    );
    all_ok(doc);
    (t, rail, carriage)
}

#[test]
fn a_carriage_drags_along_its_rail_and_stops_at_its_limit() {
    let mut doc = design();
    let (_, rail, carriage) = rail_and_carriage(&mut doc, 100.0);
    assert!(near(slide(&doc, &carriage), 100.0));
    // Its travel: flush with end A (its frame 20 in) to flush with end B.
    let row = part_row(&doc, &carriage);
    assert_eq!(row["slides"], true, "{row}");
    assert!(near(row["limits"]["min"].as_f64().unwrap(), 20.0), "{row}");
    assert!(near(row["limits"]["max"].as_f64().unwrap(), 380.0), "{row}");
    let rail_at = placed(&doc, occurrence(&rail));
    let at = placed(&doc, occurrence(&carriage)).translation;
    // Along the rail: 50 further.
    let along = placed(&doc, occurrence(&carriage)).apply_vector([1.0, 0.0, 0.0]);
    let to = |d: f64| {
        [
            at[0] + along[0] * d,
            at[1] + along[1] * d,
            at[2] + along[2] * d,
        ]
    };
    command(
        &mut doc,
        json!({"cmd": "drag_occurrence", "occurrence": carriage["occurrence"], "point": at, "target": to(50.0)}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(
        near(slide(&doc, &carriage), 150.0),
        "{}",
        slide(&doc, &carriage)
    );
    // Far past end B: it stops flush with it, and the rail stays.
    command(
        &mut doc,
        json!({"cmd": "drag_occurrence", "occurrence": carriage["occurrence"], "point": to(50.0),
               "target": to(1000.0)}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(
        near(slide(&doc, &carriage), 380.0),
        "{}",
        slide(&doc, &carriage)
    );
    assert!(close(
        placed(&doc, occurrence(&carriage)).translation,
        to(280.0)
    ));
    let rail_now = placed(&doc, occurrence(&rail));
    assert!(
        close(rail_now.translation, rail_at.translation),
        "{rail_now:?}"
    );
    assert!(near(
        part_row(&doc, &rail)["offset"].as_f64().unwrap(),
        100.0
    ));
}

#[test]
fn a_carriage_s_default_limits_follow_the_rail_s_length() {
    let mut doc = design();
    let (_, rail, carriage) = rail_and_carriage(&mut doc, 100.0);
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": rail["component"], "length": 600}),
    )
    .unwrap();
    all_ok(&doc);
    let limits = &part_row(&doc, &carriage)["limits"];
    assert!(near(limits["min"].as_f64().unwrap(), 20.0), "{limits}");
    assert!(near(limits["max"].as_f64().unwrap(), 580.0), "{limits}");
}

#[test]
fn an_offset_beyond_max_is_refused() {
    let mut doc = design();
    let (_, rail, carriage) = rail_and_carriage(&mut doc, 100.0);
    let refused = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": carriage["component"], "at": {"offset": 395}}),
    )
    .unwrap_err();
    assert!(refused.contains("beyond its travel"), "{refused}");
    assert!(near(slide(&doc, &carriage), 100.0));
    // A new carriage past it too.
    let rp = derived(&doc, component(&rail));
    let refused = command(
        &mut doc,
        json!({"cmd": "build_place", "part": part("carriage15"),
               "at": {"occurrence": rail["occurrence"], "geometry": {"mount": format!("{rp}:mount(rail)")},
                      "offset": 390}}),
    )
    .unwrap_err();
    assert!(refused.contains("beyond its travel"), "{refused}");
    // Limits of its own: the offset stays within them, and empty ones are
    // the defaults again.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": carriage["component"],
               "limits": {"min": 50, "max": 200}}),
    )
    .unwrap();
    all_ok(&doc);
    let limits = &part_row(&doc, &carriage)["limits"];
    assert!(near(limits["max"].as_f64().unwrap(), 200.0), "{limits}");
    let refused = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": carriage["component"], "at": {"offset": 250}}),
    )
    .unwrap_err();
    assert!(refused.contains("beyond its travel"), "{refused}");
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": carriage["component"],
               "limits": {"min": null, "max": null}}),
    )
    .unwrap();
    let limits = &part_row(&doc, &carriage)["limits"];
    assert!(near(limits["max"].as_f64().unwrap(), 380.0), "{limits}");
    // A profile has none.
    let refused = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": rail["component"], "limits": {"max": 100}}),
    )
    .unwrap_err();
    assert!(refused.contains("no travel limits"), "{refused}");
}

#[test]
fn a_new_max_below_the_carriage_with_an_offset_within_it() {
    // Placement's OK sends both: the carriage at 380 mm moves to 100 mm
    // while Max becomes 300 mm, which its old place is beyond.
    let mut doc = design();
    let (_, _, carriage) = rail_and_carriage(&mut doc, 380.0);
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": carriage["component"],
               "at": {"from": "a", "offset": 100}, "limits": {"min": null, "max": 300}}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(near(slide(&doc, &carriage), 100.0));
    let limits = &part_row(&doc, &carriage)["limits"];
    assert!(near(limits["max"].as_f64().unwrap(), 300.0), "{limits}");
    assert!(near(limits["min"].as_f64().unwrap(), 20.0), "{limits}");
}

#[test]
fn joint_names_follow_the_parts_and_stay_after_a_rename() {
    let mut doc = design();
    let (t, arm) = upright_and_arm(&mut doc, 100.0);
    assert_eq!(
        joint_name(&doc, &arm),
        format!("{P30} L=500 on {P30} L=1000")
    );
    // The upright's automatic name follows its length, and the joint its
    // name.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": t["component"], "length": 700}),
    )
    .unwrap();
    assert_eq!(doc.assembly().name(component(&t)), format!("{P30} L=700"));
    assert_eq!(
        joint_name(&doc, &arm),
        format!("{P30} L=500 on {P30} L=700")
    );
    // A part renamed by the user: the joint follows that name too.
    command(
        &mut doc,
        json!({"cmd": "rename_component", "component": arm["component"], "name": "Arm"}),
    )
    .unwrap();
    assert_eq!(joint_name(&doc, &arm), format!("Arm on {P30} L=700"));
    // A renamed joint keeps its name.
    command(
        &mut doc,
        json!({"cmd": "rename_feature", "uid": arm["joint"], "name": "Shelf mount"}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": t["component"], "length": 800}),
    )
    .unwrap();
    assert_eq!(doc.assembly().name(component(&t)), format!("{P30} L=800"));
    assert_eq!(joint_name(&doc, &arm), "Shelf mount");
    // A carriage's after the carriage and its rail.
    let (_, _, carriage) = rail_and_carriage(&mut doc, 100.0);
    assert!(
        joint_name(&doc, &carriage).starts_with("Carriage 15 on Rail 15 L=400"),
        "{}",
        joint_name(&doc, &carriage)
    );
}
