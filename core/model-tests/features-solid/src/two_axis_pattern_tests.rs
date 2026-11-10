// SPDX-License-Identifier: MIT
//! Circular Pattern about two axes, and copies that land where material
//! already is left out (mitcad#1588), with the mock kernel.

use std::f64::consts::{PI, TAU};

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::BoundingBox;
use mitcad_model::test_support::{block, def, num};
use mitcad_model::testing::{MockKernel, MockShape};
use mitcad_model::topo::RegionKey;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None).unwrap().uid
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn shape(doc: &Document<MockKernel>, body: &str) -> MockShape {
    let uid: BodyUid = body.parse().unwrap();
    doc.body_shape(uid)
        .unwrap_or_else(|| panic!("no body {body}"))
        .clone()
}

fn uids(doc: &Document<MockKernel>) -> Vec<String> {
    doc.bodies().iter().map(|b| b.uid.to_string()).collect()
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn assert_bounds(doc: &Document<MockKernel>, body: &str, min: [f64; 3], max: [f64; 3]) {
    let b: BoundingBox = shape(doc, body).bounds.expect("bounds");
    let near = |a: [f64; 3], e: [f64; 3]| (0..3).all(|i| (a[i] - e[i]).abs() < 1e-6);
    assert!(
        near(b.min, min) && near(b.max, max),
        "{body}: {:?} .. {:?}, expected {min:?} .. {max:?}",
        b.min,
        b.max
    );
}

fn origin(axis: &str) -> Value {
    json!({"type": "origin", "axis": axis})
}

/// A circular pattern of `objects` about `axis1` (quantity `q1`, angle
/// `a1`) and about `axis2` (`q2`, `a2`).
fn two_axes(objects: Value, first: (&str, f64, f64), second: (&str, f64, f64)) -> Value {
    json!({"type": "circular_pattern", "objects": objects,
           "axis": origin(first.0), "quantity": first.1, "angle": first.2,
           "axis2": {"axis": origin(second.0), "quantity": second.1, "angle": second.2}})
}

fn block_body() -> Value {
    json!({"type": "bodies", "bodies": ["F2.b0"]})
}

#[test]
fn a_second_axis_turns_every_copy_about_it_again() {
    // The block (60 x 40 x 20 at the origin) four times about Z, each of
    // the four half a turn about X again: 8 elements, numbered along Z
    // first (element e = k1 + 4 k2, body F3.b(e-1)).
    let mut b = block();
    let value = two_axes(block_body(), ("z", 4.0, TAU), ("x", 2.0, TAU));
    let preview: Value = serde_json::from_str(
        &b.doc
            .preview(&json!({"cmd": "add_feature", "def": value}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(preview["elements"].as_array().map(Vec::len), Some(8));
    let pattern = add(&mut b.doc, value);
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    assert_eq!(uids(&b.doc).len(), 8, "{:?}", uids(&b.doc));
    // A quarter turn about Z.
    assert_bounds(&b.doc, "F3.b0", [-40.0, 0.0, 0.0], [0.0, 60.0, 20.0]);
    // Element 4: the original half a turn about X.
    assert_bounds(&b.doc, "F3.b3", [0.0, -40.0, -20.0], [60.0, 0.0, 0.0]);
    // Element 5: a quarter turn about Z, then half a turn about X.
    assert_bounds(&b.doc, "F3.b4", [-40.0, -60.0, -20.0], [0.0, 0.0, 0.0]);
    // Element 7: three quarters about Z, then half a turn about X.
    assert_bounds(&b.doc, "F3.b6", [0.0, 0.0, -20.0], [40.0, 60.0, 0.0]);

    // The saved pattern keeps its second axis, its values parameters.
    let file: Value = serde_json::from_str(&b.doc.to_json()).unwrap();
    let saved = file["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["type"] == "circular_pattern")
        .expect("the pattern")
        .clone();
    assert!(!saved["axis2"]["axis"].is_null(), "{saved}");
    let quantity2 = saved["axis2"]["quantity"]
        .as_str()
        .unwrap_or_else(|| panic!("a parameter: {saved}"))
        .to_owned();
    assert!(saved["axis2"]["angle"].is_string(), "{saved}");
    assert!(saved["axis2"].get("symmetric").is_none(), "{saved}");
    // One element about X: the four about Z only.
    b.doc.set_parameter(&quantity2, 1.0).unwrap();
    assert_eq!(uids(&b.doc).len(), 4);
    b.doc.undo();
    assert_eq!(uids(&b.doc).len(), 8);

    // Suppressed elements count the products: element 5 left out.
    let mut suppressed = two_axes(block_body(), ("z", 4.0, TAU), ("x", 2.0, TAU));
    suppressed["suppressed_elements"] = json!([5]);
    b.doc.edit_feature(pattern, &def(suppressed)).unwrap();
    let bodies = uids(&b.doc);
    assert_eq!(bodies.len(), 7, "{bodies:?}");
    assert!(!bodies.contains(&"F3.b4".to_owned()), "{bodies:?}");

    // Symmetric about the second axis: half a turn as three elements, 0,
    // 90 and -90 degrees about X (element 8 the quarter turn back).
    let mut symmetric = two_axes(block_body(), ("z", 4.0, TAU), ("x", 3.0, PI));
    symmetric["axis2"]["symmetric"] = json!(true);
    b.doc.edit_feature(pattern, &def(symmetric)).unwrap();
    assert_eq!(uids(&b.doc).len(), 12);
    // (x, y, z) a quarter turn about X: (x, -z, y).
    assert_bounds(&b.doc, "F3.b3", [0.0, -20.0, 0.0], [60.0, 0.0, 40.0]);
    // A quarter turn back: (x, z, -y).
    assert_bounds(&b.doc, "F3.b7", [0.0, 0.0, -40.0], [60.0, 20.0, 0.0]);

    // A zero angle about the second axis is refused.
    let error = b
        .doc
        .edit_feature(
            pattern,
            &def(two_axes(block_body(), ("z", 4.0, TAU), ("x", 2.0, 0.0))),
        )
        .unwrap_err();
    assert!(error.to_string().contains("must not be zero"), "{error}");
}

#[test]
fn an_old_circular_pattern_without_a_second_axis_loads_unchanged() {
    // mitcad#1588: a pattern without `axis2` saves without it, and the file
    // opens and saves again as it was.
    let mut b = block();
    add(
        &mut b.doc,
        json!({"type": "circular_pattern", "objects": block_body(),
               "axis": origin("z"), "quantity": 4, "angle": TAU, "symmetric": false}),
    );
    let saved = b.doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let pattern = file["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["type"] == "circular_pattern")
        .expect("the pattern");
    assert!(pattern.get("axis2").is_none(), "{pattern}");
    let mut opened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(opened.to_json(), saved);
    opened.recompute();
    assert_eq!(uids(&opened), uids(&b.doc));
    assert_bounds(&opened, "F3.b0", [-40.0, 0.0, 0.0], [0.0, 60.0, 20.0]);
}

/// A peg of its own body: a circle at (100, 0) extruded 10 (Sketch2 F3,
/// Extrude2 F4), with its region.
fn peg(doc: &mut Document<MockKernel>) -> (FeatureUid, RegionKey) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_circle(sketch, [100.0, 0.0], &num(10.0))
        .unwrap()
        .region;
    let peg = add(
        doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}),
    );
    (peg, region)
}

/// Whether some body has a face of this name.
fn has_face(doc: &Document<MockKernel>, name: &str) -> bool {
    let face = name.parse().unwrap();
    doc.bodies().iter().any(|body| {
        doc.body_shape(body.uid)
            .is_some_and(|s| s.faces.contains(&face))
    })
}

#[test]
fn copies_that_land_on_the_original_or_on_each_other_are_left_out() {
    // mitcad#1588: the peg four times about Z, and those twice more about
    // Z, half a turn apart: elements 4 to 7 land where elements 2, 3, the
    // original and 1 are. Where the kernel can tell (the mock measures its
    // shapes' bounds), they are left out: three copies, three new bodies.
    let mut b = block();
    b.doc.kernel().measures.set(true);
    b.doc.kernel().copies_by_bounds.set(true);
    let (peg, region) = peg(&mut b.doc);
    let objects = json!({"type": "features", "features": [peg]});
    for compute in ["adjust", "identical"] {
        let mut value = two_axes(objects.clone(), ("z", 4.0, TAU), ("z", 2.0, PI));
        value["compute"] = json!(compute);
        let pattern = add(&mut b.doc, value);
        assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok, "{compute}");
        assert_eq!(uids(&b.doc).len(), 5, "{compute}: {:?}", uids(&b.doc));
        for e in 1..4 {
            let name = format!("{pattern}:inst{e}({peg}:end({region}))");
            assert!(has_face(&b.doc, &name), "{compute}: {name}");
        }
        for e in 4..8 {
            let name = format!("{pattern}:inst{e}({peg}:end({region}))");
            assert!(!has_face(&b.doc, &name), "{compute}: {name}");
        }
        b.doc.undo();
    }
    // A kernel that cannot tell makes every copy, as before (a document of
    // its own: the first one keeps its results for the same definitions).
    let mut b = block();
    b.doc.kernel().measures.set(true);
    let (peg2, _) = self::peg(&mut b.doc);
    let objects = json!({"type": "features", "features": [peg2]});
    let pattern = add(
        &mut b.doc,
        two_axes(objects, ("z", 4.0, TAU), ("z", 2.0, PI)),
    );
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    assert_eq!(uids(&b.doc).len(), 9, "{:?}", uids(&b.doc));
}

#[test]
fn joined_copies_of_bodies_that_add_nothing_are_left_out() {
    // mitcad#1588: the block joined with its copies (`combine`) four times
    // about Z and twice more half a turn apart: elements 4 to 7 add
    // nothing and are not joined; 1 to 3 are.
    let mut b = block();
    b.doc.kernel().measures.set(true);
    b.doc.kernel().copies_by_bounds.set(true);
    let mut value = two_axes(block_body(), ("z", 4.0, TAU), ("z", 2.0, PI));
    value["combine"] = json!(true);
    let pattern = add(&mut b.doc, value);
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    assert_eq!(uids(&b.doc), ["F2.b0"]);
    let history = shape(&b.doc, "F2.b0").history;
    for e in 1..4 {
        assert!(history.contains(&format!(",inst{e})")), "{e}: {history}");
    }
    for e in 4..8 {
        assert!(!history.contains(&format!(",inst{e})")), "{e}: {history}");
    }
}

/// The keys of the sketch's regions that have a piece of `curve` in their
/// boundary.
fn regions_along(doc: &Document<MockKernel>, sketch: &str, curve: &str) -> Vec<String> {
    let found = query(doc, json!({"query": "sketch", "uid": sketch}));
    found["regions"]
        .as_array()
        .unwrap_or_else(|| panic!("{found}"))
        .iter()
        .map(|r| match &r["key"] {
            Value::String(key) => key.clone(),
            other => other.to_string(),
        })
        .filter(|key| key.contains(&format!("{{{curve}[")) || key.contains(&format!(",{curve}[")))
        .collect()
}

/// The first curve a sketch command made.
fn made(result: &Value) -> String {
    result["made"][0]
        .as_str()
        .unwrap_or_else(|| panic!("{result}"))
        .to_owned()
}

#[test]
fn a_cube_lattice_is_one_revolve_one_pipe_and_one_two_axis_pattern() {
    // mitcad#1588: a cubic lattice of 40 mm through the API. On a plane
    // through Z at 45 degrees from XZ, a sketch with the body diagonal
    // (the revolve's axis), a ball at each of its ends and one at the
    // middle, a rod line beside the axis and a construction line along a
    // vertical edge of the cube; Revolve1 turns every region beside the
    // rod line about the diagonal (two corner balls, the centre ball and
    // the rod between them), Pipe1 runs along the edge; one circular
    // pattern of both, about X (3 over 180 degrees, symmetric) and about
    // Z (4): 12 elements, the copies of the diagonal (which land on one
    // another) and of the edge.
    let mut doc = Document::new(MockKernel::default());
    let plane = add(
        &mut doc,
        json!({"type": "construction_plane",
               "definition": {"type": "angle", "line": "z", "angle": "45 deg", "plane": "xz"}}),
    );
    let created = command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": plane.to_string()}),
    );
    let sketch = created["uid"].as_str().expect("the sketch").to_owned();
    // The diagonal from corner to corner (28.28, 20) through the origin,
    // on past the balls.
    let axis = made(&command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": sketch,
               "start": [36.45, -25.774], "end": [-36.45, 25.774]}),
    ));
    for (center, diameter) in [
        ([-28.284, 20.0], 12.0),
        ([28.284, -20.0], 12.0),
        ([0.0, 0.0], 16.0),
    ] {
        command(
            &mut doc,
            json!({"cmd": "sketch.add_circle", "sketch": sketch,
                   "center": center, "diameter": diameter}),
        );
    }
    // The rod's side 1.5 mm off the diagonal, through all three balls.
    let rod = made(&command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": sketch,
               "start": [34.832, -22.793], "end": [-33.1, 25.243]}),
    ));
    let edge = made(&command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": sketch, "construction": true,
               "start": [-28.284, -20.0], "end": [-28.284, 20.0]}),
    ));
    let regions = regions_along(&doc, &sketch, &rod);
    assert!(regions.len() >= 5, "{regions:?}");
    let profiles: Vec<Value> = regions
        .iter()
        .map(|key| json!({"sketch": sketch, "region": key}))
        .collect();
    let revolve = add(
        &mut doc,
        json!({"type": "revolve", "profiles": profiles,
               "axis": {"sketch": sketch, "curve": axis},
               "extent": {"type": "full"}, "operation": "new_body"}),
    );
    assert_eq!(status(&doc, revolve), FeatureStatus::Ok);
    let pipe = add(
        &mut doc,
        json!({"type": "pipe", "path": {"sketch": sketch, "curves": [edge]},
               "size": 3, "operation": "join"}),
    );
    assert_eq!(status(&doc, pipe), FeatureStatus::Ok);
    let mut value = two_axes(
        json!({"type": "features", "features": [revolve, pipe]}),
        ("x", 3.0, PI),
        ("z", 4.0, TAU),
    );
    value["symmetric"] = json!(true);
    value["compute"] = json!("identical");
    let preview: Value = serde_json::from_str(
        &doc.preview(&json!({"cmd": "add_feature", "def": value}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(preview["elements"].as_array().map(Vec::len), Some(12));
    let pattern = add(&mut doc, value);
    assert_eq!(status(&doc, pattern), FeatureStatus::Ok);
    // Every copy of the edge: the cube's twelve edges.
    for e in 1..12 {
        let name = format!("{pattern}:inst{e}({pipe}:side0)");
        assert!(has_face(&doc, &name), "{name}");
    }
    // The design saves and opens again as it was.
    let saved = doc.to_json();
    let mut opened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(opened.to_json(), saved);
    opened.recompute();
    assert_eq!(status(&opened, pattern), FeatureStatus::Ok);
}
