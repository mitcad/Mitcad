// SPDX-License-Identifier: MIT
//! Drawings kept in the design (mitcad#707) on the mock kernel, whose
//! views are the bodies' boxes: the definitions (directions of projected
//! and section views, checks, the file form), and the commands and
//! queries: a drawing in the root's or a component's Drawings folder,
//! base, projected, section and detail views placed and moved in line
//! with their parents, breaks, the model changed under them, undo, the
//! design file, the comparison of versions, copies of components and the
//! export of a kept drawing.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::Vec3;
use mitcad_model::ids::{ComponentUid, LARGEST_U32_ID};
use mitcad_model::internal::profile::{cross, dot};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use mitcad_model_diff::diff_states;
use mitcad_model_drawing::Projection;
use mitcad_model_drawing::def::{
    BreakDirection, Drawing, DrawingSheet, DrawingUid, DrawingView, Side, StandardView, Subject,
    ViewBreak, ViewDirection, ViewKind, section_view_axes,
};

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

fn same(a: Vec3, b: Vec3) -> bool {
    (0..3).all(|i| near(a[i], b[i]))
}

/// A 40 x 30 block as high as the parameter `height` (20 mm) at the
/// origin, and a 20 x 10 x 10 block beside it.
fn blocks() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "height", "value": "20 mm"},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": "height"}, "operation": "new_body"}},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [50, 0], "width": 20, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": R}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
        ]),
    );
    doc
}

/// The `drawing` query of a drawing's first sheet.
fn drawing(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "drawing", "drawing": uid}))
}

/// A view of the first sheet in the `drawing` query.
fn view(doc: &Document<MockKernel>, uid: &str, id: u64) -> Value {
    drawing(doc, uid)["sheets"][0]["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no view {id}"))
}

fn point(value: &Value) -> [f64; 2] {
    [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()]
}

/// A view's box on the sheet: its lower left and upper right corners.
fn sheet_box(view: &Value) -> [[f64; 2]; 2] {
    [point(&view["box"][0]), point(&view["box"][1])]
}

/// The classes of the first sheet's groups.
fn classes(doc: &Document<MockKernel>, uid: &str) -> Vec<String> {
    drawing(doc, uid)["sheets"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["class"].as_str().unwrap().to_owned())
        .collect()
}

/// The texts of the first sheet.
fn texts(doc: &Document<MockKernel>, uid: &str) -> Vec<String> {
    drawing(doc, uid)["sheets"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["items"].as_array().unwrap().clone())
        .filter_map(|i| i["text"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn projected_views_look_from_their_side_after_the_projection_method() {
    let front = StandardView::Front.axes();
    let third = Projection::ThirdAngle;
    let first = Projection::FirstAngle;
    let is = |axes: (Vec3, Vec3), view: StandardView| {
        let (e, x) = view.axes();
        same(axes.0, e) && same(axes.1, x)
    };
    // Third angle: what is seen from the right goes on the right.
    assert!(is(Side::Right.axes(front, third), StandardView::Right));
    assert!(is(Side::Above.axes(front, third), StandardView::Top));
    assert!(is(Side::Left.axes(front, third), StandardView::Left));
    assert!(is(Side::Below.axes(front, third), StandardView::Bottom));
    // First angle: on the other side.
    assert!(is(Side::Left.axes(front, first), StandardView::Right));
    assert!(is(Side::Below.axes(front, first), StandardView::Top));
    // A corner: the isometric view, whatever the method.
    assert!(is(Side::AboveRight.axes(front, third), StandardView::Iso));
    assert!(is(Side::AboveRight.axes(front, first), StandardView::Iso));
    // Every standard view is a right-handed frame with Z up where it can be.
    for view in StandardView::ALL {
        let (e, x) = view.axes();
        assert!(near(dot(e, e), 1.0) && near(dot(x, x), 1.0) && near(dot(e, x), 0.0));
        let y = cross(e, x);
        if !matches!(view, StandardView::Top | StandardView::Bottom) {
            assert!(y[2] > 0.0, "{view:?}: {y:?}");
        }
    }
    // The side a point lies on.
    assert_eq!(Side::towards([0.0, 0.0], [10.0, 1.0]), Side::Right);
    assert_eq!(Side::towards([0.0, 0.0], [-1.0, 10.0]), Side::Above);
    assert_eq!(Side::towards([0.0, 0.0], [-10.0, -9.0]), Side::BelowLeft);
    assert_eq!(Side::Above.aligned_axis(), Some(0));
    assert_eq!(Side::Left.aligned_axis(), Some(1));
    assert_eq!(Side::BelowRight.aligned_axis(), None);
}

#[test]
fn a_section_view_lines_up_with_its_parent() {
    let front = StandardView::Front.axes();
    // A vertical line on the front view: a side view of the cut, Z up.
    let (e, x) = section_view_axes([-1.0, 0.0, 0.0], front);
    assert!(same(e, [-1.0, 0.0, 0.0]) && same(x, [0.0, -1.0, 0.0]));
    assert!(same(cross(e, x), [0.0, 0.0, 1.0]));
    // A horizontal one: seen from above or below, X along.
    let (e, x) = section_view_axes([0.0, 0.0, 1.0], front);
    assert!(same(e, [0.0, 0.0, 1.0]) && same(x, [1.0, 0.0, 0.0]));
}

fn sample() -> Drawing {
    let base = DrawingView {
        id: 1,
        kind: ViewKind::Base {
            direction: ViewDirection::Named(StandardView::Front),
        },
        place: [100.0, 120.0],
        scale: Some(0.5),
        style: None,
        label: None,
        subject: Some(Subject::Bodies {
            bodies: vec![mitcad_model_drawing::def::SubjectBody {
                body: "F2.b0".parse().unwrap(),
                occurrence: None,
            }],
        }),
        breaks: vec![ViewBreak {
            direction: BreakDirection::Horizontal,
            from: 10.0,
            to: 30.0,
            gap: 6.0,
        }],
        aligned: true,
        center_marks: false,
        retrieved_dimensions: false,
        annotations: Vec::new(),
        rotation: 0.0,
        sketch: Vec::new(),
    };
    let projected = DrawingView {
        id: 2,
        kind: ViewKind::Projected {
            parent: 1,
            side: Side::Above,
        },
        place: [100.0, 180.0],
        scale: None,
        style: None,
        label: None,
        subject: None,
        breaks: Vec::new(),
        aligned: true,
        center_marks: false,
        retrieved_dimensions: false,
        annotations: Vec::new(),
        rotation: 0.0,
        sketch: Vec::new(),
    };
    let section = DrawingView {
        id: 3,
        kind: ViewKind::Section {
            parent: Some(1),
            origin: [20.0, 0.0, 0.0],
            normal: [-1.0, 0.0, 0.0],
            span: Some([-5.0, 25.0]),
            letter: "A".to_owned(),
            hatch: true,
        },
        place: [40.0, 120.0],
        scale: None,
        style: None,
        label: None,
        subject: None,
        breaks: Vec::new(),
        aligned: true,
        center_marks: false,
        retrieved_dimensions: false,
        annotations: Vec::new(),
        rotation: 0.0,
        sketch: Vec::new(),
    };
    let detail = DrawingView {
        id: 4,
        kind: ViewKind::Detail {
            parent: 1,
            center: [5.0, 5.0],
            radius: 4.0,
            letter: "B".to_owned(),
        },
        place: [200.0, 60.0],
        scale: None,
        style: None,
        label: None,
        subject: None,
        breaks: Vec::new(),
        aligned: true,
        center_marks: false,
        retrieved_dimensions: false,
        annotations: Vec::new(),
        rotation: 0.0,
        sketch: Vec::new(),
    };
    let mut sheet = DrawingSheet::new("Sheet1");
    sheet.views = vec![base, projected, section, detail];
    Drawing {
        uid: DrawingUid(1),
        name: "Drawing1".to_owned(),
        component: ComponentUid::ROOT,
        sheets: vec![sheet],
        revisions: Vec::new(),
        exported: 0,
        defaults: Default::default(),
        cut_sheet: None,
    }
}

#[test]
fn a_drawing_keeps_its_definition_only() {
    let drawing = sample();
    drawing.check().unwrap();
    let text = serde_json::to_value(&drawing).unwrap();
    assert_eq!(text["uid"], "D1");
    assert!(text.get("component").is_none(), "the root's is left out");
    assert_eq!(text["sheets"][0]["views"][1]["kind"]["type"], "projected");
    assert_eq!(text["sheets"][0]["views"][0]["kind"]["direction"], "front");
    let back: Drawing = serde_json::from_value(text).unwrap();
    assert_eq!(back, drawing);

    let sheet = &drawing.sheets[0];
    // Scales and labels follow from the parents.
    assert_eq!(sheet.scale(2), 0.5);
    assert_eq!(sheet.scale(4), 1.0);
    assert_eq!(sheet.label(3).unwrap(), "SECTION A-A");
    assert_eq!(sheet.label(4).unwrap(), "DETAIL B (1:1)");
    assert_eq!(sheet.anchor(4), [5.0, 5.0]);
    assert_eq!(sheet.dependents(1), [2, 3, 4]);
    assert_eq!(sheet.aligned_axis(2), Some(0));
    assert_eq!(sheet.aligned_axis(3), Some(1));
    assert_eq!(sheet.aligned_axis(4), None);
    assert!(matches!(sheet.subject(2), Subject::Bodies { .. }));
    assert_eq!(drawing.next_letter(), "C");
    assert_eq!(drawing.next_view_id(), 5);
    let spec = sheet.spec(3).unwrap();
    assert!(spec.section.is_some() && !spec.hidden);

    // What cannot be kept.
    let refuse = |change: &dyn Fn(&mut Drawing), message: &str| {
        let mut d = sample();
        change(&mut d);
        let error = d.check().unwrap_err();
        assert!(error.contains(message), "{error}");
    };
    refuse(
        &|d| d.sheets[0].views.swap(0, 1),
        "is not an earlier view of the sheet",
    );
    refuse(
        &|d| {
            d.sheets[0].views[1].subject = Some(Subject::Design);
        },
        "only a base view has a subject",
    );
    refuse(
        &|d| {
            d.sheets[0].views[0].breaks.push(ViewBreak {
                direction: BreakDirection::Horizontal,
                from: 20.0,
                to: 40.0,
                gap: 6.0,
            });
        },
        "its breaks overlap",
    );
    refuse(&|d| d.sheets[0].views[3].id = 2, "listed more than once");
    refuse(&|d| d.sheets.clear(), "needs a sheet");
    refuse(
        &|d| d.sheets[0].units = Some("furlong".to_owned()),
        "unknown length unit",
    );
    assert!("D12".parse::<DrawingUid>() == Ok(DrawingUid(12)));
    assert!("D".parse::<DrawingUid>().is_err() && "C1".parse::<DrawingUid>().is_err());
}

#[test]
fn views_are_placed_projected_cut_and_detailed_on_a_sheet() {
    let mut doc = blocks();
    assert_eq!(query(&doc, json!({"query": "drawings"})), json!([]));
    let added = command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    assert_eq!(added["drawing"], "D1");
    assert_eq!(added["name"], "Drawing1");
    assert_eq!(undo_label(&doc), "Add Drawing1");
    let list = query(&doc, json!({"query": "drawings"}));
    assert_eq!(list[0]["component"], "C0");
    assert_eq!(list[0]["sheets"][0]["size"], "A3");

    // A base view from the front: 70 x 20 at 1:1 with its middle at the
    // point given.
    let base = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    assert_eq!(base["view"], 1);
    let front = view(&doc, "D1", 1);
    let [lo, hi] = sheet_box(&front);
    assert!(near(lo[0], 65.0) && near(hi[0], 135.0), "{front}");
    assert!(near(lo[1], 110.0) && near(hi[1], 130.0), "{front}");
    assert_eq!(point(&front["place"]), [65.0, 110.0]);

    // A projected view dropped on the right: seen from the right (third
    // angle), in line with the base view.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "projected", "parent": 1,
               "at": [200, 140]}),
    );
    let right = view(&doc, "D1", 2);
    assert_eq!(right["side"], "right");
    assert_eq!(right["aligned"], "y");
    assert!(near(point(&right["place"])[1], 110.0), "{right}");
    let [lo, hi] = sheet_box(&right);
    assert!(
        near(hi[0] - lo[0], 30.0) && near(hi[1] - lo[1], 20.0),
        "{right}"
    );

    // A section through a vertical line on the base view.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1,
               "line": [[85, 105], [85, 135]]}),
    );
    let section = view(&doc, "D1", 3);
    assert_eq!(section["letter"], "A");
    assert_eq!(section["label"], "SECTION A-A");
    assert_eq!(section["aligned"], "y");
    // The eye on the side the line's normal points to: on the left.
    assert!(sheet_box(&section)[1][0] < 65.0, "{section}");

    // A detail of the base view's lower left corner at twice its scale.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "detail", "parent": 1,
               "center": [70, 115], "radius": 8, "at": [300, 60]}),
    );
    let detail = view(&doc, "D1", 4);
    assert_eq!(detail["letter"], "B");
    assert_eq!(detail["label"], "DETAIL B (2:1)");
    assert_eq!(point(&detail["place"]), [300.0, 60.0]);
    assert_eq!(point(&detail["circle"]["center"]), [70.0, 115.0]);
    assert!(near(detail["circle"]["radius"].as_f64().unwrap(), 8.0));
    // Inside a 32 mm circle on the sheet at most.
    let [lo, hi] = sheet_box(&detail);
    assert!(hi[0] - lo[0] <= 32.0 + 1e-6 && hi[1] - lo[1] <= 32.0 + 1e-6);

    let classes = classes(&doc, "D1");
    for class in [
        "view",
        "section-line",
        "detail-circle",
        "view-label",
        "frame",
        "title-block",
    ] {
        assert!(classes.iter().any(|c| c == class), "{class}: {classes:?}");
    }
    let texts = texts(&doc, "D1");
    for text in ["SECTION A-A", "DETAIL B (2:1)", "A", "B", "1:1", "A3"] {
        assert!(texts.iter().any(|t| t == text), "{text}: {texts:?}");
    }
    // The drawing names the bodies it shows.
    assert!(texts.iter().any(|t| t == "Body1, Body2"), "{texts:?}");
}

/// A drawing with a front view at 1:1 (its box 65..135 x 110..130 on the
/// sheet) and a view projected to its right.
fn drawn() -> Document<MockKernel> {
    let mut doc = blocks();
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "projected", "parent": 1,
               "side": "right"}),
    );
    doc
}

#[test]
fn views_move_in_line_with_their_parents() {
    let mut doc = drawn();
    let right = point(&view(&doc, "D1", 2)["place"]);
    // The base view moves; the projected view moves with it.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "move": [10, 5]}),
    );
    assert_eq!(undo_label(&doc), "Move view 1 of Drawing1");
    assert_eq!(point(&view(&doc, "D1", 1)["place"]), [75.0, 115.0]);
    assert_eq!(
        point(&view(&doc, "D1", 2)["place"]),
        [right[0] + 10.0, right[1] + 5.0]
    );
    // The projected view moves along its parent only.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 2, "move": [20, 30]}),
    );
    assert_eq!(
        point(&view(&doc, "D1", 2)["place"]),
        [right[0] + 30.0, right[1] + 5.0]
    );
    // Unless it is let go.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 2, "aligned": false,
               "move": [0, 30]}),
    );
    assert!(view(&doc, "D1", 2)["aligned"].is_null());
    assert_eq!(
        point(&view(&doc, "D1", 2)["place"]),
        [right[0] + 30.0, right[1] + 35.0]
    );
    // First angle: the same side shows the view from the left.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1", "sheet": 0,
               "projection": "first_angle"}),
    );
    let d = doc.find_drawing("D1").unwrap();
    let (e, _) = d.sheets[0].axes(2).unwrap();
    assert!(same(e, [-1.0, 0.0, 0.0]));
    // Deleting the base view takes the projected one with it.
    let deleted = command(
        &mut doc,
        json!({"cmd": "delete_drawing_view", "drawing": "D1", "view": 1}),
    );
    assert_eq!(deleted["deleted"], json!([1, 2]));
    assert!(
        drawing(&doc, "D1")["sheets"][0]["views"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn views_follow_the_model_and_are_projected_once() {
    let mut doc = drawn();
    let calls = |doc: &Document<MockKernel>| {
        doc.kernel()
            .calls
            .borrow()
            .get("project_view")
            .copied()
            .unwrap_or(0)
    };
    let before = calls(&doc);
    drawing(&doc, "D1");
    drawing(&doc, "D1");
    assert_eq!(calls(&doc), before, "projected before: kept");
    // Higher: the front view grows upward from its place.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 35}),
    );
    let [lo, hi] = sheet_box(&view(&doc, "D1", 1));
    assert!(near(lo[1], 110.0) && near(hi[1], 145.0), "{lo:?} {hi:?}");
    assert!(calls(&doc) > before);
    // The view of a body that is gone says why, and the drawing stays.
    command(&mut doc, json!({"cmd": "undo"}));
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "subject": {"type": "bodies", "bodies": [{"body": "F9.b0"}]}}),
    );
    let gone = view(&doc, "D1", 1);
    assert!(gone["error"].as_str().unwrap().contains("F9.b0"), "{gone}");
    let child = view(&doc, "D1", 2);
    assert!(child["error"].is_string(), "{child}");
}

#[test]
fn breaks_take_bands_out_of_a_view() {
    let mut doc = drawn();
    // 30 mm of the 70 mm front view out, 6 mm of paper left between.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "add_break": {"direction": "horizontal", "from": [85, 120], "to": [115, 120]}}),
    );
    let front = view(&doc, "D1", 1);
    assert_eq!(front["breaks"][0]["from"], 20.0);
    assert_eq!(front["breaks"][0]["to"], 50.0);
    let [lo, hi] = sheet_box(&front);
    assert!(near(hi[0] - lo[0], 46.0), "{front}");
    // Its break lines are drawn in the view: two more polylines.
    let polylines = |doc: &Document<MockKernel>| {
        drawing(doc, "D1")["sheets"][0]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|g| g["id"] == "view-front")
            .flat_map(|g| g["items"].as_array().unwrap().clone())
            .filter(|i| i.get("polyline").is_some())
            .count()
    };
    let broken = polylines(&doc);
    // Without breaks it is whole again.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "breaks": []}),
    );
    let [lo, hi] = sheet_box(&view(&doc, "D1", 1));
    assert!(near(hi[0] - lo[0], 70.0));
    assert_eq!(broken, polylines(&doc) + 2);
}

#[test]
fn a_drawing_is_an_undo_step_saved_and_compared() {
    let mut doc = blocks();
    let plain = doc.to_json();
    assert!(!plain.contains("\"drawings\""), "none: left out");
    let before = doc.state().clone();
    command(&mut doc, json!({"cmd": "add_drawing", "name": "Layout"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "Layout", "type": "base",
               "direction": "iso", "at": [150, 120], "style": {"shaded": true}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_sheet", "drawing": "D1", "size": "A2", "title_block":
               {"number": "DWG-7", "author": "Designer"}}),
    );
    assert_eq!(undo_label(&doc), "Add Sheet2 to Layout");
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["drawings"][0]["name"], "Layout");
    assert_eq!(file["drawings"][0]["sheets"][1]["size"], "A2");
    assert_eq!(
        file["drawings"][0]["sheets"][1]["title_block"]["number"],
        "DWG-7"
    );
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    assert_eq!(loaded.drawings().len(), 1);
    command(&mut loaded, json!({"cmd": "recompute"}));
    // The second sheet's title block: its number instead of the bodies.
    let second = query(
        &loaded,
        json!({"query": "drawing", "drawing": "D1", "sheet": 1}),
    );
    let texts: Vec<String> = second["sheets"][1]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["items"].as_array().unwrap().clone())
        .filter_map(|i| i["text"].as_str().map(str::to_owned))
        .collect();
    for text in ["DWG-7", "Designer", "A2 2/2", "DRAWING NO.", "DRAWN BY"] {
        assert!(texts.iter().any(|t| t == text), "{text}: {texts:?}");
    }
    // A shaded view shades its faces.
    let first = drawing(&loaded, "D1");
    let shades = first["sheets"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["items"].as_array().unwrap().clone())
        .filter(|i| i.get("shade").is_some())
        .count();
    assert!(shades > 0, "{first}");

    // The comparison lists the drawing.
    let diff = diff_states(&before, doc.state());
    assert_eq!(diff.drawings.len(), 1);
    assert!(diff.summary.contains("drawing"), "{}", diff.summary);

    // Undone and done again.
    for _ in 0..3 {
        command(&mut doc, json!({"cmd": "undo"}));
    }
    assert!(doc.drawings().is_empty());
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(doc.drawings().len(), 1);

    // What the file cannot hold: a drawing twice, one of no component.
    let mut twice: Value = serde_json::from_str(&saved).unwrap();
    let again = twice["drawings"][0].clone();
    twice["drawings"].as_array_mut().unwrap().push(again);
    let Err(error) = Document::from_json(&twice.to_string(), MockKernel::default()) else {
        panic!("a drawing twice was read");
    };
    assert!(
        error.to_string().contains("listed more than once"),
        "{error}"
    );
    let mut bad: Value = serde_json::from_str(&saved).unwrap();
    bad["drawings"][0]["component"] = json!("C9");
    let Err(error) = Document::from_json(&bad.to_string(), MockKernel::default()) else {
        panic!("a drawing of no component was read");
    };
    assert!(error.to_string().contains("no component C9"), "{error}");
}

#[test]
fn drawings_refuse_what_they_cannot_be() {
    let mut doc = drawn();
    for (cmd, message) in [
        (
            json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "projected"}),
            "needs a parent",
        ),
        (
            json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1}),
            "needs its line",
        ),
        (
            json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "projected", "parent": 1,
                   "subject": {"type": "design"}}),
            "only a base view has a subject",
        ),
        (
            json!({"cmd": "add_drawing_view", "drawing": "D9", "type": "base"}),
            "there is no drawing 'D9'",
        ),
        (
            json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 2, "direction": "top"}),
            "has no direction",
        ),
        (
            json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "scale": "big"}),
            "a scale is",
        ),
        (
            json!({"cmd": "delete_drawing_sheet", "drawing": "D1", "sheet": 0}),
            "one sheet at least",
        ),
        (
            json!({"cmd": "edit_drawing_sheet", "drawing": "D1", "units": "furlong"}),
            "unknown length unit",
        ),
        (
            json!({"cmd": "add_drawing", "name": "Drawing1"}),
            "already has a drawing 'Drawing1'",
        ),
    ] {
        let error = refused(&mut doc, cmd);
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn a_components_drawings_travel_with_it() {
    let mut doc = Document::new(MockKernel::default());
    let made = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Bracket"}),
    );
    let component = made["component"].as_str().unwrap().to_owned();
    let occurrence = made["occurrence"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
        ]),
    );
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing", "component": "Bracket"}),
    );
    assert_eq!(added["component"], component);
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [150, 120], "scale": "2:1"}),
    );
    // The root has a drawing of the same name: each folder has its own.
    command(&mut doc, json!({"cmd": "add_drawing"}));
    let list = query(&doc, json!({"query": "drawings"}));
    assert_eq!(list[1]["name"], "Drawing1");
    assert_eq!(list[0]["component_name"], "Bracket");
    let error = refused(
        &mut doc,
        json!({"cmd": "delete_drawing", "drawing": "Drawing1"}),
    );
    assert!(error.contains("several drawings"), "{error}");
    // A copy of the component, pasted into the root, brings a copy of its
    // drawing.
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    let copy = command(
        &mut doc,
        json!({"cmd": "paste_new", "occurrence": occurrence}),
    );
    let copied: Vec<Value> = query(&doc, json!({"query": "drawings"}))
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(copied.len(), 3);
    let new = &copied[2];
    assert_eq!(new["component"], copy["component"]);
    assert_eq!(new["name"], "Drawing1");
    // It shows the copy's body: its view is drawn.
    let shown = query(&doc, json!({"query": "drawing", "drawing": new["drawing"]}));
    assert!(shown["sheets"][0]["views"][0]["box"].is_array(), "{shown}");

    // Deleting the component deletes its drawing.
    command(
        &mut doc,
        json!({"cmd": "delete_occurrence", "occurrence": copy["occurrence"]}),
    );
    assert_eq!(doc.drawings().len(), 2);

    // Inserted into another design as a copy, the drawing comes along.
    let dir = std::env::temp_dir().join(format!("mitcad-kept-drawings-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bracket.mitcad");
    std::fs::write(&path, doc.to_json()).unwrap();
    let mut other = Document::new(MockKernel::default());
    command(
        &mut other,
        json!({"cmd": "insert_component", "path": path.to_string_lossy(), "link": false}),
    );
    let inserted = query(&other, json!({"query": "drawings"}));
    let names: Vec<&str> = inserted
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["component_name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 2, "{inserted}");
    assert!(!names.contains(&"Root"), "{inserted}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_kept_drawing_is_exported_as_it_is() {
    let mut doc = drawn();
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1,
               "line": [[85, 105], [85, 135]]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_sheet", "drawing": "D1"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "sheet": 1, "type": "base",
               "direction": "top", "at": [150, 120]}),
    );
    let dir = std::env::temp_dir().join(format!("mitcad-kept-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let svg = dir.join("kept.svg").to_string_lossy().into_owned();
    let result = command(
        &mut doc,
        json!({"cmd": "export_drawing", "path": svg, "drawing": "D1", "title": "Blocks"}),
    );
    assert_eq!(result["drawing"], "D1");
    assert_eq!(result["sheets"], 2);
    assert_eq!(result["files"].as_array().unwrap().len(), 2);
    let text = std::fs::read_to_string(&svg).unwrap();
    for part in [
        r#"id="view-front""#,
        r#"id="view-right""#,
        r#"<g class="section-line""#,
        ">SECTION A-A<",
        ">Blocks<",
        ">A3 1/2<",
    ] {
        assert!(text.contains(part), "{part}");
    }
    // One sheet of it, and the whole as a PDF.
    let one = dir.join("one.svg").to_string_lossy().into_owned();
    let result = command(
        &mut doc,
        json!({"cmd": "export_drawing", "path": one, "drawing": "D1", "sheet": 1}),
    );
    assert_eq!(result["files"], json!([one]));
    assert!(
        std::fs::read_to_string(&one)
            .unwrap()
            .contains(r#"id="view-top""#)
    );
    let pdf = dir.join("kept.pdf").to_string_lossy().into_owned();
    let result = command(
        &mut doc,
        json!({"cmd": "export_drawing", "path": pdf, "drawing": "D1"}),
    );
    assert_eq!(result["pages"], 2);
    // A kept drawing's own fields are not given with it.
    let error = refused(
        &mut doc,
        json!({"cmd": "export_drawing", "path": pdf, "drawing": "D1", "hidden_lines": true}),
    );
    assert!(error.contains("is exported as it is"), "{error}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn export_drawing_lays_out_a_drawing_the_design_can_keep() {
    let mut doc = blocks();
    // File › Export Drawing keeps the drawing it writes.
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing", "layout": {"sheet": "A3", "views": ["front", "top", "right"],
               "title": "Blocks", "hidden_lines": true}}),
    );
    assert_eq!(added["drawing"], "D1");
    let kept = doc.find_drawing("D1").unwrap().clone();
    let sheet = &kept.sheets[0];
    assert_eq!(sheet.views.len(), 3);
    assert!(sheet.annotations.dimensions && sheet.annotations.hole_table);
    assert_eq!(sheet.title_block.title.as_deref(), Some("Blocks"));
    assert!(sheet.title_block.date.is_none(), "dated when drawn");
    // Its views line up as the export's do: the top above the front.
    let front = view(&doc, "D1", 1);
    let top = view(&doc, "D1", 2);
    let [flo, fhi] = sheet_box(&front);
    let [tlo, thi] = sheet_box(&top);
    assert!(near(flo[0], tlo[0]) && near(fhi[0], thi[0]) && tlo[1] > fhi[1]);
    // The same drawing as export_drawing writes without keeping one.
    let dir = std::env::temp_dir().join(format!("mitcad-export-kept-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.svg").to_string_lossy().into_owned();
    let b = dir.join("b.svg").to_string_lossy().into_owned();
    command(
        &mut doc,
        json!({"cmd": "export_drawing", "path": a, "sheet": "A3", "views": ["front", "top", "right"],
               "title": "Blocks", "hidden_lines": true, "date": "2026-10-26"}),
    );
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1", "title_block": {"date": "2026-10-26"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "export_drawing", "path": b, "drawing": "D1"}),
    );
    let (a, b) = (
        std::fs::read_to_string(&a).unwrap(),
        std::fs::read_to_string(&b).unwrap(),
    );
    assert_eq!(a, b);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn export_drawing_numbers_the_drawings_it_lays_out() {
    // mitcad#725: File › Export Drawing offers the drawing it laid out last
    // (the highest `exported`) and writes it as it is; only a new one is
    // laid out.
    let mut doc = blocks();
    command(
        &mut doc,
        json!({"cmd": "add_drawing", "layout": {"sheet": "A3", "views": ["front", "top"]}}),
    );
    let listed = query(&doc, json!({"query": "drawings"}));
    assert_eq!(listed[0]["exported"], 1, "{listed}");
    // A drawing of its own is not one the export laid out.
    command(&mut doc, json!({"cmd": "add_drawing"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing", "layout": {"views": ["front"]}}),
    );
    let listed = query(&doc, json!({"query": "drawings"}));
    assert_eq!(listed[1]["exported"], 0, "{listed}");
    assert_eq!(listed[2]["exported"], 2, "{listed}");
    // Saved with the design, and back with undo.
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(file["drawings"][2]["exported"], 2);
    assert!(file["drawings"][1].get("exported").is_none(), "{file}");
    command(&mut doc, json!({"cmd": "undo"}));
    let listed = query(&doc, json!({"query": "drawings"}));
    assert_eq!(listed.as_array().unwrap().len(), 2, "{listed}");
    // A drawing is never laid out again: what was made by hand on it stays.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing", "drawing": "D1", "layout": {"views": ["top"]}}),
    );
    assert!(error.contains("layout"), "{error}");
    // Written as it is: its own views, after a change made by hand.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "type": "text", "at": [20, 20],
               "text": "BY HAND"}),
    );
    let dir = std::env::temp_dir().join(format!("mitcad-export-again-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let svg = dir.join("again.svg").to_string_lossy().into_owned();
    command(
        &mut doc,
        json!({"cmd": "export_drawing", "path": svg, "drawing": "D1"}),
    );
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(text.contains("BY HAND"), "the hand-made note is written");
    assert_eq!(
        query(&doc, json!({"query": "drawings"}))
            .as_array()
            .unwrap()
            .len(),
        2
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_laid_out_section_cuts_the_chosen_bodies_only() {
    let mut doc = blocks();
    // One sheet per body with a section: each section shows its sheet's body.
    command(
        &mut doc,
        json!({"cmd": "add_drawing", "layout": {"views": ["front"], "per_body": true,
               "section": {"origin": [20, 15, 0], "normal": [0, -1, 0]}}}),
    );
    let kept = doc.find_drawing("D1").unwrap().clone();
    assert_eq!(kept.sheets.len(), 2);
    for (i, sheet) in kept.sheets.iter().enumerate() {
        let section = sheet
            .views
            .iter()
            .find(|v| matches!(v.kind, ViewKind::Section { .. }))
            .expect("a section view");
        let Subject::Bodies { bodies } = sheet.subject(section.id) else {
            panic!("the section's subject: {:?}", sheet.subject(section.id));
        };
        assert_eq!(bodies.len(), 1);
        let answer = query(
            &doc,
            json!({"query": "drawing", "drawing": "D1", "sheet": i}),
        );
        let texts: Vec<String> = answer["sheets"][i]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|g| g["items"].as_array().unwrap().clone())
            .filter_map(|t| t["text"].as_str().map(str::to_owned))
            .collect();
        // The title block names the sheet's one body.
        let name = ["Body1", "Body2"][i];
        assert!(texts.iter().any(|t| t == name), "{name}: {texts:?}");
        assert!(!texts.iter().any(|t| t == "Body1, Body2"), "{texts:?}");
    }
}

#[test]
fn per_body_sheets_show_one_body_each() {
    let mut doc = blocks();
    command(
        &mut doc,
        json!({"cmd": "add_drawing", "layout": {"views": ["front", "top"], "per_body": true}}),
    );
    let kept = doc.find_drawing("D1").unwrap().clone();
    assert_eq!(kept.sheets.len(), 2);
    // View ids are unique across the sheets.
    let ids: Vec<u32> = kept
        .sheets
        .iter()
        .flat_map(|s| s.views.iter().map(|v| v.id))
        .collect();
    assert_eq!(ids, [1, 2, 3, 4]);
    for (i, sheet) in kept.sheets.iter().enumerate() {
        let answer = query(
            &doc,
            json!({"query": "drawing", "drawing": "D1", "sheet": i}),
        );
        for v in answer["sheets"][i]["views"].as_array().unwrap() {
            assert!(v["error"].is_null(), "{v}");
        }
        let width = |id: u32| {
            let v = answer["sheets"][i]["views"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == id)
                .cloned()
                .unwrap();
            let [lo, hi] = sheet_box(&v);
            (hi[0] - lo[0]) / v["scale"].as_f64().unwrap()
        };
        // The front view is as wide as the sheet's body: 40 or 20 mm.
        let front = sheet
            .views
            .iter()
            .find(|v| {
                matches!(
                    v.kind,
                    ViewKind::Base {
                        direction: ViewDirection::Named(StandardView::Front)
                    }
                )
            })
            .expect("a front view");
        assert!(near(width(front.id), [40.0, 20.0][i]), "{answer}");
    }
}

/// The ids of the hole tags on the first sheet.
fn hole_tags(doc: &Document<MockKernel>, uid: &str) -> Vec<String> {
    drawing(doc, uid)["sheets"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["class"] == "hole-tag")
        .map(|g| g["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn each_view_tags_the_holes_of_its_own_bodies() {
    let mut doc = blocks();
    // A hole down through the middle of each block.
    doc.kernel().box_holes.set(Some(4.0));
    command(
        &mut doc,
        json!({"cmd": "add_drawing", "size": "A3", "annotations": {"hole_table": true}}),
    );
    for (body, at) in [("F2.b0", [100, 120]), ("F4.b0", [250, 120])] {
        command(
            &mut doc,
            json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
                   "at": at, "scale": "1:1",
                   "subject": {"type": "bodies", "bodies": [{"body": body}]}}),
        );
    }
    // Each top view sees its own block's hole end on, not the other's.
    assert_eq!(hole_tags(&doc, "D1"), ["A1", "B1"]);
    // A break across the first block's hole takes its tag away.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "add_break": {"direction": "horizontal", "from": [95, 120], "to": [105, 120]}}),
    );
    assert_eq!(hole_tags(&doc, "D1"), ["A1"]);
    let tag = drawing(&doc, "D1")["sheets"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["class"] == "hole-tag")
        .cloned()
        .unwrap();
    // On the second view, beyond the first.
    let at = point(&tag["items"][0]["at"]);
    assert!(at[0] > 200.0, "{tag}");
}

#[test]
fn first_angle_places_views_on_the_other_side() {
    let mut doc = drawn();
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1", "projection": "first_angle"}),
    );
    // A view dropped on the left of the front: seen from the right.
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "projected", "parent": 1,
               "at": [20, 125]}),
    );
    let id = added["view"].as_u64().unwrap();
    let left = view(&doc, "D1", id);
    assert_eq!(left["side"], "left");
    assert_eq!(left["aligned"], "y");
    let d = doc.find_drawing("D1").unwrap();
    let (e, _) = d.sheets[0].axes(id as u32).unwrap();
    assert!(same(e, [1.0, 0.0, 0.0]), "{e:?}");
    assert!(sheet_box(&left)[1][0] < 65.0, "{left}");
    // A section seen from the left goes on the right in first angle
    // (third angle puts it on the left), clear of the other views.
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1,
               "line": [[85, 105], [85, 135]]}),
    );
    let section = view(&doc, "D1", added["view"].as_u64().unwrap());
    let [lo, _] = sheet_box(&section);
    assert!(lo[0] > 135.0, "{section}");
    let right = sheet_box(&view(&doc, "D1", 2));
    let [slo, shi] = sheet_box(&section);
    let apart = slo[0] >= right[1][0] || shi[0] <= right[0][0];
    assert!(apart, "{section} overlaps view 2");
}

#[test]
fn a_detail_of_a_section_is_cut_as_the_section_is() {
    let mut doc = drawn();
    let section = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1,
               "line": [[85, 105], [85, 135]], "at": [300, 120]}),
    )["view"]
        .as_u64()
        .unwrap();
    let detail = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "detail",
               "parent": section, "center": [300, 120], "radius": 6, "at": [300, 40],
               "style": {"hidden_lines": true}}),
    )["view"]
        .as_u64()
        .unwrap();
    let d = doc.find_drawing("D1").unwrap();
    let sheet = &d.sheets[0];
    let section_spec = sheet.spec(section as u32).unwrap();
    let spec = sheet.spec(detail as u32).unwrap();
    // The section's plane and direction; its own style, but a section
    // shows no hidden lines.
    assert_eq!(spec.section, section_spec.section);
    assert!(spec.section.is_some());
    assert!(same(spec.toward_eye, section_spec.toward_eye));
    assert!(!spec.hidden);
    let shown = view(&doc, "D1", detail);
    assert!(
        shown["error"].is_null() && shown["box"].is_array(),
        "{shown}"
    );
}

#[test]
fn sections_and_details_on_a_broken_view_take_its_coordinates() {
    let mut doc = drawn();
    // 30 mm of the front view (x 20..50) out: x 60 lies at 101 on the sheet.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "add_break": {"direction": "horizontal", "from": [85, 120], "to": [115, 120]}}),
    );
    let section = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1,
               "line": [[101, 105], [101, 135]], "at": [300, 120]}),
    )["view"]
        .as_u64()
        .unwrap() as u32;
    let detail = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "detail", "parent": 1,
               "center": [101, 115], "radius": 4, "at": [300, 40]}),
    )["view"]
        .as_u64()
        .unwrap() as u32;
    let d = doc.find_drawing("D1").unwrap();
    let sheet = &d.sheets[0];
    let ViewKind::Section { origin, .. } = &sheet.view(section).unwrap().kind else {
        panic!("a section");
    };
    assert!(near(origin[0], 60.0), "{origin:?}");
    let ViewKind::Detail { center, .. } = &sheet.view(detail).unwrap().kind else {
        panic!("a detail");
    };
    assert!(near(center[0], 60.0) && near(center[1], 5.0), "{center:?}");
    // Its circle is drawn where it was picked.
    let shown = view(&doc, "D1", detail as u64);
    let c = point(&shown["circle"]["center"]);
    assert!(near(c[0], 101.0) && near(c[1], 115.0), "{shown}");
}

#[test]
fn a_view_with_sections_or_details_turns_and_takes_them_along() {
    // Turning a view whose sections or details are drawn on it, or on a
    // view from it, was refused until mitcad#729; now they follow
    // (`drawing::feature_tests` checks where they go).
    let mut doc = drawn();
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "detail", "parent": 2,
               "center": [160, 115], "radius": 4, "at": [300, 40]}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 2, "scale": "fit"}),
    );
    assert!(error.contains("only a base view's scale fits"), "{error}");
    // The same direction again is no turn; a base view's scale fits.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "direction": "front",
               "scale": "fit"}),
    );
    let front = view(&doc, "D1", 1);
    assert!(front["own_scale"].as_bool().unwrap());
    assert_eq!(view(&doc, "D1", 2)["automatic_scale"], front["scale_text"]);
    for cmd in [
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "direction": "top"}),
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 2, "side": "left"}),
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1", "projection": "first_angle"}),
    ] {
        command(&mut doc, cmd);
    }
    // The detail is still drawn, on its parent.
    let detail = view(&doc, "D1", 3);
    assert!(detail.get("error").is_none(), "{detail}");
    assert_eq!(detail["parent"], 2);
}

#[test]
fn a_view_that_cannot_be_drawn_keeps_a_box() {
    let mut doc = drawn();
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "subject": {"type": "bodies", "bodies": [{"body": "F9.b0"}]}}),
    );
    let gone = view(&doc, "D1", 1);
    assert_eq!(gone["broken"], true);
    let [lo, hi] = sheet_box(&gone);
    let place = point(&gone["place"]);
    assert!(lo[0] < place[0] && place[0] < hi[0] && lo[1] < place[1] && place[1] < hi[1]);
    // It can be deleted with what comes from it.
    let deleted = command(
        &mut doc,
        json!({"cmd": "delete_drawing_view", "drawing": "D1", "view": 1}),
    );
    assert_eq!(deleted["deleted"], json!([1, 2]));
}

#[test]
fn a_made_components_drawings_move_with_it_or_keep_it() {
    let mut doc = blocks();
    let body = "F2.b0".parse().unwrap();
    let made = doc.components_from_bodies(&[body]).unwrap();
    let (feature, component) = (made[0].0.uid, made[0].1);
    command(
        &mut doc,
        json!({"cmd": "add_drawing", "component": component.to_string()}),
    );
    let (base, _) = doc
        .create_component(
            Some("Base"),
            mitcad_model::transform::Transform::IDENTITY,
            false,
        )
        .unwrap();
    // Filled into an existing component, the drawing goes along.
    doc.move_made_component(feature, base).unwrap();
    assert_eq!(doc.drawings().len(), 1);
    assert_eq!(doc.drawings()[0].component, base);

    // A new-component operation whose component has a drawing stays one.
    let new = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F3", "region": R}],
               "extent": {"type": "distance", "distance": 5}, "operation": "new_component"}}),
    );
    let uid = new["uid"].as_str().unwrap().to_owned();
    let made = doc
        .assembly()
        .created_by(uid.parse().unwrap())
        .expect("a component");
    command(
        &mut doc,
        json!({"cmd": "add_drawing", "component": made.to_string()}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": uid, "def": {"type": "extrude",
               "profiles": [{"sketch": "F3", "region": R}],
               "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}}),
    );
    assert!(
        error.contains("has drawings in its Drawings folder; delete them first"),
        "{error}"
    );
    assert_eq!(doc.drawings().len(), 2);
    // Deleting the feature keeps its component, with its drawing, as a
    // plain component.
    command(&mut doc, json!({"cmd": "delete_feature", "uid": uid}));
    assert!(doc.assembly().component(made).is_some());
    assert_eq!(doc.assembly().component(made).unwrap().created_by, None);
    assert_eq!(doc.drawings().len(), 2);
    assert!(doc.drawings().iter().any(|d| d.component == made));
}

#[test]
fn a_base_views_automatic_scale_fits_and_a_details_circle_moves() {
    let mut doc = drawn();
    // "parent" on a base view (it has none): the scale that fits the
    // sheet, as when it was added, not 1:1 (mitcad#715).
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "scale": "parent"}),
    );
    let front = view(&doc, "D1", 1);
    assert_eq!(front["own_scale"], true, "{front}");
    assert_eq!(front["scale_text"], "2:1", "{front}");
    // A detail's circle moves on its parent (the panel's Center fields, a
    // drag of the circle); the detail stays where it is on the sheet.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "detail", "parent": 1,
               "center": [70, 115], "radius": 8, "at": [300, 60]}),
    );
    let id = drawing(&doc, "D1")["sheets"][0]["views"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": id, "center": [80, 120]}),
    );
    let detail = view(&doc, "D1", id);
    assert_eq!(point(&detail["circle"]["center"]), [80.0, 120.0]);
    assert_eq!(point(&detail["place"]), [300.0, 60.0]);
    assert_eq!(undo_label(&doc), format!("Edit view {id} of Drawing1"));
}

#[test]
fn a_drawings_ids_leave_the_top_numbers_free() {
    // mitcad#944: a design file's drawing, view and annotation ids and
    // export counts stay below the reserve, so the next ones exist.
    let top = LARGEST_U32_ID;
    let mut doc = drawn();
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "type": "text", "at": [20, 20],
               "text": "NOTE"}),
    );
    let mut file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let drawing = &mut file["drawings"][0];
    drawing["uid"] = json!(format!("D{top}"));
    drawing["exported"] = json!(top);
    drawing["sheets"][0]["views"][1]["id"] = json!(top);
    drawing["sheets"][0]["notes"][0]["id"] = json!(top);
    let mut loaded = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    // A design file is read unevaluated; its bodies come with recompute.
    command(&mut loaded, json!({"cmd": "recompute"}));
    let d = format!("D{top}");
    let added = command(&mut loaded, json!({"cmd": "add_drawing", "size": "A3"}));
    assert_eq!(added["drawing"], format!("D{}", top + 1));
    let view = command(
        &mut loaded,
        json!({"cmd": "add_drawing_view", "drawing": d.as_str(), "type": "base",
               "direction": "top", "at": [300, 120], "scale": "1:1"}),
    );
    assert_eq!(view["view"], top + 1);
    let note = command(
        &mut loaded,
        json!({"cmd": "add_drawing_annotation", "drawing": d.as_str(), "type": "text",
               "at": [20, 40], "text": "NEXT"}),
    );
    assert_eq!(note["annotation"], top + 1);
    // One past the largest is refused as the file is read.
    let above = top + 1;
    for (field, message) in [
        ("uid", "invalid drawing id 'D4293918720'"),
        ("exported", "invalid export count '4293918720'"),
        ("view", "invalid view id '4293918720'"),
        ("note", "invalid annotation id '4293918720'"),
    ] {
        let mut bad = file.clone();
        let kept = &mut bad["drawings"][0];
        match field {
            "uid" => kept["uid"] = json!(format!("D{above}")),
            "exported" => kept["exported"] = json!(above),
            "view" => kept["sheets"][0]["views"][1]["id"] = json!(above),
            _ => kept["sheets"][0]["notes"][0]["id"] = json!(above),
        }
        let Err(error) = Document::from_json(&bad.to_string(), MockKernel::default()) else {
            panic!("{field} {above} was read");
        };
        assert!(error.to_string().contains(message), "{error}");
    }
}
