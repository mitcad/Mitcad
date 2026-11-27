// SPDX-License-Identifier: MIT
//! A section's extent (mitcad#2479): a section of the whole plane marks
//! the whole parent view however short its line; one limited to its line
//! (`bounded`) marks the line and gives the kernel the strip it cuts; both
//! read back after saving, on the sheet and in the PDF; files from before
//! read their sections as of the whole plane.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::drawing::trim_number;
use mitcad_model::internal::profile::dot;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use mitcad_model_drawing::def::{Drawing, ViewKind};

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

fn point(value: &Value) -> [f64; 2] {
    [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()]
}

/// A block 40 x 30 x 20 from the origin and a drawing with its front view
/// at 1:1 in the middle of an A3 sheet at [100, 120]: the block's x 0..40
/// and z 0..20 on the sheet's 80..120 and 110..130.
fn front_view() -> Document<MockKernel> {
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
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    doc
}

/// A section on the front view along a short upright line, 4 mm of the
/// block's 20 mm height; its id.
fn section(doc: &mut Document<MockKernel>, bounded: Option<bool>) -> u64 {
    let mut add = json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section",
                         "parent": 1, "line": [[95, 113], [95, 117]], "at": [250, 120]});
    if let Some(b) = bounded {
        add["bounded"] = json!(b);
    }
    command(doc, add)["view"].as_u64().unwrap()
}

/// A view of the first sheet in the `drawing` query.
fn view(doc: &Document<MockKernel>, id: u64) -> Value {
    query(doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0]["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no view {id}"))
}

/// The chain line of the section's mark on the first sheet: its two ends.
fn mark(doc: &Document<MockKernel>) -> [[f64; 2]; 2] {
    let groups =
        query(doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0]["groups"].clone();
    let group = groups
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["class"] == "section-line")
        .cloned()
        .unwrap_or_else(|| panic!("no section line: {groups}"));
    let chain = group["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["pen"] == "chain")
        .cloned()
        .unwrap_or_else(|| panic!("no chain line: {group}"));
    [point(&chain["line"][0]), point(&chain["line"][1])]
}

/// How long the mark's chain line is on the sheet: the cut's extent and
/// 6 mm beyond each end.
fn mark_length(doc: &Document<MockKernel>) -> f64 {
    let [a, b] = mark(doc);
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// The bounds the kernel was given with the section planes it cut.
fn bounds_given(doc: &Document<MockKernel>) -> Vec<Option<mitcad_model::drawing::CutBound>> {
    doc.kernel()
        .projected_sections
        .borrow()
        .iter()
        .flatten()
        .map(|plane| plane.bound)
        .collect()
}

#[test]
fn a_full_section_marks_the_whole_view_however_short_its_line() {
    let mut doc = front_view();
    let id = section(&mut doc, None);
    assert_eq!(view(&doc, id)["bounded"], false);
    // Across the front view's 20 mm, not along the 4 mm line.
    let length = mark_length(&doc);
    assert!(near(length, 20.0 + 12.0), "{length}");
    // The kernel cuts the whole plane.
    let given = bounds_given(&doc);
    assert!(
        !given.is_empty() && given.iter().all(Option::is_none),
        "{given:?}"
    );
    // Not written in the file.
    let saved = doc.to_json();
    assert!(!saved.contains("\"bounded\""), "{saved}");
}

#[test]
fn a_section_limited_to_its_line_marks_and_cuts_only_along_it() {
    let mut doc = front_view();
    let id = section(&mut doc, Some(true));
    assert_eq!(view(&doc, id)["bounded"], true);
    let length = mark_length(&doc);
    assert!(near(length, 4.0 + 12.0), "{length}");
    // The kernel is given the strip along the line: 4 mm from the first
    // click, upright (the model's z, the front view's up), in the plane.
    let d: &Drawing = doc.find_drawing("D1").unwrap();
    let ViewKind::Section { normal, .. } = &d.sheets[0].view(id as u32).unwrap().kind else {
        panic!("a section");
    };
    let normal = *normal;
    let given = bounds_given(&doc);
    let bound = given
        .iter()
        .flatten()
        .next()
        .copied()
        .unwrap_or_else(|| panic!("no bound given: {given:?}"));
    assert!(near(bound.to - bound.from, 4.0), "{bound:?}");
    assert!(near(bound.from, 0.0) || near(bound.to, 0.0), "{bound:?}");
    assert!(near(dot(bound.along, normal), 0.0), "{bound:?}");
    assert!(near(bound.along[2].abs(), 1.0), "{bound:?}");

    // Saved and opened again: the same extent on the sheet.
    let before = mark(&doc);
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let kind = &file["drawings"][0]["sheets"][0]["views"][1]["kind"];
    assert_eq!(kind["bounded"], true, "{kind}");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(view(&loaded, id)["bounded"], true);
    assert_eq!(mark(&loaded), before);

    // The PDF draws the same mark.
    let dir = mitcad_model::testing::unique_temp_dir("section-extent");
    let pdf = dir.join("section.pdf").to_string_lossy().into_owned();
    command(
        &mut doc,
        json!({"cmd": "export_drawing", "path": pdf, "drawing": "D1"}),
    );
    let text = String::from_utf8_lossy(&std::fs::read(&pdf).unwrap()).into_owned();
    let n = |v: f64| trim_number(v, 4);
    let [a, b] = before;
    let line = format!("{} {} m {} {} l S", n(a[0]), n(a[1]), n(b[0]), n(b[1]));
    assert!(text.contains(&line), "{line}");
    std::fs::remove_dir_all(&dir).ok();

    // Back to the whole plane and again limited, by editing it.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": id, "bounded": false}),
    );
    assert_eq!(view(&doc, id)["bounded"], false);
    assert!(near(mark_length(&doc), 32.0));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(view(&doc, id)["bounded"], true);
    assert!(near(mark_length(&doc), 16.0));
    // Only a section has a line to limit it to.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "bounded": true}),
    );
    assert!(error.contains("has no section line"), "{error}");
}

#[test]
fn a_saved_section_from_before_is_of_the_whole_plane() {
    // A section as files had it before mitcad#2479: no `bounded`.
    let old = json!({
        "uid": "D1",
        "name": "Drawing1",
        "sheets": [{"name": "Sheet1", "size": "A3", "views": [
            {"id": 1, "kind": {"type": "base", "direction": "front"}, "place": [100, 120],
             "scale": 1.0},
            {"id": 2, "kind": {"type": "section", "parent": 1, "origin": [20.0, 0.0, 0.0],
             "normal": [-1.0, 0.0, 0.0], "span": [-5.0, 25.0], "letter": "A"},
             "place": [250, 120]}
        ]}]
    });
    let drawing: Drawing = serde_json::from_value(old).unwrap();
    drawing.check().unwrap();
    let ViewKind::Section { bounded, .. } = &drawing.sheets[0].views[1].kind else {
        panic!("a section");
    };
    assert!(!bounded);
    // Its spec cuts the whole plane.
    let spec = drawing.sheets[0].spec(2).unwrap();
    assert!(spec.section.is_some_and(|plane| plane.bound.is_none()));
    // Written again as it was read.
    let again = serde_json::to_value(&drawing).unwrap();
    assert!(!again.to_string().contains("\"bounded\""), "{again}");

    // Limited, its spec cuts the strip along the line on the front view
    // (the model's z).
    let mut limited = drawing.clone();
    if let ViewKind::Section { bounded, .. } = &mut limited.sheets[0].views[1].kind {
        *bounded = true;
    }
    limited.check().unwrap();
    let bound = limited.sheets[0]
        .spec(2)
        .unwrap()
        .section
        .unwrap()
        .bound
        .unwrap();
    assert!(near(bound.from, -5.0) && near(bound.to, 25.0), "{bound:?}");
    assert!(near(bound.along[2].abs(), 1.0), "{bound:?}");
    // A section without a line on a parent cannot be limited to it.
    if let ViewKind::Section { span, .. } = &mut limited.sheets[0].views[1].kind {
        *span = None;
    }
    assert!(limited.check().is_err());
}
