// SPDX-License-Identifier: MIT
//! Deterministic result versions (P7d), with the mock kernel: the same
//! definitions with the same inputs give the same versions in documents
//! built apart, read from a project file, and after the cache let a result
//! go, while the cache behaves as before.

use crate::document_tests::{Block, block, extrude, num};
use crate::features::SketchPlane;
use crate::ids::FeatureUid;
use crate::testing::MockKernel;
use crate::{Document, ModelError};

/// Block with a boss joined to it (Sketch2, Extrude2 = F4).
fn joined() -> (Block, FeatureUid) {
    let mut b = block();
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_rectangle(sketch, [10.0, 10.0], &num(20.0), &num(10.0))
        .unwrap()
        .region;
    let join = extrude(sketch, &[&region], 30.0, "join", &[b.body]);
    let boss = b.doc.add_feature(&join, None).unwrap().uid;
    (b, boss)
}

#[test]
fn documents_built_apart_have_the_same_versions() {
    let (a, _) = joined();
    let (b, _) = joined();
    let (outputs, bodies) = a.doc.versions();
    assert_eq!(outputs.len(), 4);
    assert!(outputs.iter().all(|(_, v)| v.is_some()));
    assert_eq!((outputs, bodies), b.doc.versions());
}

#[test]
fn a_project_read_again_has_the_same_versions() {
    // A parameter made before the others and deleted leaves a gap in the
    // ids, which reading the file closes: versions take parameters by
    // name, so they stay.
    let mut doc = Document::new(MockKernel::default());
    doc.add_parameter("spare", 5.0, "").unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(60.0), &num(40.0))
        .unwrap()
        .region;
    doc.add_feature(&extrude(sketch, &[&region], 20.0, "new_body", &[]), None)
        .unwrap();
    doc.delete_parameter("spare").unwrap();
    let before = doc.versions();
    assert_eq!(before, block().doc.versions());
    let mut read = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    read.recompute();
    assert_eq!(read.stats().evaluated.len(), 2);
    assert_eq!(read.versions(), before);
}

#[test]
fn inputs_change_versions_and_bring_them_back() {
    let (mut b, boss) = joined();
    let (outputs, bodies) = b.doc.versions();
    let version_of = |outputs: &[(FeatureUid, Option<u128>)], uid| {
        outputs.iter().find(|(u, _)| *u == uid).unwrap().1.unwrap()
    };
    b.doc.set_parameter("d3", 25.0).unwrap();
    let (changed, changed_bodies) = b.doc.versions();
    // The sketches read nothing that changed; the extrusions did.
    assert_eq!(
        version_of(&changed, b.sketch),
        version_of(&outputs, b.sketch)
    );
    assert_ne!(
        version_of(&changed, b.extrude),
        version_of(&outputs, b.extrude)
    );
    assert_ne!(version_of(&changed, boss), version_of(&outputs, boss));
    assert_ne!(changed_bodies, bodies);
    // Eight more values push d3 = 20 out of the cache; evaluated again,
    // it has its versions back.
    for value in 30..38 {
        b.doc.set_parameter("d3", f64::from(value)).unwrap();
    }
    b.doc.set_parameter("d3", 20.0).unwrap();
    assert_eq!(b.doc.stats().evaluated, vec![b.extrude, boss]);
    assert_eq!(b.doc.versions(), (outputs, bodies));
}

#[test]
fn a_result_from_the_cache_keeps_its_version() -> Result<(), ModelError> {
    let (mut b, _) = joined();
    let before = b.doc.versions();
    b.doc.set_parameter("d3", 25.0)?;
    b.doc.undo();
    assert!(b.doc.stats().evaluated.is_empty());
    assert_eq!(b.doc.versions(), before);
    Ok(())
}
