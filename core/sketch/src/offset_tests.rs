// SPDX-License-Identifier: MIT
//! Offsets made by constraints add an independent set of constraints
//! (mitcad#1171): an exact offset of a projected outline of lines and the
//! arcs they run on into, and an offset of that offset, leave nothing
//! redundant and nothing free; fixed geometry made of fixed points is not
//! redundant among itself.

use super::solve::{DimensionInput, SolverItem, solve};
use super::*;
use crate::def::SketchDef;
use crate::edit::SketchEdit;
use crate::geometry::{Curve2, P2};
use crate::ids::FeatureUid;
use crate::parameters::Parameters;
use crate::values::ValueInput;

/// A fixed reference point at `at`, as a projection makes it: the ends of
/// curves shared (`shared`), centres each of its own.
fn fixed_point(entities: &mut Vec<Entity>, at: [f64; 2], shared: bool) -> EntityUid {
    if shared
        && let Some(e) = entities
            .iter()
            .find(|e| matches!(e.kind, EntityKind::Point { at: p } if p == at))
    {
        return e.id;
    }
    let mut e = Entity::point(EntityUid(entities.len() as u32 + 1), at);
    e.fixed = true;
    e.reference = true;
    let id = e.id;
    entities.push(e);
    id
}

/// A fixed reference curve.
fn fixed_curve(entities: &mut Vec<Entity>, kind: EntityKind) -> EntityUid {
    let mut e = Entity::new(EntityUid(entities.len() as u32 + 1), kind);
    e.fixed = true;
    e.reference = true;
    let id = e.id;
    entities.push(e);
    id
}

/// A 60 x 40 outline with corners rounded by `r`, projected (fixed lines
/// and arcs on shared fixed points), counter-clockwise from its bottom line:
/// each line runs on tangentially into the next arc.
fn rounded(r: f64) -> (SketchDef, Vec<EntityUid>) {
    let (w, h) = (60.0, 40.0);
    // A line's ends, or an arc's centre and ends.
    let curves: [(Option<P2>, P2, P2); 8] = [
        (None, [r, 0.0], [w - r, 0.0]),
        (Some([w - r, r]), [w - r, 0.0], [w, r]),
        (None, [w, r], [w, h - r]),
        (Some([w - r, h - r]), [w, h - r], [w - r, h]),
        (None, [w - r, h], [r, h]),
        (Some([r, h - r]), [r, h], [0.0, h - r]),
        (None, [0.0, h - r], [0.0, r]),
        (Some([r, r]), [0.0, r], [r, 0.0]),
    ];
    let mut entities = Vec::new();
    let mut outline = Vec::new();
    for (center, a, b) in curves {
        let (start, end) = (
            fixed_point(&mut entities, a, true),
            fixed_point(&mut entities, b, true),
        );
        let kind = match center {
            None => EntityKind::Line {
                start,
                end,
                centerline: false,
            },
            Some(c) => EntityKind::Arc {
                center: fixed_point(&mut entities, c, false),
                start,
                end,
            },
        };
        outline.push(fixed_curve(&mut entities, kind));
    }
    let def = SketchDef {
        entities,
        ..SketchDef::default()
    };
    (def, outline)
}

#[test]
fn an_exact_offset_of_a_projected_tangent_outline_and_its_offset_are_independent() {
    let (def, outline) = rounded(10.0);
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    // The projection alone: its arcs are held by their fixed points.
    let status = edit.solved().unwrap().status;
    assert!(status.redundant.is_empty(), "{:?}", status.redundant);
    assert_eq!(status.dof, 0);
    // In by 2: lines parallel at the distance and arcs about the same
    // centres, joined where they touch.
    let (_, first) = edit.offset(&outline, &ValueInput::Number(-2.0)).unwrap();
    let status = edit.solved().unwrap().status;
    assert!(status.redundant.is_empty(), "{:?}", status.redundant);
    assert!(status.conflicts.is_empty(), "{:?}", status.conflicts);
    assert_eq!(status.dof, 0, "the helpers do not slide");
    // In by 3 again, the offset of the offset.
    let (_, second) = edit.offset(&first, &ValueInput::Number(-3.0)).unwrap();
    let solved = edit.solved().unwrap();
    assert!(
        solved.status.redundant.is_empty(),
        "{:?}",
        solved.status.redundant
    );
    assert_eq!(solved.status.dof, 0, "the helpers do not slide");
    // In by 5 in all: the arcs' radius 5, the lines 5 in from the outline.
    for id in &second {
        match solved.curves.get(id) {
            Some(Curve2::Arc { radius, .. }) => {
                assert!((radius - 5.0).abs() < 1e-9, "{id:?}: radius {radius}");
            }
            Some(Curve2::Line { a, b }) => {
                let inside = |p: &[f64; 2]| {
                    [p[0] - 5.0, 55.0 - p[0], p[1] - 5.0, 35.0 - p[1]]
                        .iter()
                        .any(|d| d.abs() < 1e-9)
                };
                assert!(inside(a) && inside(b), "{id:?}: {a:?} {b:?}");
            }
            other => panic!("{id:?}: {other:?}"),
        }
    }
    // The definition keeps every constraint of the offsets: the solve only
    // leaves out those that follow from the others.
    let (def, report) = edit.finish().unwrap();
    assert!(report.status.redundant.is_empty());
    let distances = def
        .dimensions
        .iter()
        .filter(|d| matches!(d.kind, DimensionKind::LineDistance { .. }))
        .count();
    let parallels = def
        .constraints
        .iter()
        .filter(|c| matches!(c.kind, ConstraintKind::Parallel { .. }))
        .count();
    assert_eq!((distances, parallels), (8, 8));
}

/// An oval of four arcs that touch each other (radii 5 and 10, centres
/// (3, 0), (0, -4), (-3, 0), (0, 4)), projected, counter-clockwise from
/// its right arc.
fn oval() -> (SketchDef, Vec<EntityUid>) {
    let arcs: [(P2, P2, P2); 4] = [
        ([3.0, 0.0], [6.0, -4.0], [6.0, 4.0]),
        ([0.0, -4.0], [6.0, 4.0], [-6.0, 4.0]),
        ([-3.0, 0.0], [-6.0, 4.0], [-6.0, -4.0]),
        ([0.0, 4.0], [-6.0, -4.0], [6.0, -4.0]),
    ];
    let mut entities = Vec::new();
    let mut outline = Vec::new();
    for (c, a, b) in arcs {
        let (start, end) = (
            fixed_point(&mut entities, a, true),
            fixed_point(&mut entities, b, true),
        );
        let center = fixed_point(&mut entities, c, false);
        outline.push(fixed_curve(
            &mut entities,
            EntityKind::Arc { center, start, end },
        ));
    }
    let def = SketchDef {
        entities,
        ..SketchDef::default()
    };
    (def, outline)
}

#[test]
fn an_exact_offset_of_a_closed_chain_of_touching_arcs_and_its_offset_are_independent() {
    let (def, outline) = oval();
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    let status = edit.solved().unwrap().status;
    assert!(status.redundant.is_empty(), "{:?}", status.redundant);
    assert_eq!(status.dof, 0);
    // In by 1, and that in by 1 again: one arc's helper length holds the
    // radii all the way round, the others' are left out.
    let (_, first) = edit.offset(&outline, &ValueInput::Number(-1.0)).unwrap();
    let status = edit.solved().unwrap().status;
    assert!(status.redundant.is_empty(), "{:?}", status.redundant);
    assert!(
        status.redundant_sets.is_empty(),
        "{:?}",
        status.redundant_sets
    );
    assert!(status.conflicts.is_empty(), "{:?}", status.conflicts);
    assert_eq!(status.dof, 0, "the helpers do not slide");
    let (_, second) = edit.offset(&first, &ValueInput::Number(-1.0)).unwrap();
    let solved = edit.solved().unwrap();
    assert!(
        solved.status.redundant.is_empty(),
        "{:?}",
        solved.status.redundant
    );
    assert_eq!(solved.status.dof, 0, "the helpers do not slide");
    // In by 2 in all: radii 3 and 8.
    for (id, expected) in second.iter().zip([3.0, 8.0, 3.0, 8.0]) {
        match solved.curves.get(id) {
            Some(Curve2::Arc { radius, .. }) => {
                assert!((radius - expected).abs() < 1e-9, "{id:?}: radius {radius}");
            }
            other => panic!("{id:?}: {other:?}"),
        }
    }
    // Every helper keeps its length dimension in the definition.
    let (def, _) = edit.finish().unwrap();
    let lengths = def
        .dimensions
        .iter()
        .filter(|d| matches!(d.kind, DimensionKind::Length { .. }))
        .count();
    assert_eq!(lengths, 8);
}

#[test]
fn fixed_curves_made_of_fixed_points_are_not_redundant() {
    let mut entities = Vec::new();
    // A projected arc: its centre and ends fixed.
    let center = fixed_point(&mut entities, [0.0, 0.0], false);
    let start = fixed_point(&mut entities, [10.0, 0.0], true);
    let end = fixed_point(&mut entities, [0.0, 10.0], true);
    let arc = fixed_curve(&mut entities, EntityKind::Arc { center, start, end });
    // A projected circle with its fixed centre.
    let center = fixed_point(&mut entities, [30.0, 0.0], false);
    fixed_curve(
        &mut entities,
        EntityKind::Circle {
            center,
            radius: 5.0,
            start: 0.0,
        },
    );
    // A projected spline on its fixed control points.
    let control: Vec<EntityUid> = [[50.0, 0.0], [55.0, 10.0], [60.0, -10.0], [65.0, 0.0]]
        .iter()
        .enumerate()
        .map(|(i, at)| fixed_point(&mut entities, *at, i == 0 || i == 3))
        .collect();
    fixed_curve(
        &mut entities,
        EntityKind::Spline {
            degree: 3,
            control,
            weights: Vec::new(),
            knots: Vec::new(),
        },
    );
    let solved = solve(&entities, &[], &[]).unwrap();
    assert!(
        solved.status.redundant.is_empty(),
        "{:?}",
        solved.status.redundant
    );
    assert!(solved.status.redundant_sets.is_empty());
    assert_eq!(solved.status.dof, 0);
    // A dimension that repeats the fixed arc's radius still is redundant.
    let radius = DimensionKind::Radius { curve: arc };
    let dimension = DimensionInput {
        id: ConstraintUid(1),
        kind: &radius,
        value: Some(10.0),
    };
    let solved = solve(&entities, &[], &[dimension]).unwrap();
    assert_eq!(
        solved.status.redundant,
        [SolverItem::Constraint(ConstraintUid(1))]
    );
}
