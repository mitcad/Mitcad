// SPDX-License-Identifier: MIT
//! A thread's fit to its face (mitcad#1262), with the mock kernel: a size
//! far from the face's diameter and a modelled thread that cuts through
//! the wall behind its face are warnings on the result, which is kept.
//! The material behind the faces is the mock's tube
//! (`MockKernel::tube_bore`): a bore about the rod's axis.

use serde_json::{Value, json};

use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

/// The rod's round face: circle c1 extruded by Extrude1 (F2).
const SIDE: &str = "F2:side(c1)";
/// The wall of Hole1 (F3) through the rod.
const BORE: &str = "F3:hole0.wall";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn status(doc: &Document<MockKernel>, uid: u64) -> FeatureStatus {
    doc.status(FeatureUid(uid)).cloned().expect("recomputed")
}

fn warnings(doc: &Document<MockKernel>, uid: u64) -> Vec<String> {
    doc.warnings(FeatureUid(uid)).to_vec()
}

/// The rod's shape history: the thread's result kept in it.
fn history(doc: &Document<MockKernel>) -> String {
    doc.body_shape(BodyUid::new(FeatureUid(2), 0))
        .unwrap()
        .history
        .clone()
}

/// A rod of `diameter` 9 long (Sketch1 F1, Extrude1 F2), with a hole of
/// `hole` through it (Hole1 F3) when given.
fn rod(diameter: f64, hole: Option<f64>) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": diameter}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1}"}],
               "extent": {"type": "distance", "distance": 9}, "operation": "new_body"}}),
    );
    if let Some(hole) = hole {
        command(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": "hole",
                   "placement": {"type": "plane", "plane": "xy", "points": [[0, 0, 0]]},
                   "diameter": hole, "flip": true,
                   "extent": {"type": "distance", "depth": 9}}}),
        );
        assert_eq!(status(&doc, 3), FeatureStatus::Ok);
    }
    doc
}

/// Adds a thread of `thread` on `face`, `modeled` or cosmetic: its uid.
fn add_thread(doc: &mut Document<MockKernel>, face: &str, thread: Value, modeled: bool) -> u64 {
    let added = command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "thread",
               "faces": [{"body": "F2.b0", "face": face}],
               "thread": thread, "modeled": modeled}}),
    );
    let uid = added["uid"]
        .as_str()
        .and_then(|u| u.strip_prefix('F'))
        .and_then(|u| u.parse().ok())
        .unwrap_or_else(|| panic!("no uid in {added}"));
    assert_eq!(status(doc, uid), FeatureStatus::Ok, "{added}");
    uid
}

#[test]
fn a_thread_far_from_its_face_that_cuts_through_the_wall_warns_and_is_kept() {
    // The ticket's ring: Ø87.1 outside, a Ø80.1 bore, a wall of 3.5 mm.
    // M80x2 cuts the face down to its profile, its root (Ø77.835) inside
    // the bore: the ring is gone along the thread, and the thread says so.
    let mut doc = rod(87.1, None);
    doc.kernel().tube_bore.set(Some(80.1 / 2.0));
    let uid = add_thread(
        &mut doc,
        SIDE,
        json!({"designation": "M80x2", "class": "6g"}),
        true,
    );
    let on_face = "M80x2 (Ø80 mm) on face F2:side(c1) (Ø87.1 mm)";
    assert_eq!(
        warnings(&doc, uid),
        [
            format!(
                "{on_face}: the face's diameter is 7.1 mm larger than the thread's major diameter"
            ),
            format!("{on_face}: the thread cut through the 3.5 mm wall behind the face"),
        ]
    );
    // The result is kept.
    assert_eq!(doc.kernel().count("thread"), 1);
    assert!(history(&doc).starts_with("thread("), "{}", history(&doc));
}

#[test]
fn a_thread_that_fits_its_face_and_wall_does_not_warn() {
    // M80x2 on a Ø80 face with a Ø70 bore: a 5 mm wall, the thread 1.08 mm
    // deep.
    let mut doc = rod(80.0, None);
    doc.kernel().tube_bore.set(Some(35.0));
    let uid = add_thread(
        &mut doc,
        SIDE,
        json!({"designation": "M80x2", "class": "6g"}),
        true,
    );
    assert!(warnings(&doc, uid).is_empty(), "{:?}", warnings(&doc, uid));
    assert!(history(&doc).starts_with("thread("), "{}", history(&doc));
}

#[test]
fn an_internal_thread_through_a_thin_outer_wall_warns() {
    // A Ø8.5 hole (about M10x1.5's minor diameter, Ø8.376) in a Ø9.8 rod:
    // the wall to the outside is 0.65 mm, the thread cuts 0.75 mm out to
    // its major diameter.
    let mut doc = rod(9.8, Some(8.5));
    doc.kernel().tube_bore.set(Some(8.5 / 2.0));
    let uid = add_thread(
        &mut doc,
        BORE,
        json!({"designation": "M10x1.5", "class": "6H"}),
        true,
    );
    assert_eq!(
        warnings(&doc, uid),
        [
            "M10x1.5 (Ø10 mm, minor Ø8.376 mm) on face F3:hole0.wall (Ø8.5 mm): the thread cut \
          through the 0.65 mm wall behind the face"
        ]
    );
    assert!(history(&doc).starts_with("thread("), "{}", history(&doc));
    // In a Ø12 rod the 1.75 mm wall holds.
    let mut doc = rod(12.0, Some(8.5));
    doc.kernel().tube_bore.set(Some(8.5 / 2.0));
    let uid = add_thread(
        &mut doc,
        BORE,
        json!({"designation": "M10x1.5", "class": "6H"}),
        true,
    );
    assert!(warnings(&doc, uid).is_empty(), "{:?}", warnings(&doc, uid));
}

#[test]
fn a_hole_far_from_the_threads_minor_diameter_warns_even_when_cosmetic() {
    // M8x1.25 in a Ø5 hole (M6x1's): a cosmetic thread changes no
    // geometry, but the size does not suit the hole.
    let mut doc = rod(20.0, Some(5.0));
    let uid = add_thread(
        &mut doc,
        BORE,
        json!({"designation": "M8x1.25", "class": "6H"}),
        false,
    );
    assert_eq!(
        warnings(&doc, uid),
        [
            "M8x1.25 (Ø8 mm, minor Ø6.647 mm) on face F3:hole0.wall (Ø5 mm): the hole's diameter \
          is 1.647 mm smaller than the thread's minor diameter"
        ]
    );
    // M6x1 (minor Ø4.917) suits it.
    let mut doc = rod(20.0, Some(5.0));
    let uid = add_thread(
        &mut doc,
        BORE,
        json!({"designation": "M6x1", "class": "6H"}),
        false,
    );
    assert!(warnings(&doc, uid).is_empty(), "{:?}", warnings(&doc, uid));
}
