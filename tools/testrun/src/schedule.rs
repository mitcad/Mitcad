// SPDX-License-Identifier: MIT
//! The scheduling decisions as pure functions: which waiting test may
//! start now (memory budget, CPU gate, locks, the priority lane's reserve,
//! timing-sensitive tests), which of them starts first (longest first with
//! complementary pairing, the lane before the rest), each test's hard
//! memory limit, how a run is classified, and the run's critical path.

use crate::concurrency::admits;
use crate::ctest::Status;
use crate::profile::{Prediction, ResultKind};

/// The machine's limits.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// The hard limit on the running tests' predicted memory, bytes.
    pub budget: u64,
    /// The machine's cores.
    pub cores: f64,
    /// No test starts above this system load (0 to 1).
    pub cpu_gate: f64,
    /// A timing-sensitive test starts only below this load.
    pub sensitive_load: f64,
    /// Timing-sensitive tests at once.
    pub sensitive_max: usize,
    /// UI entries at once: the machine's cap, the bound (`concurrency`).
    pub ui_max: usize,
    /// The share of the budget kept for the priority lane while lane tests
    /// wait.
    pub lane_share: f64,
    /// Tests at once.
    pub max_parallel: usize,
    /// Pairing considers the waiting tests whose rank is at least this
    /// share of the longest one's.
    pub pair_window: f64,
    /// How much the pairing's balance weighs against the rank.
    pub pair_weight: f64,
    /// No test starts while the machine has less memory available, bytes.
    pub memory_floor: u64,
}

/// A test that waits to start and whose dependencies are done.
#[derive(Clone, Debug)]
pub struct Candidate<'a> {
    pub prediction: Prediction,
    /// Its predicted wall time plus the longest chain of tests waiting for
    /// it (longest first).
    pub rank: f64,
    /// Its failure score.
    pub score: f64,
    /// In the priority lane.
    pub lane: bool,
    pub sensitive: bool,
    /// A UI entry (`concurrency`), and the other UI entries it may have
    /// beside it (None: no limit of its own).
    pub ui: bool,
    pub ui_room: Option<usize>,
    /// Runs alone: RUN_SERIAL, or a rerun after its memory limit.
    pub alone: bool,
    pub locks: &'a [String],
}

/// What runs now, by prediction.
#[derive(Clone, Debug, Default)]
pub struct Running {
    pub count: usize,
    pub memory: u64,
    /// The memory of the tests outside the lane.
    pub other_memory: u64,
    pub cores: f64,
    pub sensitive: usize,
    pub lane: usize,
    pub alone: bool,
    pub locks: Vec<String>,
    /// The UI entries running, and the smallest room among them.
    pub ui: usize,
    pub ui_room: Option<usize>,
}

impl Running {
    pub fn add(&mut self, c: &Candidate) {
        self.count += 1;
        self.memory += c.prediction.memory;
        if c.lane {
            self.lane += 1;
        } else {
            self.other_memory += c.prediction.memory;
        }
        self.cores += c.prediction.cores;
        if c.sensitive {
            self.sensitive += 1;
        }
        self.alone |= c.alone;
        self.locks.extend(c.locks.iter().cloned());
        if c.ui {
            self.ui += 1;
            self.ui_room = match (self.ui_room, c.ui_room) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
    }
}

/// The machine now.
#[derive(Clone, Copy, Debug, Default)]
pub struct Now {
    /// The system's measured CPU load, 0 to 1.
    pub load: f64,
    /// Physical memory available, bytes, where the platform tells.
    pub available: Option<u64>,
    /// Tests of the priority lane wait to start.
    pub lane_waiting: bool,
}

/// Why a test does not start now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A test that runs alone runs, or this one must run alone.
    Alone,
    /// The most tests at once run.
    Parallel,
    /// A running test holds one of its resource locks.
    Lock,
    /// Its predicted memory does not fit the budget.
    Memory,
    /// The rest of the budget is kept for the priority lane.
    LaneReserve,
    /// The machine's available memory is below the floor.
    Available,
    /// The system's load is above the gate.
    Cpu,
    /// A timing-sensitive test waits for a calmer machine.
    Sensitive,
    /// The machine's UI entries at once run, or a UI entry would have more
    /// beside it than its room.
    Ui,
}

/// The load the scheduler goes by: the measured one, or the running
/// tests' predicted cores where higher (a test just started has not yet
/// loaded the processors).
pub fn effective_load(running: &Running, now: &Now, limits: &Limits) -> f64 {
    now.load.max(running.cores / limits.cores.max(1.0))
}

/// The free budget (`--memory-budget free`, mitcad#872), at a decision:
/// the running tests' predicted memory plus what the machine has
/// available beyond the reserve, less what the running tests are still
/// predicted to grow into (`pending`: a test just started has not yet
/// taken its memory, which `available` does not show), never above `cap`.
/// So a candidate fits when its prediction is within the available memory
/// less the reserve and the running tests' pending growth: when another
/// job (an agent's build) takes memory, fewer tests start; when it ends,
/// more. Without a reading of the available memory: the cap.
pub fn free_budget(
    cap: u64,
    running_memory: u64,
    pending: u64,
    available: Option<u64>,
    reserve: u64,
) -> u64 {
    let Some(available) = available else {
        return cap;
    };
    let room = available.saturating_sub(reserve).saturating_sub(pending);
    running_memory.saturating_add(room).min(cap)
}

/// What a running test may still grow into: its predicted memory less
/// the working set it has now (all of it where that is not known).
pub fn pending(predicted: u64, current: Option<u64>) -> u64 {
    predicted.saturating_sub(current.unwrap_or(0))
}

/// The rules every mode keeps: tests that run alone, resource locks.
fn admit_common(c: &Candidate, running: &Running) -> Result<(), Refusal> {
    if running.count == 0 {
        return Ok(());
    }
    if running.alone || c.alone {
        return Err(Refusal::Alone);
    }
    if c.locks.iter().any(|lock| running.locks.contains(lock)) {
        return Err(Refusal::Lock);
    }
    Ok(())
}

/// Measuring mode: a fixed number of tests at once, as ctest -j does.
pub fn admit_fixed(c: &Candidate, running: &Running, jobs: usize) -> Result<(), Refusal> {
    admit_common(c, running)?;
    if running.count > 0 && running.count >= jobs {
        return Err(Refusal::Parallel);
    }
    Ok(())
}

/// Scheduling mode: whether the test may start now. With nothing running
/// any test starts, so that a test larger than the budget still runs
/// (alone).
pub fn admit(c: &Candidate, running: &Running, now: &Now, limits: &Limits) -> Result<(), Refusal> {
    admit_common(c, running)?;
    if running.count == 0 {
        return Ok(());
    }
    // One slot is kept for the lane while lane tests wait.
    let slots = if !c.lane && now.lane_waiting && running.lane == 0 {
        limits.max_parallel.saturating_sub(1).max(1)
    } else {
        limits.max_parallel
    };
    if running.count >= slots {
        return Err(Refusal::Parallel);
    }
    let memory = c.prediction.memory;
    if running.memory.saturating_add(memory) > limits.budget {
        return Err(Refusal::Memory);
    }
    if !c.lane && now.lane_waiting {
        let others = (limits.budget as f64 * (1.0 - limits.lane_share)) as u64;
        if running.other_memory.saturating_add(memory) > others {
            return Err(Refusal::LaneReserve);
        }
    }
    // The real memory, re-read at each decision: in a VM with dynamic
    // memory the available memory grows with the demand, so only a floor,
    // not the candidate's whole prediction, is asked of it.
    if now
        .available
        .is_some_and(|available| available < limits.memory_floor)
    {
        return Err(Refusal::Available);
    }
    let load = effective_load(running, now, limits);
    if load >= limits.cpu_gate {
        return Err(Refusal::Cpu);
    }
    if c.sensitive && (load >= limits.sensitive_load || running.sensitive >= limits.sensitive_max) {
        return Err(Refusal::Sensitive);
    }
    if c.ui && !admits(running.ui, running.ui_room, c.ui_room, limits.ui_max) {
        return Err(Refusal::Ui);
    }
    Ok(())
}

/// How evenly the test would fill what is left of the memory budget and
/// the processors: 0 when both are left in the same share, down to -1.
pub fn balance(c: &Candidate, running: &Running, now: &Now, limits: &Limits) -> f64 {
    let budget = limits.budget.max(1) as f64;
    let free_memory = 1.0 - (running.memory + c.prediction.memory) as f64 / budget;
    let load = effective_load(running, now, limits);
    let free_cpu = 1.0 - load - c.prediction.cores / limits.cores.max(1.0);
    -(free_memory - free_cpu).abs().min(1.0)
}

/// Which of the candidates (all admissible) starts first: the lane's
/// highest failure score, else among the longest ranks (within the
/// pairing window of the longest) the one that best balances memory and
/// processors. The index into `candidates`.
pub fn pick(
    candidates: &[Candidate],
    running: &Running,
    now: &Now,
    limits: &Limits,
) -> Option<usize> {
    // The best by (value, rank); the first of equals.
    let mut lane: Option<(usize, f64, f64)> = None;
    for (index, c) in candidates.iter().enumerate() {
        if c.lane && better(c.score, c.rank, lane) {
            lane = Some((index, c.score, c.rank));
        }
    }
    if let Some((index, _, _)) = lane {
        return Some(index);
    }
    let best = candidates.iter().map(|c| c.rank).fold(0.0, f64::max);
    if best <= 0.0 {
        return (!candidates.is_empty()).then_some(0);
    }
    let mut chosen: Option<(usize, f64, f64)> = None;
    for (index, c) in candidates.iter().enumerate() {
        if c.rank < best * limits.pair_window {
            continue;
        }
        let value = c.rank / best + limits.pair_weight * balance(c, running, now, limits);
        if better(value, c.rank, chosen) {
            chosen = Some((index, value, c.rank));
        }
    }
    chosen.map(|(index, _, _)| index)
}

/// Whether (value, rank) is better than the best so far.
fn better(value: f64, rank: f64, best: Option<(usize, f64, f64)>) -> bool {
    match best {
        None => true,
        Some((_, v, r)) => value > v || (value >= v && rank > r),
    }
}

/// The first claim on memory (mitcad#2177): of the ready tests that may
/// start now or that only memory keeps waiting (`refusals`, `admit`'s
/// answers in the same order), the one ranked highest, where its
/// prediction is within `cap` (the most the budget can be: a test larger
/// than that waits for an empty machine as before). The index into
/// `candidates`. The tests ranked below it start only where they leave it
/// its room (`leaves_room`), so that the test at the head of the longest
/// chain (the cargo test build, which the Rust tests wait for) starts as
/// soon as the memory comes free instead of smaller tests taking it again
/// and again (in a bundle check it waited 9 to 12 minutes and then ran
/// nearly alone for 14).
pub fn claimant(
    candidates: &[Candidate],
    refusals: &[Result<(), Refusal>],
    cap: u64,
) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (index, (c, refusal)) in candidates.iter().zip(refusals).enumerate() {
        if !matches!(refusal, Ok(()) | Err(Refusal::Memory)) || c.prediction.memory > cap {
            continue;
        }
        match best {
            Some(b) if candidates[b].rank >= c.rank => {}
            _ => best = Some(index),
        }
    }
    best
}

/// Whether a candidate (other than the claimant) may start beside the
/// claim: when its prediction and the claimant's fit the budget beside the
/// running tests. A test of the priority lane passes the claim of a test
/// outside it (the lane goes first, as in `pick`).
pub fn leaves_room(
    c: &Candidate,
    claimant: &Candidate,
    running: &Running,
    limits: &Limits,
) -> bool {
    if c.lane && !claimant.lane {
        return true;
    }
    running
        .memory
        .saturating_add(c.prediction.memory)
        .saturating_add(claimant.prediction.memory)
        <= limits.budget
}

/// Each test's rank: its predicted wall time plus the largest rank of the
/// tests that wait for it (`successors`), so that the tests at the head of
/// long chains start first. A cycle counts as no successor.
pub fn ranks(walls: &[f64], successors: &[Vec<usize>]) -> Vec<f64> {
    fn visit(i: usize, walls: &[f64], next: &[Vec<usize>], state: &mut [u8], out: &mut [f64]) {
        if state[i] != 0 {
            return;
        }
        state[i] = 1;
        let mut tail: f64 = 0.0;
        for &s in &next[i] {
            visit(s, walls, next, state, out);
            if state[s] == 2 {
                tail = tail.max(out[s]);
            }
        }
        out[i] = walls[i] + tail;
        state[i] = 2;
    }
    let mut state = vec![0u8; walls.len()];
    let mut out = vec![0.0; walls.len()];
    for i in 0..walls.len() {
        visit(i, walls, successors, &mut state, &mut out);
    }
    out
}

/// What a test's hard memory limit is decided from.
#[derive(Clone, Copy, Debug, Default)]
pub struct LimitFacts {
    /// The p90 of its history's peaks (without the margin), bytes.
    pub peak_p90: u64,
    /// The largest peak of its history over the smallest (1 without one).
    pub spread: f64,
    /// The guess for its kind (`profile::kind_defaults`), bytes.
    pub kind_memory: u64,
    /// RUN_SERIAL: it runs alone, nothing to protect beside it.
    pub serial: bool,
    /// The rerun alone after a memory limit.
    pub rerun: bool,
}

/// No hard limit is below this.
pub const LIMIT_FLOOR: u64 = 4 << 30;
/// A history whose largest peak is more than this many times its smallest
/// (a warm build after a cold one) predicts nothing safe to limit by.
pub const LIMIT_SPREAD: f64 = 4.0;

/// The test's hard memory limit: none for a RUN_SERIAL test, the rerun
/// after a memory limit, a history spread over more than `LIMIT_SPREAD`,
/// or a factor of 0; else the largest of 4 GiB, `factor` times the p90 of
/// its history's peaks and `factor` times its kind's guess, at most
/// `ceiling` (but never below 4 GiB).
pub fn memory_limit(facts: &LimitFacts, factor: f64, ceiling: u64) -> Option<u64> {
    if factor <= 0.0 || facts.serial || facts.rerun || facts.spread > LIMIT_SPREAD {
        return None;
    }
    let history = (facts.peak_p90 as f64 * factor) as u64;
    let kind = (facts.kind_memory as f64 * factor) as u64;
    let limit = LIMIT_FLOOR.max(history).max(kind);
    Some(limit.min(ceiling.max(LIMIT_FLOOR)))
}

/// The peak a run killed at its memory limit is recorded with: the larger
/// of the peak seen and the limit, so that the profile rises.
pub fn recorded_peak(kind: ResultKind, peak: u64, limit: Option<u64>) -> u64 {
    match (kind, limit) {
        (ResultKind::KilledForMemory, Some(limit)) => peak.max(limit),
        _ => peak,
    }
}

/// What the runner saw of a finished test.
#[derive(Clone, Copy, Debug, Default)]
pub struct Seen {
    /// ctest's result line.
    pub status: Option<Status>,
    /// ctest exited with 0.
    pub exit_ok: bool,
    /// Over its memory limit (the platform's notice, or the peak at it).
    pub over_memory: bool,
    /// Stopped by the runner after ctest's own timeout passed.
    pub stopped: bool,
    /// The disk was below its floor or the log says it was full.
    pub disk_full: bool,
}

/// The kind of a finished run.
pub fn classify(seen: &Seen) -> ResultKind {
    match seen.status {
        Some(Status::Passed) => return ResultKind::Passed,
        Some(Status::Skipped) => return ResultKind::Skipped,
        _ => {}
    }
    if seen.stopped {
        return ResultKind::Timeout;
    }
    if seen.over_memory {
        return ResultKind::KilledForMemory;
    }
    if seen.disk_full {
        return ResultKind::Environment;
    }
    match seen.status {
        Some(Status::Timeout) => ResultKind::Timeout,
        Some(_) => ResultKind::Failed,
        // No result line: ctest itself did not run the test.
        None if seen.exit_ok => ResultKind::Passed,
        None => ResultKind::Environment,
    }
}

/// Whether a test runs in the priority lane: in scheduling mode, with a
/// failure score above 0 and at least the lane's threshold.
pub fn in_lane(scheduling: bool, score: f64, threshold: f64) -> bool {
    scheduling && score > 0.0 && score >= threshold
}

/// A test's span in the run, for the critical path.
#[derive(Clone, Debug, Default)]
pub struct Span {
    pub start: f64,
    /// 0 for a test that never started.
    pub end: f64,
    /// The tests it waited for (DEPENDS, its fixtures' setup).
    pub before: Vec<usize>,
    /// Ran alone (RUN_SERIAL, or a rerun after its memory limit).
    pub alone: bool,
    pub locks: Vec<String>,
}

/// Why a test of the critical path started when it did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Why {
    /// The first of the chain.
    Start,
    /// Memory or CPU (or a slot) came free when the test before it ended.
    Resources,
    /// The test before it held one of its resource locks.
    Lock,
    /// It or the test before it runs alone.
    Serial,
    /// It waited for the test before it (DEPENDS, a fixture's setup).
    Dependency,
}

impl Why {
    pub fn label(self) -> &'static str {
        match self {
            Why::Start => "",
            Why::Resources => "waited for memory or CPU",
            Why::Lock => "waited for a resource lock",
            Why::Serial => "waited for a test that runs alone",
            Why::Dependency => "waited for its dependency",
        }
    }
}

/// One test of the critical path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link {
    pub index: usize,
    pub why: Why,
}

/// The test that held `current` back: of the tests that ended within a
/// second of its start, a dependency, a test that ran alone (or any test
/// when it runs alone), a holder of one of its locks, else any test (its
/// end freed memory or CPU); failing those, its dependency that ended
/// last before it.
fn holder(spans: &[Span], current: usize, chain: &[usize]) -> Option<(usize, Why)> {
    let span = &spans[current];
    let mut best: Option<(usize, Why, bool)> = None;
    for (p, other) in spans.iter().enumerate() {
        let end = other.end;
        let started_later = other.start > span.start;
        if p == current || chain.contains(&p) || started_later || end <= 0.0 {
            continue;
        }
        if end > span.start + 1.0 {
            continue;
        }
        let close = end >= span.start - 1.0;
        let shares_lock = other.locks.iter().any(|l| span.locks.contains(l));
        let why = if span.before.contains(&p) {
            Why::Dependency
        } else if !close {
            continue;
        } else if span.alone || other.alone {
            Why::Serial
        } else if shares_lock {
            Why::Lock
        } else {
            Why::Resources
        };
        let better = match best {
            None => true,
            Some((b, bw, bc)) => {
                let key = (close, why);
                let best_key = (bc, bw);
                key > best_key || (key == best_key && end > spans[b].end)
            }
        };
        if better {
            best = Some((p, why, close));
        }
    }
    best.map(|(p, why, _)| (p, why))
}

/// The chain of tests that ended the run: from the test that ended last,
/// back through the test that held each one back (`holder`), with the
/// reason, first to last. The time between a link's end and the next
/// one's start is the time the next one waited.
pub fn critical_path(spans: &[Span]) -> Vec<Link> {
    let later = |a: &usize, b: &usize| spans[*a].end.total_cmp(&spans[*b].end);
    let Some(mut current) = (0..spans.len()).max_by(later) else {
        return Vec::new();
    };
    if spans[current].end <= 0.0 {
        return Vec::new();
    }
    let mut chain = vec![current];
    let mut whys = vec![Why::Start];
    while let Some((p, why)) = holder(spans, current, &chain) {
        if let Some(last) = whys.last_mut() {
            *last = why;
        }
        chain.push(p);
        whys.push(Why::Start);
        current = p;
    }
    let mut links: Vec<Link> = chain
        .into_iter()
        .zip(whys)
        .map(|(index, why)| Link { index, why })
        .collect();
    links.reverse();
    links
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::GIB;

    const LIMITS: Limits = Limits {
        budget: 16 * 1024 * 1024 * 1024,
        cores: 8.0,
        cpu_gate: 0.9,
        sensitive_load: 0.7,
        sensitive_max: 2,
        ui_max: 3,
        lane_share: 0.2,
        max_parallel: 8,
        pair_window: 0.6,
        pair_weight: 0.5,
        memory_floor: 1536 * 1024 * 1024,
    };

    fn gib(n: f64) -> u64 {
        (n * GIB) as u64
    }

    fn test<'a>(wall: f64, cores: f64, memory_gib: f64) -> Candidate<'a> {
        let memory = gib(memory_gib);
        Candidate {
            prediction: Prediction {
                wall,
                cores,
                memory,
                memory_raw: memory,
                known: true,
            },
            rank: wall,
            score: 0.0,
            lane: false,
            sensitive: false,
            ui: false,
            ui_room: None,
            alone: false,
            locks: &[],
        }
    }

    fn running(tests: &[Candidate]) -> Running {
        let mut r = Running::default();
        for t in tests {
            r.add(t);
        }
        r
    }

    #[test]
    fn memory_is_the_hard_limit() {
        let now = Now::default();
        let busy = running(&[test(10.0, 1.0, 10.0)]);
        assert_eq!(admit(&test(1.0, 1.0, 6.0), &busy, &now, &LIMITS), Ok(()));
        let big = test(1.0, 1.0, 7.0);
        assert_eq!(admit(&big, &busy, &now, &LIMITS), Err(Refusal::Memory));
        // Nothing running: even a test over the budget starts, alone.
        let huge = test(1.0, 1.0, 40.0);
        assert_eq!(admit(&huge, &Running::default(), &now, &LIMITS), Ok(()));
        // The memory the machine really has left counts too: below the
        // floor nothing more starts.
        let low = Now {
            available: Some(gib(1.0)),
            ..Now::default()
        };
        let t3 = test(1.0, 1.0, 3.0);
        assert_eq!(admit(&t3, &busy, &low, &LIMITS), Err(Refusal::Available));
    }

    #[test]
    fn dynamic_memory_needs_only_the_floor() {
        // 2 GiB available now (a VM's current assignment) is above the
        // floor: a 3 GiB test within the budget starts, as the VM grows.
        let busy = running(&[test(10.0, 1.0, 4.0)]);
        let assigned = Now {
            available: Some(gib(2.0)),
            ..Now::default()
        };
        let t3 = test(1.0, 1.0, 3.0);
        assert_eq!(admit(&t3, &busy, &assigned, &LIMITS), Ok(()));
        // The budget still holds: the predicted sum is the hard limit.
        let t13 = test(1.0, 1.0, 13.0);
        assert_eq!(admit(&t13, &busy, &assigned, &LIMITS), Err(Refusal::Memory));
    }

    /// How many tests of `each` GiB start one after another with the free
    /// budget (`free_budget`) while the machine reads `available` GiB
    /// (unchanged as they start: a test just started has not yet taken
    /// its memory, its whole prediction is pending), with `others` GiB of
    /// tests running already that use `using` GiB of it now.
    fn free_starts(available: f64, others: f64, using: f64, each: f64) -> usize {
        let cap = gib(28.0);
        let reserve = gib(6.0);
        let limits = Limits {
            max_parallel: 64,
            cores: 64.0,
            ..LIMITS
        };
        let mut started = vec![test(100.0, 0.1, others)];
        let mut pending_sum = pending(gib(others), Some(gib(using)));
        loop {
            let now = Now {
                available: Some(gib(available)),
                ..Now::default()
            };
            let r = running(&started);
            let budget = free_budget(cap, r.memory, pending_sum, now.available, reserve);
            let limits = Limits { budget, ..limits };
            let c = test(100.0, 0.1, each);
            if admit(&c, &r, &now, &limits).is_err() {
                return started.len() - 1;
            }
            pending_sum += pending(c.prediction.memory, None);
            started.push(c);
        }
    }

    #[test]
    fn the_free_budget_follows_the_available_memory() {
        let g = gib;
        // 30 GiB available, 6 reserved: 24 GiB for the tests, at most the
        // 28 GiB cap with what runs.
        assert_eq!(
            free_budget(g(28.0), g(2.0), 0, Some(g(30.0)), g(6.0)),
            g(26.0)
        );
        assert_eq!(
            free_budget(g(28.0), g(10.0), 0, Some(g(30.0)), g(6.0)),
            g(28.0)
        );
        // A running test's growth still to come is not yet in the reading.
        assert_eq!(
            free_budget(g(28.0), g(2.0), g(2.0), Some(g(30.0)), g(6.0)),
            g(24.0)
        );
        // Below the reserve only what runs keeps its place: nothing starts.
        assert_eq!(
            free_budget(g(28.0), g(4.0), 0, Some(g(5.0)), g(6.0)),
            g(4.0)
        );
        // Without a reading, the cap.
        assert_eq!(free_budget(g(28.0), g(4.0), 0, None, g(6.0)), g(28.0));
        assert_eq!(pending(g(3.0), Some(g(1.0))), g(2.0));
        assert_eq!(pending(g(3.0), Some(g(5.0))), 0);
        assert_eq!(pending(g(3.0), None), g(3.0));
    }

    #[test]
    fn a_build_beside_the_check_holds_its_tests_back() {
        // The machine free: 30 GiB available, 6 reserved; a 2 GiB test
        // already at its peak. Twelve more of 2 GiB fit in the 24 GiB.
        assert_eq!(free_starts(30.0, 2.0, 2.0, 2.0), 12);
        // An agent's build starts and takes 18 GiB: 12 GiB available, so
        // only three more start (6 GiB beyond the reserve), not twelve
        // that the build would have the machine kill.
        assert_eq!(free_starts(12.0, 2.0, 2.0, 2.0), 3);
        // The build ends: the room comes back.
        assert_eq!(free_starts(30.0, 2.0, 2.0, 2.0), 12);
        // The running test has taken only half of its 2 GiB yet: its other
        // half is kept for it.
        assert_eq!(free_starts(12.0, 2.0, 1.0, 2.0), 2);
        // Below the reserve nothing more starts beside what runs.
        assert_eq!(free_starts(5.0, 2.0, 2.0, 2.0), 0);
        // The cap holds on an idle machine (28 GiB with the running 2).
        assert_eq!(free_starts(60.0, 2.0, 2.0, 2.0), 13);
    }

    #[test]
    fn the_cpu_gate_holds_tests_back() {
        let busy = running(&[test(10.0, 1.0, 1.0)]);
        let loaded = Now {
            load: 0.95,
            ..Now::default()
        };
        let small = test(1.0, 1.0, 1.0);
        assert_eq!(admit(&small, &busy, &loaded, &LIMITS), Err(Refusal::Cpu));
        // Predicted cores count before the load shows them.
        let cores = running(&[test(10.0, 7.5, 1.0)]);
        let calm = Now::default();
        assert_eq!(admit(&small, &cores, &calm, &LIMITS), Err(Refusal::Cpu));
        assert_eq!(admit(&small, &busy, &calm, &LIMITS), Ok(()));
    }

    #[test]
    fn sensitive_tests_wait_for_a_calm_machine() {
        let mut ui = test(1.0, 1.0, 1.0);
        ui.sensitive = true;
        let busy = running(&[test(10.0, 1.0, 1.0)]);
        let medium = Now {
            load: 0.75,
            ..Now::default()
        };
        assert_eq!(admit(&ui, &busy, &medium, &LIMITS), Err(Refusal::Sensitive));
        assert_eq!(admit(&ui, &busy, &Now::default(), &LIMITS), Ok(()));
        let two = running(&[ui.clone(), ui.clone()]);
        let calm = Now::default();
        assert_eq!(admit(&ui, &two, &calm, &LIMITS), Err(Refusal::Sensitive));
    }

    #[test]
    fn ui_entries_keep_the_learnt_cap_and_their_rooms() {
        let calm = Now::default();
        let mut ui = test(30.0, 1.0, 0.5);
        ui.ui = true;
        let two_ui = running(&[ui.clone(), ui.clone(), test(100.0, 1.0, 1.0)]);
        assert_eq!(two_ui.ui, 2);
        assert_eq!(admit(&ui, &two_ui, &calm, &LIMITS), Ok(()));
        // The cap (3) counts UI entries only.
        let three = running(&[ui.clone(), ui.clone(), ui.clone()]);
        assert_eq!(admit(&ui, &three, &calm, &LIMITS), Err(Refusal::Ui));
        let plain = test(1.0, 1.0, 1.0);
        assert_eq!(admit(&plain, &three, &calm, &LIMITS), Ok(()));
        // A test with room 1 (ui-pick after failing beside 3) joins one UI
        // entry, not two; while it runs, it holds the others back too.
        let mut pick = ui.clone();
        pick.ui_room = Some(1);
        let one = running(&[ui.clone()]);
        assert_eq!(admit(&pick, &one, &calm, &LIMITS), Ok(()));
        let both = running(&[ui.clone(), ui.clone()]);
        assert_eq!(admit(&pick, &both, &calm, &LIMITS), Err(Refusal::Ui));
        let with_pick = running(&[pick.clone(), ui.clone()]);
        assert_eq!(with_pick.ui_room, Some(1));
        assert_eq!(admit(&ui, &with_pick, &calm, &LIMITS), Err(Refusal::Ui));
        assert_eq!(admit(&plain, &with_pick, &calm, &LIMITS), Ok(()));
        // Nothing running: any test starts.
        let mut lonely = pick;
        lonely.ui_room = Some(0);
        let idle = Running::default();
        assert_eq!(admit(&lonely, &idle, &calm, &LIMITS), Ok(()));
    }

    /// How many UI entries start one after another under the machine's cap
    /// for `bound`, with memory, cores and slots to spare.
    fn ui_starts(bound: Option<usize>) -> usize {
        let limits = Limits {
            ui_max: crate::concurrency::machine_cap(bound),
            max_parallel: 64,
            budget: gib(256.0),
            cores: 64.0,
            ..LIMITS
        };
        let calm = Now::default();
        let mut ui = test(30.0, 0.1, 0.5);
        ui.ui = true;
        let mut started = Running::default();
        while started.count < 64 && admit(&ui, &started, &calm, &limits).is_ok() {
            started.add(&ui);
        }
        started.ui
    }

    #[test]
    fn the_scheduler_holds_ui_entries_to_the_reported_cap() {
        // No --ui-max, no MITCAD_UI_JOBS: the default the report names (4),
        // not every UI entry the memory budget fits (a follow-up of
        // mitcad#2124: the Windows VM ran 15 at once under "cap 4").
        assert_eq!(ui_starts(None), crate::concurrency::DEFAULT_BOUND);
        assert_eq!(ui_starts(None), 4);
        // A bound is the cap.
        assert_eq!(ui_starts(Some(6)), 6);
    }

    #[test]
    fn locks_and_serial_tests_are_kept() {
        let locks = ["cargo_build".to_owned()];
        let mut a = test(1.0, 1.0, 1.0);
        a.locks = &locks;
        let held = running(&[a.clone()]);
        let now = Now::default();
        assert_eq!(admit(&a, &held, &now, &LIMITS), Err(Refusal::Lock));
        // A lock blocks only the tests with the same lock.
        let free = test(1.0, 1.0, 1.0);
        assert_eq!(admit(&free, &held, &now, &LIMITS), Ok(()));
        assert_eq!(admit_fixed(&free, &held, 4), Ok(()));
        let mut serial = test(1.0, 1.0, 1.0);
        serial.alone = true;
        let other = running(&[test(1.0, 1.0, 1.0)]);
        assert_eq!(admit(&serial, &other, &now, &LIMITS), Err(Refusal::Alone));
        assert_eq!(admit(&serial, &Running::default(), &now, &LIMITS), Ok(()));
        let alone = running(&[serial]);
        let plain = test(1.0, 1.0, 1.0);
        assert_eq!(admit(&plain, &alone, &now, &LIMITS), Err(Refusal::Alone));
        assert_eq!(admit_fixed(&plain, &alone, 4), Err(Refusal::Alone));
    }

    #[test]
    fn fixed_mode_counts_tests() {
        let plain = test(1.0, 1.0, 100.0);
        let three = running(&[plain.clone(), plain.clone(), plain.clone()]);
        assert_eq!(admit_fixed(&plain, &three, 4), Ok(()));
        assert_eq!(admit_fixed(&plain, &three, 3), Err(Refusal::Parallel));
    }

    #[test]
    fn the_lane_keeps_its_share() {
        let lw = Now {
            lane_waiting: true,
            ..Now::default()
        };
        // 12 GiB of 16 run outside the lane: 12.8 is the others' limit.
        let busy = running(&[test(10.0, 1.0, 12.0)]);
        let t2 = test(5.0, 1.0, 2.0);
        assert_eq!(admit(&t2, &busy, &lw, &LIMITS), Err(Refusal::LaneReserve));
        assert_eq!(admit(&t2, &busy, &Now::default(), &LIMITS), Ok(()));
        let mut lane = t2.clone();
        lane.lane = true;
        assert_eq!(admit(&lane, &busy, &lw, &LIMITS), Ok(()));
        // And a slot.
        let small = test(1.0, 0.1, 0.1);
        let seven: Vec<Candidate> = std::iter::repeat_n(small.clone(), 7).collect();
        let seven = running(&seven);
        assert_eq!(admit(&small, &seven, &lw, &LIMITS), Err(Refusal::Parallel));
        assert_eq!(admit(&lane, &seven, &lw, &LIMITS), Ok(()));
    }

    #[test]
    fn the_lane_goes_first_by_score() {
        let mut a = test(100.0, 1.0, 1.0);
        a.rank = 500.0;
        let mut b = test(1.0, 1.0, 1.0);
        b.lane = true;
        b.score = 12.0;
        let mut c = test(2.0, 1.0, 1.0);
        c.lane = true;
        c.score = 30.0;
        let now = Now::default();
        let picked = pick(&[a, b, c], &Running::default(), &now, &LIMITS);
        assert_eq!(picked, Some(2));
    }

    #[test]
    fn longest_first() {
        let now = Now::default();
        let tests = [
            test(10.0, 1.0, 1.0),
            test(300.0, 1.0, 1.0),
            test(50.0, 1.0, 1.0),
        ];
        assert_eq!(pick(&tests, &Running::default(), &now, &LIMITS), Some(1));
        assert_eq!(pick(&[], &Running::default(), &now, &LIMITS), None);
    }

    #[test]
    fn pairing_fills_what_is_left() {
        // Much memory left, little CPU: of two long tests, the memory-heavy
        // CPU-light one.
        let busy = running(&[test(100.0, 6.0, 1.0)]);
        let now = Now::default();
        let cpu_heavy = test(100.0, 4.0, 1.0);
        let memory_heavy = test(95.0, 0.5, 8.0);
        let tests = [cpu_heavy.clone(), memory_heavy.clone()];
        assert_eq!(pick(&tests, &busy, &now, &LIMITS), Some(1));
        // And the other way round.
        let full = running(&[test(100.0, 0.5, 12.0)]);
        let tests = [memory_heavy, cpu_heavy];
        assert_eq!(pick(&tests, &full, &now, &LIMITS), Some(1));
        // Pairing does not pass over a much longer test.
        let long = test(1000.0, 4.0, 1.0);
        let tests = [long, test(95.0, 0.5, 8.0)];
        assert_eq!(pick(&tests, &busy, &now, &LIMITS), Some(0));
    }

    #[test]
    fn the_longest_chain_has_the_first_claim_on_memory() {
        // A 4 GiB test runs; the build at the head of the longest chain
        // (13 GiB) does not fit the 16 GiB budget beside it, a small test
        // does.
        let busy = running(&[test(300.0, 1.0, 4.0)]);
        let now = Now::default();
        let build = test(900.0, 8.0, 13.0);
        let small = test(100.0, 1.0, 0.1);
        let lock = ["cargo_build".to_owned()];
        let mut locked = test(5000.0, 1.0, 1.0);
        locked.locks = &lock;
        let mut busy_locked = busy.clone();
        busy_locked.locks = lock.to_vec();
        let huge = test(9000.0, 1.0, 40.0);
        let tests = [build.clone(), small.clone(), locked, huge];
        let refusals: Vec<_> = tests
            .iter()
            .map(|c| admit(c, &busy_locked, &now, &LIMITS))
            .collect();
        assert_eq!(refusals[0], Err(Refusal::Memory));
        assert_eq!(refusals[1], Ok(()));
        assert_eq!(refusals[2], Err(Refusal::Lock));
        // Not the locked test (not memory keeps it), nor the one larger
        // than the budget can ever be: the build.
        assert_eq!(claimant(&tests, &refusals, LIMITS.budget), Some(0));
        // The small test would take the room the build waits for.
        assert!(!leaves_room(&small, &build, &busy, &LIMITS));
        // When the running test ends, both fit.
        assert!(leaves_room(&small, &build, &Running::default(), &LIMITS));
        // The priority lane passes the claim of a test outside it, not of
        // one inside it.
        let mut urgent = small.clone();
        urgent.lane = true;
        assert!(leaves_room(&urgent, &build, &busy, &LIMITS));
        let mut lane_build = build.clone();
        lane_build.lane = true;
        assert!(!leaves_room(&urgent, &lane_build, &busy, &LIMITS));
        // Of two that may start, the higher rank claims; none without a
        // candidate memory alone keeps or that fits at all.
        let refusals = [Ok(()), Ok(())];
        assert_eq!(
            claimant(&[small.clone(), build], &refusals, LIMITS.budget),
            Some(1)
        );
        let refusals = [Err(Refusal::Cpu)];
        assert_eq!(claimant(&[small], &refusals, LIMITS.budget), None);
    }

    #[test]
    fn ranks_follow_the_chains() {
        // 0 -> 1 -> 2, 3 alone.
        let walls = [10.0, 20.0, 300.0, 100.0];
        let successors = [vec![1], vec![2], vec![], vec![]];
        assert_eq!(ranks(&walls, &successors), [330.0, 320.0, 300.0, 100.0]);
        // A cycle does not loop.
        let cycle = [vec![1], vec![0]];
        let r = ranks(&[1.0, 2.0], &cycle);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn limits_are_twice_the_history_at_least_4_gib() {
        let ceiling = gib(30.0);
        let facts = LimitFacts {
            peak_p90: gib(3.0),
            spread: 1.5,
            kind_memory: gib(0.5),
            ..LimitFacts::default()
        };
        assert_eq!(memory_limit(&facts, 2.0, ceiling), Some(gib(6.0)));
        assert_eq!(memory_limit(&facts, 0.0, ceiling), None);
        // A small history (a warm build of tens of MiB) gets 4 GiB.
        let warm = LimitFacts {
            peak_p90: gib(0.05),
            ..facts
        };
        assert_eq!(memory_limit(&warm, 2.0, ceiling), Some(gib(4.0)));
        // Twice the kind's guess where that is more.
        let corpus = LimitFacts {
            kind_memory: gib(2.5),
            ..warm
        };
        assert_eq!(memory_limit(&corpus, 2.0, ceiling), Some(gib(5.0)));
        // Up to the ceiling, never below 4 GiB.
        let large = LimitFacts {
            peak_p90: gib(20.0),
            ..facts
        };
        assert_eq!(memory_limit(&large, 2.0, ceiling), Some(ceiling));
        assert_eq!(memory_limit(&warm, 2.0, gib(2.0)), Some(gib(4.0)));
    }

    #[test]
    fn some_tests_get_no_limit() {
        let ceiling = gib(30.0);
        let base = LimitFacts {
            peak_p90: gib(0.05),
            spread: 1.0,
            ..LimitFacts::default()
        };
        // RUN_SERIAL (the cargo test builds) runs alone: no limit.
        let serial = LimitFacts {
            serial: true,
            ..base
        };
        assert_eq!(memory_limit(&serial, 2.0, ceiling), None);
        // A history from 50 MiB (warm) to 5 GiB (cold): no limit.
        let spread = LimitFacts {
            spread: 100.0,
            ..base
        };
        assert_eq!(memory_limit(&spread, 2.0, ceiling), None);
        let four = LimitFacts {
            spread: 4.0,
            ..base
        };
        assert!(memory_limit(&four, 2.0, ceiling).is_some());
        // The rerun after a memory limit runs alone without one.
        let rerun = LimitFacts {
            rerun: true,
            ..base
        };
        assert_eq!(memory_limit(&rerun, 2.0, ceiling), None);
    }

    #[test]
    fn a_killed_run_raises_the_profile() {
        let killed = ResultKind::KilledForMemory;
        assert_eq!(recorded_peak(killed, gib(1.0), Some(gib(4.0))), gib(4.0));
        assert_eq!(recorded_peak(killed, gib(5.0), Some(gib(4.0))), gib(5.0));
        assert_eq!(recorded_peak(killed, gib(1.0), None), gib(1.0));
        let passed = ResultKind::Passed;
        assert_eq!(recorded_peak(passed, gib(1.0), Some(gib(4.0))), gib(1.0));
        // Killed for memory is an environment failure: +2, not +10.
        assert!(killed.environmental());
        assert_eq!(crate::profile::update_score(0.0, killed), 2.0);
    }

    #[test]
    fn runs_are_classified() {
        let seen = |status, exit_ok| Seen {
            status,
            exit_ok,
            ..Seen::default()
        };
        let kind = |status, exit_ok| classify(&seen(status, exit_ok));
        assert_eq!(kind(Some(Status::Passed), true), ResultKind::Passed);
        assert_eq!(kind(Some(Status::Skipped), true), ResultKind::Skipped);
        assert_eq!(kind(Some(Status::Failed), false), ResultKind::Failed);
        assert_eq!(kind(Some(Status::Timeout), false), ResultKind::Timeout);
        assert_eq!(kind(None, false), ResultKind::Environment);
        let killed = Seen {
            over_memory: true,
            ..seen(Some(Status::Failed), false)
        };
        assert_eq!(classify(&killed), ResultKind::KilledForMemory);
        let full = Seen {
            disk_full: true,
            ..seen(Some(Status::Failed), false)
        };
        assert_eq!(classify(&full), ResultKind::Environment);
        let stopped = Seen {
            stopped: true,
            ..seen(None, false)
        };
        assert_eq!(classify(&stopped), ResultKind::Timeout);
    }

    fn span(start: f64, end: f64, before: Vec<usize>) -> Span {
        Span {
            start,
            end,
            before,
            ..Span::default()
        }
    }

    fn link(index: usize, why: Why) -> Link {
        Link { index, why }
    }

    #[test]
    fn the_critical_path_ends_last() {
        let spans = [
            span(0.0, 100.0, vec![]),
            span(100.5, 400.0, vec![0]),
            span(0.0, 350.0, vec![]),
            span(0.0, 50.0, vec![]),
        ];
        let path = [link(0, Why::Start), link(1, Why::Dependency)];
        assert_eq!(critical_path(&spans), path);
        assert!(critical_path(&[]).is_empty());
    }

    #[test]
    fn the_critical_path_counts_waiting_behind_serial_tests() {
        // A release build alone, a debug build alone, then a UI test that
        // waited for both (a short test that started with it is no cause).
        let mut release = span(0.0, 188.8, vec![]);
        release.alone = true;
        let mut debug = span(188.9, 189.6, vec![]);
        debug.alone = true;
        let ui = span(189.6, 251.4, vec![]);
        let short = span(189.6, 189.7, vec![]);
        let spans = [release, debug, ui, short];
        let path = [
            link(0, Why::Start),
            link(1, Why::Serial),
            link(2, Why::Serial),
        ];
        assert_eq!(critical_path(&spans), path);
    }

    #[test]
    fn the_critical_path_names_locks_and_resources() {
        let locks = vec!["cargo_build".to_owned()];
        let mut a = span(0.0, 50.0, vec![]);
        a.locks = locks.clone();
        let mut b = span(50.2, 80.0, vec![]);
        b.locks = locks;
        let c = span(80.5, 120.0, vec![]);
        let path = [
            link(0, Why::Start),
            link(1, Why::Lock),
            link(2, Why::Resources),
        ];
        assert_eq!(critical_path(&[a, b, c]), path);
    }

    #[test]
    fn the_lane_takes_tests_with_a_score() {
        assert!(in_lane(true, 10.0, 1.0));
        assert!(in_lane(true, 1.0, 1.0));
        assert!(!in_lane(true, 0.0, 1.0));
        // Never a test without a failure, whatever the threshold.
        assert!(!in_lane(true, 0.0, 0.0));
        assert!(in_lane(true, 2.0, 0.0));
        assert!(!in_lane(true, 2.0, 5.0));
        // Measuring has no lane.
        assert!(!in_lane(false, 30.0, 1.0));
    }
}
