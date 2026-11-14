// SPDX-License-Identifier: MIT
//! The tool changes of a program: a macro changer's trip to the rack
//! through a zone, flagged at its line; the same trip clear, the rack
//! itself not counted; the travel after a manual change from the park
//! position, and from above the last cut without one.

use serde_json::json;

use super::*;
use crate::collision::Collision;
use crate::machine::{Bounds, MacroChanger, RackSlot};
use crate::post::{PostSettings, dialect, post_with_lines};
use crate::spindle::{Frustum, PartShape, SpindlePart};

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

/// An end mill `diameter` wide, `flutes` long, 20 mm out of the collet.
fn tool(number: u32, diameter: f64, flutes: f64) -> Tool {
    let mut t = Tool::flat_end_mill(number, diameter);
    t.flute_length = flutes;
    t.length = 20.0;
    t
}

fn op(name: &str, tool: Tool, moves: Vec<Move>) -> ProgramOp {
    serde_json::from_value(json!({
        "name": name,
        "tool": tool,
        "spindle_rpm": 18000.0,
        "toolpath": {"moves": moves},
    }))
    .unwrap()
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

/// A pocket cut with tool 1 from (10, 10) to (90, 50), then a hole with
/// tool 2 at (90, 50); the safe height 15 mm above the stock's top.
fn program() -> Program {
    let mut p = Program::new("Plate", 15.0);
    p.operations.push(op(
        "Pocket 1",
        tool(1, 6.0, 15.0),
        vec![
            rapid(10.0, 10.0, 5.0),
            line(10.0, 10.0, -2.0),
            line(90.0, 50.0, -2.0),
            rapid(90.0, 50.0, 15.0),
        ],
    ));
    p.operations.push(op(
        "Drill",
        tool(2, 4.0, 12.0),
        vec![
            rapid(90.0, 50.0, 5.0),
            line(90.0, 50.0, -5.0),
            rapid(90.0, 50.0, 15.0),
        ],
    ));
    p
}

// The work zero at (100, 100, -100) on the machine: the stock, 100 x 60 x
// 30, is at (100..200, 100..160) with its top at -100.
const AT: Placement = Placement {
    work_origin: [100.0, 100.0, -100.0],
};

fn zone(name: &str, min: P3, max: P3) -> serde_json::Value {
    json!({"name": name, "bounds": {"min": min, "max": max}})
}

fn machine(changer: ToolChanger, zones: Vec<serde_json::Value>, park: Option<P3>) -> Machine {
    let mut m: Machine = serde_json::from_value(json!({
        "name": "Router",
        "controller": "grbl",
        "travel": {"min": [0.0, 0.0, -200.0], "max": [800.0, 800.0, 0.0]},
        "spindle": {"min_rpm": 8000.0, "max_rpm": 24000.0,
                    "nose_diameter": 30.0, "nose_length": 20.0},
        "max_feed": 5000.0,
        "rapid_feed": 8000.0,
        "zones": zones,
        "park": park,
    }))
    .unwrap();
    m.tool_changer = changer;
    m
}

/// Tool 1's slot at the rack's left end, tool 2's at its right end, the
/// spindle moving to the rack with its gauge line at Z -5.
fn rack() -> ToolChanger {
    ToolChanger::Macro(MacroChanger {
        slots: vec![
            RackSlot {
                tool: 1,
                position: [20.0, 750.0, -120.0],
            },
            RackSlot {
                tool: 2,
                position: [600.0, 750.0, -120.0],
            },
        ],
        safe_z: -5.0,
        unload: vec![
            "G53 G0 Z{safe_z}".into(),
            "G53 G0 X{x} Y{y}".into(),
            "G53 G1 Z{z} F500".into(),
            "M64 P0".into(),
            "G53 G0 Z{safe_z}".into(),
        ],
        load: vec![
            "G53 G0 X{x} Y{y}".into(),
            "G53 G1 Z{z} F500".into(),
            "M65 P0".into(),
            "G53 G0 Z{safe_z}".into(),
        ],
    })
}

/// The rack, a keep-out zone round both slots below the tools' holders;
/// and a dust extractor on the way from the pocket's end to tool 1's slot,
/// its top at `top` (machine Z).
fn rack_zones_with_extractor(top: f64) -> Vec<serde_json::Value> {
    vec![
        zone(
            "the tool rack",
            [0.0, 730.0, -200.0],
            [620.0, 770.0, -125.0],
        ),
        zone(
            "the dust extractor",
            [150.0, 225.0, -200.0],
            [180.0, 255.0, top],
        ),
    ]
}

fn stock() -> Obstacle {
    Obstacle::stock_box(Bounds {
        min: [0.0, 0.0, -30.0],
        max: [100.0, 60.0, 0.0],
    })
}

fn obstacles(m: &Machine) -> Vec<Obstacle> {
    let mut o = vec![stock()];
    o.extend(Obstacle::zones(m, &AT));
    o
}

fn posted(p: &Program, m: &Machine) -> Posted {
    post_with_lines(
        p,
        dialect("grbl").unwrap().as_ref(),
        &PostSettings::default(),
        Some(m),
    )
    .unwrap()
}

fn text_of(posted: &Posted, line: usize) -> &str {
    posted.text.lines().nth(line - 1).unwrap()
}

#[test]
fn the_trip_to_the_rack_through_a_zone_is_flagged_at_the_changer_line() {
    // On its way to tool 1's slot with the gauge line at Z -5, the 6 mm
    // cutter's tip is at Z -25: the extractor standing up to Z -20 is in
    // its way.
    let m = machine(rack(), rack_zones_with_extractor(-20.0), None);
    let p = program();
    let posted = posted(&p, &m);
    let change = posted.changes[1].unwrap();
    assert!(change.by_program && change.unload.count == 5 && change.load.count == 4);
    assert_eq!(text_of(&posted, change.unload.first + 1), "G53 G0 X20 Y750");
    let r = check_with_tool_changes(
        &p,
        &m,
        &AT,
        &spindle(),
        &obstacles(&m),
        &posted,
        &CollisionOptions::default(),
    )
    .unwrap();
    assert_eq!(r.collisions.len(), 1, "{:?}", r.collisions);
    let c = &r.collisions[0];
    assert_eq!(
        c.message,
        "The tool change before Drill: the cutter runs into the dust extractor."
    );
    assert_eq!(c.change, Some(ChangeMove::Unload { line: 2 }));
    assert_eq!(c.line, Some(change.unload.first + 1));
    assert_eq!((c.operation, c.part, c.rapid), (1, PartKind::Cutter, true));
    assert_eq!((c.obstacle, c.obstacle_index), (ObstacleKind::Zone, 2));
    // In work coordinates, the tip 75 mm above the stock's top, at the
    // extractor's near side less the cutter's radius.
    assert!((c.at[2] - 75.0).abs() < 1e-6, "{c:?}");
    assert!((c.at[1] - (125.0 - 3.0)).abs() < 1e-6, "{c:?}");
    assert_eq!(r.clearance.unwrap().distance, 0.0);
}

#[test]
fn a_clear_trip_to_the_rack_has_no_contacts_and_the_rack_does_not_count() {
    // The extractor 15 mm below the cutter on the way; the changer's moves
    // down into the slots go into the rack's zone, which is where it docks.
    let m = machine(rack(), rack_zones_with_extractor(-40.0), None);
    let p = program();
    let posted = posted(&p, &m);
    let r = check_tool_changes(
        &p,
        &m,
        &AT,
        &spindle(),
        &obstacles(&m),
        &posted,
        &CollisionOptions::default(),
    )
    .unwrap();
    assert!(r.collisions.is_empty(), "{:?}", r.collisions);
    let c = r.clearance.unwrap();
    assert!(c.distance > 1.0, "{c:?}");
    assert!(c.change.is_some(), "{c:?}");
    // The operations themselves keep off the rack too.
    let whole = check_with_tool_changes(
        &p,
        &m,
        &AT,
        &spindle(),
        &obstacles(&m),
        &posted,
        &CollisionOptions::default(),
    )
    .unwrap();
    assert!(whole.collisions.is_empty(), "{:?}", whole.collisions);
}

#[test]
fn the_travel_after_a_manual_change_starts_at_the_park_position() {
    // The operator changes the tool at the park position (700, 780); the
    // spindle then comes down to the safe height and across to the hole,
    // over a box standing up to Z -50 half way.
    let extractor = vec![zone(
        "the dust extractor",
        [430.0, 450.0, -200.0],
        [460.0, 480.0, -50.0],
    )];
    let m = machine(
        ToolChanger::Manual,
        extractor.clone(),
        Some([700.0, 780.0, -2.0]),
    );
    let p = program();
    let posted = posted(&p, &m);
    let change = posted.changes[1].unwrap();
    assert!(!change.by_program && change.unload.count == 0);
    let r = check_tool_changes(
        &p,
        &m,
        &AT,
        &spindle(),
        &obstacles(&m),
        &posted,
        &CollisionOptions::default(),
    )
    .unwrap();
    assert!(!r.collisions.is_empty());
    // At the safe height the tip is at Z -85: the nut (from -65 to -45),
    // the widest part below the box's top, meets it first.
    let c = &r.collisions[0];
    assert_eq!(
        c.message,
        "The rapid move to Drill after the tool change: the collet nut runs into the dust extractor."
    );
    assert_eq!(
        (c.change, c.operation, c.part),
        (Some(ChangeMove::Travel), 1, PartKind::Nut)
    );
    assert!(
        r.collisions
            .iter()
            .all(|x| x.change == Some(ChangeMove::Travel))
    );
    assert!((c.at[2] - 15.0).abs() < 1e-6, "{c:?}");
    let line = c.line.unwrap();
    assert!(
        text_of(&posted, line).starts_with("G0 X90 Y50"),
        "line {line}"
    );
    // The first tool is in the spindle when the program starts: no travel
    // before the pocket.
    assert!(r.collisions.iter().all(|x| x.operation == 1));
    // Without a park position the spindle comes down above the pocket's
    // end, which is above the hole: nothing on the way.
    let m = machine(ToolChanger::Manual, extractor, None);
    let posted = post_with_lines(
        &p,
        dialect("grbl").unwrap().as_ref(),
        &PostSettings::default(),
        Some(&m),
    )
    .unwrap();
    let r = check_tool_changes(
        &p,
        &m,
        &AT,
        &spindle(),
        &obstacles(&m),
        &posted,
        &CollisionOptions::default(),
    )
    .unwrap();
    assert!(r.collisions.is_empty(), "{:?}", r.collisions);
}

#[test]
fn a_change_contact_goes_before_its_operations_own() {
    let own = |operation: usize, change: Option<ChangeMove>| Collision {
        message: String::new(),
        operation,
        move_index: 0,
        leg: 0,
        line: None,
        part: PartKind::Nut,
        part_name: String::new(),
        obstacle: ObstacleKind::Zone,
        obstacle_index: 0,
        rapid: true,
        at: [0.0; 3],
        change,
    };
    let mut whole = CollisionReport {
        collisions: vec![own(0, None), own(1, None)],
        ..CollisionReport::default()
    };
    let changes = CollisionReport {
        collisions: vec![
            own(1, Some(ChangeMove::Unload { line: 1 })),
            own(1, Some(ChangeMove::Travel)),
        ],
        ..CollisionReport::default()
    };
    whole.add_tool_changes(changes);
    let order: Vec<(usize, Option<ChangeMove>)> = whole
        .collisions
        .iter()
        .map(|c| (c.operation, c.change))
        .collect();
    assert_eq!(
        order,
        vec![
            (0, None),
            (1, Some(ChangeMove::Unload { line: 1 })),
            (1, Some(ChangeMove::Travel)),
            (1, None),
        ]
    );
}
