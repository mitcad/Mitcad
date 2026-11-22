// SPDX-License-Identifier: MIT
//! Extend to Meet (mitcad#2178): two lines or arcs extended or shortened
//! to a point where their whole lines and circles meet, ending in one
//! shared point; the picks say which part of each curve stays and which
//! meeting point is taken, `result` steps through the others.

use std::f64::consts::PI;

use super::*;
use crate::def::SketchDef;
use crate::edit::SketchEdit;
use crate::geometry::{Curve2, P2, dist};
use crate::ids::FeatureUid;
use crate::modify::meet_results;
use crate::parameters::Parameters;
use crate::values::ValueInput;
use mitcad_model_base::notice::FeatureError;

fn line(edit: &mut SketchEdit, a: P2, b: P2) -> EntityUid {
    let (start, end) = (edit.add_point(a), edit.add_point(b));
    edit.add_curve(EntityKind::Line {
        start,
        end,
        centerline: false,
    })
}

/// An arc about `c` of radius `r` from the angle `a0` counter-clockwise to
/// `a1` (degrees).
fn arc(edit: &mut SketchEdit, c: P2, r: f64, a0: f64, a1: f64) -> EntityUid {
    let center = edit.add_point(c);
    let start = edit.add_point(on_circle(c, r, a0));
    let end = edit.add_point(on_circle(c, r, a1));
    edit.add_curve(EntityKind::Arc { center, start, end })
}

fn on_circle(c: P2, r: f64, degrees: f64) -> P2 {
    let a = degrees * PI / 180.0;
    [c[0] + r * a.cos(), c[1] + r * a.sin()]
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
    dist(a, b) <= 1e-7
}

fn has_constraint(edit: &SketchEdit, wanted: impl Fn(&ConstraintKind) -> bool) -> bool {
    edit.def.constraints.iter().any(|c| wanted(&c.kind))
}

fn clean(edit: &SketchEdit) {
    let status = edit.solved().unwrap().status;
    assert!(
        status.conflicts.is_empty() && status.redundant.is_empty(),
        "{status:?}"
    );
}

/// What a person reads of a refusal: no curve names in it.
fn shown(error: &str) -> String {
    let message = FeatureError::from_text(error).message;
    assert!(
        !message.contains("c1") && !message.contains("c2"),
        "{message}"
    );
    message
}

#[test]
fn two_lines_meet_beyond_both_ends() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let a = line(&mut edit, [0.0, 0.0], [10.0, 0.0]);
    let b = line(&mut edit, [20.0, 5.0], [20.0, 15.0]);
    let met = edit
        .extend_to_meet(a, [2.0, 0.0], b, [20.0, 12.0], 0)
        .unwrap();
    assert_eq!(met.results, 1);
    // One shared point: the end of the first line, the start of the second.
    assert_eq!(ends(&edit, a).1, met.point);
    assert_eq!(ends(&edit, b).0, met.point);
    let (s, e) = solved_ends(&edit, a);
    assert!(near(s, [0.0, 0.0]) && near(e, [20.0, 0.0]), "{s:?} {e:?}");
    let (s, e) = solved_ends(&edit, b);
    assert!(near(s, [20.0, 0.0]) && near(e, [20.0, 15.0]), "{s:?} {e:?}");
    clean(&edit);
}

#[test]
fn a_line_across_the_other_is_shortened_keeping_the_picked_part() {
    for (pick, kept) in [
        ([5.0, 0.0], ([0.0, 0.0], [20.0, 0.0])),
        ([25.0, 0.0], ([20.0, 0.0], [30.0, 0.0])),
    ] {
        let mut params = Parameters::default();
        let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
        let a = line(&mut edit, [0.0, 0.0], [30.0, 0.0]);
        let b = line(&mut edit, [20.0, 5.0], [20.0, 15.0]);
        let (old_start, old_end) = ends(&edit, a);
        let met = edit.extend_to_meet(a, pick, b, [20.0, 12.0], 0).unwrap();
        let (s, e) = solved_ends(&edit, a);
        assert!(near(s, kept.0) && near(e, kept.1), "{pick:?}: {s:?} {e:?}");
        // The end on the meeting point's side moved; the other stayed.
        let (start, end) = ends(&edit, a);
        if pick[0] < 20.0 {
            assert_eq!(start, old_start);
            assert_eq!(end, met.point);
            assert_eq!(old_end, met.point, "the end used by this line alone moved");
        } else {
            assert_eq!(end, old_end);
            assert_eq!(start, met.point);
        }
        assert_eq!(ends(&edit, b).0, met.point);
        clean(&edit);
    }
}

#[test]
fn a_line_and_an_arc_meet_at_the_crossing_the_picks_choose() {
    let x = 75f64.sqrt();
    // The upper half circle of radius 10 and a short line inside it at
    // y = 5: their carriers cross at (-8.66, 5) and (8.66, 5).
    let cases = [
        // Picked right: the right crossing; the arc keeps its start.
        (
            [1.5, 5.0],
            20.0,
            0,
            ([-2.0, 5.0], [x, 5.0]),
            ([10.0, 0.0], [x, 5.0]),
        ),
        // Picked left: the left crossing; the arc keeps its end.
        (
            [-1.5, 5.0],
            160.0,
            0,
            ([-x, 5.0], [2.0, 5.0]),
            ([-x, 5.0], [-10.0, 0.0]),
        ),
        // Picked right, the second result (Tab): the left crossing, the
        // line's start and the arc's end moving there.
        (
            [1.5, 5.0],
            20.0,
            1,
            ([-x, 5.0], [2.0, 5.0]),
            ([10.0, 0.0], [-x, 5.0]),
        ),
        // Counted round: the third is the first again.
        (
            [1.5, 5.0],
            20.0,
            2,
            ([-2.0, 5.0], [x, 5.0]),
            ([10.0, 0.0], [x, 5.0]),
        ),
    ];
    for (line_pick, arc_degrees, result, line_after, arc_after) in cases {
        let mut params = Parameters::default();
        let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
        let l = line(&mut edit, [-2.0, 5.0], [2.0, 5.0]);
        let c = arc(&mut edit, [0.0, 0.0], 10.0, 0.0, 180.0);
        let arc_pick = on_circle([0.0, 0.0], 10.0, arc_degrees);
        let met = edit
            .extend_to_meet(l, line_pick, c, arc_pick, result)
            .unwrap();
        assert_eq!(met.results, 2, "{line_pick:?} {result}");
        assert_eq!(met.result, result % 2);
        let (s, e) = solved_ends(&edit, l);
        assert!(
            near(s, line_after.0) && near(e, line_after.1),
            "{line_pick:?} {result}: line {s:?} {e:?}"
        );
        let (s, e) = solved_ends(&edit, c);
        assert!(
            near(s, arc_after.0) && near(e, arc_after.1),
            "{line_pick:?} {result}: arc {s:?} {e:?}"
        );
        let Curve2::Arc { radius, .. } = solved_curve(&edit, c) else {
            panic!("an arc");
        };
        assert!((radius - 10.0).abs() <= 1e-7);
        let (ls, le) = ends(&edit, l);
        let (cs, ce) = ends(&edit, c);
        assert!(
            (ls == met.point || le == met.point) && (cs == met.point || ce == met.point),
            "one shared point"
        );
        clean(&edit);
    }
}

#[test]
fn two_arcs_meet_the_ends_nearer_the_picks_moving() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    // Circles of radius 10 about (0, 0) and (15, 0) cross at (7.5, ±6.61).
    let a = arc(&mut edit, [0.0, 0.0], 10.0, 50.0, 90.0);
    let b = arc(&mut edit, [15.0, 0.0], 10.0, 90.0, 130.0);
    let met = edit
        .extend_to_meet(
            a,
            on_circle([0.0, 0.0], 10.0, 60.0),
            b,
            on_circle([15.0, 0.0], 10.0, 120.0),
            0,
        )
        .unwrap();
    // Two points, each reached either way round by either arc.
    assert_eq!(met.results, 8);
    let x = [7.5, 43.75f64.sqrt()];
    // The first arc's start goes back to the crossing, the second arc's end
    // on to it.
    let (s, e) = solved_ends(&edit, a);
    assert!(near(s, x) && near(e, [0.0, 10.0]), "{s:?} {e:?}");
    let (s, e) = solved_ends(&edit, b);
    assert!(near(s, [15.0, 10.0]) && near(e, x), "{s:?} {e:?}");
    assert_eq!(ends(&edit, a).0, met.point);
    assert_eq!(ends(&edit, b).1, met.point);
    for curve in [a, b] {
        let Curve2::Arc { radius, .. } = solved_curve(&edit, curve) else {
            panic!("an arc");
        };
        assert!((radius - 10.0).abs() <= 1e-7);
    }
    clean(&edit);
}

#[test]
fn a_tangent_line_meets_an_arc_at_one_point_and_is_held_tangent() {
    let a = Curve2::Arc {
        center: [0.0, 0.0],
        radius: 10.0,
        start: 0.0,
        end: PI / 2.0,
    };
    let l = Curve2::Line {
        a: [-10.0, -5.0],
        b: [-10.0, -15.0],
    };
    // The line touches the arc's circle at (-10, 0): one point, which the
    // arc reaches either way round, the end nearer the pick first.
    let results = meet_results(&l, [-10.0, -12.0], &a, on_circle([0.0, 0.0], 10.0, 80.0)).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| near(r.point, [-10.0, 0.0])));
    assert!(results[0].b.moves_end && !results[1].b.moves_end);

    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let l = line(&mut edit, [-10.0, -5.0], [-10.0, -15.0]);
    let c = arc(&mut edit, [0.0, 0.0], 10.0, 0.0, 90.0);
    let met = edit
        .extend_to_meet(l, [-10.0, -12.0], c, on_circle([0.0, 0.0], 10.0, 80.0), 0)
        .unwrap();
    assert_eq!(met.results, 2);
    let (s, e) = solved_ends(&edit, c);
    assert!(near(s, [10.0, 0.0]) && near(e, [-10.0, 0.0]), "{s:?} {e:?}");
    let (s, e) = solved_ends(&edit, l);
    assert!(
        near(s, [-10.0, 0.0]) && near(e, [-10.0, -15.0]),
        "{s:?} {e:?}"
    );
    assert!(has_constraint(&edit, |k| matches!(
        *k,
        ConstraintKind::Tangent { a, b, .. } if (a == l && b == c) || (a == c && b == l)
    )));
    clean(&edit);
}

/// The refusal of curves `build` makes, picked at `picks`; nothing changed.
fn refused(build: impl Fn(&mut SketchEdit) -> (EntityUid, EntityUid), picks: (P2, P2)) -> String {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let (a, b) = build(&mut edit);
    let before = edit.def.entities.clone();
    let error = edit.extend_to_meet(a, picks.0, b, picks.1, 0).unwrap_err();
    assert_eq!(edit.def.entities, before, "nothing changed");
    shown(&error)
}

#[test]
fn curves_that_never_meet_are_refused_in_words() {
    let parallel = refused(
        |e| {
            (
                line(e, [0.0, 0.0], [10.0, 0.0]),
                line(e, [0.0, 5.0], [10.0, 5.0]),
            )
        },
        ([5.0, 0.0], [5.0, 5.0]),
    );
    assert!(parallel.contains("parallel"), "{parallel}");
    let collinear = refused(
        |e| {
            (
                line(e, [0.0, 0.0], [10.0, 0.0]),
                line(e, [20.0, 0.0], [30.0, 0.0]),
            )
        },
        ([5.0, 0.0], [25.0, 0.0]),
    );
    assert!(collinear.contains("one line"), "{collinear}");
    let concentric = refused(
        |e| {
            (
                arc(e, [0.0, 0.0], 10.0, 0.0, 90.0),
                arc(e, [0.0, 0.0], 5.0, 180.0, 270.0),
            )
        },
        (
            on_circle([0.0, 0.0], 10.0, 45.0),
            on_circle([0.0, 0.0], 5.0, 225.0),
        ),
    );
    assert!(concentric.contains("same centre"), "{concentric}");
    let apart = refused(
        |e| {
            (
                arc(e, [0.0, 0.0], 5.0, 0.0, 90.0),
                arc(e, [20.0, 0.0], 5.0, 90.0, 180.0),
            )
        },
        (
            on_circle([0.0, 0.0], 5.0, 45.0),
            on_circle([20.0, 0.0], 5.0, 135.0),
        ),
    );
    assert!(apart.contains("never meet"), "{apart}");
    let missed = refused(
        |e| {
            (
                line(e, [0.0, 20.0], [10.0, 20.0]),
                arc(e, [0.0, 0.0], 5.0, 0.0, 90.0),
            )
        },
        ([5.0, 20.0], on_circle([0.0, 0.0], 5.0, 45.0)),
    );
    assert!(missed.contains("misses"), "{missed}");
}

#[test]
fn one_curve_twice_and_a_circle_are_refused() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let a = line(&mut edit, [0.0, 0.0], [10.0, 0.0]);
    let center = edit.add_point([20.0, 0.0]);
    let circle = edit.add_curve(EntityKind::Circle {
        center,
        radius: 5.0,
        start: 0.0,
    });
    let twice = edit
        .extend_to_meet(a, [1.0, 0.0], a, [9.0, 0.0], 0)
        .unwrap_err();
    assert!(shown(&twice).contains("two different curves"), "{twice}");
    let round = edit
        .extend_to_meet(a, [1.0, 0.0], circle, [15.0, 0.0], 0)
        .unwrap_err();
    assert!(
        FeatureError::from_text(&round)
            .message
            .contains("only lines and arcs"),
        "{round}"
    );
}

/// A length dimension on a line, driving.
fn length(edit: &mut SketchEdit, line: EntityUid, value: f64) -> ConstraintUid {
    edit.add_dimension(
        DimensionKind::Length { line },
        Some(&ValueInput::Number(value)),
        false,
        None,
    )
    .unwrap()
}

/// The value of a length dimension.
fn length_value(edit: &SketchEdit, id: ConstraintUid) -> f64 {
    let d = edit.def.dimensions.iter().find(|d| d.id == id).unwrap();
    assert!(matches!(d.kind, DimensionKind::Length { .. }));
    edit.params.value(d.value.expect("driving")).unwrap()
}

#[test]
fn constraints_and_dimensions_keep_their_meaning() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let a = line(&mut edit, [0.0, 0.0], [10.0, 0.0]);
    let b = line(&mut edit, [20.0, 5.0], [20.0, 15.0]);
    // The first line's start fixed; the second line free to follow the
    // corner (a fixed far end too would hold the corner twice over).
    let (a0, _) = ends(&edit, a);
    edit.set_fixed(&[Ref::Point(a0)], true).unwrap();
    edit.add_constraint(ConstraintKind::Horizontal { line: a });
    edit.add_constraint(ConstraintKind::Vertical { line: b });
    let la = length(&mut edit, a, 10.0);
    let lb = length(&mut edit, b, 10.0);
    // A point held at the middle of the first line: its middle moves.
    let m = edit.add_point([5.0, 0.0]);
    edit.add_constraint(ConstraintKind::Midpoint { point: m, curve: a });
    clean(&edit);
    let met = edit
        .extend_to_meet(a, [2.0, 0.0], b, [20.0, 12.0], 0)
        .unwrap();
    // The lengths measure the new ends.
    assert!((length_value(&edit, la) - 20.0).abs() <= 1e-9);
    assert!((length_value(&edit, lb) - 15.0).abs() <= 1e-9);
    // Horizontal and vertical stay; the midpoint went.
    assert!(has_constraint(
        &edit,
        |k| matches!(*k, ConstraintKind::Horizontal { line } if line == a)
    ));
    assert!(has_constraint(
        &edit,
        |k| matches!(*k, ConstraintKind::Vertical { line } if line == b)
    ));
    assert!(!has_constraint(&edit, |k| matches!(
        *k,
        ConstraintKind::Midpoint { .. }
    )));
    // The sketch solves to the corner, nothing pulled back.
    let (s, e) = solved_ends(&edit, a);
    assert!(near(s, [0.0, 0.0]) && near(e, [20.0, 0.0]), "{s:?} {e:?}");
    let (s, e) = solved_ends(&edit, b);
    assert!(near(s, [20.0, 0.0]) && near(e, [20.0, 15.0]), "{s:?} {e:?}");
    assert_eq!(ends(&edit, a).1, met.point);
    assert_eq!(ends(&edit, b).0, met.point);
    clean(&edit);
}

#[test]
fn an_end_shared_with_a_third_curve_stays_with_it() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let a = line(&mut edit, [0.0, 0.0], [10.0, 0.0]);
    let b = line(&mut edit, [20.0, 5.0], [20.0, 15.0]);
    // A third line from the first line's end.
    let (_, shared) = ends(&edit, a);
    let up = edit.add_point([10.0, 8.0]);
    let third = edit.add_curve(EntityKind::Line {
        start: shared,
        end: up,
        centerline: false,
    });
    let met = edit
        .extend_to_meet(a, [2.0, 0.0], b, [20.0, 12.0], 0)
        .unwrap();
    assert_ne!(met.point, shared);
    assert_eq!(ends(&edit, third).0, shared, "the third line keeps its end");
    assert!(near(solved_ends(&edit, third).0, [10.0, 0.0]));
    assert!(near(solved_ends(&edit, a).1, [20.0, 0.0]));
    assert_eq!(ends(&edit, a).1, met.point);
    assert_eq!(ends(&edit, b).0, met.point);
}
