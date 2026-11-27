// SPDX-License-Identifier: MIT
//! A finish into a neck deeper than its insert cuts after the roughings
//! before it (mitcad#2549, GitHub mitcad/Mitcad#733): a 30 mm bar necked
//! to 26 mm behind a 45 degree wall (2 mm deep, radius), finished with
//! Free and a 1.5 mm insert. A roughing of the outside on the same
//! spindle that plunges as far lets it through; without one, or with one
//! that is suppressed, fails, cuts the bore or runs on the other spindle,
//! the refusal stays the undercut's depth; one that leaves more than the
//! insert cuts (bridging the neck, or a thick stock to leave) is refused
//! with what it leaves. The mock kernel turns only boxes, so the operation
//! is computed here on the neck's own profile.

use serde_json::json;

use super::*;
use crate::Document;
use crate::testing::MockKernel;

/// The neck on a 32 mm stock.
fn neck() -> TurnFrame {
    TurnFrame {
        frame: WorkFrame {
            origin: [0.0, 0.0, 0.0],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 1.0, 0.0],
            z: [0.0, 0.0, 1.0],
        },
        part: TurnPart {
            outside: vec![
                turn::PV::new(0.0, 0.0),
                turn::PV::new(15.0, 0.0),
                turn::PV::new(15.0, -10.0),
                turn::PV::new(13.0, -12.0),
                turn::PV::new(13.0, -25.0),
                turn::PV::new(15.0, -25.0),
                turn::PV::new(15.0, -40.0),
            ],
            inside: vec![],
            stock_radius: 16.0,
            stock_inner_radius: 0.0,
            stock_face: 0.0,
        },
        stock_back: -45.0,
        change: [50.0, 100.0],
    }
}

/// A lathe with a roughing tool (tool 1, a 4 mm insert) and a finishing
/// tool (tool 2, a 1.5 mm insert), both without their insert's geometry.
fn lathe() -> CamMachine {
    serde_json::from_value(json!({
        "id": "lathe",
        "machine": {
            "name": "Lathe",
            "controller": "fanuc_turning",
            "travel": {"min": [-5, 0, -300], "max": [150, 0, 200]},
            "spindle": {"min_rpm": 50, "max_rpm": 3000, "nose_diameter": 0, "nose_length": 0},
            "max_feed": 5000,
            "rapid_feed": 20000,
            "lathe": {"turret_stations": 8, "chuck_diameter": 160, "chuck_length": 50}
        },
        "tools": {"tools": [
            {"number": 1, "name": "Roughing", "shape": {"kind": "turning", "nose_radius": 0.4},
             "diameter": 20, "flute_length": 4, "length": 30},
            {"number": 2, "name": "Finishing", "shape": {"kind": "turning", "nose_radius": 0.4},
             "diameter": 20, "flute_length": 1.5, "length": 30}
        ]}
    }))
    .unwrap()
}

fn operation(id: u32, value: serde_json::Value) -> CamOperation {
    let mut value = value;
    value["id"] = json!(id);
    value["name"] = json!(format!("Operation {id}"));
    serde_json::from_value(value).unwrap()
}

/// The Free roughing of the outside with tool 1.
fn plunging_roughing() -> CamOperation {
    operation(
        1,
        json!({"kind": {"type": "turn_rough", "side": "outside", "plunge": "free"}, "tool": 1}),
    )
}

/// What a person reads of a refusal (its message, without the place).
fn refusal(result: Result<TurningProgramOp, String>) -> String {
    match result {
        Ok(op) => panic!("not refused: {:?}", op.path.moves),
        Err(e) => crate::notice::FeatureError::from_text(&e).message,
    }
}

#[test]
fn a_deep_finish_follows_the_roughings_before_it() {
    let doc = Document::new(MockKernel::default());
    let tf = neck();
    let machine = lathe();
    let finish = operation(
        9,
        json!({"kind": {"type": "turn_finish", "side": "outside", "plunge": "free"}, "tool": 2}),
    );
    let compute =
        |earlier: &[&CamOperation]| doc.turn_compute_operation(&tf, &machine, &finish, earlier);
    let too_deep = "The undercut is 2 mm deep (radius), 0.5 mm deeper than the insert's depth of cut (1.5 mm): rough it first with a Turn Roughing that plunges, finish it with Don't plunge, or with an insert that cuts that deep.";

    // No roughing: the undercut's depth.
    assert_eq!(refusal(compute(&[])), too_deep);

    // After a roughing that plunges as far: the finish goes down to the
    // neck's floor (its tip at 13).
    let rough = plunging_roughing();
    let finished = compute(&[&rough]).unwrap();
    assert!(
        finished
            .path
            .moves
            .iter()
            .any(|m| (m.to()[0] - 13.0).abs() < 1e-6),
        "{:?}",
        finished.path.moves
    );

    // Roughings that do not count: suppressed, on the sub-spindle, of the
    // bore, failing (a depth of cut deeper than its insert; Tool geometry
    // without the insert's geometry), not a roughing.
    let mut suppressed = plunging_roughing();
    suppressed.suppressed = true;
    let mut sub = plunging_roughing();
    sub.sub_spindle = true;
    let bore = operation(
        2,
        json!({"kind": {"type": "turn_rough", "side": "inside", "plunge": "free"}, "tool": 1}),
    );
    let too_deep_cuts = operation(
        3,
        json!({"kind": {"type": "turn_rough", "side": "outside", "plunge": "free"}, "tool": 1,
               "turning": {"depth_of_cut": 10}}),
    );
    let no_insert = operation(
        4,
        json!({"kind": {"type": "turn_rough", "side": "outside", "plunge": "tool"}, "tool": 1}),
    );
    let facing = operation(5, json!({"kind": {"type": "turn_face"}, "tool": 1}));
    for other in [
        &suppressed,
        &sub,
        &bore,
        &too_deep_cuts,
        &no_insert,
        &facing,
    ] {
        assert_eq!(refusal(compute(&[other])), too_deep, "{:?}", other.kind);
    }

    // A roughing bridging the neck leaves it all and its stock to leave
    // (0.2 by default) on top.
    let bridging = operation(
        6,
        json!({"kind": {"type": "turn_rough", "side": "outside"}, "tool": 1}),
    );
    assert_eq!(
        refusal(compute(&[&bridging])),
        "After the roughing before it, 2.2 mm (radius) is left in the undercut, 0.7 mm more than the insert's depth of cut (1.5 mm): let the roughing plunge as far as the finish or leave less stock, or finish it with Don't plunge."
    );
    // One plunging as far but leaving 1.6 mm.
    let thick = operation(
        7,
        json!({"kind": {"type": "turn_rough", "side": "outside", "plunge": "free"}, "tool": 1,
               "stock_to_leave": 1.6}),
    );
    assert!(
        refusal(compute(&[&thick]))
            .starts_with("After the roughing before it, 1.6 mm (radius) is left in the undercut"),
    );
    // The least any roughing leaves counts.
    assert!(compute(&[&bridging, &rough]).is_ok());
    assert!(compute(&[&rough, &thick]).is_ok());
}
