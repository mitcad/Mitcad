// SPDX-License-Identifier: MIT
//! Long kernel operations stopped on request (P7e,
//! `geometry/include/mitcad/geometry/cancel.hpp`): while a recompute with a
//! monitor evaluates a feature, the geometry asks the monitor whether to
//! stop, inside OCCT's algorithms too, and a stopped operation fails.

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
    let check = CancelCheck(Arc::clone(monitor), mitcad_import::current_progress());
    let _scope = ffi::enter_cancel_scope(Box::new(check));
    f()
}
