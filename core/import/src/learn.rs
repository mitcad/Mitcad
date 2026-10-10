// SPDX-License-Identifier: MIT
//! The learning dump (mitcad#96): every item whose definition the history
//! settled is a labelled example of what the stream decoder does not read
//! yet (which profile regions, which edges, which direction, ...).
//!
//! With [`Options::learn`](crate::Options::learn) (`--learn <dir>`, or
//! `MITCAD_IMPORT_LEARN` in `mitcad-ffi`), the import keeps, per modelling
//! item, the candidate definitions it ranked and the one the history
//! accepted, and writes one JSON line per item to
//! `<dir>/settled/<design>.jsonl` (accepted against a known history state)
//! or `<dir>/unsettled/<design>.jsonl` (none accepted, or accepted
//! unchecked): the item's raw record ([`mitcad_f3d::design::learn`]), what
//! the decoder already made of it, the candidate entities in the file's
//! own terms (sketch regions by the file's curve ids, curves with the ids
//! the sketch stores, bodies, edges) and the candidates in the same terms.
//! Fillet and chamfer failures with no candidates also retain their input
//! diagnostics in unsettled records (mitcad#106).
//! `tools/f3d-learn/learn.py` tests hypotheses on these files without
//! geometry. The format is in `core/import/README.md` (*Learning the
//! undecoded inputs*).
//!
//! The import only reports to this module: [`Importer::learn_offered`]
//! when an item's candidates are translated, [`Importer::learn_ranked`]
//! when they are ranked, [`Importer::learn_accepted`] when one is
//! accepted and [`Importer::learn_write`] at the end.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use mitcad_f3d::design::ir::{Detail, TimelineItem};
use mitcad_model::FeatureUid;
use serde_json::{Map, Value, json};

use crate::{Candidate, Importer, history::Sig, refs};

/// Where the learning dump goes.
#[derive(Debug, Clone, Default)]
pub struct LearnOptions {
    /// The dataset directory (`settled/`, `unsettled/` under it).
    pub dir: PathBuf,
    /// The `.f3d` or `.f3z` file imported, for the items' raw records
    /// (without it the lines have no `record`).
    pub path: Option<PathBuf>,
}

/// The most bodies listed in an item's context.
const MAX_BODIES: usize = 200;

/// The most decoded inputs resolved per item.
const MAX_RESOLVED: usize = 64;

/// The most matching replay edges listed for one decoded edge input.
/// Keep ambiguity diagnostics bounded even when many copies coincide.
const MAX_EDGE_MATCHES: usize = 16;

/// The most candidates listed per item (the first in the order tried; the
/// count and the accepted one's rank are of all): items with tens of
/// thousands of candidates took gigabytes.
const MAX_CANDIDATES: usize = 1000;

/// The fingerprints (bodies, faces, edges with the decoder's `_f3d`) in a
/// value: (JSON pointer, kind, the fingerprint's map).
fn fingerprints(v: &Value, at: &str, out: &mut Vec<(String, String, Map<String, Value>)>) {
    match v {
        Value::Object(m) => {
            let kind = m.get("kind").and_then(Value::as_str).unwrap_or("");
            if matches!(kind, "body" | "face" | "edge") && m.contains_key("_f3d") {
                out.push((at.to_owned(), kind.to_owned(), m.clone()));
                return;
            }
            for (k, x) in m {
                let k = k.replace('~', "~0").replace('/', "~1");
                fingerprints(x, &format!("{at}/{k}"), out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                fingerprints(x, &format!("{at}/{i}"), out);
            }
        }
        _ => {}
    }
}

/// Version of the line format.
pub const SCHEMA: u32 = 1;

/// What the import told about one item.
#[derive(Debug, Default)]
struct Entry {
    /// The candidates in the order they are tried (ranked), each once.
    candidates: Vec<Candidate>,
    /// The [`key`]s of all the candidates, in the order tried (the first
    /// [`MAX_CANDIDATES`] are `candidates`), and as a set.
    order: Vec<u64>,
    keys: HashSet<u64>,
    /// The accepted candidate and whether the history checked it.
    accepted: Option<(Candidate, Option<bool>)>,
    /// Sketch feature uid → (timeline index, sketch context) for the
    /// sketches the candidates use.
    sketches: BTreeMap<String, (i64, Value)>,
    /// The bodies before the item.
    bodies: Vec<Value>,
    /// Edges named by the candidates, with their geometry.
    edges: BTreeMap<String, Value>,
    /// The bodies, faces and edges the decoder gave for the item (JSON
    /// pointers into `known`) as the replay's.
    resolved: Map<String, Value>,
    /// Dressup matcher results for decoded edges, at the same pointers.
    /// Unlike `resolved`, this retains ambiguity and split-edge matches.
    edge_matches: Map<String, Value>,
    /// The most recent failed dressup translation, including failures
    /// that offered no candidates (mitcad#106).
    translation_error: Option<String>,
    /// Seconds from the first translation to the acceptance.
    started: Option<Instant>,
    seconds: Option<f64>,
}

/// The items told about in an import.
#[derive(Debug, Default)]
pub(crate) struct Store {
    items: BTreeMap<i64, Entry>,
    /// The item last translated (ranking applies to it).
    current: Option<i64>,
    /// Nanoseconds the dump's own work took ([`Clock`]).
    paused: Arc<AtomicU64>,
}

/// A stopwatch for an item's time that leaves out the learning dump's own
/// work (the bodies, inputs and sketches it records when an item's
/// definitions are translated): with the dump, items get the time they get
/// without it.
#[derive(Clone, Debug)]
pub(crate) struct Clock {
    started: Instant,
    /// The dump's work so far, and when the clock started.
    paused: Option<(Arc<AtomicU64>, u64)>,
}

impl Clock {
    pub(crate) fn elapsed(&self) -> Duration {
        let elapsed = self.started.elapsed();
        match &self.paused {
            Some((paused, at)) => elapsed.saturating_sub(Duration::from_nanos(
                paused.load(Ordering::Relaxed).saturating_sub(*at),
            )),
            None => elapsed,
        }
    }
}

/// The dump directory: the options', else none.
pub(crate) fn store(options: &crate::Options) -> Option<Store> {
    options.learn.as_ref().map(|_| Store::default())
}

/// A hash of a candidate's definitions: items have tens of thousands of
/// candidates, which comparing pairwise took minutes.
fn key(c: &Candidate) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(&c.defs)
        .unwrap_or_default()
        .hash(&mut h);
    h.finish()
}

impl<K: crate::ImportKernel> Importer<'_, K> {
    /// A stopwatch started now that leaves out the dump's work ([`Clock`]).
    pub(crate) fn clock(&self) -> Clock {
        Clock {
            started: Instant::now(),
            paused: self.learn.as_ref().map(|s| {
                let at = s.paused.load(Ordering::Relaxed);
                (Arc::clone(&s.paused), at)
            }),
        }
    }

    /// [`Importer::learn_offered`], its time left out of the item's and
    /// the import's ([`Clock`]; the item's deadline and the start of the
    /// import's time limit move by it).
    pub(crate) fn learn_offered_paused(
        &mut self,
        index: i64,
        item: &TimelineItem,
        translated: &Result<Vec<Candidate>, String>,
    ) {
        if self.learn.is_none() {
            return;
        }
        let started = Instant::now();
        self.learn_offered(index, item, translated);
        let spent = started.elapsed();
        if let Some(s) = &self.learn {
            let nanos = u64::try_from(spent.as_nanos()).unwrap_or(u64::MAX);
            s.paused.fetch_add(nanos, Ordering::Relaxed);
        }
        if let Some(d) = self.deadline.as_mut() {
            *d += spent;
        }
        self.started += spent;
    }

    /// An item's candidates were translated (`index`, before they are
    /// ranked); repeated translations add the new ones.
    pub(crate) fn learn_offered(
        &mut self,
        index: i64,
        item: &TimelineItem,
        translated: &Result<Vec<Candidate>, String>,
    ) {
        if self.learn.is_none() {
            return;
        }
        let dressup = matches!(item.object_type(), Some("FilletFeature" | "ChamferFeature"));
        let candidates = match translated {
            Ok(candidates) => candidates.as_slice(),
            Err(_) if dressup => &[],
            Err(_) => return,
        };
        let mut sketches = std::collections::BTreeSet::new();
        let mut bodies_named = HashSet::new();
        let mut edges_named = HashSet::new();
        for c in candidates {
            for def in &c.defs {
                collect(def, &mut sketches, &mut bodies_named, &mut edges_named);
            }
        }
        let first = !self
            .learn
            .as_ref()
            .is_some_and(|s| s.items.contains_key(&index));
        let (bodies, (resolved, edge_matches)) = if first {
            (self.learn_bodies(), self.learn_resolved(item))
        } else {
            (Vec::new(), (Map::new(), Map::new()))
        };
        let sketch_context: Vec<(String, (i64, Value))> = sketches
            .into_iter()
            .filter_map(|uid| Some((uid.clone(), self.learn_sketch(&uid)?)))
            .collect();
        let known_edges: HashSet<String> = self
            .learn
            .as_ref()
            .and_then(|s| s.items.get(&index))
            .map(|e| e.edges.keys().cloned().collect())
            .unwrap_or_default();
        edges_named.retain(|e| !known_edges.contains(e));
        let edges = self.learn_edges(&bodies_named, &edges_named);
        let Some(store) = self.learn.as_mut() else {
            return;
        };
        store.current = Some(index);
        let entry = store.items.entry(index).or_default();
        if dressup && let Err(error) = translated {
            entry.translation_error = Some(error.clone());
        }
        if first {
            entry.bodies = bodies;
            entry.resolved = resolved;
            entry.edge_matches = edge_matches;
            entry.started = Some(Instant::now());
        }
        for (uid, ctx) in sketch_context {
            entry.sketches.entry(uid).or_insert(ctx);
        }
        entry.edges.extend(edges);
        for c in candidates {
            crate::tick();
            let k = key(c);
            if entry.keys.insert(k) {
                entry.order.push(k);
                if entry.candidates.len() < MAX_CANDIDATES
                    && entry.candidates.len() + 1 == entry.order.len()
                {
                    entry.candidates.push(c.clone());
                }
            }
        }
    }

    /// The candidates of the item last translated, as ranked: they come
    /// first, in this order; those ranked out are dropped.
    pub(crate) fn learn_ranked(&mut self, ranked: &[Candidate]) {
        let Some(store) = self.learn.as_mut() else {
            return;
        };
        let Some(entry) = store.current.and_then(|i| store.items.get_mut(&i)) else {
            return;
        };
        let mut ranked_keys = HashSet::new();
        let mut order: Vec<u64> = Vec::new();
        let mut kept: HashMap<u64, &Candidate> = HashMap::new();
        for c in ranked {
            let k = key(c);
            if ranked_keys.insert(k) {
                order.push(k);
                kept.insert(k, c);
            }
        }
        // Those translated after the ranked ones (fillet guesses of later
        // states) stay after them.
        let firsts = entry
            .order
            .iter()
            .rposition(|k| ranked_keys.contains(k))
            .map_or(0, |p| p + 1);
        order.extend(
            entry.order[firsts..]
                .iter()
                .filter(|k| !ranked_keys.contains(*k)),
        );
        for (k, c) in entry.order.iter().zip(&entry.candidates) {
            kept.entry(*k).or_insert(c);
        }
        // The first of them, as long as they are kept.
        let candidates: Vec<Candidate> = order
            .iter()
            .take(MAX_CANDIDATES)
            .map_while(|k| kept.get(k).map(|c| (*c).clone()))
            .collect();
        entry.keys = order.iter().copied().collect();
        entry.order = order;
        entry.candidates = candidates;
    }

    /// Candidate `c` of item `index` was accepted (`verified`: checked
    /// against the history).
    pub(crate) fn learn_accepted(&mut self, index: i64, c: &Candidate, verified: Option<bool>) {
        let Some(store) = self.learn.as_mut() else {
            return;
        };
        let entry = store.items.entry(index).or_default();
        entry.seconds = entry.started.map(|s| s.elapsed().as_secs_f64());
        entry.accepted = Some((c.clone(), verified));
    }

    /// The bodies of the replay as they are, by uid and measures (mm).
    fn learn_bodies(&self) -> Vec<Value> {
        let kernel = self.doc.kernel();
        self.doc
            .bodies()
            .iter()
            .take(MAX_BODIES)
            .filter_map(|b| {
                crate::tick();
                let s = Sig::of(kernel, b.shape)?;
                Some(
                    json!({"uid": b.uid.to_string(), "volume": s.volume, "area": s.area,
                            "center": s.center, "faces": s.faces}),
                )
            })
            .collect()
    }

    /// The bodies, faces and edges the item's detail names (fingerprints
    /// with the decoder's names), as the replay's: by JSON pointer into the
    /// item (`/detail/...`), `{"kind", "uid"}` (a body) or `{"kind",
    /// "body", "name"}`, `null` where none is found. At most
    /// [`MAX_RESOLVED`]. Also return the actual dressup matcher's results
    /// for edge inputs, preserving the older `resolved` representation.
    fn learn_resolved(&self, item: &TimelineItem) -> (Map<String, Value>, Map<String, Value>) {
        let mut out = Map::new();
        let mut edge_matches = Map::new();
        let Some(detail) = item.detail.as_ref() else {
            return (out, edge_matches);
        };
        let v = serde_json::to_value(detail).unwrap_or(Value::Null);
        let mut found = Vec::new();
        fingerprints(&v, "/detail", &mut found);
        let mut replay = None;
        for (pointer, kind, map) in found.into_iter().take(MAX_RESOLVED) {
            let fp = mitcad_f3d::design::ir::Fingerprint::from_map(map);
            if kind == "edge" {
                let edges = replay.get_or_insert_with(|| refs::ReplayEdges::of(self.doc));
                edge_matches.insert(
                    pointer.clone(),
                    edge_match_json(
                        refs::dressup_edge_match(edges, &fp),
                        fp.f3d.as_ref().and_then(|f| f.found.as_deref()),
                    ),
                );
            }
            let at = match kind.as_str() {
                "body" => self
                    .body_ref(&fp)
                    .map(|b| json!({"kind": "body", "uid": b.to_string()})),
                "face" => refs::resolve_face(self.doc, &fp).map(
                    |(b, f)| json!({"kind": "face", "body": b.to_string(), "name": f.to_string()}),
                ),
                _ => refs::resolve_edge(self.doc, &fp).map(
                    |(b, e)| json!({"kind": "edge", "body": b.to_string(), "name": e.to_string()}),
                ),
            };
            out.insert(pointer, at.unwrap_or(Value::Null));
        }
        (out, edge_matches)
    }

    /// The geometry of the named edges (mm) on the named bodies.
    fn learn_edges(
        &self,
        bodies: &HashSet<String>,
        edges: &HashSet<String>,
    ) -> Vec<(String, Value)> {
        if edges.is_empty() {
            return Vec::new();
        }
        let kernel = self.doc.kernel();
        let mut out = Vec::new();
        for b in self.doc.bodies() {
            if !bodies.contains(&b.uid.to_string()) {
                continue;
            }
            for (name, mid, length) in refs::edge_midpoints(kernel, b.shape) {
                let name = name.to_string();
                if edges.contains(&name) {
                    out.push((
                        name,
                        json!({"body": b.uid.to_string(), "mid": mid, "length": length}),
                    ));
                }
            }
        }
        out
    }

    /// A sketch's timeline index and its regions in the file's terms.
    fn learn_sketch(&self, uid: &str) -> Option<(i64, Value)> {
        let feature: FeatureUid = uid.parse().ok()?;
        let (index, info) = self
            .sketches
            .iter()
            .find(|(_, s)| s.uid == feature)
            .map(|(i, s)| (*i, s))
            .or_else(|| {
                self.sketch_copies
                    .iter()
                    .find(|(_, (_, s))| s.uid == feature)
                    .map(|((i, _), (_, s))| (*i, s))
            })?;
        let back: HashMap<&str, &str> = info
            .ids
            .iter()
            .map(|(file, ours)| (ours.as_str(), file.as_str()))
            .collect();
        let output = self.doc.sketch_output(feature)?;
        let inside = crate::features::interior_points(&output.region_info);
        let regions: Vec<Value> = output
            .regions
            .iter()
            .zip(&output.region_info)
            .zip(inside)
            .map(|((r, info), inside)| {
                let loops: Vec<Vec<String>> = r
                    .loops
                    .iter()
                    .map(|l| {
                        l.segments
                            .iter()
                            .map(|s| file_ids(&s.key.curve.to_string(), &back))
                            .collect()
                    })
                    .collect();
                json!({
                    "key": file_ids(&r.key.to_string(), &back),
                    "ours": r.key.to_string(),
                    "outer": loops.first().cloned().unwrap_or_default(),
                    "holes": loops.get(1..).unwrap_or_default(),
                    "area": info.area,
                    "centroid": info.centroid,
                    "inside": inside,
                })
            })
            .collect();
        let back: Map<String, Value> = back
            .iter()
            .map(|(ours, file)| ((*ours).to_owned(), json!(file)))
            .collect();
        // The sketch's plane in the component's coordinates (mm), as the
        // import placed it: an extrusion's stored direction is in the same
        // space.
        let f = &output.frame;
        let frame = json!({"origin": f.origin, "x_axis": f.x_axis, "y_axis": f.y_axis,
                           "normal": crate::geom::normal(f)});
        Some((
            index,
            json!({"index": index, "regions": regions, "frame": frame, "ids": back}),
        ))
    }

    /// Writes the dump of the items told about (nothing for a try the
    /// watchdog gave up: its report is not used).
    pub(crate) fn learn_write(&mut self) {
        let Some(store) = self.learn.take() else {
            return;
        };
        let Some(options) = self.options.learn.as_ref() else {
            return;
        };
        if self.abandoned() {
            return;
        }
        // Feature uids to timeline indices.
        let mut to_index: HashMap<String, i64> = self
            .features
            .iter()
            .map(|(i, u)| (u.to_string(), *i))
            .collect();
        for (i, s) in &self.sketches {
            to_index.entry(s.uid.to_string()).or_insert(*i);
        }
        let raw = Raw::open(options.path.as_deref(), &self.report.label);
        let label = self.report.label.clone();
        let mut settled = String::new();
        let mut unsettled = String::new();
        let items = self.dump.timeline_items();
        for (index, entry) in &store.items {
            let Some(item) = items
                .iter()
                .enumerate()
                .find(|(p, it)| it.index.unwrap_or(*p as i64) == *index)
                .map(|(_, it)| it)
            else {
                continue;
            };
            // Candidate-free dressups retain unresolved/ambiguous inputs.
            // Other items accepted without candidates (sketches, datums,
            // joints) keep the existing omission.
            if entry.candidates.is_empty()
                && !matches!(item.object_type(), Some("FilletFeature" | "ChamferFeature"))
            {
                continue;
            }
            let report = self.report.items.iter().find(|r| r.index == *index);
            let line = line(
                &label,
                *index,
                item,
                entry,
                report,
                &to_index,
                raw.as_ref(),
                items,
            );
            let ok = entry
                .accepted
                .as_ref()
                .is_some_and(|(_, v)| *v == Some(true));
            let out = if ok { &mut settled } else { &mut unsettled };
            out.push_str(&serde_json::to_string(&line).expect("serializes"));
            out.push('\n');
        }
        let name = file_name(&label, options.path.as_deref());
        for (sub, text) in [("settled", settled), ("unsettled", unsettled)] {
            let dir = options.dir.join(sub);
            let path = dir.join(&name);
            if text.is_empty() {
                // A rerun leaves no stale lines of an earlier one.
                if path.exists() {
                    let _ = std::fs::write(&path, "");
                }
                continue;
            }
            if let Err(e) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, text))
            {
                self.report
                    .warnings
                    .push(format!("the learning dump {}: {e}", path.display()));
            }
        }
    }
}

/// A bounded diagnostic of the production dressup matcher. The decoder
/// context is evidence, rather than a rule inferred from its prose.
fn edge_match_json(
    result: Result<Vec<refs::FoundEdge>, refs::DressupEdgeFailure>,
    decoder_found: Option<&str>,
) -> Value {
    match result {
        Ok(edges) => json!({
            "status": "matched",
            "decoder_found": decoder_found,
            "count": edges.len(),
            "matches": edges.iter().take(MAX_EDGE_MATCHES).map(|e| {
                json!({"body": e.body.to_string(), "name": e.name.to_string(),
                       "mid": e.mid, "direction": e.direction})
            }).collect::<Vec<_>>(),
        }),
        Err(error) => {
            let candidates = match &error {
                refs::DressupEdgeFailure::AmbiguousInReplay(edges) => edges.as_slice(),
                _ => &[],
            };
            json!({
                "status": error.code(),
                "decoder_found": decoder_found,
                "count": candidates.len(),
                "candidates": candidates.iter().take(MAX_EDGE_MATCHES).map(|(body, name)| {
                    json!({"body": body.to_string(), "name": name.to_string()})
                }).collect::<Vec<_>>(),
            })
        }
    }
}

/// The file name of a design's lines: its label made safe, and a hash of
/// the path so that equal labels in different folders differ.
fn file_name(label: &str, path: Option<&Path>) -> String {
    let safe: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
        .bytes()
    {
        h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
    }
    format!("{safe}-{:08x}.jsonl", h as u32)
}

/// The sketch uids, body uids and edge names a definition names.
fn collect(
    def: &Value,
    sketches: &mut std::collections::BTreeSet<String>,
    bodies: &mut HashSet<String>,
    edges: &mut HashSet<String>,
) {
    match def {
        Value::Object(m) => {
            if let (Some(Value::String(s)), true) = (
                m.get("sketch"),
                m.contains_key("region") || m.contains_key("curve"),
            ) {
                sketches.insert(s.clone());
            }
            if let Some(Value::String(b)) = m.get("body") {
                bodies.insert(b.clone());
            }
            if let Some(Value::Array(e)) = m.get("edges") {
                for x in e {
                    if let Value::String(s) = x {
                        edges.insert(s.clone());
                    }
                }
            }
            for v in m.values() {
                collect(v, sketches, bodies, edges);
            }
        }
        Value::Array(a) => {
            for v in a {
                collect(v, sketches, bodies, edges);
            }
        }
        _ => {}
    }
}

/// Replaces the ids of a word-like token: those `map` names, starting with
/// one of `prefixes` and followed by digits, not after a letter, digit or
/// `.` (so `t3.g0.c1` changes its text id only).
fn replace_ids(s: &str, prefixes: &[char], map: &dyn Fn(&str) -> Option<String>) -> String {
    let b: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        let starts = prefixes.contains(&b[i])
            && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == '.'))
            && b.get(i + 1).is_some_and(char::is_ascii_digit);
        if starts {
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let word: String = b[i..j].iter().collect();
            match map(&word) {
                Some(r) => out.push_str(&r),
                None => out.push_str(&word),
            }
            i = j;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

/// A sketch key in the file's entity ids (`m` before ids the file does not
/// have: entities Mitcad made itself).
fn file_ids(key: &str, back: &HashMap<&str, &str>) -> String {
    replace_ids(key, &['c', 'p', 't'], &|w| {
        Some(
            back.get(w)
                .map_or_else(|| format!("m{w}"), |f| (*f).to_owned()),
        )
    })
}

/// Feature uids (`F12`, also in body uids and names) as timeline indices
/// (`T5`) where an item made them.
fn timeline_ids(s: &str, to_index: &HashMap<String, i64>) -> String {
    replace_ids(s, &['F'], &|w| to_index.get(w).map(|i| format!("T{i}")))
}

/// A definition in the file's terms: feature uids as timeline indices,
/// profile regions as `{"sketch": "T<i>", "region": <index in the
/// sketch's region list>}`.
fn in_file_terms(v: &Value, entry: &Entry, to_index: &HashMap<String, i64>) -> Value {
    match v {
        Value::Object(m) => {
            if let (Some(Value::String(s)), Some(Value::String(r))) =
                (m.get("sketch"), m.get("region"))
                && let Some((_, ctx)) = entry.sketches.get(s)
            {
                let regions = ctx["regions"].as_array();
                let at = regions.and_then(|rs| rs.iter().position(|x| x["ours"] == *r));
                let mut out = Map::new();
                out.insert("sketch".to_owned(), json!(timeline_ids(s, to_index)));
                match at {
                    Some(i) => out.insert("region".to_owned(), json!(i)),
                    None => out.insert("region_key".to_owned(), json!(r)),
                };
                return Value::Object(out);
            }
            // A sketch curve (a revolve's axis) by its id in the file.
            if let (Some(Value::String(s)), Some(Value::String(c))) =
                (m.get("sketch"), m.get("curve"))
                && let Some((_, ctx)) = entry.sketches.get(s)
            {
                let file = ctx["ids"][c.as_str()]
                    .as_str()
                    .map_or_else(|| format!("m{c}"), str::to_owned);
                let mut out = m.clone();
                out.insert("sketch".to_owned(), json!(timeline_ids(s, to_index)));
                out.insert("curve".to_owned(), json!(file));
                return Value::Object(out);
            }
            Value::Object(
                m.iter()
                    .map(|(k, x)| (k.clone(), in_file_terms(x, entry, to_index)))
                    .collect(),
            )
        }
        Value::Array(a) => Value::Array(
            a.iter()
                .map(|x| in_file_terms(x, entry, to_index))
                .collect(),
        ),
        Value::String(s) => Value::String(timeline_ids(s, to_index)),
        x => x.clone(),
    }
}

fn candidate_json(c: &Candidate, entry: &Entry, to_index: &HashMap<String, i64>) -> Value {
    json!({
        "defs": c.defs.iter().map(|d| in_file_terms(d, entry, to_index)).collect::<Vec<_>>(),
        "note": c.note,
        "guess": c.guess,
        "predicted": c.predicted,
    })
}

/// One line of the dump.
#[allow(clippy::too_many_arguments)]
fn line(
    label: &str,
    index: i64,
    item: &TimelineItem,
    entry: &Entry,
    report: Option<&crate::ItemReport>,
    to_index: &HashMap<String, i64>,
    raw: Option<&Raw>,
    items: &[TimelineItem],
) -> Value {
    let candidates: Vec<Value> = entry
        .candidates
        .iter()
        .map(|c| candidate_json(c, entry, to_index))
        .collect();
    let accepted_rank = entry.accepted.as_ref().and_then(|(a, _)| {
        let k = key(a);
        entry.order.iter().position(|x| *x == k)
    });
    let f3d = item.f3d.as_ref();
    let mut known = serde_json::to_value(item).unwrap_or(Value::Null);
    if let Some(m) = known.as_object_mut() {
        // (The record holds the raw fields; the sketch's own detail is in
        // the context.)
        m.remove("_f3d");
        if matches!(item.detail, Some(Detail::Sketch(_))) {
            m.remove("detail");
        }
    }
    let mut sketches = Map::new();
    for (index, ctx) in entry.sketches.values() {
        let mut ctx = ctx.clone();
        if let Some(m) = ctx.as_object_mut() {
            m.remove("ids");
        }
        if let Some(r) = raw {
            ctx["curves"] = Value::Object(r.sketch_curves(*index, items));
        }
        sketches.insert(format!("T{index}"), ctx);
    }
    let edges: Map<String, Value> = entry
        .edges
        .iter()
        .map(|(name, e)| {
            let mut e = e.clone();
            if let Some(b) = e["body"].as_str() {
                e["body"] = json!(timeline_ids(b, to_index));
            }
            (timeline_ids(name, to_index), e)
        })
        .collect();
    let bodies: Vec<Value> = entry
        .bodies
        .iter()
        .map(|b| {
            let mut b = b.clone();
            if let Some(u) = b["uid"].as_str() {
                b["uid"] = json!(timeline_ids(u, to_index));
            }
            b
        })
        .collect();
    let mut out = json!({
        "schema": SCHEMA,
        "file": label,
        "item": index,
        "name": item.name(),
        "type": item.object_type(),
        "class": f3d.and_then(|f| f.class.clone()),
        "class_version": f3d.and_then(|f| f.class_version),
        "object_id": f3d.and_then(|f| f.object_id),
        "f3d": f3d,
        "status": if entry.accepted.as_ref().is_some_and(|(_, v)| *v == Some(true)) {
            "settled"
        } else {
            "unsettled"
        },
        "outcome": report.map(|r| r.outcome.as_str()),
        "note": report.and_then(|r| r.note.clone()),
        "known": known,
        "record": raw.and_then(|r| f3d.and_then(|f| f.object_id).map(|id| r.record(id))),
        "context": {"sketches": sketches, "bodies": bodies, "edges": edges,
                    "resolved": in_file_terms(&Value::Object(entry.resolved.clone()), entry, to_index),
                    "edge_matches": in_file_terms(&Value::Object(entry.edge_matches.clone()), entry, to_index)},
        "candidates": candidates,
        "accepted": accepted_rank,
        "answer": entry.accepted.as_ref().map(|(c, _)| candidate_json(c, entry, to_index)),
        "cost": {"candidates": entry.order.len(), "rank": accepted_rank,
                 "seconds": entry.seconds},
    });
    if let Some(error) = &entry.translation_error {
        out["translation_error"] = json!(error);
    }
    out
}

/// The design's streams, for the items' raw records.
struct Raw {
    segment: mitcad_f3d::design::stream::Segment,
    items: HashSet<u64>,
    local_ids: HashMap<u64, (u64, String)>,
}

impl Raw {
    /// The design `label` of the file (the first one when no label fits).
    fn open(path: Option<&Path>, label: &str) -> Option<Raw> {
        use mitcad_f3d::design;
        let docs = design::documents(path?).ok()?;
        let (_, doc) = docs
            .iter()
            .find(|(l, _)| l == label)
            .or_else(|| docs.first())?;
        let streams = design::find_design_streams(doc).ok()??;
        let segment = design::stream::Segment::parse(&streams.meta, streams.bulk).ok()?;
        let decoded = design::decode::decode(&segment);
        let items = decoded.timeline.iter().map(|t| t.id).collect();
        let (_, local_ids) = design::sketch::sketch_details(&segment);
        Some(Raw {
            segment,
            items,
            local_ids,
        })
    }

    fn record(&self, id: u64) -> Value {
        let others: HashSet<u64> = self.items.iter().copied().filter(|&i| i != id).collect();
        mitcad_f3d::design::learn::item_record(&self.segment, id, &others)
    }

    /// The curves and points of the sketch of timeline item `index`, by
    /// the file's ids.
    fn sketch_curves(&self, index: i64, items: &[TimelineItem]) -> Map<String, Value> {
        let Some(feature) = items
            .iter()
            .enumerate()
            .find(|(p, it)| it.index.unwrap_or(*p as i64) == index)
            .and_then(|(_, it)| it.f3d.as_ref()?.object_id)
        else {
            return Map::new();
        };
        let Some(sketch) = mitcad_f3d::design::learn::sketch_of_feature(&self.segment, feature)
        else {
            return Map::new();
        };
        let mut out =
            mitcad_f3d::design::learn::sketch_entities(&self.segment, sketch, &self.local_ids);
        // The geometry and kind the dump gives.
        let detail = items.iter().find_map(|it| match &it.detail {
            Some(Detail::Sketch(s)) if it.f3d.as_ref()?.object_id == Some(feature) => Some(s),
            _ => None,
        });
        for c in detail.and_then(|d| d.curves.as_ref()).into_iter().flatten() {
            let Some(id) = &c.id else { continue };
            let Some(Value::Object(e)) = out.get_mut(id) else {
                continue;
            };
            e.insert("type".to_owned(), json!(c.curve_type));
            if c.is_construction == Some(true) {
                e.insert("construction".to_owned(), json!(true));
            }
            if let Some(g) = &c.geometry {
                e.insert("geometry".to_owned(), json!(g));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_clock_leaves_the_dumps_work_out() {
        let paused = Arc::new(AtomicU64::new(5));
        let clock = Clock {
            started: Instant::now() - Duration::from_secs(10),
            paused: Some((Arc::clone(&paused), 5)),
        };
        assert!(clock.elapsed() >= Duration::from_secs(10));
        paused.fetch_add(4_000_000_000, Ordering::Relaxed);
        let left = clock.elapsed();
        assert!(left >= Duration::from_secs(6) && left < Duration::from_secs(7));
        paused.fetch_add(60_000_000_000, Ordering::Relaxed);
        assert_eq!(clock.elapsed(), Duration::ZERO);
    }

    use super::*;

    #[test]
    fn dressup_diagnostics_keep_split_matches_in_file_terms() {
        let edge = |name: &str, x: f64| refs::FoundEdge {
            body: "F3.b0".parse().unwrap(),
            name: name.parse().unwrap(),
            mid: [x, 0.0, 0.0],
            direction: Some([1.0, 0.0, 0.0]),
        };
        let value = edge_match_json(
            Ok(vec![edge("E{F3:a|F3:b}", 2.0), edge("E{F3:a|F3:c}", 7.0)]),
            Some("BREP.x.smbh before state 4"),
        );
        let value = in_file_terms(
            &value,
            &Entry::default(),
            &HashMap::from([("F3".to_owned(), 9)]),
        );
        assert_eq!(value["status"], "matched");
        assert_eq!(value["count"], 2);
        assert_eq!(value["matches"][0]["body"], "T9.b0");
        assert_eq!(value["matches"][1]["name"], "E{T9:a|T9:c}");
        assert_eq!(value["matches"][1]["mid"], json!([7.0, 0.0, 0.0]));
        assert_eq!(value["decoder_found"], "BREP.x.smbh before state 4");
    }

    #[test]
    fn dressup_diagnostics_separate_failures_and_bound_ambiguity() {
        for error in [
            refs::DressupEdgeFailure::DecoderUnresolved,
            refs::DressupEdgeFailure::MissingInReplay,
        ] {
            let value = edge_match_json(Err(error.clone()), Some("decoder reason"));
            assert_eq!(value["status"], error.code());
            assert_eq!(value["decoder_found"], "decoder reason");
            assert_eq!(value["candidates"], json!([]));
        }
        let candidates = (0..MAX_EDGE_MATCHES + 3)
            .map(|i| {
                (
                    format!("F3.b{i}").parse().unwrap(),
                    "E{F3:a|F3:b}".parse().unwrap(),
                )
            })
            .collect();
        let value = edge_match_json(
            Err(refs::DressupEdgeFailure::AmbiguousInReplay(candidates)),
            None,
        );
        assert_eq!(value["status"], "ambiguous_in_replay");
        assert_eq!(value["count"], MAX_EDGE_MATCHES + 3);
        assert_eq!(
            value["candidates"].as_array().unwrap().len(),
            MAX_EDGE_MATCHES
        );
    }

    #[test]
    fn ids_in_the_files_terms() {
        let back = HashMap::from([("c50", "c5"), ("c51", "c6"), ("t9", "t1")]);
        assert_eq!(file_ids("r{c50[c51,c52]}", &back), "r{c5[c6,mc52]}");
        assert_eq!(file_ids("t9.g0.c50", &back), "t1.g0.c50");
        let to_index = HashMap::from([("F3".to_owned(), 7)]);
        assert_eq!(
            timeline_ids("E{F3:side(c1)|F4:end}", &to_index),
            "E{T7:side(c1)|F4:end}"
        );
        assert_eq!(timeline_ids("F3.b0", &to_index), "T7.b0");
        assert_eq!(timeline_ids("XF3", &to_index), "XF3");
    }

    #[test]
    fn regions_by_their_index() {
        let mut entry = Entry::default();
        entry.sketches.insert(
            "F2".to_owned(),
            (
                4,
                json!({"regions": [{"ours": "r{c1}"}, {"ours": "r{c2}"}]}),
            ),
        );
        let to_index = HashMap::from([("F2".to_owned(), 4), ("F9".to_owned(), 6)]);
        let def = json!({"type": "extrude", "profiles": [{"sketch": "F2", "region": "r{c2}"},
                         {"sketch": "F2", "region": "r{c7}"}], "participants": ["F9.b1"]});
        let t = in_file_terms(&def, &entry, &to_index);
        assert_eq!(t["profiles"][0], json!({"sketch": "T4", "region": 1}));
        assert_eq!(
            t["profiles"][1],
            json!({"sketch": "T4", "region_key": "r{c7}"})
        );
        assert_eq!(t["participants"][0], "T6.b1");
    }

    #[test]
    fn names_of_the_lines_files() {
        let a = file_name("a b.f3d", Some(Path::new("/x/a b.f3d")));
        let b = file_name("a b.f3d", Some(Path::new("/y/a b.f3d")));
        assert!(a.starts_with("a_b.f3d-") && a.ends_with(".jsonl"));
        assert_ne!(a, b);
    }
}
