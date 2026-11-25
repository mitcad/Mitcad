// SPDX-License-Identifier: MIT
//! The sub-spindle drawn in Simulate, the turret's body against it and the
//! sub-spindle's own profile through the JSON api (mitcad#2405) on the
//! mock kernel, with the block parted off into the sub-spindle of
//! sub_live_tests.rs: the turret's body and the profile kept with the
//! machine and refused in words; `cam_simulate`'s outline of the
//! sub-spindle and where it stands as each operation cuts, moving through
//! the hand-over; the turret's body running into the sub-spindle's chuck in
//! Generate's findings and the simulation's hits.

use serde_json::{Value, json};

use mitcad_model_api::DocumentApi;

use crate::sub_live_tests::{block, command, lathe, query};

/// The lathe with a turret's body round the tool (`turret`: X and Z from
/// the tool, each lowest and highest) and a nose on the sub-spindle's
/// profile, 14 mm across and 12 mm in front of its jaws' face.
fn turret_lathe(turret: Value) -> Value {
    let mut machine = lathe([-400.0, 300.0]);
    let lathe = &mut machine["machine"]["lathe"];
    lathe["turret_body"] = turret;
    lathe["sub_spindle"]["profile"] = json!([[-12, 14], [0, 14]]);
    machine
}

fn messages(found: &Value) -> Vec<String> {
    found
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["message"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn the_turret_s_body_and_the_profile_are_kept_and_refused_in_words() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put",
               "machine": turret_lathe(json!({"x": [0, 80], "z": [-10, 20]}))}),
    );
    let machines = query(&doc, json!({"query": "cam_machines"}));
    let kept = machines["machines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == json!("mill-turn"))
        .expect("the lathe")["machine"]["lathe"]
        .clone();
    assert_eq!(
        kept["turret_body"],
        json!({"x": [0.0, 80.0], "z": [-10.0, 20.0]}),
        "{kept}"
    );
    assert_eq!(
        kept["sub_spindle"]["profile"],
        json!([[-12.0, 14.0], [0.0, 14.0]]),
        "{kept}"
    );
    // Refused in words.
    let refused = doc
        .command(
            &json!({"cmd": "cam_machine_put",
                    "machine": turret_lathe(json!({"x": [80, 0], "z": [-10, 20]}))})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains(
            "The turret's body needs X and Z from the tool, each from a lowest up to a highest."
        ),
        "{refused}"
    );
    let mut backwards = turret_lathe(Value::Null);
    backwards["machine"]["lathe"]["sub_spindle"]["profile"] = json!([[10, 14], [0, 14]]);
    let refused = doc
        .command(&json!({"cmd": "cam_machine_put", "machine": backwards}).to_string())
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("The sub-spindle's profile needs two or more points in order along Z"),
        "{refused}"
    );
}

#[test]
fn simulate_draws_the_sub_spindle_where_it_is() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": turret_lathe(Value::Null)}),
    );
    // The outline: the chuck 120 mm across drawn as long, then the nose.
    let simulated = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let sub = &simulated["sub_spindle"];
    assert_eq!(
        sub["outline"],
        json!([[[0.0, 60.0], [120.0, 60.0]], [[-12.0, 7.0], [0.0, 7.0]]]),
        "{sub}"
    );
    // Without the work zero's machine place the facing on the main spindle
    // does not know where it waits; from the parting off on it stands at
    // the grip, where the view keeps the part.
    let faces: Vec<Value> = sub["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["face"].clone())
        .collect();
    assert_eq!(faces.len(), 5, "{sub}");
    assert!(faces[0].is_null(), "{sub}");
    let grip = faces[1].as_f64().expect("the grip");
    for face in &faces[2..] {
        assert_eq!(face.as_f64(), Some(grip), "{sub}");
    }
    // The ids are the operations'.
    let setups = query(&doc, json!({"query": "cam_setups"}));
    let ids: Vec<Value> = setups["setups"][0]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].clone())
        .collect();
    let drawn: Vec<Value> = sub["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].clone())
        .collect();
    assert_eq!(drawn, ids);
    // The work zero 200 mm from the machine's zero: the facing's at the far
    // end of its travel (machine Z 300), 500 from it.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {"placement": [0, 0, -200]}}),
    );
    let simulated = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let first = &simulated["sub_spindle"]["operations"][0]["face"];
    assert!((first.as_f64().unwrap() - 500.0).abs() < 1e-9, "{first}");
    // A lathe whose sub-spindle has no body draws none.
    let mut bare = lathe([-400.0, 300.0]);
    bare["machine"]["lathe"]["sub_spindle"]["chuck_diameter"] = json!(0);
    command(&mut doc, json!({"cmd": "cam_machine_put", "machine": bare}));
    let simulated = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    assert!(simulated["sub_spindle"].is_null(), "{simulated}");
}

#[test]
fn the_turret_s_body_running_into_the_sub_spindle_is_found_in_words() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {"placement": [0, 0, -200]}}),
    );
    // Without a turret's body only the tool's point is checked.
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let found = messages(&generated["setups"][0]["findings"]);
    assert!(!found.iter().any(|m| m.contains("turret")), "{found:?}");
    // The turret reaching 20 mm towards the sub-spindle from the tool: the
    // back's facing, 5 mm in front of its jaws' face, brings it into the
    // chuck; the facing on the main spindle, while it waits far off, not.
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put",
               "machine": turret_lathe(json!({"x": [0, 80], "z": [-10, 20]}))}),
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let found = messages(&generated["setups"][0]["findings"]);
    let chuck = "Turn Facing 2: the turret runs into the sub-spindle's chuck.";
    assert!(found.iter().any(|m| m == chuck), "{found:?}");
    assert!(
        !found
            .iter()
            .any(|m| m.starts_with("Turn Facing 1:") && m.contains("turret")),
        "{found:?}"
    );
    // The simulation gives the same hit.
    let simulated = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let hits = messages(&simulated["sub_spindle_hits"]);
    assert!(hits.iter().any(|m| m == chuck), "{hits:?}");
}
