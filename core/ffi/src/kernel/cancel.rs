// SPDX-License-Identifier: MIT
//! Long kernel operations stopped on request (P7e,
//! `geometry/include/mitcad/geometry/cancel.hpp`): while a recompute with a
//! monitor evaluates a feature, the geometry asks the monitor whether to
//! stop, inside OCCT's algorithms too, and a stopped operation fails.

use std::cell::RefCell;
use std::sync::Arc;

use mitcad_model::RecomputeMonitor;

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    extern "Rust" {
        /// What the geometry asks whether to stop: a recompute's monitor.
        /// It is asked from OCCT's worker threads too.
        type CancelCheck;
        fn cancel_requested(self: &CancelCheck) -> bool;
        /// The progress of an OCCT algorithm it stops advanced (from its
        /// worker threads too): progress of the .f3d import that runs the
        /// recompute, for its watchdog (mitcad#82).
        fn progressed(self: &CancelCheck);
    }

    unsafe extern "C++" {
        include!("bridge/cancel.hpp");
        include!("bridge/trycatch.hpp");

        /// While it lives, the geometry's operations on the thread that
        /// made it stop when its check asks.
        type CancelScope;

        fn enter_cancel_scope(check: Box<CancelCheck>) -> UniquePtr<CancelScope>;
    }
}

/// A recompute's monitor as the geometry asks it (it is `Sync`), with the
/// progress of the .f3d import running on the thread that made it, if any.
pub struct CancelCheck(Arc<RecomputeMonitor>, Option<Arc<mitcad_import::Progress>>);

impl CancelCheck {
    fn cancel_requested(&self) -> bool {
        self.0.is_cancelled()
    }

    fn progressed(&self) {
        if let Some(progress) = &self.1 {
            progress.tick();
        }
    }
}

/// Runs `f` with the geometry's operations on this thread stopping once
/// `monitor` is cancelled (`Kernel::interruptible`); their progress is the
/// progress of the .f3d import running on this thread (mitcad#82: a long
/// boolean that advances is not stuck).
pub fn interruptible<R>(monitor: &Arc<RecomputeMonitor>, f: impl FnOnce() -> R) -> R {
    interruptible_with(monitor, mitcad_import::current_progress(), f)
}

/// [`interruptible`] with the progress given: on a thread of its own that
/// runs a kernel call for an evaluation's thread (mitcad#1917,
/// `kernel/abandon.rs`), the progress of the import running there.
pub(crate) fn interruptible_with<R>(
    monitor: &Arc<RecomputeMonitor>,
    progress: Option<Arc<mitcad_import::Progress>>,
    f: impl FnOnce() -> R,
) -> R {
    let check = CancelCheck(Arc::clone(monitor), progress.clone());
    let _scope = ffi::enter_cancel_scope(Box::new(check));
    let _current = Current::enter(Arc::clone(monitor), progress);
    f()
}

/// The monitor this thread's kernel calls stop with, and the progress of
/// the import running here, while [`interruptible`] runs (mitcad#1917: a
/// kernel call run on a thread of its own takes them there).
pub(crate) type Stopping = (Arc<RecomputeMonitor>, Option<Arc<mitcad_import::Progress>>);

thread_local! {
    static CURRENT: RefCell<Vec<Stopping>> = const { RefCell::new(Vec::new()) };
}

/// What stops this thread's kernel calls now ([`interruptible`]), if
/// anything does.
pub(crate) fn current() -> Option<Stopping> {
    CURRENT.with(|c| c.borrow().last().cloned())
}

/// Names the monitor of [`current`] until dropped.
struct Current;

impl Current {
    fn enter(
        monitor: Arc<RecomputeMonitor>,
        progress: Option<Arc<mitcad_import::Progress>>,
    ) -> Self {
        CURRENT.with(|c| c.borrow_mut().push((monitor, progress)));
        Self
    }
}

impl Drop for Current {
    fn drop(&mut self) {
        CURRENT.with(|c| {
            c.borrow_mut().pop();
        });
    }
}
