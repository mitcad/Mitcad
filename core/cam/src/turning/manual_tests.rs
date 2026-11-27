// SPDX-License-Identifier: MIT
//! Manual G-code blocks between the operations (mitcad#2506): a bar pulled
//! after parting off, written in exact G-code with Parting Off last and with
//! an operation after it, the state the post leaves before the block and
//! states again after it, the read-back passing over the block, line
//! numbers and comments off, LinuxCNC, the sub-spindle, and what is refused
//! in words.

use super::*;
use crate::post::PostSettings;

fn rapid(x: f64, z: f64) -> TurnMove {
    TurnMove::Rapid { to: [x, z] }
}

fn line(x: f64, z: f64, feed: f64) -> TurnMove {
    TurnMove::Line { to: [x, z], feed }
}

fn turning_tool() -> TurningTool {
    TurningTool {
        number: 1,
        offset: 0,
        name: "Turning tool".into(),
        kind: TurningToolKind::Turning,
        nose_radius: 0.0,
        max_depth: 4.0,
        back_angle: None,
    }
}

fn parting_blade() -> TurningTool {
    TurningTool {
        number: 2,
        offset: 0,
        name: "Parting blade".into(),
        kind: TurningToolKind::Parting { width: 3.0 },
        nose_radius: 0.0,
        max_depth: 15.0,
        back_angle: None,
    }
}

fn op(
    name: &str,
    tool: TurningTool,
    spindle: TurnSpindle,
    moves: Vec<TurnMove>,
) -> TurningProgramOp {
    TurningProgramOp {
        name: name.into(),
        tool,
        spindle,
        direction: SpindleDirection::Cw,
        coolant: Coolant::Flood,
        path: TurnPath { moves, cycle: None },
        live: None,
        sub: false,
    }
}

const CSS: TurnSpindle = TurnSpindle::Css {
    surface_speed: 200.0,
    max_rpm: 3000.0,
};

/// A 20 mm pin turned from a 24 mm bar and parted off 23 mm from its face.
fn bar_job() -> TurningProgram {
    let mut program = TurningProgram::new("Bar job", [50.0, 100.0]);
    program.operations = vec![
        op(
            "Turn",
            turning_tool(),
            CSS,
            vec![
                rapid(10.0, 2.0),
                line(10.0, -20.0, 0.2),
                line(12.0, -20.0, 0.2),
                rapid(12.0, 2.0),
            ],
        ),
        op(
            "Part off",
            parting_blade(),
            TurnSpindle::Rpm { rpm: 800.0 },
            vec![
                rapid(12.0, -23.0),
                line(0.0, -23.0, 0.05),
                rapid(12.0, -23.0),
            ],
        ),
    ];
    program
}

/// The bar pulled out of the opened chuck with the parting blade.
fn bar_pull(at: usize) -> ManualBlock {
    ManualBlock {
        name: "Bar pull".into(),
        at,
        lines: vec![
            "G0 X0. Z-20.".into(),
            "  M11 (CHUCK OPEN)  ".into(),
            String::new(),
            "G98 G1 Z5. F500.".into(),
            "M10 (CHUCK CLOSE)".into(),
            "G0 X60.".into(),
        ],
    }
}

/// The program up to the end of the parting off and the state the post
/// leaves after it: the coolant off, the tool at the change position, the
/// spindle stopped.
const UP_TO_THE_BOUNDARY: &str = "\
%
O1000 (BAR JOB)
(WARNING: MITCAD CAM IS HIGHLY EXPERIMENTAL.)
(TREAT THIS MACHINE CODE WITH CAUTION. CHECK EVERY TOOLPATH)
(IN A SIMULATOR, THEN RUN IT FIRST WITHOUT STOCK, AT A LOW)
(FEED, WITH A HAND ON THE EMERGENCY STOP, BEFORE CUTTING.)
(BAR JOB)
(T0101 TURNING TOOL)
(T0202 PARTING BLADE)
G21 G18 G40 G99
G54
(TURN)
G0 X100. Z100.
T0101
G50 S3000
G96 S200 M3
M8
G0 X20. Z2.
G1 Z-20. F0.2
X24.
G0 Z2.
(PART OFF)
M9
G0 X100. Z100.
T0202
G97 S800 M3
M8
G0 X24. Z-23.
G1 X0. F0.05
G0 X24.
M9
X100. Z100.
M5
";

/// The block's lines as written, and the modes, the work offset, the
/// coolant and the spindle stated again after it.
const THE_BLOCK: &str = "\
(MANUAL CODE BEGIN: BAR PULL, NOT SIMULATED OR READ BACK)
G0 X0. Z-20.
M11 (CHUCK OPEN)
G98 G1 Z5. F500.
M10 (CHUCK CLOSE)
G0 X60.
(MANUAL CODE END)
G21 G18 G40 G99
G54
M9
M5
";

#[test]
fn a_bar_pull_after_parting_off_last_comes_before_the_end() {
    // Without a block: the program as before, the parting off's boundary
    // being the shutdown's own lines.
    let plain = turning_post(&bar_job(), &FanucTurning, &PostSettings::default()).unwrap();
    assert_eq!(plain, format!("{UP_TO_THE_BOUNDARY}M30\n%\n"));

    let mut program = bar_job();
    program.manual = vec![bar_pull(2)];
    program.validate().unwrap();
    let text = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap();
    let expected = format!("{UP_TO_THE_BOUNDARY}{THE_BLOCK}G0 X100. Z100.\nM30\n%\n");
    assert_eq!(text, expected);

    // Read back: the block passed over (its fencing comments' lines), the
    // moves only the generated ones, the change position's rapid after it
    // with both axes.
    let read = check_program(&text).unwrap();
    assert_eq!(read.manual, vec![[34, 40]]);
    let lines: Vec<usize> = read.moves.iter().map(|m| m.line).collect();
    assert_eq!(lines, vec![13, 18, 19, 20, 21, 24, 28, 29, 30, 32, 45]);
    assert_eq!(read.moves.last().unwrap().motion, rapid(50.0, 100.0));
    assert_eq!(read.css_lines, vec![16]);
}

#[test]
fn a_bar_pull_between_parting_off_and_the_next_operation() {
    let mut program = bar_job();
    program.operations.push(op(
        "Face next",
        turning_tool(),
        CSS,
        vec![rapid(14.0, 0.5), line(0.0, 0.5, 0.15), rapid(14.0, 2.0)],
    ));
    program.manual = vec![bar_pull(2)];
    let text = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap();
    // The next operation from its tool change on: the turret indexed again
    // though the parting blade's station was the turning tool's before,
    // the spindle's limit and speed, the coolant, the first move with both
    // axes.
    let expected = format!(
        "{UP_TO_THE_BOUNDARY}{THE_BLOCK}\
(FACE NEXT)
G0 X100. Z100.
T0101
G50 S3000
G96 S200 M3
M8
G0 X28. Z0.5
G1 X0. F0.15
G0 X28. Z2.
M9
X100. Z100.
M5
M30
%
"
    );
    assert_eq!(text, expected);
    let read = check_program(&text).unwrap();
    assert_eq!(read.manual, vec![[34, 40]]);
    assert_eq!(read.css_lines, vec![16, 49]);
    assert!(read.moves.iter().all(|m| !(34..=40).contains(&m.line)));
    // The same block at the program's start, before the first operation:
    // the tool to the change position first.
    let mut program = bar_job();
    program.manual = vec![bar_pull(0)];
    let text = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap();
    assert!(
        text.contains("G54\nG0 X100. Z100.\n(MANUAL CODE BEGIN: BAR PULL"),
        "{text}"
    );
    assert!(
        text.contains("M5\n(TURN)\nG0 X100. Z100.\nT0101\n"),
        "{text}"
    );
    check_program(&text).unwrap();
}

#[test]
fn the_block_is_fenced_with_line_numbers_and_without_comments() {
    let mut program = bar_job();
    program.manual = vec![bar_pull(2)];
    let settings = PostSettings {
        line_numbers: true,
        comments: false,
        ..PostSettings::default()
    };
    let text = turning_post(&program, &FanucTurning, &settings).unwrap();
    // The fencing comments even without comments, the block's lines
    // numbered as the others.
    assert!(
        text.contains("\n(MANUAL CODE BEGIN: BAR PULL, NOT SIMULATED OR READ BACK)\n"),
        "{text}"
    );
    assert!(text.contains("\n(MANUAL CODE END)\n"), "{text}");
    let numbered = text
        .lines()
        .find(|l| l.ends_with("M11 (CHUCK OPEN)"))
        .expect("the block's line");
    assert!(numbered.starts_with('N'), "{numbered}");
    let read = check_program(&text).unwrap();
    assert_eq!(read.manual.len(), 1);
}

#[test]
fn linuxcnc_states_its_modes_again_after_the_block() {
    let mut program = bar_job();
    program.manual = vec![bar_pull(2)];
    let text = turning_post(&program, &LinuxCncTurning, &PostSettings::default()).unwrap();
    assert!(
        text.contains(
            "(MANUAL CODE END)\nG21 G18 G7 G90 G91.1 G40 G80 G95\nG64 P0.01\nG54\nM9\nM5\nG0 X100. Z100.\nM2\n%\n"
        ),
        "{text}"
    );
    let read = check_program_as(&text, &MillTurn::default(), Control::LinuxCnc).unwrap();
    assert_eq!(read.manual.len(), 1);
    // A subroutine call and lower case letters are LinuxCNC's own.
    let mut program = bar_job();
    program.manual = vec![ManualBlock {
        name: "Bar pull".into(),
        at: 2,
        lines: vec!["o<bar_pull> call [20]".into(), "m5 ; stopped".into()],
    }];
    let text = turning_post(&program, &LinuxCncTurning, &PostSettings::default()).unwrap();
    assert!(
        text.contains("\no<bar_pull> call [20]\nm5 ; stopped\n"),
        "{text}"
    );
    check_program_as(&text, &MillTurn::default(), Control::LinuxCnc).unwrap();
    // A Fanuc control reads capitals.
    let error = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap_err();
    assert_eq!(
        error.message,
        "The manual G-code Bar pull cannot be written for Fanuc (turning): its line 1 has lower case letters: the control reads capitals."
    );
}

#[test]
fn what_a_control_would_not_run_in_a_block_is_refused_in_words() {
    let fanuc = |l: &str| manual_refusal(l, Control::Fanuc);
    let lcnc = |l: &str| manual_refusal(l, Control::LinuxCnc);
    for ok in [
        "M5",
        "G0 X0. Z-20. (TO THE BAR)",
        "M98 P2000",
        "#100=SIN[#1]",
        "IF[#1EQ0]GOTO5",
        "G65 P9010 A1.",
    ] {
        assert_eq!(fanuc(ok), None, "{ok}");
    }
    for (bad, why) in [
        ("M30", "M30, the program's end"),
        ("G0 X1. M2", "M2, the program's end"),
        ("M99", "M99"),
        ("N10 G0 X1.", "an N word"),
        ("O2000", "an O word"),
        ("%", "a tape mark"),
        ("G0 X1. (OPEN", "a comment that is not closed"),
        ("G0\tX1.", "printable ASCII"),
        ("m5", "lower case"),
        ("(MANUAL CODE END)", "fence the manual code"),
    ] {
        let got = fanuc(bad).unwrap_or_default();
        assert!(got.contains(why), "{bad}: {got}");
    }
    assert_eq!(lcnc("o<pull> call"), None);
    assert_eq!(lcnc("o100 call [1]"), None);
    assert_eq!(lcnc("m5 ; M30 in a comment"), None);
    assert_eq!(lcnc("M99"), None);
    assert!(lcnc("o100 sub").unwrap().contains("subroutine call"));
    assert!(lcnc("M2").unwrap().contains("M2"));

    // The post names the block, the control and the line.
    let mut program = bar_job();
    program.manual = vec![ManualBlock {
        name: "Bar pull".into(),
        at: 2,
        lines: vec!["M5".into(), "M30".into()],
    }];
    let error = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap_err();
    assert_eq!(
        error.message,
        "The manual G-code Bar pull cannot be written for Fanuc (turning): its line 2 has M30, the program's end; the post writes the end after the operations."
    );
    // A block without code, and one beyond the operations.
    program.manual[0].lines = vec!["  ".into()];
    assert_eq!(
        program.validate().unwrap_err().message,
        "The manual G-code Bar pull has no code."
    );
    program.manual[0] = bar_pull(3);
    assert_eq!(
        program.validate().unwrap_err().message,
        "The manual G-code Bar pull follows an operation the program does not have."
    );
}

#[test]
fn the_read_back_states_nothing_across_a_block() {
    // After the block the units, the modes and the spindle must be stated
    // again before a cut.
    let missing = "\
%
O1000
G21 G18 G40 G99
G54
T0101
G97 S500 M3
G0 X20. Z2.
(MANUAL CODE BEGIN: X, NOT SIMULATED OR READ BACK)
G20
G0 X5. Z5.
(MANUAL CODE END)
G1 X10. F0.1
M30
%
";
    let error = check_program(missing).unwrap_err();
    assert!(error.message.starts_with("Line 12: "), "{}", error.message);
    let stated = missing.replace(
        "(MANUAL CODE END)\n",
        "(MANUAL CODE END)\nG21 G18 G40 G99\nG54\nM5\nG97 S500 M3\nG0 X20. Z2.\n",
    );
    let read = check_program(&stated).unwrap();
    assert_eq!(read.manual, vec![[8, 11]]);
    let lines: Vec<usize> = read.moves.iter().map(|m| m.line).collect();
    assert_eq!(lines, vec![7, 16, 17]);
    // A block that does not end, and an end without its beginning.
    let open = "%\nG21 G18 G40 G99\n(MANUAL CODE BEGIN: X, NOT SIMULATED OR READ BACK)\nM5\n%\n";
    assert_eq!(
        check_program(open).unwrap_err().message,
        "The program ends inside manual code."
    );
    let stray = "%\nG21 G18 G40 G99\n(MANUAL CODE END)\nM30\n%\n";
    assert_eq!(
        check_program(stray).unwrap_err().message,
        "Line 3: the end of manual code that did not begin."
    );
}

#[test]
fn on_the_sub_spindle_the_block_keeps_out_of_the_hand_over() {
    use super::sub_live_tests::sub_program;
    // Between the parting off and the first operation on the sub-spindle
    // both spindles hold the part.
    let mut program = sub_program();
    program.manual = vec![bar_pull(1)];
    let error = program.validate().unwrap_err();
    assert!(
        error
            .message
            .starts_with("The manual G-code Bar pull is inside the hand-over"),
        "{}",
        error.message
    );
    // Before the parting off (and the hand-over written there) it goes.
    let mut program = sub_program();
    program.manual = vec![bar_pull(0)];
    let text = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap();
    let block = text.find("(MANUAL CODE END)").unwrap();
    let grip = text.find("(HAND-OVER TO THE SUB-SPINDLE)").unwrap();
    assert!(block < grip, "{text}");
    check_program_with(&text, &program.mill_turn).unwrap();
    // After the first operation on the sub-spindle: the sub-spindle's work
    // offset and its stop after the block.
    let mut program = sub_program();
    program.manual = vec![bar_pull(2)];
    let text = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap();
    let after = format!(
        "(MANUAL CODE END)\nG21 G18 G40 G99\nG55\nM9\n{}\n",
        program.mill_turn.sub_stop.trim()
    );
    assert!(text.contains(&after), "{text}");
    let read = check_program_with(&text, &program.mill_turn).unwrap();
    assert_eq!(read.manual.len(), 1);
}
