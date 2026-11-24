// SPDX-License-Identifier: MIT
//! Items that cannot be replayed: the stored bodies they take, skipped items and the file's base feature.

use super::*;

#[test]
fn a_feature_that_cannot_be_replayed_takes_the_files_bodies_and_the_timeline_goes_on() {
    fallback_and_continue(false, false);
}

#[test]
fn with_the_items_states_known_a_failed_item_takes_its_own_state() {
    fallback_and_continue(true, false);
}

#[test]
fn in_a_component_the_fallback_goes_into_the_component() {
    fallback_and_continue(true, true);
}

/// A fillet the decoder gives no edges of between an extrusion and a join;
/// `known`: the history names each item's state (`result_no`);
/// `component`: the bodies are those of component Plate (object 10), placed
/// 5 cm along X.
fn fallback_and_continue(known: bool, component: bool) {
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        fillet_item(2),
        extrude(3, 0, "JoinFeatureOperation", "d2"),
    ];
    // The file's states: the block, the block rounded (some fillet), then the
    // second extrusion joined to it.
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..2].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let block = shapes(&reference);
    let edge = "E{F2:side(c5[c8,c6])|F2:side(c6[c5,c7])}";
    add(
        &mut reference,
        json!({"type": "fillet", "body": "F2.b0", "edges": [edge], "radius": 2}),
    );
    let rounded = shapes(&reference);
    // The join as Mitcad makes it on the stored rounded body.
    reference.undo();
    reference
        .add_base_feature(BaseInput {
            replaces: vec![mitcad_model::BodyUid::new(FeatureUid(2), 0)],
            ..BaseInput::new(vec![ImportBody {
                name: None,
                color: None,
                shape: rounded[0].clone(),
                face_colors: Vec::new(),
            }])
        })
        .unwrap();
    let region = "r{c5[c8,c6],c6[c5,c7],c7[c6,c8],c8[c7,c5]}";
    add(
        &mut reference,
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
                               "extent": {"type": "distance", "distance": "d2"}, "flip": false,
                               "operation": "join"}),
    );
    let joined = shapes(&reference);
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), block, rounded, joined],
        items: if known {
            HashMap::from([(1, 1), (2, 2), (3, 3)])
        } else {
            HashMap::new()
        },
        component: component.then_some(10),
        ..TestGeometry::default()
    };
    let mut dump = timeline_dump(items);
    if component {
        let mut value = serde_json::to_value(&dump).unwrap();
        value["components"] = json!([{"name": "Plate", "_f3d": {"object_id": 10}}]);
        value["occurrences"] = json!([{"component": "Plate", "_f3d": {"component_object": 10},
            "transform": [[1.0, 0.0, 0.0, 5.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0],
                          [0.0, 0.0, 0.0, 1.0]]}]);
        dump = Dump::from_json(&value.to_string()).unwrap();
    }

    let mut doc = Document::new(TestKernel::default());
    let report = import_design(&mut doc, &dump, &mut geometry, &Options::default());
    if component {
        // Every item and the fallback in Plate; its body placed with it.
        let plate = doc.assembly().components[0].uid;
        assert!(
            doc.features().all(|f| f.component == plate),
            "{}",
            report.text()
        );
        assert_eq!(doc.body_component(doc.bodies()[0].uid), Some(plate));
        let instances = doc.instances();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].transform.translation, [50.0, 0.0, 0.0]);
    }
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [
            Outcome::Partial,
            Outcome::Parametric,
            Outcome::Fallback,
            Outcome::Parametric
        ],
        "{}",
        report.text()
    );
    // The fallback replaced the block, keeping its body id.
    let fallback = doc.feature(FeatureUid(3)).unwrap();
    assert_eq!(fallback.name, "Fillet1");
    let FeatureDef::Base(base) = &fallback.def else {
        panic!("a base feature");
    };
    assert_eq!(
        base.replaces,
        [mitcad_model::BodyUid::new(FeatureUid(2), 0)]
    );
    assert_eq!(report.items[3].verified, Some(true));
    assert!(report.history.reached_end, "{}", report.text());
    assert_eq!(doc.bodies().len(), 1);
}

/// Sheets (mitcad#35): an item the import does not translate made a surface
/// body; it comes in with the item's fallback, and a later join leaves it
/// alone (its participants are the solids).
#[test]
fn sheets_of_the_history_come_in_as_stored_bodies() {
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        json!({"index": 2, "name": "Patch1", "objectType": "PatchFeature", "detail": {}}),
        extrude(3, 0, "JoinFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..2].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let block = shapes(&reference);
    let region = "r{c5[c8,c6],c6[c5,c7],c7[c6,c8],c8[c7,c5]}";
    add(
        &mut reference,
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
               "extent": {"type": "distance", "distance": "d2"}, "flip": false,
               "operation": "join"}),
    );
    let joined = shapes(&reference);
    let sheet = MockShape {
        // The mock kernel's surface body.
        history: "patch,sheet".to_owned(),
        faces: block[0].faces[..1].to_vec(),
        edges: Vec::new(),
        bounds: None,
        parts: Vec::new(),
    };
    let mut geometry = TestGeometry {
        states: vec![
            Vec::new(),
            block.clone(),
            vec![block[0].clone(), sheet.clone()],
            vec![joined[0].clone(), sheet],
        ],
        items: HashMap::from([(1, 1), (2, 2), (3, 3)]),
        ..TestGeometry::default()
    };
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [
            Outcome::Partial,
            Outcome::Parametric,
            Outcome::Fallback,
            Outcome::Parametric
        ],
        "{}",
        report.text()
    );
    // The patch's fallback holds the sheet; the join works on the block.
    let FeatureDef::Base(base) = &doc.feature(FeatureUid(3)).unwrap().def else {
        panic!("a base feature");
    };
    assert_eq!(base.bodies.len(), 1);
    let FeatureDef::Extrude(join) = &doc.feature(FeatureUid(4)).unwrap().def else {
        panic!("an extrusion");
    };
    assert_eq!(
        join.participants,
        [mitcad_model::BodyUid::new(FeatureUid(2), 0)]
    );
    assert_eq!(report.items[3].verified, Some(true));
    let kinds: Vec<BodyKind> = doc
        .bodies()
        .iter()
        .map(|b| doc.kernel().body_kind(b.shape).unwrap())
        .collect();
    assert_eq!(kinds, [BodyKind::Solid, BodyKind::Sheet]);
    // The sheet is the file's: not an extra body, and no last fallback.
    assert!(report.extra_bodies.is_empty(), "{}", report.text());
    assert_eq!(doc.features().count(), 4, "{}", report.text());
}

#[test]
fn an_item_whose_state_cannot_be_rebuilt_is_taken_unchecked() {
    let (items, mut geometry) = block_history(vec![1]);
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Parametric, Outcome::Parametric],
        "{}",
        report.text()
    );
    assert_eq!(report.items[1].verified, None);
    assert!(
        report.items[1]
            .note
            .as_deref()
            .unwrap()
            .contains("not checked"),
        "{}",
        report.text()
    );
    assert_eq!(report.items[2].verified, Some(true));
    assert_eq!(report.history.unbuilt, 1);
}

#[test]
fn a_definition_that_makes_an_invalid_body_is_not_taken() {
    for validate in [false, true] {
        let (items, mut geometry) = block_history(Vec::new());
        let mut doc = Document::new(TestKernel {
            invalid: Some("join("),
            ..TestKernel::default()
        });
        let report = import_design(
            &mut doc,
            &timeline_dump(items),
            &mut geometry,
            &Options {
                validate,
                ..Options::default()
            },
        );
        let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
        let join = if validate {
            Outcome::Fallback
        } else {
            Outcome::Parametric
        };
        assert_eq!(
            outcomes,
            [Outcome::Partial, Outcome::Parametric, join],
            "{}",
            report.text()
        );
        if validate {
            assert!(report.text().contains("checker finds"), "{}", report.text());
        }
    }
}

#[test]
fn an_unchecked_item_does_not_take_every_body_away() {
    // A cut whose state cannot be rebuilt and that would remove the block.
    let (mut items, mut geometry) = block_history(vec![2]);
    items[2] = extrude(2, 0, "CutFeatureOperation", "d2");
    let mut doc = Document::new(TestKernel::default());
    doc.kernel().mock.split.set(Some(0));
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    assert_ne!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    assert!(
        report.items[2]
            .note
            .as_deref()
            .unwrap_or_default()
            .contains("it would leave no bodies"),
        "{}",
        report.text()
    );
    assert_eq!(doc.bodies().len(), 1, "{}", report.text());
}

#[test]
fn after_the_time_limit_items_take_their_states() {
    wall_clock();
    let (items, mut geometry) = block_history(Vec::new());
    let mut doc = Document::new(TestKernel::default());
    let options = Options {
        time_limit: Some(0.0),
        ..Options::default()
    };
    let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, &options);
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Fallback, Outcome::Fallback],
        "{}",
        report.text()
    );
    assert!(
        report.items[1]
            .note
            .as_deref()
            .unwrap()
            .contains("time limit")
    );
    assert_eq!(doc.bodies().len(), 1);
}

#[test]
fn items_the_kernel_hung_on_take_their_states_and_progress_is_shared() {
    let (items, mut geometry) = block_history(Vec::new());
    let mut doc = Document::new(TestKernel::default());
    let progress = std::sync::Arc::new(crate::Progress::new());
    let options = Options {
        hung_items: vec![1],
        progress: Some(progress.clone()),
        ..Options::default()
    };
    let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, &options);
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    // (The mock kernel's join on the base feature's body is not the state.)
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Fallback, Outcome::Fallback],
        "{}",
        report.text()
    );
    let note = |i: usize| report.items[i].note.clone().unwrap_or_default();
    assert!(note(1).contains("did not finish"), "{}", report.text());
    assert!(!note(2).contains("did not finish"), "{}", report.text());
    use std::sync::atomic::Ordering;
    assert_eq!(
        progress.item.load(Ordering::Relaxed),
        crate::Progress::FINISHING
    );
    assert!(progress.step.load(Ordering::Relaxed) >= 4);
}

#[test]
fn items_tried_in_vain_take_their_states_at_once_in_a_try_run_again() {
    // A first try (the kernel hung on item 1 before): the join's
    // definitions give no state (the mock kernel's join on the base
    // feature's body), which the progress keeps with why.
    use std::sync::atomic::Ordering;
    let run = |failed_items: Vec<(i64, String)>| {
        let (items, mut geometry) = block_history(Vec::new());
        let mut doc = Document::new(TestKernel::default());
        let progress = Arc::new(crate::Progress::new());
        let options = Options {
            hung_items: vec![1],
            failed_items,
            progress: Some(progress.clone()),
            ..Options::default()
        };
        let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, &options);
        (report, progress)
    };
    let (first, progress) = run(Vec::new());
    let failed = progress.failed_items();
    assert_eq!(
        failed.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        [2],
        "{}",
        first.text()
    );
    let steps = progress.step.load(Ordering::Relaxed);
    // Run again with them: the join takes its state with the same note,
    // without its definitions being tried (fewer steps), and is not kept
    // again.
    let (second, progress) = run(failed);
    let outcomes = |r: &DesignReport| r.items.iter().map(|i| i.outcome).collect::<Vec<_>>();
    assert_eq!(outcomes(&second), outcomes(&first), "{}", second.text());
    assert_eq!(
        second.items[2].note,
        first.items[2].note,
        "{}",
        second.text()
    );
    assert!(progress.step.load(Ordering::Relaxed) < steps);
    assert!(progress.failed_items().is_empty());
}

/// The block's import (sketch, new body, join) with these options: the
/// report, the document and the outcomes.
fn import_block(options: &Options) -> (DesignReport, Document<TestKernel>, Vec<Outcome>) {
    let (items, mut geometry) = block_history(Vec::new());
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, options);
    let outcomes = report.items.iter().map(|i| i.outcome).collect();
    (report, doc, outcomes)
}

#[test]
fn after_a_stop_the_features_take_their_states_and_the_report_says_where() {
    wall_clock();
    let (report, _, outcomes) = import_block(&Options::default());
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Parametric, Outcome::Parametric],
        "{}",
        report.text()
    );
    assert_eq!(report.stopped, None);

    // Stopped before it started: the sketch still comes in, the features
    // take their history states.
    let stop = Arc::new(RecomputeMonitor::new());
    stop.cancel();
    let progress = Arc::new(crate::Progress::new());
    let (report, doc, outcomes) = import_block(&Options {
        stop: Some(stop),
        progress: Some(progress.clone()),
        ..Options::default()
    });
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Fallback, Outcome::Fallback],
        "{}",
        report.text()
    );
    let note = |i: usize| report.items[i].note.clone().unwrap_or_default();
    assert!(note(1).contains(STOPPED), "{}", report.text());
    assert!(note(2).contains(STOPPED), "{}", report.text());
    let name = report.items[1].name.clone();
    assert_eq!(
        report.stopped,
        Some(StopReport {
            item: Some(1),
            name: Some(name.clone())
        })
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains(&format!("stopped at {name}"))),
        "{}",
        report.text()
    );
    assert_eq!(progress.stopped_at(), Some(1));
    assert_eq!(doc.bodies().len(), 1);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["stopped"]["item"], 1, "{json}");
}

#[test]
fn a_stop_while_a_feature_is_tried_keeps_the_items_before_it() {
    wall_clock();
    // Cancelled while the first definition is evaluated (made to take 20 s
    // longer): the stop's monitor is the document's while definitions are
    // tried, and cuts the evaluation short.
    let stop = Arc::new(RecomputeMonitor::new());
    stop.set_test_delay(20_000);
    let canceller = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            stop.cancel();
        })
    };
    let started = std::time::Instant::now();
    let (report, doc, outcomes) = import_block(&Options {
        stop: Some(stop),
        ..Options::default()
    });
    canceller.join().unwrap();
    assert!(
        started.elapsed().as_secs_f64() < 10.0,
        "the stop did not cut the evaluation short"
    );
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Fallback, Outcome::Fallback],
        "{}",
        report.text()
    );
    assert_eq!(report.stopped.as_ref().and_then(|s| s.item), Some(1));
    assert!(
        doc.monitor().is_none(),
        "the document's monitor is left as it was"
    );
    assert_eq!(doc.bodies().len(), 1);

    // A try run again after a hang stops where the stopped one did: the
    // items before it are replayed.
    let (report, _, outcomes) = import_block(&Options {
        stop_at: Some(2),
        ..Options::default()
    });
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Parametric, Outcome::Fallback],
        "{}",
        report.text()
    );
    assert_eq!(report.stopped.as_ref().and_then(|s| s.item), Some(2));
}

#[test]
fn bodies_an_unreplayed_item_added_in_the_same_step_come_in_as_its_fallback() {
    // The file's history has the sweep's body and the extrusion in one state.
    let items = vec![
        two_rectangles(0),
        json!({"index": 1, "name": "Sweep1", "objectType": "SweepFeature"}),
        extrude(2, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..1].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let large = "r{c9[c12,c10],c10[c9,c11],c11[c10,c12],c12[c11,c9]}";
    let small = "r{c13[c16,c14],c14[c13,c15],c15[c14,c16],c16[c15,c13]}";
    for (region, distance) in [(large, 5), (small, 10)] {
        add(
            &mut reference,
            json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
                   "extent": {"type": "distance", "distance": distance}, "operation": "new_body"}),
        );
    }
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), shapes(&reference)],
        ..TestGeometry::default()
    };
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes[1..],
        [Outcome::Fallback, Outcome::Parametric],
        "{}",
        report.text()
    );
    assert_eq!(report.items[2].verified, Some(true));
    assert!(report.history.reached_end, "{}", report.text());
    // The extrusion, then the sweep's body as a base feature named after it.
    assert_eq!(doc.bodies().len(), 2);
    let base = doc.feature(FeatureUid(3)).unwrap();
    assert_eq!(base.name, "Sweep1");
    assert!(matches!(base.def, FeatureDef::Base(_)));
}

#[test]
fn without_a_history_a_failure_takes_the_stored_bodies() {
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        fillet_item(2),
        extrude(3, 0, "JoinFeatureOperation", "d2"),
    ];
    struct Stored(Vec<MockShape>);
    impl StoredGeometry<MockShape> for Stored {
        fn state_count(&mut self) -> usize {
            0
        }
        fn state(&mut self, _: usize) -> Result<Vec<StoredBody<MockShape>>, String> {
            Err("no history".to_owned())
        }
        fn final_bodies(&mut self) -> Result<Vec<StoredBody<MockShape>>, String> {
            Ok(self
                .0
                .iter()
                .map(|s| StoredBody {
                    shape: s.clone(),
                    name: Some("Block".to_owned()),
                    source: "stored".to_owned(),
                    component: None,
                    id: None,
                    lineage: None,
                })
                .collect())
        }
    }
    let stored = MockShape::imported("stored block", 9);
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut Stored(vec![stored]),
        &Options::default(),
    );
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [
            Outcome::Partial,
            Outcome::Parametric,
            Outcome::Fallback,
            Outcome::Skipped
        ],
        "{}",
        report.text()
    );
    assert_eq!(report.bodies.len(), 1);
    assert_eq!(report.bodies[0].volume_difference, Some(0.0));
}

#[test]
fn an_item_without_a_state_leaves_the_next_items_state_to_it() {
    // Two extrusions of the same block; the history names the second one's
    // state, so the first one (whose operation the file does not have:
    // rolled back, say) does not take it.
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        extrude(2, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..2].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), shapes(&reference)],
        items: HashMap::from([(2, 1)]),
        ..TestGeometry::default()
    };
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Skipped, Outcome::Parametric],
        "{}",
        report.text()
    );
    assert_eq!(report.items[2].verified, Some(true));
}

/// Items the decoder names without translating them (mitcad#43): those
/// that change no body are skipped at once with what they are; one that
/// changes bodies takes its history state with the reason.
#[test]
fn named_items_without_geometry_are_skipped_and_others_say_why_they_fall_back() {
    let (mut items, mut geometry) = block_history(Vec::new());
    let named = |index: i64, name: &str, t: &str| json!({"index": index, "name": name, "objectType": t, "detail": {}});
    items[2] = named(6, "EdgeFlange1", "FlangeFeature");
    items.splice(
        2..2,
        [
            named(2, "Pin", "Occurrence"),
            named(3, "GeometricRelationship1", "GeometricRelationship"),
            named(4, "Fixings", "Group"),
            named(5, "R-Pattern1", "RectangularOccurrencePattern"),
        ],
    );
    geometry.items = HashMap::from([(1, 1), (6, 2)]);
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let got: Vec<(&str, Outcome)> = report.items[2..]
        .iter()
        .map(|i| (i.object_type.as_str(), i.outcome))
        .collect();
    assert_eq!(
        got,
        [
            ("Occurrence", Outcome::Skipped),
            ("GeometricRelationship", Outcome::Skipped),
            ("Group", Outcome::Skipped),
            ("RectangularOccurrencePattern", Outcome::Skipped),
            ("FlangeFeature", Outcome::Fallback),
        ],
        "{}",
        report.text()
    );
    let note = |k: usize| report.items[k].note.clone().unwrap_or_default();
    assert!(
        note(2).starts_with("no geometry: places occurrences"),
        "{}",
        note(2)
    );
    assert!(
        note(3).starts_with("its constraints were not decoded"),
        "{}",
        note(3)
    );
    assert!(
        note(4).starts_with("no geometry: a timeline group"),
        "{}",
        note(4)
    );
    assert!(note(6).starts_with("a sheet metal flange"), "{}", note(6));
    assert!(report.history.reached_end, "{}", report.text());
}

/// A fallback that takes a state where a body was split (mitcad#181): the
/// piece the file keeps as the body (`StoredBody::lineage`) keeps the body's
/// id, so that the split's other pieces are the bodies the file counts as
/// its new ones (a later cut names them by their index among those).
/// Without lineages the first piece of the state replaces the body.
#[test]
fn a_fallback_keeps_the_body_the_file_continues() {
    let whole = MockShape::imported("whole", 12);
    let kept = MockShape::imported("kept piece", 6);
    let first = MockShape::imported("first new piece", 7);
    let second = MockShape::imported("second new piece", 8);
    let items = vec![
        json!({"index": 0, "name": "Base Feature1", "objectType": "BaseFeature"}),
        json!({"index": 1, "name": "Split1", "objectType": "SplitBodyFeature", "detail": {}}),
    ];
    for lineages in [true, false] {
        let mut geometry = TestGeometry {
            states: vec![
                vec![whole.clone()],
                vec![first.clone(), kept.clone(), second.clone()],
            ],
            items: HashMap::from([(0, 0), (1, 1)]),
            ids: true,
            lineages: if lineages {
                vec![vec![1], vec![2, 1, 3]]
            } else {
                Vec::new()
            },
            ..TestGeometry::default()
        };
        let mut doc = Document::new(TestKernel::default());
        let report = import_design(
            &mut doc,
            &timeline_dump(items.clone()),
            &mut geometry,
            &Options::default(),
        );
        assert_eq!(
            report.items[1].outcome,
            Outcome::Fallback,
            "{}",
            report.text()
        );
        let base = &report.items[0].features[0];
        let body = doc
            .body_shape(format!("{base}.b0").parse().unwrap())
            .unwrap();
        let expected = if lineages {
            "kept piece"
        } else {
            "first new piece"
        };
        assert!(body.history.contains(expected), "{}", report.text());
    }
}

#[test]
fn an_item_the_file_keeps_no_result_for_is_skipped() {
    // mitcad#96: result number -1 with the third state byte set (its
    // change is in parts in other components, mitcad#180; the suppressed
    // ones are in `suppressed_tests`); one with none set is tried as
    // before.
    let mut fillet = fillet_item(2);
    fillet["_f3d"] = json!({"result_no": -1, "flags": "000001"});
    let mut failed = extrude(3, 0, "JoinFeatureOperation", "d2");
    failed["_f3d"] = json!({"result_no": -1, "flags": "000001"});
    let mut plain = extrude(4, 0, "NewBodyFeatureOperation", "d2");
    plain["_f3d"] = json!({"result_no": -1, "flags": "000000"});
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        fillet,
        failed,
        plain,
    ];
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    for i in [2, 3] {
        assert_eq!(
            report.items[i].outcome,
            Outcome::Skipped,
            "{}",
            report.text()
        );
        assert!(
            report.items[i]
                .note
                .as_deref()
                .is_some_and(|n| n.contains("keeps no result")),
            "{}",
            report.text()
        );
    }
    assert_eq!(
        report.items[4].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
}

#[test]
fn an_item_whose_state_the_history_does_not_hold_is_skipped() {
    // A feature suppressed in an `.ipt` part: its state is gone from the
    // history. It is not tried; the join after it gives its own state.
    let (mut items, mut geometry) = block_history(Vec::new());
    let mut joined = extrude(3, 0, "JoinFeatureOperation", "d2");
    joined["name"] = json!("Extrude3");
    items.push(joined);
    geometry.items = HashMap::from([(1, 1), (3, 2)]);
    geometry.without_result = vec![2];
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [
            Outcome::Partial,
            Outcome::Parametric,
            Outcome::Skipped,
            Outcome::Parametric
        ],
        "{}",
        report.text()
    );
    assert!(
        report.items[2]
            .note
            .as_deref()
            .is_some_and(|n| n.contains("keeps no result")),
        "{}",
        report.text()
    );
    assert_eq!(report.items[3].verified, Some(true), "{}", report.text());
}

#[test]
fn an_item_whose_operation_changed_no_body_is_skipped_as_failed() {
    // mitcad#156: the history holds the item's operation, but it left the
    // bodies as they were (an extrusion that found no body to cut in the
    // file); no definition does that, so it is skipped, not a fallback.
    let (mut items, mut geometry) = block_history(Vec::new());
    let mut failed = extrude(3, 0, "JoinFeatureOperation", "d2");
    failed["name"] = json!("Extrude3");
    items.push(failed);
    geometry.items.insert(3, 2);
    geometry.unchanged = vec![3];
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let failed = &report.items[3];
    assert_eq!(failed.outcome, Outcome::Skipped, "{}", report.text());
    assert!(
        failed
            .note
            .as_deref()
            .is_some_and(|n| n.contains("failed in the file")),
        "{}",
        report.text()
    );
    assert!(failed.features.is_empty(), "{}", report.text());
    assert_eq!(report.items[2].outcome, Outcome::Parametric);
    // Without the history's word it is not called failed.
    let (mut items, mut geometry) = block_history(Vec::new());
    items.push(extrude(3, 0, "JoinFeatureOperation", "d2"));
    geometry.items.insert(3, 2);
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let note = report.items[3].note.as_deref().unwrap_or_default();
    assert!(!note.contains("failed in the file"), "{}", report.text());
}

#[test]
fn an_item_whose_change_stays_within_the_tolerance_is_skipped() {
    // mitcad#343: a cut across components that shaves micrometres off this
    // component's body (its state after is the state before within the
    // geometric tolerance) and that no definition reaches: the bodies as
    // they are stand for its state, carrying what the items before them
    // carried; skipped, no base feature.
    let (mut items, mut geometry) = block_history(Vec::new());
    let mut item = extrude(3, 0, "JoinFeatureOperation", "d2");
    item["name"] = json!("Extrude3");
    items.push(item);
    let after = geometry.states[2].clone();
    geometry.states.push(after);
    geometry.items.insert(3, 3);
    let mut doc = Document::new(TestKernel {
        lists_new_faces: true,
        ..TestKernel::default()
    });
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let item = &report.items[3];
    assert_eq!(item.outcome, Outcome::Skipped, "{}", report.text());
    assert!(
        item.note
            .as_deref()
            .is_some_and(|n| n.contains("no change by it beyond the geometric tolerance")),
        "{}",
        report.text()
    );
    assert!(item.features.is_empty(), "{}", report.text());
    assert!(
        !doc.features().any(|f| matches!(f.def, FeatureDef::Base(_))),
        "{}",
        report.text()
    );
    assert_eq!(report.items[2].outcome, Outcome::Parametric);
}

#[test]
fn an_item_naming_an_earlier_item_s_operation_is_skipped_as_failed() {
    // mitcad#343: a chamfer after a rounding names the rounding's operation
    // (both items have its result number), so the history holds none of its
    // own and its state is the state before it; no definition does that, so
    // it is skipped as failed in the file, not a fallback.
    let (mut items, mut geometry) = block_history(Vec::new());
    let mut failed = extrude(3, 0, "JoinFeatureOperation", "d2");
    failed["name"] = json!("Extrude3");
    items.push(failed);
    geometry.items.insert(3, 2);
    geometry.shared = vec![3];
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let failed = &report.items[3];
    assert_eq!(failed.outcome, Outcome::Skipped, "{}", report.text());
    assert!(
        failed
            .note
            .as_deref()
            .is_some_and(|n| n.contains("failed in the file") && n.contains("none of its own")),
        "{}",
        report.text()
    );
    assert!(failed.features.is_empty(), "{}", report.text());
    assert_eq!(report.items[2].outcome, Outcome::Parametric);
    // Without the history's word it is not called failed.
    let (mut items, mut geometry) = block_history(Vec::new());
    items.push(extrude(3, 0, "JoinFeatureOperation", "d2"));
    geometry.items.insert(3, 2);
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let note = report.items[3].note.as_deref().unwrap_or_default();
    assert!(!note.contains("failed in the file"), "{}", report.text());
}

#[test]
fn operations_before_an_item_s_own_may_be_its_own() {
    // mitcad#156: a pattern makes an operation per copy, and the item names
    // the last; the states of the others come between the item before and
    // its own. From the state the replay is at, the item gives its own
    // state, with no base feature for the states between.
    let (items, mut geometry) = block_history(Vec::new());
    let joined = geometry.states.pop().unwrap();
    let mut other = Document::new(TestKernel::default());
    import_design(
        &mut other,
        &timeline_dump(vec![
            circle_sketch(0),
            extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        ]),
        &mut NoGeometry,
        &Options::default(),
    );
    geometry.states.push(shapes(&other));
    geometry.states.push(joined);
    geometry.items = HashMap::from([(1, 1), (2, 3)]);
    let run = |geometry: &mut TestGeometry| {
        let mut doc = Document::new(TestKernel::default());
        import_design(
            &mut doc,
            &timeline_dump(items.clone()),
            geometry,
            &Options::default(),
        )
    };
    // Without the history's word the state between comes in first, and the
    // join on it gives no state.
    let report = run(&mut geometry);
    assert_eq!(
        report.items[2].outcome,
        Outcome::Fallback,
        "{}",
        report.text()
    );
    geometry.starts = HashMap::from([(2, 1)]);
    let report = run(&mut geometry);
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    assert_eq!(report.items[2].verified, Some(true));
    assert!(report.warnings.is_empty(), "{}", report.text());
}

/// The file's own base feature (bodies without a history in the file)
/// comes in as a base feature of the bodies the file stores for it: that
/// is its translation, not a fallback standing in for it, so its feature
/// gets no import note.
#[test]
fn the_files_base_feature_is_its_stored_bodies() {
    let stored = MockShape::imported("stored body", 6);
    let items = vec![json!({"index": 0, "name": "Base Feature1", "objectType": "BaseFeature"})];
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), vec![stored]],
        items: HashMap::from([(0, 1)]),
        ..TestGeometry::default()
    };
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let item = &report.items[0];
    assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(item.input_provenance, Some(InputProvenance::Read));
    let uid: FeatureUid = item.features[0].parse().unwrap();
    assert_eq!(doc.feature(uid).unwrap().def.type_name(), "base");
    assert_eq!(doc.import_note(uid), None, "{}", report.text());
}

#[test]
fn symptoms_and_kernel_failures_of_definitions() {
    // What only says the result is not the file's (mitcad#325) ...
    assert!(symptom(NOT_IN_HISTORY));
    assert!(symptom(
        "the extrusion does not cut into any participant body"
    ));
    assert!(symptom("there are no bodies in the extrude direction"));
    assert!(!symptom(
        "fillet failed; the radius may be too large for the edges"
    ));
    // ... and what the geometry kernel failed in.
    assert!(kernel_failure(
        "the boolean operation produced an invalid shape (face: BRepCheck_UnorientableShape)"
    ));
    assert!(kernel_failure(
        "fillet failed; the radius may be too large for the edges"
    ));
    assert!(kernel_failure("fillet: TopoDS_Builder::Add"));
    assert!(kernel_failure(
        "the taper failed; the angle may close the profile"
    ));
    assert!(!kernel_failure(NOT_IN_HISTORY));
    assert!(!kernel_failure(
        "the extrusion does not cut into any participant body"
    ));
}

#[test]
fn inputs_not_decoded_fail_on_any_bodies() {
    // An item without definitions for want of decoded inputs is not tried
    // again from another state (mitcad#658) ...
    assert!(undecoded("its path was not decoded"));
    assert!(undecoded("its faces were not decoded"));
    // ... unless the history's next states were looked at for them, or
    // inputs were decoded but not found.
    assert!(!undecoded(
        "its source faces were not decoded, and no next history state lost faces"
    ));
    assert!(!undecoded(
        "its faces were not decoded (no history to find them)"
    ));
    assert!(!undecoded("an edge of its path was not found"));
    assert!(!undecoded(NOT_IN_HISTORY));
}
