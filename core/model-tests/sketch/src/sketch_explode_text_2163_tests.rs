// SPDX-License-Identifier: MIT
//! A sketch text as plain curves (mitcad#2163, GitHub mitcad/Mitcad#520),
//! with the mock kernel (its letters: an `O` of two rectangles, a `C` of
//! three lines and a quadratic Bézier curve):
//!
//! - The sketch's DXF export writes texts as their outlines, so a sketch
//!   holding only a text reopens as curves through Insert DXF.
//! - `sketch.explode_text` turns a text or a QR code into one line per
//!   straight piece and one spline per Bézier piece, sharing end points
//!   along each contour, with the same profile regions, as one undo step.
//! - An Extrude of a letter and a fillet on one of its side edges rebuild
//!   unchanged afterwards, their names of the letter's pieces rewritten;
//!   a name that no new curve matches is a warning.

use serde_json::{Value, json};

use mitcad_model::testing::{MockKernel, remove_temp, unique_temp_dir};
use mitcad_model::{Document, FeatureStatus, Kernel};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn with_text(text: &str) -> (Document<MockKernel>, String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": text, "at": [0, 0],
               "height": 10}),
    );
    let id = made["texts"][0].as_str().unwrap().to_owned();
    (d, id)
}

fn entity(s: &Value, id: &str) -> Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone()
}

/// The sketch's curves by type.
fn count(s: &Value, kind: &str) -> usize {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == kind)
        .count()
}

/// A curve's end points: a line's, a spline's first and last control
/// points.
fn ends(s: &Value, id: &str) -> (String, String) {
    let e = entity(s, id);
    let text = |v: &Value| v.as_str().unwrap().to_owned();
    match e["type"].as_str().unwrap() {
        "line" => (text(&e["start"]), text(&e["end"])),
        "spline" => {
            let control = e["control"].as_array().unwrap();
            (text(&control[0]), text(control.last().unwrap()))
        }
        other => panic!("{id} is a {other}"),
    }
}

/// The areas of the sketch's regions, sorted.
fn areas(s: &Value) -> Vec<f64> {
    let mut out: Vec<f64> = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["area"].as_f64().unwrap())
        .collect();
    out.sort_by(f64::total_cmp);
    out
}

fn same_areas(before: &[f64], after: &[f64]) {
    assert_eq!(before.len(), after.len(), "{before:?} != {after:?}");
    for (a, b) in before.iter().zip(after) {
        assert!((a - b).abs() <= 1e-6, "{before:?} != {after:?}");
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_sketch_s_dxf_holds_its_texts_as_outlines() {
    // Only a text: its 12 pieces (the O's 8 lines, the C's 3 lines and a
    // spline), which Insert DXF reads back as curves.
    let (mut d, _) = with_text("OC");
    let dir = unique_temp_dir("2163-text-dxf");
    let path = dir.join("text.dxf").to_str().unwrap().to_owned();
    let written = command(
        &mut d,
        json!({"cmd": "export_sketch", "sketch": "F1", "path": path}),
    );
    assert_eq!(written["entities"], 12, "{written}");
    command(&mut d, json!({"cmd": "sketch.create"}));
    let inserted = command(
        &mut d,
        json!({"cmd": "sketch.import_dxf", "sketch": "F2", "path": path}),
    );
    assert_eq!(inserted["curves"], 12, "{inserted}");
    let s = sketch(&d, "F2");
    assert_eq!((count(&s, "line"), count(&s, "spline")), (11, 1));
    // The letters bound the same regions there.
    same_areas(&areas(&sketch(&d, "F1")), &areas(&s));

    // Curves and a text both: every curve and the outlines.
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-5, -5],
               "width": 40, "height": 20}),
    );
    let written = command(
        &mut d,
        json!({"cmd": "export_sketch", "sketch": "F1", "path": path}),
    );
    assert_eq!(written["entities"], 16, "{written}");
    command(&mut d, json!({"cmd": "sketch.create"}));
    let inserted = command(
        &mut d,
        json!({"cmd": "sketch.import_dxf", "sketch": "F3", "path": path}),
    );
    assert_eq!(inserted["curves"], 16, "{inserted}");
    remove_temp(&dir);
}

#[test]
fn a_text_explodes_into_lines_and_splines_with_the_same_profiles() {
    let (mut d, t) = with_text("OC");
    let before = sketch(&d, "F1");
    let depth = d.undo_depth();
    let exploded = command(
        &mut d,
        json!({"cmd": "sketch.explode_text", "sketch": "F1", "text": t}),
    );
    // One undo step, one sketch step.
    assert_eq!(d.undo_depth(), depth + 1);
    assert_eq!(d.undo_label(), Some("Explode Text in Sketch1"));
    let s = sketch(&d, "F1");
    assert_eq!(s["texts"].as_array().unwrap().len(), 0, "{s}");
    assert_eq!(strings(&exploded["removed"]), std::slice::from_ref(&t));
    let made = strings(&exploded["made"]);
    assert_eq!(made.len(), 12, "{exploded}");
    assert_eq!((count(&s, "line"), count(&s, "spline")), (11, 1));
    // The C's curved piece is a quadratic spline; none is construction or
    // held by anything.
    let curved = made
        .iter()
        .find(|c| entity(&s, c)["type"] == "spline")
        .unwrap();
    assert_eq!(entity(&s, curved)["degree"], 2);
    for c in &made {
        assert_ne!(entity(&s, c)["construction"], true, "{c}");
    }
    assert_eq!(s["constraints"].as_array().map_or(0, Vec::len), 0, "{s}");
    // Each contour a closed chain: the O's outline and counter (4 each),
    // the C (4).
    for contour in [&made[0..4], &made[4..8], &made[8..12]] {
        for k in 0..contour.len() {
            let (_, end) = ends(&s, &contour[k]);
            let (start, _) = ends(&s, &contour[(k + 1) % contour.len()]);
            assert_eq!(end, start, "{contour:?}");
        }
    }
    // The same profile regions.
    same_areas(&areas(&before), &areas(&s));
    let steps = s["steps"].as_array().unwrap();
    assert_eq!(steps.last().unwrap()["kind"], "explode_text", "{s}");

    // Undo brings the text back.
    d.undo();
    assert_eq!(sketch(&d, "F1")["texts"].as_array().unwrap().len(), 1);
}

#[test]
fn a_qr_code_explodes_too() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_code", "sketch": "F1", "text": "MitCad", "at": [0, 0],
               "module": 1}),
    );
    let t = made["texts"][0].as_str().unwrap().to_owned();
    let before = sketch(&d, "F1");
    let exploded = command(
        &mut d,
        json!({"cmd": "sketch.explode_text", "sketch": "F1", "text": t}),
    );
    let s = sketch(&d, "F1");
    let made = strings(&exploded["made"]);
    assert!(made.len() > 20, "{exploded}");
    assert_eq!(count(&s, "line"), made.len());
    assert_eq!(count(&s, "spline"), 0);
    same_areas(&areas(&before), &areas(&s));
}

/// An `O` extruded (F2) and a fillet (F3) on the side edge between the
/// second and third pieces of its outline.
fn letter_with_fillet() -> (Document<MockKernel>, String) {
    let (mut d, t) = with_text("O");
    d.kernel().measures.set(true);
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": format!("r{{{t}.g0.c0}}")}],
               "extent": {"type": "distance", "distance": 2}, "operation": "new_body"}}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
               "edges": [format!("E{{F2:side({t}.g0.c0)|F2:side({t}.g0.c0)}}#1")],
               "radius": 0.5}}),
    );
    (d, t)
}

fn volume(d: &Document<MockKernel>) -> f64 {
    let shape = d.body_shape("F2.b0".parse().unwrap()).unwrap();
    d.kernel().mass_properties(shape).unwrap().volume
}

fn status(d: &Document<MockKernel>, uid: &str) -> FeatureStatus {
    d.status(uid.parse().unwrap()).unwrap().clone()
}

#[test]
fn an_extruded_letter_and_its_fillet_rebuild_after_explode_text() {
    let (mut d, t) = letter_with_fillet();
    assert_eq!(status(&d, "F3"), FeatureStatus::Ok);
    let before = volume(&d);
    let depth = d.undo_depth();
    let exploded = command(
        &mut d,
        json!({"cmd": "sketch.explode_text", "sketch": "F1", "text": t}),
    );
    // One undo step, the rewrite of the later features included.
    assert_eq!(d.undo_depth(), depth + 1);
    assert_eq!(d.undo_label(), Some("Explode Text in Sketch1"));
    let rewritten: Vec<&str> = exploded["rewritten"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["uid"].as_str().unwrap())
        .collect();
    assert_eq!(rewritten, ["F2", "F3"], "{exploded}");
    assert_eq!(exploded.get("warnings"), None, "{exploded}");
    assert_eq!(d.recompute().error, None);
    assert_eq!(status(&d, "F2"), FeatureStatus::Ok);
    assert_eq!(status(&d, "F3"), FeatureStatus::Ok);
    assert!((volume(&d) - before).abs() <= 1e-6);
    // The extrude's profile and the fillet's edge name the new curves.
    let json = d.to_json();
    assert!(!json.contains(&format!("{t}.g0")), "{json}");
    let made = strings(&exploded["made"]);
    assert!(json.contains(&format!("F2:side({}[", made[1])), "{json}");
    assert!(json.contains(&format!("F2:side({}[", made[2])), "{json}");
    // Undo puts the text and the old names back.
    d.undo();
    assert!(d.to_json().contains(&format!("{t}.g0.c0")));
    assert_eq!(status(&d, "F3"), FeatureStatus::Ok);
}

#[test]
fn a_name_no_new_curve_matches_is_a_warning() {
    let (mut d, t) = letter_with_fillet();
    // An edge numbered past the outline's four pieces.
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
               "edges": [format!("E{{F2:side({t}.g0.c0)|F2:side({t}.g0.c0)}}#9")],
               "radius": 0.5}}),
    );
    let exploded = command(
        &mut d,
        json!({"cmd": "sketch.explode_text", "sketch": "F1", "text": t}),
    );
    let warnings = strings(&exploded["warnings"]);
    assert_eq!(warnings.len(), 1, "{exploded}");
    assert!(
        warnings[0].starts_with("Fillet2 uses the edge E{")
            && warnings[0].ends_with("#9, whose name changes: check it"),
        "{warnings:?}"
    );
    // The others are rewritten all the same.
    assert_eq!(status(&d, "F3"), FeatureStatus::Ok);
}

#[test]
fn a_text_parameter_stays_when_its_text_explodes() {
    // The text follows Label, two letters: it explodes as they are now.
    let (mut d, t) = with_text("O");
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "Label", "expression": "'OO'"}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": t, "value": "Label"}),
    );
    let exploded = command(
        &mut d,
        json!({"cmd": "sketch.explode_text", "sketch": "F1", "text": t}),
    );
    assert_eq!(strings(&exploded["made"]).len(), 16, "{exploded}");
    assert!(d.parameters().find("Label").is_some());
    // A text that is not there is refused in words.
    let refused = d
        .command(&json!({"cmd": "sketch.explode_text", "sketch": "F1", "text": t}).to_string())
        .unwrap_err()
        .to_string();
    assert!(refused.contains("does not exist"), "{refused}");
}
