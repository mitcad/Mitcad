// SPDX-License-Identifier: MIT
//! Roughing into undercuts (mitcad#2549, GitHub mitcad/Mitcad#733): a
//! 30 mm bar necked to 26 mm behind a 45 degree wall, two pockets split by
//! a collar, a bore's recess and a recess turned as one arc, each roughed
//! with Plunge, then finished into it, both simulated: no gouge, no rapid
//! through stock, the floors at the stock to leave after roughing and only
//! the nose's fillets after finishing; the pockets one after the other;
//! what the roughings before a finish leave in its undercut; a plunging
//! roughing posted move by move, as generated, and one without a plunge
//! still a G71.

use super::*;
use crate::post::PostSettings;

/// Tool geometry with a VNMG insert in an SVJBR holder (a back angle of
/// 52 degrees, less 2), and Free.
const TOOL: f64 = 50.0;
const FREE: f64 = 90.0;

fn turning_tool(number: u32, max_depth: f64) -> TurningTool {
    TurningTool {
        number,
        offset: 0,
        name: "Turning tool".into(),
        kind: TurningToolKind::Turning,
        nose_radius: 0.4,
        max_depth,
        back_angle: Some(52.0),
    }
}

/// A 30 mm bar necked to 26 mm: down a 45 degree wall from z -10 to -12,
/// along the neck to z -25, straight up again, on to z -40; a 32 mm
/// stock.
fn neck() -> TurnPart {
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
        stock_radius: 16.0,
        stock_inner_radius: 0.0,
        stock_face: 0.0,
    }
}

/// Passes 1 mm deep, 0.2 to leave in X and 0.1 in Z (0.2 all round
/// into undercuts).
fn rough_op(side: TurnSide, plunge: f64) -> RoughOp {
    RoughOp {
        side,
        depth: 1.0,
        retract: 0.5,
        allowance_x: 0.2,
        allowance_z: 0.1,
        clearance: 1.0,
        feed: 0.25,
        plunge,
    }
}

fn finish_op(side: TurnSide, plunge: f64, roughed: bool) -> FinishOp {
    FinishOp {
        side,
        clearance: 1.0,
        feed: 0.1,
        plunge,
        roughed,
    }
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

/// The stock's surface (radius) at `z`: the outside's, or the bore's.
fn surface_at(sim: &TurnedStock, z: f64, inside: bool) -> f64 {
    let st = &sim.stock;
    let i =
        st.z.iter()
            .enumerate()
            .min_by(|a, b| (a.1 - z).abs().total_cmp(&(b.1 - z).abs()))
            .unwrap()
            .0;
    if inside { st.inner[i] } else { st.outer[i] }
}

/// Roughs and finishes a side plunging at `plunge` with one tool, and
/// simulates them: the roughing's path, the stock after the roughing and
/// after the whole program.
fn rough_and_finish(
    part: &TurnPart,
    side: TurnSide,
    tool: &TurningTool,
    plunge: f64,
) -> (TurnPath, TurnedStock, TurnedStock) {
    let rough_path = rough(part, &rough_op(side, plunge), tool).unwrap();
    // The finish after it: the roughing leaves its 0.2 in the undercut.
    let fin = finish_op(side, plunge, false);
    let roughings = [(rough_op(side, plunge), tool.clone())];
    assert_eq!(
        ops::too_deep_after(part, &fin, tool, &roughings).unwrap(),
        None
    );
    let finish_path = finish(part, &finish_op(side, plunge, true), tool).unwrap();
    let mut program = TurningProgram::new("Pockets", [50.0, 100.0]);
    program.operations = vec![program_op("Rough", tool, rough_path.clone())];
    let roughed = simulate_turning(part, &program, -45.0, 0.05, 0.01);
    program
        .operations
        .push(program_op("Finish", tool, finish_path));
    let finished = simulate_turning(part, &program, -45.0, 0.05, 0.01);
    (rough_path, roughed, finished)
}

fn assert_clean(sims: [&TurnedStock; 2]) {
    for sim in sims {
        assert!(sim.gouges.is_empty(), "{:?}", sim.gouges);
        assert!(sim.rapid_cuts.is_empty(), "{:?}", sim.rapid_cuts);
    }
}

/// The 2 mm neck with a 1.5 mm insert: roughed into it as moves (no
/// G71) to its stock to leave (13.2 on the floor), finished leaving only
/// the nose's fillets in the inside corners. Without the roughing the
/// finish refuses the undercut and says to rough it first.
#[test]
fn roughing_goes_down_into_a_neck_and_finishing_cleans_it() {
    let part = neck();
    let tool = turning_tool(1, 1.5);
    let (path, roughed, finished) = rough_and_finish(&part, TurnSide::Outside, &tool, TOOL);
    assert!(path.cycle.is_none(), "written as moves, not a G71");
    assert_clean([&roughed, &finished]);
    let floor = surface_at(&roughed, -18.0, false);
    assert!((floor - 13.2).abs() < 0.03, "{floor}");
    assert!(
        finished.left < 0.14,
        "{} at {:?}",
        finished.left,
        finished.left_at
    );
    let alone = finish(&part, &finish_op(TurnSide::Outside, TOOL, false), &tool);
    assert!(
        alone.unwrap_err().message.contains("rough it first"),
        "refused without the roughing"
    );
}

/// Two pockets split by a collar: the first reaches its floor (12.2)
/// before the second (11.2) is started, both at their stock to leave.
#[test]
fn roughing_takes_pockets_split_by_a_collar_one_after_the_other() {
    let part = TurnPart {
        outside: vec![
            PV::new(0.0, 0.0),
            PV::new(15.0, 0.0),
            PV::new(15.0, -5.0),
            PV::new(12.0, -8.0),
            PV::new(12.0, -14.0),
            PV::new(14.0, -14.0),
            PV::new(14.0, -18.0),
            PV::new(11.0, -21.0),
            PV::new(11.0, -28.0),
            PV::new(15.0, -28.0),
            PV::new(15.0, -40.0),
        ],
        inside: vec![],
        stock_radius: 16.0,
        stock_inner_radius: 0.0,
        stock_face: 0.0,
    };
    let tool = turning_tool(1, 4.0);
    let (path, roughed, finished) = rough_and_finish(&part, TurnSide::Outside, &tool, TOOL);
    assert_clean([&roughed, &finished]);
    for (z, bottom) in [(-11.0, 12.2), (-24.0, 11.2)] {
        let floor = surface_at(&roughed, z, false);
        assert!((floor - bottom).abs() < 0.03, "{z}: {floor}");
    }
    assert!(
        finished.left < 0.14,
        "{} at {:?}",
        finished.left,
        finished.left_at
    );
    // Depth first: the first pocket's last pass (at 13) comes before the
    // second's first (at 14, under the collar's top).
    let cuts: Vec<PXZ> = path
        .moves
        .iter()
        .filter(|m| matches!(m, TurnMove::Line { .. }))
        .map(|m| m.to())
        .collect();
    let first_floor = cuts
        .iter()
        .position(|p| p[0] < 13.1 && p[1] < -8.0 && p[1] > -14.0)
        .unwrap();
    let second_pocket = cuts
        .iter()
        .position(|p| p[0] < 14.1 && p[1] < -19.0 && p[1] > -27.5)
        .unwrap();
    assert!(first_floor < second_pocket, "{:?}", path.moves);
}

/// A 20 mm bore with a recess 24 mm across behind a 45 degree wall: the
/// mirror of the neck, roughed to 11.8 in the recess.
#[test]
fn roughing_into_a_bore_undercut_is_the_mirror() {
    let part = TurnPart {
        outside: vec![PV::new(0.0, 0.0), PV::new(20.0, 0.0), PV::new(20.0, -40.0)],
        inside: vec![
            PV::new(10.0, 0.0),
            PV::new(10.0, -10.0),
            PV::new(12.0, -12.0),
            PV::new(12.0, -20.0),
            PV::new(10.0, -20.0),
            PV::new(10.0, -30.0),
            PV::new(8.0, -30.0),
            PV::new(8.0, -40.0),
        ],
        stock_radius: 20.0,
        stock_inner_radius: 8.0,
        stock_face: 0.0,
    };
    let bar = TurningTool {
        kind: TurningToolKind::Boring { bar_diameter: 10.0 },
        name: "Boring bar".into(),
        ..turning_tool(3, 4.0)
    };
    let (path, roughed, finished) = rough_and_finish(&part, TurnSide::Inside, &bar, TOOL);
    assert!(path.cycle.is_none());
    assert_clean([&roughed, &finished]);
    let floor = surface_at(&roughed, -16.0, true);
    assert!((floor - 11.8).abs() < 0.03, "{floor}");
    assert!(
        finished.left < 0.14,
        "{} at {:?}",
        finished.left,
        finished.left_at
    );
}

/// A recess turned as one arc, 10 long and 2 deep at its middle (z -15),
/// with a 1.5 mm insert: roughed along the arc to 13.2 at its middle,
/// with Tool geometry and with Free, then finished.
#[test]
fn roughing_follows_a_curved_undercut() {
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
    let tool = turning_tool(1, 1.5);
    for plunge in [TOOL, FREE] {
        let (path, roughed, finished) = rough_and_finish(&part, TurnSide::Outside, &tool, plunge);
        assert!(path.cycle.is_none());
        assert_clean([&roughed, &finished]);
        let floor = surface_at(&roughed, -15.0, false);
        assert!((floor - 13.2).abs() < 0.03, "{plunge}: {floor}");
        assert!(
            finished.left < 0.14,
            "{plunge}: {} at {:?}",
            finished.left,
            finished.left_at
        );
    }
}

/// What the roughings before a finish into the neck leave, with a 1 mm
/// finishing insert (the neck is 2 mm deep): one plunging as far leaves
/// its 0.2; one plunging at 30 degrees leaves the wedge under its line
/// down the 45 degree wall (0.845 at the wall's foot) and its 0.2, too
/// much; a nose 0.8 bigger leaves 0.8 more in the corners; a roughing of
/// the bore, or one that fails, counts for nothing.
#[test]
fn what_the_roughings_leave_decides_a_deep_finish() {
    let part = neck();
    let finisher = turning_tool(2, 1.0);
    let rougher = turning_tool(1, 4.0);
    let fin = finish_op(TurnSide::Outside, TOOL, false);
    let after = |finisher: &TurningTool, roughings: &[(RoughOp, TurningTool)]| {
        ops::too_deep_after(&part, &fin, finisher, roughings).unwrap()
    };

    let deep = after(&finisher, &[]).unwrap();
    assert!(!deep.roughed);
    assert!((deep.depth - 2.0).abs() < 1e-9, "{deep:?}");
    assert_eq!(Some(deep), ops::too_deep(&part, &fin, &finisher).unwrap());

    let as_far = (rough_op(TurnSide::Outside, TOOL), rougher.clone());
    assert_eq!(after(&finisher, std::slice::from_ref(&as_far)), None);

    let shallow = (rough_op(TurnSide::Outside, 30.0), rougher.clone());
    let left = after(&finisher, std::slice::from_ref(&shallow)).unwrap();
    assert!(left.roughed);
    assert!((left.depth - 1.0453).abs() < 1e-3, "{left:?}");
    assert!(
        (left.at[0] - 13.0).abs() < 1e-6 && (left.at[1] + 12.0).abs() < 1e-6,
        "{left:?}"
    );
    assert_eq!(
        left.message(),
        "After the roughing before it, 1.045 mm (radius) is left in the undercut, 0.045 mm more than the insert's depth of cut (1 mm): let the roughing plunge as far as the finish or leave less stock, or finish it with Don't plunge."
    );
    assert!(
        left.detail()
            .starts_with("After the roughing before it, the undercut at Z -12 has 1.045 mm"),
        "{}",
        left.detail()
    );
    // The least any of them leaves counts.
    assert_eq!(after(&finisher, &[shallow.clone(), as_far.clone()]), None);

    // A 1.2 mm nose: 0.2 and 0.8 in the corners, just what the insert
    // cuts; not with a 0.9 mm insert.
    let big_nose = (
        rough_op(TurnSide::Outside, TOOL),
        TurningTool {
            nose_radius: 1.2,
            ..rougher.clone()
        },
    );
    assert_eq!(after(&finisher, std::slice::from_ref(&big_nose)), None);
    let thinner = turning_tool(2, 0.9);
    let left = after(&thinner, std::slice::from_ref(&big_nose)).unwrap();
    assert!(left.roughed && (left.depth - 1.0).abs() < 1e-9, "{left:?}");

    // The bore's roughing, and one whose passes are deeper than its
    // insert: the undercut's own depth again.
    let bore = (rough_op(TurnSide::Inside, TOOL), rougher.clone());
    let failing = (
        RoughOp {
            depth: 10.0,
            ..rough_op(TurnSide::Outside, TOOL)
        },
        rougher.clone(),
    );
    assert_eq!(after(&finisher, &[bore, failing]), Some(deep));
}

/// The neck roughed without a plunge and with one, then finished into it,
/// with cycles: only the roughing without a plunge is a G71, the plunging
/// roughing and the finish (its contour is not the G71's) are moves. A
/// program of the plunging roughing and the finish alone has no cycle,
/// and the plain post reads back as the generated moves.
#[test]
fn a_plunging_roughing_posts_its_moves_and_one_without_a_plunge_its_g71() {
    let part = neck();
    let tool = turning_tool(1, 4.0);
    // The finish with a tool of its own, as in a program the roughing's
    // and the finish's moves are apart by a tool change.
    let finisher = turning_tool(2, 4.0);
    let plain = rough(&part, &rough_op(TurnSide::Outside, 0.0), &tool).unwrap();
    assert!(plain.cycle.is_some());
    let into = rough(&part, &rough_op(TurnSide::Outside, TOOL), &tool).unwrap();
    assert!(into.cycle.is_none());
    let finishing = finish(&part, &finish_op(TurnSide::Outside, TOOL, true), &finisher).unwrap();
    let cycles = PostSettings {
        canned_cycles: true,
        ..PostSettings::default()
    };

    let mut mixed = TurningProgram::new("Neck", [50.0, 100.0]);
    mixed.operations = vec![
        program_op("Rough", &tool, plain),
        program_op("Into", &tool, into.clone()),
        program_op("Finish", &finisher, finishing.clone()),
    ];
    let text = turning_post(&mixed, &FanucTurning, &cycles).unwrap();
    assert_eq!(text.matches("G71 U").count(), 1, "{text}");
    assert!(!text.contains("G70"), "{text}");
    let read = check_program(&text).unwrap();
    let kinds: Vec<&str> = read.cycles.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(kinds, vec!["G71"]);

    let mut moves = TurningProgram::new("Neck", [50.0, 100.0]);
    moves.operations = vec![
        program_op("Into", &tool, into),
        program_op("Finish", &finisher, finishing),
    ];
    let text = turning_post(&moves, &FanucTurning, &cycles).unwrap();
    assert!(!text.contains("G71") && !text.contains("G70"), "{text}");
    let plain = turning_post(&moves, &FanucTurning, &PostSettings::default()).unwrap();
    let read = check_program(&plain).unwrap();
    let expected = super::plunge_tests::written(&moves);
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
}
