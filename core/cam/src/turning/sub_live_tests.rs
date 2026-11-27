// SPDX-License-Identifier: MIT
//! Tests of the live tools on the sub-spindle and the sub-spindle's own
//! machine data (mitcad#2375): a pin parted off into the sub-spindle, its
//! back faced there, a hole drilled on its back with the sub-spindle's C
//! axis indexed and a flat milled with the Y axis, in exact Fanuc G-code
//! read back with the lathe's codes; the lathe check of the sub-spindle's
//! travel, chuck and speeds; the live tools' maps and the stock in 3D on
//! the sub-spindle.

use super::check::LiveMode;
use super::live::{LiveOp, LiveSide, simulate_live};
use super::*;
use crate::machine::{Bounds, Machine, Spindle, ToolChanger};
use crate::post::PostSettings;
use crate::tool::Tool;
use crate::toolpath::{CycleKind, DrillCycle, Move, Toolpath};

fn station(number: u32) -> TurningTool {
    TurningTool {
        number,
        offset: 0,
        name: format!("Live {number}"),
        kind: TurningToolKind::Live,
        nose_radius: 0.0,
        max_depth: 10.0,
        back_angle: None,
    }
}

/// A live operation on the sub-spindle.
pub(super) fn sub_live(
    name: &str,
    number: u32,
    rpm: f64,
    side: LiveSide,
    path: Toolpath,
) -> TurningProgramOp {
    TurningProgramOp {
        name: name.into(),
        tool: station(number),
        spindle: TurnSpindle::Rpm { rpm },
        direction: SpindleDirection::Cw,
        coolant: Coolant::Flood,
        path: TurnPath::default(),
        live: Some(LiveOp {
            side,
            cutter: Tool::flat_end_mill(number, 4.0),
            path,
            rapid_feed: 3000.0,
            head: false,
        }),
        sub: true,
    }
}

pub(super) fn rapid(to: [f64; 3]) -> Move {
    Move::Rapid { to }
}

pub(super) fn line(to: [f64; 3], feed: f64) -> Move {
    Move::Line { to, feed }
}

/// The hole on the back: 6 mm off the axis at C 0, 4 mm deep from the
/// back (the sub-spindle's work Z 0), as a drilling cycle too.
pub(super) fn back_hole(bottom: f64) -> Toolpath {
    Toolpath {
        moves: vec![
            rapid([6.0, 0.0, 5.0]),
            rapid([6.0, 0.0, 1.0]),
            line([6.0, 0.0, bottom], 100.0),
            rapid([6.0, 0.0, 5.0]),
        ],
        cycles: vec![DrillCycle {
            first: 1,
            count: 3,
            kind: CycleKind::Drill,
            holes: vec![[6.0, 0.0]],
            clearance: 5.0,
            retract: 1.0,
            bottom,
            peck: 0.0,
            feed: 100.0,
        }],
    }
}

/// A pin parted off into the sub-spindle, its back faced there, a hole
/// drilled on the back and a flat milled 1 mm deep across it with the Y
/// axis, all on the sub-spindle.
pub(super) fn sub_program() -> TurningProgram {
    let parting = TurningTool {
        number: 2,
        offset: 0,
        name: "Parting blade".into(),
        kind: TurningToolKind::Parting { width: 3.0 },
        nose_radius: 0.0,
        max_depth: 15.0,
        back_angle: None,
    };
    let turning = TurningTool {
        number: 1,
        offset: 0,
        name: "Turning tool".into(),
        kind: TurningToolKind::Turning,
        nose_radius: 0.0,
        max_depth: 4.0,
        back_angle: None,
    };
    let flat = Toolpath {
        moves: vec![
            rapid([0.0, 0.0, 16.0]),
            rapid([0.0, 0.0, 12.0]),
            line([0.0, 0.0, 9.0], 200.0),
            line([0.0, 5.0, 9.0], 400.0),
            rapid([0.0, 5.0, 16.0]),
        ],
        cycles: Vec::new(),
    };
    let mut program = TurningProgram::new("Pin back", [50.0, 50.0]);
    program.hand_over = Some(HandOver {
        grip_z: -5.0,
        withdraw: 30.0,
        rpm: 500.0,
        feed: 1000.0,
        work_offset: 2,
        back_z: -20.0,
    });
    program.operations = vec![
        TurningProgramOp {
            name: "Part off".into(),
            tool: parting,
            spindle: TurnSpindle::Rpm { rpm: 800.0 },
            direction: SpindleDirection::Cw,
            coolant: Coolant::Flood,
            path: TurnPath {
                moves: vec![
                    TurnMove::Rapid { to: [12.0, -23.0] },
                    TurnMove::Line {
                        to: [0.0, -23.0],
                        feed: 0.05,
                    },
                    TurnMove::Rapid { to: [12.0, -23.0] },
                ],
                cycle: None,
            },
            live: None,
            sub: false,
        },
        TurningProgramOp {
            name: "Back face".into(),
            tool: turning,
            spindle: TurnSpindle::Css {
                surface_speed: 200.0,
                max_rpm: 3000.0,
            },
            direction: SpindleDirection::Cw,
            coolant: Coolant::Flood,
            path: TurnPath {
                moves: vec![
                    TurnMove::Rapid { to: [12.0, 0.0] },
                    TurnMove::Line {
                        to: [0.0, 0.0],
                        feed: 0.1,
                    },
                    TurnMove::Rapid { to: [2.0, 2.0] },
                ],
                cycle: None,
            },
            live: None,
            sub: true,
        },
        sub_live("Back hole", 5, 2000.0, LiveSide::Face, back_hole(-4.0)),
        sub_live(
            "Back flat",
            6,
            2500.0,
            LiveSide::Plane { c: 0.0, b: 90.0 },
            flat,
        ),
    ];
    program
}

#[test]
fn live_tools_may_follow_the_hand_over() {
    let program = sub_program();
    program.validate().unwrap();
    assert_eq!(program.first_sub(), Some(1));
    assert_eq!(program.hand_over_at(), Some(0));
    // A live operation may be the first on the sub-spindle too.
    let mut program = sub_program();
    program.operations.remove(1);
    program.validate().unwrap();
    assert_eq!(program.hand_over_at(), Some(0));
    // The sub-spindle's operations still come after the main spindle's.
    let mut program = sub_program();
    program.operations[3].sub = false;
    assert_eq!(
        program.validate().unwrap_err().message,
        "Back flat is on the main spindle after the hand-over: put it before the sub-spindle's operations."
    );
}

/// The reference program: after the back is faced the sub-spindle stops,
/// its C axis goes on with its own codes, the hole is drilled with it
/// indexed and clamped with its clamp, the flat milled on G19 with it
/// clamped at 0, and its C axis goes off at the end.
#[test]
fn live_tools_on_the_sub_spindle_post_as_fanuc_code() {
    let program = sub_program();
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
G0 X12. C0.
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
G0 C0.
M114
G19
G0 Y0. Z0.
X32.
X24.
G1 X18. F200.
Y5. F400.
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
    // Read back: the flat's cuts on its plane with the sub-spindle's C
    // axis at 0, the hole's cut per minute with the sub-spindle stopped.
    let read = check_program(&text).unwrap();
    let flat: Vec<&check::LiveRead> = read
        .live_moves
        .iter()
        .filter(|m| m.mode == LiveMode::Plane && m.motion == 1)
        .collect();
    assert_eq!(flat.len(), 2, "{:?}", read.live_moves);
    assert!(flat[0].x == 9.0 && flat[0].z == 0.0 && flat[0].c == 0.0);
    assert!(flat[1].y == 5.0 && flat[1].feed == Some(400.0));
    assert!(read.moves.iter().any(|m| m.motion
        == TurnMove::Line {
            to: [6.0, -4.0],
            feed: 100.0
        }));
}

/// With cycles the back's hole is G83 with the sub-spindle's C axis.
#[test]
fn a_hole_on_the_sub_spindle_posts_as_g83() {
    let program = sub_program();
    let settings = PostSettings {
        canned_cycles: true,
        ..PostSettings::default()
    };
    let text = turning_post(&program, &FanucTurning, &settings).unwrap();
    assert!(
        text.contains("M254\nG28 H0.\nG98\nM133 P2000\nM8\nG0 X12. C0.\nZ5.\nG83 X12. C0. Z-4. R-4. F100.\nG80\n"),
        "{text}"
    );
    let read = check_program(&text).unwrap();
    let kinds: Vec<&str> = read.cycles.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(kinds, vec!["G83"]);
}

/// A lathe whose sub-spindle stops with a code of its own: the program
/// reads back with its codes, not with the default set.
#[test]
fn the_program_check_reads_the_sub_spindle_s_own_stop() {
    let mut program = sub_program();
    program.mill_turn.sub_stop = "M305".into();
    program.mill_turn.sub_c_axis_on = vec!["M264".into()];
    let text = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap();
    assert!(text.contains("T0505\nM305\nM264\nG98\n"), "{text}");
    check_program_with(&text, &program.mill_turn).unwrap();
    // Read with the default codes the sub-spindle still turns as the hole
    // is drilled.
    assert!(
        check_program(&text)
            .unwrap_err()
            .message
            .contains("a feed before the feeds are per revolution (G99)")
    );
}

#[test]
fn the_sub_spindle_s_codes_and_data_are_checked() {
    let mut codes = MillTurn::default();
    codes.sub_c_axis_on.clear();
    assert_eq!(
        codes.validate().unwrap_err(),
        "The sub-spindle's C axis needs one to eight blocks that switch it on."
    );
    let codes = MillTurn {
        sub_c_clamp: "M114 {rpm}".into(),
        ..MillTurn::default()
    };
    assert!(codes.validate().is_err());
    let sub = SubSpindle {
        travel: Some([100.0, -100.0]),
        ..SubSpindle::default()
    };
    assert_eq!(
        sub.validate().unwrap_err(),
        "The sub-spindle's travel must run from its lowest Z up to its highest."
    );
    let sub = SubSpindle {
        min_rpm: 100.0,
        max_rpm: 50.0,
        ..SubSpindle::default()
    };
    assert!(sub.validate().is_err());
    // Its speeds: its own, else the main spindle's.
    assert_eq!(SubSpindle::default().speeds([50.0, 4000.0]), [50.0, 4000.0]);
    let sub = SubSpindle {
        max_rpm: 2000.0,
        ..SubSpindle::default()
    };
    assert_eq!(sub.speeds([50.0, 4000.0]), [0.0, 2000.0]);
    // Left out of the file at its defaults, read back when set.
    let lathe = Lathe::default();
    let json = serde_json::to_value(&lathe).unwrap();
    assert!(json.get("sub_spindle").is_none(), "{json}");
    let lathe = Lathe {
        sub_spindle: SubSpindle {
            travel: Some([-250.0, 300.0]),
            chuck_diameter: 100.0,
            ..SubSpindle::default()
        },
        ..Lathe::default()
    };
    let json = serde_json::to_value(&lathe).unwrap();
    assert_eq!(
        json["sub_spindle"],
        serde_json::json!({"travel": [-250.0, 300.0], "chuck_diameter": 100.0})
    );
    let back: Lathe = serde_json::from_value(json).unwrap();
    assert_eq!(back, lathe);
}

/// A mill-turn with a Y axis and a sub-spindle 100 mm across its chuck,
/// whose jaws' face travels from machine Z -250 to 300.
pub(super) fn sub_lathe() -> Machine {
    Machine {
        name: "Mill-turn".into(),
        controller: "fanuc_turning".into(),
        travel: Bounds {
            min: [-50.0, -40.0, -400.0],
            max: [200.0, 40.0, 300.0],
        },
        spindle: Spindle {
            min_rpm: 50.0,
            max_rpm: 4000.0,
            spin_up_seconds: 0.0,
            nose_diameter: 0.0,
            nose_length: 0.0,
        },
        max_feed: 5000.0,
        rapid_feed: 20000.0,
        zones: vec![],
        tool_changer: ToolChanger::Controller,
        park: None,
        lathe: Some(Lathe {
            axes: LatheAxes {
                c: true,
                live_tools: true,
                y: true,
                b: false,
                sub_spindle: true,
            },
            chuck_diameter: 160.0,
            chuck_length: 50.0,
            sub_spindle: SubSpindle {
                travel: Some([-250.0, 300.0]),
                chuck_diameter: 100.0,
                ..SubSpindle::default()
            },
            ..Lathe::default()
        }),
    }
}

/// The lathe check of the sub-spindle: the hand-over within its travel,
/// its speeds, no tool behind its jaws' face, its live tools where it
/// holds the part.
#[test]
fn the_lathe_check_knows_the_sub_spindle() {
    let placement = check::TurnPlacement {
        work_z: -200.0,
        chuck_z: None,
    };
    let findings = |m: &Machine, p: &TurningProgram| -> Vec<String> {
        check_lathe(m, p, &placement)
            .into_iter()
            .map(|f| f.message)
            .collect()
    };
    let program = sub_program();
    assert!(
        findings(&sub_lathe(), &program).is_empty(),
        "{:?}",
        findings(&sub_lathe(), &program)
    );
    // Its jaws' face grips at machine Z -205: beyond a travel from -150.
    let mut short = sub_lathe();
    if let Some(lathe) = &mut short.lathe {
        lathe.sub_spindle.travel = Some([-150.0, 300.0]);
    }
    let found = check_lathe(&short, &program, &placement);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        found[0].message,
        "The hand-over before Part off: the sub-spindle goes beyond its travel."
    );
    assert_eq!(found[0].operation, 0);
    assert_eq!(found[0].at, Some([0.0, 0.0, -5.0]));
    // Its own speeds: up to 2000 rpm, under the back's facing limit and
    // the synchronised 500 rpm within them.
    let mut slow = sub_lathe();
    if let Some(lathe) = &mut slow.lathe {
        lathe.sub_spindle.max_rpm = 2000.0;
    }
    assert_eq!(
        findings(&slow, &program),
        vec!["Back face: the spindle limit is above what the sub-spindle can do.".to_owned()]
    );
    let mut slower = sub_lathe();
    if let Some(lathe) = &mut slower.lathe {
        lathe.sub_spindle.min_rpm = 600.0;
        lathe.sub_spindle.max_rpm = 4000.0;
    }
    assert_eq!(
        findings(&slower, &program),
        vec![
            "The hand-over before Part off: the synchronized speed is outside what the spindles can do."
                .to_owned()
        ]
    );
    // A hole 16 mm deep from the back reaches behind the sub-spindle's
    // jaws' face (at the part's 15 mm from its back as it holds it); the
    // place is where the view draws the hole, on the part where it was.
    let mut deep = sub_program();
    if let Some(live) = &mut deep.operations[2].live {
        live.path = back_hole(-16.0);
    }
    let found = check_lathe(&sub_lathe(), &deep, &placement);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        found[0].message,
        "Back hole: the tool runs into the sub-spindle's chuck."
    );
    assert_eq!(found[0].operation, 2);
    let at = found[0].at.unwrap();
    assert!(
        (at[0] - 6.0).abs() < 1e-9 && at[1].abs() < 1e-9 && (at[2] + 4.0).abs() < 0.6,
        "{at:?}"
    );
    // A lathe without a sub-spindle has none to hold the back.
    let mut none = sub_lathe();
    if let Some(lathe) = &mut none.lathe {
        lathe.axes.sub_spindle = false;
    }
    let messages = findings(&none, &program);
    assert!(
        messages.contains(
            &"Back hole: the lathe has no sub-spindle to hold the part's back.".to_owned()
        ),
        "{messages:?}"
    );
    assert!(
        messages.contains(
            &"Back face: the lathe has no sub-spindle to turn the part's back.".to_owned()
        )
    );
    // The turret's travel where the sub-spindle holds the part: the back's
    // facing at machine Z -200 + (-20 + 30 - 0) = -190 is within it, not
    // within one that ends at -195.
    let mut near = sub_lathe();
    near.travel.max[2] = -195.0;
    let messages = findings(&near, &program);
    assert!(
        messages.contains(&"Back face: the tool goes beyond the machine's travel.".to_owned()),
        "{messages:?}"
    );
}

/// The main spindle's part-off under the hand-over (mitcad#2527): the
/// sub-spindle grips before it and turns with the main spindle through
/// it, so the part-off is held to both spindles' speeds.
#[test]
fn the_part_off_under_the_hand_over_is_held_to_the_sub_spindle_s_speeds() {
    let placement = check::TurnPlacement {
        work_z: -200.0,
        chuck_z: None,
    };
    let findings = |m: &Machine, p: &TurningProgram| -> Vec<(String, usize)> {
        check_lathe(m, p, &placement)
            .into_iter()
            .map(|f| (f.message, f.operation))
            .collect()
    };
    // The main spindle up to 4000 rpm, the sub-spindle up to 2000; the
    // back faced within the sub-spindle's.
    let mut lathe = sub_lathe();
    if let Some(l) = &mut lathe.lathe {
        l.sub_spindle.max_rpm = 2000.0;
    }
    let mut program = sub_program();
    program.operations[1].spindle = TurnSpindle::Css {
        surface_speed: 200.0,
        max_rpm: 2000.0,
    };
    assert!(
        findings(&lathe, &program).is_empty(),
        "{:?}",
        findings(&lathe, &program)
    );
    // The part-off at a constant surface speed under the main spindle's
    // limit: the post limits it after the grip, above the sub-spindle's.
    program.operations[0].spindle = TurnSpindle::Css {
        surface_speed: 80.0,
        max_rpm: 4000.0,
    };
    let text = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap();
    let grip = text
        .find("(HAND-OVER TO THE SUB-SPINDLE)")
        .expect("the grip");
    let limit = text.find("G50 S4000").expect("the part-off's limit");
    assert!(grip < limit, "{text}");
    assert_eq!(
        findings(&lathe, &program),
        vec![(
            "Part off: the spindle limit is above what the sub-spindle can do.".to_owned(),
            0
        )]
    );
    // At a constant speed above the sub-spindle's.
    program.operations[0].spindle = TurnSpindle::Rpm { rpm: 2500.0 };
    assert_eq!(
        findings(&lathe, &program),
        vec![(
            "Part off: the spindle speed is outside what the sub-spindle can do.".to_owned(),
            0
        )]
    );
    // Above the main spindle's too: the main spindle's finding.
    program.operations[0].spindle = TurnSpindle::Css {
        surface_speed: 80.0,
        max_rpm: 5000.0,
    };
    assert_eq!(
        findings(&lathe, &program),
        vec![(
            "Part off: the spindle limit is above what the spindle can do.".to_owned(),
            0
        )]
    );
    // Within the sub-spindle's: none.
    program.operations[0].spindle = TurnSpindle::Css {
        surface_speed: 80.0,
        max_rpm: 2000.0,
    };
    assert!(
        findings(&lathe, &program).is_empty(),
        "{:?}",
        findings(&lathe, &program)
    );
    // Without the hand-over the part-off turns the main spindle alone.
    let mut alone = sub_program();
    alone.operations.truncate(1);
    alone.hand_over = None;
    alone.operations[0].spindle = TurnSpindle::Css {
        surface_speed: 80.0,
        max_rpm: 4000.0,
    };
    assert!(
        findings(&lathe, &alone).is_empty(),
        "{:?}",
        findings(&lathe, &alone)
    );
}

fn pin() -> TurnPart {
    TurnPart {
        outside: vec![PV::new(0.0, 0.0), PV::new(10.0, 0.0), PV::new(10.0, -20.0)],
        inside: Vec::new(),
        stock_radius: 12.0,
        stock_inner_radius: 0.0,
        stock_face: 0.0,
    }
}

/// The live tools on the sub-spindle cut maps of their own, of the stock
/// as the sub-spindle holds it: the remnant gone, Z turned round at the
/// part's back.
#[test]
fn the_sub_spindle_s_live_tools_cut_their_own_maps() {
    let part = pin();
    let program = sub_program();
    let sims = simulate_live(&part, &program, -30.0, 0.1, 0.25, 200_000);
    let sides: Vec<(LiveSide, bool)> = sims.iter().map(|s| (s.side, s.sub)).collect();
    assert_eq!(
        sides,
        vec![
            (LiveSide::Face, true),
            (LiveSide::Plane { c: 0.0, b: 90.0 }, true)
        ]
    );
    for s in &sims {
        assert!(
            s.simulation.rapid_cuts.is_empty(),
            "{:?}",
            s.simulation.rapid_cuts
        );
        assert!(s.simulation.removed > 0.0, "{:?}", s.side);
    }
    // The back's map: the faced back at the sub-spindle's work Z 0 on the
    // part, the hole 4 mm down at 6 mm off the axis.
    let face = &sims[0];
    let grid = face.initial.grid;
    let node = |x: f64, y: f64| {
        let i = ((x - grid.origin[0]) / grid.step[0]).round() as usize;
        let j = ((y - grid.origin[1]) / grid.step[1]).round() as usize;
        grid.index(i, j)
    };
    assert!(
        face.initial.z[node(-6.0, 0.0)].abs() < 0.11,
        "{}",
        face.initial.z[node(-6.0, 0.0)]
    );
    assert!((face.simulation.stock.z[node(6.0, 0.0)] + 4.0).abs() < 1e-6);
}

/// The stock in 3D: the sub-spindle's hole and flat cut where the part's
/// back is, turned round about the work X.
#[test]
fn the_sub_spindle_s_live_tools_cut_the_stock_in_3d() {
    let part = pin();
    let program = sub_program();
    let sim = simulate_solid(&part, &program, -30.0, 0.1, 0.25, 400_000, None);
    assert!(sim.milled > 0.0);
    assert!(sim.rapid_cuts.is_empty(), "{:?}", sim.rapid_cuts);
    let v = &sim.voxels;
    let filled = |p: [f64; 3]| {
        let i = ((p[0] - v.origin[0]) / v.step).floor() as usize;
        let j = ((p[1] - v.origin[1]) / v.step).floor() as usize;
        let k = ((p[2] - v.origin[2]) / v.step).floor() as usize;
        v.filled[(k * v.n[1] + j) * v.n[0] + i]
    };
    // The hole: at the back's work X 6 (the same X), 2 mm into the part
    // from its back at main work Z -20.
    assert!(!filled([6.0, 0.0, -18.0]));
    assert!(filled([-6.0, 0.0, -18.0]));
    // The flat: 9 mm from the axis along X, the 4 mm tool on the
    // sub-spindle's Y 0 to 5 (the main spindle's Y 0 to -5) at its Z 0,
    // 2 mm into the part from its back; the other side of the axis stays.
    assert!(!filled([9.6, -2.5, -19.0]));
    assert!(filled([-9.6, -2.5, -19.0]));
    assert!(filled([9.6, -2.5, -15.0]));
    // The remnant behind the part-off is gone.
    assert!(!filled([0.0, 0.0, -27.0]));
}
