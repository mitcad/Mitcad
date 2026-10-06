// SPDX-License-Identifier: MIT
//! Progress of a recompute and a request to stop it (P7).
//!
//! A [`RecomputeMonitor`] is shared between the thread that computes and
//! the one that shows the progress: the recompute reports the timeline
//! position and the feature it evaluates, and checks for a cancel request
//! before each feature. A cancelled recompute returns [`Cancelled`], and
//! the command that ran it is rejected as a whole (see
//! `Document::set_monitor`). Results evaluated before the request stay in
//! the cache, so trying again continues from there; an evaluation that
//! ends after the request is dropped. The kernel may ask the monitor too,
//! and stop its long operations inside an evaluation (P7e,
//! `Kernel::interruptible`).

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

/// Progress of a recompute and a request to stop it; shared between the
/// thread that computes and the one that shows the progress.
///
/// The counts are for one job: a monitor starts a job unused, and the
/// evaluations of every recompute it is given add up (a command and the
/// linked designs it reads). Once cancelled it stays cancelled, so a new
/// job takes a new monitor.
#[derive(Debug)]
pub struct RecomputeMonitor {
    cancel: AtomicBool,
    /// The timeline position being computed: the number of features before
    /// it; the total at the end of a recompute.
    position: AtomicUsize,
    /// The features before the marker.
    total: AtomicUsize,
    /// Evaluations (cache misses) kept so far in this job.
    evaluated: AtomicUsize,
    /// The name of the feature last evaluated; a cache hit leaves it.
    feature: Mutex<String>,
    /// Test hook: how long each evaluation is made to take longer.
    delay_ms: AtomicU32,
    /// Test hook: cancel once this many evaluations were kept.
    cancel_after: AtomicUsize,
}

/// A recompute stopped on request; nothing it was part of took effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl Cancelled {
    /// The message of a command rejected because it was cancelled.
    pub const MESSAGE: &'static str = "the computation was cancelled";
}

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(Self::MESSAGE)
    }
}

impl std::error::Error for Cancelled {}

/// A snapshot of a [`RecomputeMonitor`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    /// The features before the one being computed (shown as `position + 1`
    /// of `total`); `total` when the recompute is done.
    pub position: usize,
    /// The features before the timeline marker.
    pub total: usize,
    /// Evaluations kept so far in the job; cache hits are not counted.
    pub evaluated: usize,
    /// The feature being evaluated, or the last one; empty before any.
    pub feature: String,
    pub cancelled: bool,
}

/// The test delay sleeps in steps this long, checking for a cancel.
const DELAY_STEP: Duration = Duration::from_millis(10);

impl Default for RecomputeMonitor {
    fn default() -> Self {
        Self {
            cancel: AtomicBool::new(false),
            position: AtomicUsize::new(0),
            total: AtomicUsize::new(0),
            evaluated: AtomicUsize::new(0),
            feature: Mutex::new(String::new()),
            delay_ms: AtomicU32::new(0),
            cancel_after: AtomicUsize::new(usize::MAX),
        }
    }
}

impl RecomputeMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks the recompute to stop: it does before the next feature, or
    /// when the evaluation running now returns, which the kernel's long
    /// operations cut short (P7e, `Kernel::interruptible`).
    pub fn cancel(&self) {
        // Nothing is published with the flag, so no ordering is needed.
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn progress(&self) -> Progress {
        Progress {
            position: self.position.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
            evaluated: self.evaluated.load(Ordering::Relaxed),
            feature: self
                .feature
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
            cancelled: self.is_cancelled(),
        }
    }

    /// For tests: every evaluation takes `ms` longer, so that a test can
    /// watch the progress and cancel a recompute of a small model. The
    /// delay ends early on a cancel, and the evaluation is then dropped as
    /// one cancelled while it ran. The recompute times leave it out.
    pub fn set_test_delay(&self, ms: u32) {
        self.delay_ms.store(ms, Ordering::Relaxed);
    }

    /// For tests: cancel as soon as `evaluations` evaluations were kept in
    /// the cache, as a user who cancels between two features.
    pub fn cancel_after(&self, evaluations: usize) {
        self.cancel_after.store(evaluations, Ordering::Relaxed);
        if self.evaluated.load(Ordering::Relaxed) >= evaluations {
            self.cancel();
        }
    }

    /// Before the feature at `position` of the `total` before the marker:
    /// stops a cancelled recompute.
    pub(crate) fn enter(&self, position: usize, total: usize) -> Result<(), Cancelled> {
        self.position.store(position, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
        self.check()
    }

    /// A feature is about to be evaluated (a cache miss).
    pub(crate) fn evaluating(&self, name: &str) {
        let mut feature = self.feature.lock().unwrap_or_else(PoisonError::into_inner);
        feature.clear();
        feature.push_str(name);
    }

    /// An evaluation returned: after the test delay, tells whether it may
    /// be kept (no cancel came while it ran).
    pub(crate) fn evaluation_done(&self) -> Result<(), Cancelled> {
        let delay = Duration::from_millis(u64::from(self.delay_ms.load(Ordering::Relaxed)));
        let mut slept = Duration::ZERO;
        while slept < delay {
            self.check()?;
            let step = DELAY_STEP.min(delay - slept);
            thread::sleep(step);
            slept += step;
        }
        self.check()
    }

    /// An evaluation went into the cache.
    pub(crate) fn kept(&self) {
        let kept = self.evaluated.fetch_add(1, Ordering::Relaxed) + 1;
        if kept >= self.cancel_after.load(Ordering::Relaxed) {
            self.cancel();
        }
    }

    /// The recompute went through all `total` features before the marker.
    pub(crate) fn finish(&self, total: usize) {
        self.position.store(total, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
    }

    fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}
