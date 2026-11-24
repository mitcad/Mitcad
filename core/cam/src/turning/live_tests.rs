// SPDX-License-Identifier: MIT
//! Live tools on the C axis (mitcad#2067): a flange's holes on the face,
//! a slot on the face, a cross hole and a wrapped slot on the diameter,
//! then a finishing cut, in exact Fanuc G-code with the C axis indexed,
//! polar and cylindrical interpolation; the drilling cycles; the program
//! check, the lathe check and the simulation of the live tools' cuts.

use super::live::{LiveOp, LiveSide, diameter_map, face_map, simulate_live};
use super::*;
use crate::machine::{Bounds, Machine, Spindle, ToolChanger};
use crate::post::PostSettings;
use crate::stock::Grid;
use crate::tool::Tool;
use crate::toolpath::{CycleKind, DrillCycle, Move, Toolpath};

fn station(number: u32, name: &str) -> TurningTool {
    TurningTool {
        number,
        offset: 0,
        name: name.into(),
        kind: TurningToolKind::Live,
        nose_radius: 0.0,
        max_depth: 10.0,
    }
}

fn live_op(
    name: &str,
    number: u32,
    diameter: f64,
    rpm: f64,
    side: LiveSide,
    path: Toolpath,
) -> TurningProgramOp {
    TurningProgramOp {
        name: name.into(),
        tool: station(number, &format!("{diameter} mm end mill")),
        spindle: TurnSpindle::Rpm { rpm },
        direction: SpindleDirection::Cw,
        coolant: Coolant::Flood,
        path: TurnPath::default(),
        live: Some(LiveOp {
            side,
            cutter: Tool::flat_end_mill(number, diameter),
            path,
            rapid_feed: 3000.0,
            head: false,
        }),
        sub: false,
    }
}

fn rapid(to: [f64; 3]) -> Move {
    Move::Rapid { to }
}

fn line(to: [f64; 3], feed: f64) -> Move {
    Move::Line { to, feed }
}

/// The length round the cylinder of `radius` at `degrees`.
fn round_at(radius: f64, degrees: f64) -> f64 {
    radius * degrees.to_radians()
}

/// The flange's program: two holes on the face (at 0 and 90 degrees on a
/// 20 mm circle), a slot on the face ending in a quarter circle, a cross
/// hole at 90 degrees on the diameter, a slot on the 24 mm diameter with
/// a half circle, and a finishing cut.
fn flange_program() -> TurningProgram {
    let holes = Toolpath {
        moves: vec![
            rapid([10.0, 0.0, 5.0]),
            rapid([10.0, 0.0, 1.0]),
            line([10.0, 0.0, -6.0], 100.0),
            rapid([10.0, 0.0, 5.0]),
            rapid([0.0, 10.0, 5.0]),
            rapid([0.0, 10.0, 1.0]),
            line([0.0, 10.0, -6.0], 100.0),
            rapid([0.0, 10.0, 5.0]),
        ],
        cycles: vec![DrillCycle {
            first: 1,
            count: 7,
            kind: CycleKind::Drill,
            holes: vec![[10.0, 0.0], [0.0, 10.0]],
            clearance: 5.0,
            retract: 1.0,
            bottom: -6.0,
            peck: 0.0,
            feed: 100.0,
        }],
    };
    let slot = Toolpath {
        moves: vec![
            rapid([10.0, 0.0, 5.0]),
            rapid([10.0, 0.0, 1.0]),
            line([10.0, 0.0, -2.0], 200.0),
            line([10.0, 5.0, -2.0], 400.0),
            Move::Arc {
                to: [5.0, 10.0, -2.0],
                center: [5.0, 5.0],
                ccw: true,
                feed: 400.0,
            },
            rapid([5.0, 10.0, 5.0]),
        ],
        cycles: Vec::new(),
    };
    let s = round_at(15.0, 90.0);
    let cross = Toolpath {
        moves: vec![
            rapid([s, -10.0, 20.0]),
            rapid([s, -10.0, 16.0]),
            line([s, -10.0, 10.0], 80.0),
            rapid([s, -10.0, 20.0]),
        ],
        cycles: vec![DrillCycle {
            first: 1,
            count: 3,
            kind: CycleKind::Peck,
            holes: vec![[s, -10.0]],
            clearance: 20.0,
            retract: 16.0,
            bottom: 10.0,
            peck: 2.0,
            feed: 80.0,
        }],
    };
    let wrapped = Toolpath {
        moves: vec![
            rapid([0.0, -5.0, 17.0]),
            rapid([0.0, -5.0, 13.0]),
            line([0.0, -5.0, 12.0], 150.0),
            line([10.0, -5.0, 12.0], 300.0),
            Move::Arc {
                to: [10.0, -15.0, 12.0],
                center: [10.0, -10.0],
                ccw: true,
                feed: 300.0,
            },
            rapid([10.0, -15.0, 17.0]),
        ],
        cycles: Vec::new(),
    };
    let finish = TurnPath {
        moves: vec![
            TurnMove::Rapid { to: [12.0, 2.0] },
            TurnMove::Line {
                to: [12.0, -20.0],
                feed: 0.1,
            },
            TurnMove::Rapid { to: [20.0, -20.0] },
        ],
        cycle: None,
    };
    let turning = TurningTool {
        number: 1,
        offset: 0,
        name: "Turning tool".into(),
        kind: TurningToolKind::Turning,
        nose_radius: 0.0,
        max_depth: 4.0,
    };
    let mut program = TurningProgram::new("Flange", [50.0, 50.0]);
    program.operations = vec![
        live_op("Face holes", 5, 5.0, 2000.0, LiveSide::Face, holes),
        live_op("Face slot", 6, 4.0, 3000.0, LiveSide::Face, slot),
        live_op(
            "Cross hole",
            7,
            3.0,
            2500.0,
            LiveSide::Diameter { radius: 15.0 },
            cross,
        ),
        live_op(
            "Wrap slot",
            8,
            4.0,
            3000.0,
            LiveSide::Diameter { radius: 12.0 },
            wrapped,
        ),
        TurningProgramOp {
            name: "Finish".into(),
            tool: turning,
            spindle: TurnSpindle::Css {
                surface_speed: 200.0,
                max_rpm: 3000.0,
            },
            direction: SpindleDirection::Cw,
            coolant: Coolant::Flood,
            path: finish,
            live: None,
            sub: false,
        },
    ];
    program
}

/// The reference program: the C axis indexed and clamped for the holes,
/// polar interpolation for the face's slot, cylindrical for the
/// diameter's, feeds per minute until the C axis is off again.
#[test]
fn live_tools_post_as_fanuc_c_axis_code() {
    let program = flange_program();
    program.validate().unwrap();
    let text = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap();
    let (_, body) = text.split_once("G54\n").expect("the header's work offset");
    let expected = "\
(FACE HOLES)
G0 X100. Z50.
T0505
M154
G28 H0.
G98
M133 P2000
M8
G0 X20. C0.
Z5.
Z1.
M14
G1 Z-6. F100.
G0 Z5.
M15
C90.
Z1.
M14
G1 Z-6.
G0 Z5.
(FACE SLOT)
M9
M135
G0 X100. Z50.
T0606
M133 P3000
M8
M15
G0 C0.
G0 X20. Z5.
G17
G12.1
G1 Z1. F3000.
Z-2. F200.
C5. F400.
G3 X10. C10. I-5. J0.
G1 Z5. F3000.
G13.1
G18
(CROSS HOLE)
M9
M135
G0 X100. Z50.
T0707
M133 P2500
M8
G0 Z-10. C90.
X40.
X32.
M14
G1 X20. F80.
G0 X40.
(WRAP SLOT)
M9
M135
G0 X100. Z50.
T0808
M133 P3000
M8
M15
G0 X34.
G0 Z-5. C0.
G19
G7.1 C12.
G1 X26. F3000.
X24. F150.
C47.746 F300.
G3 Z-10. C23.873 R5.
Z-15. C47.746 R5.
G1 X34. F3000.
G7.1 C0.
G18
(FINISH)
M135
M155
G99
M9
G0 X100. Z50.
T0101
G50 S3000
G96 S200 M3
M8
G0 X24. Z2.
G1 Z-20. F0.1
G0 X40.
M9
X100. Z50.
M5
M30
%
";
    assert_eq!(body, expected);

    // Read back: the live moves in their modes, the polar ones on the
    // face's plane, the cylindrical ones in degrees.
    let read = check_program(&text).unwrap();
    let modes: Vec<check::LiveMode> = read.live_moves.iter().map(|m| m.mode).collect();
    assert!(modes.contains(&check::LiveMode::Polar));
    assert!(modes.contains(&check::LiveMode::Cylindrical));
    let arc = read
        .live_moves
        .iter()
        .find(|m| m.mode == check::LiveMode::Polar && m.motion == 3)
        .unwrap();
    assert!((arc.x - 5.0).abs() < 1e-9 && (arc.c - 10.0).abs() < 1e-9 && arc.z == -2.0);
    assert_eq!(arc.feed, Some(400.0));
    let last = read
        .live_moves
        .iter()
        .rfind(|m| m.mode == check::LiveMode::Cylindrical && m.motion == 3)
        .unwrap();
    assert!((last.c - 47.746).abs() < 1e-9 && last.z == -15.0 && last.x == 12.0);
    // The finishing cut is turning again, per revolution.
    let finish = read.moves.last().unwrap();
    assert_eq!(finish.motion, TurnMove::Rapid { to: [50.0, 50.0] });
    assert!(read.moves.iter().any(|m| m.motion
        == TurnMove::Line {
            to: [12.0, -20.0],
            feed: 0.1
        }));
}

/// With cycles: G83 on the face for both holes (the second by its place),
/// G87 with its pecks on the diameter, each cancelled by G80.
#[test]
fn live_drilling_posts_as_g83_and_g87() {
    let program = flange_program();
    let settings = PostSettings {
        canned_cycles: true,
        ..PostSettings::default()
    };
    let text = turning_post(&program, &FanucTurning, &settings).unwrap();
    assert!(
        text.contains(
            "M8\nG0 X20. C0.\nZ5.\nG83 X20. C0. Z-6. R-4. F100.\nX20. C90.\nG80\n(FACE SLOT)\n"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "G0 Z-10. C90.\nX40.\nG87 Z-10. C90. X20. R-4. Q2000 F80.\nG80\n(WRAP SLOT)\n"
        ),
        "{text}"
    );
    let read = check_program(&text).unwrap();
    let kinds: Vec<&str> = read.cycles.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(kinds, vec!["G83", "G87"]);
}

#[test]
fn the_program_check_refuses_what_the_c_axis_would_misread() {
    let base = "%\nO1000\nG21 G18 G40 G99\nG54\nG0 X100. Z50.\nT0505\nM154\nG98\nM133 P2000\n";
    let refuse = |body: &str| {
        check_program(&format!("{base}{body}M30\n%\n"))
            .unwrap_err()
            .message
    };
    assert_eq!(
        refuse("G0 C0.\nG0 X20. Z5.\nG17\nG12.1\nG0 X10. C5.\n"),
        "Line 14: a rapid (G0) in polar coordinate interpolation, which the control does not take."
    );
    assert_eq!(
        refuse("G0 C0.\nG0 X20. Z5.\nG12.1\n"),
        "Line 12: polar coordinate interpolation outside the XY plane (G17)."
    );
    assert_eq!(
        refuse("G0 C0.\nG0 X20. Z5.\nG17\nG12.1\nT0606\n"),
        "Line 14: a tool change in polar coordinate interpolation."
    );
    assert_eq!(
        refuse("G0 C0.\nG0 X20. Z5.\nG17\nG12.1\nG1 X10. F200.\n"),
        "Line 15: the program ends in polar coordinate interpolation."
    );
    assert_eq!(
        refuse("G0 C90\n"),
        "Line 10: C needs a decimal point, or the control reads it in its smallest steps."
    );
    assert_eq!(
        refuse("G0 X20. C0.\nG99\nG1 Z-5. F100.\n"),
        "Line 12: a cut with the main spindle stopped before the feeds are per minute (G98)."
    );
    assert_eq!(
        refuse("G0 X40. Z-10.\nG19\nG7.1 C12.\nG1 Z-5. C10. F100.\nG2 Z-10. C20.\nG7.1 C0.\nG18\n"),
        "Line 14: an arc without its radius (R)."
    );
    assert_eq!(
        refuse("G0 X20. C0. Z5.\nG83 X20. C0. R-4. F100.\n"),
        "Line 11: G83 needs its Z word."
    );
}

fn mill_turn_lathe(mills: bool) -> Machine {
    Machine {
        name: "Mill-turn".into(),
        controller: "fanuc_turning".into(),
        travel: Bounds {
            min: [-5.0, 0.0, -400.0],
            max: [200.0, 0.0, 300.0],
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
                c: mills,
                live_tools: mills,
                ..LatheAxes::default()
            },
            chuck_diameter: 160.0,
            chuck_length: 50.0,
            ..Lathe::default()
        }),
    }
}

#[test]
fn the_lathe_check_needs_the_c_axis_and_the_live_spindle() {
    let mut program = flange_program();
    let placement = check::TurnPlacement {
        work_z: 0.0,
        chuck_z: Some(-60.0),
    };
    let findings = check_lathe(&mill_turn_lathe(true), &program, &placement);
    assert!(findings.is_empty(), "{findings:?}");
    let findings = check_lathe(&mill_turn_lathe(false), &program, &placement);
    assert_eq!(findings.len(), 4, "{findings:?}");
    assert_eq!(
        findings[0].message,
        "Face holes: the lathe has no C axis with live tools to cut on the face."
    );
    assert_eq!(
        findings[2].message,
        "Cross hole: the lathe has no C axis with live tools to cut on the diameter."
    );
    // Faster than the live spindle turns.
    program.operations[1].spindle = TurnSpindle::Rpm { rpm: 9000.0 };
    let findings = check_lathe(&mill_turn_lathe(true), &program, &placement);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(
        findings[0].message,
        "Face slot: the live tool's speed is outside what its spindle can do."
    );
    assert_eq!(findings[0].operation, 1);
}

#[test]
fn a_live_tool_must_be_on_a_live_station() {
    let mut program = flange_program();
    program.operations[0].tool.kind = TurningToolKind::Turning;
    assert_eq!(
        program.validate().unwrap_err().message,
        "Face holes cuts with a live tool: its station must hold one."
    );
    let mut program = flange_program();
    program.operations[4].tool.kind = TurningToolKind::Live;
    assert_eq!(
        program.validate().unwrap_err().message,
        "Finish: a live tool does not turn; give the operation a turning tool."
    );
    let mut program = flange_program();
    program.mill_turn.live_cw = "M133 P{speed}".into();
    assert_eq!(
        program.validate().unwrap_err().message,
        "The code that starts the live tool names something MitCad does not fill in: only {rpm} in the live spindle's codes."
    );
}

/// The turned stock seen from the face and unrolled on the diameter.
#[test]
fn the_turned_stock_maps_onto_the_face_and_the_diameter() {
    // A bar 20 mm in diameter with a 10 mm spigot 5 long on its face.
    let z: Vec<f64> = (0..=40).map(|i| -10.0 + 0.25 * f64::from(i)).collect();
    let outer: Vec<f64> = z
        .iter()
        .map(|z| if *z > -5.0 { 5.0 } else { 10.0 })
        .collect();
    let stock = TurnStock {
        inner: vec![0.0; z.len()],
        outer,
        z,
    };
    let grid = Grid::covering([-12.0, -12.0], [12.0, 12.0], 0.5);
    let face = face_map(&stock, grid, -20.0);
    assert_eq!(face.height_at(0.0, 0.0), Some(0.0));
    assert_eq!(face.height_at(3.0, 3.0), Some(0.0));
    assert_eq!(face.height_at(0.0, 8.0), Some(-5.0));
    assert_eq!(face.height_at(11.0, 0.0), Some(-20.0));
    let grid = Grid::covering([-10.0, -10.0], [10.0, 0.0], 0.25);
    let diameter = diameter_map(&stock, grid);
    assert_eq!(diameter.height_at(3.0, -2.0), Some(5.0));
    assert_eq!(diameter.height_at(-3.0, -8.0), Some(10.0));
}

/// The flange's live cuts out of its turned bar: holes 6 deep on the
/// face, the slot 2 deep, the cross hole down to 10 mm from the axis and
/// the wrapped slot at 12; no rapid cuts.
#[test]
fn live_tools_cut_the_turned_stock() {
    let part = TurnPart {
        outside: vec![PV::new(0.0, 0.0), PV::new(12.5, 0.0), PV::new(12.5, -30.0)],
        inside: vec![],
        stock_radius: 13.0,
        stock_inner_radius: 0.0,
        stock_face: 1.0,
    };
    let program = flange_program();
    let sims = simulate_live(&part, &program, -40.0, 0.05, 0.25, 1_000_000);
    assert_eq!(sims.len(), 3);
    for sim in &sims {
        assert!(
            sim.simulation.rapid_cuts.is_empty(),
            "{:?}",
            sim.simulation.rapid_cuts
        );
    }
    let face = &sims[0];
    assert_eq!(face.side, LiveSide::Face);
    assert_eq!(face.operations, vec![0, 1]);
    let h = |x: f64, y: f64| face.simulation.stock.height_at(x, y).unwrap();
    assert!((h(10.0, 0.0) + 6.0).abs() < 1e-6, "{}", h(10.0, 0.0));
    assert!((h(0.0, 10.0) + 6.0).abs() < 1e-6, "{}", h(0.0, 10.0));
    assert!((h(10.0, 4.0) + 2.0).abs() < 1e-6, "{}", h(10.0, 4.0));
    assert!((h(-5.0, -5.0) - 1.0).abs() < 1e-6, "{}", h(-5.0, -5.0));
    let cross = &sims[1];
    assert_eq!(cross.side, LiveSide::Diameter { radius: 15.0 });
    let r = cross
        .simulation
        .stock
        .height_at(round_at(15.0, 90.0), -10.0)
        .unwrap();
    assert!((r - 10.0).abs() < 1e-6, "{r}");
    let wrapped = &sims[2];
    let r = wrapped.simulation.stock.height_at(5.0, -5.0).unwrap();
    assert!((r - 12.0).abs() < 1e-6, "{r}");
    assert_eq!(wrapped.operations, vec![3]);
}
