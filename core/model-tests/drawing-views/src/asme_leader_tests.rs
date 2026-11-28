// SPDX-License-Identifier: MIT
//! ASME callouts and short vertical dimensions on a drawing (mitcad#2621)
//! on the mock kernel: a radius's and a diameter's leader ends in a short
//! shoulder beside its text, the first line's middle level with it, on the
//! side a drag takes it to; a short vertical dimension's value goes
//! outside on an elbow, to either side; the `drawing` query and the PDF
//! export write the same texts at the same places, and the annotations keep
//! their picks.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::CurveGeometry;
use mitcad_model::drawing::trim_number;
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use mitcad_model_drawing::sheet::text_width;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

/// ASME's shoulder and the text gap beside it, paper mm.
const SHOULDER: f64 = 3.0;
const GAP: f64 = 1.0;

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

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn point(value: &Value) -> [f64; 2] {
    [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()]
}

fn sheet(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0].clone()
}

/// The items of annotation `id`'s group on the sheet.
fn items(doc: &Document<MockKernel>, id: u64) -> Vec<Value> {
    sheet(doc)["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == format!("A{id}").as_str())
        .unwrap_or_else(|| panic!("no group of annotation {id}"))["items"]
        .as_array()
        .unwrap()
        .clone()
}

/// A group's texts: words, baseline's start (all written from their start,
/// horizontal) and height.
fn texts(items: &[Value]) -> Vec<(String, [f64; 2], f64)> {
    items
        .iter()
        .filter_map(|i| {
            let text = i.get("text")?.as_str()?.to_owned();
            assert_eq!(i["angle"].as_f64().unwrap(), 0.0, "{i}");
            assert_eq!(i["anchor"], "start", "{i}");
            Some((text, point(&i["at"]), i["height"].as_f64().unwrap()))
        })
        .collect()
}

/// A group's level lines, each from left to right.
fn level_lines(items: &[Value]) -> Vec<([f64; 2], [f64; 2])> {
    items
        .iter()
        .filter_map(|i| {
            let line = i.get("line")?;
            let (a, b) = (point(&line[0]), point(&line[1]));
            (near(a[1], b[1]) && !near(a[0], b[0])).then_some(if a[0] <= b[0] {
                (a, b)
            } else {
                (b, a)
            })
        })
        .collect()
}

/// The one shoulder of a group: its level line ASME's shoulder long.
fn shoulder(items: &[Value]) -> ([f64; 2], [f64; 2]) {
    let lines: Vec<_> = level_lines(items)
        .into_iter()
        .filter(|(a, b)| near(b[0] - a[0], SHOULDER))
        .collect();
    let [one] = lines[..] else {
        panic!("{lines:?} in {items:?}")
    };
    one
}

/// A 40 x 30 x 20 block whose kernel lists a circle of radius 5 on its top
/// and its front's right side as a line only 4 mm high; an ASME drawing
/// with a front view at 1:1 (the block's front 65..105 x 110..130 on the
/// sheet) and the top view above it. Returns the edges' names.
fn asme_drawing() -> (Document<MockKernel>, Vec<String>) {
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
    assert!(names.len() >= 3, "{names:?}");
    {
        let mut curves = doc.kernel().edge_curves.borrow_mut();
        curves.insert(
            names[1].clone(),
            CurveGeometry::Line {
                start: [40.0, 0.0, 0.0],
                end: [40.0, 0.0, 4.0],
            },
        );
        curves.insert(
            names[2].clone(),
            CurveGeometry::Circle {
                center: [20.0, 15.0, 20.0],
                normal: [0.0, 0.0, 1.0],
                radius: 5.0,
                start: [25.0, 15.0, 20.0],
                end: [25.0, 15.0, 20.0],
            },
        );
    }
    doc.kernel().drawn_curves.borrow_mut().push(
        (0..=32)
            .map(|i| {
                let a = std::f64::consts::TAU * f64::from(i) / 32.0;
                [20.0 + 5.0 * a.cos(), 15.0 + 5.0 * a.sin(), 20.0]
            })
            .collect(),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing", "size": "A3", "standard": "asme"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "projected", "parent": 1,
               "side": "above"}),
    );
    (doc, names)
}

/// Where the circle's centre is on the sheet: in the top view, which looks
/// down with the model's x and y.
fn circle_center(doc: &Document<MockKernel>) -> [f64; 2] {
    let views = sheet(doc)["views"].clone();
    let top = views
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == 2)
        .unwrap()
        .clone();
    let place = point(&top["place"]);
    [place[0] + 20.0, place[1] + 15.0]
}

/// The PDF export's content as text.
fn pdf(doc: &mut Document<MockKernel>) -> String {
    let dir = mitcad_model::testing::unique_temp_dir("asme-leader");
    let path = dir.join("asme.pdf").to_string_lossy().into_owned();
    command(
        doc,
        json!({"cmd": "export_drawing", "drawing": "D1", "path": path}),
    );
    let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
    std::fs::remove_dir_all(&dir).ok();
    text
}

/// Whether the PDF writes `words` (plain ASCII) at `at` on the sheet.
fn pdf_writes(pdf: &str, words: &str, at: [f64; 2]) -> bool {
    let n = |v: f64| trim_number(v, 4);
    pdf.contains(&format!("{} {} Tm ({words}) Tj", n(at[0]), n(at[1])))
}

#[test]
fn asme_radius_and_diameter_callouts_end_in_a_shoulder_beside_their_text() {
    let (mut doc, names) = asme_drawing();
    let c = circle_center(&doc);
    // A radius placed up and right of the circle.
    let radius = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "radius",
               "picks": [{"body": "F2.b0", "edge": names[2]}]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    let bend = [c[0] + 12.0, c[1] + 9.0];
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": radius,
               "at": bend}),
    );
    let drawn = items(&doc, radius);
    let (from, _) = shoulder(&drawn);
    assert!(near(from[0], bend[0]) && near(from[1], bend[1]), "{from:?}");
    let [(words, at, h)] = &texts(&drawn)[..] else {
        panic!("{drawn:?}")
    };
    assert_eq!(words, "R5");
    assert!(near(at[0], bend[0] + SHOULDER + GAP), "{at:?}");
    assert!(near(at[1] + h / 2.0, bend[1]), "{at:?}");
    let written = pdf(&mut doc);
    assert!(pdf_writes(&written, "R5", *at), "{at:?}");

    // Dragged across the centre: the shoulder and the text on the left,
    // the text ending beside the shoulder; the radius still of its circle.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": radius,
               "move": [-24.0, 0.0]}),
    );
    let bend = [bend[0] - 24.0, bend[1]];
    assert!(bend[0] < c[0]);
    let drawn = items(&doc, radius);
    let (from, to) = shoulder(&drawn);
    assert!(near(to[0], bend[0]) && near(from[0], bend[0] - SHOULDER));
    let [(_, at, h)] = &texts(&drawn)[..] else {
        panic!("{drawn:?}")
    };
    assert!(near(at[0] + text_width("R5", *h), from[0] - GAP), "{at:?}");
    assert!(near(at[1] + h / 2.0, bend[1]));
    assert!(pdf_writes(&pdf(&mut doc), "R5", *at));
    let annotation = sheet(&doc)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == radius)
        .cloned()
        .unwrap();
    assert_eq!(annotation["picks"][0]["edge"], names[2].as_str());
    assert_eq!(annotation["broken"], false);
    assert_eq!(annotation["label"], "R5");

    // A diameter of two lines on the left: the first line's middle level
    // with the shoulder, the second below it.
    let diameter = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "diameter",
               "picks": [{"body": "F2.b0", "edge": names[2]}]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    let bend = [c[0] - 12.0, c[1] - 9.0];
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": diameter,
               "at": bend, "format": {"after": "THRU\n2 PLACES"}}),
    );
    let drawn = items(&doc, diameter);
    let (from, to) = shoulder(&drawn);
    assert!(near(to[0], bend[0]) && near(from[0], bend[0] - SHOULDER));
    let lines = texts(&drawn);
    let place = |w: &str| {
        lines
            .iter()
            .find(|(t, _, _)| t == w)
            .unwrap_or_else(|| panic!("no {w}: {lines:?}"))
            .1
    };
    let h = lines[0].2;
    let thru = place("THRU");
    assert!(near(thru[1] + h / 2.0, bend[1]), "{thru:?}");
    assert!(
        near(thru[0] + text_width("THRU", h), from[0] - GAP),
        "{thru:?}"
    );
    let second = place("2 PLACES");
    assert!(near(second[1], thru[1] - 1.6 * h), "{second:?}");
    assert!(near(second[0] + text_width("2 PLACES", h), from[0] - GAP));
    let written = pdf(&mut doc);
    assert!(pdf_writes(&written, "THRU", thru));
    assert!(pdf_writes(&written, "2 PLACES", second));
}

#[test]
fn a_short_asme_vertical_dimension_takes_its_value_outside_on_an_elbow() {
    let (mut doc, names) = asme_drawing();
    let side = &names[1];
    let id = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "linear",
               "along": "vertical",
               "picks": [{"body": "F2.b0", "edge": side, "point": "start"},
                         {"body": "F2.b0", "edge": side, "point": "end"}]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    // Beyond the view's right side, away from it: the shoulder to the
    // right, above the line's upper end, the value beside its end.
    let drawn = items(&doc, id);
    let (from, to) = shoulder(&drawn);
    assert!(from[0] > 105.0, "{from:?}");
    let [(words, at, h)] = &texts(&drawn)[..] else {
        panic!("{drawn:?}")
    };
    assert_eq!(words, "4");
    assert!(from[1] > 114.0, "{from:?}");
    assert!(
        near(at[0], to[0] + GAP) && near(at[1] + h / 2.0, to[1]),
        "{at:?}"
    );
    assert!(pdf_writes(&pdf(&mut doc), "4", *at));
    // Its text offset's across part to the left: the shoulder to the left.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": id,
               "text_offset": [0.0, 1.0]}),
    );
    let drawn = items(&doc, id);
    let (left, right) = shoulder(&drawn);
    assert!(near(right[0], from[0]), "{left:?} {right:?}");
    let [(_, at, h)] = &texts(&drawn)[..] else {
        panic!("{drawn:?}")
    };
    assert!(near(at[0] + text_width("4", *h), left[0] - GAP), "{at:?}");
    assert!(pdf_writes(&pdf(&mut doc), "4", *at));
}
