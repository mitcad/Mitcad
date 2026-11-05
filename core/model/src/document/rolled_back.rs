// SPDX-License-Identifier: MIT
//! The rolled-back features evaluated beside the document (mitcad#1267).
//!
//! An edit before the timeline marker gives every rolled-back feature after
//! it new inputs, so none of their cached results fits any more, and moving
//! the marker forward evaluates each of them as the marker passes it. The
//! application therefore evaluates them in the background while the user
//! does nothing: [`Document::rolled_back_run`] makes a fork of the document
//! ([`Document::fork`]: the same definition and cached results, a kernel of
//! its own) with its marker at the end of the timeline, which
//! [`RolledBackRun::run`] recomputes on another thread, and
//! [`Document::adopt_rolled_back`] takes the results it evaluated into the
//! document's cache. The next marker move then finds them there, as it
//! finds the results of a marker moved back and forth.
//!
//! - **Never wrong, at most unused.** A cached result is taken only where
//!   everything it read holds (the cache's key, `recompute.rs`), so a
//!   result of a state the document no longer has is simply not taken.
//!   Adopting is still refused when the document's definition is not the
//!   one the run was made from (its marker aside): the results would only
//!   take memory.
//! - **Memory.** Adopted results count against the cache's budget like
//!   evaluated ones. Those of the features nearest the marker go first
//!   (the next ones the marker reaches), and adopting stops before the
//!   adopted results would take more than the budget leaves beside what
//!   the document's own results hold; the results used longest ago then
//!   make room for them ([`crate::recompute::Cache::trim`]). The fork holds
//!   every result it was made with, so nothing is dropped while it lives:
//!   it goes before the cache is trimmed.
//! - **Quiet.** A feature that fails in the run is not reported: its
//!   failure is not cached, and the user sees it when the marker gets
//!   there, as without the run.
//! - **Stopped at once.** The run reports to its own monitor, which the
//!   caller cancels as soon as the user does something (the kernel's long
//!   operations stop inside an evaluation too, P7e). The results it
//!   evaluated before the cancel are kept and may still be adopted.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{DocState, Document};
use crate::ids::FeatureUid;
use crate::kernel::Kernel;
use crate::monitor::RecomputeMonitor;

/// A fork of a document whose rolled-back features it evaluates, to be
/// adopted by the document ([`Document::rolled_back_run`]). It shares no
/// mutable state with the document: the definition and the cached results
/// are copies whose results are shared read-only (`Arc`), and its kernel is
/// a fork of the document's, so it may run on another thread while the
/// document is used on its own.
pub struct RolledBackRun<K: Kernel> {
    fork: Document<K>,
    /// The definition state the run was made from.
    base: DocState,
    /// The features after the marker that are not suppressed, in timeline
    /// order (the nearest the marker first).
    rolled_back: Vec<FeatureUid>,
    evaluated: usize,
    time: Duration,
    finished: bool,
}

impl<K: Kernel> Document<K> {
    /// A run that evaluates the features after the marker on a fork of the
    /// document, reporting to `monitor` (cancelled through it); None when
    /// no feature after the marker is to be evaluated (the marker at the end
    /// of the timeline, or only suppressed features after it) or the kernel
    /// cannot work on another thread ([`crate::kernel::Kernel::fork`]).
    pub fn rolled_back_run(
        &self,
        monitor: Option<Arc<RecomputeMonitor>>,
    ) -> Option<RolledBackRun<K>> {
        let rolled_back: Vec<FeatureUid> = self
            .state
            .features
            .get(self.state.marker..)?
            .iter()
            .filter(|f| !f.suppressed)
            .map(|f| f.uid)
            .collect();
        if rolled_back.is_empty() {
            return None;
        }
        let mut fork = self.fork()?;
        fork.state.marker = fork.state.features.len();
        fork.set_monitor(monitor);
        Some(RolledBackRun {
            fork,
            base: self.state.clone(),
            rolled_back,
            evaluated: 0,
            time: Duration::ZERO,
            finished: false,
        })
    }

    /// Takes into the cache the results `run` evaluated, as if they had
    /// been evaluated here, when the document's definition is still the one
    /// the run was made from (the marker may have moved); returns how many
    /// were taken. The nearest the marker go first, and no more than the
    /// memory budget leaves beside what the document's results hold (see
    /// the module comment). A run cancelled part way gives what it had
    /// evaluated by then.
    pub fn adopt_rolled_back(&mut self, run: RolledBackRun<K>) -> usize {
        if !same_definition(&run.base, &self.state) {
            return 0;
        }
        let room = self.cache.budget().map(|budget| {
            let preview = self.preview.iter().flat_map(|p| p.result.results.iter());
            let held = self
                .cache
                .held_bytes(self.result.results.iter().chain(preview));
            budget.saturating_sub(held)
        });
        let adopted = self
            .cache
            .adopt_new(&run.fork.cache, &run.rolled_back, room);
        // The fork holds every result it was made with: once it is gone the
        // cache can drop what its budget asks for.
        drop(run);
        self.cache.trim();
        adopted
    }
}

impl<K: Kernel> RolledBackRun<K> {
    /// Recomputes the fork to the end of its timeline; false when its
    /// monitor cancelled it. Failed features are left as they are (not
    /// cached, not reported).
    pub fn run(&mut self) -> bool {
        let started = Instant::now();
        self.finished = match self.fork.try_recompute() {
            Ok(stats) => {
                self.evaluated = stats.evaluated.len();
                true
            }
            Err(_) => {
                // The evaluations kept before the cancel.
                self.evaluated = self
                    .fork
                    .monitor()
                    .map_or(0, |monitor| monitor.progress().evaluated);
                false
            }
        };
        self.time = started.elapsed();
        self.finished
    }

    /// The features after the marker the run evaluates (not suppressed).
    pub fn rolled_back(&self) -> &[FeatureUid] {
        &self.rolled_back
    }

    /// The features the run evaluated (cache misses), also those of a run
    /// cancelled part way.
    pub fn evaluated(&self) -> usize {
        self.evaluated
    }

    /// How long the run took.
    pub fn time(&self) -> Duration {
        self.time
    }

    /// Whether the run went to the end of the timeline.
    pub fn finished(&self) -> bool {
        self.finished
    }

    /// The revision of the document's state the run was made from
    /// ([`Document::revision`]).
    pub fn base_revision(&self) -> u64 {
        self.fork.revision()
    }
}

/// Whether two definition states are the same but for the marker.
fn same_definition(a: &DocState, b: &DocState) -> bool {
    if a.marker == b.marker {
        return a == b;
    }
    let mut probe = a.clone();
    probe.marker = b.marker;
    probe == *b
}
