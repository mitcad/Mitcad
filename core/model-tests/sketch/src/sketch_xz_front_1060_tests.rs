// SPDX-License-Identifier: MIT
//! A new sketch on the XZ origin plane is seen from the front (mitcad#1060):
//! Create Sketch's `upright` frame there is x = +X, y = +Z with the normal
//! -Y, the view cube's FRONT side, so an extrude of it goes towards -Y.
//! Sketches already in a design keep their frames, profiles and extrude
//! directions: the XZ frame of mitcad#692 (x = -X, normal +Y) and the
//! plane's own frame (x = +X, y = -Z) read back unchanged. With the mock
//! kernel, whose extrusions' bounds follow their sketches' frames.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn vector(value: &Value) -> [f64; 3] {
    std::array::from_fn(|i| {
        value[i]
            .as_f64()
            .unwrap_or_else(|| panic!("no vector: {value}"))
    })
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= 1e-6)
}

/// Sketch F1's evaluated frame: its x axis, y axis and normal.
fn axes(doc: &Document<MockKernel>) -> [[f64; 3]; 3] {
    let frame = query(doc, json!({"query": "sketch", "uid": "F1"}))["frame"].clone();
    [
        vector(&frame["x_axis"]),
        vector(&frame["y_axis"]),
        vector(&frame["normal"]),
    ]
}

#[track_caller]
fn assert_axes(got: [[f64; 3]; 3], x: [f64; 3], y: [f64; 3], normal: [f64; 3]) {
    assert!(
        near(got[0], x) && near(got[1], y) && near(got[2], normal),
        "axes {got:?}, expected x {x:?}, y {y:?}, normal {normal:?}"
    );
}

/// Sketch F1's own frame as its definition stores it.
fn stored_frame(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "feature", "uid": "F1"}))["def"]["frame"].clone()
}

/// Body F2.b0's box: its lowest and highest corner.
fn bounds(doc: &Document<MockKernel>) -> [[f64; 3]; 2] {
    let shape = doc
        .body_shape(BodyUid::new(FeatureUid(2), 0))
        .expect("no body of F2");
    let b = shape.bounds.expect("bounds");
    [b.min, b.max]
}

#[track_caller]
fn assert_bounds(got: [[f64; 3]; 2], min: [f64; 3], max: [f64; 3]) {
    assert!(
        near(got[0], min) && near(got[1], max),
        "bounds {got:?}, expected {min:?} to {max:?}"
    );
}

/// A design with Sketch1 (F1) made by `create`, a 40 x 20 rectangle at its
/// origin, and Extrude1 (F2) 10 along its normal (Body1, F2.b0).
fn extruded(create: Value) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, create);
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 20}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": R}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}),
    );
    doc
}

#[test]
fn a_new_xz_sketch_is_seen_from_the_front() {
    let doc = extruded(json!({"cmd": "sketch.create", "plane": "xz", "upright": true}));
    // x = +X, y = +Z, the normal -Y: the view looks along +Y, from FRONT.
    assert_axes(
        axes(&doc),
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, -1.0, 0.0],
    );
    // Stored in the plane's coordinates (its x = +X, y = -Z): the own y is
    // the plane's -y.
    let own = stored_frame(&doc);
    assert!(near(vector(&own["x_axis"]), [1.0, 0.0, 0.0]), "{own}");
    assert!(near(vector(&own["y_axis"]), [0.0, -1.0, 0.0]), "{own}");
    // The rectangle lies where it was drawn, x 0 to 40 and z 0 to 20, not
    // mirrored, and the extrude's positive direction is -Y.
    assert_bounds(bounds(&doc), [0.0, -10.0, 0.0], [40.0, 0.0, 20.0]);
}

#[test]
fn existing_xz_sketches_keep_their_frames_and_extrusions() {
    // (what, how the sketch was made, its x, y and normal, Body1's box)
    type Case = (&'static str, Value, [[f64; 3]; 3], [[f64; 3]; 2]);
    let cases: [Case; 2] = [
        // Create Sketch's upright XZ frame before mitcad#1060: x = -X,
        // y = +Z, normal +Y, stored as the plane's -x and -y.
        (
            "the upright frame of mitcad#692",
            json!({"cmd": "sketch.create", "plane": "xz",
                   "frame": {"origin": [0, 0, 0], "x_axis": [-1, 0, 0], "y_axis": [0, -1, 0]}}),
            [[-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]],
            [[-40.0, 0.0, 0.0], [0.0, 10.0, 20.0]],
        ),
        // The plane's own frame (scripts, imports): x = +X, y = -Z.
        (
            "the plane's own frame",
            json!({"cmd": "sketch.create", "plane": "xz"}),
            [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
            [[0.0, 0.0, -20.0], [40.0, 10.0, 0.0]],
        ),
    ];
    for (what, create, [x, y, normal], [min, max]) in cases {
        let doc = extruded(create);
        let regions = query(&doc, json!({"query": "sketch", "uid": "F1"}))["regions"].clone();
        let own = stored_frame(&doc);
        let json = doc.to_json();
        let mut loaded = Document::from_json(&json, MockKernel::default())
            .unwrap_or_else(|e| panic!("{what}: {e}"));
        loaded.recompute();
        assert_eq!(
            loaded.to_json(),
            json,
            "{what}: saving again gives the same file"
        );
        assert_eq!(stored_frame(&loaded), own, "{what}: the stored frame");
        assert_axes(axes(&loaded), x, y, normal);
        assert_eq!(
            query(&loaded, json!({"query": "sketch", "uid": "F1"}))["regions"],
            regions,
            "{what}: the profiles"
        );
        assert_bounds(bounds(&loaded), min, max);
    }
}
