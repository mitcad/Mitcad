// SPDX-License-Identifier: MIT
//! Print Analysis kept in the document (mitcad#620): its definition with
//! the defaults, refused fields, `Print<n>` names, the bed face followed by
//! its name and a gone one as `error`; the `print_overhangs`,
//! `print_thickness` and `print_estimate` queries on the mock kernel's box
//! meshes, their cache and their cancellation; and that the analysis
//! changes no body and no 3D print export.

use std::sync::Arc;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::monitor::RecomputeMonitor;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
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

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"))
}

fn close(value: &Value, expected: f64) -> bool {
    (number(value) - expected).abs() <= TOL
}

fn mesh_calls(doc: &Document<MockKernel>) -> usize {
    doc.kernel()
        .calls
        .borrow()
        .get("triangle_mesh")
        .copied()
        .unwrap_or(0)
}

/// A 20 x 20 x 20 block (F2.b0) on a rectangle on XY.
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    doc
}

fn top() -> Value {
    json!({"body": "F2.b0", "face": format!("F2:end({R})")})
}

fn taller(doc: &mut Document<MockKernel>, height: f64) {
    command(
        doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": R}],
               "extent": {"type": "distance", "distance": height}, "operation": "new_body"}}),
    );
}

#[test]
fn a_print_analysis_takes_the_defaults_and_print_names() {
    let mut doc = block();
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "print", "bodies": ["F2.b0"]}}),
    );
    assert_eq!(added["name"], "Print1");
    assert_eq!(added["recomputed"], 0, "an analysis changes no geometry");
    assert_eq!(undo_label(&doc), "Add Print1");
    let first = &analyses(&doc)[0];
    assert_eq!(first["type"], "print");
    assert_eq!(first["visible"], true);
    assert_eq!(first["bodies"], json!(["F2.b0"]));
    assert_eq!(first["bed_face"], Value::Null);
    assert_eq!(first["overhang"], json!({"enabled": true, "angle": 45.0}));
    assert_eq!(first["thin_wall"], json!({"enabled": true, "minimum": 0.8}));
    assert_eq!(
        first["estimate"],
        json!({"enabled": true, "material": null, "line_width": 0.4, "walls": 2,
               "layer_height": 0.2, "top_bottom_layers": 4, "infill": 0.15,
               "filament_diameter": 1.75, "price_per_kg": null})
    );
    assert_eq!(first["down"], json!([0.0, 0.0, -1.0]));
    assert!(first.get("error").is_none(), "{first}");

    // Fields given in part keep the other defaults; numbering is per type.
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "print", "bodies": ["F2.b0"],
               "overhang": {"angle": 60}, "estimate": {"material": "nylon", "enabled": false}}}),
    );
    assert_eq!(added["name"], "Print2");
    let second = &analyses(&doc)[1];
    assert_eq!(second["overhang"], json!({"enabled": true, "angle": 60.0}));
    assert_eq!(second["estimate"]["enabled"], false);
    assert_eq!(second["estimate"]["line_width"], 0.4);
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": "xy"}}),
    );
    assert_eq!(added["name"], "Section1");
    // One shown at a time, print or section.
    let shown: Vec<bool> = analyses(&doc)
        .iter()
        .map(|a| a["visible"].as_bool().unwrap())
        .collect();
    assert_eq!(shown, [false, false, true]);

    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Print1", "def": {"type": "print", "bodies": ["F2.b0"],
               "thin_wall": {"minimum": 1.2}}, "visible": true}),
    );
    assert_eq!(undo_label(&doc), "Edit Print1");
    assert_eq!(analyses(&doc)[0]["thin_wall"]["minimum"], 1.2);
    assert_eq!(analyses(&doc)[0]["visible"], true);
    command(
        &mut doc,
        json!({"cmd": "rename_analysis", "name": "Print1", "new_name": "Bracket print"}),
    );
    command(
        &mut doc,
        json!({"cmd": "delete_analysis", "name": "Print2"}),
    );
    let names: Vec<Value> = analyses(&doc).iter().map(|a| a["name"].clone()).collect();
    assert_eq!(names, ["Bracket print", "Section1"]);
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(analyses(&doc)[0]["name"], "Print1");
}

#[test]
fn a_print_analysis_refuses_what_it_cannot_keep() {
    let mut doc = block();
    let add = |def: Value| json!({"cmd": "add_analysis", "def": def});
    for (def, says) in [
        (
            json!({"type": "print", "bodies": ["F2.b0"], "depth": 3}),
            "depth",
        ),
        (
            json!({"type": "print", "bodies": ["F2.b0"], "overhang": {"angel": 30}}),
            "angel",
        ),
        (
            json!({"type": "print", "bodies": ["F2.b0"], "estimate": {"infil": 0.2}}),
            "infil",
        ),
        (json!({"type": "print", "bodies": []}), "needs a body"),
        (
            json!({"type": "print", "bodies": ["F2.b0"], "overhang": {"angle": 95}}),
            "overhang angle",
        ),
        (
            json!({"type": "print", "bodies": ["F2.b0"], "thin_wall": {"minimum": 0}}),
            "minimum wall",
        ),
        (
            json!({"type": "print", "bodies": ["F2.b0"], "estimate": {"infill": 1.5}}),
            "infill",
        ),
        (
            json!({"type": "print", "bodies": ["F2.b0"], "estimate": {"walls": 1.5}}),
            "1.5",
        ),
        (
            json!({"type": "print", "bodies": ["F2.b0"], "estimate": {"material": "cheese"}}),
            "cheese",
        ),
        (
            json!({"type": "print", "bodies": ["F2.b0"], "estimate": {"price_per_kg": -3}}),
            "price",
        ),
        (json!({"type": "print", "bodies": ["F9.b0"]}), "F9.b0"),
        (
            json!({"type": "print", "bodies": ["F2.b0"], "bed_face": "xy"}),
            "planar face of a body",
        ),
        (
            json!({"type": "print", "bodies": ["F2.b0"],
                   "bed_face": {"body": "F2.b0", "face": "F2:end(r{c9})"}}),
            "F2:end(r{c9})",
        ),
        (
            json!({"type": "print", "bodies": ["F2.b0"],
                   "bed_face": {"body": "F3.b0", "face": format!("F3:end({R})")}}),
            "does not include",
        ),
    ] {
        let error = refused(&mut doc, add(def.clone()));
        assert!(error.contains(says), "{def}: {error}");
    }
    assert!(analyses(&doc).is_empty());
}

#[test]
fn the_bed_face_follows_the_model_and_a_gone_one_is_an_error() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "print", "bodies": ["F2.b0"], "bed_face": top()}}),
    );
    // Printed on its top: the top's outward normal points down.
    assert_eq!(analyses(&doc)[0]["down"], json!([0.0, 0.0, 1.0]));
    assert_eq!(analyses(&doc)[0]["bed_face"], top());
    taller(&mut doc, 30.0);
    assert_eq!(analyses(&doc)[0]["down"], json!([0.0, 0.0, 1.0]));
    let answer = query(
        &doc,
        json!({"query": "print_overhangs", "bodies": ["F2.b0"], "bed_face": top()}),
    );
    assert_eq!(answer["down"], json!([0.0, 0.0, 1.0]));
    assert!(close(&answer["overhang_area"], 0.0), "{answer}");

    // Rolled back before the block: the analysis says why, and stays.
    command(&mut doc, json!({"cmd": "set_marker", "position": 1}));
    let rolled = &analyses(&doc)[0];
    assert!(rolled.get("down").is_none(), "{rolled}");
    assert!(
        rolled["error"].as_str().unwrap().contains("F2.b0"),
        "{rolled}"
    );
    for name in ["print_overhangs", "print_thickness", "print_estimate"] {
        let answer = query(&doc, json!({"query": name, "bodies": ["F2.b0"]}));
        assert!(
            answer["error"].as_str().unwrap().contains("F2.b0"),
            "{name}: {answer}"
        );
    }
    let answer = query(
        &doc,
        json!({"query": "print_overhangs", "bodies": [], "bed_face": top()}),
    );
    assert!(
        answer["error"].as_str().unwrap().contains("bed face"),
        "{answer}"
    );
    command(&mut doc, json!({"cmd": "set_marker", "position": 2}));
    assert!(analyses(&doc)[0].get("error").is_none());
}

#[test]
fn print_analyses_are_saved_and_read_back() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "print", "bodies": ["F2.b0"], "bed_face": top(),
               "estimate": {"material": "abs", "price_per_kg": 22.5}}}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let kept = &file["analyses"][0];
    assert_eq!(kept["name"], "Print1");
    assert_eq!(kept["type"], "print");
    assert_eq!(kept["bed_face"], top());
    assert_eq!(kept["estimate"]["material"], "abs");
    assert_eq!(kept["estimate"]["price_per_kg"], 22.5);
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(analyses(&loaded), analyses(&doc));
    let unknown = saved.replace("\"walls\": 2", "\"wall_count\": 2");
    assert_ne!(unknown, saved);
    let Err(error) = Document::from_json(&unknown, MockKernel::default()) else {
        panic!("an unknown field was read");
    };
    assert!(error.to_string().contains("analyses[0]"), "{error}");
}

#[test]
fn overhangs_of_a_box_by_direction_and_bed_face() {
    let doc = block();
    // On its bottom: nothing overhangs.
    let answer = query(
        &doc,
        json!({"query": "print_overhangs", "bodies": ["F2.b0"]}),
    );
    assert_eq!(answer["down"], json!([0.0, 0.0, -1.0]));
    assert!(close(&answer["area"], 2400.0), "{answer}");
    assert!(close(&answer["overhang_area"], 0.0), "{answer}");
    let body = &answer["bodies"][0];
    assert_eq!(body["body"], "F2.b0");
    assert!(body["name"].is_string());
    assert!(close(&body["share"], 0.0));
    assert_eq!(body["triangles"], json!([]));

    // Tilted 26.6° about Y: the bottom, on the bed only along an edge,
    // overhangs; its two triangles in design coordinates.
    let answer = query(
        &doc,
        json!({"query": "print_overhangs", "bodies": ["F2.b0"], "down": [0.5, 0, -1], "angle": 45}),
    );
    assert!(close(&answer["overhang_area"], 400.0), "{answer}");
    assert!(close(&answer["share"], 1.0 / 6.0));
    let triangles = answer["bodies"][0]["triangles"].as_array().unwrap();
    assert_eq!(triangles.len(), 2);
    for triangle in triangles {
        let numbers = triangle.as_array().unwrap();
        assert_eq!(numbers.len(), 9);
        for z in [2, 5, 8] {
            assert!(close(&numbers[z], 0.0), "{triangle}");
        }
    }
    let down = &answer["down"];
    assert!(close(&down[0], 0.5 / 1.25_f64.sqrt()) && close(&down[2], -1.0 / 1.25_f64.sqrt()));

    // The design's whole body set when none is named.
    let all = query(&doc, json!({"query": "print_overhangs"}));
    assert_eq!(all["bodies"].as_array().unwrap().len(), 1);

    let error = query_error(
        &doc,
        json!({"query": "print_overhangs", "down": [0, 0, -1], "bed_face": top()}),
    );
    assert!(error.contains("either"), "{error}");
    let error = query_error(&doc, json!({"query": "print_overhangs", "down": [0, 0, 0]}));
    assert!(error.contains("nonzero"), "{error}");
    let error = query_error(&doc, json!({"query": "print_overhangs", "angle": -1}));
    assert!(error.contains("overhang angle"), "{error}");
    let error = query_error(&doc, json!({"query": "print_overhangs", "deviation": 0}));
    assert!(error.contains("deviation"), "{error}");
    let error = query_error(&doc, json!({"query": "print_overhangs", "angel": 10}));
    assert!(error.contains("angel"), "{error}");
}

#[test]
fn walls_of_a_box() {
    let doc = block();
    let answer = query(
        &doc,
        json!({"query": "print_thickness", "bodies": ["F2.b0"]}),
    );
    assert!(close(&answer["thinnest"], 20.0), "{answer}");
    assert_eq!(answer["minimum"], 0.8);
    let body = &answer["bodies"][0];
    assert!(close(&body["thinnest"], 20.0));
    assert_eq!(body["regions"], 0);
    assert_eq!(body["triangles"], json!([]));
    assert_eq!(answer["at"], body["at"]);
    assert_eq!(body["at"].as_array().unwrap().len(), 3);
    // Every wall is thinner than 25 mm: one region of all 12 triangles.
    let answer = query(
        &doc,
        json!({"query": "print_thickness", "bodies": ["F2.b0"], "minimum": 25}),
    );
    let body = &answer["bodies"][0];
    assert_eq!(body["triangles"].as_array().unwrap().len(), 12);
    assert_eq!(body["regions"], 1);
    assert_eq!(answer["regions"], 1);
    let error = query_error(&doc, json!({"query": "print_thickness", "minimum": -1}));
    assert!(error.contains("minimum wall"), "{error}");
}

#[test]
fn results_are_kept_by_body_content_and_settings() {
    let mut doc = block();
    let overhangs = json!({"query": "print_overhangs", "bodies": ["F2.b0"]});
    let first = query(&doc, overhangs.clone());
    assert_eq!(mesh_calls(&doc), 1);
    // Shown again: nothing is computed.
    assert_eq!(query(&doc, overhangs.clone()), first);
    assert_eq!(mesh_calls(&doc), 1);
    // Other settings and the walls: the same mesh.
    query(
        &doc,
        json!({"query": "print_overhangs", "bodies": ["F2.b0"], "angle": 30}),
    );
    query(
        &doc,
        json!({"query": "print_thickness", "bodies": ["F2.b0"]}),
    );
    assert_eq!(mesh_calls(&doc), 1);
    // Another deviation is another mesh.
    query(
        &doc,
        json!({"query": "print_overhangs", "bodies": ["F2.b0"], "deviation": 0.1}),
    );
    assert_eq!(mesh_calls(&doc), 2);
    assert_eq!(doc.kernel().mesh_tolerances.borrow()[1].deviation, 0.1);
    // The body changed: meshed again, the new size.
    taller(&mut doc, 30.0);
    let changed = query(&doc, overhangs);
    assert_eq!(mesh_calls(&doc), 3);
    assert!(
        close(&changed["area"], 2.0 * 400.0 + 4.0 * 600.0),
        "{changed}"
    );
}

#[test]
fn a_cancelled_job_stops_the_overhangs_and_the_walls() {
    let mut doc = block();
    let monitor = Arc::new(RecomputeMonitor::new());
    doc.set_monitor(Some(Arc::clone(&monitor)));
    // Not cancelled: answered.
    query(
        &doc,
        json!({"query": "print_overhangs", "bodies": ["F2.b0"]}),
    );
    monitor.cancel();
    let error = query_error(
        &doc,
        json!({"query": "print_overhangs", "bodies": ["F2.b0"], "angle": 50}),
    );
    assert_eq!(error, "the computation was canceled");
    let error = query_error(
        &doc,
        json!({"query": "print_thickness", "bodies": ["F2.b0"], "minimum": 2}),
    );
    assert_eq!(error, "the computation was canceled");
    doc.set_monitor(None);
    let answer = query(
        &doc,
        json!({"query": "print_thickness", "bodies": ["F2.b0"], "minimum": 2}),
    );
    assert!(close(&answer["thinnest"], 20.0));
}

#[test]
fn the_estimate_of_the_block() {
    let mut doc = block();
    // Without volumes from the kernel the estimate says why.
    let answer = query(
        &doc,
        json!({"query": "print_estimate", "bodies": ["F2.b0"]}),
    );
    assert!(
        answer["error"]
            .as_str()
            .unwrap()
            .contains("physical properties"),
        "{answer}"
    );
    doc.kernel().physical.set(true);
    // A 20 mm cube with the defaults in PLA (the body has no material).
    let answer = query(
        &doc,
        json!({"query": "print_estimate", "bodies": ["F2.b0"]}),
    );
    assert!(close(&answer["volume"], 8000.0), "{answer}");
    assert!(close(&answer["area"], 2400.0));
    assert!(close(&answer["shell_volume"], 1920.0));
    assert!(close(&answer["infill_volume"], 912.0));
    assert!(close(&answer["filament_volume"], 2832.0));
    assert!(close(&answer["mass"], 3.51168));
    let length = 2832.0 / (std::f64::consts::PI * 0.875 * 0.875) / 1000.0;
    assert!(close(&answer["length"], length));
    assert_eq!(answer["cost"], Value::Null);
    assert_eq!(answer["material"], "PLA Plastic");
    assert_eq!(answer["density"], 1.24);
    assert_eq!(answer["bodies"][0]["body"], "F2.b0");
    assert!(close(&answer["bodies"][0]["mass"], 3.51168));

    // The body's own material, and a price.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "abs"}),
    );
    let answer = query(
        &doc,
        json!({"query": "print_estimate", "bodies": ["F2.b0"], "price_per_kg": 20}),
    );
    assert_eq!(answer["material"], "ABS Plastic");
    assert!(close(&answer["mass"], 2.832 * 1.06));
    assert!(close(&answer["cost"], 20.0 * 2.832 * 1.06 / 1000.0));
    // A material chosen for the print, by id or name; a definition's
    // estimate settings as they are, `enabled` included.
    let answer = query(
        &doc,
        json!({"query": "print_estimate", "bodies": ["F2.b0"], "material": "Nylon",
               "enabled": true, "walls": 3, "infill": 0.2}),
    );
    assert_eq!(answer["material"], "Nylon");
    assert!(close(&answer["shell_volume"], 2400.0 * 3.0 * 0.4));
    assert!(close(&answer["infill_volume"], (8000.0 - 2880.0) * 0.2));
    assert!(close(&answer["mass"], (2880.0 + 1024.0) / 1000.0 * 1.14));
    let error = query_error(
        &doc,
        json!({"query": "print_estimate", "bodies": ["F2.b0"], "material": "cheese"}),
    );
    assert!(error.contains("cheese"), "{error}");
    let error = query_error(
        &doc,
        json!({"query": "print_estimate", "bodies": ["F2.b0"], "line_width": 0}),
    );
    assert!(error.contains("line width"), "{error}");
}

#[test]
fn a_print_analysis_changes_no_body_and_no_print_file() {
    let mut doc = block();
    let dir = mitcad_model::testing::unique_temp_path("print-analysis");
    // The same file name in two folders: the 3MF model is named after the
    // file.
    let (before, after) = (dir.join("before"), dir.join("after"));
    std::fs::create_dir_all(&before).unwrap();
    std::fs::create_dir_all(&after).unwrap();
    let before_path = before.join("design.3mf").to_str().unwrap().to_owned();
    let after_path = after.join("design.3mf").to_str().unwrap().to_owned();
    let bodies = query(&doc, json!({"query": "bodies", "properties": false}));
    command(
        &mut doc,
        json!({"cmd": "export", "path": before_path, "bodies": ["F2.b0"]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "print", "bodies": ["F2.b0"], "bed_face": top()}}),
    );
    query(
        &doc,
        json!({"query": "print_overhangs", "bodies": ["F2.b0"], "bed_face": top()}),
    );
    command(
        &mut doc,
        json!({"cmd": "export", "path": after_path, "bodies": ["F2.b0"]}),
    );
    assert_eq!(
        query(&doc, json!({"query": "bodies", "properties": false})),
        bodies
    );
    assert_eq!(
        std::fs::read(&before_path).unwrap(),
        std::fs::read(&after_path).unwrap()
    );
}

#[test]
fn the_walls_summary_leaves_the_triangles_out_or_caps_them() {
    // mitcad#689: an agent's tool response stays small.
    let doc = block();
    let walls = |extra: Value| {
        let mut request = json!({"query": "print_thickness", "bodies": ["F2.b0"], "minimum": 25});
        for (key, value) in extra.as_object().unwrap() {
            request[key] = value.clone();
        }
        query(&doc, request)
    };
    let full = walls(json!({}));
    assert_eq!(full["thin_triangles"], 12);
    assert_eq!(full["bodies"][0]["thin_triangles"], 12);
    assert_eq!(full["bodies"][0]["triangles"].as_array().unwrap().len(), 12);
    assert!(full.get("truncated").is_none());
    let summary = walls(json!({"triangles": false}));
    let body = &summary["bodies"][0];
    assert!(body.get("triangles").is_none(), "{summary}");
    assert_eq!(body["thin_triangles"], 12);
    assert_eq!(body["regions"], 1);
    assert_eq!(body["thinnest"], full["bodies"][0]["thinnest"]);
    assert_eq!(summary["thinnest"], full["thinnest"]);
    assert_eq!(summary["at"], full["at"]);
    let capped = walls(json!({"max_triangles": 5}));
    assert_eq!(
        capped["bodies"][0]["triangles"].as_array().unwrap().len(),
        5
    );
    assert_eq!(capped["bodies"][0]["truncated"], true);
    assert_eq!(capped["truncated"], true);
    assert_eq!(capped["thin_triangles"], 12);
    let error = query_error(&doc, json!({"query": "print_thickness", "triangles": "no"}));
    assert!(error.contains("boolean"), "{error}");
}

#[test]
fn body_versions_change_with_the_shape() {
    // mitcad#689: MCP's command results list the bodies a command changed.
    let mut doc = block();
    let plain = query(&doc, json!({"query": "bodies"}));
    assert!(plain[0].get("version").is_none());
    let version = |doc: &Document<MockKernel>| {
        query(doc, json!({"query": "bodies", "versions": true}))[0]["version"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let before = version(&doc);
    assert_eq!(before.len(), 32);
    command(
        &mut doc,
        json!({"cmd": "set_body_visible", "uid": "F2.b0", "visible": false}),
    );
    assert_eq!(version(&doc), before);
    taller(&mut doc, 30.0);
    assert_ne!(version(&doc), before);
}
