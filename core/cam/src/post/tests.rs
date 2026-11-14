// SPDX-License-Identifier: MIT
//! Golden G-code of each dialect, and the post run's options.

use super::*;
use crate::machine::{Bounds, MacroChanger, RackSlot, Spindle};
use crate::program::ProgramOp;
use crate::toolpath::Toolpath;

/// Two operations: a circle cut with tool 1 and a hole with tool 2 under
/// flood coolant.
fn program() -> Program {
    let mut p = Program::new("Bracket", 15.0);
    p.operations.push(ProgramOp {
        name: "Contour 1".into(),
        tool: Tool::flat_end_mill(1, 6.0),
        spindle_rpm: 18000.0,
        spindle: SpindleDirection::Cw,
        coolant: Coolant::Off,
        toolpath: Toolpath {
            moves: vec![
                Move::Rapid {
                    to: [10.0, 0.0, 5.0],
                },
                Move::Line {
                    to: [10.0, 0.0, -2.0],
                    feed: 300.0,
                },
                Move::Arc {
                    to: [-10.0, 0.0, -2.0],
                    center: [0.0, 0.0],
                    ccw: true,
                    feed: 1000.0,
                },
                Move::Arc {
                    to: [10.0, 0.0, -2.0],
                    center: [0.0, 0.0],
                    ccw: true,
                    feed: 1000.0,
                },
                Move::Line {
                    to: [20.0, 0.0, -2.0],
                    feed: 1000.0,
                },
                Move::Rapid {
                    to: [20.0, 0.0, 15.0],
                },
            ],
            ..Toolpath::default()
        },
    });
    p.operations.push(ProgramOp {
        name: "Drill".into(),
        tool: Tool::flat_end_mill(2, 3.0),
        spindle_rpm: 10000.0,
        spindle: SpindleDirection::Cw,
        coolant: Coolant::Flood,
        toolpath: Toolpath {
            moves: vec![
                Move::Rapid {
                    to: [5.0, 5.0, 15.0],
                },
                Move::Rapid {
                    to: [5.0, 5.0, 2.0],
                },
                Move::Line {
                    to: [5.0, 5.0, -5.0],
                    feed: 200.0,
                },
                Move::Rapid {
                    to: [5.0, 5.0, 15.0],
                },
            ],
            ..Toolpath::default()
        },
    });
    p
}

/// The moves of the first operation as every dialect writes them.
const CIRCLE: &str = "\
G0 X10 Y0
G0 Z5
G1 Z-2 F300
G3 X-10 Y0 I-10 J0 F1000
G3 X10 Y0 I10 J0
G1 X20
G0 Z15";

/// The moves of the second operation.
const HOLE: &str = "\
G0 X5 Y5
G0 Z2
G1 Z-5 F200
G0 Z15";

/// The notice every program starts with (mitcad#1013), in parentheses.
const NOTICE: &str = "\
(WARNING: MitCad CAM is highly experimental.)
(Treat this machine code with caution. Check every toolpath)
(in a simulator, then run it first without stock, at a low)
(feed, with a hand on the emergency stop, before cutting.)";

/// The notice after semicolons.
const NOTICE_SEMICOLON: &str = "\
; WARNING: MitCad CAM is highly experimental.
; Treat this machine code with caution. Check every toolpath
; in a simulator, then run it first without stock, at a low
; feed, with a hand on the emergency stop, before cutting.";

fn run(id: &str) -> String {
    let d = dialect(id).expect("a dialect");
    post(&program(), d.as_ref(), &PostSettings::default(), None).unwrap()
}

#[test]
fn every_dialect_is_listed_once_by_its_id() {
    let ids: Vec<&str> = dialects().iter().map(|d| d.id()).collect();
    assert_eq!(
        ids,
        vec![
            "grbl",
            "grblhal",
            "reprapfirmware",
            "millenniumos",
            "mach3",
            "mach4",
            "linuxcnc"
        ]
    );
    assert!(dialect("fanuc").is_none());
}

#[test]
fn grbl_stops_for_a_manual_change_after_the_first_tool() {
    let expected = format!(
        "\
{NOTICE}
(Bracket)
(T1 6 mm flat end mill, D6 mm)
(T2 3 mm flat end mill, D3 mm)
G90 G94 G17
G21
G54
G0 Z15
(Contour 1)
(Tool 1: 6 mm flat end mill)
G0 Z15
S18000 M3
{CIRCLE}
(Drill)
M5
(Change to tool 2: 3 mm flat end mill, then set its Z zero)
M0
G0 Z15
S10000 M3
M8
{HOLE}
M5
M9
M30
"
    );
    assert_eq!(run("grbl"), expected);
}

#[test]
fn grblhal_changes_tools_with_m6() {
    let expected = format!(
        "\
{NOTICE}
(Bracket)
(T1 6 mm flat end mill, D6 mm)
(T2 3 mm flat end mill, D3 mm)
G90 G94 G17
G21
G54
G0 Z15
(Contour 1)
T1 M6
G0 Z15
S18000 M3
{CIRCLE}
(Drill)
M5
T2 M6
G0 Z15
S10000 M3
M8
{HOLE}
M5
M9
M30
"
    );
    assert_eq!(run("grblhal"), expected);
}

#[test]
fn reprapfirmware_selects_tools_and_asks_for_manual_changes() {
    let expected = format!(
        "\
{NOTICE_SEMICOLON}
; Bracket
; T1 6 mm flat end mill, D6 mm
; T2 3 mm flat end mill, D3 mm
G90
G21
G17
G54
G0 Z15
; Contour 1
T1
G0 Z15
M3 S18000
{CIRCLE}
; Drill
M5
M291 P\"Change to tool 2: 3 mm flat end mill\" R\"Tool change\" S2
T2
G0 Z15
M3 S10000
M8
{HOLE}
M5
M9
M0
"
    );
    assert_eq!(run("reprapfirmware"), expected);
}

#[test]
fn millenniumos_defines_its_tools_and_waits_for_the_spindle() {
    let expected = format!(
        "\
{NOTICE_SEMICOLON}
; Bracket
; T1 6 mm flat end mill, D6 mm
; T2 3 mm flat end mill, D3 mm
G90 G21 G94
M4000 P1 R3 S\"6 mm flat end mill\"
M4000 P2 R1.5 S\"3 mm flat end mill\"
G28
G54
G0 Z15
; Contour 1
T1
G0 Z15
M3.9 S18000
{CIRCLE}
; Drill
M5.9
T2
G0 Z15
M3.9 S10000
M8
{HOLE}
M5.9
M9
G27
M0
"
    );
    assert_eq!(run("millenniumos"), expected);
}

fn mach_like(header: &str, footer: &str) -> String {
    format!(
        "\
{header}
G0 Z15
(Contour 1)
T1 M6
G43 H1
G0 Z15
S18000 M3
{CIRCLE}
(Drill)
M5
T2 M6
G43 H2
G0 Z15
S10000 M3
M8
{HOLE}
M5
M9
{footer}
"
    )
}

#[test]
fn mach3_and_mach4_change_tools_with_m6_and_the_length_offset() {
    let header = format!(
        "\
{NOTICE}
(Bracket)
(T1 6 mm flat end mill, D6 mm)
(T2 3 mm flat end mill, D3 mm)
G90 G94 G91.1 G40 G49 G17
G21
G54"
    );
    assert_eq!(run("mach3"), mach_like(&header, "M30"));
    assert_eq!(run("mach4"), mach_like(&header, "M30"));
    assert_eq!(dialect("mach3").unwrap().extension(), "tap");
}

#[test]
fn linuxcnc_is_between_percent_signs_and_blends_paths() {
    let header = format!(
        "\
%
{NOTICE}
(Bracket)
(T1 6 mm flat end mill, D6 mm)
(T2 3 mm flat end mill, D3 mm)
G17 G90 G94 G40 G49 G80
G21
G64 P0.01
G54"
    );
    assert_eq!(run("linuxcnc"), mach_like(&header, "M2\n%"));
    assert_eq!(dialect("linuxcnc").unwrap().extension(), "ngc");
}

#[test]
fn inches_convert_lengths_and_feeds_but_not_speeds() {
    let settings = PostSettings {
        units: Units::Inches,
        ..PostSettings::default()
    };
    let g = post(&program(), &GrblHal, &settings, None).unwrap();
    assert!(g.contains("\nG20\n"));
    assert!(g.contains("\nG0 Z0.5906\n"));
    assert!(g.contains("\nG0 X0.3937 Y0\n"));
    assert!(g.contains("\nG1 Z-0.0787 F11.81\n"));
    assert!(g.contains("\nG3 X-0.3937 Y0 I-0.3937 J0 F39.37\n"));
    assert!(g.contains("\nS18000 M3\n"));
}

#[test]
fn arcs_become_lines_within_the_tolerance_when_turned_off() {
    let settings = PostSettings {
        arcs: false,
        arc_tolerance: 0.05,
        ..PostSettings::default()
    };
    let g = post(&program(), &LinuxCnc, &settings, None).unwrap();
    assert!(
        !g.lines()
            .any(|l| l.starts_with("G2 ") || l.starts_with("G3 "))
    );
    let lines = g.lines().filter(|l| l.starts_with("G1 X")).count();
    // A 10 mm radius half circle at 0.05 mm is 16 chords of 0.2 radians.
    assert_eq!(lines, 2 * 16 + 1);
    assert!(g.contains("G64 P0.05"));
}

#[test]
fn line_numbers_skip_the_tape_marks_and_comments_can_be_left_out() {
    let settings = PostSettings {
        line_numbers: true,
        comments: false,
        ..PostSettings::default()
    };
    let g = post(&program(), &LinuxCnc, &settings, None).unwrap();
    let lines: Vec<&str> = g.lines().collect();
    assert_eq!(lines[0], "%");
    // The notice stays, unnumbered; every other comment is left out.
    let notice: Vec<&str> = NOTICE.lines().collect();
    assert_eq!(lines[1..=notice.len()], notice[..]);
    assert_eq!(lines[notice.len() + 1], "N10 G17 G90 G94 G40 G49 G80");
    assert_eq!(*lines.last().unwrap(), "%");
    assert_eq!(g.matches('(').count(), notice.len());
}

#[test]
fn every_program_starts_with_the_notice_that_cam_is_experimental() {
    for d in dialects() {
        for comments in [true, false] {
            let settings = PostSettings {
                comments,
                line_numbers: true,
                ..PostSettings::default()
            };
            let g = post(&program(), d.as_ref(), &settings, None).unwrap();
            let body = g.strip_prefix("%\n").unwrap_or(&g);
            let notice = match d.comment_style() {
                CommentStyle::Parentheses => NOTICE,
                CommentStyle::Semicolon => NOTICE_SEMICOLON,
            };
            assert!(
                body.starts_with(&format!("{notice}\n")),
                "{} comments {comments}: {g}",
                d.id()
            );
        }
    }
    // Nothing in it a controller acts on, nothing a cleaning changes, no
    // line LinuxCNC reads as a command.
    let out = Output::new(&PostSettings::default(), CommentStyle::Parentheses);
    for line in EXPERIMENTAL_NOTICE {
        assert!(
            !line.contains(['?', '~', '!', '(', ')', ';', '%']),
            "{line}"
        );
        assert_eq!(out.clean(line), *line);
        let upper = line.to_ascii_uppercase();
        for word in ["MSG", "DEBUG", "PRINT", "LOG", "PROBE"] {
            assert!(!upper.starts_with(word), "{line}");
        }
    }
}

#[test]
fn comments_lose_what_the_controller_cannot_read() {
    let mut p = program();
    p.operations[0].name = "Pocket (rough) \u{2013} \u{e4}".into();
    let g = post(&p, &Mach3, &PostSettings::default(), None).unwrap();
    assert!(g.contains("\n(Pocket [rough] ? ?)\n"));
    let r = post(&p, &RepRapFirmware, &PostSettings::default(), None).unwrap();
    assert!(r.contains("\n; Pocket (rough) ? ?\n"));
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

#[test]
fn a_macro_changer_puts_the_tool_away_and_takes_the_next_from_the_rack() {
    let changer = ToolChanger::Macro(MacroChanger {
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
            "G53 G0 Z{safe_z}".into(),
        ],
        load: vec![
            "G53 G0 X{x} Y{y}".into(),
            "G53 G1 Z{z} F500".into(),
            "M65 P0 (clamp {name})".into(),
            "G53 G0 Z{safe_z}".into(),
        ],
    });
    let g = post(
        &program(),
        &Grbl,
        &PostSettings::default(),
        Some(&machine(changer)),
    )
    .unwrap();
    let expected_change = "\
(Drill)
M5
G53 G0 Z-5
G53 G0 X20 Y780
G53 G1 Z-90 F500
M64 P0 (release T1)
G53 G0 Z-5
G53 G0 X60 Y780
G53 G1 Z-90.5 F500
M65 P0 (clamp 3 mm flat end mill)
G53 G0 Z-5
G0 Z15
S10000 M3
G4 P2.5
M8
";
    assert!(g.contains(expected_change), "{g}");
    // The first tool is only taken, nothing is put away.
    assert!(g.contains("(Contour 1)\nG53 G0 X20 Y780\n"));
    // The end parks the spindle in machine coordinates.
    assert!(g.ends_with("M9\nG53 G0 Z-2\nG53 G0 X5 Y790\nM30\n"), "{g}");
}

#[test]
fn a_tool_without_a_rack_slot_is_changed_by_hand() {
    let changer = ToolChanger::Macro(MacroChanger {
        slots: vec![RackSlot {
            tool: 1,
            position: [20.0, 780.0, -90.0],
        }],
        safe_z: -5.0,
        unload: vec!["G53 G0 X{x} Y{y}".into()],
        load: vec!["G53 G0 X{x} Y{y}".into()],
    });
    let g = post(
        &program(),
        &LinuxCnc,
        &PostSettings::default(),
        Some(&machine(changer)),
    )
    .unwrap();
    assert!(
        g.contains(
            "(Drill)\nM5\nG53 G0 X20 Y780\n(Change to tool 2: 3 mm flat end mill)\nM0\nG43 H2\n"
        ),
        "{g}"
    );
}

#[test]
fn an_unknown_name_in_the_changer_lines_is_refused() {
    let changer = ToolChanger::Macro(MacroChanger {
        slots: vec![RackSlot {
            tool: 1,
            position: [20.0, 780.0, -90.0],
        }],
        safe_z: -5.0,
        unload: vec![],
        load: vec!["G53 G0 X{slot_x}".into()],
    });
    let e = post(
        &program(),
        &Grbl,
        &PostSettings::default(),
        Some(&machine(changer)),
    )
    .unwrap_err();
    assert!(e.message.contains("does not know"));
}

#[test]
fn a_controller_changer_on_grbl_writes_m6_for_the_sender() {
    let g = post(
        &program(),
        &Grbl,
        &PostSettings::default(),
        Some(&machine(ToolChanger::Controller)),
    )
    .unwrap();
    assert!(g.contains("(Contour 1)\nT1 M6\n"));
    assert!(!g.contains("M0"));
    let forced = PostSettings {
        tool_length_offset: Some(true),
        ..PostSettings::default()
    };
    let g = post(&program(), &Grbl, &forced, None).unwrap();
    assert!(g.contains("(Tool 1: 6 mm flat end mill)\nG43.1 Z23\n"));
}

#[test]
fn air_blast_and_a_backwards_spindle_are_refused_where_there_is_no_command() {
    let mut p = program();
    p.operations[1].coolant = Coolant::Air;
    assert_eq!(
        post(&p, &Grbl, &PostSettings::default(), None)
            .unwrap_err()
            .message,
        "grbl has no command for air blast."
    );
    assert!(
        post(&p, &MillenniumOs, &PostSettings::default(), None)
            .unwrap()
            .contains("\nM7.1\n")
    );
    let mut q = program();
    q.operations[0].spindle = SpindleDirection::Ccw;
    assert!(
        post(&q, &Mach4, &PostSettings::default(), None)
            .unwrap()
            .contains("\nS18000 M4\n")
    );
    assert!(post(&q, &MillenniumOs, &PostSettings::default(), None).is_err());
}

#[test]
fn reprapfirmware_dwells_in_seconds_with_s() {
    let g = post(
        &program(),
        &RepRapFirmware,
        &PostSettings::default(),
        Some(&machine(ToolChanger::Controller)),
    )
    .unwrap();
    assert!(g.contains("\nM3 S18000\nG4 S2.5\n"));
    let m = post(
        &program(),
        &MillenniumOs,
        &PostSettings::default(),
        Some(&machine(ToolChanger::Controller)),
    )
    .unwrap();
    assert!(!m.contains("G4"), "M3.9 waits for the spindle itself");
    assert!(!m.contains("G53"), "MillenniumOS parks with G27");
}

#[test]
fn an_invalid_program_is_refused_in_words() {
    let mut p = program();
    p.operations[1].tool.number = 1;
    let e = post(&p, &Grbl, &PostSettings::default(), None).unwrap_err();
    assert_eq!(
        e.message,
        "Tool 1 is a different tool in Drill than before; give it its own number."
    );
    let mut q = program();
    q.operations[0].toolpath.moves.remove(0);
    assert!(post(&q, &Grbl, &PostSettings::default(), None).is_err());
}

#[test]
fn numbers_have_no_trailing_zeros_and_no_negative_zero() {
    assert_eq!(number(1.5, 3), "1.5");
    assert_eq!(number(2.0, 3), "2");
    assert_eq!(number(-0.0001, 3), "0");
    assert_eq!(number(-1.23456, 4), "-1.2346");
    assert_eq!(number(18000.4, 0), "18000");
}

/// The golden files of a dialect: the shared program in inches, and peck
/// drilling with canned cycles asked for.
fn golden(id: &str) -> (&'static str, &'static str) {
    match id {
        "grbl" => (
            include_str!("golden/grbl-inch.nc"),
            include_str!("golden/grbl-peck.nc"),
        ),
        "grblhal" => (
            include_str!("golden/grblhal-inch.nc"),
            include_str!("golden/grblhal-peck.nc"),
        ),
        "reprapfirmware" => (
            include_str!("golden/reprapfirmware-inch.gcode"),
            include_str!("golden/reprapfirmware-peck.gcode"),
        ),
        "millenniumos" => (
            include_str!("golden/millenniumos-inch.gcode"),
            include_str!("golden/millenniumos-peck.gcode"),
        ),
        "mach3" => (
            include_str!("golden/mach3-inch.tap"),
            include_str!("golden/mach3-peck.tap"),
        ),
        "mach4" => (
            include_str!("golden/mach4-inch.tap"),
            include_str!("golden/mach4-peck.tap"),
        ),
        "linuxcnc" => (
            include_str!("golden/linuxcnc-inch.ngc"),
            include_str!("golden/linuxcnc-peck.ngc"),
        ),
        _ => panic!("no golden files for {id}"),
    }
}

fn inches() -> PostSettings {
    PostSettings {
        units: Units::Inches,
        ..PostSettings::default()
    }
}

#[test]
fn every_dialect_writes_inches_with_every_length_and_feed_converted() {
    for d in dialects() {
        let g = post(&program(), d.as_ref(), &inches(), None).unwrap();
        assert_eq!(g, golden(d.id()).0, "{}", d.id());
    }
}

fn drill_tool() -> Tool {
    Tool {
        number: 3,
        name: "5 mm drill".into(),
        shape: crate::tool::ToolShape::Drill { point_angle: 118.0 },
        diameter: 5.0,
        flute_length: 30.0,
        length: 40.0,
        flutes: 2,
        cutting: None,
    }
}

/// Two holes 9 mm deep, in pecks of 3 mm (or in one go, or breaking the
/// chips), between a 10 mm clearance and a 2 mm retract height.
fn holes(peck: bool, chip_break: bool) -> Program {
    let op = crate::ops::DrillOp {
        points: vec![[30.0, 10.0], [10.0, 10.0]],
        depths: crate::ops::Depths {
            top: 0.0,
            bottom: -9.0,
            step_down: 3.0,
        },
        heights: crate::ops::Heights {
            clearance: 10.0,
            retract: 2.0,
        },
        peck,
        chip_break,
        canned_cycle: true,
        cutting: crate::tool::Cutting {
            spindle_rpm: 3000.0,
            feed: 200.0,
            plunge_feed: 150.0,
        },
    };
    let mut p = Program::new("Holes", 15.0);
    p.operations.push(ProgramOp {
        name: "Drill".into(),
        tool: drill_tool(),
        spindle_rpm: 3000.0,
        spindle: SpindleDirection::Cw,
        coolant: Coolant::Off,
        toolpath: crate::ops::drill(&op, &drill_tool()).unwrap(),
    });
    p
}

fn cycles() -> PostSettings {
    PostSettings {
        canned_cycles: true,
        ..PostSettings::default()
    }
}

#[test]
fn every_dialect_writes_peck_drilling_as_its_canned_cycle_or_as_moves() {
    for d in dialects() {
        let g = post(&holes(true, false), d.as_ref(), &cycles(), None).unwrap();
        assert_eq!(g, golden(d.id()).1, "{}", d.id());
    }
}

#[test]
fn canned_cycles_are_written_only_when_asked() {
    // Without the setting every controller gets the moves.
    let moves = post(
        &holes(true, false),
        &GrblHal,
        &PostSettings::default(),
        None,
    )
    .unwrap();
    assert_eq!(
        moves.replace("T3 M6", "(Tool 3: 5 mm drill)"),
        golden("grbl").1
    );
    // Drilling in one go and breaking chips.
    let g81 = post(&holes(false, false), &LinuxCnc, &cycles(), None).unwrap();
    assert!(
        g81.contains("\nG98 G81 X10 Y10 Z-9 R2 F150\nX30 Y10\nG80\nM5\n"),
        "{g81}"
    );
    let g73 = post(&holes(true, true), &Mach3, &cycles(), None).unwrap();
    assert!(
        g73.contains("\nG98 G73 X10 Y10 Z-9 R2 Q3 F150\nX30 Y10\nG80\n"),
        "{g73}"
    );
    // In inches.
    let settings = PostSettings {
        units: Units::Inches,
        ..cycles()
    };
    let g = post(&holes(true, false), &LinuxCnc, &settings, None).unwrap();
    assert!(
        g.contains(
            "\nG98 G83 X0.3937 Y0.3937 Z-0.3543 R0.0787 Q0.1181 F5.91\nX1.1811 Y0.3937\nG80\n"
        ),
        "{g}"
    );
    // Moves that are not what the cycle says are written out.
    let mut p = holes(true, false);
    p.operations[0].toolpath.cycles[0].retract = 3.0;
    let g = post(&p, &LinuxCnc, &cycles(), None).unwrap();
    assert!(!g.contains("G83") && g.contains("\nG1 Z-9\n"));
}

#[test]
fn millenniumos_probes_the_work_zero_when_asked_and_the_others_refuse() {
    let settings = PostSettings {
        probe_work_zero: true,
        ..PostSettings::default()
    };
    let mut p = program();
    p.work_offset = 2;
    let g = post(&p, &MillenniumOs, &settings, None).unwrap();
    assert!(
        g.contains("\nG28\n; Probe the work zero\nG6600 W2\nG55\n"),
        "{g}"
    );
    assert!(
        !post(&p, &MillenniumOs, &PostSettings::default(), None)
            .unwrap()
            .contains("G6600")
    );
    for d in dialects().iter().filter(|d| d.id() != "millenniumos") {
        let e = post(&p, d.as_ref(), &settings, None).unwrap_err();
        assert_eq!(
            e.message,
            format!("{} has no command for probing the work zero.", d.label())
        );
    }
}

#[test]
fn settings_without_the_new_fields_still_read() {
    let s: PostSettings = serde_json::from_str("{\"units\":\"inches\"}").unwrap();
    assert!(!s.canned_cycles && !s.probe_work_zero && s.units == Units::Inches);
}

#[test]
fn grbl_comments_and_names_keep_out_the_characters_grbl_acts_on_at_once() {
    // grbl and grblHAL take `?`, `~` and `!` out of the stream wherever
    // they are, a comment too (a status report, a cycle start, a feed
    // hold); found by running the output through grbl
    // (tests/controller_sim.rs).
    let mut p = program();
    p.name = "Bracket! rev ~2?".into();
    p.operations[0].name = "Contour \u{f8}6 (outside)".into();
    for id in ["grbl", "grblhal"] {
        let g = post(
            &p,
            dialect(id).unwrap().as_ref(),
            &PostSettings::default(),
            None,
        )
        .unwrap();
        assert!(
            g.contains(&format!("{NOTICE}\n(Bracket_ rev _2_)\n")),
            "{g}"
        );
        assert!(g.contains("\n(Contour _6 [outside])\n"), "{g}");
        assert!(!g.contains(['?', '~', '!']), "{g}");
    }
    // Other controllers keep them.
    let m = post(&p, &Mach3, &PostSettings::default(), None).unwrap();
    assert!(
        m.contains(&format!("{NOTICE}\n(Bracket! rev ~2?)\n")),
        "{m}"
    );
    assert!(m.contains("\n(Contour ?6 [outside])\n"), "{m}");
    // A name a changer's line takes in is cleaned the same way.
    let changer = ToolChanger::Macro(MacroChanger {
        slots: vec![RackSlot {
            tool: 2,
            position: [60.0, 780.0, -90.0],
        }],
        safe_z: -5.0,
        unload: vec![],
        load: vec!["(clamp {name})".into()],
    });
    let mut q = program();
    q.operations[1].tool.name = "3 mm (new)!".into();
    let g = post(&q, &Grbl, &PostSettings::default(), Some(&machine(changer))).unwrap();
    assert!(g.contains("\n(clamp 3 mm [new]_)\n"), "{g}");
}

#[test]
fn grbl_refuses_mist_coolant_which_its_standard_build_lacks() {
    let mut p = program();
    p.operations[1].coolant = Coolant::Mist;
    assert_eq!(
        post(&p, &Grbl, &PostSettings::default(), None)
            .unwrap_err()
            .message,
        "grbl has no command for mist coolant in its standard build; use flood coolant."
    );
    assert!(
        post(&p, &GrblHal, &PostSettings::default(), None)
            .unwrap()
            .contains("\nM7\n")
    );
}
