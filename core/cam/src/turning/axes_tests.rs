// SPDX-License-Identifier: MIT
//! Tests of the mill-turn's Y axis, B head, sub-spindle and 3D stock
//! (mitcad#2319): a flat milled with the Y axis and an off-centre hole on
//! the face, a hole on a plane tilted by the B head, in exact Fanuc G-code
//! read back; the sub-spindle's hand-over with the part-off and the back
//! faced on the sub-spindle in exact G-code; the program check's
//! refusals; the lathe check; the turned stock with the remnant left
//! behind; the plane's height maps and the stock cut in 3D.

use super::live::{LiveOp, LiveSide, PlaneKind, simulate_live};
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
        tool: station(number),
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

/// A flat 2 mm below a 40 mm bar's top with the Y axis at C 30 degrees,
/// an off-centre hole on the face with the Y axis, and a hole on a plane
/// tilted 45 degrees by the B head at C 90.
fn axes_program() -> TurningProgram {
    let flat = Toolpath {
        moves: vec![
            rapid([0.0, 0.0, 25.0]),
            rapid([0.0, 0.0, 21.0]),
            line([0.0, 0.0, 18.0], 200.0),
            line([0.0, 8.0, 18.0], 400.0),
            line([10.0, 8.0, 18.0], 400.0),
            rapid([10.0, 8.0, 25.0]),
        ],
        cycles: Vec::new(),
    };
    let hole = Toolpath {
        moves: vec![
            rapid([8.0, 6.0, 5.0]),
            rapid([8.0, 6.0, 1.0]),
            line([8.0, 6.0, -4.0], 100.0),
            rapid([8.0, 6.0, 5.0]),
        ],
        cycles: vec![DrillCycle {
            first: 1,
            count: 3,
            kind: CycleKind::Drill,
            holes: vec![[8.0, 6.0]],
            clearance: 5.0,
            retract: 1.0,
            bottom: -4.0,
            peck: 0.0,
            feed: 100.0,
        }],
    };
    let tilted = Toolpath {
        moves: vec![
            rapid([0.0, 0.0, 30.0]),
            rapid([0.0, 0.0, 22.0]),
            line([0.0, 0.0, 15.0], 80.0),
            rapid([0.0, 0.0, 30.0]),
        ],
        cycles: Vec::new(),
    };
    let mut b_hole = live_op(
        "B hole",
        11,
        4.0,
        3000.0,
        LiveSide::Plane { c: 90.0, b: 45.0 },
        tilted,
    );
    if let Some(live) = &mut b_hole.live {
        live.head = true;
    }
    let mut program = TurningProgram::new("Axes", [50.0, 50.0]);
    program.operations = vec![
        live_op(
            "Y flat",
            9,
            6.0,
            2500.0,
            LiveSide::Plane { c: 30.0, b: 90.0 },
            flat,
        ),
        live_op(
            "Y hole",
            10,
            5.0,
            2000.0,
            LiveSide::Plane { c: 0.0, b: 0.0 },
            hole,
        ),
        b_hole,
    ];
    program
}

#[test]
fn a_plane_s_frame_turns_with_c_and_b() {
    // B at 90: x towards the chuck, y the Y axis, z the machine's X.
    let flat = LiveSide::Plane { c: 0.0, b: 90.0 };
    let w = flat.to_work([2.0, 3.0, 18.0]);
    assert!(
        (w[0] - 18.0).abs() < 1e-12 && (w[1] - 3.0).abs() < 1e-12 && (w[2] + 2.0).abs() < 1e-12
    );
    // C turns it about the spindle axis.
    let turned = LiveSide::Plane { c: 90.0, b: 90.0 }.to_work([0.0, 0.0, 18.0]);
    assert!(turned[0].abs() < 1e-12 && (turned[1] - 18.0).abs() < 1e-12);
    // From the work back to the plane.
    let side = LiveSide::Plane { c: 30.0, b: 45.0 };
    let q = [1.5, -2.0, 7.0];
    let back = side.from_work(side.to_work(q));
    assert!((0..3).all(|k| (back[k] - q[k]).abs() < 1e-12), "{back:?}");
    // The tool's axis is the plane's z.
    let a = side.tool_axis(q);
    let z = side.to_work([0.0, 0.0, 1.0]);
    assert!((0..3).all(|k| (a[k] - z[k]).abs() < 1e-12));
    assert_eq!(
        LiveSide::Plane { c: 0.0, b: 0.0 }.plane_kind(),
        Some(PlaneKind::Face)
    );
    assert_eq!(flat.plane_kind(), Some(PlaneKind::Side));
    assert_eq!(side.plane_kind(), Some(PlaneKind::Tilted));
    assert_eq!(LiveSide::Face.plane_kind(), None);
}

/// The reference program: the C axis indexed and clamped at each plane,
/// G19 for the flat (X the depth as a diameter, Z the length along it),
/// G17 for the face with the Y axis, the tilted working plane for the B
/// head; everything off again at the end.
#[test]
fn y_and_b_axis_planes_post_as_fanuc_code() {
    let program = axes_program();
    program.validate().unwrap();
    let text = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap();
    let (_, body) = text.split_once("G54\n").expect("the header's work offset");
    let expected = "\
(Y FLAT)
G0 X100. Z50.
T0909
M154
G28 H0.
G98
M133 P2500
M8
G0 C30.
M14
G19
G0 Y0. Z0.
X50.
X42.
G1 X36. F200.
Y8. F400.
Z-10.
G0 X50.
G18
(Y HOLE)
M9
M135
G0 X100. Z50.
T1010
M133 P2000
M8
M15
G0 C0.
M14
G17
G0 X16. Y6.
Z5.
Z1.
G1 Z-4. F100.
G0 Z5.
G18
(B HOLE)
M9
M135
G0 X100. Z50.
T1111
M133 P3000
M8
M15
G0 C90.
M14
M108
G68.2 X0. Y0. Z0. I90. J45. K-90.
G53.1
M107
G0 Z30.
X0. Y0.
Z22.
G1 Z15. F80.
G0 Z30.
G69
G18
M9
M135
M15
M155
G99
G0 X100. Z50.
M30
%
";
    assert_eq!(body, expected);

    // Read back: the flat's cuts on its plane (X halved), the face's with
    // Y, the tilted plane's in its own frame.
    let read = check_program(&text).unwrap();
    let flat: Vec<&check::LiveRead> = read
        .live_moves
        .iter()
        .filter(|m| m.mode == check::LiveMode::Plane && m.motion == 1)
        .collect();
    assert_eq!(flat.len(), 4, "{:?}", read.live_moves);
    assert!(flat[0].x == 18.0 && flat[0].y == 0.0 && flat[0].z == 0.0 && flat[0].c == 30.0);
    assert!(flat[1].y == 8.0 && flat[1].feed == Some(400.0));
    assert!(flat[2].z == -10.0);
    assert!(flat[3].x == 8.0 && flat[3].y == 6.0 && flat[3].z == -4.0 && flat[3].c == 0.0);
    let tilted: Vec<&check::LiveRead> = read
        .live_moves
        .iter()
        .filter(|m| m.mode == check::LiveMode::Tilted)
        .collect();
    assert_eq!(tilted.len(), 4, "{:?}", read.live_moves);
    assert!(
        tilted
            .iter()
            .any(|m| m.motion == 1 && m.z == 15.0 && m.x == 0.0)
    );
}

/// With cycles: G83 for the off-centre hole with X and Y, the C axis
/// clamped, and no G83 for the tilted plane's path of moves.
#[test]
fn a_hole_on_the_face_with_y_posts_as_g83() {
    let program = axes_program();
    let settings = PostSettings {
        canned_cycles: true,
        ..PostSettings::default()
    };
    let text = turning_post(&program, &FanucTurning, &settings).unwrap();
    assert!(
        text.contains("G17\nG0 X16. Y6.\nZ5.\nG83 X16. Y6. Z-4. R-4. F100.\nG80\nG18\n(B HOLE)\n"),
        "{text}"
    );
    let read = check_program(&text).unwrap();
    let kinds: Vec<&str> = read.cycles.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(kinds, vec!["G83"]);
}

#[test]
fn the_program_check_refuses_what_y_b_and_the_sub_spindle_would_misread() {
    let base = "%\nO1000\nG21 G18 G40 G99\nG54\nG0 X100. Z50.\nT0505\nM154\nG98\nM133 P2000\n";
    let refuse = |body: &str| {
        check_program(&format!("{base}{body}M30\n%\n"))
            .unwrap_err()
            .message
    };
    assert_eq!(
        refuse("G17\nG0 C0.\nG68.2 X0. Y0. Z0. I90. J45.\n"),
        "Line 12: G68.2 needs its K word."
    );
    assert_eq!(
        refuse("G53.1\n"),
        "Line 10: G53.1 turns the tool to a tilted working plane, and there is none."
    );
    assert_eq!(
        refuse("G68.2 X0. Y0. Z0. I90. J45. K-90.\nG53.1\n"),
        "Line 12: the program ends in the tilted working plane."
    );
    assert_eq!(
        refuse("G68.2 X0. Y0. Z0. I90. J45. K-90.\nG53.1\nG0 Z30.\nG1 X5. F100.\n"),
        "Line 13: a cut from where the tool is not known."
    );
    assert_eq!(
        refuse("G0 A10. X5.\n"),
        "Line 10: A moves with the tool's axes here, which the program must not."
    );
    assert_eq!(
        refuse("G0 Y5\n"),
        "Line 10: Y needs a decimal point, or the control reads it in its smallest steps."
    );
    let turning =
        "%\nO1000\nG21 G18 G40 G99\nG54\nG0 X100. Z50.\nT0101\nG97 S500 M3\nG0 X20. Y5.\nM30\n%\n";
    assert_eq!(
        check_program(turning).unwrap_err().message,
        "Line 8: a move with the Y axis while the main spindle turns: stop it (M5) first."
    );
}

fn axes_lathe(y: bool, b: bool) -> Machine {
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
                y,
                b,
                sub_spindle: true,
            },
            chuck_diameter: 160.0,
            chuck_length: 50.0,
            ..Lathe::default()
        }),
    }
}

#[test]
fn the_lathe_check_needs_the_y_axis_and_the_b_head() {
    let program = axes_program();
    let placement = check::TurnPlacement {
        work_z: -200.0,
        chuck_z: None,
    };
    let messages = |m: &Machine| -> Vec<String> {
        check_lathe(m, &program, &placement)
            .into_iter()
            .map(|f| f.message)
            .collect()
    };
    let none = messages(&axes_lathe(false, false));
    assert!(
        none.contains(
            &"Y flat: the lathe has no Y axis to cut on a flat with the Y axis.".to_owned()
        ),
        "{none:?}"
    );
    assert!(none.contains(
        &"Y hole: the lathe has no Y axis to cut on the face with the Y axis.".to_owned()
    ));
    let no_b = messages(&axes_lathe(true, false));
    assert_eq!(
        no_b,
        vec!["B hole: the lathe has no B head to tilt the tool.".to_owned()]
    );
    assert!(messages(&axes_lathe(true, true)).is_empty());
    // The Y axis's travel: the flat's 8 mm across is beyond 5 mm.
    let mut short = axes_lathe(true, true);
    short.travel.min[1] = -5.0;
    short.travel.max[1] = 5.0;
    assert_eq!(
        messages(&short),
        vec![
            "Y flat: the tool goes beyond the machine's travel.".to_owned(),
            "Y hole: the tool goes beyond the machine's travel.".to_owned()
        ]
    );
    // A B head needs live tools; the sub-spindle's letter is its own.
    let mut lathe = Lathe::default();
    lathe.axes.y = true;
    assert!(lathe.validate().is_err());
    let mut lathe = axes_lathe(true, true).lathe.unwrap();
    lathe.mill_turn.sub_axis = "B".into();
    assert_eq!(
        lathe.validate().unwrap_err(),
        "The B head has the letter B: give the sub-spindle's axis A or E."
    );
    lathe.mill_turn.sub_axis = "W".into();
    assert!(lathe.validate().is_err());
}

/// A pin parted off into the sub-spindle and faced on its back.
fn hand_over_program() -> TurningProgram {
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
    let mut program = TurningProgram::new("Pin", [50.0, 50.0]);
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
    ];
    program
}

/// The reference program: the sub-spindle synchronised and gripping before
/// the part-off, pulled back after it, then the back on the sub-spindle in
/// its work offset with its own spindle codes.
#[test]
fn the_sub_spindle_hand_over_posts_as_fanuc_code() {
    let program = hand_over_program();
    program.validate().unwrap();
    assert_eq!(program.hand_over_at(), Some(0));
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
M9
X100. Z50.
M205
M30
%
";
    assert_eq!(body, expected);
    // Read back: the back's facing per revolution on the sub-spindle.
    let read = check_program(&text).unwrap();
    assert!(read.moves.iter().any(|m| m.motion
        == TurnMove::Line {
            to: [0.0, 0.0],
            feed: 0.1
        }));
    assert_eq!(read.css_lines.len(), 1);
}

#[test]
fn the_sub_spindle_s_operations_come_after_a_hand_over() {
    let mut program = hand_over_program();
    program.hand_over = None;
    assert_eq!(
        program.validate().unwrap_err().message,
        "Back face turns on the sub-spindle: the program needs a hand-over to it."
    );
    let mut program = hand_over_program();
    program.operations.swap(0, 1);
    assert_eq!(
        program.validate().unwrap_err().message,
        "Part off is on the main spindle after the hand-over: put it before the sub-spindle's operations."
    );
    // Without a part-off the sub-spindle grips before its first operation
    // and the main chuck lets the part go.
    let mut program = hand_over_program();
    program.operations.remove(0);
    assert_eq!(program.hand_over_at(), Some(0));
    let text = turning_post(&program, &FanucTurning, &PostSettings::default()).unwrap();
    assert!(
        text.contains("M78\nM69\nG0 A25.\nM212\nM5\nM205\nG55\n"),
        "{text}"
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

/// The part-off leaves the remnant on the main spindle; the back is faced
/// in the sub-spindle's coordinates, Z turned round at the part's back.
#[test]
fn the_turned_stock_follows_the_part_to_the_sub_spindle() {
    let part = pin();
    let program = hand_over_program();
    let sim = simulate_turning(&part, &program, -30.0, 0.1, 0.01);
    let at = |z: f64| {
        let i = sim
            .stock
            .z
            .iter()
            .position(|v| (v - z).abs() < 1e-6)
            .unwrap();
        (sim.stock.inner[i], sim.stock.outer[i])
    };
    // The remnant is gone, the part is as it was.
    assert_eq!(at(-26.0), (0.0, 0.0));
    assert_eq!(at(-10.0), (0.0, 12.0));
    assert!(sim.rapid_cuts.is_empty(), "{:?}", sim.rapid_cuts);
    // Turning round twice is no turn.
    let mut stock = sim.stock.clone();
    stock.turn_round(-20.0);
    assert!((stock.z[0] - (-20.0 - sim.stock.z[sim.stock.z.len() - 1])).abs() < 1e-9);
    stock.turn_round(-20.0);
    assert_eq!(stock.inner, sim.stock.inner);
    assert_eq!(stock.outer, sim.stock.outer);
}

/// Each plane is simulated on its own height map of the turned stock.
#[test]
fn the_planes_cut_their_own_maps() {
    let mut part = pin();
    part.outside = vec![PV::new(0.0, 0.0), PV::new(20.0, 0.0), PV::new(20.0, -20.0)];
    part.stock_radius = 20.0;
    let program = axes_program();
    let sims = simulate_live(&part, &program, -30.0, 0.1, 0.5, 200_000);
    let sides: Vec<LiveSide> = sims.iter().map(|s| s.side).collect();
    assert_eq!(
        sides,
        vec![
            LiveSide::Plane { c: 30.0, b: 90.0 },
            LiveSide::Plane { c: 0.0, b: 0.0 },
            LiveSide::Plane { c: 90.0, b: 45.0 }
        ]
    );
    for s in &sims {
        assert!(
            s.simulation.rapid_cuts.is_empty(),
            "{:?}",
            s.simulation.rapid_cuts
        );
    }
    // The flat's map: the bar's top 20 mm up across its middle, and the
    // flat milled at 18.
    let flat = &sims[0];
    let grid = flat.initial.grid;
    let node = |x: f64, y: f64| {
        let i = ((x - grid.origin[0]) / grid.step[0]).round() as usize;
        let j = ((y - grid.origin[1]) / grid.step[1]).round() as usize;
        grid.index(i, j)
    };
    assert!((flat.initial.z[node(5.0, 0.0)] - 20.0).abs() < 0.05);
    assert!((flat.simulation.stock.z[node(5.0, 8.0)] - 18.0).abs() < 1e-6);
    assert!(flat.simulation.removed > 0.0);
}

/// The whole program cut in 3D: the hole and the flat taken out of the
/// bar, nothing beyond the flat left, the surface a closed mesh.
#[test]
fn the_stock_is_cut_in_three_dimensions() {
    let mut part = pin();
    part.outside = vec![PV::new(0.0, 0.0), PV::new(20.0, 0.0), PV::new(20.0, -20.0)];
    part.stock_radius = 20.0;
    let program = axes_program();
    let sim = simulate_solid(&part, &program, -30.0, 0.1, 0.25, 400_000, None);
    let bar = std::f64::consts::PI * 400.0 * 30.0;
    assert!(
        (sim.initial - bar).abs() < 0.03 * bar,
        "{} {}",
        sim.initial,
        bar
    );
    assert!(sim.milled > 0.0);
    assert!((sim.voxels.volume() - (sim.initial - sim.milled)).abs() < 1e-6);
    assert!(sim.rapid_cuts.is_empty(), "{:?}", sim.rapid_cuts);
    // The flat at C 30: no material beyond 18 mm from the axis in its
    // direction where it was milled.
    let v = &sim.voxels;
    let dir = [30.0_f64.to_radians().cos(), 30.0_f64.to_radians().sin()];
    for k in 0..v.n[2] {
        for j in 0..v.n[1] {
            for i in 0..v.n[0] {
                let c = v.center(i, j, k);
                let along = c[0] * dir[0] + c[1] * dir[1];
                let across = -c[0] * dir[1] + c[1] * dir[0];
                let milled = (6.0..=10.0).contains(&across) && (-9.0..=-1.0).contains(&c[2]);
                if milled && along > 18.0 + v.step {
                    let index = (k * v.n[1] + j) * v.n[0] + i;
                    assert!(!v.filled[index], "material left at {c:?}");
                }
            }
        }
    }
    let mesh = v.mesh();
    assert!(!mesh.triangles.is_empty());
    // Closed: every edge runs as often one way as the other.
    let mut edges = std::collections::HashMap::new();
    for t in &mesh.triangles {
        for k in 0..3 {
            *edges.entry((t[k], t[(k + 1) % 3])).or_insert(0) += 1;
        }
    }
    assert!(
        edges
            .iter()
            .all(|(&(a, b), n)| edges.get(&(b, a)) == Some(n))
    );
}

#[test]
fn a_tilted_plane_needs_the_b_head_tool() {
    let mut program = axes_program();
    if let Some(live) = &mut program.operations[2].live {
        live.head = false;
    }
    assert_eq!(
        program.validate().unwrap_err().message,
        "B hole: a tilted plane needs the tool in the B head."
    );
}
