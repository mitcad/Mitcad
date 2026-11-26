// SPDX-License-Identifier: MIT
//! Tests of the turret's body against the main chuck and the machine's
//! fixed zones, a turret body per station, the zones' Y on a lathe with a
//! Y axis, and the zones and the turret as Simulate draws them
//! (mitcad#2412), on the pin parted off into the sub-spindle of
//! sub_live_tests.rs: the parting off on the main spindle at work Z -23,
//! the back faced, drilled and milled (a flat across Y) where the
//! sub-spindle holds the part pulled back 30 mm.

use super::sub_live_tests::{sub_lathe, sub_program};
use super::*;
use crate::machine::{Bounds, Finding, Machine, Zone};

fn with_lathe(machine: &Machine, change: impl FnOnce(&mut Lathe)) -> Machine {
    let mut m = machine.clone();
    if let Some(lathe) = &mut m.lathe {
        change(lathe);
    }
    m
}

fn zone(name: &str, min: [f64; 3], max: [f64; 3]) -> Zone {
    Zone {
        name: name.into(),
        bounds: Bounds { min, max },
    }
}

fn words(hits: &[Finding]) -> Vec<String> {
    hits.iter().map(|f| f.message.clone()).collect()
}

fn station(station: u32, x: [f64; 2], z: [f64; 2]) -> StationBody {
    StationBody { station, x, z }
}

/// The work zero at machine Z -200, the chuck's jaws' face at work Z -40.
fn placement(chuck_z: Option<f64>) -> TurnPlacement {
    TurnPlacement {
        work_z: -200.0,
        chuck_z,
    }
}

#[test]
fn a_turret_body_per_station_is_kept_and_refused_in_words() {
    let lathe = Lathe {
        turret_body: Some(TurretBody {
            x: [0.0, 80.0],
            z: [-10.0, 20.0],
        }),
        station_bodies: vec![station(3, [-5.0, 120.0], [-30.0, 25.0])],
        ..Lathe::default()
    };
    lathe.validate().unwrap();
    // Left out when none, read back when set.
    let json = serde_json::to_value(&lathe).unwrap();
    assert_eq!(
        json["station_bodies"],
        serde_json::json!([{"station": 3, "x": [-5.0, 120.0], "z": [-30.0, 25.0]}])
    );
    let back: Lathe = serde_json::from_value(json).unwrap();
    assert_eq!(back, lathe);
    let json = serde_json::to_value(Lathe::default()).unwrap();
    assert!(json.get("station_bodies").is_none(), "{json}");
    // The station's own body for its tools, the turret's for the others,
    // none without either.
    assert_eq!(
        lathe.turret_body_at(3),
        Some(TurretBody {
            x: [-5.0, 120.0],
            z: [-30.0, 25.0]
        })
    );
    assert_eq!(lathe.turret_body_at(4), lathe.turret_body);
    let only = Lathe {
        station_bodies: vec![station(3, [0.0, 1.0], [0.0, 1.0])],
        ..Lathe::default()
    };
    assert!(only.turret_body_at(4).is_none());
    assert!(only.turret_body_at(3).is_some());
    // Refused in words: a station off the turret, a station twice, ends
    // the wrong way round.
    let message = "A station's turret body needs a station on the turret, each station once.";
    for bodies in [
        vec![station(13, [0.0, 1.0], [0.0, 1.0])],
        vec![station(0, [0.0, 1.0], [0.0, 1.0])],
        vec![
            station(2, [0.0, 1.0], [0.0, 1.0]),
            station(2, [0.0, 2.0], [0.0, 2.0]),
        ],
    ] {
        let bad = Lathe {
            station_bodies: bodies,
            ..Lathe::default()
        };
        assert_eq!(bad.validate().unwrap_err(), message);
    }
    let bad = Lathe {
        station_bodies: vec![station(2, [1.0, 0.0], [0.0, 1.0])],
        ..Lathe::default()
    };
    assert_eq!(
        bad.validate().unwrap_err(),
        "The turret's body needs X and Z from the tool, each from a lowest up to a highest."
    );
}

/// The main chuck (160 mm across) with its jaws' face at work Z -40: the
/// parting blade at -23 is clear of it, the turret's body reaching 30 mm
/// back towards it from the tool is not; the back's operations, where the
/// sub-spindle holds the part far off, are clear.
#[test]
fn the_turret_s_body_runs_into_the_main_chuck() {
    let program = sub_program();
    let reaching = with_lathe(&sub_lathe(), |l| {
        l.turret_body = Some(TurretBody {
            x: [0.0, 80.0],
            z: [-30.0, 20.0],
        });
    });
    let hits = turret_hits(&reaching, &program, &placement(Some(-40.0)));
    assert_eq!(
        words(&hits),
        vec!["Part off: the turret runs into the chuck.".to_owned()]
    );
    assert_eq!(hits[0].operation, 0);
    assert_eq!(hits[0].at, Some([12.0, 0.0, -23.0]));
    // Without the chuck's place, or reaching only 10 mm back, clear.
    assert!(turret_hits(&reaching, &program, &placement(None)).is_empty());
    let short = with_lathe(&sub_lathe(), |l| {
        l.turret_body = Some(TurretBody {
            x: [0.0, 80.0],
            z: [-10.0, 20.0],
        });
    });
    assert!(turret_hits(&short, &program, &placement(Some(-40.0))).is_empty());
    // Starting 81 mm out from the tool it passes over the chuck.
    let over = with_lathe(&sub_lathe(), |l| {
        l.turret_body = Some(TurretBody {
            x: [81.0, 100.0],
            z: [-30.0, 20.0],
        });
    });
    assert!(turret_hits(&over, &program, &placement(Some(-40.0))).is_empty());
    // The lathe check gives the same, with the tool's own findings.
    let found: Vec<String> = check_lathe(&reaching, &program, &placement(Some(-40.0)))
        .into_iter()
        .map(|f| f.message)
        .filter(|m| m.ends_with("the turret runs into the chuck."))
        .collect();
    assert_eq!(found, words(&hits));
    // The parting blade's station's own body, the rest without one: only
    // the parting off is checked; the station's own over the turret's.
    let own = with_lathe(&sub_lathe(), |l| {
        l.station_bodies = vec![station(2, [0.0, 80.0], [-30.0, 20.0])];
    });
    assert_eq!(
        words(&turret_hits(&own, &program, &placement(Some(-40.0)))),
        words(&hits)
    );
    let narrow = with_lathe(&reaching, |l| {
        l.station_bodies = vec![station(2, [81.0, 100.0], [-30.0, 20.0])];
    });
    assert!(turret_hits(&narrow, &program, &placement(Some(-40.0))).is_empty());
}

/// A fixed zone 80 to 90 mm from the axis at machine Z -240 to -230, 40 to
/// 30 mm behind the work zero: the tool never reaches it, the turret's body
/// 80 mm out from the tool does while parting off; the back's operations,
/// where the sub-spindle holds the part pulled back, are clear.
#[test]
fn the_turret_s_body_runs_into_a_fixed_zone() {
    let program = sub_program();
    let guarded = with_lathe(&sub_lathe(), |l| {
        l.turret_body = Some(TurretBody {
            x: [0.0, 80.0],
            z: [-30.0, 20.0],
        });
    });
    let mut guarded = guarded;
    guarded.zones = vec![zone("Tailstock", [80.0, 0.0, -240.0], [90.0, 0.0, -230.0])];
    let hits = turret_hits(&guarded, &program, &placement(None));
    assert_eq!(
        words(&hits),
        vec!["Part off: the turret runs into Tailstock.".to_owned()]
    );
    assert_eq!(hits[0].at, Some([12.0, 0.0, -23.0]));
    // The lathe check: the turret's, not the tool's.
    let found: Vec<String> = check_lathe(&guarded, &program, &placement(None))
        .into_iter()
        .map(|f| f.message)
        .filter(|m| m.contains("Tailstock"))
        .collect();
    assert_eq!(found, words(&hits));
}

/// A fixed zone where the back's operations bring the turret's body
/// (80 to 90 mm from the axis, machine Z -180 to -175), its Y checked on a
/// lathe with a Y axis: the facing turns at the machine's Y 0, the hole is
/// drilled there, the flat milled from 0 to -5 (the sub-spindle's Y turned
/// round); a zone whose Y bounds are equal not limited in Y; on a lathe
/// without a Y axis Y not checked.
#[test]
fn a_fixed_zone_s_y_is_checked_on_a_lathe_with_a_y_axis() {
    let program = sub_program();
    let lathe = with_lathe(&sub_lathe(), |l| {
        l.turret_body = Some(TurretBody {
            x: [0.0, 80.0],
            z: [-30.0, 20.0],
        });
    });
    let found = |y: [f64; 2], machine: &Machine| {
        let mut m = machine.clone();
        m.zones = vec![zone("Guard", [80.0, y[0], -180.0], [90.0, y[1], -175.0])];
        words(&turret_hits(&m, &program, &placement(None)))
    };
    let face = "Back face: the turret runs into Guard.".to_owned();
    let hole = "Back hole: the turret runs into Guard.".to_owned();
    let flat = "Back flat: the turret runs into Guard.".to_owned();
    let all = vec![face.clone(), hole.clone(), flat.clone()];
    assert!(found([10.0, 20.0], &lathe).is_empty());
    assert_eq!(found([-10.0, 10.0], &lathe), all);
    assert_eq!(found([-3.0, -1.0], &lathe), vec![flat.clone()]);
    assert!(found([1.0, 3.0], &lathe).is_empty());
    assert_eq!(found([7.0, 7.0], &lathe), all);
    let no_y = with_lathe(&lathe, |l| l.axes.y = false);
    assert_eq!(found([10.0, 20.0], &no_y), all);
    // The tool's own point against a fixed zone: the facing at Y 0.
    let mut tool = sub_lathe();
    tool.zones = vec![zone("Guard", [11.0, 10.0, -225.0], [13.0, 20.0, -175.0])];
    let messages: Vec<String> = check_lathe(&tool, &program, &placement(None))
        .into_iter()
        .map(|f| f.message)
        .filter(|m| m.contains("Guard"))
        .collect();
    assert!(messages.is_empty(), "{messages:?}");
    tool.zones[0].bounds.min[1] = -20.0;
    let messages: Vec<String> = check_lathe(&tool, &program, &placement(None))
        .into_iter()
        .map(|f| f.message)
        .filter(|m| m.starts_with("Back face:") && m.contains("Guard"))
        .collect();
    assert_eq!(
        messages,
        vec!["Back face: the tool runs into Guard.".to_owned()]
    );
}

/// A guard moving with the sub-spindle (as in sub_turret_tests.rs) 100 to
/// 120 mm from the axis: its Y checked on a lathe with a Y axis, by the
/// turret's body and by the tool's point alike.
#[test]
fn a_moving_zone_s_y_is_checked_on_a_lathe_with_a_y_axis() {
    let program = sub_program();
    let found = |y: [f64; 2], y_axis: bool| {
        let machine = with_lathe(&sub_lathe(), |l| {
            l.axes.y = y_axis;
            l.sub_spindle.chuck_diameter = 0.0;
            l.turret_body = Some(TurretBody {
                x: [0.0, 95.0],
                z: [-10.0, 10.0],
            });
            l.sub_spindle.zones = vec![zone("Guard", [100.0, y[0], -7.0], [120.0, y[1], 5.0])];
        });
        words(&sub_spindle_hits(&machine, &program, None))
    };
    let face =
        "Back face: the turret runs into Guard, which moves with the sub-spindle.".to_owned();
    let hole =
        "Back hole: the turret runs into Guard, which moves with the sub-spindle.".to_owned();
    let flat =
        "Back flat: the turret runs into Guard, which moves with the sub-spindle.".to_owned();
    let all = vec![face.clone(), hole.clone(), flat.clone()];
    assert!(found([10.0, 20.0], true).is_empty());
    assert_eq!(found([-3.0, -1.0], true), vec![flat.clone()]);
    assert!(found([1.0, 3.0], true).is_empty());
    assert_eq!(found([5.0, 5.0], true), all);
    assert_eq!(found([10.0, 20.0], false), all);
    // The tool's own point: the guard of sub_body_tests.rs across the
    // facing's start and the flat's way back at the machine's Y -5.
    let tool = |y: [f64; 2]| {
        let machine = with_lathe(&sub_lathe(), |l| {
            l.sub_spindle.zones = vec![zone("Guard", [11.0, y[0], -15.5], [13.0, y[1], -14.5])];
        });
        words(&sub_spindle_hits(&machine, &program, None))
    };
    let tool_face =
        "Back face: the tool runs into Guard, which moves with the sub-spindle.".to_owned();
    let tool_flat =
        "Back flat: the tool runs into Guard, which moves with the sub-spindle.".to_owned();
    assert_eq!(
        tool([-100.0, 100.0]),
        vec![tool_face.clone(), tool_flat.clone()]
    );
    assert_eq!(tool([-6.0, -4.0]), vec![tool_flat.clone()]);
    assert!(tool([50.0, 60.0]).is_empty());
}

/// The zones as Simulate draws them: the fixed one where the machine has
/// it for the main spindle's facing and 30 mm nearer the main spindle for
/// the sub-spindle's operations (the view keeps the part where it was);
/// the moving one where the sub-spindle waits, then at the grip; Y on a
/// lathe with a Y axis only.
#[test]
fn simulate_draws_the_zones_where_they_are() {
    let mut machine = with_lathe(&sub_lathe(), |l| {
        l.sub_spindle.zones = vec![zone("Guard", [100.0, -100.0, -7.0], [120.0, 100.0, 5.0])];
    });
    machine.zones = vec![
        zone("Tailstock", [90.0, -5.0, -240.0], [120.0, 5.0, -230.0]),
        zone("Door", [150.0, 0.0, -300.0], [160.0, 0.0, 100.0]),
    ];
    let mut program = sub_program();
    let mut facing = program.operations[1].clone();
    facing.name = "Face".into();
    facing.sub = false;
    program.operations.insert(0, facing);
    let shown = |index: usize, work_z: Option<f64>| zones_shown(&machine, &program, index, work_z);
    let drawn = |name: &str, moving: bool, min: [f64; 3], max: [f64; 3]| ZoneShown {
        name: name.into(),
        moving,
        min,
        max,
    };
    // The facing: the fixed zones 200 mm on from the machine's Z; the
    // guard where the sub-spindle waits (machine Z 300, work Z 500).
    assert_eq!(
        shown(0, Some(-200.0)),
        vec![
            drawn("Tailstock", false, [90.0, -5.0, -40.0], [120.0, 5.0, -30.0]),
            drawn("Door", false, [150.0, 0.0, -100.0], [160.0, 0.0, 300.0]),
            drawn("Guard", true, [100.0, -100.0, 493.0], [120.0, 100.0, 505.0]),
        ]
    );
    // The parting off: the sub-spindle at the grip (work Z -5).
    assert_eq!(shown(1, Some(-200.0))[2].min, [100.0, -100.0, -12.0]);
    // The back's drilling: the fixed zones 30 mm nearer, the guard at the
    // grip where the view keeps the part.
    let drilling = shown(3, Some(-200.0));
    assert_eq!(drilling[0].min, [90.0, -5.0, -70.0]);
    assert_eq!(drilling[2].min, [100.0, -100.0, -12.0]);
    // Without the work zero's place: no fixed zone, no guard for the
    // facing, the guard at the grip for the back.
    assert!(shown(0, None).is_empty());
    assert_eq!(shown(3, None).len(), 1);
    // On a lathe without a Y axis the zones are drawn flat at Y 0.
    machine = with_lathe(&machine, |l| l.axes.y = false);
    let flat = zones_shown(&machine, &program, 0, Some(-200.0));
    assert_eq!(flat[0].min, [90.0, 0.0, -40.0]);
    assert_eq!(flat[2].max, [120.0, 0.0, 505.0]);
}

/// The turret's body as Simulate draws it round the tool: the tool's
/// station's own, along the view's X for turning, along the plane's C for
/// a live tool on a plane (Y turned round on the sub-spindle), away from
/// the axis through the tool for one on the face; none without a body.
#[test]
fn simulate_draws_the_turret_round_the_tool() {
    let mut program = sub_program();
    let lathe = Lathe {
        turret_body: Some(TurretBody {
            x: [0.0, 80.0],
            z: [-10.0, 20.0],
        }),
        station_bodies: vec![station(2, [-5.0, 60.0], [-15.0, 15.0])],
        ..sub_lathe().lathe.unwrap()
    };
    // The parting blade at station 2: its own body, along X.
    assert_eq!(
        turret_shown(&lathe, &program, 0),
        Some(TurretShown {
            x: [-5.0, 60.0],
            z: [-15.0, 15.0],
            x_axis: Some([1.0, 0.0]),
        })
    );
    // The back's facing: the turret's body, along X.
    let face = turret_shown(&lathe, &program, 1).unwrap();
    assert_eq!((face.x, face.z), ([0.0, 80.0], [-10.0, 20.0]));
    assert_eq!(face.x_axis, Some([1.0, 0.0]));
    // The hole on the face: away from the axis through the tool.
    assert_eq!(turret_shown(&lathe, &program, 2).unwrap().x_axis, None);
    // The flat on a plane at C 90 on the sub-spindle: the view's -Y; on
    // the main spindle +Y.
    if let Some(live) = &mut program.operations[3].live {
        live.side = LiveSide::Plane { c: 90.0, b: 90.0 };
    }
    let axis = turret_shown(&lathe, &program, 3).unwrap().x_axis.unwrap();
    assert!(
        axis[0].abs() < 1e-12 && (axis[1] + 1.0).abs() < 1e-12,
        "{axis:?}"
    );
    program.operations[3].sub = false;
    let axis = turret_shown(&lathe, &program, 3).unwrap().x_axis.unwrap();
    assert!(
        axis[0].abs() < 1e-12 && (axis[1] - 1.0).abs() < 1e-12,
        "{axis:?}"
    );
    // No body: nothing drawn.
    let bare = sub_lathe().lathe.unwrap();
    assert!(turret_shown(&bare, &program, 0).is_none());
    assert!(turret_shown(&lathe, &program, 9).is_none());
}
