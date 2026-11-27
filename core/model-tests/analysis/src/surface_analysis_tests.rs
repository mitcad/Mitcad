// SPDX-License-Identifier: MIT
//! The surface quality analyses kept in the document (mitcad#2517), with
//! the mock kernel: Zebra, Curvature Map, Isocurve Analysis and Environment
//! Map with their defaults and `Zebra<n>`, `Curvature<n>`, `Isocurves<n>`
//! and `Environment<n>` names, one shown at a time with the other analyses,
//! refused settings, faces of a body instead of the whole body, saved and
//! read back, and the `surface_analysis` query's stripes, values, curves
//! and mesh on the mock kernel's box (logged counts: the stripes' and the
//! curves').

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
/// The front face of the block: line c1 between c4 and c2.
const FRONT: &str = "F2:side(c1[c4,c2])";
const TOL: f64 = 1e-6;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn query_error(doc: &Document<MockKernel>, query: Value) -> String {
    match doc.query(&query.to_string()) {
        Ok(answer) => panic!("{query} was answered: {answer}"),
        Err(e) => e.to_string(),
    }
}

fn analyses(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "analyses"}))
        .as_array()
        .unwrap()
        .clone()
}

fn shown(doc: &Document<MockKernel>) -> Vec<bool> {
    analyses(doc)
        .iter()
        .map(|a| a["visible"].as_bool().unwrap())
        .collect()
}

fn close(value: &Value, expected: f64) -> bool {
    value
        .as_f64()
        .is_some_and(|actual| (actual - expected).abs() <= TOL)
}

/// A `width` x `depth` x `height` block (F2.b0) on a rectangle on XY.
fn block(width: f64, depth: f64, height: f64) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": width, "height": depth},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": height}, "operation": "new_body"}},
        ]),
    );
    doc
}

fn front() -> Value {
    json!({"body": "F2.b0", "face": FRONT})
}

#[test]
fn surface_analyses_take_their_defaults_and_names() {
    let mut doc = block(20.0, 20.0, 20.0);
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "zebra", "bodies": ["F2.b0"]}}),
    );
    assert_eq!(added["name"], "Zebra1");
    assert_eq!(added["recomputed"], 0, "an analysis changes no geometry");
    let zebra = &analyses(&doc)[0];
    assert_eq!(zebra["type"], "zebra");
    assert_eq!(zebra["stripes"], 12);
    assert_eq!(zebra["width"], 0.5);
    assert_eq!(zebra["direction"], Value::Null);
    assert_eq!(zebra["stripe_direction"], json!([0.0, 0.0, 1.0]));
    assert!(zebra.get("faces").is_none(), "{zebra}");
    assert!(zebra.get("error").is_none(), "{zebra}");

    for (def, name) in [
        (
            json!({"type": "curvature", "bodies": ["F2.b0"]}),
            "Curvature1",
        ),
        (
            json!({"type": "isocurve", "bodies": ["F2.b0"]}),
            "Isocurves1",
        ),
        (
            json!({"type": "environment", "bodies": ["F2.b0"]}),
            "Environment1",
        ),
        (
            json!({"type": "zebra", "bodies": ["F2.b0"], "direction": "x", "stripes": 20, "width": 0.3}),
            "Zebra2",
        ),
    ] {
        let added = command(&mut doc, json!({"cmd": "add_analysis", "def": def}));
        assert_eq!(added["name"], name);
    }
    let all = analyses(&doc);
    assert_eq!(all[1]["kind"], "gaussian");
    assert!(all[1].get("limit").is_none(), "{}", all[1]);
    assert_eq!(all[2]["count"], 10);
    assert!(all[2].get("curvature").is_none(), "{}", all[2]);
    assert!(all[3].get("image").is_none(), "{}", all[3]);
    assert_eq!(all[4]["stripe_direction"], json!([1.0, 0.0, 0.0]));
    // One shown at a time, as the other analyses.
    assert_eq!(shown(&doc), [false, false, false, false, true]);
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Curvature1", "def": {"type": "curvature",
               "bodies": ["F2.b0"], "kind": "mean", "limit": 0.5}, "visible": true}),
    );
    assert_eq!(analyses(&doc)[1]["kind"], "mean");
    assert_eq!(analyses(&doc)[1]["limit"], 0.5);
    assert_eq!(shown(&doc), [false, true, false, false, false]);
}

#[test]
fn surface_analyses_refuse_what_they_cannot_keep() {
    let mut doc = block(20.0, 20.0, 20.0);
    for (def, words) in [
        (json!({"type": "zebra"}), "needs a body or a face"),
        (
            json!({"type": "zebra", "bodies": ["F2.b0"], "stripes": 1}),
            "stripes must be from 2",
        ),
        (
            json!({"type": "zebra", "bodies": ["F2.b0"], "width": 1}),
            "stripe width",
        ),
        (
            json!({"type": "zebra", "bodies": ["F2.b0"], "direction": "origin"}),
            "stripe direction",
        ),
        (
            json!({"type": "curvature", "bodies": ["F2.b0"], "limit": 0}),
            "color limit",
        ),
        (
            json!({"type": "curvature", "bodies": ["F2.b0"], "kind": "total"}),
            "total",
        ),
        (
            json!({"type": "isocurve", "bodies": ["F2.b0"], "count": 0}),
            "iso curves per face",
        ),
        (
            json!({"type": "isocurve", "faces": ["xy"]}),
            "faces of bodies only",
        ),
        (
            json!({"type": "environment", "bodies": ["F2.b0"], "image": " "}),
            "no file name",
        ),
        (
            json!({"type": "environment", "bodies": ["F9.b0"]}),
            "does not exist",
        ),
        (
            json!({"type": "environment", "bodies": ["F2.b0"], "picture": "a.png"}),
            "picture",
        ),
    ] {
        let error = refused(&mut doc, json!({"cmd": "add_analysis", "def": def}));
        assert!(error.contains(words), "{def}: {error}");
    }
    assert!(analyses(&doc).is_empty());
}

#[test]
fn surface_analyses_on_faces_are_saved_and_read_back() {
    let mut doc = block(20.0, 20.0, 20.0);
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "isocurve", "faces": [front()],
               "count": 4, "curvature": true}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "zebra", "bodies": ["F2.b0"],
               "direction": {"body": "F2.b0", "face": format!("F2:end({R})")}, "stripes": 8}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "environment", "faces": [front()], "image": "studio.png"}}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["analyses"],
        json!([
            {"name": "Isocurves1", "visible": false, "type": "isocurve", "bodies": [],
             "faces": [front()], "count": 4, "curvature": true},
            {"name": "Zebra1", "visible": false, "type": "zebra", "bodies": ["F2.b0"],
             "direction": {"body": "F2.b0", "face": format!("F2:end({R})")}, "stripes": 8, "width": 0.5},
            {"name": "Environment1", "type": "environment", "bodies": [], "faces": [front()],
             "image": "studio.png"}
        ])
    );
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(analyses(&loaded), analyses(&doc));
}

#[test]
fn the_query_draws_zebra_stripes_over_the_box() {
    let doc = block(20.0, 20.0, 20.0);
    // Three stripes over the half turn from +Z: the top (0°) and the
    // bottom (180°) start a stripe, dark; the sides (90°) are half way
    // through one, light.
    let answer = query(
        &doc,
        json!({"query": "surface_analysis", "analysis": {"type": "zebra", "bodies": ["F2.b0"], "stripes": 3}}),
    );
    assert_eq!(answer["type"], "zebra");
    assert_eq!(answer["direction"], json!([0.0, 0.0, 1.0]));
    assert!(close(&answer["dark_area"], 800.0), "{answer}");
    assert!(close(&answer["light_area"], 1600.0), "{answer}");
    assert_eq!(answer["dark_triangles"], 4);
    assert_eq!(answer["light_triangles"], 8);
    let body = &answer["bodies"][0];
    assert_eq!(body["body"], "F2.b0");
    assert_eq!(body["faces"], 6);
    assert_eq!(body["triangles"]["dark"].as_array().unwrap().len(), 4);
    assert_eq!(body["triangles"]["dark"][0].as_array().unwrap().len(), 9);

    // A kept analysis' listing is taken as it is; the front face only.
    let listed = json!({"name": "Zebra1", "visible": true, "type": "zebra", "faces": [front()],
                        "stripes": 3, "stripe_direction": [0.0, 0.0, 1.0]});
    let face = query(
        &doc,
        json!({"query": "surface_analysis", "analysis": listed, "geometry": false}),
    );
    let area = face["dark_area"].as_f64().unwrap() + face["light_area"].as_f64().unwrap();
    assert!((area - 400.0).abs() < TOL, "{face}");
    assert_eq!(face["bodies"][0]["faces"], 1);
    assert!(face["bodies"][0].get("triangles").is_none(), "{face}");

    // Settings out of range are refused, a gone body is said in words.
    let error = query_error(
        &doc,
        json!({"query": "surface_analysis", "analysis": {"type": "zebra", "bodies": ["F2.b0"], "stripes": 500}}),
    );
    assert!(error.contains("stripes"), "{error}");
    let error = query_error(
        &doc,
        json!({"query": "surface_analysis", "analysis": {"type": "draft", "bodies": ["F2.b0"]}}),
    );
    assert!(error.contains("surface analysis"), "{error}");
    let gone = query(
        &doc,
        json!({"query": "surface_analysis", "analysis": {"type": "curvature", "bodies": ["F7.b0"]}}),
    );
    assert!(
        gone["error"].as_str().unwrap().contains("does not exist"),
        "{gone}"
    );
}

#[test]
fn the_query_gives_curvature_iso_curves_and_the_environment_mesh() {
    let doc = block(20.0, 20.0, 20.0);
    // A box is flat everywhere: every value zero, the scale's reach tiny.
    let curvature = query(
        &doc,
        json!({"query": "surface_analysis", "analysis": {"type": "curvature", "bodies": ["F2.b0"], "kind": "mean"}}),
    );
    assert_eq!(curvature["kind"], "mean");
    assert_eq!(curvature["unit"], "1/mm");
    assert_eq!(curvature["min"], 0.0);
    assert_eq!(curvature["max"], 0.0);
    assert_eq!(curvature["automatic"], true);
    assert!(curvature["limit"].as_f64().unwrap() > 0.0);
    assert_eq!(curvature["triangles"], 12);
    let mesh = &curvature["bodies"][0]["mesh"];
    assert_eq!(mesh["positions"].as_array().unwrap().len(), 24 * 3);
    assert_eq!(mesh["indices"].as_array().unwrap().len(), 12 * 3);
    assert_eq!(mesh["values"].as_array().unwrap().len(), 24);
    let limited = query(
        &doc,
        json!({"query": "surface_analysis", "analysis": {"type": "curvature", "bodies": ["F2.b0"], "kind": "gaussian", "limit": 0.01}}),
    );
    assert_eq!(limited["unit"], "1/mm^2");
    assert_eq!(limited["limit"], 0.01);
    assert_eq!(limited["automatic"], false);

    // Three curves of each parameter on each of the six faces.
    let isocurves = query(
        &doc,
        json!({"query": "surface_analysis", "analysis": {"type": "isocurve", "bodies": ["F2.b0"], "count": 3, "curvature": true}}),
    );
    assert_eq!(isocurves["curves"], 36);
    assert_eq!(isocurves["points"], 72);
    assert_eq!(isocurves["curvature"], true);
    assert!(isocurves["limit"].as_f64().unwrap() > 0.0);
    let curve = &isocurves["bodies"][0]["curves"][0];
    assert_eq!(curve["along"], "u");
    assert!(close(&curve["at"], 0.25), "{curve}");
    assert_eq!(curve["points"].as_array().unwrap().len(), 6);
    assert_eq!(curve["curvatures"], json!([0.0, 0.0]));
    let one_face = query(
        &doc,
        json!({"query": "surface_analysis", "analysis": {"type": "isocurve", "faces": [front()], "count": 2}}),
    );
    assert_eq!(one_face["curves"], 4);
    assert!(one_face.get("limit").is_none(), "{one_face}");

    let environment = query(
        &doc,
        json!({"query": "surface_analysis", "analysis": {"type": "environment", "bodies": ["F2.b0"]}}),
    );
    assert_eq!(environment["triangles"], 12);
    assert_eq!(environment["image"], Value::Null);
    let mesh = &environment["bodies"][0]["mesh"];
    assert_eq!(mesh["normals"].as_array().unwrap().len(), 24 * 3);
}
