// SPDX-License-Identifier: MIT
//! Coordinate System (mitcad#1775): frames at a point with axes along
//! edges, and exports in a frame, with the kernel of the construction tests
//! (`GeoKernel`: the mock plus the geometry of extruded rectangles).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::exchange::read_stl;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

use crate::construction_tests::{
    BACK, GeoKernel, RIGHT, assert_near, block, command, edge, face, query, side, top, vertex,
};

/// The block's corner at (60, 40, 20).
fn corner() -> Value {
    vertex([&top(), &side(RIGHT), &side(BACK)])
}

/// The top edge along the back, from (60, 40, 20) to (0, 40, 20).
fn back_top() -> Value {
    edge(&top(), &side(BACK))
}

/// The upright edge at (60, 40), from z 0 to 20.
fn back_right() -> Value {
    edge(&side(RIGHT), &side(BACK))
}

/// The command that adds a coordinate system of these fields.
fn add(fields: &Value) -> Value {
    let mut def = fields.clone();
    def["type"] = json!("coordinate_system");
    json!({"cmd": "add_feature", "def": def})
}

/// Adds a coordinate system that evaluates; returns its uid.
fn construct(doc: &mut Document<GeoKernel>, fields: Value) -> String {
    let added = command(doc, add(&fields));
    let uid = added["uid"].as_str().unwrap().to_owned();
    let feature = query(doc, json!({"query": "feature", "uid": uid}));
    assert_eq!(feature["status"], "ok", "{fields}: {}", feature["error"]);
    uid
}

fn frame(doc: &mut Document<GeoKernel>, fields: Value) -> Value {
    let uid = construct(doc, fields);
    query(doc, json!({"query": "datum", "uid": uid}))
}

/// Why the model refuses a coordinate system of these fields.
fn refusal(doc: &mut Document<GeoKernel>, fields: Value) -> String {
    let command = add(&fields);
    doc.command(&command.to_string())
        .map(|r| panic!("{command} succeeded: {r}"))
        .unwrap_err()
        .to_string()
}

/// The corner frame: origin at (60, 40, 20), X along the back's top edge
/// away from the corner (-X), Z down the upright edge (-Z), so Y = Z × X is
/// +Y.
fn corner_frame(doc: &mut Document<GeoKernel>) -> String {
    construct(
        doc,
        json!({"origin": corner(), "z_axis": back_right(), "x_axis": back_top()}),
    )
}

#[test]
fn a_frame_at_a_corner_has_its_axes_along_the_edges_away_from_it() {
    let mut doc = block();
    let uid = corner_frame(&mut doc);
    let datum = query(&doc, json!({"query": "datum", "uid": uid}));
    assert_eq!(datum["type"], "plane");
    assert_eq!(datum["name"], "CoordinateSystem1");
    assert_near(&datum["origin"], [60.0, 40.0, 20.0]);
    assert_near(&datum["x_axis"], [-1.0, 0.0, 0.0]);
    assert_near(&datum["y_axis"], [0.0, 1.0, 0.0]);
    assert_near(&datum["normal"], [0.0, 0.0, -1.0]);

    // The flips turn each axis round, Y following; the offset moves along
    // Z and the angle turns X and Y about it.
    let flipped = frame(
        &mut doc,
        json!({"origin": corner(), "z_axis": back_right(), "x_axis": back_top(),
               "flip_z": true, "flip_x": true}),
    );
    assert_near(&flipped["x_axis"], [1.0, 0.0, 0.0]);
    assert_near(&flipped["normal"], [0.0, 0.0, 1.0]);
    assert_near(&flipped["y_axis"], [0.0, 1.0, 0.0]);
    let moved = frame(
        &mut doc,
        json!({"origin": corner(), "z_axis": back_right(), "x_axis": back_top(),
               "offset": 5, "angle": "90 deg"}),
    );
    assert_near(&moved["origin"], [60.0, 40.0, 15.0]);
    assert_near(&moved["x_axis"], [0.0, 1.0, 0.0]);
    assert_near(&moved["y_axis"], [1.0, 0.0, 0.0]);
    assert_near(&moved["normal"], [0.0, 0.0, -1.0]);

    // Z from a face's normal (the top's, +Z), X from the component's.
    let on_top = frame(
        &mut doc,
        json!({"origin": corner(), "z_axis": face(&top())}),
    );
    assert_near(&on_top["x_axis"], [1.0, 0.0, 0.0]);
    assert_near(&on_top["normal"], [0.0, 0.0, 1.0]);
    // An origin plane's normal (XZ: +Y) for Z, an origin axis for X.
    let planes = frame(
        &mut doc,
        json!({"origin": "origin", "z_axis": "xz", "x_axis": "z"}),
    );
    assert_near(&planes["normal"], [0.0, 1.0, 0.0]);
    assert_near(&planes["x_axis"], [0.0, 0.0, 1.0]);
    assert_near(&planes["y_axis"], [1.0, 0.0, 0.0]);
    // Only the origin: the component's axes.
    let plain = frame(&mut doc, json!({"origin": corner()}));
    assert_near(&plain["x_axis"], [1.0, 0.0, 0.0]);
    assert_near(&plain["y_axis"], [0.0, 1.0, 0.0]);

    // It follows its references: a taller block moves the corner up.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d3", "value": 25}),
    );
    let taller = query(&doc, json!({"query": "datum", "uid": uid}));
    assert_near(&taller["origin"], [60.0, 40.0, 25.0]);
    assert_near(&taller["normal"], [0.0, 0.0, -1.0]);
}

#[test]
fn a_frame_is_construction_geometry_that_sketches_and_joints_take() {
    let mut doc = block();
    let uid = corner_frame(&mut doc);
    let feature = query(&doc, json!({"query": "feature", "uid": uid}));
    assert_eq!(feature["type"], "coordinate_system");
    // A sketch on it: its plane is the frame.
    command(&mut doc, json!({"cmd": "sketch.create", "plane": uid}));
    let sketch = query(&doc, json!({"query": "sketch", "uid": "F4"}));
    assert_near(&sketch["frame"]["origin"], [60.0, 40.0, 20.0]);
    assert_near(&sketch["frame"]["x_axis"], [-1.0, 0.0, 0.0]);
    assert_near(&sketch["frame"]["y_axis"], [0.0, 1.0, 0.0]);
    // Another frame's Z from it: its normal.
    let stacked = frame(&mut doc, json!({"origin": corner(), "z_axis": uid}));
    assert_near(&stacked["normal"], [0.0, 0.0, -1.0]);
}

#[test]
fn frames_refuse_what_gives_no_origin_or_direction() {
    let mut doc = block();
    // A point gives no direction, nor does a vertex.
    let error = refusal(&mut doc, json!({"origin": corner(), "z_axis": "origin"}));
    assert!(error.contains("gives no direction"), "{error}");
    let error = refusal(&mut doc, json!({"origin": corner(), "z_axis": corner()}));
    assert!(error.contains("is not an axis"), "{error}");
    // A face is no origin.
    let error = refusal(&mut doc, json!({"origin": face(&top())}));
    assert!(error.contains("is not a point or a vertex"), "{error}");
    // The same reference twice.
    let error = refusal(
        &mut doc,
        json!({"origin": corner(), "z_axis": back_top(), "x_axis": back_top()}),
    );
    assert!(error.contains("selected twice"), "{error}");
    // X along Z: the feature fails.
    let added = command(
        &mut doc,
        add(&json!({"origin": corner(), "z_axis": back_right(), "x_axis": "z"})),
    );
    let feature = query(&doc, json!({"query": "feature", "uid": added["uid"]}));
    assert_eq!(feature["status"], "error");
    let error = feature["error"].as_str().unwrap();
    assert!(error.contains("the X axis is along the Z axis"), "{error}");
}

fn triangles(path: &str) -> Vec<[[f64; 3]; 3]> {
    read_stl(&std::fs::read(path).unwrap())
}

fn box_of(triangles: &[[[f64; 3]; 3]]) -> ([f64; 3], [f64; 3]) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in triangles.iter().flatten() {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    (min, max)
}

#[track_caller]
fn assert_box(actual: ([f64; 3], [f64; 3]), expected: ([f64; 3], [f64; 3])) {
    let near = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() <= 1e-6);
    assert!(
        near(actual.0, expected.0) && near(actual.1, expected.1),
        "{actual:?}, expected {expected:?}"
    );
}

#[test]
fn exports_write_the_bodies_in_a_frame() {
    let mut doc = block();
    let uid = corner_frame(&mut doc);
    let dir = mitcad_model::testing::unique_temp_dir("coordinate-system");
    let stl = dir.join("frame.stl").to_str().unwrap().to_owned();

    // STL: the block's corner at the origin, the back's top edge along +X
    // and the upright edge along +Z: x 0..60, y -40..0, z 0..20.
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": stl, "coordinates": {"frame": "CoordinateSystem1"}}),
    );
    assert_eq!(result["bodies"], json!(["F2.b0"]));
    let written = triangles(&stl);
    assert_eq!(written.len(), 12);
    assert_box(box_of(&written), ([0.0, -40.0, 0.0], [60.0, 0.0, 20.0]));
    // By uid too; the design's coordinates as before.
    command(
        &mut doc,
        json!({"cmd": "export", "path": stl, "coordinates": {"frame": uid}}),
    );
    assert_box(
        box_of(&triangles(&stl)),
        ([0.0, -40.0, 0.0], [60.0, 0.0, 20.0]),
    );
    command(&mut doc, json!({"cmd": "export", "path": stl}));
    assert_box(
        box_of(&triangles(&stl)),
        ([0.0, 0.0, 0.0], [60.0, 40.0, 20.0]),
    );
    mitcad_model::testing::remove_temp(&dir);

    // STEP, IGES, OBJ and BRep: the body's placement takes it into the
    // frame, the corner onto the origin.
    for path in ["frame.step", "frame.igs", "frame.obj", "frame.brep"] {
        command(
            &mut doc,
            json!({"cmd": "export", "path": path, "coordinates": {"frame": "CoordinateSystem1"}}),
        );
        let placements = doc
            .kernel()
            .mock
            .written_placements
            .borrow()
            .last()
            .cloned()
            .unwrap();
        assert_eq!(placements.len(), 1, "{path}");
        assert_eq!(placements[0].len(), 1, "{path}");
        let t: Transform = placements[0][0].transform;
        let near = |p: [f64; 3], q: [f64; 3]| (0..3).all(|i| (p[i] - q[i]).abs() <= 1e-9);
        assert!(
            near(t.apply_point([60.0, 40.0, 20.0]), [0.0; 3]),
            "{path}: {t:?}"
        );
        assert!(
            near(t.apply_point([0.0, 40.0, 20.0]), [60.0, 0.0, 0.0]),
            "{path}: {t:?}"
        );
        assert!(
            near(t.apply_point([60.0, 40.0, 0.0]), [0.0, 0.0, 20.0]),
            "{path}: {t:?}"
        );
        assert!(t.is_rigid(), "{path}: {t:?}");
    }

    // What takes no frame, and what is no frame.
    for (bad, expected) in [
        (
            json!({"cmd": "export", "path": "a.usdz", "coordinates": {"frame": uid}}),
            "a USDZ file keeps the design's coordinates",
        ),
        (
            json!({"cmd": "export", "path": "a.3mf", "on_bed": true, "coordinates": {"frame": uid}}),
            "not in a coordinate system",
        ),
        (
            json!({"cmd": "export", "path": "a.stl", "coordinates": {"frame": "Sketch1"}}),
            "Sketch1 is not a coordinate system",
        ),
        (
            json!({"cmd": "export", "path": "a.stl", "coordinates": {"frame": "Nowhere"}}),
            "there is no coordinate system Nowhere",
        ),
    ] {
        let error = doc
            .command(&bad.to_string())
            .map(|r| panic!("{bad} succeeded: {r}"))
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{bad}: {error}");
    }

    // Rolled back, it has no frame.
    command(&mut doc, json!({"cmd": "set_marker", "position": 2}));
    let error = doc
        .command(
            &json!({"cmd": "export", "path": "a.stl", "coordinates": {"frame": uid}}).to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("has no frame at the timeline marker"),
        "{error}"
    );
}
