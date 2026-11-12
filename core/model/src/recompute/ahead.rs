// SPDX-License-Identifier: MIT
//! Features of independent components evaluated in parallel (mitcad#842).
//!
//! A recompute goes through the timeline one feature after another, the
//! features of every component in one list. A feature sees only its own
//! component's bodies, so while one component's fillet runs, the features
//! of other components could run beside it. With workers
//! (`Cache::workers` above one; `Document::set_recompute_workers`, the
//! environment variable `MITCAD_RECOMPUTE_WORKERS` for a whole test run),
//! the recompute cuts the timeline into windows and evaluates the
//! features of each window's components ahead, each component on a worker
//! thread with a kernel of its own ([`Kernel::fork`]), while it goes
//! through the window itself in timeline order exactly as without workers
//! (`step`).
//!
//! **Taken in order.** Where the recompute comes to a feature that the
//! cache and the result store do not have, it takes the worker's result
//! for it (waiting while the worker evaluates it) instead of evaluating
//! it, under the condition under which the cache hands out a stored
//! result: the same definition, and every read the evaluation recorded
//! (parameters, sketches, bodies, other features' outputs, placements, the
//! assembly; `Read::matches`) holds where the recompute is. In addition
//! the definition's own parameters have the values the worker saw, and
//! the evaluation succeeded (a failure, which may be the worker thread's
//! own, such as a lack of memory or stack, is evaluated again on the
//! recompute's thread). Otherwise the recompute evaluates the feature
//! itself and cancels the rest of that worker's work, which went on from
//! the result not taken. A result taken goes into the cache, the monitor
//! and the recompute's lists exactly where an evaluation of the
//! recompute's own would: uids, topological names, versions, statuses,
//! warnings and the memory cache are those of the recompute without
//! workers, whatever the workers did, and the result store is written
//! from the document's results afterwards as always. The workers keep
//! nothing: their results reach the cache only through the recompute.
//!
//! **Windows and barriers.** A window runs from where the recompute is up
//! to the next barrier, a feature after which other components may read
//! what it changed, evaluated by the recompute alone once everything
//! before it is taken:
//! - joints, rigid groups, constraint sets and motion links (applied after
//!   their evaluation on the whole assembly), and features that move
//!   occurrences or make, capture or pattern them (Move, Capture Position,
//!   Joint Origin, Pattern at Points, Component from Bodies, a feature that
//!   made a component, Body Transfer): what others read of the placements
//!   and the assembly;
//! - sketches with driven dimensions (mitcad#231) and Bound Length
//!   (mitcad#375): they write the parameters later features of any
//!   component read;
//! - an extrusion that cuts across components (its cut scope) and any
//!   feature whose last result moved occurrences, measured parameters or
//!   gave bodies to another component.
//!
//! A feature that reads other components (linked geometry, mitcad#100:
//! links in its definition, or reads of other components' bodies,
//! placements or the assembly in its last result, or of what a feature of
//! another component in the window made) waits: the recompute evaluates
//! it in order once what it reads is there, and the later features of its
//! component in the window too. The rest of each component's features in
//! the window are a worker's job, in timeline order. A window is
//! evaluated ahead only when at least two components have a feature there
//! that the cache likely lacks (no result of its definition whose reads of
//! what is known at the window's start still hold: parameter values and
//! what features before the window made); otherwise its features go one
//! after another as without workers, and a component whose features the
//! cache likely has gets no worker. The barriers keep the workers from
//! evaluating what would not be taken; the condition above keeps the
//! result right where they miss something.
//!
//! **Threads and memory.** At most `Cache::workers` threads evaluate at
//! once (OCCT's booleans use every core inside an operation already),
//! each with a stack of [`WORKER_STACK`] and its kernel's thread set-up
//! ([`Kernel::worker_thread`]). Results not yet taken count against what
//! is left of the memory cache's budget: a worker stops once they pass
//! it, and the recompute evaluates the rest. The cache keeps a result
//! while anything else holds it, and a worker holds the cached results it
//! found: before a result goes into the cache and makes it drop others for
//! its budget, the recompute stops the workers and waits for them
//! (`Ahead::release`), so that the cache drops what it drops without
//! workers. A worker's own recomputes
//! (a derived part's source) use no workers.
//!
//! **Monitor and stop.** The monitor hears of the features in timeline
//! order from the recompute, as without workers. Each job's monitor is
//! part of the recompute's ([`RecomputeMonitor::within`]), so a stop
//! request cancels every worker, also inside its kernel operations; the
//! recompute stops while it waits for a worker too, and nothing the
//! workers returned is kept.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use super::{
    Cache, FeatureResult, FeatureStatus, Output, Read, Run, Walk, def_fingerprint, evaluate,
    never_cached, output_bytes, step,
};
use crate::document::DocState;
use crate::features::{Env, FeatureDef, FeatureEntry};
use crate::ids::{ComponentUid, FeatureUid};
use crate::kernel::Kernel;
use crate::monitor::{Cancelled, RecomputeMonitor};
use crate::parameters::{ParamId, Parameters};

/// The stack of a worker: 64 MiB, as the `.f3d` import's workers have
/// (the geometry kernel recurses deeply).
const WORKER_STACK: usize = 64 << 20;

/// How often the recompute looks for a stop while it waits for a worker.
const WAIT_STEP: Duration = Duration::from_millis(20);

/// The workers a cache starts with: `MITCAD_RECOMPUTE_WORKERS` when it is
/// a number above zero (a test run of the model's tests with several),
/// else 1, no workers.
pub(super) fn default_workers() -> usize {
    static WORKERS: OnceLock<usize> = OnceLock::new();
    *WORKERS.get_or_init(|| {
        std::env::var("MITCAD_RECOMPUTE_WORKERS")
            .ok()
            .and_then(|value| value.trim().parse().ok())
            .filter(|workers| *workers > 0)
            .unwrap_or(1)
    })
}

thread_local! {
    /// Whether this thread is a recompute's worker.
    static ON_WORKER: Cell<bool> = const { Cell::new(false) };
}

/// Whether this thread is a recompute's worker: its own recomputes use no
/// workers.
pub(super) fn on_worker() -> bool {
    ON_WORKER.with(Cell::get)
}

/// How a feature takes part in a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Evaluated ahead on its component's worker.
    Ahead,
    /// Reads other components: evaluated in order by the recompute, and
    /// the later features of its component in the window too.
    Waits,
    /// Ends the window: evaluated by the recompute once all before it is
    /// taken.
    Barrier,
}

/// A stretch of the timeline whose components' features are evaluated
/// ahead.
pub(super) struct Window {
    /// Where it ends: at a barrier, the marker or the end of the timeline.
    pub(super) end: usize,
    /// The features each worker job evaluates, in timeline order: one job
    /// per component, the jobs by their first feature.
    jobs: Vec<Vec<usize>>,
}

impl Window {
    /// Whether it is worth evaluating ahead: two jobs or more.
    pub(super) fn parallel(&self) -> bool {
        self.jobs.len() >= 2
    }
}

impl<S> Cache<S> {
    /// The newest result kept of a feature, for planning (no lookup:
    /// nothing is counted or marked used).
    fn newest(&self, uid: FeatureUid) -> Option<&Output<S>> {
        self.entries.get(&uid)?.front().map(|e| e.output.as_ref())
    }

    /// The results kept of the feature's definition, newest first, as a
    /// lookup goes through them.
    fn candidates(&self, entry: &FeatureEntry) -> Vec<Arc<Output<S>>> {
        if never_cached(&entry.def) {
            return Vec::new();
        }
        self.entries
            .get(&entry.uid)
            .into_iter()
            .flatten()
            .filter(|e| e.def == entry.def)
            .map(|e| e.output.clone())
            .collect()
    }

    /// Whether the cache likely lacks the feature's result where the
    /// window that begins at `start` puts it: no result of its definition
    /// is kept whose reads of what is known at the window's start hold
    /// (`walk`: parameter values, the join tolerance, library sources and
    /// what features before the window made); what features in the window
    /// make is not known yet.
    fn likely_lacks(
        &self,
        entry: &FeatureEntry,
        walk: &Walk<'_, S>,
        state: &DocState,
        start: usize,
    ) -> bool {
        if never_cached(&entry.def) {
            return true;
        }
        let Some(kept) = self.entries.get(&entry.uid) else {
            return true;
        };
        let done = &walk.done;
        // The position of a feature before the window.
        let before = |uid: &FeatureUid| {
            state
                .feature_index
                .position(&state.features, *uid)
                .filter(|p| *p < start)
        };
        !kept.iter().any(|e| {
            e.def == entry.def
                && e.output.reads.iter().all(|read| match read {
                    Read::Param(id, bits) => walk.params.read_bits(*id) == *bits,
                    Read::JoinTolerance(bits) => state.join_tolerance.map(f64::to_bits) == *bits,
                    Read::Source(id, version) => state.library_sources.version(*id) == *version,
                    Read::PrintBed(uid, face) => {
                        super::print_bed_of(&state.body_attributes, *uid) == face.as_ref()
                    }
                    Read::PrintLayer(bits) => {
                        state.design_layer_height().map(f64::to_bits) == *bits
                    }
                    Read::Sketch(uid, version) if before(uid).is_some() => {
                        done.sketches.get(uid).map(|s| s.version) == *version
                    }
                    Read::Datum(uid, version) if before(uid).is_some() => {
                        done.datums.get(uid).map(|d| d.version) == *version
                    }
                    Read::Output(uid, version) => match before(uid) {
                        Some(p) => {
                            done.results
                                .get(p)
                                .filter(|r| r.uid == *uid)
                                .and_then(|r| r.output.as_ref())
                                .map(|o| o.version)
                                == *version
                        }
                        None => true,
                    },
                    _ => true,
                })
        })
    }
}

/// The window that begins at `start` (before the marker, `total`), where
/// the recompute is (`walk`): its end and its workers' jobs (see the
/// module comment).
pub(super) fn plan<S>(
    state: &DocState,
    cache: &Cache<S>,
    walk: &Walk<'_, S>,
    start: usize,
    total: usize,
) -> Window {
    // Each component's features evaluated ahead, and whether the cache
    // likely lacks one of them.
    let mut jobs: BTreeMap<ComponentUid, (Vec<usize>, bool)> = BTreeMap::new();
    let mut waiting: BTreeSet<ComponentUid> = BTreeSet::new();
    let mut end = start;
    while end < total {
        let entry = &state.features[end];
        if !entry.suppressed {
            match role(state, entry, cache.newest(entry.uid), start) {
                Role::Barrier => break,
                Role::Waits => {
                    waiting.insert(entry.component);
                }
                Role::Ahead if !waiting.contains(&entry.component) => {
                    let job = jobs.entry(entry.component).or_default();
                    job.0.push(end);
                    job.1 |= cache.likely_lacks(entry, walk, state, start);
                }
                Role::Ahead => {}
            }
        }
        end += 1;
    }
    let mut jobs: Vec<Vec<usize>> = jobs
        .into_values()
        .filter(|(_, lacks)| *lacks)
        .map(|(positions, _)| positions)
        .collect();
    jobs.sort_by_key(|positions| positions[0]);
    Window { end, jobs }
}

/// How the feature `entry` takes part in the window that begins at
/// `start`, by its definition and its newest kept result (`newest`).
fn role<S>(
    state: &DocState,
    entry: &FeatureEntry,
    newest: Option<&Output<S>>,
    start: usize,
) -> Role {
    let def = &entry.def;
    let changes_others = crate::joints::applies(def)
        || never_cached(def)
        || state.assembly.created_by(entry.uid).is_some()
        || matches!(
            def,
            FeatureDef::ComponentFromBodies(_)
                | FeatureDef::MoveOccurrence(_)
                | FeatureDef::CapturePosition(_)
                | FeatureDef::JointOrigin(_)
                | FeatureDef::PatternAtPoints(_)
                | FeatureDef::BoundLength(_)
                | FeatureDef::BodyTransfer(_)
        )
        || matches!(def, FeatureDef::Extrude(e) if e.active_cut_scope().is_some())
        || matches!(def, FeatureDef::Sketch(s) if s.dimensions.iter().any(|d| d.reference.is_some()))
        || newest.is_some_and(|output| {
            !output.placements.is_empty()
                || !output.measured.is_empty()
                || output.owners.values().any(|c| *c != entry.component)
        });
    if changes_others {
        return Role::Barrier;
    }
    let made_by_another = |uid: FeatureUid| {
        state
            .feature_index
            .position(&state.features, uid)
            .is_some_and(|p| p >= start && state.features[p].component != entry.component)
    };
    let reads_others = crate::policy::links_other_components(def)
        // An Envelope reads the bodies of the occurrences placed in its
        // source (mitcad#1271), from its first evaluation on.
        || matches!(def, FeatureDef::Envelope(_))
        || newest.is_some_and(|output| {
            output.reads.iter().any(|read| match read {
                Read::BodyIn(..) | Read::BodiesIn(..) | Read::Placement(..) | Read::Assembly(_) => {
                    true
                }
                Read::Sketch(uid, _)
                | Read::Datum(uid, _)
                | Read::Output(uid, _)
                | Read::SheetRules(uid, _) => made_by_another(*uid),
                Read::Body(body, _) | Read::Sheet(body, _) => made_by_another(body.feature),
                _ => false,
            })
        });
    if reads_others {
        Role::Waits
    } else {
        Role::Ahead
    }
}

/// What a worker sends for each feature of its job, in order.
struct Message<S> {
    position: usize,
    /// The result it evaluated; None where it found one in the cache's
    /// results (the recompute finds it there too).
    output: Option<Arc<Output<S>>>,
    /// The values (bits) of the parameters the definition names, as the
    /// worker had them.
    inputs: Vec<(ParamId, Option<u64>)>,
    /// What `output` takes in memory, estimated.
    bytes: u64,
}

/// A worker's job: one component's features of a window.
struct Job<'s, S> {
    positions: Vec<usize>,
    /// For each, the results the cache keeps of its definition, newest
    /// first: the worker takes the first whose reads hold, as the cache
    /// does.
    cached: Vec<Vec<Arc<Output<S>>>>,
    /// The timeline at the window's start, which the worker goes on from
    /// with its own features.
    walk: Walk<'s, S>,
    send: mpsc::Sender<Message<S>>,
    monitor: Arc<RecomputeMonitor>,
}

/// The recompute's side of a job.
struct Lane<S> {
    receive: mpsc::Receiver<Message<S>>,
    monitor: Arc<RecomputeMonitor>,
    /// A message for a later feature than the one asked for.
    pending: Option<Message<S>>,
    /// No message is wanted or will come any more.
    closed: bool,
}

/// The workers' results of a window, which the recompute takes in
/// timeline order; dropping it cancels the workers still evaluating.
pub(super) struct Ahead<'a, 's, S> {
    lanes: Vec<Lane<S>>,
    /// The lane of each feature evaluated ahead, by its position.
    lane_of: HashMap<usize, usize>,
    /// What the results sent and not yet taken take in memory, estimated.
    held: &'a AtomicU64,
    /// The jobs no worker has taken yet, and the workers.
    queue: &'a Mutex<VecDeque<Job<'s, S>>>,
    workers: Vec<std::thread::ScopedJoinHandle<'a, ()>>,
}

impl<S> Drop for Ahead<'_, '_, S> {
    fn drop(&mut self) {
        for lane in &self.lanes {
            lane.monitor.cancel();
        }
    }
}

impl<S> Ahead<'_, '_, S> {
    /// Stops the workers and waits for them to end, before the cache drops
    /// results for its budget: a worker holds results of the cache (those
    /// it found there), which the cache keeps while anything holds them,
    /// and the recompute without workers would not have kept them. What
    /// the workers sent is still taken; the rest of the window is
    /// evaluated by the recompute.
    pub(super) fn release(&mut self) {
        for lane in &self.lanes {
            lane.monitor.cancel();
        }
        self.queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        for worker in self.workers.drain(..) {
            if let Err(panic) = worker.join() {
                std::panic::resume_unwind(panic);
            }
        }
    }

    /// The result a worker evaluated ahead for the feature at `position`,
    /// waiting while it evaluates: None where no worker did, where it
    /// failed, or where it does not hold here (`env`: the definition's
    /// fingerprint `def_fp`, the reads and the definition's parameter
    /// values). Stops when `monitor` is cancelled.
    pub(super) fn take(
        &mut self,
        position: usize,
        def_fp: u128,
        env: &Env<'_, S>,
        monitor: Option<&Arc<RecomputeMonitor>>,
    ) -> Result<Option<Arc<Output<S>>>, Cancelled> {
        let Some(&index) = self.lane_of.get(&position) else {
            return Ok(None);
        };
        let held = self.held;
        let lane = &mut self.lanes[index];
        let message = loop {
            let message = match lane.pending.take() {
                Some(message) => message,
                None if lane.closed => return Ok(None),
                None => match lane.receive.recv_timeout(WAIT_STEP) {
                    Ok(message) => message,
                    Err(RecvTimeoutError::Timeout) => {
                        if monitor.is_some_and(|m| m.is_cancelled()) {
                            return Err(Cancelled);
                        }
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        lane.closed = true;
                        return Ok(None);
                    }
                },
            };
            if message.position > position {
                // The worker passed this feature by (it stopped there).
                lane.pending = Some(message);
                return Ok(None);
            }
            held.fetch_sub(message.bytes, Ordering::Relaxed);
            // (A feature before it that the recompute found elsewhere.)
            if message.position == position {
                break message;
            }
        };
        let Some(output) = message.output else {
            return Ok(None);
        };
        if output.result.is_err() {
            return Ok(None);
        }
        let holds = output.def_fp == def_fp
            && output.reads.iter().all(|read| read.matches(env))
            && message
                .inputs
                .iter()
                .all(|(id, bits)| env.params.read_bits(*id) == *bits);
        if !holds {
            // The worker went on from the result not taken: the rest of
            // its job is evaluated here.
            lane.monitor.cancel();
            lane.closed = true;
            while let Ok(message) = lane.receive.try_recv() {
                held.fetch_sub(message.bytes, Ordering::Relaxed);
            }
            return Ok(None);
        }
        Ok(Some(output))
    }
}

/// Goes through the window that begins at `start` with workers that
/// evaluate its jobs ahead: the recompute goes through its features in
/// timeline order as without workers ([`step`]) and takes the workers'
/// results where they hold. False, having done nothing, when the kernel
/// cannot fork.
pub(super) fn run_window<K: Kernel>(
    run: &Run<'_, K>,
    cache: &mut Cache<K::Shape>,
    walk: &mut Walk<'_, K::Shape>,
    start: usize,
    window: &Window,
) -> Result<bool, Cancelled> {
    let state = run.state;
    let threads = cache.workers.min(window.jobs.len());
    let kernels: Vec<K> = (0..threads).map_while(|_| run.kernel.fork()).collect();
    if kernels.is_empty() {
        return Ok(false);
    }
    // What is left of the memory cache's budget for results not taken.
    let room = cache.budget().map(|b| b.saturating_sub(cache.bytes()));
    let held = AtomicU64::new(0);
    let mut lanes = Vec::with_capacity(window.jobs.len());
    let mut lane_of = HashMap::new();
    let mut jobs = VecDeque::with_capacity(window.jobs.len());
    for positions in &window.jobs {
        let (send, receive) = mpsc::channel();
        let monitor = Arc::new(match run.monitor {
            Some(monitor) => RecomputeMonitor::within(vec![monitor.clone()], None),
            None => RecomputeMonitor::new(),
        });
        for position in positions {
            lane_of.insert(*position, lanes.len());
        }
        jobs.push_back(Job {
            positions: positions.clone(),
            cached: positions
                .iter()
                .map(|p| cache.candidates(&state.features[*p]))
                .collect(),
            walk: walk.snapshot(),
            send,
            monitor: monitor.clone(),
        });
        lanes.push(Lane {
            receive,
            monitor,
            pending: None,
            closed: false,
        });
    }
    let queue = Mutex::new(jobs);
    std::thread::scope(|scope| -> Result<bool, Cancelled> {
        let mut workers = Vec::new();
        for kernel in kernels {
            let (queue, held) = (&queue, &held);
            let spawned = std::thread::Builder::new()
                .name("mitcad-recompute-worker".to_owned())
                .stack_size(WORKER_STACK)
                .spawn_scoped(scope, move || {
                    kernel.worker_thread(|| work(&kernel, state, queue, held, room));
                });
            if let Ok(worker) = spawned {
                workers.push(worker);
            }
        }
        if workers.is_empty() {
            // (Drops the jobs' senders: the recompute evaluates every
            // feature itself.)
            queue.lock().unwrap_or_else(PoisonError::into_inner).clear();
        }
        let mut ahead = Ahead {
            lanes,
            lane_of,
            held: &held,
            queue: &queue,
            workers,
        };
        for position in start..window.end {
            step(run, cache, walk, position, Some(&mut ahead))?;
        }
        Ok(true)
    })
}

/// A worker: takes jobs until none is left.
fn work<K: Kernel>(
    kernel: &K,
    state: &DocState,
    queue: &Mutex<VecDeque<Job<'_, K::Shape>>>,
    held: &AtomicU64,
    room: Option<u64>,
) {
    ON_WORKER.with(|on| on.set(true));
    loop {
        let job = queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front();
        match job {
            Some(job) => speculate(kernel, state, job, held, room),
            None => return,
        }
    }
}

/// Evaluates a job's features in order, each where the features before it
/// in the job left the timeline, and sends what it found for each; stops
/// when the job is cancelled, when the recompute no longer listens, or
/// when the results not taken pass `room`.
fn speculate<K: Kernel>(
    kernel: &K,
    state: &DocState,
    job: Job<'_, K::Shape>,
    held: &AtomicU64,
    room: Option<u64>,
) {
    let Job {
        positions,
        cached,
        mut walk,
        send,
        monitor,
    } = job;
    for (position, cached) in positions.into_iter().zip(cached) {
        if monitor.is_cancelled() {
            return;
        }
        // The features before it that are not the job's: unknown here, as
        // if rolled back (one per position, as a recompute's results are).
        while walk.done.results.len() < position {
            let other = &state.features[walk.done.results.len()];
            walk.done.results.push(FeatureResult {
                uid: other.uid,
                status: if other.suppressed {
                    FeatureStatus::Suppressed
                } else {
                    FeatureStatus::RolledBack
                },
                output: None,
                applied_warnings: None,
            });
        }
        let entry = &state.features[position];
        let own = walk
            .components
            .get(&entry.component)
            .cloned()
            .unwrap_or_default();
        let env = walk.env(state, &own, &[]);
        let found = cached
            .iter()
            .find(|output| output.reads.iter().all(|read| read.matches(&env)))
            .cloned();
        let (output, message) = match found {
            Some(output) => (
                output,
                Message {
                    position,
                    output: None,
                    inputs: Vec::new(),
                    bytes: 0,
                },
            ),
            None => {
                if room.is_some_and(|room| held.load(Ordering::Relaxed) > room) {
                    return;
                }
                let def_fp = def_fingerprint(&entry.def, env.params);
                let output =
                    kernel.interruptible(&monitor, || evaluate(kernel, entry, env, def_fp));
                // (Cut short, perhaps: not sent.)
                if monitor.is_cancelled() {
                    return;
                }
                let bytes = output_bytes(kernel, &output);
                held.fetch_add(bytes, Ordering::Relaxed);
                let message = Message {
                    position,
                    output: Some(output.clone()),
                    inputs: parameter_inputs(&entry.def, env.params),
                    bytes,
                };
                (output, message)
            }
        };
        drop(own);
        let bytes = message.bytes;
        if send.send(message).is_err() {
            held.fetch_sub(bytes, Ordering::Relaxed);
            return;
        }
        walk.apply(kernel, state, entry, output);
    }
}

/// The parameters a definition names with their values (bits).
fn parameter_inputs(def: &FeatureDef, params: &Parameters) -> Vec<(ParamId, Option<u64>)> {
    let mut inputs = Vec::new();
    let _ = def.map_params(&mut |_, id: &ParamId| {
        inputs.push((*id, params.read_bits(*id)));
        Ok::<_, ()>(())
    });
    inputs
}
