// SPDX-License-Identifier: MIT
//! Drawings following the model on the mock kernel: a new sheet takes no
//! text notes of the last one (mitcad#742); an occurrence moved to another
//! parent takes the drawings' views, dimensions and balloons along, or is
//! refused for a component's drawing it would leave (mitcad#743); retrieved
//! sketch dimensions of a placed component drawn where its occurrences put
//! them, once per placement (mitcad#744); a datum's new letter reaching the
//! frames of the model and of every drawing, and a model frame referring to
//! a drawing's datum label without a warning (mitcad#745).

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

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn file(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(&doc.to_json()).unwrap()
}

fn sheet_of(doc: &Document<MockKernel>, drawing: &str) -> Value {
    query(doc, json!({"query": "drawing", "drawing": drawing}))["sheets"][0].clone()
}

fn annotation_of(doc: &Document<MockKernel>, drawing: &str, id: u64) -> Value {
    sheet_of(doc, drawing)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {id} in {drawing}"))
}

fn added(doc: &mut Document<MockKernel>, cmd: Value) -> u64 {
    let result = command(doc, cmd);
    result["annotation"]
        .as_u64()
        .or_else(|| result["view"].as_u64())
        .unwrap_or_else(|| panic!("{result}"))
}

/// The model's annotation of this name in the `annotations` query.
fn model_annotation(doc: &Document<MockKernel>, name: &str) -> Value {
    query(doc, json!({"query": "annotations"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {name}"))
}

/// The named edges of a body, the kernel listing their geometry.
fn edges_of(doc: &Document<MockKernel>, body: &str) -> Vec<String> {
    doc.kernel().lists_geometry.set(true);
    let body: BodyUid = body.parse().unwrap();
    let names: Vec<String> = doc
        .kernel()
        .edges(doc.body_shape(body).unwrap())
        .unwrap()
        .into_iter()
        .filter_map(|e| e.name)
        .collect();
    assert!(names.len() >= 3, "{names:?}");
    names
}

fn set_line(doc: &Document<MockKernel>, edge: &str, start: [f64; 3], end: [f64; 3]) {
    doc.kernel()
        .edge_curves
        .borrow_mut()
        .insert(edge.to_owned(), CurveGeometry::Line { start, end });
}

/// A 40 x 30 x 20 block at the origin (F2.b0) whose front's left and
/// right upright sides are the first two edges listed; a drawing D1 with a
/// front view at 1:1. Returns the edges' names.
fn block() -> (Document<MockKernel>, Vec<String>) {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    let names = edges_of(&doc, "F2.b0");
    set_line(&doc, &names[0], [0.0, 0.0, 0.0], [0.0, 0.0, 20.0]);
    set_line(&doc, &names[1], [40.0, 0.0, 0.0], [40.0, 0.0, 20.0]);
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    (doc, names)
}

fn pick(edge: &str) -> Value {
    json!({"body": "F2.b0", "edge": edge})
}

#[test]
fn a_new_sheet_takes_no_text_notes_of_the_last() {
    let (mut doc, _) = block();
    let note = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "type": "text",
               "at": [30, 60], "text": "GENERAL TOLERANCES"}),
    );
    // mitcad#742: the last sheet's setup is copied, its notes are not.
    let sheet = command(
        &mut doc,
        json!({"cmd": "add_drawing_sheet", "drawing": "D1"}),
    );
    assert_eq!(sheet["sheet"], 1, "{sheet}");
    let saved = file(&doc);
    let sheets = &saved["drawings"][0]["sheets"];
    assert_eq!(sheets[0]["notes"][0]["id"], note, "{saved}");
    assert!(sheets[1].get("notes").is_none(), "{}", sheets[1]);
    // A note of its own on the new sheet takes the next id.
    let second = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "sheet": 1, "type": "text",
               "at": [30, 60], "text": "SECOND"}),
    );
    assert_ne!(second, note);
}

/// Plate (O1: a 40 x 30 x 5 block, F2.b0) and Holder (O2: a 20 x 10 x 10
/// block at x 100, F4.b0) in the root, both where their components are.
/// Returns the occurrences and Plate's edges: the first two its front's
/// upright sides, the third its front top edge.
fn plate_and_holder() -> (Document<MockKernel>, String, String, Vec<String>) {
    let mut doc = Document::new(MockKernel::default());
    let plate = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Plate"}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    let holder = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Holder"}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [100, 0], "width": 20, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": R}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    let names = edges_of(&doc, "F2.b0");
    set_line(&doc, &names[0], [0.0, 0.0, 0.0], [0.0, 0.0, 5.0]);
    set_line(&doc, &names[1], [40.0, 0.0, 0.0], [40.0, 0.0, 5.0]);
    set_line(&doc, &names[2], [0.0, 0.0, 5.0], [40.0, 0.0, 5.0]);
    (doc, plate, holder, names)
}

#[test]
fn a_moved_occurrence_takes_the_drawings_views_dimensions_and_balloons_along() {
    let (mut doc, plate, holder, names) = plate_and_holder();
    let on = |edge: &str| json!({"body": "F2.b0", "occurrence": plate, "edge": edge});
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    // A front view of Plate:1 alone, with a dimension through it.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1",
               "subject": {"type": "occurrence", "occurrence": plate}}),
    );
    let dimension = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "linear",
               "picks": [on(&names[0]), on(&names[1])]}),
    );
    // A top view of the design, its parts list and a balloon on Plate:1.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [250, 150], "scale": "1:1"}),
    );
    added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "parts_list",
               "at": [300, 280], "table": {"levels": "parts"}}),
    );
    let balloon = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "balloon",
               "picks": [on(&names[2])]}),
    );
    let intact = |doc: &Document<MockKernel>, path: &str| {
        let sheet = sheet_of(doc, "D1");
        for view in sheet["views"].as_array().unwrap() {
            assert_ne!(view["broken"], true, "{view}");
        }
        for id in [dimension, balloon] {
            let a = annotation_of(doc, "D1", id);
            assert_eq!(a["broken"], false, "{a}");
            assert_eq!(a["picks"][0]["occurrence"], path, "{a}");
        }
        let saved = file(doc);
        assert_eq!(
            saved["drawings"][0]["sheets"][0]["views"][0]["subject"]["occurrence"], path,
            "{saved}"
        );
    };
    intact(&doc, &plate);
    assert_eq!(annotation_of(&doc, "D1", dimension)["label"], "40");

    // Plate:1 into Holder:1 (mitcad#743): the drawing follows in the same
    // step and says so.
    let moved = command(
        &mut doc,
        json!({"cmd": "reparent_occurrence", "occurrence": plate, "parent": holder}),
    );
    let updated = moved["updated"].as_array().unwrap();
    assert!(updated.iter().any(|u| u == "Drawing1"), "{moved}");
    let path = format!("{holder}/{plate}");
    intact(&doc, &path);
    assert_eq!(annotation_of(&doc, "D1", dimension)["label"], "40");

    // Undo restores the old paths with the hierarchy.
    command(&mut doc, json!({"cmd": "undo"}));
    intact(&doc, &plate);
}

#[test]
fn a_components_drawing_refuses_a_move_out_of_it() {
    let mut doc = Document::new(MockKernel::default());
    let assembly = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Assembly"}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    // Plate, placed in Assembly.
    let plate = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Plate"}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    // Assembly's drawing shows Plate:1 by its path from Assembly.
    command(
        &mut doc,
        json!({"cmd": "add_drawing", "component": "Assembly", "size": "A3"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [150, 150], "scale": "1:1",
               "subject": {"type": "occurrence", "occurrence": plate}}),
    );
    let before = file(&doc);
    let error = refused(
        &mut doc,
        json!({"cmd": "reparent_occurrence", "occurrence": format!("{assembly}/{plate}"),
               "parent": ""}),
    );
    assert!(error.contains("Drawing1 of Assembly"), "{error}");
    assert_eq!(file(&doc), before);
}

/// The level lines (dimension lines across the view's x) of the retrieved
/// dimensions labelled `text` on D1's first sheet: their x from and to.
fn retrieved_lines(doc: &Document<MockKernel>, text: &str) -> Vec<[f64; 2]> {
    sheet_of(doc, "D1")["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["class"] == "retrieved-dimension")
        .filter(|g| {
            g["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["text"] == text)
        })
        .map(|g| {
            g["items"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|i| {
                    let line = i.get("line")?;
                    let (a, b) = (&line[0], &line[1]);
                    let (ax, ay) = (a[0].as_f64()?, a[1].as_f64()?);
                    let (bx, by) = (b[0].as_f64()?, b[1].as_f64()?);
                    (near(ay, by) && !near(ax, bx)).then_some([ax.min(bx), ax.max(bx)])
                })
                .unwrap_or_else(|| panic!("no level line: {g}"))
        })
        .collect()
}

#[test]
fn retrieved_dimensions_go_where_the_occurrences_put_their_sketch() {
    let mut doc = Document::new(MockKernel::default());
    // Plate:1 100 mm along X; Plate's sketch dimensions its width (40,
    // along X) and its height (30).
    let plate = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Plate",
               "transform": {"translation": [100, 0, 0]}}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [200, 150], "scale": "1:1", "retrieved_dimensions": true}),
    );
    // The top view: the sheet's x the model's x from the view's place.
    let place = |doc: &Document<MockKernel>| {
        sheet_of(doc, "D1")["views"][0]["place"][0]
            .as_f64()
            .unwrap()
    };
    // mitcad#744: the width's ends lie on the plate's front edge, x 100
    // to 140, not where the component's own coordinates have them.
    let x = place(&doc);
    let lines = retrieved_lines(&doc, "40");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        near(lines[0][0], x + 100.0) && near(lines[0][1], x + 140.0),
        "{lines:?} from {x}"
    );

    // Plate placed twice: the dimension once for each placement. The
    // copy sits further along X too, so that its width's line differs
    // from the first one's across the view's x.
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": plate,
               "transform": {"translation": [200, 60, 0]}}),
    );
    // The view keeps what it showed (mitcad#1293): updated from the model,
    // it shows the copy too.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "subject": {"type": "design"}}),
    );
    let x = place(&doc);
    let lines = retrieved_lines(&doc, "40");
    assert_eq!(lines.len(), 2, "{lines:?}");
    for line in &lines {
        assert!(near(line[1] - line[0], 40.0), "{lines:?}");
    }
    for at in [100.0, 200.0] {
        assert!(
            lines
                .iter()
                .any(|l| near(l[0], x + at) && near(l[1], x + at + 40.0)),
            "{lines:?} from {x}, none at {at}"
        );
    }
    let groups = sheet_of(&doc, "D1")["groups"].clone();
    let heights = groups
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["class"] == "retrieved-dimension")
        .filter(|g| {
            g["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["text"] == "30")
        })
        .count();
    assert_eq!(heights, 2);
}

#[test]
fn a_model_datums_new_letter_reaches_the_drawings_frames() {
    let (mut doc, names) = block();
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "datum",
               "a": {"body": "F2.b0", "face": format!("F2:start({R})")}}),
    );
    assert_eq!(model_annotation(&doc, "DatumA")["letter"], "A");
    let frame = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "frame",
               "picks": [pick(&names[1])],
               "frame": {"characteristic": "perpendicularity", "value": 0.05, "datums": ["A"]}}),
    );
    let before = annotation_of(&doc, "D1", frame);
    assert!(before.get("warning").is_none(), "{before}");

    // mitcad#745: DatumA lettered D; the drawing's frame follows it.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "DatumA", "letter": "D"}),
    );
    assert_eq!(model_annotation(&doc, "DatumD")["letter"], "D");
    let after = annotation_of(&doc, "D1", frame);
    assert_eq!(after["frame"]["datums"], json!(["D"]), "{after}");
    assert_eq!(after["label"], "⟂ | 0.05 | D");
    assert!(after.get("warning").is_none(), "{after}");
    // A later datum A is unrelated to the frame.
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "datum", "letter": "A",
               "a": {"body": "F2.b0", "face": format!("F2:end({R})")}}),
    );
    assert_eq!(
        annotation_of(&doc, "D1", frame)["frame"]["datums"],
        json!(["D"])
    );

    // Undo, twice: the old letter with the frame on it, in one step.
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(model_annotation(&doc, "DatumA")["letter"], "A");
    assert_eq!(
        annotation_of(&doc, "D1", frame)["frame"]["datums"],
        json!(["A"])
    );
}

#[test]
fn a_drawing_datums_new_letter_reaches_the_model_and_other_drawings() {
    let (mut doc, names) = block();
    // Drawing1's datum A, and a frame of its own on it.
    let datum = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "datum",
               "picks": [pick(&names[0])]}),
    );
    assert_eq!(annotation_of(&doc, "D1", datum)["letter"], "A");
    let tolerance = json!({"characteristic": "perpendicularity", "value": 0.05, "datums": ["A"]});
    let own = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "frame",
               "picks": [pick(&names[1])], "frame": tolerance}),
    );
    // A model frame on it: no warning, the label is there (mitcad#745).
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": {"body": "F2.b0", "face": format!("F2:end({R})")},
               "at": [10, 10, 20], "frame": tolerance}),
    );
    let model = model_annotation(&doc, "Frame1");
    assert!(model["label"].get("warning").is_none(), "{model}");
    // And a frame in a second drawing.
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D2", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    let other = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D2", "view": 1, "type": "frame",
               "picks": [pick(&names[1])], "frame": tolerance}),
    );

    // Drawing1's datum lettered E: every frame on A follows it.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": datum,
               "letter": "E"}),
    );
    assert_eq!(annotation_of(&doc, "D1", datum)["letter"], "E");
    for (drawing, id) in [("D1", own), ("D2", other)] {
        let frame = annotation_of(&doc, drawing, id);
        assert_eq!(frame["frame"]["datums"], json!(["E"]), "{frame}");
        assert!(frame.get("warning").is_none(), "{frame}");
    }
    let model = model_annotation(&doc, "Frame1");
    assert_eq!(model["frame"]["datums"], json!(["E"]), "{model}");
    assert!(model["label"].get("warning").is_none(), "{model}");

    // One undo step back to A everywhere.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(annotation_of(&doc, "D1", datum)["letter"], "A");
    assert_eq!(
        annotation_of(&doc, "D2", other)["frame"]["datums"],
        json!(["A"])
    );
    assert_eq!(
        model_annotation(&doc, "Frame1")["frame"]["datums"],
        json!(["A"])
    );
}
