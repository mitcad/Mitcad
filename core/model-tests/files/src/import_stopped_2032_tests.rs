// SPDX-License-Identifier: MIT
//! A file's import stopped inside its read (mitcad#2032), with the mock
//! kernel, whose read runs until the monitor it was given is cancelled (a
//! long STEP transfer, `MockKernel::read_waits_for_cancel`): the memory
//! guard of model commands stops `import_file` during the transfer with
//! the import's own words, the user's cancel stops it as cancelled, and
//! the design stays as it was before the command.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use mitcad_model::exchange::ImportBody;
use mitcad_model::monitor::memory_guard::{self, GuardedWork, MemoryReading};
use mitcad_model::test_support::block;
use mitcad_model::testing::{MockKernel, MockShape};
use mitcad_model::{Cancelled, Document, RecomputeMonitor};
use mitcad_model_api::DocumentApi;

/// A STEP file of one body at `path`, read slowly: until the read's
/// monitor is cancelled.
fn slow_file(doc: &Document<MockKernel>, path: &str) {
    doc.kernel().files.borrow_mut().insert(
        path.to_owned(),
        vec![ImportBody {
            name: Some("Bracket".to_owned()),
            color: None,
            shape: MockShape::imported("bracket", 6),
        }],
    );
    doc.kernel().read_waits_for_cancel.set(true);
}

/// What a stopped import must leave as it was.
#[derive(Debug, PartialEq)]
struct State {
    json: String,
    revision: u64,
    undo_depth: usize,
    undo: Option<String>,
    bodies: usize,
}

fn state(doc: &Document<MockKernel>) -> State {
    State {
        json: doc.to_json(),
        revision: doc.revision(),
        undo_depth: doc.undo_depth(),
        undo: doc.undo_label().map(str::to_owned),
        bodies: doc.bodies().len(),
    }
}

fn import(doc: &mut Document<MockKernel>, path: &str) -> Result<String, String> {
    doc.command(&json!({"cmd": "import_file", "path": path}).to_string())
        .map_err(|e| e.to_string())
}

/// The memory as the guard reads it: enough before the command, then 90 %
/// of the limit while the import's read runs.
fn rising_memory() -> impl FnMut() -> MemoryReading + Send + 'static {
    let mut readings = 0;
    move || {
        readings += 1;
        let share = if readings > 1 { 0.9 } else { 0.2 };
        MemoryReading {
            share,
            text: format!("{share:.1} GiB of 1.0 GiB (memory the system has left)"),
            failed_allocations: 0,
        }
    }
}

/// The memory guard stops the import while its file is transferred: the
/// read, run under the command's monitor, stops on the guard's cancel, and
/// the command fails with the import's words (no advice to join bodies);
/// nothing of the file is in the design.
#[test]
fn memory_guard_stops_the_import_during_the_transfer() {
    let mut b = block();
    slow_file(&b.doc, "bracket.step");
    let monitor = Arc::new(RecomputeMonitor::new());
    b.doc.set_monitor(Some(Arc::clone(&monitor)));
    let before = state(&b.doc);
    let interruptible = b.doc.kernel().count("interruptible");

    let doc = &mut b.doc;
    let result = memory_guard::guarded_for(&monitor, GuardedWork::Import, rising_memory(), || {
        import(doc, "bracket.step")
    });

    let error = result.expect_err("the guard stopped the import");
    assert!(
        error.starts_with(
            "the file could not be imported because it needs more memory than the system has left"
        ),
        "{error}"
    );
    assert!(error.contains("as it was before the import"), "{error}");
    assert!(!error.contains("join the bodies"), "{error}");
    assert!(!error.contains("smaller steps"), "{error}");
    // The read ran under the monitor, which stopped it inside.
    assert_eq!(b.doc.kernel().count("interruptible"), interruptible + 1);
    assert_eq!(state(&b.doc), before);
}

/// The user's cancel during the transfer stops the import as cancelled,
/// the design as it was.
#[test]
fn users_cancel_stops_the_import_during_the_transfer() {
    let mut b = block();
    slow_file(&b.doc, "bracket.step");
    let monitor = Arc::new(RecomputeMonitor::new());
    b.doc.set_monitor(Some(Arc::clone(&monitor)));
    let before = state(&b.doc);

    let user = Arc::clone(&monitor);
    let cancel = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        user.cancel();
    });
    let error = import(&mut b.doc, "bracket.step").expect_err("the user cancelled the import");
    cancel.join().unwrap();

    assert_eq!(error, Cancelled::MESSAGE);
    assert_eq!(state(&b.doc), before);
}

/// A monitor cancelled before the command starts stops the import before
/// its read; one that is never cancelled lets it run to its end.
#[test]
fn import_under_a_monitor() {
    let mut b = block();
    slow_file(&b.doc, "bracket.step");
    let monitor = Arc::new(RecomputeMonitor::new());
    b.doc.set_monitor(Some(Arc::clone(&monitor)));
    monitor.cancel_because("stopped before it began");
    let before = state(&b.doc);
    let interruptible = b.doc.kernel().count("interruptible");
    let error = import(&mut b.doc, "bracket.step").expect_err("cancelled already");
    assert_eq!(error, "stopped before it began");
    assert_eq!(b.doc.kernel().count("interruptible"), interruptible);
    assert_eq!(state(&b.doc), before);

    // A read that does not wait imports the file under a fresh monitor.
    b.doc.kernel().read_waits_for_cancel.set(false);
    b.doc.set_monitor(Some(Arc::new(RecomputeMonitor::new())));
    import(&mut b.doc, "bracket.step").expect("imported");
    assert_ne!(state(&b.doc), before);
}
