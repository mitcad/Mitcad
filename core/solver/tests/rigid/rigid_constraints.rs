// SPDX-License-Identifier: MIT
//! Assembly constraints in the joint solver (mitcad#148): each part's
//! equations, values with limits and targets, the turn over of z and x
//! axes that point the wrong way, constraints solved with joints, the
//! degrees of freedom they leave, drags, and over-constrained and
//! conflicting sets.

use std::f64::consts::{FRAC_PI_2, FRAC_PI_6, PI};

use mitcad_solver::Freedom::{self, *};
use mitcad_solver::{
    Amount, BodyId, ConstraintPart as P, JointId, JointMotion, Pose, RigidConstraint, RigidJoint,
    RigidOptions, RigidSystem, Sense, SolveStatus,
};

type V = [f64; 3];

/// A frame at `origin` with the unit axes `z` and `x` (y = z × x).
fn frame(origin: V, z: V, x: V) -> Pose {
    let y = [
        z[1] * x[2] - z[2] * x[1],
        z[2] * x[0] - z[0] * x[2],
        z[0] * x[1] - z[1] * x[0],
    ];
    Pose {
        rotation: std::array::from_fn(|r| [x[r], y[r], z[r]]),
        translation: origin,
    }
}

fn at(origin: V) -> Pose {
    Pose::translation(origin)
}

fn opts() -> RigidOptions {
    RigidOptions::default()
}

fn constraint(
    a: Option<BodyId>,
    frame_a: Pose,
    b: Option<BodyId>,
    frame_b: Pose,
    parts: &[P],
) -> RigidConstraint {
    RigidConstraint {
        a,
        frame_a,
        b,
        frame_b,
        parts: parts.to_vec(),
        value: None,
    }
}

fn near(a: V, b: V, tol: f64) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < tol)
}

fn assert_near(a: V, b: V) {
    assert!(near(a, b, 1e-7), "{a:?} != {b:?}");
}

fn column(p: &Pose, c: usize) -> V {
    std::array::from_fn(|r| p.rotation[r][c])
}

/// Where frame `f` of `body` is now.
fn now(s: &RigidSystem, body: BodyId, f: &Pose) -> Pose {
    s.body_pose(body).after(f)
}

fn assert_holds(s: &RigidSystem, ids: &[JointId]) {
    for j in ids {
        let (d, angle) = s.joint_error(*j);
        assert!(d < 1e-7 && angle < 1e-6, "{j:?}: {d} mm, {angle} rad");
    }
}

/// A turned start, far from where the constraints put the body.
fn tilted() -> Pose {
    Pose::rotation([3.0, -2.0, 7.0], [0.3, -0.5, 0.8], 0.9).after(&at([40.0, -15.0, 5.0]))
}

/// The insert of a circle (frame a, on the body) into a circle of the world
/// (frame b): centres together, planes facing.
fn insert_parts(lock: Option<f64>) -> Vec<P> {
    let mut parts = vec![
        P::OriginX,
        P::OriginY,
        P::OriginZ(Amount::Fixed(0.0)),
        P::Parallel(Sense::Opposite),
    ];
    parts.extend(lock.map(P::Twist));
    parts
}

#[test]
fn planes_align_facing_or_the_same_way_with_an_offset() {
    let fb = frame([0.0, 0.0, 10.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    for (sense, offset) in [
        (Sense::Opposite, 0.0),
        (Sense::Same, 5.0),
        (Sense::Opposite, -2.5),
    ] {
        let mut s = RigidSystem::new();
        let body = s.add_body(false);
        let fa = tilted().after(&frame([1.0, 2.0, 3.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]));
        let c = s
            .add_constraint(constraint(
                Some(body),
                fa,
                None,
                fb,
                &[P::OriginZ(Amount::Fixed(offset)), P::Parallel(sense)],
            ))
            .unwrap();
        let r = s.solve(&opts());
        assert!(r.is_ok(), "{sense:?}: {r:?}");
        assert_holds(&s, &[c]);
        let placed = now(&s, body, &fa);
        assert!(
            (placed.translation[2] - 10.0 - offset).abs() < 1e-8,
            "{placed:?}"
        );
        let z = column(&placed, 2)[2];
        let want = if sense == Sense::Same { 1.0 } else { -1.0 };
        assert!((z - want).abs() < 1e-8, "{sense:?}: z {z}");
        // A plane on a plane leaves sliding in it and turning about its
        // normal.
        let a = s.analyze();
        assert_eq!(a.body_dof, vec![3], "{a:?}");
        assert!(a.redundant.is_empty() && a.conflicts.is_empty());
    }
}

#[test]
fn a_body_upside_down_is_turned_over() {
    // Frame a starts with its z axis along b's, but the constraint wants
    // them facing: the parallel equations hold both ways, the solve turns
    // the body over.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let fa = at([5.0, 5.0, 20.0]);
    let fb = at([0.0; 3]);
    let c = s
        .add_constraint(constraint(
            Some(body),
            fa,
            None,
            fb,
            &[P::OriginZ(Amount::Fixed(0.0)), P::Parallel(Sense::Opposite)],
        ))
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert_holds(&s, &[c]);
    let placed = now(&s, body, &fa);
    assert!((column(&placed, 2)[2] + 1.0).abs() < 1e-9);
    // Without an iteration's help as well (no placement first).
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let c = s
        .add_constraint(constraint(
            Some(body),
            fa,
            None,
            fb,
            &[P::OriginZ(Amount::Fixed(0.0)), P::Parallel(Sense::Opposite)],
        ))
        .unwrap();
    let r = s.solve(&RigidOptions {
        place: false,
        ..opts()
    });
    assert!(r.is_ok(), "{r:?}");
    assert_holds(&s, &[c]);
}

#[test]
fn points_and_axes_align() {
    // Coincident points leave the three turns; at a distance, also two
    // motions on the sphere.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let fa = tilted().after(&at([1.0, 1.0, 1.0]));
    let c = s
        .add_constraint(constraint(
            Some(body),
            fa,
            None,
            at([10.0, 0.0, 0.0]),
            &[P::OriginX, P::OriginY, P::OriginZ(Amount::Fixed(0.0))],
        ))
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert_holds(&s, &[c]);
    assert_near(now(&s, body, &fa).translation, [10.0, 0.0, 0.0]);
    assert_eq!(s.analyze().body_dof, vec![3]);

    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let c = s
        .add_constraint(constraint(
            Some(body),
            fa,
            None,
            at([10.0, 0.0, 0.0]),
            &[P::Distance(Amount::Fixed(4.0))],
        ))
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert_holds(&s, &[c]);
    let d = now(&s, body, &fa).translation;
    let dist = ((d[0] - 10.0).powi(2) + d[1].powi(2) + d[2].powi(2)).sqrt();
    assert!((dist - 4.0).abs() < 1e-8);
    assert_eq!(s.analyze().body_dof, vec![5]);

    // Axes along each other: a cylindrical pair, two motions left.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let fa = tilted().after(&frame([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
    let fb = frame([0.0, 5.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
    let c = s
        .add_constraint(constraint(
            Some(body),
            fa,
            None,
            fb,
            &[P::OriginX, P::OriginY, P::Parallel(Sense::Same)],
        ))
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert_holds(&s, &[c]);
    let placed = now(&s, body, &fa);
    assert!(placed.translation[0].abs() < 1e-8 && placed.translation[2].abs() < 1e-8);
    assert!((column(&placed, 2)[1] - 1.0).abs() < 1e-9);
    assert_eq!(s.analyze().body_dof, vec![2]);
}

#[test]
fn an_insert_turns_freely_or_keeps_its_turn() {
    let fb = frame([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let fa = tilted().after(&frame([2.0, 3.0, 4.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]));
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let c = s
        .add_constraint(constraint(Some(body), fa, None, fb, &insert_parts(None)))
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert_holds(&s, &[c]);
    assert_near(now(&s, body, &fa).translation, [0.0; 3]);
    let a = s.analyze();
    assert_eq!(a.body_dof, vec![1]);
    assert_eq!(a.dof, 1);

    // Locked at 30°: frame a's x axis 30° about b's z from b's x.
    for lock in [FRAC_PI_6, PI - 0.2, -FRAC_PI_2] {
        let mut s = RigidSystem::new();
        let body = s.add_body(false);
        let c = s
            .add_constraint(constraint(
                Some(body),
                fa,
                None,
                fb,
                &insert_parts(Some(lock)),
            ))
            .unwrap();
        assert!(s.solve(&opts()).is_ok());
        assert_holds(&s, &[c]);
        let x = column(&now(&s, body, &fa), 0);
        assert_near(x, [lock.cos(), lock.sin(), 0.0]);
        assert_eq!(s.analyze().body_dof, vec![0]);
    }
}

#[test]
fn an_axis_insert_slides_along_the_hole_when_dragged() {
    // A circle on the hole's axis, depth and turn free: a drag along the
    // axis moves it, one across it does not.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let fa = frame([20.0, 0.0, 30.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let fb = frame([0.0, 0.0, 10.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let c = s
        .add_constraint(constraint(
            Some(body),
            fa,
            None,
            fb,
            &[P::OriginX, P::OriginY, P::Parallel(Sense::Same)],
        ))
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    // The depth stays where it was.
    assert_near(now(&s, body, &fa).translation, [0.0, 0.0, 30.0]);
    assert_eq!(s.analyze().body_dof, vec![2]);
    let r = s.drag(body, [0.0, 0.0, 30.0], [0.0, 0.0, 45.0], &opts());
    assert!(r.is_ok(), "{r:?}");
    assert_holds(&s, &[c]);
    assert_near(now(&s, body, &fa).translation, [0.0, 0.0, 45.0]);
    let r = s.drag(body, [0.0, 0.0, 45.0], [10.0, 0.0, 45.0], &opts());
    assert!(r.is_ok(), "{r:?}");
    assert_holds(&s, &[c]);
    let t = now(&s, body, &fa).translation;
    assert!(t[0].abs() < 1e-7 && t[1].abs() < 1e-7, "{t:?}");
}

#[test]
fn an_angle_sets_a_hinge() {
    // A lid on a revolute joint about the world's x axis, its normal at an
    // angle to the world's z: the hinge turns to it.
    for angle in [FRAC_PI_6, FRAC_PI_2, 2.5] {
        let mut s = RigidSystem::new();
        let lid = s.add_body(false);
        let hinge = s
            .add_joint(RigidJoint {
                a: Some(lid),
                frame_a: frame([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
                b: None,
                frame_b: frame([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
                motions: vec![JointMotion::free(Rz)],
                alignment: Pose::IDENTITY,
            })
            .unwrap();
        // The lid's top face starts parallel to the world's xy-plane: the
        // angle's derivative is zero there, the placement turns it first.
        let top = frame([0.0, 10.0, 1.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        let c = s
            .add_constraint(constraint(
                Some(lid),
                top,
                None,
                at([0.0; 3]),
                &[P::Angle(Amount::Fixed(angle))],
            ))
            .unwrap();
        let r = s.solve(&opts());
        assert!(r.is_ok(), "{angle}: {r:?}");
        assert_holds(&s, &[hinge, c]);
        let z = column(&now(&s, lid, &top), 2);
        assert!((z[2] - angle.cos()).abs() < 1e-8, "{angle}: {z:?}");
        assert_eq!(s.analyze().dof, 0);
    }
}

#[test]
fn a_value_stays_within_its_limits_and_goes_to_its_target() {
    // A plane above a plane, between 2 and 8: from 20 it comes down to 8,
    // from 5 it stays; with a rest value of 3 it goes there.
    let fb = at([0.0; 3]);
    let solve = |height: f64, target: Option<f64>| -> (f64, Vec<f64>, usize) {
        let mut s = RigidSystem::new();
        let body = s.add_body(false);
        let fa = frame([1.0, 1.0, height], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]);
        let c = s
            .add_constraint(RigidConstraint {
                value: Some(JointMotion {
                    freedom: Tz,
                    min: Some(2.0),
                    max: Some(8.0),
                    target,
                }),
                ..constraint(
                    Some(body),
                    fa,
                    None,
                    fb,
                    &[P::OriginZ(Amount::Value), P::Parallel(Sense::Opposite)],
                )
            })
            .unwrap();
        let r = s.solve(&opts());
        assert!(r.is_ok(), "{r:?}");
        assert_holds(&s, &[c]);
        let z = now(&s, body, &fa).translation[2];
        (z, s.joint_values(c).to_vec(), s.analyze().body_dof[0])
    };
    let (z, values, dof) = solve(20.0, None);
    assert!((z - 8.0).abs() < 1e-8 && (values[0] - 8.0).abs() < 1e-8);
    // Free within its limits: the offset is a motion too.
    assert_eq!(dof, 4);
    let (z, _, _) = solve(5.0, None);
    assert!((z - 5.0).abs() < 1e-8);
    let (z, _, dof) = solve(5.0, Some(3.0));
    assert!((z - 3.0).abs() < 1e-8);
    assert_eq!(dof, 4, "targets do not count");

    // Limits in the wrong order are refused.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let bad = RigidConstraint {
        value: Some(JointMotion {
            freedom: Tz,
            min: Some(8.0),
            max: Some(2.0),
            target: None,
        }),
        ..constraint(
            Some(body),
            at([0.0; 3]),
            None,
            fb,
            &[P::OriginZ(Amount::Value)],
        )
    };
    assert!(s.add_constraint(bad).is_err());
    // A part holding a value the constraint does not have.
    let bad = constraint(
        Some(body),
        at([0.0; 3]),
        None,
        fb,
        &[P::OriginZ(Amount::Value)],
    );
    assert!(s.add_constraint(bad).is_err());
}

#[test]
fn tangent_parts_hold_a_cylinder_on_a_plane() {
    // A roller's axis (frame a) across the world's z, 6 above its xy-plane.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let fa = tilted().after(&frame([0.0; 3], [0.0, 0.6, 0.8], [1.0, 0.0, 0.0]));
    let c = s
        .add_constraint(constraint(
            Some(body),
            fa,
            None,
            at([0.0; 3]),
            &[P::Perpendicular, P::OriginZ(Amount::Fixed(6.0))],
        ))
        .unwrap();
    let r = s.solve(&opts());
    assert!(r.is_ok(), "{r:?}");
    assert_holds(&s, &[c]);
    let placed = now(&s, body, &fa);
    assert!(column(&placed, 2)[2].abs() < 1e-8);
    assert!((placed.translation[2] - 6.0).abs() < 1e-8);
    // Rolling and sliding on the plane: four motions.
    assert_eq!(s.analyze().body_dof, vec![4]);

    // Two cylinders side by side: axes along each other, 5 + 3 apart.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let fa = tilted().after(&frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]));
    let c = s
        .add_constraint(constraint(
            Some(body),
            fa,
            None,
            at([0.0; 3]),
            &[P::Parallel(Sense::Either), P::Radial(Amount::Fixed(8.0))],
        ))
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert_holds(&s, &[c]);
    let t = now(&s, body, &fa).translation;
    assert!(((t[0] * t[0] + t[1] * t[1]).sqrt() - 8.0).abs() < 1e-8);
}

#[test]
fn constraints_solve_with_joints() {
    // A slider along the world's x carries a block; an insert of a pin
    // (a second body) into the block's hole, and the pin's top face 7 below
    // a fixed plane: the slider moves so that the pin reaches the plane.
    let mut s = RigidSystem::new();
    let block = s.add_body(false);
    let pin = s.add_body(false);
    let slide = s
        .add_joint(RigidJoint {
            a: Some(block),
            frame_a: frame([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            b: None,
            frame_b: frame([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            motions: vec![JointMotion::free(Tz)],
            alignment: Pose::IDENTITY,
        })
        .unwrap();
    let hole = frame([10.0, 0.0, 5.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let pin_bottom = frame([50.0, 30.0, 40.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]);
    let insert = s
        .add_constraint(constraint(
            Some(pin),
            pin_bottom,
            Some(block),
            hole,
            &insert_parts(None),
        ))
        .unwrap();
    // The pin's axis (along its bottom's normal, upwards) on a fixed axis
    // parallel to z through x = 25.
    let pin_axis = frame([50.0, 30.0, 40.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let guide = frame([25.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let on_axis = s
        .add_constraint(constraint(
            Some(pin),
            pin_axis,
            None,
            guide,
            &[P::OriginX, P::OriginY],
        ))
        .unwrap();
    let r = s.solve(&opts());
    assert!(r.is_ok(), "{r:?}");
    assert_holds(&s, &[slide, insert, on_axis]);
    // The hole's centre came to x = 25: the slider moved 15.
    assert!(
        (s.joint_values(slide)[0] - 15.0).abs() < 1e-7,
        "{:?}",
        s.joint_values(slide)
    );
    assert_near(now(&s, pin, &pin_bottom).translation, [25.0, 0.0, 5.0]);
    // Left: the pin's turn about its axis.
    assert_eq!(s.analyze().dof, 1);
}

#[test]
fn over_constrained_sets_are_reported() {
    // Two planes of the body on the same fixed plane, 10 apart on the
    // body: they cannot both hold; the newer one is reported with both.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let parts = [P::OriginZ(Amount::Fixed(0.0)), P::Parallel(Sense::Opposite)];
    let bottom = frame([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]);
    let raised = frame([0.0, 0.0, 10.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]);
    let c1 = s
        .add_constraint(constraint(Some(body), bottom, None, at([0.0; 3]), &parts))
        .unwrap();
    let c2 = s
        .add_constraint(constraint(Some(body), raised, None, at([0.0; 3]), &parts))
        .unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.status, SolveStatus::Conflicting, "{r:?}");
    assert!(
        r.conflicts
            .iter()
            .any(|c| c.joint == c2 && c.involved.contains(&c1)),
        "{r:?}"
    );
    // The same plane twice is redundant: it holds.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let c1 = s
        .add_constraint(constraint(Some(body), bottom, None, at([0.0; 3]), &parts))
        .unwrap();
    let c2 = s
        .add_constraint(constraint(Some(body), bottom, None, at([0.0; 3]), &parts))
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    let a = s.analyze();
    assert!(
        a.redundant
            .iter()
            .any(|d| d.joint == c2 && d.involved.contains(&c1)),
        "{a:?}"
    );
    assert!(a.conflicts.is_empty());

    // A constraint against a rigid joint that holds the body elsewhere.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let j = s
        .add_joint(RigidJoint {
            a: Some(body),
            frame_a: at([0.0; 3]),
            b: None,
            frame_b: at([0.0, 0.0, 30.0]),
            motions: Vec::new(),
            alignment: Pose::IDENTITY,
        })
        .unwrap();
    let c = s
        .add_constraint(constraint(Some(body), bottom, None, at([0.0; 3]), &parts))
        .unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.status, SolveStatus::Conflicting, "{r:?}");
    assert!(
        r.conflicts
            .iter()
            .any(|d| d.joint == c && d.involved.contains(&j)),
        "{r:?}"
    );
    // A conflicting value target is named.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    s.add_joint(RigidJoint {
        a: Some(body),
        frame_a: at([0.0; 3]),
        b: None,
        frame_b: at([0.0, 0.0, 30.0]),
        motions: vec![JointMotion::free(Freedom::Rz)],
        alignment: Pose::IDENTITY,
    })
    .unwrap();
    let c = s
        .add_constraint(RigidConstraint {
            value: Some(JointMotion {
                target: Some(5.0),
                ..JointMotion::free(Tz)
            }),
            ..constraint(
                Some(body),
                bottom,
                None,
                at([0.0; 3]),
                &[P::OriginZ(Amount::Value)],
            )
        })
        .unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.status, SolveStatus::Conflicting, "{r:?}");
    assert!(
        r.conflicts.iter().any(|d| d.targets.contains(&(c, Tz))),
        "{r:?}"
    );
}

#[test]
fn a_constraint_reports_its_error_and_measure() {
    let c = RigidConstraint {
        value: Some(JointMotion {
            min: Some(0.0),
            max: Some(4.0),
            ..JointMotion::free(Tz)
        }),
        ..constraint(
            None,
            frame([3.0, 0.0, 6.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]),
            None,
            at([0.0; 3]),
            &[P::OriginZ(Amount::Value), P::Parallel(Sense::Opposite)],
        )
    };
    assert_eq!(c.measured(), Some(6.0));
    let (d, angle) = c.error();
    assert!((d - 2.0).abs() < 1e-12 && angle < 1e-12, "{d} {angle}");
    // Pointing the same way is half a turn off.
    let same = RigidConstraint {
        frame_a: at([3.0, 0.0, 2.0]),
        ..c.clone()
    };
    let (d, angle) = same.error();
    assert!(d < 1e-12 && (angle - PI).abs() < 1e-12, "{d} {angle}");
}
