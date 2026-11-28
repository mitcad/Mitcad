// SPDX-License-Identifier: MIT
//! Beads (mitcad#2645) on the mock kernel: a bead kept in the sheet's state
//! in its panel's chart, saved, reopened and edited; the Flat Pattern's
//! beads and their DXF layer; the refusals in words. The solids (each a
//! thickness thick across, their volumes) are checked with OCCT by the CLI
//! test `cli.sheet_bead`, the section's geometry by `sheet::bead::tests`.

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::{def, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;
use serde_json::{Value, json};

/// An 80 x 60 plate (t 1, r 2, front reference on XY), the kernel measuring.
fn plate() -> (Document<MockKernel>, BodyUid) {
    let mut doc = Document::new(MockKernel::default());
    let rules = json!({"type":"sheet_rules","thickness":1,"bend_radius":2,
        "allowance":{"type":"k_factor","k":0.4},"reference_side":"front",
        "bend_relief":{"type":"round","width":1,"depth":3},
        "corner_relief":{"type":"rectangular","width":1,"depth":3}});
    let rules = doc.add_feature(&def(rules), None).unwrap().uid;
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(80.0), &num(60.0))
        .unwrap()
        .region;
    let face =
        json!({"type":"sheet_face","rules":rules,"profiles":[{"sketch":sketch,"region":region}]});
    let uid = doc.add_feature(&def(face), None).unwrap().uid;
    doc.kernel().measures.set(true);
    (doc, BodyUid::new(uid, 0))
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// A new sketch on `plane` with a line from `start` to `end` (its `c1`).
fn line_sketch(
    doc: &mut Document<MockKernel>,
    plane: &str,
    start: [f64; 2],
    end: [f64; 2],
) -> FeatureUid {
    let made = command(doc, json!({"cmd":"sketch.create","plane":plane}));
    let sketch: FeatureUid = serde_json::from_value(made["uid"].clone()).unwrap();
    command(
        doc,
        json!({"cmd":"sketch.add_line","sketch":sketch,"start":start,"end":end}),
    );
    sketch
}

fn bead(body: BodyUid, sketch: FeatureUid, curves: Value, extra: Value) -> Value {
    let mut def = json!({"type":"sheet_bead","body":body,"sketch":sketch,"curves":curves,
        "width":10,"depth":3});
    for (key, value) in extra.as_object().unwrap() {
        def[key] = value.clone();
    }
    def
}

/// Whether a text carries an internal feature id (`F26`).
fn has_feature_id(text: &str) -> bool {
    text.as_bytes()
        .windows(2)
        .any(|w| w[0] == b'F' && w[1].is_ascii_digit())
}

#[test]
fn a_bead_is_kept_in_its_panels_chart_saved_reopened_and_edited() {
    let (mut doc, body) = plate();
    let sketch = line_sketch(&mut doc, "xy", [20.0, 30.0], [60.0, 30.0]);
    let revolves = doc.kernel().count("revolve");
    let uid = doc
        .add_feature(&def(bead(body, sketch, json!(["c1"]), json!({}))), None)
        .unwrap()
        .uid;
    assert_eq!(doc.status(uid), Some(&FeatureStatus::Ok));
    // A line: its two tools are prisms; its two round ends each turn the
    // half sections of both tools half a turn.
    assert_eq!(doc.kernel().count("revolve") - revolves, 4);
    let state = query(&doc, json!({"query":"sheet_state","body":body}));
    assert_eq!(state["status"], "valid", "{state}");
    let overlay = &state["overlays"][0];
    assert_eq!(overlay["type"], "bead", "{overlay}");
    assert_eq!(overlay["feature"], json!(uid));
    assert_eq!(overlay["panel"], json!({"feature":body.feature,"slot":0}));
    assert_eq!(overlay["side"], "front");
    assert_eq!(
        overlay["chains"][0]["elements"][0],
        json!({"type":"line","start":[20.0,30.0],"end":[60.0,30.0]})
    );
    assert_eq!(overlay["section"]["radius"], 2.0, "the rules' bend radius");
    assert_eq!(overlay["section"]["direction"], "out");

    // Saved and reopened: the same bead.
    let text = doc.to_json();
    let mut opened = Document::from_json(&text, MockKernel::default()).unwrap();
    opened.kernel().measures.set(true);
    opened.recompute();
    assert_eq!(opened.status(uid), Some(&FeatureStatus::Ok));
    let again = query(&opened, json!({"query":"sheet_state","body":body}));
    assert_eq!(again["overlays"][0]["chains"], overlay["chains"]);

    // Edited: a V pressed in with run-out ends and its own radius.
    let edited = bead(
        body,
        sketch,
        json!(["c1"]),
        json!({"profile":"v","direction":"in","ends":"run_out","run_out":6,"radius":1}),
    );
    doc.edit_feature(uid, &def(edited)).unwrap();
    assert_eq!(doc.status(uid), Some(&FeatureStatus::Ok));
    let state = query(&doc, json!({"query":"sheet_state","body":body}));
    let section = &state["overlays"][0]["section"];
    assert_eq!(section["profile"], "v");
    assert_eq!(section["ends"], "run_out");
    assert_eq!(section["run_out"], 6.0);
    assert_eq!(section["radius"], 1.0);

    // Undo leaves the sheet without it.
    doc.undo();
    doc.undo();
    let state = query(&doc, json!({"query":"sheet_state","body":body}));
    assert!(state.get("overlays").is_none(), "{state}");
}

#[test]
fn the_flat_pattern_draws_beads_on_their_own_dxf_layer() {
    let (mut doc, body) = plate();
    let sketch = line_sketch(&mut doc, "xy", [20.0, 30.0], [60.0, 30.0]);
    let uid = doc
        .add_feature(&def(bead(body, sketch, json!(["c1"]), json!({}))), None)
        .unwrap()
        .uid;
    assert_eq!(doc.status(uid), Some(&FeatureStatus::Ok));
    command(&mut doc, json!({"cmd":"set_flat_pattern","body":body}));
    let flat = query(&doc, json!({"query":"flat_pattern","body":body}));
    assert_eq!(flat["status"], "valid", "{flat}");
    // The blank stays flat.
    assert_eq!(flat["area"], 4800.0);
    let beads = flat["beads"].as_array().unwrap();
    assert_eq!(beads.len(), 1);
    assert_eq!(beads[0]["feature"], json!(uid));
    // Standing out of the front, viewed from the front.
    assert_eq!(beads[0]["direction"], "up");
    assert_eq!(
        beads[0]["centerlines"],
        json!([{"type":"line","start":[20.0,30.0],"end":[60.0,30.0]}])
    );
    // Two sides and two round ends.
    assert_eq!(beads[0]["outline"].as_array().unwrap().len(), 4);
    let dir = mitcad_model::testing::unique_temp_dir("bead-dxf");
    let path = dir.join("flat.dxf").to_string_lossy().into_owned();
    command(
        &mut doc,
        json!({"cmd":"export_flat_pattern_dxf","body":body,"path":path}),
    );
    let drawing = mitcad_dxf::read_file(&path).unwrap();
    assert!(drawing.layer("BEAD").is_some());
    let on_bead = drawing
        .entities
        .iter()
        .filter(|e| e.layer == "BEAD")
        .count();
    assert_eq!(on_bead, 5, "the outline's four pieces and the centre line");
    // The outline is not cut: the cut layers hold the plate's rectangle.
    assert_eq!(
        drawing
            .entities
            .iter()
            .filter(|e| e.layer == "CUT_OUTER")
            .count(),
        4
    );
    // Viewed from the back the bead is pressed away from the viewer.
    command(
        &mut doc,
        json!({"cmd":"set_flat_pattern","body":body,"reference_side":"back"}),
    );
    let back = query(&doc, json!({"query":"flat_pattern","body":body}));
    assert_eq!(back["beads"][0]["direction"], "down");
    assert_eq!(
        back["beads"][0]["centerlines"],
        json!([{"type":"line","start":[-20.0,30.0],"end":[-60.0,30.0]}])
    );
}

#[test]
fn beads_that_do_not_fit_are_refused_in_words() {
    let (mut doc, body) = plate();
    let refused = |doc: &mut Document<MockKernel>, def_: Value, part: &str| {
        let uid = doc.add_feature(&def(def_), None).unwrap().uid;
        let Some(FeatureStatus::Failed(error)) = doc.status(uid) else {
            panic!("not refused: {:?}", doc.status(uid));
        };
        assert!(error.message.contains(part), "{}", error.message);
        assert!(!has_feature_id(&error.message), "{}", error.message);
        doc.undo();
    };
    // A sketch that is not on a face of the sheet.
    let off = line_sketch(&mut doc, "xz", [20.0, 30.0], [60.0, 30.0]);
    refused(
        &mut doc,
        bead(body, off, json!(["c1"]), json!({})),
        "must lie on a flat face of the sheet",
    );
    // Too near the plate's edge for half its width.
    let near = line_sketch(&mut doc, "xy", [3.0, 30.0], [60.0, 30.0]);
    refused(
        &mut doc,
        bead(body, near, json!(["c1"]), json!({})),
        "does not fit on its face",
    );
    // A round bead deeper than half its width.
    let line = line_sketch(&mut doc, "xy", [20.0, 30.0], [60.0, 30.0]);
    refused(
        &mut doc,
        bead(body, line, json!(["c1"]), json!({"depth":6})),
        "at most half as deep as it is wide",
    );
    // A run-out longer than the line holds.
    refused(
        &mut doc,
        bead(
            body,
            line,
            json!(["c1"]),
            json!({"ends":"run_out","run_out":25}),
        ),
        "holds its ramp",
    );
    // A line and an arc at a corner (the arc leaves the line's end square to
    // it): c1, then the arc c4 about (55, 30) from the line's end p3.
    let corner = line_sketch(&mut doc, "xy", [20.0, 30.0], [40.0, 30.0]);
    command(
        &mut doc,
        json!({"cmd":"sketch.arc","sketch":corner,"mode":"center","center":[55,30],
               "start":"p3","end":[55,45]}),
    );
    refused(
        &mut doc,
        bead(body, corner, json!(["c1", "c4"]), json!({})),
        "turns at a corner only where two lines meet",
    );
    // An arc tighter than half the bead's width.
    let made = command(&mut doc, json!({"cmd":"sketch.create","plane":"xy"}));
    let tight: FeatureUid = serde_json::from_value(made["uid"].clone()).unwrap();
    command(
        &mut doc,
        json!({"cmd":"sketch.arc","sketch":tight,"mode":"center","center":[40,30],
               "start":[44,30],"end":[40,34]}),
    );
    refused(
        &mut doc,
        bead(body, tight, json!(["c1"]), json!({})),
        "tighter than half the bead's width",
    );
    // A second bead too near the first.
    let first = doc
        .add_feature(&def(bead(body, line, json!(["c1"]), json!({}))), None)
        .unwrap()
        .uid;
    assert_eq!(doc.status(first), Some(&FeatureStatus::Ok));
    let close = line_sketch(&mut doc, "xy", [20.0, 38.0], [60.0, 38.0]);
    refused(
        &mut doc,
        bead(body, close, json!(["c1"]), json!({})),
        "beads overlap",
    );
}
