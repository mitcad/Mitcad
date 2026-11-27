// SPDX-License-Identifier: MIT
//! Finishing into undercuts (mitcad#2503, GitHub mitcad/Mitcad#707): a
//! 30 mm bar necked to 26 mm behind a 45° wall and a bore with a 26 mm
//! recess behind one, each finished with Don't plunge (0°), by a tool's
//! geometry (its back angle less 2°) and Free (90°); an undercut deeper
//! than the insert cuts refused with its depth and the insert's; the depth
//! of a recess turned as one arc measured between its vertices; the
//! simulation's trailing edge and the material left under the line; the
//! post writing a plunging finish move by move, and G70 only over the
//! roughing's own contour.

use super::*;
use crate::post::PostSettings;

/// Don't plunge, Tool geometry with a VNMG insert in an SVJBR holder (a
/// back angle of 52°, less 2°), and Free.
const NONE: f64 = 0.0;
const TOOL: f64 = 50.0;
const FREE: f64 = 90.0;

fn turning_tool(number: u32, back_angle: Option<f64>, max_depth: f64) -> TurningTool {
    TurningTool {
        number,
        offset: 0,
        name: "Finishing tool".into(),
        kind: TurningToolKind::Turning,
        nose_radius: 0.4,
        max_depth,
        back_angle,
    }
}

fn boring_bar(back_angle: Option<f64>) -> TurningTool {
    TurningTool {
        number: 2,
        offset: 0,
        name: "Boring bar".into(),
        kind: TurningToolKind::Boring { bar_diameter: 6.0 },
        nose_radius: 0.4,
        max_depth: 4.0,
        back_angle,
    }
}

/// A 30 mm bar necked to 26 mm: down a 45° wall from z -10 to -12, along
/// the neck to z -25, straight up again, on to z -40.
fn neck(stock_radius: f64) -> TurnPart {
    TurnPart {
        outside: vec![
            PV::new(0.0, 0.0),
            PV::new(15.0, 0.0),
            PV::new(15.0, -10.0),
            PV::new(13.0, -12.0),
            PV::new(13.0, -25.0),
            PV::new(15.0, -25.0),
            PV::new(15.0, -40.0),
        ],
        inside: vec![],
        stock_radius,
        stock_inner_radius: 0.0,
        stock_face: 0.0,
    }
}

/// A 20 mm bore with a 26 mm recess: out along a 45° wall from z -10 to
/// -13, along the recess to z -25, straight in again, on to z -40; in a
/// 40 mm tube of 16 mm bore.
fn recessed_bore() -> TurnPart {
    TurnPart {
        outside: vec![PV::new(0.0, 0.0), PV::new(20.0, 0.0), PV::new(20.0, -40.0)],
        inside: vec![
            PV::new(10.0, 0.0),
            PV::new(10.0, -10.0),
            PV::new(13.0, -13.0),
            PV::new(13.0, -25.0),
            PV::new(10.0, -25.0),
            PV::new(10.0, -40.0),
        ],
        stock_radius: 20.0,
        stock_inner_radius: 8.0,
        stock_face: 0.0,
    }
}

fn finish_op(side: TurnSide, plunge: f64) -> FinishOp {
    FinishOp {
        side,
        clearance: 1.0,
        feed: 0.1,
        plunge,
        roughed: false,
    }
}

/// Points of the feed moves between z -1 and -39 (away from the face and
/// the part's end): along each line, and each arc's end.
fn cut_points(path: &TurnPath) -> Vec<PXZ> {
    let mut out = Vec::new();
    let mut from: Option<PXZ> = None;
    for m in &path.moves {
        let to = m.to();
        match (from, m) {
            (Some(a), TurnMove::Line { .. }) => {
                for k in 0..=20 {
                    let u = k as f64 / 20.0;
                    out.push([a[0] + u * (to[0] - a[0]), a[1] + u * (to[1] - a[1])]);
                }
            }
            (_, TurnMove::Rapid { .. }) => {}
            _ => out.push(to),
        }
        from = Some(to);
    }
    out.retain(|p| p[1] < -1.0 && p[1] > -39.0);
    out
}

fn has_point(path: &TurnPath, at: PXZ, tolerance: f64) -> bool {
    path.moves
        .iter()
        .any(|m| (m.to()[0] - at[0]).abs() < tolerance && (m.to()[1] - at[1]).abs() < tolerance)
}

/// The largest Z where the tip runs on a radius: where the contour first
/// reaches the floor of the undercut.
fn first_at(path: &TurnPath, x: f64) -> f64 {
    path.moves
        .iter()
        .map(|m| m.to())
        .filter(|p| (p[0] - x).abs() < 1e-6)
        .map(|p| p[1])
        .fold(f64::MIN, f64::max)
}

/// The neck in each mode. Don't plunge bridges it at 30 mm, as before.
/// Tool geometry (50°) goes down the 45° wall, the tip reaching the 26 mm
/// floor 0.566 past the wall's foot (the 0.4 nose on the wall), and along
/// the neck to its far wall; Free does the same, as the wall is no steeper.
/// A tool whose back angle allows only 30° leaves the wall along a 30°
/// line to the floor, where Free still follows the wall.
#[test]
fn the_neck_is_finished_in_each_plunge_mode() {
    let part = neck(16.0);
    let tool = turning_tool(1, Some(52.0), 4.0);
    let finished =
        |plunge: f64| finish(&part, &finish_op(TurnSide::Outside, plunge), &tool).unwrap();
    let lowest = |path: &TurnPath| {
        cut_points(path)
            .iter()
            .map(|p| p[0])
            .fold(f64::MAX, f64::min)
    };

    let bridged = finished(NONE);
    assert!(lowest(&bridged) >= 15.0 - 1e-6, "{:?}", bridged.moves);

    let into = finished(TOOL);
    assert!((lowest(&into) - 13.0).abs() < 1e-6, "{:?}", into.moves);
    let foot = -12.0 - 0.4 * std::f64::consts::SQRT_2;
    assert!(has_point(&into, [13.0, foot], 1e-4), "{:?}", into.moves);
    assert!(has_point(&into, [13.0, -25.0], 1e-6), "{:?}", into.moves);

    let free = finished(FREE);
    assert_eq!(free.moves, into.moves);

    // A back angle of 32°: 30° down from the wall's top. The 2 mm drop
    // takes 3.46 along Z: the floor is reached past the wall's foot.
    let shallow = finished(30.0);
    assert!((lowest(&shallow) - 13.0).abs() < 1e-6);
    let on_floor = first_at(&shallow, 13.0);
    assert!(on_floor < -13.0 && on_floor > -14.5, "{on_floor}");
    assert!(first_at(&free, 13.0) > -12.6, "{:?}", free.moves);
}

/// The bore in each mode: the same as the outside seen in a mirror. Don't
/// plunge stays in the 20 mm bore; Tool geometry and Free go out along the
/// 45° wall into the 26 mm recess and along it; at 30° the recess is
/// reached along a 30° line from the wall's top.
#[test]
fn a_bore_is_finished_in_each_plunge_mode() {
    let part = recessed_bore();
    let bar = boring_bar(Some(52.0));
    let finished = |plunge: f64| finish(&part, &finish_op(TurnSide::Inside, plunge), &bar).unwrap();
    let widest = |path: &TurnPath| {
        cut_points(path)
            .iter()
            .map(|p| p[0])
            .fold(f64::MIN, f64::max)
    };

    let bridged = finished(NONE);
    assert!(
        (widest(&bridged) - 10.0).abs() < 1e-6,
        "{:?}",
        bridged.moves
    );

    let into = finished(TOOL);
    assert!((widest(&into) - 13.0).abs() < 1e-6, "{:?}", into.moves);
    let foot = -13.0 - 0.4 * std::f64::consts::SQRT_2;
    assert!(has_point(&into, [13.0, foot], 1e-4), "{:?}", into.moves);
    assert!(has_point(&into, [13.0, -25.0], 1e-6), "{:?}", into.moves);
    assert_eq!(finished(FREE).moves, into.moves);

    // The 3 mm step along a 30° line takes 5.2 along Z.
    let shallow = finished(30.0);
    let on_floor = first_at(&shallow, 13.0);
    assert!(on_floor < -14.5 && on_floor > -16.5, "{on_floor}");
}

/// An undercut deeper than the insert's depth of cut is refused with the
/// depth measured (radius) and the insert's limit; Don't plunge does not
/// go into it and is not refused.
#[test]
fn an_undercut_deeper_than_the_insert_cuts_is_refused() {
    let part = neck(16.0);
    let shallow_insert = turning_tool(1, Some(52.0), 1.0);
    for plunge in [TOOL, FREE] {
        let e = finish(
            &part,
            &finish_op(TurnSide::Outside, plunge),
            &shallow_insert,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(
            e,
            "The undercut is 2 mm deep (radius), 1 mm deeper than the insert's depth of cut (1 mm): rough it first with a Turn Roughing that plunges, finish it with Don't plunge, or with an insert that cuts that deep."
        );
        // Where: the wall's foot, for the refusal's place and the agents'
        // detail, never in the message.
        let deep = ops::too_deep(
            &part,
            &finish_op(TurnSide::Outside, plunge),
            &shallow_insert,
        )
        .unwrap()
        .unwrap();
        assert_eq!(deep.at, [13.0, -12.0]);
        assert!(
            deep.detail()
                .starts_with("The undercut at Z -12 is 2 mm deep")
        );
    }
    assert_eq!(
        ops::too_deep(&part, &finish_op(TurnSide::Outside, NONE), &shallow_insert).unwrap(),
        None
    );
    assert!(finish(&part, &finish_op(TurnSide::Outside, NONE), &shallow_insert).is_ok());
    // A bore's recess likewise: 3 mm out from the 20 mm bore.
    let bar = TurningTool {
        max_depth: 2.5,
        ..boring_bar(Some(52.0))
    };
    let e = finish(&recessed_bore(), &finish_op(TurnSide::Inside, FREE), &bar)
        .unwrap_err()
        .to_string();
    assert!(
        e.starts_with("The undercut is 3 mm deep (radius), 0.5 mm deeper than the insert's depth of cut (2.5 mm)"),
        "{e}"
    );
}

/// A recess turned as one arc between two vertices, 10 long and 2 deep at
/// its middle (z -15), where the profile has no vertex: its depth is
/// measured along the arc, refused for a 1.5 mm insert, and with a 2.5 mm
/// one the tip follows the arc (its centre moved by the nose to the tip).
#[test]
fn a_curved_undercut_is_measured_between_its_vertices() {
    let part = TurnPart {
        outside: vec![
            PV::new(0.0, 0.0),
            PV::new(15.0, 0.0),
            PV::arc(15.0, -10.0, -0.4),
            PV::new(15.0, -20.0),
            PV::new(15.0, -30.0),
        ],
        inside: vec![],
        stock_radius: 16.0,
        stock_inner_radius: 0.0,
        stock_face: 0.0,
    };
    let e = finish(
        &part,
        &finish_op(TurnSide::Outside, FREE),
        &turning_tool(1, None, 1.5),
    )
    .unwrap_err()
    .to_string();
    assert!(
        e.starts_with("The undercut is 2 mm deep (radius), 0.5 mm deeper than the insert's depth of cut (1.5 mm)"),
        "{e}"
    );
    // Deepest at the arc's middle, between its vertices.
    let deep = ops::too_deep(
        &part,
        &finish_op(TurnSide::Outside, FREE),
        &turning_tool(1, None, 1.5),
    )
    .unwrap()
    .unwrap();
    assert!(
        (deep.at[0] - 13.0).abs() < 1e-6 && (deep.at[1] + 15.0).abs() < 1e-6,
        "{deep:?}"
    );
    let tool = turning_tool(1, Some(52.0), 2.5);
    let arc_centre = [20.25 - 0.4, -15.0 - 0.4];
    let follows_the_arc = |path: &TurnPath| {
        path.moves.iter().any(|m| {
            matches!(m, TurnMove::Arc { center, cw: true, .. }
                if (center[0] - arc_centre[0]).abs() < 1e-6
                    && (center[1] - arc_centre[1]).abs() < 1e-6)
        })
    };
    for plunge in [TOOL, FREE] {
        let path = finish(&part, &finish_op(TurnSide::Outside, plunge), &tool).unwrap();
        assert!(follows_the_arc(&path), "{plunge}: {:?}", path.moves);
    }
    let bridged = finish(&part, &finish_op(TurnSide::Outside, NONE), &tool).unwrap();
    assert!(!follows_the_arc(&bridged), "{:?}", bridged.moves);
}

fn program_op(name: &str, tool: &TurningTool, path: TurnPath) -> TurningProgramOp {
    TurningProgramOp {
        name: name.into(),
        tool: tool.clone(),
        spindle: TurnSpindle::Css {
            surface_speed: 200.0,
            max_rpm: 3000.0,
        },
        direction: SpindleDirection::Cw,
        coolant: Coolant::Flood,
        path,
        live: None,
        sub: false,
    }
}

fn simulated(part: &TurnPart, tool: &TurningTool, path: &TurnPath) -> TurnedStock {
    let mut program = TurningProgram::new("Neck", [50.0, 100.0]);
    program.operations = vec![program_op("Finish", tool, path.clone())];
    simulate_turning(part, &program, -45.0, 0.05, 0.01)
}

/// The simulation draws the trailing edge from the tool's back angle. The
/// finish down the 45° wall cuts nothing of the part with a 52° tool and
/// leaves only the nose's fillets in the inner corners; with a 30° tool,
/// or a tool without its insert (its edge flat behind the nose), the
/// trailing edge cuts into the wall. The 30° finish leaves the wall's
/// foot under its line, 0.85 thick (radius) at z -12, and gouges nothing
/// with a 32° tool.
#[test]
fn the_simulation_sees_the_trailing_edge_and_what_is_left() {
    let part = neck(15.0);
    let tool = turning_tool(1, Some(52.0), 4.0);
    let path = finish(&part, &finish_op(TurnSide::Outside, TOOL), &tool).unwrap();
    let sim = simulated(&part, &tool, &path);
    assert!(sim.gouges.is_empty(), "{:?}", sim.gouges);
    assert!(sim.rapid_cuts.is_empty(), "{:?}", sim.rapid_cuts);
    assert!(sim.left < 0.41, "{} at {:?}", sim.left, sim.left_at);
    for back in [Some(30.0), None] {
        let other = turning_tool(1, back, 4.0);
        let sim = simulated(&part, &other, &path);
        assert!(!sim.gouges.is_empty(), "{back:?}");
    }

    let tool = turning_tool(1, Some(32.0), 4.0);
    let path = finish(&part, &finish_op(TurnSide::Outside, 30.0), &tool).unwrap();
    let sim = simulated(&part, &tool, &path);
    assert!(sim.gouges.is_empty(), "{:?}", sim.gouges);
    assert!(sim.left > 0.6 && sim.left < 0.9, "{}", sim.left);
    let at = sim.left_at.unwrap();
    assert!(at[1] > -12.6 && at[1] < -11.4, "{at:?}");
}

/// The moves the plain post writes: the change position at each new tool
/// and at the end, and the moves that move at the program's digits.
pub(super) fn written(program: &TurningProgram) -> Vec<TurnMove> {
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

/// Roughing the neck (bridged, a G71) and finishing it into the neck: the
/// finish's contour is not the roughing's, so it is written move by move,
/// no G70, the tip down to 26 mm; the plain post reads back as the
/// generated moves. Finished with Don't plunge it is a G70 again, and so is
/// a plunging finish of a part without an undercut (its contour is the
/// roughing's).
#[test]
fn a_plunging_finish_posts_its_moves_and_g70_only_on_the_roughing_contour() {
    let part = neck(16.0);
    let rougher = TurningTool {
        name: "Roughing tool".into(),
        ..turning_tool(1, None, 4.0)
    };
    let finisher = turning_tool(2, Some(52.0), 4.0);
    let rough_op = RoughOp {
        side: TurnSide::Outside,
        depth: 2.0,
        retract: 0.5,
        allowance_x: 0.2,
        allowance_z: 0.1,
        clearance: 1.0,
        feed: 0.25,
        plunge: 0.0,
    };
    let program_for = |part: &TurnPart, plunge: f64| {
        let mut program = TurningProgram::new("Neck", [50.0, 100.0]);
        program.operations = vec![
            program_op("Rough", &rougher, rough(part, &rough_op, &rougher).unwrap()),
            program_op(
                "Finish",
                &finisher,
                finish(part, &finish_op(TurnSide::Outside, plunge), &finisher).unwrap(),
            ),
        ];
        program
    };
    let cycles = PostSettings {
        canned_cycles: true,
        ..PostSettings::default()
    };

    let into = program_for(&part, TOOL);
    let text = turning_post(&into, &FanucTurning, &cycles).unwrap();
    assert!(text.contains("G71 "), "{text}");
    assert!(!text.contains("G70"), "{text}");
    // Down the wall to the 26 mm neck and along it, as the supplied
    // program has it.
    assert!(text.contains("X26. Z-12.566"), "{text}");
    let read = check_program(&text).unwrap();
    let kinds: Vec<&str> = read.cycles.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(kinds, vec!["G71"]);

    // Without cycles: the moves read back as generated.
    let plain = turning_post(&into, &FanucTurning, &PostSettings::default()).unwrap();
    let read = check_program(&plain).unwrap();
    let expected = written(&into);
    assert_eq!(read.moves.len(), expected.len(), "{plain}");
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
    }

    let bridged = program_for(&part, NONE);
    let text = turning_post(&bridged, &FanucTurning, &cycles).unwrap();
    assert_eq!(text.matches("G70 P").count(), 1, "{text}");

    // A pin without an undercut: plunging changes nothing, G70 it is.
    let pin = TurnPart {
        outside: vec![PV::new(0.0, 0.0), PV::new(10.0, 0.0), PV::new(10.0, -20.0)],
        inside: vec![],
        stock_radius: 15.0,
        stock_inner_radius: 0.0,
        stock_face: 0.0,
    };
    for plunge in [TOOL, FREE] {
        let text = turning_post(&program_for(&pin, plunge), &FanucTurning, &cycles).unwrap();
        assert_eq!(text.matches("G70 P").count(), 1, "{plunge}: {text}");
    }
}
