// SPDX-License-Identifier: MIT
//! A 3D sketch created on a planar face keeps the frame it was placed with
//! (mitcad#2465, follow-up of mitcad#2409), with the mock kernel: the
//! creating edit stores the face's frame in the support (`placed`) in its
//! own undo step; a support moved onto another face by an edit stores that
//! face's frame; a side face facing near +X that leans the other way (a
//! draft's pull direction flipped or its angle's sign changed) carries the
//! sketch's points about the hinge, where the default frame would turn half
//! a turn; a 3D sketch saved before keeps the default frame, opened or
//! edited.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

/// The box's side at x = 60 (`side1`), the face the 3D sketch is on.
const SIDE: &str = "F1:side1";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.query(&value.to_string()).unwrap()).unwrap()
}

fn sketch3d(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "sketch3d", "uid": "F2"}))
}

fn triple(value: &Value) -> [f64; 3] {
    let v = |i: usize| {
        value[i]
            .as_f64()
            .unwrap_or_else(|| panic!("no number at {i}: {value}"))
    };
    [v(0), v(1), v(2)]
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The side's plane leaning by `degrees` about the hinge, the line x = 60,
/// y = 0 along Z, as a draft about the XZ plane leans it: normal
/// (cos, sin, 0).
fn lean(kernel: &MockKernel, degrees: f64) {
    let a = degrees.to_radians();
    kernel.surfaces.borrow_mut().insert(
        SIDE.to_owned(),
        SurfaceGeometry::Plane {
            origin: [60.0, 0.0, 0.0],
            normal: [a.cos(), a.sin(), 0.0],
        },
    );
    // The opposite side, upright, for a support moved there.
    kernel.surfaces.borrow_mut().insert(
        "F1:side3".to_owned(),
        SurfaceGeometry::Plane {
            origin: [0.0, 0.0, 0.0],
            normal: [-1.0, 0.0, 0.0],
        },
    );
}

/// A kernel with box faces and the side leaning by `degrees`.
fn kernel(degrees: f64) -> MockKernel {
    let kernel = MockKernel::default();
    kernel.box_faces.set(true);
    lean(&kernel, degrees);
    kernel
}

/// The position of the 3D sketch's first point in the component.
fn position(doc: &Document<MockKernel>) -> [f64; 3] {
    let s = sketch3d(doc);
    triple(&s["points"][0]["position"])
}

/// `p` turned about the hinge (Z through (60, 0)) by `degrees`.
fn about_hinge(p: [f64; 3], degrees: f64) -> [f64; 3] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (dx, dy) = (p[0] - 60.0, p[1]);
    [60.0 + dx * cos - dy * sin, dx * sin + dy * cos, p[2]]
}

/// The design text without the frames its sketches were placed with: as
/// saved before them. Counts what it took out.
fn without_placed(value: &mut Value, taken: &mut usize) {
    match value {
        Value::Object(map) => {
            if map.remove("placed").is_some() {
                *taken += 1;
            }
            map.values_mut().for_each(|v| without_placed(v, taken));
        }
        Value::Array(items) => items.iter_mut().for_each(|v| without_placed(v, taken)),
        _ => {}
    }
}

fn status_ok(doc: &Document<MockKernel>) {
    let s = sketch3d(doc);
    assert!(s["failure"].is_null(), "{s}");
    assert!(s["frame"].is_object(), "{s}");
}

/// A 60 x 40 x 20 box (F1) with its side at x = 60 leaning 5 degrees, a 3D
/// sketch (F2) created on that side and a fixed point on it, off its
/// middle; the point's position.
fn sketch_on_the_side() -> (Document<MockKernel>, [f64; 3]) {
    let mut doc = Document::new(kernel(5.0));
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box",
               "plane": {"type": "origin", "plane": "xy"}, "corner": [0, 0],
               "length": 60, "width": 40, "height": 20, "operation": "new_body"}}),
    );
    let created = command(
        &mut doc,
        json!({"cmd": "sketch3d.create", "support": {"face": SIDE, "body": "F1.b0"}}),
    );
    assert_eq!(created["uid"], "F2", "{created}");
    status_ok(&doc);
    // The support keeps the frame the sketch was created with: the frame
    // it has.
    let s = sketch3d(&doc);
    let placed = &s["support"]["placed"];
    assert!(placed.is_object(), "{s}");
    let frame = &s["frame"];
    for axis in ["origin", "x_axis", "y_axis"] {
        assert!(
            distance(triple(&placed[axis]), triple(&frame[axis])) < 1e-9,
            "{axis}: {s}"
        );
    }
    // A face facing near +X: the default rule's x is near -Y.
    assert!(triple(&frame["x_axis"])[1] < -0.9, "{s}");
    // A point about (60, 26, 13) on the side, whichever way the axes point.
    let origin = triple(&frame["origin"]);
    let offset = [60.0 - origin[0], 26.0 - origin[1], 13.0 - origin[2]];
    let u = (dot(offset, triple(&frame["x_axis"])) * 1000.0).round() / 1000.0;
    let v = (dot(offset, triple(&frame["y_axis"])) * 1000.0).round() / 1000.0;
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": "F2", "at": [u, v, 0], "fixed": true}),
    );
    status_ok(&doc);
    let at = position(&doc);
    assert!(
        at[1] > 20.0 && at[1] < 32.0 && at[2] > 10.0 && at[2] < 16.0,
        "{at:?}"
    );
    (doc, at)
}

#[test]
fn a_3d_sketch_created_on_a_face_stores_its_frame_in_the_same_step() {
    let (mut doc, _) = sketch_on_the_side();
    // The point's step and the creation's: undone, no sketch and no frame.
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(!doc.to_json().contains("\"placed\""));
    command(&mut doc, json!({"cmd": "redo"}));
    let s = sketch3d(&doc);
    assert!(s["support"]["placed"].is_object(), "{s}");
}

#[test]
fn a_3d_sketch_on_a_leaning_face_keeps_its_place_when_the_lean_changes_side() {
    let (doc, at) = sketch_on_the_side();
    let saved = doc.to_json();
    // The side leans the other way (-5 degrees, a flipped pull direction or
    // an angle of the other sign) and further (8 degrees): the point turns
    // with the side about the hinge, on the same side of the face.
    for degrees in [-5.0, 8.0, 5.0] {
        let mut opened = Document::from_json(&saved, kernel(degrees)).unwrap();
        opened.recompute();
        status_ok(&opened);
        let now = position(&opened);
        let expected = about_hinge(at, degrees - 5.0);
        assert!(
            distance(now, expected) < 0.01,
            "{degrees} degrees: {now:?}, expected {expected:?}"
        );
    }
}

#[test]
fn a_3d_sketch_saved_before_keeps_the_default_frame() {
    let (doc, at) = sketch_on_the_side();
    let mut old: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let mut taken = 0;
    without_placed(&mut old, &mut taken);
    assert_eq!(taken, 1, "{old}");
    let old = old.to_string();
    // Opened as it was: where it was, and an edit of it stores no frame.
    let mut opened = Document::from_json(&old, kernel(5.0)).unwrap();
    opened.recompute();
    status_ok(&opened);
    assert!(distance(position(&opened), at) < 1e-6);
    command(
        &mut opened,
        json!({"cmd": "sketch3d.add_point", "sketch": "F2", "at": [1, 1, 0]}),
    );
    status_ok(&opened);
    assert!(!opened.to_json().contains("\"placed\""));
    // The side leaning the other way turns its default frame half a turn,
    // as before: the point is not where the lean takes it.
    let mut opened = Document::from_json(&old, kernel(-5.0)).unwrap();
    opened.recompute();
    status_ok(&opened);
    assert!(distance(position(&opened), about_hinge(at, -10.0)) > 1.0);
    assert!(!opened.to_json().contains("\"placed\""));
}

#[test]
fn a_3d_sketch_moved_onto_another_face_stores_that_face_s_frame() {
    let (mut doc, _) = sketch_on_the_side();
    let feature = query(&doc, json!({"query": "feature", "uid": "F2"}));
    let mut def = feature["def"].clone();
    assert!(def["support"]["placed"].is_object(), "{feature}");
    // The support moved to the opposite side, as a client gives it: no
    // placed frame.
    def["support"] = json!({"face": "F1:side3", "body": "F1.b0"});
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": def}),
    );
    status_ok(&doc);
    let s = sketch3d(&doc);
    assert_eq!(s["support"]["face"], "F1:side3", "{s}");
    let placed = &s["support"]["placed"];
    assert!(placed.is_object(), "{s}");
    for axis in ["origin", "x_axis", "y_axis"] {
        assert!(
            distance(triple(&placed[axis]), triple(&s["frame"][axis])) < 1e-9,
            "{axis}: {s}"
        );
    }
    // The same face again without the frame: the one it had stays.
    let kept = placed.clone();
    let mut def = query(&doc, json!({"query": "feature", "uid": "F2"}))["def"].clone();
    def["support"] = json!({"face": "F1:side3", "body": "F1.b0"});
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": def}),
    );
    assert_eq!(sketch3d(&doc)["support"]["placed"], kept);
}
