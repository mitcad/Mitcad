// SPDX-License-Identifier: MIT
//! Collisions of the spindle assembly along programs: a clear path, a nut
//! that reaches the stock at depth, a rapid through the stock, a zone and
//! a fixture's clamp, with the clearance in each.

use serde_json::json;

use super::*;
use crate::post::{dialect, post_with_lines};
use crate::program::ProgramOp;
use crate::spindle::{SpindleModel, SpindlePart};
use crate::tool::Tool;
use crate::toolpath::Move;

/// A 6 mm end mill with 15 mm flutes, 20 mm out of the collet.
fn tool() -> Tool {
    let mut t = Tool::flat_end_mill(1, 6.0);
    t.flute_length = 15.0;
    t.length = 20.0;
    t
}

/// A collet nut of 30 mm, 20 mm long, under a spindle of 80 mm.
fn spindle() -> SpindleModel {
    let part = |name: &str, kind: PartKind, f: Frustum| SpindlePart {
        name: name.into(),
        kind,
        cutting: false,
        shape: PartShape::Revolved { sections: vec![f] },
        margin: 0.0,
    };
    SpindleModel {
        parts: vec![
            part("Nut", PartKind::Nut, Frustum::cylinder(0.0, 20.0, 15.0)),
            part(
                "Spindle",
                PartKind::Spindle,
                Frustum::cylinder(20.0, 200.0, 40.0),
            ),
        ],
    }
}

/// 100 x 60 x 30, its top at Z 0.
fn stock() -> Obstacle {
    Obstacle::stock_box(Bounds {
        min: [0.0, 0.0, -30.0],
        max: [100.0, 60.0, 0.0],
    })
}

fn program(name: &str, safe_z: f64, moves: Vec<Move>) -> Program {
    let op: ProgramOp = serde_json::from_value(json!({
        "name": name,
        "tool": tool(),
        "spindle_rpm": 18000.0,
        "toolpath": {"moves": moves},
    }))
    .unwrap();
    let mut p = Program::new("Test", safe_z);
    p.operations.push(op);
    p
}

fn rapid(x: f64, y: f64, z: f64) -> Move {
    Move::Rapid { to: [x, y, z] }
}

fn line(x: f64, y: f64, z: f64) -> Move {
    Move::Line {
        to: [x, y, z],
        feed: 600.0,
    }
}

/// A closed box of twelve triangles facing out.
fn box_mesh(min: P3, max: P3) -> TriMesh {
    let corner = |i: usize| -> P3 {
        std::array::from_fn(|k| if (i >> k) & 1 == 1 { max[k] } else { min[k] })
    };
    let quads: [[u32; 4]; 6] = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    TriMesh {
        vertices: (0..8).map(corner).collect(),
        triangles: quads
            .iter()
            .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
            .collect(),
    }
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn a_clear_path_reports_its_clearance() {
    // Down 8 mm into the stock and along: the shank's end stays 7 mm
    // above the stock's top, the nut 12 mm; the cutter's rapids keep 10
    // mm above it.
    let p = program(
        "Pocket 1",
        15.0,
        vec![
            rapid(50.0, 30.0, 10.0),
            line(50.0, 30.0, -8.0),
            line(60.0, 30.0, -8.0),
            rapid(60.0, 30.0, 15.0),
        ],
    );
    let r = check_collisions(&p, &spindle(), &[stock()], &CollisionOptions::before_cut());
    assert!(r.collisions.is_empty(), "{:?}", r.collisions);
    let c = r.clearance.unwrap();
    assert!(near(c.distance, 7.0, 1e-6), "{c:?}");
    assert_eq!(c.part, PartKind::Shank);
    assert_eq!(c.operation, 0);
    assert!(near(c.at[2], -8.0, 1e-6), "{c:?}");
}

#[test]
fn the_nut_reaches_the_stock_at_depth() {
    // A plunge 3.5 mm off the stock's side, 25 mm deep with 20 mm of
    // stick-out: the shank keeps 0.5 mm off the wall, but the 30 mm nut
    // reaches over the stock and meets its top when the tip is 20 mm down.
    let p = program(
        "Contour 1",
        15.0,
        vec![
            rapid(-3.5, 30.0, 5.0),
            line(-3.5, 30.0, -25.0),
            line(-3.5, 50.0, -25.0),
            rapid(-3.5, 50.0, 15.0),
        ],
    );
    let mut r = check_collisions(&p, &spindle(), &[stock()], &CollisionOptions::before_cut());
    assert_eq!(r.collisions.len(), 1, "{:?}", r.collisions);
    let c = &r.collisions[0];
    assert_eq!(c.message, "Contour 1: the collet nut runs into the stock.");
    assert_eq!((c.operation, c.move_index, c.rapid), (0, 1, false));
    assert_eq!((c.part, c.obstacle), (PartKind::Nut, ObstacleKind::Stock));
    assert!(near(c.at[2], -20.0, 1e-6) && c.at[0] == -3.5, "{c:?}");
    assert_eq!(r.clearance.as_ref().unwrap().distance, 0.0);
    // The move's line in the posted program.
    let posted = post_with_lines(
        &p,
        dialect("grbl").unwrap().as_ref(),
        &crate::post::PostSettings::default(),
        None,
    )
    .unwrap();
    r.set_lines(&posted.moves);
    let n = r.collisions[0].line.unwrap();
    let text = posted.text.lines().nth(n - 1).unwrap();
    assert!(text.starts_with("G1 Z-25"), "line {n}: {text}");
}

#[test]
fn a_shank_that_touches_the_wall_is_a_collision() {
    // A plunge 3 mm off the wall, 18 mm deep: the cutter's side runs
    // along the wall (it cuts), and the shank above it, as wide, touches
    // the wall's edge as soon as its end comes down to the stock's top.
    let p = program(
        "Contour 1",
        15.0,
        vec![rapid(-3.0, 30.0, 5.0), line(-3.0, 30.0, -18.0)],
    );
    let r = check_collisions(&p, &spindle(), &[stock()], &CollisionOptions::before_cut());
    assert_eq!(r.collisions.len(), 1, "{:?}", r.collisions);
    let c = &r.collisions[0];
    assert_eq!(
        c.message,
        "Contour 1: the tool's shank runs into the stock."
    );
    assert_eq!((c.part, c.move_index), (PartKind::Shank, 1));
    assert!(near(c.at[2], -15.0, 1e-6), "{c:?}");
    assert_eq!(r.clearance.unwrap().distance, 0.0);
}

#[test]
fn a_rapid_through_the_stock_is_a_collision_for_the_cutter() {
    let p = program(
        "Pocket 1",
        15.0,
        vec![
            rapid(20.0, 30.0, 5.0),
            line(20.0, 30.0, -2.0),
            rapid(80.0, 30.0, -2.0),
            rapid(80.0, 30.0, 15.0),
        ],
    );
    let r = check_collisions(&p, &spindle(), &[stock()], &CollisionOptions::before_cut());
    assert_eq!(r.collisions.len(), 1, "{:?}", r.collisions);
    let c = &r.collisions[0];
    assert_eq!(
        c.message,
        "Pocket 1: a rapid move takes the cutter into the stock."
    );
    assert_eq!((c.move_index, c.rapid, c.part), (2, true, PartKind::Cutter));
    assert_eq!(c.at, [20.0, 30.0, -2.0]);
    assert_eq!(r.clearance.unwrap().distance, 0.0);
}

#[test]
fn a_drills_pecks_go_back_down_their_hole_fast() {
    // Back down to just above the last peck at a rapid, and straight up
    // out of the hole: only where the drill already cut.
    let p = program(
        "Drill 1",
        15.0,
        vec![
            rapid(10.0, 10.0, 2.0),
            line(10.0, 10.0, -3.0),
            rapid(10.0, 10.0, 2.0),
            rapid(10.0, 10.0, -2.5),
            line(10.0, 10.0, -6.0),
            rapid(10.0, 10.0, 2.0),
        ],
    );
    let r = check_collisions(&p, &spindle(), &[stock()], &CollisionOptions::before_cut());
    assert!(r.collisions.is_empty(), "{:?}", r.collisions);
    // The nearest is the drill coming down fast to 2 mm above the top
    // (the shank's end 6 mm deep is 9 mm above it).
    let c = r.clearance.unwrap();
    assert!(near(c.distance, 2.0, 1e-6), "{c:?}");
    assert_eq!((c.part, c.move_index), (PartKind::Cutter, 0));
}

#[test]
fn a_keep_out_zone_is_entered_by_the_nut() {
    // A tool setter 20 mm right of the stock, standing 30 mm above its top.
    let machine: Machine = serde_json::from_value(json!({
        "name": "Router",
        "controller": "grblhal",
        "travel": {"min": [0.0, 0.0, -200.0], "max": [800.0, 800.0, 0.0]},
        "spindle": {"min_rpm": 8000.0, "max_rpm": 24000.0,
                    "nose_diameter": 30.0, "nose_length": 20.0},
        "max_feed": 5000.0,
        "rapid_feed": 8000.0,
        "zones": [{"name": "the tool setter",
                   "bounds": {"min": [220.0, 120.0, -110.0], "max": [240.0, 140.0, -70.0]}}],
    }))
    .unwrap();
    let placement = Placement {
        work_origin: [100.0, 100.0, -100.0],
    };
    let zones = Obstacle::zones(&machine, &placement);
    assert_eq!(
        zones[0].shape,
        ObstacleShape::Box {
            bounds: Bounds {
                min: [120.0, 20.0, -10.0],
                max: [140.0, 40.0, 30.0],
            }
        }
    );
    // Along the stock's top 2 mm down, out past its side: the cutter
    // stops 2 mm short of the setter, the 30 mm nut does not.
    let p = program(
        "Face 1",
        15.0,
        vec![
            rapid(90.0, 30.0, 5.0),
            line(90.0, 30.0, -2.0),
            line(115.0, 30.0, -2.0),
            rapid(115.0, 30.0, 15.0),
        ],
    );
    let mut obstacles = vec![stock()];
    obstacles.extend(zones);
    let r = check_collisions(&p, &spindle(), &obstacles, &CollisionOptions::before_cut());
    assert_eq!(r.collisions.len(), 1, "{:?}", r.collisions);
    let c = &r.collisions[0];
    assert_eq!(
        c.message,
        "Face 1: the collet nut runs into the tool setter."
    );
    assert_eq!((c.obstacle, c.obstacle_index), (ObstacleKind::Zone, 1));
    assert_eq!(c.move_index, 2);
    // The nut's edge meets the setter's side at x 120.
    assert!(near(c.at[0], 105.0, 1e-6), "{c:?}");
    assert_eq!(r.clearance.unwrap().distance, 0.0);
}

#[test]
fn a_rapid_across_runs_the_cutter_into_a_clamp() {
    // A clamp in front of the stock, 15 mm above its top; a rapid across
    // it 5 mm above the top: the cutter's side meets it at y -5.
    let clamp = Obstacle::fixture(
        "the front clamp",
        box_mesh([30.0, -25.0, -5.0], [50.0, -5.0, 15.0]),
    );
    let p = program(
        "Contour 2",
        30.0,
        vec![rapid(40.0, 30.0, 5.0), rapid(40.0, -40.0, 5.0)],
    );
    let r = check_collisions(
        &p,
        &spindle(),
        &[stock(), clamp],
        &CollisionOptions::before_cut(),
    );
    assert_eq!(r.collisions.len(), 1, "{:?}", r.collisions);
    let c = &r.collisions[0];
    assert_eq!(
        c.message,
        "Contour 2: the cutter runs into the front clamp."
    );
    assert_eq!(
        (c.obstacle, c.part, c.move_index),
        (ObstacleKind::Fixture, PartKind::Cutter, 1)
    );
    assert!(near(c.at[1], -2.0, 1e-6) && c.at[2] == 5.0, "{c:?}");
    // Before it, the closest was the cutter 5 mm over the stock (the shank
    // is higher); the report's is the contact.
    assert_eq!(r.clearance.unwrap().distance, 0.0);

    // Up at 25 mm the cutter clears the clamp by 10 mm, the nut by more.
    let high = program(
        "Contour 2",
        30.0,
        vec![rapid(40.0, 30.0, 25.0), rapid(40.0, -40.0, 25.0)],
    );
    let clamp = Obstacle::fixture(
        "the front clamp",
        box_mesh([30.0, -25.0, -5.0], [50.0, -5.0, 15.0]),
    );
    let r = check_collisions(&high, &spindle(), &[clamp], &CollisionOptions::before_cut());
    assert!(r.collisions.is_empty(), "{:?}", r.collisions);
    let c = r.clearance.unwrap();
    assert!(near(c.distance, 10.0, 1e-6), "{c:?}");
    assert_eq!(c.part, PartKind::Cutter);
}

#[test]
fn an_arc_keeps_its_clearance_within_the_tolerance() {
    // A full circle of radius 20 about (50, 30) from its left, 2 mm deep:
    // at its right the nut's edge is 1 mm short of a zone at x 86.
    let zone = Obstacle {
        kind: ObstacleKind::Zone,
        name: "the dust shoe bracket".into(),
        shape: ObstacleShape::Box {
            bounds: Bounds {
                min: [86.0, -50.0, -50.0],
                max: [120.0, 120.0, 30.0],
            },
        },
    };
    let p = program(
        "Contour 3",
        15.0,
        vec![
            rapid(30.0, 30.0, 5.0),
            line(30.0, 30.0, -2.0),
            Move::Arc {
                to: [30.0, 30.0, -2.0],
                center: [50.0, 30.0],
                ccw: true,
                feed: 600.0,
            },
        ],
    );
    let options = CollisionOptions::default();
    let r = check_collisions(&p, &spindle(), &[zone], &options);
    assert!(r.collisions.is_empty(), "{:?}", r.collisions);
    let c = r.clearance.unwrap();
    assert!(
        c.distance <= 1.0 + 1e-9 && c.distance >= 1.0 - options.arc_tolerance - 1e-9,
        "{c:?}"
    );
    assert_eq!((c.part, c.move_index), (PartKind::Nut, 2));
}

#[test]
fn a_point_inside_a_closed_mesh_is_inside() {
    let faces = box_mesh([0.0; 3], [10.0; 3]).faces();
    assert!(inside([5.0, 5.0, 5.0], &faces));
    assert!(!inside([15.0, 5.0, 5.0], &faces));
    assert!(box_mesh([0.0; 3], [1.0; 3]).closed());
}

// The stock as cut (`StockModel::AsCut`, the default): the tests above
// take it as before the cut.

fn operation(name: &str, tool: Tool, moves: Vec<Move>) -> ProgramOp {
    ProgramOp {
        name: name.into(),
        tool,
        spindle_rpm: 18000.0,
        spindle: crate::program::SpindleDirection::Cw,
        coolant: crate::program::Coolant::Off,
        toolpath: crate::toolpath::Toolpath {
            moves,
            cycles: vec![],
        },
    }
}

#[test]
fn the_nut_passes_through_material_already_cut_away() {
    // A 40 mm end mill clears a slot 20 mm deep (10 to 90 in X, 10 to 50
    // in Y); then the 6 mm end mill goes 5 mm deeper in it, its nut 5 mm
    // below the stock's top but over the slot's floor.
    let mut big = Tool::flat_end_mill(2, 40.0);
    big.flute_length = 25.0;
    big.length = 30.0;
    let mut p = Program::new("Test", 15.0);
    p.operations.push(operation(
        "Slot 1",
        big,
        vec![
            rapid(30.0, 30.0, 5.0),
            line(30.0, 30.0, -20.0),
            line(70.0, 30.0, -20.0),
            rapid(70.0, 30.0, 15.0),
        ],
    ));
    p.operations.push(operation(
        "Pocket 1",
        tool(),
        vec![
            rapid(50.0, 30.0, 5.0),
            line(50.0, 30.0, -25.0),
            line(60.0, 30.0, -25.0),
            rapid(60.0, 30.0, 15.0),
        ],
    ));
    // Before the cut, the nut runs into the stock's top.
    let before = check_collisions(&p, &spindle(), &[stock()], &CollisionOptions::before_cut());
    assert!(
        before
            .collisions
            .iter()
            .any(|c| c.operation == 1 && c.part == PartKind::Nut),
        "{:?}",
        before.collisions
    );
    // As cut, the slot holds no material there.
    let r = check_collisions(&p, &spindle(), &[stock()], &CollisionOptions::default());
    assert!(r.collisions.is_empty(), "{:?}", r.collisions);
    let c = r.clearance.unwrap();
    assert!(c.distance > 1.0, "{c:?}");
}

#[test]
fn as_cut_a_rapid_through_the_stock_is_still_a_collision() {
    let p = program(
        "Pocket 1",
        15.0,
        vec![
            rapid(20.0, 30.0, 5.0),
            line(20.0, 30.0, -2.0),
            rapid(80.0, 30.0, -2.0),
            rapid(80.0, 30.0, 15.0),
        ],
    );
    let r = check_collisions(&p, &spindle(), &[stock()], &CollisionOptions::default());
    assert_eq!(r.collisions.len(), 1, "{:?}", r.collisions);
    let c = &r.collisions[0];
    assert_eq!(
        c.message,
        "Pocket 1: a rapid move takes the cutter into the stock."
    );
    assert_eq!((c.move_index, c.rapid, c.part), (2, true, PartKind::Cutter));
    // Just past the hole it plunged.
    assert!(c.at[0] > 20.0 && c.at[0] < 21.0 && c.at[2] == -2.0, "{c:?}");
    assert_eq!(r.clearance.unwrap().distance, 0.0);
}

#[test]
fn as_cut_a_drills_pecks_go_back_down_their_hole_fast() {
    let p = program(
        "Drill 1",
        15.0,
        vec![
            rapid(10.0, 10.0, 2.0),
            line(10.0, 10.0, -3.0),
            rapid(10.0, 10.0, 2.0),
            rapid(10.0, 10.0, -2.5),
            line(10.0, 10.0, -6.0),
            rapid(10.0, 10.0, 2.0),
        ],
    );
    let r = check_collisions(&p, &spindle(), &[stock()], &CollisionOptions::default());
    assert!(r.collisions.is_empty(), "{:?}", r.collisions);
    let c = r.clearance.unwrap();
    assert!(near(c.distance, 2.0, 1e-6), "{c:?}");
    assert_eq!((c.part, c.move_index), (PartKind::Cutter, 0));
}

#[test]
fn as_cut_the_shank_still_meets_a_wall_deeper_than_the_flutes() {
    // 18 mm down beside the stock's side with 15 mm of flutes: the shank
    // touches the wall's edge as it does before the cut.
    let p = program(
        "Contour 1",
        15.0,
        vec![rapid(-3.0, 30.0, 5.0), line(-3.0, 30.0, -18.0)],
    );
    let r = check_collisions(&p, &spindle(), &[stock()], &CollisionOptions::default());
    assert!(
        r.collisions
            .iter()
            .any(|c| c.part == PartKind::Shank && c.move_index == 1),
        "{:?}",
        r.collisions
    );
}
