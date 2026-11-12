// SPDX-License-Identifier: MIT
//! Drags of a linkage with two closed loops past its toggle positions
//! (mitcad#1666): the recompute in timeline order reproduces what a drag
//! keeps, and a drag after which a joint would fail moves nothing.

use serde_json::{Value, json};

use mitcad_model::assembly::orthonormality_error;
use mitcad_model::datum::Vec3;
use mitcad_model::ids::{FeatureUid, OccurrenceUid};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn run(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn shift(x: f64, y: f64, z: f64) -> Transform {
    Transform::translation([x, y, z])
}

fn turn(angle: f64) -> Transform {
    Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], angle).unwrap()
}

/// A component without bodies, its occurrence at `at` in the root.
fn link(doc: &mut Document<MockKernel>, name: &str, at: Transform) -> OccurrenceUid {
    let (_, o) = doc.create_component(Some(name), at, false).unwrap();
    o
}

/// An origin on a point of an occurrence's component.
fn pin(occurrence: &str, p: Vec3) -> Value {
    json!({"occurrence": occurrence, "geometry": {"point": p}})
}

/// An origin on a point of the root (fixed).
fn ground(p: Vec3) -> Value {
    json!({"geometry": {"point": p}})
}

fn world(doc: &Document<MockKernel>, o: OccurrenceUid, p: Vec3) -> Vec3 {
    doc.placement(o).unwrap().apply_point(p)
}

/// The reported linkage's layout: a lever drives a rocker through a short
/// link, and the rocker a second rocker (the wheel) through a long rod.
/// The first loop (lever 50, link 25, rocker 20, ground pivots 72 apart) is
/// not a Grashof one, so the lever turns only between two toggle positions
/// (about -5° and 72°, the link and the rocker in line). The rocker turns
/// on an axle hinged to the ground at the same point, as the reported
/// one's discs do (a turn the joints leave to either). Each link is
/// inserted away from where the joints put it.
struct Linkage {
    doc: Document<MockKernel>,
    lever: OccurrenceUid,
    links: Vec<OccurrenceUid>,
    joints: Vec<FeatureUid>,
}

fn linkage() -> Linkage {
    let mut doc = Document::new(MockKernel::default());
    let mut links = Vec::new();
    // Where the joints hold with the lever at 30°, then moved off.
    for (i, (name, at)) in [
        ("Lever", turn(30f64.to_radians())),
        (
            "Link",
            shift(43.301_270_189, 25.0, 0.0).after(&turn(1.595_851_189)),
        ),
        ("Rocker", shift(60.0, 40.0, 0.0).after(&turn(2.618_446_832))),
        (
            "Rod",
            shift(77.325_035_837, 30.007_846_416, 0.0).after(&turn(-0.394_961_042)),
        ),
        (
            "Wheel",
            shift(200.0, -40.0, 0.0).after(&turn(0.661_879_546)),
        ),
        ("Axle", shift(60.0, 40.0, 0.0)),
    ]
    .into_iter()
    .enumerate()
    {
        let i = i as f64;
        let at = shift(7.0 * i, -11.0 * i, 0.0)
            .after(&at)
            .after(&turn(0.2 * i));
        links.push(link(&mut doc, name, at));
    }
    let mut joints = Vec::new();
    for (a, b) in [
        (pin("Lever:1", [0.0; 3]), ground([0.0; 3])),
        (pin("Axle:1", [0.0; 3]), ground([60.0, 40.0, 0.0])),
        (pin("Rocker:1", [0.0; 3]), pin("Axle:1", [0.0; 3])),
        (pin("Link:1", [0.0; 3]), pin("Lever:1", [50.0, 0.0, 0.0])),
        // Closes the first loop.
        (
            pin("Link:1", [25.0, 0.0, 0.0]),
            pin("Rocker:1", [20.0, 0.0, 0.0]),
        ),
        (pin("Wheel:1", [0.0; 3]), ground([200.0, -40.0, 0.0])),
        (pin("Rod:1", [0.0; 3]), pin("Rocker:1", [-20.0, 0.0, 0.0])),
        // Closes the second loop.
        (
            pin("Rod:1", [150.0, 0.0, 0.0]),
            pin("Wheel:1", [20.0, 0.0, 0.0]),
        ),
    ] {
        let result = run(
            &mut doc,
            json!({"cmd": "add_joint", "kind": "revolute", "a": a, "b": b}),
        );
        assert_eq!(result["error"], Value::Null, "{result}");
        joints.push(result["uid"].as_str().unwrap().parse().unwrap());
    }
    Linkage {
        doc,
        lever: links[0],
        links,
        joints,
    }
}

impl Linkage {
    /// The joints that fail, with why.
    fn failed(&self) -> Vec<String> {
        self.joints
            .iter()
            .filter_map(|j| match self.doc.status(*j) {
                Some(FeatureStatus::Failed(e)) => Some(format!("{j}: {}", e.message)),
                _ => None,
            })
            .collect()
    }

    /// The lever's turn about its pivot, in degrees.
    fn lever_angle(&self) -> f64 {
        let t = self.doc.placement(self.lever).unwrap();
        t.linear[1][0].atan2(t.linear[0][0]).to_degrees()
    }
}

#[test]
fn dragging_two_closed_loops_past_their_toggles_keeps_every_joint() {
    let mut l = linkage();
    assert!(l.failed().is_empty(), "{:?}", l.failed());
    let (mut lowest, mut highest) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut kept = 0;
    // Each link pulled around and out of reach, near and far: the lever
    // and the rockers stop at their toggle positions and come back on
    // either side of them.
    for step in 0..140_usize {
        let o = l.links[[0, 0, 2, 0, 4, 1, 3][step % 7]];
        let grab = world(&l.doc, o, [15.0, 0.0, 0.0]);
        let angle = step as f64 * 0.61;
        let radius = [30.0, 120.0, 600.0, 3000.0, 100_000.0, 10.0][step % 6];
        let target = [radius * angle.cos(), radius * angle.sin(), 0.0];
        let Ok(dragged) = l.doc.drag_occurrence(o, Some(grab), target) else {
            continue;
        };
        kept += 1;
        // Every joint still holds where the timeline puts the links ...
        let failed = l.failed();
        assert!(failed.is_empty(), "step {step}: {failed:?}");
        // ... and that is where the drag left them.
        for (moved, kept_at) in &dragged.placements {
            let now = l.doc.placement(*moved).unwrap();
            for p in [[0.0; 3], [100.0, 0.0, 0.0], [0.0, 100.0, 0.0]] {
                let off = mitcad_vec3::distance(kept_at.apply_point(p), now.apply_point(p));
                assert!(off < 1e-4, "step {step}: {moved} recomputed {off} mm away");
            }
        }
        // The placements stay rotations and translations to rounding: a
        // distortion doubled by each solve once shortened the links until
        // the loops could not close (mitcad#1664), and kept just within
        // what opening accepts it still turned circular edges seen from
        // another occurrence into B-splines.
        for o in &l.links {
            let own = l.doc.assembly().occurrence(*o).unwrap().transform;
            let placed = l.doc.placement(*o).unwrap();
            for t in [own, placed] {
                let error = orthonormality_error(&t.linear);
                assert!(error <= 1e-12, "step {step}: {o} off a rotation by {error}");
            }
        }
        lowest = lowest.min(l.lever_angle());
        highest = highest.max(l.lever_angle());
    }
    assert!(kept >= 100, "only {kept} of 140 drags were kept");
    // The lever went to both toggle positions.
    assert!(lowest < -2.0 && highest > 68.0, "{lowest}° to {highest}°");
}

#[test]
fn a_drag_after_which_a_joint_would_fail_moves_nothing() {
    // An arm hinged to the ground at the origin, and an as-built hinge of
    // it whose axis is a marker's origin, there too to begin with: the
    // marker turns on its own hinge 30 off its origin, so dragging it
    // takes that axis away from the arm's hinge. (A drag cannot know that
    // the axis follows the marker.)
    let mut doc = Document::new(MockKernel::default());
    let arm = link(&mut doc, "Arm", Transform::IDENTITY);
    let marker = link(&mut doc, "Marker", Transform::IDENTITY);
    for (a, b) in [
        (pin("Arm:1", [0.0; 3]), ground([0.0; 3])),
        (pin("Marker:1", [30.0, 0.0, 0.0]), ground([30.0, 0.0, 0.0])),
    ] {
        let added = run(
            &mut doc,
            json!({"cmd": "add_joint", "kind": "revolute", "a": a, "b": b}),
        );
        assert_eq!(added["error"], Value::Null, "{added}");
    }
    // The arm's own hinge is driven: it keeps the arm's turn.
    run(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"rz": "0 deg"}}),
    );
    let hinge = run(
        &mut doc,
        json!({"cmd": "add_as_built_joint", "kind": "revolute", "a": "Arm:1", "b": "",
               "origin": pin("Marker:1", [0.0; 3])}),
    );
    assert_eq!(hinge["error"], Value::Null, "{hinge}");
    let hinge: FeatureUid = hinge["uid"].as_str().unwrap().parse().unwrap();
    // The arm turns about both hinges at once; its hinge is driven there.
    run(
        &mut doc,
        json!({"cmd": "drag_occurrence", "occurrence": "Arm:1",
               "point": [100, 0, 0], "target": [80, 60, 0]}),
    );
    assert_eq!(doc.status(hinge), Some(&FeatureStatus::Ok));
    assert!(world(&doc, arm, [100.0, 0.0, 0.0])[1] > 10.0);
    let before = doc.to_json();
    let placements = |d: &Document<MockKernel>| [arm, marker].map(|o| d.placement(o).unwrap());
    let was = placements(&doc);
    // Turning the marker moves the as-built hinge's axis off the arm's own
    // hinge: recomputed there, the two cannot hold together with the arm
    // turned.
    let error = doc
        .command(
            &json!({"cmd": "drag_occurrence", "occurrence": "Marker:1",
                    "point": [0, 0, 0], "target": [5, 20, 0]})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Marker:1 was not moved")
            && error.contains("AsBuiltJoint1 would no longer hold"),
        "{error}"
    );
    assert_eq!(placements(&doc), was);
    assert_eq!(doc.status(hinge), Some(&FeatureStatus::Ok));
    assert_eq!(doc.to_json(), before);
}
