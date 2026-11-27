// SPDX-License-Identifier: MIT
//! Couplings of joints' motions (mitcad#466): a gear pair, a rack and
//! pinion, a screw on one cylindrical joint and a table (a cam), driven
//! from either side, dragged, analysed, and over-constrained.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use mitcad_solver::Freedom::{self, *};
use mitcad_solver::{
    BodyId, JointId, JointMotion, Pose, Relation, RigidCoupling, RigidJoint, RigidOptions,
    RigidSystem, SolveStatus,
};

fn opts() -> RigidOptions {
    RigidOptions::default()
}

/// A joint of one free body to the fixed world at `origin`, with these
/// motions (driven to `target` when given).
fn to_world(
    s: &mut RigidSystem,
    body: BodyId,
    origin: [f64; 3],
    motions: &[(Freedom, Option<f64>)],
) -> JointId {
    s.add_joint(RigidJoint {
        a: Some(body),
        frame_a: Pose::translation(origin),
        b: None,
        frame_b: Pose::translation(origin),
        motions: motions
            .iter()
            .map(|(f, t)| JointMotion {
                target: *t,
                ..JointMotion::free(*f)
            })
            .collect(),
        alignment: Pose::IDENTITY,
    })
    .unwrap()
}

fn linear(ratio: f64, offset: f64) -> Relation {
    Relation::Linear { ratio, offset }
}

fn value(s: &RigidSystem, joint: JointId) -> f64 {
    s.joint_values(joint)[0]
}

/// Two gears on revolute joints: the second turns half as far the other
/// way; driving either one turns the other.
#[test]
fn gears_turn_together_from_either_side() {
    let mut s = RigidSystem::new();
    let (g1, g2) = (s.add_body(false), s.add_body(false));
    let j1 = to_world(&mut s, g1, [0.0; 3], &[(Rz, Some(1.0))]);
    let j2 = to_world(&mut s, g2, [30.0, 0.0, 0.0], &[(Rz, None)]);
    let link = s
        .add_coupling(RigidCoupling {
            driver: (j1, Rz),
            follower: (j2, Rz),
            relation: linear(-0.5, 0.0),
        })
        .unwrap();
    let r = s.solve(&opts());
    assert!(r.is_ok(), "{r:?}");
    assert!((value(&s, j1) - 1.0).abs() < 1e-9);
    assert!((value(&s, j2) + 0.5).abs() < 1e-9, "{}", value(&s, j2));
    // The body turned with its value.
    let p = s.body_pose(g2).apply_point([40.0, 0.0, 0.0]);
    assert!((p[0] - (30.0 + 10.0 * 0.5f64.cos())).abs() < 1e-8, "{p:?}");
    assert!((p[1] + 10.0 * 0.5f64.sin()).abs() < 1e-8, "{p:?}");
    let (d, a) = s.joint_error(link);
    assert!(d < 1e-9 && a < 1e-9);

    // The follower driven while the driver is free drives the driver.
    s.set_motion(j1, JointMotion::free(Rz)).unwrap();
    s.set_motion(
        j2,
        JointMotion {
            target: Some(-1.0),
            ..JointMotion::free(Rz)
        },
    )
    .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert!((value(&s, j1) - 2.0).abs() < 1e-9, "{}", value(&s, j1));

    // Both driven: the driver wins, the follower's target is let go.
    s.set_motion(
        j1,
        JointMotion {
            target: Some(0.2),
            ..JointMotion::free(Rz)
        },
    )
    .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert!((value(&s, j2) + 0.1).abs() < 1e-9, "{}", value(&s, j2));
}

/// A free gear turned past half a turn keeps its turn: its mate tells
/// which turn it is at, so nothing jumps.
#[test]
fn a_free_gear_past_half_a_turn_stays_where_it_is() {
    let mut s = RigidSystem::new();
    let (g1, g2) = (s.add_body(false), s.add_body(false));
    let turned = |origin: [f64; 3], angle: f64| {
        let mut p = Pose::rotation(origin, [0.0, 0.0, 1.0], angle);
        p.translation = origin;
        p
    };
    let j1 = s
        .add_joint(RigidJoint {
            a: Some(g1),
            frame_a: turned([0.0; 3], 4.0),
            b: None,
            frame_b: Pose::IDENTITY,
            motions: vec![JointMotion::free(Rz)],
            alignment: Pose::IDENTITY,
        })
        .unwrap();
    let j2 = s
        .add_joint(RigidJoint {
            a: Some(g2),
            frame_a: turned([30.0, 0.0, 0.0], -2.0),
            b: None,
            frame_b: Pose::translation([30.0, 0.0, 0.0]),
            motions: vec![JointMotion::free(Rz)],
            alignment: Pose::IDENTITY,
        })
        .unwrap();
    s.add_coupling(RigidCoupling {
        driver: (j1, Rz),
        follower: (j2, Rz),
        relation: linear(-0.5, 0.0),
    })
    .unwrap();
    assert!(s.solve(&opts()).is_ok());
    for body in [g1, g2] {
        let (d, a) = s.body_pose(body).distance(&Pose::IDENTITY);
        assert!(d < 1e-9 && a < 1e-9, "{body:?} moved {d} {a}");
    }
    assert!((value(&s, j1) - 4.0).abs() < 1e-9, "{}", value(&s, j1));
}

/// A pinion of radius 8 turning drives a rack sliding along x; its free
/// motion counts once.
#[test]
fn a_rack_follows_its_pinion_and_the_pair_has_one_freedom() {
    let mut s = RigidSystem::new();
    let (pinion, rack) = (s.add_body(false), s.add_body(false));
    let jp = to_world(&mut s, pinion, [0.0; 3], &[(Rz, None)]);
    let jr = to_world(&mut s, rack, [0.0, -8.0, 0.0], &[(Tx, None)]);
    s.add_coupling(RigidCoupling {
        driver: (jp, Rz),
        follower: (jr, Tx),
        relation: linear(8.0, 0.0),
    })
    .unwrap();
    let a = s.analyze();
    assert_eq!(a.dof, 1, "{a:?}");
    assert!(a.conflicts.is_empty() && a.redundant.is_empty());
    s.set_motion(
        jp,
        JointMotion {
            target: Some(FRAC_PI_2),
            ..JointMotion::free(Rz)
        },
    )
    .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert!((value(&s, jr) - 4.0 * PI).abs() < 1e-9);
    let p = s.body_pose(rack).apply_point([0.0, -8.0, 0.0]);
    assert!((p[0] - 4.0 * PI).abs() < 1e-8, "{p:?}");

    // Dragging the rack turns the pinion.
    let r = s.drag(rack, [0.0, -8.0, 0.0], [20.0, -8.0, 0.0], &opts());
    assert!(r.is_ok(), "{r:?}");
    let travel = value(&s, jr);
    assert!((value(&s, jp) * 8.0 - travel).abs() < 1e-7);
}

/// A screw: the slide of a cylindrical joint advances its lead per turn.
#[test]
fn a_screw_couples_a_joints_own_turn_and_slide() {
    let mut s = RigidSystem::new();
    let nut = s.add_body(false);
    let j = to_world(&mut s, nut, [0.0; 3], &[(Tz, None), (Rz, Some(3.0 * TAU))]);
    s.add_coupling(RigidCoupling {
        driver: (j, Rz),
        follower: (j, Tz),
        relation: linear(1.5 / TAU, 0.0),
    })
    .unwrap();
    assert!(s.solve(&opts()).is_ok());
    assert!((s.joint_values(j)[0] - 4.5).abs() < 1e-9);
    // The turn and the slide move as one (targets do not count).
    assert_eq!(s.analyze().dof, 1);
}

/// A table: the follower rides a periodic lift (an eccentric's 10 + 5 cos)
/// sampled at every degree; between samples it stays within them.
#[test]
fn a_table_drives_a_follower_and_lets_its_target_go() {
    let n = 360;
    let lift: Vec<f64> = (0..n)
        .map(|k| 10.0 + 5.0 * (TAU * k as f64 / n as f64).cos())
        .collect();
    let mut s = RigidSystem::new();
    let (cam, follower) = (s.add_body(false), s.add_body(false));
    let jc = to_world(&mut s, cam, [0.0; 3], &[(Rz, Some(1.0))]);
    // The follower's own target is let go: the table drives it.
    let jf = to_world(&mut s, follower, [0.0, 50.0, 0.0], &[(Tx, Some(99.0))]);
    s.add_coupling(RigidCoupling {
        driver: (jc, Rz),
        follower: (jf, Tx),
        relation: Relation::Table {
            values: lift.clone(),
            offset: 2.0,
        },
    })
    .unwrap();
    let r = s.solve(&opts());
    assert!(r.is_ok(), "{r:?}");
    let expected = 12.0 + 5.0 * 1.0f64.cos();
    assert!((value(&s, jf) - expected).abs() < 1e-4, "{}", value(&s, jf));
    // Exact at the samples, periodic past a turn.
    let rel = Relation::Table {
        values: lift,
        offset: 0.0,
    };
    assert!((rel.follow(0.0) - 15.0).abs() < 1e-12);
    assert!((rel.follow(TAU + PI) - 5.0).abs() < 1e-9);
    assert!((rel.follow(-FRAC_PI_2) - 10.0).abs() < 1e-9);
}

/// A coupling that contradicts two driven motions is named in the
/// conflict; a bad coupling is refused.
#[test]
fn contradicting_couplings_are_reported_and_bad_ones_refused() {
    let mut s = RigidSystem::new();
    let (g1, g2, g3) = (s.add_body(false), s.add_body(false), s.add_body(false));
    let j1 = to_world(&mut s, g1, [0.0; 3], &[(Rz, None)]);
    let j2 = to_world(&mut s, g2, [30.0, 0.0, 0.0], &[(Rz, Some(1.0))]);
    let j3 = to_world(&mut s, g3, [60.0, 0.0, 0.0], &[(Rz, Some(1.0))]);
    s.add_coupling(RigidCoupling {
        driver: (j1, Rz),
        follower: (j2, Rz),
        relation: linear(-1.0, 0.0),
    })
    .unwrap();
    let last = s
        .add_coupling(RigidCoupling {
            driver: (j1, Rz),
            follower: (j3, Rz),
            relation: linear(1.0, 0.0),
        })
        .unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.status, SolveStatus::Conflicting, "{r:?}");
    assert!(
        r.conflicts.iter().any(|c| c.involved.contains(&last)),
        "{r:?}"
    );

    assert!(
        s.add_coupling(RigidCoupling {
            driver: (j1, Rz),
            follower: (j1, Rz),
            relation: linear(1.0, 0.0),
        })
        .is_err()
    );
    assert!(
        s.add_coupling(RigidCoupling {
            driver: (j1, Tx),
            follower: (j2, Rz),
            relation: linear(1.0, 0.0),
        })
        .is_err()
    );
    assert!(
        s.add_coupling(RigidCoupling {
            driver: (j1, Rz),
            follower: (last, Rz),
            relation: linear(1.0, 0.0),
        })
        .is_err()
    );
    assert!(
        s.add_coupling(RigidCoupling {
            driver: (j1, Rz),
            follower: (j2, Rz),
            relation: linear(f64::NAN, 0.0),
        })
        .is_err()
    );
}
