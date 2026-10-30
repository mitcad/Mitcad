// SPDX-License-Identifier: MIT
//! The result store (P7d): results of costly feature evaluations kept on
//! disk, so that opening a design again, or another version of it, does not
//! evaluate them again.
//!
//! A result is found by what the memory cache keys it by: the feature's
//! uid and definition (the file's name, a fingerprint of both) and what the
//! evaluation read (a variant in the file whose reads match the inputs the
//! feature sees now). Versions are fingerprints of the same
//! ([`crate::recompute`]), so a result read from the store has the version
//! it had when it was evaluated, and the features after it find theirs.
//! The path of a project does not enter the key: a copy or an older
//! version of a design finds the same results.
//!
//! Layout: `<dir>/v1/<k[0..2]>/<k>.mrs`, `k` the key in hex. A file:
//!
//! | Bytes | What |
//! |---|---|
//! | 8 | `MITCADRS` |
//! | 4 | format (1), little-endian like every number here |
//! | 8 | length of the payload |
//! | 16 | fingerprint of the payload (a damaged file is removed) |
//! | payload | the build id, a JSON header (feature, name, type, document), the variants |
//!
//! A variant (at most four per file, the newest first) is a JSON head
//! (reads, error, warnings, pattern elements, body changes, tool, the
//! evaluation's time, the walls a tapped hole's thread took whole) and its
//! shapes ([`Kernel::shape_bytes`],
//! zlib-compressed). Results stored by another build (`build_id`: the
//! kernel's format and the program, `geometry::kernel_build_id` in the
//! bridge) are not used, and are replaced when the feature is stored
//! again. Files are written to a temporary file and renamed, so programs
//! that share the folder see whole files. A hit touches the file's
//! modification time, and [`gc`] removes the files used longest ago.
//!
//! Not stored: sketches and construction features (cheap, and their
//! outputs have no stored form), features that move occurrences, failed
//! evaluations of a cancelled job (never cached), and what
//! [`Document::persist_results`] is told is too quick, unless restoring it
//! is cheaper than evaluating it again (below).
//!
//! **Base features** (mitcad#887) are stored like other features, also
//! those that only bring in their bodies, although their definitions hold
//! the bodies' B-rep data: evaluating one reads that data and then names
//! the faces and derives the edges' and vertices' names from them, which
//! a stored shape keeps. For a body of 23 000 faces, reading took
//! 0.3-0.5 s and naming 0.9-1.0 s (the edges' and vertices' names 0.7 s of
//! it), where restoring it takes 0.45 s (its file's shape mapped once,
//! `geometry::deserialize_shape`); with lazy restore (below) a base
//! feature's body that the next restored result replaces is not read at
//! all.
//!
//! **Quick results** (mitcad#882): a result whose evaluation took less
//! than `persist_results`'s `min_time` is stored too when its shapes are
//! small for its time: their memory (`Kernel::shape_memory`, the bodies it
//! sets and its tool) at most [`QUICK_MEMORY_PER_MS`] per millisecond the
//! evaluation took. Reading a result costs its file (about 0.6 MB a
//! millisecond) and, once something reads its bodies, unpacking them; but
//! a restored result also leaves the state before it packed, so in a run
//! of restored features only the last state is unpacked, where evaluating
//! the quick feature unpacks the state it reads. Measured on three large
//! imported designs (mitcad#882), the quick extrudes, fillets, chamfers,
//! revolves and patterns took at most 48 KiB per millisecond; storing
//! them took the warm recompute from 1.5 to 0.46 s, 3.5 to 3.2 s (its
//! sketches left) and 3.2 to 1.55 s, the store growing by 7.8, 0.2 and
//! 3.0 MB (30 %, 1 % and 111 %) and the first open's writing by under
//! 0.1 s. The limit keeps out results that are quick and large: moves of
//! a millisecond with 0.5-1 MB of shapes (their stored shapes take longer
//! to unpack than the move to evaluate), and copies of bodies into a
//! component, which would only grow the store.
//! `MITCAD_STORE_QUICK=0` stores only what took `min_time`
//! ([`ResultStore::with_quick_results`]).
//!
//! **Lazy restore** (mitcad#858): with a kernel that can unpack shapes
//! later ([`Kernel::shape_unpacker`]), a result found keeps the shapes of
//! the bodies it sets packed (`recompute::Changes`) until
//! something reads them; a state the next restored result replaces is
//! never unpacked (the recompute's module comment). The tool is read at
//! once. `MITCAD_LAZY_RESTORE=0` reads every result whole. The shapes a
//! recompute unpacks together (those an evaluation reads, mitcad#886,
//! before a joint, and the bodies left at its end) are unpacked on the machine's
//! threads, a result's several shapes too (mitcad#887): a base feature's
//! two large bodies take as long as the larger one.
//!
//! **Display meshes** (mitcad#858): the application keeps each shown
//! body's display triangulations ([`Kernel::display_mesh_bytes`]) under
//! the body's version and its meshing options, in
//! `<dir>/v1/<k[0..2]>/<k>.mms` beside the results: the same header with
//! `MITCADDM`, a payload of the build id, a JSON header (version, options,
//! document) and the zlib-compressed triangulations. Opening the design
//! again gives a body whose shape has that version its triangulations back
//! ([`ResultStore::restore_display_mesh`]) instead of meshing it; another
//! build or other options find none. [`gc`], [`clear`] and [`usage`] count
//! them with the results.
//!
//! [`Document::persist_results`]: crate::Document::persist_results

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use crate::features::{Env, FeatureDef, FeatureEntry};
use crate::fingerprint::Fingerprint;
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::{Kernel, KernelError, ShapeUnpacker};
use crate::parameters::Parameters;
use crate::recompute::{
    Change, ChangeHead, Changes, Flag, Output, Part, Read, Versioned, output_version, part_version,
};
use crate::transform::Transform;

const MAGIC: &[u8; 8] = b"MITCADRS";
const FORMAT: u32 = 1;
/// The folder of this layout under the store's directory.
const LAYOUT: &str = "v1";
const EXTENSION: &str = "mrs";
/// Display mesh files (mitcad#858).
const MESH_MAGIC: &[u8; 8] = b"MITCADDM";
const MESH_FORMAT: u32 = 1;
const MESH_EXTENSION: &str = "mms";
const HEADER: usize = 8 + 4 + 8 + 16;
/// Results kept per file: the same definition with other inputs.
const VARIANTS: usize = 4;
/// zlib level of stored shapes: fast, as storing is part of opening.
const LEVEL: u8 = 1;
/// [`gc`] removes the oldest files until this share of the budget is used.
const GC_TARGET: f64 = 0.8;
/// Temporary files older than this were left by a program that stopped.
const STALE_TEMPORARY: Duration = Duration::from_secs(3600);
/// A result quicker than `persist_results`'s minimum is stored when its
/// shapes take at most this much memory per millisecond of its evaluation
/// (mitcad#882; the module comment).
pub const QUICK_MEMORY_PER_MS: u64 = 128 * 1024;

/// A store folder, with the build results must have been stored by and the
/// document they are stored for (shown by [`usage`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultStore {
    dir: PathBuf,
    build_id: String,
    label: String,
    /// Quick results are stored with at most this much shape memory per
    /// millisecond of evaluation; None: not (mitcad#882).
    quick: Option<u64>,
}

/// What [`crate::Document::persist_results`] wrote.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PersistReport {
    /// Results written, and their shapes.
    pub results: usize,
    pub shapes: usize,
    /// Of the results, those quicker than the minimum, stored as cheaper
    /// to restore than to evaluate (mitcad#882).
    pub quick: usize,
    /// Bytes of the files written.
    pub bytes: u64,
    /// Results that could not be written (a shape the kernel could not
    /// write, a folder that cannot be written).
    pub failed: usize,
    pub errors: Vec<String>,
    pub time: Duration,
}

/// What [`gc`] or [`clear`] found and removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GcReport {
    /// Files kept and their bytes.
    pub files: u64,
    pub bytes: u64,
    pub removed: u64,
    pub removed_bytes: u64,
}

/// What this process did with result stores, for diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct StoreStats {
    /// Lookups of results the memory cache did not have.
    pub lookups: u64,
    pub hits: u64,
    /// Lookups that found no file, or none of its variants fit.
    pub misses: u64,
    /// Files removed as damaged, and files of another build passed over.
    pub damaged: u64,
    pub other_build: u64,
    /// Bytes read for hits and the time reading and rebuilding them took.
    pub read_bytes: u64,
    pub read_ns: u64,
    /// Results written, their bytes and the time.
    pub writes: u64,
    pub write_bytes: u64,
    pub write_ns: u64,
    pub write_errors: u64,
    /// Hits whose bodies' shapes stayed packed (mitcad#858), and of them
    /// those unpacked since, with their packed bytes and the time
    /// unpacking took (results unpacked at once on several threads,
    /// mitcad#887, each count their own).
    pub deferred: u64,
    pub unpacked: u64,
    pub unpack_bytes: u64,
    pub unpack_ns: u64,
    /// Display meshes (mitcad#858): bodies looked up and found, the faces
    /// they gave triangulations, the bytes read and the time; meshes
    /// written, their bytes and the time.
    pub mesh_lookups: u64,
    pub mesh_hits: u64,
    pub mesh_faces: u64,
    pub mesh_read_bytes: u64,
    pub mesh_read_ns: u64,
    pub mesh_writes: u64,
    pub mesh_write_bytes: u64,
    pub mesh_write_ns: u64,
}

macro_rules! counters {
    ($($field:ident),* $(,)?) => {
        struct Counters { $($field: AtomicU64,)* }
        static COUNTERS: Counters = Counters { $($field: AtomicU64::new(0),)* };
        /// The counters of every store this process used.
        pub fn stats() -> StoreStats {
            StoreStats { $($field: COUNTERS.$field.load(Ordering::Relaxed),)* }
        }
    };
}

counters!(
    lookups,
    hits,
    misses,
    damaged,
    other_build,
    read_bytes,
    read_ns,
    writes,
    write_bytes,
    write_ns,
    write_errors,
    deferred,
    unpacked,
    unpack_bytes,
    unpack_ns,
    mesh_lookups,
    mesh_hits,
    mesh_faces,
    mesh_read_bytes,
    mesh_read_ns,
    mesh_writes,
    mesh_write_bytes,
    mesh_write_ns,
);

fn count(counter: &AtomicU64, by: u64) {
    counter.fetch_add(by, Ordering::Relaxed);
}

fn nanos(time: Duration) -> u64 {
    u64::try_from(time.as_nanos()).unwrap_or(u64::MAX)
}

/// A version in files: 32 hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hex(u128);

impl Serialize for Hex {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&format_args!("{:032x}", self.0))
    }
}

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        u128::from_str_radix(&text, 16)
            .map(Hex)
            .map_err(serde::de::Error::custom)
    }
}

/// The feature a file is of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Meta {
    feature: FeatureUid,
    def: Hex,
    name: String,
    #[serde(rename = "type")]
    kind: String,
    /// The document it was stored for last.
    document: String,
}

/// A [`Read`] in files: a parameter by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredRead {
    SheetRules(FeatureUid, Option<Hex>),
    Sheet(BodyUid, Option<Hex>),
    Param(String, Option<u64>),
    Sketch(FeatureUid, Option<Hex>),
    Body(BodyUid, Option<Hex>),
    AllBodies(Vec<(BodyUid, Hex)>),
    Output(FeatureUid, Option<Hex>),
    Datum(FeatureUid, Option<Hex>),
    BodyIn(ComponentUid, BodyUid, Option<Hex>),
    BodiesIn(ComponentUid, Vec<(BodyUid, Hex)>),
    Placement(OccurrenceUid, Option<[u64; 12]>),
    Assembly(Hex),
    JoinTolerance(Option<u64>),
    Source(crate::build::SourceId, Option<Hex>),
    Pieces(FeatureUid, Vec<Hex>),
}

impl StoredRead {
    fn new(read: &Read, params: &Parameters) -> Self {
        let hex = |v: &Option<u128>| v.map(Hex);
        match read {
            Read::SheetRules(uid, v) => Self::SheetRules(*uid, hex(v)),
            Read::Sheet(uid, v) => Self::Sheet(*uid, hex(v)),
            Read::Param(id, bits) => Self::Param(params.name(*id), *bits),
            Read::Sketch(uid, v) => Self::Sketch(*uid, hex(v)),
            Read::Body(uid, v) => Self::Body(*uid, hex(v)),
            Read::AllBodies(bodies) => {
                Self::AllBodies(bodies.iter().map(|(uid, v)| (*uid, Hex(*v))).collect())
            }
            Read::Output(uid, v) => Self::Output(*uid, hex(v)),
            Read::Datum(uid, v) => Self::Datum(*uid, hex(v)),
            Read::BodyIn(component, uid, v) => Self::BodyIn(*component, *uid, hex(v)),
            Read::BodiesIn(component, bodies) => Self::BodiesIn(
                *component,
                bodies.iter().map(|(uid, v)| (*uid, Hex(*v))).collect(),
            ),
            Read::Placement(uid, bits) => Self::Placement(*uid, *bits),
            Read::Assembly(key) => Self::Assembly(Hex(*key)),
            Read::JoinTolerance(bits) => Self::JoinTolerance(*bits),
            Read::Source(id, v) => Self::Source(*id, hex(v)),
            Read::Pieces(uid, fits) => Self::Pieces(*uid, fits.iter().map(|f| Hex(*f)).collect()),
        }
    }

    /// The read with the parameter ids of `params`; None for a parameter
    /// that is not there.
    fn read(&self, params: &Parameters) -> Option<Read> {
        let version = |v: &Option<Hex>| v.map(|h| h.0);
        Some(match self {
            Self::SheetRules(uid, v) => Read::SheetRules(*uid, version(v)),
            Self::Sheet(uid, v) => Read::Sheet(*uid, version(v)),
            Self::Param(name, bits) => Read::Param(params.find(name)?, *bits),
            Self::Sketch(uid, v) => Read::Sketch(*uid, version(v)),
            Self::Body(uid, v) => Read::Body(*uid, version(v)),
            Self::AllBodies(bodies) => {
                Read::AllBodies(bodies.iter().map(|(uid, v)| (*uid, v.0)).collect())
            }
            Self::Output(uid, v) => Read::Output(*uid, version(v)),
            Self::Datum(uid, v) => Read::Datum(*uid, version(v)),
            Self::BodyIn(component, uid, v) => Read::BodyIn(*component, *uid, version(v)),
            Self::BodiesIn(component, bodies) => Read::BodiesIn(
                *component,
                bodies.iter().map(|(uid, v)| (*uid, v.0)).collect(),
            ),
            Self::Placement(uid, bits) => Read::Placement(*uid, *bits),
            Self::Assembly(key) => Read::Assembly(key.0),
            Self::JoinTolerance(bits) => Read::JoinTolerance(*bits),
            Self::Source(id, v) => Read::Source(*id, version(v)),
            Self::Pieces(uid, fits) => Read::Pieces(*uid, fits.iter().map(|f| f.0).collect()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredChange {
    /// The body takes shape `shape` of the variant.
    Set {
        body: BodyUid,
        shape: usize,
    },
    Remove {
        body: BodyUid,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct VariantHead {
    #[serde(default)]
    sheet_protocol: u32,
    #[serde(default)]
    sheet_rules: Option<crate::features::SheetRulesDef<f64>>,
    #[serde(default)]
    sheets: BTreeMap<BodyUid, crate::sheet::SheetChange>,
    #[serde(default)]
    sheet_footprint: Vec<BodyUid>,
    reads: Vec<StoredRead>,
    /// The evaluation's error; None when it succeeded.
    error: Option<String>,
    warnings: Vec<String>,
    /// A pattern's elements: linear part (row-major) and translation.
    elements: Vec<([[f64; 3]; 3], [f64; 3])>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pattern_anchor: Option<crate::features::point_pattern::PatternAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sweep_place: Option<crate::features::sweep::SweepPlace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    move_basis: Option<crate::features::moves::MotionBasis>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    owners: BTreeMap<BodyUid, ComponentUid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cut_targets: Option<crate::features::extrude_scope::CutTargets>,
    changes: Vec<StoredChange>,
    tool: Option<usize>,
    /// How long the evaluation took.
    ms: f64,
    /// What it remembers of the pieces of its splits, and the bodies that
    /// are other pieces than before (mitcad#408).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    split_pieces: Option<Vec<crate::features::PieceGroup>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    changed_pieces: Vec<(BodyUid, String)>,
    /// A tapped hole's walls its modelled thread took whole, as they were
    /// (mitcad#436).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    threaded_walls: Vec<crate::features::hole::ThreadedWall>,
    /// A Fillet's or Chamfer's picked edges as polylines (mitcad#957), so
    /// that a warm open shows them as a build does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    inputs: Option<crate::features::FeatureInputs>,
}

/// A stored result: its head and its shapes, zlib-compressed.
#[derive(Debug, Clone, PartialEq)]
struct Variant {
    head: VariantHead,
    shapes: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq)]
struct StoreFile {
    build_id: String,
    meta: Meta,
    variants: Vec<Variant>,
}

/// Why a file could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileError {
    Missing,
    Unreadable,
    /// Damaged, cut short, or not a store file: no use to anyone.
    Damaged,
}

/// Whether results of the definition go to the store (see the module
/// comment).
pub(crate) fn storable(def: &FeatureDef) -> bool {
    match def {
        FeatureDef::SheetRules(_) => false,
        FeatureDef::Sketch(_)
        | FeatureDef::ConstructionPlane(_)
        | FeatureDef::ConstructionAxis(_)
        | FeatureDef::ConstructionPoint(_)
        | FeatureDef::MoveOccurrence(_)
        | FeatureDef::CapturePosition(_) => false,
        // Component patterns publish placements and depend on assembly structure.
        FeatureDef::PatternAtPoints(p)
            if matches!(
                p.objects,
                crate::features::PatternObjects::Components { .. }
            ) =>
        {
            false
        }
        // Joints between occurrences (mitcad#55).
        FeatureDef::Joint(_)
        | FeatureDef::AsBuiltJoint(_)
        | FeatureDef::JointOrigin(_)
        | FeatureDef::RigidGroup(_) => false,
        // Assembly constraints (mitcad#148).
        FeatureDef::ConstraintSet(_) => false,
        // BUILD (mitcad#375): mount frames and measured lengths are not stored.
        FeatureDef::DerivedPart(_) | FeatureDef::BoundLength(_) => false,
        // Base features too, also those that only bring in bodies
        // (mitcad#887): a stored body keeps the names evaluating derives.
        _ => true,
    }
}

/// The key of a feature's results: its uid and definition.
fn key(uid: FeatureUid, def_fp: u128) -> u128 {
    Fingerprint::new("store key")
        .u64(uid.0)
        .u128(def_fp)
        .finish()
}

/// The key of a body shape's display meshes: its version and the meshing
/// options (mitcad#858).
fn mesh_key(version: u128, options: &str) -> u128 {
    Fingerprint::new("display mesh key")
        .u128(version)
        .str(options)
        .finish()
}

/// The memory of the shapes a result would store, as the kernel estimates
/// it: the bodies it sets and its tool (mitcad#882). Shapes still packed
/// (a result read from the store) count as nothing.
pub(crate) fn shape_memory<K: Kernel>(kernel: &K, output: &Output<K::Shape>) -> u64 {
    let changes: u64 = if output.changes.is_unpacked() {
        output
            .changes
            .unpacked()
            .iter()
            .map(|change| match change {
                Change::Set(_, shape) => kernel.shape_memory(&shape.value),
                Change::Remove(_) => 0,
            })
            .sum()
    } else {
        0
    };
    changes
        + output
            .tool
            .as_ref()
            .map_or(0, |tool| kernel.shape_memory(tool))
}

/// Whether quick results are stored when cheaper to restore than to
/// evaluate (mitcad#882): unless `MITCAD_STORE_QUICK` is `0` or `off`.
fn store_quick() -> bool {
    static QUICK: OnceLock<bool> = OnceLock::new();
    *QUICK.get_or_init(|| {
        !matches!(
            std::env::var("MITCAD_STORE_QUICK").as_deref(),
            Ok("0" | "off")
        )
    })
}

/// Whether results are found with their bodies' shapes packed until
/// something reads them (mitcad#858): unless `MITCAD_LAZY_RESTORE` is `0`
/// or `off`.
pub(crate) fn lazy_restore() -> bool {
    static LAZY: OnceLock<bool> = OnceLock::new();
    *LAZY.get_or_init(|| {
        !matches!(
            std::env::var("MITCAD_LAZY_RESTORE").as_deref(),
            Ok("0" | "off")
        )
    })
}

/// A shape of a file, zlib-compressed, unpacked.
fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    miniz_oxide::inflate::decompress_to_vec_zlib(data)
        .map_err(|e| format!("a stored shape does not decompress: {:?}", e.status))
}

/// `work` done on each item, on as many threads as the machine has (at
/// most one per item), the results in the items' order (mitcad#887:
/// unpacking stored shapes). A panic in `work` goes on in the caller.
pub(crate) fn in_parallel<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(items.len());
    if threads <= 1 {
        return items.iter().map(work).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut done: Vec<(usize, R)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(at) else {
                            break done;
                        };
                        done.push((at, work(item)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect()
    });
    done.sort_unstable_by_key(|(at, _)| *at);
    done.into_iter().map(|(_, result)| result).collect()
}

/// The shapes of the bodies a found result sets, still packed (mitcad#858;
/// [`Changes::unpacked`] unpacks them when something first reads them).
pub(crate) struct Packed<S> {
    unpack: fn(&[u8]) -> Result<S, KernelError>,
    stand_in: fn() -> S,
    /// Each change's shape, zlib-compressed (empty for a removal).
    shapes: Vec<Vec<u8>>,
    /// The file they came from: removed if one of them does not unpack.
    path: PathBuf,
}

impl<S> Packed<S> {
    /// The bytes of the packed shapes.
    pub(crate) fn bytes(&self) -> u64 {
        self.shapes.iter().map(|shape| shape.len() as u64).sum()
    }

    /// The changes `heads` (the result's) with their shapes. A shape that
    /// does not unpack (not one this build wrote whole) keeps the
    /// stand-in, and the file is removed, so that the feature is evaluated
    /// the next time it is not in memory.
    pub(crate) fn unpack(self, heads: &[ChangeHead]) -> Vec<Change<S>> {
        let start = Instant::now();
        let changes = heads
            .iter()
            .zip(&self.shapes)
            .map(|(head, data)| self.unpack_one(head, data))
            .collect();
        self.unpacked(start, changes)
    }

    /// As [`Packed::unpack`], with the shapes unpacked on threads of their
    /// own (mitcad#887).
    pub(crate) fn unpack_in_parallel(self, heads: &[ChangeHead]) -> Vec<Change<S>>
    where
        S: Send,
    {
        let start = Instant::now();
        let jobs: Vec<_> = heads.iter().zip(&self.shapes).collect();
        let changes = in_parallel(&jobs, |(head, data)| self.unpack_one(head, data));
        self.unpacked(start, changes)
    }

    /// A change with its shape, and why the shape did not unpack.
    fn unpack_one(&self, head: &ChangeHead, data: &[u8]) -> (Change<S>, Option<String>) {
        match *head {
            ChangeHead::Set(body, version) => {
                let (value, failed) = match inflate(data)
                    .and_then(|bytes| (self.unpack)(&bytes).map_err(|e| e.to_string()))
                {
                    Ok(value) => (value, None),
                    Err(error) => ((self.stand_in)(), Some(error)),
                };
                (Change::Set(body, Versioned { version, value }), failed)
            }
            ChangeHead::Remove(body) => (Change::Remove(body), None),
        }
    }

    /// The changes unpacked since `start`, counted; the file removed when
    /// a shape did not unpack.
    fn unpacked(
        &self,
        start: Instant,
        unpacked: Vec<(Change<S>, Option<String>)>,
    ) -> Vec<Change<S>> {
        let bytes = self.bytes();
        let mut failed = None;
        let changes = unpacked
            .into_iter()
            .map(|(change, error)| {
                if error.is_some() {
                    failed = error;
                }
                change
            })
            .collect();
        if let Some(error) = failed {
            eprintln!(
                "Result store: {}: a stored shape does not unpack: {error}",
                self.path.display()
            );
            count(&COUNTERS.damaged, 1);
            let _ = fs::remove_file(&self.path);
        }
        count(&COUNTERS.unpacked, 1);
        count(&COUNTERS.unpack_bytes, bytes);
        count(&COUNTERS.unpack_ns, nanos(start.elapsed()));
        changes
    }
}

/// The body shape a display mesh file is of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MeshMeta {
    version: Hex,
    options: String,
    /// The document it was stored for last.
    document: String,
}

impl ResultStore {
    /// The store in `dir` (made when a result is written), for results of
    /// the build `build_id`.
    pub fn new(dir: impl Into<PathBuf>, build_id: &str) -> Self {
        Self {
            dir: dir.into(),
            build_id: build_id.to_owned(),
            label: String::new(),
            quick: store_quick().then_some(QUICK_MEMORY_PER_MS),
        }
    }

    /// Results written are marked as the document's (its file name).
    pub fn with_label(mut self, label: &str) -> Self {
        self.label = label.to_owned();
        self
    }

    /// Results quicker than `persist_results`'s minimum are stored when
    /// their shapes take at most `memory_per_ms` bytes per millisecond of
    /// their evaluation; None stores none of them (mitcad#882). The
    /// default is [`QUICK_MEMORY_PER_MS`], None with `MITCAD_STORE_QUICK=0`.
    pub fn with_quick_results(mut self, memory_per_ms: Option<u64>) -> Self {
        self.quick = memory_per_ms;
        self
    }

    pub fn quick_results(&self) -> Option<u64> {
        self.quick
    }

    /// Whether a quick result whose evaluation took `time` and whose
    /// shapes take `memory` bytes is stored (the module comment).
    pub fn worth_storing(&self, time: Duration, memory: u64) -> bool {
        // memory <= rate * milliseconds, in nanoseconds.
        self.quick.is_some_and(|rate| {
            u128::from(memory) * 1_000_000 <= u128::from(rate) * time.as_nanos()
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn build_id(&self) -> &str {
        &self.build_id
    }

    fn path(&self, key: u128) -> PathBuf {
        let name = format!("{key:032x}");
        self.dir
            .join(LAYOUT)
            .join(&name[..2])
            .join(format!("{name}.{EXTENSION}"))
    }

    /// The result of `entry` stored for the inputs `env` gives, with the
    /// versions it was evaluated with; None when there is none (or it
    /// cannot be read: a damaged file is removed). With an unpacker, the
    /// shapes of the bodies it sets stay packed (mitcad#858).
    pub(crate) fn find<K: Kernel>(
        &self,
        kernel: &K,
        entry: &FeatureEntry,
        def_fp: u128,
        env: &Env<'_, K::Shape>,
        unpacker: Option<&ShapeUnpacker<K::Shape>>,
    ) -> Option<Output<K::Shape>> {
        if !storable(&entry.def) {
            return None;
        }
        count(&COUNTERS.lookups, 1);
        let start = Instant::now();
        let path = self.path(key(entry.uid, def_fp));
        let (file, size) = match read_file(&path) {
            Ok(read) => read,
            Err(FileError::Damaged) => {
                count(&COUNTERS.damaged, 1);
                count(&COUNTERS.misses, 1);
                let _ = fs::remove_file(&path);
                return None;
            }
            Err(FileError::Missing | FileError::Unreadable) => {
                count(&COUNTERS.misses, 1);
                return None;
            }
        };
        if file.build_id != self.build_id {
            count(&COUNTERS.other_build, 1);
            count(&COUNTERS.misses, 1);
            return None;
        }
        if file.meta.feature != entry.uid || file.meta.def != Hex(def_fp) {
            count(&COUNTERS.misses, 1);
            return None;
        }
        for variant in file.variants {
            if variant.head.sheet_protocol != crate::sheet::ALGORITHM_VERSION {
                continue;
            }
            let Some(reads) = variant
                .head
                .reads
                .iter()
                .map(|r| r.read(env.params))
                .collect::<Option<Vec<Read>>>()
            else {
                continue;
            };
            if !reads.iter().all(|r| r.matches(env)) {
                continue;
            }
            let hit = Hit {
                uid: entry.uid,
                def_fp,
                reads,
                path: &path,
            };
            return match restore(kernel, hit, variant, env.params, unpacker) {
                Ok(output) => {
                    touch(&path);
                    count(&COUNTERS.hits, 1);
                    count(&COUNTERS.read_bytes, size);
                    count(&COUNTERS.read_ns, nanos(start.elapsed()));
                    Some(output)
                }
                Err(_) => {
                    count(&COUNTERS.damaged, 1);
                    count(&COUNTERS.misses, 1);
                    let _ = fs::remove_file(&path);
                    None
                }
            };
        }
        count(&COUNTERS.misses, 1);
        None
    }

    /// Writes the result of `entry` (evaluated with `params`) to the store,
    /// before the variants stored for other inputs; returns the bytes of
    /// the file.
    pub fn write<K: Kernel>(
        &self,
        kernel: &K,
        entry: &FeatureEntry,
        output: &Output<K::Shape>,
        params: &Parameters,
    ) -> Result<(u64, usize), String> {
        let start = Instant::now();
        let mut shapes = Vec::new();
        let mut store = |shape: &K::Shape| -> Result<usize, String> {
            let bytes = kernel.shape_bytes(shape).map_err(|e| e.to_string())?;
            shapes.push(miniz_oxide::deflate::compress_to_vec_zlib(&bytes, LEVEL));
            Ok(shapes.len() - 1)
        };
        let mut changes = Vec::with_capacity(output.changes.len());
        for change in &output.changes {
            changes.push(match change {
                Change::Set(body, shape) => StoredChange::Set {
                    body: *body,
                    shape: store(&shape.value)?,
                },
                Change::Remove(body) => StoredChange::Remove { body: *body },
            });
        }
        let tool = output.tool.as_ref().map(&mut store).transpose()?;
        let variant = Variant {
            head: VariantHead {
                sheet_protocol: crate::sheet::ALGORITHM_VERSION,
                sheet_rules: output.sheet_rules.as_ref().map(|r| r.value.clone()),
                sheets: output.sheets.clone(),
                sheet_footprint: output.sheet_footprint.clone(),
                reads: output
                    .reads
                    .iter()
                    .map(|r| StoredRead::new(r, params))
                    .collect(),
                error: output.result.clone().err(),
                warnings: output.warnings.clone(),
                pattern_anchor: output.pattern_anchor.clone(),
                sweep_place: output.sweep_place,
                move_basis: output.move_basis,
                owners: output.owners.clone(),
                cut_targets: output.cut_targets.clone(),
                elements: output
                    .elements
                    .iter()
                    .map(|t| (t.linear, t.translation))
                    .collect(),
                changes,
                tool,
                ms: output.time.as_secs_f64() * 1000.0,
                split_pieces: output.split_pieces.as_deref().cloned(),
                changed_pieces: output.changed_pieces.clone(),
                threaded_walls: output.threaded_walls.clone(),
                inputs: output.inputs.clone(),
            },
            shapes,
        };
        let shape_count = variant.shapes.len();
        let meta = Meta {
            feature: entry.uid,
            def: Hex(output.def_fp),
            name: entry.name.clone(),
            kind: entry.def.type_name().to_owned(),
            document: self.label.clone(),
        };
        let path = self.path(key(entry.uid, output.def_fp));
        // The variants stored before for other inputs stay, if this build
        // stored them.
        let mut variants = match read_file(&path) {
            Ok((file, _)) if file.build_id == self.build_id && file.meta.def == meta.def => {
                file.variants
            }
            _ => Vec::new(),
        };
        variants.retain(|v| v.head.reads != variant.head.reads);
        variants.insert(0, variant);
        variants.truncate(VARIANTS);
        let bytes = encode(&StoreFile {
            build_id: self.build_id.clone(),
            meta,
            variants,
        });
        let written = write_atomic(&path, &bytes).map_err(|e| {
            count(&COUNTERS.write_errors, 1);
            format!("{}: {e}", path.display())
        })?;
        count(&COUNTERS.writes, 1);
        count(&COUNTERS.write_bytes, written);
        count(&COUNTERS.write_ns, nanos(start.elapsed()));
        Ok((written, shape_count))
    }

    fn mesh_path(&self, key: u128) -> PathBuf {
        let name = format!("{key:032x}");
        self.dir
            .join(LAYOUT)
            .join(&name[..2])
            .join(format!("{name}.{MESH_EXTENSION}"))
    }

    /// Whether the store has a file of display triangulations for a body
    /// shape of `version` meshed with `options`, without reading it
    /// (mitcad#881: a body the open would take them for is not meshed
    /// during its recompute). The file may still turn out to be of another
    /// build or damaged when [`ResultStore::restore_display_mesh`] reads
    /// it.
    pub fn has_display_mesh(&self, version: u128, options: &str) -> bool {
        self.mesh_path(mesh_key(version, options)).is_file()
    }

    /// Gives `shape`, a body shape of `version`, the display triangulations
    /// stored for it with the meshing `options` (mitcad#858); how many
    /// faces took one. None when there are none: no file, another build's
    /// or other options' (not used), a damaged one (removed), or
    /// triangulations the kernel finds are not of this shape (nothing
    /// written; the body is meshed and stored again).
    pub fn restore_display_mesh<K: Kernel>(
        &self,
        kernel: &K,
        version: u128,
        options: &str,
        shape: &K::Shape,
    ) -> Option<usize> {
        count(&COUNTERS.mesh_lookups, 1);
        let start = Instant::now();
        let path = self.mesh_path(mesh_key(version, options));
        let bytes = fs::read(&path).ok()?;
        let Ok((build_id, meta, data)) = decode_mesh(&bytes) else {
            count(&COUNTERS.damaged, 1);
            let _ = fs::remove_file(&path);
            return None;
        };
        if build_id != self.build_id {
            count(&COUNTERS.other_build, 1);
            return None;
        }
        if meta.version != Hex(version) || meta.options != options {
            return None;
        }
        let Ok(mesh) = inflate(data) else {
            count(&COUNTERS.damaged, 1);
            let _ = fs::remove_file(&path);
            return None;
        };
        let faces = kernel.restore_display_mesh(shape, &mesh).ok()?;
        touch(&path);
        count(&COUNTERS.mesh_hits, 1);
        count(&COUNTERS.mesh_faces, faces as u64);
        count(&COUNTERS.mesh_read_bytes, bytes.len() as u64);
        count(&COUNTERS.mesh_read_ns, nanos(start.elapsed()));
        Some(faces)
    }

    /// Writes the display triangulations of `shape`, a body shape of
    /// `version` meshed for display with `options` (mitcad#858), over any
    /// stored before; returns the bytes of the file.
    pub fn write_display_mesh<K: Kernel>(
        &self,
        kernel: &K,
        version: u128,
        options: &str,
        shape: &K::Shape,
    ) -> Result<u64, String> {
        let start = Instant::now();
        let mesh = kernel
            .display_mesh_bytes(shape)
            .map_err(|e| e.to_string())?;
        self.write_display_mesh_bytes(version, options, &mesh, start)
    }

    /// Writes `mesh`, the display triangulations of a body shape of
    /// `version` meshed with `options` as [`Kernel::display_mesh_bytes`]
    /// gives them, over any stored before; returns the bytes of the file.
    /// The application's writer after an open (mitcad#880) calls it on its
    /// own thread, with triangulations it took in the open's job: any
    /// thread may, while documents use the store. The file is written
    /// whole or not at all (a temporary file renamed), so an open that
    /// reads meanwhile finds the previous one or none.
    pub fn write_display_mesh_data(
        &self,
        version: u128,
        options: &str,
        mesh: &[u8],
    ) -> Result<u64, String> {
        self.write_display_mesh_bytes(version, options, mesh, Instant::now())
    }

    fn write_display_mesh_bytes(
        &self,
        version: u128,
        options: &str,
        mesh: &[u8],
        start: Instant,
    ) -> Result<u64, String> {
        let meta = MeshMeta {
            version: Hex(version),
            options: options.to_owned(),
            document: self.label.clone(),
        };
        let bytes = encode_mesh(
            &self.build_id,
            &meta,
            &miniz_oxide::deflate::compress_to_vec_zlib(mesh, LEVEL),
        );
        let path = self.mesh_path(mesh_key(version, options));
        let written = write_atomic(&path, &bytes).map_err(|e| {
            count(&COUNTERS.write_errors, 1);
            format!("{}: {e}", path.display())
        })?;
        count(&COUNTERS.mesh_writes, 1);
        count(&COUNTERS.mesh_write_bytes, written);
        count(&COUNTERS.mesh_write_ns, nanos(start.elapsed()));
        Ok(written)
    }
}

/// A display mesh file: the build id, the header and the compressed
/// triangulations.
fn encode_mesh(build_id: &str, meta: &MeshMeta, mesh: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(mesh.len() + 256);
    put_data(&mut payload, build_id.as_bytes());
    put_data(
        &mut payload,
        &serde_json::to_vec(meta).expect("headers serialize"),
    );
    put_data(&mut payload, mesh);
    let mut out = Vec::with_capacity(HEADER + payload.len());
    frame(&mut out, MESH_MAGIC, MESH_FORMAT, &payload);
    out
}

fn decode_mesh(bytes: &[u8]) -> Result<(String, MeshMeta, &[u8]), FileError> {
    let mut cursor = Cursor {
        data: payload_of(bytes, MESH_MAGIC, MESH_FORMAT)?,
        at: 0,
    };
    let build_id = String::from_utf8(cursor.data()?.to_vec()).map_err(|_| FileError::Damaged)?;
    let meta: MeshMeta = cursor.json()?;
    let mesh = cursor.data()?;
    if cursor.at != cursor.data.len() {
        return Err(FileError::Damaged);
    }
    Ok((build_id, meta, mesh))
}

/// A variant whose reads fit: the feature, the reads (with this
/// document's parameter ids) and the file.
struct Hit<'a> {
    uid: FeatureUid,
    def_fp: u128,
    reads: Vec<Read>,
    path: &'a Path,
}

/// The result of a variant whose reads fit: its shapes read with the
/// kernel (with an unpacker, those of the bodies it sets packed instead,
/// mitcad#858), its versions made again from what it read.
fn restore<K: Kernel>(
    kernel: &K,
    hit: Hit<'_>,
    variant: Variant,
    params: &Parameters,
    unpacker: Option<&ShapeUnpacker<K::Shape>>,
) -> Result<Output<K::Shape>, String> {
    let Hit {
        uid,
        def_fp,
        reads,
        path,
    } = hit;
    let Variant { head, shapes } = variant;
    let data = |i: usize| {
        shapes
            .get(i)
            .ok_or_else(|| format!("stored shape {i} is missing"))
    };
    let shape = |i: usize| -> Result<K::Shape, String> {
        let bytes = inflate(data(i)?)?;
        kernel.shape_from_bytes(&bytes).map_err(|e| e.to_string())
    };
    let version = output_version(uid, def_fp, &reads, params);
    let heads: Vec<ChangeHead> = head
        .changes
        .iter()
        .enumerate()
        .map(|(i, change)| match change {
            StoredChange::Set { body, .. } => {
                ChangeHead::Set(*body, part_version(version, Part::Body(i)))
            }
            StoredChange::Remove { body } => ChangeHead::Remove(*body),
        })
        .collect();
    let changes = match unpacker {
        Some(unpacker) => {
            let mut packed = Vec::with_capacity(head.changes.len());
            for change in &head.changes {
                packed.push(match change {
                    StoredChange::Set { shape: index, .. } => data(*index)?.clone(),
                    StoredChange::Remove { .. } => Vec::new(),
                });
            }
            count(&COUNTERS.deferred, 1);
            Changes::packed(
                heads,
                Packed {
                    unpack: unpacker.unpack,
                    stand_in: unpacker.stand_in,
                    shapes: packed,
                    path: path.to_owned(),
                },
            )
        }
        None => {
            let mut changes = Vec::with_capacity(head.changes.len());
            for (change, stored) in heads.iter().zip(&head.changes) {
                changes.push(match (*change, stored) {
                    (ChangeHead::Set(body, version), StoredChange::Set { shape: index, .. }) => {
                        Change::Set(
                            body,
                            Versioned {
                                version,
                                value: shape(*index)?,
                            },
                        )
                    }
                    (ChangeHead::Set(body, _) | ChangeHead::Remove(body), _) => {
                        Change::Remove(body)
                    }
                });
            }
            Changes::new(changes)
        }
    };
    Ok(Output {
        sheet_rules: head.sheet_rules.clone().map(|value| Versioned {
            version: part_version(version, Part::SheetRules),
            value,
        }),
        sheets: head.sheets.clone(),
        sheet_footprint: head.sheet_footprint.clone(),
        version,
        def_fp,
        reads,
        time: Duration::from_secs_f64(head.ms.max(0.0) / 1000.0),
        persisted: Flag::new(true),
        result: head.error.clone().map_or(Ok(()), Err),
        warnings: head.warnings.clone(),
        pattern_anchor: head.pattern_anchor.clone(),
        sweep_place: head.sweep_place,
        move_basis: head.move_basis,
        owners: head.owners.clone(),
        cut_targets: head.cut_targets.clone(),
        elements: head
            .elements
            .iter()
            .map(|(linear, translation)| Transform {
                linear: *linear,
                translation: *translation,
            })
            .collect(),
        sketch: None,
        changes,
        tool: head.tool.map(shape).transpose()?,
        datum: None,
        placements: Vec::new(),
        mounts: None,
        measured: Vec::new(),
        split_pieces: head.split_pieces.clone().map(std::sync::Arc::new),
        changed_pieces: head.changed_pieces.clone(),
        threaded_walls: head.threaded_walls.clone(),
        inputs: head.inputs.clone(),
    })
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_data(out: &mut Vec<u8>, data: &[u8]) {
    put_u64(out, data.len() as u64);
    out.extend_from_slice(data);
}

fn encode(file: &StoreFile) -> Vec<u8> {
    let mut payload = Vec::new();
    put_data(&mut payload, file.build_id.as_bytes());
    put_data(
        &mut payload,
        &serde_json::to_vec(&file.meta).expect("headers serialize"),
    );
    put_u32(&mut payload, file.variants.len() as u32);
    for variant in &file.variants {
        put_data(
            &mut payload,
            &serde_json::to_vec(&variant.head).expect("heads serialize"),
        );
        put_u32(&mut payload, variant.shapes.len() as u32);
        for shape in &variant.shapes {
            put_data(&mut payload, shape);
        }
    }
    let mut out = Vec::with_capacity(HEADER + payload.len());
    frame(&mut out, MAGIC, FORMAT, &payload);
    out
}

/// The header (`magic`, `format`, length, checksum) and the payload.
fn frame(out: &mut Vec<u8>, magic: &[u8; 8], format: u32, payload: &[u8]) {
    out.extend_from_slice(magic);
    put_u32(out, format);
    put_u64(out, payload.len() as u64);
    out.extend_from_slice(&checksum(payload).to_le_bytes());
    out.extend_from_slice(payload);
}

fn checksum(payload: &[u8]) -> u128 {
    crate::fingerprint::of_bytes("store file", payload)
}

/// Reads the parts of a payload in order.
struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, size: usize) -> Result<&'a [u8], FileError> {
        if size > self.data.len() - self.at {
            return Err(FileError::Damaged);
        }
        let part = &self.data[self.at..self.at + size];
        self.at += size;
        Ok(part)
    }

    fn u32(&mut self) -> Result<u32, FileError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
    }

    fn u64(&mut self) -> Result<u64, FileError> {
        let bytes = self.take(8)?;
        Ok(u64::from_le_bytes(bytes.try_into().expect("8 bytes")))
    }

    fn data(&mut self) -> Result<&'a [u8], FileError> {
        let size = usize::try_from(self.u64()?).map_err(|_| FileError::Damaged)?;
        self.take(size)
    }

    fn json<T: serde::de::DeserializeOwned>(&mut self) -> Result<T, FileError> {
        serde_json::from_slice(self.data()?).map_err(|_| FileError::Damaged)
    }
}

/// The header and the payload of a file's bytes, the checksum checked.
fn payload(bytes: &[u8]) -> Result<&[u8], FileError> {
    payload_of(bytes, MAGIC, FORMAT)
}

/// The payload of a file of `magic` and `format`, the checksum checked.
fn payload_of<'a>(bytes: &'a [u8], magic: &[u8; 8], format: u32) -> Result<&'a [u8], FileError> {
    if bytes.len() < HEADER || &bytes[..8] != magic {
        return Err(FileError::Damaged);
    }
    let stored = u32::from_le_bytes(bytes[8..12].try_into().expect("4 bytes"));
    if stored != format {
        return Err(FileError::Damaged);
    }
    let size = u64::from_le_bytes(bytes[12..20].try_into().expect("8 bytes"));
    let sum = u128::from_le_bytes(bytes[20..36].try_into().expect("16 bytes"));
    let payload = &bytes[HEADER..];
    if payload.len() as u64 != size {
        return Err(FileError::Damaged);
    }
    if checksum(payload) != sum {
        return Err(FileError::Damaged);
    }
    Ok(payload)
}

fn decode(bytes: &[u8]) -> Result<StoreFile, FileError> {
    let mut cursor = Cursor {
        data: payload(bytes)?,
        at: 0,
    };
    let build_id = String::from_utf8(cursor.data()?.to_vec()).map_err(|_| FileError::Damaged)?;
    let meta: Meta = cursor.json()?;
    let count = cursor.u32()?;
    let mut variants = Vec::new();
    for _ in 0..count {
        let head: VariantHead = cursor.json()?;
        let shapes = cursor.u32()?;
        let mut data = Vec::new();
        for _ in 0..shapes {
            data.push(cursor.data()?.to_vec());
        }
        let refers = |i: usize| i < data.len();
        let valid = head.changes.iter().all(|c| match c {
            StoredChange::Set { shape, .. } => refers(*shape),
            StoredChange::Remove { .. } => true,
        }) && head.tool.is_none_or(refers);
        if !valid {
            return Err(FileError::Damaged);
        }
        variants.push(Variant { head, shapes: data });
    }
    if cursor.at != cursor.data.len() {
        return Err(FileError::Damaged);
    }
    Ok(StoreFile {
        build_id,
        meta,
        variants,
    })
}

/// A file and its size.
fn read_file(path: &Path) -> Result<(StoreFile, u64), FileError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(FileError::Missing),
        Err(_) => return Err(FileError::Unreadable),
    };
    decode(&bytes).map(|file| (file, bytes.len() as u64))
}

/// Writes a file whole: a temporary file next to it, then renamed over it.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<u64> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = path.parent().expect("store files are in a folder");
    fs::create_dir_all(dir)?;
    let name = path.file_name().expect("a file name").to_string_lossy();
    let temporary = dir.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    if let Err(e) = fs::write(&temporary, bytes).and_then(|()| fs::rename(&temporary, path)) {
        let _ = fs::remove_file(&temporary);
        return Err(e);
    }
    Ok(bytes.len() as u64)
}

/// A hit makes the file the newest for [`gc`].
fn touch(path: &Path) {
    if let Ok(file) = fs::OpenOptions::new().write(true).open(path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

/// A store file or a temporary one, with its size and time.
struct Found {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
    temporary: bool,
    /// A display mesh file (mitcad#858).
    mesh: bool,
}

fn files(dir: &Path) -> Vec<Found> {
    let mut found = Vec::new();
    let Ok(folders) = fs::read_dir(dir.join(LAYOUT)) else {
        return found;
    };
    for folder in folders.flatten() {
        let Ok(entries) = fs::read_dir(folder.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let temporary = name.starts_with('.') && name.ends_with(".tmp");
            let mesh = path.extension().is_some_and(|e| e == MESH_EXTENSION);
            let ours = mesh || path.extension().is_some_and(|e| e == EXTENSION);
            if !temporary && !ours {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            found.push(Found {
                path,
                bytes: metadata.len(),
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                temporary,
                mesh,
            });
        }
    }
    found
}

/// Removes the files used longest ago until the store holds 80 % of
/// `budget` bytes, when it holds more than `budget`; and temporary files
/// left by programs that stopped. Other programs may use the store
/// meanwhile: what one of them reads is whole, or missing.
pub fn gc(dir: &Path, budget: u64) -> GcReport {
    let mut report = GcReport::default();
    let now = SystemTime::now();
    let mut kept = Vec::new();
    for file in files(dir) {
        if file.temporary {
            let age = now.duration_since(file.modified).unwrap_or_default();
            if age > STALE_TEMPORARY && fs::remove_file(&file.path).is_ok() {
                report.removed += 1;
                report.removed_bytes += file.bytes;
            }
            continue;
        }
        kept.push(file);
    }
    let mut total: u64 = kept.iter().map(|f| f.bytes).sum();
    if total > budget {
        let target = (budget as f64 * GC_TARGET) as u64;
        kept.sort_by_key(|f| f.modified);
        let mut remaining = Vec::new();
        for file in kept {
            if total > target && fs::remove_file(&file.path).is_ok() {
                total -= file.bytes;
                report.removed += 1;
                report.removed_bytes += file.bytes;
            } else {
                remaining.push(file);
            }
        }
        kept = remaining;
    }
    report.files = kept.len() as u64;
    report.bytes = total;
    report
}

/// What the store holds, for diagnostics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StoreUsage {
    /// Every store file and its bytes.
    pub files: u64,
    pub bytes: u64,
    /// Files of other builds (never used; replaced, or removed by [`gc`]).
    pub other_files: u64,
    pub other_bytes: u64,
    /// Display mesh files (mitcad#858), of every build, and their bytes.
    pub meshes: u64,
    pub mesh_bytes: u64,
    /// Files that are not store files (they go when read).
    pub damaged: u64,
    /// This build's files.
    pub entries: Vec<StoredEntry>,
}

/// A file of the store: one feature's results.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEntry {
    /// The document it was stored for last.
    pub document: String,
    pub feature: FeatureUid,
    pub name: String,
    pub type_name: String,
    /// Results for different inputs.
    pub variants: u32,
    pub bytes: u64,
    pub modified: SystemTime,
}

/// The start of a file: its build and its feature, without the shapes.
fn read_head(path: &Path) -> Result<(String, Meta, u32), FileError> {
    use std::io::Read as _;
    let mut file = fs::File::open(path).map_err(|_| FileError::Unreadable)?;
    let mut header = [0u8; HEADER];
    file.read_exact(&mut header)
        .map_err(|_| FileError::Damaged)?;
    if &header[..8] != MAGIC || header[8..12] != FORMAT.to_le_bytes() {
        return Err(FileError::Damaged);
    }
    let mut part = |limit: u64| -> Result<Vec<u8>, FileError> {
        let mut size = [0u8; 8];
        file.read_exact(&mut size).map_err(|_| FileError::Damaged)?;
        let size = u64::from_le_bytes(size);
        if size > limit {
            return Err(FileError::Damaged);
        }
        let mut data = vec![0u8; size as usize];
        file.read_exact(&mut data).map_err(|_| FileError::Damaged)?;
        Ok(data)
    };
    let build_id = String::from_utf8(part(1 << 16)?).map_err(|_| FileError::Damaged)?;
    let meta = serde_json::from_slice(&part(1 << 20)?).map_err(|_| FileError::Damaged)?;
    let mut count = [0u8; 4];
    file.read_exact(&mut count)
        .map_err(|_| FileError::Damaged)?;
    Ok((build_id, meta, u32::from_le_bytes(count)))
}

/// What the store in `dir` holds, the files of `build_id` by feature. Reads
/// the start of each file only.
pub fn usage(dir: &Path, build_id: &str) -> StoreUsage {
    let mut usage = StoreUsage::default();
    for file in files(dir) {
        if file.temporary {
            continue;
        }
        usage.files += 1;
        usage.bytes += file.bytes;
        if file.mesh {
            usage.meshes += 1;
            usage.mesh_bytes += file.bytes;
            continue;
        }
        match read_head(&file.path) {
            Ok((build, meta, variants)) if build == build_id => usage.entries.push(StoredEntry {
                document: meta.document,
                feature: meta.feature,
                name: meta.name,
                type_name: meta.kind,
                variants,
                bytes: file.bytes,
                modified: file.modified,
            }),
            Ok(_) => {
                usage.other_files += 1;
                usage.other_bytes += file.bytes;
            }
            Err(_) => usage.damaged += 1,
        }
    }
    usage
}

/// Removes every stored result (Preferences' Clear).
pub fn clear(dir: &Path) -> GcReport {
    let mut report = GcReport::default();
    for file in files(dir) {
        if fs::remove_file(&file.path).is_ok() {
            report.removed += 1;
            report.removed_bytes += file.bytes;
        } else {
            report.files += 1;
            report.bytes += file.bytes;
        }
    }
    report
}

#[cfg(any(test, feature = "testing"))]
pub mod testing {
    //! Files of a test's own store.
    use super::*;

    /// The store files under `dir`, with their sizes.
    pub fn store_files(dir: &Path) -> Vec<(PathBuf, u64)> {
        files(dir)
            .into_iter()
            .filter(|f| !f.temporary)
            .map(|f| (f.path, f.bytes))
            .collect()
    }

    /// The display mesh files under `dir` (mitcad#858), with their sizes.
    pub fn mesh_files(dir: &Path) -> Vec<(PathBuf, u64)> {
        files(dir)
            .into_iter()
            .filter(|f| !f.temporary && f.mesh)
            .map(|f| (f.path, f.bytes))
            .collect()
    }

    /// Sets a file's time, as if it were last used then.
    pub fn set_time(path: &Path, time: SystemTime) {
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(time).unwrap();
    }
}
