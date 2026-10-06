// SPDX-License-Identifier: MIT
//! Background computation (P7): the progress and cancel request of a job,
//! shared by the thread that computes a document and the thread that shows
//! the progress.
//!
//! The application attaches a new [`JobControl`] to the document before it
//! hands the document to its worker thread, and detaches it when the job is
//! back; every recompute of the job reports to it and stops between two
//! features once it is cancelled, and the geometry's long operations stop
//! inside the feature being evaluated (P7e, `kernel/cancel.rs`). A
//! cancelled command, undo, redo, preview or `update_links` fails with "the
//! computation was cancelled" and changes nothing
//! (`core/model/src/api/commands.md`, "Progress and cancellation"); a
//! cancelled `import_f3d` stops early and keeps what it imported (T1e).

use std::sync::Arc;

use mitcad_model::RecomputeMonitor;

use crate::Document;
use crate::ffi::JobProgress;

/// A job's [`RecomputeMonitor`]. It is `Sync` (atomics and a mutex), so
/// both threads may use it through `&JobControl` at once. A cancelled
/// control stays cancelled: every job takes a new one.
pub struct JobControl(Arc<RecomputeMonitor>);

pub fn new_job_control() -> Box<JobControl> {
    Box::new(JobControl(Arc::new(RecomputeMonitor::new())))
}

impl JobControl {
    /// Asks the job to stop: its recompute stops before the next feature,
    /// or when the evaluation running now returns, which the geometry's
    /// long operations cut short (P7e).
    pub fn cancel(&self) {
        self.0.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    pub fn progress(&self) -> JobProgress {
        let progress = self.0.progress();
        JobProgress {
            position: progress.position,
            total: progress.total,
            evaluated: progress.evaluated,
            feature: progress.feature,
            cancelled: progress.cancelled,
        }
    }

    /// For tests: every evaluation of the job takes `ms` longer.
    pub fn set_test_delay(&self, ms: u32) {
        self.0.set_test_delay(ms);
    }
}

impl Document {
    /// The document's recomputes report to `control` until
    /// [`Document::detach_job`].
    pub(crate) fn attach_job(&mut self, control: &JobControl) {
        self.0.set_monitor(Some(Arc::clone(&control.0)));
    }

    pub(crate) fn detach_job(&mut self) {
        self.0.set_monitor(None);
    }

    /// Runs `f` without the job's monitor, attached again afterwards (to
    /// the document `f` leaves, which may be a new one). The imports of
    /// .f3d designs build their documents in several steps and are not
    /// undone as a whole when cancelled, so a job does not cancel them;
    /// the timeline import takes the job's cancel as a request to stop
    /// early instead (T1e, `f3d_import.rs`).
    pub(crate) fn without_job<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let monitor = self.0.monitor().cloned();
        self.0.set_monitor(None);
        let result = f(self);
        self.0.set_monitor(monitor);
        result
    }
}
