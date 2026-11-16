// SPDX-License-Identifier: MIT
//! Sketch texts moved with Move/Copy (mitcad#1911, GitHub
//! mitcad/Mitcad#369): `sketch.move`'s `texts`. A text without a frame
//! moves its anchor (and turns with a rotation), a text in a frame moves
//! its frame's rectangle, a copy copies the text, a text on a path is
//! refused; Edit Text's `at` places a text and reads back.

use std::f64::consts::FRAC_PI_2;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn with_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string())
        .map(|r| panic!("{command} must fail, got {r}"))
        .unwrap_err()
        .to_string()
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn text(s: &Value, id: &str) -> Value {
    s["texts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone()
}

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

/// Where a point is.
fn at(s: &Value, point: &str) -> [f64; 2] {
    let p = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == point)
        .unwrap_or_else(|| panic!("no {point} in {s}"));
    xy(&p["at"])
}

/// The baseline tolerance of geometric comparisons.
fn assert_near(a: [f64; 2], b: [f64; 2], what: &str) {
    assert!(
        (a[0] - b[0]).abs() <= 1e-6 && (a[1] - b[1]).abs() <= 1e-6,
        "{what}: {a:?}, expected {b:?}"
    );
}

fn added_text(doc: &mut Document<MockKernel>, extra: Value) -> String {
    let mut cmd = json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "AB", "height": 5});
    for (k, v) in extra.as_object().unwrap() {
        cmd[k] = v.clone();
    }
    let made = command(doc, cmd);
    made["texts"][0].as_str().unwrap().to_owned()
}

#[test]
fn a_text_without_a_frame_moves_its_anchor() {
    let mut d = with_sketch();
    let t = added_text(&mut d, json!({"at": [5, 5]}));
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "texts": [t], "by": [10, -2]}),
    );
    assert_near(xy(&text(&sketch(&d), &t)["at"]), [15.0, 3.0], "Translate");
    // Point to Point: by the target less the origin.
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "texts": [t],
               "from": [15, 3], "to": [0, 0]}),
    );
    assert_near(
        xy(&text(&sketch(&d), &t)["at"]),
        [0.0, 0.0],
        "Point to Point",
    );
    // Rotate about a pivot turns the anchor and the text's angle.
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "texts": [t],
               "rotate": {"center": [-10, 0], "angle": FRAC_PI_2}}),
    );
    let s = sketch(&d);
    assert_near(xy(&text(&s, &t)["at"]), [-10.0, 10.0], "Rotate");
    let angle = text(&s, &t)["angle"].as_f64().unwrap();
    assert!((angle - FRAC_PI_2).abs() <= 1e-9, "angle {angle}");
    // Without a pivot: about the text's anchor, which stays.
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "texts": [t],
               "rotate": {"angle": FRAC_PI_2}}),
    );
    let s = sketch(&d);
    assert_near(
        xy(&text(&s, &t)["at"]),
        [-10.0, 10.0],
        "Rotate about its anchor",
    );
    let angle = text(&s, &t)["angle"].as_f64().unwrap();
    assert!(
        (angle.abs() - std::f64::consts::PI).abs() <= 1e-9,
        "angle {angle}"
    );
}

#[test]
fn a_qr_code_moves_with_a_curve() {
    let mut d = with_sketch();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_code", "sketch": "F1", "text": "M", "at": [20, 0], "module": 1}),
    );
    let code = made["texts"][0].as_str().unwrap().to_owned();
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    )["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|e| e.as_str().filter(|e| e.starts_with('c')).map(str::to_owned))
        .unwrap();
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [line], "texts": [code],
               "by": [0, 5]}),
    );
    assert_near(xy(&text(&sketch(&d), &code)["at"]), [20.0, 5.0], "the code");
}

#[test]
fn a_text_in_a_frame_moves_its_rectangle() {
    let mut d = with_sketch();
    let t = added_text(
        &mut d,
        json!({"frame": {"corner": [0, 0], "diagonal": [30, 10]}}),
    );
    let s = sketch(&d);
    let frame: Vec<String> = text(&s, &t)["frame"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(frame.len(), 3);
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "texts": [t], "by": [5, 20]}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &frame[0]), [5.0, 20.0], "the frame's corner");
    assert_near(at(&s, &frame[1]), [35.0, 20.0], "its x corner");
    assert_near(at(&s, &frame[2]), [5.0, 30.0], "its y corner");
    // The fourth corner moves too: every point of the rectangle's lines.
    let corners: Vec<[f64; 2]> = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "point")
        .map(|e| xy(&e["at"]))
        .collect();
    assert!(
        corners
            .iter()
            .any(|c| (c[0] - 35.0).abs() <= 1e-6 && (c[1] - 30.0).abs() <= 1e-6),
        "the fourth corner: {corners:?}"
    );
    assert_near(xy(&text(&s, &t)["at"]), [5.0, 20.0], "the text's anchor");
    // A quarter turn about the frame's corner turns the rectangle and the
    // text with it.
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "texts": [t],
               "rotate": {"center": frame[0], "angle": FRAC_PI_2}}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &frame[1]), [5.0, 50.0], "the x corner turned");
    assert_near(at(&s, &frame[2]), [-5.0, 20.0], "the y corner turned");
    let angle = text(&s, &t)["angle"].as_f64().unwrap();
    assert!((angle - FRAC_PI_2).abs() <= 1e-6, "angle {angle}");
}

#[test]
fn create_copy_copies_the_text() {
    let mut d = with_sketch();
    let free = added_text(&mut d, json!({"at": [5, 5]}));
    let framed = added_text(
        &mut d,
        json!({"frame": {"corner": [0, 20], "diagonal": [30, 30]}}),
    );
    let lines_before = sketch(&d)["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "line")
        .count();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "texts": [free, framed],
               "by": [100, 0], "copy": true}),
    );
    let copies: Vec<String> = made["made"]
        .as_array()
        .unwrap_or_else(|| panic!("no made in {made}"))
        .iter()
        .map(|t| t.as_str().unwrap().to_owned())
        .filter(|t| t.starts_with('t'))
        .collect();
    assert_eq!(copies.len(), 2, "{made}");
    let s = sketch(&d);
    // The originals stay.
    assert_near(xy(&text(&s, &free)["at"]), [5.0, 5.0], "the original");
    assert_near(xy(&text(&s, &copies[0])["at"]), [105.0, 5.0], "the copy");
    assert_eq!(text(&s, &copies[0])["text"], "AB");
    // The framed text's copy has a rectangle of its own.
    let frame: Vec<String> = text(&s, &copies[1])["frame"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(frame.len(), 3);
    assert_near(
        at(&s, &frame[0]),
        [100.0, 20.0],
        "the copied frame's corner",
    );
    assert_near(at(&s, &frame[1]), [130.0, 20.0], "its x corner");
    let lines_after = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "line")
        .count();
    assert_eq!(
        lines_after,
        lines_before + 4,
        "the copied rectangle's lines"
    );
}

#[test]
fn a_text_on_a_path_is_refused() {
    let mut d = with_sketch();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [40, 0],
               "construction": true}),
    );
    let line = made["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|e| e.as_str().filter(|e| e.starts_with('c')).map(str::to_owned))
        .unwrap();
    let t = added_text(&mut d, json!({"path": {"curve": line}}));
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "texts": [t], "by": [0, 10]}),
    );
    assert!(
        error.contains("a text on a path follows its curve: move the curve instead"),
        "{error}"
    );
    // The curve moves it.
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [line], "by": [0, 10]}),
    );
    assert_eq!(text(&sketch(&d), &t)["path"]["curve"], line);
    // Nothing to move.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "by": [0, 10]}),
    );
    assert!(error.contains("\"entities\" or \"texts\""), "{error}");
}

#[test]
fn edit_text_places_a_text_by_its_position() {
    let mut d = with_sketch();
    let t = added_text(&mut d, json!({"at": [5, 5]}));
    command(
        &mut d,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": t, "at": [-12.5, 7.25]}),
    );
    assert_near(
        xy(&text(&sketch(&d), &t)["at"]),
        [-12.5, 7.25],
        "Edit Text's at",
    );
    // And back where it was, with the rest of the text as it was.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": t, "at": [5, 5]}),
    );
    let s = sketch(&d);
    assert_near(xy(&text(&s, &t)["at"]), [5.0, 5.0], "back");
    assert_eq!(text(&s, &t)["text"], "AB");
}
