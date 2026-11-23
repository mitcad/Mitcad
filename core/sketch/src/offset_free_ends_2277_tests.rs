// SPDX-License-Identifier: MIT
//! The free ends of an exact offset's results can be dragged and stay
//! (mitcad#2277, GitHub mitcad/Mitcad#570). Before every solve the seed
//! (mitcad#641) puts each result where the offset of its source is; it put
//! an arc's free ends back on its source's angles and a line's free end on
//! its source's end, so that a drag of an end the solve counts free was
//! undone by the next solve. Now a drag stores how far the end lies from
//! its source's end (an arc's angle, a line's distance along it) and the
//! seed applies it after moving the result with its source.

use std::f64::consts::PI;

use super::*;
use crate::def::SketchDef;
use crate::edit::SketchEdit;
use crate::geometry::{P2, dist};
use crate::ids::FeatureUid;
use crate::modify::DragTarget;
use crate::parameters::{ParamId, Parameters};
use crate::solve::Solved;
use crate::values::ValueInput;

const TOL: f64 = 1e-6;

fn fixed_point(id: u32, at: P2) -> Entity {
    let mut e = Entity::point(EntityUid(id), at);
    e.fixed = true;
    e
}

fn solve(def: &SketchDef, params: &Parameters) -> Solved {
    def.solve_with(&mut |id| params.value(id).ok_or_else(String::new), &|id| {
        params.name(id)
    })
    .expect("the sketch solves")
}

/// Where a point is stored.
fn stored(def: &SketchDef, p: EntityUid) -> P2 {
    match def.entity(p).map(|e| &e.kind) {
        Some(EntityKind::Point { at }) => *at,
        other => panic!("p{} is no point: {other:?}", p.0),
    }
}

/// An arc's centre, start and end points.
fn arc_points(def: &SketchDef, arc: EntityUid) -> (EntityUid, EntityUid, EntityUid) {
    match def.entity(arc).map(|e| &e.kind) {
        Some(EntityKind::Arc { center, start, end }) => (*center, *start, *end),
        other => panic!("c{} is no arc: {other:?}", arc.0),
    }
}

fn line_points(def: &SketchDef, line: EntityUid) -> (EntityUid, EntityUid) {
    match def.entity(line).map(|e| &e.kind) {
        Some(EntityKind::Line { start, end, .. }) => (*start, *end),
        other => panic!("c{} is no line: {other:?}", line.0),
    }
}

/// The angle of `p` about `c`.
fn angle(c: P2, p: P2) -> f64 {
    (p[1] - c[1]).atan2(p[0] - c[0])
}

/// Whether two angles are the same direction.
fn same_angle(a: f64, b: f64) -> bool {
    let d = (a - b).rem_euclid(2.0 * PI);
    d.min(2.0 * PI - d) < TOL
}

/// A quarter arc c4 about p1 = (0, 0) of radius 10 from p2 = (10, 0) to
/// p3, offset 2 mm alone. With `span`, p3 is free at (6, 8) and a
/// horizontal distance from p1 of `span` (a parameter of the sketch) holds
/// it; else p3 is fixed at (0, 10), and the whole arc is fixed as a linked
/// projection is. The definition, the source arc, the result arc and the
/// span's parameter.
fn arc_offset(
    params: &mut Parameters,
    span: Option<f64>,
) -> (SketchDef, EntityUid, EntityUid, Option<ParamId>) {
    let mut entities = vec![
        fixed_point(1, [0.0, 0.0]),
        fixed_point(2, [10.0, 0.0]),
        if span.is_some() {
            Entity::point(EntityUid(3), [6.0, 8.0])
        } else {
            fixed_point(3, [0.0, 10.0])
        },
    ];
    let mut arc = Entity::new(
        EntityUid(4),
        EntityKind::Arc {
            center: EntityUid(1),
            start: EntityUid(2),
            end: EntityUid(3),
        },
    );
    arc.fixed = span.is_none();
    entities.push(arc);
    let def = SketchDef {
        entities,
        ..SketchDef::default()
    };
    let mut edit = SketchEdit::new(def, params, FeatureUid(1), "Sketch1");
    let param = span.map(|span| {
        edit.add_dimension(
            DimensionKind::HorizontalDistance {
                a: EntityUid(1),
                b: EntityUid(3),
                signed: false,
            },
            Some(&ValueInput::Number(span)),
            false,
            None,
        )
        .unwrap()
    });
    let (_, results) = edit
        .offset(&[EntityUid(4)], &ValueInput::Number(2.0))
        .unwrap();
    assert_eq!(results.len(), 1);
    let (def, _) = edit.finish().unwrap();
    let param = param.map(|id| {
        def.dimensions
            .iter()
            .find(|d| d.id == id)
            .and_then(|d| d.value)
            .expect("the span's parameter")
    });
    // A new offset starts with its ends on the source's ends.
    assert!(def.offsets[0].shifts.is_empty(), "{:?}", def.offsets[0]);
    (def, EntityUid(4), results[0], param)
}

/// The result arc's start dragged along its circle to `turn` from where it
/// is, in an edit that is then finished.
fn drag_start(
    def: SketchDef,
    params: &mut Parameters,
    result: EntityUid,
    turn: f64,
) -> (SketchDef, P2) {
    let solved = solve(&def, params);
    let (center, start, _) = arc_points(&def, result);
    let (c, s) = (solved.points[&center], solved.points[&start]);
    let r = dist(c, s);
    let t = angle(c, s) + turn;
    let target = [c[0] + r * t.cos(), c[1] + r * t.sin()];
    let mut edit = SketchEdit::new(def, params, FeatureUid(1), "Sketch1");
    edit.drag(&[DragTarget::Point(start, target)]).unwrap();
    assert!(dist(edit.at(start).unwrap(), target) < TOL);
    let (def, _) = edit.finish().unwrap();
    (def, target)
}

#[test]
fn a_dragged_end_of_an_arc_offset_stays_where_it_was_dragged() {
    let mut params = Parameters::default();
    let (def, source, result, _) = arc_offset(&mut params, None);
    let before = solve(&def, &params);
    let (_, _, end) = arc_points(&def, result);
    let end_at = before.points[&end];
    let (def, target) = drag_start(def, &mut params, result, -20f64.to_radians());
    let (center, start, _) = arc_points(&def, result);
    // Stored where it was dragged, and how far from its source's end.
    assert!(dist(stored(&def, start), target) < TOL);
    let shifts = &def.offsets[0].shifts;
    assert_eq!(shifts.len(), 1, "{shifts:?}");
    assert_eq!(shifts[0].point, start);
    assert!(
        (shifts[0].by + 20f64.to_radians()).abs() < TOL,
        "{shifts:?}"
    );
    // A later solve starts it there and keeps it there; the end nobody
    // dragged stays on its source's end; the degrees of freedom are as the
    // solve counts them (the ends are free).
    let after = solve(&def, &params);
    assert!(dist(after.points[&start], target) < TOL, "{after:?}");
    assert!(dist(after.points[&end], end_at) < TOL, "{after:?}");
    assert_eq!(after.status.dof, before.status.dof);
    assert!(dist(after.points[&center], [0.0, 0.0]) < TOL);
    // Another edit, which solves the sketch again, keeps it too.
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    edit.add_point([50.0, 50.0]);
    let (def, _) = edit.finish().unwrap();
    assert!(dist(stored(&def, start), target) < TOL);
    let again = solve(&def, &params);
    assert!(dist(again.points[&start], target) < TOL, "{again:?}");
    // Dragged back onto its source's angle, it has no shift again.
    let (_, s, _) = arc_points(&def, source);
    let back = angle([0.0, 0.0], again.points[&s]) - angle([0.0, 0.0], target);
    let (def, _) = drag_start(def, &mut params, result, back);
    assert!(
        def.offsets[0].shifts.iter().all(|s| s.by.abs() < TOL),
        "{:?}",
        def.offsets[0]
    );
}

#[test]
fn a_dragged_free_end_of_an_open_chains_line_offset_stays() {
    // Two fixed lines turning left at (20, 0), offset 2 mm to their right
    // (outward): results along y = -2 and x = 22 meeting at their corner.
    let mut entities = vec![
        fixed_point(1, [0.0, 0.0]),
        fixed_point(2, [20.0, 0.0]),
        fixed_point(3, [20.0, 20.0]),
    ];
    for (id, (start, end)) in [(4, (1, 2)), (5, (2, 3))] {
        let mut line = Entity::new(
            EntityUid(id),
            EntityKind::Line {
                start: EntityUid(start),
                end: EntityUid(end),
                centerline: false,
            },
        );
        line.fixed = true;
        entities.push(line);
    }
    let def = SketchDef {
        entities,
        ..SketchDef::default()
    };
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    let (_, results) = edit
        .offset(&[EntityUid(4), EntityUid(5)], &ValueInput::Number(2.0))
        .unwrap();
    assert_eq!(results.len(), 2);
    let (def, _) = edit.finish().unwrap();
    let before = solve(&def, &params);
    // The first result's end that the second does not share is free.
    let (a, b) = line_points(&def, results[0]);
    let (c, d) = line_points(&def, results[1]);
    let free = if a == c || a == d { b } else { a };
    let other = if c == a || c == b { d } else { c };
    let from = before.points[&free];
    let other_at = before.points[&other];
    // Dragged 5 mm on along its line, away from the corner.
    let target = [from[0] - 5.0, from[1]];
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    edit.drag(&[DragTarget::Point(free, target)]).unwrap();
    let (def, _) = edit.finish().unwrap();
    assert!(dist(stored(&def, free), target) < TOL);
    let shifts = &def.offsets[0].shifts;
    assert_eq!(shifts.len(), 1, "{shifts:?}");
    assert_eq!(shifts[0].point, free);
    assert!((shifts[0].by.abs() - 5.0).abs() < TOL, "{shifts:?}");
    // A later solve keeps it; the other free end stays on its source's.
    let after = solve(&def, &params);
    assert!(dist(after.points[&free], target) < TOL, "{after:?}");
    assert!(dist(after.points[&other], other_at) < TOL, "{after:?}");
    assert_eq!(after.status.dof, before.status.dof);
}

#[test]
fn a_parameter_moves_an_undragged_end_and_a_dragged_one_keeps_its_shift() {
    let mut params = Parameters::default();
    let (def, source, result, span) = arc_offset(&mut params, Some(6.0));
    let span = span.unwrap();
    let turn = -20f64.to_radians();
    let (def, _) = drag_start(def, &mut params, result, turn);
    // The source's end moves round its circle: x 6 to 3.
    params.set_value(span, 3.0).unwrap();
    let solved = solve(&def, &params);
    let (sc, ss, se) = arc_points(&def, source);
    let (rc, rs, re) = arc_points(&def, result);
    let c = solved.points[&sc];
    assert!(dist(solved.points[&rc], c) < TOL);
    assert!((solved.points[&se][0] - 3.0).abs() < TOL, "{solved:?}");
    // The end nobody dragged follows its source's end ...
    assert!(
        same_angle(angle(c, solved.points[&re]), angle(c, solved.points[&se])),
        "{solved:?}"
    );
    // ... and the dragged one keeps its angle from its source's.
    assert!(
        same_angle(
            angle(c, solved.points[&rs]),
            angle(c, solved.points[&ss]) + turn
        ),
        "{solved:?}"
    );
}

#[test]
fn a_source_moved_farther_than_the_distance_keeps_the_dragged_end_and_the_side() {
    // mitcad#641's case: a linked projection moved its source 28 mm, more
    // than the 2 mm distance, before the solve.
    let mut params = Parameters::default();
    let (def, source, result, _) = arc_offset(&mut params, None);
    let radius = |solved: &Solved, arc: EntityUid, def: &SketchDef| {
        let (c, s, _) = arc_points(def, arc);
        dist(solved.points[&c], solved.points[&s])
    };
    let before = radius(&solve(&def, &params), result, &def);
    let turn = -20f64.to_radians();
    let (mut def, _) = drag_start(def, &mut params, result, turn);
    let (sc, ss, se) = arc_points(&def, source);
    for e in &mut def.entities {
        if let EntityKind::Point { at } = &mut e.kind
            && [sc, ss, se].contains(&e.id)
        {
            at[0] += 28.0;
        }
    }
    let solved = solve(&def, &params);
    let (rc, rs, re) = arc_points(&def, result);
    let c = solved.points[&sc];
    assert!(dist(c, [28.0, 0.0]) < TOL, "{solved:?}");
    assert!(dist(solved.points[&rc], c) < TOL, "{solved:?}");
    // On its side: the same radius as before the move.
    assert!(
        (radius(&solved, result, &def) - before).abs() < TOL,
        "{solved:?}"
    );
    assert!(
        same_angle(angle(c, solved.points[&re]), angle(c, solved.points[&se])),
        "{solved:?}"
    );
    assert!(
        same_angle(
            angle(c, solved.points[&rs]),
            angle(c, solved.points[&ss]) + turn
        ),
        "{solved:?}"
    );
}
