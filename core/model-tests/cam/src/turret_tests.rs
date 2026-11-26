// SPDX-License-Identifier: MIT
//! The turret's body against the main chuck and the machine's fixed
//! zones, a turret body per station, and the zones and the turret as
//! Simulate draws them, through the JSON api (mitcad#2412) on the mock
//! kernel, with the block parted off into the sub-spindle of
//! sub_live_tests.rs (pulled back 40 mm): the bodies of single stations
//! kept with the machine and refused in words; the turret's hits in
//! Generate's findings and in `cam_simulate`'s `turret_hits`;
//! `cam_simulate`'s `zones` and `turret` per operation.

use serde_json::{Value, json};

use mitcad_model_api::DocumentApi;

use crate::sub_live_tests::{block, command, lathe, query};

/// The lathe with a turret's body round the tool (X and Z from the tool,
/// each lowest and highest; null: none), the bodies of single stations,
/// a fixed zone of the machine and a zone moving with the sub-spindle.
fn turret_lathe(turret: Value, stations: Value, zones: Value) -> Value {
    let mut machine = lathe([-400.0, 300.0]);
    machine["machine"]["zones"] = zones;
    let lathe = &mut machine["machine"]["lathe"];
    lathe["turret_body"] = turret;
    lathe["station_bodies"] = stations;
    lathe["sub_spindle"]["zones"] = json!([
        {"name": "Sub guard", "bounds": {"min": [100, -50, -60], "max": [140, 50, 0]}}
    ]);
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

/// The block with the work zero at machine Z -200 and the chuck's jaws
/// 2 mm behind the stock's back.
fn placed() -> mitcad_model::Document<mitcad_model::testing::MockKernel> {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {
            "placement": [0, 0, -200],
            "lathe": {"hand_over": {"withdraw": "40 mm"}, "chuck_gap": "2 mm"}
        }}),
    );
    doc
}

#[test]
fn a_turret_body_per_station_is_kept_and_refused_in_words() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": turret_lathe(
            json!({"x": [0, 80], "z": [-10, 20]}),
            json!([{"station": 2, "x": [-5, 60], "z": [-15, 15]}]),
            json!([]))}),
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
        kept["station_bodies"],
        json!([{"station": 2, "x": [-5.0, 60.0], "z": [-15.0, 15.0]}]),
        "{kept}"
    );
    // A station off the turret, a station twice: refused in words.
    for stations in [
        json!([{"station": 13, "x": [0, 1], "z": [0, 1]}]),
        json!([{"station": 2, "x": [0, 1], "z": [0, 1]},
               {"station": 2, "x": [0, 2], "z": [0, 2]}]),
    ] {
        let refused = doc
            .command(
                &json!({"cmd": "cam_machine_put",
                        "machine": turret_lathe(Value::Null, stations, json!([]))})
                .to_string(),
            )
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains(
                "A station's turret body needs a station on the turret, each station once."
            ),
            "{refused}"
        );
    }
}

/// The turret's body reaching 30 mm back towards the main chuck from the
/// tool, and 100 mm out from it into a fixed zone 100 to 120 mm from the
/// axis along the whole Z travel, which the tool itself never reaches:
/// the facing on the main spindle brings it into both; reaching no way
/// back nor out, neither. The same hits in Generate's findings and in the
/// simulation.
#[test]
fn the_turret_s_body_running_into_the_chuck_and_a_fixed_zone_is_found_in_words() {
    let mut doc = placed();
    let fixed = json!([
        {"name": "Tailstock", "bounds": {"min": [100, -50, -1000], "max": [120, 50, 1000]}}
    ]);
    // Without a turret's body only the tool's point is checked.
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put",
               "machine": turret_lathe(Value::Null, json!([]), fixed.clone())}),
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let found = messages(&generated["setups"][0]["findings"]);
    assert!(!found.iter().any(|m| m.contains("Tailstock")), "{found:?}");
    assert!(!found.iter().any(|m| m.contains("turret")), "{found:?}");
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": turret_lathe(
            json!({"x": [0, 100], "z": [-30, 5]}), json!([]), fixed.clone())}),
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let found = messages(&generated["setups"][0]["findings"]);
    let chuck = "Turn Facing 1: the turret runs into the chuck.";
    let tailstock = "Turn Facing 1: the turret runs into Tailstock.";
    assert!(found.iter().any(|m| m == chuck), "{found:?}");
    assert!(found.iter().any(|m| m == tailstock), "{found:?}");
    // The simulation gives the same hits.
    let simulated = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let hits = messages(&simulated["turret_hits"]);
    assert!(hits.iter().any(|m| m == chuck), "{hits:?}");
    assert!(hits.iter().any(|m| m == tailstock), "{hits:?}");
    // The turning tool's station reaching no way back nor out: the facing
    // is clear of both.
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": turret_lathe(
            json!({"x": [0, 100], "z": [-30, 5]}),
            json!([{"station": 1, "x": [0, 1], "z": [0, 1]}]),
            fixed)}),
    );
    let simulated = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let hits = messages(&simulated["turret_hits"]);
    assert!(
        !hits.iter().any(|m| m.starts_with("Turn Facing 1:")),
        "{hits:?}"
    );
}

#[test]
fn simulate_gives_the_zones_and_the_turret_where_they_are() {
    let mut doc = placed();
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": turret_lathe(
        json!({"x": [0, 80], "z": [-10, 20]}),
        json!([{"station": 2, "x": [-5, 60], "z": [-15, 15]}]),
        json!([
            {"name": "Tailstock", "bounds": {"min": [100, -5, -240], "max": [120, 5, -230]}},
            {"name": "Door", "bounds": {"min": [150, 0, -300], "max": [160, 0, 100]}}
        ]))}),
    );
    let simulated = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let setups = query(&doc, json!({"query": "cam_setups"}));
    let ids: Vec<Value> = setups["setups"][0]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].clone())
        .collect();
    // The zones of every operation, by its id: the fixed ones, then the
    // one moving with the sub-spindle.
    let zones = simulated["zones"].as_array().unwrap();
    let listed: Vec<Value> = zones.iter().map(|z| z["id"].clone()).collect();
    assert_eq!(listed, ids, "{zones:?}");
    let facing = zones[0]["zones"].as_array().unwrap();
    assert_eq!(facing.len(), 3, "{facing:?}");
    assert_eq!(facing[0]["name"], json!("Tailstock"));
    assert_eq!(facing[0]["moving"], json!(false));
    // 200 mm on from the machine's Z; Y kept on the lathe with a Y axis,
    // the door not limited in Y drawn at 0.
    assert_eq!(facing[0]["min"], json!([100.0, -5.0, -40.0]));
    assert_eq!(facing[0]["max"], json!([120.0, 5.0, -30.0]));
    assert_eq!(facing[1]["min"], json!([150.0, 0.0, -100.0]));
    // The guard where the sub-spindle waits: its jaws' face at the far end
    // of its travel (machine Z 300, work Z 500).
    assert_eq!(facing[2]["name"], json!("Sub guard"));
    assert_eq!(facing[2]["moving"], json!(true));
    assert_eq!(facing[2]["min"], json!([100.0, -50.0, 440.0]));
    // The back's facing on the sub-spindle: the fixed zones 40 mm nearer
    // the main spindle, the guard at the grip, where the parting off has
    // it too.
    let back = zones[2]["zones"].as_array().unwrap();
    assert_eq!(back[0]["min"], json!([100.0, -5.0, -80.0]));
    let grip = simulated["sub_spindle"]["operations"][2]["face"]
        .as_f64()
        .expect("the grip");
    let guard = back[2]["min"][2].as_f64().unwrap();
    assert!((guard - (grip - 60.0)).abs() < 1e-9, "{back:?}");
    assert_eq!(zones[1]["zones"][2]["min"], back[2]["min"]);
    // The turret: the parting blade's station's own body, the turret's for
    // the others; along X for turning, away from the axis for the hole on
    // the face, along the plane's C (0, X) for the flat.
    let turret = simulated["turret"].as_array().unwrap();
    let listed: Vec<Value> = turret.iter().map(|t| t["id"].clone()).collect();
    assert_eq!(listed, ids, "{turret:?}");
    assert_eq!(turret[0]["x"], json!([0.0, 80.0]));
    assert_eq!(turret[0]["x_axis"], json!([1.0, 0.0]));
    assert_eq!(turret[1]["x"], json!([-5.0, 60.0]));
    assert_eq!(turret[1]["z"], json!([-15.0, 15.0]));
    assert!(turret[3]["x_axis"].is_null(), "{turret:?}");
    assert_eq!(turret[4]["x_axis"][0].as_f64(), Some(1.0), "{turret:?}");
    // Without the work zero's machine place: no fixed zones, no hits.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {"placement": null}}),
    );
    let simulated = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let facing = simulated["zones"][0]["zones"].as_array().unwrap();
    assert!(facing.is_empty(), "{facing:?}");
    assert_eq!(simulated["turret_hits"], json!([]));
    // A lathe without a turret's body draws none.
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": lathe([-400.0, 300.0])}),
    );
    let simulated = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    assert_eq!(simulated["turret"], json!([]));
}
