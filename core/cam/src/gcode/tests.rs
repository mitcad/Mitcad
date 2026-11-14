// SPDX-License-Identifier: MIT
//! The G-code reader: arcs in every plane, inches and incremental moves,
//! work and machine coordinates, canned cycles, what it reads past and
//! what it refuses; and every dialect's posted program read back as the
//! program's own moves.

use std::collections::BTreeMap;

use super::*;
use crate::machine::{Bounds, Machine, MacroChanger, RackSlot, Spindle, ToolChanger};
use crate::post::{PostSettings, Units, dialects, post_with_lines};
use crate::program::{Coolant, Program, ProgramOp, SpindleDirection};
use crate::tool::Tool;
use crate::toolpath::{Move, Toolpath};

/// Work coordinates equal to machine coordinates, no tool lengths.
fn plain() -> GcodeReader {
    GcodeReader::new(
        [Some([0.0; 3]), None, None, None, None, None],
        BTreeMap::new(),
    )
}

fn near3(a: P3, b: P3, tol: f64) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= tol)
}

fn end(m: &GMove) -> P3 {
    known(m.to).expect("a known end")
}

#[test]
fn arcs_are_read_by_center_and_by_radius_in_every_plane() {
    let text = "\
G21 G90 G17 G54
G0 X10 Y0 Z0
G3 X0 Y10 I-10 J0 F600
G2 X10 Y0 R10
G2 X10 Y0 Z-2 I-10 J0
G18 G3 X0 Z-12 I-10 K0
G17 G3 X-10 Y0 R-10
";
    let moves = plain().read(text).unwrap();
    assert_eq!(moves.len(), 6, "{moves:?}");
    // A quarter turn about the origin, counter-clockwise.
    assert_eq!(
        moves[1].motion,
        Motion::Arc {
            center: [0.0, 0.0, 0.0],
            ccw: true,
            plane: Plane::Xy
        }
    );
    assert_eq!(moves[1].feed, Some(600.0));
    assert_eq!(moves[1].line, 3);
    // Back by its radius, clockwise: the same center.
    let Motion::Arc { center, ccw, .. } = moves[2].motion else {
        panic!("{:?}", moves[2]);
    };
    assert!(near3(center, [0.0, 0.0, 0.0], 1e-9) && !ccw, "{center:?}");
    // A whole helical turn down 2 mm: through the far side halfway down.
    let pts = moves[3].points(0.001);
    assert_eq!(*pts.last().unwrap(), [10.0, 0.0, -2.0]);
    assert!(
        pts.iter().any(|p| near3(*p, [-10.0, 0.0, -1.0], 0.2)),
        "{pts:?}"
    );
    assert!(pts.iter().all(|p| (p[0].hypot(p[1]) - 10.0).abs() < 1e-9));
    // In the ZX plane, counter-clockwise from +X towards -Z: a quarter
    // turn about (0, 0, -2) that stays on the +X, -Z side.
    assert_eq!(
        moves[4].motion,
        Motion::Arc {
            center: [0.0, 0.0, -2.0],
            ccw: true,
            plane: Plane::Zx
        }
    );
    let pts = moves[4].points(0.001);
    assert!(pts.len() > 2);
    assert!(
        pts.iter()
            .all(|p| p[0] >= -1e-9 && p[2] <= -2.0 + 1e-9 && p[1] == 0.0),
        "{pts:?}"
    );
    assert!(near3(end(&moves[4]), [0.0, 0.0, -12.0], 1e-9));
    // A negative radius takes the longer way: from (0, 0, -12) in XY to
    // (-10, 0) counter-clockwise, more than half a turn.
    let Motion::Arc { center, .. } = moves[5].motion else {
        panic!("{:?}", moves[5]);
    };
    let sweep = arc_sweep([0.0, 0.0], [-10.0, 0.0], [center[0], center[1]], true);
    assert!(sweep > std::f64::consts::PI, "{center:?} {sweep}");
}

#[test]
fn inches_incremental_moves_and_the_tool_length_are_converted_to_the_gauge_line() {
    // G54's zero at (100, 100, -100); tool 1 is 20 mm long.
    let mut reader = GcodeReader::new(
        [Some([100.0, 100.0, -100.0]), None, None, None, None, None],
        BTreeMap::from([(1, 20.0)]),
    );
    reader.set_tool(Some(1));
    let text = "\
G20 G90 G54
G0 X1 Y2 Z0.5
G1 Z-0.1 F10
G91 G1 X0.5
G90 G21 G0 Z5
G53 G0 Z-5
";
    let moves = reader.read(text).unwrap();
    assert_eq!(moves.len(), 5, "{moves:?}");
    assert!(near3(end(&moves[0]), [125.4, 150.8, -67.3], 1e-9));
    // Inches per minute become millimetres per minute.
    assert!((moves[1].feed.unwrap() - 254.0).abs() < 1e-9);
    assert!(near3(end(&moves[1]), [125.4, 150.8, -82.54], 1e-9));
    assert!(near3(end(&moves[2]), [138.1, 150.8, -82.54], 1e-9));
    assert!(near3(end(&moves[3]), [138.1, 150.8, -75.0], 1e-9));
    // Machine coordinates place the gauge line itself.
    assert!(moves[4].machine && !moves[3].machine);
    assert!(near3(end(&moves[4]), [138.1, 150.8, -5.0], 1e-9));
    assert!(!reader.inches());
}

#[test]
fn a_tool_change_forgets_the_position_and_brings_its_length() {
    let mut reader = GcodeReader::new(
        [Some([0.0; 3]), None, None, None, None, None],
        BTreeMap::from([(1, 20.0), (2, 30.0)]),
    );
    let moves = reader
        .read("G0 X1 Y2 Z3\nT2 M6\nG0 Z10\nG0 X5 Y6\n")
        .unwrap();
    assert_eq!(reader.tool(), Some(2));
    // After the change only Z is known again, the tool's 30 mm above.
    assert_eq!(moves[1].to, [None, None, Some(40.0)]);
    assert_eq!(moves[1].ends(), None);
    assert_eq!(moves[2].to, [Some(5.0), Some(6.0), Some(40.0)]);
    // T alone changes the tool where the controller says so.
    let mut reader = plain();
    reader.select_on_t = true;
    reader.read("G0 X1 Y2 Z3\nT4\n").unwrap();
    assert_eq!((reader.tool(), reader.at()), (Some(4), [None; 3]));
}

#[test]
fn comments_line_numbers_quoted_texts_and_checksums_are_read_past() {
    let text = "\
%
N10 (Bracket) G0 X1 ; the rest
N20 M291 P\"Change to (tool) 2\" R\"Tool change\" S2
N30 M4000 P1 R3 S\"6 mm \"\"end\"\" mill\"
N40 G1 X2 F100*57
G43.1 Z25
G4 P2.5
G28
G0 X3
";
    let moves = plain().read(text).unwrap();
    assert_eq!(
        moves.iter().map(|m| m.line).collect::<Vec<_>>(),
        vec![2, 5, 9]
    );
    assert_eq!(moves[1].to[0], Some(2.0));
    // After homing only the axis named again is known.
    assert_eq!(moves[2].to, [Some(3.0), None, None]);
}

#[test]
fn what_the_reader_does_not_follow_is_refused_with_its_line() {
    for (text, line, message) in [
        (
            "G0 X0\nG10 L2 P1 X0\n",
            2,
            "A line sets a work offset (G10 or G92), which the reader does not follow.",
        ),
        // A changer's line whose value was not filled in.
        ("G0 X{x}\n", 1, "A word of a line has no number."),
        (
            "G0 X1 = 2\n",
            1,
            "A line has a character the reader does not know.",
        ),
        (
            "G65 P9000\n",
            1,
            "A line has a command the reader does not know.",
        ),
        (
            "#1 = 5\n",
            1,
            "A line uses parameters or expressions, which the reader does not follow.",
        ),
        (
            "G0 X0 Y0 Z0\nG91 G53 G0 Z5\n",
            2,
            "A move in machine coordinates (G53) must be absolute.",
        ),
        ("G0 X0 Y0 Z0\nG1 X5\n", 2, "A cut has no feed."),
        (
            "G0 X0 Y0 Z0\nG1 X10 F100\nG3 X0 Y5 I-5 J0\n",
            3,
            "An arc's end is not on its circle.",
        ),
        (
            "G0 X0 A10\n",
            1,
            "A line moves an axis other than X, Y and Z, which the reader does not follow.",
        ),
        (
            "G59 G0 X1\n",
            1,
            "A move is in a work offset whose place on the machine is not known.",
        ),
        ("(open\n", 1, "A comment is not closed."),
    ] {
        let e = plain().read(text).unwrap_err();
        assert_eq!((e.line, e.message.as_str()), (line, message), "{text}");
    }
}

#[test]
fn a_canned_cycle_is_read_as_the_moves_of_each_hole() {
    let text = "\
G90 G0 X0 Y0 Z10
G98 G83 X5 Y5 Z-6 R2 Q2 F200
X15
G80
G99 G81 X20 Y5 Z-3 R1
G0 Z15
";
    let moves = plain().read(text).unwrap();
    let ends: Vec<(usize, Motion, P3)> = moves.iter().map(|m| (m.line, m.motion, end(m))).collect();
    use Motion::{Feed, Rapid};
    assert_eq!(
        ends,
        vec![
            (1, Rapid, [0.0, 0.0, 10.0]),
            (2, Rapid, [5.0, 5.0, 10.0]),
            (2, Rapid, [5.0, 5.0, 2.0]),
            (2, Feed, [5.0, 5.0, -6.0]),
            (2, Rapid, [5.0, 5.0, 10.0]),
            (3, Rapid, [15.0, 5.0, 10.0]),
            (3, Rapid, [15.0, 5.0, 2.0]),
            (3, Feed, [15.0, 5.0, -6.0]),
            (3, Rapid, [15.0, 5.0, 10.0]),
            // G99 goes back to the retract height only.
            (5, Rapid, [20.0, 5.0, 10.0]),
            (5, Rapid, [20.0, 5.0, 1.0]),
            (5, Feed, [20.0, 5.0, -3.0]),
            (5, Rapid, [20.0, 5.0, 1.0]),
            (6, Rapid, [20.0, 5.0, 15.0]),
        ]
    );
    assert_eq!(moves[3].feed, Some(200.0));
}

// The round trip.

/// Two tools: a plunge, arcs both ways, a helical whole turn and a cut
/// with tool 1 off the round millimetre; a hole and rapids that rise and
/// sink with tool 2 under flood coolant.
fn round_trip_program() -> Program {
    let mut p = Program::new("Round trip", 15.0);
    let arc = |to: P3, center: [f64; 2], ccw: bool, feed: f64| Move::Arc {
        to,
        center,
        ccw,
        feed,
    };
    p.operations.push(ProgramOp {
        name: "Pocket 1".into(),
        tool: Tool::flat_end_mill(1, 6.0),
        spindle_rpm: 18000.0,
        spindle: SpindleDirection::Cw,
        coolant: Coolant::Off,
        toolpath: Toolpath {
            moves: vec![
                Move::Rapid {
                    to: [12.3456, 7.891, 5.0],
                },
                Move::Line {
                    to: [12.3456, 7.891, -1.5],
                    feed: 333.3,
                },
                arc([32.3456, 7.891, -1.5], [22.3456, 7.891], true, 1234.5),
                arc([27.3456, 12.891, -1.5], [27.3456, 7.891], false, 1234.5),
                arc([27.3456, 12.891, -3.0], [27.3456, 7.891], true, 800.0),
                Move::Line {
                    to: [40.0, 20.0, -3.0],
                    feed: 1234.5,
                },
                Move::Rapid {
                    to: [40.0, 20.0, 15.0],
                },
            ],
            ..Toolpath::default()
        },
    });
    p.operations.push(ProgramOp {
        name: "Drill 1".into(),
        tool: Tool::flat_end_mill(2, 3.0),
        spindle_rpm: 10000.0,
        spindle: SpindleDirection::Cw,
        coolant: Coolant::Flood,
        toolpath: Toolpath {
            moves: vec![
                Move::Rapid {
                    to: [-5.5, 60.25, 2.0],
                },
                Move::Line {
                    to: [-5.5, 60.25, -4.0],
                    feed: 150.0,
                },
                Move::Rapid {
                    to: [-5.5, 60.25, 15.0],
                },
                Move::Rapid {
                    to: [70.125, 3.0, 20.0],
                },
            ],
            ..Toolpath::default()
        },
    });
    p
}

fn machine(changer: ToolChanger) -> Machine {
    Machine {
        name: "Router".into(),
        controller: "grbl".into(),
        travel: Bounds {
            min: [0.0, 0.0, -150.0],
            max: [800.0, 800.0, 0.0],
        },
        spindle: Spindle {
            min_rpm: 8000.0,
            max_rpm: 24000.0,
            spin_up_seconds: 2.5,
            nose_diameter: 40.0,
            nose_length: 60.0,
        },
        max_feed: 5000.0,
        rapid_feed: 8000.0,
        zones: vec![],
        tool_changer: changer,
        park: Some([5.0, 790.0, -2.0]),
    }
}

fn rack() -> ToolChanger {
    ToolChanger::Macro(MacroChanger {
        slots: vec![
            RackSlot {
                tool: 1,
                position: [20.0, 780.0, -90.0],
            },
            RackSlot {
                tool: 2,
                position: [60.0, 780.0, -90.5],
            },
        ],
        safe_z: -5.0,
        unload: vec![
            "G53 G0 Z{safe_z}".into(),
            "G53 G0 X{x} Y{y}".into(),
            "G53 G1 Z{z} F500".into(),
            "M64 P0 (release T{tool})".into(),
            "G4 P1".into(),
            "G53 G0 Z{safe_z}".into(),
        ],
        load: vec![
            "G53 G0 X{x} Y{y}".into(),
            "G53 G1 Z{z} F500".into(),
            "M65 P0 (clamp {name})".into(),
            "G53 G0 Z{safe_z}".into(),
        ],
    })
}

#[test]
fn every_dialects_program_reads_back_as_its_moves_within_the_rounding() {
    let program = round_trip_program();
    let mut variants = Vec::new();
    for units in [Units::Millimetres, Units::Inches] {
        for arcs in [true, false] {
            variants.push(PostSettings {
                units,
                arcs,
                line_numbers: !arcs,
                tool_length_offset: Some(arcs),
                ..PostSettings::default()
            });
        }
    }
    let mut checked = 0;
    for d in dialects() {
        for changer in [ToolChanger::Manual, ToolChanger::Controller, rack()] {
            let m = machine(changer);
            for s in &variants {
                let what = format!(
                    "{} {:?} {:?} arcs {}",
                    d.id(),
                    m.tool_changer,
                    s.units,
                    s.arcs
                );
                let posted = post_with_lines(&program, d.as_ref(), s, Some(&m)).unwrap();
                assert_eq!(posted.moves.len(), 2, "{what}");
                assert_eq!(posted.changes.len(), 2, "{what}");
                let mut reader = plain();
                reader.select_on_t = matches!(d.id(), "reprapfirmware" | "millenniumos");
                let read = reader.read(&posted.text).unwrap_or_else(|e| {
                    panic!("{what}: line {}: {}\n{}", e.line, e.message, posted.text)
                });
                let mut by_line: BTreeMap<usize, Vec<&GMove>> = BTreeMap::new();
                for r in &read {
                    by_line.entry(r.line).or_default().push(r);
                }
                // The last digit's half, and a center from two rounded numbers.
                let inches = s.units == Units::Inches;
                let tol = if inches { 25.4e-4 / 2.0 } else { 0.0005 } + 1e-9;
                let feed_tol = if inches { 0.254 / 2.0 } else { 0.05 } + 1e-9;
                let mut program_lines = std::collections::BTreeSet::new();
                for (oi, op) in program.operations.iter().enumerate() {
                    for (mi, mv) in op.toolpath.moves.iter().enumerate() {
                        let span = posted.moves[oi][mi];
                        if span.count == 0 {
                            continue;
                        }
                        let lines = span.first..span.first + span.count;
                        program_lines.extend(lines.clone());
                        let got: Vec<&GMove> = lines
                            .flat_map(|l| by_line.get(&l).cloned().unwrap_or_default())
                            .collect();
                        let at = format!("{what}: operation {oi} move {mi}");
                        let last = got.last().unwrap_or_else(|| panic!("{at}: nothing read"));
                        assert!(near3(end(last), mv.to(), tol), "{at}: {last:?} {mv:?}");
                        match *mv {
                            Move::Rapid { .. } => {
                                assert!(got.iter().all(|g| g.motion == Motion::Rapid), "{at}");
                            }
                            Move::Line { feed, .. } => {
                                assert_eq!(got.len(), 1, "{at}");
                                assert_eq!(got[0].motion, Motion::Feed, "{at}");
                                assert!((got[0].feed.unwrap() - feed).abs() <= feed_tol, "{at}");
                            }
                            Move::Arc {
                                center, ccw, feed, ..
                            } => {
                                for g in &got {
                                    assert!((g.feed.unwrap() - feed).abs() <= feed_tol, "{at}");
                                }
                                if s.arcs {
                                    assert_eq!(got.len(), 1, "{at}");
                                    let Motion::Arc {
                                        center: c,
                                        ccw: turn,
                                        plane,
                                    } = got[0].motion
                                    else {
                                        panic!("{at}: {:?}", got[0]);
                                    };
                                    assert_eq!((turn, plane), (ccw, Plane::Xy), "{at}");
                                    assert!(
                                        (c[0] - center[0]).abs() <= 2.0 * tol
                                            && (c[1] - center[1]).abs() <= 2.0 * tol,
                                        "{at}: {c:?}"
                                    );
                                } else {
                                    assert!(got.iter().all(|g| g.motion == Motion::Feed), "{at}");
                                }
                            }
                        }
                        checked += 1;
                    }
                }
                // Every other cut is the changer's own, in machine coordinates.
                for r in &read {
                    if r.motion != Motion::Rapid && !program_lines.contains(&r.line) {
                        assert!(r.machine, "{what}: line {}", r.line);
                    }
                }
            }
        }
    }
    assert!(checked > 7 * 3 * 4 * 10, "{checked}");
}
