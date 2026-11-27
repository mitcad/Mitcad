// SPDX-License-Identifier: MIT
//! The `curvature_comb` query (mitcad#2520) on the mock kernel: a circle's
//! teeth are all 1/r times the scale long and stand outside it; a line has
//! no comb; an S-shaped spline has one inflection, at the parameter and
//! point where its bend changes sides; sketch curves come in the design's
//! coordinates or, with `in_sketch`, in their sketch's; an edge's curve
//! (the mock kernel's named curve) is combed as well; without a scale the
//! longest tooth is a quarter of the curves' box; bad inputs are refused
//! or reported per item.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::EdgeName;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn comb(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn refused(doc: &Document<MockKernel>, query: Value) -> String {
    doc.query(&query.to_string())
        .map(|r| panic!("{query} must fail, got {r}"))
        .unwrap_err()
        .to_string()
}

fn xyz(value: &Value) -> [f64; 3] {
    std::array::from_fn(|i| value[i].as_f64().unwrap_or_else(|| panic!("{value}")))
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

/// A new document with Sketch1 (F1) on `plane`.
fn with_sketch(plane: &str) -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create", "plane": plane}));
    d
}

/// The id of the sketch's curve whose solved geometry has `key`.
fn curve_with(doc: &Document<MockKernel>, key: &str) -> String {
    let s = comb(doc, json!({"query": "sketch", "uid": "F1"}));
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["geometry"].get(key).is_some())
        .and_then(|e| e["id"].as_str())
        .unwrap_or_else(|| panic!("no curve with {key} in {s}"))
        .to_owned()
}

fn item(curve: &str) -> Value {
    json!({"sketch": "F1", "curve": curve})
}

#[test]
fn a_circle_has_teeth_of_one_length_and_a_line_none() {
    let mut d = with_sketch("xy");
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [5, 5], "radius": 8}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [20, 0], "end": [40, 10]}),
    );
    let circle = curve_with(&d, "radius");
    let line = curve_with(&d, "end");
    let answer = comb(
        &d,
        json!({"query": "curvature_comb", "items": [item(&circle), item(&line)], "teeth": 24, "scale": 40}),
    );
    assert_eq!(answer["scale"], json!(40.0));
    assert_eq!(answer["inflections"], json!(0));
    let combs = answer["combs"].as_array().unwrap();
    assert_eq!(combs.len(), 2, "{answer}");
    let round = &combs[0];
    assert_eq!(round["item"], json!(0));
    assert_eq!(round["kind"], json!("sketch_curve"));
    assert_eq!(round["closed"], json!(true));
    assert!(close(round["min_radius"].as_f64().unwrap(), 8.0), "{round}");
    let teeth = round["teeth"].as_array().unwrap();
    assert_eq!(teeth.len(), 24);
    for tooth in teeth {
        // 1/r times the scale: 40 / 8 = 5 mm, outside the circle.
        let (point, tip) = (xyz(&tooth["point"]), xyz(&tooth["tip"]));
        assert!(close(distance(point, tip), 5.0), "{tooth}");
        assert!(close(distance(tip, [5.0, 5.0, 0.0]), 13.0), "{tooth}");
        assert!(close(tooth["curvature"].as_f64().unwrap(), 1.0 / 8.0));
    }
    // The line: no teeth and no inflection.
    let straight = &combs[1];
    assert_eq!(straight["straight"], json!(true), "{straight}");
    assert_eq!(straight["teeth"], json!([]));
    assert_eq!(straight["inflections"], json!([]));
    assert!(straight["min_radius"].is_null());
}

#[test]
fn an_s_spline_has_one_inflection_at_its_middle() {
    let mut d = with_sketch("xy");
    // Control points symmetric through (15, 0): the bend changes sides at
    // the parameter's middle, at that point.
    let made = command(
        &mut d,
        json!({"cmd": "sketch.spline", "sketch": "F1",
               "points": [[0, 0], [10, 10], [20, -10], [30, 0]], "degree": 3}),
    );
    let spline = made["made"][0].as_str().unwrap().to_owned();
    let answer = comb(
        &d,
        json!({"query": "curvature_comb", "items": [item(&spline)], "teeth": 31}),
    );
    assert_eq!(answer["inflections"], json!(1), "{answer}");
    let s = &answer["combs"][0];
    assert_eq!(s["closed"], json!(false));
    let inflections = s["inflections"].as_array().unwrap();
    assert_eq!(inflections.len(), 1);
    assert!(close(inflections[0]["t"].as_f64().unwrap(), 0.5), "{s}");
    assert!(distance(xyz(&inflections[0]["point"]), [15.0, 0.0, 0.0]) < 1e-6);
    // The teeth change sides there: before it they stand above the curve
    // (it bends down), after it below.
    let teeth = s["teeth"].as_array().unwrap();
    assert_eq!(teeth.len(), 31);
    let side = |tooth: &Value| xyz(&tooth["tip"])[1] - xyz(&tooth["point"])[1];
    assert!(side(&teeth[2]) > 0.0 && side(&teeth[28]) < 0.0, "{s}");
    // The middle tooth stands on the inflection: no length.
    assert!(
        teeth[15]["curvature"].as_f64().unwrap() < 1e-6,
        "{}",
        teeth[15]
    );
    // Without a scale the longest tooth is a quarter of the box's diagonal
    // (the teeth's points span 30 × the spline's height).
    let longest = teeth
        .iter()
        .map(|t| distance(xyz(&t["point"]), xyz(&t["tip"])))
        .fold(0.0, f64::max);
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for t in teeth {
        let p = xyz(&t["point"]);
        lo = std::array::from_fn(|i| lo[i].min(p[i]));
        hi = std::array::from_fn(|i| hi[i].max(p[i]));
    }
    // The curve's largest bend lies between teeth: no tooth is longer.
    assert!(longest <= distance(lo, hi) / 4.0 + 1e-9, "{longest}");
    assert!(longest > 0.9 * distance(lo, hi) / 4.0, "{longest}");
}

#[test]
fn sketch_curves_in_the_design_and_in_their_sketch() {
    let mut d = with_sketch("xz");
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [0, 0], "radius": 10}),
    );
    let circle = curve_with(&d, "radius");
    let query = |in_sketch: bool| {
        json!({"query": "curvature_comb", "items": [item(&circle)], "teeth": 8, "scale": 20,
               "in_sketch": in_sketch})
    };
    let frame = comb(&d, json!({"query": "sketch", "uid": "F1"}))["frame"].clone();
    let normal = xyz(&frame["normal"]);
    let placed = comb(&d, query(false));
    let flat = comb(&d, query(true));
    let placed_teeth = placed["combs"][0]["teeth"].as_array().unwrap().clone();
    let flat_teeth = flat["combs"][0]["teeth"].as_array().unwrap().clone();
    assert_eq!(placed_teeth.len(), 8);
    for (p, f) in placed_teeth.iter().zip(&flat_teeth) {
        // In the sketch: on z = 0; in the design: in the sketch's plane.
        assert_eq!(xyz(&f["point"])[2], 0.0);
        let point = xyz(&p["point"]);
        let tip = xyz(&p["tip"]);
        let along = |v: [f64; 3]| v[0] * normal[0] + v[1] * normal[1] + v[2] * normal[2];
        assert!(
            along(point).abs() < 1e-9 && along(tip).abs() < 1e-9,
            "{p} {normal:?}"
        );
        assert!(close(distance(point, tip), 2.0), "{p}");
        assert!(
            close(distance(xyz(&f["point"]), xyz(&f["tip"])), 2.0),
            "{f}"
        );
    }
}

/// A 60 × 40 block 20 high (Sketch1 F1, Extrude1 F2, Body1 F2.b0); one of
/// its edges by name.
fn block_edge() -> (Document<MockKernel>, String) {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    let shape = doc
        .body_shape(BodyUid::new(FeatureUid(2), 0))
        .expect("Body1")
        .clone();
    let (a, b) = shape.edges.first().expect("an edge").clone();
    (doc, EdgeName::new(a, b).to_string())
}

#[test]
fn an_edge_curve_is_combed_in_the_design() {
    let (doc, edge) = block_edge();
    // The mock kernel's curve of that edge: an S in the XZ plane at y = 5,
    // as a B-spline.
    doc.kernel().named_curves.borrow_mut().insert(
        edge.clone(),
        Curve3::BSpline {
            degree: 3,
            poles: vec![
                [0.0, 5.0, 0.0],
                [10.0, 5.0, 10.0],
                [20.0, 5.0, -10.0],
                [30.0, 5.0, 0.0],
            ],
            weights: Vec::new(),
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        },
    );
    let answer = comb(
        &doc,
        json!({"query": "curvature_comb", "items": [{"body": "F2.b0", "edge": edge}], "teeth": 20}),
    );
    let s = &answer["combs"][0];
    assert_eq!(s["kind"], json!("edge"), "{answer}");
    assert_eq!(s["teeth"].as_array().unwrap().len(), 20);
    let inflections = s["inflections"].as_array().unwrap();
    assert_eq!(inflections.len(), 1, "{s}");
    assert!(close(inflections[0]["t"].as_f64().unwrap(), 0.5));
    assert!(distance(xyz(&inflections[0]["point"]), [15.0, 5.0, 0.0]) < 1e-6);
    assert!(answer["scale"].as_f64().unwrap() > 0.0);
    // A circle edge of radius 10.
    doc.kernel().named_curves.borrow_mut().insert(
        edge.clone(),
        Curve3::Conic {
            center: [30.0, 20.0, 20.0],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major: 10.0,
            minor: 10.0,
            start: 0.0,
            end: 0.0,
            closed: true,
        },
    );
    let answer = comb(
        &doc,
        json!({"query": "curvature_comb", "items": [{"body": "F2.b0", "edge": edge}], "teeth": 12, "scale": 30}),
    );
    for tooth in answer["combs"][0]["teeth"].as_array().unwrap() {
        assert!(
            close(distance(xyz(&tooth["point"]), xyz(&tooth["tip"])), 3.0),
            "{tooth}"
        );
    }
}

#[test]
fn bad_inputs_are_refused_or_reported_per_item() {
    let mut d = with_sketch("xy");
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [0, 0], "radius": 3}),
    );
    let circle = curve_with(&d, "radius");
    let teeth = refused(
        &d,
        json!({"query": "curvature_comb", "items": [item(&circle)], "teeth": 1}),
    );
    assert!(teeth.contains("teeth must be from 2 to 1000"), "{teeth}");
    let scale = refused(
        &d,
        json!({"query": "curvature_comb", "items": [item(&circle)], "scale": -1}),
    );
    assert!(scale.contains("positive"), "{scale}");
    // A curve the sketch lacks: its entry says so; the circle is combed.
    let answer = comb(
        &d,
        json!({"query": "curvature_comb", "items": [item("c99"), item(&circle)]}),
    );
    let combs = answer["combs"].as_array().unwrap();
    assert_eq!(combs.len(), 2);
    assert_eq!(combs[0]["item"], json!(0));
    assert!(
        combs[0]["error"]
            .as_str()
            .unwrap()
            .contains("has no curve c99"),
        "{answer}"
    );
    assert_eq!(combs[1]["teeth"].as_array().unwrap().len(), 50);
}

#[test]
fn a_curve_given_by_its_geometry_is_combed_as_its_sketch_curve() {
    // Sketch mode sends the curves it read from the `sketch` query; the
    // combs are those of the sketch's curves in its coordinates.
    let mut d = with_sketch("yz");
    let made = command(
        &mut d,
        json!({"cmd": "sketch.spline", "sketch": "F1",
               "points": [[0, 0], [10, 10], [20, -10], [30, 0]], "degree": 3}),
    );
    let spline = made["made"][0].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
               "start": [40, 0], "through": [50, 10], "end": [60, 0]}),
    );
    let arc = curve_with(&d, "start_angle");
    let s = comb(&d, json!({"query": "sketch", "uid": "F1"}));
    let geometry = |id: &str| {
        s["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .unwrap()["geometry"]
            .clone()
    };
    let by_id = comb(
        &d,
        json!({"query": "curvature_comb", "items": [item(&spline), item(&arc)], "teeth": 12,
               "scale": 5, "in_sketch": true}),
    );
    let by_geometry = comb(
        &d,
        json!({"query": "curvature_comb", "teeth": 12, "scale": 5,
               "items": [{"geometry": geometry(&spline)}, {"geometry": geometry(&arc)}]}),
    );
    for k in 0..2 {
        let a = &by_id["combs"][k];
        let b = &by_geometry["combs"][k];
        assert_eq!(b["kind"], json!("geometry"), "{by_geometry}");
        assert_eq!(a["teeth"], b["teeth"], "{k}");
        assert_eq!(a["inflections"], b["inflections"], "{k}");
    }
    assert_eq!(by_geometry["inflections"], json!(1));
    // Not a curve's geometry: its entry says so.
    let answer = comb(
        &d,
        json!({"query": "curvature_comb", "items": [{"geometry": {"control": [[0, 0]]}}]}),
    );
    assert!(
        answer["combs"][0]["error"]
            .as_str()
            .unwrap()
            .contains("is not a sketch curve's geometry"),
        "{answer}"
    );
}
