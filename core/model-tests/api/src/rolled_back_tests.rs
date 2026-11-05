// SPDX-License-Identifier: MIT
//! The rolled-back features evaluated beside the document and adopted by it
//! (mitcad#1267, `core/model/src/document/rolled_back.rs`), with the mock
//! kernel: after an edit before the marker, a run evaluated to the end and
//! adopted leaves nothing for the next marker move to evaluate; a run of a
//! state the document no longer has, a cancelled one and one over the
//! memory budget adopt nothing.

use std::sync::Arc;

use mitcad_model::RecomputeMonitor;
use mitcad_model::ids::FeatureUid;
use mitcad_model::test_support::{Block, block};

/// The block (F1 Sketch1, F2 Extrude1: Body1) with a boss joined to it (F3
/// Sketch2, F4 Extrude2), the marker after the block: the boss rolled
/// back. Then a circle joined to Body1 before the marker (F5, F6), so the
/// boss's join has a new input.
fn edited_before_marker() -> (Block, FeatureUid, FeatureUid) {
    let mut b = block();
    let (sketch, _, boss) = b.join_boss();
    b.doc.set_marker(2).unwrap();
    assert!(b.evaluated().is_empty(), "rolled back: nothing to evaluate");
    let body = b.body;
    b.circle_extrude("join", &[body]);
    assert_eq!(b.doc.marker(), 4, "the circle went in before the marker");
    (b, sketch, boss)
}

fn end(b: &Block) -> usize {
    b.doc.features().count()
}

#[test]
fn an_adopted_run_leaves_nothing_for_the_marker_move() {
    let (mut b, sketch, boss) = edited_before_marker();
    let mut run = b
        .doc
        .rolled_back_run(None)
        .expect("features after the marker");
    assert_eq!(run.rolled_back(), &[sketch, boss]);
    // On a thread of its own, as the application runs it.
    let finished = std::thread::scope(|s| s.spawn(|| run.run()).join().unwrap());
    assert!(finished, "the run went to the end");
    assert!(run.finished());
    let evaluated = run.evaluated();
    assert!(evaluated >= 1, "the boss's join has a new input: evaluated");
    assert_eq!(
        b.doc.adopt_rolled_back(run),
        evaluated,
        "each evaluation adopted"
    );
    let at = end(&b);
    b.doc.set_marker(at).unwrap();
    assert!(
        b.evaluated().is_empty(),
        "the marker move evaluated {:?}",
        b.evaluated()
    );
    assert!(b.status(boss).is_ok(), "{:?}", b.status(boss));
}

#[test]
fn without_a_run_the_marker_move_evaluates_the_rolled_back_features() {
    let (mut b, _, boss) = edited_before_marker();
    let at = end(&b);
    b.doc.set_marker(at).unwrap();
    assert!(b.evaluated().contains(&boss), "{:?}", b.evaluated());
}

#[test]
fn a_run_made_before_a_further_edit_adopts_nothing() {
    let (mut b, _, boss) = edited_before_marker();
    let mut run = b
        .doc
        .rolled_back_run(None)
        .expect("features after the marker");
    assert!(run.run());
    assert!(run.evaluated() >= 1);
    // Another join before the marker: the run's inputs are no longer the
    // document's.
    let body = b.body;
    b.circle_extrude("join", &[body]);
    assert_eq!(
        b.doc.adopt_rolled_back(run),
        0,
        "a stale run adopts nothing"
    );
    let at = end(&b);
    b.doc.set_marker(at).unwrap();
    assert!(
        b.evaluated().contains(&boss),
        "the edited state's boss is evaluated: {:?}",
        b.evaluated()
    );
}

#[test]
fn a_cancelled_run_adopts_nothing_it_did_not_evaluate() {
    let (mut b, _, boss) = edited_before_marker();
    let monitor = Arc::new(RecomputeMonitor::new());
    let mut run = b
        .doc
        .rolled_back_run(Some(Arc::clone(&monitor)))
        .expect("features after the marker");
    monitor.cancel();
    assert!(!run.run(), "cancelled");
    assert!(!run.finished());
    assert_eq!(run.evaluated(), 0);
    assert_eq!(b.doc.adopt_rolled_back(run), 0);
    let at = end(&b);
    b.doc.set_marker(at).unwrap();
    assert!(b.evaluated().contains(&boss), "{:?}", b.evaluated());
}

#[test]
fn a_run_over_the_memory_budget_adopts_only_what_fits() {
    let (mut b, _, boss) = edited_before_marker();
    // A budget the document's own results already fill: no room.
    b.doc.set_memory_budget(Some(1));
    let mut run = b
        .doc
        .rolled_back_run(None)
        .expect("features after the marker");
    assert!(run.run());
    assert!(run.evaluated() >= 1);
    assert_eq!(b.doc.adopt_rolled_back(run), 0, "nothing fits the budget");
    let at = end(&b);
    b.doc.set_marker(at).unwrap();
    assert!(b.evaluated().contains(&boss), "{:?}", b.evaluated());
}

#[test]
fn nothing_after_the_marker_needs_no_run() {
    let mut b = block();
    b.join_boss();
    assert!(
        b.doc.rolled_back_run(None).is_none(),
        "the marker at the end"
    );
    b.doc.set_marker(2).unwrap();
    let run = b.doc.rolled_back_run(None).expect("the boss rolled back");
    assert_eq!(run.base_revision(), b.doc.revision());
}
