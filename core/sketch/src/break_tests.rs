// SPDX-License-Identifier: MIT
//! Break and Split at Point (mitcad#2108): a curve split into pieces that
//! all stay, at the curves crossing it or at a point; the pieces share their
//! ends, stay one line or curve, and the old curve's constraints and
//! dimensions go to the piece they belong to.

use super::*;
use crate::def::SketchDef;
use crate::edit::SketchEdit;
use crate::geometry::{Curve2, P2, dist};
use crate::ids::FeatureUid;
use crate::modify::{BreakAt, BreakMode};
use crate::parameters::Parameters;
use crate::values::ValueInput;

fn line(edit: &mut SketchEdit, a: P2, b: P2) -> EntityUid {
    let (start, end) = (edit.add_point(a), edit.add_point(b));
    edit.add_curve(EntityKind::Line {
        start,
        end,
        centerline: false,
    })
}

fn ends(edit: &SketchEdit, curve: EntityUid) -> (EntityUid, EntityUid) {
    edit.def.entity(curve).unwrap().kind.ends().unwrap()
}

fn solved_curve(edit: &SketchEdit, curve: EntityUid) -> Curve2 {
    edit.solved().unwrap().curves[&curve].clone()
}

fn solved_ends(edit: &SketchEdit, curve: EntityUid) -> (P2, P2) {
    solved_curve(edit, curve).ends().unwrap()
}

fn near(a: P2, b: P2) -> bool {
    dist(a, b) <= 1e-9
}

fn notes(edit: &SketchEdit) -> String {
    edit.report.notes.join(" | ")
}

fn has_constraint(edit: &SketchEdit, wanted: impl Fn(&ConstraintKind) -> bool) -> bool {
    edit.def.constraints.iter().any(|c| wanted(&c.kind))
}

#[test]
fn a_line_crossed_by_two_lines_breaks_into_three_collinear_pieces() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = line(&mut edit, [0.0, 0.0], [30.0, 0.0]);
    line(&mut edit, [10.0, -5.0], [10.0, 5.0]);
    line(&mut edit, [20.0, -5.0], [20.0, 5.0]);
    let dof = edit.solved().unwrap().status.dof;
    let pieces = edit
        .break_curve(c, BreakAt::At([15.0, 0.0]), BreakMode::Crossings)
        .unwrap();
    assert_eq!(pieces.len(), 3);
    assert_eq!(pieces[0], c, "the curve keeps the first piece");
    let expected = [
        ([0.0, 0.0], [10.0, 0.0]),
        ([10.0, 0.0], [20.0, 0.0]),
        ([20.0, 0.0], [30.0, 0.0]),
    ];
    for (piece, (a, b)) in pieces.iter().zip(expected) {
        let (s, e) = solved_ends(&edit, *piece);
        assert!(near(s, a) && near(e, b), "{piece:?}: {s:?} {e:?}");
    }
    // One point at each crossing, shared by the pieces either side.
    assert_eq!(ends(&edit, pieces[0]).1, ends(&edit, pieces[1]).0);
    assert_eq!(ends(&edit, pieces[1]).1, ends(&edit, pieces[2]).0);
    for pair in pieces.windows(2) {
        assert!(has_constraint(&edit, |k| {
            matches!(*k, ConstraintKind::Collinear { a, b } if a == pair[0] && b == pair[1])
        }));
    }
    let status = edit.solved().unwrap().status;
    assert!(status.redundant.is_empty(), "{:?}", status.redundant);
    // The two split points are held on the crossing lines: no freedom more.
    assert_eq!(status.dof, dof);
}

#[test]
fn an_arc_breaks_at_the_crossing_beside_the_pick() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let (center, start, end) = (
        edit.add_point([0.0, 0.0]),
        edit.add_point([10.0, 0.0]),
        edit.add_point([-10.0, 0.0]),
    );
    let arc = edit.add_curve(EntityKind::Arc { center, start, end });
    line(&mut edit, [5.0, 0.0], [5.0, 15.0]);
    let pieces = edit
        .break_curve(arc, BreakAt::At([0.0, 10.0]), BreakMode::Crossings)
        .unwrap();
    assert_eq!(pieces.len(), 2);
    let crossing = [5.0, 75f64.sqrt()];
    assert!(near(solved_ends(&edit, pieces[0]).1, crossing));
    assert!(near(solved_ends(&edit, pieces[1]).0, crossing));
    assert!(near(solved_ends(&edit, pieces[1]).1, [-10.0, 0.0]));
    // One centre and a shared end keep the pieces on one circle, without a
    // redundant equal.
    for piece in &pieces {
        let Curve2::Arc { radius, .. } = solved_curve(&edit, *piece) else {
            panic!("an arc");
        };
        assert!((radius - 10.0).abs() <= 1e-9);
        assert!(matches!(edit.def.entity(*piece).unwrap().kind,
            EntityKind::Arc { center: c, .. } if c == center));
    }
    assert!(edit.solved().unwrap().status.redundant.is_empty());
}

#[test]
fn a_circle_breaks_into_two_arcs_between_the_crossings_around_the_pick() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let center = edit.add_point([0.0, 0.0]);
    let circle = edit.add_curve(EntityKind::Circle {
        center,
        radius: 10.0,
        start: 0.0,
    });
    line(&mut edit, [5.0, -15.0], [5.0, 15.0]);
    line(&mut edit, [-5.0, -15.0], [-5.0, 15.0]);
    let pieces = edit
        .break_curve(circle, BreakAt::At([0.0, 10.0]), BreakMode::Crossings)
        .unwrap();
    assert_eq!(pieces.len(), 2);
    let h = 75f64.sqrt();
    // The rest from the upper left crossing round to the upper right one,
    // the piece around the pick between them.
    let (s, e) = solved_ends(&edit, pieces[0]);
    assert!(near(s, [-5.0, h]) && near(e, [5.0, h]), "{s:?} {e:?}");
    let (s, e) = solved_ends(&edit, pieces[1]);
    assert!(near(s, [5.0, h]) && near(e, [-5.0, h]), "{s:?} {e:?}");
    for piece in &pieces {
        assert!(matches!(
            edit.def.entity(*piece).unwrap().kind,
            EntityKind::Arc { .. }
        ));
    }
    assert!(edit.solved().unwrap().status.redundant.is_empty());
}

#[test]
fn split_at_point_splits_a_line_at_the_nearest_point() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = line(&mut edit, [0.0, 0.0], [30.0, 0.0]);
    let pieces = edit
        .break_curve(c, BreakAt::At([12.0, 0.3]), BreakMode::Point)
        .unwrap();
    assert_eq!(pieces.len(), 2);
    assert!(near(solved_ends(&edit, pieces[0]).1, [12.0, 0.0]));
    assert!(near(solved_ends(&edit, pieces[1]).0, [12.0, 0.0]));
    assert!(near(solved_ends(&edit, pieces[1]).1, [30.0, 0.0]));
}

#[test]
fn split_at_an_existing_point_makes_it_the_shared_end() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = line(&mut edit, [0.0, 0.0], [30.0, 0.0]);
    let q = edit.add_point([12.0, 0.0]);
    edit.add_constraint(ConstraintKind::Coincident {
        point: q,
        entity: Ref::Curve(c),
    });
    let pieces = edit
        .break_curve(c, BreakAt::Point(q), BreakMode::Point)
        .unwrap();
    assert_eq!(ends(&edit, pieces[0]).1, q);
    assert_eq!(ends(&edit, pieces[1]).0, q);
    // The point on the curve is its end now: nothing more holds it there.
    assert!(!has_constraint(&edit, |k| matches!(
        *k,
        ConstraintKind::Coincident { point, entity: Ref::Curve(_) } if point == q
    )));
}

#[test]
fn constraints_and_dimensions_go_to_the_piece_they_belong_to() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = line(&mut edit, [0.0, 0.0], [30.0, 0.0]);
    let (start, end) = ends(&edit, c);
    // A point on the line beyond the split, and a dimension to the line's
    // end from a point above it.
    let q = edit.add_point([25.0, 0.0]);
    edit.add_constraint(ConstraintKind::Coincident {
        point: q,
        entity: Ref::Curve(c),
    });
    let above = edit.add_point([30.0, 8.0]);
    let distance = edit
        .add_dimension(
            DimensionKind::Distance { a: end, b: above },
            Some(&ValueInput::Number(8.0)),
            false,
            None,
        )
        .unwrap();
    let length = edit
        .add_dimension(
            DimensionKind::Length { line: c },
            Some(&ValueInput::Number(30.0)),
            false,
            None,
        )
        .unwrap();
    let pieces = edit
        .break_curve(c, BreakAt::At([10.0, 0.0]), BreakMode::Point)
        .unwrap();
    // The point on the line now lies on the second piece.
    assert!(has_constraint(&edit, |k| matches!(
        *k,
        ConstraintKind::Coincident { point, entity: Ref::Curve(on) } if point == q && on == pieces[1]
    )));
    // The dimension on the end measures the same point, the second
    // piece's end.
    let d = edit
        .def
        .dimensions
        .iter()
        .find(|d| d.id == distance)
        .unwrap();
    assert_eq!(d.kind, DimensionKind::Distance { a: end, b: above });
    assert_eq!(ends(&edit, pieces[1]).1, end);
    // The length measures the whole, between the old ends, driven.
    let d = edit.def.dimensions.iter().find(|d| d.id == length).unwrap();
    assert_eq!(d.kind, DimensionKind::Distance { a: start, b: end });
    assert!(d.value.is_none(), "driven");
    assert!(
        notes(&edit).contains("driven dimension"),
        "{}",
        notes(&edit)
    );
    let status = edit.solved().unwrap().status;
    assert!(status.conflicts.is_empty() && status.redundant.is_empty());
}

#[test]
fn a_radius_stays_on_the_first_piece_and_holds_the_others() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let (center, start, end) = (
        edit.add_point([0.0, 0.0]),
        edit.add_point([10.0, 0.0]),
        edit.add_point([-10.0, 0.0]),
    );
    let arc = edit.add_curve(EntityKind::Arc { center, start, end });
    let radius = edit
        .add_dimension(
            DimensionKind::Radius { curve: arc },
            Some(&ValueInput::Number(10.0)),
            false,
            None,
        )
        .unwrap();
    let pieces = edit
        .break_curve(arc, BreakAt::At([0.0, 10.0]), BreakMode::Point)
        .unwrap();
    let d = edit.def.dimensions.iter().find(|d| d.id == radius).unwrap();
    assert_eq!(d.kind, DimensionKind::Radius { curve: pieces[0] });
    // The radius made 12: the second piece follows.
    edit.set_dimension(radius, &ValueInput::Number(12.0))
        .unwrap();
    for piece in &pieces {
        let Curve2::Arc { radius, .. } = solved_curve(&edit, *piece) else {
            panic!("an arc");
        };
        assert!((radius - 12.0).abs() <= 1e-6, "{radius}");
    }
}

#[test]
fn a_tangent_goes_to_the_piece_that_touches() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = line(&mut edit, [0.0, 0.0], [30.0, 0.0]);
    let center = edit.add_point([25.0, 5.0]);
    let circle = edit.add_curve(EntityKind::Circle {
        center,
        radius: 5.0,
        start: 0.0,
    });
    edit.add_constraint(ConstraintKind::tangent(c, circle));
    let pieces = edit
        .break_curve(c, BreakAt::At([10.0, 0.0]), BreakMode::Point)
        .unwrap();
    assert!(has_constraint(&edit, |k| matches!(
        *k,
        ConstraintKind::Tangent { a, b, .. } if a == pieces[1] && b == circle
    )));
}

#[test]
fn a_midpoint_on_the_split_curve_goes_with_a_note() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = line(&mut edit, [0.0, 0.0], [30.0, 0.0]);
    let m = edit.add_point([15.0, 0.0]);
    edit.add_constraint(ConstraintKind::Midpoint { point: m, curve: c });
    edit.break_curve(c, BreakAt::At([5.0, 0.0]), BreakMode::Point)
        .unwrap();
    assert!(!has_constraint(&edit, |k| matches!(
        *k,
        ConstraintKind::Midpoint { .. }
    )));
    assert!(notes(&edit).contains("midpoint"), "{}", notes(&edit));
}

#[test]
fn a_projected_curve_is_refused() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = line(&mut edit, [0.0, 0.0], [30.0, 0.0]);
    edit.def.entity_mut(c).unwrap().reference = true;
    let error = edit
        .break_curve(c, BreakAt::At([10.0, 0.0]), BreakMode::Point)
        .unwrap_err();
    assert!(error.contains("projected"), "{error}");
}

#[test]
fn an_offsets_curve_is_freed_first() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = line(&mut edit, [0.0, 0.0], [30.0, 0.0]);
    let (_, made) = edit.offset(&[c], &ValueInput::Number(5.0)).unwrap();
    let copy = made[0];
    let y = solved_ends(&edit, copy).0[1];
    let pieces = edit
        .break_curve(copy, BreakAt::At([10.0, y]), BreakMode::Point)
        .unwrap();
    assert_eq!(pieces.len(), 2);
    assert!(
        notes(&edit).contains("the offset is no longer linked to its source"),
        "{}",
        notes(&edit)
    );
}

#[test]
fn refusals_say_why_in_words() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let c = line(&mut edit, [0.0, 0.0], [30.0, 0.0]);
    let error = edit
        .break_curve(c, BreakAt::At([10.0, 0.0]), BreakMode::Crossings)
        .unwrap_err();
    assert!(error.contains("no other curve crosses"), "{error}");
    let error = edit
        .break_curve(c, BreakAt::At([30.0, 0.0]), BreakMode::Point)
        .unwrap_err();
    assert!(error.contains("already ends there"), "{error}");
    let center = edit.add_point([50.0, 0.0]);
    let circle = edit.add_curve(EntityKind::Circle {
        center,
        radius: 5.0,
        start: 0.0,
    });
    let error = edit
        .break_curve(circle, BreakAt::At([55.0, 0.0]), BreakMode::Point)
        .unwrap_err();
    assert!(error.contains("closed curve"), "{error}");
}
