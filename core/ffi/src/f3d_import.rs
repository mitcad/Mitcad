// SPDX-License-Identifier: MIT
//! The .f3d import with the timeline (T1): the design dump IR decoded
//! from the file (or an external dump) replayed by `mitcad-import`, with the
//! file's bodies built with OCCT as the replay's reference:
//!
//! - the `.smbh` blobs' ASM history, rolled back state by state (A4b),
//!   gives the bodies after each operation. A timeline item's `result_no`
//!   is the number of the state its operation made, so the states of all
//!   blobs are merged in the timeline's order into one sequence of body
//!   sets, oldest first, the last being the stored design (in the order of
//!   the state numbers when no item names one);
//! - without a history, the `.smb` blobs' top-level bodies are the stored
//!   design.
//!
//! Which states hold the same bodies is told from the history when the
//! file is opened (`mitcad_asm::history_states`); a state's bodies are
//! converted to the neutral model when the importer first asks for them,
//! or for those of a state it asks for by the timeline when the file is
//! opened (mitcad#675), and built with OCCT only when the importer asks for
//! the state.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use cxx::SharedPtr;
use mitcad_asm::asm::AsmFile;
use mitcad_asm::asm::history::History;
use mitcad_asm::convert::{self, BodyPrefix, ConvertedBody, Options as ConvertOptions};
use mitcad_asm::history_states::{self, DistinctBody};
use mitcad_dump::Dump;
use mitcad_f3d_file::F3dFile;
use mitcad_import::{StoredBody, StoredGeometry};
use mitcad_model::RecomputeMonitor;
use mitcad_model_api::ApiError;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::Document;
use crate::brep_import::{Source, to_ffi};
use crate::f3d_references::References;
use crate::kernel::exchange::ffi::{
    F3dBuilt, brep_data, f3d_bases, f3d_bases_add, f3d_build_body, f3d_build_body_from,
    f3d_built_kept, f3d_built_note, f3d_built_reused, f3d_built_shape, f3d_read_body,
};
use crate::kernel::{OcctKernel, Shape};

/// Faces, edges, vertices and the rounded sum of the vertices of a body.
type BodyKey = (usize, usize, usize, [i64; 3]);

/// The largest distance (mm) from an edge's end to its vertex that a body
/// of a history state may have and still be built.
const MAX_VERTEX_GAP: f64 = 1e-2;

/// A body of a blob at some state, converted to the neutral model when it
/// is first asked for ([`StoredFile::body`], mitcad#675).
struct Variant {
    blob: usize,
    /// Its `body` record.
    record: usize,
    /// The operations of its blob's history undone to the state it is
    /// converted at (0 without a history).
    step: usize,
    body: OnceLock<ConvertedBody>,
    /// Whether it has the faces to reuse healed ones ([`REUSE_MIN_FACES`]),
    /// once told ([`StoredFile::large`]).
    large: OnceLock<bool>,
}

/// A history blob's ASM data, kept to convert its states' bodies when they
/// are asked for.
struct HistorySource {
    file: AsmFile,
    history: History,
}

struct Blob {
    entry: String,
    asm_version: String,
    /// The component (object id) the design streams link the blob to.
    component: Option<u64>,
    /// Distinct body sets, oldest first, with the number of the ASM state
    /// that made each (`i64::MIN` before the first).
    states: Vec<(i64, Vec<usize>)>,
    /// Every operation of the history by its state number: the index into
    /// `states` of the bodies after it (operations that changed nothing
    /// share the set before them).
    ops: HashMap<i64, usize>,
    /// The operations that changed the blob's bodies (the others share the
    /// set before them).
    changed: HashSet<i64>,
}

/// A timeline item's operation in the history.
struct ItemResult {
    index: i64,
    /// The number of the ASM state its operation made (`result_no`).
    result: i64,
    /// The component that owns the item (`_f3d.component`).
    owner: Option<u64>,
}

/// Whether an item changed its blob's bodies: its operation `result` did
/// (it is among `changed`), or one of the operations no item names between
/// the last one an earlier item named in the blob (`after`) and its own,
/// which are its own (mitcad#719: a pattern of holes whose copies are
/// operations of their own before its last, which changes nothing). The
/// operations before the first named one are the blob's own start, not
/// the item's: an item that failed in the file there stays one.
fn item_changed(changed: &HashSet<i64>, after: Option<i64>, result: i64) -> bool {
    changed.contains(&result)
        || after.is_some_and(|after| changed.iter().any(|&op| after < op && op < result))
}

/// The bodies of one document of an .f3d or .f3z file, converted to the
/// neutral model: read once for all tries of an import (mitcad#80), each of
/// which builds them with OCCT as it needs them ([`F3dGeometry`]).
pub struct StoredFile {
    document: String,
    blobs: Vec<Blob>,
    variants: Vec<Variant>,
    /// Per blob, its history's data (None without a history).
    sources: Vec<Option<Arc<HistorySource>>>,
    options: ConvertOptions,
    /// Global states: per history blob, the index into its `states`.
    states: Vec<Vec<usize>>,
    /// Blobs with a history, in `states` order.
    history: Vec<usize>,
    /// Blobs without: their top-level bodies, for the stored design when
    /// there is no history.
    plain: Vec<usize>,
    /// The global state each timeline item's operation made, by item
    /// index (when the states follow the timeline).
    item_states: HashMap<i64, usize>,
    /// The components (object ids) whose blobs each item's operation
    /// changed, by item index (F6).
    item_components: HashMap<i64, Vec<u64>>,
    /// The items whose operation changed no body of the blobs that hold it
    /// (mitcad#156).
    unchanged_items: HashSet<i64>,
    /// The items that name the operation of an item before them, so that
    /// they have none of their own (mitcad#343).
    shared_items: HashSet<i64>,
    /// The global state before the operations no item names that came
    /// right before each item's own in its blob (mitcad#156), by item index.
    item_starts: HashMap<i64, usize>,
    /// Component names by object id, from the file's design streams.
    component_names: HashMap<u64, String>,
    /// The bodies a try is building, and those a try the watchdog gave up
    /// while it built them built after all, by variant (mitcad#82): another
    /// try waits for a build in progress and reads the body from its B-rep
    /// data rather than building it again. Healing one body of 23 000 faces
    /// took 84 s, 74 s of it in one call of OCCT's, longer than a hang
    /// limit of 60 s: the next try got the body when the call returned.
    /// (Shared with the bodies healed while the file is read, mitcad#245.)
    builds: Arc<Mutex<HashMap<usize, SharedBuild>>>,
    /// Notified whenever a build in `builds` ends.
    build_ended: Arc<Condvar>,
    /// The bodies to build ahead of the replay, oldest state first
    /// ([`F3dGeometry::build_ahead`], mitcad#103).
    ahead: Mutex<std::collections::VecDeque<usize>>,
    /// Per global state, whether the replay asks for it by the timeline
    /// (mitcad#642): the first and the last state, each item's state and
    /// the state its own operations start from. The states between them
    /// are those of the operations of one item (a pattern's copies, one
    /// operation each) or of operations no item names.
    anchors: Vec<bool>,
    /// Per variant, its base when it is large enough to reuse healed faces
    /// ([`StoredFile::base`]): the variant of the same body (record) in the
    /// blob's nearest state before the one it first appears in that the
    /// replay asks for ([`StoredFile::anchors`]), when that is large enough
    /// too (mitcad#181). Its build takes the faces it shares with the
    /// base's build as healed there. The base is fixed by the file, not by
    /// which builds happen to exist, so the bodies are the same with any
    /// number of threads (mitcad#243). (A base in the state right before,
    /// whatever the replay asks for, made a pattern of 372 copies, one
    /// operation each, build every copy's state in a chain: 40 minutes for
    /// the item's state alone, mitcad#642.) Whether the variant itself is
    /// large is told when needed ([`StoredFile::large`]): most variants of
    /// such a pattern are never converted (mitcad#675).
    bases: Vec<Option<usize>>,
    /// Per variant, the variants it would be the base of
    /// ([`StoredFile::bases`]); its heirs are those of them that are large
    /// ([`StoredFile::has_heirs`]).
    heirs: Vec<Vec<usize>>,
    /// What the builds of variants with heirs kept for their heirs' builds
    /// (None: their build kept nothing), until the heirs are built or
    /// released. Healing a body of 5 000 faces took 25–45 s, a state of
    /// the history each.
    healed: Arc<Mutex<HashMap<usize, Option<SharedPtr<F3dBuilt>>>>>,
}

/// The smallest body (faces) whose build reuses healed faces
/// (`mitcad::brep::kMinReuseFaces`): smaller ones have no base.
const REUSE_MIN_FACES: usize = 256;

/// The bases and heirs of variants ([`StoredFile::bases`]), each base and
/// heir whatever the heir's own size, given each variant's record, whether
/// a variant of a state the replay asks for is large, and each history
/// blob's states (their variants, oldest state first, each with whether
/// the replay asks for it: the base comes from the nearest such state
/// before).
fn bases_of(
    records: &[usize],
    large: impl Fn(usize) -> bool,
    histories: &[Vec<(&[usize], bool)>],
) -> (Vec<Option<usize>>, Vec<Vec<usize>>) {
    let mut bases = vec![None; records.len()];
    let mut heirs = vec![Vec::new(); records.len()];
    for states in histories {
        // The nearest state before that the replay asks for.
        let mut anchor: Option<&[usize]> = None;
        for (k, &(after, asked)) in states.iter().enumerate() {
            if k > 0 {
                let before = states[k - 1].0;
                for &v in after {
                    // (A variant is in consecutive states only: one in the
                    // state before is not new here, nor in any earlier.)
                    if before.contains(&v) {
                        continue;
                    }
                    if let Some(&b) = anchor
                        .into_iter()
                        .flatten()
                        .find(|&&b| records[b] == records[v])
                        .filter(|&&b| large(b))
                    {
                        bases[v] = Some(b);
                        heirs[b].push(v);
                    }
                }
            }
            if asked {
                anchor = Some(after);
            }
        }
    }
    (bases, heirs)
}

/// Which global states the replay asks for by the timeline
/// ([`StoredFile::anchors`]): all of them when no item has a state.
fn anchors_of(
    count: usize,
    item_states: &HashMap<i64, usize>,
    item_starts: &HashMap<i64, usize>,
) -> Vec<bool> {
    if item_states.is_empty() {
        return vec![true; count];
    }
    let mut anchors = vec![false; count];
    let named = item_states.values().chain(item_starts.values());
    for &s in named.chain([0, count.saturating_sub(1)].iter()) {
        if let Some(a) = anchors.get_mut(s) {
            *a = true;
        }
    }
    anchors
}

/// The first `count` states the replay asks for by the timeline
/// (`anchors`, [`StoredFile::anchors`]), oldest first: those whose bodies
/// its first item builds ahead ([`STATES_AHEAD`] after the one it asks
/// for).
fn first_asked(anchors: &[bool], count: usize) -> Vec<usize> {
    (0..anchors.len())
        .filter(|&s| anchors[s])
        .take(count)
        .collect()
}

/// Which of `variants` (of the states the replay asks for first) are healed
/// while the file is read ([`StoredFile::heal_first_asked`]): those of
/// [`EARLY_MIN_FACES`] faces or more (`faces`) without a base (`bases`, by
/// variant) that are not healing already (`healing`), largest first.
fn heal_while_reading(
    variants: &[usize],
    faces: impl Fn(usize) -> usize,
    bases: &[Option<usize>],
    healing: &[usize],
) -> Vec<usize> {
    let mut chosen: Vec<(usize, usize)> = variants
        .iter()
        .copied()
        .filter(|&v| bases.get(v).copied().flatten().is_none() && !healing.contains(&v))
        .map(|v| (v, faces(v)))
        .filter(|&(_, f)| f >= EARLY_MIN_FACES)
        .collect();
    chosen.sort_by_key(|&(v, f)| (std::cmp::Reverse(f), v));
    chosen.into_iter().map(|(v, _)| v).collect()
}

/// A body's build shared by the tries of an import ([`StoredFile::builds`]).
enum SharedBuild {
    /// A try is building it.
    InProgress,
    /// Built by a try the watchdog gave up meanwhile: its B-rep data (None:
    /// it did not build).
    Given(Option<Arc<Vec<u8>>>),
    /// Built ahead of the replay (mitcad#103), for the try that asks for it
    /// first (None: it did not build).
    Ready(Option<SharedPtr<Shape>>),
}

/// What a try does about a body ([`StoredFile::claim`]).
enum Claim {
    /// Builds it, as no other try does.
    Build,
    /// Reads the B-rep data a try the watchdog gave up built (None: it did
    /// not build).
    Given(Option<Arc<Vec<u8>>>),
    /// Takes the body built ahead (None: it did not build).
    Ready(Option<SharedPtr<Shape>>),
}

impl StoredFile {
    /// What this try does about the body of `variant`: waits while another
    /// try builds it; takes it from a try the watchdog gave up that built
    /// it; else builds it, marked in progress until [`StoredFile::end_build`].
    /// The wait shows no progress: a build that does not return still
    /// looks hung.
    fn claim(&self, variant: usize) -> Claim {
        let mut builds = self
            .builds
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            match builds.get(&variant) {
                Some(SharedBuild::InProgress) => {
                    // A build ahead of the replay moves the watchdog's step
                    // as the try's own would (mitcad#103).
                    let ticks = BUILD_TICKS.load(Ordering::Relaxed);
                    builds = self
                        .build_ended
                        .wait_timeout(builds, Duration::from_millis(100))
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .0;
                    if BUILD_TICKS.load(Ordering::Relaxed) != ticks {
                        mitcad_import::tick();
                    }
                }
                Some(SharedBuild::Ready(_)) => {
                    if let Some(SharedBuild::Ready(shape)) = builds.remove(&variant) {
                        return Claim::Ready(shape);
                    }
                }
                Some(SharedBuild::Given(data)) => return Claim::Given(data.clone()),
                None => {
                    builds.insert(variant, SharedBuild::InProgress);
                    return Claim::Build;
                }
            }
        }
    }

    /// The end of this try's build of `variant` ([`Claim::Build`]): `given`
    /// for the other tries, or nothing (another try builds it again).
    fn end_build(&self, variant: usize, given: Option<SharedBuild>) {
        let mut builds = self
            .builds
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match given {
            Some(build) => builds.insert(variant, build),
            None => builds.remove(&variant),
        };
        drop(builds);
        self.build_ended.notify_all();
    }
}

impl StoredFile {
    /// Reads the body blobs of a document. `dump` (the decoded design)
    /// links blobs to components. `threads`: how many threads convert the
    /// bodies of a blob's states at once.
    pub fn new(
        doc: &F3dFile,
        document: &str,
        dump: Option<&Dump>,
        threads: usize,
    ) -> Result<Self, String> {
        let components: HashMap<String, u64> = dump
            .and_then(|d| d.f3d.as_ref())
            .and_then(|f| f.brep_blobs.as_ref())
            .into_iter()
            .flatten()
            .filter_map(|b| Some((b.file.clone()?, b.component.flatten()?)))
            .collect();
        let component_names = dump
            .and_then(|d| d.components.as_ref())
            .into_iter()
            .flatten()
            .filter_map(|c| {
                Some((
                    c.f3d.as_ref()?.object_id?,
                    c.name.clone().flatten().filter(|n| !n.is_empty())?,
                ))
            })
            .collect();
        let options = ConvertOptions::default();
        let mut geometry = StoredFile {
            document: document.to_owned(),
            blobs: Vec::new(),
            variants: Vec::new(),
            sources: Vec::new(),
            options,
            states: Vec::new(),
            history: Vec::new(),
            plain: Vec::new(),
            item_states: HashMap::new(),
            item_components: HashMap::new(),
            unchanged_items: HashSet::new(),
            shared_items: HashSet::new(),
            item_starts: HashMap::new(),
            component_names,
            builds: Arc::new(Mutex::new(HashMap::new())),
            build_ended: Arc::new(Condvar::new()),
            ahead: Mutex::new(std::collections::VecDeque::new()),
            anchors: Vec::new(),
            bases: Vec::new(),
            heirs: Vec::new(),
            healed: Arc::new(Mutex::new(HashMap::new())),
        };
        // The threads converting the bodies, shared with the large bodies
        // healed while the file is read (mitcad#245), which take at most
        // half of them.
        let share = mitcad_import::Share::new(threads);
        let mut early = Early {
            keep: threads.div_ceil(2),
            faces_left: EARLY_FACES,
            builds: Vec::new(),
        };
        for blob in doc.body_blobs() {
            let data = doc
                .read_large(&blob.entry)
                .map_err(|e| format!("{}: {e}", blob.entry))?;
            let file = AsmFile::parse(&data).map_err(|e| format!("{}: {e}", blob.entry))?;
            let file_name = blob
                .entry
                .rsplit('/')
                .next()
                .unwrap_or(&blob.entry)
                .to_owned();
            let index = geometry.blobs.len();
            let mut b = Blob {
                entry: blob.entry.clone(),
                asm_version: file.header.asm_version.clone(),
                component: components.get(&file_name).copied(),
                states: Vec::new(),
                ops: HashMap::new(),
                changed: HashSet::new(),
            };
            let records: Vec<usize> = convert::body_records(&file)
                .into_iter()
                .filter(|(_, top)| *top)
                .map(|(r, _)| r)
                .collect();
            let history = if blob.history {
                History::parse(&file).ok().flatten()
            } else {
                None
            };
            let source = match history {
                Some(h) => {
                    let source = Arc::new(HistorySource { file, history: h });
                    let (file, h) = (&source.file, &source.history);
                    // From the current state back: k operations undone, so
                    // the bodies after operation k. A body as it is in the
                    // state after is that state's (one variant, built and
                    // measured once): each state keeps only the bodies that
                    // differ, so the converted bodies take the memory of the
                    // distinct ones, not of every body in every state
                    // (mitcad#80: gigabytes for a long history of many
                    // bodies). Which bodies are the same is told from the
                    // history (`history_states`), as converting them all
                    // and comparing them would tell; a distinct body is
                    // converted when it is asked for (mitcad#675: converting
                    // every state's bodies was most of the time of reading a
                    // design whose pattern made 372 states the replay never
                    // asks for).
                    let states = h.states.len() + 1;
                    // The oldest state with bodies is converted first, and
                    // its large bodies are healed while the rest is read
                    // (mitcad#245): the replay's first item asks for them.
                    let first = (threads > 1)
                        .then(|| {
                            (0..states).rev().find(|&k| {
                                let view = h.view(k);
                                records.iter().any(|r| view.get(r) != Some(&None))
                            })
                        })
                        .flatten();
                    let first_bodies = first.map(|k| {
                        let view = h.view(k);
                        let bodies: Vec<ConvertedBody> = records
                            .iter()
                            .filter(|r| view.get(r) != Some(&None))
                            .map(|&r| convert::convert_body_at(file, r, &options, Some(&view)))
                            .collect();
                        (k, bodies)
                    });
                    let healing: Vec<(usize, Arc<EarlyBuild>)> = match &first_bodies {
                        Some((_, bodies)) => {
                            let source = Source {
                                document: &geometry.document,
                                blob: &b.entry,
                                asm_version: &b.asm_version,
                                history: true,
                                history_step: 0,
                            };
                            early.start(&geometry.room(), &source, bodies, &share)
                        }
                        None => Vec::new(),
                    };
                    let sets = history_states::body_sets(file, h, &records, &options, first_bodies);
                    b.changed.extend(sets.changed);
                    // The distinct bodies of the oldest state with bodies,
                    // as converted.
                    let first_ids = sets.first;
                    let newest_first = sets.newest_first;
                    let count = newest_first.len();
                    b.ops = sets
                        .ops
                        .into_iter()
                        .map(|(id, j)| (id, count - 1 - j))
                        .collect();
                    // The variants in the order of the states, oldest first.
                    let mut distinct: Vec<Option<DistinctBody>> =
                        sets.bodies.into_iter().map(Some).collect();
                    let mut variants: Vec<Option<usize>> = vec![None; distinct.len()];
                    for (time, bodies) in newest_first.into_iter().rev() {
                        let ids: Vec<usize> = bodies
                            .into_iter()
                            .map(|d| {
                                *variants[d].get_or_insert_with(|| {
                                    let body = distinct[d].take().expect("taken once");
                                    geometry.history_variant(index, body)
                                })
                            })
                            .collect();
                        b.states.push((time, ids));
                    }
                    // The variants the bodies healed while reading are of.
                    for (i, build) in healing {
                        match first_ids.as_ref().and_then(|ids| variants[*ids.get(i)?]) {
                            Some(v) => early.builds.push((v, build)),
                            None => build.unwanted(),
                        }
                    }
                    geometry.history.push(index);
                    Some(source)
                }
                None => {
                    let ids = records
                        .iter()
                        .map(|&r| {
                            let body = convert::convert_body(&file, r, &options);
                            geometry.variant(index, body)
                        })
                        .collect();
                    b.states.push((i64::MIN, ids));
                    geometry.plain.push(index);
                    None
                }
            };
            geometry.sources.push(source);
            geometry.blobs.push(b);
        }
        // The operations the timeline items made (`result_no` is the number
        // of the ASM state of the item's result, counted per component),
        // with the components that own them.
        let results: Vec<ItemResult> = dump
            .and_then(|d| d.timeline.as_ref())
            .and_then(|t| t.items.as_ref())
            .into_iter()
            .flatten()
            .filter_map(|item| {
                let f3d = item.f3d.as_ref()?;
                // A component made from bodies: the operation that put its
                // bodies into the new component (mitcad#156).
                // A tapped hole with a modelled thread: the thread's
                // operation, after the bore's (mitcad#719), whose state
                // the hole's definition with its thread makes.
                let result = f3d
                    .last_result_no
                    .or(f3d.result_no.filter(|&n| n > 0))
                    .or(f3d.made_result_no)?;
                Some(ItemResult {
                    index: item.index?,
                    result: i64::from(result),
                    owner: f3d.component,
                })
            })
            .filter(|r| r.result > 0)
            .collect();
        geometry.link_histories(&results, threads, &share);
        if !geometry.merge_by_timeline(&results) {
            geometry.merge();
        }
        geometry.link_bases(threads, &share, &mut early);
        geometry.take_early(early);
        geometry.trace_histories();
        Ok(geometry)
    }

    /// Converts the bodies of `variants` not converted yet, on up to
    /// `threads` threads at once (shared with the bodies healed while the
    /// file is read).
    fn convert_variants(&self, variants: &[usize], threads: usize, share: &mitcad_import::Share) {
        let left: Vec<usize> = variants
            .iter()
            .copied()
            .filter(|&v| self.variants[v].body.get().is_none())
            .collect();
        mitcad_import::in_order_sharing(
            left.len(),
            threads,
            Some(share),
            |i| {
                self.body(left[i]);
            },
            |_, ()| {},
        );
    }

    /// A variant's body, converted when first asked for (mitcad#675).
    fn body(&self, v: usize) -> &ConvertedBody {
        let variant = &self.variants[v];
        variant.body.get_or_init(|| {
            // (Only the variants of history blobs are made without their
            // body.)
            let source = self.sources[variant.blob]
                .as_ref()
                .expect("a variant without its body is of a history");
            let view = source.history.view(variant.step);
            convert::convert_body_at(&source.file, variant.record, &self.options, Some(&view))
        })
    }

    /// Whether a variant has the faces to reuse healed ones
    /// ([`REUSE_MIN_FACES`]): from its body when converted, else from its
    /// first faces only.
    fn large(&self, v: usize) -> bool {
        let variant = &self.variants[v];
        *variant.large.get_or_init(|| {
            if let Some(body) = variant.body.get() {
                return body.body.faces.len() >= REUSE_MIN_FACES;
            }
            let Some(source) = self.sources[variant.blob].as_ref() else {
                return self.body(v).body.faces.len() >= REUSE_MIN_FACES;
            };
            let view = source.history.view(variant.step);
            let prefix = convert::convert_body_prefix(
                &source.file,
                variant.record,
                &self.options,
                Some(&view),
                None,
                convert::Limit {
                    faces: REUSE_MIN_FACES,
                    edges: usize::MAX,
                },
            );
            match prefix {
                BodyPrefix::Whole(body) => body.body.faces.len() >= REUSE_MIN_FACES,
                // (More faces than that.)
                BodyPrefix::Part(..) => true,
            }
        })
    }

    /// Whether a variant is large, when that is known without converting
    /// anything.
    fn known_large(&self, v: usize) -> Option<bool> {
        let variant = &self.variants[v];
        variant.large.get().copied().or_else(|| {
            variant
                .body
                .get()
                .map(|b| b.body.faces.len() >= REUSE_MIN_FACES)
        })
    }

    /// Whether any of `variants` is large, those known first.
    fn any_large(&self, variants: impl Iterator<Item = usize> + Clone) -> bool {
        variants.clone().any(|v| self.known_large(v) == Some(true))
            || variants
                .into_iter()
                .any(|v| self.known_large(v).is_none() && self.large(v))
    }

    /// A variant's base ([`StoredFile::bases`]).
    fn base(&self, v: usize) -> Option<usize> {
        self.bases[v].filter(|_| self.large(v))
    }

    /// Whether a variant is the base of any other ([`StoredFile::heirs`]).
    fn has_heirs(&self, v: usize) -> bool {
        self.any_large(self.heirs[v].iter().copied())
    }

    /// Gives each variant of a history blob its base ([`StoredFile::bases`]):
    /// the variant of the same record in the blob's nearest state before
    /// the one it first appears in that the replay asks for
    /// ([`StoredFile::anchors`]; a variant appears in consecutive states
    /// only). The bodies of the states the replay asks for are converted
    /// here, on up to `threads` threads at once: those of the states it
    /// asks for first ([`first_asked`]) before the others, and their large
    /// bodies without a base are healed while the rest is read
    /// ([`StoredFile::heal_first_asked`], mitcad#676).
    fn link_bases(&mut self, threads: usize, share: &Arc<mitcad_import::Share>, early: &mut Early) {
        self.anchors = anchors_of(self.states.len(), &self.item_states, &self.item_starts);
        let variants_of = |states: &[usize]| {
            let mut variants: Vec<usize> = states
                .iter()
                .flat_map(|&s| self.state_variants(s))
                .collect();
            variants.sort_unstable();
            variants.dedup();
            variants
        };
        let first = first_asked(&self.anchors, STATES_AHEAD + 1);
        let first_variants = variants_of(&first);
        self.convert_variants(&first_variants, threads, share);
        if let Some(&last) = first.last() {
            self.heal_first_asked(&first_variants, last, share, early);
        }
        let all: Vec<usize> = (0..self.states.len())
            .filter(|&s| self.anchors[s])
            .collect();
        let asked = variants_of(&all);
        self.convert_variants(&asked, threads, share);
        let records: Vec<usize> = self.variants.iter().map(|v| v.record).collect();
        let histories = self.asked_histories(None);
        let (bases, heirs) = bases_of(&records, |b| self.large(b), &histories);
        (self.bases, self.heirs) = (bases, heirs);
    }

    /// Each history blob's states (their variants, oldest first, each with
    /// whether the replay asks for it, [`StoredFile::anchors`]), for
    /// [`bases_of`]; with `upto`, only those up to the blob's state in that
    /// global state.
    fn asked_histories(&self, upto: Option<usize>) -> Vec<Vec<(&[usize], bool)>> {
        self.history
            .iter()
            .enumerate()
            .map(|(i, &h)| {
                let mut asked = vec![false; self.blobs[h].states.len()];
                for (state, _) in self.states.iter().zip(&self.anchors).filter(|(_, a)| **a) {
                    if let Some(a) = asked.get_mut(state[i]) {
                        *a = true;
                    }
                }
                let count = upto.map_or(asked.len(), |s| self.states[s][i] + 1);
                self.blobs[h]
                    .states
                    .iter()
                    .zip(asked)
                    .take(count)
                    .map(|((_, s), a)| (&s[..], a))
                    .collect()
            })
            .collect()
    }

    /// Starts healing the large bodies of the states the replay asks for
    /// first (`variants`, converted; the states up to global state `last`)
    /// while the rest of the file is read, as those of a blob's oldest
    /// state are ([`Early`], mitcad#245): the replay's first item builds
    /// them ahead at once ([`F3dGeometry::build_ahead`]), and a large one
    /// took the first items longer than they took themselves (mitcad#676:
    /// a path pattern's own state, 8 191 faces healed in 6.7 s, waited for
    /// by its item 0.3 s into the replay). Only bodies without a base: the
    /// healing builds them as the replay would without one. Their bases
    /// come from the states up to `last` alone, as [`bases_of`] gives them
    /// from the whole history (a base is the same body in an earlier state
    /// the replay asks for).
    fn heal_first_asked(
        &self,
        variants: &[usize],
        last: usize,
        share: &Arc<mitcad_import::Share>,
        early: &mut Early,
    ) {
        let records: Vec<usize> = self.variants.iter().map(|v| v.record).collect();
        let histories = self.asked_histories(Some(last));
        let (bases, _) = bases_of(&records, |b| self.large(b), &histories);
        let healing: Vec<usize> = early.builds.iter().map(|&(v, _)| v).collect();
        let faces = |v: usize| self.body(v).body.faces.len();
        let room = self.room();
        for v in heal_while_reading(variants, faces, &bases, &healing) {
            let variant = &self.variants[v];
            let blob = &self.blobs[variant.blob];
            let source = Source {
                document: &self.document,
                blob: &blob.entry,
                asm_version: &blob.asm_version,
                history: true,
                history_step: 0,
            };
            match early.start_one(&room, &source, self.body(v), share) {
                Started::Healing(build) => early.builds.push((v, build)),
                Started::Skipped => {}
                Started::NoThread => break,
            }
        }
    }

    /// Traces each history blob's component and operations, and the state
    /// each item made (`MITCAD_IMPORT_TRACE`).
    fn trace_histories(&self) {
        if std::env::var_os("MITCAD_IMPORT_TRACE").is_none() {
            return;
        }
        for &h in &self.history {
            let blob = &self.blobs[h];
            let mut ops: Vec<i64> = blob.ops.keys().copied().collect();
            ops.sort_unstable();
            eprintln!(
                "import: history {}: component {:?}, {} body sets, operations {ops:?}",
                blob.entry,
                blob.component,
                blob.states.len()
            );
        }
        let mut items: Vec<(&i64, &usize)> = self.item_states.iter().collect();
        items.sort_unstable();
        let mut unchanged: Vec<i64> = self.unchanged_items.iter().copied().collect();
        unchanged.sort_unstable();
        let mut shared: Vec<i64> = self.shared_items.iter().copied().collect();
        shared.sort_unstable();
        eprintln!(
            "import: {} history states; item states {items:?}; items that changed no body {unchanged:?}; \
             items naming an earlier item's operation {shared:?}",
            self.states.len()
        );
        eprintln!(
            "import: {} of {} distinct bodies converted while reading",
            self.variants
                .iter()
                .filter(|v| v.body.get().is_some())
                .count(),
            self.variants.len()
        );
    }

    /// What tells a body of a blob from the others: its numbers of faces,
    /// edges and vertices and the sum of its vertices.
    fn body_key(&self, v: usize) -> BodyKey {
        let body = &self.body(v).body;
        let mut sum = [0.0; 3];
        for vertex in &body.vertices {
            for (s, p) in sum.iter_mut().zip(vertex.point) {
                *s += p;
            }
        }
        (
            body.faces.len(),
            body.edges.len(),
            body.vertices.len(),
            sum.map(|s| (s * 1e4).round() as i64),
        )
    }

    /// The solids of components stored only in `.smb` blobs (no history
    /// blob is theirs), unless one of `present` is the same body.
    fn history_less_bodies(&self, present: &[usize]) -> Vec<usize> {
        let with_history: std::collections::HashSet<u64> = self
            .history
            .iter()
            .filter_map(|&h| self.blobs[h].component)
            .collect();
        let known: std::collections::HashSet<BodyKey> =
            present.iter().map(|&v| self.body_key(v)).collect();
        self.plain
            .iter()
            .filter(|&&b| {
                self.blobs[b]
                    .component
                    .is_some_and(|c| !with_history.contains(&c))
            })
            .flat_map(|&b| self.blobs[b].states[0].1.iter().copied())
            .filter(|&v| self.body(v).body.is_solid() && !known.contains(&self.body_key(v)))
            .collect()
    }

    /// The components of the history blobs (F6). The design streams link
    /// only the `.smb` blobs to components, but every component has its
    /// own `.smb`/`.smbh` pair.
    ///
    /// The items say it first (mitcad#156): an item's result number names
    /// an operation of its owner's history (`ItemResult::owner`), so a
    /// history blob belongs to the component whose items name most of its
    /// operations, where they name more of its operations than another
    /// component's items do and more than of another blob's (the numbers
    /// count per component: inserted parts' base features are all their
    /// component's operation 3). The bodies settle the other blobs
    /// ([`StoredFile::link_by_bodies`]). By the bodies alone, a body moved
    /// into another component (a new component from bodies), which is in
    /// that component's `.smb` too, linked the history of the root, where
    /// the body was made, to the component it went to; then neither history
    /// was its component's, and every item of the design went into one
    /// component.
    fn link_histories(
        &mut self,
        items: &[ItemResult],
        threads: usize,
        share: &mitcad_import::Share,
    ) {
        let mut taken: HashSet<u64> = self
            .history
            .iter()
            .filter_map(|&h| self.blobs[h].component)
            .collect();
        let mut named: Vec<(usize, usize, u64)> = Vec::new();
        for &h in &self.history {
            if self.blobs[h].component.is_some() {
                continue;
            }
            let mut by_owner: HashMap<u64, usize> = HashMap::new();
            for item in items {
                if let Some(owner) = item.owner
                    && self.blobs[h].ops.contains_key(&item.result)
                {
                    *by_owner.entry(owner).or_default() += 1;
                }
            }
            named.extend(by_owner.into_iter().map(|(c, n)| (n, h, c)));
        }
        named.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        // Only where the items tell one blob from the others: the numbers
        // count per component, so components whose items name the same
        // numbers (every inserted part's base feature is its operation 3)
        // name every blob that has them alike, and the bodies settle those.
        for &(n, h, component) in &named {
            if self.blobs[h].component.is_some() || taken.contains(&component) {
                continue;
            }
            let rival_blob = named.iter().any(|&(m, hh, cc)| {
                cc == component && hh != h && m >= n && self.blobs[hh].component.is_none()
            });
            let rival_component = named
                .iter()
                .any(|&(m, hh, cc)| hh == h && cc != component && m >= n && !taken.contains(&cc));
            if rival_blob || rival_component {
                continue;
            }
            taken.insert(component);
            self.blobs[h].component = Some(component);
        }
        self.link_by_bodies(threads, share);
    }

    /// [`StoredFile::link_histories`] by the bodies, for the blobs no
    /// owned item names (external dumps give no owners): most `.smbh`
    /// bodies are also in the `.smb`, as they are or as they were earlier
    /// in the history (ASM_FORMAT.md), so a history blob belongs to the
    /// component of the `.smb` blob that holds most of its bodies of any
    /// state. The bodies of every state of those blobs are converted for it
    /// (on up to `threads` threads at once).
    fn link_by_bodies(&mut self, threads: usize, share: &mitcad_import::Share) {
        let key = |v: usize| self.body_key(v);
        type Key = BodyKey;
        let plain: Vec<(u64, std::collections::HashSet<Key>)> = self
            .plain
            .iter()
            .filter_map(|&b| {
                let component = self.blobs[b].component?;
                let keys = self.blobs[b].states[0].1.iter().map(|&v| key(v)).collect();
                Some((component, keys))
            })
            .collect();
        if plain.is_empty() {
            // (Nothing to link a history to.)
            return;
        }
        let unlinked: Vec<usize> = self
            .history
            .iter()
            .filter(|&&h| self.blobs[h].component.is_none())
            .flat_map(|&h| {
                self.blobs[h]
                    .states
                    .iter()
                    .flat_map(|(_, s)| s.iter().copied())
            })
            .collect();
        self.convert_variants(&unlinked, threads, share);
        // Every pairing with a body in common, the most bodies first; each
        // component has one history blob.
        let mut pairs = Vec::new();
        for &h in &self.history {
            if self.blobs[h].component.is_some() {
                continue;
            }
            let mut keys: Vec<Key> = self.blobs[h]
                .states
                .iter()
                .flat_map(|(_, variants)| variants.iter().map(|&v| key(v)))
                .collect();
            keys.sort_unstable();
            keys.dedup();
            for (component, set) in &plain {
                let score = keys.iter().filter(|k| set.contains(k)).count();
                if score > 0 {
                    pairs.push((score, h, *component));
                }
            }
        }
        pairs.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut taken: std::collections::HashSet<u64> = self
            .history
            .iter()
            .filter_map(|&h| self.blobs[h].component)
            .collect();
        for (_, h, component) in pairs {
            if self.blobs[h].component.is_none() && taken.insert(component) {
                self.blobs[h].component = Some(component);
            }
        }
    }

    fn variant(&mut self, blob: usize, body: ConvertedBody) -> usize {
        self.variants.push(Variant {
            blob,
            record: body.record,
            step: 0,
            body: OnceLock::from(body),
            large: OnceLock::new(),
        });
        self.variants.len() - 1
    }

    /// A variant of a history blob, its body converted or not yet.
    fn history_variant(&mut self, blob: usize, distinct: DistinctBody) -> usize {
        self.variants.push(Variant {
            blob,
            record: distinct.record,
            step: distinct.step,
            body: distinct.body.map_or_else(OnceLock::new, OnceLock::from),
            large: OnceLock::new(),
        });
        self.variants.len() - 1
    }

    /// The history blobs' states in the order of the timeline: each item
    /// whose result is an operation of the history moves the blobs that
    /// have it to the state after it. Operations no item names come one
    /// state each, those of a blob before the next named one, the rest at
    /// the end (components inserted from other designs bring a history of
    /// their own). False when no item names an operation.
    ///
    /// The state numbers count per component (mitcad#37: two components'
    /// histories both have an operation 3), so an item's number names the
    /// operation of the blob of the component that owns the item
    /// (`_f3d.component`) when that blob has it; else (a feature that
    /// changes another component's bodies) of the blobs that have the
    /// number and no item of their component names it, those whose bodies
    /// the operation changed (all when it changed none).
    fn merge_by_timeline(&mut self, items: &[ItemResult]) -> bool {
        let mut at: HashMap<i64, Vec<(usize, usize)>> = HashMap::new();
        for (h, &blob) in self.history.iter().enumerate() {
            for (&id, &s) in &self.blobs[blob].ops {
                at.entry(id).or_default().push((h, s));
            }
        }
        if !items.iter().any(|i| at.contains_key(&i.result)) {
            return false;
        }
        let owned_by = |item: &ItemResult, h: usize| {
            item.owner.is_some() && self.blobs[self.history[h]].component == item.owner
        };
        // The operations items name in their owners' blobs: another item
        // whose owner's blob lacks its number (a feature that changes
        // another component's bodies) does not take them.
        let claimed: HashSet<(usize, i64)> = items
            .iter()
            .flat_map(|item| {
                at.get(&item.result)
                    .into_iter()
                    .flatten()
                    .filter(|&&(h, _)| owned_by(item, h))
                    .map(|&(h, _)| (h, item.result))
            })
            .collect();
        let mut current = vec![0usize; self.history.len()];
        self.states.push(current.clone());
        // The operations an earlier item took (blob, number).
        let mut named: HashSet<(usize, i64)> = HashSet::new();
        // Per blob, the last operation an earlier item named.
        let mut last_named: Vec<Option<i64>> = vec![None; self.history.len()];
        for item in items {
            let (index, result) = (item.index, item.result);
            let Some(all) = at.get(&result) else {
                continue;
            };
            let blob_of = |h: usize| &self.blobs[self.history[h]];
            let owned: Vec<(usize, usize)> = all
                .iter()
                .copied()
                .filter(|&(h, _)| owned_by(item, h))
                .collect();
            let free: Vec<(usize, usize)> = all
                .iter()
                .copied()
                .filter(|&(h, _)| !claimed.contains(&(h, result)))
                .collect();
            let changed: Vec<(usize, usize)> = free
                .iter()
                .copied()
                .filter(|&(h, _)| blob_of(h).changed.contains(&result))
                .collect();
            let targets = if !owned.is_empty() {
                owned
            } else if free.len() > 1 && !changed.is_empty() {
                changed
            } else if !free.is_empty() {
                free
            } else {
                all.clone()
            };
            let mut components: Vec<u64> = targets
                .iter()
                .filter_map(|&(h, _)| blob_of(h).component)
                .collect();
            components.sort_unstable();
            components.dedup();
            self.item_components.insert(index, components);
            if !targets
                .iter()
                .any(|&(h, _)| item_changed(&blob_of(h).changed, last_named[h], result))
            {
                self.unchanged_items.insert(index);
            }
            for &(h, _) in &targets {
                last_named[h] = Some(last_named[h].map_or(result, |n| n.max(result)));
            }
            // An item that names the operation an earlier item took has none
            // of its own (mitcad#343: a chamfer after a rounding, both naming
            // the rounding's operation; the chamfer's edges the file lost).
            if targets.iter().all(|&(h, _)| named.contains(&(h, result))) {
                self.shared_items.insert(index);
            }
            named.extend(targets.iter().map(|&(h, _)| (h, result)));
            let mut moved = false;
            let mut back = false;
            let begun = self.states.len() - 1;
            for &(h, s) in &targets {
                while current[h] + 1 < s {
                    current[h] += 1;
                    self.states.push(current.clone());
                }
                if current[h] < s {
                    current[h] = s;
                    moved = true;
                }
                back |= s < current[h];
            }
            if moved {
                self.states.push(current.clone());
            }
            if !back {
                self.item_states.insert(index, self.states.len() - 1);
                // Operations of its blob no item names right before its
                // own: maybe its own (mitcad#156).
                if targets.len() == 1 && self.states.len() - 1 > begun + 1 {
                    self.item_starts.insert(index, begun);
                }
            }
        }
        for h in 0..self.history.len() {
            let last = self.blobs[self.history[h]].states.len() - 1;
            while current[h] < last {
                current[h] += 1;
                self.states.push(current.clone());
            }
        }
        true
    }

    /// The history blobs' states in the order of their state numbers.
    fn merge(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let mut current = vec![0usize; self.history.len()];
        self.states.push(current.clone());
        let mut events: Vec<(i64, usize, usize)> = Vec::new();
        for (h, &blob) in self.history.iter().enumerate() {
            // A blob's own order wins over numbers that go back (a newest
            // state numbered 0).
            let mut last = i64::MIN;
            for (s, (time, _)) in self.blobs[blob].states.iter().enumerate().skip(1) {
                last = last.max(*time);
                events.push((last, h, s));
            }
        }
        events.sort();
        let mut i = 0;
        while i < events.len() {
            let time = events[i].0;
            while i < events.len() && events[i].0 == time {
                current[events[i].1] = events[i].2;
                i += 1;
            }
            self.states.push(current.clone());
        }
    }

    /// Whether the conversion of a variant lost faces: faces left out,
    /// broken topology, or a sheet where the stored design has a solid (a
    /// roll-back the history decoder does not complete). A body that is so
    /// in the stored design too makes its states unknown as well: it is
    /// mostly a solid of the file's that the conversion does not close, so a
    /// state without it would mislead (T1b tried that: the main body of a
    /// corpus design).
    fn lost_faces(&self, v: usize) -> bool {
        let body = self.body(v);
        let check = &body.check;
        // Edge ends up to 0.01 mm off their vertices (rolled-back B-spline
        // edges of some intermediate states) only widen the tolerances.
        let gaps = check.vertex_mismatches > 0 && check.max_vertex_gap > MAX_VERTEX_GAP;
        if body.skipped_faces > 0 || check.unpaired_coedges > 0 || check.open_loops > 0 || gaps {
            return true;
        }
        if body.body.is_solid() {
            return false;
        }
        let blob = &self.blobs[self.variants[v].blob];
        blob.states.last().is_some_and(|(_, last)| {
            last.iter()
                .any(|&w| self.variants[w].record == body.record && self.body(w).body.is_solid())
        })
    }

    /// The variants of a global state.
    fn state_variants(&self, index: usize) -> Vec<usize> {
        self.history
            .iter()
            .zip(&self.states[index])
            .flat_map(|(&blob, &s)| self.blobs[blob].states[s].1.iter().copied())
            .collect()
    }
}

thread_local! {
    /// While a body is built: when it last made progress, and the longest
    /// time it made none so far (for the trace).
    static BUILD_PROGRESS: Cell<Option<(Instant, f64)>> = const { Cell::new(None) };
}

/// The build of a body advanced (`f3d_build_body`'s healing, face by face):
/// the import's progress, so that its watchdog takes only a kernel call
/// that does not return for a hang (mitcad#82: a body of 23 000 faces took
/// 84 s to heal).
pub(crate) fn build_progress() {
    mitcad_import::tick();
    BUILD_TICKS.fetch_add(1, Ordering::Relaxed);
    BUILD_PROGRESS.with(|p| {
        if let Some((last, longest)) = p.get() {
            let now = Instant::now();
            p.set(Some((now, longest.max((now - last).as_secs_f64()))));
        }
    });
}

/// A build this try claimed ([`Claim::Build`]): it ends when this is
/// dropped (also on a panic), with `given` for the other tries.
struct Claimed<'f> {
    file: &'f StoredFile,
    variant: usize,
    given: Option<SharedBuild>,
}

impl Drop for Claimed<'_> {
    fn drop(&mut self) {
        self.file.end_build(self.variant, self.given.take());
    }
}

/// The bodies of a document of an .f3d or .f3z file for one try of its
/// import: the file's bodies ([`StoredFile`], shared by the tries) built
/// with OCCT as the try asks for them.
pub struct F3dGeometry {
    file: Arc<StoredFile>,
    /// Per variant, its shape once built (None inside: it did not build).
    built: Vec<Option<Option<SharedPtr<Shape>>>>,
}

impl F3dGeometry {
    pub fn new(file: Arc<StoredFile>) -> Self {
        let built = vec![None; file.variants.len()];
        Self { file, built }
    }

    /// The shape of a variant: built once by this try, or by another one
    /// ([`StoredFile::claim`], mitcad#82); None when it does not build.
    fn build(&mut self, variant: usize) -> Option<SharedPtr<Shape>> {
        if let Some(built) = &self.built[variant] {
            return built.clone();
        }
        let file = Arc::clone(&self.file);
        let shape = match file.claim(variant) {
            Claim::Build => {
                let mut claimed = Claimed {
                    file: &file,
                    variant,
                    given: None,
                };
                let shape = self.heal(variant);
                // Given up meanwhile: the next try waits for this build
                // rather than making its own (what it made is the same).
                if mitcad_import::abandoned() {
                    claimed.given = match &shape {
                        Some(s) => brep_data(s)
                            .ok()
                            .map(|data| SharedBuild::Given(Some(Arc::new(data)))),
                        None => Some(SharedBuild::Given(None)),
                    };
                }
                shape
            }
            Claim::Given(Some(data)) => {
                let shape = f3d_read_body(&data);
                self.trace_body(variant, "read from the build of a try given up");
                if shape.is_null() {
                    self.heal(variant)
                } else {
                    Some(shape)
                }
            }
            Claim::Given(None) => None,
            Claim::Ready(shape) => {
                self.trace_body(variant, "built ahead");
                shape
            }
        };
        self.built[variant] = Some(shape.clone());
        shape
    }

    /// Builds the shape of a variant from the file's data with OCCT.
    fn heal(&self, variant: usize) -> Option<SharedPtr<Shape>> {
        self.file.heal(variant)
    }

    /// Traces what happened to a variant's body (`MITCAD_IMPORT_TRACE`).
    fn trace_body(&self, variant: usize, what: &str) {
        trace_body(&self.file, variant, what);
    }

    /// The bodies of variants that build, with the number of those that do
    /// not (or lost faces in the conversion).
    fn bodies(&mut self, variants: &[usize]) -> (Vec<StoredBody<SharedPtr<Shape>>>, usize) {
        let mut broken = 0;
        let mut bodies = Vec::new();
        for &v in variants {
            // Building a large state's hundreds of bodies takes minutes, a
            // kernel call each: the watchdog sees them (mitcad#82). A try it
            // gave up builds no more (what it makes is dropped).
            mitcad_import::tick();
            if mitcad_import::abandoned() {
                broken += 1;
                continue;
            }
            let Some(shape) = self.build(v) else {
                broken += 1;
                continue;
            };
            let file = &self.file;
            let variant = &file.variants[v];
            if file.lost_faces(v) {
                broken += 1;
            }
            let blob = &file.blobs[variant.blob];
            let name = blob.entry.rsplit('/').next().unwrap_or(&blob.entry);
            bodies.push(StoredBody {
                shape,
                name: None,
                source: format!("{name}#{}", variant.record),
                component: blob.component,
                id: Some(v as u64),
                // (A body keeps its record through the rolled-back states
                // of its blob, also where an operation changed it.)
                lineage: Some(((variant.blob as u64) << 32) | variant.record as u64),
            });
        }
        (bodies, broken)
    }
}

/// Traces what happened to a variant's body (`MITCAD_IMPORT_TRACE`).
fn trace_body(file: &StoredFile, variant: usize, what: &str) {
    if std::env::var_os("MITCAD_IMPORT_TRACE").is_none() {
        return;
    }
    let v = &file.variants[variant];
    let entry = &file.blobs[v.blob].entry;
    eprintln!(
        "import: body {}#{} ({} faces) {what}",
        entry.rsplit('/').next().unwrap_or(entry),
        v.record,
        file.body(variant).body.faces.len()
    );
}

impl StoredFile {
    /// Builds the shape of a variant from the file's data with OCCT.
    fn heal(&self, variant: usize) -> Option<SharedPtr<Shape>> {
        let file = self;
        let v = &file.variants[variant];
        let blob = &file.blobs[v.blob];
        let source = Source {
            document: &file.document,
            blob: &blob.entry,
            asm_version: &blob.asm_version,
            history: !file.plain.contains(&v.blob),
            history_step: 0,
        };
        let data = to_ffi(&source, self.body(variant));
        // (Before the clock: waiting for the base is not this build's.)
        let base = self.base_build(variant);
        let clock = Instant::now();
        BUILD_PROGRESS.with(|p| p.set(Some((clock, 0.0))));
        // A build longer than the hang limit that returns, for the tests
        // (mitcad#82): the file's first body takes 1.5 s, without progress.
        if variant == 0 && std::env::var_os("MITCAD_IMPORT_STALL").is_some_and(|s| s == "slow") {
            std::thread::sleep(Duration::from_millis(1500));
        }
        let (shape, how) = self.build_reusing(variant, &data, base.as_ref());
        let seconds = clock.elapsed().as_secs_f64();
        let still = BUILD_PROGRESS
            .with(Cell::take)
            .map_or(seconds, |(last, longest)| {
                longest.max(last.elapsed().as_secs_f64())
            });
        if seconds >= 1.0 || !how.is_empty() {
            trace_body(
                self,
                variant,
                &format!("built in {seconds:.2} s, at most {still:.2} s without progress{how}"),
            );
        }
        shape
    }

    /// What the build of the base of `variant` ([`StoredFile::bases`])
    /// kept for it (None: it has no base, or its base's build kept
    /// nothing). Waits while another thread builds the base (ticking as
    /// [`StoredFile::claim`] does), and builds the base itself when no
    /// build of it kept anything for its heirs yet or any more (a state
    /// the replay skipped, or one released): the base is the same whatever
    /// the order of the builds, so the body is too (mitcad#243).
    fn base_build(&self, variant: usize) -> Option<SharedPtr<F3dBuilt>> {
        let base = self.base(variant)?;
        let mut builds = self
            .builds
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            // (Under `builds`: a build records what it kept before it
            // ends there.)
            if let Some(kept) = self
                .healed
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&base)
            {
                return kept.clone();
            }
            match builds.get(&base) {
                Some(SharedBuild::InProgress) => {
                    let ticks = BUILD_TICKS.load(Ordering::Relaxed);
                    builds = self
                        .build_ended
                        .wait_timeout(builds, Duration::from_millis(100))
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .0;
                    if BUILD_TICKS.load(Ordering::Relaxed) != ticks {
                        mitcad_import::tick();
                    }
                }
                Some(_) => {
                    // Built and handed on, but what it kept went with its
                    // heirs' builds: built again for what it keeps.
                    drop(builds);
                    trace_body(self, base, "built again as the base of the next state");
                    let _ = self.heal(base);
                    builds = self
                        .builds
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
                None => {
                    // Not built yet: built here, and handed on as a body
                    // built ahead in case the replay asks for it.
                    builds.insert(base, SharedBuild::InProgress);
                    drop(builds);
                    trace_body(self, base, "built as the base of the next state");
                    let mut claimed = Claimed {
                        file: self,
                        variant: base,
                        given: None,
                    };
                    claimed.given = Some(SharedBuild::Ready(self.heal(base)));
                    drop(claimed);
                    builds = self
                        .builds
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
            }
        }
    }

    /// Builds a body (`variant`) from its B-rep data with the faces it
    /// shares with its base's build (`base`) as healed there (mitcad#181),
    /// keeping what its heirs' builds take from it
    /// ([`StoredFile::healed`]); the shape and, for the trace, how many
    /// faces were taken (or why none) and whether it was kept. As
    /// [`build_stored_body`] when an allocation failed.
    fn build_reusing(
        &self,
        variant: usize,
        data: &crate::brep_import::ffi::BrepBodyData,
        base: Option<&SharedPtr<F3dBuilt>>,
    ) -> (Option<SharedPtr<Shape>>, String) {
        let mut bases = f3d_bases();
        if let Some(base) = base {
            f3d_bases_add(bases.pin_mut(), base);
        }
        let keep = self.has_heirs(variant);
        let built = match f3d_build_body_from(data, &bases, keep) {
            Ok(built) => Some(built),
            Err(e) => {
                if let Some(progress) = mitcad_import::current_progress() {
                    progress.low_memory(&format!(
                        "{} ({})",
                        e.what(),
                        crate::memory::memory_use().describe()
                    ));
                }
                None
            }
        };
        let built = built.filter(|b| b.as_ref().is_some_and(|b| !f3d_built_shape(b).is_null()));
        let kept = built
            .as_ref()
            .filter(|b| b.as_ref().is_some_and(f3d_built_kept))
            .cloned();
        self.record_build(variant, keep.then_some(kept.clone()));
        let Some(b) = built.as_ref().and_then(|b| b.as_ref()) else {
            return (None, String::new());
        };
        let shape = f3d_built_shape(b);
        let reused = f3d_built_reused(b);
        let note = f3d_built_note(b);
        let mut how = String::new();
        if reused > 0 {
            how.push_str(&format!(", {reused} faces as healed in another state"));
        }
        if !note.is_empty() {
            how.push_str(&format!(", {note}"));
        }
        if kept.is_some() {
            how.push_str(", kept for the next state");
        }
        (Some(shape), how)
    }

    /// Records the build of `variant`: what it kept for its heirs (`kept`;
    /// None when it has none), and that its base's build is no longer
    /// needed when this was the base's only heir.
    fn record_build(&self, variant: usize, kept: Option<Option<SharedPtr<F3dBuilt>>>) {
        // (Told before the lock: telling whether another heir is large may
        // convert its first faces.)
        let only_heir_of = self.base(variant).filter(|&base| {
            !self.any_large(self.heirs[base].iter().copied().filter(|&h| h != variant))
        });
        let mut healed = self
            .healed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(kept) = kept {
            healed.insert(variant, kept);
        }
        if let Some(base) = only_heir_of {
            healed.remove(&base);
        }
    }
}

/// Builds a body of the file from its B-rep data with OCCT
/// (`f3d_build_body`); None when it does not build. When an allocation
/// failed meanwhile (mitcad#132, also one OCCT's healing or checker caught
/// inside), the import running on this thread is low on memory.
pub(crate) fn build_stored_body(
    data: &crate::brep_import::ffi::BrepBodyData,
) -> Option<SharedPtr<Shape>> {
    match f3d_build_body(data) {
        Ok(shape) => (!shape.is_null()).then_some(shape),
        Err(e) => {
            if let Some(progress) = mitcad_import::current_progress() {
                progress.low_memory(&format!(
                    "{} ({})",
                    e.what(),
                    crate::memory::memory_use().describe()
                ));
            }
            None
        }
    }
}

// Large bodies healed while the file is read (mitcad#245).

/// The smallest body (faces) healed while the file is read: smaller ones
/// take the replay little time.
const EARLY_MIN_FACES: usize = 1000;

/// The faces of all bodies healed while the file is read: the shapes wait
/// for the replay in memory.
const EARLY_FACES: usize = 100_000;

/// The builds of a file's bodies ([`StoredFile::builds`],
/// [`StoredFile::build_ended`], [`StoredFile::healed`]), shared with the
/// threads healing bodies while the file is read.
#[derive(Clone)]
struct Room {
    builds: Arc<Mutex<HashMap<usize, SharedBuild>>>,
    build_ended: Arc<Condvar>,
    healed: Arc<Mutex<HashMap<usize, Option<SharedPtr<F3dBuilt>>>>>,
}

impl Room {
    /// Hands a body healed while the file was read to the replay as the
    /// build of `variant` (as a build ahead's, with what it kept for its
    /// heirs when `keep`); one that did not build is left to the replay.
    fn hand_over(&self, variant: usize, keep: bool, built: Option<SharedPtr<F3dBuilt>>) {
        let shape = built
            .as_ref()
            .and_then(|b| b.as_ref())
            .map(f3d_built_shape)
            .filter(|s| !s.is_null());
        // (What it kept first, as a build records it before it ends.)
        if keep && shape.is_some() {
            let kept = built.filter(|b| b.as_ref().is_some_and(f3d_built_kept));
            self.healed
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(variant, kept);
        }
        let mut builds = self
            .builds
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match shape {
            Some(shape) => builds.insert(variant, SharedBuild::Ready(Some(shape))),
            None => builds.remove(&variant),
        };
        drop(builds);
        self.build_ended.notify_all();
    }
}

/// Where a body healed while the file is read stands ([`EarlyBuild`]).
enum EarlyState {
    /// Being healed; its variant not known yet.
    Healing,
    /// Being healed for the variant (marked in progress in the builds).
    Wanted { variant: usize, keep: bool },
    /// Healed (None: it did not build); its variant not known yet.
    Healed(Option<SharedPtr<F3dBuilt>>),
    /// Not wanted (or handed over).
    Unwanted,
}

/// A large body of a blob's oldest state with bodies, healed on a thread of
/// its own while the rest of the file is read and handed to the replay as a
/// body built ahead: the replay's first item asks for it, and healing the
/// large bodies of a design took the first item as long as reading the
/// whole file. It is built from the same data as the replay would build it
/// (no base: no older state has the body), so the body is the same. So is a
/// large body without a base of the states the replay asks for first
/// ([`StoredFile::heal_first_asked`], mitcad#676).
struct EarlyBuild {
    record: usize,
    top_level: bool,
    issues: Vec<String>,
    skipped_faces: usize,
    state: Mutex<EarlyState>,
}

impl EarlyBuild {
    fn lock(&self) -> std::sync::MutexGuard<'_, EarlyState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether `body` (the variant's) is the body this healed: the
    /// conversion of the same state gives the same body; the rest of the
    /// data built from must be the same too.
    fn matches(&self, body: &ConvertedBody) -> bool {
        body.record == self.record
            && body.top_level == self.top_level
            && body.issues == self.issues
            && body.skipped_faces == self.skipped_faces
    }

    /// The healing ended (`built`).
    fn healed(&self, built: Option<SharedPtr<F3dBuilt>>, room: &Room) {
        let mut state = self.lock();
        match std::mem::replace(&mut *state, EarlyState::Unwanted) {
            EarlyState::Healing => *state = EarlyState::Healed(built),
            EarlyState::Wanted { variant, keep } => {
                drop(state);
                room.hand_over(variant, keep, built);
            }
            EarlyState::Healed(_) | EarlyState::Unwanted => {}
        }
    }

    /// The body is the build of `variant`: handed over when healed, and
    /// marked in progress until then (a try asking for it waits).
    fn want(&self, variant: usize, keep: bool, room: &Room) {
        let mut state = self.lock();
        match std::mem::replace(&mut *state, EarlyState::Unwanted) {
            EarlyState::Healing => {
                room.builds
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(variant, SharedBuild::InProgress);
                *state = EarlyState::Wanted { variant, keep };
            }
            EarlyState::Healed(built) => {
                drop(state);
                room.hand_over(variant, keep, built);
            }
            EarlyState::Wanted { .. } | EarlyState::Unwanted => {}
        }
    }

    /// The body is no build's: dropped once healed.
    fn unwanted(&self) {
        *self.lock() = EarlyState::Unwanted;
    }
}

/// The bodies healed while a file is read ([`StoredFile::new`]).
struct Early {
    /// The converting threads the healing leaves ([`mitcad_import::Share::take`]).
    keep: usize,
    /// The faces that may still be healed ([`EARLY_FACES`]).
    faces_left: usize,
    /// Those healing, by variant.
    builds: Vec<(usize, Arc<EarlyBuild>)>,
}

impl Early {
    /// Starts healing the large `bodies` of a blob's oldest state with
    /// bodies, largest first, each on a thread taken from the converting
    /// ones (`share`) while enough are left; the healings, by the index of
    /// their body.
    fn start(
        &mut self,
        room: &Room,
        source: &Source,
        bodies: &[ConvertedBody],
        share: &Arc<mitcad_import::Share>,
    ) -> Vec<(usize, Arc<EarlyBuild>)> {
        let mut large: Vec<usize> = (0..bodies.len())
            .filter(|&i| bodies[i].body.faces.len() >= EARLY_MIN_FACES)
            .collect();
        large.sort_by_key(|&i| std::cmp::Reverse(bodies[i].body.faces.len()));
        let mut started = Vec::new();
        for i in large {
            match self.start_one(room, source, &bodies[i], share) {
                Started::Healing(build) => started.push((i, build)),
                Started::Skipped => {}
                Started::NoThread => break,
            }
        }
        started
    }

    /// Starts healing `body` on a thread taken from the converting ones
    /// (`share`) when its faces fit in what is left of [`EARLY_FACES`] and
    /// a thread is left.
    fn start_one(
        &mut self,
        room: &Room,
        source: &Source,
        body: &ConvertedBody,
        share: &Arc<mitcad_import::Share>,
    ) -> Started {
        let faces = body.body.faces.len();
        if faces > self.faces_left {
            return Started::Skipped;
        }
        let Some(taken) = share.take(self.keep) else {
            return Started::NoThread;
        };
        let build = Arc::new(EarlyBuild {
            record: body.record,
            top_level: body.top_level,
            issues: body.issues.clone(),
            skipped_faces: body.skipped_faces,
            state: Mutex::new(EarlyState::Healing),
        });
        let data = to_ffi(source, body);
        let name = format!(
            "{}#{} ({faces} faces)",
            source.blob.rsplit('/').next().unwrap_or(source.blob),
            body.record
        );
        let (healing, room) = (Arc::clone(&build), room.clone());
        let counted = BuildingAhead::new();
        let thread = std::thread::Builder::new()
            .name("mitcad-import-bodies".to_owned())
            .stack_size(mitcad_import::WORKER_STACK)
            .spawn(move || {
                let _counted = counted;
                crate::kernel::exchange::ffi::catch_occt_crashes();
                let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    heal_early(&data, &name)
                }))
                .ok()
                .flatten();
                drop(taken);
                healing.healed(built, &room);
            });
        if thread.is_err() {
            return Started::Skipped;
        }
        self.faces_left -= faces;
        Started::Healing(build)
    }
}

/// Whether [`Early::start_one`] started healing a body.
enum Started {
    Healing(Arc<EarlyBuild>),
    /// Not started: its faces do not fit in what is left of
    /// [`EARLY_FACES`], or its thread did not start.
    Skipped,
    /// No thread is left.
    NoThread,
}

/// Heals a body from its data as the replay's build of it would (no base;
/// keeping what its heirs take, which leaves the body the same), measured
/// as a body built ahead; None when it does not build or an allocation
/// failed (the replay then builds it).
fn heal_early(
    data: &crate::brep_import::ffi::BrepBodyData,
    name: &str,
) -> Option<SharedPtr<F3dBuilt>> {
    let clock = Instant::now();
    BUILD_PROGRESS.with(|p| p.set(Some((clock, 0.0))));
    let built = f3d_build_body_from(data, &f3d_bases(), true).ok()?;
    let shape = built
        .as_ref()
        .map(f3d_built_shape)
        .filter(|s| !s.is_null())?;
    if let Some(s) = shape.as_ref() {
        // (The shape keeps its measures for the replay.)
        let _ = crate::kernel::query::ffi::mass_properties(s);
    }
    if std::env::var_os("MITCAD_IMPORT_TRACE").is_some() {
        let seconds = clock.elapsed().as_secs_f64();
        let still = BUILD_PROGRESS
            .with(Cell::take)
            .map_or(seconds, |(last, longest)| {
                longest.max(last.elapsed().as_secs_f64())
            });
        let note = built.as_ref().map(f3d_built_note).unwrap_or_default();
        eprintln!(
            "import: body {name} healed while the file was read in {seconds:.2} s, \
             at most {still:.2} s without progress{}{note}",
            if note.is_empty() { "" } else { ", " }
        );
    }
    Some(built)
}

impl StoredFile {
    /// What the threads healing bodies share with the replay's builds.
    fn room(&self) -> Room {
        Room {
            builds: Arc::clone(&self.builds),
            build_ended: Arc::clone(&self.build_ended),
            healed: Arc::clone(&self.healed),
        }
    }

    /// Gives the bodies healed while the file was read to their variants
    /// (once the bases and heirs are known): the body of a variant without
    /// a base built from the same data; any other is dropped.
    fn take_early(&self, early: Early) {
        let room = self.room();
        for (variant, build) in early.builds {
            if self.base(variant).is_none() && build.matches(self.body(variant)) {
                build.want(variant, self.has_heirs(variant), &room);
            } else {
                build.unwanted();
            }
        }
    }
}

// The history's bodies built ahead of the replay (mitcad#103).

/// How many history states after the one the replay asks for have their
/// bodies built ahead (of those it asks for by the timeline,
/// [`StoredFile::anchors`]).
const STATES_AHEAD: usize = 4;

/// Moves whenever a body's build makes progress, on any thread: a try
/// waiting for a build ahead of it ticks its watchdog with it.
static BUILD_TICKS: AtomicU64 = AtomicU64::new(0);

/// Threads building bodies ahead that still run (counted in
/// [`abandoned_imports`]: the process must not run its static destructors
/// under them).
static BUILDING_AHEAD: AtomicUsize = AtomicUsize::new(0);

/// Counts a thread in [`BUILDING_AHEAD`] until dropped.
struct BuildingAhead;

impl BuildingAhead {
    fn new() -> Self {
        BUILDING_AHEAD.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for BuildingAhead {
    fn drop(&mut self) {
        BUILDING_AHEAD.fetch_sub(1, Ordering::SeqCst);
    }
}

impl StoredFile {
    /// Claims `variant` for a build ahead: false when it is built or being
    /// built already.
    fn claim_ahead(&self, variant: usize) -> bool {
        let mut builds = self
            .builds
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if builds.contains_key(&variant) {
            return false;
        }
        builds.insert(variant, SharedBuild::InProgress);
        true
    }

    /// Builds the bodies queued in [`StoredFile::ahead`] while the import
    /// has a thread for it ([`mitcad_import::Progress::helper`]), each
    /// measured as the replay measures it (the shape keeps what it
    /// measured): what the replay then asks for is there.
    fn build_queued(self: &Arc<Self>, progress: &Arc<mitcad_import::Progress>) {
        // (A thread per body queued at most; those running take some.)
        let queued = self
            .ahead
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len();
        for _ in 0..queued {
            let Some(helper) = progress.helper() else {
                return;
            };
            let (file, progress_owned) = (Arc::clone(self), Arc::clone(progress));
            let counted = BuildingAhead::new();
            let started = std::thread::Builder::new()
                .name("mitcad-import-bodies".to_owned())
                .stack_size(mitcad_import::WORKER_STACK)
                .spawn(move || {
                    let _counted = counted;
                    let _helper = helper;
                    crate::kernel::exchange::ffi::catch_occt_crashes();
                    // Its builds' progress is the import's (the watchdog's).
                    let _ticking = mitcad_import::Ticking::new(Some(Arc::clone(&progress_owned)));
                    file.build_ahead_loop(&progress_owned);
                });
            if started.is_err() {
                return;
            }
        }
    }

    /// A thread's builds ahead, until the queue is empty or the import
    /// needs the memory or was given up.
    fn build_ahead_loop(&self, progress: &mitcad_import::Progress) {
        loop {
            if progress.abandoned()
                || progress.is_memory_low()
                || progress.memory_share() >= AHEAD_BODIES_MEMORY
            {
                return;
            }
            let next = self
                .ahead
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .pop_front();
            let Some(variant) = next else {
                return;
            };
            if !self.claim_ahead(variant) {
                continue;
            }
            let mut claimed = Claimed {
                file: self,
                variant,
                given: None,
            };
            let shape = self.heal(variant);
            if let Some(s) = shape.as_ref().and_then(|s| s.as_ref()) {
                // (The shape keeps its measures for the replay.)
                let _ = crate::kernel::query::ffi::mass_properties(s);
            }
            claimed.given = Some(SharedBuild::Ready(shape));
        }
    }

    /// Drops the bodies built ahead that no try took, except `kept`.
    fn release_ahead(&self, kept: &[bool]) {
        let mut builds = self
            .builds
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        builds.retain(|&v, b| !matches!(b, SharedBuild::Ready(_)) || kept.get(v) == Some(&true));
    }
}

/// The share of the tightest memory limit above which no bodies are built
/// ahead, as for definitions evaluated ahead (`mitcad_import`'s helpers).
const AHEAD_BODIES_MEMORY: f64 = 0.5;

impl F3dGeometry {
    /// Builds the bodies of the states after `state` that this try has not
    /// built yet on threads of the import's budget (mitcad#103): the
    /// replay asks for them next, and building a large body takes seconds
    /// to minutes. The bodies are the same as when the replay builds them.
    fn build_ahead(&self, state: usize) {
        let Some(progress) = mitcad_import::current_progress() else {
            return;
        };
        let file = &self.file;
        let last = file.states.len();
        {
            let mut queue = file
                .ahead
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            // (The states the replay asks for by the timeline: those
            // between, such as a pattern's copies, it mostly skips.)
            let next = (state + 1..last).filter(|&s| file.anchors.get(s) != Some(&false));
            for s in next.take(STATES_AHEAD) {
                for v in file.state_variants(s) {
                    if self.built[v].is_none() && !queue.contains(&v) {
                        queue.push_back(v);
                    }
                }
            }
        }
        file.build_queued(&progress);
    }
}

impl StoredGeometry<SharedPtr<Shape>> for F3dGeometry {
    fn state_count(&mut self) -> usize {
        self.file.states.len()
    }

    fn item_state(&mut self, index: i64) -> Option<usize> {
        self.file.item_states.get(&index).copied()
    }

    fn item_components(&mut self, index: i64) -> Option<Vec<u64>> {
        self.file
            .item_components
            .get(&index)
            .filter(|c| !c.is_empty())
            .cloned()
    }

    fn component_names(&mut self) -> HashMap<u64, String> {
        self.file.component_names.clone()
    }

    fn item_unchanged(&mut self, index: i64) -> bool {
        self.file.unchanged_items.contains(&index)
    }

    fn item_start(&mut self, index: i64) -> Option<usize> {
        self.file.item_starts.get(&index).copied()
    }

    fn item_shares_operation(&mut self, index: i64) -> bool {
        self.file.shared_items.contains(&index)
    }

    fn history_components(&mut self) -> Option<HashSet<u64>> {
        // (Without a history the stored design's blobs name theirs.)
        let file = &self.file;
        (!file.history.is_empty()).then(|| {
            file.history
                .iter()
                .filter_map(|&h| file.blobs[h].component)
                .collect()
        })
    }

    /// The shapes built for bodies only the states before `state` have;
    /// the bodies of the stored design stay.
    fn release_before(&mut self, state: usize) {
        let file = &self.file;
        let mut kept = vec![false; file.variants.len()];
        for s in state..file.states.len() {
            for v in file.state_variants(s) {
                kept[v] = true;
            }
        }
        for &b in &file.plain {
            for &v in &file.blobs[b].states[0].1 {
                kept[v] = true;
            }
        }
        file.release_ahead(&kept);
        // What builds kept for heirs no later state has goes too (an heir
        // asked for again builds its base again, mitcad#243).
        // (Told before the lock: telling whether an heir is large may
        // convert its first faces.)
        let held: Vec<usize> = file
            .healed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .keys()
            .copied()
            .collect();
        let gone: HashSet<usize> = held
            .into_iter()
            .filter(|&b| !file.any_large(file.heirs[b].iter().copied().filter(|&h| kept[h])))
            .collect();
        file.healed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|b, _| !gone.contains(b));
        // (A body that did not build is not tried again.)
        for (built, kept) in self.built.iter_mut().zip(kept) {
            if !kept && matches!(built, Some(Some(_))) {
                *built = None;
            }
        }
    }

    fn state(&mut self, index: usize) -> Result<Vec<StoredBody<SharedPtr<Shape>>>, String> {
        if index >= self.file.states.len() {
            return Err(format!("there is no history state {index}"));
        }
        let variants = self.file.state_variants(index);
        self.build_ahead(index);
        match self.bodies(&variants) {
            (bodies, 0) => Ok(bodies),
            (_, broken) => Err(format!(
                "{broken} of the bodies of history state {index} could not be rebuilt"
            )),
        }
    }

    /// The last history state's bodies (those that build) and the bodies
    /// of components stored only in `.smb` blobs; without a history, the
    /// `.smb` blobs' top-level bodies. (With a history the `.smb` blobs of
    /// components with a history also hold bodies that are not the
    /// design's, such as feature tools and sketch faces; a component
    /// without one has no features, so its `.smb` holds its bodies.)
    fn final_bodies(&mut self) -> Result<Vec<StoredBody<SharedPtr<Shape>>>, String> {
        let file = &self.file;
        let variants = match file.states.len().checked_sub(1) {
            Some(last) => {
                let mut variants = file.state_variants(last);
                variants.extend(file.history_less_bodies(&variants));
                variants
            }
            None => file
                .plain
                .iter()
                .flat_map(|&b| file.blobs[b].states[0].1.iter().copied())
                .collect(),
        };
        Ok(self.bodies(&variants).0)
    }
}

/// The JSON of the `import_f3d` command with the timeline.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineOptions {
    /// The document of an .f3z package to import (its label or a prefix of
    /// it); when left out, the root design its `Manifest.json` names, else
    /// the one with the longest timeline.
    #[serde(default)]
    pub design: Option<String>,
    /// A dump (JSON, SCHEMA.md) to replay instead of the file's own streams;
    /// the file still gives the bodies.
    #[serde(default)]
    pub dump: Option<String>,
    /// Do not check the replay against the ASM history.
    #[serde(default)]
    pub no_verify: bool,
    /// Leave out items that fail instead of using the file's bodies.
    #[serde(default)]
    pub no_fallback: bool,
    /// Skip the geometric comparison of the final bodies (volumes only).
    #[serde(default)]
    pub no_compare: bool,
    /// Seconds after which the remaining items take the file's bodies without
    /// trying definitions.
    #[serde(default)]
    pub time_limit: Option<f64>,
    /// A factor on the time an item is tried from its own start first
    /// (`mitcad_import::Options::own_start_scale`, half of the item's 60 s;
    /// mitcad#847), and on a thread's sizing offset's 2 s (mitcad#874): a debug build's checks give it their slowdown, so that
    /// an item's outcome does not hang on the build or the load. None, or
    /// not a number above 0: 1. Passed on to the documents the design
    /// refers to.
    #[serde(default)]
    pub own_start_time_scale: Option<f64>,
    /// How far (mm) the replay's geometry may be from the file's where no
    /// exact result is (`mitcad_import::Options::geometric_tolerance`,
    /// mitcad#224); 0.01 mm when left out.
    #[serde(default)]
    pub geometric_tolerance: Option<f64>,
    /// A readable report instead of JSON.
    #[serde(default)]
    pub text: bool,
    /// Also write the JSON report to this file.
    #[serde(default)]
    pub report_path: Option<String>,
    /// Only list the file's designs (`label`, timeline `items`), importing
    /// nothing.
    #[serde(default)]
    pub list: bool,
    /// Seconds without progress after which the import is taken to hang in
    /// the geometry kernel: it runs on a thread of its own, and is run
    /// again with the item it hung on taking the file's bodies (a new
    /// document only).
    #[serde(default)]
    pub hang_limit: Option<f64>,
    /// The memory (MiB) the import may take, beside the process's own
    /// limits (mitcad#80): the import's memory guard measures the process
    /// against the tightest of them.
    #[serde(default)]
    pub memory_limit: Option<f64>,
    /// The threads that evaluate an item's definitions
    /// (`mitcad_import::Options::threads`, mitcad#95); none or 0: one.
    #[serde(default)]
    pub threads: Option<usize>,
    /// The directory of the learning dump (mitcad#96,
    /// `mitcad_import::learn`); `MITCAD_IMPORT_LEARN` when left out.
    #[serde(default)]
    pub learn: Option<String>,
    /// Folders searched for the documents the design refers to (its
    /// components of other documents, mitcad#196), beside the reference
    /// folder next to it (`<design> vN.refs`).
    #[serde(default)]
    pub references: Vec<String>,
    /// Files of documents the design refers to, by their URNs (the
    /// application's file dialog); their folders are searched too.
    #[serde(default)]
    pub reference_files: HashMap<String, String>,
    /// Documents the user chose not to give (by URN): reported as
    /// skipped rather than missing.
    #[serde(default)]
    pub skipped_references: Vec<String>,
    /// The components of other documents stay empty (as before
    /// mitcad#196).
    #[serde(default)]
    pub no_references: bool,
    /// Only list the documents the design refers to and where their files
    /// are, importing nothing (`f3d_references::list`).
    #[serde(default)]
    pub list_references: bool,
    /// The files being imported, outermost first: a document that refers
    /// to one of them is not imported again (a cycle).
    #[serde(skip)]
    pub(crate) stack: Vec<String>,
    /// The documents the import has imported for references so far, shared
    /// by the imports of the documents it refers to (mitcad#229): each is
    /// imported once and reused for every occurrence and level.
    #[serde(skip)]
    pub(crate) reference_cache: Option<crate::f3d_references::ReferenceCache>,
    /// What a retry of an item on the file's exact state does with the
    /// replay's bodies that match the state (mitcad#283,
    /// `mitcad_import::RetryBodies`): `keep` (the default) or `replace`;
    /// `ask`: the user is asked through the document's job the first time a
    /// retry would replace a body that matches (the application's import
    /// process, `JobControl::question`), kept without a job.
    #[serde(default)]
    pub retry_bodies: Option<RetryBodiesOption>,
    /// The choice made of it, shared by the import's tries and the imports
    /// of the documents it refers to.
    #[serde(skip)]
    pub(crate) retry_choice: Option<Arc<mitcad_import::RetryChoice>>,
    /// Which items whose decoded definition builds a result that deviates
    /// from the file's stay parametric with it (mitcad#379,
    /// `mitcad_import::AcceptDeviations`): `true` or `"all"` every one,
    /// timeline indices (`[88, 93]`) those; left out, `false` or `"none"`:
    /// none, they take the file's bodies. The report lists them all.
    /// Not passed on to the documents the design refers to.
    #[serde(default)]
    pub accept_deviations: Option<AcceptDeviationsOption>,
}

/// The import's `own_start_scale` of
/// [`TimelineOptions::own_start_time_scale`] (mitcad#847): 1 without a
/// factor above 0.
fn own_start_scale(options: &TimelineOptions) -> f64 {
    options
        .own_start_time_scale
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(1.0)
}

/// [`TimelineOptions::accept_deviations`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum AcceptDeviationsOption {
    Flag(bool),
    Word(String),
    Items(Vec<i64>),
}

impl AcceptDeviationsOption {
    /// The import's setting; Err for a word other than `all` or `none`.
    pub fn to_import(&self) -> Result<mitcad_import::AcceptDeviations, String> {
        use mitcad_import::AcceptDeviations;
        match self {
            Self::Flag(true) => Ok(AcceptDeviations::All),
            Self::Flag(false) => Ok(AcceptDeviations::None),
            Self::Word(w) => AcceptDeviations::parse(w)
                .ok_or_else(|| format!("accept_deviations: {w:?} is not all, none or indices")),
            Self::Items(items) => Ok(AcceptDeviations::Items(items.iter().copied().collect())),
        }
    }
}

/// [`TimelineOptions::retry_bodies`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryBodiesOption {
    #[default]
    Keep,
    Replace,
    Ask,
}

/// The import's choice for retries on the file's exact state (mitcad#283):
/// asked through `job` where the option says so and there is one.
fn retry_choice(
    option: RetryBodiesOption,
    job: Option<&Arc<RecomputeMonitor>>,
) -> mitcad_import::RetryChoice {
    use mitcad_import::{RetryBodies, RetryChoice};
    match (option, job) {
        (RetryBodiesOption::Replace, _) => RetryChoice::new(RetryBodies::Replace),
        (RetryBodiesOption::Ask, Some(job)) => RetryChoice::ask(Box::new(JobQuestion(job.clone()))),
        _ => RetryChoice::new(RetryBodies::Keep),
    }
}

/// A question of the import to the user through the document's job: the
/// application's import process prints it and passes the answer back
/// (`app/files/F3dImport.cpp`).
struct JobQuestion(Arc<RecomputeMonitor>);

impl mitcad_import::AskUser for JobQuestion {
    fn ask(&self, question: &str) {
        self.0.ask(question);
    }

    fn answer(&self) -> Option<String> {
        self.0.take_answer()
    }
}

/// What a try of the import is told by the watchdog.
#[derive(Debug, Clone, Default)]
struct Try {
    /// Items the kernel hung on before.
    hung: Vec<i64>,
    /// The final bodies by volume only (the kernel hung comparing them).
    no_compare: bool,
    /// Seconds left of the time limit.
    time_limit: Option<f64>,
    /// No timeline: the stored bodies as they are.
    bodies_only: bool,
    progress: Option<Arc<mitcad_import::Progress>>,
    /// The request to stop early: the document's job (T1e).
    stop: Option<Arc<RecomputeMonitor>>,
    /// Where a stopped try stopped (`mitcad_import::Options::stop_at`).
    stop_at: Option<i64>,
    /// Items whose definitions gave no state in earlier tries
    /// (`mitcad_import::Options::failed_items`).
    failed: Vec<(i64, String)>,
}

/// A value moved to the thread that waits for it.
///
/// SAFETY: only the document of an import thread and its result cross,
/// once the thread is done with them (they are sent as its last act). The
/// document's shapes are C++ objects behind `shared_ptr`s and OCCT handles,
/// whose reference counts are atomic; nothing else refers to them.
pub(crate) struct Handover<T>(pub(crate) T);

unsafe impl<T> Send for Handover<T> {}

/// Import threads the hang watchdog gave up that still run (mitcad#82):
/// see [`abandoned_imports`].
static ABANDONED_RUNNING: AtomicUsize = AtomicUsize::new(0);

/// How many import threads the hang watchdog gave up still run: their
/// geometry kernel call has not returned yet; and the workers that
/// evaluated an item's definitions ahead and still run, whose result is no
/// longer wanted (mitcad#95). A process that ends while
/// one runs must end without its static destructors (C++ `std::_Exit`
/// once its output is written; `mitcad-cli`, the application's import
/// worker): the shared libraries' finalizers tear down OCCT's state under
/// the thread, which then crashes (mitcad#82: a segmentation fault at exit
/// on Linux after an import that hung, the thread healing a body).
pub fn abandoned_imports() -> usize {
    // (Workers that evaluate definitions ahead and still run, mitcad#95:
    // their results are no longer wanted once the import has ended.)
    ABANDONED_RUNNING.load(Ordering::SeqCst)
        + mitcad_import::running_workers()
        + BUILDING_AHEAD.load(Ordering::SeqCst)
}

/// The model's JSON commands for the import (mitcad#778): the features a
/// cut and paste carries run through the api's `command`, which
/// `mitcad-import` does not depend on (also the `.ipt` import's).
pub(crate) fn import_commands() -> mitcad_import::Commands {
    mitcad_import::Commands::new::<OcctKernel>(|doc, json| {
        use mitcad_model_api::DocumentApi;
        doc.command(json).map_err(|e| e.to_string())
    })
}

/// Starts the threads that evaluate an item's definitions ahead
/// (mitcad#95): an import thread's stack, with OCCT's crash handlers as on
/// the import's thread. None start when the stack cannot be reserved (the
/// import's thread then evaluates the definition).
fn worker_spawner() -> mitcad_import::Spawner {
    mitcad_import::Spawner(Arc::new(|job: mitcad_import::WorkerJob| {
        std::thread::Builder::new()
            .name("mitcad-import-worker".to_owned())
            .stack_size(mitcad_import::WORKER_STACK)
            .spawn(move || {
                crate::kernel::exchange::ffi::catch_occt_crashes();
                job();
            })
            .is_ok()
    }))
}

/// A try's thread runs, has finished, or was given up while it ran.
const TRY_RUNNING: u8 = 0;
const TRY_FINISHED: u8 = 1;
const TRY_ABANDONED: u8 = 2;

/// Whether a try's thread runs, counted in [`ABANDONED_RUNNING`] once the
/// watchdog gave it up ([`TryThread::abandon`]) until it ends.
#[derive(Clone)]
pub(crate) struct TryThread(Arc<AtomicU8>);

impl TryThread {
    pub(crate) fn new() -> Self {
        Self(Arc::new(AtomicU8::new(TRY_RUNNING)))
    }

    /// The watchdog gave the try up: counted until its thread ends.
    pub(crate) fn abandon(&self) {
        ABANDONED_RUNNING.fetch_add(1, Ordering::SeqCst);
        if self
            .0
            .compare_exchange(
                TRY_RUNNING,
                TRY_ABANDONED,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_err()
        {
            // (It has just ended.)
            ABANDONED_RUNNING.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// Held by a try's thread to its very end (dropped last): no longer counted.
pub(crate) struct TryEnd(pub(crate) TryThread);

impl Drop for TryEnd {
    fn drop(&mut self) {
        if self.0.0.swap(TRY_FINISHED, Ordering::SeqCst) == TRY_ABANDONED {
            ABANDONED_RUNNING.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// How many times an import that hung is run again.
pub(crate) const HANG_RETRIES: usize = 8;

/// The stack of an import thread (the main thread's is 8 MiB on Linux).
const IMPORT_STACK: usize = 256 << 20;

/// Smaller stacks an import thread is started on when the full one does
/// not fit: an abandoned try keeps its stack and memory, and under an
/// address-space limit (`ulimit -v`) the next try's reservation can fail
/// (mitcad#58). A smaller stack can overflow in OCCT's deepest recursions,
/// which fails that feature (the item then takes the file's bodies), but
/// the import goes on.
const SMALLER_STACKS: [usize; 3] = [64 << 20, 16 << 20, 8 << 20];

/// Starts `run` on a new import thread, on the full stack or, when that
/// cannot be reserved, on the largest smaller one that can; the stack size
/// it got, or the error of the last attempt.
pub(crate) fn spawn_import<F>(run: F) -> std::io::Result<usize>
where
    F: FnOnce() + Send + 'static,
{
    // The closure is handed from attempt to attempt until one starts.
    let run = Arc::new(Mutex::new(Some(run)));
    let mut error = None;
    for stack in std::iter::once(IMPORT_STACK).chain(SMALLER_STACKS) {
        let run = run.clone();
        let started = std::thread::Builder::new()
            .name("mitcad-import".to_owned())
            // OCCT recurses deeply (booleans, fillets); the default
            // thread stack of 2 MiB overflows where the main thread's
            // does not, and the overflow is a failed feature.
            .stack_size(stack)
            .spawn(move || {
                let run = run.lock().map(|mut r| r.take()).unwrap_or(None);
                if let Some(run) = run {
                    run();
                }
            });
        match started {
            Ok(_) => return Ok(stack),
            Err(e) => error = Some(e),
        }
    }
    Err(error.expect("at least one stack size is tried"))
}

/// The share of the tightest memory limit at which the import drops what
/// it can do without (`Progress::tight_on_memory`), and again at each 5 %
/// more; below it, memory that was low has recovered.
const MEMORY_TIGHT: f64 = 0.70;

/// The share at which the import is low on memory (`Progress::low_memory`):
/// it cuts the definition being evaluated short and tries no more until
/// the memory has recovered. An allocation that fails ends the process
/// (Rust's cannot be caught), and one definition can take gigabytes (a
/// pattern of a thousand copies).
const MEMORY_LOW: f64 = 0.85;

/// How often the memory guard measures the process.
pub(crate) const MEMORY_POLL: Duration = Duration::from_millis(100);

/// The import's memory guard (mitcad#80): measures the process against
/// its limits (`memory::memory_use`, and the import's own
/// `memory_limit`) and tells a try when memory gets tight or low; also the
/// `.ipt` import's (`ipt_import.rs`).
pub(crate) struct MemoryGuard {
    /// The import's own limit in bytes.
    limit: Option<u64>,
    /// The share at which the try is next told that memory is tight.
    next_tight: f64,
    /// The allocations in the geometry kernel that had failed when last
    /// measured.
    failed_allocations: u64,
    /// When the allocator's free memory is given back to the system
    /// (mitcad#885).
    release: mitcad_import::FreeMemoryRelease,
}

impl MemoryGuard {
    fn new(options: &TimelineOptions) -> Self {
        Self::with_limit(options.memory_limit)
    }

    /// A guard with the import's own limit in MiB (none or 0: the
    /// process's limits only).
    pub(crate) fn with_limit(memory_limit: Option<f64>) -> Self {
        let memory = crate::memory::memory_use();
        Self {
            limit: memory_limit
                .filter(|m| *m > 0.0)
                .map(|m| (m * f64::from(1u32 << 20)) as u64),
            next_tight: MEMORY_TIGHT,
            failed_allocations: memory.failed_allocations,
            release: mitcad_import::FreeMemoryRelease::new(memory.resident),
        }
    }

    pub(crate) fn check(&mut self, progress: &mitcad_import::Progress) {
        let mut memory = crate::memory::memory_use().within(self.limit);
        // The allocator's free memory back to the system first (mitcad#885):
        // what is resident beyond what the process uses counts against its
        // limits.
        let now = Instant::now();
        if self
            .release
            .due(memory.resident, memory.share() >= MEMORY_TIGHT, now)
        {
            crate::memory::release_free_memory();
            memory = crate::memory::memory_use().within(self.limit);
            self.release.done(memory.resident, now);
        }
        let share = memory.share();
        let text = memory.describe();
        progress.set_memory(&text);
        progress.set_memory_share(share);
        // An allocation in the kernel failed (mitcad#132): also one it
        // caught inside, which no operation reported, and one on a thread
        // of no import. The memory ran out, whatever it is now.
        if memory.failed_allocations != self.failed_allocations {
            self.failed_allocations = memory.failed_allocations;
            progress.low_memory_unattributed(&format!(
                "an allocation in the geometry kernel failed, {text}"
            ));
            return;
        }
        if share >= MEMORY_LOW {
            progress.low_memory(&text);
            return;
        }
        if share < MEMORY_TIGHT {
            progress.memory_recovered();
        }
        if share >= self.next_tight {
            progress.tight_on_memory();
            self.next_tight = share + 0.05;
        } else {
            // (Down again after the import dropped what it could.)
            self.next_tight = self.next_tight.min((share + 0.05).max(MEMORY_TIGHT));
        }
    }

    /// Runs `f` with the guard measuring the process on a thread of its
    /// own (an import without the watchdog, whose loop measures it).
    pub(crate) fn around<T>(
        mut self,
        progress: &Arc<mitcad_import::Progress>,
        f: impl FnOnce() -> T,
    ) -> T {
        self.check(progress);
        let (done, finished) = std::sync::mpsc::channel::<()>();
        let watched = progress.clone();
        let guard = std::thread::Builder::new()
            .name("mitcad-import-memory".to_owned())
            .stack_size(1 << 20)
            .spawn(move || {
                while matches!(
                    finished.recv_timeout(MEMORY_POLL),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                ) {
                    self.check(&watched);
                }
            });
        let result = f();
        drop(done);
        if let Ok(guard) = guard {
            let _ = guard.join();
        }
        result
    }
}

/// A file's design and bodies, read once for all tries of its import
/// (mitcad#80: a try run again after a hang read and converted the file
/// again while the abandoned one still held its memory).
struct Prepared {
    /// The design's label.
    label: String,
    /// The design to replay: the file's (None without a design segment) or
    /// an external dump.
    dump: Option<Dump>,
    /// The file's bodies.
    file: Arc<StoredFile>,
    /// The package's other designs.
    others: Vec<String>,
}

/// Reads the design to import and its bodies.
fn prepare(path: &str, options: &TimelineOptions) -> Result<Prepared, ApiError> {
    let fail = |e: String| ApiError(e);
    let clock = Instant::now();
    let package = mitcad_f3d_design::package(std::path::Path::new(path)).map_err(fail)?;
    if package.is_empty() {
        return Err(ApiError(format!("{path} holds no design")));
    }
    let labels: Vec<String> = package.labels().map(str::to_owned).collect();
    // The design asked for, else a package's root design where its
    // manifest names it (mitcad#229).
    let named = match &options.design {
        Some(wanted) => Some(
            labels
                .iter()
                .position(|label| label == wanted || label.ends_with(&format!("!{wanted}")))
                .ok_or_else(|| {
                    ApiError(format!(
                        "no design {wanted}; the file has {}",
                        labels.join(", ")
                    ))
                })?,
        ),
        None => labels
            .iter()
            .position(|label| Some(label) == package.root.as_ref()),
    };
    // The design's streams; without one named, each design's, to choose
    // the one with the most timeline items (the last of equals). Only the
    // chosen one's inputs are found in the bodies' history (mitcad#103).
    // A package's designs are inflated one at a time, the best so far kept
    // (mitcad#923).
    let (chosen, opened, mut decoded) = match named {
        Some(index) => {
            let doc = package.open(index).map_err(fail)?;
            let design = mitcad_f3d_design::decode_streams(&doc, &labels[index]).map_err(fail)?;
            (index, doc, design)
        }
        None => {
            let mut best: Option<(usize, usize, mitcad_f3d_design::PackageDocument<'_>, _)> = None;
            for (index, label) in labels.iter().enumerate() {
                let doc = package.open(index).map_err(fail)?;
                let design = mitcad_f3d_design::decode_streams(&doc, label).map_err(fail)?;
                let items = design.dump.as_ref().map_or(0, |d| d.timeline_items().len());
                if best.as_ref().is_none_or(|b| items >= b.1) {
                    best = Some((index, items, doc, design));
                }
            }
            let (index, _, doc, design) = best.expect("the package holds a design");
            (index, doc, design)
        }
    };
    let doc: &F3dFile = &opened;
    let label = labels[chosen].clone();
    let others: Vec<String> = labels
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != chosen)
        .map(|(_, l)| l.clone())
        .collect();
    trace_reading("design streams decoded", clock);
    // (An external dump stands in for the file's design.)
    let resolve = options.dump.is_none();
    let external = match &options.dump {
        Some(dump_path) => {
            let text = std::fs::read_to_string(dump_path)
                .map_err(|e| ApiError(format!("{dump_path}: {e}")))?;
            Some(Dump::from_json(&text).map_err(|e| ApiError(format!("{dump_path}: {e}")))?)
        }
        None => None,
    };
    // Its items name the history's operations where they carry the file's
    // result numbers (`_f3d.result_no`, as the decoder's own dump does; a
    // synthetic design of the tests, mitcad#379); else the file's design
    // does.
    let numbered = external.as_ref().filter(|d| {
        d.timeline_items().iter().any(|i| {
            i.f3d
                .as_ref()
                .and_then(|f| f.result_no)
                .is_some_and(|n| n > 0)
        })
    });
    let threads = options.threads.unwrap_or(1).max(1);
    let file = if resolve && threads > 1 {
        // The inputs are found while the bodies are converted: they read
        // the same history, and neither needs the other (the bodies need
        // the blobs' links, components and the items' states, which finding
        // the inputs leaves as they are).
        let links = decoded.dump.clone();
        // The threads shared between the two.
        let (converting, finding) = (threads / 2, threads - threads / 2);
        let resolved = Mutex::new((decoded, false));
        let resolve = || {
            let mut design = resolved
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !design.1 {
                let clock = Instant::now();
                mitcad_f3d_design::resolve_inputs(&mut design.0, doc, finding);
                design.1 = true;
                trace_reading("inputs found", clock);
            }
        };
        let file = std::thread::scope(|scope| {
            let resolving = std::thread::Builder::new()
                .name("mitcad-import-inputs".to_owned())
                .stack_size(IMPORT_STACK)
                .spawn_scoped(scope, resolve);
            let clock = Instant::now();
            let file = StoredFile::new(doc, &label, links.as_ref(), converting);
            trace_reading("bodies converted", clock);
            match resolving {
                Ok(thread) => {
                    if let Err(panic) = thread.join() {
                        std::panic::resume_unwind(panic);
                    }
                }
                // (Not started: here.)
                Err(_) => resolve(),
            }
            file
        });
        decoded = resolved
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .0;
        file
    } else {
        if resolve {
            let clock = Instant::now();
            mitcad_f3d_design::resolve_inputs(&mut decoded, doc, threads);
            trace_reading("inputs found", clock);
        }
        let clock = Instant::now();
        let file = StoredFile::new(doc, &label, numbered.or(decoded.dump.as_ref()), threads);
        trace_reading("bodies converted", clock);
        file
    }
    .map_err(fail)?;
    // The entities the design names in other documents (mitcad#196) are
    // found in those documents' own histories, when the files are at hand.
    if resolve
        && !options.no_references
        && let Some(dump) = decoded.dump.as_ref()
    {
        let files = crate::f3d_references::external_files(path, &label, options, dump);
        if !files.is_empty() {
            let clock = Instant::now();
            let documents: Vec<mitcad_f3d_design::inputs::ExternalFile<'_>> = files
                .iter()
                .map(
                    |(guid, key, file)| mitcad_f3d_design::inputs::ExternalFile {
                        guid: guid.clone(),
                        key: key.clone(),
                        file,
                    },
                )
                .collect();
            mitcad_f3d_design::resolve_external_inputs(&mut decoded, &documents);
            // Derives' placements kept as occurrence paths of their source
            // designs (mitcad#324).
            mitcad_f3d_design::resolve_derive_placements(&mut decoded, &documents);
            trace_reading("inputs in other documents found", clock);
        }
    }
    let dump = match external {
        Some(external) => Some(external),
        None => decoded.dump,
    };
    Ok(Prepared {
        label,
        dump,
        file: Arc::new(file),
        others,
    })
}

/// Traces how long a part of reading the file took (`MITCAD_IMPORT_TRACE`).
fn trace_reading(what: &str, clock: Instant) {
    if std::env::var_os("MITCAD_IMPORT_TRACE").is_some() {
        eprintln!(
            "import: file read: {what} in {:.2} s",
            clock.elapsed().as_secs_f64()
        );
    }
}

/// [`prepare`] on a thread with an import thread's stack (as a try's, for
/// deep recursions), for the watched import; joined, so that its stack is
/// gone before the first try's is reserved.
fn prepare_on_thread(path: &str, options: &TimelineOptions) -> Result<Prepared, ApiError> {
    let (path_owned, options_owned) = (path.to_owned(), options.clone());
    let started = std::thread::Builder::new()
        .name("mitcad-import".to_owned())
        .stack_size(IMPORT_STACK)
        .spawn(move || prepare(&path_owned, &options_owned));
    match started {
        Ok(thread) => thread
            .join()
            .unwrap_or_else(|_| Err(ApiError("the import stopped without a result".to_owned()))),
        Err(_) => prepare(path, options),
    }
}

/// The designs of a file as JSON: `{"designs": [{"label", "items"}]}`.
fn list_designs(path: &str) -> Result<String, ApiError> {
    let documents = mitcad_f3d_design::documents(std::path::Path::new(path)).map_err(ApiError)?;
    let mut designs = Vec::new();
    for (label, doc) in &documents {
        let design = mitcad_f3d_design::decode_streams(doc, label).map_err(ApiError)?;
        let items = design.dump.as_ref().map_or(0, |d| d.timeline_items().len());
        designs.push(json!({"label": label, "items": items}));
    }
    Ok(serde_json::to_string_pretty(&json!({"designs": designs})).expect("serializes"))
}

fn file_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

impl Document {
    /// Imports an .f3d design with its timeline (see the module docs);
    /// returns the import report as JSON (or text).
    pub(crate) fn import_f3d_timeline(
        &mut self,
        path: &str,
        json: &str,
    ) -> Result<String, ApiError> {
        let json = if json.trim().is_empty() { "{}" } else { json };
        let options: TimelineOptions = serde_json::from_str(json)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        if options.list {
            return list_designs(path);
        }
        if options.list_references {
            // The application runs the listing in the import process
            // (mitcad#922: `--import-worker --list-references`), watched by
            // the memory guard like an import; a fault of the readers ends
            // that process, not the application. Not caught here: the
            // application reports the process's end.
            let progress = Arc::new(mitcad_import::Progress::new());
            let listed = MemoryGuard::new(&options)
                .around(&progress, || {
                    // (For tests: a panic as a stream parser's bug would.)
                    mitcad_model::reader_guard::test_panic(path);
                    mitcad_import::reference_folder::list(
                        path,
                        &options.references,
                        &options.reference_files,
                    )
                })
                .map_err(ApiError)?;
            return Ok(serde_json::to_string_pretty(&listed).expect("serializes"));
        }
        let report = self.import_timeline(path, &options)?;
        let json = serde_json::to_string_pretty(&report).expect("the report serializes");
        if let Some(file) = &options.report_path {
            std::fs::write(file, format!("{json}\n"))
                .map_err(|e| ApiError(format!("{file}: {e}")))?;
        }
        Ok(if options.text { report.text() } else { json })
    }

    /// The import of the design; the documents it refers to are imported
    /// first and fill its components of other documents when it is replayed
    /// (mitcad#196), all one undo step.
    pub(crate) fn import_timeline(
        &mut self,
        path: &str,
        options: &TimelineOptions,
    ) -> Result<mitcad_import::ImportReport, ApiError> {
        self.import_design_timeline(path, options)
    }

    /// The import of the design itself.
    fn import_design_timeline(
        &mut self,
        path: &str,
        options: &TimelineOptions,
    ) -> Result<mitcad_import::ImportReport, ApiError> {
        // A job's cancel stops it early (T1e): the modelling items from the
        // one being replayed on take the file's bodies, and the document is
        // the import's all the same. The job's monitor is the import's stop
        // request, not the document's (Document::without_job).
        let stop = self.0.monitor().cloned();
        if let Some(accept) = &options.accept_deviations {
            accept.to_import().map_err(ApiError)?;
        }
        // The choice for retries on the file's exact state (mitcad#283), one
        // for the import, its tries and the documents it refers to.
        let mut options = options.clone();
        if options.retry_choice.is_none() {
            options.retry_choice = Some(Arc::new(retry_choice(
                options.retry_bodies.unwrap_or_default(),
                stop.as_ref(),
            )));
        }
        let options = &options;
        // For the application's UI test: the question asked at the start,
        // as the first retry that would replace a matching body asks it.
        if std::env::var_os("MITCAD_TEST_IMPORT_QUESTION").is_some()
            && let Some(choice) = &options.retry_choice
        {
            choice.decide(
                "the test question",
                "The import's test question (MITCAD_TEST_IMPORT_QUESTION): keep the imported \
                 bodies or replace them with the stored bodies?",
                || !stop.as_ref().is_some_and(|s| s.is_cancelled()),
            );
        }
        self.without_job(|document| {
            let fresh = document.0.features().next().is_none() && document.0.undo_depth() == 0;
            match options.hang_limit.filter(|s| *s > 0.0) {
                Some(limit) if fresh => document.import_watched(path, options, limit, stop),
                _ => {
                    let progress = Arc::new(mitcad_import::Progress::new());
                    let attempt = Try {
                        time_limit: options.time_limit,
                        no_compare: options.no_compare,
                        progress: Some(progress.clone()),
                        stop,
                        ..Try::default()
                    };
                    MemoryGuard::new(options).around(&progress, || {
                        let prepared = prepare(path, options)?;
                        let references = document.references_of(path, options, &prepared);
                        document.import_try(&prepared, &references, path, options, &attempt)
                    })
                }
            }
        })
    }

    /// The import on a thread of its own, watched: when it makes no
    /// progress for `limit` seconds (a kernel call that does not return),
    /// the try is given up (`Progress::abandon`: it stops at its next check
    /// and its result is dropped) and the import runs again on a new
    /// document with the item it hung on taking the file's bodies (and the
    /// items before it whose definitions gave no state taking them at once,
    /// without trying their definitions again); hung while
    /// comparing the final bodies, again comparing volumes only; hung
    /// elsewhere, with the stored bodies as they are. A given-up thread
    /// keeps its processor until its kernel call returns (mitcad#71). A
    /// try that hung after a stop runs again stopped where it stopped
    /// (where it hung, when it had not seen the stop yet).
    fn import_watched(
        &mut self,
        path: &str,
        options: &TimelineOptions,
        limit: f64,
        stop: Option<Arc<RecomputeMonitor>>,
    ) -> Result<mitcad_import::ImportReport, ApiError> {
        let mut attempt = Try {
            no_compare: options.no_compare,
            stop,
            ..Try::default()
        };
        // The file is read once: the tries share its design and bodies.
        let prepared = Arc::new(prepare_on_thread(path, options)?);
        // The documents it refers to are imported before the tries (each
        // with a watchdog of its own), and fill its components in each.
        let references = Arc::new(self.references_of(path, options, &prepared));
        let mut warnings = Vec::new();
        for _ in 0..=HANG_RETRIES {
            // Each try has the whole time limit: the items before the one
            // the kernel hung on are replayed again.
            attempt.time_limit = options.time_limit;
            let progress = Arc::new(mitcad_import::Progress::new());
            attempt.progress = Some(progress.clone());
            let mut guard = MemoryGuard::new(options);
            guard.check(&progress);
            let (sender, receiver) = std::sync::mpsc::channel();
            let (path_owned, options_owned, try_owned, prepared_shared, references_shared) = (
                path.to_owned(),
                options.clone(),
                attempt.clone(),
                prepared.clone(),
                references.clone(),
            );
            let watched = progress.clone();
            let thread = TryThread::new();
            let ended = TryEnd(thread.clone());
            let stack = spawn_import(move || {
                // Dropped last, once the try's document and shapes are gone.
                let _ended = ended;
                let mut document = Document(mitcad_model::Document::new(OcctKernel));
                let result = document.import_try(
                    &prepared_shared,
                    &references_shared,
                    &path_owned,
                    &options_owned,
                    &try_owned,
                );
                drop(prepared_shared);
                drop(references_shared);
                drop(try_owned);
                // A try given up (mitcad#71) stopped early: what it made is
                // dropped here, never reported.
                if !watched.abandoned() {
                    let _ = sender.send(Handover((document, result)));
                }
            })
            .map_err(|e| {
                ApiError(if warnings.is_empty() {
                    format!("cannot start the import: {e}")
                } else {
                    // The abandoned tries still hold their stacks and memory.
                    format!(
                        "cannot start the import again after the geometry kernel hung ({}): \
                         no memory is left for a new import thread while the abandoned one \
                         holds its own ({e})",
                        warnings.join("; ")
                    )
                })
            })?;
            if stack < IMPORT_STACK {
                warnings.push(format!(
                    "the import ran on a {} MiB stack instead of {} MiB (the memory left{})",
                    stack >> 20,
                    IMPORT_STACK >> 20,
                    if warnings.is_empty() {
                        String::new()
                    } else {
                        " beside the abandoned try".to_owned()
                    }
                ));
            }
            let mut step = u64::MAX;
            let mut moved = Instant::now();
            let hung = loop {
                match receiver.recv_timeout(MEMORY_POLL) {
                    Ok(Handover((document, result))) => {
                        let mut report = result?;
                        if let Some(design) = report.designs.first_mut() {
                            design.warnings.extend(warnings);
                        }
                        *self = document;
                        return Ok(report);
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        guard.check(&progress);
                        let now = progress.step.load(Ordering::Relaxed);
                        if now != step {
                            step = now;
                            moved = Instant::now();
                        } else if moved.elapsed().as_secs_f64() > limit {
                            let item = progress.item.load(Ordering::Relaxed);
                            // The try stops once its kernel call returns
                            // (or at once, where the call stops on request),
                            // so it does not compete with the next one.
                            progress.abandon();
                            thread.abandon();
                            break item;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        return Err(ApiError("the import stopped without a result".to_owned()));
                    }
                }
            };
            // The items before the one it hung on replay as before: those
            // whose definitions gave no state take the file's bodies at once.
            for (item, reason) in progress.failed_items() {
                if !attempt.failed.iter().any(|(i, _)| *i == item) {
                    attempt.failed.push((item, reason));
                }
            }
            if attempt.stop.as_ref().is_some_and(|s| s.is_cancelled()) {
                attempt.stop_at = Some(progress.stopped_at().unwrap_or(match hung {
                    i if i >= 0 => i,
                    mitcad_import::Progress::STARTING => i64::MIN,
                    _ => i64::MAX,
                }));
                attempt.stop = None;
            }
            let what = match hung {
                mitcad_import::Progress::FINISHING if !attempt.no_compare => {
                    attempt.no_compare = true;
                    "comparing the final bodies; they are compared by volume only".to_owned()
                }
                i if i >= 0 && !attempt.hung.contains(&i) && !attempt.bodies_only => {
                    attempt.hung.push(i);
                    format!("timeline item {i}; it takes the file's bodies")
                }
                _ if !attempt.bodies_only => {
                    attempt.bodies_only = true;
                    "the import; the stored bodies come in without the timeline".to_owned()
                }
                _ => break,
            };
            warnings.push(format!(
                "the geometry kernel did not return for {limit} s on {what}"
            ));
        }
        Err(ApiError(format!(
            "the import of {path} hung in the geometry kernel"
        )))
    }

    /// The documents the prepared design refers to, imported from their
    /// files (mitcad#196); none for an import without references.
    fn references_of(
        &mut self,
        path: &str,
        options: &TimelineOptions,
        prepared: &Prepared,
    ) -> References {
        match &prepared.dump {
            Some(dump) if !options.no_references => {
                self.load_references(path, &prepared.label, options, dump)
            }
            _ => References::default(),
        }
    }

    /// One try of the import of a prepared file into this document.
    fn import_try(
        &mut self,
        prepared: &Prepared,
        references: &References,
        path: &str,
        options: &TimelineOptions,
        attempt: &Try,
    ) -> Result<mitcad_import::ImportReport, ApiError> {
        // The import tries many definitions on imported geometry; an OCCT
        // crash in one of them is then an error of that attempt.
        crate::kernel::exchange::ffi::catch_occt_crashes();
        // A kernel call that never returns, for the tests (mitcad#82; the
        // importer has another, `compare`): every try builds the file's
        // bodies over and over without progress, so the import ends hung
        // while the threads the watchdog gave up still run in the kernel.
        if std::env::var_os("MITCAD_IMPORT_STALL").is_some_and(|v| v == "busy") {
            let geometry = F3dGeometry::new(prepared.file.clone());
            loop {
                for v in 0..geometry.built.len() {
                    let _ = geometry.heal(v);
                }
                if geometry.built.is_empty() {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
        // Without a design segment, the stored bodies come in as they are
        // (the importer compares and replaces the empty replay).
        let bodies_only = Dump {
            source: Some(mitcad_dump::Source {
                mode: Some("f3d_stream".to_owned()),
                file: Some(prepared.label.clone()),
                ..Default::default()
            }),
            // The stored bodies keep the file's appearances (mitcad#228).
            f3d: prepared
                .dump
                .as_ref()
                .and_then(|d| d.f3d.as_ref())
                .and_then(|f| f.appearance.clone())
                .map(|a| mitcad_dump::DumpF3d {
                    appearance: Some(a),
                    ..Default::default()
                }),
            ..Dump::default()
        };
        let dump = match &prepared.dump {
            Some(dump) if !attempt.bodies_only => dump,
            _ => &bodies_only,
        };
        let mut geometry = F3dGeometry::new(prepared.file.clone());
        let import_options = mitcad_import::Options {
            verify: !options.no_verify,
            fallback: !options.no_fallback,
            compare: !attempt.no_compare,
            undo_label: Some(format!("Import {}", file_name(path))),
            time_limit: attempt.time_limit.filter(|t| *t > 0.0),
            hung_items: attempt.hung.clone(),
            progress: attempt.progress.clone(),
            stop: attempt.stop.clone(),
            stop_at: attempt.stop_at,
            failed_items: attempt.failed.clone(),
            threads: options.threads.unwrap_or(1).max(1),
            own_start_scale: own_start_scale(options),
            spawn: Some(worker_spawner()),
            commands: Some(import_commands()),
            geometric_tolerance: options
                .geometric_tolerance
                .filter(|t| t.is_finite() && *t > 0.0)
                .unwrap_or(mitcad_import::DEFAULT_GEOMETRIC_TOLERANCE),
            retry_bodies: options.retry_choice.clone(),
            accept_deviations: options
                .accept_deviations
                .as_ref()
                .and_then(|a| a.to_import().ok())
                .unwrap_or_default(),
            learn: options
                .learn
                .clone()
                .or_else(|| std::env::var("MITCAD_IMPORT_LEARN").ok())
                .filter(|d| !d.is_empty())
                .map(|dir| mitcad_import::learn::LearnOptions {
                    dir: dir.into(),
                    path: Some(path.into()),
                }),
            ..mitcad_import::Options::default()
        };
        // The documents the design refers to fill its components of other
        // documents before its items are replayed (mitcad#196).
        let sources = references.sources();
        let mut design = mitcad_import::import_design_with(
            &mut self.0,
            dump,
            &mut geometry,
            &import_options,
            references.has_documents().then_some(&sources),
        );
        references.finish(path, &self.0, &mut design);
        let mut report = mitcad_import::ImportReport {
            file: file_name(path).to_owned(),
            designs: vec![design],
        };
        if !prepared.others.is_empty() {
            report.designs[0].warnings.push(format!(
                "other designs in the package: {}",
                prepared.others.join(", ")
            ));
        }
        Ok(report)
    }

    /// `{"cmd": "import_f3d", "path": ..., ...}`: the timeline import as a
    /// document command (the model's commands cannot reach the file's
    /// bodies).
    pub(crate) fn import_f3d_command(&mut self, mut command: Value) -> Result<Value, ApiError> {
        let path = command
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError("import_f3d needs a \"path\"".to_owned()))?
            .to_owned();
        if let Some(map) = command.as_object_mut() {
            map.remove("cmd");
            map.remove("path");
        }
        let options: TimelineOptions = serde_json::from_value(command)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        let report = self.import_timeline(&path, &options)?;
        let design = &report.designs[0];
        let counts: HashMap<&str, usize> = design
            .counts()
            .into_iter()
            .map(|(o, n)| (o.as_str(), n))
            .collect();
        Ok(json!({
            "file": report.file,
            "design": design.label,
            "items": design.items.len(),
            "counts": counts,
            "report": serde_json::to_value(&report).expect("the report serializes"),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The own-start time scale of the command's JSON (`mitcad-cli
    /// import-f3d --own-start-time-scale`) reaches the import's options
    /// (mitcad#847): it scales the time an item is tried from its own
    /// start (half of the item's 60 s) and a thread's sizing offset's
    /// (mitcad#874), and leaves the item's time as it is; left out or not
    /// above 0, 30 s.
    #[test]
    fn the_own_start_time_scale_reaches_the_import_s_options() {
        let import = |json: Value| {
            let options: TimelineOptions = serde_json::from_value(json).unwrap();
            mitcad_import::Options {
                own_start_scale: own_start_scale(&options),
                ..mitcad_import::Options::default()
            }
        };
        let scaled = import(json!({"own_start_time_scale": 3.0}));
        assert_eq!(scaled.own_start_scale, 3.0);
        assert_eq!(scaled.own_start_seconds(), 90.0);
        // (A thread's sizing offset too, mitcad#874.)
        assert_eq!(scaled.sizing_offset_seconds(), 6.0);
        assert_eq!(scaled.item_seconds, 60.0);
        let halved = import(json!({"own_start_time_scale": 0.5}));
        assert_eq!(halved.own_start_seconds(), 15.0);
        for json in [
            json!({}),
            json!({"own_start_time_scale": 0.0}),
            json!({"own_start_time_scale": -2.0}),
        ] {
            assert_eq!(import(json).own_start_seconds(), 30.0);
        }
    }

    /// An item changed its blob where its operation did, or where the
    /// operations no item names since the last named one did (a pattern
    /// whose copies come before its last operation, mitcad#719); the
    /// operations before the blob's first named one are not an item's.
    #[test]
    fn an_item_changed_its_blob_by_its_own_operations() {
        // Operations 2 (the blob's start), 10 (named), 12, 13 (no item
        // names them) changed the bodies; 11 and 14 did not.
        let changed: HashSet<i64> = [2, 10, 12, 13].into();
        assert!(item_changed(&changed, None, 10));
        // An item that failed in the file after the blob's start.
        assert!(!item_changed(&changed, None, 11));
        // A pattern whose copies are 12 and 13, its own last 14.
        assert!(item_changed(&changed, Some(10), 14));
        // An item that failed in the file right after a named one.
        assert!(!item_changed(&changed, Some(10), 11));
    }

    /// A history like a path pattern's (mitcad#676): three small states
    /// the replay asks for, the copies' states it skips, the pattern's own
    /// state with a large body whose state before has no large body (no
    /// base), the next states' bodies based on it, and a large new body in
    /// a later state.
    struct Pattern {
        states: Vec<Vec<usize>>,
        anchors: Vec<bool>,
        records: Vec<usize>,
        faces: Vec<usize>,
    }

    fn pattern() -> Pattern {
        Pattern {
            // Global states (one blob): their variants.
            states: vec![
                vec![0],
                vec![1],
                vec![2],
                vec![3],
                vec![4],
                vec![5],
                vec![6],
                vec![7],
                vec![8],
                vec![8, 9],
            ],
            anchors: vec![
                true, true, true, false, false, false, true, true, true, true,
            ],
            records: vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 2],
            faces: vec![12, 29, 37, 2_000, 2_100, 2_200, 8_191, 6_721, 6_723, 5_000],
        }
    }

    fn bases(p: &Pattern, upto: usize) -> Vec<Option<usize>> {
        let history: Vec<(&[usize], bool)> = p
            .states
            .iter()
            .zip(&p.anchors)
            .take(upto + 1)
            .map(|(s, &a)| (&s[..], a))
            .collect();
        bases_of(&p.records, |v| p.faces[v] >= REUSE_MIN_FACES, &[history]).0
    }

    #[test]
    fn the_first_states_asked_for_skip_those_between() {
        let p = pattern();
        assert_eq!(
            first_asked(&p.anchors, STATES_AHEAD + 1),
            vec![0, 1, 2, 6, 7]
        );
        assert_eq!(first_asked(&p.anchors, 2), vec![0, 1]);
        assert_eq!(first_asked(&[false, false], 3), Vec::<usize>::new());
    }

    #[test]
    fn the_bases_of_the_first_states_come_from_those_states_alone() {
        // As the whole history gives them: a base is in an earlier state.
        let p = pattern();
        let first = first_asked(&p.anchors, STATES_AHEAD + 1);
        let last = *first.last().unwrap();
        let (some, all) = (bases(&p, last), bases(&p, p.states.len() - 1));
        for v in first.iter().flat_map(|&s| p.states[s].iter().copied()) {
            assert_eq!(some[v], all[v], "variant {v}");
        }
        // The pattern's state has no base (37 faces before it), the next
        // state's body has the pattern's as its base.
        assert_eq!(all[6], None);
        assert_eq!(all[7], Some(6));
    }

    #[test]
    fn a_pattern_s_large_state_is_healed_while_the_file_is_read() {
        let p = pattern();
        let first = first_asked(&p.anchors, STATES_AHEAD + 1);
        let variants: Vec<usize> = first
            .iter()
            .flat_map(|&s| p.states[s].iter().copied())
            .collect();
        let bases = bases(&p, *first.last().unwrap());
        let faces = |v: usize| p.faces[v];
        // Not the small bodies, not the one with a base, not the later
        // state's (not among the first asked for).
        assert_eq!(heal_while_reading(&variants, faces, &bases, &[]), vec![6]);
        // Not one healing already (a blob's oldest state's).
        assert!(heal_while_reading(&variants, faces, &bases, &[6]).is_empty());
        // Largest first.
        let no_bases = vec![None; p.faces.len()];
        assert_eq!(
            heal_while_reading(&[3, 4, 5, 6, 7], faces, &no_bases, &[]),
            vec![6, 7, 5, 4, 3]
        );
    }
}
