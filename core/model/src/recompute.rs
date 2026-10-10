// SPDX-License-Identifier: MIT
//! Recompute: evaluates the timeline up to the marker, feature by feature,
//! each against the body state the previous features left.
//!
//! Results are cached by what the evaluation read: the definition, the
//! values of the parameters it used and the identities (versions) of the
//! sketch outputs and body shapes it took as inputs. A cache hit hands out
//! the stored versions, so a whole chain of hits follows when inputs come
//! back. Undo, redo and restoring a value therefore do not recompute work
//! already done. A few results are kept per feature.
//!
//! A version is a fingerprint of what made the result (P7d): the feature's
//! uid, its definition and what it read, which holds the versions of its
//! inputs. The same definition with the same inputs therefore has the same
//! version in every document and every process, so results kept on disk
//! fit in again after cheap features before them were evaluated anew.
//!
//! A recompute given a [`RecomputeMonitor`] reports its progress to it and
//! stops between features when asked (P7), and the kernel's long
//! operations stop inside an evaluation (P7e, [`Kernel::interruptible`]).
//! An evaluation that returns after the request is not cached: it may be
//! one the request cut short.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::datum::Datum;
use crate::document::DocState;
use crate::features::{
    BodyChange, Env, EvalContext, FeatureDef, FeatureEntry, PlacementChange, SketchOutput,
};
use crate::fingerprint::Fingerprint;
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::{Kernel, KernelError};
use crate::monitor::{Cancelled, RecomputeMonitor};
use crate::parameters::{ParamId, Parameters};
use crate::store::ResultStore;
use crate::transform::Transform;

/// Identity of a computed result, sketch output or body shape: a
/// fingerprint of what made it (see the module comment).
pub(crate) type Version = u128;

#[derive(Debug, Clone)]
pub(crate) struct Versioned<T> {
    pub version: Version,
    pub value: T,
}

/// The bodies at a point of the timeline.
pub(crate) type BodyState<S> = BTreeMap<BodyUid, Versioned<S>>;

/// The outcome of a feature in the last recompute.
#[derive(Debug, Clone, PartialEq)]
pub enum FeatureStatus {
    Ok,
    /// Evaluation failed with this message; the state passed it by.
    Failed(String),
    Suppressed,
    /// After the timeline marker, not evaluated.
    RolledBack,
}

impl FeatureStatus {
    pub fn is_ok(&self) -> bool {
        *self == Self::Ok
    }

    /// `ok`, `error`, `suppressed` or `rolled_back`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed(_) => "error",
            Self::Suppressed => "suppressed",
            Self::RolledBack => "rolled_back",
        }
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub(crate) enum Change<S> {
    Set(BodyUid, Versioned<S>),
    Remove(BodyUid),
}

/// A mark a shared result can take later (atomic, so that results stay
/// `Sync` like the rest of the document).
#[derive(Debug, Default)]
pub(crate) struct Flag(AtomicBool);

impl Flag {
    pub fn new(value: bool) -> Self {
        Self(AtomicBool::new(value))
    }

    pub fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn set(&self, value: bool) {
        self.0.store(value, Ordering::Relaxed);
    }
}

/// What a feature evaluation produced, as stored in the cache.
#[derive(Debug)]
pub(crate) struct Output<S> {
    /// Identity of this result, for features that read another's output.
    pub version: Version,
    /// The fingerprint of the definition the version was made with.
    pub def_fp: u128,
    /// What the evaluation read: with the definition, the cache's key.
    pub reads: Vec<Read>,
    /// How long the evaluation took (for a result from the store, the
    /// evaluation that stored it).
    pub time: Duration,
    /// In the result store: read from it, or written to it (P7d).
    pub persisted: Flag,
    pub result: Result<(), String>,
    /// What an evaluation that succeeded built with caveats (P9).
    pub warnings: Vec<String>,
    /// A pattern's elements by number, suppressed ones too (P9).
    pub elements: Vec<Transform>,
    pub sketch: Option<Versioned<Arc<SketchOutput>>>,
    pub changes: Vec<Change<S>>,
    pub tool: Option<S>,
    pub datum: Option<Versioned<Datum>>,
    /// Occurrences the feature moves (F6).
    pub placements: Vec<PlacementChange>,
}

pub(crate) struct FeatureResult<S> {
    pub uid: FeatureUid,
    pub status: FeatureStatus,
    /// None for suppressed and rolled back features.
    pub output: Option<Arc<Output<S>>>,
}

impl<S> FeatureResult<S> {
    /// The warnings of a feature that succeeded (P9); none otherwise.
    pub fn warnings(&self) -> &[String] {
        match &self.output {
            Some(output) if self.status.is_ok() => &output.warnings,
            _ => &[],
        }
    }

    /// The shape this feature gave the body, if it set one.
    pub fn shape_set(&self, body: BodyUid) -> Option<&S> {
        self.output
            .as_ref()?
            .changes
            .iter()
            .find_map(|change| match change {
                Change::Set(uid, shape) if *uid == body => Some(&shape.value),
                _ => None,
            })
    }
}

/// A recorded input of an evaluation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Read {
    /// A parameter value (as bits), None when missing.
    Param(ParamId, Option<u64>),
    Sketch(FeatureUid, Option<Version>),
    Body(BodyUid, Option<Version>),
    AllBodies(Vec<(BodyUid, Version)>),
    /// The output of an earlier feature (its tool), None without one.
    Output(FeatureUid, Option<Version>),
    Datum(FeatureUid, Option<Version>),
    /// A body of another component (linked geometry, mitcad#100).
    BodyIn(ComponentUid, BodyUid, Option<Version>),
    /// Every body of another component.
    BodiesIn(ComponentUid, Vec<(BodyUid, Version)>),
    /// An occurrence's placement at this point (its matrix as bits).
    Placement(OccurrenceUid, Option<[u64; 12]>),
}

/// A placement's matrix as bits, for [`Read::Placement`].
pub(crate) fn placement_bits(t: &Transform) -> [u64; 12] {
    std::array::from_fn(|i| {
        let (r, c) = (i / 4, i % 4);
        if c == 3 {
            t.translation[r].to_bits()
        } else {
            t.linear[r][c].to_bits()
        }
    })
}

impl Read {
    pub(crate) fn matches<S>(&self, env: &Env<'_, S>) -> bool {
        match self {
            Self::Param(id, bits) => env.params.value(*id).map(f64::to_bits) == *bits,
            Self::Sketch(uid, version) => env.sketches.get(uid).map(|s| s.version) == *version,
            Self::Body(uid, version) => env.bodies.get(uid).map(|b| b.version) == *version,
            Self::AllBodies(bodies) => env
                .bodies
                .iter()
                .map(|(uid, b)| (*uid, b.version))
                .eq(bodies.iter().copied()),
            Self::Output(uid, version) => {
                env.history
                    .iter()
                    .find(|r| r.uid == *uid)
                    .and_then(|r| r.output.as_ref())
                    .map(|o| o.version)
                    == *version
            }
            Self::Datum(uid, version) => env.datums.get(uid).map(|d| d.version) == *version,
            Self::BodyIn(component, uid, version) => {
                env.components
                    .get(component)
                    .and_then(|bodies| bodies.get(uid))
                    .map(|b| b.version)
                    == *version
            }
            Self::BodiesIn(component, bodies) => env
                .components
                .get(component)
                .into_iter()
                .flat_map(|b| b.iter())
                .map(|(uid, b)| (*uid, b.version))
                .eq(bodies.iter().copied()),
            Self::Placement(uid, bits) => env.placements.get(uid).map(placement_bits) == *bits,
        }
    }

    /// Writes the read to a version's fingerprint: a parameter by its name
    /// (ids are numbered anew when a project is read), the rest by ids and
    /// versions.
    fn fingerprint(&self, fingerprint: &mut Fingerprint, params: &Parameters) {
        fn version(fingerprint: &mut Fingerprint, version: Option<Version>) {
            fingerprint.option(version, |f, v| {
                f.u128(v);
            });
        }
        fn body(fingerprint: &mut Fingerprint, uid: BodyUid) {
            fingerprint.u64(uid.feature.0).u32(uid.index);
        }
        match self {
            Self::Param(id, bits) => {
                fingerprint
                    .u8(0)
                    .str(&params.name(*id))
                    .option(*bits, |f, b| {
                        f.u64(b);
                    });
            }
            Self::Sketch(uid, v) => version(fingerprint.u8(1).u64(uid.0), *v),
            Self::Body(uid, v) => {
                body(fingerprint.u8(2), *uid);
                version(fingerprint, *v);
            }
            Self::AllBodies(bodies) => {
                fingerprint.u8(3).u64(bodies.len() as u64);
                for (uid, v) in bodies {
                    body(fingerprint, *uid);
                    fingerprint.u128(*v);
                }
            }
            Self::Output(uid, v) => version(fingerprint.u8(4).u64(uid.0), *v),
            Self::Datum(uid, v) => version(fingerprint.u8(5).u64(uid.0), *v),
            Self::BodyIn(component, uid, v) => {
                body(fingerprint.u8(6).u32(component.0), *uid);
                version(fingerprint, *v);
            }
            Self::BodiesIn(component, bodies) => {
                fingerprint.u8(7).u32(component.0).u64(bodies.len() as u64);
                for (uid, v) in bodies {
                    body(fingerprint, *uid);
                    fingerprint.u128(*v);
                }
            }
            Self::Placement(uid, bits) => {
                fingerprint.u8(8).u32(uid.0).option(*bits, |f, bits| {
                    for b in bits {
                        f.u64(b);
                    }
                });
            }
        }
    }
}

/// The fingerprint of a feature's definition (P7d), with parameters by
/// their names. A base feature's B-rep data is taken by its own
/// fingerprint, which the data keeps, so it is not compressed for this.
pub(crate) fn def_fingerprint(def: &FeatureDef, params: &Parameters) -> u128 {
    let mut fingerprint = Fingerprint::new("def");
    fingerprint.str(def.type_name());
    if let FeatureDef::Base(base) = def {
        base.fingerprint(&mut fingerprint);
    } else {
        let named = def
            .map_params(&mut |_, id| Ok::<_, ()>(params.name(*id)))
            .expect("names never fail");
        serde_json::to_writer(&mut fingerprint, &named).expect("definitions serialize");
    }
    fingerprint.finish()
}

/// The version of a feature's result: its uid, definition and reads.
pub(crate) fn output_version(
    uid: FeatureUid,
    def: u128,
    reads: &[Read],
    params: &Parameters,
) -> Version {
    let mut fingerprint = Fingerprint::new("output");
    fingerprint.u64(uid.0).u128(def).u64(reads.len() as u64);
    for read in reads {
        read.fingerprint(&mut fingerprint, params);
    }
    fingerprint.finish()
}

/// The parts of a result with versions of their own.
#[derive(Clone, Copy)]
pub(crate) enum Part {
    Sketch,
    /// The body of the i-th change.
    Body(usize),
    Datum,
}

/// The version of a part of the result `output`.
pub(crate) fn part_version(output: Version, part: Part) -> Version {
    let (block, index) = match part {
        Part::Sketch => (1, 0),
        Part::Body(i) => (2, i as u64),
        Part::Datum => (3, 0),
    };
    Fingerprint::new("part")
        .u128(output)
        .u8(block)
        .u64(index)
        .finish()
}

struct CacheEntry<S> {
    def: FeatureDef,
    output: Arc<Output<S>>,
    /// What the result takes in memory, estimated ([`output_bytes`]).
    bytes: u64,
    /// When it was last used, by the cache's clock.
    used: u64,
}

/// What the cache did, for diagnostics (P7d).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CacheStats {
    /// Features looked up (neither suppressed nor rolled back), and how
    /// many were in the cache.
    pub lookups: u64,
    pub hits: u64,
    /// Results taken from the result store instead of evaluated.
    pub restored: u64,
    /// Results dropped: beyond the memory budget or the results kept per
    /// feature, or cleared; and their bytes.
    pub evicted: u64,
    pub evicted_bytes: u64,
}

/// What one feature's results take in the cache.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CacheUsage {
    pub uid: FeatureUid,
    pub type_name: &'static str,
    pub results: usize,
    pub bytes: u64,
}

/// Stored results, newest first per feature, and the result store on disk
/// behind them (P7d). With a budget, the results used longest ago go when
/// the cache takes more memory than it (estimated): not those a result of
/// the document holds, which take their memory anyway. They can still come
/// from the store.
pub(crate) struct Cache<S> {
    entries: HashMap<FeatureUid, VecDeque<CacheEntry<S>>>,
    pub store: Option<ResultStore>,
    budget: Option<u64>,
    bytes: u64,
    clock: u64,
    pub stats: CacheStats,
}

/// Results kept per feature: enough for a few undo steps and value changes.
const ENTRIES_PER_FEATURE: usize = 8;

impl<S> Default for Cache<S> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            store: None,
            budget: None,
            bytes: 0,
            clock: 0,
            stats: CacheStats::default(),
        }
    }
}

/// The memory a result takes, estimated: its shapes as the kernel
/// estimates them ([`Kernel::shape_memory`]; parts shared with other
/// results are counted with each), a sketch's regions and what it read.
fn output_bytes<K: Kernel>(kernel: &K, output: &Output<K::Shape>) -> u64 {
    let mut bytes = 256 + 48 * output.reads.len() as u64;
    for change in &output.changes {
        if let Change::Set(_, shape) = change {
            bytes += kernel.shape_memory(&shape.value);
        }
    }
    if let Some(tool) = &output.tool {
        bytes += kernel.shape_memory(tool);
    }
    if let Some(sketch) = &output.sketch {
        let segments: usize = sketch
            .value
            .regions
            .iter()
            .flat_map(|r| &r.loops)
            .map(|l| l.segments.len())
            .sum();
        // The regions, and their outlines and areas again.
        bytes += 4096 + 2 * 160 * segments as u64;
    }
    bytes
}

// Definitions evaluated in parallel (the .f3d import, mitcad#95).
impl<S> Cache<S> {
    /// A copy for a document evaluated on another thread
    /// ([`crate::Document::fork`]): the same results (shared) and budget,
    /// without the result store.
    pub(crate) fn fork(&self) -> Self {
        let entries = self
            .entries
            .iter()
            .map(|(uid, list)| {
                let list = list
                    .iter()
                    .map(|e| CacheEntry {
                        def: e.def.clone(),
                        output: e.output.clone(),
                        bytes: e.bytes,
                        used: e.used,
                    })
                    .collect();
                (*uid, list)
            })
            .collect();
        Self {
            entries,
            store: None,
            budget: self.budget,
            bytes: self.bytes,
            clock: self.clock,
            stats: CacheStats::default(),
        }
    }

    /// Takes the newest results of `uids` from `other` (a fork's), as if
    /// they had been evaluated here; returns how many.
    pub(crate) fn adopt(&mut self, other: &Cache<S>, uids: &[FeatureUid]) -> usize {
        let mut adopted = 0;
        for uid in uids {
            let Some(theirs) = other.entries.get(uid).and_then(|l| l.front()) else {
                continue;
            };
            let used = self.tick();
            let entries = self.entries.entry(*uid).or_default();
            if let Some(i) = entries
                .iter()
                .position(|e| Arc::ptr_eq(&e.output, &theirs.output))
            {
                let mut known = entries.remove(i).expect("index is in range");
                known.used = used;
                entries.push_front(known);
                continue;
            }
            entries.push_front(CacheEntry {
                def: theirs.def.clone(),
                output: theirs.output.clone(),
                bytes: theirs.bytes,
                used,
            });
            self.bytes += theirs.bytes;
            while entries.len() > ENTRIES_PER_FEATURE {
                let dropped = entries.pop_back().expect("more than the limit");
                self.bytes -= dropped.bytes;
                self.stats.evicted += 1;
                self.stats.evicted_bytes += dropped.bytes;
            }
            adopted += 1;
        }
        self.trim();
        adopted
    }
}

impl<S: Clone> Recomputed<S> {
    /// A copy for a forked document ([`crate::Document::fork`]); the
    /// results are shared.
    pub(crate) fn fork(&self) -> Self {
        Self {
            results: self
                .results
                .iter()
                .map(|r| FeatureResult {
                    uid: r.uid,
                    status: r.status.clone(),
                    output: r.output.clone(),
                })
                .collect(),
            bodies: self.bodies.clone(),
            owners: self.owners.clone(),
            sketches: self.sketches.clone(),
            datums: self.datums.clone(),
            placements: self.placements.clone(),
            evaluated: Vec::new(),
            times: Vec::new(),
            restored: Vec::new(),
            restore_times: Vec::new(),
            joints: self.joints.clone(),
        }
    }
}

impl<S> Cache<S> {
    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    fn lookup(&mut self, entry: &FeatureEntry, env: &Env<'_, S>) -> Option<Arc<Output<S>>> {
        self.stats.lookups += 1;
        let now = self.tick();
        let entries = self.entries.get_mut(&entry.uid)?;
        let index = entries
            .iter()
            .position(|e| e.def == entry.def && e.output.reads.iter().all(|r| r.matches(env)))?;
        let mut hit = entries.remove(index).expect("index is in range");
        hit.used = now;
        let output = hit.output.clone();
        entries.push_front(hit);
        self.stats.hits += 1;
        Some(output)
    }

    fn insert<K: Kernel<Shape = S>>(
        &mut self,
        kernel: &K,
        entry: &FeatureEntry,
        output: Arc<Output<S>>,
    ) {
        let bytes = output_bytes(kernel, &output);
        let used = self.tick();
        let entries = self.entries.entry(entry.uid).or_default();
        entries.push_front(CacheEntry {
            def: entry.def.clone(),
            output,
            bytes,
            used,
        });
        self.bytes += bytes;
        while entries.len() > ENTRIES_PER_FEATURE {
            let dropped = entries.pop_back().expect("more than the limit");
            self.bytes -= dropped.bytes;
            self.stats.evicted += 1;
            self.stats.evicted_bytes += dropped.bytes;
        }
        self.trim();
    }

    /// The result the store keeps for the inputs `env` gives, now in the
    /// cache too.
    fn restore<K: Kernel<Shape = S>>(
        &mut self,
        kernel: &K,
        entry: &FeatureEntry,
        def_fp: u128,
        env: &Env<'_, S>,
    ) -> Option<Arc<Output<S>>> {
        let output = Arc::new(self.store.as_ref()?.find(kernel, entry, def_fp, env)?);
        self.insert(kernel, entry, output.clone());
        self.stats.restored += 1;
        Some(output)
    }

    /// The most memory the cache's results may take (estimated); None: no
    /// limit. Results go at once when they take more.
    pub fn set_budget(&mut self, budget: Option<u64>) {
        self.budget = budget;
        self.trim();
    }

    pub fn budget(&self) -> Option<u64> {
        self.budget
    }

    /// The memory the results take, estimated.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Drops the results used longest ago until they fit the budget,
    /// leaving those a result of the document holds.
    pub fn trim(&mut self) {
        let Some(budget) = self.budget else {
            return;
        };
        while self.bytes > budget && self.drop_oldest() {}
    }

    /// Drops every result no result of the document holds; returns their
    /// number and bytes.
    pub fn clear(&mut self) -> (u64, u64) {
        let before = (self.stats.evicted, self.stats.evicted_bytes);
        while self.drop_oldest() {}
        (
            self.stats.evicted - before.0,
            self.stats.evicted_bytes - before.1,
        )
    }

    /// Drops the result used longest ago that nothing else holds.
    fn drop_oldest(&mut self) -> bool {
        let oldest = self
            .entries
            .iter()
            .flat_map(|(uid, list)| list.iter().enumerate().map(move |(i, e)| (*uid, i, e)))
            .filter(|(_, _, e)| Arc::strong_count(&e.output) == 1)
            .min_by_key(|(_, _, e)| e.used)
            .map(|(uid, i, _)| (uid, i));
        let Some((uid, i)) = oldest else {
            return false;
        };
        let list = self.entries.get_mut(&uid).expect("found above");
        let dropped = list.remove(i).expect("found above");
        if list.is_empty() {
            self.entries.remove(&uid);
        }
        self.bytes -= dropped.bytes;
        self.stats.evicted += 1;
        self.stats.evicted_bytes += dropped.bytes;
        true
    }

    /// What each feature's results take, the largest first.
    pub fn usage(&self) -> Vec<CacheUsage> {
        let mut usage: Vec<CacheUsage> = self
            .entries
            .iter()
            .map(|(uid, list)| CacheUsage {
                uid: *uid,
                type_name: list.front().map_or("", |e| e.def.type_name()),
                results: list.len(),
                bytes: list.iter().map(|e| e.bytes).sum(),
            })
            .collect();
        usage.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.uid.cmp(&b.uid)));
        usage
    }
}

/// The evaluated timeline.
pub(crate) struct Recomputed<S> {
    /// One per timeline feature, in timeline order.
    pub results: Vec<FeatureResult<S>>,
    /// The bodies at the marker, of every component, each in its
    /// component's coordinates.
    pub bodies: Arc<BodyState<S>>,
    /// The component each body at the marker belongs to (F6).
    pub owners: BTreeMap<BodyUid, ComponentUid>,
    pub sketches: BTreeMap<FeatureUid, Versioned<Arc<SketchOutput>>>,
    /// The datums of construction features before the marker.
    pub datums: BTreeMap<FeatureUid, Versioned<Datum>>,
    /// Every occurrence's placement at the marker: its own transform, moved
    /// by the timeline features before the marker.
    pub placements: BTreeMap<OccurrenceUid, Transform>,
    /// Features evaluated by this recompute (cache misses), in order.
    pub evaluated: Vec<FeatureUid>,
    /// How long each of `evaluated` took (P10: which features make a
    /// recompute slow).
    pub times: Vec<Duration>,
    /// Features whose results came from the result store (P7d), in order,
    /// and how long reading each took.
    pub restored: Vec<FeatureUid>,
    pub restore_times: Vec<Duration>,
    /// Joints and rigid groups before the marker (mitcad#55).
    pub joints: crate::joints::JointsState,
}

impl<S> Default for Recomputed<S> {
    fn default() -> Self {
        Self {
            results: Vec::new(),
            bodies: Arc::new(BTreeMap::new()),
            owners: BTreeMap::new(),
            sketches: BTreeMap::new(),
            datums: BTreeMap::new(),
            placements: BTreeMap::new(),
            evaluated: Vec::new(),
            times: Vec::new(),
            restored: Vec::new(),
            restore_times: Vec::new(),
            joints: crate::joints::JointsState::default(),
        }
    }
}

impl<S> Recomputed<S> {
    pub fn result(&self, uid: FeatureUid) -> Option<&FeatureResult<S>> {
        self.results.iter().find(|r| r.uid == uid)
    }
}

/// Evaluates `state`'s timeline up to the marker. With a monitor, it
/// reports each feature and stops with [`Cancelled`] when asked, also
/// inside an evaluation where the kernel can ([`Kernel::interruptible`]);
/// what was evaluated before the request stays in the cache.
pub(crate) fn recompute<K: Kernel>(
    kernel: &K,
    state: &DocState,
    cache: &mut Cache<K::Shape>,
    monitor: Option<&Arc<RecomputeMonitor>>,
) -> Result<Recomputed<K::Shape>, Cancelled> {
    let mut done = Recomputed::default();
    let assembly = &state.assembly;
    let total = state.marker.min(state.features.len());
    // Each component's bodies; a feature sees only its own component's.
    let mut components: BTreeMap<ComponentUid, Arc<BodyState<K::Shape>>> = BTreeMap::new();
    let none: Arc<BodyState<K::Shape>> = Arc::new(BTreeMap::new());
    done.placements = assembly
        .occurrences
        .iter()
        .map(|o| (o.uid, o.transform))
        .collect();
    for (position, entry) in state.features.iter().enumerate() {
        if position < total
            && let Some(monitor) = monitor
        {
            monitor.enter(position, total)?;
        }
        let status = if position >= state.marker {
            Some(FeatureStatus::RolledBack)
        } else if entry.suppressed {
            Some(FeatureStatus::Suppressed)
        } else {
            None
        };
        if let Some(status) = status {
            done.results.push(FeatureResult {
                uid: entry.uid,
                status,
                output: None,
            });
            continue;
        }

        let own = components.get(&entry.component).unwrap_or(&none).clone();
        let env = Env {
            params: &state.parameters,
            bodies: &own,
            owners: &done.owners,
            assembly,
            sketches: &done.sketches,
            datums: &done.datums,
            features: &state.features,
            history: &done.results,
            components: &components,
            placements: &done.placements,
        };
        let output = match cache.lookup(entry, &env) {
            Some(output) => output,
            None => {
                let def_fp = def_fingerprint(&entry.def, &state.parameters);
                let start = Instant::now();
                if let Some(output) = cache.restore(kernel, entry, def_fp, &env) {
                    done.restore_times.push(start.elapsed());
                    done.restored.push(entry.uid);
                    output
                } else {
                    let output = match monitor {
                        Some(monitor) => {
                            monitor.evaluating(&entry.name);
                            kernel.interruptible(monitor, || evaluate(kernel, entry, env, def_fp))
                        }
                        None => evaluate(kernel, entry, env, def_fp),
                    };
                    // A result that came back after a cancel request is not
                    // kept: the request may have cut it short (P7), or the
                    // kernel stopped inside and failed (P7e).
                    if let Some(monitor) = monitor {
                        monitor.evaluation_done()?;
                    }
                    done.times.push(output.time);
                    // Nor is a failure for lack of memory: the feature may
                    // build once memory is free again (mitcad#80).
                    if !matches!(&output.result, Err(m) if KernelError::is_out_of_memory(m)) {
                        cache.insert(kernel, entry, output.clone());
                    }
                    if let Some(monitor) = monitor {
                        monitor.kept();
                    }
                    done.evaluated.push(entry.uid);
                    output
                }
            }
        };

        let mut status = match &output.result {
            Ok(()) => FeatureStatus::Ok,
            Err(message) => FeatureStatus::Failed(message.clone()),
        };
        // Placements depend on the assembly's flags, which the cache does
        // not key on: they are applied here.
        // A move takes the occurrences rigid groups join to the moved one
        // along (mitcad#55).
        let placements = if done.joints.groups.is_empty() {
            output.placements.clone()
        } else {
            crate::joints::with_groups(&done.joints.groups, entry.component, &output.placements)
        };
        if status.is_ok()
            && let Err(message) = place(state, entry, &placements, &mut done.placements)
        {
            status = FeatureStatus::Failed(message);
        }
        // Joints and rigid groups (mitcad#55) read the placements and the
        // other components' bodies at this point: applied here too.
        if status.is_ok()
            && crate::joints::applies(&entry.def)
            && let Err(message) = crate::joints::apply(kernel, state, entry, &components, &mut done)
        {
            status = FeatureStatus::Failed(message);
        }
        if status.is_ok() || output.placements.is_empty() {
            if let Some(sketch) = &output.sketch {
                done.sketches.insert(entry.uid, sketch.clone());
            }
            if let Some(datum) = &output.datum {
                done.datums.insert(entry.uid, datum.clone());
            }
            // Bodies the feature sets go to the component it made, if any.
            let target = assembly.created_by(entry.uid).unwrap_or(entry.component);
            for change in &output.changes {
                match change {
                    Change::Set(uid, shape) => {
                        if let Some(old) = done.owners.insert(*uid, target)
                            && old != target
                            && let Some(bodies) = components.get_mut(&old)
                        {
                            Arc::make_mut(bodies).remove(uid);
                        }
                        Arc::make_mut(components.entry(target).or_default())
                            .insert(*uid, shape.clone());
                    }
                    Change::Remove(uid) => {
                        if let Some(old) = done.owners.remove(uid)
                            && let Some(bodies) = components.get_mut(&old)
                        {
                            Arc::make_mut(bodies).remove(uid);
                        }
                    }
                }
            }
        }
        done.results.push(FeatureResult {
            uid: entry.uid,
            status,
            output: Some(output),
        });
    }
    let mut all = BTreeMap::new();
    for bodies in components.values() {
        all.extend(bodies.iter().map(|(uid, b)| (*uid, b.clone())));
    }
    done.bodies = Arc::new(all);
    // The joints' frames and values where the occurrences are at the
    // marker.
    crate::joints::finish(&mut done.joints, &done.placements);
    if let Some(monitor) = monitor {
        monitor.finish(total);
    }
    Ok(done)
}

/// Applies a feature's placement changes: each occurrence must be placed
/// in the feature's component and not be grounded.
pub(crate) fn place(
    state: &DocState,
    entry: &FeatureEntry,
    changes: &[PlacementChange],
    placements: &mut BTreeMap<OccurrenceUid, Transform>,
) -> Result<(), String> {
    let assembly = &state.assembly;
    let mut next = placements.clone();
    for change in changes {
        let (PlacementChange::Move(uid, _) | PlacementChange::Set(uid, _)) = change;
        let Some(occurrence) = assembly.occurrence(*uid) else {
            return Err(format!("occurrence {uid} does not exist"));
        };
        let name = assembly.occurrence_name(*uid);
        if occurrence.parent != entry.component {
            return Err(format!(
                "{name} is placed in {}, not in {}, the component of this feature",
                assembly.name(occurrence.parent),
                assembly.name(entry.component)
            ));
        }
        if occurrence.grounded {
            return Err(format!("{name} is grounded"));
        }
        let current = next.get(uid).copied().unwrap_or(occurrence.transform);
        let placed = match change {
            PlacementChange::Move(_, by) => by.after(&current),
            PlacementChange::Set(_, at) => *at,
        };
        next.insert(*uid, placed);
    }
    *placements = next;
    Ok(())
}

fn evaluate<K: Kernel>(
    kernel: &K,
    entry: &FeatureEntry,
    env: Env<'_, K::Shape>,
    def_fp: u128,
) -> Arc<Output<K::Shape>> {
    let params = env.params;
    let start = Instant::now();
    let mut ctx = EvalContext::new(kernel, entry.uid, env);
    let result = entry.def.evaluator::<K>().evaluate(&mut ctx);
    let time = start.elapsed();
    let reads = std::mem::take(&mut ctx.reads);
    let warnings = std::mem::take(&mut ctx.warnings);
    let elements = std::mem::take(&mut ctx.elements);
    let version = output_version(entry.uid, def_fp, &reads, params);
    let output = match result {
        Ok(produced) => Output {
            version,
            def_fp,
            reads,
            time,
            persisted: Flag::new(false),
            result: Ok(()),
            warnings,
            elements,
            sketch: produced.sketch.map(|sketch| Versioned {
                version: part_version(version, Part::Sketch),
                value: Arc::new(sketch),
            }),
            changes: produced
                .changes
                .into_iter()
                .enumerate()
                .map(|(i, change)| match change {
                    BodyChange::Set(uid, shape) => Change::Set(
                        uid,
                        Versioned {
                            version: part_version(version, Part::Body(i)),
                            value: shape,
                        },
                    ),
                    BodyChange::Remove(uid) => Change::Remove(uid),
                })
                .collect(),
            tool: produced.tool,
            datum: produced.datum.map(|datum| Versioned {
                version: part_version(version, Part::Datum),
                value: datum,
            }),
            placements: produced.placements,
        },
        Err(message) => Output {
            version,
            def_fp,
            reads,
            time,
            persisted: Flag::new(false),
            result: Err(message),
            warnings: Vec::new(),
            elements: Vec::new(),
            sketch: None,
            changes: Vec::new(),
            tool: None,
            datum: None,
            placements: Vec::new(),
        },
    };
    Arc::new(output)
}
