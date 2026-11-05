// SPDX-License-Identifier: MIT
//! An Envelope's source occurrence (mitcad#1276) with the mock kernel: a
//! component copied with Paste New keeps its Envelope's source (the copy's
//! own occurrence), and an occurrence moved to another parent takes the
//! Envelopes that read it along, as other occurrence paths are
//! (`joints::remap_occurrences`, `reparent.rs`); a move that would take
//! the source out of the Envelope's component is refused.

use serde_json::json;

use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::test_support::{def, extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, FeatureStatus};

/// A 10 x 10 x 5 block in the active component.
fn block(doc: &mut Document<MockKernel>) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    doc.add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap();
}

/// A component with a block placed in the active component at x = `x`;
/// the active component stays.
fn machine(doc: &mut Document<MockKernel>, name: &str, x: f64) -> OccurrenceUid {
    let active = doc.assembly().active;
    let (_, occurrence) = doc
        .create_component(Some(name), Transform::translation([x, 0.0, 0.0]), true)
        .unwrap();
    block(doc);
    doc.activate_component(active).unwrap();
    occurrence
}

/// An Envelope of the occurrence path in the active component.
fn envelope(doc: &mut Document<MockKernel>, path: &str) -> FeatureUid {
    let added = doc
        .add_feature(
            &def(json!({"type": "envelope", "occurrence": path, "detail": 10})),
            None,
        )
        .unwrap();
    assert_eq!(doc.status(added.uid), Some(&FeatureStatus::Ok));
    added.uid
}

/// The source occurrence path of an Envelope.
fn source(doc: &Document<MockKernel>, uid: FeatureUid) -> String {
    match &doc.feature(uid).unwrap().def {
        FeatureDef::Envelope(e) => e.occurrence.to_string(),
        other => panic!("{uid} is not an envelope: {other:?}"),
    }
}

#[test]
fn a_copied_component_keeps_its_envelopes_source() {
    let mut doc = Document::new(MockKernel::default());
    // Cell:1 holds Machine:1 and an Envelope of it.
    let (cell, cell_occurrence) = doc
        .create_component(Some("Cell"), Transform::IDENTITY, true)
        .unwrap();
    let inner = machine(&mut doc, "Machine", 100.0);
    let original = envelope(&mut doc, &inner.to_string());
    doc.activate_component(ComponentUid::ROOT).unwrap();

    // Paste New of Cell:1: a new component with its own Machine
    // occurrence, and its Envelope reads that one.
    let (copy, _) = doc
        .paste_new(
            cell_occurrence,
            Some(Transform::translation([0.0, 300.0, 0.0])),
        )
        .unwrap();
    assert_ne!(copy, cell);
    let copied_inner = doc.assembly().children(copy).next().unwrap().uid;
    assert_ne!(copied_inner, inner);
    let copied = doc
        .features()
        .find(|f| f.component == copy && matches!(f.def, FeatureDef::Envelope(_)))
        .map(|f| f.uid)
        .expect("the copy's envelope");
    assert_eq!(source(&doc, copied), copied_inner.to_string());
    assert_eq!(doc.status(copied), Some(&FeatureStatus::Ok));
    // The original's is as it was.
    assert_eq!(source(&doc, original), inner.to_string());
    assert_eq!(doc.status(original), Some(&FeatureStatus::Ok));
}

#[test]
fn a_reparented_occurrence_takes_its_envelope_along() {
    let mut doc = Document::new(MockKernel::default());
    // Cell:1 (O1) empty at the origin, Machine:1 (O2) in the root, an
    // Envelope of Machine:1 in the root.
    let (cell_component, cell) = doc
        .create_component(Some("Cell"), Transform::IDENTITY, false)
        .unwrap();
    let inner = machine(&mut doc, "Machine", 100.0);
    let root_envelope = envelope(&mut doc, &inner.to_string());

    // Machine:1 moved into Cell:1: the Envelope reads it there.
    doc.reparent_occurrence(&[inner], &[cell]).unwrap();
    assert_eq!(
        doc.assembly().occurrence(inner).unwrap().parent,
        cell_component
    );
    assert_eq!(source(&doc, root_envelope), format!("{cell}/{inner}"));
    assert_eq!(doc.status(root_envelope), Some(&FeatureStatus::Ok));
    // And back into the root: the path is the occurrence again.
    doc.reparent_occurrence(&[cell, inner], &[]).unwrap();
    assert_eq!(source(&doc, root_envelope), inner.to_string());
    assert_eq!(doc.status(root_envelope), Some(&FeatureStatus::Ok));
    // One undo step each.
    assert!(doc.undo().is_some());
    assert_eq!(source(&doc, root_envelope), format!("{cell}/{inner}"));
}

#[test]
fn an_occurrence_does_not_leave_the_component_of_its_envelope() {
    let mut doc = Document::new(MockKernel::default());
    // Cell:1 (O1) holds Machine:1 and an Envelope of it.
    let (cell_component, cell) = doc
        .create_component(Some("Cell"), Transform::IDENTITY, true)
        .unwrap();
    let inner = machine(&mut doc, "Machine", 100.0);
    let cell_envelope = envelope(&mut doc, &inner.to_string());
    doc.activate_component(ComponentUid::ROOT).unwrap();
    // Out of Cell:1 into the root: the Envelope would lose its source.
    let error = doc
        .reparent_occurrence(&[cell, inner], &[])
        .unwrap_err()
        .to_string();
    assert!(error.contains("Envelope"), "{error}");
    assert_eq!(
        doc.assembly().occurrence(inner).unwrap().parent,
        cell_component
    );
    assert_eq!(source(&doc, cell_envelope), inner.to_string());
}
