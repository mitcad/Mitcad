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

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
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
use crate::kernel::exchange::ffi::f3d_build_body;
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
    built: Option<Option<SharedPtr<Shape>>>,
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
}

/// The bodies of one document of an .f3d or .f3z file.
pub struct F3dGeometry {
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
}

impl F3dGeometry {
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
        let mut geometry = F3dGeometry {
            document: document.to_owned(),
            blobs: Vec::new(),
            variants: Vec::new(),
            states: Vec::new(),
            history: Vec::new(),
            plain: Vec::new(),
            item_states: HashMap::new(),
            item_components: HashMap::new(),
            component_names,
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
                    // the bodies after operation k.
                    let mut newest_first: Vec<(i64, Vec<ConvertedBody>)> = Vec::new();
                    let mut ops: Vec<(i64, usize)> = Vec::new();
                    for k in 0..=h.states.len() {
                        let view = h.view(k);
                        let bodies: Vec<ConvertedBody> = records
                            .iter()
                            .filter(|r| view.get(r) != Some(&None))
                            .map(|&r| {
                                if k == 0 {
                                    convert::convert_body(&file, r, &options)
                                } else {
                                    convert::convert_body_at(&file, r, &options, Some(&view))
                                }
                            })
                            .collect();
                        let time = h.states.get(k).map_or(i64::MIN, |s| s.id);
                        let same = newest_first.last().is_some_and(|(_, last)| {
                            last.len() == bodies.len()
                                && last.iter().zip(&bodies).all(|(a, b)| a.body == b.body)
                        });
                        if same {
                            // The older state made it: keep the older time.
                            newest_first.last_mut().expect("checked").0 = time;
                        } else {
                            newest_first.push((time, bodies));
                        }
                        if let Some(s) = h.states.get(k) {
                            ops.push((s.id, newest_first.len() - 1));
                        }
                    }
                    let count = newest_first.len();
                    b.ops = ops.into_iter().map(|(id, j)| (id, count - 1 - j)).collect();
                    // A body as it was in the state before is that state's
                    // variant: built and measured once.
                    let mut previous: Vec<usize> = Vec::new();
                    for (time, bodies) in newest_first.into_iter().rev() {
                        let ids: Vec<usize> = bodies
                            .into_iter()
                            .map(|body| {
                                let same = previous.iter().copied().find(|&v| {
                                    let old = &geometry.variants[v].body;
                                    old.record == body.record && old.body == body.body
                                });
                                same.unwrap_or_else(|| geometry.variant(index, body))
                            })
                            .collect();
                        previous.clone_from(&ids);
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
        // of the ASM state of the item's result).
        let results: Vec<(i64, i64)> = dump
            .and_then(|d| d.timeline.as_ref())
            .and_then(|t| t.items.as_ref())
            .into_iter()
            .flatten()
            .filter_map(|item| {
                let result = item.f3d.as_ref()?.result_no?;
                Some((item.index?, i64::from(result)))
            })
            .filter(|(_, r)| *r > 0)
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
        self.variants.push(Variant {
            blob,
            body,
            built: None,
        });
        self.variants.len() - 1
    }

    /// The history blobs' states in the order of the timeline: each item
    /// whose result is an operation of the history moves the blobs that
    /// have it to the state after it. Operations no item names come one
    /// state each, those of a blob before the next named one, the rest at
    /// the end (components inserted from other designs bring a history of
    /// their own). False when no item names an operation.
    fn merge_by_timeline(&mut self, items: &[(i64, i64)]) -> bool {
        let mut at: HashMap<i64, Vec<(usize, usize)>> = HashMap::new();
        for (h, &blob) in self.history.iter().enumerate() {
            for (&id, &s) in &self.blobs[blob].ops {
                at.entry(id).or_default().push((h, s));
            }
        }
        if !items.iter().any(|(_, r)| at.contains_key(r)) {
            return false;
        }
        let mut current = vec![0usize; self.history.len()];
        self.states.push(current.clone());
        for &(index, result) in items {
            let Some(targets) = at.get(&result) else {
                continue;
            };
            let mut components: Vec<u64> = targets
                .iter()
                .filter_map(|&(h, _)| self.blobs[self.history[h]].component)
                .collect();
            components.sort_unstable();
            components.dedup();
            self.item_components.insert(index, components);
            let mut moved = false;
            let mut back = false;
            for &(h, s) in targets {
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

    fn build(&mut self, variant: usize) -> Option<SharedPtr<Shape>> {
        if let Some(built) = &self.variants[variant].built {
            return built.clone();
        }
        let v = &self.variants[variant];
        let blob = &self.blobs[v.blob];
        let source = Source {
            document: &self.document,
            blob: &blob.entry,
            asm_version: &blob.asm_version,
            history: !self.plain.contains(&v.blob),
            history_step: 0,
        };
        let data = to_ffi(&source, &v.body);
        let shape = f3d_build_body(&data);
        let shape = (!shape.is_null()).then_some(shape);
        self.variants[variant].built = Some(shape.clone());
        shape
    }

    /// The bodies of variants that build, with the number of those that do
    /// not (or lost faces in the conversion).
    fn bodies(&mut self, variants: &[usize]) -> (Vec<StoredBody<SharedPtr<Shape>>>, usize) {
        let mut broken = 0;
        let mut bodies = Vec::new();
        for &v in variants {
            let Some(shape) = self.build(v) else {
                broken += 1;
                continue;
            };
            let variant = &self.variants[v];
            if self.lost_faces(v) {
                broken += 1;
            }
            let blob = &self.blobs[variant.blob];
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

impl StoredGeometry<SharedPtr<Shape>> for F3dGeometry {
    fn state_count(&mut self) -> usize {
        self.states.len()
    }

    fn item_state(&mut self, index: i64) -> Option<usize> {
        self.item_states.get(&index).copied()
    }

    fn item_components(&mut self, index: i64) -> Option<Vec<u64>> {
        self.item_components
            .get(&index)
            .filter(|c| !c.is_empty())
            .cloned()
    }

    fn component_names(&mut self) -> HashMap<u64, String> {
        self.component_names.clone()
    }

    fn state(&mut self, index: usize) -> Result<Vec<StoredBody<SharedPtr<Shape>>>, String> {
        if index >= self.states.len() {
            return Err(format!("there is no history state {index}"));
        }
        let variants = self.state_variants(index);
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
        let variants = match self.states.len().checked_sub(1) {
            Some(last) => {
                let mut variants = self.state_variants(last);
                variants.extend(self.history_less_bodies(&variants));
                variants
            }
            None => self
                .plain
                .iter()
                .flat_map(|&b| self.blobs[b].states[0].1.iter().copied())
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
}

/// A value moved to the thread that waits for it.
///
/// SAFETY: only the document of an import thread and its result cross,
/// once the thread is done with them (they are sent as its last act). The
/// document's shapes are C++ objects behind `shared_ptr`s and OCCT handles,
/// whose reference counts are atomic; nothing else refers to them.
struct Handover<T>(T);

unsafe impl<T> Send for Handover<T> {}

/// How many times an import that hung is run again.
const HANG_RETRIES: usize = 8;

/// The stack of an import thread (the main thread's is 8 MiB on Linux).
const IMPORT_STACK: usize = 256 << 20;

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
                _ => document.import_try(
                    path,
                    options,
                    &Try {
                        time_limit: options.time_limit,
                        no_compare: options.no_compare,
                        stop,
                        ..Try::default()
                    },
                ),
            }
        })
    }

    /// The import on a thread of its own, watched: when it makes no
    /// progress for `limit` seconds (a kernel call that does not return),
    /// the thread is left to itself and the import runs again on a new
    /// document with the item it hung on taking the file's bodies; hung while
    /// comparing the final bodies, again comparing volumes only; hung
    /// elsewhere, with the stored bodies as they are. A left thread keeps
    /// its processor until the program ends. A try that hung after a stop
    /// runs again stopped where it stopped (where it hung, when it had not
    /// seen the stop yet).
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
        let mut warnings = Vec::new();
        for _ in 0..=HANG_RETRIES {
            // Each try has the whole time limit: the items before the one
            // the kernel hung on are replayed again.
            attempt.time_limit = options.time_limit;
            let progress = Arc::new(mitcad_import::Progress::new());
            attempt.progress = Some(progress.clone());
            let (sender, receiver) = std::sync::mpsc::channel();
            let (path_owned, options_owned, try_owned) =
                (path.to_owned(), options.clone(), attempt.clone());
            std::thread::Builder::new()
                .name("mitcad-import".to_owned())
                // OCCT recurses deeply (booleans, fillets); the default
                // thread stack of 2 MiB overflows where the main thread's
                // does not, and the overflow is a failed feature.
                .stack_size(IMPORT_STACK)
                .spawn(move || {
                    let mut document = Document(mitcad_model::Document::new(OcctKernel));
                    let result = document.import_try(&path_owned, &options_owned, &try_owned);
                    drop(try_owned);
                    let _ = sender.send(Handover((document, result)));
                })
                .map_err(|e| ApiError(format!("cannot start the import: {e}")))?;
            let mut step = u64::MAX;
            let mut moved = Instant::now();
            let hung = loop {
                match receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok(Handover((document, result))) => {
                        let mut report = result?;
                        if let Some(design) = report.designs.first_mut() {
                            design.warnings.extend(warnings);
                        }
                        *self = document;
                        return Ok(report);
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        let now = progress.step.load(Ordering::Relaxed);
                        if now != step {
                            step = now;
                            moved = Instant::now();
                        } else if moved.elapsed().as_secs_f64() > limit {
                            break progress.item.load(Ordering::Relaxed);
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        return Err(ApiError("the import stopped without a result".to_owned()));
                    }
                }
            };
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

    /// One try of the import into this document.
    fn import_try(
        &mut self,
        path: &str,
        options: &TimelineOptions,
        attempt: &Try,
    ) -> Result<mitcad_import::ImportReport, ApiError> {
        let fail = |e: String| ApiError(e);
        // The import tries many definitions on imported geometry; an OCCT
        // crash in one of them is then an error of that attempt.
        crate::kernel::exchange::ffi::catch_occt_crashes();
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
        let (label, doc, decoded) = &designs[chosen];
        // Without a design segment, the stored bodies come in as they are
        // (the importer compares and replaces the empty replay).
        let bodies_only = || Dump {
            source: Some(mitcad_f3d::design::ir::Source {
                mode: Some("f3d_stream".to_owned()),
                file: Some(label.clone()),
                ..Default::default()
            }),
            ..Dump::default()
        };
        let dump = match &options.dump {
            _ if attempt.bodies_only => bodies_only(),
            Some(dump_path) => {
                let text = std::fs::read_to_string(dump_path)
                    .map_err(|e| ApiError(format!("{dump_path}: {e}")))?;
                Dump::from_json(&text).map_err(|e| ApiError(format!("{dump_path}: {e}")))?
            }
            None => decoded.clone().unwrap_or_else(bodies_only),
        };
        let mut geometry = F3dGeometry::new(doc, label, decoded.as_ref()).map_err(fail)?;
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
            ..mitcad_import::Options::default()
        };
        let design =
            mitcad_import::import_design(&mut self.0, &dump, &mut geometry, &import_options);
        let mut report = mitcad_import::ImportReport {
            file: file_name(path).to_owned(),
            designs: vec![design],
        };
        if designs.len() > 1 {
            let others: Vec<String> = designs
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != chosen)
                .map(|(_, (l, _, _))| l.clone())
                .collect();
            report.designs[0].warnings.push(format!(
                "other designs in the package: {}",
                others.join(", ")
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
