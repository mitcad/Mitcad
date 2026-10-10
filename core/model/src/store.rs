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
//! evaluation's time) and its shapes ([`Kernel::shape_bytes`],
//! zlib-compressed). Results stored by another build (`build_id`: the
//! kernel's format and the program, `geometry::kernel_build_id` in the
//! bridge) are not used, and are replaced when the feature is stored
//! again. Files are written to a temporary file and renamed, so programs
//! that share the folder see whole files. A hit touches the file's
//! modification time, and [`gc`] removes the files used longest ago.
//!
//! Not stored: sketches and construction features (cheap, and their
//! outputs have no stored form), features that move occurrences, base
//! features that only bring in their bodies (reading those is what
//! evaluating them does), failed evaluations of a cancelled job (never
//! cached), and what [`Document::persist_results`] is told is too quick.
//!
//! [`Document::persist_results`]: crate::Document::persist_results

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use crate::features::{Env, FeatureDef, FeatureEntry, Operation};
use crate::fingerprint::Fingerprint;
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::parameters::Parameters;
use crate::recompute::{Change, Flag, Output, Part, Read, Versioned, output_version, part_version};
use crate::transform::Transform;

const MAGIC: &[u8; 8] = b"MITCADRS";
const FORMAT: u32 = 1;
/// The folder of this layout under the store's directory.
const LAYOUT: &str = "v1";
const EXTENSION: &str = "mrs";
const HEADER: usize = 8 + 4 + 8 + 16;
/// Results kept per file: the same definition with other inputs.
const VARIANTS: usize = 4;
/// zlib level of stored shapes: fast, as storing is part of opening.
const LEVEL: u8 = 1;
/// [`gc`] removes the oldest files until this share of the budget is used.
const GC_TARGET: f64 = 0.8;
/// Temporary files older than this were left by a program that stopped.
const STALE_TEMPORARY: Duration = Duration::from_secs(3600);

/// A store folder, with the build results must have been stored by and the
/// document they are stored for (shown by [`usage`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultStore {
    dir: PathBuf,
    build_id: String,
    label: String,
}

/// What [`crate::Document::persist_results`] wrote.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PersistReport {
    /// Results written, and their shapes.
    pub results: usize,
    pub shapes: usize,
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
    Param(String, Option<u64>),
    Sketch(FeatureUid, Option<Hex>),
    Body(BodyUid, Option<Hex>),
    AllBodies(Vec<(BodyUid, Hex)>),
    Output(FeatureUid, Option<Hex>),
    Datum(FeatureUid, Option<Hex>),
    BodyIn(ComponentUid, BodyUid, Option<Hex>),
    BodiesIn(ComponentUid, Vec<(BodyUid, Hex)>),
    Placement(OccurrenceUid, Option<[u64; 12]>),
}

impl StoredRead {
    fn new(read: &Read, params: &Parameters) -> Self {
        let hex = |v: &Option<u128>| v.map(Hex);
        match read {
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
        }
    }

    /// The read with the parameter ids of `params`; None for a parameter
    /// that is not there.
    fn read(&self, params: &Parameters) -> Option<Read> {
        let version = |v: &Option<Hex>| v.map(|h| h.0);
        Some(match self {
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
    reads: Vec<StoredRead>,
    /// The evaluation's error; None when it succeeded.
    error: Option<String>,
    warnings: Vec<String>,
    /// A pattern's elements: linear part (row-major) and translation.
    elements: Vec<([[f64; 3]; 3], [f64; 3])>,
    changes: Vec<StoredChange>,
    tool: Option<usize>,
    /// How long the evaluation took.
    ms: f64,
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
        FeatureDef::Sketch(_)
        | FeatureDef::ConstructionPlane(_)
        | FeatureDef::ConstructionAxis(_)
        | FeatureDef::ConstructionPoint(_)
        | FeatureDef::MoveOccurrence(_)
        | FeatureDef::CapturePosition(_) => false,
        // Joints between occurrences (mitcad#55).
        FeatureDef::Joint(_)
        | FeatureDef::AsBuiltJoint(_)
        | FeatureDef::JointOrigin(_)
        | FeatureDef::RigidGroup(_) => false,
        FeatureDef::Base(base) => {
            !matches!(base.operation, Operation::NewBody | Operation::NewComponent)
        }
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

impl ResultStore {
    /// The store in `dir` (made when a result is written), for results of
    /// the build `build_id`.
    pub fn new(dir: impl Into<PathBuf>, build_id: &str) -> Self {
        Self {
            dir: dir.into(),
            build_id: build_id.to_owned(),
            label: String::new(),
        }
    }

    /// Results written are marked as the document's (its file name).
    pub fn with_label(mut self, label: &str) -> Self {
        self.label = label.to_owned();
        self
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
    /// cannot be read: a damaged file is removed).
    pub(crate) fn find<K: Kernel>(
        &self,
        kernel: &K,
        entry: &FeatureEntry,
        def_fp: u128,
        env: &Env<'_, K::Shape>,
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
        for variant in &file.variants {
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
            return match restore(kernel, entry.uid, def_fp, reads, variant, env.params) {
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
    pub(crate) fn write<K: Kernel>(
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
                reads: output
                    .reads
                    .iter()
                    .map(|r| StoredRead::new(r, params))
                    .collect(),
                error: output.result.clone().err(),
                warnings: output.warnings.clone(),
                elements: output
                    .elements
                    .iter()
                    .map(|t| (t.linear, t.translation))
                    .collect(),
                changes,
                tool,
                ms: output.time.as_secs_f64() * 1000.0,
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
}

/// The result of a variant whose reads fit: its shapes read with the
/// kernel, its versions made again from what it read.
fn restore<K: Kernel>(
    kernel: &K,
    uid: FeatureUid,
    def_fp: u128,
    reads: Vec<Read>,
    variant: &Variant,
    params: &Parameters,
) -> Result<Output<K::Shape>, String> {
    let shapes = variant
        .shapes
        .iter()
        .map(|data| {
            let bytes = miniz_oxide::inflate::decompress_to_vec_zlib(data)
                .map_err(|e| format!("a stored shape does not decompress: {:?}", e.status))?;
            kernel.shape_from_bytes(&bytes).map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    let shape = |i: usize| {
        shapes
            .get(i)
            .cloned()
            .ok_or_else(|| format!("stored shape {i} is missing"))
    };
    let head = &variant.head;
    let version = output_version(uid, def_fp, &reads, params);
    let mut changes = Vec::with_capacity(head.changes.len());
    for (i, change) in head.changes.iter().enumerate() {
        changes.push(match change {
            StoredChange::Set { body, shape: index } => Change::Set(
                *body,
                Versioned {
                    version: part_version(version, Part::Body(i)),
                    value: shape(*index)?,
                },
            ),
            StoredChange::Remove { body } => Change::Remove(*body),
        });
    }
    Ok(Output {
        version,
        def_fp,
        reads,
        time: Duration::from_secs_f64(head.ms.max(0.0) / 1000.0),
        persisted: Flag::new(true),
        result: head.error.clone().map_or(Ok(()), Err),
        warnings: head.warnings.clone(),
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
    out.extend_from_slice(MAGIC);
    put_u32(&mut out, FORMAT);
    put_u64(&mut out, payload.len() as u64);
    out.extend_from_slice(&checksum(&payload).to_le_bytes());
    out.extend_from_slice(&payload);
    out
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
    if bytes.len() < HEADER || &bytes[..8] != MAGIC {
        return Err(FileError::Damaged);
    }
    let format = u32::from_le_bytes(bytes[8..12].try_into().expect("4 bytes"));
    if format != FORMAT {
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
            let ours = path.extension().is_some_and(|e| e == EXTENSION);
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

#[cfg(test)]
pub(crate) mod testing {
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

    /// Sets a file's time, as if it were last used then.
    pub fn set_time(path: &Path, time: SystemTime) {
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(time).unwrap();
    }
}
