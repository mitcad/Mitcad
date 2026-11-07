// SPDX-License-Identifier: MIT
//! The model's items in a drawing's views (mitcad#1406) on the mock
//! kernel: Model Items (`drawing_model_items`) switches the sketch
//! dimensions and the model's annotations on for a sheet's views; each
//! model annotation is drawn once, in the first view that sees its plane
//! square on (a hole callout where it sees its hole end on), with the
//! model's value, so a changed model changes it; one hidden in the model is
//! not brought in; a sketch dimension or a model annotation drawn in one
//! view is not drawn again in another; and one kept as the drawing's own
//! (`keep_drawing_model_items`) is moved without changing the model.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::SurfaceGeometry;
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

fn sheet(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0].clone()
}

/// The model's annotations the first sheet's views bring in: each name
/// with the view that draws it, in the sheet's order.
fn drawn(doc: &Document<MockKernel>) -> Vec<(String, u64)> {
    sheet(doc)["model_items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["name"].as_str().unwrap().to_owned(),
                e["view"].as_u64().unwrap(),
            )
        })
        .collect()
}

/// The `model_items` entry of the model's annotation `name`.
fn item(doc: &Document<MockKernel>, name: &str) -> Value {
    sheet(doc)["model_items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("{name} is not drawn: {}", sheet(doc)["model_items"]))
}

/// The texts of the sheet's groups of this class.
fn texts_of(doc: &Document<MockKernel>, class: &str) -> Vec<String> {
    sheet(doc)["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["class"] == class)
        .flat_map(|g| g["items"].as_array().unwrap().clone())
        .filter_map(|i| i["text"].as_str().map(str::to_owned))
        .collect()
}

fn groups_of(doc: &Document<MockKernel>, class: &str) -> usize {
    sheet(doc)["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["class"] == class)
        .count()
}

/// A 20 x 20 block (F2.b0) on XY, as high as the parameter `height` (20
/// mm), its sketch's rectangle dimensioned by its width and height, a 6.6
/// mm hole 10 deep into its top at (10, 10) (F3, its wall a cylinder along
/// Z); the model's annotations: the block's height between its top and
/// bottom (Offset1, in a plane square to Y), datum A on its top and a hole
/// callout on the hole (HoleCallout1). A drawing with three views at 1:1:
/// the front (1), the top above it (2) and the right side (3).
fn annotated_part() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    // The hole's wall as the kernel measures it: its axis says which view
    // looks into the hole.
    doc.kernel().surfaces.borrow_mut().insert(
        "F3:hole0.wall".to_owned(),
        SurfaceGeometry::Cylinder {
            origin: [10.0, 10.0, 10.0],
            axis: [0.0, 0.0, 1.0],
            radius: 3.3,
        },
    );
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "height", "value": "20 mm"},
            {"cmd": "add_parameter", "name": "drill", "value": "6.6 mm"},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": "height"}, "operation": "new_body"}},
            {"cmd": "add_feature", "def": {"type": "hole",
              "placement": {"type": "face", "body": "F2.b0", "face": format!("F2:end({R})"),
                            "points": [[10, 10, 20]]},
              "diameter": "drill", "extent": {"type": "distance", "depth": 10}}},
        ]),
    );
    let top = json!({"body": "F2.b0", "face": format!("F2:end({R})")});
    let bottom = json!({"body": "F2.b0", "face": format!("F2:start({R})")});
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "a": top, "b": bottom, "plane": {"normal": [0, 1, 0]}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "datum", "a": top}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "hole_callout",
               "a": {"body": "F2.b0", "face": "F3:hole0.wall"}, "at": [10.0, 13.3, 20.0]}),
    );
    let names: Vec<String> = query(&doc, json!({"query": "annotations"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["Offset1", "DatumA", "HoleCallout1"], "{names:?}");
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
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "projected", "parent": 1,
               "side": "right"}),
    );
    doc
}

#[test]
fn model_items_draw_each_annotation_once_in_the_view_that_faces_it() {
    let mut doc = annotated_part();
    // Nothing of the model's until Model Items.
    assert!(drawn(&doc).is_empty());
    assert_eq!(groups_of(&doc, "retrieved-dimension"), 0);
    let answer = command(
        &mut doc,
        json!({"cmd": "drawing_model_items", "drawing": "D1"}),
    );
    // No views given: all the sheet's, both kinds on, one undo step.
    assert_eq!(answer["views"], json!([1, 2, 3]), "{answer}");
    assert_eq!(answer["switched"], json!([1, 2, 3]), "{answer}");
    assert_eq!(undo_label(&doc), "Model Items on Sheet1 of Drawing1");
    for view in sheet(&doc)["views"].as_array().unwrap() {
        assert_eq!(view["retrieved_dimensions"], true, "{view}");
        assert_eq!(view["model_annotations"], true, "{view}");
    }
    // Each once: the height in the front view, which sees its plane square
    // on; the datum on the top and the hole's callout in the top view.
    let mut items = drawn(&doc);
    items.sort();
    assert_eq!(
        items,
        [
            ("DatumA".to_owned(), 2),
            ("HoleCallout1".to_owned(), 2),
            ("Offset1".to_owned(), 1),
        ],
        "{items:?}"
    );
    // Drawn by the drawing's engine with the model's words and values.
    let height = item(&doc, "Offset1");
    assert_eq!(height["kind"], "offset", "{height}");
    assert_eq!(height["type"], "linear", "{height}");
    assert_eq!(height["label"], "20", "{height}");
    assert!(!height["box"].is_null(), "{height}");
    assert_eq!(item(&doc, "DatumA")["type"], "datum");
    assert_eq!(item(&doc, "DatumA")["label"], "A");
    let callout = item(&doc, "HoleCallout1");
    assert_eq!(callout["type"], "note", "{callout}");
    assert_eq!(callout["label"], "Ø6.6 ↧ 10", "{callout}");
    assert_eq!(groups_of(&doc, "model-annotation"), 3);
    let texts = texts_of(&doc, "model-annotation");
    for text in ["20", "A", "Ø6.6 ↧ 10"] {
        assert!(texts.iter().any(|t| t == text), "{text} in {texts:?}");
    }
    // The sketch's width and height too, once each, in the top view.
    assert_eq!(groups_of(&doc, "retrieved-dimension"), 2);

    // The model changed: the drawn values follow it.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 30}),
    );
    assert_eq!(item(&doc, "Offset1")["label"], "30");
    assert!(texts_of(&doc, "model-annotation").iter().any(|t| t == "30"));
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "drill", "value": 8}),
    );
    assert_eq!(item(&doc, "HoleCallout1")["label"], "Ø8 ↧ 10");

    // Saved and read again: the views keep their flags.
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let views = &saved["drawings"][0]["sheets"][0]["views"];
    assert_eq!(views[0]["model_annotations"], true, "{views}");
    assert_eq!(views[0]["retrieved_dimensions"], true, "{views}");

    // Undo: the views as they were.
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(drawn(&doc).is_empty(), "{:?}", drawn(&doc));
}

#[test]
fn model_items_take_the_views_given_and_the_kinds_asked_for() {
    let mut doc = annotated_part();
    // The top view alone, its model annotations only.
    let answer = command(
        &mut doc,
        json!({"cmd": "drawing_model_items", "drawing": "D1", "views": [2],
               "sketch_dimensions": false}),
    );
    assert_eq!(answer["switched"], json!([2]), "{answer}");
    let views = sheet(&doc)["views"].clone();
    assert_eq!(views[1]["model_annotations"], true, "{views}");
    assert_eq!(views[1]["retrieved_dimensions"], false, "{views}");
    assert_eq!(views[0]["model_annotations"], false, "{views}");
    let mut items = drawn(&doc);
    items.sort();
    assert_eq!(
        items,
        [("DatumA".to_owned(), 2), ("HoleCallout1".to_owned(), 2)],
        "{items:?}"
    );
    assert_eq!(groups_of(&doc, "retrieved-dimension"), 0);
    // Asked again: nothing to switch, no undo step.
    let answer = command(
        &mut doc,
        json!({"cmd": "drawing_model_items", "drawing": "D1", "views": [2],
               "sketch_dimensions": false}),
    );
    assert_eq!(answer["switched"], json!([]), "{answer}");
    // Refused: neither kind, a view the sheet has not.
    let error = refused(
        &mut doc,
        json!({"cmd": "drawing_model_items", "drawing": "D1", "sketch_dimensions": false,
               "model_annotations": false}),
    );
    assert!(error.contains("neither was asked for"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "drawing_model_items", "drawing": "D1", "views": [9]}),
    );
    assert!(error.contains("has no view 9"), "{error}");
    // The flag by itself on a view, as the view's panel sets it.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "model_annotations": true}),
    );
    assert!(drawn(&doc).contains(&("Offset1".to_owned(), 1)));
}

#[test]
fn an_annotation_hidden_in_the_model_is_not_brought_in() {
    let mut doc = annotated_part();
    command(
        &mut doc,
        json!({"cmd": "drawing_model_items", "drawing": "D1"}),
    );
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1", "visible": false}),
    );
    let names: Vec<String> = drawn(&doc).into_iter().map(|(n, _)| n).collect();
    assert!(!names.contains(&"Offset1".to_owned()), "{names:?}");
    assert!(names.contains(&"DatumA".to_owned()), "{names:?}");
    // Shown again: back in the front view.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1", "visible": true}),
    );
    assert!(drawn(&doc).contains(&("Offset1".to_owned(), 1)));
}

#[test]
fn an_item_drawn_in_one_view_is_not_drawn_again_in_another() {
    let mut doc = annotated_part();
    // A back view and a bottom view: the back sees the height's plane
    // square on as the front does, the bottom the sketch and the datum's
    // plane as the top does.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "back",
               "at": [320, 120], "scale": "1:1"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "bottom",
               "at": [320, 220], "scale": "1:1"}),
    );
    command(
        &mut doc,
        json!({"cmd": "drawing_model_items", "drawing": "D1"}),
    );
    let items = drawn(&doc);
    for name in ["Offset1", "DatumA", "HoleCallout1"] {
        let views: Vec<u64> = items
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| *v)
            .collect();
        assert_eq!(views.len(), 1, "{name} in views {views:?}");
    }
    // In the first view on the sheet that sees each.
    assert!(items.contains(&("Offset1".to_owned(), 1)), "{items:?}");
    assert!(items.contains(&("DatumA".to_owned(), 2)), "{items:?}");
    // The sketch's width and height once each, though the bottom view sees
    // them too.
    assert_eq!(groups_of(&doc, "retrieved-dimension"), 2);
    // Without the front view's, the back view draws the height.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "model_annotations": false}),
    );
    assert!(
        drawn(&doc).contains(&("Offset1".to_owned(), 4)),
        "{:?}",
        drawn(&doc)
    );
}

#[test]
fn a_model_annotation_is_kept_as_the_drawings_own_without_changing_the_model() {
    let mut doc = annotated_part();
    command(
        &mut doc,
        json!({"cmd": "drawing_model_items", "drawing": "D1"}),
    );
    let model_before = query(&doc, json!({"query": "annotations"}));
    let kept = command(
        &mut doc,
        json!({"cmd": "keep_drawing_model_items", "drawing": "D1", "view": 1,
               "names": ["Offset1"]}),
    );
    assert_eq!(kept["kept"], json!(["Offset1"]), "{kept}");
    assert_eq!(undo_label(&doc), "Keep Offset1 in Drawing1");
    let id = kept["annotations"][0].as_u64().unwrap();
    // The drawing's own linear dimension on the block's top and bottom,
    // measuring the same, marked as kept from the model's.
    let own = sheet(&doc)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap();
    assert_eq!(own["type"], "linear", "{own}");
    assert_eq!(own["view"], 1, "{own}");
    assert_eq!(own["kept_from"], "Offset1", "{own}");
    assert_eq!(own["broken"], false, "{own}");
    assert!(
        (own["value"].as_f64().unwrap() - 20.0).abs() < 1e-6,
        "{own}"
    );
    // The model's is no longer brought in on this sheet: drawn once.
    let names: Vec<String> = drawn(&doc).into_iter().map(|(n, _)| n).collect();
    assert!(!names.contains(&"Offset1".to_owned()), "{names:?}");
    // Moved on the sheet: the drawing's, the model unchanged.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": id,
               "move": [8, 0]}),
    );
    assert_eq!(query(&doc, json!({"query": "annotations"})), model_before);
    // Renamed in the model: the kept one follows, so it is not brought in
    // again.
    command(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Offset1", "new_name": "Height"}),
    );
    let names: Vec<String> = drawn(&doc).into_iter().map(|(n, _)| n).collect();
    assert!(!names.contains(&"Height".to_owned()), "{names:?}");
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let own = &saved["drawings"][0]["sheets"][0]["views"][0]["annotations"][0];
    assert_eq!(own["kept_from"], "Height", "{own}");
    // Refused: a view that does not draw it, a datum letter twice.
    let error = refused(
        &mut doc,
        json!({"cmd": "keep_drawing_model_items", "drawing": "D1", "view": 3,
               "names": ["DatumA"]}),
    );
    assert!(error.contains("does not draw DatumA"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "keep_drawing_model_items", "drawing": "D1", "view": 3}),
    );
    assert!(
        error.contains("draws no annotations of the model"),
        "{error}"
    );
}
