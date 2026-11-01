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
//!
//! A monitor can also be part of others ([`RecomputeMonitor::within`]):
//! it is cancelled once one of them is, or once its deadline passes (the
//! `.f3d` import gives each definition it tries the time left of the
//! item's, mitcad#69). Deadlines are on the import's clock ([`now`],
//! mitcad#874): the time the process's work took, without the time its
//! threads waited for a processor that other work held.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

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
    /// Cancelled with any of these ([`RecomputeMonitor::within`]).
    parents: Vec<Arc<RecomputeMonitor>>,
    /// Cancelled once this passes, on the clock of [`now`].
    deadline: Option<Instant>,
    /// A question the job puts to its user, and the user's answer
    /// ([`RecomputeMonitor::ask`]).
    question: Mutex<Question>,
}

/// A job's question to its user ([`RecomputeMonitor::ask`]).
#[derive(Debug, Default)]
struct Question {
    /// Asked and not answered yet.
    asked: Option<String>,
    /// Given and not taken yet.
    answer: Option<String>,
}

/// A recompute stopped on request; nothing it was part of took effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl Cancelled {
    /// The message of a command rejected because it was cancelled.
    pub const MESSAGE: &'static str = "the computation was canceled";
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
            parents: Vec::new(),
            deadline: None,
            question: Mutex::new(Question::default()),
        }
    }
}

impl RecomputeMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    /// A monitor for part of the work of others: cancelled once one of
    /// `parents` is, or once `deadline` passes (and when cancelled itself).
    /// It counts its own evaluations, and slows them down by the longest
    /// test delay of its parents.
    pub fn within(parents: Vec<Arc<RecomputeMonitor>>, deadline: Option<Instant>) -> Self {
        let delay = parents
            .iter()
            .map(|p| p.delay_ms.load(Ordering::Relaxed))
            .max()
            .unwrap_or(0);
        Self {
            delay_ms: AtomicU32::new(delay),
            parents,
            deadline,
            ..Self::default()
        }
    }

    /// Whether its deadline passed ([`RecomputeMonitor::within`]; on the
    /// clock of [`now`]).
    pub fn past_deadline(&self) -> bool {
        self.deadline.is_some_and(|d| now() >= d)
    }

    /// Asks the recompute to stop: it does before the next feature, or
    /// when the evaluation running now returns, which the kernel's long
    /// operations cut short (P7e, `Kernel::interruptible`).
    pub fn cancel(&self) {
        // Nothing is published with the flag, so no ordering is needed.
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Whether it was cancelled, one of its parents was, or its deadline
    /// passed ([`RecomputeMonitor::within`]). Once true it stays so.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
            || self.past_deadline()
            || self.parents.iter().any(|p| p.is_cancelled())
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

    /// For tests: every evaluation, and every result restored from the
    /// result store (mitcad#887), takes `ms` longer, so that a test can
    /// watch the progress and cancel a recompute of a small model. The
    /// delay ends early on a cancel, and the evaluation is then dropped as
    /// one cancelled while it ran. The recompute times leave it out.
    pub fn set_test_delay(&self, ms: u32) {
        self.delay_ms.store(ms, Ordering::Relaxed);
    }

    /// For tests: waits the test delay (`set_test_delay`), or until a
    /// cancel, where a job that evaluates no features does a step of its
    /// work (an item of a `.dwg` import, mitcad#589), so that a test can
    /// cancel it while it runs.
    pub fn test_pause(&self) {
        let delay = Duration::from_millis(u64::from(self.delay_ms.load(Ordering::Relaxed)));
        let mut slept = Duration::ZERO;
        while slept < delay && !self.is_cancelled() {
            let step = DELAY_STEP.min(delay - slept);
            thread::sleep(step);
            slept += step;
        }
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
    pub fn enter(&self, position: usize, total: usize) -> Result<(), Cancelled> {
        self.position.store(position, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
        self.check()
    }

    /// A feature is about to be evaluated (a cache miss).
    pub fn evaluating(&self, name: &str) {
        let mut feature = self.feature.lock().unwrap_or_else(PoisonError::into_inner);
        feature.clear();
        feature.push_str(name);
    }

    /// An evaluation returned: after the test delay, tells whether it may
    /// be kept (no cancel came while it ran).
    pub fn evaluation_done(&self) -> Result<(), Cancelled> {
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

    /// Puts `question` to the job's user (the `.f3d` import asks what to do
    /// with the bodies it already imported, mitcad#283): the thread that
    /// shows the progress finds it with [`RecomputeMonitor::question`] and
    /// gives the user's answer with [`RecomputeMonitor::answer`], which the
    /// job takes with [`RecomputeMonitor::take_answer`]. A new question
    /// replaces one not answered and drops an answer not taken.
    pub fn ask(&self, question: &str) {
        let mut q = self.question.lock().unwrap_or_else(PoisonError::into_inner);
        q.asked = Some(question.to_owned());
        q.answer = None;
    }

    /// The question asked and not answered yet ([`RecomputeMonitor::ask`]).
    pub fn question(&self) -> Option<String> {
        self.question
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .asked
            .clone()
    }

    /// Answers the question asked; without one it does nothing.
    pub fn answer(&self, answer: &str) {
        let mut q = self.question.lock().unwrap_or_else(PoisonError::into_inner);
        if q.asked.take().is_some() {
            q.answer = Some(answer.to_owned());
        }
    }

    /// The answer given to the question, once (None before it is given).
    pub fn take_answer(&self) -> Option<String> {
        self.question
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .answer
            .take()
    }

    /// An evaluation went into the cache.
    pub fn kept(&self) {
        let kept = self.evaluated.fetch_add(1, Ordering::Relaxed) + 1;
        if kept >= self.cancel_after.load(Ordering::Relaxed) {
            self.cancel();
        }
    }

    /// The recompute went through all `total` features before the marker.
    pub fn finish(&self, total: usize) {
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

/// The import's clock (mitcad#874): an instant that moves as the wall
/// clock does while the process's threads get the processors they want,
/// and more slowly while they wait for processors that other work holds.
/// The time limits of a design's import (an item's time and its shares,
/// a definition's own time, the deadlines of [`RecomputeMonitor::within`])
/// are on it, so that what an import decides does not hang on the load
/// beside it: on an idle machine it is the wall clock's time, and a
/// definition that takes 14 s alone still takes about 14 s of it when
/// other processes keep it waiting half the time. Its instants are only
/// compared with each other (and [`since`]); a wall clock's instant is not
/// one of them.
///
/// Linux measures it: every [`PACE_STEP`] the clock takes, over all the
/// process's threads (`/proc/self/task/*/schedstat`), the time they ran
/// and the time they waited runnable for a processor, and moves on by
/// that interval times the share they ran. A thread blocked (waiting for
/// a lock, a worker or the user) counts as the wall clock does. A thread
/// that runs while the other logical processor of its core is busy too
/// (simultaneous multithreading) runs slower without waiting: once more
/// logical processors are busy than the machine has cores
/// (`/proc/stat`), the clock moves more slowly by the share of the
/// processors that share a core, down to [`SHARED_CORE_SPEED`] when all
/// are busy. Elsewhere it is the wall clock.
pub fn now() -> Instant {
    if let Some(at) = PACE_FOR_TESTS.with(|p| p.get()).map(|(start, rate)| {
        start
            + Instant::now()
                .saturating_duration_since(start)
                .mul_f64(rate)
    }) {
        return at;
    }
    if WALL_FOR_TESTS.load(Ordering::Relaxed) {
        return Instant::now();
    }
    PACE.get_or_init(Pace::new).now()
}

/// For tests: [`now`] is the wall clock in the whole process from this
/// call on, on every thread (a thread's [`pace_for_tests`] still goes
/// first), so that a test whose outcome hangs on a time limit runs out of
/// it after the same wall time however busy the machine is.
#[doc(hidden)]
pub fn wall_pace_for_tests() {
    WALL_FOR_TESTS.store(true, Ordering::Relaxed);
}

static WALL_FOR_TESTS: AtomicBool = AtomicBool::new(false);

/// The time on [`now`]'s clock since `instant` (one of its instants).
pub fn since(instant: Instant) -> Duration {
    now().saturating_duration_since(instant)
}

/// For tests: on the calling thread, [`now`] moves at `rate` times the
/// wall clock from this call on (as under a load that leaves the process
/// that share of the processors); None goes back to the measured clock.
#[doc(hidden)]
pub fn pace_for_tests(rate: Option<f64>) {
    PACE_FOR_TESTS.with(|p| p.set(rate.map(|r| (Instant::now(), r))));
}

thread_local! {
    static PACE_FOR_TESTS: std::cell::Cell<Option<(Instant, f64)>> =
        const { std::cell::Cell::new(None) };
}

/// How often [`now`]'s clock measures the process's share of the
/// processors at most (in between it moves at the share last measured).
pub const PACE_STEP: Duration = Duration::from_millis(100);

/// The least share [`now`]'s clock moves at: a process that gets no
/// processor at all for a while still has its limits pass in the end.
const PACE_FLOOR: f64 = 0.02;

/// How fast a thread runs, against a core to itself, while the other
/// logical processor of its core is busy (and the cores are clocked down
/// as when all are busy): a pattern of threads that took 34 s alone took
/// 66 s of the time it ran beside 32 busy processes on 16 cores with two
/// logical processors each (mitcad#874).
pub const SHARED_CORE_SPEED: f64 = 0.5;

static PACE: OnceLock<Pace> = OnceLock::new();

/// [`now`]'s clock.
struct Pace {
    /// Its instants are this plus the clock's time.
    base: Instant,
    state: Mutex<PaceState>,
}

struct PaceState {
    /// The wall clock at the last measurement, the clock's time then and
    /// the share of the processors the process got in the interval before.
    wall: Instant,
    at: Duration,
    rate: f64,
    /// The latest time given out: the clock never goes back.
    given: Duration,
    /// Each thread's time run and time waited for a processor
    /// (nanoseconds) at the last measurement.
    threads: std::collections::HashMap<u64, (u64, u64)>,
    /// The machine's logical processors and cores, and its processors'
    /// busy and total time (`/proc/stat`) at the last measurement.
    logical: f64,
    cores: f64,
    machine: (u64, u64),
}

impl Pace {
    fn new() -> Self {
        let mut threads = std::collections::HashMap::new();
        sample_threads(&mut threads);
        let (logical, cores) = processors();
        let machine = machine_times();
        let wall = Instant::now();
        Self {
            base: wall,
            state: Mutex::new(PaceState {
                wall,
                at: Duration::ZERO,
                rate: 1.0,
                given: Duration::ZERO,
                threads,
                logical,
                cores,
                machine,
            }),
        }
    }

    fn now(&self) -> Instant {
        let wall = Instant::now();
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let step = wall.saturating_duration_since(s.wall);
        if step >= PACE_STEP {
            let (ran, waited) = sample_threads(&mut s.threads);
            let machine = machine_times();
            let (busy, total) = (
                machine.0.saturating_sub(s.machine.0),
                machine.1.saturating_sub(s.machine.1),
            );
            s.machine = machine;
            let busy = if total == 0 {
                0.0
            } else {
                s.logical * busy as f64 / total as f64
            };
            let rate = (pace_rate(ran, waited) * core_speed(busy, s.logical, s.cores))
                .clamp(PACE_FLOOR, 1.0);
            s.rate = rate;
            s.at += step.mul_f64(rate);
            s.wall = wall;
        }
        let at = s.at + wall.saturating_duration_since(s.wall).mul_f64(s.rate);
        let at = at.max(s.given);
        s.given = at;
        self.base + at
    }
}

/// The share of the processors a process got whose threads ran `ran` and
/// waited runnable `waited` (nanoseconds, together) in an interval: 1
/// when they neither ran nor waited (all blocked), never below
/// [`PACE_FLOOR`].
pub fn pace_rate(ran: u64, waited: u64) -> f64 {
    let wanted = ran.saturating_add(waited);
    if wanted == 0 {
        return 1.0;
    }
    (ran as f64 / wanted as f64).clamp(PACE_FLOOR, 1.0)
}

/// How fast a thread runs against a core to itself when `busy` of the
/// machine's `logical` processors on its `cores` are busy: 1 while no
/// more are busy than there are cores (each can have one to itself), then
/// down to [`SHARED_CORE_SPEED`] as the busy ones beyond the cores share
/// them, all at `logical`.
pub fn core_speed(busy: f64, logical: f64, cores: f64) -> f64 {
    if logical <= cores || busy <= cores {
        return 1.0;
    }
    let shared = ((busy - cores) / (logical - cores)).clamp(0.0, 1.0);
    1.0 - (1.0 - SHARED_CORE_SPEED) * shared
}

/// The machine's logical processors and cores (Linux: the `cpuN` lines
/// of `/proc/stat` and the first one's siblings in its topology); the
/// same number twice where they cannot be read (no core is shared).
fn processors() -> (f64, f64) {
    #[cfg(target_os = "linux")]
    {
        let logical = std::fs::read_to_string("/proc/stat")
            .map(|t| {
                t.lines()
                    .filter(|l| {
                        l.starts_with("cpu") && l.as_bytes().get(3).is_some_and(u8::is_ascii_digit)
                    })
                    .count()
            })
            .unwrap_or(0);
        // "0,16" or "0-1": the logical processors of the first core.
        let siblings =
            std::fs::read_to_string("/sys/devices/system/cpu/cpu0/topology/thread_siblings_list")
                .map(|t| {
                    t.trim()
                        .split(',')
                        .map(|part| match part.split_once('-') {
                            Some((a, b)) => match (a.parse::<usize>(), b.parse::<usize>()) {
                                (Ok(a), Ok(b)) if b >= a => b - a + 1,
                                _ => 1,
                            },
                            None => 1,
                        })
                        .sum::<usize>()
                })
                .unwrap_or(1)
                .max(1);
        if logical > 0 {
            return (logical as f64, (logical / siblings).max(1) as f64);
        }
    }
    (1.0, 1.0)
}

/// The machine's processors' busy and total time so far (Linux: the
/// `cpu` line of `/proc/stat`, in its ticks; idle and waiting for
/// input are not busy); (0, 0) where it cannot be read.
fn machine_times() -> (u64, u64) {
    #[cfg(target_os = "linux")]
    {
        if let Ok(text) = std::fs::read_to_string("/proc/stat")
            && let Some(line) = text.lines().next()
            && line.starts_with("cpu ")
        {
            let fields: Vec<u64> = line
                .split_whitespace()
                .skip(1)
                .filter_map(|f| f.parse().ok())
                .collect();
            // user nice system idle iowait irq softirq steal (guest
            // times are counted in user and nice already).
            let total: u64 = fields.iter().take(8).sum();
            let idle = fields.get(3).copied().unwrap_or(0) + fields.get(4).copied().unwrap_or(0);
            return (total.saturating_sub(idle), total);
        }
    }
    (0, 0)
}

/// Reads every thread's time run and time waited for a processor into
/// `threads` (by thread id), and gives the growth of both, summed, since
/// the last reading (a new thread's from its start; the threads that
/// ended since are dropped). (0, 0) where it cannot be read.
fn sample_threads(threads: &mut std::collections::HashMap<u64, (u64, u64)>) -> (u64, u64) {
    #[cfg(target_os = "linux")]
    {
        let Ok(dir) = std::fs::read_dir("/proc/self/task") else {
            return (0, 0);
        };
        let mut seen = std::collections::HashMap::with_capacity(threads.len());
        let (mut ran, mut waited) = (0u64, 0u64);
        for entry in dir.flatten() {
            let Some(tid) = entry
                .file_name()
                .to_str()
                .and_then(|t| t.parse::<u64>().ok())
            else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(entry.path().join("schedstat")) else {
                continue;
            };
            let mut fields = text.split_whitespace().map(|f| f.parse::<u64>().ok());
            let (Some(Some(run)), Some(Some(wait))) = (fields.next(), fields.next()) else {
                continue;
            };
            let (run0, wait0) = threads.get(&tid).copied().unwrap_or((0, 0));
            ran = ran.saturating_add(run.saturating_sub(run0));
            waited = waited.saturating_add(wait.saturating_sub(wait0));
            seen.insert(tid, (run, wait));
        }
        *threads = seen;
        (ran, waited)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = threads;
        (0, 0)
    }
}
