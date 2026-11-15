// SPDX-License-Identifier: MIT
//! Sketches on cylindrical and conical faces (mitcad#1768) through the JSON
//! api with the mock kernel: a sketch on a rod's side in its developed
//! coordinates, a dimensioned rectangle raised and a text sunk on the face,
//! the face split by a line once around it, the curves' lengths on the face
//! and the sketch_surface query's points, rays and nearest points; the
//! sketch following the rod's radius and length; the same on a cone; the
//! refusals. The volumes on OCCT are the cli script's
//! (`tools/cli/tests/face_sketch.json`).

use std::f64::consts::{FRAC_PI_2, FRAC_PI_6, PI};

use serde_json::{Value, json};

use mitcad_model::kernel::{ConeFace, Curve3};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was not refused: {result}"),
        Err(e) => e.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(
        &doc.query(&query.to_string())
            .unwrap_or_else(|e| panic!("{query}: {e}")),
    )
    .unwrap()
}

fn status_ok(doc: &Document<MockKernel>, feature: &str) {
    let uid = feature.parse().unwrap();
    match doc.status(uid) {
        Some(FeatureStatus::Ok) => {}
        other => panic!("{feature}: {other:?}"),
    }
}

fn failure(doc: &Document<MockKernel>, feature: &str) -> String {
    match doc.status(feature.parse().unwrap()) {
        Some(FeatureStatus::Failed(message)) => message.agent_text().to_owned(),
        other => panic!("{feature} did not fail: {other:?}"),
    }
}

fn last_spec(doc: &Document<MockKernel>) -> String {
    doc.kernel()
        .specs
        .borrow()
        .last()
        .cloned()
        .unwrap_or_default()
}

fn near(a: &Value, b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i].as_f64().unwrap() - b[i]).abs() < 1e-9)
}

fn near2(a: &Value, b: [f64; 2], tolerance: f64) -> bool {
    (0..2).all(|i| (a[i].as_f64().unwrap() - b[i]).abs() < tolerance)
}

fn faces(doc: &Document<MockKernel>) -> Vec<String> {
    doc.body_shape("F2.b0".parse().unwrap())
        .map(|s| s.faces.iter().map(ToString::to_string).collect())
        .unwrap_or_default()
}

/// Sketch1 (F1) with a circle of diameter 20 (d1) extruded 30 (d2) into
/// the rod F2.b0, its side F2:side(c1) a cylinder of radius 10 along +Z
/// from z = 0.
fn rod() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 20}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1}"}],
               "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}}),
    );
    status_ok(&d, "F2");
    d
}

/// The curves of the sketch on its face with their lengths there.
fn lengths(doc: &Document<MockKernel>, sketch: &str) -> Vec<(String, f64)> {
    let answer = query(
        doc,
        json!({"query": "sketch_surface", "sketch": sketch, "curves": true}),
    );
    answer["curves"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["id"].as_str().unwrap().to_owned(),
                c["length"].as_f64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn a_sketch_on_a_rods_side_raises_sinks_and_splits_it() {
    let mut d = rod();
    // Sketch2 (F3) on the rod's side, its origin on the +Y side.
    let created = command(
        &mut d,
        json!({"cmd": "sketch.create", "upright": true,
               "plane": {"face": "F2:side(c1)", "body": "F2.b0",
                         "developed": {"angle": FRAC_PI_2}}}),
    );
    assert_eq!(created["uid"], "F3");
    status_ok(&d, "F3");
    let s = query(&d, json!({"query": "sketch", "uid": "F3"}));
    assert_eq!(s["plane"]["developed"]["angle"], json!(FRAC_PI_2), "{s}");
    let surface = &s["surface"];
    assert_eq!(surface["kind"], "cylinder", "{s}");
    assert_eq!(surface["radius"], 10.0);
    assert_eq!(surface["internal"], false);
    assert!(
        near(&surface["axis"]["origin"], [0.0, 0.0, 0.0]),
        "{surface}"
    );
    assert!(near(&surface["axis"]["direction"], [0.0, 0.0, 1.0]));
    // The carrier plane touches the rod at +Y, seen from outside; upright
    // is ignored (the sketch's y runs up the rod already).
    assert!(near(&s["frame"]["origin"], [0.0, 10.0, 0.0]), "{s}");
    assert!(near(&s["frame"]["normal"], [0.0, 1.0, 0.0]));
    assert!(near(&s["frame"]["x_axis"], [-1.0, 0.0, 0.0]));

    // A dimensioned 6 x 4 rectangle from y = 10 to 14, a text above it and a
    // line once around the rod at y = 5.
    let rectangle = command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [-3, 10],
               "width": 6, "height": 4}),
    );
    assert_eq!(
        rectangle["dimensions"].as_array().unwrap().len(),
        2,
        "{rectangle}"
    );
    let region = rectangle["region"].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F3", "text": "MIT", "at": [-6, 20],
               "height": 5}),
    );
    let around = 10.0 * PI;
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F3", "start": [-around, 5],
               "end": [around, 5]}),
    );
    let ring = line["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|e| e.starts_with('c'))
        .unwrap()
        .to_owned();
    status_ok(&d, "F3");
    let s = query(&d, json!({"query": "sketch", "uid": "F3"}));
    let texts: Vec<String> = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["key"].as_str().unwrap().to_owned())
        .filter(|k| *k != region)
        .collect();
    assert_eq!(texts.len(), 3, "{s}");

    // The curves' lengths on the face: the rectangle's 6 and 4, the ring
    // the rod's circumference.
    let found = lengths(&d, "F3");
    let length = |id: &str| found.iter().find(|(c, _)| c == id).unwrap().1;
    assert!((length(&ring) - 2.0 * around).abs() < 1e-6, "{found:?}");
    let mut sides: Vec<f64> = rectangle["curves"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| length(c.as_str().unwrap()))
        .collect();
    sides.sort_by(f64::total_cmp);
    for (got, want) in sides.iter().zip([4.0, 4.0, 6.0, 6.0]) {
        assert!((got - want).abs() < 1e-6, "{sides:?}");
    }

    // The rectangle raised 1.5 out of the rod (Extrude, F4), wrapped as an
    // emboss on the rod's own cylinder.
    let profiles = json!([{"sketch": "F3", "region": region}]);
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude", "profiles": profiles,
               "extent": {"type": "distance", "distance": 1.5}, "operation": "join"}}),
    );
    status_ok(&d, "F4");
    assert_eq!(
        last_spec(&d),
        "wrap F4 about 0 0 0 along 0 0 1 radius 10 to 11.5"
    );
    // The text sunk 0.5 into it (Emboss, F5, no face of its own).
    let texts: Vec<Value> = texts
        .iter()
        .map(|k| json!({"sketch": "F3", "region": k}))
        .collect();
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "emboss", "profiles": texts,
               "depth": 0.5, "mode": "sink"}}),
    );
    status_ok(&d, "F5");
    assert_eq!(
        last_spec(&d),
        "wrap F5 about 0 0 0 along 0 0 1 radius 10 to 9.5"
    );
    let shape = d.body_shape("F2.b0".parse().unwrap()).unwrap();
    assert!(
        shape.history.contains("wrap(F4:10..11.5)") && shape.history.contains("wrap(F5:10..9.5)"),
        "{}",
        shape.history
    );
    let before = faces(&d).len();
    // The side split by the ring (Split Face, F6): its bands on the face.
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "split_face", "body": "F2.b0",
               "faces": ["F2:side(c1)"], "tool": {"sketch": "F3", "curves": [ring]}}}),
    );
    status_ok(&d, "F6");
    assert_eq!(
        last_spec(&d),
        format!("bands F6 {ring} about 0 0 0 along 0 0 1 radius 10 cone 0 from -0.5 to 0.5")
    );
    let after = faces(&d);
    assert_eq!(after.len(), before + 1, "{after:?}");
    assert!(
        after.iter().any(|f| f == "F2:side(c1)#0") && after.iter().any(|f| f == "F2:side(c1)#1")
    );

    // Points, rays and model points through the sketch_surface query.
    let answer = query(
        &d,
        json!({"query": "sketch_surface", "sketch": "F3",
               "points": [[0, 0], [5.0 * PI, 2]],
               "rays": [{"origin": [0, 50, 12], "direction": [0, -2, 0]},
                        {"origin": [0, 0, 50], "direction": [0, 0, -1]}],
               "near": [[-20, 0, 7], [0, 0, 3]]}),
    );
    assert!(
        near(&answer["points"][0]["at"], [0.0, 10.0, 0.0]),
        "{answer}"
    );
    assert!(near(&answer["points"][0]["normal"], [0.0, 1.0, 0.0]));
    // A quarter turn along x is the -X side.
    assert!(
        near(&answer["points"][1]["at"], [-10.0, 0.0, 2.0]),
        "{answer}"
    );
    assert!(near(&answer["points"][1]["normal"], [-1.0, 0.0, 0.0]));
    let hit = &answer["rays"][0];
    assert_eq!(hit["hit"], true);
    assert!(near2(&hit["point"], [0.0, 12.0], 1e-9), "{hit}");
    assert!((hit["distance"].as_f64().unwrap() - 40.0).abs() < 1e-9);
    assert_eq!(answer["rays"][1]["hit"], false);
    assert!(near2(&answer["near"][0], [5.0 * PI, 7.0], 1e-9), "{answer}");
    assert!(answer["near"][1].is_null());
}

#[test]
fn the_sketch_follows_the_rods_radius_and_length() {
    let mut d = rod();
    command(
        &mut d,
        json!({"cmd": "sketch.create",
               "plane": {"face": "F2:side(c1)", "body": "F2.b0", "developed": {}}}),
    );
    let rectangle = command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [-3, 10],
               "width": 6, "height": 4}),
    );
    let region = rectangle["region"].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F3", "region": region}],
               "extent": {"type": "distance", "distance": 1}, "flip": true,
               "operation": "cut"}}),
    );
    status_ok(&d, "F4");
    assert_eq!(
        last_spec(&d),
        "wrap F4 about 0 0 0 along 0 0 1 radius 10 to 9"
    );
    // Angle 0: the origin on the +X side.
    let s = query(&d, json!({"query": "sketch", "uid": "F3"}));
    assert!(near(&s["frame"]["origin"], [10.0, 0.0, 0.0]), "{s}");
    // The rod made 30 across and 40 long: the sketch lies on the new side,
    // its curves as long around it as before, and the cut follows.
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 30}),
    );
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d2", "value": 40}),
    );
    status_ok(&d, "F3");
    status_ok(&d, "F4");
    let s = query(&d, json!({"query": "sketch", "uid": "F3"}));
    assert_eq!(s["surface"]["radius"], 15.0, "{s}");
    assert!(near(&s["frame"]["origin"], [15.0, 0.0, 0.0]), "{s}");
    assert_eq!(
        last_spec(&d),
        "wrap F4 about 0 0 0 along 0 0 1 radius 15 to 14"
    );
    let mut sides: Vec<f64> = lengths(&d, "F3").iter().map(|(_, l)| *l).collect();
    sides.sort_by(f64::total_cmp);
    for (got, want) in sides.iter().zip([4.0, 4.0, 6.0, 6.0]) {
        assert!((got - want).abs() < 1e-6, "{sides:?}");
    }
}

/// A block (F2.b0) whose side F2:side(…) the mock takes for a cone with its
/// apex at z = 40 opening downwards (half angle 30 degrees), its edges the
/// circles at z = 0 and z = 20.
fn cone() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 60, "height": 40}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    let tan = FRAC_PI_6.tan();
    d.kernel().face_cones.borrow_mut().insert(
        "F2:side(c1[c4,c2])".to_owned(),
        ConeFace {
            apex: [0.0, 0.0, 40.0],
            axis: [0.0, 0.0, -1.0],
            half_angle: FRAC_PI_6,
            internal: false,
        },
    );
    let circle = |z: f64| Curve3::Conic {
        center: [0.0, 0.0, z],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: (40.0 - z) * tan,
        minor: (40.0 - z) * tan,
        start: 0.0,
        end: 2.0 * PI,
        closed: true,
    };
    *d.kernel().curves.borrow_mut() = vec![circle(0.0), circle(20.0)];
    d
}

#[test]
fn a_sketch_on_a_cone_is_its_development() {
    let mut d = cone();
    command(
        &mut d,
        json!({"cmd": "sketch.create",
               "plane": {"face": "F2:side(c1[c4,c2])", "body": "F2.b0", "developed": {}}}),
    );
    status_ok(&d, "F3");
    let s = query(&d, json!({"query": "sketch", "uid": "F3"}));
    let surface = &s["surface"];
    assert_eq!(surface["kind"], "cone", "{s}");
    assert_eq!(surface["half_angle"], json!(FRAC_PI_6));
    assert!(near(&surface["apex"], [0.0, 0.0, 40.0]));
    // The origin at the face's low end along +Z (z = 0, 40 below the apex)
    // on the +X side; y runs up.
    let r0 = 40.0 * FRAC_PI_6.tan();
    assert!(
        (surface["radius"].as_f64().unwrap() - r0).abs() < 1e-9,
        "{s}"
    );
    assert!(near(&s["frame"]["origin"], [r0, 0.0, 0.0]), "{s}");
    assert!(near(&s["frame"]["y_axis"], [0.0, 0.0, 1.0]));
    assert!(near(&s["frame"]["normal"], [1.0, 0.0, 0.0]));

    // A 6 x 4 rectangle raised 1 along the cone's normals.
    let rectangle = command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [-3, 5],
               "width": 6, "height": 4}),
    );
    let region = rectangle["region"].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F3", "region": region}],
               "extent": {"type": "distance", "distance": 1}, "operation": "join"}}),
    );
    status_ok(&d, "F4");
    let spec = last_spec(&d);
    assert!(
        spec.starts_with("wrap F4 about 0 0 40 along -0 -0 -1 radius ")
            || spec.starts_with("wrap F4 about 0 0 40 along 0 0 -1 radius "),
        "{spec}"
    );
    // A development keeps lengths: the rectangle's sides are 6 and 4 on
    // the cone too.
    let mut sides: Vec<f64> = lengths(&d, "F3").iter().map(|(_, l)| *l).collect();
    sides.sort_by(f64::total_cmp);
    for (got, want) in sides.iter().zip([4.0, 4.0, 6.0, 6.0]) {
        assert!((got - want).abs() < 1e-6, "{sides:?}");
    }
    // A point of the sketch goes onto the cone: its distance from the axis
    // is its height below the apex by tan 30.
    let answer = query(
        &d,
        json!({"query": "sketch_surface", "sketch": "F3", "points": [[2, 7]]}),
    );
    let at = &answer["points"][0]["at"];
    let (x, y, z) = (
        at[0].as_f64().unwrap(),
        at[1].as_f64().unwrap(),
        at[2].as_f64().unwrap(),
    );
    assert!(
        (x.hypot(y) - (40.0 - z) * FRAC_PI_6.tan()).abs() < 1e-9,
        "{answer}"
    );
    // Split by a line through the apex's image: a generator.
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F3", "start": [0, 1], "end": [0, 12]}),
    );
    let curves = lengths(&d, "F3");
    let generator = curves
        .iter()
        .find(|(_, l)| (l - 11.0).abs() < 1e-6)
        .unwrap()
        .0
        .clone();
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "split_face", "body": "F2.b0",
               "faces": ["F2:side(c1[c4,c2])"], "tool": {"sketch": "F3", "curves": [generator]}}}),
    );
    status_ok(&d, "F5");
    let spec = last_spec(&d);
    assert!(
        spec.starts_with(&format!("bands F5 {generator} about 0 0 40")),
        "{spec}"
    );
    assert!(spec.contains(&format!("cone {FRAC_PI_6}")), "{spec}");
}

#[test]
fn curved_face_sketches_are_refused_where_they_do_not_fit() {
    let mut d = rod();
    // A face that is neither a cylinder nor a cone fails the sketch.
    let flat = command(
        &mut d,
        json!({"cmd": "sketch.create",
               "plane": {"face": "F2:end(r{c1})", "body": "F2.b0", "developed": {}}}),
    );
    let error = failure(&d, flat["uid"].as_str().unwrap());
    assert!(error.contains("neither cylindrical nor conical"), "{error}");
    // A frame of its own is refused.
    let error = refused(
        &mut d,
        json!({"cmd": "sketch.create",
               "plane": {"face": "F2:side(c1)", "body": "F2.b0", "developed": {}},
               "frame": {"origin": [1, 0, 0], "x_axis": [1, 0, 0], "y_axis": [0, 1, 0]}}),
    );
    assert!(error.contains("no frame or placement"), "{error}");
    // A developed sketch's profiles: Revolve refuses them, Emboss with a
    // face of its own too.
    let created = command(
        &mut d,
        json!({"cmd": "sketch.create",
               "plane": {"face": "F2:side(c1)", "body": "F2.b0", "developed": {}}}),
    );
    let sketch = created["uid"].as_str().unwrap().to_owned();
    let rectangle = command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": sketch, "corner": [-3, 10],
               "width": 6, "height": 4}),
    );
    let region = rectangle["region"].as_str().unwrap().to_owned();
    let revolved = command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "revolve",
               "profiles": [{"sketch": sketch, "region": region}], "axis": "z",
               "extent": {"type": "full"}, "operation": "join"}}),
    );
    let error = failure(&d, revolved["uid"].as_str().unwrap());
    assert!(error.contains("lies on a curved face"), "{error}");
    let embossed = command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "emboss",
               "profiles": [{"sketch": sketch, "region": region}], "depth": 1,
               "face": {"body": "F2.b0", "face": "F2:side(c1)"}}}),
    );
    let error = failure(&d, embossed["uid"].as_str().unwrap());
    assert!(
        error.contains("no face, projection or direction"),
        "{error}"
    );
}

#[test]
fn the_developed_plane_reads_back_and_rejects_a_bad_angle() {
    let plane: mitcad_model::features::SketchPlane = serde_json::from_value(
        json!({"face": "F2:side(c1)", "body": "F2.b0", "developed": {"angle": 0.25}}),
    )
    .unwrap();
    assert!(plane.is_developed());
    assert_eq!(
        serde_json::to_value(&plane).unwrap(),
        json!({"face": "F2:side(c1)", "body": "F2.b0", "developed": {"angle": 0.25}})
    );
    let flat: mitcad_model::features::SketchPlane =
        serde_json::from_value(json!({"face": "F2:end(r{c1})"})).unwrap();
    assert!(!flat.is_developed());
    assert!(
        serde_json::from_value::<mitcad_model::features::SketchPlane>(
            json!({"face": "F2:side(c1)", "developed": {"angle": "x"}})
        )
        .is_err()
    );
}
