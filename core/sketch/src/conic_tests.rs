// SPDX-License-Identifier: MIT
//! Conic curves in sketch edits (mitcad#2236): the tool and its refusals,
//! Rho as a parameter, a dragged shoulder, and Trim, Break and Extend
//! keeping a conic on the same curve.

use super::*;
use crate::conic::ConicPiece;
use crate::def::SketchDef;
use crate::edit::{PointInput, SketchEdit};
use crate::geometry::{Curve2, P2, dist};
use crate::ids::FeatureUid;
use crate::modify::{BreakAt, BreakMode, DragTarget};
use crate::parameters::Parameters;
use crate::values::ValueInput;

const S: P2 = [0.0, 0.0];
const E: P2 = [20.0, 0.0];
const A: P2 = [10.0, 10.0];

fn conic(edit: &mut SketchEdit, rho: f64) -> EntityUid {
    edit.conic(
        PointInput::At(S),
        PointInput::At(E),
        PointInput::At(A),
        Some(&ValueInput::Number(rho)),
        false,
    )
    .unwrap()
}

fn line(edit: &mut SketchEdit, a: P2, b: P2) -> EntityUid {
    let (start, end) = (edit.add_point(a), edit.add_point(b));
    edit.add_curve(EntityKind::Line {
        start,
        end,
        centerline: false,
    })
}

fn solved_conic(edit: &SketchEdit, id: EntityUid) -> ConicPiece {
    let solved = edit.solved().unwrap();
    ConicPiece::of_curve(&solved.curves[&id]).expect("a conic")
}

/// Every point of `part` lies on `whole` (within 1e-6 mm).
fn on_curve(part: &ConicPiece, whole: &Curve2) {
    for i in 0..=40 {
        let p = part.point(f64::from(i) / 40.0).unwrap();
        let (_, d) = whole.closest(p);
        assert!(d < 1e-6, "{p:?} is {d} off");
    }
}

/// The parabola of the default conic continued: y = x (20 - x) / 20.
fn on_parabola(part: &ConicPiece) {
    for i in 0..=40 {
        let p = part.point(f64::from(i) / 40.0).unwrap();
        assert!((p[1] - p[0] * (20.0 - p[0]) / 20.0).abs() < 1e-6, "{p:?}");
    }
}

#[test]
fn the_tool_draws_a_conic_whose_shoulder_is_a_point() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = conic(&mut edit, 0.3);
    let EntityKind::Conic { apex, rho, .. } = edit.def.entity(c).unwrap().kind else {
        panic!("a conic");
    };
    assert_eq!(rho, 0.3);
    assert!(edit.def.entity(apex).unwrap().is_point());
    let solved = solved_conic(&edit, c);
    assert_eq!(solved.apex, A);
    assert!((solved.rho - 0.3).abs() < 1e-12);
    // A number stays the conic's own: no parameter.
    assert!(edit.def.conic_rhos.is_empty());
}

#[test]
fn the_tool_refuses_conics_without_a_shape() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let draw = |edit: &mut SketchEdit, apex: P2, rho: f64| {
        edit.conic(
            PointInput::At(S),
            PointInput::At(E),
            PointInput::At(apex),
            Some(&ValueInput::Number(rho)),
            false,
        )
        .unwrap_err()
    };
    assert!(draw(&mut edit, A, 1.5).contains("between 0 and 1"));
    assert!(draw(&mut edit, A, 0.0).contains("between 0 and 1"));
    assert!(draw(&mut edit, [7.0, 0.0], 0.5).contains("line through its ends"));
    assert!(draw(&mut edit, E, 0.5).contains("shoulder is at one of its ends"));
    let shared = edit.add_point([5.0, 5.0]);
    let error = edit
        .conic(
            PointInput::Existing(shared),
            PointInput::At(E),
            PointInput::Existing(shared),
            None,
            false,
        )
        .unwrap_err();
    assert!(error.contains("shoulder is at one of its ends"), "{error}");
    assert!(edit.def.entities.iter().all(|e| e.is_point()));
}

/// Rho as a parameter: an expression becomes the conic's own parameter,
/// whose value the solve takes; a changed value reshapes the conic.
#[test]
fn rho_follows_its_parameter() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = conic(&mut edit, 0.5);
    edit.set_conic_rho(c, &ValueInput::Name("0.2 + 0.1".to_owned()))
        .unwrap();
    let param = edit.def.conic_rho(c).expect("a parameter");
    assert!((solved_conic(&edit, c).rho - 0.3).abs() < 1e-12);
    // The parameter changed: the solve takes the new value.
    edit.set_conic_rho(c, &ValueInput::Number(0.8)).unwrap();
    assert_eq!(edit.def.conic_rho(c), Some(param), "the same parameter");
    assert!((solved_conic(&edit, c).rho - 0.8).abs() < 1e-12);
    // Out of range: refused.
    let error = edit
        .set_conic_rho(c, &ValueInput::Name("0.5 * 3".to_owned()))
        .unwrap_err();
    assert!(error.contains("between 0 and 1"), "{error}");
    // An empty name: the conic keeps its value, without the parameter.
    edit.set_conic_rho(c, &ValueInput::Name(String::new()))
        .unwrap();
    assert!(edit.def.conic_rho(c).is_none());
    assert!((solved_conic(&edit, c).rho - 0.8).abs() < 1e-12);
    // Only a conic has a Rho.
    let l = line(&mut edit, [0.0, -5.0], [20.0, -5.0]);
    assert!(
        edit.set_conic_rho(l, &ValueInput::Number(0.5))
            .unwrap_err()
            .contains("only a conic")
    );
}

/// Dragging the shoulder reshapes the conic: its end tangents still run
/// through it.
#[test]
fn a_dragged_shoulder_reshapes_the_conic() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = conic(&mut edit, 0.5);
    let EntityKind::Conic { apex, .. } = edit.def.entity(c).unwrap().kind else {
        panic!("a conic");
    };
    edit.drag(&[DragTarget::Point(apex, [12.0, 16.0])]).unwrap();
    let solved = edit.solved().unwrap();
    let curve = &solved.curves[&c];
    let at = solved.points[&apex];
    assert!(dist(at, [12.0, 16.0]) < 1e-6, "{at:?}");
    let [p0, d0, _] = curve.eval(0.0);
    let [p1, d1, _] = curve.eval(1.0);
    let off = |p: P2, d: P2| crate::geometry::cross(d, crate::geometry::sub(at, p)).abs();
    assert!(off(p0, d0) < 1e-9 * crate::geometry::norm(d0));
    assert!(off(p1, d1) < 1e-9 * crate::geometry::norm(d1));
}

/// A tangent line at the conic's end holds the shoulder on that line.
#[test]
fn a_tangent_at_an_end_runs_through_the_shoulder() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = conic(&mut edit, 0.5);
    let EntityKind::Conic { start, apex, .. } = edit.def.entity(c).unwrap().kind else {
        panic!("a conic");
    };
    let far = edit.add_point([-10.0, -5.0]);
    let l = edit.add_curve(EntityKind::Line {
        start: far,
        end: start,
        centerline: false,
    });
    edit.add_constraint(ConstraintKind::Tangent {
        a: l,
        b: c,
        at: Some(start),
    });
    let solved = edit.solved().unwrap();
    let (a, s, f) = (
        solved.points[&apex],
        solved.points[&start],
        solved.points[&far],
    );
    let cross = crate::geometry::cross(crate::geometry::sub(s, f), crate::geometry::sub(a, s));
    assert!(cross.abs() < 1e-6, "{f:?} {s:?} {a:?}");
}

/// Trim keeps the piece as a conic on the same curve.
#[test]
fn a_trimmed_conic_is_a_conic_on_the_same_curve() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = conic(&mut edit, 0.7);
    let whole = solved_conic(&edit, c).curve();
    line(&mut edit, [6.0, -5.0], [6.0, 20.0]);
    edit.trim(c, [2.0, 2.0]).unwrap();
    let EntityKind::Conic { rho, .. } = edit.def.entity(c).unwrap().kind else {
        panic!("still a conic");
    };
    let piece = solved_conic(&edit, c);
    assert!((piece.rho - rho).abs() < 1e-12);
    assert!((piece.start[0] - 6.0).abs() < 1e-6, "{piece:?}");
    assert!(dist(piece.end, E) < 1e-9);
    on_curve(&piece, &whole);
}

/// Break at a crossing line: two conics on the same curve, smooth where
/// they meet.
#[test]
fn a_broken_conic_is_two_conics_on_the_same_curve() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = conic(&mut edit, 0.5);
    line(&mut edit, [8.0, -5.0], [8.0, 20.0]);
    let pieces = edit
        .break_curve(c, BreakAt::At([4.0, 2.0]), BreakMode::Crossings)
        .unwrap();
    assert_eq!(pieces.len(), 2);
    for p in &pieces {
        assert!(matches!(
            edit.def.entity(*p).unwrap().kind,
            EntityKind::Conic { .. }
        ));
        on_parabola(&solved_conic(&edit, *p));
    }
    assert!(edit.def.constraints.iter().any(|k| matches!(
        k.kind,
        ConstraintKind::Tangent { a, b, .. } if a == pieces[0] && b == pieces[1]
    )));
}

/// Extend runs a conic on along its own curve to the curve it reaches.
#[test]
fn an_extended_conic_runs_on_along_its_curve() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = conic(&mut edit, 0.5);
    // The parabola y = x (20 - x) / 20 reaches y = -5 at x = 10 + 10 √2.
    line(&mut edit, [0.0, -5.0], [40.0, -5.0]);
    edit.extend(c, [19.0, 1.0]).unwrap();
    let piece = solved_conic(&edit, c);
    on_parabola(&piece);
    let x = 10.0 + 10.0 * 2f64.sqrt();
    assert!(dist(piece.end, [x, -5.0]) < 1e-6, "{piece:?}");
    assert!(dist(piece.start, S) < 1e-9);
}
