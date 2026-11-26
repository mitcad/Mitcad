// SPDX-License-Identifier: MIT
//! LinuxCNC on a lathe (mitcad#2425): a reference program in exact G-code
//! (facing, a roughing pass, finishing with a corner radius, a thread in
//! two passes, two tools, one with an offset number of its own), read back
//! as LinuxCNC reads it; inches with line numbers and a remapped M6;
//! canned cycles asked for and written as moves; a tapered thread's K;
//! what it does not write, refused in words; and the reader refusing what
//! a Fanuc lathe control would take.

use super::*;
use crate::post::{PostSettings, Units};

fn tool(number: u32, offset: u32, name: &str, kind: TurningToolKind) -> TurningTool {
    TurningTool {
        number,
        offset,
        name: name.into(),
        kind,
        nose_radius: 0.4,
        max_depth: 4.0,
    }
}

fn op(
    name: &str,
    tool: &TurningTool,
    spindle: TurnSpindle,
    moves: Vec<TurnMove>,
) -> TurningProgramOp {
    TurningProgramOp {
        name: name.into(),
        tool: tool.clone(),
        spindle,
        direction: SpindleDirection::Cw,
        coolant: Coolant::Flood,
        path: TurnPath { moves, cycle: None },
        live: None,
        sub: false,
    }
}

fn rapid(x: f64, z: f64) -> TurnMove {
    TurnMove::Rapid { to: [x, z] }
}

fn line(x: f64, z: f64, feed: f64) -> TurnMove {
    TurnMove::Line { to: [x, z], feed }
}

fn thread_pass(x: f64, z: f64, lead: f64) -> TurnMove {
    TurnMove::Thread { to: [x, z], lead }
}

fn css(surface_speed: f64, max_rpm: f64) -> TurnSpindle {
    TurnSpindle::Css {
        surface_speed,
        max_rpm,
    }
}

/// The reference program: a 16 mm pin faced, roughed, finished with a
/// 1 mm corner radius at its face, and an M16x1.5 thread cut in two
/// passes with tool 3 on offset 13.
fn reference() -> TurningProgram {
    let turning = tool(1, 0, "Turning tool", TurningToolKind::Turning);
    let threading = tool(
        3,
        13,
        "Threading tool",
        TurningToolKind::Threading {
            angle: 60.0,
            inside: false,
        },
    );
    let mut program = TurningProgram::new("Reference", [50.0, 100.0]);
    program.operations = vec![
        op(
            "Face",
            &turning,
            css(200.0, 3000.0),
            vec![
                rapid(16.0, 2.0),
                rapid(16.0, 0.0),
                line(0.0, 0.0, 0.15),
                rapid(0.0, 2.0),
                rapid(16.0, 2.0),
            ],
        ),
        op(
            "Turn",
            &turning,
            css(200.0, 3000.0),
            vec![
                rapid(12.0, 2.0),
                line(12.0, -20.0, 0.25),
                line(14.0, -20.0, 0.25),
                rapid(14.0, 2.0),
            ],
        ),
        op(
            "Finish",
            &turning,
            css(250.0, 3500.0),
            vec![
                rapid(0.0, 2.0),
                line(0.0, 0.0, 0.1),
                line(7.0, 0.0, 0.1),
                TurnMove::Arc {
                    to: [8.0, -1.0],
                    center: [7.0, -1.0],
                    cw: false,
                    feed: 0.1,
                },
                line(8.0, -20.0, 0.1),
                line(16.0, -20.0, 0.1),
                rapid(16.0, 2.0),
            ],
        ),
        op(
            "Thread",
            &threading,
            TurnSpindle::Rpm { rpm: 796.0 },
            vec![
                rapid(9.0, 4.0),
                rapid(7.5, 4.0),
                thread_pass(7.5, -15.0, 1.5),
                rapid(9.0, -15.0),
                rapid(9.0, 4.0),
                rapid(7.08, 4.0),
                thread_pass(7.08, -15.0, 1.5),
                rapid(9.0, -15.0),
                rapid(9.0, 4.0),
            ],
        ),
    ];
    program
}

fn same(a: &TurnMove, b: &TurnMove) -> bool {
    let close = |p: PXZ, q: PXZ| (p[0] - q[0]).abs() < 1e-9 && (p[1] - q[1]).abs() < 1e-9;
    let near = |f: f64, g: f64| (f - g).abs() < 1e-9;
    match (*a, *b) {
        (TurnMove::Rapid { to: p }, TurnMove::Rapid { to: q }) => close(p, q),
        (TurnMove::Line { to: p, feed: f }, TurnMove::Line { to: q, feed: g }) => {
            close(p, q) && near(f, g)
        }
        (
            TurnMove::Arc {
                to: p,
                center: c,
                cw,
                feed: f,
            },
            TurnMove::Arc {
                to: q,
                center: d,
                cw: dw,
                feed: g,
            },
        ) => close(p, q) && close(c, d) && cw == dw && near(f, g),
        (TurnMove::Thread { to: p, lead: f }, TurnMove::Thread { to: q, lead: g }) => {
            close(p, q) && near(f, g)
        }
        _ => false,
    }
}

#[test]
fn the_reference_program_posts_for_linuxcnc_longhand() {
    let program = reference();
    let dialect = turning_dialect("linuxcnc_turning").expect("the LinuxCNC turning dialect");
    assert_eq!(dialect.label(), "LinuxCNC (turning)");
    assert_eq!(dialect.extension(), "ngc");
    assert_eq!(dialect.control(), Control::LinuxCnc);
    let text = turning_post(&program, &LinuxCncTurning, &PostSettings::default()).unwrap();
    let expected = "\
%
(WARNING: MITCAD CAM IS HIGHLY EXPERIMENTAL.)
(TREAT THIS MACHINE CODE WITH CAUTION. CHECK EVERY TOOLPATH)
(IN A SIMULATOR, THEN RUN IT FIRST WITHOUT STOCK, AT A LOW)
(FEED, WITH A HAND ON THE EMERGENCY STOP, BEFORE CUTTING.)
(REFERENCE)
(T1 TURNING TOOL)
(T3 H13 THREADING TOOL)
G21 G18 G7 G90 G91.1 G40 G80 G95
G64 P0.01
G54
(FACE)
G0 X100. Z100.
T1 M6 G43
G96 D3000 S200 M3
M8
G0 X32. Z2.
Z0.
G1 X0. F0.15
G0 Z2.
X32.
(TURN)
X24.
G1 Z-20. F0.25
X28.
G0 Z2.
(FINISH)
G96 D3500 S250 M3
X0.
G1 Z0. F0.1
X14.
G3 X16. Z-1. I0. K-1.
G1 Z-20.
X32.
G0 Z2.
(THREAD)
M9
G0 X100. Z100.
T3 M6 G43 H13
G97 S796 M3
M8
G0 X18. Z4.
X15.
G33 Z-15. K1.5
G0 X18.
Z4.
X14.16
G33 Z-15. K1.5
G0 X18.
Z4.
M9
X100. Z100.
M5
M2
%
";
    assert_eq!(text, expected);

    // Read back as LinuxCNC reads it: the moves are the program's, the
    // change position's rapids around the tool changes, X a diameter.
    let read = check_program_as(&text, &MillTurn::default(), Control::LinuxCnc).unwrap();
    assert!(read.cycles.is_empty());
    assert!(read.diameter_mode);
    assert_eq!(read.css_lines, vec![15, 28]);
    let change = rapid(50.0, 100.0);
    let ops = &program.operations;
    let mut wanted = vec![change];
    wanted.extend(ops[0].path.moves.iter().copied());
    wanted.extend(ops[1].path.moves.iter().copied());
    wanted.extend(ops[2].path.moves.iter().copied());
    wanted.push(change);
    wanted.extend(ops[3].path.moves.iter().copied());
    wanted.push(change);
    assert_eq!(read.moves.len(), wanted.len(), "{:?}", read.moves);
    for (got, want) in read.moves.iter().zip(&wanted) {
        assert!(
            same(&got.motion, want),
            "line {}: {:?} against {want:?}",
            got.line,
            got.motion
        );
    }
    // A Fanuc control's reading refuses it: G7 is not one of its codes.
    let fanuc = check_program(&text).unwrap_err();
    assert_eq!(fanuc.message, "Line 9: G7 is not a command it reads.");
}

#[test]
fn inches_line_numbers_and_a_remapped_tool_change() {
    let program = reference();
    let settings = PostSettings {
        units: Units::Inches,
        line_numbers: true,
        tool_length_offset: Some(false),
        ..PostSettings::default()
    };
    let text = turning_post(&program, &LinuxCncTurning, &settings).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "%");
    assert!(
        lines.contains(&"N10 G20 G18 G7 G90 G91.1 G40 G80 G95"),
        "{text}"
    );
    // The blend within 0.01 mm, in inches.
    assert!(lines.contains(&"N20 G64 P0.0004"), "{text}");
    // Only T M6: the remapped M6 applies the offset itself.
    assert!(lines.iter().any(|l| l.ends_with(" T1 M6")), "{text}");
    assert!(lines.iter().any(|l| l.ends_with(" T3 M6")), "{text}");
    assert!(!text.contains("G43"), "{text}");
    // 200 m/min is 656 ft/min; the limit stays in rpm.
    assert!(
        lines.iter().any(|l| l.ends_with(" G96 D3000 S656 M3")),
        "{text}"
    );
    // The lead in inches per revolution.
    assert!(
        lines.iter().any(|l| l.ends_with(" G33 Z-0.5906 K0.059055")),
        "{text}"
    );
    let read = check_program_as(&text, &MillTurn::default(), Control::LinuxCnc).unwrap();
    let first_thread = read
        .moves
        .iter()
        .find_map(|m| match m.motion {
            TurnMove::Thread { to, lead } => Some((to, lead)),
            _ => None,
        })
        .unwrap();
    assert!((first_thread.0[0] - 7.5).abs() < 2e-3, "{first_thread:?}");
    assert!((first_thread.0[1] + 15.0).abs() < 2e-3, "{first_thread:?}");
    assert!((first_thread.1 - 1.5).abs() < 1e-4, "{first_thread:?}");
}

/// Canned cycles asked for: LinuxCNC's are not written; the generated
/// facing, roughing and finishing go out as their moves, and the program
/// says so.
#[test]
fn canned_cycles_are_written_as_moves() {
    let part = TurnPart {
        outside: vec![PV::new(0.0, 0.0), PV::new(10.0, 0.0), PV::new(10.0, -20.0)],
        inside: vec![],
        stock_radius: 15.0,
        stock_inner_radius: 0.0,
        stock_face: 1.0,
    };
    let rougher = tool(1, 0, "Turning tool", TurningToolKind::Turning);
    let facing = face(
        &part,
        &FaceOp {
            step: 1.0,
            clearance: 2.0,
            feed: 0.2,
        },
        &rougher,
    )
    .unwrap();
    let roughing = rough(
        &part,
        &RoughOp {
            side: TurnSide::Outside,
            depth: 2.5,
            retract: 0.5,
            allowance_x: 0.2,
            allowance_z: 0.1,
            clearance: 2.0,
            feed: 0.25,
        },
        &rougher,
    )
    .unwrap();
    let finishing = finish(
        &part,
        &FinishOp {
            side: TurnSide::Outside,
            clearance: 2.0,
            feed: 0.1,
        },
        &rougher,
    )
    .unwrap();
    assert!(roughing.cycle.is_some() && finishing.cycle.is_some());
    let mut program = TurningProgram::new("Pin", [50.0, 100.0]);
    for (name, path) in [("Face", facing), ("Rough", roughing), ("Finish", finishing)] {
        let mut o = op(name, &rougher, css(200.0, 3000.0), vec![]);
        o.path = path;
        program.operations.push(o);
    }
    let settings = PostSettings {
        canned_cycles: true,
        ..PostSettings::default()
    };
    let text = turning_post(&program, &LinuxCncTurning, &settings).unwrap();
    assert!(
        text.contains("(TURNING CYCLES ARE WRITTEN AS MOVES FOR THIS CONTROL)\n"),
        "{text}"
    );
    for code in ["G70", "G71", "G76", "G99", "G50"] {
        assert!(!text.contains(code), "{code} in {text}");
    }
    let read = check_program_as(&text, &MillTurn::default(), Control::LinuxCnc).unwrap();
    assert!(read.cycles.is_empty());
    // Every move of the three toolpaths, and the two change rapids.
    let moves: usize = program.operations.iter().map(|o| o.path.moves.len()).sum();
    assert!(read.moves.len() <= moves + 2 && read.moves.len() > moves / 2);
    // The same settings on a Fanuc control write its cycles.
    let fanuc = turning_post(&program, &FanucTurning, &settings).unwrap();
    assert!(fanuc.contains("G71 ") && fanuc.contains("G70 "), "{fanuc}");
    assert!(!fanuc.contains("WRITTEN AS MOVES"), "{fanuc}");
}

/// A tapered thread's pass: K is the lead along the move, read back as
/// the lead along the longer axis.
#[test]
fn a_tapered_thread_pass_carries_its_lead_along_the_move() {
    let threading = tool(
        2,
        0,
        "Threading tool",
        TurningToolKind::Threading {
            angle: 60.0,
            inside: false,
        },
    );
    let mut program = TurningProgram::new("Taper", [50.0, 100.0]);
    program.operations.push(op(
        "Taper thread",
        &threading,
        TurnSpindle::Rpm { rpm: 500.0 },
        vec![
            rapid(9.0, 2.0),
            rapid(8.0, 2.0),
            thread_pass(7.0, -8.0, 1.5),
            rapid(9.0, -8.0),
            rapid(9.0, 2.0),
        ],
    ));
    let text = turning_post(&program, &LinuxCncTurning, &PostSettings::default()).unwrap();
    // 1.5 along Z over 10 mm, the move 10.0499 long.
    assert!(text.contains("\nG33 X14. Z-8. K1.5075\n"), "{text}");
    let read = check_program_as(&text, &MillTurn::default(), Control::LinuxCnc).unwrap();
    let lead = read
        .moves
        .iter()
        .find_map(|m| match m.motion {
            TurnMove::Thread { lead, .. } => Some(lead),
            _ => None,
        })
        .unwrap();
    assert!((lead - 1.5).abs() < 1e-4, "{lead}");
}

/// What LinuxCNC's dialect does not write is refused in words; mist
/// coolant is M7 there.
#[test]
fn what_it_does_not_write_is_refused() {
    let mut program = reference();
    program.operations[0].coolant = Coolant::Air;
    let e = turning_post(&program, &LinuxCncTurning, &PostSettings::default()).unwrap_err();
    assert_eq!(
        e.message,
        "LinuxCNC (turning) has flood and mist coolant here, no air blast."
    );

    let mut program = reference();
    program.operations[0].coolant = Coolant::Mist;
    let text = turning_post(&program, &LinuxCncTurning, &PostSettings::default()).unwrap();
    assert!(text.contains("G96 D3000 S200 M3\nM7\n"), "{text}");

    // The sub-spindle's hand-over.
    let mut program = reference();
    program.operations.truncate(2);
    program.operations[1].name = "Back face".into();
    program.operations[1].sub = true;
    program.hand_over = Some(HandOver {
        grip_z: -30.0,
        withdraw: 5.0,
        rpm: 500.0,
        feed: 1000.0,
        work_offset: 2,
        back_z: -20.0,
    });
    let e = turning_post(&program, &LinuxCncTurning, &PostSettings::default()).unwrap_err();
    assert_eq!(
        e.message,
        "Back face: LinuxCNC (turning) has no sub-spindle here."
    );

    // Probing the work zero.
    let settings = PostSettings {
        probe_work_zero: true,
        ..PostSettings::default()
    };
    let e = turning_post(&reference(), &LinuxCncTurning, &settings).unwrap_err();
    assert_eq!(
        e.message,
        "LinuxCNC (turning) has no probing of the work zero here."
    );
}

/// LinuxCNC's reading refuses what would cut other than meant there: a
/// Fanuc control's codes, X before diameter mode, a surface speed without
/// its limit, a thread without its K or with an F.
#[test]
fn the_linuxcnc_reading_refuses_fanuc_codes() {
    let refused = |text: &str| {
        check_program_as(text, &MillTurn::default(), Control::LinuxCnc)
            .unwrap_err()
            .message
    };
    assert_eq!(
        refused("O1000 (PIN)\nG21 G18 G7 G95\n"),
        "Line 1: an O word, which calls a subroutine on this control."
    );
    assert_eq!(
        refused("G21 G18 G90 G95\nG0 X20. Z2.\n"),
        "Line 2: a move in X before the program sets diameter mode (G7)."
    );
    assert_eq!(
        refused("G21 G18 G7 G8 G95\n"),
        "Line 1: radius mode (G8), but the program writes X as a diameter."
    );
    assert_eq!(
        refused("G21 G18 G7 G99\n"),
        "Line 1: G99 sets the cycles' return level on this control; feeds per minute and per revolution are G94 and G95."
    );
    assert_eq!(
        refused("G21 G18 G7 G95\nG50 S3000\n"),
        "Line 2: G50 is not a command it reads on this control."
    );
    assert_eq!(
        refused("G21 G18 G7 G95\nG96 S200 M3\n"),
        "Line 2: a constant surface speed without a spindle limit (D) in its block."
    );
    assert_eq!(
        refused("G21 G18 G7 G95\nG97 S500 M3 D3000\n"),
        "Line 2: D without G96 to use it."
    );
    let threading = "G21 G18 G7 G95\nG97 S500 M3\nG0 X20. Z2.\n";
    assert_eq!(
        refused(&format!("{threading}G33 Z-10.\n")),
        "Line 4: G33 without its lead (K)."
    );
    assert_eq!(
        refused(&format!("{threading}G33 Z-10. K1.5 F1.5\n")),
        "Line 4: F with G33, whose lead is its K word."
    );
    assert_eq!(
        refused(&format!("{threading}G32 Z-10. F1.5\n")),
        "Line 4: G32 is not a command it reads on this control."
    );
    assert_eq!(
        refused(&format!("{threading}G0 X20. C90.\n")),
        "Line 4: the C word, an axis the program does not drive on this control."
    );
    // A tool change stops the spindle there: a cut after it without
    // starting the spindle again.
    assert_eq!(
        refused("G21 G18 G7 G95\nG97 S500 M3\nG0 X20. Z2.\nT2 M6 G43\nG1 Z-10. F0.1\n"),
        "Line 5: a cut with the main spindle stopped before the feeds are per minute (G94)."
    );
    // The Fanuc reading of a Fanuc program is unchanged.
    let fanuc = "G21 G18 G40 G99\nG50 S3000\nG96 S200 M3\nG0 X20. Z2.\nG1 Z-10. F0.1\n";
    assert!(check_program(fanuc).is_ok());
    assert_eq!(
        check_program_as(fanuc, &MillTurn::default(), Control::LinuxCnc)
            .unwrap_err()
            .message,
        "Line 1: G99 sets the cycles' return level on this control; feeds per minute and per revolution are G94 and G95."
    );
}
