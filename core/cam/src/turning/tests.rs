// SPDX-License-Identifier: MIT
//! Turning from the part's profile to the Fanuc program: a pin in exact
//! G-code, and a shaft with shoulders, a groove, a thread and a bore
//! through every operation, both posts (cycles and moves), the program
//! check, the lathe check and the simulation of the turned stock.

use super::*;
use crate::machine::{Bounds, Machine, Spindle, ToolChanger};
use crate::post::PostSettings;

fn turning_tool(number: u32, name: &str, nose_radius: f64) -> TurningTool {
    TurningTool {
        number,
        offset: 0,
        name: name.into(),
        kind: TurningToolKind::Turning,
        nose_radius,
        max_depth: 4.0,
        back_angle: None,
    }
}

fn op(name: &str, tool: &TurningTool, spindle: TurnSpindle, path: TurnPath) -> TurningProgramOp {
    TurningProgramOp {
        name: name.into(),
        tool: tool.clone(),
        spindle,
        direction: SpindleDirection::Cw,
        coolant: Coolant::Flood,
        path,
        live: None,
        sub: false,
    }
}

fn css(surface_speed: f64, max_rpm: f64) -> TurnSpindle {
    TurnSpindle::Css {
        surface_speed,
        max_rpm,
    }
}

fn cycles() -> PostSettings {
    PostSettings {
        canned_cycles: true,
        ..PostSettings::default()
    }
}

/// A pin 20 mm in diameter and 20 long from a 30 mm bar with 1 mm to
/// face: facing, roughing as G71 and finishing as G70, sharp tools.
#[test]
fn a_pin_posts_as_fanuc_turning_cycles() {
    let part = TurnPart {
        outside: vec![PV::new(0.0, 0.0), PV::new(10.0, 0.0), PV::new(10.0, -20.0)],
        inside: vec![],
        stock_radius: 15.0,
        stock_inner_radius: 0.0,
        stock_face: 1.0,
    };
    let rougher = turning_tool(1, "Turning tool", 0.0);
    let finisher = turning_tool(2, "Finishing tool", 0.0);
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
            plunge: 0.0,
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
            plunge: 0.0,
            roughed: false,
        },
        &finisher,
    )
    .unwrap();
    // The roughing passes: one at 25 mm, then along the contour.
    let near = |m: &TurnMove, kind: &str, to: PXZ| {
        let k = match m {
            TurnMove::Rapid { .. } => "rapid",
            TurnMove::Line { .. } => "line",
            _ => "other",
        };
        k == kind && (m.to()[0] - to[0]).abs() < 1e-9 && (m.to()[1] - to[1]).abs() < 1e-9
    };
    let passes = [
        ("rapid", [17.0, 3.0]),
        ("rapid", [12.5, 3.0]),
        ("line", [12.5, -19.9]),
        ("line", [13.0, -19.4]),
        ("rapid", [13.0, 3.0]),
        ("rapid", [10.2, 3.0]),
        ("line", [10.2, 0.1]),
        ("line", [10.2, -19.9]),
        ("line", [17.2, -19.9]),
        ("rapid", [17.2, 3.0]),
        ("rapid", [17.0, 3.0]),
    ];
    assert_eq!(roughing.moves.len(), passes.len(), "{:?}", roughing.moves);
    for (m, (kind, to)) in roughing.moves.iter().zip(passes) {
        assert!(near(m, kind, to), "{m:?} against {kind} to {to:?}");
    }
    let mut program = TurningProgram::new("Pin", [50.0, 100.0]);
    program.operations = vec![
        op("Face", &rougher, css(200.0, 3000.0), facing),
        op("Rough", &rougher, css(200.0, 3000.0), roughing),
        op("Finish", &finisher, css(250.0, 3500.0), finishing),
    ];
    let text = turning_post(&program, &FanucTurning, &cycles()).unwrap();
    let expected = "\
%
O1000 (PIN)
(WARNING: MITCAD CAM IS HIGHLY EXPERIMENTAL.)
(TREAT THIS MACHINE CODE WITH CAUTION. CHECK EVERY TOOLPATH)
(IN A SIMULATOR, THEN RUN IT FIRST WITHOUT STOCK, AT A LOW)
(FEED, WITH A HAND ON THE EMERGENCY STOP, BEFORE CUTTING.)
(PIN)
(T0101 TURNING TOOL)
(T0202 FINISHING TOOL)
G21 G18 G40 G99
G54
(FACE)
G0 X100. Z100.
T0101
G50 S3000
G96 S200 M3
M8
G0 X34. Z3.
Z0.
G1 X0. F0.2
G0 Z2.
X34.
Z3.
(ROUGH)
G71 U2.5 R0.5
G71 P1000 Q1010 U0.4 W0.1 F0.25
N1000 G0 X20.
G1 Z0. F0.1
Z-20.
N1010 X34.
(FINISH)
M9
G0 X100. Z100.
T0202
G50 S3500
G96 S250 M3
M8
G0 X34. Z3.
G70 P1000 Q1010
M9
G0 X100. Z100.
M5
M30
%
";
    assert_eq!(text, expected);
    let read = check_program(&text).unwrap();
    assert_eq!(
        read.cycles,
        vec![("G71".to_owned(), 26), ("G70".to_owned(), 39)]
    );
    assert_eq!(read.css_lines, vec![16, 36]);
    // Without cycles, the moves: X is the diameter.
    let plain = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap();
    assert!(!plain.contains("G71") && !plain.contains("G70"));
    assert!(
        plain.contains("\nX25.\nG1 Z-19.9 F0.25\nX26. Z-19.4\nG0 Z3.\n"),
        "{plain}"
    );
    let read = check_program(&plain).unwrap();
    assert!(read.cycles.is_empty());
}

fn shaft() -> TurnPart {
    // From the face: a 1 mm chamfer, the 20 mm thread diameter with a
    // 17 mm relief groove behind it, a 28 mm shoulder with a chamfer, a
    // 30 mm diameter ending in a 1 mm fillet against a 35 mm shoulder.
    // A 12 mm bore 20 deep with a chamfer at its mouth, in a tube of
    // 8 mm bore; a 40 mm bar with 1 mm to face.
    let fillet = (-std::f64::consts::FRAC_PI_8).tan();
    TurnPart {
        outside: vec![
            PV::new(0.0, 0.0),
            PV::new(9.0, 0.0),
            PV::new(10.0, -1.0),
            PV::new(10.0, -14.0),
            PV::new(8.5, -14.0),
            PV::new(8.5, -17.0),
            PV::new(10.0, -17.0),
            PV::new(10.0, -20.0),
            PV::new(14.0, -20.0),
            PV::new(15.0, -21.0),
            PV::arc(15.0, -34.0, fillet),
            PV::new(16.0, -35.0),
            PV::new(17.5, -35.0),
            PV::new(17.5, -50.0),
        ],
        inside: vec![
            PV::new(7.0, 0.0),
            PV::new(6.0, -1.0),
            PV::new(6.0, -20.0),
            PV::new(4.0, -20.0),
            PV::new(4.0, -50.0),
        ],
        stock_radius: 20.0,
        stock_inner_radius: 4.0,
        stock_face: 1.0,
    }
}

fn shaft_program(part: &TurnPart) -> TurningProgram {
    let rougher = turning_tool(1, "Roughing", 0.4);
    let finisher = turning_tool(2, "Finishing", 0.4);
    let bar = TurningTool {
        number: 3,
        offset: 0,
        name: "Boring bar".into(),
        kind: TurningToolKind::Boring { bar_diameter: 6.0 },
        nose_radius: 0.4,
        max_depth: 2.0,
        back_angle: None,
    };
    let grooving = TurningTool {
        number: 4,
        offset: 0,
        name: "Grooving 2 mm".into(),
        kind: TurningToolKind::Grooving {
            width: 2.0,
            inside: false,
        },
        nose_radius: 0.1,
        max_depth: 3.0,
        back_angle: None,
    };
    let threading = TurningTool {
        number: 5,
        offset: 0,
        name: "Thread 60".into(),
        kind: TurningToolKind::Threading {
            angle: 60.0,
            inside: false,
        },
        nose_radius: 0.0,
        max_depth: 2.0,
        back_angle: None,
    };
    let parting = TurningTool {
        number: 6,
        offset: 0,
        name: "Parting 3 mm".into(),
        kind: TurningToolKind::Parting { width: 3.0 },
        nose_radius: 0.0,
        max_depth: 22.0,
        back_angle: None,
    };
    let rough_op = |side| RoughOp {
        side,
        depth: 1.5,
        retract: 0.5,
        allowance_x: 0.2,
        allowance_z: 0.1,
        clearance: 1.0,
        feed: 0.25,
        plunge: 0.0,
    };
    let finish_op = |side| FinishOp {
        side,
        clearance: 1.0,
        feed: 0.1,
        plunge: 0.0,
        roughed: false,
    };
    let mut program = TurningProgram::new("Shaft", [100.0, 150.0]);
    program.operations = vec![
        op(
            "Face",
            &rougher,
            css(200.0, 3000.0),
            face(
                part,
                &FaceOp {
                    step: 2.0,
                    clearance: 2.0,
                    feed: 0.2,
                },
                &rougher,
            )
            .unwrap(),
        ),
        op(
            "Rough outside",
            &rougher,
            css(200.0, 3000.0),
            rough(part, &rough_op(TurnSide::Outside), &rougher).unwrap(),
        ),
        op(
            "Rough bore",
            &bar,
            css(150.0, 2500.0),
            rough(part, &rough_op(TurnSide::Inside), &bar).unwrap(),
        ),
        op(
            "Finish outside",
            &finisher,
            css(250.0, 3500.0),
            finish(part, &finish_op(TurnSide::Outside), &finisher).unwrap(),
        ),
        op(
            "Finish bore",
            &bar,
            css(150.0, 2500.0),
            finish(part, &finish_op(TurnSide::Inside), &bar).unwrap(),
        ),
        op(
            "Relief groove",
            &grooving,
            css(100.0, 1500.0),
            groove(
                part,
                &GrooveOp {
                    side: TurnSide::Outside,
                    z_from: -14.0,
                    z_to: -17.0,
                    peck: 0.0,
                    clearance: 1.0,
                    feed: 0.05,
                },
                &grooving,
            )
            .unwrap(),
        ),
        op(
            "Thread M20",
            &threading,
            TurnSpindle::Rpm { rpm: 600.0 },
            thread(
                &ThreadOp {
                    side: TurnSide::Outside,
                    crest: 10.0,
                    z_start: -1.0,
                    z_end: -13.0,
                    lead: 1.5,
                    height: ops::external_thread_height(1.5),
                    first_depth: 0.35,
                    min_depth: 0.05,
                    finish_allowance: 0.05,
                    spring_passes: 1,
                    run_in: 3.0,
                    clearance: 1.0,
                },
                &threading,
            )
            .unwrap(),
        ),
        op(
            "Part off",
            &parting,
            css(80.0, 1200.0),
            part_off(
                part,
                &PartOp {
                    z: None,
                    to_x: 0.0,
                    peck: 2.0,
                    clearance: 2.0,
                    feed: 0.05,
                },
                &parting,
            )
            .unwrap(),
        ),
    ];
    program
}

fn lathe() -> Machine {
    Machine {
        name: "Lathe".into(),
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
            turret_stations: 12,
            chuck_diameter: 160.0,
            chuck_length: 50.0,
            axes: LatheAxes::default(),
            mill_turn: MillTurn::default(),
            sub_spindle: SubSpindle::default(),
            turret_body: None,
            station_bodies: Vec::new(),
        }),
    }
}

/// What the plain post writes as moves: the change position at each new
/// tool and at the end, the moves that move at the program's digits.
fn written(program: &TurningProgram) -> Vec<TurnMove> {
    let key = |p: PXZ| {
        (
            (2.0 * p[0] * 1000.0).round() as i64,
            (p[1] * 1000.0).round() as i64,
        )
    };
    let change = TurnMove::Rapid {
        to: program.change_position,
    };
    let mut out = Vec::new();
    let mut at: Option<(i64, i64)> = None;
    let mut tool = None;
    let mut push = |m: TurnMove, at: &mut Option<(i64, i64)>| {
        let k = key(m.to());
        if *at != Some(k) {
            out.push(m);
        }
        *at = Some(k);
    };
    for op in &program.operations {
        if tool != Some(op.tool.number) {
            // The post forgets where the tool is before the change.
            at = None;
            push(change, &mut at);
            at = None;
            tool = Some(op.tool.number);
        }
        for m in &op.path.moves {
            push(*m, &mut at);
        }
    }
    push(change, &mut at);
    out
}

#[test]
fn a_shaft_with_a_groove_a_thread_and_a_bore_turns_and_checks() {
    let part = shaft();
    let program = shaft_program(&part);
    program.validate().unwrap();

    // With cycles: G71 and G70 outside and in the bore, G76 for the
    // thread, the spindle at a constant surface speed under its limit and
    // at a constant speed for the thread.
    let text = turning_post(&program, &FanucTurning, &cycles()).unwrap();
    assert_eq!(text.matches("G71 U1.5 R0.5\n").count(), 2, "{text}");
    assert!(text.contains(" U0.4 W0.1 F0.25\n"), "{text}");
    assert!(text.contains(" U-0.4 W0.1 F0.25\n"), "{text}");
    assert_eq!(text.matches("G70 P").count(), 2, "{text}");
    assert!(
        text.contains("G76 P010060 Q50 R0.05\nG76 X18.16 Z-13. P920 Q350 F1.5\n"),
        "{text}"
    );
    assert!(text.contains("T0101\nG50 S3000\nG96 S200 M3\n"), "{text}");
    assert!(text.contains("T0505\nG97 S600 M3\n"), "{text}");
    assert!(text.contains("T0606\nG50 S1200\nG96 S80 M3\n"), "{text}");
    // No length without its decimal point.
    for line in text.lines().filter(|l| !l.starts_with('(')) {
        for w in line.split_whitespace() {
            if w.starts_with(['X', 'Z', 'I', 'K']) {
                assert!(w.contains('.'), "{line}");
            }
        }
    }
    let read = check_program(&text).unwrap();
    let kinds: Vec<&str> = read.cycles.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(kinds, vec!["G71", "G71", "G70", "G70", "G76"]);
    assert_eq!(read.css_lines.len(), 6);
    // Numbered lines: the cycles refer to the line numbers.
    let numbered = turning_post(
        &program,
        &FanucTurning,
        &PostSettings {
            line_numbers: true,
            ..cycles()
        },
    )
    .unwrap();
    assert_eq!(check_program(&numbered).unwrap().cycles.len(), 5);

    // Without cycles: the moves, read back the same within the program's
    // digits; X is written as the diameter.
    let plain = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap();
    assert!(plain.contains("G32 "), "{plain}");
    let read = check_program(&plain).unwrap();
    assert!(read.diameter_mode && read.cycles.is_empty());
    let expected = written(&program);
    assert_eq!(read.moves.len(), expected.len());
    for (r, e) in read.moves.iter().zip(&expected) {
        let (a, b) = (r.motion.to(), e.to());
        assert!(
            (a[0] - b[0]).abs() < 6e-4 && (a[1] - b[1]).abs() < 6e-4,
            "line {}: {:?} against {:?}",
            r.line,
            r.motion,
            e
        );
        assert_eq!(
            std::mem::discriminant(&r.motion),
            std::mem::discriminant(e),
            "line {}",
            r.line
        );
        if let (Some(f), Some(g)) = (r.motion.feed(), e.feed()) {
            assert!((f - g).abs() < 1e-6, "line {}", r.line);
        }
        if let (
            TurnMove::Arc { center: c, cw, .. },
            TurnMove::Arc {
                center: d, cw: dw, ..
            },
        ) = (r.motion, *e)
        {
            assert!((c[0] - d[0]).abs() < 1.1e-3 && (c[1] - d[1]).abs() < 1.1e-3);
            assert_eq!(cw, dw);
        }
    }

    // The lathe: everything within its limits, clear of the chuck.
    let placement = check::TurnPlacement {
        work_z: 0.0,
        chuck_z: Some(-60.0),
    };
    let findings = check_lathe(&lathe(), &program, &placement);
    assert!(findings.is_empty(), "{findings:?}");
    // A chuck closer to the part is hit by the parting blade.
    let near = check::TurnPlacement {
        work_z: 0.0,
        chuck_z: Some(-52.0),
    };
    let findings = check_lathe(&lathe(), &program, &near);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(
        findings[0].message,
        "Part off: the tool runs into the chuck."
    );
    assert_eq!(findings[0].operation, 7);

    // The turned stock: no rapid into it, no cut into the part, and no
    // more left than the nose radius leaves in the inner corners.
    let sim = simulate_turning(&part, &program, -60.0, 0.05, 0.01);
    assert!(sim.rapid_cuts.is_empty(), "{:?}", sim.rapid_cuts);
    assert!(sim.gouges.is_empty(), "{:?}", sim.gouges);
    assert!(sim.left < 0.41, "{} at {:?}", sim.left, sim.left_at);
    // Parted off: nothing left between the part and the blade's far side.
    let i = sim
        .stock
        .z
        .iter()
        .position(|z| (*z + 51.5).abs() < 0.03)
        .unwrap();
    assert!(sim.stock.outer[i] <= sim.stock.inner[i] + 1e-9);
}

/// A bore roughed with no stock to leave (mitcad#2527): G71's allowance
/// is `U0.`, and the read-back takes the side from the contour against
/// where the cycle starts, as the post does, not from U's sign.
#[test]
fn a_bore_roughed_with_no_stock_to_leave_posts_as_a_cycle_and_reads_back() {
    let part = shaft();
    let bar = TurningTool {
        number: 3,
        offset: 0,
        name: "Boring bar".into(),
        kind: TurningToolKind::Boring { bar_diameter: 6.0 },
        nose_radius: 0.4,
        max_depth: 2.0,
        back_angle: None,
    };
    let roughing = rough(
        &part,
        &RoughOp {
            side: TurnSide::Inside,
            depth: 1.5,
            retract: 0.5,
            allowance_x: 0.0,
            allowance_z: 0.0,
            clearance: 1.0,
            feed: 0.25,
            plunge: 0.0,
        },
        &bar,
    )
    .unwrap();
    let mut program = TurningProgram::new("Bore", [100.0, 150.0]);
    program.operations = vec![op("Rough bore", &bar, css(150.0, 2500.0), roughing)];
    program.validate().unwrap();
    let text = turning_post(&program, &FanucTurning, &cycles()).unwrap();
    assert!(text.contains("G71 U1.5 R0.5\n"), "{text}");
    assert!(text.contains(" U0. W0. F0.25\n"), "{text}");
    let read = check_program(&text).unwrap();
    let kinds: Vec<&str> = read.cycles.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(kinds, vec!["G71"]);
    // An allowance on the other side of the contour is refused, and a
    // cycle that starts where the tool is not known.
    let base = "%\nO1000\nG21 G18 G40 G99\nG54\n";
    let refuse = |body: &str| {
        check_program(&format!("{base}{body}%\n"))
            .unwrap_err()
            .message
    };
    // The contour from X 10 (a diameter): outside from X 20, inside from
    // X 4.
    let cycle = |start: &str, u: &str| {
        format!("{start}G71 U1. R0.5\nG71 P10 Q20 U{u} W0. F0.2\nN10 G0 X10.\nN20 G1 Z-5. F0.1\n")
    };
    for (start, u) in [("20.", "0."), ("4.", "0."), ("20.", "0.4"), ("4.", "-0.4")] {
        let text = format!("{base}{}%\n", cycle(&format!("G0 X{start} Z2.\n"), u));
        assert!(check_program(&text).is_ok(), "{text}");
    }
    let other_side = "Line 7: G71's allowance (U) is on the other side of its contour.";
    assert_eq!(refuse(&cycle("G0 X20. Z2.\n", "-0.4")), other_side);
    assert_eq!(refuse(&cycle("G0 X4. Z2.\n", "0.4")), other_side);
    assert_eq!(
        refuse(&cycle("", "0.")),
        "Line 6: G71 from where the tool is not known."
    );
}

#[test]
fn threading_writes_its_passes_deeper_and_shallower() {
    let tool = TurningTool {
        number: 5,
        offset: 0,
        name: "Thread".into(),
        kind: TurningToolKind::Threading {
            angle: 60.0,
            inside: false,
        },
        nose_radius: 0.0,
        max_depth: 2.0,
        back_angle: None,
    };
    let op = ThreadOp {
        side: TurnSide::Outside,
        crest: 10.0,
        z_start: 0.0,
        z_end: -10.0,
        lead: 1.0,
        height: 0.6,
        first_depth: 0.2,
        min_depth: 0.05,
        finish_allowance: 0.05,
        spring_passes: 2,
        run_in: 2.0,
        clearance: 1.0,
    };
    let depths = op.depths();
    let rounded: Vec<f64> = depths.iter().map(|d| (d * 1e6).round() / 1e6).collect();
    // 0.2 times the square root of the pass, at least 0.05 more each
    // time, up to 0.55; then twice the full depth.
    assert_eq!(
        rounded,
        vec![0.2, 0.282843, 0.34641, 0.4, 0.45, 0.5, 0.55, 0.6, 0.6]
    );
    let path = thread(&op, &tool).unwrap();
    assert_eq!(path.moves[0], TurnMove::Rapid { to: [11.0, 2.0] });
    let TurnMove::Thread { to, lead } = path.moves[2] else {
        panic!("{:?}", path.moves[2]);
    };
    assert!((to[0] - 9.8).abs() < 1e-9 && to[1] == -10.0 && lead == 1.0);
    // A boring bar cannot thread.
    let bar = TurningTool {
        kind: TurningToolKind::Boring { bar_diameter: 6.0 },
        ..tool
    };
    assert_eq!(
        thread(&op, &bar).unwrap_err().message,
        "Threading needs a threading tool."
    );
}

#[test]
fn the_program_check_refuses_what_a_lathe_would_misread() {
    let base = "%\nO1000\nG21 G18 G40 G99\nG54\n";
    let refuse = |body: &str| {
        check_program(&format!("{base}{body}%\n"))
            .unwrap_err()
            .message
    };
    assert_eq!(
        refuse("G0 X20 Z2.\n"),
        "Line 5: X needs a decimal point, or the control reads it in its smallest steps."
    );
    assert_eq!(
        refuse("G96 S200 M3\n"),
        "Line 5: a constant surface speed without a spindle limit (G50 S) before it."
    );
    assert_eq!(
        refuse("G97 S500 M3\nG0 X20. Z2.\nG98\nG1 Z-5. F100.\n"),
        "Line 8: a feed before the feeds are per revolution (G99)."
    );
    assert_eq!(
        refuse("G0 X20. Z2.\nG71 U1. R0.5\nG71 P10 Q20 U0.4 W0.1 F0.2\n"),
        "Line 7: the cycle's blocks are not in the program."
    );
    assert_eq!(
        refuse(
            "G0 X20. Z2.\nG71 U1. R0.5\nG71 P10 Q20 U0.4 W0.1 F0.2\nN10 G0 X10. Z0.\nN20 G1 X20. F0.1\n"
        ),
        "Line 7: the first block of the contour must move in X alone."
    );
}

#[test]
fn a_part_on_the_profile_validates_in_words() {
    let mut part = shaft();
    part.stock_radius = 15.0;
    assert_eq!(
        part.validate().unwrap_err(),
        "The part is wider than its stock."
    );
    let mut part = shaft();
    part.stock_inner_radius = 0.0;
    let tool = TurningTool {
        number: 3,
        offset: 0,
        name: "Bar".into(),
        kind: TurningToolKind::Boring { bar_diameter: 6.0 },
        nose_radius: 0.4,
        max_depth: 2.0,
        back_angle: None,
    };
    let e = rough(
        &part,
        &RoughOp {
            side: TurnSide::Inside,
            depth: 1.0,
            retract: 0.5,
            allowance_x: 0.0,
            allowance_z: 0.0,
            clearance: 1.0,
            feed: 0.2,
            plunge: 0.0,
        },
        &tool,
    )
    .unwrap_err();
    assert_eq!(
        e.message,
        "The stock has no hole for the boring bar: drill it first or give the stock a bore."
    );
}
