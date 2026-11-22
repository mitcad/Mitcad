// SPDX-License-Identifier: MIT
//! Scoped Cut tools repeat locally without losing their target selection.

use serde_json::{Value, json};

use crate::features::SketchPlane;
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::notice::{NoticePlace, user_text_leaks};
use crate::test_support::{def, num};
use crate::testing::MockKernel;
use crate::{Document, EdgeName, FaceName, FeatureStatus, Transform};

fn body(doc: &mut Document<MockKernel>) -> BodyUid {
    let uid = doc
        .add_feature(
            &def(json!({"type":"box", "plane":"xy",
        "length":50, "width":50, "height":20, "operation":"new_body"})),
            None,
        )
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

fn scope() -> Value {
    json!({"context":{"component":"C0", "occurrence":""},
        "targets":{"type":"automatic", "excluded":[], "excluded_bodies":[]}})
}

fn cut(doc: &mut Document<MockKernel>, scope: Option<Value>) -> FeatureUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_circle(sketch, [15.0, 25.0], &num(10.0))
        .unwrap()
        .region;
    let mut value = json!({"type":"extrude", "profiles":[{"sketch":sketch,"region":region}],
        "extent":{"type":"distance","distance":6},"operation":"cut"});
    if let Some(scope) = scope {
        value["cut_scope"] = scope;
    }
    doc.add_feature(&def(value), None).unwrap().uid
}

fn mirror(cut: FeatureUid, compute: &str) -> Value {
    json!({"type":"mirror", "objects":{"type":"features","features":[cut]},
        "plane":{"type":"fixed","origin":[25,0,0],"normal":[1,0,0]},"compute":compute})
}

fn history(doc: &Document<MockKernel>, body: BodyUid) -> String {
    doc.body_shape(body).unwrap().history.clone()
}

/// A body in a component "Other" of its own at `x` along X (the root's
/// body spans 0 to 50), the root active again.
fn other_at(doc: &mut Document<MockKernel>, x: f64) -> BodyUid {
    doc.create_component(Some("Other"), Transform::translation([x, 0.0, 0.0]), true)
        .unwrap();
    let other = body(doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    other
}

/// The mirror or pattern `uid` is refused in words (mitcad#2126): `words`,
/// naming the component Other, without ids, with Other's occurrence as
/// its place.
fn refused(doc: &Document<MockKernel>, uid: FeatureUid, words: &str) {
    let Some(FeatureStatus::Failed(error)) = doc.status(uid) else {
        panic!("{:?}", doc.status(uid));
    };
    assert!(error.message.contains(words), "{error:?}");
    assert!(error.message.contains("Other"), "{error:?}");
    assert_eq!(user_text_leaks(&error.message), None, "{error:?}");
    assert!(
        error.places.contains(&NoticePlace::occurrence("O1")),
        "{error:?}"
    );
}

fn failed_scope(doc: &Document<MockKernel>, uid: FeatureUid) {
    refused(doc, uid, "it also cuts Other");
}

fn pattern(cut: FeatureUid) -> Value {
    json!({"type":"rectangular_pattern",
        "objects":{"type":"features","features":[cut]},
        "direction1":{"axis":{"type":"origin","axis":"x"}, "quantity":2,"distance":20},
        "distance_type":"spacing"})
}

#[test]
fn scoped_cut_mirror_preview_edit_undo_and_reopen_match_legacy() {
    for scoped in [false, true] {
        for compute in ["adjust", "identical"] {
            let mut doc = Document::new(MockKernel::default());
            let target = body(&mut doc);
            let original = cut(&mut doc, scoped.then(scope));
            let before = history(&doc, target);
            let definition = def(mirror(original, compute));
            assert_eq!(
                doc.preview_add(&definition, None).unwrap().status,
                FeatureStatus::Ok
            );
            assert_eq!(
                history(&doc, target),
                before,
                "preview must not mutate the cut"
            );
            let repeated = doc.add_feature(&definition, None).unwrap().uid;
            assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok));
            assert!(history(&doc, target).starts_with("cut(cut("));
            assert_eq!(doc.body_component(target), Some(ComponentUid::ROOT));
            let mirrored = history(&doc, target);
            doc.undo();
            assert_eq!(history(&doc, target), before);
            doc.redo();
            assert_eq!(history(&doc, target), mirrored);
            // d1 is the box's length: an upstream dimension still invalidates
            // and recomputes both the source cut and its reflected tool.
            doc.set_parameter("d1", 55.0).unwrap();
            assert_eq!(doc.status(original), Some(&FeatureStatus::Ok));
            assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok));
            let saved = doc.to_json();
            let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
            reopened.recompute();
            assert_eq!(reopened.status(repeated), Some(&FeatureStatus::Ok));
            assert_eq!(history(&reopened, target), history(&doc, target));
            assert_eq!(reopened.body_component(target), Some(ComponentUid::ROOT));
            assert_eq!(reopened.to_json(), saved);
        }
    }
}

#[test]
fn scoped_cut_pattern_respects_explicit_component_and_body_exclusion() {
    let mut doc = Document::new(MockKernel::default());
    let target = body(&mut doc);
    let excluded = body(&mut doc);
    let untouched = history(&doc, excluded);
    let mut selection = scope();
    selection["targets"] = json!({"type":"explicit", "included":[selection["context"].clone()],
        "excluded_bodies":[{"component":"C0", "occurrence":"", "body":excluded}]});
    let original = cut(&mut doc, Some(selection));
    let repeated = doc
        .add_feature(
            &def(json!({"type":"rectangular_pattern",
        "objects":{"type":"features","features":[original]},
        "direction1":{"axis":{"type":"origin","axis":"x"}, "quantity":2,"distance":20},
        "distance_type":"spacing"})),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok));
    assert!(history(&doc, target).starts_with("cut(cut("));
    assert_eq!(history(&doc, excluded), untouched);
    assert_eq!(doc.body_component(target), Some(ComponentUid::ROOT));
    assert_eq!(doc.body_component(excluded), Some(ComponentUid::ROOT));
}

#[test]
fn scoped_cut_replay_empty_selection_never_becomes_all_bodies() {
    for targets in [
        json!({"type":"explicit","included":[]}),
        json!({"type":"automatic","excluded":[{"component":"C0","occurrence":""}]}),
        json!({"type":"automatic","excluded_bodies":[{"component":"C0","occurrence":"","body":"F1.b0"}]}),
    ] {
        let mut doc = Document::new(MockKernel::default());
        let target = body(&mut doc);
        let untouched = history(&doc, target);
        let mut selection = scope();
        selection["targets"] = targets;
        let original = cut(&mut doc, Some(selection));
        let repeated = doc
            .add_feature(&def(mirror(original, "adjust")), None)
            .unwrap()
            .uid;
        assert!(matches!(
            doc.status(repeated),
            Some(FeatureStatus::Failed(_))
        ));
        assert_eq!(history(&doc, target), untouched);
    }
}

#[test]
fn scoped_cut_replay_rejects_foreign_targets_but_keeps_deliberate_exclusions() {
    for excluded in [false, true] {
        let mut doc = Document::new(MockKernel::default());
        let local = body(&mut doc);
        doc.create_component(Some("Other"), Transform::IDENTITY, true)
            .unwrap();
        let foreign = body(&mut doc);
        doc.activate_component(ComponentUid::ROOT).unwrap();
        let mut selection = scope();
        if excluded {
            selection["targets"]["excluded"] = json!([{"component":"C1","occurrence":"O1"}]);
        }
        let original = cut(&mut doc, Some(selection));
        let foreign_before = history(&doc, foreign);
        let repeated = doc
            .add_feature(&def(mirror(original, "adjust")), None)
            .unwrap()
            .uid;
        if excluded {
            assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok));
            assert!(history(&doc, local).starts_with("cut(cut("));
        } else {
            failed_scope(&doc, repeated);
            // A source selection edit repairs the cached replay without
            // changing the mirror or its local tool geometry.
            doc.edit_feature(original, &def(json!({"type":"extrude",
                "profiles":[{"sketch":FeatureUid(original.0 - 1),"region":"r{c1}"}],
                "extent":{"type":"distance","distance":6},"operation":"cut",
                "cut_scope":{"context":{"component":"C0","occurrence":""},
                    "targets":{"type":"explicit","included":[{"component":"C0","occurrence":""}]}}
            }))).unwrap();
            assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok));
            doc.undo();
            failed_scope(&doc, repeated);
        }
        assert_eq!(history(&doc, foreign), foreign_before);
        assert_eq!(doc.body_component(foreign), Some(ComponentUid(1)));
    }
}

#[test]
fn scoped_cut_replay_refuses_explicit_foreign_scope_in_words_not_as_a_tool_error() {
    let mut doc = Document::new(MockKernel::default());
    body(&mut doc);
    other_at(&mut doc, 0.0);
    let mut selection = scope();
    selection["targets"] =
        json!({"type":"explicit","included":[{"component":"C1","occurrence":"O1"}]});
    let original = cut(&mut doc, Some(selection));
    // The scoped cut has a tool; the mirror is refused where it is
    // evaluated, naming the component the scope includes (mitcad#2126).
    let repeated = doc
        .add_feature(&def(mirror(original, "adjust")), None)
        .unwrap()
        .uid;
    failed_scope(&doc, repeated);
    let Some(FeatureStatus::Failed(error)) = doc.status(repeated) else {
        unreachable!()
    };
    assert!(!error.message.contains("no tool body"), "{error:?}");
}

#[test]
fn scoped_cut_replay_assembly_change_invalidates_cached_local_classification() {
    let mut doc = Document::new(MockKernel::default());
    let local = body(&mut doc);
    let original = cut(&mut doc, Some(scope()));
    let repeated = doc
        .add_feature(&def(mirror(original, "adjust")), None)
        .unwrap()
        .uid;
    assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok));
    doc.create_component(Some("Other"), Transform::IDENTITY, true)
        .unwrap();
    let foreign = body(&mut doc);
    // The new body is later than the mirror and must not affect its scope.
    assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok));
    // The same cached replay now sees a newly available eligible target.
    doc.move_feature(foreign.feature, 1).unwrap();
    failed_scope(&doc, repeated);
    assert_eq!(doc.body_component(local), Some(ComponentUid::ROOT));
    assert_eq!(doc.body_component(foreign), Some(ComponentUid(1)));
    doc.undo();
    assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok));
}

#[test]
fn scoped_cut_replay_keeps_own_shared_definition_and_skips_foreign_one() {
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(
        Some("Part"),
        Transform::translation([100.0, 0.0, 0.0]),
        true,
    )
    .unwrap();
    let target = body(&mut doc);
    let selection = json!({"context":{"component":"C1","occurrence":"O1"},
        "targets":{"type":"explicit","included":[{"component":"C1","occurrence":"O1"}]}});
    let original = cut(&mut doc, Some(selection));
    let repeated = doc
        .add_feature(&def(mirror(original, "adjust")), None)
        .unwrap()
        .uid;
    assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok));
    assert!(history(&doc, target).starts_with("cut(cut("));
    assert_eq!(doc.body_component(target), Some(ComponentUid(1)));
    let replayed = history(&doc, target);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.copy_occurrence(
        OccurrenceUid(1),
        Some(Transform::translation([200.0, 0.0, 0.0])),
    )
    .unwrap();
    // A cut inside Part edits Part's definition, which both occurrences
    // show (mitcad#769): a second occurrence refuses neither the cut nor
    // its mirrored replay, and the body keeps both cuts.
    assert_eq!(doc.status(original), Some(&FeatureStatus::Ok));
    assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok));
    assert_eq!(history(&doc, target), replayed);
    assert_eq!(doc.body_component(target), Some(ComponentUid(1)));
    doc.undo();
    assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok));

    // A cut from the root, whose tool also reaches one occurrence of a
    // shared Part, stays local on replay: Part's shared definition is
    // refused and left uncut, the root's own body is cut by both.
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(Some("Part"), Transform::IDENTITY, true)
        .unwrap();
    let shared = body(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let root = body(&mut doc);
    doc.copy_occurrence(
        OccurrenceUid(1),
        Some(Transform::translation([200.0, 0.0, 0.0])),
    )
    .unwrap();
    let untouched = history(&doc, shared);
    let original = cut(&mut doc, Some(scope()));
    let repeated = doc
        .add_feature(&def(mirror(original, "adjust")), None)
        .unwrap()
        .uid;
    assert_eq!(doc.status(original), Some(&FeatureStatus::Ok));
    assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok));
    assert!(history(&doc, root).starts_with("cut(cut("));
    assert_eq!(history(&doc, shared), untouched);
}

/// mitcad#2126: a cut in one component repeats beside another component's
/// body it does not reach; the copies cut only the own body.
#[test]
fn scoped_cut_repeats_beside_a_component_it_does_not_reach() {
    for repeat in ["adjust", "identical", "pattern"] {
        let mut doc = Document::new(MockKernel::default());
        let local = body(&mut doc);
        let other = other_at(&mut doc, 200.0);
        let untouched = history(&doc, other);
        let original = cut(&mut doc, Some(scope()));
        assert_eq!(doc.status(original), Some(&FeatureStatus::Ok));
        let definition = match repeat {
            "pattern" => pattern(original),
            compute => mirror(original, compute),
        };
        assert_eq!(
            doc.preview_add(&def(definition.clone()), None)
                .unwrap()
                .status,
            FeatureStatus::Ok,
            "{repeat}"
        );
        let repeated = doc.add_feature(&def(definition), None).unwrap().uid;
        assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok), "{repeat}");
        assert!(history(&doc, local).starts_with("cut(cut("), "{repeat}");
        assert_eq!(history(&doc, other), untouched, "{repeat}");
        assert_eq!(doc.body_component(other), Some(ComponentUid(1)));
        // Saved and opened again, the same.
        let mut reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
        reopened.recompute();
        assert_eq!(
            reopened.status(repeated),
            Some(&FeatureStatus::Ok),
            "{repeat}"
        );
        assert_eq!(history(&reopened, other), untouched, "{repeat}");
    }
}

/// mitcad#2126: a cut that also cuts another component is not repeated:
/// the refusal names the component, and its body keeps only the cut.
#[test]
fn scoped_cut_that_cuts_another_component_is_refused_in_words() {
    for (repeat, words) in [
        ("adjust", "Could not mirror "),
        ("identical", "Could not mirror "),
        ("pattern", "Could not repeat "),
    ] {
        let mut doc = Document::new(MockKernel::default());
        let local = body(&mut doc);
        let other = other_at(&mut doc, 0.0);
        let untouched = history(&doc, other);
        let original = cut(&mut doc, Some(scope()));
        assert_eq!(doc.status(original), Some(&FeatureStatus::Ok));
        let cut_once = history(&doc, other);
        assert_ne!(cut_once, untouched, "the cut reaches Other");
        let definition = match repeat {
            "pattern" => pattern(original),
            compute => mirror(original, compute),
        };
        let repeated = doc.add_feature(&def(definition), None).unwrap().uid;
        refused(&doc, repeated, words);
        failed_scope(&doc, repeated);
        assert_eq!(history(&doc, other), cut_once, "{repeat}");
        assert!(!history(&doc, local).starts_with("cut(cut("), "{repeat}");
    }
}

/// mitcad#2126: a cut that reaches only its own component, but whose copy
/// would reach another component's body, is refused; that body is left as
/// it was, never cut by the copy or silently left out.
#[test]
fn scoped_cut_whose_copy_reaches_another_component_is_refused() {
    for (repeat, words) in [
        ("adjust", "Could not mirror "),
        ("identical", "Could not mirror "),
        ("pattern", "Could not repeat "),
    ] {
        let mut doc = Document::new(MockKernel::default());
        body(&mut doc);
        // Other spans 40 to 90: the cut (5 to 25) misses it, the copy
        // (25 to 45, mirrored at 25 or moved by 20) reaches it.
        let other = other_at(&mut doc, 40.0);
        let untouched = history(&doc, other);
        let original = cut(&mut doc, Some(scope()));
        assert_eq!(doc.status(original), Some(&FeatureStatus::Ok));
        assert_eq!(history(&doc, other), untouched, "the cut misses Other");
        let definition = match repeat {
            "pattern" => pattern(original),
            compute => mirror(original, compute),
        };
        let repeated = doc.add_feature(&def(definition), None).unwrap().uid;
        refused(&doc, repeated, words);
        refused(&doc, repeated, "would also cut Other");
        assert_eq!(history(&doc, other), untouched, "{repeat}");

        // Other left out of the cut's scope: the copy cuts only the own
        // body, as the cut does.
        let mut selection = scope();
        selection["targets"]["excluded"] = json!([{"component":"C1","occurrence":"O1"}]);
        doc.edit_feature(
            original,
            &def(json!({"type":"extrude",
            "profiles":[{"sketch":FeatureUid(original.0 - 1),"region":"r{c1}"}],
            "extent":{"type":"distance","distance":6},"operation":"cut",
            "cut_scope":selection})),
        )
        .unwrap();
        assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok), "{repeat}");
        assert_eq!(history(&doc, other), untouched, "{repeat}");
    }
}

/// A scoped cut of the own body (as [`cut`]) and a fillet on the rim of
/// its pocket.
fn rounded_cut(doc: &mut Document<MockKernel>, body: BodyUid) -> (FeatureUid, FeatureUid) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_circle(sketch, [15.0, 25.0], &num(10.0))
        .unwrap()
        .region;
    let cut = doc
        .add_feature(
            &def(
                json!({"type":"extrude", "profiles":[{"sketch":sketch,"region":region}],
                "extent":{"type":"distance","distance":6},"operation":"cut",
                "cut_scope":scope()}),
            ),
            None,
        )
        .unwrap()
        .uid;
    let side: FaceName = format!("{cut}:side(c1)").parse().unwrap();
    let rim = EdgeName::new(side, FaceName::end(cut, region));
    let fillet = doc
        .add_feature(
            &def(
                json!({"type":"fillet", "body":body.to_string(), "edges":[rim],
                "radius":1}),
            ),
            None,
        )
        .unwrap()
        .uid;
    (cut, fillet)
}

/// mitcad#2171: a pattern with Compute Optimized of a scoped cut and the
/// fillet on its rim copies the faces the features made. Each copy of the
/// cut is checked against the other components' bodies first, as with the
/// other compute options: a copy that would reach one refuses the pattern
/// and that body is left as it was; with the body out of reach the faces
/// are copied, and the body is still as it was.
#[test]
fn optimized_pattern_of_a_rounded_scoped_cut_checks_each_copy() {
    // Other spans 40 to 90, where the copy (25 to 45) reaches it, or 200
    // to 250, out of reach.
    for (x, reaches) in [(40.0, true), (200.0, false)] {
        let mut doc = Document::new(MockKernel::default());
        let local = body(&mut doc);
        let other = other_at(&mut doc, x);
        let untouched = history(&doc, other);
        let (original, fillet) = rounded_cut(&mut doc, local);
        assert_eq!(doc.status(original), Some(&FeatureStatus::Ok), "{x}");
        assert_eq!(doc.status(fillet), Some(&FeatureStatus::Ok), "{x}");
        assert_eq!(history(&doc, other), untouched, "the cut misses Other");
        let rounded = history(&doc, local);
        let fillets = doc.kernel().count("fillet");
        let mut definition = pattern(original);
        definition["objects"]["features"] = json!([original, fillet]);
        definition["compute"] = json!("optimized");
        let repeated = doc.add_feature(&def(definition), None).unwrap().uid;
        if reaches {
            refused(&doc, repeated, "Could not repeat ");
            refused(&doc, repeated, "would also cut Other");
            // Refused before any face is copied.
            assert_eq!(doc.kernel().count("feature_changes"), 0);
            assert_eq!(history(&doc, local), rounded);
        } else {
            assert_eq!(doc.status(repeated), Some(&FeatureStatus::Ok));
            // The faces of the cut and its fillet copied once; the fillet
            // is not worked out again on the copy.
            assert_eq!(doc.kernel().count("feature_changes"), 1);
            assert_eq!(doc.kernel().count("fillet"), fillets);
            assert_ne!(history(&doc, local), rounded);
        }
        assert_eq!(history(&doc, other), untouched, "{x}");
        assert_eq!(doc.body_component(other), Some(ComponentUid(1)));
    }
}
