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
//! Bodies are converted to the neutral model when the file is opened and
//! built with OCCT only when the importer asks for a state.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use cxx::SharedPtr;
use mitcad_f3d::F3dFile;
use mitcad_f3d::asm::AsmFile;
use mitcad_f3d::asm::history::History;
use mitcad_f3d::convert::{self, ConvertedBody, Options as ConvertOptions};
use mitcad_f3d::design::ir::Dump;
use mitcad_import::{StoredBody, StoredGeometry};
use mitcad_model::RecomputeMonitor;
use mitcad_model::api::ApiError;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::Document;
use crate::brep_import::{Source, to_ffi};
use crate::kernel::exchange::ffi::{brep_data, f3d_build_body, f3d_read_body};
use crate::kernel::{OcctKernel, Shape};

/// Faces, edges, vertices and the rounded sum of the vertices of a body.
type BodyKey = (usize, usize, usize, [i64; 3]);

/// The largest distance (mm) from an edge's end to its vertex that a body
/// of a history state may have and still be built.
const MAX_VERTEX_GAP: f64 = 1e-2;

/// A body of a blob at some state.
struct Variant {
    blob: usize,
    body: ConvertedBody,
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

/// The bodies of one document of an .f3d or .f3z file, converted to the
/// neutral model: read once for all tries of an import (mitcad#80), each of
/// which builds them with OCCT as it needs them ([`F3dGeometry`]).
pub struct StoredFile {
    document: String,
    blobs: Vec<Blob>,
    variants: Vec<Variant>,
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
    /// Component names by object id, from the file's design streams.
    component_names: HashMap<u64, String>,
    /// The bodies a try is building, and those a try the watchdog gave up
    /// while it built them built after all, by variant (mitcad#82): another
    /// try waits for a build in progress and reads the body from its B-rep
    /// data rather than building it again. Healing one body of 23 000 faces
    /// took 84 s, 74 s of it in one call of OCCT's, longer than a hang
    /// limit of 60 s: the next try got the body when the call returned.
    builds: Mutex<HashMap<usize, SharedBuild>>,
    /// Notified whenever a build in `builds` ends.
    build_ended: Condvar,
}

/// A body's build shared by the tries of an import ([`StoredFile::builds`]).
enum SharedBuild {
    /// A try is building it.
    InProgress,
    /// Built by a try the watchdog gave up meanwhile: its B-rep data (None:
    /// it did not build).
    Given(Option<Arc<Vec<u8>>>),
}

/// What a try does about a body ([`StoredFile::claim`]).
enum Claim {
    /// Builds it, as no other try does.
    Build,
    /// Reads the B-rep data a try the watchdog gave up built (None: it did
    /// not build).
    Given(Option<Arc<Vec<u8>>>),
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
                    builds = self
                        .build_ended
                        .wait(builds)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
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
    /// links blobs to components.
    pub fn new(doc: &F3dFile, document: &str, dump: Option<&Dump>) -> Result<Self, String> {
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
            states: Vec::new(),
            history: Vec::new(),
            plain: Vec::new(),
            item_states: HashMap::new(),
            item_components: HashMap::new(),
            component_names,
            builds: Mutex::new(HashMap::new()),
            build_ended: Condvar::new(),
        };
        for blob in doc.body_blobs() {
            let data = doc
                .read(&blob.entry)
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
            match history {
                Some(h) => {
                    // From the current state back: k operations undone, so
                    // the bodies after operation k. A body as it is in the
                    // state after is that state's (one variant, built and
                    // measured once): each state keeps only the bodies that
                    // differ, so the converted bodies take the memory of the
                    // distinct ones, not of every body in every state
                    // (mitcad#80: gigabytes for a long history of many
                    // bodies).
                    let mut distinct: Vec<ConvertedBody> = Vec::new();
                    let mut newest_first: Vec<(i64, Vec<usize>)> = Vec::new();
                    let mut ops: Vec<(i64, usize)> = Vec::new();
                    for k in 0..=h.states.len() {
                        let view = h.view(k);
                        let kept = distinct.len();
                        let after: &[usize] = newest_first.last().map_or(&[], |(_, ids)| ids);
                        let mut bodies: Vec<usize> = Vec::new();
                        for &r in records.iter().filter(|r| view.get(r) != Some(&None)) {
                            let body = if k == 0 {
                                convert::convert_body(&file, r, &options)
                            } else {
                                convert::convert_body_at(&file, r, &options, Some(&view))
                            };
                            let same = after.iter().copied().find(|&d| {
                                let old = &distinct[d];
                                old.record == body.record && old.body == body.body
                            });
                            bodies.push(same.unwrap_or_else(|| {
                                distinct.push(body);
                                distinct.len() - 1
                            }));
                        }
                        let time = h.states.get(k).map_or(i64::MIN, |s| s.id);
                        let same = newest_first.last().is_some_and(|(_, last)| {
                            last.len() == bodies.len()
                                && last
                                    .iter()
                                    .zip(&bodies)
                                    .all(|(&a, &b)| a == b || distinct[a].body == distinct[b].body)
                        });
                        if same {
                            // The older state made it: keep the older time.
                            newest_first.last_mut().expect("checked").0 = time;
                            distinct.truncate(kept);
                        } else {
                            newest_first.push((time, bodies));
                            // The operation undone last changed the bodies.
                            if let Some(s) = k.checked_sub(1).and_then(|j| h.states.get(j)) {
                                b.changed.insert(s.id);
                            }
                        }
                        if let Some(s) = h.states.get(k) {
                            ops.push((s.id, newest_first.len() - 1));
                        }
                    }
                    let count = newest_first.len();
                    b.ops = ops.into_iter().map(|(id, j)| (id, count - 1 - j)).collect();
                    // The variants in the order of the states, oldest first.
                    let mut distinct: Vec<Option<ConvertedBody>> =
                        distinct.into_iter().map(Some).collect();
                    let mut variants: Vec<Option<usize>> = vec![None; distinct.len()];
                    for (time, bodies) in newest_first.into_iter().rev() {
                        let ids: Vec<usize> = bodies
                            .into_iter()
                            .map(|d| {
                                *variants[d].get_or_insert_with(|| {
                                    let body = distinct[d].take().expect("taken once");
                                    geometry.variant(index, body)
                                })
                            })
                            .collect();
                        b.states.push((time, ids));
                    }
                    geometry.history.push(index);
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
                }
            }
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
                Some(ItemResult {
                    index: item.index?,
                    result: i64::from(f3d.result_no?),
                    owner: f3d.component,
                })
            })
            .filter(|r| r.result > 0)
            .collect();
        geometry.link_histories();
        if !geometry.merge_by_timeline(&results) {
            geometry.merge();
        }
        Ok(geometry)
    }

    /// The components of the history blobs (F6). The design streams link
    /// only the `.smb` blobs to components, but every component has its
    /// own `.smb`/`.smbh` pair, and most `.smbh` bodies are also in the
    /// `.smb`, as they are or as they were earlier in the history
    /// (ASM_FORMAT.md): a history blob belongs to the component of the
    /// `.smb` blob that holds most of its bodies of any state.
    /// What tells a body of a blob from the others: its numbers of faces,
    /// edges and vertices and the sum of its vertices.
    fn body_key(&self, v: usize) -> BodyKey {
        let body = &self.variants[v].body.body;
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
            .filter(|&v| {
                self.variants[v].body.body.is_solid() && !known.contains(&self.body_key(v))
            })
            .collect()
    }

    fn link_histories(&mut self) {
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
        self.variants.push(Variant { blob, body });
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
            let mut moved = false;
            let mut back = false;
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
        let body = &self.variants[v].body;
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
            last.iter().any(|&w| {
                let stored = &self.variants[w].body;
                stored.record == body.record && stored.body.is_solid()
            })
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
        };
        self.built[variant] = Some(shape.clone());
        shape
    }

    /// Builds the shape of a variant from the file's data with OCCT.
    fn heal(&self, variant: usize) -> Option<SharedPtr<Shape>> {
        let file = &self.file;
        let v = &file.variants[variant];
        let blob = &file.blobs[v.blob];
        let source = Source {
            document: &file.document,
            blob: &blob.entry,
            asm_version: &blob.asm_version,
            history: !file.plain.contains(&v.blob),
            history_step: 0,
        };
        let data = to_ffi(&source, &v.body);
        let clock = Instant::now();
        BUILD_PROGRESS.with(|p| p.set(Some((clock, 0.0))));
        // A build longer than the hang limit that returns, for the tests
        // (mitcad#82): the file's first body takes 1.5 s, without progress.
        if variant == 0 && std::env::var_os("MITCAD_IMPORT_STALL").is_some_and(|s| s == "slow") {
            std::thread::sleep(Duration::from_millis(1500));
        }
        let shape = f3d_build_body(&data);
        let seconds = clock.elapsed().as_secs_f64();
        let still = BUILD_PROGRESS
            .with(Cell::take)
            .map_or(seconds, |(last, longest)| {
                longest.max(last.elapsed().as_secs_f64())
            });
        if seconds >= 1.0 {
            self.trace_body(
                variant,
                &format!("built in {seconds:.2} s, at most {still:.2} s without progress"),
            );
        }
        (!shape.is_null()).then_some(shape)
    }

    /// Traces what happened to a variant's body (`MITCAD_IMPORT_TRACE`).
    fn trace_body(&self, variant: usize, what: &str) {
        if std::env::var_os("MITCAD_IMPORT_TRACE").is_none() {
            return;
        }
        let v = &self.file.variants[variant];
        let entry = &self.file.blobs[v.blob].entry;
        eprintln!(
            "import: body {}#{} ({} faces) {what}",
            entry.rsplit('/').next().unwrap_or(entry),
            v.body.record,
            v.body.body.faces.len()
        );
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
                source: format!("{name}#{}", variant.body.record),
                component: blob.component,
                id: Some(v as u64),
            });
        }
        (bodies, broken)
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
    /// it); the one with the longest timeline when left out.
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
struct Handover<T>(T);

unsafe impl<T> Send for Handover<T> {}

/// Import threads the hang watchdog gave up that still run (mitcad#82):
/// see [`abandoned_imports`].
static ABANDONED_RUNNING: AtomicUsize = AtomicUsize::new(0);

/// How many import threads the hang watchdog gave up still run: their
/// geometry kernel call has not returned yet. A process that ends while
/// one runs must end without its static destructors (C++ `std::_Exit`
/// once its output is written; `mitcad-cli`, the application's import
/// worker): the shared libraries' finalizers tear down OCCT's state under
/// the thread, which then crashes (mitcad#82: a segmentation fault at exit
/// on Linux after an import that hung, the thread healing a body).
pub fn abandoned_imports() -> usize {
    ABANDONED_RUNNING.load(Ordering::SeqCst)
}

/// A try's thread runs, has finished, or was given up while it ran.
const TRY_RUNNING: u8 = 0;
const TRY_FINISHED: u8 = 1;
const TRY_ABANDONED: u8 = 2;

/// Whether a try's thread runs, counted in [`ABANDONED_RUNNING`] once the
/// watchdog gave it up ([`TryThread::abandon`]) until it ends.
#[derive(Clone)]
struct TryThread(Arc<AtomicU8>);

impl TryThread {
    fn new() -> Self {
        Self(Arc::new(AtomicU8::new(TRY_RUNNING)))
    }

    /// The watchdog gave the try up: counted until its thread ends.
    fn abandon(&self) {
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
struct TryEnd(TryThread);

impl Drop for TryEnd {
    fn drop(&mut self) {
        if self.0.0.swap(TRY_FINISHED, Ordering::SeqCst) == TRY_ABANDONED {
            ABANDONED_RUNNING.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// How many times an import that hung is run again.
const HANG_RETRIES: usize = 8;

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
fn spawn_import<F>(run: F) -> std::io::Result<usize>
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
const MEMORY_POLL: Duration = Duration::from_millis(100);

/// The import's memory guard (mitcad#80): measures the process against
/// its limits (`memory::memory_use`, and the import's own
/// `memory_limit`) and tells a try when memory gets tight or low.
struct MemoryGuard {
    /// The import's own limit in bytes.
    limit: Option<u64>,
    /// The share at which the try is next told that memory is tight.
    next_tight: f64,
}

impl MemoryGuard {
    fn new(options: &TimelineOptions) -> Self {
        Self {
            limit: options
                .memory_limit
                .filter(|m| *m > 0.0)
                .map(|m| (m * f64::from(1u32 << 20)) as u64),
            next_tight: MEMORY_TIGHT,
        }
    }

    fn check(&mut self, progress: &mitcad_import::Progress) {
        let memory = crate::memory::memory_use().within(self.limit);
        let share = memory.share();
        let text = memory.describe();
        progress.set_memory(&text);
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
    fn around<T>(mut self, progress: &Arc<mitcad_import::Progress>, f: impl FnOnce() -> T) -> T {
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
    let documents = mitcad_f3d::design::documents(std::path::Path::new(path)).map_err(fail)?;
    if documents.is_empty() {
        return Err(ApiError(format!("{path} holds no design")));
    }
    // The design and its dump.
    let mut designs = Vec::new();
    for (label, doc) in &documents {
        let design = mitcad_f3d::design::decode_document(doc, label).map_err(fail)?;
        designs.push((label.clone(), doc, design.dump));
    }
    let chosen = match &options.design {
        Some(wanted) => designs
            .iter()
            .position(|(label, _, _)| label == wanted || label.ends_with(&format!("!{wanted}")))
            .ok_or_else(|| {
                let labels: Vec<&str> = designs.iter().map(|(l, _, _)| l.as_str()).collect();
                ApiError(format!(
                    "no design {wanted}; the file has {}",
                    labels.join(", ")
                ))
            })?,
        None => (0..designs.len())
            .max_by_key(|&i| {
                designs[i]
                    .2
                    .as_ref()
                    .map_or(0, |d| d.timeline_items().len())
            })
            .unwrap_or(0),
    };
    let others: Vec<String> = designs
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != chosen)
        .map(|(_, (l, _, _))| l.clone())
        .collect();
    let (label, doc, decoded) = designs.swap_remove(chosen);
    drop(designs);
    let file = StoredFile::new(doc, &label, decoded.as_ref()).map_err(fail)?;
    let dump = match &options.dump {
        Some(dump_path) => {
            let text = std::fs::read_to_string(dump_path)
                .map_err(|e| ApiError(format!("{dump_path}: {e}")))?;
            Some(Dump::from_json(&text).map_err(|e| ApiError(format!("{dump_path}: {e}")))?)
        }
        None => decoded,
    };
    Ok(Prepared {
        label,
        dump,
        file: Arc::new(file),
        others,
    })
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
    let documents = mitcad_f3d::design::documents(std::path::Path::new(path)).map_err(ApiError)?;
    let mut designs = Vec::new();
    for (label, doc) in &documents {
        let design = mitcad_f3d::design::decode_document(doc, label).map_err(ApiError)?;
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
        let report = self.import_timeline(path, &options)?;
        let json = serde_json::to_string_pretty(&report).expect("the report serializes");
        if let Some(file) = &options.report_path {
            std::fs::write(file, format!("{json}\n"))
                .map_err(|e| ApiError(format!("{file}: {e}")))?;
        }
        Ok(if options.text { report.text() } else { json })
    }

    pub(crate) fn import_timeline(
        &mut self,
        path: &str,
        options: &TimelineOptions,
    ) -> Result<mitcad_import::ImportReport, ApiError> {
        // A job's cancel stops it early (T1e): the modelling items from the
        // one being replayed on take the file's bodies, and the document is
        // the import's all the same. The job's monitor is the import's stop
        // request, not the document's (Document::without_job).
        let stop = self.0.monitor().cloned();
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
                        document.import_try(&prepared, path, options, &attempt)
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
            let (path_owned, options_owned, try_owned, prepared_shared) = (
                path.to_owned(),
                options.clone(),
                attempt.clone(),
                prepared.clone(),
            );
            let watched = progress.clone();
            let thread = TryThread::new();
            let ended = TryEnd(thread.clone());
            let stack = spawn_import(move || {
                // Dropped last, once the try's document and shapes are gone.
                let _ended = ended;
                let mut document = Document(mitcad_model::Document::new(OcctKernel));
                let result =
                    document.import_try(&prepared_shared, &path_owned, &options_owned, &try_owned);
                drop(prepared_shared);
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

    /// One try of the import of a prepared file into this document.
    fn import_try(
        &mut self,
        prepared: &Prepared,
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
            source: Some(mitcad_f3d::design::ir::Source {
                mode: Some("f3d_stream".to_owned()),
                file: Some(prepared.label.clone()),
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
            ..mitcad_import::Options::default()
        };
        let design =
            mitcad_import::import_design(&mut self.0, dump, &mut geometry, &import_options);
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
