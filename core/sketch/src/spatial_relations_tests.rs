// SPDX-License-Identifier: MIT
//! 3D relations and dimensions of mitcad#1661, without the document: general
//! parallel and perpendicular directions (lines and arcs' normals), the
//! angle between two directions, symmetry of two points about a plane (a
//! frame plane, or one found outside the sketch), concentric arcs and
//! circles (centre and normal, or the centre only), curvature continuity
//! (G2) at a spline's end, points on another plane than the sketch plane,
//! a driving spline length and a spline end's tangent magnitude. For each:
//! the solve, a drag that keeps it, the degrees of freedom it leaves
//! (under-constrained), the over-constrained cases (the same relation or
//! dimension again is redundant; one that cannot hold with it, or with fixed
//! points, fails as a conflict), the file form read back, and where a
//! relation takes only some curves or ends, the refusal of others in words.
//! The angle's over-constrained case is a second driving angle that
//! contradicts it; driven dimensions get their reference parameters.

use std::collections::BTreeMap;

use crate::def::SketchPlane;
use crate::ids::{EntityUid, FeatureUid};
use crate::parameters::Parameters;
use crate::spatial::*;
use crate::spatial_edit::*;
use crate::values::ValueInput;

use SpatialPointInput::{At, Existing};

const TOL: f64 = 1e-6;

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= TOL)
}

fn solve(def: &SpatialSketchDef, params: &Parameters) -> Result<SpatialSolved, SpatialFailure> {
    solve_with(def, params, &SpatialInputs::default())
}

fn solve_with(
    def: &SpatialSketchDef,
    params: &Parameters,
    inputs: &SpatialInputs,
) -> Result<SpatialSolved, SpatialFailure> {
    let mut value = |id| params.value(id).ok_or_else(|| "no value".to_owned());
    def.build_with(&mut value, inputs)?.solve()
}

fn drag(
    def: &SpatialSketchDef,
    params: &Parameters,
    goals: &[(EntityUid, [f64; 3])],
) -> Result<SpatialSolved, SpatialFailure> {
    let mut value = |id| params.value(id).ok_or_else(|| "no value".to_owned());
    def.build(&mut value, &BTreeMap::new())?.drag(goals)
}

fn edit<'a>(def: SpatialSketchDef, params: &'a mut Parameters) -> SpatialEdit<'a> {
    SpatialEdit::new(def, params, FeatureUid(1), "3DSketch1")
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// A line's unit direction where the solve put its points.
fn direction(solved: &SpatialSolved, def: &SpatialSketchDef, line: EntityUid) -> [f64; 3] {
    let [a, b] = def.line(line).unwrap();
    let d = sub(solved.points[&b], solved.points[&a]);
    let n = norm(d);
    [d[0] / n, d[1] / n, d[2] / n]
}

/// A line from a fixed start, its end free.
fn free_line(e: &mut SpatialEdit<'_>, start: [f64; 3], end: [f64; 3]) -> EntityUid {
    let line = e.add_line(At(start), At(end), false).unwrap();
    let [a, _] = e.def.line(line).unwrap();
    e.fix(a).unwrap();
    line
}

/// Over-constrained by contradicting relations: the solve fails as a
/// conflict (or finds no solution).
fn contradicts(def: &SpatialSketchDef, params: &Parameters) {
    let failure = solve(def, params).unwrap_err();
    assert!(
        matches!(
            failure.kind,
            SpatialFailureKind::Conflict | SpatialFailureKind::NotConverged
        ),
        "{failure}"
    );
}

/// The file form read back gives the same definition.
fn reads_back(def: &SpatialSketchDef, params: &Parameters) {
    let written: SpatialSketchDef<String> = def
        .map_params(&mut |_: &str, id: &crate::parameters::ParamId| Ok::<_, ()>(params.name(*id)))
        .unwrap();
    let json = serde_json::to_value(&written).unwrap();
    let read: SpatialSketchDef<String> = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(read, written, "{json}");
}

#[test]
fn parallel_and_perpendicular_directions_in_space() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let a = free_line(&mut e, [0.0; 3], [40.0, 1.0, 2.0]);
    let b = free_line(&mut e, [0.0, 20.0, 0.0], [30.0, 25.0, 10.0]);
    let c = free_line(&mut e, [50.0, 0.0, 0.0], [52.0, 30.0, 4.0]);
    let (def, _, _) = e.finish().unwrap();
    assert_eq!(solve(&def, &params).unwrap().status.dof, 9);

    // Parallel: no intersection implied, either way along each other.
    let mut e = edit(def, &mut params);
    let parallel = e
        .add_constraint(SpatialConstraintKind::Parallel { a, b })
        .unwrap();
    let perpendicular = e
        .add_constraint(SpatialConstraintKind::Perpendicular { a, b: c })
        .unwrap();
    let (mut def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert_eq!(
        solved.status.dof, 6,
        "two rows of parallel, one of perpendicular"
    );
    let (u, v, w) = (
        direction(&solved, &def, a),
        direction(&solved, &def, b),
        direction(&solved, &def, c),
    );
    assert!(norm(cross(u, v)) <= TOL, "{u:?} {v:?}");
    assert!(dot(u, w).abs() <= TOL, "{u:?} {w:?}");
    // The lines do not meet: parallel keeps them apart.
    let [_, b_end] = def.line(b).unwrap();
    assert!(solved.points[&b_end][1] > 15.0);
    def.store_solved(&solved);

    // A drag of b's end turns a too, and both relations hold.
    let [_, a_end] = def.line(a).unwrap();
    let dragged = drag(&def, &params, &[(b_end, [10.0, 40.0, 25.0])]).unwrap();
    let (u, v, w) = (
        direction(&dragged, &def, a),
        direction(&dragged, &def, b),
        direction(&dragged, &def, c),
    );
    assert!(norm(cross(u, v)) <= TOL && dot(u, w).abs() <= TOL);
    assert!(!near(dragged.points[&a_end], solved.points[&a_end]));

    // The same relation twice is redundant; parallel to a line it is at
    // right angles to is refused.
    let mut e = edit(def.clone(), &mut params);
    let twice = e
        .add_constraint(SpatialConstraintKind::Parallel { a: b, b: a })
        .unwrap();
    let (twin, _, _) = e.finish().unwrap();
    let solved = solve(&twin, &params).unwrap();
    assert!(solved.status.redundant.contains(&twice), "{solved:?}");
    let mut e = edit(def.clone(), &mut params);
    e.add_constraint(SpatialConstraintKind::Parallel { a, b: c })
        .unwrap();
    let (contradicting, _, _) = e.finish().unwrap();
    let failure = solve(&contradicting, &params).unwrap_err();
    assert!(
        matches!(
            failure.kind,
            SpatialFailureKind::Conflict | SpatialFailureKind::NotConverged
        ),
        "{failure}"
    );
    // A line with itself, or a spline, is refused in words.
    let mut e = edit(def.clone(), &mut params);
    e.add_constraint(SpatialConstraintKind::Parallel { a, b: a })
        .unwrap();
    let error = e.finish().unwrap_err();
    assert!(error.contains("two different curves"), "{error}");

    // Kept in the file, read back the same.
    assert!(matches!(
        def.constraint(parallel).unwrap().kind,
        SpatialConstraintKind::Parallel { .. }
    ));
    assert!(def.constraint(perpendicular).is_some());
    reads_back(&def, &params);
}

#[test]
fn a_line_parallel_or_at_right_angles_to_an_arcs_normal() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let circle = e
        .add_circle(At([0.0; 3]), 10.0, [0.2, 0.1, 1.0], None, false)
        .unwrap();
    let center = e.def.curve(circle).unwrap().points()[0];
    e.fix(center).unwrap();
    let line = free_line(&mut e, [30.0, 0.0, 0.0], [35.0, 4.0, 20.0]);
    let (base, _, _) = e.finish().unwrap();
    let mut e = edit(base.clone(), &mut params);
    e.add_constraint(SpatialConstraintKind::Parallel { a: line, b: circle })
        .unwrap();
    let (mut def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    let normal_of = |solved: &SpatialSolved| match solved.curves.get(&circle) {
        Some(SolvedCurve::Round { normal, .. }) => *normal,
        other => panic!("{other:?}"),
    };
    let normal = normal_of(&solved);
    assert!(norm(cross(direction(&solved, &def, line), normal)) <= TOL);
    // radius 1 + normal 2 + line end 3, less 2.
    assert_eq!(solved.status.dof, 4);
    def.store_solved(&solved);

    // A drag of the line's end turns the circle's plane with it.
    let [_, end] = def.line(line).unwrap();
    let target = [50.0, 20.0, 10.0];
    let dragged = drag(&def, &params, &[(end, target)]).unwrap();
    assert!(near(dragged.points[&end], target), "{dragged:?}");
    let turned = normal_of(&dragged);
    assert!(norm(cross(direction(&dragged, &def, line), turned)) <= TOL);
    assert!(norm(cross(normal, turned)) > 0.1, "{normal:?} {turned:?}");
    reads_back(&def, &params);

    // The same relation the other way round follows from the first; at
    // right angles as well contradicts it.
    let mut e = edit(def.clone(), &mut params);
    let twice = e
        .add_constraint(SpatialConstraintKind::Parallel { a: circle, b: line })
        .unwrap();
    let (twin, _, _) = e.finish().unwrap();
    let solved = solve(&twin, &params).unwrap();
    assert!(solved.status.redundant.contains(&twice), "{solved:?}");
    assert_eq!(solved.status.dof, 4);
    let mut e = edit(def, &mut params);
    e.add_constraint(SpatialConstraintKind::Perpendicular { a: line, b: circle })
        .unwrap();
    let (contradicting, _, _) = e.finish().unwrap();
    contradicts(&contradicting, &params);

    // At right angles alone: one row.
    let mut e = edit(base, &mut params);
    e.add_constraint(SpatialConstraintKind::Perpendicular { a: line, b: circle })
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert!(dot(direction(&solved, &def, line), normal_of(&solved)).abs() <= TOL);
    assert_eq!(solved.status.dof, 5);
}

#[test]
fn an_angle_between_lines_drives_and_measures() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let a = e
        .add_line(At([0.0; 3]), At([40.0, 0.0, 0.0]), false)
        .unwrap();
    for p in e.def.line(a).unwrap() {
        e.fix(p).unwrap();
    }
    let b = free_line(&mut e, [0.0, 20.0, 0.0], [30.0, 40.0, 10.0]);
    let angle = e
        .add_dimension(
            SpatialDimensionKind::Angle { a, b },
            Some(&ValueInput::Number(std::f64::consts::FRAC_PI_3)),
            false,
        )
        .unwrap();
    let driven = e
        .add_dimension(SpatialDimensionKind::Angle { a: b, b: a }, None, true)
        .unwrap();
    let (mut def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    let (u, v) = (direction(&solved, &def, a), direction(&solved, &def, b));
    let measured = norm(cross(u, v)).atan2(dot(u, v));
    assert!(
        (measured - std::f64::consts::FRAC_PI_3).abs() <= 1e-9,
        "{measured}"
    );
    assert!((solved.status.measured[&angle] - std::f64::consts::FRAC_PI_3).abs() <= 1e-9);
    assert!((solved.status.measured[&driven] - std::f64::consts::FRAC_PI_3).abs() <= 1e-9);
    assert_eq!(solved.status.dof, 2, "b's end keeps two freedoms");
    def.store_solved(&solved);
    // A drag keeps the angle.
    let [_, end] = def.line(b).unwrap();
    let dragged = drag(&def, &params, &[(end, [10.0, 50.0, -20.0])]).unwrap();
    let (u, v) = (direction(&dragged, &def, a), direction(&dragged, &def, b));
    assert!((norm(cross(u, v)).atan2(dot(u, v)) - std::f64::consts::FRAC_PI_3).abs() <= 1e-9);
    // A second driving angle on the same lines contradicts it.
    let mut e = edit(def.clone(), &mut params);
    e.add_dimension(
        SpatialDimensionKind::Angle { a, b },
        Some(&ValueInput::Number(1.0)),
        false,
    )
    .unwrap();
    let (contradicting, _, _) = e.finish().unwrap();
    assert!(solve(&contradicting, &params).is_err());
    // An angle of nothing or of a half turn is refused: parallel is meant.
    let mut e = edit(def.clone(), &mut params);
    e.set_dimension_value(angle, &ValueInput::Number(0.0))
        .unwrap();
    let error = e.finish().unwrap_err();
    assert!(error.contains("parallel"), "{error}");
    reads_back(&def, &params);
}

#[test]
fn two_points_are_mirrored_across_a_plane() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let p = e.add_point([10.0, 5.0, 3.0]).unwrap();
    e.fix(p).unwrap();
    let q = e.add_point([-8.0, 4.0, -2.0]).unwrap();
    e.add_constraint(SpatialConstraintKind::Symmetric {
        a: p,
        b: q,
        plane: SpatialPlaneRef::Frame(SpatialAxis::X),
    })
    .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert!(near(solved.points[&q], [-10.0, 5.0, 3.0]), "{solved:?}");
    assert!(solved.status.fully_constrained);
    reads_back(&def, &params);

    // About a plane found outside the sketch (a construction plane, a
    // face, another sketch's plane): its local plane given before the
    // solve; without it the relation is a reference failure.
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let p = e.add_point([10.0, 5.0, 3.0]).unwrap();
    e.fix(p).unwrap();
    let q = e.add_point([12.0, 4.0, 9.0]).unwrap();
    let outside = e
        .add_constraint(SpatialConstraintKind::Symmetric {
            a: p,
            b: q,
            plane: SpatialPlaneRef::Construction(FeatureUid(7)),
        })
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    assert_eq!(def.plane_records().count(), 1);
    let failure = solve(&def, &params).unwrap_err();
    assert_eq!(failure.kind, SpatialFailureKind::Reference, "{failure}");
    assert_eq!(failure.constraints, [outside]);
    let inputs = SpatialInputs {
        surfaces: BTreeMap::from([(
            outside,
            SpatialSurface::Plane {
                origin: [0.0, 0.0, 5.0],
                normal: [0.0, 0.0, 1.0],
            },
        )]),
        ..SpatialInputs::default()
    };
    let solved = solve_with(&def, &params, &inputs).unwrap();
    assert!(near(solved.points[&q], [10.0, 5.0, 7.0]), "{solved:?}");
    // A free point held elsewhere contradicts the mirror.
    let mut e = edit(def, &mut params);
    e.fix(q).unwrap();
    let (fixed, _, _) = e.finish().unwrap();
    let failure = solve_with(&fixed, &params, &inputs).unwrap_err();
    assert!(
        matches!(
            failure.kind,
            SpatialFailureKind::Conflict | SpatialFailureKind::NotConverged
        ),
        "{failure}"
    );
}

#[test]
fn mirrored_points_follow_a_drag() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let p = e.add_point([10.0, 5.0, 3.0]).unwrap();
    let q = e.add_point([-8.0, 4.0, -2.0]).unwrap();
    e.add_constraint(SpatialConstraintKind::Symmetric {
        a: p,
        b: q,
        plane: SpatialPlaneRef::Frame(SpatialAxis::X),
    })
    .unwrap();
    let (mut def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert_eq!(solved.status.dof, 3, "q follows p");
    assert!(!solved.status.fully_constrained);
    def.store_solved(&solved);

    // Either point dragged carries its mirror image along.
    let dragged = drag(&def, &params, &[(p, [20.0, 1.0, 2.0])]).unwrap();
    assert!(near(dragged.points[&p], [20.0, 1.0, 2.0]), "{dragged:?}");
    assert!(near(dragged.points[&q], [-20.0, 1.0, 2.0]), "{dragged:?}");
    let dragged = drag(&def, &params, &[(q, [-4.0, 9.0, -6.0])]).unwrap();
    assert!(near(dragged.points[&p], [4.0, 9.0, -6.0]), "{dragged:?}");

    // The same symmetry with the points the other way round follows from
    // the first.
    let mut e = edit(def, &mut params);
    let twice = e
        .add_constraint(SpatialConstraintKind::Symmetric {
            a: q,
            b: p,
            plane: SpatialPlaneRef::Frame(SpatialAxis::X),
        })
        .unwrap();
    let (twin, _, _) = e.finish().unwrap();
    let solved = solve(&twin, &params).unwrap();
    assert!(solved.status.redundant.contains(&twice), "{solved:?}");
    assert_eq!(solved.status.dof, 3);
}

#[test]
fn concentric_arcs_and_circles_with_or_without_their_planes() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let a = e
        .add_circle(At([0.0; 3]), 10.0, [0.0, 0.0, 1.0], None, false)
        .unwrap();
    let centre = e.def.curve(a).unwrap().points()[0];
    e.fix(centre).unwrap();
    let b = e
        .add_circle(At([5.0, 3.0, 1.0]), 20.0, [0.1, 0.2, 1.0], None, false)
        .unwrap();
    let (base, _, _) = e.finish().unwrap();
    assert_eq!(solve(&base, &params).unwrap().status.dof, 9);

    let mut e = edit(base.clone(), &mut params);
    e.add_constraint(SpatialConstraintKind::Concentric { a, b, normal: true })
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    let centre_b = def.curve(b).unwrap().points()[0];
    assert!(near(solved.points[&centre_b], [0.0; 3]));
    let normal = |c: EntityUid| match solved.curves.get(&c) {
        Some(SolvedCurve::Round { normal, .. }) => *normal,
        other => panic!("{other:?}"),
    };
    assert!(norm(cross(normal(a), normal(b))) <= TOL);
    assert_eq!(solved.status.dof, 4);
    reads_back(&def, &params);

    // The centre only: the planes stay free.
    let mut e = edit(base, &mut params);
    let only = e
        .add_constraint(SpatialConstraintKind::Concentric {
            a,
            b,
            normal: false,
        })
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert!(near(solved.points[&centre_b], [0.0; 3]));
    assert_eq!(solved.status.dof, 6);
    let json = serde_json::to_value(def.constraint(only).unwrap()).unwrap();
    assert_eq!(json["normal"], false, "{json}");
    // A line is no arc or circle.
    let mut e = edit(def, &mut params);
    let line = free_line(&mut e, [0.0; 3], [1.0, 2.0, 3.0]);
    e.add_constraint(SpatialConstraintKind::Concentric {
        a,
        b: line,
        normal: true,
    })
    .unwrap();
    let error = e.finish().unwrap_err();
    assert!(error.contains("two arcs or circles"), "{error}");
}

#[test]
fn concentric_circles_follow_a_drag_and_hold_against_fixed_centres() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let a = e
        .add_circle(At([0.0; 3]), 10.0, [0.0, 0.0, 1.0], None, false)
        .unwrap();
    let b = e
        .add_circle(At([5.0, 3.0, 1.0]), 20.0, [0.1, 0.2, 1.0], None, false)
        .unwrap();
    e.add_constraint(SpatialConstraintKind::Concentric { a, b, normal: true })
        .unwrap();
    let (mut def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    // Two centres 6, two normals 4, two radii 2, less the centre 3 and the
    // normal 2.
    assert_eq!(solved.status.dof, 7);
    def.store_solved(&solved);
    let (centre_a, centre_b) = (
        def.curve(a).unwrap().points()[0],
        def.curve(b).unwrap().points()[0],
    );

    // A drag of one centre carries the other; the planes stay parallel.
    let target = [4.0, -2.0, 6.0];
    let dragged = drag(&def, &params, &[(centre_a, target)]).unwrap();
    assert!(near(dragged.points[&centre_a], target), "{dragged:?}");
    assert!(near(dragged.points[&centre_b], target), "{dragged:?}");
    let normal = |c: EntityUid| match dragged.curves.get(&c) {
        Some(SolvedCurve::Round { normal, .. }) => *normal,
        other => panic!("{other:?}"),
    };
    assert!(norm(cross(normal(a), normal(b))) <= TOL);

    // The same relation the other way round follows from the first.
    let mut e = edit(def, &mut params);
    let twice = e
        .add_constraint(SpatialConstraintKind::Concentric {
            a: b,
            b: a,
            normal: true,
        })
        .unwrap();
    let (twin, _, _) = e.finish().unwrap();
    let solved = solve(&twin, &params).unwrap();
    assert!(solved.status.redundant.contains(&twice), "{solved:?}");
    assert_eq!(solved.status.dof, 7);

    // Two centres fixed apart cannot be one.
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let a = e
        .add_circle(At([0.0; 3]), 10.0, [0.0, 0.0, 1.0], None, false)
        .unwrap();
    let b = e
        .add_circle(At([5.0, 3.0, 1.0]), 20.0, [0.1, 0.2, 1.0], None, false)
        .unwrap();
    for c in [a, b] {
        let centre = e.def.curve(c).unwrap().points()[0];
        e.fix(centre).unwrap();
    }
    e.add_constraint(SpatialConstraintKind::Concentric {
        a,
        b,
        normal: false,
    })
    .unwrap();
    let (contradicting, _, _) = e.finish().unwrap();
    contradicts(&contradicting, &params);
}

/// `C''(0)` of a clamped B-spline from its control net and knots: the
/// derivative of its derivative curve.
fn second_derivative_at_start(net: &SpatialSplineGeometry) -> ([f64; 3], [f64; 3]) {
    let p = net.degree as f64;
    let (u, c) = (&net.knots, &net.control_points);
    let d = net.degree;
    let q0 = sub(c[1], c[0]).map(|x| p * x / (u[d + 1] - u[1]));
    let q1 = sub(c[2], c[1]).map(|x| p * x / (u[d + 2] - u[2]));
    let second = sub(q1, q0).map(|x| (p - 1.0) * x / (u[d + 1] - u[2]));
    (q0, second)
}

#[test]
fn curvature_continuity_where_a_spline_leaves_a_line() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let line = e
        .add_line(At([-20.0, 0.0, 0.0]), At([0.0, 0.0, 0.0]), false)
        .unwrap();
    let [a, b] = e.def.line(line).unwrap();
    e.fix(a).unwrap();
    e.fix(b).unwrap();
    let spline = e
        .add_spline(&SplineRequest::fit(vec![
            Existing(b),
            At([10.0, 5.0, 0.0]),
            At([20.0, 0.0, 5.0]),
            At([30.0, 5.0, 5.0]),
        ]))
        .unwrap();
    let g2 = e
        .add_constraint(SpatialConstraintKind::Curvature {
            a: line,
            b: spline.spline,
            at: b,
            sense: None,
        })
        .unwrap();
    let (mut def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    let SolvedCurve::Spline(net) = &solved.curves[&spline.spline] else {
        panic!("{:?}", solved.curves[&spline.spline]);
    };
    let (d1, d2) = second_derivative_at_start(net);
    let curvature = norm(cross(d1, d2)) / norm(d1).powi(3);
    assert!(curvature <= 1e-8, "{curvature}");
    assert!(
        d1[0] > 0.0 && d1[1].abs() <= TOL && d1[2].abs() <= TOL,
        "{d1:?}"
    );
    assert_eq!(solved.tangent_senses[&g2], SpatialSense::Positive);
    def.store_solved(&solved);
    assert!(matches!(
        def.constraint(g2).unwrap().kind,
        SpatialConstraintKind::Curvature {
            sense: Some(SpatialSense::Positive),
            ..
        }
    ));
    // The end's tangent handle can be dragged: it is held.
    assert!(def.tangent_handle(spline.spline, 0).is_ok());
    // A drag of a fit point keeps the continuity.
    let dragged = drag(&def, &params, &[(spline.points[2], [22.0, 4.0, 8.0])]).unwrap();
    let SolvedCurve::Spline(net) = &dragged.curves[&spline.spline] else {
        panic!("not a spline");
    };
    let (d1, d2) = second_derivative_at_start(net);
    assert!(norm(cross(d1, d2)) / norm(d1).powi(3) <= 1e-8);
    reads_back(&def, &params);
    // Two lines have no curvature continuity: a spline takes part.
    let mut e = edit(def, &mut params);
    let other = free_line(&mut e, [0.0, 0.0, 0.0], [0.0, 10.0, 0.0]);
    e.add_constraint(SpatialConstraintKind::Curvature {
        a: line,
        b: other,
        at: b,
        sense: None,
    })
    .unwrap();
    let error = e.finish().unwrap_err();
    assert!(error.contains("curvature continuity"), "{error}");
}

/// A fixed line along x to the origin and a cubic control-point spline
/// starting where it ends, its next control points at `p1` and `p2`.
fn line_and_spline(
    e: &mut SpatialEdit<'_>,
    p1: [f64; 3],
    p2: [f64; 3],
) -> (EntityUid, EntityUid, SplineMade) {
    let line = e
        .add_line(At([-20.0, 0.0, 0.0]), At([0.0, 0.0, 0.0]), false)
        .unwrap();
    let [a, b] = e.def.line(line).unwrap();
    e.fix(a).unwrap();
    e.fix(b).unwrap();
    let spline = e
        .add_spline(&SplineRequest::control(
            vec![Existing(b), At(p1), At(p2), At([30.0, 10.0, 5.0])],
            3,
        ))
        .unwrap();
    (line, b, spline)
}

#[test]
fn curvature_continuity_leaves_freedoms_and_meets_contradictions() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let (line, joint, spline) = line_and_spline(&mut e, [10.0, 2.0, 1.0], [20.0, 5.0, 0.0]);
    e.add_constraint(SpatialConstraintKind::Curvature {
        a: line,
        b: spline.spline,
        at: joint,
        sense: None,
    })
    .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    let SolvedCurve::Spline(net) = &solved.curves[&spline.spline] else {
        panic!("{:?}", solved.curves[&spline.spline]);
    };
    let (d1, d2) = second_derivative_at_start(net);
    assert!(norm(cross(d1, d2)) / norm(d1).powi(3) <= 1e-8);
    // Under-constrained: the second control point slides along the line's
    // direction, the third along it too (the second derivative along the
    // tangent), the last is free: 1 + 1 + 3.
    assert_eq!(solved.status.dof, 5, "{:?}", solved.status);
    assert!(!solved.status.fully_constrained);
    assert!(!solved.status.determined_curves.contains(&spline.spline));
    reads_back(&def, &params);

    // The same continuity again follows from the first.
    let mut e = edit(def, &mut params);
    let twice = e
        .add_constraint(SpatialConstraintKind::Curvature {
            a: line,
            b: spline.spline,
            at: joint,
            sense: None,
        })
        .unwrap();
    let (twin, _, _) = e.finish().unwrap();
    let solved = solve(&twin, &params).unwrap();
    assert!(solved.status.redundant.contains(&twice), "{solved:?}");
    assert_eq!(solved.status.dof, 5);

    // Control points fixed so that the spline bends where it leaves the
    // line: tangent there, but its curvature cannot be the line's.
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let (line, joint, spline) = line_and_spline(&mut e, [10.0, 0.0, 0.0], [20.0, 6.0, 0.0]);
    e.fix(spline.points[1]).unwrap();
    e.fix(spline.points[2]).unwrap();
    let g2 = e
        .add_constraint(SpatialConstraintKind::Curvature {
            a: line,
            b: spline.spline,
            at: joint,
            sense: None,
        })
        .unwrap();
    let (contradicting, _, _) = e.finish().unwrap();
    let failure = solve(&contradicting, &params).unwrap_err();
    assert!(
        matches!(
            failure.kind,
            SpatialFailureKind::Conflict | SpatialFailureKind::NotConverged
        ),
        "{failure}"
    );
    if failure.kind == SpatialFailureKind::Conflict {
        assert!(failure.constraints.contains(&g2), "{failure:?}");
    }
}

#[test]
fn points_are_kept_on_another_plane() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let line = e
        .add_line(At([4.0, 1.0, 2.0]), At([9.0, 7.0, 3.0]), false)
        .unwrap();
    let ends = e.def.line(line).unwrap();
    let on = e
        .add_constraint(SpatialConstraintKind::OnPlane {
            points: ends.to_vec(),
            plane: SpatialPlaneRef::Frame(SpatialAxis::X),
        })
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    for p in ends {
        assert!(solved.points[&p][0].abs() <= TOL, "{solved:?}");
    }
    assert_eq!(solved.status.dof, 4, "each point slides on the plane");
    let dragged = drag(&def, &params, &[(ends[1], [3.0, 20.0, -4.0])]).unwrap();
    assert!(near(dragged.points[&ends[1]], [0.0, 20.0, -4.0]));
    reads_back(&def, &params);
    // A face's plane found outside the sketch.
    let mut e = edit(def, &mut params);
    e.remove_relation(on).unwrap();
    let outside = e
        .add_constraint(SpatialConstraintKind::OnPlane {
            points: vec![ends[0]],
            plane: SpatialPlaneRef::Face(SpatialSource::Face {
                body: crate::ids::BodyUid::new(FeatureUid(2), 0),
                face: Box::new("F2:top".parse().unwrap()),
                link: None,
            }),
        })
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let inputs = SpatialInputs {
        surfaces: BTreeMap::from([(
            outside,
            SpatialSurface::Plane {
                origin: [0.0, 0.0, 12.0],
                normal: [0.0, 0.0, 1.0],
            },
        )]),
        ..SpatialInputs::default()
    };
    let solved = solve_with(&def, &params, &inputs).unwrap();
    assert!((solved.points[&ends[0]][2] - 12.0).abs() <= TOL);
    // A point source is no plane.
    let mut e = edit(def, &mut params);
    e.add_constraint(SpatialConstraintKind::OnPlane {
        points: vec![ends[1]],
        plane: SpatialPlaneRef::Face(SpatialSource::Origin),
    })
    .unwrap();
    let error = e.finish().unwrap_err();
    assert!(error.contains("plane of a relation"), "{error}");
}

#[test]
fn on_plane_twice_is_redundant_and_against_a_fixed_point_contradicts() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let line = e
        .add_line(At([4.0, 1.0, 2.0]), At([9.0, 7.0, 3.0]), false)
        .unwrap();
    let ends = e.def.line(line).unwrap();
    e.add_constraint(SpatialConstraintKind::OnPlane {
        points: ends.to_vec(),
        plane: SpatialPlaneRef::Frame(SpatialAxis::X),
    })
    .unwrap();
    let twice = e
        .add_constraint(SpatialConstraintKind::OnPlane {
            points: vec![ends[1]],
            plane: SpatialPlaneRef::Frame(SpatialAxis::X),
        })
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert!(solved.status.redundant.contains(&twice), "{solved:?}");
    assert_eq!(solved.status.dof, 4);

    // A point fixed off the plane cannot be on it (the stored start is at
    // x = 4).
    let mut e = edit(def, &mut params);
    e.fix(ends[0]).unwrap();
    let (contradicting, _, _) = e.finish().unwrap();
    contradicts(&contradicting, &params);
}

#[test]
fn a_spline_length_and_a_tangent_magnitude_drive() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let spline = e
        .add_spline(&SplineRequest::control(
            vec![
                At([0.0; 3]),
                At([10.0, 0.0, 0.0]),
                At([20.0, 10.0, 0.0]),
                At([30.0, 10.0, 5.0]),
            ],
            3,
        ))
        .unwrap();
    for p in &spline.points[..3] {
        e.fix(*p).unwrap();
    }
    // Driving at its length now: the geometry stays.
    let length = e
        .add_dimension(
            SpatialDimensionKind::SplineLength {
                curve: spline.spline,
            },
            None,
            false,
        )
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let before = def.position(spline.points[3]).unwrap();
    let solved = solve(&def, &params).unwrap();
    assert!(near(solved.points[&spline.points[3]], before));
    assert_eq!(solved.status.dof, 2);
    // A new value moves the free end; the measure agrees with it.
    let mut e = edit(def, &mut params);
    e.set_dimension_value(length, &ValueInput::Number(45.0))
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert!(
        (solved.status.measured[&length] - 45.0).abs() <= 1e-6,
        "{solved:?}"
    );
    reads_back(&def, &params);

    // The start's tangent magnitude: 3 |P1 - P0| on a single cubic span.
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let spline = e
        .add_spline(&SplineRequest::control(
            vec![
                At([0.0; 3]),
                At([8.0, 2.0, 0.0]),
                At([20.0, 10.0, 0.0]),
                At([30.0, 10.0, 5.0]),
            ],
            3,
        ))
        .unwrap();
    e.fix(spline.points[0]).unwrap();
    let magnitude = e
        .add_dimension(
            SpatialDimensionKind::TangentMagnitude {
                curve: spline.spline,
                end: SpatialCurveEnd::Start,
            },
            Some(&ValueInput::Number(30.0)),
            false,
        )
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    let leg = sub(
        solved.points[&spline.points[1]],
        solved.points[&spline.points[0]],
    );
    assert!((norm(leg) - 10.0).abs() <= 1e-6, "{leg:?}");
    assert!((solved.status.measured[&magnitude] - 30.0).abs() <= 1e-6);
    let named = def
        .dimension(magnitude)
        .unwrap()
        .map_value(&mut |_: &str, id: &crate::parameters::ParamId| Ok::<_, ()>(params.name(*id)))
        .unwrap();
    let json = serde_json::to_value(&named).unwrap();
    assert_eq!(json["type"], "tangent_magnitude", "{json}");
    assert_eq!(json["end"], "start", "{json}");

    // A fit-point spline's natural end has its tangent already.
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let fit = e
        .add_spline(&SplineRequest::fit(vec![
            At([0.0; 3]),
            At([10.0, 5.0, 0.0]),
            At([20.0, 0.0, 5.0]),
        ]))
        .unwrap();
    e.add_dimension(
        SpatialDimensionKind::TangentMagnitude {
            curve: fit.spline,
            end: SpatialCurveEnd::End,
        },
        Some(&ValueInput::Number(20.0)),
        false,
    )
    .unwrap();
    let error = e.finish().unwrap_err();
    assert!(error.contains("natural end"), "{error}");
}

#[test]
fn a_spline_length_holds_through_a_drag() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let spline = e
        .add_spline(&SplineRequest::control(
            vec![
                At([0.0; 3]),
                At([10.0, 0.0, 0.0]),
                At([20.0, 10.0, 0.0]),
                At([30.0, 10.0, 5.0]),
            ],
            3,
        ))
        .unwrap();
    for p in &spline.points[..3] {
        e.fix(*p).unwrap();
    }
    let curve = spline.spline;
    let length = e
        .add_dimension(
            SpatialDimensionKind::SplineLength { curve },
            Some(&ValueInput::Number(45.0)),
            false,
        )
        .unwrap();
    let (mut def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert!((solved.status.measured[&length] - 45.0).abs() <= 1e-6);
    assert_eq!(solved.status.dof, 2, "the free end slides on a surface");
    def.store_solved(&solved);

    // A drag of the free end keeps the length; the fixed points stay.
    let end = spline.points[3];
    let dragged = drag(&def, &params, &[(end, [40.0, 25.0, 10.0])]).unwrap();
    assert!(
        (dragged.status.measured[&length] - 45.0).abs() <= 1e-6,
        "{dragged:?}"
    );
    assert!(!near(dragged.points[&end], solved.points[&end]));
    for p in &spline.points[..3] {
        assert!(near(dragged.points[p], solved.points[p]));
    }

    // The same length again follows from the first; another contradicts it.
    let mut e = edit(def.clone(), &mut params);
    let twice = e
        .add_dimension(
            SpatialDimensionKind::SplineLength { curve },
            Some(&ValueInput::Number(45.0)),
            false,
        )
        .unwrap();
    let (twin, _, _) = e.finish().unwrap();
    let solved = solve(&twin, &params).unwrap();
    assert!(solved.status.redundant.contains(&twice), "{solved:?}");
    assert_eq!(solved.status.dof, 2);
    let mut e = edit(def, &mut params);
    e.add_dimension(
        SpatialDimensionKind::SplineLength { curve },
        Some(&ValueInput::Number(50.0)),
        false,
    )
    .unwrap();
    let (contradicting, _, _) = e.finish().unwrap();
    contradicts(&contradicting, &params);
}

#[test]
fn a_tangent_magnitude_holds_through_a_drag() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let spline = e
        .add_spline(&SplineRequest::control(
            vec![
                At([0.0; 3]),
                At([8.0, 2.0, 0.0]),
                At([20.0, 10.0, 0.0]),
                At([30.0, 10.0, 5.0]),
            ],
            3,
        ))
        .unwrap();
    e.fix(spline.points[0]).unwrap();
    let (curve, end) = (spline.spline, SpatialCurveEnd::Start);
    let magnitude = e
        .add_dimension(
            SpatialDimensionKind::TangentMagnitude { curve, end },
            Some(&ValueInput::Number(30.0)),
            false,
        )
        .unwrap();
    let (mut def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    // Under-constrained: the second control point on a sphere about the
    // first (2), the other two free (6).
    assert_eq!(solved.status.dof, 8, "{:?}", solved.status);
    assert!(!solved.status.fully_constrained);
    def.store_solved(&solved);

    // A drag of the second control point toward +y: it stays 10 from the
    // start (3 |P1 - P0| = 30), along the pull.
    let (p0, p1) = (spline.points[0], spline.points[1]);
    let dragged = drag(&def, &params, &[(p1, [0.0, 20.0, 0.0])]).unwrap();
    let leg = sub(dragged.points[&p1], dragged.points[&p0]);
    assert!((norm(leg) - 10.0).abs() <= 1e-6, "{leg:?}");
    assert!(leg[1] / norm(leg) >= 1.0 - 1e-6, "{leg:?}");
    assert!((dragged.status.measured[&magnitude] - 30.0).abs() <= 1e-6);
    assert!(near(dragged.points[&p0], [0.0; 3]));

    // The same magnitude again follows from the first; another contradicts
    // it.
    let mut e = edit(def.clone(), &mut params);
    let twice = e
        .add_dimension(
            SpatialDimensionKind::TangentMagnitude { curve, end },
            Some(&ValueInput::Number(30.0)),
            false,
        )
        .unwrap();
    let (twin, _, _) = e.finish().unwrap();
    let solved = solve(&twin, &params).unwrap();
    assert!(solved.status.redundant.contains(&twice), "{solved:?}");
    assert_eq!(solved.status.dof, 8);
    let mut e = edit(def, &mut params);
    e.add_dimension(
        SpatialDimensionKind::TangentMagnitude { curve, end },
        Some(&ValueInput::Number(24.0)),
        false,
    )
    .unwrap();
    let (contradicting, _, _) = e.finish().unwrap();
    contradicts(&contradicting, &params);
}

#[test]
fn driven_dimensions_get_reference_parameters() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::new(SketchPlane::Xy), &mut params);
    let line = free_line(&mut e, [0.0; 3], [30.0, 40.0, 0.0]);
    let driven = e
        .add_dimension(SpatialDimensionKind::Length { line }, None, true)
        .unwrap();
    let (def, report, _) = e.finish().unwrap();
    let reference = def.dimension(driven).unwrap().reference.unwrap();
    assert!(params.is_reference(reference));
    assert!(
        report.parameters.contains(&params.name(reference)),
        "{report:?}"
    );
    // Made driving, it gives its reference up; a driving one has none.
    let mut e = edit(def, &mut params);
    e.set_driven(driven, false).unwrap();
    let (def, _, _) = e.finish().unwrap();
    assert!(def.dimension(driven).unwrap().reference.is_none());
    assert!(def.dimension(driven).unwrap().value.is_some());
    reads_back(&def, &params);
}
