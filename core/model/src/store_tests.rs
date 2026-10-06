// SPDX-License-Identifier: MIT
//! The result store (P7d), with the mock kernel: results written after a
//! recompute are found by a new document of the same design, only what
//! changed is evaluated, and damaged files, files of another build, quick
//! evaluations and results of renamed parameters are left out.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use serde_json::json;

use crate::document_tests::{Block, block, corner, def, extrude, num};
use crate::features::SketchPlane;
use crate::ids::FeatureUid;
use crate::store::testing::{set_time, store_files};
use crate::store::{self, ResultStore};
use crate::testing::MockKernel;
use crate::{Document, RecomputeMonitor};

const BUILD: &str = "test build";

/// A store folder of its own, removed afterwards.
struct Folder(PathBuf);

impl Folder {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mitcad-store-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }

    fn store(&self) -> ResultStore {
        ResultStore::new(&self.0, BUILD).with_label("part.mitcad")
    }

    fn files(&self) -> Vec<(PathBuf, u64)> {
        store_files(&self.0)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Block (F1, F2 with d3), a boss joined to it (F3, F4 with d6) and a
/// fillet of a corner (F5, d7): three results the store takes.
struct Part {
    b: Block,
    boss: FeatureUid,
    fillet: FeatureUid,
}

fn part() -> Part {
    let mut b = block();
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_rectangle(sketch, [10.0, 10.0], &num(20.0), &num(10.0))
        .unwrap()
        .region;
    let join = extrude(sketch, &[&region], 30.0, "join", &[b.body]);
    let boss = b.doc.add_feature(&join, None).unwrap().uid;
    let edge = corner(b.extrude, 1, 0);
    let fillet = def(json!({"type": "fillet", "body": b.body, "edges": [edge], "radius": 2.0}));
    let fillet = b.doc.add_feature(&fillet, None).unwrap().uid;
    Part { b, boss, fillet }
}

/// The project read again with a store, as when the design is opened.
fn open(json: &str, store: Option<ResultStore>) -> Document<MockKernel> {
    let mut doc = Document::from_json(json, MockKernel::default()).unwrap();
    doc.set_result_store(store);
    doc.recompute();
    doc
}

fn histories(doc: &Document<MockKernel>) -> Vec<String> {
    doc.bodies()
        .iter()
        .map(|b| format!("{} {}", b.uid, b.shape.history))
        .collect()
}

#[test]
fn a_new_document_takes_the_stored_results() {
    let folder = Folder::new();
    let mut p = part();
    p.b.doc.set_result_store(Some(folder.store()));
    let report = p.b.doc.persist_results(Duration::ZERO);
    // The extrusions and the fillet, not the sketches; the bodies and the
    // extrusions' tools.
    assert_eq!((report.results, report.failed), (3, 0));
    assert!(report.shapes >= 5, "{report:?}");
    assert_eq!(folder.files().len(), 3);
    assert_eq!(
        report.bytes,
        folder.files().iter().map(|(_, size)| size).sum::<u64>()
    );
    // Nothing new to write.
    assert_eq!(p.b.doc.persist_results(Duration::ZERO).results, 0);

    let json = p.b.doc.to_json();
    let opened = open(&json, Some(folder.store()));
    let stats = opened.stats();
    assert_eq!(stats.restored, vec![p.b.extrude, p.boss, p.fillet]);
    assert_eq!(stats.evaluated, vec![p.b.sketch, FeatureUid(3)]);
    assert_eq!(stats.restore_times.len(), 3);
    assert_eq!(opened.kernel().count("extrude"), 0);
    assert_eq!(opened.kernel().count("fillet"), 0);
    assert_eq!(opened.kernel().count("shape_from_bytes"), report.shapes);
    assert_eq!(histories(&opened), histories(&p.b.doc));
    assert_eq!(opened.versions(), p.b.doc.versions());
    // What was read is in the memory cache now, and not written again.
    let mut opened = opened;
    opened.set_parameter("d7", 3.0).unwrap();
    opened.undo();
    assert!(opened.stats().evaluated.is_empty());
    assert!(opened.stats().restored.is_empty());
    assert_eq!(opened.persist_results(Duration::ZERO).results, 0);

    // The command's answer says so.
    let mut command = Document::from_json(&json, MockKernel::default()).unwrap();
    command.set_result_store(Some(folder.store()));
    let answer: serde_json::Value =
        serde_json::from_str(&command.command(r#"{"cmd": "recompute"}"#).unwrap()).unwrap();
    assert_eq!(answer["recomputed"], 2);
    assert_eq!(answer["restored"], 3);
}

#[test]
fn a_change_evaluates_what_follows_it() {
    let folder = Folder::new();
    let mut p = part();
    p.b.doc.set_result_store(Some(folder.store()));
    p.b.doc.persist_results(Duration::ZERO);
    // The boss higher: the block comes from the store, the boss and the
    // fillet after it are evaluated.
    p.b.doc.set_parameter("d6", 35.0).unwrap();
    let mut changed = open(&p.b.doc.to_json(), Some(folder.store()));
    assert_eq!(changed.stats().restored, vec![p.b.extrude]);
    assert_eq!(
        changed.stats().evaluated,
        vec![p.b.sketch, FeatureUid(3), p.boss, p.fillet]
    );
    assert_eq!(histories(&changed), histories(&p.b.doc));
    // Stored too, the file of each holds both variants.
    assert_eq!(changed.persist_results(Duration::ZERO).results, 2);
    assert_eq!(folder.files().len(), 3);
    p.b.doc.undo();
    let original = open(&p.b.doc.to_json(), Some(folder.store()));
    assert_eq!(original.stats().restored.len(), 3);
    let again = open(&changed.to_json(), Some(folder.store()));
    assert_eq!(again.stats().restored.len(), 3);
    assert_eq!(histories(&again), histories(&changed));
}

#[test]
fn damaged_files_and_other_builds_are_passed_over() {
    let folder = Folder::new();
    let mut p = part();
    p.b.doc.set_result_store(Some(folder.store()));
    p.b.doc.persist_results(Duration::ZERO);
    let json = p.b.doc.to_json();
    let before = store::stats();

    // Another build's results are not used; storing again replaces them.
    let other = ResultStore::new(&folder.0, "another build");
    let mut doc = open(&json, Some(other.clone()));
    assert!(doc.stats().restored.is_empty());
    assert_eq!(doc.stats().evaluated.len(), 5);
    assert_eq!(doc.persist_results(Duration::ZERO).results, 3);
    assert_eq!(folder.files().len(), 3);
    assert_eq!(open(&json, Some(other)).stats().restored.len(), 3);
    assert!(
        open(&json, Some(folder.store()))
            .stats()
            .restored
            .is_empty()
    );
    assert!(store::stats().other_build > before.other_build);

    // Back to this build's, then each file damaged a different way: each
    // is removed when read, and the feature evaluated.
    let mut doc = open(&json, Some(folder.store()));
    doc.persist_results(Duration::ZERO);
    let files = folder.files();
    assert_eq!(files.len(), 3);
    let damage: [&dyn Fn(&Path); 3] = [
        &|path| std::fs::write(path, b"not a store file").unwrap(),
        &|path| {
            let mut bytes = std::fs::read(path).unwrap();
            let last = bytes.len() - 1;
            bytes[last] ^= 0xff;
            std::fs::write(path, bytes).unwrap();
        },
        &|path| {
            let bytes = std::fs::read(path).unwrap();
            std::fs::write(path, &bytes[..bytes.len() / 2]).unwrap();
        },
    ];
    for ((path, _), damage) in files.iter().zip(damage) {
        damage(path);
    }
    let damaged = store::stats().damaged;
    let doc = open(&json, Some(folder.store()));
    assert!(doc.stats().restored.is_empty());
    assert_eq!(doc.stats().evaluated.len(), 5);
    assert_eq!(histories(&doc), histories(&p.b.doc));
    assert!(folder.files().is_empty());
    assert!(store::stats().damaged >= damaged + 3);
}

#[test]
fn a_shape_the_kernel_cannot_read_is_evaluated() {
    let folder = Folder::new();
    let mut p = part();
    p.b.doc.set_result_store(Some(folder.store()));
    p.b.doc.persist_results(Duration::ZERO);
    let mut doc = Document::from_json(&p.b.doc.to_json(), MockKernel::default()).unwrap();
    doc.kernel().fail.borrow_mut().insert("shape_from_bytes");
    doc.set_result_store(Some(folder.store()));
    doc.recompute();
    assert!(doc.stats().restored.is_empty());
    assert_eq!(histories(&doc), histories(&p.b.doc));
    // And one that cannot be written is reported, not stored.
    let folder = Folder::new();
    let mut p = part();
    p.b.doc.kernel().fail.borrow_mut().insert("shape_bytes");
    p.b.doc.set_result_store(Some(folder.store()));
    let report = p.b.doc.persist_results(Duration::ZERO);
    assert_eq!((report.results, report.failed), (0, 3));
    assert!(
        report.errors[0].contains("shape_bytes failed"),
        "{report:?}"
    );
    assert!(folder.files().is_empty());
}

#[test]
fn quick_failed_and_renamed_results_are_not_written() {
    let folder = Folder::new();
    let mut p = part();
    p.b.doc.set_result_store(Some(folder.store()));
    // The mock evaluates at once: nothing takes an hour.
    assert_eq!(
        p.b.doc.persist_results(Duration::from_secs(3600)).results,
        0
    );
    // A renamed parameter: the results it was evaluated with hold its old
    // name. The fillet's radius, d7, is renamed; its result stays in the
    // cache (the rename recomputes nothing) but is not written.
    p.b.doc.rename_parameter("d7", "radius").unwrap();
    assert_eq!(p.b.doc.persist_results(Duration::ZERO).results, 2);
    // A failed feature is not written either.
    let folder = Folder::new();
    let mut q = part();
    q.b.doc.kernel().fail.borrow_mut().insert("fillet");
    q.b.doc.set_parameter("d7", 2.5).unwrap();
    assert!(!q.b.doc.status(q.fillet).unwrap().is_ok());
    q.b.doc.set_result_store(Some(folder.store()));
    assert_eq!(q.b.doc.persist_results(Duration::ZERO).results, 2);
}

#[test]
fn restored_results_are_not_counted_as_evaluated() {
    let folder = Folder::new();
    let mut p = part();
    p.b.doc.set_result_store(Some(folder.store()));
    p.b.doc.persist_results(Duration::ZERO);
    let mut doc = Document::from_json(&p.b.doc.to_json(), MockKernel::default()).unwrap();
    doc.set_result_store(Some(folder.store()));
    let monitor = Arc::new(RecomputeMonitor::new());
    doc.set_monitor(Some(monitor.clone()));
    doc.try_recompute().unwrap();
    assert_eq!(monitor.progress().evaluated, 2);
    assert_eq!(monitor.progress().position, 5);
}

#[test]
fn gc_removes_the_files_used_longest_ago() {
    let folder = Folder::new();
    let mut p = part();
    p.b.doc.set_result_store(Some(folder.store()));
    p.b.doc.persist_results(Duration::ZERO);
    let mut files = folder.files();
    files.sort();
    let total: u64 = files.iter().map(|(_, size)| size).sum();
    // Within the budget: nothing goes.
    let report = store::gc(&folder.0, total);
    assert_eq!((report.files, report.bytes, report.removed), (3, total, 0));
    // The oldest go until 80 % of the budget is used.
    let base = SystemTime::now() - Duration::from_secs(3600);
    for (i, (path, _)) in files.iter().enumerate() {
        set_time(path, base + Duration::from_secs(60 * i as u64));
    }
    let budget = total - 1;
    let report = store::gc(&folder.0, budget);
    assert!(report.removed >= 1);
    assert!(report.bytes as f64 <= budget as f64 * 0.8);
    let left: Vec<PathBuf> = folder.files().into_iter().map(|(path, _)| path).collect();
    assert!(!left.contains(&files[0].0));
    assert_eq!(report.files as usize, left.len());
    // A hit makes a file the newest: the newest stays, whichever it was.
    let doc = open(&p.b.doc.to_json(), Some(folder.store()));
    assert_eq!(doc.stats().restored.len(), left.len());
    // A temporary file left by a program that stopped goes too.
    let dir = files[0].0.parent().unwrap().to_owned();
    std::fs::create_dir_all(&dir).unwrap();
    let temporary = dir.join(".x.mrs.1.0.tmp");
    std::fs::write(&temporary, b"partial").unwrap();
    set_time(&temporary, base - Duration::from_secs(3600));
    store::gc(&folder.0, u64::MAX);
    assert!(!temporary.exists());
    // Clear removes everything.
    let report = store::clear(&folder.0);
    assert_eq!(report.removed as usize, left.len());
    assert!(folder.files().is_empty());
}

#[test]
fn without_a_store_nothing_is_written_or_read() {
    let mut p = part();
    assert_eq!(p.b.doc.persist_results(Duration::ZERO), Default::default());
    let doc = open(&p.b.doc.to_json(), None);
    assert!(doc.stats().restored.is_empty());
    assert_eq!(doc.kernel().count("shape_bytes"), 0);
}
