// SPDX-License-Identifier: MIT
//! A drawing's dimensioning standard (mitcad#2047) on the mock kernel: ISO
//! 129-1 by default and left out of the file, ASME from the ANSI template
//! or chosen; under ASME a vertical dimension's value is horizontal (in the
//! `drawing` query and in an SVG export) and an inch value has no unit and
//! no leading zero; changing it is one undo step.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::CurveGeometry;
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
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
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn answer(doc: &Document<MockKernel>, drawing: &str) -> Value {
    query(doc, json!({"query": "drawing", "drawing": drawing}))
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

/// A 40 x 30 x 20 block whose front's left side (x 0, z 0 to 20) the
/// kernel lists as a line; returns its name.
fn block() -> (Document<MockKernel>, String) {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    doc.kernel().lists_geometry.set(true);
    let body: BodyUid = "F2.b0".parse().unwrap();
    let names: Vec<String> = doc
        .kernel()
        .edges(doc.body_shape(body).unwrap())
        .unwrap()
        .into_iter()
        .filter_map(|e| e.name)
        .collect();
    let side = names[0].clone();
    doc.kernel().edge_curves.borrow_mut().insert(
        side.clone(),
        CurveGeometry::Line {
            start: [0.0, 0.0, 0.0],
            end: [0.0, 0.0, 20.0],
        },
    );
    (doc, side)
}

/// A drawing with a front view at 1:1 and a vertical dimension of the
/// block's left side (20 mm high).
fn dimensioned(doc: &mut Document<MockKernel>, side: &str, add: Value) -> String {
    let added = command(doc, add);
    let drawing = added["drawing"].as_str().unwrap().to_owned();
    command(
        doc,
        json!({"cmd": "add_drawing_view", "drawing": drawing, "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    command(
        doc,
        json!({"cmd": "add_drawing_annotation", "drawing": drawing, "view": 1, "type": "linear",
               "along": "vertical",
               "picks": [{"body": "F2.b0", "edge": side, "point": "start"},
                         {"body": "F2.b0", "edge": side, "point": "end"}]}),
    );
    drawing
}

/// The texts (words, angle) of the dimension's group on the first sheet.
fn dimension_texts(doc: &Document<MockKernel>, drawing: &str) -> Vec<(String, f64)> {
    let sheet = answer(doc, drawing)["sheets"][0].clone();
    let group = sheet["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["class"] == "dimension")
        .cloned()
        .unwrap_or_else(|| panic!("no dimension: {}", sheet["groups"]));
    group["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| {
            Some((
                i.get("text")?.as_str()?.to_owned(),
                i["angle"].as_f64().unwrap_or(0.0),
            ))
        })
        .collect()
}

#[test]
fn the_ansi_template_brings_asme_and_iso_stays_out_of_the_file() {
    let (mut doc, side) = block();
    let iso = dimensioned(
        &mut doc,
        &side,
        json!({"cmd": "add_drawing", "template": "iso"}),
    );
    let asme = dimensioned(
        &mut doc,
        &side,
        json!({"cmd": "add_drawing", "template": "ansi"}),
    );
    assert_eq!(answer(&doc, &iso)["standard"], "iso");
    assert_eq!(answer(&doc, &asme)["standard"], "asme");
    // A vertical dimension: along its line under ISO, horizontal under ASME.
    let [(text, angle)] = &dimension_texts(&doc, &iso)[..] else {
        panic!("{:?}", dimension_texts(&doc, &iso))
    };
    assert_eq!(text, "20");
    assert!((angle - 90.0f64.to_radians()).abs() < 1e-9, "{angle}");
    let [(text, angle)] = &dimension_texts(&doc, &asme)[..] else {
        panic!("{:?}", dimension_texts(&doc, &asme))
    };
    assert_eq!(text, "20");
    assert_eq!(*angle, 0.0);
    // Saved: ISO left out, ASME kept; read again the same.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let drawings = file["drawings"].as_array().unwrap();
    assert!(drawings[0].get("defaults").is_none(), "{}", drawings[0]);
    assert_eq!(drawings[1]["defaults"], json!({"standard": "asme"}));
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    // A file of before (no standard anywhere) opens as ISO.
    let mut old = file.clone();
    old["drawings"][1]
        .as_object_mut()
        .unwrap()
        .remove("defaults");
    let loaded = Document::from_json(&old.to_string(), MockKernel::default()).unwrap();
    assert_eq!(answer(&loaded, &asme)["standard"], "iso");
    // Given, it wins over the template's.
    let given = command(
        &mut doc,
        json!({"cmd": "add_drawing", "template": "ansi", "standard": "iso"}),
    );
    assert_eq!(
        answer(&doc, given["drawing"].as_str().unwrap())["standard"],
        "iso"
    );
}

#[test]
fn changing_the_standard_is_one_undo_step_that_every_dimension_follows() {
    let (mut doc, side) = block();
    let drawing = dimensioned(
        &mut doc,
        &side,
        json!({"cmd": "add_drawing", "size": "A3", "units": "in"}),
    );
    // ISO in inches: the unit after the value, its leading zero kept.
    let [(text, angle)] = &dimension_texts(&doc, &drawing)[..] else {
        panic!()
    };
    assert_eq!(text, "0.787 in");
    assert!(angle.abs() > 0.1);
    command(
        &mut doc,
        json!({"cmd": "edit_drawing", "drawing": drawing, "standard": "asme"}),
    );
    assert_eq!(undo_label(&doc), "Edit Drawing1");
    assert_eq!(answer(&doc, &drawing)["standard"], "asme");
    let [(text, angle)] = &dimension_texts(&doc, &drawing)[..] else {
        panic!()
    };
    assert_eq!(text, ".787");
    assert_eq!(*angle, 0.0);
    // An SVG export writes it horizontal: no rotation on its text.
    let dir = mitcad_model::testing::unique_temp_dir("asme");
    let svg = dir.join("asme.svg");
    command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": drawing, "path": svg.to_string_lossy()}),
    );
    let text = std::fs::read_to_string(&svg).unwrap();
    let line = text
        .lines()
        .find(|l| l.starts_with("<text") && l.ends_with(">.787</text>"))
        .unwrap_or_else(|| panic!("{text}"));
    assert!(!line.contains("rotate("), "{line}");
    let _ = std::fs::remove_dir_all(&dir);
    // Undone in one step: ISO again.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(answer(&doc, &drawing)["standard"], "iso");
    let [(text, _)] = &dimension_texts(&doc, &drawing)[..] else {
        panic!()
    };
    assert_eq!(text, "0.787 in");
}
