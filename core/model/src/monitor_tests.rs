// SPDX-License-Identifier: MIT
//! Progress and cancellation of recomputes (P7), with the mock kernel: a
//! cancelled command, undo, redo, preview or link update changes nothing
//! but the cache, which keeps what was evaluated before the cancel.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::document_tests::{Block, block, extrude, num};
use crate::features::SketchPlane;
use crate::ids::FeatureUid;
use crate::testing::MockKernel;
use crate::{Document, InsertOptions, ModelError, Progress, RecomputeMonitor};

/// Block (Sketch1, Extrude1 with d3) and a boss joined to it: Sketch2
/// and Extrude2 (F4). A new d3 evaluates Extrude1 and Extrude2; Sketch2
/// comes from the cache.
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

fn monitor(doc: &mut Document<MockKernel>) -> Arc<RecomputeMonitor> {
    let monitor = Arc::new(RecomputeMonitor::new());
    doc.set_monitor(Some(monitor.clone()));
    monitor
}

fn value(doc: &Document<MockKernel>, name: &str) -> f64 {
    let params = doc.parameters();
    params.value(params.find(name).unwrap()).unwrap()
}

/// What a cancelled command must leave as it was.
#[derive(Debug, PartialEq)]
struct Snapshot {
    json: String,
    revision: u64,
    modified: bool,
    undo: Option<String>,
    redo: Option<String>,
    undo_depth: usize,
    recomputes: u64,
    evaluated: Vec<FeatureUid>,
    bodies: Vec<String>,
}

fn snapshot(doc: &Document<MockKernel>) -> Snapshot {
    Snapshot {
        json: doc.to_json(),
        revision: doc.revision(),
        modified: doc.is_modified(),
        undo: doc.undo_label().map(str::to_owned),
        redo: doc.redo_label().map(str::to_owned),
        undo_depth: doc.undo_depth(),
        recomputes: doc.recompute_count(),
        evaluated: doc.stats().evaluated,
        bodies: doc
            .bodies()
            .iter()
            .map(|b| format!("{} {}", b.name, b.shape.history))
            .collect(),
    }
}

#[test]
fn progress_gives_the_position_the_feature_and_the_evaluations() {
    let (mut b, _) = joined();
    let monitor = monitor(&mut b.doc);
    assert_eq!(monitor.progress(), Progress::default());
    b.doc.set_parameter("d3", 35.0).unwrap();
    assert_eq!(
        monitor.progress(),
        Progress {
            position: 4,
            total: 4,
            evaluated: 2,
            feature: "Extrude2".to_owned(),
            cancelled: false,
        }
    );
    // Only the features before the marker count; cache hits leave the
    // feature and the evaluations.
    b.doc.set_marker(2).unwrap();
    assert_eq!(
        (monitor.progress().position, monitor.progress().total),
        (2, 2)
    );
    assert_eq!(monitor.progress().evaluated, 2);
    assert_eq!(monitor.progress().feature, "Extrude2");

    // Cancelled after one evaluation: it stops before the next feature.
    b.doc.set_marker(4).unwrap();
    let monitor = self::monitor(&mut b.doc);
    monitor.cancel_after(1);
    assert_eq!(b.doc.set_parameter("d3", 50.0), Err(ModelError::Cancelled));
    assert_eq!(
        monitor.progress(),
        Progress {
            position: 2,
            total: 4,
            evaluated: 1,
            feature: "Extrude1".to_owned(),
            cancelled: true,
        }
    );
}

#[test]
fn a_cancelled_command_changes_nothing_and_keeps_what_was_evaluated() {
    let (mut b, boss) = joined();
    // A redo step that a command would drop.
    b.doc.set_parameter("d1", 70.0).unwrap();
    b.doc.undo().unwrap();
    let before = snapshot(&b.doc);
    assert_eq!(before.redo.as_deref(), Some("Change d1"));

    let monitor = monitor(&mut b.doc);
    monitor.cancel_after(1);
    let error = b.doc.set_parameter("d3", 35.0).unwrap_err();
    assert_eq!(error, ModelError::Cancelled);
    assert_eq!(error.to_string(), "the computation was cancelled");
    assert_eq!(snapshot(&b.doc), before);
    assert_eq!(value(&b.doc, "d3"), 20.0);

    // Extrude1 was evaluated before the cancel and stays in the cache:
    // the next try evaluates only the rest.
    b.doc.set_monitor(None);
    b.doc.set_parameter("d3", 35.0).unwrap();
    assert_eq!(b.doc.stats().evaluated, vec![boss]);
    assert_eq!(value(&b.doc, "d3"), 35.0);
    assert_eq!(b.doc.undo_label(), Some("Change d3"));
    assert_eq!(b.doc.redo_label(), None);
    assert_ne!(b.doc.revision(), before.revision);

    // The JSON command is rejected with the same message.
    let monitor = self::monitor(&mut b.doc);
    monitor.cancel();
    let before = snapshot(&b.doc);
    let error = b
        .doc
        .command(r#"{"cmd": "set_parameter", "name": "d3", "value": 40}"#)
        .unwrap_err();
    assert_eq!(error.0, "the computation was cancelled");
    let error = b.doc.command(r#"{"cmd": "recompute"}"#).unwrap_err();
    assert_eq!(error.0, "the computation was cancelled");
    // The wrapper without a result keeps the last results.
    assert_eq!(b.doc.recompute(), b.doc.stats());
    assert_eq!(snapshot(&b.doc), before);
    // Commands that compute nothing are not cancelled.
    b.doc.set_parameter_comment("d3", "depth").unwrap();
    assert_eq!(b.doc.undo_label(), Some("Change Comment of d3"));
}

#[test]
fn an_evaluation_that_returns_after_the_cancel_is_not_cached() {
    let (mut b, boss) = joined();
    let monitor = monitor(&mut b.doc);
    // The cancel comes while Extrude2 joins, after Extrude1 was done.
    *b.doc.kernel().cancel_on.borrow_mut() = Some(("join", monitor.clone()));
    let joins = b.doc.kernel().count("join");
    let before = snapshot(&b.doc);
    assert_eq!(b.doc.set_parameter("d3", 35.0), Err(ModelError::Cancelled));
    assert_eq!(b.doc.kernel().count("join"), joins + 1);
    assert_eq!(snapshot(&b.doc), before);
    assert_eq!(monitor.progress().evaluated, 1);

    b.doc.set_monitor(None);
    b.doc.set_parameter("d3", 35.0).unwrap();
    // Extrude1 came from the cache, Extrude2 was evaluated again.
    assert_eq!(b.doc.stats().evaluated, vec![boss]);
    assert_eq!(b.doc.kernel().count("join"), joins + 2);
}

#[test]
fn a_kernel_operation_stopped_inside_an_evaluation_is_a_cancel() {
    let (mut b, boss) = joined();
    // Without a monitor the kernel's operations cannot be stopped.
    b.doc.set_parameter("d3", 25.0).unwrap();
    assert_eq!(b.doc.kernel().count("interruptible"), 0);
    // With one, each evaluation runs where they can (P7e), and only those.
    let monitor = monitor(&mut b.doc);
    b.doc.set_parameter("d3", 30.0).unwrap();
    assert_eq!(b.doc.kernel().count("interruptible"), 2);
    b.doc.set_parameter("d3", 25.0).unwrap();
    assert_eq!(b.doc.stats().evaluated, Vec::<FeatureUid>::new());
    assert_eq!(b.doc.kernel().count("interruptible"), 2);
    assert!(b.doc.kernel().interrupt.borrow().is_none());

    // The cancel comes while Extrude2 joins, which then stops and fails.
    *b.doc.kernel().cancel_on.borrow_mut() = Some(("join", monitor.clone()));
    b.doc.kernel().stop_on_cancel.set(true);
    let before = snapshot(&b.doc);
    assert_eq!(b.doc.set_parameter("d3", 35.0), Err(ModelError::Cancelled));
    assert_eq!(snapshot(&b.doc), before);
    // The failure was not kept: Extrude2 is evaluated again and succeeds.
    b.doc.kernel().cancel_on.borrow_mut().take();
    b.doc.set_monitor(None);
    b.doc.set_parameter("d3", 35.0).unwrap();
    assert_eq!(b.doc.stats().evaluated, vec![boss]);
    assert!(b.doc.stats().error.is_none());
    assert_eq!(value(&b.doc, "d3"), 35.0);
}

#[test]
fn cancelled_undo_and_redo_keep_their_steps() {
    let (mut b, _) = joined();
    let initial = b.doc.revision();
    b.doc.set_parameter("d3", 35.0).unwrap();
    let before = snapshot(&b.doc);
    monitor(&mut b.doc).cancel();
    assert_eq!(b.doc.try_undo(), Err(ModelError::Cancelled));
    assert_eq!(b.doc.undo(), None);
    let error = b.doc.command(r#"{"cmd": "undo"}"#).unwrap_err();
    assert_eq!(error.0, "the computation was cancelled");
    assert_eq!(snapshot(&b.doc), before);
    assert_eq!(value(&b.doc, "d3"), 35.0);

    b.doc.set_monitor(None);
    assert_eq!(b.doc.try_undo(), Ok(Some("Change d3".to_owned())));
    assert_eq!(value(&b.doc, "d3"), 20.0);
    let undone = snapshot(&b.doc);
    assert_eq!(undone.revision, initial);

    monitor(&mut b.doc).cancel();
    assert_eq!(b.doc.try_redo(), Err(ModelError::Cancelled));
    assert_eq!(b.doc.redo(), None);
    assert_eq!(snapshot(&b.doc), undone);

    // A monitor that is not cancelled changes nothing.
    monitor(&mut b.doc);
    assert_eq!(b.doc.try_redo(), Ok(Some("Change d3".to_owned())));
    assert_eq!(value(&b.doc, "d3"), 35.0);
    assert_eq!(b.doc.revision(), before.revision);
    // Nothing to undo is no error.
    let mut empty = Document::new(MockKernel::default());
    monitor(&mut empty).cancel();
    assert_eq!(empty.try_undo(), Ok(None));
}

#[test]
fn a_cancelled_preview_leaves_none() {
    let mut b = block();
    let edited = extrude(b.sketch, &[&b.region], 25.0, "new_body", &[]);
    b.doc.preview_edit(b.extrude, &edited).unwrap();
    assert!(b.doc.preview_body(b.body).is_some());
    let before = snapshot(&b.doc);

    monitor(&mut b.doc).cancel();
    let edited = extrude(b.sketch, &[&b.region], 30.0, "new_body", &[]);
    assert_eq!(
        b.doc.preview_edit(b.extrude, &edited),
        Err(ModelError::Cancelled)
    );
    assert!(b.doc.preview_body(b.body).is_none());
    assert!(b.doc.preview_feature().is_none());
    let json = serde_json::json!({"cmd": "edit_feature", "uid": b.extrude, "def": {
        "type": "extrude", "profiles": [{"sketch": b.sketch, "region": b.region}],
        "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}});
    let error = b.doc.preview(&json.to_string()).unwrap_err();
    assert_eq!(error.0, "the computation was cancelled");
    assert_eq!(snapshot(&b.doc), before);

    // The command itself computes again.
    b.doc.set_monitor(None);
    b.doc.edit_feature(b.extrude, &edited).unwrap();
    assert_eq!(value(&b.doc, "d3"), 30.0);
}

#[test]
fn a_cancelled_command_array_keeps_the_commands_before() {
    let mut b = block();
    // The first command evaluates Extrude1, its last feature; the cancel
    // stops the second.
    monitor(&mut b.doc).cancel_after(1);
    let error = b
        .doc
        .command(
            r#"[{"cmd": "set_parameter", "name": "d3", "value": 35},
                {"cmd": "set_parameter", "name": "d1", "value": 80}]"#,
        )
        .unwrap_err();
    assert_eq!(error.0, "commands[1]: the computation was cancelled");
    assert_eq!(value(&b.doc, "d3"), 35.0);
    assert_eq!(value(&b.doc, "d1"), 60.0);
    assert_eq!(b.doc.undo_label(), Some("Change d3"));
}

#[test]
fn a_cancelled_link_update_opens_nothing() {
    let dir = std::env::temp_dir().join(format!("mitcad-p7-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut part = block();
    let path = dir.join("part.mitcad");
    std::fs::write(&path, part.doc.to_json()).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let options = InsertOptions {
        base: Some(dir.clone()),
        ..InsertOptions::default()
    };
    doc.insert_component("part.mitcad", &options).unwrap();
    let saved = doc.to_json();
    part.doc.set_parameter("d3", 30.0).unwrap();
    std::fs::write(&path, part.doc.to_json()).unwrap();

    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    monitor(&mut reopened).cancel();
    let before = snapshot(&reopened);
    let warnings = reopened.load_warnings().to_vec();
    assert_eq!(
        reopened.update_links(Some(&dir)),
        Err(ModelError::Cancelled)
    );
    assert_eq!(snapshot(&reopened), before);
    assert_eq!(reopened.load_warnings(), warnings);

    reopened.set_monitor(None);
    let messages = reopened.update_links(Some(&dir)).unwrap();
    assert_eq!(messages, ["part is updated from part.mitcad"]);
    assert_ne!(reopened.revision(), before.revision);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_test_delay_slows_each_evaluation() {
    let mut b = block();
    monitor(&mut b.doc).set_test_delay(30);
    let start = Instant::now();
    b.doc.set_parameter("d3", 35.0).unwrap();
    assert!(start.elapsed() >= Duration::from_millis(30));
    assert_eq!(b.doc.stats().evaluated, vec![b.extrude]);
}

#[test]
fn a_cancel_from_another_thread_cuts_the_delay_short() {
    let mut b = block();
    let monitor = monitor(&mut b.doc);
    monitor.set_test_delay(60_000);
    let start = Instant::now();
    let limit = Duration::from_secs(30);
    let result = std::thread::scope(|scope| {
        let doc = &mut b.doc;
        let job = scope.spawn(move || doc.set_parameter("d3", 35.0));
        // Extrude1 is being evaluated: cancel it.
        while monitor.progress().feature != "Extrude1" && start.elapsed() < limit {
            std::thread::sleep(Duration::from_millis(1));
        }
        monitor.cancel();
        job.join().unwrap()
    });
    assert_eq!(result, Err(ModelError::Cancelled));
    assert!(start.elapsed() < limit, "{:?}", start.elapsed());
    assert_eq!(value(&b.doc, "d3"), 20.0);
    // Extrude1 returned after the cancel, so it is evaluated again.
    b.doc.set_monitor(None);
    b.doc.set_parameter("d3", 35.0).unwrap();
    assert_eq!(b.doc.stats().evaluated, vec![b.extrude]);
}

#[test]
fn a_monitor_within_others_stops_with_them_or_at_its_deadline() {
    let job = Arc::new(RecomputeMonitor::new());
    let other = Arc::new(RecomputeMonitor::new());
    let part = RecomputeMonitor::within(vec![job.clone(), other.clone()], None);
    assert!(!part.is_cancelled());
    other.cancel();
    assert!(part.is_cancelled());
    assert!(!job.is_cancelled());
    // A deadline that passed cancels it, not its parents.
    let late = RecomputeMonitor::within(vec![job.clone()], Some(Instant::now()));
    assert!(late.is_cancelled() && late.past_deadline() && late.progress().cancelled);
    assert!(!job.is_cancelled());
    let early =
        RecomputeMonitor::within(Vec::new(), Some(Instant::now() + Duration::from_secs(60)));
    assert!(!early.is_cancelled() && !early.past_deadline());

    // A kernel operation of an evaluation stops once the deadline passes.
    let (mut b, _) = joined();
    b.doc.kernel().stop_on_cancel.set(true);
    let part = Arc::new(RecomputeMonitor::within(
        vec![job.clone()],
        Some(Instant::now()),
    ));
    b.doc.set_monitor(Some(part));
    let before = snapshot(&b.doc);
    assert_eq!(b.doc.set_parameter("d3", 35.0), Err(ModelError::Cancelled));
    assert_eq!(snapshot(&b.doc), before);
    // It takes the test delay of its parents.
    job.set_test_delay(30);
    let part = Arc::new(RecomputeMonitor::within(vec![job.clone()], None));
    b.doc.set_monitor(Some(part));
    let start = Instant::now();
    b.doc.set_parameter("d3", 35.0).unwrap();
    assert!(start.elapsed() >= Duration::from_millis(30));
    assert_eq!(value(&b.doc, "d3"), 35.0);
}
