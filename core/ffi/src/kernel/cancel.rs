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
    }

    unsafe extern "C++" {
        include!("bridge/cancel.hpp");

        /// While it lives, the geometry's operations on the thread that
        /// made it stop when its check asks.
        type CancelScope;

        fn enter_cancel_scope(check: Box<CancelCheck>) -> UniquePtr<CancelScope>;
    }
}

/// A recompute's monitor as the geometry asks it (it is `Sync`).
pub struct CancelCheck(Arc<RecomputeMonitor>);

impl CancelCheck {
    fn cancel_requested(&self) -> bool {
        self.0.is_cancelled()
    }
}

/// Runs `f` with the geometry's operations on this thread stopping once
/// `monitor` is cancelled (`Kernel::interruptible`).
pub fn interruptible<R>(monitor: &Arc<RecomputeMonitor>, f: impl FnOnce() -> R) -> R {
    let _scope = ffi::enter_cancel_scope(Box::new(CancelCheck(Arc::clone(monitor))));
    f()
}
