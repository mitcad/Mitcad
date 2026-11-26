// SPDX-License-Identifier: MIT
//! Copies of a component keep the light bulbs of its sketches
//! (mitcad#2449): the `.f3d` import fills the components standing for
//! components of other documents with copies of those documents' designs,
//! and the copies showed every sketch that a document hides and no feature
//! uses.

use mitcad_model::Document;
use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::ids::{ComponentUid, FeatureUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;

/// A part with a block (Sketch1, used by Extrude1), a sketch no feature
/// uses (Sketch2) and another one (Sketch3), all in the root.
fn part() -> (Document<MockKernel>, [FeatureUid; 3]) {
    let mut doc = Document::new(MockKernel::default());
    let used = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(used, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    doc.add_feature(&extrude(used, &[&region], 5.0, "new_body", &[]), None)
        .unwrap();
    let hidden = doc.add_sketch(SketchPlane::Xz).unwrap().uid;
    let shown = doc.add_sketch(SketchPlane::Yz).unwrap().uid;
    doc.recompute();
    (doc, [used, hidden, shown])
}

#[test]
fn a_filled_copy_shows_and_hides_the_sketches_as_the_source_does() {
    let (mut source, [used, hidden, shown]) = part();
    // The file's light bulbs: the used sketch shown, an unused one hidden;
    // the other unused one follows the default (shown).
    source.set_feature_visible(used, true).unwrap();
    source.set_feature_visible(hidden, false).unwrap();
    assert_eq!(source.feature_shown(used), Some(true));
    assert_eq!(source.feature_shown(hidden), Some(false));
    assert_eq!(source.feature_shown(shown), Some(true));

    let mut doc = Document::new(MockKernel::default());
    let (into, _) = doc
        .create_component(Some("Inserted"), Transform::IDENTITY, false)
        .unwrap();
    let copy = doc
        .fill_component_copy(&source, into, Some("Part"))
        .unwrap();
    let of = |uid: FeatureUid| copy.features[&uid];
    assert_eq!(doc.feature_shown(of(used)), Some(true));
    assert_eq!(doc.feature_shown(of(hidden)), Some(false));
    assert_eq!(doc.feature_shown(of(shown)), Some(true));
    // Kept only where they differ from the default, as the source keeps
    // them: the unchanged sketch follows it.
    assert_eq!(doc.feature_visible(of(used)), Some(true));
    assert_eq!(doc.feature_visible(of(hidden)), Some(false));
    assert_eq!(doc.feature_visible(of(shown)), None);
    // The copy's undo takes them away with the copy.
    assert!(doc.undo().is_some());
    assert_eq!(doc.feature_shown(of(hidden)), None);
}

#[test]
fn a_component_copied_into_the_design_keeps_its_hidden_sketches() {
    let (mut source, [_, hidden, _]) = part();
    source.set_feature_visible(hidden, false).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let (component, _) = doc
        .add_component_copy(
            &source,
            Some("Part"),
            ComponentUid::ROOT,
            Transform::IDENTITY,
        )
        .unwrap();
    let sketch = |n: usize| {
        doc.features()
            .filter(|f| f.component == component && matches!(f.def, FeatureDef::Sketch(_)))
            .nth(n)
            .map(|f| f.uid)
            .unwrap()
    };
    let (copy_used, copy_hidden, copy_shown) = (sketch(0), sketch(1), sketch(2));
    // The used one hidden by default, the hidden one hidden by its light
    // bulb, the other one shown.
    assert_eq!(doc.feature_shown(copy_used), Some(false));
    assert_eq!(doc.feature_shown(copy_hidden), Some(false));
    assert_eq!(doc.feature_shown(copy_shown), Some(true));
}
