// SPDX-License-Identifier: MIT
//! Points held on a derived offset of a mixed chain, a line followed by a
//! spline (mitcad#2278): a line started at the offset's outer end gets a
//! point of its own held there, on the exact offset of the source line's
//! end; a point on the line's offset and one at the spline's offset end
//! stay on the exact offset as the source moves.

use super::*;
use crate::def::SketchDef;
use crate::edit::{PointInput, SketchEdit};
use crate::geometry::{P2, dist};
use crate::ids::FeatureUid;
use crate::parameters::Parameters;
use crate::values::ValueInput;

/// A line from (0, 0) to (10, 0) and a fitted spline from there through
/// (20, 5) to (30, 0), offset 2 to the right of the chain (below the
/// line): the line, the spline, the line's start point and the offset's
/// results.
fn mixed(edit: &mut SketchEdit<'_>) -> (EntityUid, EntityUid, EntityUid, Vec<EntityUid>) {
    let a = edit.add_point([0.0, 0.0]);
    let b = edit.add_point([10.0, 0.0]);
    let c = edit.add_point([20.0, 5.0]);
    let e = edit.add_point([30.0, 0.0]);
    let line = edit.add_curve(EntityKind::Line {
        start: a,
        end: b,
        centerline: false,
    });
    let spline = edit.add_curve(EntityKind::FittedSpline {
        points: vec![b, c, e],
        closed: false,
    });
    let (_, results) = edit
        .offset(&[line, spline], &ValueInput::Number(2.0))
        .unwrap();
    assert!(edit.def.offsets[0].derived);
    (line, spline, a, results)
}

/// The exact offset by 2 to the right of a line from `a` to `b`, at `a`.
fn right_of(a: P2, b: P2) -> P2 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let n = dx.hypot(dy);
    [a[0] + 2.0 * dy / n, a[1] - 2.0 * dx / n]
}

#[test]
fn a_line_started_at_a_derived_offset_end_gets_a_point_held_there() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let (line, _, a, results) = mixed(&mut edit);
    let Some(EntityKind::Line { start: corner, .. }) =
        edit.def.entity(results[0]).map(|e| e.kind.clone())
    else {
        panic!("the line's offset is a line");
    };
    let drawn = edit
        .add_line(
            PointInput::Existing(corner),
            PointInput::At([-6.0, -9.0]),
            false,
            false,
        )
        .unwrap();
    let Some(EntityKind::Line { start: own, .. }) = edit.def.entity(drawn).map(|e| e.kind.clone())
    else {
        panic!("a line");
    };
    assert_ne!(own, corner, "the computed point is not shared");
    assert!(edit.def.constraints.iter().any(|c| c.kind
        == ConstraintKind::Coincident {
            point: own,
            entity: Ref::Point(corner),
        }));
    let solved = edit.solved().unwrap();
    assert!(dist(solved.points[&own], [0.0, -2.0]) < 1e-9);
    // The source line's start moves: the drawn line's start follows on
    // the exact offset.
    if let Some(EntityKind::Point { at }) = edit.def.entity_mut(a).map(|e| &mut e.kind) {
        *at = [0.0, 3.0];
    }
    let solved = edit.solved().unwrap();
    let Some(crate::geometry::Curve2::Line { a: s, b: t }) = solved.curves.get(&line).cloned()
    else {
        panic!("a line");
    };
    let want = right_of(s, t);
    assert!(dist(solved.points[&own], want) < 1e-7, "{want:?}");
    // The offset's own corner is where the point is held.
    assert!(dist(solved.points[&corner], want) < 1e-7);
    edit.finish().unwrap();
}

#[test]
fn points_on_a_line_piece_and_at_a_spline_end_of_a_mixed_offset() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let (line, spline, _, results) = mixed(&mut edit);
    // On the line's offset.
    let on = edit.add_point([5.0, -1.4]);
    edit.add_constraint(ConstraintKind::Coincident {
        point: on,
        entity: Ref::Curve(results[0]),
    });
    // At the spline's offset's end, the outer end of the chain.
    let Some(EntityKind::Spline { control, .. }) =
        edit.def.entity(results[1]).map(|e| e.kind.clone())
    else {
        panic!("the spline's offset is a spline");
    };
    let tip = edit.add_point([31.0, -3.0]);
    edit.add_constraint(ConstraintKind::Coincident {
        point: tip,
        entity: Ref::Point(*control.last().unwrap()),
    });
    let solved = edit.solved().unwrap();
    let q = solved.points[&on];
    assert!(
        (q[1] + 2.0).abs() < 1e-9 && (0.0..=10.0).contains(&q[0]),
        "{q:?}"
    );
    let curve = &solved.curves[&spline];
    let (_, hi) = curve.domain();
    let [end, d1, _] = curve.eval(hi);
    let n = d1[0].hypot(d1[1]);
    let want = [end[0] + 2.0 * d1[1] / n, end[1] - 2.0 * d1[0] / n];
    assert!(dist(solved.points[&tip], want) < 1e-7, "{want:?}");
    // The degrees of freedom: the source's, the point on the line's offset
    // one more (along it), the end point none.
    let source_only = {
        let mut params = Parameters::default();
        let mut alone =
            SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
        mixed(&mut alone);
        alone.solved().unwrap().status.dof
    };
    assert_eq!(solved.status.dof, source_only + 1);
    let _ = line;
    edit.finish().unwrap();
}
