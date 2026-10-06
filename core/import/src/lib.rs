// SPDX-License-Identifier: MIT
//! The .f3d design import (T1): a design dump IR ([`mitcad_f3d::design::ir`],
//! from an `.f3d` file's streams or an external dump) replayed as
//! Mitcad document commands.
//!
//! - Parameters first, with the file's names and expressions ([`params`]).
//! - Then the timeline in order: sketches ([`sketch`]), construction
//!   geometry and features with their references resolved: sketch curves
//!   and points by the file's ids, profiles by area and centroid, bodies,
//!   faces and edges by their fingerprints against the replayed bodies.
//! - The bodies stored in the file ([`history`]) check the replay: after each
//!   feature the bodies must equal a state of the ASM history (the item's
//!   own state when the history names it). That also chooses what the
//!   stream decoder cannot tell (which profiles, which edges, which
//!   direction).
//! - A feature that cannot be translated or replayed is replaced by a base
//!   feature holding the file's bodies after it (the fallback), and the
//!   timeline continues on those bodies.
//! - The report ([`report`]) tells per item how it came in and how the
//!   final bodies agree with the stored design.
//!
//! The importer is generic over the [`Kernel`]; `mitcad-ffi` runs it with
//! OCCT and the bodies of the `.f3d` file.
//!
//! Components (F6, [`components`]): the file's components and occurrences
//! become Mitcad's before the replay, and every item goes into its
//! component, whose bodies are in its own coordinates as the ASM blobs
//! store them; the occurrences place them in the design.
//!
//! FreeCAD documents ([`freecad`]): the bodies FreeCAD stored, in the
//! document's parts and links; their history is a later stage.

pub mod components;
pub mod geom;
pub mod history;
pub mod params;
pub mod report;
pub mod sketch;
// FreeCAD documents (.FCStd): bodies and structure.
pub mod freecad;

mod features;
mod ops;
mod refs;
// Sweeps, pipes and lofts (F3).
mod sweeps;

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use mitcad_f3d::design::ir::{Dump, TimelineItem};
use mitcad_model::assembly::inverse;
use mitcad_model::features::{FeatureDef, ValueInput};
use mitcad_model::{
    BaseInput, BodyUid, ComponentUid, Document, FeatureUid, ImportBody, Kernel, RecomputeMonitor,
    SketchFrame, Transform,
};
use serde_json::Value;

pub use history::{NoGeometry, Oracle, Sig, StoredBody, StoredGeometry};
pub use report::{
    BodyReport, ComponentReport, DesignReport, ImportReport, ItemReport, LightBulbReport, Outcome,
    StopReport,
};

use params::ParamMap;

/// Import settings.
#[derive(Debug, Clone)]
pub struct Options {
    /// Check the replay against the ASM history and let it choose what the
    /// dump leaves open.
    pub verify: bool,
    /// Replace items that cannot be replayed with the file's bodies.
    pub fallback: bool,
    /// The most definitions tried for one item, after one set of bodies.
    pub max_candidates: usize,
    /// The most definitions evaluated for one item in all.
    pub max_attempts: usize,
    /// Seconds after which no more definitions are tried for an item.
    pub item_seconds: f64,
    /// How many history states a fallback may skip at once.
    pub fallback_states: usize,
    /// Compare the final bodies geometrically (surface deviation), not only
    /// by volume.
    pub compare: bool,
    /// Merge the import into one undo step with this label.
    pub undo_label: Option<String>,
    /// Seconds after which the remaining items take the file's bodies without
    /// trying definitions (the history's states where they are known).
    pub time_limit: Option<f64>,
    /// Timeline items (indices) the geometry kernel hung on in an earlier
    /// try: they take the file's bodies without trying definitions.
    pub hung_items: Vec<i64>,
    /// Where the import is, for a watchdog on another thread.
    pub progress: Option<Arc<Progress>>,
    /// A request to stop early (T1e, the user's Stop; `mitcad-ffi` gives
    /// the document's job): once it is cancelled, the modelling items from
    /// the one being replayed on take the file's bodies without trying (more)
    /// definitions, as after the time limit, and the final bodies are
    /// compared by volume only; the report says where it stopped
    /// ([`DesignReport::stopped`]). While definitions are tried it is the
    /// document's monitor, so that the kernel cuts a long operation short
    /// (P7e); its test delay slows them down.
    pub stop: Option<Arc<RecomputeMonitor>>,
    /// The item an earlier try stopped at (run again after the kernel hung
    /// in it, `mitcad-ffi`): it and the modelling items after it take
    /// the file's bodies as after a stop.
    pub stop_at: Option<i64>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            verify: true,
            fallback: true,
            max_candidates: 20,
            max_attempts: 40,
            item_seconds: 60.0,
            fallback_states: 6,
            compare: true,
            undo_label: None,
            time_limit: None,
            hung_items: Vec::new(),
            progress: None,
            stop: None,
            stop_at: None,
        }
    }
}

/// How far an import has come, shared with a watchdog: a kernel call
/// cannot be interrupted, so a watchdog that sees no step for a long time
/// gives the import up and runs it again with the item it hung on in
/// [`Options::hung_items`] (`mitcad-ffi`, `hang_limit`).
#[derive(Debug)]
pub struct Progress {
    /// Grows whenever the import moves on (an item starts, a definition or
    /// a fallback is evaluated).
    pub step: AtomicU64,
    /// The timeline index of the item being replayed, [`Progress::STARTING`]
    /// before the first and [`Progress::FINISHING`] while the final bodies
    /// are compared.
    pub item: AtomicI64,
    /// The timeline index of the item the import stopped at
    /// ([`Options::stop`]), [`Progress::NOT_STOPPED`] before; a try run
    /// again stops there ([`Options::stop_at`]).
    pub stopped: AtomicI64,
}

impl Progress {
    pub const STARTING: i64 = -1;
    pub const FINISHING: i64 = -2;
    pub const NOT_STOPPED: i64 = i64::MIN;

    pub fn new() -> Self {
        Self {
            step: AtomicU64::new(0),
            item: AtomicI64::new(Self::STARTING),
            stopped: AtomicI64::new(Self::NOT_STOPPED),
        }
    }

    /// The item the import stopped at, once it did.
    pub fn stopped_at(&self) -> Option<i64> {
        let at = self.stopped.load(Ordering::Relaxed);
        (at != Self::NOT_STOPPED).then_some(at)
    }

    fn tick(&self) {
        self.step.fetch_add(1, Ordering::Relaxed);
    }

    fn at(&self, item: i64) {
        self.item.store(item, Ordering::Relaxed);
        self.tick();
    }
}

impl Default for Progress {
    fn default() -> Self {
        Self::new()
    }
}

/// A sketch that was imported.
#[derive(Debug, Clone)]
pub(crate) struct SketchInfo {
    pub uid: FeatureUid,
    /// Entity id in the file → Mitcad id.
    pub ids: HashMap<String, String>,
    /// The sketch's frame in model space (mm).
    #[allow(dead_code)]
    pub frame: SketchFrame,
}

/// Definitions to try for an item: usually one feature, several when
/// Mitcad needs one per body (a fillet of edges on two bodies).
#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub defs: Vec<Value>,
    /// What it assumes, for the report.
    pub note: Option<String>,
    /// A guess only the history can confirm: not used without one.
    pub guess: bool,
    /// The volume it should add (negative: remove), mm³, when it can be
    /// estimated without the kernel; guesses closest to the history's next
    /// states are tried first.
    pub predicted: Option<f64>,
}

impl Candidate {
    pub fn new(def: Value) -> Self {
        Self {
            defs: vec![def],
            note: None,
            guess: false,
            predicted: None,
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// How closely its result must match the history (relative): lofts
    /// with end conditions or rails follow Mitcad's own rules for them
    /// (commands.md, `loft`), which do not give every stored surface
    /// exactly (round sections stored as polynomial circles, rails with end
    /// conditions), so they get the looser [`history::CONVENTIONS`]; so do
    /// replace faces (P5: continued B-splines, which side of a target).
    /// So do the fillet and chamfer options P6 builds itself: curvature
    /// continuous, asymmetric and mid radius fillets, setback corners, and
    /// miter and blend chamfer corners.
    pub fn tolerance(&self) -> f64 {
        let conventions = |def: &Value| {
            let imposes = |key: &str| {
                def.get(key)
                    .and_then(|c| c["type"].as_str())
                    .is_some_and(|t| t != "free" && t != "point_sharp")
            };
            let loft = def["type"] == "loft"
                && (def["rails"].as_array().is_some_and(|r| !r.is_empty())
                    || imposes("start_condition")
                    || imposes("end_condition"));
            let own_set = |set: &Value| {
                set["continuity"] == "curvature"
                    || set["size"]["type"] == "asymmetric"
                    || set["size"]["mid"].as_array().is_some_and(|m| !m.is_empty())
            };
            let fillet = def["type"] == "fillet"
                && (def["rolling_ball_corners"] == false
                    || def["sets"]
                        .as_array()
                        .is_some_and(|s| s.iter().any(own_set)));
            let chamfer = def["type"] == "chamfer"
                && def["corner"]
                    .as_str()
                    .is_some_and(|c| c == "miter" || c == "blend");
            loft || fillet || chamfer || def["type"] == "replace_face"
        };
        if self.defs.iter().any(conventions) {
            history::CONVENTIONS
        } else {
            history::RELATIVE
        }
    }
}

/// An item that failed and waits for the history to tell what it did.
#[derive(Debug, Clone)]
struct Pending {
    /// Its entry in the report.
    report: usize,
    reason: String,
}

/// Where the replay's bodies come from before a candidate is tried.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Before {
    /// As they are (the pending items changed nothing).
    Keep,
    /// Replaced by a history state (the pending items' fallback).
    State(usize),
}

pub(crate) struct Importer<'a, K: Kernel> {
    doc: &'a mut Document<K>,
    dump: &'a Dump,
    options: &'a Options,
    oracle: Oracle<'a, K::Shape>,
    params: ParamMap,
    sketches: HashMap<i64, SketchInfo>,
    /// Timeline index → the feature made for it (construction geometry,
    /// features that later items refer to).
    features: HashMap<i64, FeatureUid>,
    report: DesignReport,
    pending: Vec<Pending>,
    /// Without a history: the bodies were replaced by the stored design.
    at_final: bool,
    /// When the import started (for `Options::time_limit`).
    started: std::time::Instant,
    /// The file's components as Mitcad's, and the items in them.
    components: components::Components,
    /// The component of the item being replayed.
    component: ComponentUid,
    /// Sketches imported again into another component than the first
    /// (the file's new-component features use their parent's sketches), by
    /// sketch index and component.
    sketch_copies: HashMap<(i64, ComponentUid), SketchInfo>,
    /// The modelling item the import stopped at ([`Options::stop`]).
    stopped_at: Cell<Option<i64>>,
    /// A modelling item's definitions are being tried: the stop request
    /// is the document's monitor ([`Importer::try_candidate`]).
    trying: bool,
}

/// Imports a design into the document (normally a new one). The report
/// tells how each timeline item came in.
pub fn import_design<K: Kernel>(
    doc: &mut Document<K>,
    dump: &Dump,
    geometry: &mut dyn StoredGeometry<K::Shape>,
    options: &Options,
) -> DesignReport {
    let depth = doc.undo_depth();
    let report = DesignReport {
        label: dump
            .source
            .as_ref()
            .and_then(|s| s.file.clone())
            .or_else(|| dump.document.as_ref().and_then(|d| d.name.clone()))
            .unwrap_or_default(),
        source: dump
            .source
            .as_ref()
            .and_then(|s| s.mode.clone())
            .unwrap_or_default(),
        ..DesignReport::default()
    };
    let mut importer = Importer {
        oracle: Oracle::new(geometry),
        doc,
        dump,
        options,
        params: ParamMap::default(),
        sketches: HashMap::new(),
        features: HashMap::new(),
        report,
        pending: Vec::new(),
        at_final: false,
        started: std::time::Instant::now(),
        components: components::Components::default(),
        component: ComponentUid::ROOT,
        sketch_copies: HashMap::new(),
        stopped_at: Cell::new(None),
        trying: false,
    };
    if !options.verify {
        importer.oracle.enabled = false;
    }
    importer.run();
    importer.light_bulbs();
    let Importer { doc, report, .. } = importer;
    if let Some(label) = &options.undo_label {
        doc.merge_undo(depth, label);
    }
    report
}

/// The note of an item the geometry kernel hung on.
const HUNG: &str =
    "the geometry kernel did not finish a definition of it (given up by the import's watchdog)";

/// The note of a modelling item not replayed because the import was
/// stopped ([`Options::stop`]).
const STOPPED: &str = "the import was stopped";

/// Whether `MITCAD_IMPORT_TRACE` asks for the replay's attempts on
/// standard error.
fn tracing() -> bool {
    static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRACE.get_or_init(|| std::env::var_os("MITCAD_IMPORT_TRACE").is_some())
}

/// Seconds since the first trace, for traces.
fn trace_clock() -> f64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
}

macro_rules! trace {
    ($($arg:tt)*) => {
        if tracing() {
            eprintln!("import: [{:.2}] {}", trace_clock(), format!($($arg)*));
        }
    };
}

fn item_name(item: &TimelineItem) -> String {
    item.name().unwrap_or("?").to_owned()
}

/// Replaces the strings `"$k"` in a definition by the uid of the k-th
/// feature added before it ([`Importer::add_all`]).
fn substitute(def: &mut Value, uids: &[FeatureUid]) {
    match def {
        Value::String(s) => {
            if let Some(uid) = s
                .strip_prefix('$')
                .and_then(|k| k.parse::<usize>().ok())
                .and_then(|k| uids.get(k))
            {
                *s = uid.to_string();
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| substitute(v, uids)),
        Value::Object(map) => map.values_mut().for_each(|v| substitute(v, uids)),
        _ => {}
    }
}

/// Whether a candidate's predicted volume change is the most it can
/// change the volume by: an extrusion by distances without taper, thin
/// walls or a start object adds or removes at most its prism (overlaps
/// only take from it).
fn bounded(c: &Candidate) -> bool {
    fn by_distance(e: &Value) -> bool {
        match e["type"].as_str() {
            Some("distance" | "symmetric") => e.get("taper").is_none(),
            Some("two_sides") => by_distance(&e["side1"]) && by_distance(&e["side2"]),
            _ => false,
        }
    }
    c.predicted.is_some()
        && c.defs.last().is_some_and(|d| {
            d["type"] == "extrude"
                && d.get("thin").is_none()
                && d.get("start").is_none()
                && by_distance(&d["extent"])
        })
}

/// Whether a feature that adds (or removes) at most `bound` mm³ can make a
/// change of `change` mm³. A change the other way is left to the history
/// check: a state may also change another component's bodies. The
/// regions' areas come from sampled outlines: 5 % of slack.
fn can_change(bound: f64, change: f64) -> bool {
    bound * change <= 0.0 || bound.abs() >= 0.95 * change.abs()
}

/// A fillet or chamfer candidate of one definition: the definition
/// without its edges, and its edges (by body).
fn dressup_edges(c: &Candidate) -> Option<(Value, BTreeSet<String>)> {
    let [def] = c.defs.as_slice() else {
        return None;
    };
    if def["type"] != "fillet" && def["type"] != "chamfer" {
        return None;
    }
    let body = def["body"].as_str()?;
    let mut shape = def.clone();
    let mut edges = BTreeSet::new();
    for set in shape["sets"].as_array_mut()? {
        for e in set["edges"].as_array()?.iter().filter_map(Value::as_str) {
            edges.insert(format!("{body}|{e}"));
        }
        set.as_object_mut()?.remove("edges");
    }
    Some((shape, edges))
}

/// A candidate of one definition with participants: the definition
/// without them, and them.
fn participants_of(c: &Candidate) -> Option<(Value, BTreeSet<String>)> {
    let [def] = c.defs.as_slice() else {
        return None;
    };
    let limited: BTreeSet<String> = def
        .get("participants")?
        .as_array()?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let mut free = def.clone();
    free.as_object_mut()?.remove("participants");
    Some((free, limited))
}

/// Every string in a JSON value.
fn strings<'v>(value: &'v Value, out: &mut Vec<&'v str>) {
    match value {
        Value::String(s) => out.push(s),
        Value::Array(items) => items.iter().for_each(|v| strings(v, out)),
        Value::Object(map) => map.values().for_each(|v| strings(v, out)),
        _ => {}
    }
}

/// A candidate in one line, for traces.
fn summary(c: &Candidate) -> String {
    let defs: Vec<String> = c
        .defs
        .iter()
        .map(|d| {
            let mut d = d.clone();
            if let Some(m) = d.as_object_mut() {
                m.remove("entities");
            }
            let text = d.to_string();
            if text.chars().count() > 300 {
                format!("{}…", text.chars().take(300).collect::<String>())
            } else {
                text
            }
        })
        .collect();
    defs.join(" + ")
}

/// Volumes of signatures (and areas and faces), for traces.
fn short(sigs: &[Sig]) -> Vec<String> {
    sigs.iter()
        .map(|s| {
            format!(
                "{:.3}/{:.3}/{}@{:.2},{:.2},{:.2}",
                s.volume, s.area, s.faces, s.center[0], s.center[1], s.center[2]
            )
        })
        .collect()
}

/// What a timeline item is, for the replay.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Class {
    Sketch,
    /// Construction plane, axis or point: no bodies.
    Datum,
    /// Changes no body in its component (joints, occurrences, ...).
    NoGeometry,
    /// A modelling feature (or an unknown item, which may be one).
    Geometry,
}

fn classify(object_type: Option<&str>) -> Class {
    match object_type {
        Some("Sketch") => Class::Sketch,
        Some("ConstructionPlane" | "ConstructionAxis" | "ConstructionPoint") => Class::Datum,
        Some(
            "Joint" | "AsBuiltJoint" | "JointOrigin" | "RigidGroup" | "Canvas" | "Occurrence"
            | "Decal" | "Snapshot" | "MotionLink" | "ContactSet",
        ) => Class::NoGeometry,
        _ => Class::Geometry,
    }
}

impl<'a, K: Kernel> Importer<'a, K> {
    fn run(&mut self) {
        self.set_units();
        self.params = params::import_parameters(self.doc, self.dump, &mut self.report.parameters);
        let names = self.oracle.component_names();
        self.components = components::import_components(
            self.doc,
            self.dump,
            &names,
            &mut self.report.components,
            &mut self.report.warnings,
        );
        components::assign_items(
            self.dump,
            &mut self.oracle,
            &mut self.components,
            &mut self.report.components,
        );
        self.report.history.states = self.oracle.count();
        if self.oracle.enabled {
            // The replay starts with no bodies: the first state without
            // solids, if the history starts that way.
            self.oracle.cursor = self.oracle.find(self.doc.kernel(), 0, &[]);
        }
        let dump = self.dump;
        for (position, item) in dump.timeline_items().iter().enumerate() {
            self.item(position, item);
        }
        self.finish();
    }

    fn set_units(&mut self) {
        let unit = self
            .dump
            .document
            .as_ref()
            .and_then(|d| d.default_length_units.as_deref())
            .and_then(|u| mitcad_model::expr::Unit::parse(u).ok())
            .and_then(|u| u.length());
        if let Some((length, 1)) = unit
            && let Err(e) = self.doc.set_units(length)
        {
            self.report.warnings.push(format!("document units: {e}"));
        }
    }

    /// The light bulbs of the imported sketches and construction features
    /// (mitcad#6), after the replay: one is kept only where the file's
    /// differs from Mitcad's default (a sketch hidden while a feature uses
    /// it, construction geometry shown), so a sketch that follows the
    /// default still shows when its last consumer is deleted. Where the
    /// file does not tell, the default decides. Part of the import's undo
    /// step.
    fn light_bulbs(&mut self) {
        let dump = self.dump;
        for (position, item) in dump.timeline_items().iter().enumerate() {
            let index = item.index.unwrap_or(position as i64);
            let (uid, sketch) = match item.object_type() {
                Some("Sketch") => (self.sketches.get(&index).map(|s| s.uid), true),
                Some("ConstructionPlane" | "ConstructionAxis" | "ConstructionPoint") => {
                    (self.features.get(&index).copied(), false)
                }
                _ => continue,
            };
            // Imported as what it is (not as a fallback's base feature).
            let Some(uid) = uid.filter(|uid| self.doc.feature_shown(*uid).is_some()) else {
                continue;
            };
            let bulb = item.light_bulb();
            let report = &mut self.report.light_bulbs;
            if sketch {
                report.sketch_bulbs += 1;
                report.sketch_bulbs_unknown += usize::from(bulb.is_none());
            } else {
                report.construction_bulbs += 1;
                report.construction_bulbs_unknown += usize::from(bulb.is_none());
            }
            let Some(on) = bulb else {
                continue;
            };
            if self.doc.feature_shown(uid) == Some(on) {
                continue;
            }
            match self.doc.set_feature_visible(uid, on) {
                Ok(()) => self.report.light_bulbs.bulbs_set += 1,
                Err(e) => self
                    .report
                    .warnings
                    .push(format!("light bulb of {}: {e}", item.name().unwrap_or("?"))),
            }
        }
    }

    fn item(&mut self, position: usize, item: &TimelineItem) {
        let index = item.index.unwrap_or(position as i64);
        let object_type = item.object_type().map(str::to_owned);
        let name = item
            .name()
            .filter(|n| !n.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Item{index}"));
        let report = ItemReport {
            index,
            name: name.clone(),
            object_type: object_type.clone().unwrap_or_else(|| {
                item.f3d
                    .as_ref()
                    .and_then(|f| f.base_name.clone())
                    .map_or("?".to_owned(), |b| format!("?{b}"))
            }),
            outcome: Outcome::Skipped,
            features: Vec::new(),
            verified: None,
            note: None,
            component: None,
        };
        self.report.items.push(report);
        let at = self.report.items.len() - 1;
        // (The application's progress dialog shows it.)
        trace!("item {}: {name}", position + 1);
        self.component = self.components.of_item(index);
        if !self.component.is_root() {
            self.report.items[at].component = Some(self.doc.assembly().name(self.component));
        }
        if let Some(p) = &self.options.progress {
            p.at(index);
        }
        if item.is_suppressed == Some(true) {
            self.note(at, Outcome::Skipped, "suppressed in the file");
            return;
        }
        let started = std::time::Instant::now();
        let class = classify(object_type.as_deref());
        if class != Class::Geometry && self.options.hung_items.contains(&index) {
            self.note(at, Outcome::Skipped, HUNG);
            return;
        }
        match class {
            Class::Sketch => self.sketch_item(at, index, item),
            Class::Datum => self.datum_item(at, index, item),
            Class::NoGeometry => self.note(
                at,
                Outcome::Skipped,
                "no geometry (assembly relationship or display item)",
            ),
            Class::Geometry => {
                let originals = self.sketches_into_component(at, index, item);
                self.geometry_item(at, index, item);
                self.sketches.extend(originals);
            }
        }
        trace!(
            "{name}: {} in {:.2} s",
            self.report.items[at].outcome.as_str(),
            started.elapsed().as_secs_f64()
        );
    }

    fn note(&mut self, at: usize, outcome: Outcome, note: impl Into<String>) {
        let item = &mut self.report.items[at];
        item.outcome = outcome;
        let note = note.into();
        if !note.is_empty() {
            item.note = Some(note);
        }
    }

    /// Adds a feature from its JSON definition; Ok with its uid when it
    /// evaluated, Err with the reason (the feature is then removed again).
    pub(crate) fn add(&mut self, def: &Value, name: Option<&str>) -> Result<FeatureUid, String> {
        self.tick();
        // Items may have no name (the decoder's unknown ones).
        let name = name.filter(|n| !n.trim().is_empty());
        let component = Some(self.def_component(def));
        let def: FeatureDef<ValueInput> =
            serde_json::from_value(def.clone()).map_err(|e| format!("definition: {e}"))?;
        let added = match self.interruptible(|doc| doc.add_feature_to(&def, name, component)) {
            Ok(added) => added,
            // A name Mitcad already uses: take the default one.
            Err(e) if name.is_some() && e.to_string().contains("already named") => self
                .interruptible(|doc| doc.add_feature_to(&def, None, component))
                .map_err(|e| e.to_string())?,
            Err(e) => return Err(e.to_string()),
        };
        match self.doc.status(added.uid).and_then(|s| s.error()) {
            None => Ok(added.uid),
            Some(error) => {
                let error = error.to_owned();
                self.doc.undo();
                Err(error)
            }
        }
    }

    /// The component a definition goes into: that of the bodies, sketches
    /// and construction geometry it names when they agree, else the
    /// item's (a feature works on its component's bodies, and the history
    /// may tell another component than the one the item was given).
    fn def_component(&self, def: &Value) -> ComponentUid {
        let mut found = BTreeSet::new();
        let mut texts = Vec::new();
        strings(def, &mut texts);
        for text in texts {
            if let Ok(body) = text.parse::<BodyUid>() {
                found.extend(self.doc.body_component(body));
            } else if let Ok(uid) = text.parse::<FeatureUid>()
                && let Some(f) = self.doc.feature(uid)
                && matches!(
                    f.def,
                    FeatureDef::Sketch(_)
                        | FeatureDef::ConstructionPlane(_)
                        | FeatureDef::ConstructionAxis(_)
                        | FeatureDef::ConstructionPoint(_)
                )
            {
                found.insert(f.component);
            }
        }
        match (found.len(), found.first()) {
            (1, Some(c)) => *c,
            _ => self.component,
        }
    }

    /// Adds a candidate's features (all or none). A string `"$k"` in a
    /// definition stands for the feature made from definition k before it
    /// (a sketch made for the item); such sketches keep their default
    /// names.
    pub(crate) fn add_all(
        &mut self,
        candidate: &Candidate,
        name: Option<&str>,
    ) -> Result<Vec<FeatureUid>, String> {
        let depth = self.doc.undo_depth();
        let mut uids: Vec<FeatureUid> = Vec::new();
        let mut named = 0;
        for def in &candidate.defs {
            let mut def = def.clone();
            if !uids.is_empty() {
                substitute(&mut def, &uids);
            }
            let own = match name {
                _ if def["type"] == "sketch" && candidate.defs.len() > 1 => None,
                Some(n) if named == 0 => Some(n.to_owned()),
                Some(n) => Some(format!("{n}.{}", named + 1)),
                None => None,
            };
            if own.is_some() {
                named += 1;
            }
            match self.add(&def, own.as_deref()) {
                Ok(uid) => uids.push(uid),
                Err(e) => {
                    self.undo_to(depth);
                    return Err(e);
                }
            }
        }
        Ok(uids)
    }

    /// Runs `f` on the document with the stop request as its monitor while
    /// a modelling item's definitions are tried, so that a stop cuts the
    /// evaluation short (P7e): the command then fails and changes nothing.
    /// Undo never runs so: a cancelled monitor would refuse it.
    fn interruptible<T>(&mut self, f: impl FnOnce(&mut Document<K>) -> T) -> T {
        let monitor = match &self.options.stop {
            Some(m) if self.trying && !m.is_cancelled() => Arc::clone(m),
            _ => return f(self.doc),
        };
        let before = self.doc.monitor().cloned();
        self.doc.set_monitor(Some(monitor));
        let result = f(self.doc);
        self.doc.set_monitor(before);
        result
    }

    /// Adds a candidate of the modelling item `index` (see
    /// [`Importer::add_all`]): none once the import was stopped, and a stop
    /// while it is evaluated fails it ([`Options::stop`]).
    fn try_candidate(
        &mut self,
        index: i64,
        candidate: &Candidate,
        name: Option<&str>,
    ) -> Result<Vec<FeatureUid>, String> {
        if self.stopped(index) {
            return Err(STOPPED.to_owned());
        }
        self.trying = true;
        let added = self.add_all(candidate, name);
        self.trying = false;
        match added {
            Err(_) if self.stopped(index) => Err(STOPPED.to_owned()),
            added => added,
        }
    }

    /// Whether a stop was asked for (or an earlier try stopped).
    fn stop_requested(&self) -> bool {
        self.options.stop_at.is_some()
            || self.options.stop.as_ref().is_some_and(|m| m.is_cancelled())
    }

    /// Whether the modelling item `index` is not replayed because the
    /// import was stopped: once a stop is asked for, the first item asked
    /// about is where it stopped, and the items after it are not replayed
    /// either (in a try run again, the items from [`Options::stop_at`]).
    fn stopped(&self, index: i64) -> bool {
        let stopped = match self.options.stop_at {
            Some(at) => index >= at,
            None => self.stop_requested(),
        };
        if stopped && self.stopped_at.get().is_none() {
            self.stopped_at.set(Some(index));
            if let Some(p) = &self.options.progress {
                p.stopped.store(index, Ordering::Relaxed);
            }
        }
        stopped
    }

    /// The report's stop and its warning, when the import was stopped.
    fn report_stop(&mut self) {
        if !self.stop_requested() {
            return;
        }
        let item = self.stopped_at.get();
        let name = item
            .and_then(|i| self.report.items.iter().find(|r| r.index == i))
            .map(|r| r.name.clone());
        self.report.warnings.push(match &name {
            Some(name) => format!(
                "the import was stopped at {name}: it and the modelling items after it took \
                 the file's bodies without being replayed, and the final bodies were compared by \
                 volume only"
            ),
            None => "the import was stopped after its last item: the final bodies were compared \
                     by volume only"
                .to_owned(),
        });
        self.report.stopped = Some(StopReport { item, name });
    }

    /// Takes back commands down to an undo depth.
    fn undo_to(&mut self, depth: usize) {
        while self.doc.undo_depth() > depth {
            if self.doc.undo().is_none() {
                break;
            }
        }
    }

    /// The model parameters the item made become the feature's own.
    fn adopt(&mut self, index: i64, uid: FeatureUid) {
        let names = self.params.owned_by(index).to_vec();
        if !names.is_empty() {
            let _ = self.doc.adopt_parameters(uid, &names);
        }
    }

    /// Signatures of the solids at the marker.
    fn current_sigs(&self) -> Vec<Sig> {
        let kernel = self.doc.kernel();
        self.doc
            .bodies()
            .iter()
            .filter_map(|b| Sig::of(kernel, b.shape))
            .collect()
    }

    fn current_bodies(&self) -> Vec<(BodyUid, Sig)> {
        let kernel = self.doc.kernel();
        self.doc
            .bodies()
            .iter()
            .filter_map(|b| Some((b.uid, Sig::of(kernel, b.shape)?)))
            .collect()
    }

    /// The sketches a modelling item can use (those it names, else the last
    /// one before it) in the item's component: one imported into another
    /// component is imported again into this one (a new-component feature
    /// of the file's uses its parent's sketch; its bodies, and so the copy,
    /// are in the new component's coordinates, which are the parent's when
    /// it is made). Only for items whose component the dump or the history
    /// gives. Returns the sketches to put back after the item.
    fn sketches_into_component(
        &mut self,
        at: usize,
        index: i64,
        item: &TimelineItem,
    ) -> Vec<(i64, SketchInfo)> {
        if !self.components.is_known(index) {
            return Vec::new();
        }
        let mut wanted = Vec::new();
        if let Ok(value) = serde_json::to_value(item) {
            components::references(&value, &mut wanted);
        }
        wanted.retain(|i| *i < index && self.sketches.contains_key(i));
        if wanted.is_empty()
            && let Some(last) = self.sketches.keys().filter(|k| **k < index).max()
        {
            wanted.push(*last);
        }
        wanted.sort_unstable();
        wanted.dedup();
        let mut originals = Vec::new();
        for i in wanted {
            let info = self.sketches[&i].clone();
            if self.doc.feature(info.uid).map(|f| f.component) == Some(self.component) {
                continue;
            }
            let copy = match self.sketch_copies.get(&(i, self.component)) {
                Some(copy) => Some(copy.clone()),
                None => self.copy_sketch(i, &info),
            };
            if let Some(copy) = copy {
                self.report.items[at].features.push(copy.uid.to_string());
                self.sketches.insert(i, copy);
                originals.push((i, info));
            }
        }
        originals
    }

    /// Imports sketch `i` again into the current component, moved from the
    /// coordinates of its own component into this one's (the file's
    /// assembly context: a feature of one component may use another's
    /// sketch); None when it does not import, lands elsewhere, or a
    /// component is placed more than once (no one way to move it).
    fn copy_sketch(&mut self, i: i64, original: &SketchInfo) -> Option<SketchInfo> {
        let dump = self.dump;
        let item = dump
            .timeline_items()
            .iter()
            .enumerate()
            .find(|(p, it)| it.index.unwrap_or(*p as i64) == i)?
            .1;
        let from = self.doc.feature(original.uid)?.component;
        let moved = inverse(&self.placement(self.component)?).after(&self.placement(from)?);
        let depth = self.doc.undo_depth();
        let result = self.import_sketch(i, item, Some(&moved));
        let copy = self.sketches.insert(i, original.clone());
        match (result, copy) {
            (Ok(_), Some(copy))
                if self.doc.feature(copy.uid).map(|f| f.component) == Some(self.component) =>
            {
                trace!("sketch {i} imported again into {}", self.component);
                self.sketch_copies.insert((i, self.component), copy.clone());
                Some(copy)
            }
            _ => {
                self.undo_to(depth);
                None
            }
        }
    }

    /// Where a component is placed in the design, when it is placed once
    /// (the root: as it is).
    fn placement(&self, component: ComponentUid) -> Option<Transform> {
        let assembly = self.doc.assembly();
        let paths = assembly.paths_to(component);
        let [path] = paths.as_slice() else {
            return None;
        };
        let mut placed = Transform::IDENTITY;
        for o in path {
            placed = placed.after(&assembly.occurrence(*o)?.transform);
        }
        Some(placed)
    }

    fn sketch_item(&mut self, at: usize, index: i64, item: &TimelineItem) {
        match self.import_sketch(index, item, None) {
            Ok((uid, outcome, note)) => {
                self.report.items[at].features.push(uid.to_string());
                self.note(at, outcome, note);
            }
            Err(reason) => self.note(at, Outcome::Skipped, reason),
        }
    }

    fn datum_item(&mut self, at: usize, index: i64, item: &TimelineItem) {
        match self.translate_datum(item) {
            Ok(candidates) => {
                let mut last = String::new();
                for c in candidates {
                    match self.add_all(&c, item.name()) {
                        Ok(uids) => {
                            self.accepted(at, index, &uids, &c, None);
                            if c.note.as_deref().is_some_and(|n| n.starts_with("a fixed")) {
                                self.report.items[at].outcome = Outcome::Partial;
                            }
                            return;
                        }
                        Err(e) => last = e,
                    }
                }
                self.note(at, Outcome::Skipped, last);
            }
            Err(reason) => self.note(at, Outcome::Skipped, reason),
        }
    }

    /// A modelling feature: its candidates, checked against the history
    /// when there is one.
    fn geometry_item(&mut self, at: usize, index: i64, item: &TimelineItem) {
        if self.at_final {
            // The stored design is the base body: the item continues on it
            // when it leaves it as the file stored it.
            if self.stopped(index) {
                self.note(
                    at,
                    Outcome::Skipped,
                    format!("{STOPPED}; the stored bodies stand in for it"),
                );
            } else if !self.on_base(at, index, item, false) {
                self.note(
                    at,
                    Outcome::Skipped,
                    "after the fallback to the stored bodies (no history to continue from)",
                );
            }
            return;
        }
        if self.oracle.enabled {
            self.verified(at, index, item);
            return;
        }
        if self.options.hung_items.contains(&index) {
            self.failed(at, HUNG.to_owned());
            return;
        }
        if self.stopped(index) {
            self.failed(at, STOPPED.to_owned());
            return;
        }
        match self.translate(index, item) {
            Ok(c) if c.is_empty() => self.failed(at, "nothing to try".to_owned()),
            Ok(mut c) => {
                c.truncate(self.options.max_candidates);
                self.unverified(at, index, item, &c);
            }
            Err(reason) => self.failed(at, reason),
        }
    }

    fn unverified(&mut self, at: usize, index: i64, item: &TimelineItem, candidates: &[Candidate]) {
        let mut last = "only guesses, which need the file's history to check".to_owned();
        for c in candidates.iter().filter(|c| !c.guess) {
            match self.try_candidate(index, c, item.name()) {
                Ok(uids) => {
                    self.accepted(at, index, &uids, c, None);
                    return;
                }
                Err(e) => {
                    let stopped = e == STOPPED;
                    last = e;
                    if stopped {
                        break;
                    }
                }
            }
        }
        self.failed(at, last);
    }

    fn accepted(
        &mut self,
        at: usize,
        index: i64,
        uids: &[FeatureUid],
        c: &Candidate,
        verified: Option<bool>,
    ) {
        // The item's own feature, not a sketch made for it.
        let own = uids
            .iter()
            .copied()
            .find(|u| {
                !matches!(
                    self.doc.feature(*u).map(|f| &f.def),
                    Some(FeatureDef::Sketch(_))
                )
            })
            .or(uids.first().copied());
        if let Some(first) = own {
            self.features.insert(index, first);
            self.adopt(index, first);
        }
        let item = &mut self.report.items[at];
        item.features.extend(uids.iter().map(ToString::to_string));
        item.verified = verified;
        self.note(at, Outcome::Parametric, c.note.clone().unwrap_or_default());
    }

    /// A warning when a result was kept beyond the usual tolerance (see
    /// [`Candidate::tolerance`]).
    fn loosely_matched(&mut self, name: &str, distance: f64) {
        if distance > history::RELATIVE {
            self.report.warnings.push(format!(
                "{name} differs from the file's history by {:.2} % (Mitcad's own end conditions \
                 and rails of lofts)",
                distance * 100.0
            ));
        }
    }

    /// The item's candidates against the history, also after the pending
    /// items' possible fallbacks. The candidates are made for the bodies
    /// they start from (edges found against a fallback's bodies).
    fn verified(&mut self, at: usize, index: i64, item: &TimelineItem) {
        if let Some(state) = self.oracle.item_state(index) {
            self.verified_at(at, index, item, state);
            return;
        }
        let named = self.oracle.state_of_item(index);
        trace!(
            "{}: no state of its own (named {named:?}), the replay at {:?}, {} pending",
            item_name(item),
            self.oracle.cursor,
            self.pending.len()
        );
        // Its state is behind the replay: a fallback before it took the
        // bodies past it, so they hold its change already. It continues on
        // them when it leaves them so (references resolved against them).
        if named.zip(self.oracle.cursor).is_some_and(|(s, c)| s <= c)
            && self.pending.is_empty()
            && self.give_up(index).is_none()
            && self.on_base(at, index, item, true)
        {
            return;
        }
        if let Some(reason) = self.give_up(index) {
            self.failed(at, reason);
            return;
        }
        let mut befores = vec![Before::Keep];
        if !self.pending.is_empty() {
            let next = self.oracle.next_index();
            let last = self.oracle.count();
            befores.extend(
                (next..last)
                    .take(self.options.fallback_states)
                    .map(Before::State),
            );
        }
        let mut last_error = String::from("no definition gave the file's result");
        let mut attempts = 0;
        let started = std::time::Instant::now();
        let budget = self.options.item_seconds;
        let out_of_time = || started.elapsed().as_secs_f64() > budget;
        for before in befores {
            if self.stopped(index) {
                last_error = STOPPED.to_owned();
                break;
            }
            if attempts >= self.options.max_attempts || out_of_time() {
                break;
            }
            let depth = self.doc.undo_depth();
            let mut fallback = None;
            let base = match before {
                Before::Keep => self.oracle.cursor,
                Before::State(q) => match self.fallback_to(q, None) {
                    Ok(uid) => {
                        fallback = uid;
                        Some(q)
                    }
                    Err(e) => {
                        trace!("{}: {before:?} fallback failed: {e}", item_name(item));
                        last_error = e;
                        self.undo_to(depth);
                        continue;
                    }
                },
            };
            let from = base.map_or(0, |b| b + 1);
            let candidates = match self.translate(index, item) {
                Ok(c) if !c.is_empty() => self.rank(c, from),
                Ok(_) => {
                    trace!("{}: {before:?} nothing to try", item_name(item));
                    last_error = "nothing to try".to_owned();
                    self.undo_to(depth);
                    continue;
                }
                Err(e) => {
                    trace!("{}: {before:?} {e}", item_name(item));
                    last_error = e;
                    self.undo_to(depth);
                    // Only edges and faces found against the bodies can
                    // change with a fallback before the item.
                    if before == Before::Keep && !features::depends_on_bodies(item) {
                        break;
                    }
                    continue;
                }
            };
            // After a fallback only the likeliest few.
            let limit = match before {
                Before::Keep => candidates.len(),
                Before::State(_) => candidates.len().min(4),
            };
            for c in candidates.iter().take(limit) {
                if attempts >= self.options.max_attempts || out_of_time() {
                    last_error = format!("{last_error} (gave up after {attempts} attempts)");
                    break;
                }
                attempts += 1;
                let inner = self.doc.undo_depth();
                match self.try_candidate(index, c, item.name()) {
                    Ok(uids) => {
                        let current = self.current_sigs();
                        let kernel = self.doc.kernel();
                        // With items pending and nothing in their place, an
                        // approximate match could hide what they changed.
                        let exact = before == Before::Keep && !self.pending.is_empty();
                        if let Some((r, d)) =
                            self.oracle
                                .find_within(kernel, from, &current, exact, c.tolerance())
                        {
                            trace!(
                                "{}: {before:?} {} -> state {r}",
                                item_name(item),
                                summary(c)
                            );
                            self.commit(before, base, fallback);
                            self.oracle.cursor = Some(r);
                            self.report.history.matched += 1;
                            self.accepted(at, index, &uids, c, Some(true));
                            self.loosely_matched(&item_name(item), d);
                            return;
                        }
                        if exact && self.with_pending_bodies(at, index, from, &uids, c, &current) {
                            return;
                        }
                        let kernel = self.doc.kernel();
                        if tracing() {
                            let next = self.oracle.sigs(kernel, from);
                            trace!(
                                "{}: {before:?} {} -> {:?}, next state {from}: {:?}",
                                item_name(item),
                                summary(c),
                                short(&current),
                                short(&next)
                            );
                        }
                        self.undo_to(inner);
                        last_error = "its result is not in the file's history".to_owned();
                    }
                    Err(e) => {
                        trace!("{}: {before:?} {} failed: {e}", item_name(item), summary(c));
                        let stopped = e == STOPPED;
                        last_error = e;
                        if stopped {
                            break;
                        }
                    }
                }
            }
            self.undo_to(depth);
        }
        self.failed(at, last_error);
    }

    /// An item whose state the history names: the replay is brought to the
    /// state before it (a base feature for what the items before changed
    /// without being replayed), then its candidates must give `state`, else
    /// the state comes in as its fallback.
    fn verified_at(&mut self, at: usize, index: i64, item: &TimelineItem, state: usize) {
        let name = item_name(item);
        let start = if self.oracle.cursor == Some(state) {
            Some(state)
        } else {
            state.checked_sub(1)
        };
        trace!(
            "{name}: its state {state}, the replay at {:?}, {} pending",
            self.oracle.cursor,
            self.pending.len()
        );
        let kernel = self.doc.kernel();
        let expected = match start {
            Some(s) => self.oracle.try_sigs(kernel, s),
            None => Some(Vec::new()),
        };
        let current = self.current_sigs();
        if expected.is_none() {
            // Not rebuilt: the replay goes on as it is.
            trace!("{name}: the state before it could not be rebuilt");
        } else if expected.is_some_and(|e| history::bodies_distance(&e, &current).is_some()) {
            // The pending items changed nothing (or are in the state).
            self.commit(Before::Keep, start, None);
        } else if let Some(s) = start {
            let depth = self.doc.undo_depth();
            let pending = !self.pending.is_empty();
            match self.fallback_to(s, None) {
                Ok(fallback) => {
                    if !pending && let Some(uid) = fallback {
                        let base = self.doc.feature(uid).map(|f| f.name.clone());
                        self.report.warnings.push(format!(
                            "{} brings the bodies to the file's state before {name}",
                            base.unwrap_or_default()
                        ));
                    }
                    self.commit(Before::State(s), Some(s), fallback);
                }
                Err(e) => {
                    trace!("{name}: the state before it did not come in: {e}");
                    self.undo_to(depth);
                }
            }
        }
        if start.is_some() {
            self.oracle.cursor = start;
        }
        let kernel = self.doc.kernel();
        let target = self.oracle.try_sigs(kernel, state);
        let Some(target) = target else {
            self.unchecked(at, index, item, state);
            return;
        };
        let mut last_error = String::from("no definition gave the file's result");
        let started = std::time::Instant::now();
        let budget = self.options.item_seconds;
        let translated = match self.give_up(index) {
            Some(reason) => Err(reason),
            None => self.translate(index, item),
        };
        let translated = translated.map(|c| self.rank(c, state));
        // Extrusions whose prism holds less than the state adds or
        // removes (or that change the volume the other way) are not tried.
        let change = target.iter().map(|s| s.volume).sum::<f64>()
            - self.current_sigs().iter().map(|s| s.volume).sum::<f64>();
        let translated = translated.map(|mut c| {
            let before = c.len();
            c.retain(|c| !bounded(c) || c.predicted.is_some_and(|p| can_change(p, change)));
            if c.len() < before {
                trace!(
                    "{name}: {} of {before} extrusions cannot change the volume by {change:.3}",
                    before - c.len()
                );
            }
            c
        });
        match translated {
            Ok(c) if c.is_empty() => last_error = "nothing to try".to_owned(),
            Ok(candidates) => {
                // The bodies a definition without participants changed:
                // limited to participants that include them it gives the
                // same result, which is not built again (on large bodies
                // each try takes seconds).
                let before: HashMap<BodyUid, Sig> = self.current_bodies().into_iter().collect();
                let mut changed_by: Vec<(&Value, BTreeSet<String>)> = Vec::new();
                // The geometry kernel fails a fillet or chamfer of more
                // edges too (contour by contour, unless at a vertex): such
                // guesses are left out.
                let mut failed_dressups: Vec<(Value, BTreeSet<String>)> = Vec::new();
                let mut attempt = 0;
                for c in &candidates {
                    // (Participants in another component take the feature
                    // there: not the same.)
                    if let Some((free, limited)) = participants_of(c)
                        && self.def_component(&free) == self.def_component(&c.defs[0])
                        && changed_by
                            .iter()
                            .any(|(d, changed)| **d == free && changed.is_subset(&limited))
                    {
                        trace!(
                            "{name}: {} gives what it gave without participants",
                            summary(c)
                        );
                        continue;
                    }
                    if let Some((shape, edges)) = dressup_edges(c)
                        && failed_dressups
                            .iter()
                            .any(|(s, failed)| *s == shape && failed.is_subset(&edges))
                    {
                        trace!("{name}: {} has the edges of one that failed", summary(c));
                        continue;
                    }
                    if attempt >= self.options.max_attempts
                        || started.elapsed().as_secs_f64() > budget
                    {
                        last_error = format!("{last_error} (gave up after {attempt} attempts)");
                        break;
                    }
                    attempt += 1;
                    let inner = self.doc.undo_depth();
                    match self.try_candidate(index, c, item.name()) {
                        Ok(uids) => {
                            let bodies = self.current_bodies();
                            if let [def] = c.defs.as_slice()
                                && def.get("participants").is_none()
                            {
                                let after: HashMap<BodyUid, Sig> = bodies.iter().copied().collect();
                                let changed = before
                                    .iter()
                                    .filter(|(uid, sig)| {
                                        !after.get(uid).is_some_and(|s| s.same(sig))
                                    })
                                    .map(|(uid, _)| uid.to_string())
                                    .collect();
                                changed_by.push((def, changed));
                            }
                            let current: Vec<Sig> = bodies.iter().map(|(_, s)| *s).collect();
                            if let Some((d, _)) =
                                history::bodies_distance_within(&target, &current, c.tolerance())
                            {
                                trace!("{name}: {} -> its state {state}", summary(c));
                                self.oracle.cursor = Some(state);
                                self.report.history.matched += 1;
                                self.accepted(at, index, &uids, c, Some(true));
                                self.loosely_matched(&name, d);
                                return;
                            }
                            trace!(
                                "{name}: {} -> {:?}, its state {state}: {:?}",
                                summary(c),
                                short(&current),
                                short(&target)
                            );
                            self.undo_to(inner);
                            last_error = "its result is not in the file's history".to_owned();
                        }
                        Err(e) if e == STOPPED => {
                            last_error = e;
                            break;
                        }
                        Err(e) => {
                            trace!("{name}: {} failed: {e}", summary(c));
                            // More edges can end a rounding at a vertex.
                            if !e.contains("vertex") {
                                failed_dressups.extend(dressup_edges(c));
                            }
                            last_error = e;
                        }
                    }
                }
            }
            Err(e) => {
                trace!("{name}: {e}");
                last_error = e;
            }
        }
        if !self.options.fallback {
            self.note(at, Outcome::Skipped, last_error);
            return;
        }
        let label = self.report.items[at].name.clone();
        let depth = self.doc.undo_depth();
        match self.fallback_to(state, Some(&label)) {
            Ok(Some(uid)) => {
                self.oracle.cursor = Some(state);
                self.report.items[at].features.push(uid.to_string());
                self.note(at, Outcome::Fallback, last_error);
                // Items waiting since a state that was not rebuilt.
                for p in std::mem::take(&mut self.pending) {
                    let note = format!("{}; in the fallback of {label}", p.reason);
                    self.note(p.report, Outcome::Fallback, note);
                }
            }
            Ok(None) => {
                self.oracle.cursor = Some(state);
                self.commit(Before::Keep, Some(state), None);
                self.note(
                    at,
                    Outcome::Skipped,
                    format!("{last_error}; the file's history shows no change by it"),
                );
            }
            Err(e) => {
                self.undo_to(depth);
                self.note(at, Outcome::Skipped, format!("{last_error}; fallback: {e}"));
            }
        }
    }

    /// An item whose change the bodies hold already (a base feature before
    /// it brought them past it, or the stored design without a history):
    /// its first definition (`guesses` too) that builds and leaves the
    /// bodies as they are, accepted parametric. False when none does (the
    /// bodies unchanged).
    fn on_base(&mut self, at: usize, index: i64, item: &TimelineItem, guesses: bool) -> bool {
        let Ok(candidates) = self.translate(index, item) else {
            return false;
        };
        let before = self.current_sigs();
        let started = std::time::Instant::now();
        let budget = self.options.item_seconds;
        let usable = candidates.iter().filter(|c| guesses || !c.guess);
        for c in usable.take(self.options.max_candidates) {
            if started.elapsed().as_secs_f64() > budget {
                break;
            }
            let depth = self.doc.undo_depth();
            let uids = match self.try_candidate(index, c, item.name()) {
                Ok(uids) => uids,
                Err(e) if e == STOPPED => break,
                Err(_) => continue,
            };
            let after = self.current_sigs();
            if history::bodies_distance(&before, &after).is_some_and(|(_, exact)| exact) {
                trace!(
                    "{}: {} leaves the bodies as they are",
                    item_name(item),
                    summary(c)
                );
                self.accepted(at, index, &uids, c, Some(true));
                let report = &mut self.report.items[at];
                let base = concat!(
                    "its change is in the bodies before it already ",
                    "(a base feature took them past it); it leaves them as they are"
                );
                report.note = Some(match report.note.take() {
                    Some(n) if !n.is_empty() => format!("{n}; {base}"),
                    _ => base.to_owned(),
                });
                return true;
            }
            self.undo_to(depth);
        }
        false
    }

    /// An item whose state could not be rebuilt: its first definition that
    /// builds is taken unchecked (a guess when there is nothing else); when
    /// none does, it waits for the next fallback.
    fn unchecked(&mut self, at: usize, index: i64, item: &TimelineItem, state: usize) {
        let name = item_name(item);
        trace!("{name}: its state {state} could not be rebuilt");
        self.oracle.cursor = Some(state);
        let unchecked = "not checked: its state in the file's history could not be rebuilt";
        let candidates = match self.give_up(index) {
            Some(reason) => Err(reason),
            None => self.translate(index, item),
        };
        let mut last = match candidates {
            Ok(candidates) => {
                let ordered = candidates
                    .iter()
                    .filter(|c| !c.guess)
                    .chain(candidates.iter().filter(|c| c.guess));
                let mut last = "nothing to try".to_owned();
                for c in ordered.take(self.options.max_candidates) {
                    match self.try_candidate(index, c, item.name()) {
                        Ok(uids) => {
                            self.accepted(at, index, &uids, c, None);
                            let report = &mut self.report.items[at];
                            report.note = Some(match report.note.take() {
                                Some(n) if !n.is_empty() => format!("{n}; {unchecked}"),
                                _ => unchecked.to_owned(),
                            });
                            return;
                        }
                        Err(e) => {
                            let stopped = e == STOPPED;
                            last = e;
                            if stopped {
                                break;
                            }
                        }
                    }
                }
                last
            }
            Err(e) => e,
        };
        last = format!("{last}; {unchecked}");
        if self.options.fallback {
            self.pending.push(Pending {
                report: at,
                reason: last,
            });
            self.note(at, Outcome::Fallback, "");
        } else {
            self.note(at, Outcome::Skipped, last);
        }
    }

    /// The design's time limit is over: the remaining items take the file's
    /// bodies without trying definitions.
    fn out_of_time(&self) -> bool {
        self.options
            .time_limit
            .is_some_and(|t| self.started.elapsed().as_secs_f64() > t)
    }

    /// Why no definitions are tried for the item: the geometry kernel hung
    /// on it in an earlier try, the import was stopped, or the time limit
    /// is over.
    fn give_up(&self, index: i64) -> Option<String> {
        if self.options.hung_items.contains(&index) {
            Some(HUNG.to_owned())
        } else if self.stopped(index) {
            Some(STOPPED.to_owned())
        } else if self.out_of_time() {
            Some("the import's time limit was reached".to_owned())
        } else {
            None
        }
    }

    /// The history states the guesses for an item look at: the next `n`.
    pub(crate) fn lookahead(&self, n: usize) -> std::ops::Range<usize> {
        let from = self.oracle.next_index();
        from..self.oracle.count().min(from + n)
    }

    /// The import moves on (for the watchdog).
    pub(crate) fn tick(&self) {
        if let Some(p) = &self.options.progress {
            p.tick();
        }
    }

    /// The item's result (`current`, just added) is part of a next state
    /// whose other bodies the pending items made in the same step: those
    /// come in by a base feature for them, and the item is accepted.
    fn with_pending_bodies(
        &mut self,
        at: usize,
        index: i64,
        from: usize,
        uids: &[FeatureUid],
        c: &Candidate,
        current: &[Sig],
    ) -> bool {
        let kernel = self.doc.kernel();
        let before = match self.oracle.cursor {
            Some(q) => self.oracle.sigs(kernel, q),
            None => Vec::new(),
        };
        // The item must have changed something itself.
        if current.is_empty() || history::bodies_distance(&before, current).is_some_and(|d| d.1) {
            return false;
        }
        let span = self.pending.len() + 1;
        let Some(q) = self.oracle.find_superset(kernel, from, span, current) else {
            return false;
        };
        let depth = self.doc.undo_depth();
        let fallback = match self.fallback_to(q, None) {
            Ok(f) => f,
            Err(e) => {
                trace!(
                    "{}: pending bodies of state {q}: {e}",
                    self.report.items[at].name
                );
                self.undo_to(depth);
                return false;
            }
        };
        let kernel = self.doc.kernel();
        let state = self.oracle.sigs(kernel, q);
        if !history::bodies_distance(&state, &self.current_sigs()).is_some_and(|d| d.1) {
            self.undo_to(depth);
            return false;
        }
        trace!(
            "{}: {} -> state {q} with the pending items' bodies",
            self.report.items[at].name,
            summary(c)
        );
        let pending = std::mem::take(&mut self.pending);
        self.settle_fallback(pending, fallback);
        self.oracle.cursor = Some(q);
        self.report.history.matched += 1;
        self.accepted(at, index, uids, c, Some(true));
        true
    }

    /// Candidates in the order to try against the history: by how close
    /// their estimated volume change comes to a change of one of the next
    /// states, those without an estimate in their own order. At most
    /// `max_candidates`.
    fn rank(&mut self, mut candidates: Vec<Candidate>, from: usize) -> Vec<Candidate> {
        if candidates.iter().any(|c| c.predicted.is_some()) {
            let kernel = self.doc.kernel();
            let volume = |sigs: &[Sig]| sigs.iter().map(|s| s.volume).sum::<f64>();
            let now = volume(&self.current_sigs());
            let end = self.oracle.count().min(from + 4);
            let changes: Vec<f64> = (from..end)
                .map(|q| volume(&self.oracle.sigs(kernel, q)) - now)
                .collect();
            let score = |c: &Candidate| match c.predicted {
                Some(p) => changes
                    .iter()
                    .map(|d| (p - d).abs() / d.abs().max(1e-6))
                    .fold(f64::INFINITY, f64::min),
                None => 0.5,
            };
            candidates.sort_by(|a, b| score(a).total_cmp(&score(b)));
        }
        candidates.truncate(self.options.max_candidates);
        candidates
    }

    /// The pending items are settled by `before`; `fallback` is the base
    /// feature it added.
    fn commit(&mut self, before: Before, base: Option<usize>, fallback: Option<FeatureUid>) {
        let pending = std::mem::take(&mut self.pending);
        match before {
            Before::Keep => {
                for p in pending {
                    let note = format!("{}; the file's history shows no change by it", p.reason);
                    self.note(p.report, Outcome::Skipped, note);
                }
            }
            Before::State(q) => {
                self.oracle.cursor = base.or(Some(q));
                self.settle_fallback(pending, fallback);
            }
        }
    }

    /// Reports pending items as replaced by a base feature.
    fn settle_fallback(&mut self, pending: Vec<Pending>, fallback: Option<FeatureUid>) {
        let base = fallback.and_then(|uid| {
            let name = self.doc.feature(uid)?.name.clone();
            Some((uid.to_string(), name))
        });
        let count = pending.len();
        for (i, p) in pending.into_iter().enumerate() {
            let note = if i + 1 == count {
                p.reason.clone()
            } else {
                format!(
                    "{}; in the fallback of {}",
                    p.reason,
                    base.as_ref().map_or("", |b| b.1.as_str())
                )
            };
            if i + 1 == count
                && let Some((uid, _)) = &base
            {
                self.report.items[p.report].features.push(uid.clone());
            }
            self.note(p.report, Outcome::Fallback, note);
        }
    }

    /// An item that could not be replayed.
    fn failed(&mut self, at: usize, reason: String) {
        if !self.options.fallback {
            self.note(at, Outcome::Skipped, reason);
            return;
        }
        if self.oracle.enabled {
            self.pending.push(Pending { report: at, reason });
            self.note(at, Outcome::Fallback, "");
            return;
        }
        // No history: the stored bodies stand in for the rest.
        let name = self.report.items[at].name.clone();
        match self.fallback_final(&name) {
            Ok(uid) => {
                self.at_final = true;
                self.report.items[at].features.push(uid.to_string());
                self.note(
                    at,
                    Outcome::Fallback,
                    format!("{reason}; replaced by the stored bodies"),
                );
            }
            Err(e) => self.note(at, Outcome::Skipped, format!("{reason}; fallback: {e}")),
        }
    }

    /// Replaces the replay's bodies that differ from history state `q` with
    /// that state's bodies, as one base feature named after the pending
    /// item. Ok(None) when nothing differs.
    fn fallback_to(&mut self, q: usize, name: Option<&str>) -> Result<Option<FeatureUid>, String> {
        let kernel = self.doc.kernel();
        let state: Vec<(StoredBody<K::Shape>, Sig)> = self.oracle.state(kernel, q)?.to_vec();
        let name = name.map(str::to_owned).or_else(|| {
            self.pending
                .last()
                .map(|p| self.report.items[p.report].name.clone())
        });
        self.replace_bodies(&state, name.as_deref(), &format!("ASM history state {q}"))
    }

    /// Without a history: every body replaced by the stored design's.
    fn fallback_final(&mut self, name: &str) -> Result<FeatureUid, String> {
        let kernel = self.doc.kernel();
        let finals: Vec<(StoredBody<K::Shape>, Sig)> = self
            .oracle
            .final_bodies()?
            .into_iter()
            .filter_map(|b| {
                let s = Sig::of(kernel, &b.shape)?;
                Some((b, s))
            })
            .collect();
        if finals.is_empty() {
            return Err("the file stores no solid bodies".to_owned());
        }
        self.replace_bodies(&finals, Some(name), "stored design")?
            .ok_or_else(|| "the bodies are the stored ones already".to_owned())
    }

    /// Base features, one per component that changes: bodies of `target`
    /// not in the replay replace the replay's bodies not in `target`, in
    /// their components (a body in another component than the file's moves
    /// there). Returns the first.
    fn replace_bodies(
        &mut self,
        target: &[(StoredBody<K::Shape>, Sig)],
        name: Option<&str>,
        source: &str,
    ) -> Result<Option<FeatureUid>, String> {
        self.tick();
        let current: Vec<(BodyUid, Sig, ComponentUid)> = self
            .current_bodies()
            .into_iter()
            .map(|(uid, sig)| {
                let c = self.doc.body_component(uid).unwrap_or(ComponentUid::ROOT);
                (uid, sig, c)
            })
            .collect();
        let mut used = vec![false; current.len()];
        type Bodies<'b, S> = Vec<(&'b StoredBody<S>, &'b Sig)>;
        let mut new: BTreeMap<ComponentUid, Bodies<'_, K::Shape>> = BTreeMap::new();
        for (body, sig) in target {
            let component = self.components.of_body(body.component);
            match (0..current.len())
                .find(|&i| !used[i] && current[i].2 == component && current[i].1.same(sig))
            {
                Some(i) => used[i] = true,
                None => new.entry(component).or_default().push((body, sig)),
            }
        }
        let mut old: BTreeMap<ComponentUid, Vec<(BodyUid, Sig)>> = BTreeMap::new();
        for ((uid, sig, component), used) in current.iter().zip(&used) {
            if !used {
                old.entry(*component).or_default().push((*uid, *sig));
            }
        }
        if new.is_empty() && old.is_empty() {
            return Ok(None);
        }
        let components: BTreeSet<ComponentUid> = new.keys().chain(old.keys()).copied().collect();
        let mut inputs = Vec::new();
        for component in components {
            let new = new.remove(&component).unwrap_or_default();
            let mut old = old.remove(&component).unwrap_or_default();
            // Pair each new body with the nearest old one, so bodies keep
            // their identity where they can.
            let mut replaces = Vec::new();
            let mut bodies = Vec::new();
            for (body, sig) in &new {
                let nearest = old
                    .iter()
                    .enumerate()
                    .min_by(|a, b| {
                        let d = |s: &Sig| geom::distance(s.center, sig.center);
                        d(&a.1.1).total_cmp(&d(&b.1.1))
                    })
                    .map(|(i, _)| i);
                if let Some(i) = nearest {
                    replaces.push(old.remove(i).0);
                }
                bodies.push(*body);
            }
            // Bodies are listed replacing ones first.
            let paired = replaces.len();
            let mut ordered: Vec<ImportBody<K::Shape>> = Vec::new();
            for body in bodies.iter().take(paired).chain(bodies.iter().skip(paired)) {
                ordered.push(ImportBody {
                    name: body.name.clone(),
                    color: None,
                    shape: body.shape.clone(),
                });
            }
            // Removed bodies come after the replacing ones (only them: a
            // base feature without bodies removes the bodies it replaces).
            replaces.extend(old.iter().map(|(uid, _)| *uid));
            inputs.push(BaseInput {
                name: if inputs.is_empty() {
                    name.filter(|n| !n.trim().is_empty()).map(str::to_owned)
                } else {
                    None
                },
                source: Some(format!("{} ({source})", self.report.label)),
                replaces,
                component: Some(component),
                ..BaseInput::new(ordered)
            });
        }
        let label = name.map_or_else(|| "Add Base Feature".to_owned(), |n| format!("Add {n}"));
        let imported = match self.doc.add_base_features(&label, inputs.clone()) {
            Ok(i) => i,
            // The name is taken (the item was tried already): default name.
            Err(e) if e.to_string().contains("already named") => {
                inputs[0].name = None;
                self.doc
                    .add_base_features(&label, inputs)
                    .map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        };
        let uids: Vec<FeatureUid> = imported.iter().map(|i| i.feature.uid).collect();
        if let Some(e) = uids
            .iter()
            .find_map(|uid| self.doc.status(*uid).and_then(|s| s.error()))
        {
            let e = e.to_owned();
            self.doc.undo();
            return Err(e);
        }
        Ok(uids.first().copied())
    }

    /// The end of the timeline: pending items take the last history state,
    /// and the bodies are compared with the stored design.
    fn finish(&mut self) {
        if !self.pending.is_empty() {
            let pending = std::mem::take(&mut self.pending);
            let name = pending
                .last()
                .map(|p| self.report.items[p.report].name.clone());
            let result = match self.oracle.last() {
                Some(q) => self.fallback_to(q, name.as_deref()),
                None => Err("no history state".to_owned()),
            };
            match result {
                Ok(Some(uid)) => {
                    self.oracle.cursor = self.oracle.last();
                    self.settle_fallback(pending, Some(uid));
                }
                Ok(None) => {
                    for p in pending {
                        let note = format!("{}; the bodies are the file's without it", p.reason);
                        self.note(p.report, Outcome::Skipped, note);
                    }
                }
                Err(e) => {
                    for p in pending {
                        let note = format!("{}; fallback: {e}", p.reason);
                        self.note(p.report, Outcome::Skipped, note);
                    }
                }
            }
        }
        let history = &mut self.report.history;
        history.built = self.oracle.built;
        history.unbuilt = self.oracle.unbuilt();
        history.reached_end = self.oracle.enabled
            && self.oracle.cursor.is_some()
            && self.oracle.cursor == self.oracle.last();
        if let Some(p) = &self.options.progress {
            p.at(Progress::FINISHING);
        }
        // A stop from now on has nothing left to cut short.
        self.report_stop();
        // A kernel call that never returns, to test the watchdog with.
        if self.options.compare
            && std::env::var_os("MITCAD_IMPORT_STALL").is_some_and(|v| v == "compare")
        {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        }
        self.compare_final();
    }

    /// Whether a stored body has no body like it in its component in the
    /// replay (the replay put it into another one).
    fn components_differ(&self, finals: &[(StoredBody<K::Shape>, Sig)]) -> bool {
        let current: Vec<(Sig, ComponentUid)> = self
            .current_bodies()
            .into_iter()
            .map(|(uid, sig)| {
                let c = self.doc.body_component(uid).unwrap_or(ComponentUid::ROOT);
                (sig, c)
            })
            .collect();
        finals.iter().any(|(body, sig)| {
            let component = self.components.of_body(body.component);
            !current
                .iter()
                .any(|(s, c)| *c == component && s.matches(sig))
        })
    }

    /// The bodies stored in the file against the replay, and a last fallback
    /// where they differ.
    fn compare_final(&mut self) {
        let finals = match self.oracle.final_bodies() {
            Ok(f) => f,
            Err(e) => {
                self.report.warnings.push(format!("stored bodies: {e}"));
                return;
            }
        };
        let kernel = self.doc.kernel();
        let finals: Vec<(StoredBody<K::Shape>, Sig)> = finals
            .into_iter()
            .filter_map(|b| {
                let s = Sig::of(kernel, &b.shape)?;
                Some((b, s))
            })
            .collect();
        if finals.is_empty() {
            return;
        }
        let differs = !history::same_bodies(
            &finals.iter().map(|(_, s)| *s).collect::<Vec<_>>(),
            &self.current_sigs(),
        ) || self.components_differ(&finals);
        if differs && self.options.fallback && !self.at_final {
            let replayed = self.doc.bodies().len();
            match self.replace_bodies(&finals, Some("Stored bodies"), "stored design") {
                Ok(Some(uid)) if replayed == 0 => self.report.warnings.push(format!(
                    "nothing replayed: the file's bodies come in as they are ({uid})"
                )),
                Ok(Some(uid)) => self.report.warnings.push(format!(
                    "the replay differs from the stored design; {uid} replaces the bodies that differ"
                )),
                Ok(None) => {}
                Err(e) => self
                    .report
                    .warnings
                    .push(format!("the replay differs from the stored design: {e}")),
            }
        }
        // Over the time limit or stopped: volumes only.
        let compare = self.options.compare && !self.out_of_time() && self.report.stopped.is_none();
        let progress = self.options.progress.clone();
        let tick = move || {
            if let Some(p) = &progress {
                p.tick();
            }
        };
        self.report.bodies = refs::compare_bodies(self.doc, &finals, compare, &tick);
        let matched: Vec<String> = self
            .report
            .bodies
            .iter()
            .filter_map(|b| b.mitcad.clone())
            .collect();
        self.report.extra_bodies = self
            .doc
            .bodies()
            .iter()
            .filter(|b| !matched.iter().any(|m| m.starts_with(&b.uid.to_string())))
            .map(|b| format!("{} {}", b.uid, b.name))
            .collect();
    }
}

#[cfg(test)]
mod tests;
