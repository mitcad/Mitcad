// SPDX-License-Identifier: MIT
//! A tapped hole mirrored or patterned (mitcad#2501), with the mock kernel:
//! the threads query lists the hole's cosmetic thread on each copy of its
//! wall too, at the copy's place, with the hole's size, class, hand and
//! part; a plain hole's copies have none, and a modelled thread's copies
//! are listed as before.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::FeatureStatus;
use mitcad_model::features::FeatureDef;
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::{Axis, Cylinder};
use mitcad_model::test_support::{Block, block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const TOP: &str = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";
const WALL: &str = "F3:hole0.wall";
const COPY: &str = "F4:inst1(F3:hole0.wall)";

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

/// Hole1 (F3): 10 deep at (10, 10) on the block's top, with `extra`.
fn hole_def(extra: Value) -> FeatureDef<mitcad_model::ValueInput> {
    let mut value = json!({"type": "hole",
        "placement": {"type": "face", "body": "F2.b0", "face": TOP, "points": [[10, 10, 20]]},
        "diameter": 6, "extent": {"type": "distance", "depth": 10}});
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    def(value)
}

/// The block (60 x 40 x 20) with Hole1 (F3) made with `extra`; the wall
/// and its mirror image across x = 30 are cylinders as a kernel gives a
/// hole's wall: their axes point into the material, down from the top.
fn holed(extra: Value) -> (Block, FeatureUid) {
    let mut b = block();
    {
        let mut cylinders = b.doc.kernel().face_cylinders.borrow_mut();
        for (name, x) in [(WALL, 10.0), (COPY, 50.0)] {
            cylinders.insert(
                name.to_owned(),
                Cylinder {
                    axis: Axis {
                        origin: [x, 10.0, 20.0],
                        direction: [0.0, 0.0, -1.0],
                    },
                    radius: 4.917_468_245_269_452 / 2.0,
                    length: 10.0,
                    internal: true,
                },
            );
        }
    }
    let uid = b.doc.add_feature(&hole_def(extra), None).unwrap().uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    (b, uid)
}

/// Mirror1 (F4): Hole1 mirrored across the plane x = 30.
fn mirrored(doc: &mut Document<MockKernel>, hole: FeatureUid) -> FeatureUid {
    let uid = doc
        .add_feature(
            &def(json!({"type": "mirror",
                        "objects": {"type": "features", "features": [hole]},
                        "plane": {"origin": [30, 0, 0], "normal": [1, 0, 0]}})),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(doc, uid), FeatureStatus::Ok);
    let faces: Vec<String> = doc
        .body_shape("F2.b0".parse().unwrap())
        .unwrap()
        .faces
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(faces.iter().any(|f| f == COPY), "{faces:?}");
    uid
}

fn near(value: &Value, expected: [f64; 3]) -> bool {
    (0..3).all(|i| {
        value[i]
            .as_f64()
            .is_some_and(|v| (v - expected[i]).abs() < 1e-9)
    })
}

#[test]
fn a_mirrored_tapped_holes_copy_has_the_cosmetic_thread_at_its_place() {
    let (mut b, hole) = holed(json!({"thread": {"designation": "M6x1", "class": "6H",
                                                "length": 4, "offset": 2}}));
    let alone = query(&b.doc, json!({"query": "threads", "cosmetic": true}));
    assert_eq!(alone.as_array().map(Vec::len), Some(1), "{alone}");
    mirrored(&mut b.doc, hole);

    // Both walls, each at its own place, with the hole's thread: its
    // designation, class and hand, and its part 4 long from 2 below the
    // hole's start (the low end of each wall's axis) on each.
    let listed = query(&b.doc, json!({"query": "threads", "cosmetic": true}));
    let threads = listed.as_array().unwrap();
    assert_eq!(threads.len(), 2, "{listed}");
    for (thread, face, x) in [(&threads[0], WALL, 10.0), (&threads[1], COPY, 50.0)] {
        for (key, wanted) in [
            ("feature", json!("F3")),
            ("body", json!("F2.b0")),
            ("face", json!(face)),
            ("designation", json!("M6x1")),
            ("class", json!("6H")),
            ("right_handed", json!(true)),
            ("modeled", json!(false)),
            ("internal", json!(true)),
            ("pitch", json!(1.0)),
        ] {
            assert_eq!(thread[key], wanted, "{key}: {listed}");
        }
        assert!(near(&thread["start"], [x, 10.0, 18.0]), "{listed}");
        assert!(near(&thread["end"], [x, 10.0, 14.0]), "{listed}");
    }
    // The query without `cosmetic` lists the same.
    assert_eq!(query(&b.doc, json!({"query": "threads"})), listed);

    // An edit of the hole's tap changes both: M8x1.25 left-handed, 5 long
    // from 1 below the start.
    b.doc
        .edit_feature(
            hole,
            &hole_def(json!({"thread": {"designation": "M8x1.25", "class": "6H",
                                        "right_handed": false, "length": 5, "offset": 1}})),
        )
        .unwrap();
    assert_eq!(status(&b.doc, hole), FeatureStatus::Ok);
    let edited = query(&b.doc, json!({"query": "threads", "cosmetic": true}));
    let threads = edited.as_array().unwrap();
    assert_eq!(threads.len(), 2, "{edited}");
    for (thread, face, x) in [(&threads[0], WALL, 10.0), (&threads[1], COPY, 50.0)] {
        assert_eq!(thread["face"], face, "{edited}");
        assert_eq!(thread["designation"], "M8x1.25", "{edited}");
        assert_eq!(thread["right_handed"], false, "{edited}");
        assert_eq!(thread["pitch"], 1.25, "{edited}");
        assert!(near(&thread["start"], [x, 10.0, 19.0]), "{edited}");
        assert!(near(&thread["end"], [x, 10.0, 14.0]), "{edited}");
    }

    // Over the whole wall: from the top to the bottom of each.
    b.doc
        .edit_feature(hole, &hole_def(json!({"thread": {"designation": "M6x1"}})))
        .unwrap();
    let whole = query(&b.doc, json!({"query": "threads", "cosmetic": true}));
    let threads = whole.as_array().unwrap();
    assert_eq!(threads.len(), 2, "{whole}");
    for (thread, x) in [(&threads[0], 10.0), (&threads[1], 50.0)] {
        assert!(near(&thread["start"], [x, 10.0, 20.0]), "{whole}");
        assert!(near(&thread["end"], [x, 10.0, 10.0]), "{whole}");
    }

    // Suppressed, the hole has no thread, nor its copy.
    b.doc.set_suppressed(hole, true).unwrap();
    assert_eq!(query(&b.doc, json!({"query": "threads"})), json!([]));
}

#[test]
fn a_mirrored_plain_hole_has_no_thread() {
    let (mut b, hole) = holed(json!({}));
    mirrored(&mut b.doc, hole);
    assert_eq!(query(&b.doc, json!({"query": "threads"})), json!([]));
    assert_eq!(
        query(&b.doc, json!({"query": "threads", "cosmetic": true})),
        json!([])
    );
}

#[test]
fn a_mirrored_modelled_tap_is_listed_as_before() {
    // A hole with a modelled thread has no tool body to mirror: a mirror of
    // the feature is refused, as before mitcad#2501, and so is an edit
    // that models the tap of a hole a mirror repeats.
    let (mut b, hole) = holed(json!({"thread": {"designation": "M6x1", "modeled": true}}));
    let before = query(&b.doc, json!({"query": "threads"}));
    let error = b
        .doc
        .add_feature(
            &def(json!({"type": "mirror",
                        "objects": {"type": "features", "features": [hole]},
                        "plane": {"origin": [30, 0, 0], "normal": [1, 0, 0]}})),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("a hole has no tool body"), "{error}");
    let (mut cosmetic, tapped) = holed(json!({"thread": {"designation": "M6x1"}}));
    mirrored(&mut cosmetic.doc, tapped);
    let error = cosmetic
        .doc
        .edit_feature(
            tapped,
            &hole_def(json!({"thread": {"designation": "M6x1", "modeled": true}})),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("a hole has no tool body"), "{error}");

    // A mirror of the body copies the modelled thread with it, in the
    // geometry: the view draws no rings, and the threads query lists the
    // hole's own wall only, as before.
    let mirror = b
        .doc
        .add_feature(
            &def(json!({"type": "mirror",
                        "objects": {"type": "bodies", "bodies": ["F2.b0"]},
                        "plane": {"origin": [70, 0, 0], "normal": [1, 0, 0]}})),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, mirror), FeatureStatus::Ok);
    assert_eq!(
        query(&b.doc, json!({"query": "threads", "cosmetic": true})),
        json!([])
    );
    let listed = query(&b.doc, json!({"query": "threads"}));
    assert_eq!(listed, before);
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .all(|thread| thread["face"] == WALL),
        "{listed}"
    );
}

#[test]
fn patterned_and_body_mirrored_copies_of_a_tapped_hole_have_its_thread() {
    // A pattern's copies (`F4:inst1`, `F4:inst2`) take the thread as a
    // mirror's copy does.
    let (mut b, hole) = holed(json!({"thread": {"designation": "M6x1", "class": "6H"}}));
    let pattern = b
        .doc
        .add_feature(
            &def(json!({"type": "rectangular_pattern",
                        "objects": {"type": "features", "features": [hole]},
                        "direction1": {"axis": "x", "quantity": 3, "distance": 20},
                        "distance_type": "spacing"})),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    let listed = query(&b.doc, json!({"query": "threads", "cosmetic": true}));
    let mut faces: Vec<&str> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|thread| thread["face"].as_str().unwrap())
        .collect();
    faces.sort_unstable();
    assert_eq!(
        faces,
        [WALL, "F4:inst1(F3:hole0.wall)", "F4:inst2(F3:hole0.wall)"],
        "{listed}"
    );
    for thread in listed.as_array().unwrap() {
        assert_eq!(thread["feature"], "F3", "{listed}");
        assert_eq!(thread["designation"], "M6x1", "{listed}");
        assert_eq!(thread["class"], "6H", "{listed}");
    }
    b.doc.undo();

    // A mirror of the body: the image is a body of its own, whose copy of
    // the wall has the thread.
    let mirror = b
        .doc
        .add_feature(
            &def(json!({"type": "mirror",
                        "objects": {"type": "bodies", "bodies": ["F2.b0"]},
                        "plane": {"origin": [70, 0, 0], "normal": [1, 0, 0]}})),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, mirror), FeatureStatus::Ok);
    let listed = query(&b.doc, json!({"query": "threads", "cosmetic": true}));
    let threads = listed.as_array().unwrap();
    assert_eq!(threads.len(), 2, "{listed}");
    let wall = threads
        .iter()
        .find(|t| t["face"] == WALL)
        .expect("the wall");
    let copy = threads
        .iter()
        .find(|t| t["face"] == COPY)
        .expect("the copy");
    assert_eq!(wall["body"], "F2.b0", "{listed}");
    assert_ne!(copy["body"], wall["body"], "{listed}");
    assert_eq!(copy["designation"], "M6x1", "{listed}");
}
