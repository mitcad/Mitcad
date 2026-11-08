// SPDX-License-Identifier: MIT
//! The memory guard of a model command (mitcad#1480): while a command
//! computes, the process's memory is measured against its limits, and the
//! command is stopped with an error that says so before the process runs
//! out of memory. An allocation that fails ends the process (Rust's
//! cannot be caught), and on Linux the system ends a process that takes
//! more than it has left; one boolean of many large, nearly flat spheres
//! took several GiB, and the headless MCP server was ended without a word
//! to its client.
//!
//! The guard cancels the command's monitor with its reason
//! ([`RecomputeMonitor::cancel_because`]): the recompute stops before the
//! next feature, and the geometry's long operations (booleans, fillets,
//! sweeps, ...) stop inside the feature being evaluated (P7e). A cancelled
//! command changes nothing, so the document stays as it was before it.
//! The measurement is the caller's (`mitcad-ffi` measures the process
//! against its limits as the imports' memory guard does, mitcad#80), so
//! that the guard is tested here with readings of its own.

use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::RecomputeMonitor;

/// The process's memory as the guard is given it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemoryReading {
    /// The share of the tightest memory limit in use; 0 without a known
    /// limit.
    pub share: f64,
    /// What is in use against which limit, for the error ("5.9 GiB of
    /// 6.9 GiB (memory the system has left)").
    pub text: String,
    /// How many allocations inside the geometry kernel have failed in the
    /// process so far (also those it caught inside, mitcad#132).
    pub failed_allocations: u64,
}

/// The share of the tightest limit at which a command is stopped: as the
/// imports' guard cuts a definition short (mitcad#80), with room left for
/// the memory a boolean takes between two measurements.
pub const MEMORY_LOW: f64 = 0.85;

/// How often the guard measures the process while a command computes.
/// A boolean of many B-spline faces (mitcad#1493: a plate and 38 flattened
/// spheres in one join) grew by about 6 GiB a second, every thread of
/// the kernel building its own sampling grids of the faces: measured
/// every 0.1 s, it passed a limit of 600 MiB by half before the guard saw
/// it near (925 MiB at the peak, 870 MiB at the guard's first look), so
/// the guard measures more often.
pub const MEMORY_POLL: Duration = Duration::from_millis(20);

/// How far ahead the guard looks at the memory's growth: a command whose
/// memory, growing on as it grew since the last measurement, would reach
/// [`MEMORY_LOW`] within this time is stopped now. It covers the next
/// measurement and the time the kernel's threads take to see the cancel
/// (each finishes the intersection it is in).
pub const MEMORY_AHEAD: Duration = Duration::from_millis(100);

/// The error of a command stopped for `reading`, or None while the memory
/// suffices: at [`MEMORY_LOW`] of the tightest limit, or once an
/// allocation in the geometry kernel failed after the command started
/// (`failed_before` failed before it).
pub fn refusal(reading: &MemoryReading, failed_before: u64) -> Option<String> {
    refusal_ahead(reading, failed_before, 0.0)
}

/// [`refusal`] for memory that grows by `growth` (shares of the limit a
/// second, from the last measurement): also when it would reach
/// [`MEMORY_LOW`] within [`MEMORY_AHEAD`] at that rate.
pub fn refusal_ahead(reading: &MemoryReading, failed_before: u64, growth: f64) -> Option<String> {
    let cause = if reading.failed_allocations > failed_before {
        " (an allocation in the geometry kernel failed)"
    } else if reading.share >= MEMORY_LOW {
        ""
    } else if low_ahead(reading.share, growth) {
        " (growing too fast to go on)"
    } else {
        return None;
    };
    Some(format!(
        "the operation ran out of memory{cause} at {} and was stopped before the system would \
         end the process; the document is as it was before the command. Do the work in smaller \
         steps, for example join the bodies in smaller groups",
        reading.text
    ))
}

/// Whether memory at `share` of the tightest limit, growing by `growth`
/// (shares of the limit a second, [`MemoryGrowth`]), would reach
/// [`MEMORY_LOW`] within [`MEMORY_AHEAD`]; also the imports' memory guard's
/// (`mitcad-import-base`, mitcad#1505).
pub fn low_ahead(share: f64, growth: f64) -> bool {
    share + growth.max(0.0) * MEMORY_AHEAD.as_secs_f64() >= MEMORY_LOW
}

/// How fast the memory grows from one measurement to the next, for the
/// look-ahead ([`low_ahead`]); also the imports' memory guard's
/// (`mitcad-import-base`, mitcad#1505).
#[derive(Debug, Clone, Default)]
pub struct MemoryGrowth {
    /// The last measurement: when, and the share of the limit.
    last: Option<(Instant, f64)>,
}

impl MemoryGrowth {
    /// The growth up to `share` measured at `now` since the last
    /// measurement, in shares of the limit a second (0 at the first, or
    /// when it did not grow); `now` becomes the last measurement.
    pub fn measured(&mut self, now: Instant, share: f64) -> f64 {
        let rate = self.last.map_or(0.0, |last| growth(last, (now, share)));
        self.last = Some((now, share));
        rate
    }
}

/// Runs `f`, a command computing with `monitor`, while `measure` is asked
/// for the process's memory before it and every [`MEMORY_POLL`] on a
/// thread of the guard's own; once the memory runs low, or grows so fast
/// that it would within [`MEMORY_AHEAD`] ([`refusal_ahead`]), the monitor
/// is cancelled with the reason, and `f`'s
/// computation fails at its next check. The guard stops measuring when
/// `f` returns or the monitor is cancelled otherwise (by the user).
pub fn guarded<T>(
    monitor: &Arc<RecomputeMonitor>,
    mut measure: impl FnMut() -> MemoryReading + Send + 'static,
    f: impl FnOnce() -> T,
) -> T {
    let first = measure();
    let failed_before = first.failed_allocations;
    if monitor.is_cancelled() || refuse(monitor, &first, failed_before, 0.0) {
        // (Fails at its first check.)
        return f();
    }
    let (done, finished) = mpsc::channel::<()>();
    let watched = Arc::clone(monitor);
    let mut growth = MemoryGrowth::default();
    growth.measured(Instant::now(), first.share);
    let watch = std::thread::Builder::new()
        .name("mitcad-command-memory".to_owned())
        .stack_size(1 << 20)
        .spawn(move || {
            while matches!(
                finished.recv_timeout(MEMORY_POLL),
                Err(RecvTimeoutError::Timeout)
            ) {
                if watched.is_cancelled() {
                    break;
                }
                let reading = measure();
                let rate = growth.measured(Instant::now(), reading.share);
                if refuse(&watched, &reading, failed_before, rate) {
                    break;
                }
            }
        });
    // (Without the thread, the command runs as it did without a guard.)
    let result = f();
    drop(done);
    if let Ok(watch) = watch {
        let _ = watch.join();
    }
    result
}

/// How fast the memory grew from `before` to `after` (a time and a share
/// each), in shares of the limit a second; 0 when it did not grow. The
/// time counts as at least half a [`MEMORY_POLL`], so that two
/// measurements close together do not make a small step a fast growth.
fn growth(before: (Instant, f64), after: (Instant, f64)) -> f64 {
    let seconds = after
        .0
        .saturating_duration_since(before.0)
        .max(MEMORY_POLL / 2)
        .as_secs_f64();
    (after.1 - before.1).max(0.0) / seconds
}

/// Cancels `monitor` with the reason when `reading`, growing at `rate`,
/// refuses the command; whether it did.
fn refuse(
    monitor: &RecomputeMonitor,
    reading: &MemoryReading,
    failed_before: u64,
    rate: f64,
) -> bool {
    match refusal_ahead(reading, failed_before, rate) {
        Some(reason) => {
            monitor.cancel_because(&reason);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    fn reading(share: f64, failed_allocations: u64) -> MemoryReading {
        MemoryReading {
            share,
            text: format!("{share:.1} GiB of 1.0 GiB (the command memory limit)"),
            failed_allocations,
        }
    }

    #[test]
    fn refusal_at_the_low_share_or_a_failed_allocation() {
        assert_eq!(refusal(&reading(0.5, 0), 0), None);
        assert_eq!(refusal(&reading(MEMORY_LOW - 0.01, 3), 3), None);
        let low = refusal(&reading(0.9, 0), 0).expect("low on memory");
        assert!(
            low.starts_with(
                "the operation ran out of memory at 0.9 GiB of 1.0 GiB (the command memory limit)"
            ),
            "{low}"
        );
        assert!(low.contains("as it was before the command"), "{low}");
        assert!(low.contains("join the bodies in smaller groups"), "{low}");
        let failed = refusal(&reading(0.2, 4), 3).expect("an allocation failed");
        assert!(
            failed.starts_with(
                "the operation ran out of memory (an allocation in the geometry kernel failed) at"
            ),
            "{failed}"
        );
    }

    /// A command that starts low on memory fails at its first check.
    #[test]
    fn refused_before_the_work() {
        let monitor = Arc::new(RecomputeMonitor::new());
        let cancelled = guarded(&monitor, || reading(0.95, 0), || monitor.is_cancelled());
        assert!(cancelled);
        let reason = monitor.cancel_reason().expect("a reason");
        assert!(reason.contains("ran out of memory at 0.9"), "{reason}");
    }

    /// Memory that runs low while the command computes cancels it then.
    #[test]
    fn refused_while_the_work_runs() {
        let monitor = Arc::new(RecomputeMonitor::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        let measure = move || {
            // Low from the third measurement on.
            let n = counted.fetch_add(1, Ordering::SeqCst);
            reading(if n >= 2 { 0.9 } else { 0.3 }, 0)
        };
        let stopped = guarded(&monitor, measure, || {
            let start = Instant::now();
            while !monitor.is_cancelled() && start.elapsed() < Duration::from_secs(30) {
                std::thread::sleep(Duration::from_millis(5));
            }
            monitor.is_cancelled()
        });
        assert!(stopped, "the guard did not stop the command");
        assert!(calls.load(Ordering::SeqCst) >= 3);
        assert!(
            monitor
                .cancel_reason()
                .is_some_and(|r| r.contains("ran out of memory at 0.9"))
        );
    }

    /// Memory that would reach the low share within the look-ahead at
    /// the rate it grows is refused before it gets there; slower growth,
    /// or memory given back, is not.
    #[test]
    fn refusal_ahead_of_fast_growth() {
        assert_eq!(refusal_ahead(&reading(0.5, 0), 0, 1.0), None);
        assert_eq!(refusal_ahead(&reading(0.5, 0), 0, -20.0), None);
        let fast = refusal_ahead(&reading(0.5, 0), 0, 4.0).expect("growing fast");
        assert!(
            fast.starts_with("the operation ran out of memory (growing too fast to go on) at 0.5"),
            "{fast}"
        );
        assert!(fast.contains("join the bodies in smaller groups"), "{fast}");
        // At the low share the plain reason, whatever the growth.
        let low = refusal_ahead(&reading(0.9, 0), 0, 4.0).expect("low");
        assert!(
            low.starts_with("the operation ran out of memory at 0.9"),
            "{low}"
        );
    }

    #[test]
    fn growth_in_shares_a_second() {
        let start = Instant::now();
        let later = start + Duration::from_millis(200);
        assert!((growth((start, 0.1), (later, 0.5)) - 2.0).abs() < 1e-9);
        assert_eq!(growth((start, 0.5), (later, 0.1)), 0.0);
        // Two measurements close together count as half a poll apart.
        let close = start + Duration::from_millis(1);
        let half = MEMORY_POLL.as_secs_f64() / 2.0;
        assert!((growth((start, 0.1), (close, 0.2)) - 0.1 / half).abs() < 1e-9);
    }

    /// The growth from the last measurement: none at the first, then in
    /// shares a second, and memory given back is no growth.
    #[test]
    fn growth_from_the_last_measurement() {
        let start = Instant::now();
        let mut growth = MemoryGrowth::default();
        assert_eq!(growth.measured(start, 0.4), 0.0);
        let rate = growth.measured(start + Duration::from_millis(100), 0.5);
        assert!((rate - 1.0).abs() < 1e-9, "{rate}");
        assert_eq!(
            growth.measured(start + Duration::from_millis(200), 0.3),
            0.0
        );
        // From the last measurement, not the first.
        let rate = growth.measured(start + Duration::from_millis(250), 0.35);
        assert!((rate - 1.0).abs() < 1e-9, "{rate}");
        // At 0.8, 0.6 a second reaches the low share within the look-ahead,
        // 0.4 a second does not; nor does memory given back.
        assert!(low_ahead(0.8, 0.6));
        assert!(!low_ahead(0.8, 0.4));
        assert!(!low_ahead(0.8, -3.0));
    }

    /// A command whose memory jumps between two measurements (a boolean's
    /// threads all building their grids at once, mitcad#1493) is stopped
    /// below the low share: from 0.1 to 0.8 within any delay up to 1.4 s
    /// of the guard's thread.
    #[test]
    fn refused_while_the_memory_jumps() {
        let monitor = Arc::new(RecomputeMonitor::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        let measure = move || {
            let n = counted.fetch_add(1, Ordering::SeqCst);
            reading(if n >= 1 { 0.8 } else { 0.1 }, 0)
        };
        let stopped = guarded(&monitor, measure, || {
            let start = Instant::now();
            while !monitor.is_cancelled() && start.elapsed() < Duration::from_secs(30) {
                std::thread::sleep(Duration::from_millis(5));
            }
            monitor.is_cancelled()
        });
        assert!(stopped, "the guard did not stop the command");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(
            monitor
                .cancel_reason()
                .is_some_and(|r| r.contains("growing too fast to go on) at 0.8"))
        );
    }

    /// Memory that grows slowly is left alone below the low share.
    #[test]
    fn slow_growth_leaves_the_command_alone() {
        let monitor = Arc::new(RecomputeMonitor::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        let measure = move || {
            let n = counted.fetch_add(1, Ordering::SeqCst);
            reading(0.3 + 0.0005 * n as f64, 0)
        };
        let value = guarded(&monitor, measure, || {
            std::thread::sleep(Duration::from_millis(300));
            7
        });
        assert_eq!(value, 7);
        assert!(!monitor.is_cancelled());
        assert!(calls.load(Ordering::SeqCst) >= 3);
    }

    /// An allocation in the kernel that fails while the command computes
    /// stops it, whatever the share.
    #[test]
    fn refused_after_a_failed_allocation() {
        let monitor = Arc::new(RecomputeMonitor::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        let measure = move || {
            let n = counted.fetch_add(1, Ordering::SeqCst);
            // Two failed before the command, a third during it.
            reading(0.1, if n >= 1 { 3 } else { 2 })
        };
        let stopped = guarded(&monitor, measure, || {
            let start = Instant::now();
            while !monitor.is_cancelled() && start.elapsed() < Duration::from_secs(30) {
                std::thread::sleep(Duration::from_millis(5));
            }
            monitor.is_cancelled()
        });
        assert!(stopped);
        assert!(
            monitor
                .cancel_reason()
                .is_some_and(|r| r.contains("an allocation in the geometry kernel failed"))
        );
    }

    /// With enough memory the command runs as without the guard.
    #[test]
    fn enough_memory_leaves_the_command_alone() {
        let monitor = Arc::new(RecomputeMonitor::new());
        let value = guarded(
            &monitor,
            || reading(0.2, 0),
            || {
                std::thread::sleep(Duration::from_millis(250));
                42
            },
        );
        assert_eq!(value, 42);
        assert!(!monitor.is_cancelled());
        assert_eq!(monitor.cancel_reason(), None);
    }

    /// The user's cancel takes no reason; a monitor within another has
    /// the other's.
    #[test]
    fn cancel_reasons() {
        let user = RecomputeMonitor::new();
        user.cancel();
        user.cancel_because("late");
        assert!(user.is_cancelled());
        assert_eq!(user.cancel_reason(), None);

        let parent = Arc::new(RecomputeMonitor::new());
        let child = RecomputeMonitor::within(vec![Arc::clone(&parent)], None);
        assert_eq!(child.cancel_reason(), None);
        parent.cancel_because("out of memory");
        parent.cancel_because("second");
        assert!(child.is_cancelled());
        assert_eq!(child.cancel_reason().as_deref(), Some("out of memory"));
    }
}
