// SPDX-License-Identifier: MIT
//! The joint solver: rigid bodies joined by joints (mitcad#55). Each
//! kind's free motions, values driven to targets, limits, conflicts and
//! redundant joints, the degrees of freedom from the rank, chains with
//! loops (a four-bar linkage, a slider-crank), fixed and free bodies,
//! drags, and convergence from bad starting placements.

use std::f64::consts::{FRAC_PI_2, FRAC_PI_3, PI};

use mitcad_solver::Freedom::{self, *};
use mitcad_solver::{
    BodyId, JointId, JointMotion, Pose, RigidJoint, RigidOptions, RigidSystem, SolveStatus,
    motion_pose,
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

/// A frame along the model's axes.
fn at(origin: V) -> Pose {
    Pose::translation(origin)
}

fn free(motions: &[Freedom]) -> Vec<JointMotion> {
    motions.iter().map(|m| JointMotion::free(*m)).collect()
}

fn joint(
    a: Option<BodyId>,
    frame_a: Pose,
    b: Option<BodyId>,
    frame_b: Pose,
    m: &[Freedom],
) -> RigidJoint {
    RigidJoint {
        a,
        frame_a,
        b,
        frame_b,
        motions: free(m),
        alignment: Pose::IDENTITY,
    }
}

fn opts() -> RigidOptions {
    RigidOptions::default()
}

fn near(a: V, b: V, tol: f64) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < tol)
}

fn assert_near(a: V, b: V) {
    assert!(near(a, b, 1e-8), "{a:?} != {b:?}");
}

fn dist(a: V, b: V) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

/// Where a point given at the body's start is now.
fn moved(s: &RigidSystem, body: BodyId, p: V) -> V {
    s.body_pose(body).apply_point(p)
}

fn assert_holds(s: &RigidSystem, joints: &[JointId]) {
    for j in joints {
        let (d, angle) = s.joint_error(*j);
        assert!(d < 1e-8 && angle < 1e-8, "joint {j:?}: {d} mm, {angle} rad");
    }
}

/// A turn about a skew axis through a point.
fn skew_turn(angle: f64) -> Pose {
    Pose::rotation([3.0, -2.0, 7.0], [0.3, -0.5, 0.8], angle)
}

const KINDS: [&[Freedom]; 9] = [
    &[],
    &[Rz],
    &[Tx],
    &[Ty],
    &[Tz],
    &[Tz, Rz],
    &[Tx, Rz],
    &[Tx, Ty, Rz],
    &[Rz, Ry, Rx],
];

#[test]
fn every_kind_holds_with_its_free_motions() {
    // Frame b on the fixed world, turned; frame a on a body that starts
    // elsewhere and turned another way.
    let fb = frame([10.0, 20.0, 30.0], [0.0, 0.6, 0.8], [1.0, 0.0, 0.0]);
    let start = skew_turn(2.0).after(&Pose::translation([40.0, -15.0, 5.0]));
    let fa = start.after(&frame([1.0, 2.0, 3.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]));
    let sample = |m: Freedom| match m {
        Tx => 3.0,
        Ty => -2.5,
        Tz => 7.0,
        Rx => 0.3,
        Ry => -0.4,
        Rz => 1.1,
    };
    for motions in KINDS {
        let mut s = RigidSystem::new();
        let body = s.add_body(false);
        let j = s
            .add_joint(joint(Some(body), fa, None, fb, motions))
            .unwrap();
        let r = s.solve(&opts());
        assert!(r.is_ok(), "{motions:?}: {r:?}");
        assert_holds(&s, &[j]);
        // Its degrees of freedom are its free motions.
        let a = s.analyze();
        assert_eq!(a.dof, motions.len(), "{motions:?}");
        assert_eq!(a.body_dof, vec![motions.len()]);
        assert!(a.redundant.is_empty() && a.conflicts.is_empty());

        // Driven to values, frame a is where they put it.
        let values: Vec<f64> = motions.iter().map(|m| sample(*m)).collect();
        for (m, v) in motions.iter().zip(&values) {
            s.set_motion(
                j,
                JointMotion {
                    target: Some(*v),
                    ..JointMotion::free(*m)
                },
            )
            .unwrap();
        }
        let r = s.solve(&opts());
        assert!(r.is_ok(), "{motions:?}: {r:?}");
        let wanted = fb.after(&motion_pose(motions, &values));
        let now = s.body_pose(body).after(&fa);
        let (d, angle) = now.distance(&wanted);
        assert!(
            d < 1e-8 && angle < 1e-9,
            "{motions:?}: {now:?} != {wanted:?}"
        );
        for (v, w) in s.joint_values(j).iter().zip(&values) {
            assert!((v - w).abs() < 1e-9, "{motions:?}: {:?}", s.joint_values(j));
        }
        // The targets do not count as joint equations.
        assert_eq!(s.analyze().dof, motions.len());
    }
}

#[test]
fn a_joint_keeps_its_free_motions_where_they_are() {
    // A body on a slider along z, 25 above and turned: placed onto the
    // axis without changing its height or turning more than it must.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let fa = at([4.0, -3.0, 25.0]);
    let j = s
        .add_joint(joint(Some(body), fa, None, at([0.0; 3]), &[Tz]))
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert_near(moved(&s, body, fa.translation), [0.0, 0.0, 25.0]);
    assert!((s.joint_values(j)[0] - 25.0).abs() < 1e-9);
    // A revolute joint keeps the turn about its axis.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let turned = Pose::rotation([0.0; 3], [0.0, 0.0, 1.0], 0.7).after(&Pose::rotation(
        [0.0; 3],
        [1.0, 0.0, 0.0],
        0.2,
    ));
    let fa = Pose::translation([5.0, 5.0, 1.0]).after(&turned);
    let j = s
        .add_joint(joint(Some(body), fa, None, at([0.0; 3]), &[Rz]))
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert!(
        (s.joint_values(j)[0] - 0.7).abs() < 0.05,
        "{:?}",
        s.joint_values(j)
    );
    assert_holds(&s, &[j]);
}

#[test]
fn limits_hold_free_motions_and_targets_drive_them() {
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let fa = at([0.0, 0.0, 25.0]);
    let j = s
        .add_joint(RigidJoint {
            motions: vec![JointMotion {
                min: Some(0.0),
                max: Some(10.0),
                ..JointMotion::free(Tz)
            }],
            ..joint(Some(body), fa, None, at([0.0; 3]), &[])
        })
        .unwrap();
    let r = s.solve(&opts());
    assert!(r.is_ok(), "{r:?}");
    // Kept within the limits, at the one it is held at.
    assert_near(moved(&s, body, fa.translation), [0.0, 0.0, 10.0]);
    assert_eq!(r.at_limits, vec![(j, Tz)]);
    // Dragged beyond the limit, it stays there.
    let r = s.drag(body, [0.0, 0.0, 10.0], [0.0, 0.0, 50.0], &opts());
    assert!(r.is_ok(), "{r:?}");
    assert!((s.joint_values(j)[0] - 10.0).abs() < 1e-9);
    // Dragged inside, it follows.
    let r = s.drag(body, [0.0, 0.0, 10.0], [3.0, 1.0, 4.0], &opts());
    assert!(r.is_ok(), "{r:?}");
    assert!(
        (s.joint_values(j)[0] - 4.0).abs() < 1e-6,
        "{:?}",
        s.joint_values(j)
    );
    // Driven to a value, it goes there.
    s.set_motion(
        j,
        JointMotion {
            min: Some(0.0),
            max: Some(10.0),
            target: Some(7.5),
            freedom: Tz,
        },
    )
    .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert_near(moved(&s, body, fa.translation), [0.0, 0.0, 7.5]);
    // A turn with limits takes whole turns into account.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let fa = Pose::rotation([0.0; 3], [0.0, 0.0, 1.0], -FRAC_PI_2);
    let j = s
        .add_joint(RigidJoint {
            motions: vec![JointMotion {
                min: Some(PI),
                max: Some(2.0 * PI),
                ..JointMotion::free(Rz)
            }],
            ..joint(Some(body), fa, None, Pose::IDENTITY, &[])
        })
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert!(
        (s.joint_values(j)[0] - 1.5 * PI).abs() < 1e-9,
        "{:?}",
        s.joint_values(j)
    );
    // Wrong limits and repeated motions are refused.
    assert!(
        s.set_motion(
            j,
            JointMotion {
                min: Some(1.0),
                max: Some(0.0),
                ..JointMotion::free(Rz)
            }
        )
        .is_err()
    );
    assert!(s.set_motion(j, JointMotion::free(Tx)).is_err());
    assert!(
        s.add_joint(joint(Some(body), fa, None, Pose::IDENTITY, &[Rz, Rz]))
            .is_err()
    );
    assert!(s.add_joint(joint(None, fa, None, fa, &[])).is_err());
}

#[test]
fn contradicting_joints_are_reported_and_move_nothing() {
    // Two revolute joints to the world on parallel axes 10 apart.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let j1 = s
        .add_joint(joint(Some(body), at([0.0; 3]), None, at([0.0; 3]), &[Rz]))
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    let j2 = s
        .add_joint(joint(
            Some(body),
            at([20.0, 0.0, 0.0]),
            None,
            at([10.0, 0.0, 0.0]),
            &[Rz],
        ))
        .unwrap();
    let before = s.body_pose(body);
    let r = s.solve(&opts());
    assert_eq!(r.status, SolveStatus::Conflicting, "{r:?}");
    // The newest joint over-constrains, with the first.
    assert_eq!(r.conflicts[0].joint, j2, "{r:?}");
    assert_eq!(r.conflicts[0].target, None);
    assert_eq!(r.conflicts[0].involved, vec![j1, j2]);
    assert_eq!(s.body_pose(body), before);
    let a = s.analyze();
    assert!(!a.conflicts.is_empty());

    // On the same axis, the second joint is redundant: it holds, and the
    // body still turns.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let j1 = s
        .add_joint(joint(Some(body), at([0.0; 3]), None, at([0.0; 3]), &[Rz]))
        .unwrap();
    let j2 = s
        .add_joint(joint(
            Some(body),
            at([0.0, 0.0, 12.0]),
            None,
            at([0.0, 0.0, 12.0]),
            &[Rz],
        ))
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    let a = s.analyze();
    assert_eq!(a.dof, 1);
    assert_eq!(a.redundant.len(), 1, "{a:?}");
    assert_eq!(a.redundant[0].joint, j2);
    assert_eq!(a.redundant[0].involved, vec![j1, j2]);

    // A target the joints cannot reach.
    let mut s = RigidSystem::new();
    let body = s.add_body(false);
    let j1 = s
        .add_joint(RigidJoint {
            motions: vec![JointMotion {
                target: Some(0.5),
                ..JointMotion::free(Rz)
            }],
            ..joint(Some(body), at([0.0; 3]), None, at([0.0; 3]), &[])
        })
        .unwrap();
    s.add_joint(joint(
        Some(body),
        at([5.0, 0.0, 0.0]),
        None,
        at([5.0, 0.0, 0.0]),
        &[],
    ))
    .unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.status, SolveStatus::Conflicting, "{r:?}");
    assert!(
        r.conflicts.iter().any(|c| c.involved.contains(&j1)),
        "{r:?}"
    );
}

/// The four-bar linkage: ground pivots 100 apart, crank 30, coupler 100,
/// rocker 60; each link starts somewhere else.
struct FourBar {
    s: RigidSystem,
    links: [BodyId; 3],
    joints: [JointId; 4],
    /// Each link's pins where it starts.
    pins: [[V; 2]; 3],
}

fn four_bar() -> FourBar {
    let mut s = RigidSystem::new();
    let pins = [
        [[0.0, 200.0, 50.0], [30.0, 200.0, 50.0]],
        [[0.0, -300.0, 0.0], [100.0, -300.0, 0.0]],
        [[500.0, 0.0, 0.0], [500.0, 60.0, 0.0]],
    ];
    let links = [s.add_body(false), s.add_body(false), s.add_body(false)];
    let pin = |l: usize, k: usize| at(pins[l][k]);
    let revolute =
        |a: Option<BodyId>, fa: Pose, b: Option<BodyId>, fb: Pose| joint(a, fa, b, fb, &[Rz]);
    let joints = [
        s.add_joint(revolute(Some(links[0]), pin(0, 0), None, at([0.0; 3])))
            .unwrap(),
        s.add_joint(revolute(
            Some(links[1]),
            pin(1, 0),
            Some(links[0]),
            pin(0, 1),
        ))
        .unwrap(),
        s.add_joint(revolute(
            Some(links[2]),
            pin(2, 0),
            Some(links[1]),
            pin(1, 1),
        ))
        .unwrap(),
        s.add_joint(revolute(
            Some(links[2]),
            pin(2, 1),
            None,
            at([100.0, 0.0, 0.0]),
        ))
        .unwrap(),
    ];
    FourBar {
        s,
        links,
        joints,
        pins,
    }
}

impl FourBar {
    fn pin(&self, l: usize, k: usize) -> V {
        moved(&self.s, self.links[l], self.pins[l][k])
    }

    fn check(&self) {
        assert_holds(&self.s, &self.joints);
        assert_near(self.pin(0, 0), [0.0; 3]);
        assert_near(self.pin(2, 1), [100.0, 0.0, 0.0]);
        assert_near(self.pin(0, 1), self.pin(1, 0));
        assert_near(self.pin(1, 1), self.pin(2, 0));
        for (l, length) in [30.0, 100.0, 60.0].into_iter().enumerate() {
            assert!((dist(self.pin(l, 0), self.pin(l, 1)) - length).abs() < 1e-8);
        }
        // It stays in its plane.
        for l in 0..3 {
            for k in 0..2 {
                assert!(self.pin(l, k)[2].abs() < 1e-8);
            }
        }
    }
}

#[test]
fn a_four_bar_linkage_closes_and_follows_its_crank() {
    let mut fb = four_bar();
    let r = fb.s.solve(&opts());
    assert!(r.is_ok(), "{r:?}");
    fb.check();
    // One degree of freedom from the rank; a count of the joints'
    // constraints says -2: three of the closing joint's equations repeat.
    let a = fb.s.analyze();
    assert_eq!(a.dof, 1, "{a:?}");
    assert_eq!(a.redundant.len(), 1);
    assert_eq!(a.redundant[0].joint, fb.joints[3]);
    assert!(a.conflicts.is_empty());
    // Each link alone is held by its neighbours.
    assert_eq!(a.body_dof, vec![0, 0, 0]);

    // Driven by the crank's angle.
    for angle in [FRAC_PI_3, 2.0, -1.0, 3.0] {
        fb.s.set_motion(
            fb.joints[0],
            JointMotion {
                target: Some(angle),
                ..JointMotion::free(Rz)
            },
        )
        .unwrap();
        let r = fb.s.solve(&opts());
        assert!(r.is_ok(), "{angle}: {r:?}");
        fb.check();
        assert_near(fb.pin(0, 1), [30.0 * angle.cos(), 30.0 * angle.sin(), 0.0]);
    }
    // Dragging the rocker turns the crank.
    fb.s.set_motion(fb.joints[0], JointMotion::free(Rz))
        .unwrap();
    let before = fb.s.joint_values(fb.joints[0])[0];
    let grab = fb.pin(2, 0);
    let target = [grab[0] + 5.0, grab[1] - 5.0, grab[2]];
    let r = fb.s.drag(fb.links[2], fb.pins[2][0], target, &opts());
    assert!(r.is_ok(), "{r:?}");
    fb.check();
    assert!((fb.s.joint_values(fb.joints[0])[0] - before).abs() > 1e-3);
}

#[test]
fn a_slider_crank_moves_its_piston() {
    // Crank 20 about the origin, rod 80, piston sliding along x.
    let mut s = RigidSystem::new();
    let crank = s.add_body(false);
    let rod = s.add_body(false);
    let piston = s.add_body(false);
    let crank_pins = [[50.0, 50.0, 0.0], [70.0, 50.0, 0.0]];
    let rod_pins = [[-40.0, 0.0, 9.0], [40.0, 0.0, 9.0]];
    let piston_pin = [0.0, -90.0, 3.0];
    let j1 = s
        .add_joint(joint(
            Some(crank),
            at(crank_pins[0]),
            None,
            at([0.0; 3]),
            &[Rz],
        ))
        .unwrap();
    let j2 = s
        .add_joint(joint(
            Some(rod),
            at(rod_pins[0]),
            Some(crank),
            at(crank_pins[1]),
            &[Rz],
        ))
        .unwrap();
    let j3 = s
        .add_joint(joint(
            Some(piston),
            at(piston_pin),
            Some(rod),
            at(rod_pins[1]),
            &[Rz],
        ))
        .unwrap();
    let j4 = s
        .add_joint(joint(
            Some(piston),
            at(piston_pin),
            None,
            at([0.0; 3]),
            &[Tx],
        ))
        .unwrap();
    let r = s.solve(&opts());
    assert!(r.is_ok(), "{r:?}");
    assert_holds(&s, &[j1, j2, j3, j4]);
    assert_eq!(s.analyze().dof, 1);
    for angle in [0.0, 0.5, FRAC_PI_2, 2.5, PI, -2.0] {
        s.set_motion(
            j1,
            JointMotion {
                target: Some(angle),
                ..JointMotion::free(Rz)
            },
        )
        .unwrap();
        let r = s.solve(&opts());
        assert!(r.is_ok(), "{angle}: {r:?}");
        assert_holds(&s, &[j1, j2, j3, j4]);
        let x = 20.0 * angle.cos() + (80.0_f64.powi(2) - (20.0 * angle.sin()).powi(2)).sqrt();
        assert_near(moved(&s, piston, piston_pin), [x, 0.0, 0.0]);
        assert!((s.joint_values(j4)[0] - x).abs() < 1e-8);
    }
}

#[test]
fn fixed_bodies_stay_and_free_sides_move_onto_them() {
    // Two free bodies: a moves onto b.
    let mut s = RigidSystem::new();
    let a = s.add_body(false);
    let b = s.add_body(false);
    let j = s
        .add_joint(joint(
            Some(a),
            at([100.0, 0.0, 0.0]),
            Some(b),
            frame([0.0, 0.0, 5.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]),
            &[],
        ))
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert_eq!(s.body_pose(b), Pose::IDENTITY);
    assert_holds(&s, &[j]);
    // Joined to a fixed body, both move together, as one.
    let ground = s.add_body(true);
    let j2 = s
        .add_joint(joint(
            Some(b),
            at([0.0; 3]),
            Some(ground),
            at([0.0, 50.0, 0.0]),
            &[],
        ))
        .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert_eq!(s.body_pose(ground), Pose::IDENTITY);
    assert_holds(&s, &[j, j2]);
    assert_near(s.body_pose(b).translation, [0.0, 50.0, 0.0]);
    assert_near(moved(&s, a, [100.0, 0.0, 0.0]), [0.0, 50.0, 5.0]);
    let analysis = s.analyze();
    assert_eq!(analysis.dof, 0);
    assert_eq!(analysis.body_dof, vec![0, 0, 0]);
    // A body without joints keeps its six.
    let lone = s.add_body(false);
    assert_eq!(s.analyze().dof, 6);
    assert_eq!(s.analyze().body_dof[lone.0 as usize], 6);
    // Two fixed bodies whose joint does not hold: a conflict.
    let other = s.add_body(true);
    s.add_joint(joint(
        Some(other),
        at([1.0; 3]),
        Some(ground),
        at([0.0; 3]),
        &[],
    ))
    .unwrap();
    let r = s.solve(&opts());
    assert_ne!(r.status, SolveStatus::Converged);
}

#[test]
fn drags_move_the_bodies_as_little_as_possible() {
    // A bar on a revolute joint at the origin, its end dragged a quarter
    // turn round.
    let mut s = RigidSystem::new();
    let bar = s.add_body(false);
    let j = s
        .add_joint(joint(Some(bar), at([0.0; 3]), None, at([0.0; 3]), &[Rz]))
        .unwrap();
    let r = s.drag(bar, [50.0, 0.0, 0.0], [0.0, 50.0, 0.0], &opts());
    assert!(r.is_ok(), "{r:?}");
    assert_near(moved(&s, bar, [50.0, 0.0, 0.0]), [0.0, 50.0, 0.0]);
    assert!((s.joint_values(j)[0] - FRAC_PI_2).abs() < 1e-6);
    // Toward a point it cannot reach, as near as it can.
    let r = s.drag(bar, [0.0, 50.0, 0.0], [0.0, 80.0, 30.0], &opts());
    assert!(r.is_ok(), "{r:?}");
    assert_near(moved(&s, bar, [50.0, 0.0, 0.0]), [0.0, 50.0, 0.0]);
    // A free body follows exactly.
    let lone = s.add_body(false);
    let r = s.drag(lone, [1.0, 2.0, 3.0], [4.0, 6.0, 3.0], &opts());
    assert!(r.is_ok(), "{r:?}");
    assert_near(s.body_pose(lone).translation, [3.0, 4.0, 0.0]);
}

#[test]
fn bad_starting_placements_converge() {
    // Without placing first: the iteration alone from bodies turned up to
    // nearly and exactly half a turn about skew axes.
    for kind in [&[][..], &[Rz][..], &[Tz, Rz][..]] {
        for angle in [0.5, 1.5, 2.5, 3.0, PI - 1e-3, PI] {
            for axis in [[1.0, 0.0, 0.0], [0.3, -0.5, 0.8], [0.0, 0.0, 1.0]] {
                let mut s = RigidSystem::new();
                let body = s.add_body(false);
                let start = Pose::rotation([20.0, 0.0, 0.0], axis, angle)
                    .after(&Pose::translation([30.0, -10.0, 4.0]));
                let fa = start.after(&at([5.0, 5.0, 0.0]));
                let j = s
                    .add_joint(joint(Some(body), fa, None, at([0.0; 3]), kind))
                    .unwrap();
                let r = s.solve(&RigidOptions {
                    place: false,
                    ..opts()
                });
                assert!(r.is_ok(), "{kind:?} {angle} {axis:?}: {r:?}");
                assert_holds(&s, &[j]);
            }
        }
    }
    // The four-bar without placing first.
    let mut fb = four_bar();
    let r = fb.s.solve(&RigidOptions {
        place: false,
        ..opts()
    });
    assert!(r.is_ok(), "{r:?}");
    fb.check();
}

#[test]
fn a_chain_places_each_link_in_order() {
    // Three links in a row from a fixed base, each turned at rest values:
    // placing needs no iteration and every joint holds exactly.
    let mut s = RigidSystem::new();
    let base = s.add_body(true);
    let links: Vec<BodyId> = (0..3).map(|_| s.add_body(false)).collect();
    let mut previous = base;
    let mut joints = Vec::new();
    for (i, link) in links.iter().enumerate() {
        let start = 100.0 * (i + 1) as f64;
        let j = s
            .add_joint(RigidJoint {
                motions: vec![JointMotion {
                    target: Some(0.25),
                    ..JointMotion::free(Rz)
                }],
                ..joint(
                    Some(*link),
                    at([start, start, 0.0]),
                    Some(previous),
                    at([10.0 * i as f64, 0.0, 0.0]),
                    &[],
                )
            })
            .unwrap();
        joints.push(j);
        previous = *link;
    }
    let r = s.solve(&opts());
    assert!(r.is_ok(), "{r:?}");
    assert!(r.iterations <= 1, "{r:?}");
    assert_holds(&s, &joints);
}
