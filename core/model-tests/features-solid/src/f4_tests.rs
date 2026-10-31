// SPDX-License-Identifier: MIT
//! Patterns, mirrors, combine, moves, alignments, scales and primitives in
//! documents, with the mock kernel.

use serde_json::{Value, json};

use mitcad_model::datum::{CurveGeometry, SurfaceGeometry};
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::BoundingBox;
use mitcad_model::test_support::{block, def, num};
use mitcad_model::testing::{MockKernel, MockShape};
use mitcad_model::topo::{EdgeName, FaceName, RegionKey};
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None).unwrap().uid
}

fn shape(doc: &Document<MockKernel>, body: &str) -> MockShape {
    let uid: BodyUid = body.parse().unwrap();
    doc.body_shape(uid)
        .unwrap_or_else(|| panic!("no body {body}"))
        .clone()
}

fn bounds(doc: &Document<MockKernel>, body: &str) -> BoundingBox {
    shape(doc, body).bounds.expect("bounds")
}

fn assert_bounds(doc: &Document<MockKernel>, body: &str, min: [f64; 3], max: [f64; 3]) {
    let b = bounds(doc, body);
    let near = |a: [f64; 3], e: [f64; 3]| (0..3).all(|i| (a[i] - e[i]).abs() < 1e-9);
    assert!(
        near(b.min, min) && near(b.max, max),
        "{body}: {:?} .. {:?}, expected {min:?} .. {max:?}",
        b.min,
        b.max
    );
}

fn uids(doc: &Document<MockKernel>) -> Vec<String> {
    doc.bodies().iter().map(|b| b.uid.to_string()).collect()
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn x_pattern(quantity: f64, distance: f64, extra: Value) -> Value {
    let mut value = json!({"type": "rectangular_pattern",
        "objects": {"type": "bodies", "bodies": ["F2.b0"]},
        "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": quantity,
                       "distance": distance},
        "distance_type": "spacing"});
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    value
}

/// The block's region and a hole: Sketch2 (F3) with a 5 mm circle c1 at
/// (10, 10) cut through all by Extrude2 (F4).
fn hole(doc: &mut Document<MockKernel>) -> (FeatureUid, RegionKey) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_circle(sketch, [10.0, 10.0], &num(5.0))
        .unwrap()
        .region;
    let cut = add(
        doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "through_all"}, "operation": "cut"}),
    );
    (cut, region)
}

#[test]
fn rectangular_patterns_of_bodies_make_new_bodies() {
    let mut b = block();
    let added = b
        .doc
        .add_feature(&def(x_pattern(3.0, 100.0, json!({}))), None)
        .unwrap();
    assert_eq!(added.name, "RectangularPattern1");
    assert_eq!(added.parameters, vec!["d4", "d5"]);
    let pattern = added.uid;
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0", "F3.b1"]);
    assert_bounds(&b.doc, "F3.b0", [100.0, 0.0, 0.0], [160.0, 40.0, 20.0]);
    assert_bounds(&b.doc, "F3.b1", [200.0, 0.0, 0.0], [260.0, 40.0, 20.0]);
    let names: Vec<String> = b.doc.bodies().iter().map(|b| b.name.clone()).collect();
    assert_eq!(names, ["Body1", "Body2", "Body3"]);
    // Copied faces carry the element and the original name.
    let end = FaceName::end(b.extrude, b.region.clone());
    let copy = shape(&b.doc, "F3.b1");
    assert!(
        copy.faces.contains(
            &"F3:inst2(F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}))"
                .parse()
                .unwrap()
        )
    );
    assert!(!copy.faces.contains(&end));

    // Extent: the distance spans all elements.
    let mut extent = x_pattern(3.0, 100.0, json!({"distance_type": "extent"}));
    extent["direction1"]["distance"] = json!("d5");
    extent["direction1"]["quantity"] = json!("d4");
    b.doc.edit_feature(pattern, &def(extent)).unwrap();
    assert_bounds(&b.doc, "F3.b0", [50.0, 0.0, 0.0], [110.0, 40.0, 20.0]);
    assert_bounds(&b.doc, "F3.b1", [100.0, 0.0, 0.0], [160.0, 40.0, 20.0]);

    // Symmetric: the quantity's elements on both sides of the original,
    // element 2 the first backwards (three elements: 0, 1, -1).
    let mut symmetric = x_pattern(3.0, 100.0, json!({}));
    symmetric["direction1"]["symmetric"] = json!(true);
    b.doc.edit_feature(pattern, &def(symmetric)).unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0", "F3.b1"]);
    assert_bounds(&b.doc, "F3.b0", [100.0, 0.0, 0.0], [160.0, 40.0, 20.0]);
    assert_bounds(&b.doc, "F3.b1", [-100.0, 0.0, 0.0], [-40.0, 40.0, 20.0]);
    // An even quantity has the extra element before the original (four
    // elements: 0, 1, -1, -2).
    let mut symmetric = x_pattern(4.0, 100.0, json!({}));
    symmetric["direction1"]["symmetric"] = json!(true);
    b.doc.edit_feature(pattern, &def(symmetric)).unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0", "F3.b1", "F3.b2"]);
    assert_bounds(&b.doc, "F3.b2", [-200.0, 0.0, 0.0], [-140.0, 40.0, 20.0]);

    // A suppressed element leaves out its body; the others keep their ids.
    b.doc
        .edit_feature(
            pattern,
            &def(x_pattern(3.0, 100.0, json!({"suppressed_elements": [1]}))),
        )
        .unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b1"]);
    assert_bounds(&b.doc, "F3.b1", [200.0, 0.0, 0.0], [260.0, 40.0, 20.0]);
    let error = b
        .doc
        .edit_feature(
            pattern,
            &def(x_pattern(3.0, 100.0, json!({"suppressed_elements": [0]}))),
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("element 0 is the original"),
        "{error}"
    );
    let error = b
        .doc
        .edit_feature(pattern, &def(x_pattern(2.5, 100.0, json!({}))))
        .unwrap_err();
    assert!(error.to_string().contains("whole number"), "{error}");
    let error = b
        .doc
        .edit_feature(pattern, &def(x_pattern(3.0, 0.0, json!({}))))
        .unwrap_err();
    assert!(error.to_string().contains("must not be zero"), "{error}");
}

#[test]
fn a_negative_pattern_distance_goes_against_the_axis() {
    // mitcad#447: the distance is signed; the block is 0..60 along x.
    let mut b = block();
    let pattern = add(&mut b.doc, x_pattern(3.0, -20.0, json!({})));
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    assert_bounds(&b.doc, "F3.b0", [-20.0, 0.0, 0.0], [40.0, 40.0, 20.0]);
    assert_bounds(&b.doc, "F3.b1", [-40.0, 0.0, 0.0], [20.0, 40.0, 20.0]);
    // A negative extent spans the elements backwards.
    b.doc
        .edit_feature(
            pattern,
            &def(x_pattern(3.0, -40.0, json!({"distance_type": "extent"}))),
        )
        .unwrap();
    assert_bounds(&b.doc, "F3.b1", [-40.0, 0.0, 0.0], [20.0, 40.0, 20.0]);
    // Symmetric: the copies of the positive distance (element 1 the other
    // side).
    let mut symmetric = x_pattern(3.0, -20.0, json!({}));
    symmetric["direction1"]["symmetric"] = json!(true);
    b.doc.edit_feature(pattern, &def(symmetric)).unwrap();
    assert_bounds(&b.doc, "F3.b0", [-20.0, 0.0, 0.0], [40.0, 40.0, 20.0]);
    assert_bounds(&b.doc, "F3.b1", [20.0, 0.0, 0.0], [80.0, 40.0, 20.0]);
    // A second direction turns as well.
    let mut two = x_pattern(2.0, -20.0, json!({}));
    two["direction2"] =
        json!({"axis": {"type": "origin", "axis": "y"}, "quantity": 2, "distance": -50});
    b.doc.edit_feature(pattern, &def(two)).unwrap();
    assert_bounds(&b.doc, "F3.b2", [-20.0, -50.0, 0.0], [40.0, -10.0, 20.0]);
    // Zero is refused, in either direction.
    let error = b
        .doc
        .edit_feature(pattern, &def(x_pattern(3.0, 0.0, json!({}))))
        .unwrap_err();
    assert!(error.to_string().contains("must not be zero"), "{error}");
    let mut zero = x_pattern(2.0, -20.0, json!({}));
    zero["direction2"] =
        json!({"axis": {"type": "origin", "axis": "y"}, "quantity": 2, "distance": 0});
    let error = b.doc.edit_feature(pattern, &def(zero)).unwrap_err();
    assert!(error.to_string().contains("must not be zero"), "{error}");
}

#[test]
fn patterns_scale_their_copies_about_the_carried_centre() {
    // A pattern's `scale` (mitcad#59): element e of n scaled by
    // 1 + (scale - 1) e / (n - 1) about the original's centre carried to
    // the element. The block is 60 x 40 x 20 at the origin.
    let mut b = block();
    b.doc.kernel().measures.set(true);
    let uid = add(&mut b.doc, x_pattern(3.0, 40.0, json!({"scale": 2})));
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let copy1 = BodyUid::new(uid, 0).to_string();
    let copy2 = BodyUid::new(uid, 1).to_string();
    // 1.5 times about (70, 20, 10), twice about (110, 20, 10).
    assert_bounds(&b.doc, &copy1, [25.0, -10.0, -5.0], [115.0, 50.0, 25.0]);
    assert_bounds(&b.doc, &copy2, [50.0, -20.0, -10.0], [170.0, 60.0, 30.0]);
    let answer: Value = serde_json::from_str(
        &b.doc
            .query(&json!({"query": "feature", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert!(answer["def"].get("scale").is_some());
    // The factor is a parameter, checked like the others.
    let message = b
        .doc
        .edit_feature(uid, &def(x_pattern(3.0, 40.0, json!({"scale": 0}))))
        .unwrap_err()
        .to_string();
    assert!(message.contains("must be a positive factor"), "{message}");
    // A circular pattern scales its copies too.
    let turn = add(
        &mut b.doc,
        json!({"type": "circular_pattern", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "axis": {"type": "origin", "axis": "z"}, "quantity": 2,
               "angle": std::f64::consts::TAU, "scale": 0.5}),
    );
    assert_eq!(status(&b.doc, turn), FeatureStatus::Ok);
    // Half size about (-30, -20, 10), the centre turned half round.
    assert_bounds(
        &b.doc,
        &BodyUid::new(turn, 0).to_string(),
        [-45.0, -30.0, 5.0],
        [-15.0, -10.0, 15.0],
    );
}

#[test]
fn two_directions_number_the_elements_direction_one_first() {
    let mut b = block();
    let mut value = x_pattern(3.0, 100.0, json!({"suppressed_elements": [4]}));
    value["direction2"] =
        json!({"axis": {"type": "origin", "axis": "y"}, "quantity": 2, "distance": 50});
    add(&mut b.doc, value);
    // Elements 1..5 are bodies b0..b4; element 4 is (1, 1).
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0", "F3.b1", "F3.b2", "F3.b4"]);
    assert_bounds(&b.doc, "F3.b2", [0.0, 50.0, 0.0], [60.0, 90.0, 20.0]);
    assert_bounds(&b.doc, "F3.b4", [200.0, 50.0, 0.0], [260.0, 90.0, 20.0]);
}

#[test]
fn a_circle_of_a_sketch_is_an_axis_through_its_centre() {
    let mut b = block();
    // A circle at (100, 0) of a sketch on XY: the axis through its centre
    // normal to the sketch turns the body about (100, 0).
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let circle = b
        .doc
        .add_circle(sketch, [100.0, 0.0], &num(10.0))
        .unwrap()
        .curves[0];
    let axis = json!({"sketch": sketch.to_string(), "curve": circle.curve_name()});
    let pattern = add(
        &mut b.doc,
        json!({"type": "circular_pattern",
               "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "axis": axis, "quantity": 2, "angle": std::f64::consts::TAU}),
    );
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    // The body (0..60, 0..40) half a turn about (100, 0): (140..200, -40..0).
    assert_bounds(&b.doc, "F4.b0", [140.0, -40.0, 0.0], [200.0, 0.0, 20.0]);
}

#[test]
fn circular_patterns_turn_about_the_axis() {
    let mut b = block();
    let circular = |angle: f64, quantity: f64, symmetric: bool| {
        json!({"type": "circular_pattern",
               "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "axis": {"type": "origin", "axis": "z"}, "quantity": quantity, "angle": angle,
               "symmetric": symmetric})
    };
    let pattern = add(&mut b.doc, circular(std::f64::consts::TAU, 4.0, false));
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0", "F3.b1", "F3.b2"]);
    // A full turn spaces four elements by 90 degrees.
    assert_bounds(&b.doc, "F3.b0", [-40.0, 0.0, 0.0], [0.0, 60.0, 20.0]);
    assert_bounds(&b.doc, "F3.b1", [-60.0, -40.0, 0.0], [0.0, 0.0, 20.0]);
    // A partial angle spans the elements: 90 / (4 - 1) = 30 degrees.
    b.doc
        .edit_feature(
            pattern,
            &def(circular(std::f64::consts::FRAC_PI_2, 4.0, false)),
        )
        .unwrap();
    assert_bounds(&b.doc, "F3.b2", [-40.0, 0.0, 0.0], [0.0, 60.0, 20.0]);
    // Symmetric: the elements on both sides of the original at the same
    // spacing (180 / (3 - 1) = 90 degrees: 0, 90 and -90); a negative angle
    // turns back.
    b.doc
        .edit_feature(pattern, &def(circular(std::f64::consts::PI, 3.0, true)))
        .unwrap();
    assert_bounds(&b.doc, "F3.b1", [0.0, -60.0, 0.0], [40.0, 0.0, 20.0]);
    // Four elements over 90 degrees: -60, -30, 0 and 30.
    b.doc
        .edit_feature(
            pattern,
            &def(circular(std::f64::consts::FRAC_PI_2, 4.0, true)),
        )
        .unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0", "F3.b1", "F3.b2"]);
    b.doc
        .edit_feature(
            pattern,
            &def(circular(-std::f64::consts::FRAC_PI_2, 2.0, false)),
        )
        .unwrap();
    assert_bounds(&b.doc, "F3.b0", [0.0, -60.0, 0.0], [40.0, 0.0, 20.0]);
}

#[test]
fn patterns_of_features_repeat_the_tool_and_follow_edits() {
    let mut b = block();
    let (cut, _) = hole(&mut b.doc);
    let cuts = b.doc.kernel().count("cut");
    let feature_pattern = |compute: &str, quantity: Value| {
        json!({"type": "rectangular_pattern",
               "objects": {"type": "features", "features": [cut]},
               "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": quantity,
                              "distance": 20},
               "distance_type": "spacing", "compute": compute})
    };
    let unions = b.doc.kernel().count("unite");
    let pattern = add(&mut b.doc, feature_pattern("adjust", json!(3)));
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    // Adjust rebuilds the tool at each copy and cuts with them together
    // (no body splits, so the bodies are those of one copy after another).
    assert_eq!(b.doc.kernel().count("cut"), cuts + 1);
    assert_eq!(b.doc.kernel().count("unite"), unions + 1);
    assert_eq!(uids(&b.doc), ["F2.b0"]);
    let body = shape(&b.doc, "F2.b0");
    assert!(body.history.contains("inst2"), "{}", body.history);
    let wall: FaceName = "F5:inst2(F4:side(c1))".parse().unwrap();
    assert!(body.faces.contains(&wall));

    // A fillet on the rim of the third hole.
    let rim = EdgeName::new(wall.clone(), "F5:inst2(F4:end(r{c1}))".parse().unwrap());
    let fillet = add(
        &mut b.doc,
        json!({"type": "fillet", "body": "F2.b0", "edges": [rim], "radius": 1}),
    );
    assert_eq!(status(&b.doc, fillet), FeatureStatus::Ok);
    // More elements keep the names of the others (d5 is the quantity).
    b.doc.set_parameter("d5", 4.0).unwrap();
    assert_eq!(status(&b.doc, fillet), FeatureStatus::Ok);
    b.doc.set_parameter("d5", 2.0).unwrap();
    assert!(matches!(status(&b.doc, fillet), FeatureStatus::Failed(m) if m.contains("F5:inst2")));
    b.doc.undo();
    b.doc.undo();
    assert_eq!(status(&b.doc, fillet), FeatureStatus::Ok);

    // Changing the patterned feature (the hole's diameter d4) recomputes
    // the pattern; undo takes the cached results.
    b.doc.set_parameter("d4", 6.0).unwrap();
    assert!(b.doc.stats().evaluated.contains(&pattern));
    b.doc.undo();
    assert!(b.doc.stats().evaluated.is_empty());

    // Identical moves the stored tool to every element: one boolean.
    let (cuts, unions) = (b.doc.kernel().count("cut"), b.doc.kernel().count("unite"));
    b.doc
        .edit_feature(pattern, &def(feature_pattern("identical", json!("d5"))))
        .unwrap();
    assert_eq!(b.doc.kernel().count("cut"), cuts + 1);
    assert_eq!(b.doc.kernel().count("unite"), unions + 1);
    assert_eq!(status(&b.doc, fillet), FeatureStatus::Ok);

    // A suppressed patterned feature fails the pattern.
    b.doc.set_suppressed(cut, true).unwrap();
    assert!(
        matches!(status(&b.doc, pattern), FeatureStatus::Failed(m) if m.contains("suppressed"))
    );
}

#[test]
fn adjust_rebuilds_an_extrusion_up_to_an_object_at_each_copy() {
    let mut b = block();
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_circle(sketch, [10.0, 10.0], &num(5.0))
        .unwrap()
        .region;
    // Up to a plane that the pattern does not move.
    let boss = add(
        &mut b.doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "to_object", "object": {"type": "plane",
                   "plane": {"origin": [0.0, 0.0, 25.0], "normal": [0.0, 0.0, 1.0]}}},
               "operation": "join", "participants": ["F2.b0"]}),
    );
    let specs = |doc: &Document<MockKernel>| -> Vec<String> {
        doc.kernel()
            .specs
            .borrow()
            .iter()
            .filter(|s| s.starts_with(&format!("extrude {boss} ")))
            .cloned()
            .collect()
    };
    assert_eq!(specs(&b.doc).len(), 1);
    let pattern = |compute: &str| {
        json!({"type": "rectangular_pattern",
               "objects": {"type": "features", "features": [boss]},
               "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": 3,
                              "distance": 20},
               "distance_type": "spacing", "compute": compute})
    };
    let adjust = add(&mut b.doc, pattern("adjust"));
    assert_eq!(status(&b.doc, adjust), FeatureStatus::Ok);
    // The tool is built again for each of the two copies, up to the same
    // plane.
    let built = specs(&b.doc);
    assert_eq!(built.len(), 3, "{built:?}");
    assert!(
        built[1..]
            .iter()
            .all(|s| s.contains("to plane [0.0, 0.0, 25.0] [0.0, 0.0, 1.0]")),
        "{built:?}"
    );
    // Identical moves the finished tool: nothing is built again.
    b.doc
        .edit_feature(adjust, &def(pattern("identical")))
        .unwrap();
    assert_eq!(specs(&b.doc).len(), 3);
}

/// An extrusion that joins a body up to a face of that body takes the face
/// away; Adjust builds each copy up to the face as the extrusion found it.
#[test]
fn adjust_finds_a_face_the_original_took_away() {
    let mut b = block();
    let top: FaceName = format!("F2:end({})", b.region).parse().unwrap();
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_circle(sketch, [10.0, 10.0], &num(5.0))
        .unwrap()
        .region;
    *b.doc.kernel().lose.borrow_mut() = vec![top.clone()];
    let boss = add(
        &mut b.doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "to_object", "object": {"type": "face", "body": "F2.b0",
                          "face": top.to_string()}},
               "operation": "join", "participants": ["F2.b0"]}),
    );
    assert_eq!(status(&b.doc, boss), FeatureStatus::Ok);
    assert!(!shape(&b.doc, "F2.b0").faces.contains(&top));
    let pattern = add(
        &mut b.doc,
        json!({"type": "rectangular_pattern",
               "objects": {"type": "features", "features": [boss]},
               "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": 3,
                              "distance": 20},
               "distance_type": "spacing", "compute": "adjust"}),
    );
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    let built: Vec<String> = b
        .doc
        .kernel()
        .specs
        .borrow()
        .iter()
        .filter(|s| s.starts_with(&format!("extrude {boss} ")))
        .cloned()
        .collect();
    assert_eq!(built.len(), 3, "{built:?}");
    assert!(
        built.iter().all(|s| s.contains(&format!("face {top}"))),
        "{built:?}"
    );
}

#[test]
fn cuts_leave_out_copies_that_stay_away_from_the_bodies() {
    let mut b = block();
    let (cut, _) = hole(&mut b.doc);
    // Holes 20 mm apart from x = 10: the block (x 0..60) reaches the first
    // three; the copies at x = 70 and 90 are left out of the union.
    let pattern = add(
        &mut b.doc,
        json!({"type": "rectangular_pattern",
               "objects": {"type": "features", "features": [cut]},
               "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": 5,
                              "distance": 20},
               "distance_type": "spacing"}),
    );
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    let history = shape(&b.doc, "F2.b0").history;
    assert!(
        history.contains("inst1") && history.contains("inst2"),
        "{history}"
    );
    assert!(
        !history.contains("inst3") && !history.contains("inst4"),
        "{history}"
    );
    // So are they by Identical.
    let identical = json!({"type": "rectangular_pattern",
        "objects": {"type": "features", "features": [cut]},
        "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": 5, "distance": 20},
        "distance_type": "spacing", "compute": "identical"});
    b.doc.edit_feature(pattern, &def(identical)).unwrap();
    let history = shape(&b.doc, "F2.b0").history;
    assert!(
        history.contains("inst2") && !history.contains("inst3"),
        "{history}"
    );
}

#[test]
fn copies_of_features_can_keep_to_the_bodies_the_feature_changed() {
    let mut b = block();
    let (cut, _) = hole(&mut b.doc);
    // A second block at x 75..95, made after the hole: the copies at x = 70
    // and 90 reach it.
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_rectangle(sketch, [75.0, 0.0], &num(20.0), &num(40.0))
        .unwrap()
        .region;
    add(
        &mut b.doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}),
    );
    let other = uids(&b.doc)[1].clone();
    let pattern = |original_bodies: bool| {
        json!({"type": "rectangular_pattern",
               "objects": {"type": "features", "features": [cut]},
               "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": 5,
                              "distance": 20},
               "distance_type": "spacing", "original_bodies": original_bodies})
    };
    // The hole has no participants: its copies cut every body they reach.
    let uid = add(&mut b.doc, pattern(false));
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let history = shape(&b.doc, &other).history;
    assert!(history.contains("inst4"), "{history}");
    // Kept to the bodies the hole changed, they leave the second block alone.
    b.doc.edit_feature(uid, &def(pattern(true))).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let history = shape(&b.doc, &other).history;
    assert!(!history.contains("inst"), "{history}");
    let history = shape(&b.doc, "F2.b0").history;
    assert!(
        history.contains("inst1") && history.contains("inst2"),
        "{history}"
    );
    // The definition keeps the setting.
    let saved: Value = serde_json::from_str(
        &b.doc
            .query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(saved["def"]["original_bodies"], json!(true));
}

#[test]
fn patterns_of_new_body_features_make_bodies() {
    let mut b = block();
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_circle(sketch, [100.0, 0.0], &num(10.0))
        .unwrap()
        .region;
    let peg = add(
        &mut b.doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}),
    );
    for compute in ["adjust", "identical"] {
        let pattern = add(
            &mut b.doc,
            json!({"type": "circular_pattern", "objects": {"type": "features", "features": [peg]},
                   "axis": {"type": "origin", "axis": "z"}, "quantity": 3,
                   "angle": std::f64::consts::TAU, "compute": compute}),
        );
        assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
        let names: Vec<String> = b.doc.bodies().iter().map(|b| b.name.clone()).collect();
        assert_eq!(names, ["Body1", "Body2", "Body3", "Body4"], "{compute}");
        assert!(
            shape(&b.doc, &format!("{pattern}.b0")).faces.contains(
                &format!("{pattern}:inst1({peg}:end({region}))")
                    .parse()
                    .unwrap()
            )
        );
        b.doc.undo();
    }
}

#[test]
fn patterns_of_patterns_repeat_the_originals_at_every_product() {
    let mut b = block();
    let (cut, _) = hole(&mut b.doc);
    let row = add(
        &mut b.doc,
        json!({"type": "rectangular_pattern", "objects": {"type": "features", "features": [cut]},
               "direction1": {"axis": "x", "quantity": 3, "distance": 20},
               "distance_type": "spacing", "compute": "identical", "suppressed_elements": [1]}),
    );
    let faces = |doc: &Document<MockKernel>| -> Vec<String> {
        shape(doc, "F2.b0")
            .faces
            .iter()
            .map(ToString::to_string)
            .filter(|f| f.contains("side(c1)"))
            .collect()
    };
    for compute in ["identical", "adjust"] {
        let cuts = b.doc.kernel().count("cut");
        // About the block's middle, so that the turned holes stay in it
        // (copies that stay away from the bodies are left out).
        let turn = add(
            &mut b.doc,
            json!({"type": "circular_pattern", "objects": {"type": "features", "features": [row]},
                   "axis": {"origin": [30, 20, 0], "direction": [0, 0, 1]}, "quantity": 2,
                   "angle": std::f64::consts::TAU, "compute": compute}),
        );
        assert_eq!(status(&b.doc, turn), FeatureStatus::Ok, "{compute}");
        // Identical: the hole's copies at the row's two places (the
        // original's and element 2's) are cut in one operation (mitcad#181);
        // Adjust too, as the two places' copies stay apart (mitcad#855).
        assert_eq!(b.doc.kernel().count("cut") - cuts, 1, "{compute}");
        // The hole, the row's element 2 (1 is suppressed), and both turned:
        // the inner instance named inside the outer one.
        let mut names = faces(&b.doc);
        names.sort();
        assert_eq!(
            names,
            [
                "F4:side(c1)",
                "F5:inst2(F4:side(c1))",
                "F6:inst1(F4:side(c1))",
                "F6:inst1(F5:inst2(F4:side(c1)))",
            ],
            "{compute}"
        );
        b.doc.undo();
    }
    // A mirror of the row (in the block's middle), and a pattern of that
    // mirror: reflections are moved as finished tools.
    let mirror = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "features", "features": [row]},
               "plane": {"origin": [30, 0, 0], "normal": [1, 0, 0]}}),
    );
    let shifted = add(
        &mut b.doc,
        json!({"type": "rectangular_pattern", "objects": {"type": "features", "features": [mirror]},
               "direction1": {"axis": "y", "quantity": 2, "distance": 5},
               "distance_type": "spacing"}),
    );
    assert_eq!(status(&b.doc, shifted), FeatureStatus::Ok);
    let names = faces(&b.doc);
    for name in [
        "F7:inst1(F6:inst1(F5:inst2(F4:side(c1))))",
        "F7:inst1(F6:inst1(F4:side(c1)))",
        "F7:inst1(F5:inst2(F4:side(c1)))",
        "F7:inst1(F4:side(c1))",
    ] {
        assert!(names.iter().any(|n| n == name), "{name}: {names:?}");
    }
    // The inner pattern's change recomputes the outer one.
    b.doc.set_parameter("d6", 30.0).unwrap();
    assert!(b.doc.stats().evaluated.contains(&shifted));
    b.doc.undo();
    // A pattern of a pattern of bodies (mitcad#547) copies the block as it
    // was before the inner pattern at every product of the elements: the
    // block and its copy at x = 100 mirrored to new bodies.
    let block_box = bounds(&b.doc, "F2.b0");
    let bodies = add(&mut b.doc, x_pattern(2.0, 100.0, json!({})));
    let mirror = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "features", "features": [bodies]},
               "plane": "yz"}),
    );
    assert_eq!(status(&b.doc, mirror), FeatureStatus::Ok);
    for shift in [0.0, 100.0] {
        let min = [
            -(block_box.max[0] + shift),
            block_box.min[1],
            block_box.min[2],
        ];
        let max = [
            -(block_box.min[0] + shift),
            block_box.max[1],
            block_box.max[2],
        ];
        assert!(has_box(&b.doc, min, max), "{shift}: {min:?} .. {max:?}");
    }
}

#[test]
fn features_without_a_tool_cannot_be_patterned() {
    let mut b = block();
    let edge = mitcad_model::test_support::corner(b.extrude, 1, 0);
    let fillet = add(
        &mut b.doc,
        json!({"type": "fillet", "body": "F2.b0", "edges": [edge], "radius": 1}),
    );
    let error = b
        .doc
        .add_feature(
            &def(
                json!({"type": "mirror", "objects": {"type": "features", "features": [b.sketch]},
                        "plane": {"type": "origin", "plane": "yz"}}),
            ),
            None,
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Sketch1 (F1) cannot be patterned"),
        "{error}"
    );
    // A fillet can (mitcad#105); where its mirrored edge is on no body, the
    // mirror fails.
    let mirror = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "features", "features": [fillet]},
               "plane": {"type": "origin", "plane": "yz"}}),
    );
    assert!(
        matches!(status(&b.doc, mirror), FeatureStatus::Failed(m)
            if m.contains("no copy of the edges of Fillet1 is on a body")),
        "{:?}",
        status(&b.doc, mirror)
    );
}

#[test]
fn patterns_and_mirrors_repeat_fillets_and_chamfers_on_the_copies() {
    // The hole (Extrude2, F4) with a chamfer on its rim (Chamfer1, F5, d5)
    // and a fillet between the chamfer and the wall (Fillet1, F6, d6).
    let mut b = block();
    let (cut, region) = hole(&mut b.doc);
    let side: FaceName = format!("{cut}:side(c1)").parse().unwrap();
    let rim = EdgeName::new(side.clone(), FaceName::end(cut, region.clone()));
    let chamfer = add(
        &mut b.doc,
        json!({"type": "chamfer", "body": "F2.b0", "edges": [rim],
               "size": {"type": "equal_distance", "distance": 1}}),
    );
    let bevel = FaceName::chamfer(chamfer, rim.clone());
    let fillet = add(
        &mut b.doc,
        json!({"type": "fillet", "body": "F2.b0", "edges": [EdgeName::new(bevel, side)],
               "radius": 0.5}),
    );
    let row = |objects: Value| {
        json!({"type": "rectangular_pattern", "objects": {"type": "features", "features": objects},
               "direction1": {"axis": "x", "quantity": 3, "distance": 20},
               "distance_type": "spacing"})
    };
    let counts =
        |doc: &Document<MockKernel>| (doc.kernel().count("chamfer"), doc.kernel().count("fillet"));
    let (chamfers, fillets) = counts(&b.doc);
    // Listed out of the timeline's order: the hole's copies come first.
    let pattern = add(&mut b.doc, row(json!([fillet, chamfer, cut])));
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    // Each in one operation for both copies.
    assert_eq!(counts(&b.doc), (chamfers + 1, fillets + 1));
    let faces = |doc: &Document<MockKernel>| -> Vec<String> {
        shape(doc, "F2.b0")
            .faces
            .iter()
            .map(ToString::to_string)
            .collect()
    };
    // The copies' faces: the chamfer's and the fillet's are the pattern's.
    // `inst`: the instances around a face of the hole, outermost first.
    let copies = |pattern: FeatureUid, inst: &[String]| {
        let wrap = |face: String| {
            inst.iter()
                .rev()
                .fold(face, |name, i| format!("{i}({name})"))
        };
        let side: FaceName = wrap("F4:side(c1)".to_owned()).parse().unwrap();
        let end: FaceName = wrap(format!("F4:end({region})")).parse().unwrap();
        let bevel = FaceName::chamfer(pattern, EdgeName::new(side.clone(), end));
        let round = FaceName::fillet(pattern, EdgeName::new(bevel.clone(), side));
        [bevel.to_string(), round.to_string()]
    };
    let names = faces(&b.doc);
    for k in 1..=2 {
        for face in copies(pattern, &[format!("F7:inst{k}")]) {
            assert!(names.contains(&face), "{face}: {names:?}");
        }
    }
    assert!(b.doc.warnings(pattern).is_empty());

    // The chamfer's distance recomputes the pattern.
    b.doc.set_parameter("d5", 2.0).unwrap();
    assert!(b.doc.stats().evaluated.contains(&pattern));
    b.doc.undo();

    // Copies off the block (x = 70 and 90) reach no body, nor do their
    // chamfers and fillets: a warning.
    b.doc.set_parameter("d7", 5.0).unwrap();
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    let warnings = b.doc.warnings(pattern).join("; ");
    assert!(
        warnings
            .contains("Chamfer1 is not repeated where the copies lack its edges (elements 3, 4)"),
        "{warnings}"
    );

    // A chamfer that fails on all copies at once is tried copy by copy,
    // then with the hole's copies one element after another; failing
    // there too, it fails the pattern.
    b.doc.kernel().fail.borrow_mut().insert("chamfer");
    let (chamfers, _) = counts(&b.doc);
    b.doc.set_parameter("d8", 21.0).unwrap();
    assert!(
        matches!(status(&b.doc, pattern), FeatureStatus::Failed(m)
            if m.contains("Chamfer1 at element 1: chamfer failed")),
        "{:?}",
        status(&b.doc, pattern)
    );
    assert_eq!(counts(&b.doc).0, chamfers + 3);
    b.doc.kernel().fail.borrow_mut().clear();
    b.doc.undo();
    b.doc.undo();

    // A pattern of the pattern repeats them at every product.
    let shifted = add(
        &mut b.doc,
        json!({"type": "rectangular_pattern", "objects": {"type": "features", "features": [pattern]},
               "direction1": {"axis": "y", "quantity": 2, "distance": 5},
               "distance_type": "spacing"}),
    );
    assert_eq!(status(&b.doc, shifted), FeatureStatus::Ok);
    let names = faces(&b.doc);
    let outer = "F8:inst1".to_owned();
    for inst in [
        vec![outer.clone()],
        vec![outer.clone(), "F7:inst1".to_owned()],
        vec![outer, "F7:inst2".to_owned()],
    ] {
        // The inner instance's name inside the outer one's.
        for face in copies(shifted, &inst) {
            assert!(names.contains(&face), "{face}: {names:?}");
        }
    }
    b.doc.undo();

    // A mirror of the hole and its chamfer.
    let mirror = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "features", "features": [cut, chamfer]},
               "plane": {"origin": [30, 0, 0], "normal": [1, 0, 0]}}),
    );
    assert_eq!(status(&b.doc, mirror), FeatureStatus::Ok);
    let [bevel, _] = copies(mirror, &[format!("{mirror}:inst1")]);
    assert!(faces(&b.doc).contains(&bevel), "{bevel}");
    b.doc.undo();

    // A chamfer patterned without the hole: nothing to repeat on by name,
    // nor by place (no hole's copy there).
    let alone = add(&mut b.doc, row(json!([chamfer])));
    assert!(
        matches!(status(&b.doc, alone), FeatureStatus::Failed(m)
            if m.contains("no copy of the edges of Chamfer1 is on a body")),
        "{:?}",
        status(&b.doc, alone)
    );
}

#[test]
fn patterns_and_mirrors_repeat_threads_on_the_copies() {
    // The hole (Extrude2, F4) with a modelled thread on its wall (Thread1,
    // F5): a tapped hole as the .f3d import makes it (mitcad#719).
    let mut b = block();
    let (cut, _) = hole(&mut b.doc);
    let side = format!("{cut}:side(c1)");
    let thread = add(
        &mut b.doc,
        json!({"type": "thread", "faces": [{"body": "F2.b0", "face": side}],
               "thread": {"designation": "M10x1.5"}, "modeled": true}),
    );
    assert_eq!(status(&b.doc, thread), FeatureStatus::Ok);
    let threads = |doc: &Document<MockKernel>| -> Vec<String> {
        doc.kernel()
            .specs
            .borrow()
            .iter()
            .filter(|s| s.starts_with("thread "))
            .cloned()
            .collect()
    };
    let before = threads(&b.doc).len();
    let pattern = add(
        &mut b.doc,
        json!({"type": "rectangular_pattern",
               "objects": {"type": "features", "features": [thread, cut]},
               "direction1": {"axis": "x", "quantity": 3, "distance": 20},
               "distance_type": "spacing"}),
    );
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    assert!(b.doc.warnings(pattern).is_empty());
    // One operation for both copies of the wall, right-handed.
    let made = threads(&b.doc)[before..].to_vec();
    assert_eq!(made.len(), 1, "{made:?}");
    for k in 1..=2 {
        assert!(
            made[0].contains(&format!("{pattern}:inst{k}({side})")),
            "{made:?}"
        );
    }
    assert!(made[0].contains("right true"), "{made:?}");
    b.doc.undo();

    // A mirror's image turns the other way.
    let before = threads(&b.doc).len();
    let mirror = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "features", "features": [cut, thread]},
               "plane": {"origin": [30, 0, 0], "normal": [1, 0, 0]}}),
    );
    assert_eq!(status(&b.doc, mirror), FeatureStatus::Ok);
    let made = threads(&b.doc)[before..].to_vec();
    assert_eq!(made.len(), 1, "{made:?}");
    assert!(
        made[0].contains(&format!("{mirror}:inst1({side})")),
        "{made:?}"
    );
    assert!(made[0].contains("right false"), "{made:?}");
    b.doc.undo();

    // A cosmetic thread (here with its end chamfers, its only geometry)
    // keeps its hand and designation in a mirror (mitcad#733).
    b.doc
        .edit_feature(
            thread,
            &def(
                json!({"type": "thread", "faces": [{"body": "F2.b0", "face": side}],
                        "thread": {"designation": "M10x1.5"}, "chamfer": "both"}),
            ),
        )
        .unwrap();
    assert_eq!(status(&b.doc, thread), FeatureStatus::Ok);
    let before = threads(&b.doc).len();
    let mirror = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "features", "features": [cut, thread]},
               "plane": {"origin": [30, 0, 0], "normal": [1, 0, 0]}}),
    );
    assert_eq!(status(&b.doc, mirror), FeatureStatus::Ok);
    let made = threads(&b.doc)[before..].to_vec();
    assert_eq!(made.len(), 1, "{made:?}");
    assert!(
        made[0].contains(&format!("{mirror}:inst1({side})")),
        "{made:?}"
    );
    assert!(
        made[0].contains("right true") && made[0].contains("pitch 1.5"),
        "{made:?}"
    );
    assert!(made[0].contains("cosmetic chamfer"), "{made:?}");
}

/// The hole (Extrude2, F4) with a chamfer on its rim (Chamfer1, F5) and a
/// fillet between the chamfer and the wall (Fillet1, F6).
fn rounded_hole(doc: &mut Document<MockKernel>) -> (FeatureUid, FeatureUid, FeatureUid, RegionKey) {
    let (cut, region) = hole(doc);
    let side: FaceName = format!("{cut}:side(c1)").parse().unwrap();
    let rim = EdgeName::new(side.clone(), FaceName::end(cut, region.clone()));
    let chamfer = add(
        doc,
        json!({"type": "chamfer", "body": "F2.b0", "edges": [rim],
               "size": {"type": "equal_distance", "distance": 1}}),
    );
    let bevel = FaceName::chamfer(chamfer, rim);
    let fillet = add(
        doc,
        json!({"type": "fillet", "body": "F2.b0", "edges": [EdgeName::new(bevel, side)],
               "radius": 0.5}),
    );
    (cut, chamfer, fillet, region)
}

fn row_of_features(objects: &[FeatureUid], compute: &str) -> Value {
    json!({"type": "rectangular_pattern",
           "objects": {"type": "features", "features": objects},
           "direction1": {"axis": "x", "quantity": 3, "distance": 20},
           "distance_type": "spacing", "compute": compute})
}

#[test]
fn optimized_patterns_copy_the_faces_of_fillets_and_chamfers() {
    let mut b = block();
    let (cut, chamfer, fillet, _) = rounded_hole(&mut b.doc);
    let counts =
        |doc: &Document<MockKernel>| (doc.kernel().count("chamfer"), doc.kernel().count("fillet"));
    let before = counts(&b.doc);
    let pattern = add(
        &mut b.doc,
        row_of_features(&[cut, chamfer, fillet], "optimized"),
    );
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    // The faces the features made, once for all of them; the chamfer and
    // the fillet are not worked out on any copy.
    assert_eq!(b.doc.kernel().count("feature_changes"), 1);
    assert_eq!(counts(&b.doc), before);
    // Each copy carries the faces of the hole, its chamfer and its fillet.
    let names: Vec<String> = shape(&b.doc, "F2.b0")
        .faces
        .iter()
        .map(ToString::to_string)
        .collect();
    for k in 1..=2 {
        for made in ["F4:side(c1)", "F5:chamfer(", "F6:fillet("] {
            let copy = format!("F7:inst{k}({made}");
            assert!(
                names.iter().any(|n| n.starts_with(&copy)),
                "{copy}: {names:?}"
            );
        }
    }

    // A copy that stands on nothing (the first element's) is left out.
    b.doc.kernel().unsupported.borrow_mut().insert(0);
    let mut wider = row_of_features(&[cut, chamfer, fillet], "optimized");
    wider["direction1"]["distance"] = json!(19);
    b.doc.edit_feature(pattern, &def(wider)).unwrap();
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    let names: Vec<String> = shape(&b.doc, "F2.b0")
        .faces
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(!names.iter().any(|n| n.starts_with("F7:inst1(F4:side(c1))")));
    assert!(names.iter().any(|n| n.starts_with("F7:inst2(F4:side(c1))")));
    b.doc.kernel().unsupported.borrow_mut().clear();
    b.doc
        .edit_feature(
            pattern,
            &def(row_of_features(&[cut, chamfer, fillet], "optimized")),
        )
        .unwrap();

    // The same pattern as Adjust works the chamfer and the fillet out on
    // the copies, in one operation for both.
    let calls = b.doc.kernel().count("feature_changes");
    let adjust = add(
        &mut b.doc,
        row_of_features(&[cut, chamfer, fillet], "adjust"),
    );
    assert_eq!(status(&b.doc, adjust), FeatureStatus::Ok);
    assert_eq!(counts(&b.doc), (before.0 + 1, before.1 + 1));
    assert_eq!(b.doc.kernel().count("feature_changes"), calls);
    b.doc.undo();
    let adjusted = counts(&b.doc);

    // A failure is the pattern's: the roundings are not tried copy by copy.
    b.doc.kernel().fail.borrow_mut().insert("feature_changes");
    let mut wider = row_of_features(&[cut, chamfer, fillet], "optimized");
    wider["direction1"]["distance"] = json!(21);
    b.doc.edit_feature(pattern, &def(wider)).unwrap();
    assert!(
        matches!(status(&b.doc, pattern), FeatureStatus::Failed(m)
            if m.contains("feature_changes failed")),
        "{:?}",
        status(&b.doc, pattern)
    );
    assert_eq!(counts(&b.doc), adjusted);
    b.doc.kernel().fail.borrow_mut().clear();

    // Features without a rounding are copied as their tools, as before.
    let calls = b.doc.kernel().count("feature_changes");
    b.doc
        .edit_feature(pattern, &def(row_of_features(&[cut], "optimized")))
        .unwrap();
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    assert_eq!(b.doc.kernel().count("feature_changes"), calls);
}

#[test]
fn a_chamfer_patterned_alone_goes_on_the_edges_at_its_copies_places() {
    // Three holes (Extrude2 and RectangularPattern1, 20 mm apart along x),
    // a chamfer on the first one's rim, and a pattern of the chamfer
    // alone: the other rims are found by their places.
    let mut b = block();
    let (cut, region) = hole(&mut b.doc);
    let row = |objects: Value| {
        json!({"type": "rectangular_pattern", "objects": {"type": "features", "features": objects},
               "direction1": {"axis": "x", "quantity": 3, "distance": 20},
               "distance_type": "spacing"})
    };
    let holes = add(&mut b.doc, row(json!([cut])));
    let rim = |inst: Option<u32>| {
        let wrap = |face: FaceName| match inst {
            Some(k) => mitcad_model::transform::Instance {
                feature: holes,
                index: k,
            }
            .face(face),
            None => face,
        };
        EdgeName::new(
            wrap(format!("{cut}:side(c1)").parse().unwrap()),
            wrap(FaceName::end(cut, region.clone())),
        )
    };
    for (k, x) in [(None, 15.0), (Some(1), 35.0), (Some(2), 55.0)] {
        b.doc
            .kernel()
            .middles
            .borrow_mut()
            .insert(rim(k).to_string(), [x, 10.0, 20.0]);
    }
    let chamfer = add(
        &mut b.doc,
        json!({"type": "chamfer", "body": "F2.b0", "edges": [rim(None)],
               "size": {"type": "equal_distance", "distance": 1}}),
    );
    let pattern = add(&mut b.doc, row(json!([chamfer])));
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    assert!(
        b.doc.warnings(pattern).is_empty(),
        "{:?}",
        b.doc.warnings(pattern)
    );
    let faces = shape(&b.doc, "F2.b0").faces;
    for k in [1, 2] {
        let bevel = FaceName::chamfer(pattern, rim(Some(k)));
        assert!(faces.contains(&bevel), "{bevel}: {faces:?}");
    }
}

#[test]
fn mirrors_copy_bodies_and_features() {
    let mut b = block();
    let mirror = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "plane": {"type": "origin", "plane": "yz"}}),
    );
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0"]);
    assert_bounds(&b.doc, "F3.b0", [-60.0, 0.0, 0.0], [0.0, 40.0, 20.0]);
    assert!(shape(&b.doc, "F3.b0").history.contains("inst1"));
    // Combined: the copy touches the original and joins it.
    b.doc
        .edit_feature(
            mirror,
            &def(
                json!({"type": "mirror", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
                        "plane": {"type": "origin", "plane": "yz"}, "combine": true}),
            ),
        )
        .unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0"]);
    assert!(shape(&b.doc, "F2.b0").history.starts_with("join("));
    // A copy that does not touch stays a body.
    *b.doc.kernel().touch.borrow_mut() = Some(vec![false]);
    b.doc
        .edit_feature(
            mirror,
            &def(
                json!({"type": "mirror", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
                        "plane": {"type": "origin", "plane": "yz"}, "combine": true,
                        "compute": "identical"}),
            ),
        )
        .unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0"]);
    *b.doc.kernel().touch.borrow_mut() = None;

    // A feature mirrored in a planar face of the block (the mock's face is
    // at x = 30, so that the image of the hole lies in the block: copies
    // that stay away from the bodies are left out).
    let mut b = block();
    let (cut, _) = hole(&mut b.doc);
    let face = "F2:side(c2[c1,c3])";
    b.doc.kernel().surfaces.borrow_mut().insert(
        face.to_owned(),
        SurfaceGeometry::Plane {
            origin: [30.0, 0.0, 0.0],
            normal: [1.0, 0.0, 0.0],
        },
    );
    let mirror = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "features", "features": [cut]},
               "plane": {"type": "face", "body": "F2.b0", "face": face}}),
    );
    assert_eq!(status(&b.doc, mirror), FeatureStatus::Ok);
    let body = shape(&b.doc, "F2.b0");
    assert!(
        body.faces
            .contains(&"F5:inst1(F4:side(c1))".parse().unwrap())
    );
    assert!(body.history.contains("moved("), "{}", body.history);
    // The mirror depends on the face's feature.
    let error = b.doc.delete_feature(b.extrude, false).unwrap_err();
    assert!(error.to_string().contains("Mirror1"), "{error}");
    let error = b
        .doc
        .add_feature(
            &def(
                json!({"type": "mirror", "objects": {"type": "features", "features": [cut]},
                        "plane": {"type": "origin", "plane": "yz"}, "combine": true}),
            ),
            None,
        )
        .unwrap_err();
    assert!(error.to_string().contains("bodies only"), "{error}");
}

#[test]
fn a_mirror_of_a_feature_and_of_its_mirror_copies_it_once() {
    // Mirror2 reflects the hole and Mirror1, whose original is the hole
    // too: the hole is reflected once (a second copy at the same place
    // would reach nothing), Mirror1's copy once more.
    let mut b = block();
    let (cut, _) = hole(&mut b.doc);
    let first = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "features", "features": [cut]},
               "plane": {"origin": [30, 0, 0], "normal": [1, 0, 0]}}),
    );
    let second = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "features", "features": [cut, first]},
               "plane": {"origin": [0, 20, 0], "normal": [0, 1, 0]}}),
    );
    assert_eq!(status(&b.doc, second), FeatureStatus::Ok);
    let mut names: Vec<String> = shape(&b.doc, "F2.b0")
        .faces
        .iter()
        .map(ToString::to_string)
        .filter(|f| f.contains("side(c1)"))
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "F4:side(c1)",
            "F5:inst1(F4:side(c1))",
            "F6:inst1(F4:side(c1))",
            "F6:inst1(F5:inst1(F4:side(c1)))",
        ]
    );
}

#[test]
fn faces_pattern_as_bosses_or_pockets() {
    let mut b = block();
    let (cut, region) = hole(&mut b.doc);
    b.doc.kernel().voids.set(true);
    let wall = format!("{cut}:side(c1)");
    let pattern = add(
        &mut b.doc,
        json!({"type": "circular_pattern",
               "objects": {"type": "faces", "body": "F2.b0", "faces": [wall]},
               "axis": {"type": "fixed", "origin": [30, 20, 0], "direction": [0, 0, 1]},
               "quantity": 2, "angle": std::f64::consts::TAU}),
    );
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    assert_eq!(b.doc.kernel().count("face_tool"), 1);
    let body = shape(&b.doc, "F2.b0");
    assert!(body.history.starts_with("cut("), "{}", body.history);
    assert!(
        body.faces
            .contains(&format!("F5:inst1({wall})").parse().unwrap())
    );
    let _ = region;
}

fn second_block(doc: &mut Document<MockKernel>) -> FeatureUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [20.0, 0.0], &num(40.0), &num(40.0))
        .unwrap()
        .region;
    add(
        doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}),
    )
}

#[test]
fn combine_joins_cuts_and_intersects_bodies() {
    let combine = |operation: &str, keep: bool| {
        json!({"type": "combine", "target": "F2.b0", "tools": ["F4.b0"],
               "operation": operation, "keep_tools": keep})
    };
    let mut b = block();
    second_block(&mut b.doc);
    let uid = add(&mut b.doc, combine("join", false));
    assert_eq!(b.doc.feature(uid).unwrap().name, "Combine1");
    assert_eq!(uids(&b.doc), ["F2.b0"]);
    assert!(shape(&b.doc, "F2.b0").history.starts_with("unite("));
    b.doc
        .edit_feature(uid, &def(combine("join", true)))
        .unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0", "F4.b0"]);

    // A cut that splits the target makes bodies of the combine.
    b.doc.kernel().split.set(Some(2));
    b.doc
        .edit_feature(uid, &def(combine("cut", false)))
        .unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0", "F5.b0"]);
    // An intersection keeps its pieces as one body.
    b.doc
        .edit_feature(uid, &def(combine("intersect", false)))
        .unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0"]);
    assert!(shape(&b.doc, "F2.b0").history.starts_with("unite(common("));
    b.doc.kernel().split.set(None);
    *b.doc.kernel().touch.borrow_mut() = Some(vec![false]);
    b.doc
        .edit_feature(uid, &def(combine("intersect", true)))
        .unwrap();
    assert!(
        matches!(status(&b.doc, uid), FeatureStatus::Failed(m) if m.contains("do not intersect"))
    );
    *b.doc.kernel().touch.borrow_mut() = None;

    for (value, message) in [
        (
            json!({"type": "combine", "target": "F2.b0", "tools": ["F2.b0"], "operation": "join"}),
            "both the target and a tool",
        ),
        (
            json!({"type": "combine", "target": "F2.b0", "tools": [], "operation": "join"}),
            "no tool bodies",
        ),
        (
            json!({"type": "combine", "target": "F2.b0", "tools": ["F1.b0"], "operation": "cut"}),
            "does not create bodies",
        ),
    ] {
        let error = b.doc.add_feature(&def(value), None).unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn moves_and_copies_place_bodies() {
    let mut b = block();
    let moved = add(
        &mut b.doc,
        json!({"type": "move", "bodies": ["F2.b0"],
               "transform": {"type": "translate_xyz", "x": 10, "y": 5, "z": 0}}),
    );
    assert_eq!(b.doc.feature(moved).unwrap().name, "Move1");
    assert_bounds(&b.doc, "F2.b0", [10.0, 5.0, 0.0], [70.0, 45.0, 20.0]);
    // A move keeps the face names.
    assert!(
        shape(&b.doc, "F2.b0")
            .faces
            .contains(&FaceName::end(b.extrude, b.region.clone()))
    );

    let copied = add(
        &mut b.doc,
        json!({"type": "move", "bodies": ["F2.b0"], "copy": true,
               "transform": {"type": "rotate", "axis": {"type": "origin", "axis": "z"},
                             "angle": std::f64::consts::FRAC_PI_2}}),
    );
    assert_eq!(uids(&b.doc), ["F2.b0", "F4.b0"]);
    assert_bounds(&b.doc, "F4.b0", [-45.0, 10.0, 0.0], [-5.0, 70.0, 20.0]);
    assert!(
        shape(&b.doc, "F4.b0")
            .faces
            .iter()
            .all(|f| f.to_string().starts_with("F4:inst1(F2:"))
    );
    let _ = copied;

    add(
        &mut b.doc,
        json!({"type": "move", "bodies": ["F2.b0"],
               "transform": {"type": "point_to_point", "from": {"type": "fixed", "point": [10, 5, 0]},
                             "to": {"type": "origin"}}}),
    );
    assert_bounds(&b.doc, "F2.b0", [0.0, 0.0, 0.0], [60.0, 40.0, 20.0]);
    add(
        &mut b.doc,
        json!({"type": "move", "bodies": ["F2.b0"],
               "transform": {"type": "free", "matrix": [[0, -1, 0, 100], [1, 0, 0, 0], [0, 0, 1, 0]]}}),
    );
    assert_bounds(&b.doc, "F2.b0", [60.0, 0.0, 0.0], [100.0, 60.0, 20.0]);

    let error = b
        .doc
        .add_feature(
            &def(json!({"type": "move", "bodies": ["F2.b0"],
                        "transform": {"type": "free", "matrix": [[2, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0]]}})),
            None,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("rotation and a translation"),
        "{error}"
    );
    let error = b
        .doc
        .add_feature(
            &def(json!({"type": "move", "bodies": [],
                        "transform": {"type": "translate_xyz", "x": 1, "y": 0, "z": 0}})),
            None,
        )
        .unwrap_err();
    assert!(error.to_string().contains("no bodies"), "{error}");
}

#[test]
fn move_previews_report_what_their_motion_turns_about() {
    // mitcad#838: a preview reports a Rotate's axis and the points of
    // Point to Point and Point to Position as the move resolved them, so
    // that a drag of its angle or position moves the shown bodies as the
    // exact result does; nothing for Free Move and Translate.
    let mut b = block();
    let before = b.doc.features().count();
    let motion = |doc: &mut Document<MockKernel>, transform: Value| {
        let command = json!({"cmd": "add_feature",
                             "def": {"type": "move", "bodies": ["F2.b0"], "transform": transform}});
        let report: Value =
            serde_json::from_str(&doc.preview(&command.to_string()).unwrap()).unwrap();
        assert_eq!(report["status"], "ok", "{report}");
        report["motion"].clone()
    };
    // The axis as the move turns about it: a unit direction, its sense
    // kept (right-handed about it).
    let rotate = json!({"type": "rotate",
                        "axis": {"type": "fixed", "origin": [30, 20, 5], "direction": [0, 0, -2]},
                        "angle": 0.5});
    assert_eq!(
        motion(&mut b.doc, rotate),
        json!({"type": "rotate", "origin": [30.0, 20.0, 5.0], "direction": [0.0, 0.0, -1.0]})
    );
    let points = json!({"type": "point_to_point", "from": {"type": "fixed", "point": [10, 5, 0]},
                        "to": {"type": "origin"}});
    assert_eq!(
        motion(&mut b.doc, points),
        json!({"type": "point_to_point", "from": [10.0, 5.0, 0.0], "to": [0.0, 0.0, 0.0]})
    );
    let position = json!({"type": "point_to_position", "point": {"type": "fixed", "point": [10, 5, 0]},
                          "x": 1, "y": 2, "z": 3});
    assert_eq!(
        motion(&mut b.doc, position),
        json!({"type": "point_to_position", "point": [10.0, 5.0, 0.0]})
    );
    let translate = json!({"type": "translate_xyz", "x": 1, "y": 2, "z": 3});
    assert_eq!(motion(&mut b.doc, translate), Value::Null);
    // An origin axis.
    let rotate = json!({"type": "rotate", "axis": {"type": "origin", "axis": "x"}, "angle": 0.5});
    assert_eq!(
        motion(&mut b.doc, rotate),
        json!({"type": "rotate", "origin": [0.0, 0.0, 0.0], "direction": [1.0, 0.0, 0.0]})
    );
    // Previews add nothing.
    assert_eq!(b.doc.features().count(), before);
}

#[test]
fn move_face_previews_report_the_axis_they_turn_about() {
    // mitcad#854: a Move Face preview reports the axis its Rotate turns
    // about, as a move's does, so that the app's ring runs about it: a
    // circular edge whose circle's own axis points down (-Z) is taken in
    // its canonical direction (+Z), the way the faces turn.
    let mut b = block();
    let before = b.doc.features().count();
    let top = FaceName::end(b.extrude, b.region.clone());
    let body = shape(&b.doc, "F2.b0");
    let rim = body
        .edges
        .iter()
        .find(|(one, other)| *one == top || *other == top)
        .map(|(one, other)| {
            format!("E{{{one}|{other}}}")
                .parse::<EdgeName>()
                .unwrap()
                .to_string()
        })
        .expect("an edge of the top face");
    b.doc.kernel().edge_curves.borrow_mut().insert(
        rim.clone(),
        CurveGeometry::Circle {
            center: [30.0, 20.0, 20.0],
            normal: [0.0, 0.0, -1.0],
            radius: 5.0,
            start: [35.0, 20.0, 20.0],
            end: [35.0, 20.0, 20.0],
        },
    );
    let report = |doc: &mut Document<MockKernel>, transform: Value| -> Value {
        let command = json!({"cmd": "add_feature",
                             "def": {"type": "move_face", "body": "F2.b0", "faces": [top],
                                     "transform": transform}});
        serde_json::from_str(&doc.preview(&command.to_string()).unwrap()).unwrap()
    };
    let rotate = |angle: f64| {
        json!({"type": "rotate", "axis": {"type": "edge", "body": "F2.b0", "edge": rim},
               "angle": angle})
    };
    let about =
        json!({"type": "rotate", "origin": [30.0, 20.0, 20.0], "direction": [0.0, 0.0, 1.0]});
    let turned = report(&mut b.doc, rotate(0.1));
    assert_eq!(turned["status"], "ok", "{turned}");
    assert_eq!(turned["motion"], about);
    // A motion that moves nothing fails, and still reports the axis the
    // ring turns about.
    let still = report(&mut b.doc, rotate(0.0));
    assert_ne!(still["status"], "ok", "{still}");
    assert_eq!(still["motion"], about);
    // Point to Point and Point to Position: the points it moves from.
    let points = report(
        &mut b.doc,
        json!({"type": "point_to_point", "from": {"type": "fixed", "point": [10, 5, 20]},
               "to": {"type": "fixed", "point": [10, 5, 22]}}),
    );
    assert_eq!(points["status"], "ok", "{points}");
    assert_eq!(
        points["motion"],
        json!({"type": "point_to_point", "from": [10.0, 5.0, 20.0], "to": [10.0, 5.0, 22.0]})
    );
    let position = report(
        &mut b.doc,
        json!({"type": "point_to_position", "point": {"type": "fixed", "point": [10, 5, 20]},
               "x": 10, "y": 5, "z": 23}),
    );
    assert_eq!(position["status"], "ok", "{position}");
    assert_eq!(
        position["motion"],
        json!({"type": "point_to_position", "point": [10.0, 5.0, 20.0]})
    );
    // Translate reports none.
    let translate = report(
        &mut b.doc,
        json!({"type": "translate_xyz", "x": 0, "y": 0, "z": 2}),
    );
    assert_eq!(translate["status"], "ok", "{translate}");
    assert_eq!(translate["motion"], Value::Null);
    assert_eq!(b.doc.features().count(), before);
}

#[test]
fn alignments_and_scales() {
    let mut b = block();
    // The block's top face (the mock's middle (30, 20, 20)) against a plane
    // at z = 100 facing up: turned over and its middle onto the plane's
    // origin.
    let align = add(
        &mut b.doc,
        json!({"type": "align", "bodies": ["F2.b0"],
               "from": {"type": "plane", "plane": {"type": "face", "body": "F2.b0",
                        "face": "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})"}},
               "to": {"type": "plane", "plane": {"type": "fixed", "origin": [0, 0, 100],
                      "normal": [0, 0, 1]}}}),
    );
    assert_eq!(status(&b.doc, align), FeatureStatus::Ok);
    assert_bounds(&b.doc, "F2.b0", [-30.0, -20.0, 100.0], [30.0, 20.0, 120.0]);
    let error = b
        .doc
        .add_feature(
            &def(json!({"type": "align", "bodies": ["F2.b0"],
                        "from": {"type": "axis", "axis": {"type": "origin", "axis": "x"}},
                        "to": {"type": "point", "point": {"type": "origin"}}})),
            None,
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cannot align an axis to a point"),
        "{error}"
    );

    let mut b = block();
    let scale = add(
        &mut b.doc,
        json!({"type": "scale", "bodies": ["F2.b0"], "point": {"type": "origin"},
               "scale": {"type": "non_uniform", "x": 2, "y": 1, "z": 0.5}}),
    );
    assert_eq!(b.doc.feature(scale).unwrap().name, "Scale1");
    assert_bounds(&b.doc, "F2.b0", [0.0, 0.0, 0.0], [120.0, 40.0, 10.0]);
    let error = b
        .doc
        .add_feature(
            &def(
                json!({"type": "scale", "bodies": ["F2.b0"], "point": {"type": "origin"},
                        "scale": {"type": "uniform", "factor": 0}}),
            ),
            None,
        )
        .unwrap_err();
    assert!(error.to_string().contains("greater than zero"), "{error}");
}

#[test]
fn primitives_are_tools_that_patterns_rebuild() {
    let mut doc = Document::new(MockKernel::default());
    let added = doc
        .add_feature(
            &def(
                json!({"type": "box", "plane": {"type": "origin", "plane": "xy"},
                        "corner": [5, 5], "length": 30, "width": 20, "height": 10,
                        "operation": "new_body"}),
            ),
            None,
        )
        .unwrap();
    assert_eq!(added.name, "Box1");
    assert_eq!(added.parameters, vec!["d1", "d2", "d3"]);
    assert_eq!(uids(&doc), ["F1.b0"]);
    assert!(
        shape(&doc, "F1.b0")
            .faces
            .contains(&"F1:side3".parse().unwrap())
    );
    assert_bounds(&doc, "F1.b0", [5.0, 5.0, 0.0], [35.0, 25.0, 10.0]);
    // A symmetric height centres the box on the plane.
    doc.edit_feature(
        added.uid,
        &def(
            json!({"type": "box", "plane": {"type": "origin", "plane": "xy"},
                    "corner": [5, 5], "length": "d1", "width": "d2", "height": "d3",
                    "symmetric": true, "operation": "new_body"}),
        ),
    )
    .unwrap();
    assert_bounds(&doc, "F1.b0", [5.0, 5.0, -5.0], [35.0, 25.0, 5.0]);

    // A cylinder joined to the box, patterned with Adjust: each copy is
    // rebuilt on the moved plane.
    let cylinder = add(
        &mut doc,
        json!({"type": "cylinder", "plane": {"type": "origin", "plane": "xy"}, "center": [10, 10],
               "diameter": 4, "height": 20, "operation": "join"}),
    );
    let primitives = doc.kernel().count("primitive");
    add(
        &mut doc,
        json!({"type": "rectangular_pattern",
               "objects": {"type": "features", "features": [cylinder]},
               "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": 3,
                              "distance": 5},
               "distance_type": "spacing"}),
    );
    assert_eq!(doc.kernel().count("primitive"), primitives + 2);
    let body = shape(&doc, "F1.b0");
    assert!(
        body.history.contains("cylinder(F2:2x2x20@20,10,0)"),
        "{}",
        body.history
    );
    assert!(body.faces.contains(&"F3:inst2(F2:top)".parse().unwrap()));

    let error = doc
        .add_feature(
            &def(
                json!({"type": "torus", "plane": {"type": "origin", "plane": "xy"},
                        "diameter": 10, "section_diameter": 10, "position": "inside",
                        "operation": "new_body"}),
            ),
            None,
        )
        .unwrap_err();
    assert!(error.to_string().contains("too large"), "{error}");
    let error = doc
        .add_feature(
            &def(
                json!({"type": "sphere", "plane": {"type": "origin", "plane": "xy"},
                        "diameter": 10, "operation": "new_body", "participants": ["F1.b0"]}),
            ),
            None,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("no participant bodies"),
        "{error}"
    );
    // A cut sphere that misses every body fails.
    *doc.kernel().touch.borrow_mut() = Some(vec![false]);
    let sphere = add(
        &mut doc,
        json!({"type": "sphere", "plane": {"type": "origin", "plane": "xz"},
               "center": [100, 100], "diameter": 10, "operation": "cut"}),
    );
    assert!(
        matches!(status(&doc, sphere), FeatureStatus::Failed(m) if m.contains("does not cut into"))
    );
}

#[test]
fn path_patterns_follow_sketch_curves() {
    let mut b = block();
    // Line c1 of the block's rectangle runs from (0, 0) to (60, 0).
    let path = |path: Value, along: bool| {
        json!({"type": "path_pattern", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "path": path, "quantity": 3, "distance": 60, "distance_type": "extent",
               "start": 0.5, "along_path": along})
    };
    let pattern = add(
        &mut b.doc,
        path(
            json!({"type": "sketch_curve", "sketch": "F1", "curve": "c1"}),
            false,
        ),
    );
    assert_bounds(&b.doc, "F3.b0", [30.0, 0.0, 0.0], [90.0, 40.0, 20.0]);
    assert_bounds(&b.doc, "F3.b1", [60.0, 0.0, 0.0], [120.0, 40.0, 20.0]);

    // Along a circle of radius 10 about the origin, a quarter turn apart.
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let curve = b
        .doc
        .add_circle(sketch, [0.0, 0.0], &num(20.0))
        .unwrap()
        .curves[0];
    let mut circle = path(
        json!({"type": "sketch_curve", "sketch": sketch, "curve": curve.curve_name()}),
        true,
    );
    circle["start"] = json!(0);
    circle["distance"] = json!(std::f64::consts::PI * 5.0);
    circle["distance_type"] = json!("spacing");
    let around = add(&mut b.doc, circle);
    assert_eq!(status(&b.doc, around), FeatureStatus::Ok);
    assert_bounds(&b.doc, "F5.b0", [-40.0, 0.0, 0.0], [0.0, 60.0, 20.0]);
    assert_bounds(&b.doc, "F5.b1", [-60.0, -40.0, 0.0], [0.0, 0.0, 20.0]);
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    let error = b
        .doc
        .add_feature(
            &def(path(
                json!({"type": "sketch_curve", "sketch": "F1", "curve": "c9"}),
                false,
            )),
            None,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("Sketch1 has no curve c9"),
        "{error}"
    );
}

#[test]
fn path_patterns_follow_chains_of_sketch_curves() {
    // mitcad#464: along c1 from (0, 0) to (60, 0) and on up c2 to (60, 40),
    // 100 long; elements 35 apart from the start: at (35, 0) and (60, 10).
    let mut b = block();
    let chain = |along: bool| {
        json!({"type": "path_pattern", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "path": {"sketch": "F1", "curves": ["c1", "c2"]}, "quantity": 3, "distance": 35,
               "distance_type": "spacing", "along_path": along})
    };
    let moved = add(&mut b.doc, chain(false));
    assert_eq!(status(&b.doc, moved), FeatureStatus::Ok);
    assert_bounds(&b.doc, "F3.b0", [35.0, 0.0, 0.0], [95.0, 40.0, 20.0]);
    assert_bounds(&b.doc, "F3.b1", [60.0, 10.0, 0.0], [120.0, 50.0, 20.0]);
    // Turned with the path: the copy on c2 a quarter turn about the start,
    // then moved there.
    let turned = add(&mut b.doc, chain(true));
    assert_eq!(status(&b.doc, turned), FeatureStatus::Ok);
    assert_bounds(&b.doc, "F4.b0", [35.0, 0.0, 0.0], [95.0, 40.0, 20.0]);
    assert_bounds(&b.doc, "F4.b1", [20.0, 10.0, 0.0], [60.0, 70.0, 20.0]);
    // From the far end: flipped, the elements go back from the start.
    let mut back = chain(false);
    back["start"] = json!(1.0);
    back["flip"] = json!(true);
    let from_end = add(&mut b.doc, back);
    assert_eq!(status(&b.doc, from_end), FeatureStatus::Ok);
    // At (60, 5) and (30, 0), moved from (60, 40).
    assert_bounds(&b.doc, "F5.b0", [0.0, -35.0, 0.0], [60.0, 5.0, 20.0]);
    assert_bounds(&b.doc, "F5.b1", [-30.0, -40.0, 0.0], [30.0, 0.0, 20.0]);
    // The curves must join.
    let mut apart = chain(false);
    apart["path"] = json!({"sketch": "F1", "curves": ["c1", "c3"]});
    let failed = add(&mut b.doc, apart);
    assert!(
        matches!(status(&b.doc, failed), FeatureStatus::Failed(m) if m.contains("not connected"))
    );
}

#[test]
fn path_patterns_go_round_closed_chains_from_a_point() {
    // mitcad#673: the block's outline, 200 long and closed. The original
    // sits at the outline's point nearest (30, -5, 10), 30 along c1; the
    // copies 70 apart at 100 (the corner (60, 40)), 170 ((0, 30) on the
    // last side) and 240, round again to 40 ((40, 0)).
    let mut b = block();
    let chain = |curves: Value| {
        json!({"type": "path_pattern", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "path": {"sketch": "F1", "curves": curves}, "quantity": 4, "distance": 70,
               "distance_type": "spacing", "start_point": [30, -5, 10]})
    };
    let pattern = add(&mut b.doc, chain(json!(["c1", "c2", "c3", "c4"])));
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    assert_bounds(&b.doc, "F3.b0", [30.0, 40.0, 0.0], [90.0, 80.0, 20.0]);
    assert_bounds(&b.doc, "F3.b1", [-30.0, 30.0, 0.0], [30.0, 70.0, 20.0]);
    assert_bounds(&b.doc, "F3.b2", [10.0, 0.0, 0.0], [70.0, 40.0, 20.0]);
    // An open chain goes on straight past its end instead: 240 is 80
    // beyond (0, 40) along c3's line, at (-80, 40).
    let open = add(&mut b.doc, chain(json!(["c1", "c2", "c3"])));
    assert_eq!(status(&b.doc, open), FeatureStatus::Ok);
    assert_bounds(&b.doc, "F4.b2", [-110.0, 40.0, 0.0], [-50.0, 80.0, 20.0]);
}

#[test]
fn combined_copies_of_new_body_features_join_their_body() {
    // mitcad#673: copies of the block's own extrusion (a new body) along c1,
    // 30 apart, joined to the block instead of new bodies of their own.
    let pattern = |combine: bool| {
        json!({"type": "path_pattern", "objects": {"type": "features", "features": ["F2"]},
               "path": {"type": "sketch_curve", "sketch": "F1", "curve": "c1"}, "quantity": 3,
               "distance": 30, "distance_type": "spacing", "combine": combine})
    };
    let mut b = block();
    let apart = add(&mut b.doc, pattern(false));
    assert_eq!(status(&b.doc, apart), FeatureStatus::Ok);
    assert_eq!(uids(&b.doc).len(), 3);
    let mut b = block();
    let joined = add(&mut b.doc, pattern(true));
    assert_eq!(status(&b.doc, joined), FeatureStatus::Ok);
    assert_eq!(uids(&b.doc), ["F2.b0"]);
    assert_bounds(&b.doc, "F2.b0", [0.0, 0.0, 0.0], [120.0, 40.0, 20.0]);
    assert!(shape(&b.doc, "F2.b0").history.starts_with("join("));
    // Not for faces.
    let error = b
        .doc
        .add_feature(
            &def(json!({"type": "path_pattern",
                        "objects": {"type": "faces", "body": "F2.b0", "faces": ["F2:side(c1)"]},
                        "path": {"type": "sketch_curve", "sketch": "F1", "curve": "c1"},
                        "quantity": 2, "distance": 30, "distance_type": "spacing",
                        "combine": true})),
            None,
        )
        .map(|_| ())
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("combine applies to bodies and features"),
        "{error}"
    );
}

#[test]
fn files_keep_the_definitions() {
    let mut b = block();
    let (cut, _) = hole(&mut b.doc);
    second_block(&mut b.doc);
    for value in [
        x_pattern(
            3.0,
            100.0,
            json!({"compute": "identical", "suppressed_elements": [2]}),
        ),
        json!({"type": "circular_pattern", "objects": {"type": "features", "features": [cut]},
               "axis": {"type": "edge", "body": "F2.b0",
                        "edge": "E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"},
               "quantity": 4, "angle": std::f64::consts::TAU}),
        json!({"type": "mirror", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "plane": {"type": "fixed", "origin": [30, 0, 0], "normal": [1, 0, 0]},
               "combine": true}),
        json!({"type": "combine", "target": "F2.b0", "tools": ["F6.b0"], "operation": "cut",
               "keep_tools": true, "new_component": true}),
        json!({"type": "move", "bodies": ["F2.b0"], "copy": true,
               "transform": {"type": "translate_along",
                             "axis": {"type": "fixed", "origin": [0, 0, 0], "direction": [0, 1, 0]},
                             "distance": 50}}),
        json!({"type": "scale", "bodies": ["F2.b0"], "point": {"type": "fixed", "point": [1, 2, 3]},
               "scale": {"type": "uniform", "factor": 2}}),
        json!({"type": "torus", "plane": {"type": "origin", "plane": "yz"}, "diameter": 40,
               "section_diameter": 10, "operation": "new_body"}),
    ] {
        add(&mut b.doc, value);
    }
    let json = b.doc.to_json();
    let loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json);
    let features: Vec<_> = loaded.features().map(|f| f.def.clone()).collect();
    let original: Vec<_> = b.doc.features().map(|f| f.def.clone()).collect();
    assert_eq!(features, original);
    assert!(json.contains("rectangular_pattern"), "{json}");
}

#[test]
fn remove_body_takes_bodies_out_of_the_design() {
    let mut doc = block().doc;
    let before = uids(&doc);
    assert_eq!(before.len(), 1);
    let removal = add(
        &mut doc,
        json!({"type": "remove_body", "bodies": [before[0]]}),
    );
    assert_eq!(status(&doc, removal), FeatureStatus::Ok);
    assert!(uids(&doc).is_empty());
    // A feature that refers to the removed body fails.
    let moved = doc.add_feature(
        &def(json!({"type": "move", "bodies": [before[0]],
                    "transform": {"type": "translate_xyz", "x": 1.0, "y": 0.0, "z": 0.0}})),
        None,
    );
    assert!(moved.is_err() || matches!(status(&doc, moved.unwrap().uid), FeatureStatus::Failed(_)));
    // No bodies, and none twice.
    assert!(
        doc.add_feature(&def(json!({"type": "remove_body", "bodies": []})), None)
            .is_err()
    );
}

/// A join that touched two of its participants merged the second into the
/// first; a pattern of it joins its copies to the body they went into
/// (mitcad#406), and says so when a participant is gone.
#[test]
fn copies_of_a_join_use_the_body_its_participants_went_into() {
    // Two rings turned about z by one revolve (F2.b0 and F2.b1) and a post
    // from the lower ring to the upper one, joined to both.
    let mut doc = Document::new(MockKernel::default());
    let section = doc.add_sketch(SketchPlane::Xz).unwrap().uid;
    let mut rings = Vec::new();
    for z in [0.0, 100.0] {
        let region = doc
            .add_rectangle(section, [20.0, z], &num(5.0), &num(5.0))
            .unwrap()
            .region;
        rings.push(json!({"sketch": section, "region": region}));
    }
    let revolve = add(
        &mut doc,
        json!({"type": "revolve", "profiles": rings, "axis": "z",
               "extent": {"type": "full"}, "operation": "new_body"}),
    );
    assert_eq!(
        uids(&doc),
        [format!("{revolve}.b0"), format!("{revolve}.b1")]
    );
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_circle(sketch, [22.5, 0.0], &num(2.0))
        .unwrap()
        .region;
    let post = add(
        &mut doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "distance", "distance": 105}, "operation": "join",
               "participants": [format!("{revolve}.b0"), format!("{revolve}.b1")]}),
    );
    assert_eq!(status(&doc, post), FeatureStatus::Ok);
    assert_eq!(uids(&doc), [format!("{revolve}.b0")]);
    let circular = |compute: &str| {
        json!({"type": "circular_pattern", "objects": {"type": "features", "features": [post]},
               "axis": {"type": "origin", "axis": "z"}, "quantity": 6,
               "angle": std::f64::consts::TAU, "compute": compute})
    };
    let joins = doc.kernel().count("join");
    let pattern = add(&mut doc, circular("identical"));
    assert_eq!(status(&doc, pattern), FeatureStatus::Ok);
    // The copies join the merged rings, once.
    assert_eq!(doc.kernel().count("join"), joins + 1);
    assert_eq!(uids(&doc), [format!("{revolve}.b0")]);
    let history = shape(&doc, &format!("{revolve}.b0")).history;
    assert!(
        history.contains("inst1") && history.contains("inst5"),
        "{history}"
    );

    // The participants the other way round: the upper ring keeps its id,
    // and the copies go to it.
    doc.edit_feature(
        post,
        &def(
            json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                   "extent": {"type": "distance", "distance": 105}, "operation": "join",
                   "participants": [format!("{revolve}.b1"), format!("{revolve}.b0")]}),
        ),
    )
    .unwrap();
    assert_eq!(status(&doc, pattern), FeatureStatus::Ok);
    assert_eq!(uids(&doc), [format!("{revolve}.b1")]);
    let history = shape(&doc, &format!("{revolve}.b1")).history;
    assert!(history.contains("inst5"), "{history}");

    // Adjust rebuilds a bridge up to a plane at each copy, without the
    // participant it merged: two blocks of one extrusion (x 0..10 and
    // 50..60) and a bridge between them, joined to both, repeated along y.
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let mut blocks = Vec::new();
    for x in [0.0, 50.0] {
        let region = doc
            .add_rectangle(sketch, [x, 0.0], &num(10.0), &num(40.0))
            .unwrap()
            .region;
        blocks.push(json!({"sketch": sketch, "region": region}));
    }
    let pair = add(
        &mut doc,
        json!({"type": "extrude", "profiles": blocks,
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}),
    );
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [5.0, 0.0], &num(50.0), &num(5.0))
        .unwrap()
        .region;
    let bridge = add(
        &mut doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "to_object", "object": {"type": "plane",
                   "plane": {"origin": [0.0, 0.0, 20.0], "normal": [0.0, 0.0, 1.0]}}},
               "operation": "join",
               "participants": [format!("{pair}.b0"), format!("{pair}.b1")]}),
    );
    assert_eq!(uids(&doc), [format!("{pair}.b0")]);
    let pattern = add(
        &mut doc,
        json!({"type": "rectangular_pattern",
               "objects": {"type": "features", "features": [bridge]},
               "direction1": {"axis": {"type": "origin", "axis": "y"}, "quantity": 3,
                              "distance": 10},
               "distance_type": "spacing", "compute": "adjust"}),
    );
    assert_eq!(status(&doc, pattern), FeatureStatus::Ok);
    let built = doc
        .kernel()
        .specs
        .borrow()
        .iter()
        .filter(|s| s.starts_with(&format!("extrude {bridge} ")))
        .count();
    assert_eq!(built, 3);
    let history = shape(&doc, &format!("{pair}.b0")).history;
    assert!(history.contains("inst2"), "{history}");

    // A cut that left nothing of one of its participants: its copies have
    // nothing to cut there.
    let mut b = block();
    let other = second_block(&mut b.doc);
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_circle(sketch, [40.0, 10.0], &num(5.0))
        .unwrap()
        .region;
    *b.doc.kernel().touch.borrow_mut() = Some(vec![false, true]);
    b.doc.kernel().split.set(Some(0));
    let cut = add(
        &mut b.doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "through_all"}, "operation": "cut",
               "participants": ["F2.b0", format!("{other}.b0")]}),
    );
    *b.doc.kernel().touch.borrow_mut() = None;
    b.doc.kernel().split.set(None);
    assert_eq!(status(&b.doc, cut), FeatureStatus::Ok);
    assert_eq!(uids(&b.doc), ["F2.b0"]);
    let pattern = add(
        &mut b.doc,
        json!({"type": "rectangular_pattern", "objects": {"type": "features", "features": [cut]},
               "direction1": {"axis": {"type": "origin", "axis": "y"}, "quantity": 2,
                              "distance": 20},
               "distance_type": "spacing", "compute": "identical"}),
    );
    assert!(
        matches!(status(&b.doc, pattern), FeatureStatus::Failed(m)
            if m.contains(&format!("{other}.b0 of Extrude3 was consumed by Extrude3 itself"))),
        "{:?}",
        status(&b.doc, pattern)
    );
}

// Copies joined to their bodies and scaled copies (mitcad#539).

/// Whether a body has these bounds.
fn has_box(doc: &Document<MockKernel>, min: [f64; 3], max: [f64; 3]) -> bool {
    let near = |a: [f64; 3], e: [f64; 3]| (0..3).all(|i| (a[i] - e[i]).abs() < 1e-9);
    doc.bodies().iter().any(|body| {
        doc.body_shape(body.uid)
            .and_then(|s| s.bounds)
            .is_some_and(|b| near(b.min, min) && near(b.max, max))
    })
}

/// A box moved by `by` and scaled by `s` about its own centre.
fn moved_scaled(b: BoundingBox, by: [f64; 3], s: f64) -> ([f64; 3], [f64; 3]) {
    let center: [f64; 3] = std::array::from_fn(|i| (b.min[i] + b.max[i]) / 2.0 + by[i]);
    let half: [f64; 3] = std::array::from_fn(|i| (b.max[i] - b.min[i]) / 2.0 * s);
    (
        std::array::from_fn(|i| center[i] - half[i]),
        std::array::from_fn(|i| center[i] + half[i]),
    )
}

/// A peg of its own body: a circle at (100, 0) extruded 10 (Sketch2 F3,
/// Extrude2 F4).
fn peg(doc: &mut Document<MockKernel>) -> FeatureUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_circle(sketch, [100.0, 0.0], &num(10.0))
        .unwrap()
        .region;
    add(
        doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}),
    )
}

#[test]
fn patterns_of_bodies_join_their_copies_with_combine() {
    let mut b = block();
    // Copies 30 apart overlap the 60 long block: all joined to it.
    let row = add(&mut b.doc, x_pattern(3.0, 30.0, json!({"combine": true})));
    assert_eq!(status(&b.doc, row), FeatureStatus::Ok);
    assert_eq!(uids(&b.doc), ["F2.b0"]);
    assert!(shape(&b.doc, "F2.b0").history.starts_with("join("));
    // A pattern of the joined body copies the earlier copies too (the
    // FreeCAD import's MultiTransform of the whole body).
    let turn = add(
        &mut b.doc,
        json!({"type": "circular_pattern", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "axis": {"type": "origin", "axis": "z"}, "quantity": 2,
               "angle": std::f64::consts::TAU, "combine": true}),
    );
    assert_eq!(status(&b.doc, turn), FeatureStatus::Ok);
    assert_eq!(uids(&b.doc), ["F2.b0"]);
    let history = shape(&b.doc, "F2.b0").history;
    assert!(history.starts_with("join(join(join("), "{history}");
    // A copy that touches nothing stays a body of its own.
    *b.doc.kernel().touch.borrow_mut() = Some(vec![false]);
    let apart = add(&mut b.doc, x_pattern(2.0, 500.0, json!({"combine": true})));
    *b.doc.kernel().touch.borrow_mut() = None;
    assert_eq!(status(&b.doc, apart), FeatureStatus::Ok);
    assert_eq!(uids(&b.doc), ["F2.b0".to_owned(), format!("{apart}.b0")]);
    // Features are not joined so.
    let (cut, _) = hole(&mut b.doc);
    let error = b
        .doc
        .add_feature(
            &def(json!({"type": "rectangular_pattern",
                       "objects": {"type": "features", "features": [cut]},
                       "direction1": {"axis": "x", "quantity": 2, "distance": 20},
                       "distance_type": "spacing", "combine": true})),
            None,
        )
        .unwrap_err();
    assert!(error.to_string().contains("bodies only"), "{error}");
}

/// A pattern along x of the block (A, F2.b0) and the peg (B, F4.b0)
/// together, with `combine` and the given extra options.
fn block_and_peg_row(distance: f64, extra: Value) -> Value {
    let mut value = x_pattern(2.0, distance, extra);
    value["objects"] = json!({"type": "bodies", "bodies": ["F2.b0", "F4.b0"]});
    value["combine"] = json!(true);
    value
}

fn sorted(mut items: Vec<String>) -> Vec<String> {
    items.sort();
    items
}

#[test]
fn combine_any_joins_a_copy_with_every_patterned_body_it_touches() {
    // mitcad#1006: the block A (x 0..60) and the peg B (about x = 100) in
    // one pattern, 70 along x: A's copy (x 70..130) stays away from A but
    // reaches B; B's copy (x about 170) reaches nothing.
    let mut b = block();
    let peg = peg(&mut b.doc);
    assert_eq!(peg.to_string(), "F4");
    // With `combine` alone a copy joins only its own original: A's copy
    // does not touch A (the mock's join is told so) and stays a body.
    *b.doc.kernel().touch.borrow_mut() = Some(vec![false]);
    let row = add(&mut b.doc, block_and_peg_row(70.0, json!({})));
    *b.doc.kernel().touch.borrow_mut() = None;
    assert_eq!(status(&b.doc, row), FeatureStatus::Ok);
    assert_eq!(sorted(uids(&b.doc)), ["F2.b0", "F4.b0", "F5.b0", "F5.b1"]);
    // With `combine_any` A's copy joins B, the body it reaches.
    b.doc
        .edit_feature(
            row,
            &def(block_and_peg_row(70.0, json!({"combine_any": true}))),
        )
        .unwrap();
    assert_eq!(status(&b.doc, row), FeatureStatus::Ok);
    assert_eq!(sorted(uids(&b.doc)), ["F2.b0", "F4.b0", "F5.b1"]);
    let joined = shape(&b.doc, "F4.b0").history;
    assert!(joined.starts_with("join("), "{joined}");
    assert!(joined.contains("inst1"), "{joined}");
    assert!(!shape(&b.doc, "F2.b0").history.starts_with("join("));
    let reach = bounds(&b.doc, "F4.b0");
    assert!((reach.min[0] - 70.0).abs() < 1e-9, "{reach:?}");
    assert!((reach.max[0] - 130.0).abs() < 1e-9, "{reach:?}");
    // 50 along x A's copy (x 50..110) touches A and reaches B: all three
    // join into A, the first patterned body, and B goes.
    b.doc
        .edit_feature(
            row,
            &def(block_and_peg_row(50.0, json!({"combine_any": true}))),
        )
        .unwrap();
    assert_eq!(status(&b.doc, row), FeatureStatus::Ok);
    assert_eq!(sorted(uids(&b.doc)), ["F2.b0", "F5.b1"]);
    let merged = shape(&b.doc, "F2.b0");
    assert!(merged.history.starts_with("join("), "{}", merged.history);
    assert!(merged.history.contains("inst1"), "{}", merged.history);
    assert!(
        merged
            .faces
            .iter()
            .any(|f| f.to_string().starts_with("F4:")),
        "{:?}",
        merged.faces
    );
    assert_bounds(
        &b.doc,
        "F2.b0",
        [0.0, bounds(&b.doc, "F2.b0").min[1], 0.0],
        [110.0, 40.0, 20.0],
    );
    // `combine_any` needs `combine`.
    let mut alone = block_and_peg_row(50.0, json!({"combine_any": true}));
    alone["combine"] = json!(false);
    let error = b.doc.add_feature(&def(alone), None).unwrap_err();
    assert!(
        error.to_string().contains("combine_any needs combine"),
        "{error}"
    );
}

#[test]
fn scale_groups_scale_runs_of_elements_alike() {
    let mut b = block();
    b.doc.kernel().measures.set(true);
    // Four elements in two runs: elements 0 and 1 as they are, 2 and 3
    // twice the size about their centres (the block is 60 x 40 x 20).
    let uid = add(
        &mut b.doc,
        x_pattern(4.0, 100.0, json!({"scale": 2, "scale_groups": 2})),
    );
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_bounds(&b.doc, "F3.b0", [100.0, 0.0, 0.0], [160.0, 40.0, 20.0]);
    assert_bounds(&b.doc, "F3.b1", [170.0, -20.0, -10.0], [290.0, 60.0, 30.0]);
    assert_bounds(&b.doc, "F3.b2", [270.0, -20.0, -10.0], [390.0, 60.0, 30.0]);
    // Runs that do not divide the elements fail the pattern; groups need
    // a scale and two runs at least.
    b.doc
        .edit_feature(
            uid,
            &def(x_pattern(
                4.0,
                100.0,
                json!({"scale": 2, "scale_groups": 3}),
            )),
        )
        .unwrap();
    assert!(
        matches!(status(&b.doc, uid), FeatureStatus::Failed(m) if m.contains("do not divide")),
        "{:?}",
        status(&b.doc, uid)
    );
    b.doc
        .edit_feature(
            uid,
            &def(x_pattern(
                4.0,
                100.0,
                json!({"scale": 2, "scale_groups": 5}),
            )),
        )
        .unwrap();
    assert!(matches!(status(&b.doc, uid), FeatureStatus::Failed(_)));
    let error = b
        .doc
        .edit_feature(uid, &def(x_pattern(4.0, 100.0, json!({"scale_groups": 2}))))
        .unwrap_err();
    assert!(error.to_string().contains("need a scale"), "{error}");
    let error = b
        .doc
        .edit_feature(
            uid,
            &def(x_pattern(
                4.0,
                100.0,
                json!({"scale": 2, "scale_groups": 1}),
            )),
        )
        .unwrap_err();
    assert!(error.to_string().contains("at least 2"), "{error}");
}

#[test]
fn scaled_patterns_of_patterns_scale_each_copy_about_its_own_centre() {
    let mut b = block();
    b.doc.kernel().measures.set(true);
    let peg = peg(&mut b.doc);
    let original = bounds(&b.doc, &format!("{peg}.b0"));
    let row = add(
        &mut b.doc,
        json!({"type": "rectangular_pattern", "objects": {"type": "features", "features": [peg]},
               "direction1": {"axis": "x", "quantity": 2, "distance": 50},
               "distance_type": "spacing", "compute": "identical"}),
    );
    // The row moved along y and scaled twice: the peg's copy and the copy
    // of the row's copy, each about its own centre (FreeCAD's Scaled
    // transformation after two patterns).
    let grown = add(
        &mut b.doc,
        json!({"type": "rectangular_pattern", "objects": {"type": "features", "features": [row]},
               "direction1": {"axis": "y", "quantity": 2, "distance": 50},
               "distance_type": "spacing", "compute": "identical", "scale": 2}),
    );
    assert_eq!(status(&b.doc, grown), FeatureStatus::Ok);
    for dx in [0.0, 50.0] {
        let (min, max) = moved_scaled(original, [dx, 50.0, 0.0], 2.0);
        assert!(has_box(&b.doc, min, max), "{dx}: {min:?} .. {max:?}");
    }
    // A pattern of the scaled pattern places those copies as they are.
    let raised = add(
        &mut b.doc,
        json!({"type": "rectangular_pattern", "objects": {"type": "features", "features": [grown]},
               "direction1": {"axis": "z", "quantity": 2, "distance": 100},
               "distance_type": "spacing", "compute": "identical"}),
    );
    assert_eq!(status(&b.doc, raised), FeatureStatus::Ok);
    for dx in [0.0, 50.0] {
        let (min, max) = moved_scaled(original, [dx, 50.0, 100.0], 2.0);
        assert!(has_box(&b.doc, min, max), "{dx}: {min:?} .. {max:?}");
    }
    let (min, max) = moved_scaled(original, [50.0, 0.0, 100.0], 1.0);
    assert!(has_box(&b.doc, min, max));
}

#[test]
fn scale_patterns_scale_copies_where_they_are() {
    let mut b = block();
    b.doc.kernel().measures.set(true);
    let added = b
        .doc
        .add_feature(
            &def(json!({"type": "scale_pattern",
                        "objects": {"type": "bodies", "bodies": ["F2.b0"]},
                        "quantity": 3, "scale": 2})),
            None,
        )
        .unwrap();
    assert_eq!(added.name, "ScalePattern1");
    assert_eq!(status(&b.doc, added.uid), FeatureStatus::Ok);
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0", "F3.b1"]);
    // 1.5 and 2 times about the block's centre (30, 20, 10).
    assert_bounds(&b.doc, "F3.b0", [-15.0, -10.0, -5.0], [75.0, 50.0, 25.0]);
    assert_bounds(&b.doc, "F3.b1", [-30.0, -20.0, -10.0], [90.0, 60.0, 30.0]);
    let message = b
        .doc
        .edit_feature(
            added.uid,
            &def(json!({"type": "scale_pattern",
                        "objects": {"type": "bodies", "bodies": ["F2.b0"]},
                        "quantity": 3, "scale": 0})),
        )
        .unwrap_err()
        .to_string();
    assert!(message.contains("must be a positive factor"), "{message}");
    b.doc.undo();

    // Of a feature, and a pattern of it: the scaled copy moved (FreeCAD's
    // Scaled transformation first in a MultiTransform).
    let peg = peg(&mut b.doc);
    let original = bounds(&b.doc, &format!("{peg}.b0"));
    let shrunk = add(
        &mut b.doc,
        json!({"type": "scale_pattern", "objects": {"type": "features", "features": [peg]},
               "quantity": 2, "scale": 0.5, "compute": "identical"}),
    );
    assert_eq!(status(&b.doc, shrunk), FeatureStatus::Ok);
    let (min, max) = moved_scaled(original, [0.0; 3], 0.5);
    assert!(has_box(&b.doc, min, max));
    let row = add(
        &mut b.doc,
        json!({"type": "rectangular_pattern", "objects": {"type": "features", "features": [shrunk]},
               "direction1": {"axis": "x", "quantity": 2, "distance": 50},
               "distance_type": "spacing", "compute": "identical"}),
    );
    assert_eq!(status(&b.doc, row), FeatureStatus::Ok);
    let (min, max) = moved_scaled(original, [50.0, 0.0, 0.0], 0.5);
    assert!(has_box(&b.doc, min, max));
    let (min, max) = moved_scaled(original, [50.0, 0.0, 0.0], 1.0);
    assert!(has_box(&b.doc, min, max));
}

#[test]
fn a_mirror_scales_its_image_about_the_image_s_centre() {
    let mut b = block();
    b.doc.kernel().measures.set(true);
    let mirror = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "plane": {"type": "origin", "plane": "yz"}, "scale": 0.5}),
    );
    assert_eq!(status(&b.doc, mirror), FeatureStatus::Ok);
    // The image [-60, 0] x [0, 40] x [0, 20] halved about (-30, 20, 10).
    assert_bounds(&b.doc, "F3.b0", [-45.0, 10.0, 5.0], [-15.0, 30.0, 15.0]);
}

// Scales counted over the copies of patterns of patterns, several scales,
// a scale centre and skipped copies (mitcad#542).

/// A rectangular pattern of features along an origin axis.
fn row(features: Value, axis: &str, quantity: f64, distance: f64, extra: Value) -> Value {
    let mut value = json!({"type": "rectangular_pattern", "objects": {"type": "features",
                                                                      "features": features},
        "direction1": {"axis": {"type": "origin", "axis": axis}, "quantity": quantity,
                       "distance": distance},
        "distance_type": "spacing", "compute": "identical"});
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    value
}

#[test]
fn scale_runs_over_the_copies_of_patterns_of_patterns() {
    let mut b = block();
    b.doc.kernel().measures.set(true);
    let peg = peg(&mut b.doc);
    let original = bounds(&b.doc, &format!("{peg}.b0"));
    // Four runs over the four copies of two patterns of two (FreeCAD's
    // Scaled transformation of more occurrences than the last pattern has
    // elements): the inner pattern scales its copy (j = 1) and the outer
    // one each of its copies (j = 2, 3) to its run's factor, 1 to 2.
    let runs = json!({"scale": 2, "scale_groups": 4, "scale_copies": 4});
    let inner = add(&mut b.doc, row(json!([peg]), "x", 2.0, 50.0, runs.clone()));
    let outer = add(&mut b.doc, row(json!([inner]), "y", 2.0, 50.0, runs));
    assert_eq!(status(&b.doc, outer), FeatureStatus::Ok);
    for (by, s) in [
        ([50.0, 0.0, 0.0], 4.0 / 3.0),
        ([0.0, 50.0, 0.0], 5.0 / 3.0),
        ([50.0, 50.0, 0.0], 2.0),
    ] {
        let (min, max) = moved_scaled(original, by, s);
        assert!(has_box(&b.doc, min, max), "{by:?} {s}: {min:?} .. {max:?}");
    }
    // The copies counted must hold the pattern's own.
    b.doc
        .edit_feature(
            outer,
            &def(row(
                json!([inner]),
                "y",
                2.0,
                50.0,
                json!({"scale": 2, "scale_groups": 2, "scale_copies": 2}),
            )),
        )
        .unwrap();
    assert!(
        matches!(status(&b.doc, outer), FeatureStatus::Failed(m) if m.contains("more than the")),
        "{:?}",
        status(&b.doc, outer)
    );
}

#[test]
fn several_scales_multiply_and_a_scale_centre_is_carried() {
    let mut b = block();
    b.doc.kernel().measures.set(true);
    // Two scales of four elements: in two runs up to 2 (1, 1, 2, 2) and by
    // element up to 1.5 (1, 7/6, 4/3, 3/2): element 1 is 7/6 the size.
    let uid = add(
        &mut b.doc,
        x_pattern(
            4.0,
            100.0,
            json!({"scale": 2, "scale_groups": 2, "scales": [{"scale": 1.5}]}),
        ),
    );
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let block_box = bounds(&b.doc, "F2.b0");
    let (min, max) = moved_scaled(block_box, [100.0, 0.0, 0.0], 7.0 / 6.0);
    assert_bounds(&b.doc, "F3.b0", min, max);
    let (min, max) = moved_scaled(block_box, [300.0, 0.0, 0.0], 3.0);
    assert_bounds(&b.doc, "F3.b2", min, max);
    let answer: Value = serde_json::from_str(
        &b.doc
            .query(&json!({"query": "feature", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert!(answer["def"]["scales"][0]["scale"].is_string(), "{answer}");
    b.doc.undo();
    // About the origin carried to the copy (FreeCAD's whole body): the
    // block at x = 100 twice the size from its corner (100, 0, 0).
    let uid = add(
        &mut b.doc,
        x_pattern(
            2.0,
            100.0,
            json!({"scale": 2, "scale_center": {"type": "origin"}}),
        ),
    );
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_bounds(&b.doc, "F3.b0", [100.0, 0.0, 0.0], [220.0, 80.0, 40.0]);
    // A centre without a scale is refused.
    let error = b
        .doc
        .edit_feature(
            uid,
            &def(x_pattern(
                2.0,
                100.0,
                json!({"scale_center": {"type": "origin"}}),
            )),
        )
        .unwrap_err();
    assert!(error.to_string().contains("needs a scale"), "{error}");
}

#[test]
fn skipped_copies_are_not_made_but_patterns_of_the_pattern_place_them() {
    let mut b = block();
    let peg = peg(&mut b.doc);
    let original = bounds(&b.doc, &format!("{peg}.b0"));
    // Three along x without its copy 1, then that row along y: the row's
    // copy 1 is made at y = 50 only.
    let inner = add(
        &mut b.doc,
        row(json!([peg]), "x", 3.0, 50.0, json!({"skipped_copies": [1]})),
    );
    assert_eq!(status(&b.doc, inner), FeatureStatus::Ok);
    let at = |by: [f64; 3]| moved_scaled(original, by, 1.0);
    let (min, max) = at([50.0, 0.0, 0.0]);
    assert!(!has_box(&b.doc, min, max));
    let (min, max) = at([100.0, 0.0, 0.0]);
    assert!(has_box(&b.doc, min, max));
    let outer = add(&mut b.doc, row(json!([inner]), "y", 2.0, 50.0, json!({})));
    assert_eq!(status(&b.doc, outer), FeatureStatus::Ok);
    for by in [[0.0, 50.0, 0.0], [50.0, 50.0, 0.0], [100.0, 50.0, 0.0]] {
        let (min, max) = at(by);
        assert!(has_box(&b.doc, min, max), "{by:?}");
    }
    // The outer pattern skips copies by their numbers p + 3 e: copy 4 is
    // the inner copy 1 at element 1.
    b.doc
        .edit_feature(
            outer,
            &def(row(
                json!([inner]),
                "y",
                2.0,
                50.0,
                json!({"skipped_copies": [4]}),
            )),
        )
        .unwrap();
    assert_eq!(status(&b.doc, outer), FeatureStatus::Ok);
    let (min, max) = at([50.0, 50.0, 0.0]);
    assert!(!has_box(&b.doc, min, max));
    let (min, max) = at([100.0, 50.0, 0.0]);
    assert!(has_box(&b.doc, min, max));
    // Copies of bodies too (mitcad#547): the body of element 1 is not
    // made, element 2's keeps its id. Never the original.
    let skipping = add(
        &mut b.doc,
        x_pattern(3.0, 100.0, json!({"skipped_copies": [1]})),
    );
    assert_eq!(status(&b.doc, skipping), FeatureStatus::Ok);
    let ids = uids(&b.doc);
    assert!(!ids.contains(&format!("{skipping}.b0")), "{ids:?}");
    assert!(ids.contains(&format!("{skipping}.b1")), "{ids:?}");
    let error = b
        .doc
        .edit_feature(
            outer,
            &def(row(
                json!([inner]),
                "y",
                2.0,
                50.0,
                json!({"skipped_copies": [0]}),
            )),
        )
        .unwrap_err();
    assert!(error.to_string().contains("the original"), "{error}");
}

#[test]
fn patterns_of_patterns_of_bodies_copy_the_body_at_every_product() {
    // mitcad#547 (FreeCAD's MultiTransform of the whole body): a row of the
    // block joined to it, then a pattern of the row joining its copies of
    // the block as it was before the row.
    let mut b = block();
    b.doc.kernel().measures.set(true);
    let original = bounds(&b.doc, "F2.b0");
    let row = add(&mut b.doc, x_pattern(2.0, 30.0, json!({"combine": true})));
    let y_row = |extra: Value| {
        let mut value = json!({"type": "rectangular_pattern",
            "objects": {"type": "features", "features": [row]},
            "direction1": {"axis": {"type": "origin", "axis": "y"}, "quantity": 2,
                           "distance": 200},
            "distance_type": "spacing"});
        for (key, item) in extra.as_object().unwrap() {
            value[key] = item.clone();
        }
        value
    };
    let grid = add(&mut b.doc, y_row(json!({"combine": true})));
    assert_eq!(status(&b.doc, grid), FeatureStatus::Ok);
    assert_eq!(uids(&b.doc), ["F2.b0"]);
    assert!(shape(&b.doc, "F2.b0").history.starts_with("join("));
    // Without combine: new bodies, the block at both places of the row.
    b.doc.edit_feature(grid, &def(y_row(json!({})))).unwrap();
    assert_eq!(status(&b.doc, grid), FeatureStatus::Ok);
    for dx in [0.0, 30.0] {
        let (min, max) = moved_scaled(original, [dx, 200.0, 0.0], 1.0);
        assert!(has_box(&b.doc, min, max), "{dx}: {min:?} .. {max:?}");
    }
    // Scaled about the origin carried to each copy, the row's copy 1 at
    // the element (copy 3) skipped: the block twice its size from its
    // corner (0, 200, 0) only.
    b.doc
        .edit_feature(
            grid,
            &def(y_row(
                json!({"scale": 2, "scale_center": {"type": "origin"},
                              "skipped_copies": [3]}),
            )),
        )
        .unwrap();
    assert_eq!(status(&b.doc, grid), FeatureStatus::Ok);
    let ids: Vec<String> = uids(&b.doc)
        .into_iter()
        .filter(|id| id.starts_with(&format!("{grid}.")))
        .collect();
    assert_eq!(ids.len(), 1, "{ids:?}");
    assert!(has_box(&b.doc, [0.0, 200.0, 0.0], [120.0, 280.0, 40.0]));
    // Patterns of bodies and features together are refused.
    let (cut, _) = hole(&mut b.doc);
    let mixed = add(
        &mut b.doc,
        json!({"type": "rectangular_pattern",
               "objects": {"type": "features", "features": [row, cut]},
               "direction1": {"axis": {"type": "origin", "axis": "y"}, "quantity": 2,
                              "distance": 10},
               "distance_type": "spacing"}),
    );
    assert!(
        matches!(status(&b.doc, mixed), FeatureStatus::Failed(m) if m.contains("together")),
        "{:?}",
        status(&b.doc, mixed)
    );
}

/// A pattern of the block's body with `quantity`: rectangular along x,
/// circular about z (a full turn) or along the rectangle's line c1.
fn pattern_with(kind: &str, quantity: Value) -> Value {
    let objects = json!({"type": "bodies", "bodies": ["F2.b0"]});
    match kind {
        "rectangular" => json!({"type": "rectangular_pattern", "objects": objects,
            "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": quantity,
                           "distance": 100},
            "distance_type": "spacing"}),
        "circular" => json!({"type": "circular_pattern", "objects": objects,
            "axis": {"type": "origin", "axis": "z"}, "quantity": quantity,
            "angle": std::f64::consts::TAU}),
        _ => json!({"type": "path_pattern", "objects": objects,
            "path": {"type": "sketch_curve", "sketch": "F1", "curve": "c1"},
            "quantity": quantity, "distance": 60, "distance_type": "extent"}),
    }
}

#[test]
fn pattern_quantities_are_unitless_counts_in_every_document_unit() {
    // mitcad#662: the same copies in mm, cm and inch documents, whether the
    // quantity is a number, an expression of parameters or a unitless user
    // parameter; the pattern's own parameter is `3`, not `3 mm`.
    use mitcad_model::expr::LengthUnit;
    for unit in [
        LengthUnit::Millimetre,
        LengthUnit::Centimetre,
        LengthUnit::Inch,
    ] {
        for kind in ["rectangular", "circular", "path"] {
            for (quantity, own) in [
                (json!(3), Some("3")),
                (json!("d1 / 20 mm"), Some("d1 / 20 mm")),
                (json!("n * 2 - 3"), Some("n * 2 - 3")),
                (json!("n"), None),
            ] {
                let what = format!("{unit} {kind} {quantity}");
                let mut b = block();
                b.doc.set_units(unit).unwrap();
                b.doc
                    .add_parameter_expression("n", "3", None, "count")
                    .unwrap();
                let added = b
                    .doc
                    .add_feature(&def(pattern_with(kind, quantity)), None)
                    .unwrap_or_else(|e| panic!("{what}: {e}"));
                assert_eq!(status(&b.doc, added.uid), FeatureStatus::Ok, "{what}");
                assert_eq!(b.doc.bodies().len(), 3, "{what}");
                let params = b.doc.parameters();
                let n = params.get(params.find("n").unwrap()).unwrap();
                assert!(n.unit().is_unitless(), "{what}");
                if let Some(expression) = own {
                    // The quantity's parameter is made first.
                    let id = params.find(&added.parameters[0]).unwrap();
                    let param = params.get(id).unwrap();
                    assert!(param.unit().is_unitless(), "{what}: {}", param.unit());
                    assert_eq!(param.expression(), expression, "{what}");
                    assert_eq!(param.value(), 3.0, "{what}");
                }
            }
        }
    }
}

#[test]
fn a_length_or_angle_in_a_count_says_how_to_write_a_number() {
    // mitcad#662: `d1 / 3` is a length, as a new quantity and in Change
    // Parameters, with the expression that gives a number.
    let mut b = block();
    let error = b
        .doc
        .add_feature(&def(pattern_with("rectangular", json!("d1 / 3"))), None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("the result is a length (20 mm); for a count write `d1 / 3 mm`"),
        "{error}"
    );
    let error = b
        .doc
        .add_feature(&def(pattern_with("circular", json!("30 deg"))), None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("the result is an angle (30 deg); for a count write `30`"),
        "{error}"
    );
    let added = b
        .doc
        .add_feature(&def(pattern_with("rectangular", json!(3))), None)
        .unwrap();
    let quantity = added.parameters[0].clone();
    let error = b
        .doc
        .set_parameter_expression(&quantity, "d1 / 3", None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("the result is a length (20 mm); for a count write `d1 / 3 mm`"),
        "{error}"
    );
    b.doc
        .set_parameter_expression(&quantity, "d1 / 30 mm", None)
        .unwrap();
    assert_eq!(b.doc.bodies().len(), 2);
    // A length parameter is no count either.
    let error = b
        .doc
        .edit_feature(added.uid, &def(pattern_with("rectangular", json!("d1"))))
        .unwrap_err()
        .to_string();
    assert!(error.contains("'d1' is a length"), "{error}");

    // In an inch document the hint is in inches.
    let mut b = block();
    b.doc
        .set_units(mitcad_model::expr::LengthUnit::Inch)
        .unwrap();
    let error = b
        .doc
        .add_feature(&def(pattern_with("path", json!("d1 / 3"))), None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("the result is a length (0.787402 in); for a count write `d1 / 3 in`"),
        "{error}"
    );
}

#[test]
fn a_count_that_is_not_whole_is_an_error_with_a_hint() {
    // mitcad#662: no silent rounding; floor or round make it whole.
    let mut b = block();
    for quantity in [json!(2.5), json!("d1 / 25 mm")] {
        let error = b
            .doc
            .add_feature(&def(pattern_with("rectangular", quantity)), None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("whole number"), "{error}");
        assert!(
            error.contains("use floor(…) or round(…) for a whole number"),
            "{error}"
        );
    }
    let added = b
        .doc
        .add_feature(
            &def(pattern_with("rectangular", json!("floor(d1 / 25 mm)"))),
            None,
        )
        .unwrap();
    assert_eq!(status(&b.doc, added.uid), FeatureStatus::Ok);
    assert_eq!(b.doc.bodies().len(), 2);
    // Below one is no count at all: no hint.
    let error = b
        .doc
        .edit_feature(added.uid, &def(pattern_with("rectangular", json!(0))))
        .unwrap_err()
        .to_string();
    assert!(error.contains("at least 1"), "{error}");
    assert!(!error.contains("floor"), "{error}");
}

#[test]
fn a_construction_point_s_fraction_is_unitless() {
    // mitcad#662: a fraction of a path is a number in every document unit;
    // a physical distance is a length.
    let mut b = block();
    b.doc
        .set_units(mitcad_model::expr::LengthUnit::Inch)
        .unwrap();
    let path = json!({"type": "sketch_curve", "sketch": "F1", "curve": "c1"});
    let added = b
        .doc
        .add_feature(
            &def(json!({"type": "construction_point", "definition":
                {"type": "along_path", "path": path, "distance": 0.5}})),
            None,
        )
        .unwrap();
    let params = b.doc.parameters();
    let fraction = params
        .get(params.find(&added.parameters[0]).unwrap())
        .unwrap();
    assert!(fraction.unit().is_unitless());
    assert_eq!(fraction.expression(), "0.5");
    assert_eq!(fraction.comment(), "Point1 fraction");
    let created = b
        .doc
        .edit_feature(
            added.uid,
            &def(json!({"type": "construction_point", "definition":
                {"type": "along_path", "path": path, "distance": 25.4, "physical": true}})),
        )
        .unwrap();
    let params = b.doc.parameters();
    let length = params.get(params.find(&created[0]).unwrap()).unwrap();
    assert_eq!(length.unit(), mitcad_model::expr::Unit::IN);
    assert_eq!(length.value(), 25.4);
}

/// Primitive positions take values like their sizes (mitcad#689): an
/// expression becomes a parameter of the feature and moves the primitive
/// when the parameters it uses change; a plain number stays a number, as
/// stored documents and imports have it, and both load from a file.
#[test]
fn primitive_positions_take_expressions_and_follow_parameters() {
    use mitcad_model::features::FeatureDef;
    use mitcad_model::features::primitive::Coord;

    let mut doc = Document::new(MockKernel::default());
    doc.add_parameter("L", 40.0, "").unwrap();
    doc.add_parameter("W", 20.0, "").unwrap();
    let added = doc
        .add_feature(
            &def(
                json!({"type": "box", "plane": {"type": "origin", "plane": "xy"},
                        "corner": ["-L/2", "-W/2"], "length": "L", "width": "W", "height": 10,
                        "operation": "new_body"}),
            ),
            None,
        )
        .unwrap();
    // The corner's expressions and the height are the feature's dimensions.
    assert_eq!(added.parameters.len(), 3, "{:?}", added.parameters);
    assert_bounds(&doc, "F1.b0", [-20.0, -10.0, 0.0], [20.0, 10.0, 10.0]);
    let corner = |doc: &Document<MockKernel>| match &doc.features().next().unwrap().def {
        FeatureDef::Box(def) => def.corner.clone(),
        other => panic!("not a box: {other:?}"),
    };
    assert!(matches!(corner(&doc), [Coord::Value(_), Coord::Value(_)]));

    // A parameter change moves it.
    doc.set_parameter("L", 60.0).unwrap();
    assert_bounds(&doc, "F1.b0", [-30.0, -10.0, 0.0], [30.0, 10.0, 10.0]);

    // A cylinder centred by an expression and one by numbers.
    add(
        &mut doc,
        json!({"type": "cylinder", "plane": {"type": "origin", "plane": "xy"},
               "center": ["L/2 + 5 mm", 0], "diameter": 4, "height": 5, "operation": "new_body"}),
    );
    assert_bounds(&doc, "F2.b0", [35.0, 0.0, 0.0], [37.0, 2.0, 5.0]);
    add(
        &mut doc,
        json!({"type": "sphere", "plane": {"type": "origin", "plane": "xy"},
               "center": [7, 8], "diameter": 4, "operation": "new_body"}),
    );
    doc.set_parameter("L", 20.0).unwrap();
    assert_bounds(&doc, "F1.b0", [-10.0, -10.0, 0.0], [10.0, 10.0, 10.0]);
    assert_bounds(&doc, "F2.b0", [15.0, 0.0, 0.0], [17.0, 2.0, 5.0]);
    assert_bounds(&doc, "F3.b0", [7.0, 8.0, 0.0], [9.0, 10.0, 2.0]);

    // In the file: plain numbers as before, values by their parameters.
    let json = doc.to_json();
    let file: Value = serde_json::from_str(&json).unwrap();
    let features = file["features"].as_array().unwrap();
    assert!(
        features[0]["corner"]
            .as_array()
            .unwrap()
            .iter()
            .all(Value::is_string),
        "{}",
        features[0]
    );
    assert!(features[1]["center"][0].is_string(), "{}", features[1]);
    assert_eq!(features[1]["center"][1], json!(0.0));
    assert_eq!(features[2]["center"], json!([7.0, 8.0]));
    let mut loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json);
    assert!(matches!(
        corner(&loaded),
        [Coord::Value(_), Coord::Value(_)]
    ));
    loaded.recompute();
    assert_bounds(&loaded, "F1.b0", [-10.0, -10.0, 0.0], [10.0, 10.0, 10.0]);
    loaded.set_parameter("W", 30.0).unwrap();
    assert_bounds(&loaded, "F1.b0", [-10.0, -15.0, 0.0], [10.0, 15.0, 10.0]);

    // A document stored with plain numbers (every one before mitcad#689)
    // loads them as numbers, with no parameters of their own.
    let mut old = Document::new(MockKernel::default());
    let plain = old
        .add_feature(
            &def(
                json!({"type": "box", "plane": {"type": "origin", "plane": "xy"},
                        "corner": [5, 5], "length": 30, "width": 20, "height": 10,
                        "operation": "new_body"}),
            ),
            None,
        )
        .unwrap();
    assert_eq!(plain.parameters, vec!["d1", "d2", "d3"]);
    let json = old.to_json();
    let file: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(file["features"][0]["corner"], json!([5.0, 5.0]));
    let loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(corner(&loaded), [Coord::Number(5.0), Coord::Number(5.0)]);
    assert_eq!(loaded.to_json(), json);

    // A position that is not a number or an expression is refused.
    let error = serde_json::from_value::<FeatureDef<mitcad_model::features::ValueInput>>(
        json!({"type": "box", "plane": {"type": "origin", "plane": "xy"},
               "corner": [true, 0], "length": 1, "width": 1, "height": 1,
               "operation": "new_body"}),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("a number or an expression"),
        "{error}"
    );
}

/// The block with four join extrudes of 2 x 2 mm squares, 3, 8, 13 and
/// 18 mm from (30, 20) along x (with `touching`, the second one against
/// the first, 5 mm from it), and the names of their end faces.
fn four_joins(touching: bool) -> (Document<MockKernel>, Vec<FeatureUid>, Vec<String>) {
    let mut doc = block().doc;
    let mut features = Vec::new();
    let mut ends = Vec::new();
    for i in 0..4 {
        let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
        let x = if touching && i == 1 {
            35.0
        } else {
            33.0 + 5.0 * f64::from(i)
        };
        let region = doc
            .add_rectangle(sketch, [x, 19.0], &num(2.0), &num(2.0))
            .unwrap()
            .region;
        let extrude = add(
            &mut doc,
            json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                   "extent": {"type": "distance", "distance": 30}, "operation": "join",
                   "participants": ["F2.b0"]}),
        );
        assert_eq!(status(&doc, extrude), FeatureStatus::Ok);
        ends.push(format!("{extrude}:end({region})"));
        features.push(extrude);
    }
    (doc, features, ends)
}

#[test]
fn a_pattern_of_join_features_apart_from_each_other_joins_once() {
    // mitcad#855: the copies of features whose boxes stay apart join the
    // body in one operation (a circular pattern x6 of four joins), named
    // as the features one after another name them; features whose copies
    // meet go one after another.
    for compute in ["adjust", "identical"] {
        for touching in [false, true] {
            let (mut doc, features, ends) = four_joins(touching);
            let joins = doc.kernel().count("join");
            let pattern = add(
                &mut doc,
                json!({"type": "circular_pattern",
                       "objects": {"type": "features", "features": features},
                       "axis": {"origin": [30, 20, 0], "direction": [0, 0, 1]},
                       "quantity": 6, "angle": std::f64::consts::TAU, "compute": compute}),
            );
            let case = format!("{compute}, touching {touching}");
            assert_eq!(status(&doc, pattern), FeatureStatus::Ok, "{case}");
            // Touching: the first feature alone, then the other three.
            let expected = if touching { 2 } else { 1 };
            assert_eq!(doc.kernel().count("join") - joins, expected, "{case}");
            assert_eq!(uids(&doc), ["F2.b0"], "{case}");
            let body = shape(&doc, "F2.b0");
            for end in &ends {
                for k in 1..=5 {
                    let name: FaceName = format!("{pattern}:inst{k}({end})").parse().unwrap();
                    assert!(body.faces.contains(&name), "{case}: {name}");
                }
            }
        }
    }
}
