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
//!
//! With workers (`Cache::workers` above one), the features of
//! independent components are evaluated ahead on threads of their own and
//! their results taken in timeline order (mitcad#842, `recompute/ahead.rs`,
//! which describes the scheduling and its barriers); the result is the
//! one without workers.
//!
//! A result read from the result store keeps the shapes of the bodies it
//! sets packed (mitcad#858, [`Changes`]): the body states hold a stand-in
//! ([`Kernel::shape_unpacker`]) under the stored version, and the shapes are
//! unpacked once something reads them: an evaluation unpacks the results
//! that set the bodies it reads as it reads them (mitcad#886: a sketch on a
//! face the result that set the face's body, a join its participants'),
//! and the walk's states hold those shapes after it; the others stay
//! packed. A joint unpacks every body first, and the bodies left at the end
//! are unpacked. A state a later restored result replaces before that is
//! never unpacked; its result unpacks it if something reads it later (the
//! timeline's history). Unpacking an evaluation does is not part of its
//! time. Not where workers evaluate ahead, nor for a preview.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use crate::datum::Datum;
use crate::document::DocState;
use crate::features::{
    BodyChange, Brep, Env, EvalContext, FeatureDef, FeatureEntry, PlacementChange, SketchOutput,
};
use crate::fingerprint::Fingerprint;
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::{Kernel, KernelError, ShapeUnpacker};
use crate::monitor::{Cancelled, RecomputeMonitor};
use crate::parameters::{ParamId, Parameters};
use crate::store::ResultStore;
use crate::transform::Transform;

// Features of independent components evaluated in parallel (mitcad#842).
mod ahead;
// Bodies handed out as soon as no later feature changes them (mitcad#881).
mod final_bodies;
pub use final_bodies::{FinalBodies, FinalBody};

/// Identity of a computed result, sketch output or body shape: a
/// fingerprint of what made it (see the module comment).
pub(crate) type Version = u128;

#[derive(Debug, Clone)]
pub struct Versioned<T> {
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
pub enum Change<S> {
    Set(BodyUid, Versioned<S>),
    Remove(BodyUid),
}

impl<S> Change<S> {
    pub fn head(&self) -> ChangeHead {
        match self {
            Self::Set(uid, shape) => ChangeHead::Set(*uid, shape.version),
            Self::Remove(uid) => ChangeHead::Remove(*uid),
        }
    }
}

/// A body change without its shape (mitcad#858): the body a result sets,
/// with the version of the shape it gives it, or removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeHead {
    Set(BodyUid, Version),
    Remove(BodyUid),
}

/// The body changes of a result ([`Output::changes`]). A result read from
/// the result store keeps its shapes packed (mitcad#858) until something
/// reads them: [`Changes::heads`], [`Changes::len`] and
/// [`Changes::is_empty`] tell what it changes without unpacking them;
/// the changes themselves (a slice, [`Changes::unpacked`]) unpack them
/// first, once, on whatever thread reads them first.
pub struct Changes<S> {
    heads: Vec<ChangeHead>,
    unpacked: OnceLock<Vec<Change<S>>>,
    packed: Mutex<Option<crate::store::Packed<S>>>,
}

impl<S> Changes<S> {
    pub fn new(changes: Vec<Change<S>>) -> Self {
        Self {
            heads: changes.iter().map(Change::head).collect(),
            unpacked: OnceLock::from(changes),
            packed: Mutex::new(None),
        }
    }

    /// Changes whose shapes `packed` makes when they are first read.
    pub(crate) fn packed(heads: Vec<ChangeHead>, packed: crate::store::Packed<S>) -> Self {
        Self {
            heads,
            unpacked: OnceLock::new(),
            packed: Mutex::new(Some(packed)),
        }
    }

    /// The bodies set, with their versions, and removed, in order.
    pub fn heads(&self) -> &[ChangeHead] {
        &self.heads
    }

    pub fn len(&self) -> usize {
        self.heads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heads.is_empty()
    }

    /// Whether the shapes are there (made by an evaluation, or unpacked).
    pub fn is_unpacked(&self) -> bool {
        self.unpacked.get().is_some()
    }

    /// The changes with their shapes, in the order of [`Changes::heads`];
    /// packed ones are unpacked first.
    pub fn unpacked(&self) -> &[Change<S>] {
        self.unpacked.get_or_init(|| {
            let packed = self
                .packed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            packed.map_or_else(Vec::new, |packed| packed.unpack(&self.heads))
        })
    }

    /// Unpacks the shapes as [`Changes::unpacked`] does, each on a thread
    /// of its own (mitcad#887).
    pub(crate) fn unpack_in_parallel(&self)
    where
        S: Send,
    {
        self.unpacked.get_or_init(|| {
            let packed = self
                .packed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            packed.map_or_else(Vec::new, |packed| packed.unpack_in_parallel(&self.heads))
        });
    }

    /// The bytes of the packed shapes; 0 once they are unpacked.
    pub(crate) fn packed_bytes(&self) -> u64 {
        self.packed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map_or(0, crate::store::Packed::bytes)
    }
}

impl<S> Default for Changes<S> {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl<S> std::ops::Deref for Changes<S> {
    type Target = [Change<S>];

    fn deref(&self) -> &[Change<S>] {
        self.unpacked()
    }
}

impl<'a, S> IntoIterator for &'a Changes<S> {
    type Item = &'a Change<S>;
    type IntoIter = std::slice::Iter<'a, Change<S>>;

    fn into_iter(self) -> Self::IntoIter {
        self.unpacked().iter()
    }
}

impl<S> FromIterator<Change<S>> for Changes<S> {
    fn from_iter<I: IntoIterator<Item = Change<S>>>(changes: I) -> Self {
        Self::new(changes.into_iter().collect())
    }
}

impl<S: std::fmt::Debug> std::fmt::Debug for Changes<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.unpacked.get() {
            Some(changes) => std::fmt::Debug::fmt(changes, f),
            None => f.debug_tuple("Packed").field(&self.heads).finish(),
        }
    }
}

/// A mark a shared result can take later (atomic, so that results stay
/// `Sync` like the rest of the document).
#[derive(Debug, Default)]
pub struct Flag(AtomicBool);

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
pub struct Output<S> {
    pub sheet_rules: Option<Versioned<crate::features::SheetRulesDef<f64>>>,
    pub sheets: BTreeMap<BodyUid, crate::sheet::SheetChange>,
    pub sheet_footprint: Vec<BodyUid>,
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
    pub pattern_anchor: Option<crate::features::point_pattern::PatternAnchor>,
    /// Where a sweep's profile lies on its path (mitcad#795).
    pub sweep_place: Option<crate::features::sweep::SweepPlace>,
    /// What a move's motion was worked out from (mitcad#838).
    pub move_basis: Option<crate::features::moves::MotionBasis>,
    pub sketch: Option<Versioned<Arc<SketchOutput>>>,
    /// The bodies it sets and removes; packed when read from the result
    /// store (mitcad#858).
    pub changes: Changes<S>,
    pub owners: BTreeMap<BodyUid, ComponentUid>,
    pub cut_targets: Option<crate::features::extrude_scope::CutTargets>,
    pub tool: Option<S>,
    pub datum: Option<Versioned<Datum>>,
    /// Occurrences the feature moves (F6).
    pub placements: Vec<PlacementChange>,
    /// A derived part's mount frames (mitcad#375).
    pub mounts: Option<crate::features::derived_part::Mounts>,
    /// Reference parameters it measured (a bound length, mitcad#375).
    pub measured: Vec<(ParamId, f64)>,
    /// What it remembers of the pieces it split bodies into, when it used
    /// that memory, and the bodies that are other pieces than before, with
    /// their warnings (mitcad#408).
    pub split_pieces: Option<Arc<Vec<crate::features::PieceGroup>>>,
    pub changed_pieces: Vec<(BodyUid, String)>,
    /// A tapped hole's walls whose modelled thread has its crest off them,
    /// as they were (mitcad#436, mitcad#628).
    pub threaded_walls: Vec<crate::features::hole::ThreadedWall>,
    /// What the feature picked, where it was (a Fillet's or a Chamfer's
    /// edges as polylines, mitcad#957): the `feature_inputs` query.
    pub inputs: Option<crate::features::FeatureInputs>,
}

pub struct FeatureResult<S> {
    pub uid: FeatureUid,
    pub status: FeatureStatus,
    /// None for suppressed and rolled back features.
    pub output: Option<Arc<Output<S>>>,
    /// The output's warnings with those of applying it after the
    /// evaluation (a joint's geometry found under another name after a
    /// sketch change, mitcad#272), when there are such.
    pub applied_warnings: Option<Vec<String>>,
}

impl<S> FeatureResult<S> {
    /// The warnings of a feature that succeeded (P9); none otherwise.
    pub fn warnings(&self) -> &[String] {
        match &self.output {
            Some(output) if self.status.is_ok() => {
                self.applied_warnings.as_deref().unwrap_or(&output.warnings)
            }
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
pub enum Read {
    SheetRules(FeatureUid, Option<Version>),
    Sheet(BodyUid, Option<Version>),
    /// A parameter value (as bits; a text parameter's text as its hash,
    /// mitcad#664), None when missing.
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
    /// Assembly identities, availability, multiplicity and read-only state.
    Assembly(u128),
    /// The document's join tolerance (as bits), None without one
    /// (mitcad#298).
    JoinTolerance(Option<u64>),
    /// A library source of the design (mitcad#375), None when missing.
    Source(crate::build::SourceId, Option<Version>),
    /// What the feature remembers of the pieces it split bodies into
    /// (mitcad#408): the fingerprints the result fits, the memory it was
    /// evaluated with and the memory it leaves.
    Pieces(FeatureUid, Vec<u128>),
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
            Self::SheetRules(uid, version) => {
                env.sheet_rules.get(uid).map(|s| s.version) == *version
            }
            Self::Sheet(uid, version) => env.sheets.get(uid).map(|s| s.version) == *version,
            // A number's bits, a text's hash (mitcad#664).
            Self::Param(id, bits) => env.params.read_bits(*id) == *bits,
            Self::Sketch(uid, version) => env.sketches.get(uid).map(|s| s.version) == *version,
            Self::Body(uid, version) => env.bodies.get(uid).map(|b| b.version) == *version,
            Self::AllBodies(bodies) => env
                .bodies
                .iter()
                .map(|(uid, b)| (*uid, b.version))
                .eq(bodies.iter().copied()),
            Self::Output(uid, version) => {
                env.result_of(*uid)
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
            Self::Assembly(key) => crate::features::extrude_scope::assembly_key(env) == *key,
            Self::JoinTolerance(bits) => env.join_tolerance.map(f64::to_bits) == *bits,
            Self::Source(id, version) => env.sources.version(*id) == *version,
            Self::Pieces(uid, fits) => fits.contains(&crate::features::pieces::fingerprint(
                env.split_pieces.get(uid),
            )),
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
            Self::SheetRules(uid, v) => version(fingerprint.u8(10).u64(uid.0), *v),
            Self::Sheet(uid, v) => {
                body(fingerprint.u8(11), *uid);
                version(fingerprint, *v);
            }
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
            Self::Assembly(key) => {
                fingerprint.u8(9).u128(*key);
            }
            Self::JoinTolerance(bits) => {
                fingerprint.u8(12).option(*bits, |f, b| {
                    f.u64(b);
                });
            }
            Self::Source(id, v) => version(fingerprint.u8(13).u32(id.0), *v),
            Self::Pieces(uid, fits) => {
                fingerprint.u8(14).u64(uid.0).u64(fits.len() as u64);
                for fit in fits {
                    fingerprint.u128(*fit);
                }
            }
        }
    }
}

/// The fingerprint of a feature's definition (P7d), with parameters by
/// their names. A base feature's B-rep data is taken by its own
/// fingerprint, which the data keeps, so it is not compressed for this.
pub(crate) fn def_fingerprint(def: &FeatureDef, params: &Parameters) -> u128 {
    let mut fingerprint = Fingerprint::new("def");
    fingerprint.u32(crate::sheet::ALGORITHM_VERSION);
    fingerprint.u64(crate::sheet::TOLERANCE.to_bits());
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
    SheetRules,
    Sketch,
    /// The body of the i-th change.
    Body(usize),
    Datum,
}

/// The version of a part of the result `output`.
pub(crate) fn part_version(output: Version, part: Part) -> Version {
    let (block, index) = match part {
        Part::SheetRules => (4, 0),
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
pub struct CacheStats {
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
pub struct CacheUsage {
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
pub struct Cache<S> {
    entries: HashMap<FeatureUid, VecDeque<CacheEntry<S>>>,
    pub store: Option<ResultStore>,
    budget: Option<u64>,
    bytes: u64,
    clock: u64,
    pub stats: CacheStats,
    /// B-rep data of base features being added and the shapes it was
    /// written from (mitcad#244): the evaluation adopts the shape
    /// ([`Kernel::adopt_shape`]) instead of reading the data. Set only
    /// while a command that adds such features computes; not forked.
    pub(crate) written_from: Vec<(Brep, S)>,
    /// The feature the open command previews (mitcad#712): the recompute
    /// makes its [`Recomputed::preview_tool`]. Set only while a preview
    /// computes; not forked, and not part of any cached result.
    pub(crate) preview: Option<FeatureUid>,
    /// Threads that evaluate the features of independent components ahead
    /// (mitcad#842, `recompute/ahead.rs`); 1: none, every feature is
    /// evaluated on the recompute's own thread. Not forked: a fork's
    /// recomputes use none.
    pub(crate) workers: usize,
    /// Where the next recompute that goes through the timeline from its
    /// start puts the bodies no later feature changes (mitcad#881,
    /// `recompute/final_bodies.rs`); taken by the next recompute, which
    /// closes it. Not forked.
    pub(crate) final_bodies: Option<FinalBodies<S>>,
}

/// Results kept per feature: enough for a few undo steps and value changes.
const ENTRIES_PER_FEATURE: usize = 8;

/// The memory a packed shape takes once unpacked, estimated as its packed
/// (compressed) bytes times this (mitcad#858).
const PACKED_MEMORY_FACTOR: u64 = 8;

impl<S> Default for Cache<S> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            store: None,
            budget: None,
            bytes: 0,
            clock: 0,
            stats: CacheStats::default(),
            written_from: Vec::new(),
            preview: None,
            workers: ahead::default_workers(),
            final_bodies: None,
        }
    }
}

/// The memory a result takes, estimated: its shapes as the kernel
/// estimates them ([`Kernel::shape_memory`]; parts shared with other
/// results are counted with each), a sketch's regions and what it read.
fn output_bytes<K: Kernel>(kernel: &K, output: &Output<K::Shape>) -> u64 {
    let mut bytes = 256 + 48 * output.reads.len() as u64;
    bytes += serde_json::to_vec(&output.sheets).map_or(0, |v| v.len() as u64);
    bytes += 24 * output.sheet_footprint.len() as u64;
    if output.changes.is_unpacked() {
        for change in output.changes.unpacked() {
            if let Change::Set(_, shape) = change {
                bytes += kernel.shape_memory(&shape.value);
            }
        }
    } else {
        // Shapes still packed (mitcad#858): what they will take once
        // unpacked, by their packed bytes, without unpacking them.
        bytes += PACKED_MEMORY_FACTOR * output.changes.packed_bytes();
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
            written_from: Vec::new(),
            preview: None,
            workers: 1,
            final_bodies: None,
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
            sheets: self.sheets.clone(),
            sheet_rules: self.sheet_rules.clone(),
            results: self
                .results
                .iter()
                .map(|r| FeatureResult {
                    uid: r.uid,
                    status: r.status.clone(),
                    output: r.output.clone(),
                    applied_warnings: r.applied_warnings.clone(),
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
            measured: self.measured.clone(),
            split_pieces: self.split_pieces.clone(),
            changed_pieces: self.changed_pieces.clone(),
            gone_pieces: self.gone_pieces.clone(),
            preview_tool: self.preview_tool.clone(),
            resume: None,
        }
    }

    /// The start of a recompute that goes on after this one's features
    /// ([`recompute_after`]): its results so far, without what the
    /// recompute itself did (evaluated, restored, measured), what it makes
    /// at its end (the bodies, the joints' frames) and its preview's tool.
    fn continued(&self) -> Self {
        Self {
            sheets: self.sheets.clone(),
            sheet_rules: self.sheet_rules.clone(),
            results: self
                .results
                .iter()
                .map(|r| FeatureResult {
                    uid: r.uid,
                    status: r.status.clone(),
                    output: r.output.clone(),
                    applied_warnings: r.applied_warnings.clone(),
                })
                .collect(),
            owners: self.owners.clone(),
            sketches: self.sketches.clone(),
            datums: self.datums.clone(),
            placements: self.placements.clone(),
            split_pieces: self.split_pieces.clone(),
            changed_pieces: self.changed_pieces.clone(),
            gone_pieces: self.gone_pieces.clone(),
            ..Self::default()
        }
    }
}

/// Whether a feature is evaluated anew by every recompute. These cheap
/// evaluations bind definition-state occurrence IDs. An edit can allocate
/// new IDs for an earlier definition, so geometry/placement reads alone
/// are not a sufficient cache key.
fn never_cached(def: &FeatureDef) -> bool {
    matches!(def, FeatureDef::PatternAtPoints(p)
        if matches!(p.objects, crate::features::PatternObjects::Components { .. }))
}

impl<S> Cache<S> {
    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    fn lookup(&mut self, entry: &FeatureEntry, env: &Env<'_, S>) -> Option<Arc<Output<S>>> {
        self.stats.lookups += 1;
        if never_cached(&entry.def) {
            return None;
        }
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
    /// cache too; `before_insert` sees it before it goes in. With an
    /// unpacker, the shapes of the bodies it sets stay packed (mitcad#858).
    fn restore<K: Kernel<Shape = S>>(
        &mut self,
        kernel: &K,
        entry: &FeatureEntry,
        def_fp: u128,
        env: &Env<'_, S>,
        unpacker: Option<&ShapeUnpacker<S>>,
        before_insert: impl FnOnce(&Self, &Output<S>),
    ) -> Option<Arc<Output<S>>> {
        let output = Arc::new(
            self.store
                .as_ref()?
                .find(kernel, entry, def_fp, env, unpacker)?,
        );
        before_insert(self, &output);
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

    /// Whether a result of `bytes` going in would make the cache drop
    /// results for its budget ([`Cache::trim`]).
    fn would_trim(&self, bytes: u64) -> bool {
        self.budget
            .is_some_and(|budget| self.bytes + bytes > budget)
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

    /// The definition a feature's result was evaluated from, while the
    /// cache keeps it (mitcad#679: the `sketch` query shows a sketch's
    /// evaluated solve while its definition is the same).
    pub(crate) fn definition_of(
        &self,
        uid: FeatureUid,
        output: &Arc<Output<S>>,
    ) -> Option<&FeatureDef> {
        self.entries
            .get(&uid)?
            .iter()
            .find(|e| Arc::ptr_eq(&e.output, output))
            .map(|e| &e.def)
    }
}

/// The evaluated timeline.
pub struct Recomputed<S> {
    pub sheets: crate::sheet::SheetStates,
    pub sheet_rules: crate::sheet::SheetRules,
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
    /// The values driven dimensions measured where they differ from their
    /// reference parameters' (mitcad#231): later features used them, and
    /// the document takes them.
    pub measured: Vec<(ParamId, f64)>,
    /// What the features that used it remember of the pieces they split
    /// bodies into (mitcad#408): the document takes it.
    pub split_pieces: crate::features::SplitPieces,
    /// Bodies that are other pieces of a split than before, with the
    /// warning of the features that name them, and pieces no longer made,
    /// with the feature that made them, at the marker.
    pub changed_pieces: BTreeMap<BodyUid, String>,
    pub gone_pieces: BTreeMap<BodyUid, FeatureUid>,
    /// What the open command's preview shows of the previewed feature
    /// besides its result ([`crate::features::Evaluate::preview_tool`],
    /// mitcad#712): made by a recompute for [`Cache::preview`] only, never
    /// cached.
    pub preview_tool: Option<S>,
    /// What a later recompute needs to go on after this one's features
    /// (mitcad#841, [`recompute_after`]); None when it cannot.
    pub(crate) resume: Option<ResumePoint<S>>,
}

/// Where a recompute ended, for one that goes on after its features
/// ([`recompute_after`]): the timeline it evaluated, which every feature
/// before the marker was, the assembly its features were placed in, each
/// component's bodies at its end, and the parameters its driven dimensions
/// and bound lengths measure, with their values (bits) at its end.
pub(crate) struct ResumePoint<S> {
    features: Vec<Arc<FeatureEntry>>,
    assembly: crate::assembly::Assembly,
    components: BTreeMap<ComponentUid, Arc<BodyState<S>>>,
    measured: Vec<(ParamId, Option<u64>)>,
}

impl<S> Default for Recomputed<S> {
    fn default() -> Self {
        Self {
            sheets: BTreeMap::new(),
            sheet_rules: BTreeMap::new(),
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
            measured: Vec::new(),
            split_pieces: BTreeMap::new(),
            changed_pieces: BTreeMap::new(),
            gone_pieces: BTreeMap::new(),
            preview_tool: None,
            resume: None,
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
    recompute_after(kernel, state, cache, monitor, None)
}

/// Like [`recompute`]; `before` is the result of an earlier state. When
/// `state` begins with the features `before` evaluated, the same entries
/// (an import's batches appended at the end, mitcad#841), and nothing they
/// read of the state changed ([`resume_point`]), their results are taken
/// from `before` and only the features after them are looked up and
/// evaluated: the result is the one a recompute from the start makes,
/// which would find each of them in the cache, without going through
/// them. Otherwise the timeline is evaluated from the start.
///
/// With workers (`Cache::workers` above one) the features of
/// independent components are evaluated ahead on threads of their own and
/// taken in timeline order (`recompute/ahead.rs`, mitcad#842): the result,
/// the cache and what the monitor is told are those without workers.
pub(crate) fn recompute_after<K: Kernel>(
    kernel: &K,
    state: &DocState,
    cache: &mut Cache<K::Shape>,
    monitor: Option<&Arc<RecomputeMonitor>>,
    before: Option<&Recomputed<K::Shape>>,
) -> Result<Recomputed<K::Shape>, Cancelled> {
    let total = state.marker.min(state.features.len());
    // (A copy: the evaluations below borrow it while the cache changes.)
    let written_from = cache.written_from.clone();
    let resumed = match before {
        Some(before) if cache.preview.is_none() => {
            resume_point(before, state).map(|point| (before, point))
        }
        _ => None,
    };
    let params = Cow::Borrowed(&state.parameters);
    // Where the features to go through begin.
    let first;
    let mut walk = match resumed {
        Some((before, point)) => {
            first = point.features.len();
            Walk {
                done: before.continued(),
                components: point.components.clone(),
                params,
                measured_ids: point.measured.iter().map(|(id, _)| *id).collect(),
                touched: Vec::new(),
                not_resumable: false,
                unpacker: None,
                pending: BTreeMap::new(),
                planner: None,
            }
        }
        None => {
            first = 0;
            let placements = state
                .assembly
                .occurrences
                .iter()
                .filter(|o| o.pattern.is_none())
                .map(|o| (o.uid, o.transform))
                .collect();
            Walk {
                done: Recomputed {
                    placements,
                    ..Recomputed::default()
                },
                components: BTreeMap::new(),
                params,
                measured_ids: Vec::new(),
                touched: Vec::new(),
                not_resumable: false,
                unpacker: None,
                pending: BTreeMap::new(),
                planner: None,
            }
        }
    };
    let run = Run {
        kernel,
        state,
        monitor,
        written_from: &written_from,
        total,
    };
    // Workers evaluate ahead in a plain recompute only: not for a preview
    // (its tool is made beside the result), not while base features adopt
    // the shapes they were written from (those are the recompute's
    // kernel's), and not on another recompute's worker.
    let workers = cache.workers > 1
        && cache.preview.is_none()
        && written_from.is_empty()
        && !ahead::on_worker();
    // Results restored from the store keep the shapes of the bodies they
    // set packed until something may read them (mitcad#858; the module
    // comment): not with workers, which read the body states on their own
    // threads, nor for a preview.
    if cache.workers <= 1
        && cache.preview.is_none()
        && cache.store.is_some()
        && crate::store::lazy_restore()
    {
        walk.unpacker = kernel.shape_unpacker().ok();
    }
    // The bodies no later feature changes, into the feed the cache was
    // given (mitcad#881): only a recompute through the whole timeline
    // from its start, not a preview's. Closed when the walk goes, however
    // the recompute ends.
    if let Some(feed) = cache.final_bodies.take() {
        if resumed.is_none() && cache.preview.is_none() {
            walk.planner = Some(final_bodies::Planner::new(state, total, feed));
        } else {
            feed.close();
        }
    }
    let count = state.features.len();
    let mut position = first;
    while position < count {
        if workers && position < total {
            let window = ahead::plan(state, cache, &walk, position, total);
            if window.parallel() && ahead::run_window(&run, cache, &mut walk, position, &window)? {
                position = window.end;
                continue;
            }
            // Nothing to evaluate side by side: the window's features, or
            // the barrier that ends it, one after another.
            let end = window.end.max(position + 1);
            for at in position..end {
                step(&run, cache, &mut walk, at, None)?;
            }
            position = end;
            continue;
        }
        step(&run, cache, &mut walk, position, None)?;
        position += 1;
    }
    // The bodies left are the result's: their shapes unpacked.
    walk.unpack_pending();
    let Walk {
        mut done,
        components,
        params,
        mut measured_ids,
        touched,
        not_resumable,
        unpacker: _,
        pending: _,
        planner: _,
    } = walk;
    let all = match resumed {
        // The earlier result's bodies with those the features after it
        // set or removed: a body is in the component that owns it.
        Some((before, _)) => {
            let mut all = (*before.bodies).clone();
            for uid in touched {
                let body = done
                    .owners
                    .get(&uid)
                    .and_then(|owner| components.get(owner))
                    .and_then(|bodies| bodies.get(&uid));
                match body {
                    Some(body) => {
                        all.insert(uid, body.clone());
                    }
                    None => {
                        all.remove(&uid);
                    }
                }
            }
            all
        }
        None => {
            let mut all = BTreeMap::new();
            for bodies in components.values() {
                all.extend(bodies.iter().map(|(uid, b)| (*uid, b.clone())));
            }
            all
        }
    };
    done.bodies = Arc::new(all);
    // The joints' frames and values where the occurrences are at the
    // marker.
    crate::joints::finish(&mut done.joints, &done.placements);
    if let Some(monitor) = monitor {
        monitor.finish(total);
    }
    // (Not after a preview's recompute, nor with features after the
    // marker: a later recompute could not take this one's results as
    // those of its first features.)
    if !not_resumable && cache.preview.is_none() && state.marker >= state.features.len() {
        measured_ids.sort_unstable();
        measured_ids.dedup();
        done.resume = Some(ResumePoint {
            features: state.features.clone(),
            assembly: state.assembly.clone(),
            components,
            measured: measured_ids
                .into_iter()
                .map(|id| (id, params.read_bits(id)))
                .collect(),
        });
    }
    Ok(done)
}

/// The bodies whose states hold a stand-in, with the restored results
/// that set them (mitcad#858).
pub(crate) type Pending<S> = BTreeMap<BodyUid, Arc<Output<S>>>;

/// The version the result gives the body `uid` (its last change of it),
/// read without unpacking it.
fn set_version<S>(output: &Output<S>, uid: BodyUid) -> Option<Version> {
    output
        .changes
        .heads()
        .iter()
        .rev()
        .find_map(|head| match *head {
            ChangeHead::Set(set, version) if set == uid => Some(version),
            _ => None,
        })
}

/// Unpacks the shapes of the bodies `read` (each with the version its
/// state holds) whose states hold a stand-in: the restored results that
/// set them, each once, on the machine's threads (mitcad#886; a result's
/// several shapes too, mitcad#887). Whether any result was unpacked.
pub(crate) fn unpack_read<S: Send + Sync>(
    pending: &Pending<S>,
    read: impl IntoIterator<Item = (BodyUid, Version)>,
) -> bool {
    let mut packed: Vec<&Changes<S>> = Vec::new();
    let mut seen = HashSet::new();
    for (uid, version) in read {
        if let Some(output) = pending.get(&uid)
            && !output.changes.is_unpacked()
            && set_version(output, uid) == Some(version)
            && seen.insert(Arc::as_ptr(output))
        {
            packed.push(&output.changes);
        }
    }
    if packed.is_empty() {
        return false;
    }
    crate::store::in_parallel(&packed, |changes| changes.unpack_in_parallel());
    true
}

/// The shape of the body `uid` whose state holds `held`: where that is a
/// restored result's stand-in, the shape unpacked from the result
/// (mitcad#886; unpacked here if it was not yet).
pub(crate) fn held_shape<'a, S>(
    pending: &'a Pending<S>,
    uid: BodyUid,
    held: &'a Versioned<S>,
) -> &'a Versioned<S> {
    pending
        .get(&uid)
        .filter(|output| set_version(output, uid) == Some(held.version))
        .and_then(|output| {
            output
                .changes
                .unpacked()
                .iter()
                .rev()
                .find_map(|change| match change {
                    Change::Set(set, shape) if *set == uid => Some(shape),
                    _ => None,
                })
        })
        .unwrap_or(held)
}

/// What every feature of one recompute is evaluated with.
struct Run<'a, K: Kernel> {
    kernel: &'a K,
    state: &'a DocState,
    monitor: Option<&'a Arc<RecomputeMonitor>>,
    /// B-rep data of base features being added and the shapes it was
    /// written from (`Cache::written_from`).
    written_from: &'a [(Brep, K::Shape)],
    /// The features before the marker.
    total: usize,
}

/// What a recompute has made so far as it goes through the timeline. A
/// worker that evaluates features ahead goes on from a copy of its own
/// (`recompute/ahead.rs`).
struct Walk<'s, S> {
    done: Recomputed<S>,
    /// Each component's bodies; a feature sees only its own component's.
    components: BTreeMap<ComponentUid, Arc<BodyState<S>>>,
    /// The parameters with the values driven dimensions measured so far
    /// (mitcad#231). (Where a recompute goes on after an earlier one, the
    /// state holds what the earlier one measured: `resume_point`.)
    params: Cow<'s, Parameters>,
    /// The parameters driven dimensions and bound lengths measure.
    measured_ids: Vec<ParamId>,
    /// The bodies the features gone through set or removed, when the
    /// bodies at the end are the earlier result's with these changed.
    touched: Vec<BodyUid>,
    /// Whether a feature gone through keeps a later recompute from going
    /// on after this one: a joint (applied after its evaluation, on what
    /// the whole state holds) or a feature evaluated anew every time.
    not_resumable: bool,
    /// Results from the result store keep their bodies' shapes packed
    /// (mitcad#858): how they are unpacked, and what stands in the body
    /// states meanwhile; None: they are read whole.
    unpacker: Option<ShapeUnpacker<S>>,
    /// The bodies whose states hold a stand-in, with the result that sets
    /// them.
    pending: BTreeMap<BodyUid, Arc<Output<S>>>,
    /// Puts the bodies no later feature changes into the feed the cache
    /// was given (mitcad#881); None without one, and on a worker's copy.
    planner: Option<final_bodies::Planner<S>>,
}

/// Where a feature's result came from, for the recompute's lists.
enum Came {
    Cache,
    /// The result store, read in this long.
    Store(Duration),
    /// An evaluation that took this long.
    Evaluated(Duration),
}

/// Goes through the feature at `position`: takes its result from the
/// cache, from the result store or from a worker that evaluated it ahead
/// (`ahead`, mitcad#842), or evaluates it, and applies the result.
fn step<K: Kernel>(
    run: &Run<'_, K>,
    cache: &mut Cache<K::Shape>,
    walk: &mut Walk<'_, K::Shape>,
    position: usize,
    mut ahead: Option<&mut ahead::Ahead<'_, '_, K::Shape>>,
) -> Result<(), Cancelled> {
    let (kernel, state, monitor) = (run.kernel, run.state, run.monitor);
    let entry = &state.features[position];
    if position < run.total
        && let Some(monitor) = monitor
    {
        monitor.enter(position, run.total)?;
    }
    let status = if position >= state.marker {
        Some(FeatureStatus::RolledBack)
    } else if entry.suppressed {
        Some(FeatureStatus::Suppressed)
    } else {
        None
    };
    if let Some(status) = status {
        walk.done.results.push(FeatureResult {
            uid: entry.uid,
            status,
            output: None,
            applied_warnings: None,
        });
        return Ok(());
    }
    if crate::joints::applies(&entry.def) || never_cached(&entry.def) {
        walk.not_resumable = true;
    }
    if let FeatureDef::Sketch(def) = &entry.def {
        walk.measured_ids
            .extend(def.dimensions.iter().filter_map(|d| d.reference));
    }

    // (The component's bodies are let go once the output is there: a
    // second holder of them would make each body the feature sets copy
    // every body of the component, quadratic in its bodies, mitcad#413.)
    let own = walk
        .components
        .get(&entry.component)
        .cloned()
        .unwrap_or_default();
    // The result from the cache or the result store; else the definition's
    // fingerprint, to evaluate it with.
    let found = {
        let env = walk.env(state, &own, run.written_from);
        match cache.lookup(entry, &env) {
            Some(output) => Ok((output, Came::Cache)),
            None => {
                let def_fp = def_fingerprint(&entry.def, env.params);
                let start = Instant::now();
                // (Workers let go of the results they hold before the cache
                // drops any for its budget, mitcad#842.)
                let restored = cache.restore(
                    kernel,
                    entry,
                    def_fp,
                    &env,
                    walk.unpacker.as_ref(),
                    |cache, output| {
                        if let Some(ahead) = ahead.as_deref_mut()
                            && cache.would_trim(output_bytes(kernel, output))
                        {
                            ahead.release();
                        }
                    },
                );
                match restored {
                    Some(output) => {
                        // A test's delay holds a restored result too, so a
                        // design whose results all come from the store still
                        // shows its progress (mitcad#887).
                        if let Some(monitor) = monitor {
                            monitor.test_pause();
                        }
                        Ok((output, Came::Store(start.elapsed())))
                    }
                    None => Err(def_fp),
                }
            }
        }
    };
    // (An evaluation unpacks what it reads of bodies whose restored shapes
    // are still packed as it reads them, mitcad#886: `EvalContext::body`.)
    let env = walk.env(state, &own, run.written_from);
    let (output, came) = match found {
        Ok(found) => found,
        Err(def_fp) => {
            if let Some(monitor) = monitor {
                monitor.evaluating(&entry.name);
            }
            // A worker's result where one evaluated the feature ahead
            // and what it read holds here (mitcad#842).
            let from_worker = match ahead.as_deref_mut() {
                Some(ahead) => ahead.take(position, def_fp, &env, monitor)?,
                None => None,
            };
            let output = match (from_worker, monitor) {
                (Some(output), _) => output,
                (None, Some(monitor)) => {
                    kernel.interruptible(monitor, || evaluate(kernel, entry, env, def_fp))
                }
                (None, None) => evaluate(kernel, entry, env, def_fp),
            };
            // A result that came back after a cancel request is not
            // kept: the request may have cut it short (P7), or the
            // kernel stopped inside and failed (P7e). One that ran
            // out of memory says so all the same: the request may
            // have come for that very failure (the .f3d import's
            // memory guard counts failed allocations, mitcad#132).
            let out_of_memory =
                matches!(&output.result, Err(m) if KernelError::is_out_of_memory(m));
            if let Some(monitor) = monitor
                && !out_of_memory
            {
                monitor.evaluation_done()?;
            }
            // Nor is a failure for lack of memory: the feature may
            // build once memory is free again (mitcad#80).
            if !out_of_memory {
                if let Some(ahead) = &mut ahead
                    && cache.would_trim(output_bytes(kernel, &output))
                {
                    ahead.release();
                }
                cache.insert(kernel, entry, output.clone());
            }
            if let Some(monitor) = monitor {
                monitor.kept();
            }
            let time = output.time;
            (output, Came::Evaluated(time))
        }
    };
    // What the open command's preview shows of its feature besides the
    // result (mitcad#712): made for the preview only, against what the
    // feature saw, whether its result was cached or not, and kept out
    // of the cache.
    let preview_tool = (cache.preview == Some(entry.uid) && output.result.is_ok()).then(|| {
        let mut ctx = EvalContext::new(kernel, entry.uid, env);
        entry.def.evaluator::<K>().preview_tool(&mut ctx)
    });
    drop(own);
    if let Some(tool) = preview_tool {
        walk.done.preview_tool = tool;
    }
    // The bodies whose shapes the evaluation unpacked hold them from here
    // on (mitcad#886); the others stay packed.
    if matches!(came, Came::Evaluated(_)) && !walk.pending.is_empty() {
        walk.settle_unpacked();
    }
    match came {
        Came::Cache => {}
        Came::Store(time) => {
            walk.done.restore_times.push(time);
            walk.done.restored.push(entry.uid);
        }
        Came::Evaluated(time) => {
            walk.done.times.push(time);
            walk.done.evaluated.push(entry.uid);
        }
    }
    let applied = walk.planner.is_some().then(|| output.clone());
    walk.apply(kernel, state, entry, output);
    // The bodies final from here on (mitcad#881).
    if let (Some(planner), Some(output)) = (walk.planner.as_mut(), applied) {
        planner.after(
            position,
            &output,
            &walk.components,
            &walk.done.owners,
            &walk.pending,
        );
    }
    Ok(())
}

impl<'s, S> Walk<'s, S> {
    /// What the feature evaluated next reads: `own`, its component's
    /// bodies, and what the timeline before it made.
    fn env<'a>(
        &'a self,
        state: &'a DocState,
        own: &'a BodyState<S>,
        written_from: &'a [(Brep, S)],
    ) -> Env<'a, S> {
        Env {
            sheets: &self.done.sheets,
            sheet_rules: &self.done.sheet_rules,
            params: &self.params,
            bodies: own,
            pending: &self.pending,
            owners: &self.done.owners,
            assembly: &state.assembly,
            sketches: &self.done.sketches,
            datums: &self.done.datums,
            features: &state.features,
            index: &state.feature_index,
            history: &self.done.results,
            components: &self.components,
            placements: &self.done.placements,
            written_from,
            join_tolerance: state.join_tolerance,
            sources: &state.library_sources,
            split_pieces: &state.split_pieces,
            changed_pieces: &self.done.changed_pieces,
            gone_pieces: &self.done.gone_pieces,
        }
    }
}

impl<'s, S: Clone + Send + Sync> Walk<'s, S> {
    /// A copy to go on from on another thread (`recompute/ahead.rs`): what
    /// was made so far, shared, without what this walk did itself.
    fn snapshot(&self) -> Walk<'s, S> {
        Walk {
            done: self.done.fork(),
            components: self.components.clone(),
            params: self.params.clone(),
            measured_ids: Vec::new(),
            touched: Vec::new(),
            not_resumable: false,
            unpacker: None,
            pending: BTreeMap::new(),
            planner: None,
        }
    }

    /// Gives the bodies whose states hold a stand-in their shapes, unpacked
    /// from the results that set them (mitcad#858).
    fn unpack_pending(&mut self) {
        // The results' shapes first, on the machine's threads (mitcad#887).
        let read: Vec<(BodyUid, Version)> = self
            .pending
            .iter()
            .filter_map(|(uid, output)| Some((*uid, set_version(output, *uid)?)))
            .collect();
        unpack_read(&self.pending, read);
        self.settle(|_| true);
    }

    /// Gives the bodies whose results' shapes were unpacked meanwhile (an
    /// evaluation read them, mitcad#886) their shapes; the others keep
    /// their stand-ins.
    fn settle_unpacked(&mut self) {
        self.settle(|output| output.changes.is_unpacked());
    }

    /// Gives the bodies of the results `which` picks their shapes (those
    /// still packed unpacked here), and no longer pending.
    fn settle(&mut self, which: impl Fn(&Output<S>) -> bool) {
        let settled: Vec<BodyUid> = self
            .pending
            .iter()
            .filter(|(_, output)| which(output))
            .map(|(uid, _)| *uid)
            .collect();
        for uid in settled {
            let Some(output) = self.pending.remove(&uid) else {
                continue;
            };
            let shape = output
                .changes
                .unpacked()
                .iter()
                .rev()
                .find_map(|change| match change {
                    Change::Set(set, shape) if *set == uid => Some(shape),
                    _ => None,
                });
            let (Some(shape), Some(owner)) = (shape, self.done.owners.get(&uid).copied()) else {
                continue;
            };
            if let Some(bodies) = self.components.get_mut(&owner)
                && bodies
                    .get(&uid)
                    .is_some_and(|body| body.version == shape.version)
            {
                Arc::make_mut(bodies).insert(uid, shape.clone());
            }
        }
    }

    /// Applies the result of the feature `entry` where the timeline is:
    /// its placements, joints, sketch, datum, measured values and body
    /// changes, and its status.
    fn apply<K: Kernel<Shape = S>>(
        &mut self,
        kernel: &K,
        state: &DocState,
        entry: &FeatureEntry,
        output: Arc<Output<S>>,
    ) {
        // Joints read the bodies of every component (mitcad#858).
        if crate::joints::applies(&entry.def) {
            self.unpack_pending();
        }
        let assembly = &state.assembly;
        let done = &mut self.done;
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
        let mut applied_warnings = None;
        if status.is_ok() && crate::joints::applies(&entry.def) {
            match crate::joints::apply(kernel, state, entry, &self.components, done) {
                Ok(warnings) if !warnings.is_empty() => {
                    let mut all = output.warnings.clone();
                    for warning in warnings {
                        if !all.contains(&warning) {
                            all.push(warning);
                        }
                    }
                    applied_warnings = Some(all);
                }
                Ok(_) => {}
                Err(message) => status = FeatureStatus::Failed(message),
            }
        }
        if status.is_ok() || output.placements.is_empty() {
            if status.is_ok()
                && let Some(rules) = &output.sheet_rules
            {
                done.sheet_rules.insert(entry.uid, rules.clone());
            }
            if let Some(sketch) = &output.sketch {
                done.sketches.insert(entry.uid, sketch.clone());
                measure_references(entry, &sketch.value, &mut self.params, &mut done.measured);
            }
            if let Some(datum) = &output.datum {
                done.datums.insert(entry.uid, datum.clone());
            }
            // A bound length's measured value (mitcad#375) sizes what follows.
            self.measured_ids
                .extend(output.measured.iter().map(|(id, _)| *id));
            if status.is_ok() {
                for (id, value) in &output.measured {
                    if self.params.value(*id).map(f64::to_bits) != Some(value.to_bits())
                        && self.params.to_mut().set_measured(*id, *value).is_ok()
                    {
                        done.measured.push((*id, *value));
                    }
                }
            }
            // Bodies the feature sets go to the component it made, if any.
            // A result restored with its shapes packed gives them a
            // stand-in until something may read them (mitcad#858).
            let target = assembly.created_by(entry.uid).unwrap_or(entry.component);
            let stand_in = match &self.unpacker {
                Some(unpacker) if !output.changes.is_unpacked() => Some(unpacker.stand_in),
                _ => None,
            };
            for (i, head) in output.changes.heads().iter().enumerate() {
                match *head {
                    ChangeHead::Set(uid, version) => {
                        self.touched.push(uid);
                        let target = output.owners.get(&uid).copied().unwrap_or(target);
                        crate::sheet::publish(
                            &mut done.sheets,
                            uid,
                            target,
                            version,
                            entry.uid,
                            output.sheets.get(&uid),
                        );
                        if let Some(old) = done.owners.insert(uid, target)
                            && old != target
                            && let Some(bodies) = self.components.get_mut(&old)
                        {
                            Arc::make_mut(bodies).remove(&uid);
                        }
                        let shape = match stand_in {
                            Some(stand_in) => {
                                self.pending.insert(uid, output.clone());
                                Versioned {
                                    version,
                                    value: stand_in(),
                                }
                            }
                            None => {
                                self.pending.remove(&uid);
                                match output.changes.unpacked().get(i) {
                                    Some(Change::Set(_, shape)) => shape.clone(),
                                    _ => continue,
                                }
                            }
                        };
                        Arc::make_mut(self.components.entry(target).or_default())
                            .insert(uid, shape);
                    }
                    ChangeHead::Remove(uid) => {
                        self.touched.push(uid);
                        self.pending.remove(&uid);
                        done.sheets.remove(&uid);
                        if let Some(old) = done.owners.remove(&uid)
                            && let Some(bodies) = self.components.get_mut(&old)
                        {
                            Arc::make_mut(bodies).remove(&uid);
                        }
                    }
                }
            }
            if status.is_ok() {
                follow_pieces(done, entry.uid, &output);
            }
        }
        if let FeatureStatus::Failed(message) = &status {
            crate::sheet::block(
                &mut done.sheets,
                &output.sheet_footprint,
                entry.uid,
                message,
            );
        }
        done.results.push(FeatureResult {
            uid: entry.uid,
            status,
            output: Some(output),
            applied_warnings,
        });
    }
}

/// The point after which a recompute of `state` can go on from `before`
/// ([`recompute_after`]), if it can: `state` begins with the features
/// `before` evaluated (the same entries), in the same assembly, and
/// nothing those features read of the state besides the timeline before
/// them changed: the values of the parameters they read or measure, the
/// join tolerance, the library sources and what they remember of the
/// pieces of their splits. A recompute from the start then finds each of
/// their results in the cache (their reads match, by induction over the
/// timeline) and does with them what the earlier recompute did.
fn resume_point<'b, S>(before: &'b Recomputed<S>, state: &DocState) -> Option<&'b ResumePoint<S>> {
    let point = before.resume.as_ref()?;
    let count = point.features.len();
    if before.results.len() != count || state.features.len() < count || state.marker < count {
        return None;
    }
    let same_features = point
        .features
        .iter()
        .zip(&state.features)
        .all(|(a, b)| Arc::ptr_eq(a, b));
    if !same_features || point.assembly != state.assembly {
        return None;
    }
    let params = &state.parameters;
    if point
        .measured
        .iter()
        .any(|(id, bits)| params.read_bits(*id) != *bits)
    {
        return None;
    }
    let reads_hold = before
        .results
        .iter()
        .filter_map(|r| r.output.as_ref())
        .flat_map(|output| &output.reads)
        .all(|read| match read {
            Read::Param(id, bits) => params.read_bits(*id) == *bits,
            Read::JoinTolerance(bits) => state.join_tolerance.map(f64::to_bits) == *bits,
            Read::Source(id, version) => state.library_sources.version(*id) == *version,
            Read::Pieces(uid, fits) => fits.contains(&crate::features::pieces::fingerprint(
                state.split_pieces.get(uid),
            )),
            // What earlier features made and the assembly: the same.
            _ => true,
        });
    reads_hold.then_some(point)
}

/// Keeps the pieces of splits up to date after a feature that succeeded
/// (mitcad#408): what it remembers, the bodies that are other pieces than
/// before (a body keeps that warning while it lasts, until a split gives it
/// a piece that matches) and the pieces no longer made.
fn follow_pieces<S>(done: &mut Recomputed<S>, feature: FeatureUid, output: &Output<S>) {
    let groups = output.split_pieces.as_deref().map_or(&[][..], |g| &g[..]);
    let matched = |body: &BodyUid| {
        groups
            .iter()
            .flat_map(|g| &g.pieces)
            .any(|p| p.body == *body && !p.gone)
    };
    let mut set = Vec::new();
    for change in output.changes.heads() {
        match change {
            ChangeHead::Set(uid, _) => {
                set.push(*uid);
                done.gone_pieces.remove(uid);
                match output.changed_pieces.iter().find(|(b, _)| b == uid) {
                    Some((_, message)) => {
                        done.changed_pieces.insert(*uid, message.clone());
                    }
                    None if matched(uid) => {
                        done.changed_pieces.remove(uid);
                    }
                    None => {}
                }
            }
            ChangeHead::Remove(uid) => {
                done.changed_pieces.remove(uid);
            }
        }
    }
    // A piece remembered as gone, or one of the feature's own the feature
    // did not make this time, is no longer made.
    for piece in groups.iter().flat_map(|g| &g.pieces) {
        if !set.contains(&piece.body) && (piece.gone || piece.body.feature == feature) {
            done.gone_pieces.insert(piece.body, feature);
        }
    }
    if let Some(groups) = &output.split_pieces {
        done.split_pieces.insert(feature, groups.clone());
    }
}

/// Sets the reference parameters of a sketch's driven dimensions to the
/// values the evaluated sketch measures (mitcad#231), so the features
/// after it use them; `measured` collects those that changed.
fn measure_references(
    entry: &FeatureEntry,
    sketch: &SketchOutput,
    params: &mut std::borrow::Cow<'_, Parameters>,
    measured: &mut Vec<(ParamId, f64)>,
) {
    let FeatureDef::Sketch(def) = &entry.def else {
        return;
    };
    for d in &def.dimensions {
        let (Some(id), Some(value)) = (d.reference, sketch.status().driven.get(&d.id)) else {
            continue;
        };
        if params.value(id).map(f64::to_bits) != Some(value.to_bits())
            && params.to_mut().set_measured(id, *value).is_ok()
        {
            measured.push((id, *value));
        }
    }
}

/// Applies a feature's placement changes: each occurrence must be placed
/// in the feature's component and not be grounded.
pub(crate) fn place(
    state: &DocState,
    entry: &FeatureEntry,
    changes: &[PlacementChange],
    placements: &mut BTreeMap<OccurrenceUid, Transform>,
) -> Result<(), String> {
    // (Most features move nothing: no copy of the placements for them.)
    if changes.is_empty() {
        return Ok(());
    }
    let assembly = &state.assembly;
    let mut next = placements.clone();
    for change in changes {
        let (PlacementChange::Move(uid, _) | PlacementChange::Set(uid, _)) = change;
        let Some(occurrence) = assembly.occurrence(*uid) else {
            return Err(format!("occurrence {uid} does not exist"));
        };
        let name = assembly.occurrence_name(*uid);
        if let Some(pattern) = &occurrence.pattern
            && pattern.feature != entry.uid
            && !next.contains_key(uid)
        {
            return Err(format!(
                "{name} is unavailable: its Pattern at Points feature has no active result"
            ));
        }
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
    crate::sheet::footprint(&entry.def, &mut ctx);
    let mut result = entry.def.evaluator::<K>().evaluate(&mut ctx);
    if let Ok(produced) = &mut result {
        crate::sheet::adapt(&mut ctx, produced);
        crate::sheet::complete_changes(&ctx, produced);
    }
    // (Not the restored shapes it unpacked as it read them, mitcad#886.)
    let time = start.elapsed().saturating_sub(ctx.unpack_time);
    // (Records the memory of pieces as read, mitcad#408.)
    let (split_pieces, changed_pieces) = ctx.finish_pieces();
    let reads = std::mem::take(&mut ctx.reads);
    let warnings = std::mem::take(&mut ctx.warnings);
    // Angles of more than one turn where one is the most (mitcad#714).
    let turns = turn_warnings(entry, params);
    let elements = std::mem::take(&mut ctx.elements);
    let version = output_version(entry.uid, def_fp, &reads, params);
    let output = match result {
        Ok(produced) => Output {
            sheet_rules: produced.sheet_rules.map(|value| Versioned {
                version: part_version(version, Part::SheetRules),
                value,
            }),
            sheets: produced.sheets,
            sheet_footprint: ctx.sheet_footprint.into_iter().collect(),
            version,
            def_fp,
            reads,
            time,
            persisted: Flag::new(false),
            result: Ok(()),
            warnings: with_turns(warnings, &turns),
            elements,
            pattern_anchor: ctx.pattern_anchor,
            sweep_place: ctx.sweep_place,
            move_basis: ctx.move_basis,
            owners: produced.owners,
            cut_targets: ctx.cut_targets,
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
            mounts: (!produced.mounts.is_empty()).then(|| std::sync::Arc::new(produced.mounts)),
            measured: produced.measured,
            split_pieces,
            changed_pieces,
            threaded_walls: produced.threaded_walls,
            inputs: produced.inputs,
        },
        Err(message) => Output {
            sheet_rules: None,
            sheets: BTreeMap::new(),
            sheet_footprint: ctx.sheet_footprint.into_iter().collect(),
            version,
            def_fp,
            reads,
            time,
            persisted: Flag::new(false),
            result: Err(failed_with_turns(message, &turns)),
            warnings: Vec::new(),
            elements: Vec::new(),
            pattern_anchor: None,
            // Known also when the sweep failed (mitcad#795): the panel
            // shows its distances for a path the extent does not fit.
            sweep_place: ctx.sweep_place,
            // Known also when the move failed (mitcad#854): a Move Face
            // whose faces cannot follow the angle (or one that moves
            // nothing) still turns its ring about the model's axis.
            move_basis: ctx.move_basis,
            owners: BTreeMap::new(),
            cut_targets: ctx.cut_targets,
            sketch: None,
            changes: Changes::default(),
            tool: None,
            datum: None,
            placements: Vec::new(),
            mounts: None,
            measured: Vec::new(),
            split_pieces: None,
            changed_pieces: Vec::new(),
            threaded_walls: Vec::new(),
            inputs: None,
        },
    };
    Arc::new(output)
}

/// What a feature's angles of more than one turn say, in slots where more
/// than one turn means nothing (mitcad#714, `parameters::one_turn_slot`):
/// a JSON number in a slot is radians, so `"angle": 90` meant as degrees
/// turns by 5156.6°. A value that is a short number in radians but not in
/// degrees came as such a number and says how to write degrees:
/// `Move3 angle: 90 is read as radians (5156.6°); write "90 deg" for
/// degrees`; any other says it is more than one turn.
fn turn_warnings(entry: &FeatureEntry, params: &Parameters) -> Vec<String> {
    let feature = entry.def.type_name();
    let mut found = Vec::new();
    let _ = entry.def.map_params(&mut |slot, id: &ParamId| {
        if mitcad_model_base::internal::parameters::one_turn_slot(feature, slot)
            && let Some(value) = params.value(*id)
            && value.abs() > std::f64::consts::TAU * (1.0 + 1e-9)
        {
            found.push(turn_warning(&entry.name, slot, value));
        }
        Ok::<_, ()>(())
    });
    found
}

fn turn_warning(feature: &str, slot: &str, radians: f64) -> String {
    let label = crate::features::slot_label(slot);
    let degrees = radians.to_degrees();
    let shown = format!("{degrees:.1}");
    let shown = shown.strip_suffix(".0").unwrap_or(&shown);
    if short_number(radians) && !short_number(degrees) {
        format!(
            "{feature} {label}: {radians} is read as radians ({shown}°); write \"{radians} deg\" for degrees"
        )
    } else {
        format!("{feature} {label}: {shown}° is more than one turn")
    }
}

/// At most six significant digits (`90`, `1.5`, `720`), as typed.
fn short_number(value: f64) -> bool {
    let text = value.abs().to_string();
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    digits.trim_start_matches('0').trim_end_matches('0').len() <= 6
}

fn with_turns(mut warnings: Vec<String>, turns: &[String]) -> Vec<String> {
    for turn in turns {
        if !warnings.contains(turn) {
            warnings.push(turn.clone());
        }
    }
    warnings
}

/// A failed feature's message with what its angles say (the warnings of
/// a feature that failed are not shown).
fn failed_with_turns(message: String, turns: &[String]) -> String {
    if turns.is_empty() {
        return message;
    }
    format!("{message}; {}", turns.join("; "))
}
