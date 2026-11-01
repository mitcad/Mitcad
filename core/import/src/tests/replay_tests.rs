// SPDX-License-Identifier: MIT
//! The replay's basics on the block design: parameters, sketches, extrusions, the geometric tolerance, undo and forked documents.

use super::*;
use mitcad_model_api::DocumentApi;

#[test]
fn the_imports_joins_fuse_within_its_geometric_tolerance() {
    // A join on the block: the document keeps the import's geometric
    // tolerance as its join tolerance (mitcad#298), and the join is made
    // within it.
    let mut source = serde_json::to_value(block_dump()).unwrap();
    source["timeline"]["items"]
        .as_array_mut()
        .unwrap()
        .push(extrude(2, 0, "JoinFeatureOperation", "d2"));
    let mut doc = Document::new(MockKernel::default());
    let options = Options {
        geometric_tolerance: 0.02,
        undo_label: Some("Import".to_owned()),
        ..Options::default()
    };
    let report = import_design(&mut doc, &dump(source), &mut NoGeometry, &options);
    assert_eq!(report.items[2].outcome, Outcome::Parametric);
    assert_eq!(doc.join_tolerance(), Some(0.02));
    assert!(doc.kernel().count("join_within") > 0);
    assert!(
        doc.kernel()
            .specs
            .borrow()
            .iter()
            .all(|s| !s.starts_with("join within") || s == "join within 0.02")
    );
    // Part of the import's one undo step.
    doc.undo();
    assert_eq!(doc.join_tolerance(), None);
    assert!(doc.features().next().is_none());
}

#[test]
fn the_imports_cuts_are_retried_within_its_geometric_tolerance() {
    // A cut into the block goes through the kernel's cut within the
    // document's join tolerance (mitcad#235), which retries a cut that
    // fails exactly with a fuzzy value up to it.
    let mut source = serde_json::to_value(block_dump()).unwrap();
    source["timeline"]["items"]
        .as_array_mut()
        .unwrap()
        .push(extrude(2, 0, "CutFeatureOperation", "d2"));
    let mut doc = Document::new(MockKernel::default());
    let options = Options {
        geometric_tolerance: 0.02,
        ..Options::default()
    };
    import_design(&mut doc, &dump(source), &mut NoGeometry, &options);
    assert!(doc.kernel().count("cut_within") > 0);
    assert!(
        doc.kernel()
            .specs
            .borrow()
            .iter()
            .any(|s| s == "cut within 0.02")
    );
}

#[test]
fn an_extrusion_of_texts_the_file_no_longer_has_says_so() {
    // The text profiles name texts the design no longer has (mitcad#332):
    // no region of the sketch stands in for them.
    let mut source = serde_json::to_value(block_dump()).unwrap();
    source["timeline"]["items"][1]["detail"]["_f3d_missing_texts"] = json!([2696, 2741]);
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump(source));
    assert_ne!(report.items[1].outcome, Outcome::Parametric);
    let note = report.items[1].note.clone().unwrap_or_default();
    assert!(
        note.contains(
            "its profile is 2 sketch texts the file no longer has (deleted from Sketch1)"
        ),
        "{note}"
    );
}

#[test]
fn input_provenance_keeps_unknowns_and_reports_the_last_sketch_default() {
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &block_dump());
    // A successful first candidate alone is no proof that every input
    // was read. The sketch is not a modelling candidate and is not counted.
    assert_eq!(report.items[0].input_provenance, None);
    assert_eq!(
        report.items[1].input_provenance,
        Some(InputProvenance::Unknown)
    );
    assert_eq!(
        report.input_counts(),
        BTreeMap::from([(InputProvenance::Unknown, 1)])
    );

    let mut source = serde_json::to_value(block_dump()).unwrap();
    source["timeline"]["items"][1]["detail"]
        .as_object_mut()
        .unwrap()
        .remove("profile");
    let mut doc = Document::new(MockKernel::default());
    let guessed = import(&mut doc, &dump(source));
    // Provenance does not change the established no-history replay rule.
    assert_eq!(guessed.items[1].outcome, Outcome::Parametric);
    assert_eq!(guessed.items[1].features, report.items[1].features);
    assert_eq!(guessed.items[1].verified, None);
    assert_eq!(
        guessed.items[1].input_provenance,
        Some(InputProvenance::Inferred)
    );
    assert_eq!(
        guessed.input_counts(),
        BTreeMap::from([(InputProvenance::Inferred, 1)])
    );
    let json = serde_json::to_value(&guessed).unwrap();
    assert_eq!(json["items"][1]["input_provenance"], "inferred");
    assert!(
        guessed
            .text()
            .contains("modeling inputs: 0 read, 1 inferred, 0 unknown")
    );
}

#[test]
fn parameters_keep_names_and_expressions_in_dependency_order() {
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &block_dump());
    assert_eq!(report.parameters.imported, 5, "{:?}", report.parameters);
    assert!(report.parameters.literal.is_empty());
    assert!(
        report.parameters.mismatched.is_empty(),
        "{:?}",
        report.parameters
    );
    let p = |name: &str| {
        let params = doc.parameters();
        let id = params.find(name).expect(name);
        let p = params.get(id).unwrap();
        (p.expression().to_owned(), p.value())
    };
    assert_eq!(p("width"), ("height * 2".to_owned(), 40.0));
    assert_eq!(p("d1"), ("width".to_owned(), 40.0));
    // Model parameters belong to the items that made them.
    let params = doc.parameters();
    let owner = |name: &str| params.owner(params.find(name).unwrap());
    assert_eq!(owner("d1"), Some(FeatureUid(1)));
    assert_eq!(owner("d2"), Some(FeatureUid(2)));
    assert_eq!(owner("width"), None);
    // The file's favorites keep their star.
    assert!(doc.is_favorite_parameter("height"));
    assert!(!doc.is_favorite_parameter("width"));
}

#[test]
fn sketches_and_extrudes_replay_with_parameters() {
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &block_dump());
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    // The polygon constraint (of two lines) is left out, so the sketch is
    // partial.
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Parametric],
        "{}",
        report.text()
    );
    assert!(
        report.items[0]
            .note
            .as_deref()
            .unwrap()
            .contains("PolygonConstraint: a polygon needs its corners and center")
    );
    let sketch = doc.feature(FeatureUid(1)).unwrap();
    assert_eq!(sketch.name, "Sketch1");
    let FeatureDef::Sketch(s) = &sketch.def else {
        panic!("a sketch");
    };
    assert_eq!(s.entities.len(), 8);
    assert_eq!(s.constraints.len(), 2);
    assert_eq!(s.dimensions.len(), 1);
    assert!(s.frame.is_none());
    let extrude = doc.feature(FeatureUid(2)).unwrap();
    assert_eq!(extrude.name, "Extrude1");
    let def = doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap();
    let def: Value = serde_json::from_str(&def).unwrap();
    assert_eq!(def["def"]["extent"]["distance"], "d2");
    assert_eq!(def["def"]["operation"], "new_body");
    assert_eq!(doc.bodies().len(), 1);
    // A parameter edit flows through the imported expressions.
    doc.set_parameter_expression("height", "30 mm", None)
        .unwrap();
    assert_eq!(doc.status(FeatureUid(1)).map(|s| s.is_ok()), Some(true));
}

#[test]
fn undo_takes_back_the_whole_import() {
    let mut doc = Document::new(MockKernel::default());
    let options = Options {
        undo_label: Some("Import block.f3d".to_owned()),
        ..Options::default()
    };
    import_design(&mut doc, &block_dump(), &mut NoGeometry, &options);
    assert_eq!(doc.undo_label(), Some("Import block.f3d"));
    doc.undo();
    assert_eq!(doc.features().count(), 0);
    assert_eq!(doc.parameters().len(), 0);
}

#[test]
fn later_definitions_refer_to_earlier_features() {
    let uids: Vec<FeatureUid> = vec!["F7".parse().unwrap()];
    let mut def = json!({"type": "extrude", "profiles": [{"sketch": "$0", "region": "r{c1}"}],
                         "note": "$1", "other": "$x"});
    substitute(&mut def, &uids);
    assert_eq!(def["profiles"][0]["sketch"], "F7");
    // Unknown indices and other strings stay.
    assert_eq!(def["note"], "$1");
    assert_eq!(def["other"], "$x");
}

#[test]
fn a_document_fork_adds_features_as_the_document_would() {
    // A fork's feature gets the uid the document's would, and its result,
    // adopted, makes adding the same definition to the document reuse it.
    let mut doc = Document::new(MockKernel::default());
    import(&mut doc, &block_dump());
    let def: FeatureDef<mitcad_model::ValueInput> = serde_json::from_value(json!({
        "type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c5[c8,c6],c6[c5,c7],c7[c6,c8],c8[c7,c5]}"}],
        "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}))
    .unwrap();
    let mut fork = doc.fork().expect("the mock kernel forks");
    let forked = fork.add_feature(&def, None).unwrap().uid;
    let extrudes = |d: &Document<MockKernel>| d.kernel().calls.borrow().get("extrude").copied();
    assert_eq!(extrudes(&fork), Some(1));
    assert_eq!(doc.adopt_results(&fork, &[forked]), 1);
    let before = extrudes(&doc);
    let added = doc.add_feature(&def, None).unwrap().uid;
    assert_eq!(added, forked);
    assert_eq!(extrudes(&doc), before, "evaluated again");
    assert_eq!(doc.status(added).and_then(|s| s.error()), None);
}

#[test]
fn a_join_may_change_the_volume_by_the_bodies_it_merges() {
    // A prism of 26 mm³ whose join merges a body of 230 mm³ that the
    // history's state still holds: a change of 256 mm³ is possible for a
    // join, not for a cut.
    let candidate = |operation: &str| {
        let mut c = Candidate::new(json!({"type": "extrude", "operation": operation,
            "extent": {"type": "distance", "distance": 1.0}}));
        c.predicted = Some(26.0);
        c
    };
    let kept = |operation: &str, mergeable: f64| {
        feasible("Extrude", vec![candidate(operation)], 256.0, mergeable).len()
    };
    assert_eq!(kept("join", 0.0), 0);
    assert_eq!(kept("join", 230.0), 1);
    assert_eq!(kept("cut", 230.0), 0);
}
