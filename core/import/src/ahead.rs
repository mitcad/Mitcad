// SPDX-License-Identifier: MIT
//! An item's definitions evaluated ahead, in parallel (mitcad#95).
//!
//! The importer tries an item's candidate definitions one after another
//! and takes the first that gives the history's state; most of an
//! import's time goes to the definitions it rejects. With
//! [`crate::Options::threads`] above one, while the importer evaluates
//! candidate *k* on its document (from the second candidate of a list on:
//! the first is usually the one taken), workers evaluate the candidates
//! after it, each on its own copy of the document as it is before the item
//! ([`Document::fork`]: the same definition and cached results, shared; a
//! kernel of its own, [`Kernel::fork`]). The importer still decides in
//! rank order, exactly as without workers: when it comes to a candidate a
//! worker has evaluated (or waits for it), it takes the worker's result
//! instead of evaluating it again, and the worker's results go into the
//! document's cache ([`Document::adopt_results`]) as if evaluated there.
//! A rejected candidate leaves the document as it is (it was undone
//! anyway); the one accepted is added to the document again and reuses
//! the worker's results, so its features, uids, topological names and
//! report notes are those of the run without workers. Once the importer
//! leaves the candidates (one was accepted, or none is left), the workers
//! still evaluating are cancelled through their monitors, as a stop
//! cancels a definition.
//!
//! A worker's result stands in only where the run without workers would
//! have had the same one: its share of the item's time is the first
//! round's share ([`SHARE`]); one that gave way at it is evaluated again on
//! the document when the importer would give it the rest of the time.
//! Results that depend on the time (definitions cut short by the item's
//! time) can differ, as they do between two runs without workers.
//!
//! Only the first round of the plain candidate lists is evaluated ahead
//! ([`Importer::try_state`], and each set of bodies of an item without a
//! state of its own): the second round of definitions that gave way, the
//! closer definition after a loose match, the edits of a pattern's
//! extrusions and the definitions on bodies the history holds already
//! stay on the importer's thread.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::history::Sig;
use crate::{
    CANDIDATE_OUT_OF_TIME, Candidate, GAVE_WAY, Importer, LOW_MEMORY, OUT_OF_TIME, Progress, SHARE,
    STOPPED, Ticking, candidate_seconds, seconds, summary, trace_clock, tracing,
};
use mitcad_model::{
    BodyUid, ComponentUid, Document, FeatureUid, Kernel, KernelError, RecomputeMonitor,
};

/// A kernel the import can use: its documents may go to the threads that
/// evaluate definitions ahead (their shapes are read from several threads
/// at once; [`Kernel::fork`] says whether it can work on another thread).
pub trait ImportKernel: Kernel<Shape: Send + Sync> + Send + 'static {}

impl<K> ImportKernel for K
where
    K: Kernel + Send + 'static,
    K::Shape: Send + Sync,
{
}

/// A job for a new thread.
pub type WorkerJob = Box<dyn FnOnce() + Send + 'static>;

/// Starts the threads that evaluate definitions ahead
/// ([`crate::Options::spawn`]): runs the job on a new thread, false when
/// none could be started (the definition is then evaluated on the import's
/// thread). `mitcad-ffi` installs the geometry kernel's crash handlers on
/// them first.
#[derive(Clone)]
pub struct Spawner(pub Arc<dyn Fn(WorkerJob) -> bool + Send + Sync>);

impl std::fmt::Debug for Spawner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Spawner")
    }
}

/// The stack of a worker: 64 MiB. The import's own thread has 256 MiB
/// (`mitcad-ffi`: the geometry kernel recurses deeply, and an overflow
/// fails a definition), but a worker's stack counts against the memory
/// limits the import measures itself against (an address-space or data
/// limit counts reserved stacks whole), and seven of 256 MiB would take a
/// corpus run's whole limit. The kernel's own worker threads (its parallel
/// booleans) run on the system's default stack of a few MiB.
pub const WORKER_STACK: usize = 64 << 20;

impl Spawner {
    /// Plain threads with [`WORKER_STACK`].
    pub fn threads() -> Self {
        Spawner(Arc::new(|job: WorkerJob| {
            std::thread::Builder::new()
                .name("mitcad-import-worker".to_owned())
                .stack_size(WORKER_STACK)
                .spawn(job)
                .is_ok()
        }))
    }
}

/// Worker threads of all imports that still run: also those whose result is
/// no longer wanted, whose kernel call has not returned yet. A process
/// must not run its static destructors under them (`mitcad-ffi`,
/// `abandoned_imports`).
static RUNNING: AtomicUsize = AtomicUsize::new(0);

/// How many threads that evaluate definitions ahead still run, in all
/// imports of the process.
pub fn running_workers() -> usize {
    RUNNING.load(Ordering::SeqCst)
}

/// Counts a running worker until it is dropped (its thread's last act).
struct Running(Arc<AtomicUsize>);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
        RUNNING.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The share of the tightest memory limit above which no more definitions
/// are evaluated ahead ([`Progress::set_memory_share`]): each worker holds
/// its definition's memory and a thread stack, and the import gives up
/// definitions at 85 % (mitcad#80).
pub(crate) const AHEAD_MEMORY: f64 = 0.5;

/// The share of the tightest memory limit at which the definitions being
/// evaluated ahead are cut short ([`Progress::set_memory_share`]): the
/// import's own definition keeps the memory, as without workers.
pub(crate) const AHEAD_STOP: f64 = 0.6;

/// How often the importer looks for a stop while it waits for a worker.
const WAIT_STEP: Duration = Duration::from_millis(20);

/// What a worker found for a candidate.
pub(crate) struct Done<K: Kernel> {
    added: Result<Vec<FeatureUid>, String>,
    /// The solids afterwards (when added).
    bodies: Vec<(BodyUid, Sig)>,
    /// Seconds it evaluated.
    ran: f64,
    /// Which of its deadlines had passed when it ended: the item's, its
    /// own ([`candidate_seconds`]), its share.
    past_item: bool,
    past_own: bool,
    past_share: bool,
    /// The worker's document, whose results an accepted candidate adopts.
    doc: Document<K>,
}

/// A candidate given to a worker.
struct Job<K: Kernel> {
    /// Cancels it (no longer wanted).
    cancel: Arc<RecomputeMonitor>,
    done: mpsc::Receiver<Done<K>>,
    /// The share of the item's time it was given.
    share: Option<f64>,
}

/// The candidates of one list being evaluated ahead; dropping it cancels
/// the workers still evaluating.
pub(crate) struct Ahead<K: Kernel> {
    jobs: BTreeMap<usize, Job<K>>,
    /// The next candidate to give a worker.
    next: usize,
    /// The share of the item's time each is given (the first round's).
    share: f64,
    /// No more workers (the kernel has no fork).
    off: bool,
    /// A definition of the list was evaluated (and not taken).
    tried: bool,
}

impl<K: Kernel> Ahead<K> {
    /// A candidate the importer passes over: its worker is not needed.
    pub fn forget(&mut self, k: usize) {
        if let Some(job) = self.jobs.remove(&k) {
            job.cancel.cancel();
        }
    }
}

impl<K: Kernel> Drop for Ahead<K> {
    fn drop(&mut self) {
        for job in self.jobs.values() {
            job.cancel.cancel();
        }
    }
}

/// A candidate evaluated, on the document or by a worker.
pub(crate) struct Evaluated {
    pub result: Result<Vec<FeatureUid>, String>,
    /// A worker's: the solids its features left (when added), which the
    /// document does not hold yet ([`Importer::take_over`]).
    pub bodies: Option<Vec<(BodyUid, Sig)>>,
    /// A worker's: the seconds it evaluated.
    pub ran: Option<f64>,
}

impl Evaluated {
    /// Whether the candidate's features are only in a worker's document.
    pub fn ahead(&self) -> bool {
        self.bodies.is_some()
    }
}

/// What adding a candidate's features needs besides the document: the
/// item's component, the monitor its evaluation runs under (None: the
/// document's own), and the import's progress.
pub(crate) struct Adding<'p> {
    pub component: ComponentUid,
    pub monitor: Option<Arc<RecomputeMonitor>>,
    pub progress: Option<&'p Progress>,
    /// Whether an operation that runs out of memory tells the import that
    /// it is low on memory ([`Progress::low_memory`]; not a worker's,
    /// which is evaluated again on the import's thread).
    pub memory: bool,
}

/// Evaluates a candidate on a worker's document.
#[allow(clippy::too_many_arguments)]
fn evaluate_on<K: Kernel>(
    mut doc: Document<K>,
    candidate: &Candidate,
    name: Option<&str>,
    component: ComponentUid,
    parents: Vec<Arc<RecomputeMonitor>>,
    item_deadline: Option<Instant>,
    share: Option<f64>,
    progress: Option<Arc<Progress>>,
) -> Done<K> {
    // Its ticks are the import's (the watchdog's), its kernel calls too.
    let _ticking = Ticking::new(progress.clone());
    let clock = Instant::now();
    let own = candidate_seconds(candidate).map(|s| clock + seconds(s));
    let shared = share.map(|s| clock + seconds(s));
    let deadline = [item_deadline, own, shared].into_iter().flatten().min();
    let adding = Adding {
        component,
        monitor: Some(Arc::new(RecomputeMonitor::within(parents, deadline))),
        progress: progress.as_deref(),
        memory: false,
    };
    let added = crate::add_all(&mut doc, candidate, name, &adding);
    let bodies = if added.is_ok() {
        crate::bodies_of(&doc)
    } else {
        Vec::new()
    };
    let past = |d: Option<Instant>| d.is_some_and(|d| Instant::now() >= d);
    Done {
        added,
        bodies,
        ran: clock.elapsed().as_secs_f64(),
        past_item: past(item_deadline),
        past_own: past(own),
        past_share: past(shared),
        doc,
    }
}

impl<K: crate::ImportKernel> Importer<'_, K> {
    /// Workers for a list of candidates tried with `budget` seconds of the
    /// item's time ([`crate::Options::threads`]); None without.
    pub(crate) fn ahead(&self, budget: f64) -> Option<Ahead<K>> {
        (self.options.threads > 1).then(|| Ahead {
            jobs: BTreeMap::new(),
            next: 0,
            share: SHARE * budget,
            off: false,
            tried: false,
        })
    }

    /// Whether another worker may start: fewer run than the threads allow
    /// (the importer's own is one of them), and the process has memory to
    /// spare.
    fn worker_free(&self) -> bool {
        if self.workers.load(Ordering::SeqCst) + 1 >= self.options.threads {
            return false;
        }
        self.options
            .progress
            .as_ref()
            .is_none_or(|p| !p.is_memory_low() && p.memory_share() < AHEAD_MEMORY)
    }

    /// A thread of the import's budget for a worker (shared with the
    /// bodies built ahead, mitcad#103): None when there is no room; `Some(None)`
    /// without a progress to share it (then [`Importer::worker_free`]
    /// counts the workers alone).
    fn worker_helper(&self) -> Option<Option<Helper>> {
        match &self.options.progress {
            Some(p) => p.helper().map(Some),
            None => Some(None),
        }
    }

    /// Gives the candidates after `k` (up to `until`) to free workers.
    fn launch(
        &mut self,
        ahead: &mut Ahead<K>,
        candidates: &[Candidate],
        k: usize,
        until: usize,
        name: Option<&str>,
    ) {
        ahead.next = ahead.next.max(k + 1);
        while !ahead.off && ahead.next < until.min(candidates.len()) && self.worker_free() {
            let j = ahead.next;
            let Some(helper) = self.worker_helper() else {
                break;
            };
            let Some(doc) = self.doc.fork() else {
                ahead.off = true;
                break;
            };
            let cancel = Arc::new(RecomputeMonitor::new());
            let mut parents = self.try_parents();
            parents.push(Arc::clone(&cancel));
            parents.extend(
                self.options
                    .progress
                    .as_ref()
                    .and_then(|p| p.ahead_monitor()),
            );
            let (sender, done) = mpsc::channel();
            let candidate = candidates[j].clone();
            let owned_name = name.map(str::to_owned);
            let component = self.component;
            let deadline = self.deadline;
            let share = Some(ahead.share);
            let progress = self.options.progress.clone();
            self.workers.fetch_add(1, Ordering::SeqCst);
            RUNNING.fetch_add(1, Ordering::SeqCst);
            let running = Running(Arc::clone(&self.workers));
            let job: WorkerJob = Box::new(move || {
                let _running = running;
                let _helper = helper;
                let done = evaluate_on(
                    doc,
                    &candidate,
                    owned_name.as_deref(),
                    component,
                    parents,
                    deadline,
                    share,
                    progress,
                );
                // (Nobody waits once the importer has moved on.)
                let _ = sender.send(done);
            });
            let spawner = self.options.spawn.clone().unwrap_or_else(Spawner::threads);
            // A job that does not start is dropped, and with it its count.
            if !(spawner.0)(job) {
                ahead.off = true;
                break;
            }
            trace!(
                "{}: definition {} evaluated ahead",
                name.unwrap_or("?"),
                j + 1
            );
            ahead.jobs.insert(
                j,
                Job {
                    cancel,
                    done,
                    share,
                },
            );
            ahead.next += 1;
        }
    }

    /// Evaluates candidate `k` of `candidates` (the modelling item `index`)
    /// for at most `share` seconds ([`Importer::try_candidate_for`]): with
    /// `ahead`, the candidates after it, up to `until`, go to free workers
    /// first, and a worker's result for it stands in where it is the one
    /// the document would give. A worker's result that is accepted must
    /// then be adopted ([`Importer::take_over`]).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn evaluate(
        &mut self,
        ahead: Option<&mut Ahead<K>>,
        candidates: &[Candidate],
        k: usize,
        until: usize,
        index: i64,
        name: Option<&str>,
        share: Option<f64>,
    ) -> Evaluated {
        if let Some(ahead) = ahead {
            // Not before the first definition was rejected: it is usually
            // the one taken, and workers evaluating the next ones would
            // only take processors and memory from it.
            if ahead.tried {
                self.launch(ahead, candidates, k, until, name);
            }
            ahead.tried = true;
            if let Some(job) = ahead.jobs.remove(&k)
                && let Some(evaluated) = self.take(job, candidates, k, index, name, share)
            {
                return evaluated;
            }
        }
        Evaluated {
            result: self.try_candidate_for(index, &candidates[k], name, share),
            bodies: None,
            ran: None,
        }
    }

    /// A worker's result for candidate `k`, once it is there; None when
    /// the document must evaluate it (the worker gave way at a share the
    /// importer would not give it, or ended without a result).
    fn take(
        &mut self,
        job: Job<K>,
        candidates: &[Candidate],
        k: usize,
        index: i64,
        name: Option<&str>,
        share: Option<f64>,
    ) -> Option<Evaluated> {
        let halted = |result: String| Evaluated {
            result: Err(result),
            bodies: None,
            ran: None,
        };
        let done = loop {
            if self.abandoned() || self.stopped(index) {
                job.cancel.cancel();
                return Some(halted(STOPPED.to_owned()));
            }
            if self.memory_low() {
                job.cancel.cancel();
                return Some(halted(LOW_MEMORY.to_owned()));
            }
            match job.done.recv_timeout(WAIT_STEP) {
                Ok(done) => break done,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                // (Its thread ended without a result.)
                Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            }
        };
        let cancelled = |e: &str| e == mitcad_model::Cancelled::MESSAGE;
        let result = match done.added {
            Err(_) if self.abandoned() || self.stopped(index) => Err(STOPPED.to_owned()),
            Err(_) if self.memory_low() => Err(LOW_MEMORY.to_owned()),
            Err(e) if cancelled(&e) && done.past_item => Err(OUT_OF_TIME.to_owned()),
            Err(e) if cancelled(&e) && done.past_own => Err(CANDIDATE_OUT_OF_TIME.to_owned()),
            // At the share the document would give it too.
            Err(e) if cancelled(&e) && done.past_share && share == job.share => {
                Err(GAVE_WAY.to_owned())
            }
            // Given more time on the document, cancelled otherwise (the
            // memory was short for workers), or out of memory beside the
            // other workers: evaluated on the document.
            Err(e) if cancelled(&e) || KernelError::is_out_of_memory(&e) => return None,
            added => added,
        };
        trace!(
            "{}: definition {} evaluated ahead in {:.2} s",
            name.unwrap_or("?"),
            k + 1,
            done.ran
        );
        if let Ok(uids) = &result {
            // As the document would keep them in its cache.
            self.doc.adopt_results(&done.doc, uids);
            if tracing() {
                trace!(
                    "{}: {} ahead -> {} solids",
                    name.unwrap_or("?"),
                    summary(&candidates[k]),
                    done.bodies.len()
                );
            }
        }
        let bodies = result.is_ok().then_some(done.bodies);
        Some(Evaluated {
            result,
            bodies,
            ran: Some(done.ran),
        })
    }

    /// Brings an accepted candidate that a worker evaluated into the
    /// document: its definitions are added again, reusing the worker's
    /// results (the same uids, shapes and names). The uids of the
    /// candidate's features.
    pub(crate) fn take_over(
        &mut self,
        index: i64,
        candidate: &Candidate,
        name: Option<&str>,
        evaluated: &Evaluated,
    ) -> Result<Vec<FeatureUid>, String> {
        let uids = evaluated.result.clone()?;
        if !evaluated.ahead() {
            return Ok(uids);
        }
        let added = self.try_candidate_for(index, candidate, name, None)?;
        if added != uids {
            trace!(
                "{}: {} added as {added:?}, ahead as {uids:?}",
                name.unwrap_or("?"),
                summary(candidate)
            );
        }
        Ok(added)
    }
}

// One budget of threads for the work beside the import's own (mitcad#103):
// the definitions evaluated ahead here, and the history's bodies that
// `mitcad-ffi` builds ahead of the replay.

/// The threads working beside an import's own, and how many it may have in
/// all ([`crate::Options::threads`]).
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
    /// ([`crate::Options::threads`]; set by [`crate::import_design`]).
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

/// Runs `f` on `0..count` on up to `threads` threads and hands the results
/// to `consume` in order, as `for k in 0..count { consume(k, f(k)) }` would
/// (mitcad#103): the threads work at most `2 × threads` ahead of `consume`,
/// so that the results waiting take little memory. With one thread (or one
/// item) everything runs on the calling thread. A panic in `f` or
/// `consume` ends the others and goes on on the calling thread.
pub fn in_order<T: Send>(
    count: usize,
    threads: usize,
    f: impl Fn(usize) -> T + Sync,
    mut consume: impl FnMut(usize, T),
) {
    let threads = threads.min(count);
    if threads <= 1 {
        for k in 0..count {
            consume(k, f(k));
        }
        return;
    }
    let window = 2 * threads;
    // The next item to start, and the items consumed.
    let progress = std::sync::Mutex::new((0usize, 0usize));
    let moved = std::sync::Condvar::new();
    let lock = || {
        progress
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    };
    // Ends the threads when the caller's side ends (also on a panic): no
    // more items start, and none waits for the window.
    struct Ending<'a>(&'a dyn Fn());
    impl Drop for Ending<'_> {
        fn drop(&mut self) {
            (self.0)();
        }
    }
    let end = || {
        lock().0 = count;
        moved.notify_all();
    };
    type Outcome<T> = (usize, std::thread::Result<T>);
    let (sender, results) = std::sync::mpsc::channel::<Outcome<T>>();
    let work = |sender: std::sync::mpsc::Sender<Outcome<T>>| {
        loop {
            let k = {
                let mut p = lock();
                while p.0 < count && p.0 >= p.1 + window {
                    p = moved
                        .wait(p)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
                if p.0 >= count {
                    return;
                }
                p.0 += 1;
                p.0 - 1
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(k)));
            if sender.send((k, result)).is_err() {
                return;
            }
        }
    };
    std::thread::scope(|scope| {
        let _ending = Ending(&end);
        for _ in 0..threads {
            let (work, sender) = (&work, sender.clone());
            // (One that does not start leaves the work to the others.)
            let _ = std::thread::Builder::new()
                .name("mitcad-import-helper".to_owned())
                .stack_size(WORKER_STACK)
                .spawn_scoped(scope, move || work(sender));
        }
        // Once every thread has ended, receiving fails.
        drop(sender);
        let mut waiting = std::collections::BTreeMap::new();
        for k in 0..count {
            let result = loop {
                if let Some(result) = waiting.remove(&k) {
                    break result;
                }
                match results.recv() {
                    Ok((j, Ok(result))) => {
                        waiting.insert(j, result);
                    }
                    Ok((_, Err(panic))) => std::panic::resume_unwind(panic),
                    // No thread runs (none started): here.
                    Err(_) => {
                        let mut p = lock();
                        p.0 = p.0.max(k + 1);
                        drop(p);
                        break f(k);
                    }
                }
            };
            consume(k, result);
            lock().1 = k + 1;
            moved.notify_all();
        }
    });
}
