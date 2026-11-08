// SPDX-License-Identifier: MIT
//! What the imports' memory guard (mitcad#80; `mitcad-ffi`, which
//! measures the process against its limits) tells an import's
//! [`Progress`] from its measurements: memory tight
//! ([`Progress::tight_on_memory`]), low ([`Progress::low_memory`]) or
//! recovered ([`Progress::memory_recovered`]).
//!
//! It measures as often as the memory guard of model commands and looks
//! ahead with its code (`mitcad_model::monitor::memory_guard`, mitcad#1493,
//! mitcad#1505): a definition whose memory, growing on as it grew over the
//! last two measurements, would reach [`MEMORY_LOW`] within
//! [`MEMORY_AHEAD`] is cut short now, before it gets there. A boolean of
//! many B-spline faces grew by about 6 GiB a second, so that, measured
//! every 0.1 s, it passed a limit before the guard saw it near.
//!
//! The import looks ahead more carefully than a command, as a definition
//! cut short is an item that takes the file's bodies: only from
//! [`LOOK_AHEAD_FROM`] of the limit on, and only at growth kept up over
//! two measurements (the slower of the two). A single jump between two
//! measurements is no growth to go on with: a new thread's allocator arena
//! or one large buffer, or another import of the same process (the corpus
//! runs import three designs at once), moved a data limit's reading by
//! hundreds of MiB within 0.02 s, and a design's extrusion at 40 % of the
//! limit was cut short.

use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Instant;

pub use mitcad_model::monitor::memory_guard::{
    MEMORY_AHEAD, MEMORY_LOW, MEMORY_POLL, MemoryGrowth, MemoryReading, low_ahead,
};

use crate::Progress;

/// The share of the tightest memory limit at which the import drops what
/// it can do without ([`Progress::tight_on_memory`]), and again at each 5 %
/// more; below it, memory that was low has recovered.
pub const MEMORY_TIGHT: f64 = 0.70;

/// The share of the tightest memory limit from which the import looks
/// ahead at the memory's growth ([`low_ahead`]); below it, growth at any
/// rate leaves at least one more measurement before the low share.
pub const LOOK_AHEAD_FROM: f64 = 0.5;

/// The import's memory guard: `measure` gives the process's memory against
/// its limits (`mitcad-ffi`'s, or a test's), and [`ImportMemoryGuard::check`]
/// tells the import's [`Progress`] what it means. At [`MEMORY_LOW`] of the
/// tightest limit, or when the memory grows so fast that it would get there
/// within [`MEMORY_AHEAD`] ([`low_ahead`]; from [`LOOK_AHEAD_FROM`] on, at
/// growth kept up over two measurements), the import is low on memory: it
/// cuts the definition being evaluated short and tries no more until the
/// memory has recovered. An allocation that fails ends the process (Rust's
/// cannot be caught), and one definition can take gigabytes (a pattern of a
/// thousand copies).
pub struct ImportMemoryGuard<M> {
    /// Measures the process.
    measure: M,
    /// The share at which the import is next told that memory is tight.
    next_tight: f64,
    /// The allocations in the geometry kernel that had failed when last
    /// measured.
    failed_allocations: u64,
    /// How fast the memory grows between measurements.
    growth: MemoryGrowth,
    /// The growth measured at the measurement before (shares of the limit
    /// a second).
    grew: f64,
}

impl<M: FnMut() -> MemoryReading> ImportMemoryGuard<M> {
    /// A guard measuring with `measure`, from now on: the allocations in
    /// the kernel that failed before are not the import's.
    pub fn new(mut measure: M) -> Self {
        let first = measure();
        let mut growth = MemoryGrowth::default();
        growth.measured(Instant::now(), first.share);
        Self {
            measure,
            next_tight: MEMORY_TIGHT,
            failed_allocations: first.failed_allocations,
            growth,
            grew: 0.0,
        }
    }

    /// Measures the process and tells `progress` what it means.
    pub fn check(&mut self, progress: &Progress) {
        let reading = (self.measure)();
        self.judge(progress, &reading, Instant::now());
    }

    /// Tells `progress` what `reading`, measured at `now`, means.
    fn judge(&mut self, progress: &Progress, reading: &MemoryReading, now: Instant) {
        let share = reading.share;
        let text = &reading.text;
        // The growth kept up over the last two measurements.
        let grows = self.growth.measured(now, share);
        let rate = grows.min(self.grew);
        self.grew = grows;
        progress.set_memory(text);
        progress.set_memory_share(share);
        // An allocation in the kernel failed (mitcad#132): also one it
        // caught inside, which no operation reported, and one on a thread
        // of no import. The memory ran out, whatever it is now.
        if reading.failed_allocations != self.failed_allocations {
            self.failed_allocations = reading.failed_allocations;
            progress.low_memory_unattributed(&format!(
                "an allocation in the geometry kernel failed, {text}"
            ));
            return;
        }
        if share >= MEMORY_LOW {
            progress.low_memory(text);
            return;
        }
        // Growing so fast that it would be low before the next measurements
        // and the kernel's threads see the cancel (mitcad#1505).
        if share >= LOOK_AHEAD_FROM && low_ahead(share, rate) {
            progress.low_memory(&format!("growing too fast to go on, {text}"));
            return;
        }
        if share < MEMORY_TIGHT {
            progress.memory_recovered();
        }
        if share >= self.next_tight {
            progress.tight_on_memory();
            self.next_tight = share + 0.05;
        } else {
            // (Down again after the import dropped what it could.)
            self.next_tight = self.next_tight.min((share + 0.05).max(MEMORY_TIGHT));
        }
    }

    /// Runs `f` with the guard measuring the process every [`MEMORY_POLL`]
    /// on a thread of its own (an import without the watchdog, whose loop
    /// measures it).
    pub fn around<T>(mut self, progress: &Arc<Progress>, f: impl FnOnce() -> T) -> T
    where
        M: Send + 'static,
    {
        self.check(progress);
        let (done, finished) = mpsc::channel::<()>();
        let watched = Arc::clone(progress);
        let guard = std::thread::Builder::new()
            .name("mitcad-import-memory".to_owned())
            .stack_size(1 << 20)
            .spawn(move || {
                while matches!(
                    finished.recv_timeout(MEMORY_POLL),
                    Err(RecvTimeoutError::Timeout)
                ) {
                    self.check(&watched);
                }
            });
        let result = f();
        drop(done);
        if let Ok(guard) = guard {
            let _ = guard.join();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn reading(share: f64, failed_allocations: u64) -> MemoryReading {
        MemoryReading {
            share,
            text: format!("{share:.1} GiB of 1.0 GiB (the import's memory limit)"),
            failed_allocations,
        }
    }

    /// A guard whose first, fake measurement is at `first`.
    fn guard(first: f64) -> ImportMemoryGuard<impl FnMut() -> MemoryReading> {
        ImportMemoryGuard::new(move || reading(first, 0))
    }

    /// An import step whose memory keeps growing so fast that it would
    /// reach the low share within the look-ahead is cut short below it,
    /// and the import goes on once the memory has stopped growing below the
    /// tight share. A single jump, growth below [`LOOK_AHEAD_FROM`] and
    /// slow growth are left alone.
    #[test]
    fn a_fast_growing_step_is_cut_short_below_the_limit() {
        let progress = Progress::new();
        let mut guard = guard(0.1);
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        // 0.1 to 0.3 in 20 ms: one jump, nothing to go on with yet.
        guard.judge(&progress, &reading(0.3, 0), at(20));
        assert!(!progress.is_memory_low(), "a single jump was cut short");
        // 0.3 to 0.45, 7.5 shares a second kept up, but below the floor.
        guard.judge(&progress, &reading(0.45, 0), at(40));
        assert!(
            !progress.is_memory_low(),
            "growth below the floor was cut short"
        );
        // 0.45 to 0.6, kept up: past 0.85 within 0.1 s.
        guard.judge(&progress, &reading(0.6, 0), at(60));
        assert!(progress.is_memory_low(), "the fast growth was let through");
        let (memory, _) = progress.first_low_memory().expect("low");
        assert_eq!(
            memory,
            "growing too fast to go on, 0.6 GiB of 1.0 GiB (the import's memory limit)"
        );
        // Stopped growing below the tight share: recovered.
        guard.judge(&progress, &reading(0.6, 0), at(80));
        assert!(!progress.is_memory_low());
        // A single jump above the floor (another import's buffer) is left
        // alone, as is slow growth near the low share (0.05 shares a
        // second); at the low share, low.
        guard.judge(&progress, &reading(0.8, 0), at(100));
        assert!(!progress.is_memory_low(), "a single jump was cut short");
        guard.judge(&progress, &reading(0.8, 0), at(300));
        guard.judge(&progress, &reading(0.81, 0), at(500));
        guard.judge(&progress, &reading(0.82, 0), at(700));
        assert!(!progress.is_memory_low(), "slow growth was cut short");
        guard.judge(&progress, &reading(0.86, 0), at(1200));
        assert!(progress.is_memory_low());
        assert_eq!(progress.memory_lows(), 2);
    }

    /// The import's memory growing fast, a tenth of the limit at each
    /// measurement of the guard's thread (a boolean's threads building their
    /// grids, mitcad#1493), cuts the step short below the low share.
    #[test]
    fn fast_growth_is_cut_short_on_the_guard_s_thread() {
        let progress = Arc::new(Progress::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        let measure = move || {
            // The guard's first measurement and its check before the step
            // at 0.1, then 0.2, 0.3, ... up to 0.84 on the guard's thread.
            let n = counted.fetch_add(1, Ordering::SeqCst);
            let share = if n >= 2 {
                (n as f64 / 10.0).min(0.84)
            } else {
                0.1
            };
            reading(share, 0)
        };
        let low = ImportMemoryGuard::new(measure).around(&progress, || {
            let start = Instant::now();
            while !progress.is_memory_low() && start.elapsed() < Duration::from_secs(30) {
                std::thread::sleep(Duration::from_millis(5));
            }
            progress.is_memory_low()
        });
        assert!(low, "the guard did not cut the step short");
        assert!(calls.load(Ordering::SeqCst) >= 6);
        assert!(progress.memory_share() < MEMORY_LOW);
        let (memory, _) = progress.first_low_memory().expect("low");
        assert!(
            memory.starts_with("growing too fast to go on, 0."),
            "{memory}"
        );
    }

    /// At the low share the plain reading; a failed allocation in the
    /// kernel after the guard started, whatever the share; tight from 70 %
    /// and again at each 5 % more.
    #[test]
    fn low_failed_and_tight() {
        let progress = Progress::new();
        let mut guard = ImportMemoryGuard::new(|| reading(0.1, 2));
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        guard.judge(&progress, &reading(0.1, 2), at(1000));
        assert!(!progress.is_memory_low());
        guard.judge(&progress, &reading(0.1, 3), at(2000));
        assert!(progress.is_memory_low());
        let (memory, _) = progress.first_low_memory().expect("low");
        assert!(
            memory.starts_with("an allocation in the geometry kernel failed, 0.1 GiB"),
            "{memory}"
        );
        guard.judge(&progress, &reading(0.1, 3), at(3000));
        assert!(!progress.is_memory_low());
        guard.judge(&progress, &reading(0.72, 3), at(13000));
        assert!(progress.take_tight());
        guard.judge(&progress, &reading(0.74, 3), at(23000));
        assert!(!progress.take_tight());
        guard.judge(&progress, &reading(0.78, 3), at(33000));
        assert!(progress.take_tight());
        guard.judge(&progress, &reading(0.9, 3), at(43000));
        assert!(progress.is_memory_low());
        assert_eq!(progress.memory_lows(), 2);
    }
}
