// SPDX-License-Identifier: MIT
//! The letters of a sketch text on a path as profiles (mitcad#1993): a
//! text picked while its letters overlapped on a tight arc, then flipped
//! to the arc's other side where they stand apart, extrudes every letter,
//! not the larger of each pair the old references lay between; and a
//! letter whose outline does not close is told, with its place.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::ProfileRef;
use mitcad_model::ids::FeatureUid;
use mitcad_model::internal::features::selected_regions;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const F1: FeatureUid = FeatureUid(1);

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn timeline_entry(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "timeline"}))["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == json!(uid))
        .cloned()
        .unwrap_or_else(|| panic!("no {uid} in the timeline"))
}

/// Sketch1 (F1) with a construction circle of radius 12 about the origin
/// and `word` 10 mm high centred along it, as the user's text on an arc
/// (the mock font's letters are 5 mm wide and 7 mm high, 7 mm apart).
/// Without flips the letters stand on the circle's inner side, upside
/// down, and lean into each other: neighbours overlap. Returns the
/// document and the text's id.
fn text_on_circle(word: &str) -> (Document<MockKernel>, String) {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 24}),
    );
    let sketch = query(&doc, json!({"query": "sketch", "uid": "F1"}));
    let circle = sketch["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "circle")
        .unwrap_or_else(|| panic!("no circle: {sketch}"))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!({"cmd": "sketch.set_construction", "sketch": "F1", "curves": [circle]}),
    );
    let made = command(
        &mut doc,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": word, "height": 10,
               "font": "Mock Sans", "align": "center", "path": {"curve": circle}}),
    );
    let text = made["texts"][0].as_str().unwrap().to_owned();
    (doc, text)
}

/// Turns the text half round about its middle on the circle (Flip
/// Horizontal and Flip Vertical, mitcad#1946): on the outer side, upright,
/// the letters apart.
fn flip(doc: &mut Document<MockKernel>, text: &str) {
    command(
        doc,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": text,
               "flip_x": true, "flip_y": true}),
    );
}

/// Every region of Sketch1 as a profile reference.
fn every_region(doc: &Document<MockKernel>) -> Vec<ProfileRef> {
    doc.sketch_output(F1)
        .expect("Sketch1 evaluated")
        .region_info
        .iter()
        .map(|r| ProfileRef {
            sketch: F1,
            region: r.profile.key.clone(),
        })
        .collect()
}

/// The current keys of the regions `profiles` select in Sketch1, sorted
/// (each selected region matched by its loops).
fn selected_keys(doc: &Document<MockKernel>, profiles: &[ProfileRef]) -> Vec<String> {
    let output = doc.sketch_output(F1).expect("Sketch1 evaluated");
    let selected =
        selected_regions(output, profiles).unwrap_or_else(|p| panic!("no region for {}", p.region));
    for region in &selected {
        assert!(
            output
                .region_info
                .iter()
                .any(|r| r.profile.loops == region.loops),
            "{} is no region of the sketch",
            region.key
        );
    }
    let mut keys: Vec<String> = output
        .region_info
        .iter()
        .filter(|r| selected.iter().any(|s| s.loops == r.profile.loops))
        .map(|r| r.profile.key.to_string())
        .collect();
    keys.sort();
    keys
}

fn extrude(doc: &mut Document<MockKernel>, profiles: &[ProfileRef]) -> String {
    let def = json!({"type": "extrude", "profiles": profiles,
                     "extent": {"type": "distance", "distance": 2}, "operation": "new_body"});
    let added = command(doc, json!({"cmd": "add_feature", "def": def}));
    added["uid"].as_str().unwrap().to_owned()
}

#[test]
fn letters_picked_where_they_overlapped_all_extrude_after_the_flip() {
    // Picked inside the circle, where each letter's outline crosses its
    // neighbour's: the regions are the letters' parts and the lenses
    // between them, every one of them picked.
    let (mut doc, text) = text_on_circle("LMN");
    let picked = every_region(&doc);
    assert!(picked.len() > 3, "the letters overlap: {picked:?}");
    let extrusion = extrude(&mut doc, &picked);
    assert_eq!(timeline_entry(&doc, &extrusion)["status"], json!("ok"));
    // Flipped to the outer side the three letters stand apart, three
    // regions. Every old reference names pieces of two outlines that no
    // longer cross; each finds both letters' regions, so the extrusion
    // takes all three letters (it took the first two only, the last
    // letter lost to the tie between equal letters).
    flip(&mut doc, &text);
    let mut letters: Vec<String> = every_region(&doc)
        .iter()
        .map(|p| p.region.to_string())
        .collect();
    letters.sort();
    let t = &text;
    assert_eq!(
        letters,
        [
            format!("r{{{t}.g0.c0}}"),
            format!("r{{{t}.g1.c0}}"),
            format!("r{{{t}.g2.c0}}")
        ]
    );
    assert_eq!(selected_keys(&doc, &picked), letters);
    // The references found under other keys warn, the regions their places.
    let entry = timeline_entry(&doc, &extrusion);
    assert_eq!(entry["status"], json!("warning"), "{entry}");
}

#[test]
fn the_reported_references_between_b_and_c_find_both_letters() {
    // The reported design's Emboss as it was stored: the A, and two
    // regions between the outlines of the B and the C, picked when they
    // overlapped. With the letters apart, both references found the B
    // alone (the larger of the two), and the C was not raised.
    let (mut doc, text) = text_on_circle("ABC");
    flip(&mut doc, &text);
    let t = &text;
    let reference = |region: String| -> ProfileRef {
        serde_json::from_value(json!({"sketch": "F1", "region": region})).unwrap()
    };
    let profiles = [
        reference(format!("r{{{t}.g0.c0}}")),
        reference(format!(
            "r{{{t}.g1.c0[{t}.g2.c0,{t}.g2.c0]#0,{t}.g2.c0[{t}.g1.c0,{t}.g1.c0]#0}}"
        )),
        reference(format!(
            "r{{{t}.g1.c0[{t}.g2.c0,{t}.g2.c0]#1,{t}.g2.c0[{t}.g1.c0,{t}.g1.c0]#0}}"
        )),
    ];
    // The letters with their counters as holes; the counters' insides
    // stay out.
    assert_eq!(
        selected_keys(&doc, &profiles),
        [
            format!("r{{{t}.g0.c0}}"),
            format!("r{{{t}.g1.c0}}"),
            format!("r{{{t}.g2.c0}}")
        ]
    );
    // The profile query shows both letters for each of the two.
    for p in &profiles[1..] {
        let means = query(
            &doc,
            json!({"query": "profile", "sketch": "F1", "region": p.region}),
        );
        assert_eq!(means["exact"], json!(false));
        let mut found: Vec<&str> = means["regions"]
            .as_array()
            .unwrap_or_else(|| panic!("{means}"))
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect();
        found.sort_unstable();
        assert_eq!(
            found,
            [format!("r{{{t}.g1.c0}}"), format!("r{{{t}.g2.c0}}")],
            "{means}"
        );
    }
}

#[test]
fn a_letter_whose_outline_does_not_close_is_told() {
    // The mock font's `~` is an outline with a gap: it bounds no profile,
    // and the sketch says so in words, the letter's place a box about it.
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "L~", "at": [0, 0],
               "height": 10, "font": "Mock Sans"}),
    );
    assert_eq!(every_region(&doc).len(), 1);
    let entry = timeline_entry(&doc, "F1");
    assert_eq!(entry["status"], json!("warning"), "{entry}");
    let notice = entry["notices"]
        .as_array()
        .unwrap_or_else(|| panic!("{entry}"))
        .iter()
        .find(|n| {
            n["message"]
                == json!(
                    "an outline of the letter ~ of a text is not closed, so it bounds no profile"
                )
        })
        .unwrap_or_else(|| panic!("no notice of the letter: {entry}"));
    let places = notice["places"].as_array().unwrap();
    assert_eq!(places.len(), 1, "{notice}");
    assert_eq!(places[0]["kind"], json!("box"));
    // About the second letter: one advance (7 mm) and its side bearing
    // from the start, from 8 mm to 13 mm and 0 to 7 mm high, with a margin.
    let (min, max) = (&places[0]["min"], &places[0]["max"]);
    let at = |v: &Value, i: usize| v[i].as_f64().unwrap();
    assert!(at(min, 0) < 8.0 && at(min, 0) > 7.0, "{notice}");
    assert!(at(max, 0) > 13.0 && at(max, 0) < 14.0, "{notice}");
    assert!(at(min, 1) < 0.0 && at(max, 1) > 7.0, "{notice}");
    assert!(at(min, 2) < 0.0 && at(max, 2) > 0.0, "{notice}");
}
