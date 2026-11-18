// SPDX-License-Identifier: MIT
//! Kernel calls given up by a Cancel or the time limit (mitcad#1917).
//!
//! A kernel call cannot be interrupted from outside, and some of OCCT's
//! algorithms never look at their progress range (the fillet's and the
//! chamfer's `Build`): a call that does not return held the recompute, and
//! with it the application's Cancel, for good (a rounding whose corners meet
//! in the middle of its edge, mitcad#1819). Such calls run here on a thread
//! of their own ([`abandonable`]) while the evaluation's thread waits for
//! them. It takes the call's result when it comes, and gives the call up
//! once the recompute's monitor is cancelled (the user's Cancel, the MCP
//! cancel, the memory guard, the import's watchdog and deadlines) or once
//! the call has run for the kernel's time limit ([`kernel_time_limit`]): a
//! cancel fails the call as cancelled, the limit with
//! [`time_limit_message`], a failure in words for the feature.
//!
//! A call given up goes on on its thread, since OCCT's state cannot be torn
//! down under it, with its own monitor cancelled: the geometry's own steps
//! and OCCT's algorithms that look at a progress range stop at their next
//! check, and the call's result is dropped when it returns. One that loops
//! for good (the rounding above spun in a projection of a contact of no
//! length) keeps a processor busy until the process ends. It shares its input shapes with the work that goes on, as the
//! import's definitions evaluated on other threads do (mitcad#95). While one
//! runs, a process ends without its static destructors
//! (`abandoned_imports`, as after the import's given-up tries, mitcad#82).
//!
//! The calls run here (mitcad#1917, mitcad#1922), each one whose OCCT
//! algorithm does not look at its progress range everywhere:
//!
//! - fillets and chamfers (`BRepFilletAPI_*::Build` ignore the range);
//! - shells and offsets: Shell, Offset Face, Thicken, Offset Surface
//!   (OCCT's offset looks at the range only between its steps);
//! - sweeps: Sweep (solid, along guide surfaces, of a body face, and as a
//!   surface), Pipe, Coil, Helix and Loft (solid and surface): OCCT's pipe
//!   shell, pipe and loft (`BRepOffsetAPI_MakePipeShell`, `_MakePipe`,
//!   `_ThruSections`) ignore the range, and our helical sweeps have none (a
//!   pipe along a tight helix ran for minutes, mitcad#1788);
//! - healing: Delete Face (OCCT's defeaturing heals the body over the
//!   faces; it ran 300 s without returning in a fillet's retry,
//!   mitcad#1337) and a body built from its boundary (the IFC import's
//!   breps, healed by OCCT's ShapeFix, which has no range; its repair of
//!   intersecting wires once never returned, mitcad#495);
//! - drawings' views: OCCT's hidden-line removal (`HLRBRep_Algo`,
//!   `HLRBRep_PolyAlgo`) has no range, and a large view's ran for over a
//!   minute (mitcad#1997).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use mitcad_model::{KernelError, RecomputeMonitor};

use super::cancel;

/// The kernel's time limit when none is set: far longer than any operation
/// that finishes takes (minutes for the largest booleans and roundings),
/// far shorter than a user waits for one that does not.
pub const DEFAULT_TIME_LIMIT: Duration = Duration::from_secs(300);

/// The environment variable that sets the kernel's time limit in seconds
/// (0 for none) for the whole process.
pub const TIME_LIMIT_VARIABLE: &str = "MITCAD_KERNEL_TIME_LIMIT";

/// For tests: the environment variable that makes every call run here first
/// wait this many seconds without looking at any cancel or limit, as a call
/// that does not return (a fillet whose corners meet in the middle of its
/// edge, mitcad#1819, before it was refused).
pub const TEST_HANG_VARIABLE: &str = "MITCAD_TEST_KERNEL_HANG";

/// How often the waiting thread looks at the monitor and the clock.
const POLL: Duration = Duration::from_millis(10);

/// Not read from the environment yet.
const UNSET: u64 = u64::MAX;

/// The kernel's time limit in milliseconds, 0 for none.
static LIMIT_MS: AtomicU64 = AtomicU64::new(UNSET);

/// The test hang in milliseconds, 0 for none.
static TEST_HANG_MS: AtomicU64 = AtomicU64::new(UNSET);

/// The calls given up that still run.
static ABANDONED: AtomicUsize = AtomicUsize::new(0);

/// Milliseconds of a time in seconds from the environment; `default` when
/// it is missing or not a number of seconds.
fn read_ms(variable: &str, default: Duration) -> u64 {
    std::env::var(variable)
        .ok()
        .and_then(|text| text.trim().parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map_or_else(
            || u64::try_from(default.as_millis()).unwrap_or(u64::MAX - 1),
            |seconds| (seconds * 1000.0).round().min((u64::MAX - 1) as f64) as u64,
        )
}

/// A setting in milliseconds, read from `variable` the first time.
fn setting(value: &AtomicU64, variable: &str, default: Duration) -> Option<Duration> {
    let mut ms = value.load(Ordering::Relaxed);
    if ms == UNSET {
        let read = read_ms(variable, default);
        ms = match value.compare_exchange(UNSET, read, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => read,
            Err(set) => set,
        };
    }
    (ms > 0).then(|| Duration::from_millis(ms))
}

/// How long a kernel call run by [`abandonable`] may take before it is given
/// up and fails with [`time_limit_message`]; None for no limit. From
/// [`TIME_LIMIT_VARIABLE`] (seconds, 0 for none), else
/// [`DEFAULT_TIME_LIMIT`], unless [`set_kernel_time_limit`] set it. On the
/// wall clock: a hard limit a person can count on, far above what a call
/// that returns takes on a busy machine too.
pub fn kernel_time_limit() -> Option<Duration> {
    THREAD_SETTINGS.with(|s| s.get()).map_or_else(
        || setting(&LIMIT_MS, TIME_LIMIT_VARIABLE, DEFAULT_TIME_LIMIT),
        |(limit, _)| limit,
    )
}

/// Milliseconds of a setting, 0 for none.
fn as_ms(time: Option<Duration>) -> u64 {
    time.map_or(0, |t| {
        u64::try_from(t.as_millis()).unwrap_or(u64::MAX - 1).max(1)
    })
}

/// Sets the kernel's time limit for the whole process (None for none).
pub fn set_kernel_time_limit(limit: Option<Duration>) {
    LIMIT_MS.store(as_ms(limit), Ordering::Relaxed);
}

/// For tests: the hang of [`TEST_HANG_VARIABLE`], or the one
/// [`set_test_hang`] set.
fn test_hang_time() -> Option<Duration> {
    THREAD_SETTINGS.with(|s| s.get()).map_or_else(
        || setting(&TEST_HANG_MS, TEST_HANG_VARIABLE, Duration::ZERO),
        |(_, hang)| hang,
    )
}

/// For tests: every call run by [`abandonable`] in the whole process first
/// waits `hang` without looking at any cancel or limit, as a kernel call
/// that does not return (None: none, as without [`TEST_HANG_VARIABLE`]).
#[doc(hidden)]
pub fn set_test_hang(hang: Option<Duration>) {
    TEST_HANG_MS.store(as_ms(hang), Ordering::Relaxed);
}

thread_local! {
    /// For tests: the time limit and the test hang of the calls made on
    /// this thread, in place of the process's ([`with_test_settings`]).
    static THREAD_SETTINGS: std::cell::Cell<Option<(Option<Duration>, Option<Duration>)>> =
        const { std::cell::Cell::new(None) };
}

/// For tests: runs `f` with the time limit `limit` and the test hang `hang`
/// for the calls it makes on this thread only, so that tests running beside
/// each other in one process keep the process's settings.
#[doc(hidden)]
pub fn with_test_settings<R>(
    limit: Option<Duration>,
    hang: Option<Duration>,
    f: impl FnOnce() -> R,
) -> R {
    struct Restore(Option<(Option<Duration>, Option<Duration>)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            THREAD_SETTINGS.with(|s| s.set(self.0));
        }
    }
    let _restore = Restore(THREAD_SETTINGS.with(|s| s.replace(Some((limit, hang)))));
    f()
}

/// Waits a test hang, deaf to every cancel.
fn test_hang(hang: Option<Duration>) {
    if let Some(hang) = hang {
        std::thread::sleep(hang);
    }
}

/// How many calls given up still run: their kernel call has not returned.
pub fn abandoned_kernel_calls() -> usize {
    ABANDONED.load(Ordering::SeqCst)
}

/// The failure of a call given up at the time limit, for the feature: what
/// went wrong in words (mitcad#1917). `limit` in whole seconds, at least 1.
pub fn time_limit_message(limit: Duration) -> String {
    let seconds = limit.as_secs_f64().round().max(1.0);
    format!("the geometry kernel did not finish within {seconds} s; the operation was given up")
}

/// The failure of a call given up on a cancel (as the geometry's own
/// `Cancelled`): the recompute drops it as cancelled.
pub const CANCELLED_MESSAGE: &str = "the operation was cancelled";

/// Where a call is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Running,
    /// Given up while running: counted in [`ABANDONED`] until it returns.
    Abandoned,
    Done,
}

fn lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs `call`, a kernel call that may not return (the module lists them),
/// so that the evaluation's thread can give it up (see the
/// module): with the monitor of the [`cancel::interruptible`] it runs in,
/// it fails as cancelled within [`POLL`] of a cancel; after
/// [`kernel_time_limit`] it fails with [`time_limit_message`]. Without a
/// monitor and a limit it runs on this thread. `call` runs on a thread with
/// the import's worker stack and OCCT's crash handlers, its geometry stopped
/// by the monitor and by its being given up; a panic in it is its failure.
pub fn abandonable<T: Send + 'static>(
    call: impl FnOnce() -> Result<T, KernelError> + Send + 'static,
) -> Result<T, KernelError> {
    let stopping = cancel::current();
    let limit = kernel_time_limit();
    let hang = test_hang_time();
    if stopping.is_none() && limit.is_none() {
        test_hang(hang);
        return call();
    }
    let (monitor, progress) = match stopping {
        Some((monitor, progress)) => (Some(monitor), progress),
        None => (None, None),
    };
    // The call's own monitor: cancelled with the evaluation's, and when
    // the call is given up, so that the geometry's operations inside stop.
    let own = Arc::new(RecomputeMonitor::within(
        monitor.iter().cloned().collect(),
        None,
    ));
    let state = Arc::new(Mutex::new(State::Running));
    let (sender, receiver) = mpsc::sync_channel::<Result<T, KernelError>>(1);
    let call = Arc::new(Mutex::new(Some(call)));
    let started = {
        let own = Arc::clone(&own);
        let state = Arc::clone(&state);
        let call = Arc::clone(&call);
        std::thread::Builder::new()
            .name("mitcad-kernel-call".to_owned())
            .stack_size(mitcad_import::WORKER_STACK)
            .spawn(move || {
                crate::kernel::exchange::ffi::catch_occt_crashes();
                let taken = call.lock().unwrap_or_else(PoisonError::into_inner).take();
                let result = taken.map_or_else(
                    || Err(KernelError::failed(CANCELLED_MESSAGE)),
                    |call| {
                        catch_unwind(AssertUnwindSafe(|| {
                            cancel::interruptible_with(&own, progress, || {
                                test_hang(hang);
                                call()
                            })
                        }))
                        .unwrap_or_else(|_| {
                            Err(KernelError::failed(
                                "the geometry kernel's operation ended unexpectedly",
                            ))
                        })
                    },
                );
                // (Sent before the call counts as done: a waiter that finds
                // it done finds its result.)
                let _ = sender.send(result);
                let mut state = lock(&state);
                if *state == State::Abandoned {
                    ABANDONED.fetch_sub(1, Ordering::SeqCst);
                }
                *state = State::Done;
            })
    };
    if started.is_err() {
        // No stack for the thread: the call runs here, as before.
        let taken = call.lock().unwrap_or_else(PoisonError::into_inner).take();
        return match taken {
            Some(call) => {
                test_hang(hang);
                call()
            }
            None => Err(KernelError::failed(CANCELLED_MESSAGE)),
        };
    }
    let start = Instant::now();
    loop {
        match receiver.recv_timeout(POLL) {
            Ok(result) => return result,
            Err(RecvTimeoutError::Disconnected) => {
                return Err(KernelError::failed(
                    "the geometry kernel's operation ended unexpectedly",
                ));
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        let cancelled = monitor.as_ref().is_some_and(|m| m.is_cancelled());
        let over = limit.filter(|limit| start.elapsed() >= *limit);
        if !cancelled && over.is_none() {
            continue;
        }
        {
            let mut state = lock(&state);
            if *state == State::Running {
                *state = State::Abandoned;
                ABANDONED.fetch_add(1, Ordering::SeqCst);
            }
        }
        own.cancel();
        // A call that returned meanwhile: its result.
        if let Ok(result) = receiver.try_recv() {
            let mut state = lock(&state);
            if *state == State::Abandoned {
                ABANDONED.fetch_sub(1, Ordering::SeqCst);
                *state = State::Done;
            }
            drop(state);
            if !cancelled {
                return result;
            }
        }
        return Err(match over {
            Some(limit) if !cancelled => KernelError::Failed(time_limit_message(limit)),
            _ => KernelError::failed(CANCELLED_MESSAGE),
        });
    }
}

/// The settings for C++ (`mitcad_bridge/lib.h`), in seconds, 0 for none.
pub mod bridge {
    use std::time::Duration;

    /// Seconds of a setting from C++: none for 0, a negative or no number.
    fn duration(seconds: f64) -> Option<Duration> {
        (seconds.is_finite() && seconds > 0.0).then(|| Duration::from_secs_f64(seconds.min(1.0e9)))
    }

    pub fn kernel_time_limit() -> f64 {
        super::kernel_time_limit().map_or(0.0, |l| l.as_secs_f64())
    }

    pub fn set_kernel_time_limit(seconds: f64) {
        super::set_kernel_time_limit(duration(seconds));
    }

    pub fn set_test_kernel_hang(seconds: f64) {
        super::set_test_hang(duration(seconds));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The calls given up are counted in the whole process: one test that
    /// counts them at a time.
    static COUNTING: Mutex<()> = Mutex::new(());

    /// Waits until the calls earlier tests gave up have returned.
    fn settle() {
        let deadline = Instant::now() + Duration::from_secs(10);
        while abandoned_kernel_calls() > 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_call_that_returns_gives_its_result() {
        with_test_settings(Some(Duration::from_secs(60)), None, || {
            assert_eq!(abandonable(|| Ok(7)), Ok(7));
            assert_eq!(
                abandonable::<i32>(|| Err(KernelError::failed("no"))),
                Err(KernelError::failed("no"))
            );
            let monitor = Arc::new(RecomputeMonitor::new());
            let got = cancel::interruptible(&monitor, || abandonable(|| Ok("yes")));
            assert_eq!(got, Ok("yes"));
        });
        // Without a monitor and a limit, on this thread.
        let here = std::thread::current().id();
        let ran = with_test_settings(None, None, || {
            abandonable(move || Ok(std::thread::current().id()))
        });
        assert_eq!(ran, Ok(here));
    }

    #[test]
    fn a_cancel_gives_up_a_call_that_does_not_return_at_once() {
        let _counting = COUNTING.lock().unwrap_or_else(PoisonError::into_inner);
        settle();
        let monitor = Arc::new(RecomputeMonitor::new());
        let canceller = {
            let monitor = Arc::clone(&monitor);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                monitor.cancel();
                Instant::now()
            })
        };
        let before = abandoned_kernel_calls();
        let got = with_test_settings(None, Some(Duration::from_secs(2)), || {
            cancel::interruptible(&monitor, || abandonable(|| Ok(1)))
        });
        let returned = Instant::now();
        let cancelled = canceller.join().unwrap();
        assert_eq!(got, Err(KernelError::failed(CANCELLED_MESSAGE)));
        assert!(
            returned.duration_since(cancelled) < Duration::from_secs(1),
            "returned {:?} after the cancel",
            returned.duration_since(cancelled)
        );
        assert!(abandoned_kernel_calls() > before);
    }

    #[test]
    fn the_time_limit_fails_a_call_in_words() {
        let _counting = COUNTING.lock().unwrap_or_else(PoisonError::into_inner);
        let start = Instant::now();
        let got = with_test_settings(
            Some(Duration::from_millis(400)),
            Some(Duration::from_secs(2)),
            || abandonable(|| Ok(1)),
        );
        let took = start.elapsed();
        assert_eq!(
            got,
            Err(KernelError::Failed(time_limit_message(
                Duration::from_millis(400)
            )))
        );
        assert!(
            took >= Duration::from_millis(400) && took < Duration::from_secs(2),
            "{took:?}"
        );
        assert_eq!(
            time_limit_message(Duration::from_secs(300)),
            "the geometry kernel did not finish within 300 s; the operation was given up"
        );
    }

    #[test]
    fn a_given_up_call_counts_until_it_returns() {
        let _counting = COUNTING.lock().unwrap_or_else(PoisonError::into_inner);
        settle();
        let before = abandoned_kernel_calls();
        let got = with_test_settings(
            Some(Duration::from_millis(100)),
            Some(Duration::from_millis(600)),
            || abandonable(|| Ok(1)),
        );
        assert!(got.is_err());
        assert_eq!(abandoned_kernel_calls(), before + 1);
        let deadline = Instant::now() + Duration::from_secs(5);
        while abandoned_kernel_calls() > before && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(abandoned_kernel_calls(), before);
    }
}
