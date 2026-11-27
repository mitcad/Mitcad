// SPDX-License-Identifier: MIT
//! A cosmetic Thread feature mirrored or patterned (mitcad#2537), with the
//! mock kernel: the threads query lists the thread on each copy of its
//! face too, at the copy's place, with the thread's size, class, hand and
//! part, as it lists a tapped hole's (mitcad#2501). A copy of the
//! cylinder alone (a mirror of the boss without the thread, a mirror of
//! the body made before the thread) has none, and a copy a Thread of its
//! own threads is listed under that Thread only.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::FeatureStatus;
use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::{Axis, Cylinder};
use mitcad_model::test_support::{Block, block, def, extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

/// The boss' round face: Sketch2's circle (F3) joined by Extrude2 (F4).
const SIDE: &str = "F4:side(c1)";

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn cosmetic(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "threads", "cosmetic": true}))
        .as_array()
        .unwrap()
        .clone()
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    let uid = doc.add_feature(&def(value), None).unwrap().uid;
    assert_eq!(status(doc, uid), FeatureStatus::Ok, "{uid}");
    uid
}

/// `face` as the kernel gives a boss' round face 10 across standing 10
/// high on the block's top at (x, y): its axis from the top of the boss
/// down to the block, as an extruded circle's runs.
fn cylinder(doc: &Document<MockKernel>, face: &str, [x, y]: [f64; 2]) {
    doc.kernel().face_cylinders.borrow_mut().insert(
        face.to_owned(),
        Cylinder {
            axis: Axis {
                origin: [x, y, 30.0],
                direction: [0.0, 0.0, -1.0],
            },
            radius: 5.0,
            length: 10.0,
            internal: false,
        },
    );
}

/// The block (60 x 40 x 20, F2) with a boss at (10, 10) (Sketch2 F3,
/// Extrude2 F4) joined to it.
fn bossed() -> Block {
    let mut b = block();
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_circle(sketch, [10.0, 10.0], &num(5.0))
        .unwrap()
        .region;
    let join = extrude(sketch, &[&region], 10.0, "join", &[b.body]);
    let boss = b.doc.add_feature(&join, None).unwrap().uid;
    assert_eq!(boss, FeatureUid(4));
    cylinder(&b.doc, SIDE, [10.0, 10.0]);
    b
}

/// Thread1 on the boss with `extra`: M10x1.5 6g unless it says otherwise.
fn thread_def(extra: Value) -> FeatureDef<mitcad_model::ValueInput> {
    let mut value = json!({"type": "thread", "faces": [{"body": "F2.b0", "face": SIDE}],
                           "thread": {"designation": "M10x1.5", "class": "6g"}});
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    def(value)
}

/// The boss with Thread1 (F5) made with `extra`.
fn threaded(extra: Value) -> (Block, FeatureUid) {
    let mut b = bossed();
    let uid = b.doc.add_feature(&thread_def(extra), None).unwrap().uid;
    assert_eq!(uid, FeatureUid(5));
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    (b, uid)
}

fn mirror(objects: Value, x: f64) -> Value {
    json!({"type": "mirror", "objects": objects,
           "plane": {"origin": [x, 0, 0], "normal": [1, 0, 0]}})
}

fn faces(doc: &Document<MockKernel>, body: &str) -> Vec<String> {
    doc.body_shape(body.parse().unwrap())
        .unwrap()
        .faces
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn near(value: &Value, expected: [f64; 3]) -> bool {
    (0..3).all(|i| {
        value[i]
            .as_f64()
            .is_some_and(|v| (v - expected[i]).abs() < 1e-9)
    })
}

#[test]
fn a_mirrored_cosmetic_threads_copy_has_the_thread_at_its_place() {
    // 4 long from 1 below the high end of the axis (the block's top, z =
    // 20): from z = 25 down to z = 21.
    let (mut b, thread) = threaded(json!({"length": 4, "offset": 1}));
    assert_eq!(cosmetic(&b.doc).len(), 1);
    let copy = "F6:inst1(F4:side(c1))";
    cylinder(&b.doc, copy, [50.0, 10.0]);
    let mirrored = add(
        &mut b.doc,
        mirror(
            json!({"type": "features", "features": ["F4", thread]}),
            30.0,
        ),
    );
    assert!(faces(&b.doc, "F2.b0").iter().any(|f| f == copy));

    // The boss and its image, each at its own place, with the thread's
    // designation, class, hand and part.
    let threads = cosmetic(&b.doc);
    assert_eq!(threads.len(), 2, "{threads:?}");
    for (thread, face, x) in [(&threads[0], SIDE, 10.0), (&threads[1], copy, 50.0)] {
        for (key, wanted) in [
            ("feature", json!("F5")),
            ("body", json!("F2.b0")),
            ("face", json!(face)),
            ("designation", json!("M10x1.5")),
            ("class", json!("6g")),
            ("right_handed", json!(true)),
            ("modeled", json!(false)),
            ("internal", json!(false)),
            ("pitch", json!(1.5)),
            ("radius", json!(5.0)),
        ] {
            assert_eq!(thread[key], wanted, "{key}: {threads:?}");
        }
        assert!(near(&thread["start"], [x, 10.0, 25.0]), "{threads:?}");
        assert!(near(&thread["end"], [x, 10.0, 21.0]), "{threads:?}");
    }
    // The query without `cosmetic` lists the same.
    assert_eq!(
        query(&b.doc, json!({"query": "threads"})),
        Value::Array(threads)
    );

    // An edit of the thread changes both on recompute: M10x1.25
    // left-handed, 6 long from 2 below the high end.
    b.doc
        .edit_feature(
            thread,
            &thread_def(json!({"thread": {"designation": "M10x1.25", "class": "6g",
                                          "right_handed": false},
                               "length": 6, "offset": 2})),
        )
        .unwrap();
    assert_eq!(status(&b.doc, thread), FeatureStatus::Ok);
    assert_eq!(status(&b.doc, mirrored), FeatureStatus::Ok);
    let edited = cosmetic(&b.doc);
    assert_eq!(edited.len(), 2, "{edited:?}");
    for (thread, face, x) in [(&edited[0], SIDE, 10.0), (&edited[1], copy, 50.0)] {
        assert_eq!(thread["face"], face, "{edited:?}");
        assert_eq!(thread["designation"], "M10x1.25", "{edited:?}");
        assert_eq!(thread["right_handed"], false, "{edited:?}");
        assert_eq!(thread["pitch"], 1.25, "{edited:?}");
        assert!(near(&thread["start"], [x, 10.0, 28.0]), "{edited:?}");
        assert!(near(&thread["end"], [x, 10.0, 22.0]), "{edited:?}");
    }

    // Over the whole face: from the top of each boss to the block.
    b.doc.edit_feature(thread, &thread_def(json!({}))).unwrap();
    let whole = cosmetic(&b.doc);
    assert_eq!(whole.len(), 2, "{whole:?}");
    for (thread, x) in [(&whole[0], 10.0), (&whole[1], 50.0)] {
        assert!(near(&thread["start"], [x, 10.0, 30.0]), "{whole:?}");
        assert!(near(&thread["end"], [x, 10.0, 20.0]), "{whole:?}");
    }

    // Suppressed, the thread is on neither.
    b.doc.set_suppressed(thread, true).unwrap();
    assert_eq!(query(&b.doc, json!({"query": "threads"})), json!([]));
}

#[test]
fn a_mirrored_plain_cylinder_has_no_thread() {
    // No thread at all.
    let mut b = bossed();
    cylinder(&b.doc, "F5:inst1(F4:side(c1))", [50.0, 10.0]);
    add(
        &mut b.doc,
        mirror(json!({"type": "features", "features": ["F4"]}), 30.0),
    );
    assert!(
        faces(&b.doc, "F2.b0")
            .iter()
            .any(|f| f == "F5:inst1(F4:side(c1))")
    );
    assert_eq!(query(&b.doc, json!({"query": "threads"})), json!([]));
    assert!(cosmetic(&b.doc).is_empty());

    // The boss mirrored without its thread: the image is a plain cylinder,
    // and the thread stays on the boss alone.
    let (mut b, _) = threaded(json!({}));
    let copy = "F6:inst1(F4:side(c1))";
    cylinder(&b.doc, copy, [50.0, 10.0]);
    add(
        &mut b.doc,
        mirror(json!({"type": "features", "features": ["F4"]}), 30.0),
    );
    assert!(faces(&b.doc, "F2.b0").iter().any(|f| f == copy));
    let threads = cosmetic(&b.doc);
    assert_eq!(threads.len(), 1, "{threads:?}");
    assert_eq!(threads[0]["face"], SIDE, "{threads:?}");

    // A mirror of the body made before the thread: its image of the boss
    // has no thread either.
    let mut b = bossed();
    let image = add(
        &mut b.doc,
        mirror(json!({"type": "bodies", "bodies": ["F2.b0"]}), 70.0),
    );
    assert_eq!(image, FeatureUid(5));
    let thread = add(
        &mut b.doc,
        json!({"type": "thread", "faces": [{"body": "F2.b0", "face": SIDE}],
               "thread": {"designation": "M10x1.5", "class": "6g"}}),
    );
    let threads = cosmetic(&b.doc);
    assert_eq!(threads.len(), 1, "{threads:?}");
    assert_eq!(threads[0]["feature"], thread.to_string(), "{threads:?}");
    assert_eq!(threads[0]["face"], SIDE, "{threads:?}");
}

#[test]
fn patterned_and_body_mirrored_copies_of_a_cosmetic_thread_have_it() {
    // A pattern's copies (`F6:inst1`, `F6:inst2`) take the thread as a
    // mirror's copy does, each at its place.
    let (mut b, thread) = threaded(json!({"length": 4, "offset": 1}));
    for (k, x) in [(1, 30.0), (2, 50.0)] {
        cylinder(&b.doc, &format!("F6:inst{k}({SIDE})"), [x, 10.0]);
    }
    add(
        &mut b.doc,
        json!({"type": "rectangular_pattern",
               "objects": {"type": "features", "features": ["F4", thread]},
               "direction1": {"axis": "x", "quantity": 3, "distance": 20},
               "distance_type": "spacing"}),
    );
    let mut threads = cosmetic(&b.doc);
    threads.sort_by_key(|t| t["face"].as_str().unwrap_or_default().to_owned());
    let listed: Vec<&str> = threads
        .iter()
        .map(|t| t["face"].as_str().unwrap())
        .collect();
    assert_eq!(
        listed,
        [SIDE, "F6:inst1(F4:side(c1))", "F6:inst2(F4:side(c1))"],
        "{threads:?}"
    );
    for (thread, x) in threads.iter().zip([10.0, 30.0, 50.0]) {
        assert_eq!(thread["feature"], "F5", "{threads:?}");
        assert_eq!(thread["designation"], "M10x1.5", "{threads:?}");
        assert_eq!(thread["class"], "6g", "{threads:?}");
        assert!(near(&thread["start"], [x, 10.0, 25.0]), "{threads:?}");
        assert!(near(&thread["end"], [x, 10.0, 21.0]), "{threads:?}");
    }
    b.doc.undo();

    // A mirror of the body after the thread: the image is a body of its
    // own, whose copy of the boss has the thread.
    let image = add(
        &mut b.doc,
        mirror(json!({"type": "bodies", "bodies": ["F2.b0"]}), 70.0),
    );
    let threads = cosmetic(&b.doc);
    assert_eq!(threads.len(), 2, "{threads:?}");
    let own = threads
        .iter()
        .find(|t| t["face"] == SIDE)
        .expect("the boss");
    let copy = threads
        .iter()
        .find(|t| t["face"] == format!("{image}:inst1({SIDE})"))
        .expect("the copy");
    assert_eq!(own["body"], "F2.b0", "{threads:?}");
    assert_ne!(copy["body"], own["body"], "{threads:?}");
    assert_eq!(copy["feature"], "F5", "{threads:?}");
    assert_eq!(copy["designation"], "M10x1.5", "{threads:?}");
}

#[test]
fn a_copy_with_a_thread_of_its_own_is_listed_under_it_only() {
    // The boss and its thread mirrored, and the image threaded by hand
    // too (Thread2, F7): the image is listed once, under Thread2.
    let (mut b, thread) = threaded(json!({}));
    let copy = "F6:inst1(F4:side(c1))";
    cylinder(&b.doc, copy, [50.0, 10.0]);
    add(
        &mut b.doc,
        mirror(
            json!({"type": "features", "features": ["F4", thread]}),
            30.0,
        ),
    );
    let own = add(
        &mut b.doc,
        json!({"type": "thread", "faces": [{"body": "F2.b0", "face": copy}],
               "thread": {"designation": "M10x1.25", "class": "6g"}}),
    );
    let threads = cosmetic(&b.doc);
    assert_eq!(threads.len(), 2, "{threads:?}");
    assert_eq!(threads[0]["feature"], "F5", "{threads:?}");
    assert_eq!(threads[0]["face"], SIDE, "{threads:?}");
    assert_eq!(threads[1]["feature"], own.to_string(), "{threads:?}");
    assert_eq!(threads[1]["face"], copy, "{threads:?}");
    assert_eq!(threads[1]["designation"], "M10x1.25", "{threads:?}");
}
