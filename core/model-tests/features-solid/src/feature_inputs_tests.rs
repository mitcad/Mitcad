// SPDX-License-Identifier: MIT
//! Feature inputs drawn where they were (mitcad#957), with the mock kernel:
//! a Fillet's and a Chamfer's build keeps its picked edges as polylines,
//! the `feature_inputs` query returns them without a geometry call, and a
//! document opened without building has none.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use mitcad_model::datum::CurveGeometry;
use mitcad_model::ids::FeatureUid;
use mitcad_model::test_support::{Block, block, corner, def};
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{EdgeName, FaceName};
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None).unwrap().uid
}

fn inputs(doc: &Document<MockKernel>, uid: FeatureUid) -> Value {
    let query = json!({"query": "feature_inputs", "uid": uid}).to_string();
    serde_json::from_str(&doc.query(&query).unwrap()).unwrap()
}

fn calls(doc: &Document<MockKernel>) -> BTreeMap<&'static str, usize> {
    doc.kernel().calls.borrow().clone()
}

/// The body's edge that `edge` names, by its canonical name (the mock's
/// `edge_curves` key).
fn body_edge(b: &Block, edge: &EdgeName) -> String {
    let shape = b.doc.body_shape(b.body).expect("the block's body");
    let [x, y] = edge.faces();
    shape
        .edges
        .iter()
        .find(|(one, other)| {
            (one.matches(x) && other.matches(y)) || (one.matches(y) && other.matches(x))
        })
        .map(|(one, other)| EdgeName::new(one.clone(), other.clone()).to_string())
        .unwrap_or_else(|| panic!("no edge {edge} in the block"))
}

/// The block (F1, F2) with its four vertical edges rounded (F3).
fn rounded() -> (Block, FeatureUid) {
    let mut b = block();
    let edges: Vec<EdgeName> = (0..4).map(|i| corner(b.extrude, 1, i)).collect();
    // The first edge's curve: a line up the block's corner.
    let first = body_edge(&b, &edges[0]);
    b.doc.kernel().edge_curves.borrow_mut().insert(
        first,
        CurveGeometry::Line {
            start: [0.0, 0.0, 0.0],
            end: [0.0, 0.0, 20.0],
        },
    );
    let body = b.body;
    let fillet = add(
        &mut b.doc,
        json!({"type": "fillet", "body": body, "edges": edges, "radius": 2.0}),
    );
    (b, fillet)
}

#[test]
fn a_fillet_keeps_its_picked_edges_and_the_query_only_reads_them() {
    let (b, fillet) = rounded();
    assert_eq!(b.doc.status(fillet), Some(&FeatureStatus::Ok));
    // Worked out by the build, once with each rounding, on the body before
    // the fillet.
    let kernel = b.doc.kernel();
    assert!(kernel.count("fillet") >= 1);
    assert_eq!(kernel.count("edge_polylines"), kernel.count("fillet"));

    let before = calls(&b.doc);
    let answer = inputs(&b.doc, fillet);
    assert_eq!(calls(&b.doc), before, "the query made a geometry call");
    assert_eq!(answer["uid"], json!(fillet));
    assert_eq!(answer["name"], "Fillet1");
    assert_eq!(answer["stored"], true);
    let lines = answer["edges"].as_array().expect("edges");
    assert_eq!(lines.len(), 4, "{answer}");
    for line in lines {
        let points = line.as_array().expect("a polyline");
        assert!(points.len() >= 2, "{line}");
        assert!(
            points
                .iter()
                .all(|p| p.as_array().is_some_and(|p| p.len() == 3))
        );
    }
    assert_eq!(lines[0], json!([[0.0, 0.0, 0.0], [0.0, 0.0, 20.0]]));
    // The same as the model keeps them.
    let kept = b.doc.feature_inputs(fillet).expect("kept");
    assert_eq!(kept.edges.len(), 4);
    assert_eq!(kept.edges[0], vec![[0.0, 0.0, 0.0], [0.0, 0.0, 20.0]]);

    // Asked again (the selection, then the hover), still no geometry call.
    let again = inputs(&b.doc, fillet);
    assert_eq!(again, answer);
    assert_eq!(calls(&b.doc), before);
}

#[test]
fn a_document_opened_without_building_has_no_inputs_and_computes_none() {
    let (b, fillet) = rounded();
    let json = b.doc.to_json();
    let opened = Document::from_json(&json, MockKernel::default()).unwrap();
    let before = calls(&opened);
    let answer = inputs(&opened, fillet);
    assert_eq!(answer["stored"], false, "{answer}");
    assert_eq!(answer["edges"], json!([]));
    assert!(opened.feature_inputs(fillet).is_none());
    // Nothing is built for the query: no fillet, no polylines.
    assert_eq!(calls(&opened), before, "the query made a geometry call");
    assert_eq!(opened.kernel().count("edge_polylines"), 0);
    assert_eq!(opened.kernel().count("fillet"), 0);

    // Its next build keeps them.
    let mut opened = opened;
    opened.recompute();
    assert!(opened.kernel().count("fillet") >= 1);
    assert_eq!(
        opened.kernel().count("edge_polylines"),
        opened.kernel().count("fillet")
    );
    assert_eq!(
        inputs(&opened, fillet)["edges"].as_array().map(Vec::len),
        Some(4)
    );
}

#[test]
fn a_chamfer_keeps_the_edges_of_its_faces_each_once() {
    let mut b = block();
    let top = FaceName::end(b.extrude, b.region.clone());
    let side = corner(b.extrude, 1, 0);
    let shape = b.doc.body_shape(b.body).expect("the block's body").clone();
    // The top face's edges, and the vertical edge besides.
    let expected = shape
        .edges
        .iter()
        .filter(|(one, other)| one.matches(&top) || other.matches(&top))
        .count()
        + 1;
    assert!(expected > 1);
    let body = b.body;
    let chamfer = add(
        &mut b.doc,
        json!({"type": "chamfer", "body": body, "sets": [
            {"faces": [top], "size": {"type": "equal_distance", "distance": 1.0}},
            {"edges": [side], "size": {"type": "equal_distance", "distance": 1.0}}]}),
    );
    assert_eq!(b.doc.status(chamfer), Some(&FeatureStatus::Ok));
    let answer = inputs(&b.doc, chamfer);
    assert_eq!(answer["stored"], true);
    assert_eq!(
        answer["edges"].as_array().map(Vec::len),
        Some(expected),
        "{answer}"
    );
}

#[test]
fn features_without_inputs_failed_suppressed_or_rolled_back_have_none() {
    let (mut b, fillet) = rounded();
    // Not a feature with picked inputs.
    let extrude = inputs(&b.doc, b.extrude);
    assert_eq!(extrude["stored"], false);
    assert_eq!(extrude["edges"], json!([]));
    // Rolled back.
    b.doc.set_marker(2).unwrap();
    assert_eq!(inputs(&b.doc, fillet)["stored"], false);
    b.doc.set_marker(3).unwrap();
    assert_eq!(inputs(&b.doc, fillet)["stored"], true);
    // Suppressed.
    b.doc.set_suppressed(fillet, true).unwrap();
    assert_eq!(inputs(&b.doc, fillet)["stored"], false);
    b.doc.set_suppressed(fillet, false).unwrap();
    assert_eq!(inputs(&b.doc, fillet)["stored"], true);
    // Failed: nothing kept, though the edges were found.
    b.doc.kernel().fail.borrow_mut().insert("fillet");
    b.doc.set_parameter("d3", 3.0).unwrap();
    assert!(matches!(
        b.doc.status(fillet),
        Some(FeatureStatus::Failed(_))
    ));
    assert_eq!(inputs(&b.doc, fillet)["stored"], false);
    // An unknown feature is an error.
    let error = b
        .doc
        .query(&json!({"query": "feature_inputs", "uid": "F99"}).to_string())
        .unwrap_err();
    assert!(error.to_string().contains("F99"), "{error}");
}

#[test]
fn a_kernel_without_polylines_builds_the_fillet_all_the_same() {
    let mut b = block();
    b.doc.kernel().fail.borrow_mut().insert("edge_polylines");
    let edge = corner(b.extrude, 1, 0);
    let body = b.body;
    let fillet = add(
        &mut b.doc,
        json!({"type": "fillet", "body": body, "edges": [edge], "radius": 2.0}),
    );
    // Asked for with the rounding, refused, and the fillet built without
    // them.
    assert!(b.doc.kernel().count("edge_polylines") >= 1);
    assert_eq!(b.doc.status(fillet), Some(&FeatureStatus::Ok));
    assert_eq!(inputs(&b.doc, fillet)["stored"], false);
}
