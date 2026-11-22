// SPDX-License-Identifier: MIT
//! A drawing's own hole callouts (mitcad#2054) on the mock kernel: a
//! `hole_callout` annotation on a hole seen end on writes the lines the
//! model's hole callout writes, worked out whenever the sheet is drawn, so
//! that they follow the hole; its own texts before and after them stay
//! through a change, and a text in place of them (`format.replace`) is
//! drawn while the `drawing` query still gives the hole's lines. A tapped
//! hole gives its thread, a round face of no hole its diameter, and a pick
//! of no hole or circle is refused. A model hole callout kept as the
//! drawing's own is such a callout too.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::{CurveGeometry, SurfaceGeometry};
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::{Axis, Cylinder, Kernel};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
/// The block's side that the kernel makes a round face along x (no hole's).
const ROUND: &str = "F2:side(c1[c4,c2])";

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

/// The first sheet's annotation of this id in the `drawing` query.
fn annotation(doc: &Document<MockKernel>, id: u64) -> Value {
    sheet(doc)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {id}: {}", sheet(doc)["annotations"]))
}

/// The texts the sheet draws for the drawing's hole callouts.
fn callout_texts(doc: &Document<MockKernel>) -> Vec<String> {
    sheet(doc)["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["class"] == "hole-callout")
        .flat_map(|g| g["items"].as_array().unwrap().clone())
        .filter_map(|i| i["text"].as_str().map(str::to_owned))
        .collect()
}

/// A view's place on the sheet.
fn view_place(doc: &Document<MockKernel>, id: u64) -> [f64; 2] {
    let view = sheet(doc)["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no view {id}"));
    [
        view["place"][0].as_f64().unwrap(),
        view["place"][1].as_f64().unwrap(),
    ]
}

/// A 20 x 20 block (F2.b0) on XY, as high as the parameter `height` (20
/// mm), with a hole (F3) into its top at (10, 10), its drill the parameter
/// `drill` (6.6 mm) and its depth `depth` (10 mm), with `extra` fields (its
/// kind, thread); its wall a cylinder along Z. The block's side `ROUND` is
/// a round face of radius 3 along X, of no hole; one of its edges a
/// straight line (returned). A drawing with the front view at 1:1 (1) and
/// the top view above it (2), which sees the hole end on.
fn holed_drawing(extra: Value) -> (Document<MockKernel>, String) {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    let mut hole = json!({"type": "hole",
        "placement": {"type": "face", "body": "F2.b0", "face": format!("F2:end({R})"),
                      "points": [[10, 10, 20]]},
        "diameter": "drill", "extent": {"type": "distance", "depth": "depth"}});
    for (key, field) in extra.as_object().unwrap() {
        hole[key] = field.clone();
    }
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "height", "value": "20 mm"},
            {"cmd": "add_parameter", "name": "drill", "value": "6.6 mm"},
            {"cmd": "add_parameter", "name": "depth", "value": "10 mm"},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": "height"}, "operation": "new_body"}},
            {"cmd": "add_feature", "def": hole},
        ]),
    );
    doc.kernel().lists_geometry.set(true);
    {
        let mut surfaces = doc.kernel().surfaces.borrow_mut();
        surfaces.insert(
            "F3:hole0.wall".to_owned(),
            SurfaceGeometry::Cylinder {
                origin: [10.0, 10.0, 10.0],
                axis: [0.0, 0.0, 1.0],
                radius: 3.3,
            },
        );
        surfaces.insert(
            ROUND.to_owned(),
            SurfaceGeometry::Cylinder {
                origin: [0.0, 3.0, 17.0],
                axis: [1.0, 0.0, 0.0],
                radius: 3.0,
            },
        );
    }
    doc.kernel().face_cylinders.borrow_mut().insert(
        ROUND.to_owned(),
        Cylinder {
            axis: Axis {
                origin: [0.0, 3.0, 17.0],
                direction: [1.0, 0.0, 0.0],
            },
            radius: 3.0,
            length: 20.0,
            internal: false,
        },
    );
    let body: BodyUid = "F2.b0".parse().unwrap();
    let names: Vec<String> = doc
        .kernel()
        .edges(doc.body_shape(body).unwrap())
        .unwrap()
        .into_iter()
        .filter_map(|e| e.name)
        .collect();
    let straight = names
        .iter()
        .find(|n| !n.contains("side(c1[") && !n.contains("F3:"))
        .cloned()
        .expect("an edge away from the round face and the hole");
    let mut curves = doc.kernel().edge_curves.borrow_mut();
    curves.insert(
        straight.clone(),
        CurveGeometry::Line {
            start: [0.0, 20.0, 20.0],
            end: [20.0, 20.0, 20.0],
        },
    );
    // The round faces' ends are circles, as a kernel gives them: the
    // drawing sees a round face by them (its extent along its axis). The
    // hole's rim at the block's top.
    let rim = names
        .iter()
        .find(|n| n.contains("F3:hole0.wall"))
        .expect("an edge of the hole's wall");
    curves.insert(
        rim.clone(),
        CurveGeometry::Circle {
            center: [10.0, 10.0, 20.0],
            normal: [0.0, 0.0, 1.0],
            radius: 3.3,
            start: [13.3, 10.0, 20.0],
            end: [13.3, 10.0, 20.0],
        },
    );
    // The round side's two ends, at x = 0 and x = 20.
    let ends: Vec<&String> = names.iter().filter(|n| n.contains("side(c1[")).collect();
    assert!(ends.len() >= 2, "two edges of the round face: {names:?}");
    for (edge, x) in ends.into_iter().zip([0.0, 20.0]) {
        curves.insert(
            edge.clone(),
            CurveGeometry::Circle {
                center: [x, 3.0, 17.0],
                normal: [1.0, 0.0, 0.0],
                radius: 3.0,
                start: [x, 6.0, 17.0],
                end: [x, 6.0, 17.0],
            },
        );
    }
    drop(curves);
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
    (doc, straight)
}

/// A counterbored hole: ⌴ Ø11 ↧ 6.8.
fn counterbored() -> (Document<MockKernel>, String) {
    holed_drawing(json!({"kind": {"type": "counterbore", "diameter": 11, "depth": 6.8}}))
}

/// A hole callout added on the top view to the hole's wall, its text up
/// and right of the hole; its id.
fn add_callout(doc: &mut Document<MockKernel>) -> u64 {
    let place = view_place(doc, 2);
    let added = command(
        doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "hole_callout",
               "picks": [{"body": "F2.b0", "face": "F3:hole0.wall"}],
               "at": [place[0] + 25.0, place[1] + 25.0]}),
    );
    added["annotation"].as_u64().unwrap()
}

fn format(doc: &mut Document<MockKernel>, id: u64, format: Value) {
    command(
        doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": id,
               "format": format}),
    );
}

#[test]
fn a_dragged_hole_callout_previews_the_same_group_as_its_move_draws() {
    let (mut doc, _) = counterbored();
    let id = add_callout(&mut doc);
    format(
        &mut doc,
        id,
        json!({"before": "TYP\n", "after": "2 PLACES"}),
    );
    let group_id = format!("A{id}");
    for (delta, free) in [([-55.0, 0.0], false), ([55.0, 0.0], true)] {
        let moved = json!({"cmd": "edit_drawing_annotation", "drawing": "D1",
                           "annotation": id, "move": delta});
        let before = sheet(&doc);
        let snap = query(
            &doc,
            json!({"query": "drawing_snap", "command": moved, "free": free}),
        );
        assert!(snap["preview"]["group"]["items"].is_array(), "{snap}");
        assert_eq!(sheet(&doc), before, "the preview is a read-only query");

        let mut moved = moved;
        moved["move"] = snap["move"].clone();
        command(&mut doc, moved);
        let after = sheet(&doc);
        let group = after["groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|group| group["id"] == group_id)
            .unwrap();
        assert_eq!(&snap["preview"]["group"], group);
        let entry = after["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == id)
            .unwrap();
        assert_eq!(snap["preview"]["box"], entry["box"]);
    }
}

#[test]
fn a_hole_callout_writes_its_hole_and_follows_it() {
    let (mut doc, _) = counterbored();
    let id = add_callout(&mut doc);
    assert_eq!(
        undo_label(&doc),
        format!("Add hole callout {id} to Drawing1")
    );
    let callout = annotation(&doc, id);
    assert_eq!(callout["type"], "hole_callout", "{callout}");
    assert_eq!(callout["broken"], false, "{callout}");
    assert_eq!(
        callout["lines"],
        json!(["Ø6.6 ↧ 10", "⌴ Ø11 ↧ 6.8"]),
        "{callout}"
    );
    assert_eq!(callout["label"], "Ø6.6 ↧ 10 / ⌴ Ø11 ↧ 6.8", "{callout}");
    assert_eq!(callout["measured"], "Ø6.6 ↧ 10\n⌴ Ø11 ↧ 6.8", "{callout}");
    // Drawn on a leader as a note, its lines as the model's callout writes
    // them.
    let texts = callout_texts(&doc);
    for line in ["Ø6.6 ↧ 10", "⌴ Ø11 ↧ 6.8"] {
        assert!(texts.iter().any(|t| t == line), "{line} in {texts:?}");
    }

    // Its own texts before and after, one undo step.
    format(
        &mut doc,
        id,
        json!({"before": "TYP", "after": "BOTH SIDES"}),
    );
    assert_eq!(
        undo_label(&doc),
        format!("Edit hole callout {id} of Drawing1")
    );
    assert_eq!(
        annotation(&doc, id)["label"],
        "TYP Ø6.6 ↧ 10 / ⌴ Ø11 ↧ 6.8 BOTH SIDES"
    );
    // The hole's diameter and depth changed: the lines follow, the own
    // texts stay.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "drill", "value": 8}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "depth", "value": 12}),
    );
    let callout = annotation(&doc, id);
    assert_eq!(
        callout["lines"],
        json!(["Ø8 ↧ 12", "⌴ Ø11 ↧ 6.8"]),
        "{callout}"
    );
    assert_eq!(
        callout["label"], "TYP Ø8 ↧ 12 / ⌴ Ø11 ↧ 6.8 BOTH SIDES",
        "{callout}"
    );
    assert!(
        callout_texts(&doc).iter().any(|t| t == "TYP Ø8 ↧ 12"),
        "{:?}",
        callout_texts(&doc)
    );

    // Replace: the drawing's lines in place of the hole's, which the
    // query still gives.
    format(&mut doc, id, json!({"replace": "Ø9 THRU\nSPOTFACE Ø14"}));
    let callout = annotation(&doc, id);
    assert_eq!(
        callout["label"], "TYP Ø9 THRU / SPOTFACE Ø14 BOTH SIDES",
        "{callout}"
    );
    assert_eq!(
        callout["lines"],
        json!(["Ø8 ↧ 12", "⌴ Ø11 ↧ 6.8"]),
        "{callout}"
    );
    assert_eq!(callout["measured"], "Ø8 ↧ 12\n⌴ Ø11 ↧ 6.8", "{callout}");
    assert_eq!(callout["format"]["replace"], "Ø9 THRU\nSPOTFACE Ø14");
    let texts = callout_texts(&doc);
    assert!(texts.iter().any(|t| t == "TYP Ø9 THRU"), "{texts:?}");
    assert!(!texts.iter().any(|t| t.contains("↧ 12")), "{texts:?}");
    // Empty: the hole's lines again.
    format(&mut doc, id, json!({"replace": ""}));
    let callout = annotation(&doc, id);
    assert!(callout["format"].get("replace").is_none(), "{callout}");
    assert_eq!(
        callout["label"], "TYP Ø8 ↧ 12 / ⌴ Ø11 ↧ 6.8 BOTH SIDES",
        "{callout}"
    );
    // One undo step: replaced again.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        annotation(&doc, id)["label"],
        "TYP Ø9 THRU / SPOTFACE Ø14 BOTH SIDES"
    );
    // No decimals, prefix or tolerance of a dimension's, no text of its
    // own.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": id,
               "format": {"decimals": 2}}),
    );
    assert!(error.contains("no decimals"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": id,
               "text": "DEBURR"}),
    );
    assert!(error.contains("no text of its own"), "{error}");

    // Kept in the design file and read again as it was.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let kept = &file["drawings"][0]["sheets"][0]["views"][1]["annotations"][0];
    assert_eq!(kept["type"], "hole_callout", "{kept}");
    assert_eq!(kept["format"]["before"], "TYP", "{kept}");
    assert_eq!(kept["format"]["replace"], "Ø9 THRU\nSPOTFACE Ø14", "{kept}");
    assert!(kept.get("text").is_none(), "{kept}");
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
}

#[test]
fn adding_and_deleting_a_hole_callout_are_one_undo_step_each() {
    let (mut doc, _) = counterbored();
    let id = add_callout(&mut doc);
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(
        sheet(&doc)["annotations"].as_array().unwrap().is_empty(),
        "{}",
        sheet(&doc)["annotations"]
    );
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(annotation(&doc, id)["type"], "hole_callout");
    command(
        &mut doc,
        json!({"cmd": "delete_drawing_annotation", "drawing": "D1", "annotation": id}),
    );
    assert!(sheet(&doc)["annotations"].as_array().unwrap().is_empty());
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(annotation(&doc, id)["type"], "hole_callout");
}

#[test]
fn a_tapped_hole_gives_its_thread_and_a_round_face_its_diameter() {
    // A tapped hole: its thread and depth, the hole's lines.
    let (mut doc, _) = holed_drawing(json!({"extent": {"type": "distance", "depth": 12},
        "thread": {"designation": "M6x1", "class": "6H"}}));
    let id = add_callout(&mut doc);
    let callout = annotation(&doc, id);
    assert_eq!(callout["lines"], json!(["M6x1 - 6H ↧ 12"]), "{callout}");
    assert_eq!(callout["label"], "M6x1 - 6H ↧ 12", "{callout}");

    // A round face of no hole, seen from the side in the front view: its
    // diameter.
    let (mut doc, _) = counterbored();
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "hole_callout",
               "picks": [{"body": "F2.b0", "face": ROUND}]}),
    );
    let callout = annotation(&doc, added["annotation"].as_u64().unwrap());
    assert_eq!(callout["lines"], json!(["Ø6"]), "{callout}");
    assert_eq!(callout["broken"], false, "{callout}");
}

#[test]
fn a_hole_callout_without_a_hole_or_circle_is_refused_in_words() {
    let (mut doc, straight) = counterbored();
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "hole_callout",
               "picks": [{"body": "F2.b0", "edge": straight}]}),
    );
    assert!(error.contains("a hole callout points at a hole"), "{error}");
    // In words: no edge's or face's name.
    assert!(!error.contains("E{") && !error.contains("F2:"), "{error}");
    assert!(sheet(&doc)["annotations"].as_array().unwrap().is_empty());
    // The hole gone: broken, with the reason in words.
    let id = add_callout(&mut doc);
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F3"}));
    let broken = annotation(&doc, id);
    assert_eq!(broken["broken"], true, "{broken}");
    assert!(broken["error"].is_string(), "{broken}");
}

#[test]
fn a_kept_model_hole_callout_is_a_drawing_hole_callout_that_follows_its_hole() {
    let (mut doc, _) = counterbored();
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "hole_callout",
               "a": {"body": "F2.b0", "face": "F3:hole0.wall"}, "at": [10.0, 13.3, 20.0]}),
    );
    command(
        &mut doc,
        json!({"cmd": "drawing_model_items", "drawing": "D1"}),
    );
    let kept = command(
        &mut doc,
        json!({"cmd": "keep_drawing_model_items", "drawing": "D1", "view": 2,
               "names": ["HoleCallout1"]}),
    );
    assert_eq!(kept["kept"], json!(["HoleCallout1"]), "{kept}");
    let id = kept["annotations"][0].as_u64().unwrap();
    let own = annotation(&doc, id);
    // The drawing's own hole callout on the same hole, not a note of its
    // words.
    assert_eq!(own["type"], "hole_callout", "{own}");
    assert_eq!(own["kept_from"], "HoleCallout1", "{own}");
    assert_eq!(own["picks"][0]["face"], "F3:hole0.wall", "{own}");
    assert!(own.get("text").is_none(), "{own}");
    assert_eq!(own["lines"], json!(["Ø6.6 ↧ 10", "⌴ Ø11 ↧ 6.8"]), "{own}");
    // It follows the hole and takes its own texts.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "drill", "value": 8}),
    );
    format(&mut doc, id, json!({"after": "2 PLACES"}));
    assert_eq!(
        annotation(&doc, id)["label"],
        "Ø8 ↧ 10 / ⌴ Ø11 ↧ 6.8 2 PLACES"
    );
}
