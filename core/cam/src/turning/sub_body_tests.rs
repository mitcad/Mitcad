// SPDX-License-Identifier: MIT
//! Tests of the sub-spindle's body and the zones moving with it, a control
//! that mirrors only Z on the sub-spindle, and the main spindle's
//! operations after the sub-spindle's (mitcad#2392): the machine data kept
//! and refused in words; where the sub-spindle is as each operation cuts;
//! the lathe check of its chuck, its housing and its moving zones, there
//! and while it waits; the exact Fanuc program of the live tools on the
//! sub-spindle of a control that mirrors only Z, read back.

use super::check::{LiveMode, TurnPlacement};
use super::live::LiveSide;
use super::sub_live_tests::{back_hole, rapid, sub_lathe, sub_live, sub_program};
use super::*;
use crate::machine::{Bounds, Machine, Zone};
use crate::post::PostSettings;
use crate::toolpath::{CycleKind, DrillCycle, Move, Toolpath};

fn zone(name: &str, min: [f64; 3], max: [f64; 3]) -> Zone {
    Zone {
        name: name.into(),
        bounds: Bounds { min, max },
    }
}

fn messages(machine: &Machine, program: &TurningProgram, work_z: f64) -> Vec<String> {
    let placement = TurnPlacement {
        work_z,
        chuck_z: None,
    };
    check_lathe(machine, program, &placement)
        .into_iter()
        .map(|f| f.message)
        .collect()
}

fn with_sub(machine: &Machine, change: impl FnOnce(&mut SubSpindle)) -> Machine {
    let mut m = machine.clone();
    if let Some(lathe) = &mut m.lathe {
        change(&mut lathe.sub_spindle);
    }
    m
}

#[test]
fn the_sub_spindle_s_body_is_kept_and_refused_in_words() {
    let sub = SubSpindle {
        chuck_diameter: 20.0,
        chuck_length: 10.0,
        housing_diameter: 200.0,
        housing_length: 150.0,
        park: Some(250.0),
        zones: vec![zone("Guard", [11.0, -100.0, -15.5], [13.0, 100.0, -14.5])],
        ..SubSpindle::default()
    };
    sub.validate().unwrap();
    // Left out at their defaults, read back when set.
    let json = serde_json::to_value(&sub).unwrap();
    assert_eq!(json["chuck_length"], serde_json::json!(10.0));
    assert_eq!(json["housing_diameter"], serde_json::json!(200.0));
    assert_eq!(json["park"], serde_json::json!(250.0));
    assert_eq!(json["zones"][0]["name"], serde_json::json!("Guard"));
    let back: SubSpindle = serde_json::from_value(json).unwrap();
    assert_eq!(back, sub);
    let json = serde_json::to_value(SubSpindle::default()).unwrap();
    assert_eq!(json, serde_json::json!({}));
    // The parts of the body: the chuck 10 mm deep within 10 mm of the
    // axis, then the housing within 100 mm; the jaws' face itself free.
    assert_eq!(sub.body_at(6.0, 0.0), None);
    assert_eq!(sub.body_at(6.0, 1.0), Some(SubBody::Chuck));
    assert_eq!(sub.body_at(15.0, 1.0), None);
    assert_eq!(sub.body_at(6.0, 11.0), Some(SubBody::Housing));
    assert_eq!(sub.body_at(90.0, 100.0), Some(SubBody::Housing));
    assert_eq!(sub.body_at(90.0, 161.0), None);
    assert_eq!(sub.body_at(110.0, 50.0), None);
    // A chuck without a length reaches back without end, as before.
    let endless = SubSpindle {
        chuck_diameter: 100.0,
        ..SubSpindle::default()
    };
    assert_eq!(endless.body_at(40.0, 500.0), Some(SubBody::Chuck));
    // Where it waits: its own place, else the far end of its travel.
    assert_eq!(sub.waiting_z(), Some(250.0));
    let travel = SubSpindle {
        travel: Some([-250.0, 300.0]),
        ..SubSpindle::default()
    };
    assert_eq!(travel.waiting_z(), Some(300.0));
    assert_eq!(SubSpindle::default().waiting_z(), None);
    // Refused in words.
    let bad = SubSpindle {
        housing_length: -1.0,
        ..SubSpindle::default()
    };
    assert_eq!(
        bad.validate().unwrap_err(),
        "The sub-spindle's chuck length and its housing's size must be zero or more."
    );
    let bad = SubSpindle {
        park: Some(f64::NAN),
        ..SubSpindle::default()
    };
    assert_eq!(
        bad.validate().unwrap_err(),
        "Where the sub-spindle waits must be a machine Z of its jaws' face."
    );
    let bad = SubSpindle {
        zones: vec![zone("Guard", [0.0, 0.0, 5.0], [1.0, 1.0, -5.0])],
        ..SubSpindle::default()
    };
    assert_eq!(
        bad.validate().unwrap_err(),
        "A zone that moves with the sub-spindle needs a name and bounds from a minimum up to a maximum."
    );
    // The Z-only control is the lathe's: off by default, kept when on.
    assert!(!MillTurn::default().sub_mirror_z_only);
    let codes = MillTurn {
        sub_mirror_z_only: true,
        ..MillTurn::default()
    };
    assert!(!codes.is_default());
    let json = serde_json::to_value(&codes).unwrap();
    assert_eq!(json["sub_mirror_z_only"], serde_json::json!(true));
    let back: MillTurn = serde_json::from_value(json).unwrap();
    assert_eq!(back, codes);
}

/// Where the sub-spindle's jaws' face is (the main spindle's work Z): at the
/// grip for the parting off it holds the part through, pulled back for its
/// own operations, where it waits for the main spindle's.
#[test]
fn the_sub_spindle_is_where_each_operation_cuts() {
    let lathe = sub_lathe().lathe.unwrap();
    let mut program = sub_program();
    // A facing on the main spindle first.
    let mut facing = program.operations[1].clone();
    facing.name = "Face".into();
    facing.sub = false;
    program.operations.insert(0, facing);
    // The facing: where it waits, the far end of its travel at machine Z
    // 300, 500 from the work zero at -200; not known without the work zero.
    assert_eq!(sub_face_at(&lathe, &program, 0, Some(-200.0)), Some(500.0));
    assert_eq!(sub_face_at(&lathe, &program, 0, None), None);
    // The parting off: at the grip; the back's facing: pulled back 30.
    assert_eq!(sub_face_at(&lathe, &program, 1, None), Some(-5.0));
    assert_eq!(sub_face_at(&lathe, &program, 2, None), Some(25.0));
    assert_eq!(sub_face_at(&lathe, &program, 3, Some(-200.0)), Some(25.0));
    // Its own place to wait.
    let mut parked = lathe.clone();
    parked.sub_spindle.park = Some(-150.0);
    assert_eq!(sub_face_at(&parked, &program, 0, Some(-200.0)), Some(50.0));
    // A lathe without a sub-spindle has none.
    let mut none = lathe;
    none.axes.sub_spindle = false;
    assert_eq!(sub_face_at(&none, &program, 2, None), None);
}

/// The lathe check of the sub-spindle's body: a hole that reaches past the
/// chuck into the housing, the zones moving with it where it holds the
/// part, its chuck where it waits too near, each finding in words where
/// the view draws the move.
#[test]
fn the_lathe_check_knows_the_sub_spindle_s_body_and_zones() {
    let body = with_sub(&sub_lathe(), |s| {
        s.chuck_diameter = 20.0;
        s.chuck_length = 10.0;
        s.housing_diameter = 200.0;
    });
    let program = sub_program();
    assert!(messages(&body, &program, -200.0).is_empty());
    // The hole 26 mm deep from the back: as the sub-spindle holds the part
    // pulled back 30 mm its bottom is 11 mm behind the jaws' face, past
    // the chuck's 10.
    let mut deep = sub_program();
    if let Some(live) = &mut deep.operations[2].live {
        live.path = back_hole(-26.0);
    }
    let found = check_lathe(
        &body,
        &deep,
        &TurnPlacement {
            work_z: -200.0,
            chuck_z: None,
        },
    );
    let hits: Vec<&crate::machine::Finding> = found
        .iter()
        .filter(|f| f.message.contains("sub-spindle's"))
        .collect();
    assert_eq!(hits.len(), 2, "{found:?}");
    assert_eq!(
        hits[0].message,
        "Back hole: the tool runs into the sub-spindle's chuck."
    );
    assert_eq!(
        hits[1].message,
        "Back hole: the tool runs into the sub-spindle's housing."
    );
    assert_eq!(hits[1].operation, 2);
    // Drawn on the part where it was: the first point past the chuck's
    // 10 mm, about 25.5 mm into the part from its back at -20.
    let at = hits[1].at.unwrap();
    assert!(
        (at[0] - 6.0).abs() < 1e-9 && at[1].abs() < 1e-9 && (at[2] - 5.5).abs() < 0.3,
        "{at:?}"
    );
    // A guard that moves with the sub-spindle, 14.5 to 15.5 mm in front of
    // its jaws' face and 11 to 13 mm from the axis: the back's facing
    // starts there and the flat passes it.
    let guarded = with_sub(&sub_lathe(), |s| {
        s.zones = vec![zone("Guard", [11.0, -100.0, -15.5], [13.0, 100.0, -14.5])];
    });
    assert_eq!(
        messages(&guarded, &program, -200.0),
        vec![
            "Back face: the tool runs into Guard, which moves with the sub-spindle.".to_owned(),
            "Back flat: the tool runs into Guard, which moves with the sub-spindle.".to_owned(),
        ]
    );
    // The same guard as a fixed zone of the machine is not where the part
    // is now: the machine's own zones stay where they are.
    let mut fixed = sub_lathe();
    fixed.zones = vec![zone("Guard", [11.0, -100.0, -15.5], [13.0, 100.0, -14.5])];
    assert!(messages(&fixed, &program, -200.0).is_empty());
    // The guard found without the work zero too (the simulation's hits).
    let hits = sub_spindle_hits(&guarded, &program, None);
    assert_eq!(hits.len(), 2, "{hits:?}");
    assert_eq!(hits[0].operation, 1);
    assert_eq!(hits[0].at, Some([12.0, 0.0, -20.0]));
    // A facing on the main spindle while the sub-spindle waits at machine
    // Z -199, a millimetre in front of the work zero at -200: the facing's
    // retract to 2 mm in front of it goes behind its jaws' face.
    let mut first = sub_program();
    let mut facing = first.operations[1].clone();
    facing.name = "Face".into();
    facing.sub = false;
    first.operations.insert(0, facing);
    let near = with_sub(&sub_lathe(), |s| s.park = Some(-199.0));
    assert_eq!(
        messages(&near, &first, -200.0),
        vec!["Face: the tool runs into the sub-spindle's chuck.".to_owned()]
    );
    // Waiting at the far end of its travel it is clear.
    assert!(messages(&sub_lathe(), &first, -200.0).is_empty());
}

/// The back's hole at C 90 and a flat on the plane at C 30, the tool
/// moving 5 mm along the plane's Y.
fn turned_program() -> TurningProgram {
    let mut program = sub_program();
    let hole = Toolpath {
        moves: vec![
            rapid([0.0, 6.0, 5.0]),
            rapid([0.0, 6.0, 1.0]),
            Move::Line {
                to: [0.0, 6.0, -4.0],
                feed: 100.0,
            },
            rapid([0.0, 6.0, 5.0]),
        ],
        cycles: vec![DrillCycle {
            first: 1,
            count: 3,
            kind: CycleKind::Drill,
            holes: vec![[0.0, 6.0]],
            clearance: 5.0,
            retract: 1.0,
            bottom: -4.0,
            peck: 0.0,
            feed: 100.0,
        }],
    };
    program.operations[2] = sub_live("Back hole", 5, 2000.0, LiveSide::Face, hole);
    if let Some(live) = &mut program.operations[3].live {
        live.side = LiveSide::Plane { c: 30.0, b: 90.0 };
    }
    program
}

/// The reference program of a control that mirrors only Z on the
/// sub-spindle: the hole at C -90 and the flat at C -30, its Y -5, the
/// turning and the hand-over as on a control that turns the part round.
#[test]
fn a_z_only_control_gets_y_and_c_turned_round() {
    let program = turned_program();
    let turned = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap();
    assert!(turned.contains("\nG0 X12. C90.\n"), "{turned}");
    assert!(turned.contains("\nG0 C30.\n"), "{turned}");
    assert!(turned.contains("\nY5. F400.\n"), "{turned}");
    let mut program = turned_program();
    program.mill_turn.sub_mirror_z_only = true;
    let text = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap();
    let (_, body) = text.split_once("G54\n").expect("the header's work offset");
    let expected = "\
(HAND-OVER TO THE SUB-SPINDLE)
G0 X100. Z50.
G97 S500 M3
M213
M79
G0 A25.
G98
G1 A-5. F1000.
G99
M78
(PART OFF)
G0 X100. Z50.
T0202
G97 S800 M3
M8
G0 X24. Z-23.
G1 X0. F0.05
G0 X24.
M9
G0 A25.
M212
M5
M205
G55
(BACK FACE)
G0 X100. Z50.
T0101
G50 S3000
G96 S200 M203
M8
G0 X24. Z0.
G1 X0. F0.1
G0 X4. Z2.
(BACK HOLE)
M9
G0 X100. Z50.
T0505
M205
M254
G28 H0.
G98
M133 P2000
M8
G0 X12. C-90.
Z5.
Z1.
M114
G1 Z-4. F100.
G0 Z5.
(BACK FLAT)
M9
M135
G0 X100. Z50.
T0606
M133 P2500
M8
M115
G0 C-30.
M114
G19
G0 Y0. Z0.
X32.
X24.
G1 X18. F200.
Y-5. F400.
G0 X32.
G18
M9
M135
M115
M255
G99
G0 X100. Z50.
M30
%
";
    assert_eq!(body, expected);
    // Read back with the lathe's codes: the flat's cut along Y the other
    // way.
    let read = check_program_with(&text, &program.mill_turn).unwrap();
    let flat: Vec<&check::LiveRead> = read
        .live_moves
        .iter()
        .filter(|m| m.mode == LiveMode::Plane && m.motion == 1)
        .collect();
    assert_eq!(flat.len(), 2, "{:?}", read.live_moves);
    assert!(flat[1].y == -5.0 && flat[1].feed == Some(400.0));
    // With cycles the hole is G83 at C -90.
    let settings = PostSettings {
        canned_cycles: true,
        ..PostSettings::default()
    };
    let text = turning_post(&program, &FanucTurning, &settings).unwrap();
    assert!(
        text.contains("\nG83 X12. C-90. Z-4. R-4. F100.\n"),
        "{text}"
    );
}

/// The mirrored operation is the same cuts: arcs run the other way, a
/// plane's C and the path's Y (on the diameter its length round) turned
/// round, the rest as it was; twice is the operation again.
#[test]
fn the_mirrored_live_operation_turns_y_round() {
    let arc = Toolpath {
        moves: vec![
            rapid([5.0, 2.0, 3.0]),
            Move::Arc {
                to: [3.0, 4.0, 3.0],
                center: [3.0, 2.0],
                ccw: true,
                feed: 300.0,
            },
        ],
        cycles: Vec::new(),
    };
    let op = sub_live(
        "Arc",
        7,
        2000.0,
        LiveSide::Plane { c: 45.0, b: 0.0 },
        arc.clone(),
    );
    let live = op.live.unwrap();
    let mirrored = live.mirrored_y();
    assert_eq!(mirrored.side, LiveSide::Plane { c: -45.0, b: 0.0 });
    assert_eq!(
        mirrored.path.moves[1],
        Move::Arc {
            to: [3.0, -4.0, 3.0],
            center: [3.0, -2.0],
            ccw: false,
            feed: 300.0,
        }
    );
    assert_eq!(mirrored.mirrored_y(), live);
    // On the diameter the length round the cylinder turns round.
    let op = sub_live("Slot", 8, 2000.0, LiveSide::Diameter { radius: 10.0 }, arc);
    let mirrored = op.live.unwrap().mirrored_y();
    assert_eq!(mirrored.path.moves[0], rapid([-5.0, 2.0, 3.0]));
    // Its points are the same cuts seen with the work Y turned round.
    let mirrored = live.mirrored_y();
    let points = live.points(0.01);
    let turned = mirrored.points(0.01);
    assert_eq!(points.len(), turned.len());
    for (p, q) in points.iter().zip(&turned) {
        let a = live.side.to_work(p.0);
        let b = mirrored.side.to_work(q.0);
        assert!(
            (a[0] - b[0]).abs() < 1e-9 && (a[1] + b[1]).abs() < 1e-9 && (a[2] - b[2]).abs() < 1e-9,
            "{a:?} {b:?}"
        );
    }
}
