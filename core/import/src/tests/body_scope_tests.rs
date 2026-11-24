// SPDX-License-Identifier: MIT
//! A receiving document's solids and sheets stay outside history replay,
//! including matching geometry, fallback replacement and candidate workers.

use super::*;
use mitcad_model_api::DocumentApi;

#[test]
fn verified_replay_and_fallback_preserve_existing_bodies() {
    let (items, source) = block_history(Vec::new());
    let original_shape = source.states[1][0].clone();
    for matching in [true, false] {
        for fallback in [false, true] {
            for threads in [1, 2] {
                let mut doc = Document::new(TestKernel::default());
                let mut original = original_shape.clone();
                if !matching {
                    original.history.push_str("(other receiving body)");
                }
                let sheet = MockShape {
                    history: "existing,sheet".to_owned(),
                    faces: original_shape.faces[..1].to_vec(),
                    edges: Vec::new(),
                    bounds: None,
                    parts: Vec::new(),
                };
                doc.add_base_feature(BaseInput::new(vec![
                    ImportBody {
                        name: Some("Existing solid".to_owned()),
                        color: None,
                        shape: original.clone(),
                        face_colors: Vec::new(),
                    },
                    ImportBody {
                        name: Some("Existing sheet".to_owned()),
                        color: None,
                        shape: sheet.clone(),
                        face_colors: Vec::new(),
                    },
                ]))
                .unwrap();
                let old: Vec<_> = doc.bodies().iter().map(|b| b.uid).collect();
                let original = doc.body_shape(old[0]).unwrap().clone();
                let sheet = doc.body_shape(old[1]).unwrap().clone();
                doc.set_body_material(old[0], Some("aluminum")).unwrap();
                doc.set_body_visible(old[1], false).unwrap();
                let attributes: Vec<_> = old.iter().map(|&uid| doc.body_attributes(uid)).collect();
                let versions: Vec<_> = old.iter().map(|&uid| doc.body_version(uid)).collect();
                let before: Value =
                    serde_json::from_str(&doc.query(r#"{"query":"bodies"}"#).unwrap()).unwrap();
                let depth = doc.undo_depth();
                let mut replay = items.clone();
                let mut geometry = TestGeometry {
                    states: source.states.clone(),
                    items: source.items.clone(),
                    ..TestGeometry::default()
                };
                if fallback {
                    replay[1]["objectType"] = json!("UnsupportedFeature");
                    replay[1]["detail"] = json!({});
                }
                let report = import_design(
                    &mut doc,
                    &timeline_dump(replay),
                    &mut geometry,
                    &Options {
                        threads,
                        undo_label: Some("Import scoped design".to_owned()),
                        ..Options::default()
                    },
                );
                let note = report.text();
                assert_eq!(
                    doc.bodies().len(),
                    3,
                    "matching={matching}, fallback={fallback}, threads={threads}: {note}"
                );
                assert_eq!(doc.body_shape(old[0]), Some(&original), "{note}");
                assert_eq!(doc.body_shape(old[1]), Some(&sheet), "{note}");
                for (i, &uid) in old.iter().enumerate() {
                    assert_eq!(doc.body_attributes(uid), attributes[i], "{note}");
                    assert_eq!(doc.body_version(uid), versions[i], "{note}");
                }
                assert!(report.history.reached_end, "{note}");
                assert!(report.extra_bodies.is_empty(), "{note}");
                assert_eq!(report.bodies.len(), 1, "{note}");
                assert!(
                    report
                        .bodies
                        .iter()
                        .all(|b| b.uid.is_some_and(|uid| !old.contains(&uid))),
                    "{note}"
                );
                if fallback {
                    assert_eq!(report.items[1].outcome, Outcome::Fallback, "{note}");
                    // The stored base has different mock provenance from a
                    // replayed extrusion, so the next join can also fall
                    // back. Check its final geometry below instead of
                    // requiring a verified parametric continuation.
                } else {
                    assert_eq!(report.items[1].verified, Some(true), "{note}");
                    assert_eq!(report.items[2].verified, Some(true), "{note}");
                }
                let imported = doc
                    .bodies()
                    .into_iter()
                    .find(|body| !old.contains(&body.uid))
                    .expect("the replay's new body");
                let actual = Sig::of(doc.kernel(), imported.shape).expect("imported solid");
                let expected = Sig::of(doc.kernel(), &source.states[2][0]).expect("stored solid");
                assert!(
                    actual.same(&expected),
                    "the final imported geometry: {note}"
                );
                assert_eq!(doc.undo_depth(), depth + 1);
                doc.undo().unwrap();
                let restored: Value =
                    serde_json::from_str(&doc.query(r#"{"query":"bodies"}"#).unwrap()).unwrap();
                assert_eq!(restored, before);
                doc.redo().unwrap();
                assert_eq!(doc.bodies().len(), 3);
                assert_eq!(doc.body_shape(old[0]), Some(&original));
                assert_eq!(doc.body_shape(old[1]), Some(&sheet));
                for (i, &uid) in old.iter().enumerate() {
                    assert_eq!(doc.body_attributes(uid), attributes[i]);
                }
            }
        }
    }
}

#[test]
fn scope_tracks_document_identity_and_worker_copies_and_restores_after_drop() {
    let mut first = Document::new(TestKernel::default());
    let mut second = Document::new(TestKernel::default());
    let (_, source) = block_history(Vec::new());
    let shape = source.states[1][0].clone();
    for doc in [&mut first, &mut second] {
        doc.add_base_feature(BaseInput::new(vec![ImportBody {
            name: None,
            color: None,
            shape: shape.clone(),
            face_colors: Vec::new(),
        }]))
        .unwrap();
    }
    let uid = first.bodies()[0].uid;
    let scope = body_scope::Scope::capture(&first);
    {
        let _scope = scope.enter(&first);
        assert!(body_scope::bodies(&first).is_empty());
        assert!(body_scope::shape(&first, uid).is_none());
        assert_eq!(body_scope::bodies(&second).len(), 1); // the same UID is a different document
        {
            let _nested = body_scope::Scope::capture(&second).enter(&second);
            assert!(body_scope::bodies(&first).is_empty());
            assert!(body_scope::bodies(&second).is_empty());
        }
        assert_eq!(body_scope::bodies(&second).len(), 1);
        let fork = first.fork().unwrap();
        std::thread::spawn(move || {
            let _scope = scope.enter(&fork);
            assert!(body_scope::bodies(&fork).is_empty());
            assert!(body_scope::shape(&fork, uid).is_none());
        })
        .join()
        .unwrap();
    }
    assert_eq!(body_scope::bodies(&first).len(), 1);
    assert!(body_scope::shape(&first, uid).is_some());
}

#[test]
fn an_explicit_candidate_cannot_remove_a_receivers_body() {
    let (_, source) = block_history(Vec::new());
    let shape = source.states[1][0].clone();
    let mut doc = Document::new(TestKernel::default());
    doc.add_base_feature(BaseInput::new(vec![ImportBody {
        name: None,
        color: None,
        shape: shape.clone(),
        face_colors: Vec::new(),
    }]))
    .unwrap();
    let uid = doc.bodies()[0].uid;
    let shape = doc.body_shape(uid).unwrap().clone();
    let version = doc.body_version(uid);
    let depth = doc.undo_depth();
    let scope = body_scope::Scope::capture(&doc);
    let _scope = scope.enter(&doc);
    let adding = Adding {
        component: ComponentUid::ROOT,
        monitor: None,
        progress: None,
        memory: false,
    };
    let result = mitcad_import_replay::internal::add(
        &mut doc,
        &json!({"type":"remove_body","bodies":[uid]}),
        None,
        &adding,
    );
    assert!(result.unwrap_err().contains("outside this import"));
    assert_eq!(doc.undo_depth(), depth);
    assert_eq!(doc.body_version(uid), version);
    assert_eq!(doc.body_shape(uid), Some(&shape));
}

#[test]
fn an_initial_join_does_not_use_receiving_bodies_as_targets() {
    let (mut items, source) = block_history(Vec::new());
    items.truncate(2);
    items[1]["detail"]["operation"] = json!("JoinFeatureOperation");
    let shape = source.states[1][0].clone();
    let mut geometry = TestGeometry {
        states: source.states[..2].to_vec(),
        items: HashMap::from([(1, 1)]),
        ..TestGeometry::default()
    };
    let mut doc = Document::new(TestKernel::default());
    doc.add_base_feature(BaseInput::new(vec![ImportBody {
        name: None,
        color: None,
        shape: shape.clone(),
        face_colors: Vec::new(),
    }]))
    .unwrap();
    let uid = doc.bodies()[0].uid;
    let shape = doc.body_shape(uid).unwrap().clone();
    let version = doc.body_version(uid);
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    assert_eq!(report.items[1].verified, Some(true), "{}", report.text());
    assert_eq!(doc.bodies().len(), 2, "{}", report.text());
    assert_eq!(doc.body_version(uid), version);
    assert_eq!(doc.body_shape(uid), Some(&shape));
}
