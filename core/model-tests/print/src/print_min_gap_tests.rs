// SPDX-License-Identifier: MIT
//! Narrow gaps between bodies printed in place (mitcad#1519) with the mock
//! kernel: the `print_min_gap` query on blocks 0.2 mm and 0.5 mm apart and
//! touching ones (left to `interference`, but their faces measured for a
//! narrow gap beside the contact, mitcad#1672), the gap from the query, the
//! design's printed fit parameter or the default, a plate's bodies, the
//! results kept by the bodies' shapes, cancellation, errors, the print
//! analysis' `min_gap` settings and `design_review`'s `min-gap` rule.

use std::sync::Arc;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::monitor::RecomputeMonitor;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
/// The baseline tolerance of geometric comparisons, mm.
const TOL: f64 = 1e-6;

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

fn query_error(doc: &Document<MockKernel>, query: Value) -> String {
    match doc.query(&query.to_string()) {
        Ok(answer) => panic!("{query} was answered: {answer}"),
        Err(e) => e.to_string(),
    }
}

fn close(value: &Value, expected: f64) -> bool {
    value
        .as_f64()
        .is_some_and(|value| (value - expected).abs() <= TOL)
}

fn measured(doc: &Document<MockKernel>) -> usize {
    doc.kernel()
        .calls
        .borrow()
        .get("measure_between")
        .copied()
        .unwrap_or(0)
}

/// A block of `width` x `depth` x 20 mm with its corner at `corner` on XY,
/// a body of its own.
fn block(doc: &mut Document<MockKernel>, corner: [f64; 2], width: f64, depth: f64) {
    let sketch = command(doc, json!({"cmd": "sketch.create"}));
    let sketch = sketch["uid"].as_str().unwrap().to_owned();
    command(
        doc,
        json!([
            {"cmd": "sketch.add_rectangle", "sketch": sketch, "corner": corner,
             "width": width, "height": depth},
            {"cmd": "add_feature", "def": {"type": "extrude",
              "profiles": [{"sketch": sketch, "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
}

/// Four blocks of 20 mm: A (F2.b0) at the origin, B (F4.b0) 0.2 mm beside
/// it along X, C (F6.b0) 0.5 mm beside B, and D (F8.b0, 10 mm wide)
/// touching A from -Y.
fn blocks() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    block(&mut doc, [0.0, 0.0], 20.0, 20.0);
    block(&mut doc, [20.2, 0.0], 20.0, 20.0);
    block(&mut doc, [40.7, 0.0], 20.0, 20.0);
    block(&mut doc, [0.0, -20.0], 10.0, 20.0);
    let uids: Vec<String> = doc.bodies().iter().map(|b| b.uid.to_string()).collect();
    assert_eq!(uids, ["F2.b0", "F4.b0", "F6.b0", "F8.b0"]);
    doc
}

#[test]
fn bodies_0_2_mm_apart_are_reported_0_5_mm_apart_and_touching_ones_are_not() {
    let doc = blocks();
    let answer = query(&doc, json!({"query": "print_min_gap"}));
    assert!(close(&answer["gap"], 0.3), "{answer}");
    assert_eq!(answer["gap_from"], "default");
    assert_eq!(answer["bodies"], 4);
    let pairs = answer["pairs"].as_array().unwrap();
    assert_eq!(pairs.len(), 1, "{answer}");
    let pair = &pairs[0];
    assert_eq!(pair["a"], "F2.b0");
    assert_eq!(pair["b"], "F4.b0");
    assert_eq!(pair["names"].as_array().unwrap().len(), 2);
    assert!(close(&pair["distance"], 0.2), "{pair}");
    assert!(close(&answer["closest"], 0.2));
    // The closest points, in design coordinates: on A's side at x = 20,
    // on B's at x = 20.2.
    assert!(close(&pair["on_a"][0], 20.0), "{pair}");
    assert!(close(&pair["on_b"][0], 20.2), "{pair}");
    // A and D touch: interference's, not a narrow gap.
    assert_eq!(answer["touching"], 1, "{answer}");

    // The same with the bodies named, in any order.
    let named = query(
        &doc,
        json!({"query": "print_min_gap", "bodies": ["F6.b0", "F4.b0"]}),
    );
    assert_eq!(named["bodies"], 2);
    assert_eq!(named["pairs"], json!([]), "0.5 mm is no narrow gap");
    assert_eq!(named["closest"], Value::Null);
    assert_eq!(named["touching"], 0);
}

#[test]
fn the_gap_comes_from_the_query_the_printed_fit_parameter_or_the_default() {
    let mut doc = blocks();
    // A wider gap: B and C are too close as well, the closest first.
    let answer = query(&doc, json!({"query": "print_min_gap", "gap": 0.6}));
    assert_eq!(answer["gap_from"], "query");
    let pairs = answer["pairs"].as_array().unwrap();
    assert_eq!(pairs.len(), 2, "{answer}");
    assert_eq!(
        (&pairs[0]["a"], &pairs[0]["b"]),
        (&json!("F2.b0"), &json!("F4.b0"))
    );
    assert_eq!(
        (&pairs[1]["a"], &pairs[1]["b"]),
        (&json!("F4.b0"), &json!("F6.b0"))
    );
    assert!(close(&pairs[1]["distance"], 0.5), "{answer}");
    // A gap at the threshold is no narrow gap.
    let at = query(&doc, json!({"query": "print_min_gap", "gap": 0.2}));
    assert_eq!(at["pairs"], json!([]), "{at}");
    // The design's print_in_place_gap.
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "print_in_place_gap", "expression": "0.6 mm"}),
    );
    let answer = query(&doc, json!({"query": "print_min_gap"}));
    assert_eq!(answer["gap_from"], "parameter");
    assert!(close(&answer["gap"], 0.6), "{answer}");
    assert_eq!(answer["pairs"].as_array().unwrap().len(), 2);
    // The query's own gap wins.
    let answer = query(&doc, json!({"query": "print_min_gap", "gap": 0.3}));
    assert_eq!(answer["pairs"].as_array().unwrap().len(), 1);

    for (bad, says) in [
        (json!({"query": "print_min_gap", "gap": 0}), "gap"),
        (json!({"query": "print_min_gap", "gap": -0.1}), "gap"),
        (json!({"query": "print_min_gap", "gaps": 0.3}), "gaps"),
        (
            json!({"query": "print_min_gap", "bodies": ["F2.b0"], "plate": "plate1"}),
            "either",
        ),
        (json!({"query": "print_min_gap", "plate": "Hinge"}), "Hinge"),
    ] {
        let error = query_error(&doc, bad.clone());
        assert!(error.contains(says), "{bad}: {error}");
    }
    // A body that is not there.
    let answer = query(&doc, json!({"query": "print_min_gap", "bodies": ["F9.b0"]}));
    assert!(
        answer["error"].as_str().unwrap().contains("F9.b0"),
        "{answer}"
    );
}

#[test]
fn a_plate_checks_the_bodies_that_print_on_it() {
    let mut doc = blocks();
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(&mut doc, json!({"cmd": "add_print_plate", "name": "Hinge"}));
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "Hinge",
               "items": [{"body": "F2.b0"}, {"body": "F4.b0"}]}),
    );
    let hinge = query(&doc, json!({"query": "print_min_gap", "plate": "Hinge"}));
    assert_eq!(hinge["bodies"], 2, "{hinge}");
    assert_eq!(hinge["pairs"].as_array().unwrap().len(), 1);
    // What no item names prints on the first plate: C and D, far apart.
    let first = query(&doc, json!({"query": "print_min_gap", "plate": "plate1"}));
    assert_eq!(first["bodies"], 2, "{first}");
    assert_eq!(first["pairs"], json!([]));
    assert_eq!(first["touching"], 0);
}

#[test]
fn distances_are_kept_by_the_bodies_and_far_bodies_are_not_measured() {
    let doc = blocks();
    query(&doc, json!({"query": "print_min_gap"}));
    // Only A-B and A-D (touching) have boxes within 0.3 mm of each other:
    // B and C are 0.5 mm apart, the others farther.
    assert_eq!(measured(&doc), 2, "A-B and A-D");
    // Asked again, with a smaller gap: the distances are kept.
    query(&doc, json!({"query": "print_min_gap", "gap": 0.25}));
    assert_eq!(measured(&doc), 2);
    // A wider gap reaches B-C too: only it is measured.
    query(&doc, json!({"query": "print_min_gap", "gap": 0.6}));
    assert_eq!(measured(&doc), 3);
}

#[test]
fn a_cancelled_job_stops_the_gaps() {
    let mut doc = blocks();
    let monitor = Arc::new(RecomputeMonitor::new());
    doc.set_monitor(Some(Arc::clone(&monitor)));
    monitor.cancel();
    let error = query_error(&doc, json!({"query": "print_min_gap"}));
    assert_eq!(error, "the computation was canceled");
    doc.set_monitor(None);
    let answer = query(&doc, json!({"query": "print_min_gap"}));
    assert_eq!(answer["pairs"].as_array().unwrap().len(), 1);
}

#[test]
fn a_print_analysis_keeps_its_min_gap_settings() {
    let mut doc = blocks();
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "print", "bodies": ["F2.b0", "F4.b0"]}}),
    );
    let analyses = query(&doc, json!({"query": "analyses"}));
    assert_eq!(
        analyses[0]["min_gap"],
        json!({"enabled": true, "gap": null}),
        "{analyses}"
    );
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Print1", "def": {"type": "print",
               "bodies": ["F2.b0", "F4.b0"], "min_gap": {"enabled": false, "gap": 0.4}}}),
    );
    let analyses = query(&doc, json!({"query": "analyses"}));
    assert_eq!(
        analyses[0]["min_gap"],
        json!({"enabled": false, "gap": 0.4})
    );
    // Saved and read back.
    let saved = doc.to_json();
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    for (def, says) in [
        (
            json!({"type": "print", "bodies": ["F2.b0"], "min_gap": {"gap": 0}}),
            "gap",
        ),
        (
            json!({"type": "print", "bodies": ["F2.b0"], "min_gap": {"size": 0.3}}),
            "size",
        ),
    ] {
        let Err(error) = doc.command(&json!({"cmd": "add_analysis", "def": def}).to_string())
        else {
            panic!("{def} was accepted");
        };
        assert!(error.to_string().contains(says), "{def}: {error}");
    }
}

#[test]
fn design_review_reports_narrow_gaps_of_the_checked_bodies() {
    let doc = blocks();
    let answer = query(
        &doc,
        json!({"query": "design_review", "checks": ["min-gap"], "print": true}),
    );
    assert_eq!(answer["errors"], json!([]), "{answer}");
    let findings = answer["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1, "{answer}");
    let finding = &findings[0];
    assert_eq!(finding["rule"], "min-gap");
    assert_eq!(finding["level"], "warning");
    assert_eq!(finding["target"], "F2.b0");
    let message = finding["message"].as_str().unwrap();
    assert!(message.contains("0.20 mm apart"), "{message}");
    assert!(message.contains("0.3 mm"), "{message}");
    assert_eq!(finding["guide"]["topic"], "3d-printing");
    // Without print checks, none.
    let off = query(
        &doc,
        json!({"query": "design_review", "checks": ["min-gap"], "print": false}),
    );
    assert_eq!(off["findings"], json!([]));
}

/// A pair of faces `face_separations` gives: `on` the closest points (on
/// the first body, on the second) and `normals` the faces' outward normals
/// there.
fn faces(
    indices: [usize; 2],
    on: [[f64; 3]; 2],
    normals: [[f64; 3]; 2],
) -> mitcad_model::kernel::FaceSeparation {
    let d: Vec<f64> = (0..3).map(|k| on[1][k] - on[0][k]).collect();
    mitcad_model::kernel::FaceSeparation {
        face_a: indices[0],
        face_b: indices[1],
        distance: (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt(),
        on_a: on[0],
        on_b: on[1],
        normal_a: normals[0],
        normal_b: normals[1],
    }
}

/// A plate (F2.b0, 20 mm) and a wheel (F4.b0) lying on its face at
/// y = 20, as the mock's blocks: they touch. The wheel's peg of Ø5 goes
/// into the plate along -Y through a hole of Ø5.2 about x = 10, z = 10, as
/// the faces the kernel measures tell: the plate's face on the wheel's
/// (a contact), the hole's wall and the peg's 0.1 mm apart across air, the
/// hole's rim on the wheel (a contact) and the plate's face beside the
/// peg at the rim (0.1 mm, but facing the wheel, not the peg).
fn pegged() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    block(&mut doc, [0.0, 0.0], 20.0, 20.0);
    block(&mut doc, [0.0, 20.0], 20.0, 20.0);
    *doc.kernel().face_gaps.borrow_mut() = Some(vec![
        faces([0, 0], [[10.0, 20.0, 14.0]; 2], [[0.0; 3]; 2]),
        faces(
            [1, 1],
            [[12.6, 15.0, 10.0], [12.5, 15.0, 10.0]],
            [[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
        ),
        faces([1, 0], [[12.6, 20.0, 10.0]; 2], [[0.0; 3]; 2]),
        faces(
            [0, 1],
            [[12.6, 20.0, 10.0], [12.5, 20.0, 10.0]],
            [[0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
        ),
    ]);
    doc
}

fn face_measured(doc: &Document<MockKernel>) -> usize {
    doc.kernel()
        .calls
        .borrow()
        .get("face_separations")
        .copied()
        .unwrap_or(0)
}

#[test]
fn a_peg_in_its_hole_is_a_narrow_gap_though_the_bodies_touch() {
    // mitcad#1672: the wheel lies on the plate, so the bodies touch, but
    // the peg's wall is 0.1 mm from the hole's.
    let doc = pegged();
    let answer = query(&doc, json!({"query": "print_min_gap", "gap": 0.3}));
    let pairs = answer["pairs"].as_array().unwrap();
    assert_eq!(pairs.len(), 1, "{answer}");
    let pair = &pairs[0];
    assert_eq!((&pair["a"], &pair["b"]), (&json!("F2.b0"), &json!("F4.b0")));
    assert!(close(&pair["distance"], 0.1), "{pair}");
    assert_eq!(pair["touches"], true, "{pair}");
    // At the peg: the hole's wall on the plate, the peg's on the wheel.
    assert!(close(&pair["on_a"][0], 12.6), "{pair}");
    assert!(close(&pair["on_b"][0], 12.5), "{pair}");
    assert!(close(&pair["on_a"][1], 15.0), "{pair}");
    assert!(close(&answer["closest"], 0.1), "{answer}");
    assert_eq!(answer["touching"], 1, "{answer}");
    assert_eq!(answer["unmeasured"], 0, "{answer}");
    assert_eq!(face_measured(&doc), 1);

    // Asked again: kept. A narrower gap measures the faces again, and
    // 0.1 mm is no narrow gap under 0.1 mm.
    query(&doc, json!({"query": "print_min_gap", "gap": 0.3}));
    assert_eq!(face_measured(&doc), 1);
    let narrow = query(&doc, json!({"query": "print_min_gap", "gap": 0.1}));
    assert_eq!(narrow["pairs"], json!([]), "{narrow}");
    assert_eq!(narrow["unmeasured"], 0);
    assert_eq!(face_measured(&doc), 2);

    // The design review's min-gap rule finds it too.
    let review = query(
        &doc,
        json!({"query": "design_review", "checks": ["min-gap"], "print": true}),
    );
    let findings = review["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1, "{review}");
    assert!(
        findings[0]["message"]
            .as_str()
            .unwrap()
            .contains("0.10 mm apart"),
        "{review}"
    );
}

#[test]
fn touching_boxes_without_a_narrow_gap_elsewhere_report_none() {
    // mitcad#1672: two blocks face to face; their faces touch at the
    // contact and its edges, and the next faces are 20 mm away.
    let doc = pegged();
    *doc.kernel().face_gaps.borrow_mut() = Some(vec![
        faces([0, 0], [[10.0, 20.0, 10.0]; 2], [[0.0; 3]; 2]),
        faces([2, 2], [[20.0, 20.0, 10.0]; 2], [[0.0; 3]; 2]),
        faces([3, 0], [[20.0, 20.0, 10.0]; 2], [[0.0; 3]; 2]),
        faces(
            [4, 5],
            [[0.0, 0.0, 10.0], [0.0, 40.0, 10.0]],
            [[0.0, -1.0, 0.0], [0.0, 1.0, 0.0]],
        ),
    ]);
    let answer = query(&doc, json!({"query": "print_min_gap"}));
    assert_eq!(answer["pairs"], json!([]), "{answer}");
    assert_eq!(answer["closest"], Value::Null);
    assert_eq!(answer["touching"], 1);
    assert_eq!(answer["unmeasured"], 0, "{answer}");
}

#[test]
fn a_peg_that_overlaps_its_hole_is_no_gap() {
    // A peg 0.1 mm too thick: its wall lies in the plate, facing away
    // from the hole's, so the pair is interference's.
    let doc = pegged();
    *doc.kernel().face_gaps.borrow_mut() = Some(vec![
        faces([0, 0], [[10.0, 20.0, 14.0]; 2], [[0.0; 3]; 2]),
        faces(
            [1, 1],
            [[12.4, 15.0, 10.0], [12.5, 15.0, 10.0]],
            [[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
        ),
    ]);
    let answer = query(&doc, json!({"query": "print_min_gap"}));
    assert_eq!(answer["pairs"], json!([]), "{answer}");
    assert_eq!(answer["touching"], 1);
}

#[test]
fn touching_bodies_whose_faces_cannot_be_measured_are_counted() {
    // A kernel without face separations: the touching pair is not
    // measured, and the answer says so instead of claiming no gap.
    let doc = blocks();
    let answer = query(&doc, json!({"query": "print_min_gap"}));
    assert_eq!(answer["touching"], 1, "{answer}");
    assert_eq!(answer["unmeasured"], 1, "{answer}");
    // The apart pair is measured as before.
    assert_eq!(answer["pairs"].as_array().unwrap().len(), 1);
    assert_eq!(answer["pairs"][0]["touches"], false);
}
