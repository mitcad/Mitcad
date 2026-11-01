// SPDX-License-Identifier: MIT
//! Annotations on drawings' views (mitcad#707 phase 2) on the mock kernel:
//! dimensions picked on views by model topology or by sheet points, with
//! #702's tolerances; a diameter and centre marks on a top view; notes,
//! datum labels, frames and finish symbols; series of dimensions; the
//! model changed under them, a broken one, undo, the design file, a
//! component's copy and the export.

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

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn sheet(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0].clone()
}

/// The first sheet's annotation of this id in the `drawing` query.
fn annotation(doc: &Document<MockKernel>, id: u64) -> Value {
    sheet(doc)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {id}"))
}

fn texts(doc: &Document<MockKernel>) -> Vec<String> {
    sheet(doc)["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["items"].as_array().unwrap().clone())
        .filter_map(|i| i["text"].as_str().map(str::to_owned))
        .collect()
}

fn classes(doc: &Document<MockKernel>) -> Vec<String> {
    sheet(doc)["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["class"].as_str().unwrap().to_owned())
        .collect()
}

/// The sheet y of a horizontal dimension's line as drawn: a level line of
/// its group (`A<id>`).
fn dimension_line_y(doc: &Document<MockKernel>, id: u64) -> f64 {
    let groups = sheet(doc)["groups"].clone();
    let group = groups
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == format!("A{id}").as_str())
        .unwrap_or_else(|| panic!("no group of annotation {id}"))
        .clone();
    group["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| {
            let line = i.get("line")?;
            let (a, b) = (point(&line[0]), point(&line[1]));
            (near(a[1], b[1]) && !near(a[0], b[0])).then_some(a[1])
        })
        .next()
        .unwrap_or_else(|| panic!("annotation {id} has no level line: {group}"))
}

fn point(value: &Value) -> [f64; 2] {
    [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()]
}

/// A 40 x 30 x 20 block at the origin and a 20 x 10 x 10 block beside it,
/// the kernel listing three edges of the first: the front's left and
/// right upright sides and a circle of radius 5 on its top; a drawing with
/// a front view at 1:1 (the block's front 65..105 x 110..130 on the
/// sheet) and the top view above it. Returns the edges' names.
fn annotated() -> (Document<MockKernel>, Vec<String>) {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [50, 0], "width": 20, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": R}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
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
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
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

fn pick(edge: &str) -> Value {
    json!({"body": "F2.b0", "edge": edge})
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

#[test]
fn dimensions_on_views_take_tolerances_and_follow_the_model() {
    let (mut doc, names) = annotated();
    // A linear dimension between the front's sides, by their names, with
    // #702's ± tolerance.
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "linear",
               "picks": [pick(&names[0]), pick(&names[1])],
               "tolerance": {"type": "symmetric", "value": 0.1}}),
    );
    assert_eq!(added["annotation"], 1);
    assert_eq!(undo_label(&doc), "Add dimension 1 to Drawing1");
    let linear = annotation(&doc, 1);
    assert_eq!(linear["broken"], false, "{linear}");
    assert_eq!(linear["along"], "horizontal");
    assert!(near(linear["value"].as_f64().unwrap(), 40.0), "{linear}");
    assert_eq!(linear["label"], "40 ±0.1");
    assert!(texts(&doc).iter().any(|t| t == "40 ±0.1"));
    // Placed for itself beyond the view's box: above or below it.
    let [lo, hi] = [point(&linear["box"][0]), point(&linear["box"][1])];
    assert!(hi[1] > 130.0 || lo[1] < 110.0, "{linear}");

    // The same picked by clicks on the sides: a point on each.
    let clicked = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "linear",
               "picks": [[65, 120], [105, 121]], "at": [85, 100]}),
    );
    let id = clicked["annotation"].as_u64().unwrap();
    let second = annotation(&doc, id);
    assert_eq!(second["picks"][0]["edge"], names[0].as_str());
    assert_eq!(second["picks"][1]["edge"], names[1].as_str());
    assert!(
        near(second["line_offset"].as_f64().unwrap(), -20.0),
        "{second}"
    );
    assert_eq!(second["label"], "40");

    // What a click picks, for the application to show.
    let picked = query(
        &doc,
        json!({"query": "drawing_pick", "drawing": "D1", "view": 1, "at": [65.5, 118]}),
    );
    assert_eq!(picked["pick"]["edge"], names[0].as_str());
    assert_eq!(picked["seen"], "line");
    let end = query(
        &doc,
        json!({"query": "drawing_pick", "drawing": "D1", "at": [65.5, 129.5]}),
    );
    assert_eq!(end["view"], 1);
    assert_eq!(end["pick"]["point"], "end", "{end}");
    let nothing = query(
        &doc,
        json!({"query": "drawing_pick", "drawing": "D1", "view": 1, "at": [85, 120]}),
    );
    assert!(nothing["pick"].is_null(), "{nothing}");

    // The model changes: the dimension follows its edge.
    doc.kernel().edge_curves.borrow_mut().insert(
        names[1].clone(),
        CurveGeometry::Line {
            start: [45.0, 0.0, 0.0],
            end: [45.0, 0.0, 20.0],
        },
    );
    let followed = annotation(&doc, 1);
    assert!(
        near(followed["value"].as_f64().unwrap(), 45.0),
        "{followed}"
    );
    assert_eq!(followed["label"], "45 ±0.1");

    // Dragged: its line moves across, its text along; then edited through
    // #702's fields.
    let before = linear["box"].clone();
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": 1,
               "move": [0, 5]}),
    );
    assert_eq!(undo_label(&doc), "Move dimension 1 of Drawing1");
    assert_ne!(annotation(&doc, 1)["box"], before);
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": 1,
               "tolerance": {"type": "fit", "code": "h7"}, "format": {"decimals": 1}}),
    );
    assert_eq!(annotation(&doc, 1)["label"], "45.0 h7");
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": 1,
               "tolerance": null, "format": {"decimals": null, "before": "2 x"}}),
    );
    assert_eq!(annotation(&doc, 1)["label"], "2 x 45");

    // Broken: the view no longer shows the body; it is not drawn, and says
    // why.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "subject": {"type": "bodies", "bodies": [{"body": "F4.b0"}]}}),
    );
    let broken = annotation(&doc, 1);
    assert_eq!(broken["broken"], true, "{broken}");
    assert!(
        broken["error"]
            .as_str()
            .unwrap()
            .contains("does not show the body F2.b0"),
        "{broken}"
    );
    // A broken annotation is a warning about it (mitcad#1040).
    assert_eq!(broken["level"], "warning", "{broken}");
    assert_eq!(broken["notices"][0]["level"], "warning", "{broken}");
    assert_eq!(broken["notices"][0]["message"], broken["error"], "{broken}");
    assert_eq!(broken["notices"][0]["target"], "1", "{broken}");
    assert!(broken["box"].is_array());
    let warnings = sheet(&doc)["warnings"].clone();
    assert!(
        warnings
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().starts_with("annotation 1 on view 1")),
        "{warnings}"
    );
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(annotation(&doc, 1)["broken"], false);

    // Deleted, and back.
    command(
        &mut doc,
        json!({"cmd": "delete_drawing_annotation", "drawing": "D1", "annotation": id}),
    );
    assert_eq!(
        undo_label(&doc),
        format!("Delete annotation {id} of Drawing1")
    );
    command(&mut doc, json!({"cmd": "undo"}));
    // The edge moved above, the dimension with it.
    assert_eq!(annotation(&doc, id)["label"], "45");
}

#[test]
fn round_edges_get_diameters_and_centre_marks() {
    let (mut doc, names) = annotated();
    let c = circle_center(&doc);
    // The top view's automatic centre mark on the circle it sees end on.
    let automatic = classes(&doc).iter().filter(|g| *g == "center-mark").count();
    assert!(automatic >= 1, "{:?}", classes(&doc));
    // Off: none.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 2, "center_marks": false}),
    );
    assert!(!classes(&doc).iter().any(|g| g == "center-mark"));
    // A diameter clicked on the circle's rim, and a centre mark on request.
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "diameter",
               "picks": [[c[0] + 5.0, c[1]]],
               "tolerance": {"type": "symmetric", "value": 0.05}}),
    );
    let diameter = annotation(&doc, added["annotation"].as_u64().unwrap());
    assert_eq!(diameter["picks"][0]["edge"], names[2].as_str());
    assert!(near(diameter["value"].as_f64().unwrap(), 10.0));
    assert_eq!(diameter["label"], "Ø10 ±0.05");
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "center_mark",
               "picks": [[c[0], c[1] - 5.0]]}),
    );
    assert_eq!(
        classes(&doc).iter().filter(|g| *g == "center-mark").count(),
        1
    );
    // A radius of a straight edge is refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "radius",
               "picks": [pick(&names[0])]}),
    );
    assert!(error.contains("not a circle"), "{error}");
    // Seen from the side the circle is a line: its diameter across it.
    let side = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "diameter",
               "picks": [pick(&names[2])]}),
    );
    assert_eq!(
        annotation(&doc, side["annotation"].as_u64().unwrap())["label"],
        "Ø10"
    );
    // An ordinate from the front's left side to its circle's centre.
    let ordinate = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "ordinate",
               "picks": [{"body": "F2.b0", "edge": names[2], "point": "center"},
                         [c[0], c[1]]]}),
    );
    let o = annotation(&doc, ordinate["annotation"].as_u64().unwrap());
    assert_eq!(o["label"], "0", "{o}");
}

#[test]
fn notes_symbols_and_series_are_drawn() {
    let (mut doc, names) = annotated();
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "note",
               "picks": [[65, 125]], "text": "DEBURR\nALL EDGES"}),
    );
    assert_eq!(undo_label(&doc), "Add note 1 to Drawing1");
    let note = annotation(&doc, 1);
    assert_eq!(note["label"], "DEBURR / ALL EDGES");
    assert!(note["at"].is_array(), "where it was clicked: {note}");
    let all = texts(&doc);
    assert!(all.iter().any(|t| t == "DEBURR") && all.iter().any(|t| t == "ALL EDGES"));
    // A note needs its text.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "note",
               "picks": [[65, 125]]}),
    );
    assert!(error.contains("needs a text"), "{error}");
    // A datum label takes the next free letter, a frame refers to it.
    let datum = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "datum",
               "picks": [pick(&names[0])]}),
    );
    let datum = annotation(&doc, datum["annotation"].as_u64().unwrap());
    assert_eq!(datum["letter"], "A");
    let frame = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "frame",
               "picks": [pick(&names[1])],
               "frame": {"characteristic": "perpendicularity", "value": 0.05, "datums": ["A"]}}),
    );
    let frame = annotation(&doc, frame["annotation"].as_u64().unwrap());
    assert_eq!(frame["label"], "⟂ | 0.05 | A");
    assert!(frame.get("warning").is_none(), "{frame}");
    assert!(classes(&doc).iter().any(|g| g == "feature-control-frame"));
    // A surface finish symbol with its requirement.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "finish",
               "picks": [pick(&names[1])],
               "finish": {"process": "removal", "text": "Ra 1.6"}}),
    );
    assert!(texts(&doc).iter().any(|t| t == "Ra 1.6"));
    // A text note on the sheet.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "type": "text",
               "at": [30, 60], "text": "GENERAL TOLERANCES ISO 2768-m"}),
    );
    assert!(
        texts(&doc)
            .iter()
            .any(|t| t == "GENERAL TOLERANCES ISO 2768-m")
    );
    // A baseline: from the first side to the second and to the circle's
    // centre, stacked.
    let series = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "linear",
               "series": "baseline", "along": "horizontal",
               "picks": [pick(&names[0]), pick(&names[1]),
                         {"body": "F2.b0", "edge": names[2], "point": "center"}]}),
    );
    let ids: Vec<u64> = series["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(undo_label(&doc), "Add 2 dimensions to Drawing1");
    let (a, b) = (annotation(&doc, ids[0]), annotation(&doc, ids[1]));
    assert_eq!(a["label"], "40");
    assert_eq!(b["label"], "20");
    // Their picks' middles lie at different heights (y 120 and 125 on the
    // sheet); the lines as drawn are stacked 7 mm apart above the view.
    let (ya, yb) = (
        dimension_line_y(&doc, ids[0]),
        dimension_line_y(&doc, ids[1]),
    );
    assert!(near(ya, 140.0), "{ya}");
    assert!(near(yb - ya, 7.0), "{ya} {yb}");
    // A chain: from each to the next, its lines in a row.
    let chain = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "linear",
               "series": "chain", "along": "horizontal", "at": [85, 100],
               "picks": [pick(&names[0]), {"body": "F2.b0", "edge": names[2], "point": "center"},
                         pick(&names[1])]}),
    );
    let ids: Vec<u64> = chain["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    assert_eq!(ids.len(), 2);
    let (ya, yb) = (
        dimension_line_y(&doc, ids[0]),
        dimension_line_y(&doc, ids[1]),
    );
    assert!(near(ya, 100.0) && near(yb, 100.0), "{ya} {yb}");
    assert_eq!(annotation(&doc, ids[0])["label"], "20");
    assert_eq!(annotation(&doc, ids[1])["label"], "20");

    // Saved and read again: the annotations and the note are kept.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let views = &file["drawings"][0]["sheets"][0]["views"];
    assert_eq!(views[0]["annotations"][0]["type"], "note");
    assert_eq!(
        file["drawings"][0]["sheets"][0]["notes"][0]["at"],
        json!([30.0, 60.0])
    );
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);

    // The export draws them.
    let dir = mitcad_model::testing::unique_temp_dir("annotated");
    let svg = dir.join("annotated.svg");
    let written = command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": "D1", "path": svg.to_string_lossy()}),
    );
    assert!(
        written["dimensions"].as_u64().unwrap_or(0) >= 2,
        "{written}"
    );
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(text.contains("DEBURR") && text.contains("Ra 1.6"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn annotations_refuse_what_they_cannot_be() {
    let (mut doc, names) = annotated();
    // Parallel edges make no angle.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "angle",
               "picks": [pick(&names[0]), pick(&names[1])]}),
    );
    assert!(error.contains("parallel"), "{error}");
    // Nothing near a click.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "linear",
               "picks": [[85, 120], [105, 120]]}),
    );
    assert!(error.contains("nothing to pick"), "{error}");
    // A linear dimension takes two picks; a text note no picks (a text in
    // a view is drawn in it, mitcad#729).
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "linear",
               "picks": [pick(&names[0])]}),
    );
    assert!(error.contains("takes 2 picks"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "sheet": 0, "type": "text",
               "at": [10, 10], "text": "X", "picks": [[85, 120]]}),
    );
    assert!(error.contains("on the sheet"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "arrow",
               "picks": []}),
    );
    assert!(error.contains("unknown annotation type"), "{error}");
    // A view's deletion takes its annotations along.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "note",
               "picks": [pick(&names[2])], "text": "HOLE"}),
    );
    command(
        &mut doc,
        json!({"cmd": "delete_drawing_view", "drawing": "D1", "view": 2}),
    );
    assert!(sheet(&doc)["annotations"].as_array().unwrap().is_empty());
}
