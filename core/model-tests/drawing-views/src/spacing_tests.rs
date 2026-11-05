// SPDX-License-Identifier: MIT
//! Dimension spacing (mitcad#1173) on the mock kernel: the `drawing_snap`
//! query snaps a new dimension's placing click, and a placed one's drag,
//! to the spacing of the dimension placed before it in the drawing and to
//! multiples of it; the first dimension, a free place and what is no
//! dimension stay where they are clicked; the commands then place exactly
//! where the answer says. A note's drag snaps its leader to multiples of
//! 45 degrees (mitcad#1294).

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

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"))
}

fn sheet(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0].clone()
}

fn line_offset(doc: &Document<MockKernel>, id: u64) -> f64 {
    let annotations = sheet(doc)["annotations"].clone();
    let annotation = annotations
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap_or_else(|| panic!("no annotation {id}"))
        .clone();
    number(&annotation["line_offset"])
}

/// A 40 x 20 front of a block at 1:1 (the kernel listing its left and
/// right upright sides) on a drawing; returns the edges' names and the
/// front view's box on the sheet, the lowest and the highest y.
fn front_view() -> (Document<MockKernel>, Vec<String>, f64, f64) {
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
    assert!(names.len() >= 2, "{names:?}");
    {
        let mut curves = doc.kernel().edge_curves.borrow_mut();
        curves.insert(
            names[0].clone(),
            CurveGeometry::Line {
                start: [0.0, 0.0, 0.0],
                end: [0.0, 0.0, 20.0],
            },
        );
        curves.insert(
            names[1].clone(),
            CurveGeometry::Line {
                start: [40.0, 0.0, 0.0],
                end: [40.0, 0.0, 20.0],
            },
        );
    }
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    let view = sheet(&doc)["views"][0].clone();
    let (low, high) = (number(&view["box"][0][1]), number(&view["box"][1][1]));
    assert!(high > low, "{view}");
    (doc, names, low, high)
}

/// The command a placing click of a horizontal dimension between the two
/// sides at sheet y `y` would run.
fn dimension_at(names: &[String], y: f64) -> Value {
    json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "linear",
           "picks": [{"body": "F2.b0", "edge": names[0]}, {"body": "F2.b0", "edge": names[1]}],
           "at": [60, y]})
}

fn snap(doc: &Document<MockKernel>, command: &Value, free: bool) -> Value {
    query(
        doc,
        json!({"query": "drawing_snap", "command": command, "free": free}),
    )
}

/// Adds the dimension the command makes where the answer puts it; its id.
fn place(doc: &mut Document<MockKernel>, mut add: Value, answer: &Value) -> u64 {
    add["at"] = answer["at"].clone();
    command(doc, add)["annotation"].as_u64().unwrap()
}

#[test]
fn dimensions_snap_to_the_spacing_of_the_one_placed_before() {
    let (mut doc, names, low, high) = front_view();

    // The first dimension, 12 below the view: nothing to snap to, where
    // it is clicked.
    let first = dimension_at(&names, low - 12.0);
    let answer = snap(&doc, &first, false);
    assert_eq!(answer["snapped"], false, "{answer}");
    assert_eq!(answer["at"], json!([60.0, low - 12.0]), "{answer}");
    assert_eq!(answer["family"], "line", "{answer}");
    assert!(near(number(&answer["beyond"]), 12.0), "{answer}");
    let one = place(&mut doc, first, &answer);
    assert!(near(line_offset(&doc, one), number(&answer["line_offset"])));
    // Its picks' middle, across the sheet: a line_offset counts from it.
    let middle = low - 12.0 - line_offset(&doc, one);

    // The second, on the other side 13.5 out: the first's spacing, 12.
    let second = dimension_at(&names, high + 13.5);
    let answer = snap(&doc, &second, false);
    assert_eq!(answer["snapped"], true, "{answer}");
    assert!(near(number(&answer["spacing"]), 12.0), "{answer}");
    assert_eq!(answer["multiple"], 1, "{answer}");
    assert_eq!(answer["from"], one, "{answer}");
    assert!(near(number(&answer["at"][1]), high + 12.0), "{answer}");
    assert!(near(number(&answer["beyond"]), 12.0), "{answer}");
    assert!(
        near(number(&answer["line_offset"]), high + 12.0 - middle),
        "{answer}"
    );
    // Placed freely (Alt): where it is clicked.
    let free = snap(&doc, &second, true);
    assert_eq!(free["snapped"], false, "{free}");
    assert!(near(number(&free["at"][1]), high + 13.5), "{free}");
    assert!(near(number(&free["beyond"]), 13.5), "{free}");
    // The command places it where the answer said.
    let two = place(&mut doc, second, &answer);
    assert!(near(line_offset(&doc, two), number(&answer["line_offset"])));

    // A third below, 23 out: two spacings, stacked outside the first.
    let answer = snap(&doc, &dimension_at(&names, low - 23.0), false);
    assert_eq!(answer["multiple"], 2, "{answer}");
    assert_eq!(answer["from"], two, "{answer}");
    assert!(near(number(&answer["at"][1]), low - 24.0), "{answer}");
    let three = place(&mut doc, dimension_at(&names, low - 23.0), &answer);

    // The third's spacing is its step from the first inside it, 12, not
    // its whole 24: a fourth 37 out snaps to three spacings, 36.
    let answer = snap(&doc, &dimension_at(&names, low - 37.0), false);
    assert_eq!(answer["snapped"], true, "{answer}");
    assert!(near(number(&answer["spacing"]), 12.0), "{answer}");
    assert_eq!(answer["multiple"], 3, "{answer}");
    assert_eq!(answer["from"], three, "{answer}");
    assert!(near(number(&answer["at"][1]), low - 36.0), "{answer}");
    // Between two multiples, farther than the reach from both: where it
    // is clicked.
    let answer = snap(&doc, &dimension_at(&names, low - 18.0), false);
    assert_eq!(answer["snapped"], false, "{answer}");
    assert!(near(number(&answer["at"][1]), low - 18.0), "{answer}");

    // The second dragged 11 farther out: the third's spacing (the
    // dimension placed last but it), two spacings out.
    let before = line_offset(&doc, two);
    let drag = json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": two,
                      "move": [4, 11]});
    let answer = snap(&doc, &drag, false);
    assert_eq!(answer["snapped"], true, "{answer}");
    assert!(near(number(&answer["move"][0]), 4.0), "{answer}");
    assert!(near(number(&answer["move"][1]), 12.0), "{answer}");
    assert_eq!(answer["multiple"], 2, "{answer}");
    assert_eq!(answer["from"], three, "{answer}");
    assert!(
        near(number(&answer["line_offset"]), before + 12.0),
        "{answer}"
    );
    let free = snap(&doc, &drag, true);
    assert_eq!(free["move"], json!([4.0, 11.0]), "{free}");
    let mut moved = drag.clone();
    moved["move"] = answer["move"].clone();
    command(&mut doc, moved);
    assert!(near(line_offset(&doc, two), before + 12.0));
}

#[test]
fn what_is_no_dimension_is_placed_as_clicked() {
    let (mut doc, names, low, _) = front_view();
    let first = dimension_at(&names, low - 12.0);
    let answer = snap(&doc, &first, false);
    place(&mut doc, first, &answer);
    // A note on a leader: its place as clicked.
    let note = json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "note",
                      "picks": [{"body": "F2.b0", "edge": names[0]}], "text": "A",
                      "at": [40, low - 13.0]});
    let answer = snap(&doc, &note, false);
    assert_eq!(answer, json!({"at": [40.0, low - 13.0], "snapped": false}));
    // Another command than the two: refused.
    let wrong = doc.query(
        &json!({"query": "drawing_snap",
                "command": {"cmd": "delete_drawing_annotation", "drawing": "D1", "annotation": 1}})
        .to_string(),
    );
    assert!(wrong.is_err(), "{wrong:?}");
}

#[test]
fn a_notes_drag_snaps_its_leader_to_45_degrees() {
    // mitcad#1294: a note's leader runs from its arrow to its text's
    // shoulder, `text_offset` from the arrow; a drag that leaves it 47
    // degrees up snaps it to 45, its length kept; free, as dragged.
    let (mut doc, names, low, _) = front_view();
    let note = json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "note",
                      "picks": [{"body": "F2.b0", "edge": names[0]}], "text": "A",
                      "at": [40, low - 13.0]});
    let id = command(&mut doc, note)["annotation"].as_u64().unwrap();
    let offset = |doc: &Document<MockKernel>| {
        let annotations = sheet(doc)["annotations"].clone();
        let a = annotations
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id)
            .unwrap()
            .clone();
        [number(&a["text_offset"][0]), number(&a["text_offset"][1])]
    };
    let before = offset(&doc);
    let r = 20.0;
    let (s, c) = 47.0_f64.to_radians().sin_cos();
    let delta = [r * c - before[0], r * s - before[1]];
    let drag = json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": id,
                      "move": delta});
    let answer = snap(&doc, &drag, false);
    assert_eq!(answer["snapped"], true, "{answer}");
    assert!(near(number(&answer["angle"]), 45.0), "{answer}");
    let (u, v) = (
        number(&answer["text_offset"][0]),
        number(&answer["text_offset"][1]),
    );
    assert!(near(u, v) && near(u.hypot(v), r), "{answer}");
    // Free (the application's modifier held): as dragged.
    let free = snap(&doc, &drag, true);
    assert_eq!(free["snapped"], false, "{free}");
    assert!(near(number(&free["move"][0]), delta[0]), "{free}");
    assert!(near(number(&free["move"][1]), delta[1]), "{free}");
    // Ten degrees off: as dragged.
    let (s, c) = 55.0_f64.to_radians().sin_cos();
    let off = json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": id,
                     "move": [r * c - before[0], r * s - before[1]]});
    assert_eq!(snap(&doc, &off, false)["snapped"], false);
    // The command moves it where the answer said.
    let mut moved = drag.clone();
    moved["move"] = answer["move"].clone();
    command(&mut doc, moved);
    let after = offset(&doc);
    assert!(near(after[0], u) && near(after[1], v), "{after:?}");
}
