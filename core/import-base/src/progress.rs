// SPDX-License-Identifier: MIT
//! How far an import has come, for its watchdog and the memory guard
//! (`mitcad-ffi`): the [`Progress`] of every import (the `.f3d` and `.ipt`
//! replay, IFC, `.rvt`) and the one of the import running on a thread
//! ([`tick`], [`current_progress`], [`Ticking`]): one instance in the
//! process, which the geometry kernel's cancel check reads.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};

use mitcad_model::RecomputeMonitor;

/// How far an import has come, shared with a watchdog: a kernel call
/// cannot be interrupted, so a watchdog that sees no step for a long time
/// gives the import up and runs it again with the item it hung on in
/// `mitcad_import::Options::hung_items` (`mitcad-ffi`, `hang_limit`).
#[derive(Debug)]
pub struct Progress {
    /// Grows whenever the import moves on (an item starts, a definition or
    /// a fallback is evaluated).
    pub step: AtomicU64,
    /// The timeline index of the item being replayed, [`Progress::STARTING`]
    /// before the first and [`Progress::FINISHING`] while the final bodies
    /// are compared.
    pub item: AtomicI64,
    /// The timeline index of the item the import stopped at
    /// (`mitcad_import::Options::stop`), [`Progress::NOT_STOPPED`] before; a try run
    /// again stops there (`mitcad_import::Options::stop_at`).
    pub stopped: AtomicI64,
    /// The modelling items whose definitions gave no state, with why
    /// (`mitcad_import::Options::failed_items` of a try run again).
    failed: std::sync::Mutex<Vec<(i64, String)>>,
    /// Cancelled when the watchdog gives the try up
    /// ([`Progress::abandon`]).
    abandoned: Arc<RecomputeMonitor>,
    /// The memory of the process (mitcad#80): see [`Progress::low_memory`].
    memory: std::sync::Mutex<Memory>,
    /// Set when the import is to drop what it can do without
    /// ([`Progress::tight_on_memory`]).
    memory_tight: std::sync::atomic::AtomicBool,
    /// The share of the tightest memory limit in use as last measured
    /// ([`Progress::set_memory_share`]), as f64 bits.
    memory_share: AtomicU64,
    /// Cancelled while the memory is too short for definitions evaluated
    /// ahead ([`Progress::set_memory_share`]); a new one once there is
    /// enough again.
    ahead: std::sync::Mutex<Arc<RecomputeMonitor>>,
    /// The threads that work beside the import's own, and how many it may
    /// have in all ([`Progress::helper`], mitcad#103).
    helpers: Arc<Helpers>,
    /// A recompute of the whole design the import runs as one model
    /// command ([`Progress::recompute_monitor`]), whose features are its
    /// steps.
    recompute: std::sync::Mutex<Option<Arc<RecomputeMonitor>>>,
}

/// What the memory guard told an import (mitcad#80).
#[derive(Debug)]
struct Memory {
    /// Cancelled while the process is low on memory; a new one once it has
    /// recovered.
    low: Arc<RecomputeMonitor>,
    /// How many times it ran low.
    lows: u64,
    /// The memory in use as last measured.
    last: String,
    /// When it first ran low: the memory in use, and where the import was
    /// (as [`Progress::item`]).
    first_low: Option<(String, i64)>,
    /// Whether `first_low` is the memory guard's word for a failed
    /// allocation it saw before the operation that failed said which it
    /// was ([`Progress::low_memory_unattributed`]).
    first_low_unattributed: bool,
    /// A worker's operation that ran out of memory before the import first
    /// ran low ([`Progress::name_failed_allocation`]).
    named: Option<String>,
}

impl Progress {
    pub const STARTING: i64 = -1;
    pub const FINISHING: i64 = -2;
    pub const NOT_STOPPED: i64 = i64::MIN;

    pub fn new() -> Self {
        Self {
            step: AtomicU64::new(0),
            item: AtomicI64::new(Self::STARTING),
            stopped: AtomicI64::new(Self::NOT_STOPPED),
            failed: std::sync::Mutex::new(Vec::new()),
            abandoned: Arc::new(RecomputeMonitor::new()),
            memory: std::sync::Mutex::new(Memory {
                low: Arc::new(RecomputeMonitor::new()),
                lows: 0,
                last: String::new(),
                first_low: None,
                first_low_unattributed: false,
                named: None,
            }),
            memory_tight: std::sync::atomic::AtomicBool::new(false),
            memory_share: AtomicU64::new(0),
            ahead: std::sync::Mutex::new(Arc::new(RecomputeMonitor::new())),
            helpers: Arc::default(),
            recompute: std::sync::Mutex::new(None),
        }
    }

    /// A monitor for a recompute of the whole design that the import runs
    /// as one model command (mitcad#2085: the turn of a Y-up design to
    /// stand on Z up recomputes every feature, minutes for a large design
    /// in which no kernel call hangs): each feature it reaches or evaluates
    /// is a step of the import ([`Progress::steps`]), and it is cancelled
    /// when the watchdog gives the try up, so that the try ends and frees
    /// its memory before the next one.
    /// `also`: a monitor it is cancelled with too (the document's own, a
    /// stop of the user's).
    pub fn recompute_monitor(&self, also: Option<Arc<RecomputeMonitor>>) -> Arc<RecomputeMonitor> {
        let parents = std::iter::once(Arc::clone(&self.abandoned))
            .chain(also)
            .collect();
        let monitor = Arc::new(RecomputeMonitor::within(parents, None));
        if let Ok(mut r) = self.recompute.lock() {
            *r = Some(Arc::clone(&monitor));
        }
        monitor
    }

    /// How far the import has come, for its watchdog: its [`Progress::step`]
    /// and the features of a recompute it runs
    /// ([`Progress::recompute_monitor`]); a value that changes is progress.
    pub fn steps(&self) -> u64 {
        let recompute = self.recompute.lock().ok().and_then(|r| {
            r.as_ref().map(|m| {
                let p = m.progress();
                (p.position as u64).wrapping_add((p.evaluated as u64) << 32)
            })
        });
        self.step
            .load(Ordering::Relaxed)
            .wrapping_add(recompute.unwrap_or(0))
    }

    /// Memory is getting tight (mitcad#80; `mitcad-ffi`'s memory guard,
    /// before it is low): before its next item the import drops the cached
    /// results of the definitions it tried and the bodies of the history
    /// states behind it, which it builds again if it needs them; what it
    /// imports stays the same.
    pub fn tight_on_memory(&self) {
        self.memory_tight.store(true, Ordering::Relaxed);
    }

    /// Whether the import is asked to drop what it can do without
    /// ([`Progress::tight_on_memory`]); the request is taken.
    pub fn take_tight(&self) -> bool {
        self.memory_tight.swap(false, Ordering::Relaxed)
    }

    /// The process is low on memory (mitcad#80; `mitcad-ffi`'s memory
    /// guard, or a kernel operation that failed for lack of it): an
    /// allocation that fails would end the process. The definition being
    /// evaluated is cut short (P7e) and its item takes the file's bodies,
    /// as do the modelling items after it until the memory has recovered
    /// ([`Progress::memory_recovered`]); the import drops the results and
    /// history states it can do without, and compares the final bodies by
    /// volume only if the memory is still low then. `memory` says how much
    /// is in use, for the report.
    pub fn low_memory(&self, memory: &str) {
        self.went_low(memory, false);
    }

    /// As [`Progress::low_memory`], for an allocation in the kernel that
    /// failed without saying in which operation (the memory guard counts
    /// the failed allocations, mitcad#132): when the operation that failed
    /// reports it later ([`Progress::low_memory`]), the report tells its
    /// words instead (the guard polls on a thread of its own and can see
    /// the count, and the memory recover, before the operation has
    /// thrown).
    pub fn low_memory_unattributed(&self, memory: &str) {
        self.went_low(memory, true);
    }

    fn went_low(&self, memory: &str, unattributed: bool) {
        let item = self.item.load(Ordering::Relaxed);
        let Ok(mut m) = self.memory.lock() else {
            return;
        };
        // An operation names the failure the guard saw first (the guard
        // may have seen the memory recover meanwhile: a failed allocation
        // leaves the memory as it was).
        if !unattributed && m.first_low_unattributed {
            m.first_low_unattributed = false;
            if let Some((text, _)) = m.first_low.as_mut() {
                memory.clone_into(text);
            }
        }
        if m.low.is_cancelled() {
            return;
        }
        m.lows += 1;
        if m.first_low.is_none() {
            let named = if unattributed { m.named.take() } else { None };
            m.first_low_unattributed = unattributed && named.is_none();
            m.first_low = Some((named.unwrap_or_else(|| memory.to_owned()), item));
        }
        m.low.cancel();
    }

    /// A worker's operation ran out of memory (it is evaluated again on the
    /// import's thread, which tells whether the import is low on memory):
    /// names the failed allocation the memory guard counted, or counts
    /// next ([`Progress::low_memory_unattributed`]), in the report.
    pub fn name_failed_allocation(&self, memory: &str) {
        let Ok(mut m) = self.memory.lock() else {
            return;
        };
        if m.first_low_unattributed {
            m.first_low_unattributed = false;
            if let Some((text, _)) = m.first_low.as_mut() {
                memory.clone_into(text);
            }
        } else if m.first_low.is_none() {
            m.named = Some(memory.to_owned());
        }
    }

    /// The memory has recovered from being low ([`Progress::low_memory`]):
    /// the modelling items from the next one on are replayed again.
    pub fn memory_recovered(&self) {
        if let Ok(mut m) = self.memory.lock()
            && m.low.is_cancelled()
        {
            m.low = Arc::new(RecomputeMonitor::new());
        }
    }

    /// Whether the process is low on memory ([`Progress::low_memory`]).
    pub fn is_memory_low(&self) -> bool {
        self.memory.lock().is_ok_and(|m| m.low.is_cancelled())
    }

    /// How many times the process ran low on memory.
    pub fn memory_lows(&self) -> u64 {
        self.memory.lock().map_or(0, |m| m.lows)
    }

    /// When the process first ran low on memory: how much was in use, and
    /// where the import was (the item being replayed,
    /// [`Progress::STARTING`] or [`Progress::FINISHING`]).
    pub fn first_low_memory(&self) -> Option<(String, i64)> {
        self.memory.lock().ok().and_then(|m| m.first_low.clone())
    }

    /// Cancelled once the process is low on memory, for the definitions
    /// evaluated from now on.
    pub fn memory_monitor(&self) -> Option<Arc<RecomputeMonitor>> {
        self.memory.lock().ok().map(|m| Arc::clone(&m.low))
    }

    /// The memory in use as last measured (e.g. "7.1 GiB of 11.4 GiB
    /// (address-space limit)"), for traces.
    pub fn set_memory(&self, memory: &str) {
        if let Ok(mut m) = self.memory.lock() {
            memory.clone_into(&mut m.last);
        }
    }

    /// The share of the tightest memory limit the process uses as last
    /// measured (`mitcad-ffi`'s memory guard): above half of it no more
    /// definitions are evaluated ahead (mitcad#95).
    /// From `AHEAD_STOP` of it on, the definitions being
    /// evaluated ahead are cut short (and evaluated again on the import's
    /// thread when it comes to them).
    pub fn set_memory_share(&self, share: f64) {
        self.memory_share.store(share.to_bits(), Ordering::Relaxed);
        if let Ok(mut ahead) = self.ahead.lock() {
            if share >= AHEAD_STOP {
                ahead.cancel();
            } else if share < AHEAD_MEMORY && ahead.is_cancelled() {
                *ahead = Arc::new(RecomputeMonitor::new());
            }
        }
    }

    /// Cancelled while the memory is too short for definitions evaluated
    /// ahead ([`Progress::set_memory_share`]).
    pub fn ahead_monitor(&self) -> Option<Arc<RecomputeMonitor>> {
        self.ahead.lock().ok().map(|a| Arc::clone(&a))
    }

    /// The share last measured ([`Progress::set_memory_share`]); 0 before.
    pub fn memory_share(&self) -> f64 {
        f64::from_bits(self.memory_share.load(Ordering::Relaxed))
    }

    /// The memory last measured ([`Progress::set_memory`]); empty before.
    pub fn memory(&self) -> String {
        self.memory
            .lock()
            .map(|m| m.last.clone())
            .unwrap_or_default()
    }

    /// The watchdog gave this try up (mitcad#71): the import stops at its
    /// next check, before the next item or definition, and the kernel's
    /// long operations of a definition being tried stop inside (P7e). What
    /// it made is not used: it reports nothing more and leaves its
    /// document as it is.
    pub fn abandon(&self) {
        self.abandoned.cancel();
    }

    /// Cancelled once the watchdog gives this try up ([`Progress::abandon`]),
    /// for the definitions evaluated from now on.
    pub fn abandoned_monitor(&self) -> Arc<RecomputeMonitor> {
        Arc::clone(&self.abandoned)
    }

    /// Whether the watchdog gave this try up ([`Progress::abandon`]).
    pub fn abandoned(&self) -> bool {
        self.abandoned.is_cancelled()
    }

    /// The item the import stopped at, once it did.
    pub fn stopped_at(&self) -> Option<i64> {
        let at = self.stopped.load(Ordering::Relaxed);
        (at != Self::NOT_STOPPED).then_some(at)
    }

    /// The modelling items so far whose definitions were tried and gave no
    /// state, with why (not those that took the file's bodies without
    /// trying: hung, stopped, out of time, or failed in an earlier try).
    pub fn failed_items(&self) -> Vec<(i64, String)> {
        self.failed.lock().map(|f| f.clone()).unwrap_or_default()
    }

    pub fn failed(&self, item: i64, reason: &str) {
        if let Ok(mut f) = self.failed.lock() {
            f.push((item, reason.to_owned()));
        }
    }

    /// The import moved on: a kernel call returned. Long loops of the
    /// import's own (building and measuring a history state's bodies,
    /// finding faces and edges by their fingerprints, comparing bodies)
    /// tick after each kernel call, so that the watchdog takes only a
    /// kernel call that does not return for a hang (mitcad#82); see
    /// [`tick`].
    pub fn tick(&self) {
        self.step.fetch_add(1, Ordering::Relaxed);
    }

    pub fn at(&self, item: i64) {
        self.item.store(item, Ordering::Relaxed);
        self.tick();
    }
}

impl Default for Progress {
    fn default() -> Self {
        Self::new()
    }
}

thread_local! {
    /// The progress of the import running on this thread
    /// (`mitcad_import::import_design`), for [`tick`].
    static TICKING: std::cell::RefCell<Option<Arc<Progress>>> =
        const { std::cell::RefCell::new(None) };
}

/// Ticks the progress of the import running on this thread, if any
/// ([`Progress::tick`]): for loops deep in the import, and in the stored
/// geometry that builds a state's bodies (`mitcad-ffi`), that run many
/// kernel calls in a row (mitcad#82: building and measuring the first
/// history state of a large design took 110 s, which the watchdog took for
/// a hang). Call it between kernel calls only: a call that does not return
/// must still look like one.
pub fn tick() {
    TICKING.with(|t| {
        if let Some(p) = t.borrow().as_ref() {
            p.tick();
        }
    });
}

/// The progress of the import running on this thread, if any: for the
/// geometry kernel's long operations, whose progress (reported from their
/// worker threads too) is the import's (`mitcad-ffi`, mitcad#82).
pub fn current_progress() -> Option<Arc<Progress>> {
    TICKING.with(|t| t.borrow().clone())
}

/// Whether the import running on this thread was given up by the
/// watchdog ([`Progress::abandon`]): long loops may stop early, as what
/// the try makes is dropped.
pub fn abandoned() -> bool {
    TICKING.with(|t| t.borrow().as_ref().is_some_and(|p| p.abandoned()))
}

/// While it lives, [`tick`] ticks `progress` on this thread; the import
/// before it (none, usually) afterwards. (`mitcad-ffi`'s threads that
/// build the history's bodies ahead tick their import's, mitcad#103.)
pub struct Ticking(Option<Arc<Progress>>);

impl Ticking {
    pub fn new(progress: Option<Arc<Progress>>) -> Self {
        Self(TICKING.with(|t| t.replace(progress)))
    }
}

impl Drop for Ticking {
    fn drop(&mut self) {
        let before = self.0.take();
        TICKING.with(|t| *t.borrow_mut() = before);
    }
}

/// The share of the tightest memory limit above which no more definitions
/// are evaluated ahead ([`Progress::set_memory_share`]): each worker holds
/// its definition's memory and a thread stack, and the import gives up
/// definitions at 85 % (mitcad#80).
pub const AHEAD_MEMORY: f64 = 0.5;

/// The share of the tightest memory limit at which the definitions being
/// evaluated ahead are cut short ([`Progress::set_memory_share`]): the
/// import's own definition keeps the memory, as without workers.
pub(crate) const AHEAD_STOP: f64 = 0.6;

// One budget of threads for the work beside the import's own (mitcad#103):
// the definitions `mitcad-import` evaluates ahead, and the history's bodies
// that `mitcad-ffi` builds ahead of the replay.

/// The threads working beside an import's own, and how many it may have in
/// all (`mitcad_import::Options::threads`).
#[derive(Debug, Default)]
pub(crate) struct Helpers {
    running: AtomicUsize,
    threads: AtomicUsize,
}

/// A thread of the import's budget ([`Progress::helper`]), given back when
/// dropped.
#[derive(Debug)]
pub struct Helper(Arc<Helpers>);

impl Drop for Helper {
    fn drop(&mut self) {
        self.0.running.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Progress {
    /// How many threads the import may use in all, its own one of them
    /// (`mitcad_import::Options::threads`; set by `mitcad_import::import_design`).
    pub fn set_threads(&self, threads: usize) {
        self.helpers.threads.store(threads.max(1), Ordering::SeqCst);
    }

    /// A thread to work beside the import's own: while fewer work than its
    /// threads allow and the process uses less than half of its tightest
    /// memory limit ([`AHEAD_MEMORY`]) and is not low on memory; None
    /// otherwise.
    pub fn helper(&self) -> Option<Helper> {
        if self.is_memory_low() || self.memory_share() >= AHEAD_MEMORY {
            return None;
        }
        let threads = self.helpers.threads.load(Ordering::SeqCst).max(1);
        let running = &self.helpers.running;
        let mut n = running.load(Ordering::SeqCst);
        loop {
            if n + 1 >= threads {
                return None;
            }
            match running.compare_exchange(n, n + 1, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => break,
                Err(now) => n = now,
            }
        }
        Some(Helper(Arc::clone(&self.helpers)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recompute the import runs as one model command is progress for
    /// its watchdog as it reaches and evaluates features, and is cancelled
    /// when the try is given up (mitcad#2085).
    #[test]
    fn a_recompute_s_features_are_steps_and_stop_when_given_up() {
        let progress = Progress::new();
        let before = progress.steps();
        let user = Arc::new(RecomputeMonitor::new());
        let monitor = progress.recompute_monitor(Some(Arc::clone(&user)));
        assert_eq!(progress.steps(), before, "no feature reached yet");
        monitor.enter(1, 10).unwrap();
        let reached = progress.steps();
        assert_ne!(reached, before);
        monitor.kept();
        assert_ne!(progress.steps(), reached);
        assert!(!monitor.is_cancelled());
        progress.abandon();
        assert!(monitor.is_cancelled());
        // With the user's stop too.
        let other = Progress::new().recompute_monitor(Some(Arc::clone(&user)));
        user.cancel();
        assert!(other.is_cancelled());
    }
}
