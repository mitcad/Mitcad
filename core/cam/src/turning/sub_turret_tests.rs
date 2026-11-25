// SPDX-License-Identifier: MIT
//! Tests of the sub-spindle drawn in Simulate, the turret's body against
//! the sub-spindle, and the sub-spindle's own revolved profile
//! (mitcad#2405): the machine data kept and refused in words; where the
//! view draws the sub-spindle as each operation cuts and its outline; the
//! turret's body running into the sub-spindle's chuck and into a zone
//! moving with it; a nose of the profile in front of its jaws' face.

use super::sub_live_tests::{back_hole, sub_lathe, sub_program};
use super::*;
use crate::machine::{Bounds, Finding, Machine, Zone};

fn with_lathe(machine: &Machine, change: impl FnOnce(&mut Lathe)) -> Machine {
    let mut m = machine.clone();
    if let Some(lathe) = &mut m.lathe {
        change(lathe);
    }
    m
}

fn turret(x: [f64; 2], z: [f64; 2]) -> Option<TurretBody> {
    Some(TurretBody { x, z })
}

fn words(hits: &[Finding]) -> Vec<String> {
    hits.iter().map(|f| f.message.clone()).collect()
}

/// A nose 14 mm across reaching 12 mm in front of the jaws' face, then a
/// body 40 mm across 50 mm deep behind it.
fn nose() -> Vec<[f64; 2]> {
    vec![[-12.0, 14.0], [0.0, 14.0], [0.0, 40.0], [50.0, 40.0]]
}

#[test]
fn the_turret_s_body_and_the_profile_are_kept_and_refused_in_words() {
    let lathe = Lathe {
        turret_body: turret([0.0, 80.0], [-10.0, 20.0]),
        sub_spindle: SubSpindle {
            profile: nose(),
            ..SubSpindle::default()
        },
        ..Lathe::default()
    };
    lathe.validate().unwrap();
    // Left out at their defaults, read back when set.
    let json = serde_json::to_value(&lathe).unwrap();
    assert_eq!(
        json["turret_body"],
        serde_json::json!({"x": [0.0, 80.0], "z": [-10.0, 20.0]})
    );
    assert_eq!(
        json["sub_spindle"]["profile"][2],
        serde_json::json!([0.0, 40.0])
    );
    let back: Lathe = serde_json::from_value(json).unwrap();
    assert_eq!(back, lathe);
    let json = serde_json::to_value(Lathe::default()).unwrap();
    assert!(json.get("turret_body").is_none(), "{json}");
    assert!(json.get("sub_spindle").is_none(), "{json}");
    // Refused in words.
    let bad = Lathe {
        turret_body: turret([80.0, 0.0], [-10.0, 20.0]),
        ..Lathe::default()
    };
    assert_eq!(
        bad.validate().unwrap_err(),
        "The turret's body needs X and Z from the tool, each from a lowest up to a highest."
    );
    let message = "The sub-spindle's profile needs two or more points in order along Z, each with a diameter of zero or more.";
    for profile in [
        vec![[0.0, 40.0]],
        vec![[10.0, 40.0], [0.0, 40.0]],
        vec![[0.0, -1.0], [10.0, 40.0]],
        vec![[0.0, f64::NAN], [10.0, 40.0]],
    ] {
        let bad = SubSpindle {
            profile,
            ..SubSpindle::default()
        };
        assert_eq!(bad.validate().unwrap_err(), message);
    }
}

/// The profile's body: the nose in front of the jaws' face, the step at
/// the face, the body behind it; a point outside each is clear.
#[test]
fn the_profile_is_a_part_of_the_body() {
    let sub = SubSpindle {
        profile: nose(),
        ..SubSpindle::default()
    };
    assert!(sub.has_body());
    assert_eq!(sub.profile_radius(-12.0), Some(7.0));
    assert_eq!(sub.profile_radius(0.0), Some(20.0));
    assert_eq!(sub.profile_radius(25.0), Some(20.0));
    assert_eq!(sub.profile_radius(-13.0), None);
    assert_eq!(sub.profile_radius(51.0), None);
    assert_eq!(sub.body_at(4.0, -5.0), Some(SubBody::Profile));
    assert_eq!(sub.body_at(8.0, -5.0), None);
    assert_eq!(sub.body_at(19.0, 10.0), Some(SubBody::Profile));
    assert_eq!(sub.body_at(21.0, 10.0), None);
    assert_eq!(sub.body_at(4.0, -13.0), None);
    // A cone: halfway its radius is halfway.
    let cone = SubSpindle {
        profile: vec![[0.0, 20.0], [10.0, 40.0]],
        ..SubSpindle::default()
    };
    assert_eq!(cone.profile_radius(5.0), Some(15.0));
    // A box reaching into it: the largest radius over its length counts.
    assert_eq!(cone.body_across(14.0, [-5.0, 5.0]), vec![SubBody::Profile]);
    assert!(cone.body_across(16.0, [-5.0, 5.0]).is_empty());
    assert_eq!(cone.body_across(19.0, [8.0, 30.0]), vec![SubBody::Profile]);
    assert!(cone.body_across(1.0, [11.0, 30.0]).is_empty());
    // The chuck and the housing as a point finds them, the jaws' face free.
    let body = SubSpindle {
        chuck_diameter: 20.0,
        chuck_length: 10.0,
        housing_diameter: 200.0,
        housing_length: 150.0,
        ..SubSpindle::default()
    };
    assert_eq!(body.body_across(5.0, [-20.0, 0.0]), Vec::<SubBody>::new());
    assert_eq!(body.body_across(5.0, [-20.0, 1.0]), vec![SubBody::Chuck]);
    assert_eq!(
        body.body_across(5.0, [-20.0, 11.0]),
        vec![SubBody::Chuck, SubBody::Housing]
    );
    assert_eq!(body.body_across(50.0, [5.0, 11.0]), vec![SubBody::Housing]);
    assert!(body.body_across(50.0, [161.0, 170.0]).is_empty());
}

/// The sub-spindle's outline as the view draws it and where it stands as
/// each operation cuts: at the grip for the parting off and on the part
/// where it was for its own operations (the view keeps the part where it
/// was before the hand-over pulled it back), where it waits for the main
/// spindle's.
#[test]
fn the_view_draws_the_sub_spindle_where_it_is() {
    let sub = SubSpindle {
        chuck_diameter: 100.0,
        housing_diameter: 180.0,
        housing_length: 0.0,
        profile: nose(),
        ..SubSpindle::default()
    };
    // Parts without end as long as their diameter.
    assert_eq!(
        sub.outline(),
        vec![
            vec![[0.0, 50.0], [100.0, 50.0]],
            vec![[0.0, 90.0], [180.0, 90.0]],
            vec![[-12.0, 7.0], [0.0, 7.0], [0.0, 20.0], [50.0, 20.0]],
        ]
    );
    let small = SubSpindle {
        chuck_diameter: 10.0,
        chuck_length: 5.0,
        housing_diameter: 12.0,
        ..SubSpindle::default()
    };
    assert_eq!(
        small.outline(),
        vec![vec![[0.0, 5.0], [5.0, 5.0]], vec![[5.0, 6.0], [25.0, 6.0]]]
    );
    assert!(SubSpindle::default().outline().is_empty());
    let lathe = sub_lathe().lathe.unwrap();
    let mut program = sub_program();
    // A facing on the main spindle first.
    let mut facing = program.operations[1].clone();
    facing.name = "Face".into();
    facing.sub = false;
    program.operations.insert(0, facing);
    // The facing: where it waits, at the far end of its travel (machine
    // Z 300, 500 from the work zero at -200); the parting off at the grip;
    // the sub-spindle's operations at the grip too, where the view keeps
    // the part, though it holds it 30 mm farther.
    assert_eq!(
        sub_face_shown(&lathe, &program, 0, Some(-200.0)),
        Some(500.0)
    );
    assert_eq!(sub_face_shown(&lathe, &program, 0, None), None);
    assert_eq!(sub_face_shown(&lathe, &program, 1, None), Some(-5.0));
    assert_eq!(sub_face_at(&lathe, &program, 2, None), Some(25.0));
    assert_eq!(sub_face_shown(&lathe, &program, 2, None), Some(-5.0));
    assert_eq!(
        sub_face_shown(&lathe, &program, 4, Some(-200.0)),
        Some(-5.0)
    );
    let mut none = lathe;
    none.axes.sub_spindle = false;
    assert_eq!(sub_face_shown(&none, &program, 2, None), None);
}

/// The turret's body against the sub-spindle's chuck (100 mm across,
/// without end): reaching 20 mm towards it from the tool it runs into it
/// in every operation where the sub-spindle is near; reaching 13.8 mm only
/// at the hole's bottom; without a turret body only the tool's point is
/// checked, which is clear.
#[test]
fn the_turret_s_body_runs_into_the_sub_spindle_s_chuck() {
    let program = sub_program();
    assert!(sub_spindle_hits(&sub_lathe(), &program, None).is_empty());
    let far = with_lathe(&sub_lathe(), |l| {
        l.turret_body = turret([0.0, 80.0], [-10.0, 20.0]);
    });
    let hits = sub_spindle_hits(&far, &program, None);
    assert_eq!(
        words(&hits),
        vec![
            "Part off: the turret runs into the sub-spindle's chuck.".to_owned(),
            "Back face: the turret runs into the sub-spindle's chuck.".to_owned(),
            "Back hole: the turret runs into the sub-spindle's chuck.".to_owned(),
            "Back flat: the turret runs into the sub-spindle's chuck.".to_owned(),
        ]
    );
    // Where the view draws the tool: the parting blade's first place; the
    // back's facing at its start on the part where it was.
    assert_eq!(hits[0].at, Some([12.0, 0.0, -23.0]));
    assert_eq!(hits[1].operation, 1);
    assert_eq!(hits[1].at, Some([12.0, 0.0, -20.0]));
    // The lathe check gives the same.
    let placement = TurnPlacement {
        work_z: -200.0,
        chuck_z: None,
    };
    let found: Vec<String> = check_lathe(&far, &program, &placement)
        .into_iter()
        .map(|f| f.message)
        .filter(|m| m.contains("turret"))
        .collect();
    assert_eq!(found, words(&hits));
    // 13.8 mm: the hole's bottom 11 mm in front of the jaws' face is the only
    // place it reaches; first where the drill is 1.5 mm into the back.
    let near = with_lathe(&sub_lathe(), |l| {
        l.turret_body = turret([0.0, 80.0], [-10.0, 13.8]);
    });
    let hits = sub_spindle_hits(&near, &program, None);
    assert_eq!(
        words(&hits),
        vec!["Back hole: the turret runs into the sub-spindle's chuck.".to_owned()]
    );
    let at = hits[0].at.unwrap();
    assert!(
        (at[0] - 6.0).abs() < 1e-9 && at[1].abs() < 1e-9 && (at[2] + 18.5).abs() < 0.3,
        "{at:?}"
    );
    // Starting 51 mm out from the tool it passes over the chuck.
    let over = with_lathe(&sub_lathe(), |l| {
        l.turret_body = turret([51.0, 80.0], [-10.0, 20.0]);
    });
    assert!(sub_spindle_hits(&over, &program, None).is_empty());
}

/// A guard moving with the sub-spindle 100 to 120 mm from the axis, 7 mm
/// in front of its jaws' face to 5 mm behind it: the tool never reaches
/// it, the turret's body 95 mm out from the tool does where the
/// sub-spindle holds the part.
#[test]
fn the_turret_s_body_runs_into_a_zone_moving_with_the_sub_spindle() {
    let program = sub_program();
    let guarded = with_lathe(&sub_lathe(), |l| {
        l.sub_spindle.chuck_diameter = 0.0;
        l.sub_spindle.zones = vec![Zone {
            name: "Guard".into(),
            bounds: Bounds {
                min: [100.0, -100.0, -7.0],
                max: [120.0, 100.0, 5.0],
            },
        }];
    });
    assert!(sub_spindle_hits(&guarded, &program, None).is_empty());
    let turreted = with_lathe(&guarded, |l| {
        l.turret_body = turret([0.0, 95.0], [-10.0, 10.0]);
    });
    assert_eq!(
        words(&sub_spindle_hits(&turreted, &program, None)),
        vec![
            "Back face: the turret runs into Guard, which moves with the sub-spindle.".to_owned(),
            "Back hole: the turret runs into Guard, which moves with the sub-spindle.".to_owned(),
            "Back flat: the turret runs into Guard, which moves with the sub-spindle.".to_owned(),
        ]
    );
}

/// The profile's nose 14 mm across, 12 mm in front of the jaws' face: the
/// back's hole 6 mm off the axis runs into it where it is 11 mm in front
/// of the face; the tool's other places are clear.
#[test]
fn a_profile_s_nose_is_checked_as_the_chuck_is() {
    let program = sub_program();
    let nosed = with_lathe(&sub_lathe(), |l| {
        l.sub_spindle.chuck_diameter = 0.0;
        l.sub_spindle.profile = nose();
    });
    let hits = sub_spindle_hits(&nosed, &program, None);
    assert_eq!(
        words(&hits),
        vec!["Back hole: the tool runs into the sub-spindle's body.".to_owned()]
    );
    let at = hits[0].at.unwrap();
    assert!((at[2] + 17.0).abs() < 0.3, "{at:?}");
    // A shallower hole stays clear of it.
    let mut shallow = sub_program();
    if let Some(live) = &mut shallow.operations[2].live {
        live.path = back_hole(-2.0);
    }
    assert!(sub_spindle_hits(&nosed, &shallow, None).is_empty());
    // The turret's body reaching 20 mm towards the sub-spindle finds the
    // body behind the face as well.
    let turreted = with_lathe(&nosed, |l| {
        l.turret_body = turret([0.0, 80.0], [-10.0, 20.0]);
    });
    let found = words(&sub_spindle_hits(&turreted, &program, None));
    assert!(
        found.contains(&"Back face: the turret runs into the sub-spindle's body.".to_owned()),
        "{found:?}"
    );
}
