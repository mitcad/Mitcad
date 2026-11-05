// SPDX-License-Identifier: MIT
//! Envelope (mitcad#1271) on the mock kernel, whose meshes are the bodies'
//! boxes: the definition and its parameter, a new body of planar faces
//! named after the feature with its source's appearance, the source only
//! read, a recompute that follows the source, the bodies of an
//! occurrence's component, listed bodies, and the refusals (no bodies, a
//! detail size of zero, a body listed twice). The OCCT kernel's envelope
//! is checked by `cli.envelope` (`tools/cli/tests/envelope.json`).

use serde_json::json;

use super::{EnvelopeDef, MIN_DEFAULT_DETAIL, default_detail};
use crate::features::{FeatureDef, SketchPlane};
use crate::ids::{BodyUid, ComponentUid};
use crate::kernel::BoundingBox;
use crate::test_support::{block, def, extrude, num};
use crate::testing::{MockKernel, MockShape};
use crate::transform::Transform;
use crate::{Document, FeatureStatus};

fn shape(doc: &Document<MockKernel>, body: BodyUid) -> MockShape {
    doc.body_shape(body)
        .unwrap_or_else(|| panic!("no body {body}"))
        .clone()
}

/// A second block, 20 × 10 × 50 at x = 100, as a new body.
fn second_block(doc: &mut Document<MockKernel>) -> BodyUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [100.0, 0.0], &num(20.0), &num(10.0))
        .unwrap()
        .region;
    let made = doc
        .add_feature(&extrude(sketch, &[&region], 50.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    BodyUid::new(made, 0)
}

#[test]
fn the_definition_reads_and_writes_its_fields() {
    let read: FeatureDef<f64> = serde_json::from_value(
        json!({"type": "envelope", "occurrence": "O1/O4", "bodies": ["F2.b0"], "detail": 20}),
    )
    .unwrap();
    let FeatureDef::Envelope(envelope) = &read else {
        panic!("{read:?}");
    };
    assert_eq!(envelope.occurrence.to_string(), "O1/O4");
    assert_eq!(envelope.bodies, vec!["F2.b0".parse::<BodyUid>().unwrap()]);
    assert_eq!(envelope.detail, Some(20.0));
    assert_eq!(
        serde_json::to_value(&read).unwrap(),
        json!({"type": "envelope", "occurrence": "O1/O4", "bodies": ["F2.b0"], "detail": 20.0})
    );
    // Everything left out: the feature's component, the default size.
    let bare: EnvelopeDef<f64> = serde_json::from_value(json!({})).unwrap();
    assert!(bare.occurrence.is_empty() && bare.bodies.is_empty() && bare.detail.is_none());
    assert!(serde_json::from_value::<EnvelopeDef<f64>>(json!({"margin": 1})).is_err());
}

#[test]
fn the_default_detail_size_is_a_share_of_the_diagonal_at_least_a_few_millimetres() {
    let small = BoundingBox {
        min: [0.0; 3],
        max: [60.0, 40.0, 20.0],
    };
    assert_eq!(default_detail(&small), MIN_DEFAULT_DETAIL);
    let large = BoundingBox {
        min: [0.0; 3],
        max: [3000.0, 4000.0, 0.0],
    };
    assert!((default_detail(&large) - 50.0).abs() < 1e-9);
}

/// Around every body of the component: a new body of planar faces named
/// after the feature; the bodies stay as they were, and the envelope
/// follows their edits.
#[test]
fn an_envelope_wraps_the_component_and_follows_it() {
    let mut b = block();
    second_block(&mut b.doc);
    let before = shape(&b.doc, b.body);
    let added = b
        .doc
        .add_feature(&def(json!({"type": "envelope", "detail": 10})), None)
        .unwrap();
    assert_eq!(added.name, "Envelope1");
    assert_eq!(added.parameters.len(), 1, "{:?}", added.parameters);
    assert_eq!(b.doc.status(added.uid), Some(&FeatureStatus::Ok));
    let body = BodyUid::new(added.uid, 0);
    let envelope = shape(&b.doc, body);
    assert!(
        envelope.history.contains("boundary"),
        "{}",
        envelope.history
    );
    // The two blocks are 40 mm apart, further than the detail size: two
    // boxes of six faces.
    assert_eq!(envelope.faces.len(), 12, "{:?}", envelope.faces);
    assert_eq!(b.doc.body_name(body), "Envelope1");
    assert_eq!(shape(&b.doc, b.body), before);
    // Tessellated within a quarter of the detail size.
    let tolerances = b.doc.kernel().mesh_tolerances.borrow().clone();
    assert!(
        tolerances.iter().any(|t| (t.deviation - 2.5).abs() < 1e-12),
        "{tolerances:?}"
    );
    let info = b.doc.infos(added.uid).join("; ");
    assert!(info.contains("detail size 10 mm: 12 faces"), "{info}");
    // The first block made 95 wide: the 5 mm gap closes, one body of
    // other faces.
    b.doc.set_parameter("d1", 95.0).unwrap();
    assert_eq!(b.doc.status(added.uid), Some(&FeatureStatus::Ok));
    let joined = shape(&b.doc, body).faces.len();
    assert!(joined != 12 && joined > 6, "{joined}");
    // Back to 60 wide and a detail size of 50 mm: the 40 mm gap closes
    // too.
    b.doc.set_parameter("d1", 60.0).unwrap();
    assert_eq!(shape(&b.doc, body).faces.len(), 12);
    b.doc.set_parameter(&added.parameters[0], 50.0).unwrap();
    assert_eq!(b.doc.status(added.uid), Some(&FeatureStatus::Ok));
    assert_ne!(shape(&b.doc, body).faces.len(), 12);
}

/// The listed bodies only (one block: six faces); a body listed twice and
/// a detail size of zero are refused.
#[test]
fn an_envelope_wraps_the_bodies_it_lists() {
    let mut b = block();
    let second = second_block(&mut b.doc);
    let one = b
        .doc
        .add_feature(
            &def(json!({"type": "envelope", "bodies": [second.to_string()], "detail": 10})),
            None,
        )
        .unwrap();
    assert_eq!(b.doc.status(one.uid), Some(&FeatureStatus::Ok));
    assert_eq!(shape(&b.doc, BodyUid::new(one.uid, 0)).faces.len(), 6);
    let error = b
        .doc
        .add_feature(
            &def(json!({"type": "envelope", "bodies": [second.to_string(), second.to_string()]})),
            None,
        )
        .unwrap_err();
    assert!(error.to_string().contains("more than once"), "{error}");
    let error = b
        .doc
        .add_feature(&def(json!({"type": "envelope", "detail": 0})), None)
        .unwrap_err();
    assert!(error.to_string().contains("more than 0"), "{error}");
}

/// The envelope's body takes the appearance of the first source body
/// that has one.
#[test]
fn an_envelope_takes_its_sources_appearance() {
    let mut b = block();
    b.doc
        .set_body_appearance(b.body, Some("paint_red"))
        .unwrap();
    let added = b
        .doc
        .add_feature(&def(json!({"type": "envelope", "detail": 10})), None)
        .unwrap();
    let body = BodyUid::new(added.uid, 0);
    assert_eq!(
        b.doc.body_attributes(body).appearance.as_deref(),
        Some("paint_red")
    );
    // One undo step takes both away.
    assert!(b.doc.undo().is_some());
    assert!(b.doc.body_shape(body).is_none());
    assert_eq!(b.doc.body_attributes(body).appearance, None);
}

/// With an occurrence, the bodies of its component (and of what is
/// placed in it) as the occurrence places them, the envelope in the
/// feature's own component.
#[test]
fn an_envelope_wraps_an_occurrences_bodies() {
    let mut doc = Document::new(MockKernel::default());
    let (machine, occurrence) = doc
        .create_component(
            Some("Machine"),
            Transform::translation([500.0, 0.0, 0.0]),
            true,
        )
        .unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(60.0), &num(40.0))
        .unwrap()
        .region;
    let frame = doc
        .add_feature(&extrude(sketch, &[&region], 20.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    assert_eq!(doc.body_component(BodyUid::new(frame, 0)), Some(machine));
    doc.activate_component(ComponentUid::ROOT).unwrap();
    // The root has no bodies of its own: wrapping it wraps what is placed
    // in it.
    let all = doc
        .add_feature(&def(json!({"type": "envelope", "detail": 10})), None)
        .unwrap();
    assert_eq!(doc.status(all.uid), Some(&FeatureStatus::Ok));
    let of_machine = doc
        .add_feature(
            &def(json!({"type": "envelope", "occurrence": occurrence.to_string(), "detail": 10})),
            None,
        )
        .unwrap();
    assert_eq!(doc.status(of_machine.uid), Some(&FeatureStatus::Ok));
    let body = BodyUid::new(of_machine.uid, 0);
    assert_eq!(doc.body_component(body), Some(ComponentUid::ROOT));
    assert_eq!(shape(&doc, body).faces.len(), 6);
    // Both envelopes were built (where the occurrence places the body is
    // checked with the OCCT kernel, `cli.envelope`).
    let boundary_calls = doc.kernel().calls.borrow().get("boundary_body").copied();
    assert!(boundary_calls.is_some_and(|n| n >= 2), "{boundary_calls:?}");
    // A listed body of that component.
    let listed = doc
        .add_feature(
            &def(
                json!({"type": "envelope", "occurrence": occurrence.to_string(),
                        "bodies": [BodyUid::new(frame, 0).to_string()], "detail": 10}),
            ),
            None,
        )
        .unwrap();
    assert_eq!(doc.status(listed.uid), Some(&FeatureStatus::Ok));
    // An occurrence path that does not exist fails the feature.
    let missing = doc
        .add_feature(&def(json!({"type": "envelope", "occurrence": "O99"})), None)
        .unwrap();
    assert!(matches!(
        doc.status(missing.uid),
        Some(FeatureStatus::Failed(m)) if m.contains("does not exist")
    ));
}

/// With no bodies there is nothing to wrap.
#[test]
fn an_envelope_needs_bodies() {
    let mut doc = Document::new(MockKernel::default());
    let added = doc
        .add_feature(&def(json!({"type": "envelope"})), None)
        .unwrap();
    assert!(matches!(
        doc.status(added.uid),
        Some(FeatureStatus::Failed(m)) if m.contains("no bodies to wrap")
    ));
}
