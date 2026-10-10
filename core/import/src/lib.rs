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
// Joints, as-built joints, joint origins and ground items (mitcad#55).
mod joints;
// Captured positions (mitcad#75).
mod captures;
// The material a history state removed (mitcad#85).
mod removed;
// The learning dump of settled items (mitcad#96).
pub mod learn;
// Profile regions from the decoded loops (mitcad#96).
mod profiles;
// Bodies by the item that made them, consumed combine tools (mitcad#96).
mod producers;

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use mitcad_f3d::design::ir::{Detail, Dump, TimelineItem};
use mitcad_model::assembly::inverse;
use mitcad_model::features::{FeatureDef, ValueInput};
use mitcad_model::{
    BaseInput, BodyUid, ComponentUid, Document, FeatureUid, ImportBody, Kernel, KernelError,
    RecomputeMonitor, SketchFrame, Transform,
};
use serde_json::Value;

pub use history::{NoGeometry, Oracle, SheetSig, Sig, StoredBody, StoredGeometry};
pub use report::{
    BodyReport, ComponentReport, DesignReport, ImportReport, ItemReport, JointReport,
    LightBulbReport, LowMemoryReport, Outcome, StopReport,
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
    /// Modelling items whose definitions an earlier try found no state for,
    /// with why ([`Progress::failed_items`]; run again after the kernel
    /// hung on a later item, `mitcad-ffi`): the items before the hung one
    /// replay as before, so these take the file's bodies again at once,
    /// with the same reason, without trying their definitions a second time
    /// (a pattern of a thousand copies can take minutes a try).
    pub failed_items: Vec<(i64, String)>,
    /// The threads that evaluate an item's definitions (mitcad#95): with
    /// more than one, workers evaluate the definitions ranked after the
    /// one being evaluated, each on its own copy of the document, and the
    /// import takes their results in rank order (the same import as with
    /// one, faster). 1 (the default): one after another on the import's
    /// thread.
    pub threads: usize,
    /// Starts the workers' threads (None: plain threads with
    /// [`WORKER_STACK`]).
    pub spawn: Option<Spawner>,
    /// Write the learning dump of the items the history settled
    /// (mitcad#96, [`learn`]).
    pub learn: Option<learn::LearnOptions>,
    /// Check the bodies a definition that gives an item's history state
    /// made or changed with the geometry kernel's checker
    /// ([`Kernel::is_valid`]): one that makes an invalid body is not taken,
    /// and the item takes the file's bodies (the `.ipt` import, mitcad#60:
    /// a boolean checked only near its change).
    ///
    /// [`Kernel::is_valid`]: mitcad_model::Kernel::is_valid
    pub validate: bool,
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
            failed_items: Vec::new(),
            threads: 1,
            spawn: None,
            learn: None,
            validate: false,
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
    /// The modelling items whose definitions gave no state, with why
    /// ([`Options::failed_items`] of a try run again).
    failed: std::sync::Mutex<Vec<(i64, String)>>,
    /// Cancelled when the watchdog gives the try up
    /// ([`Progress::abandon`]).
    abandoned: Arc<RecomputeMonitor>,
    /// The memory of the process (mitcad#80): see [`Progress::low_memory`].
    memory: std::sync::Mutex<Memory>,
    /// Set when the import is to drop what it can do without
    /// ([`Progress::tight_on_memory`]).
    memory_tight: std::sync::atomic::AtomicBool,
    /// The share of the tightest memory limit in use as last measured
    /// ([`Progress::set_memory_share`]), as f64 bits.
    memory_share: AtomicU64,
    /// Cancelled while the memory is too short for definitions evaluated
    /// ahead ([`Progress::set_memory_share`]); a new one once there is
    /// enough again.
    ahead: std::sync::Mutex<Arc<RecomputeMonitor>>,
    /// The threads that work beside the import's own, and how many it may
    /// have in all ([`Progress::helper`], mitcad#103).
    helpers: Arc<ahead::Helpers>,
}

/// What the memory guard told an import (mitcad#80).
#[derive(Debug)]
struct Memory {
    /// Cancelled while the process is low on memory; a new one once it has
    /// recovered.
    low: Arc<RecomputeMonitor>,
    /// How many times it ran low.
    lows: u64,
    /// The memory in use as last measured.
    last: String,
    /// When it first ran low: the memory in use, and where the import was
    /// (as [`Progress::item`]).
    first_low: Option<(String, i64)>,
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
            failed: std::sync::Mutex::new(Vec::new()),
            abandoned: Arc::new(RecomputeMonitor::new()),
            memory: std::sync::Mutex::new(Memory {
                low: Arc::new(RecomputeMonitor::new()),
                lows: 0,
                last: String::new(),
                first_low: None,
            }),
            memory_tight: std::sync::atomic::AtomicBool::new(false),
            memory_share: AtomicU64::new(0),
            ahead: std::sync::Mutex::new(Arc::new(RecomputeMonitor::new())),
            helpers: Arc::default(),
        }
    }

    /// Memory is getting tight (mitcad#80; `mitcad-ffi`'s memory guard,
    /// before it is low): before its next item the import drops the cached
    /// results of the definitions it tried and the bodies of the history
    /// states behind it, which it builds again if it needs them; what it
    /// imports stays the same.
    pub fn tight_on_memory(&self) {
        self.memory_tight.store(true, Ordering::Relaxed);
    }

    /// Whether the import is asked to drop what it can do without
    /// ([`Progress::tight_on_memory`]); the request is taken.
    fn take_tight(&self) -> bool {
        self.memory_tight.swap(false, Ordering::Relaxed)
    }

    /// The process is low on memory (mitcad#80; `mitcad-ffi`'s memory
    /// guard, or a kernel operation that failed for lack of it): an
    /// allocation that fails would end the process. The definition being
    /// evaluated is cut short (P7e) and its item takes the file's bodies,
    /// as do the modelling items after it until the memory has recovered
    /// ([`Progress::memory_recovered`]); the import drops the results and
    /// history states it can do without, and compares the final bodies by
    /// volume only if the memory is still low then. `memory` says how much
    /// is in use, for the report.
    pub fn low_memory(&self, memory: &str) {
        let item = self.item.load(Ordering::Relaxed);
        if let Ok(mut m) = self.memory.lock()
            && !m.low.is_cancelled()
        {
            m.lows += 1;
            if m.first_low.is_none() {
                m.first_low = Some((memory.to_owned(), item));
            }
            m.low.cancel();
        }
    }

    /// The memory has recovered from being low ([`Progress::low_memory`]):
    /// the modelling items from the next one on are replayed again.
    pub fn memory_recovered(&self) {
        if let Ok(mut m) = self.memory.lock()
            && m.low.is_cancelled()
        {
            m.low = Arc::new(RecomputeMonitor::new());
        }
    }

    /// Whether the process is low on memory ([`Progress::low_memory`]).
    pub fn is_memory_low(&self) -> bool {
        self.memory.lock().is_ok_and(|m| m.low.is_cancelled())
    }

    /// How many times the process ran low on memory.
    pub fn memory_lows(&self) -> u64 {
        self.memory.lock().map_or(0, |m| m.lows)
    }

    /// When the process first ran low on memory: how much was in use, and
    /// where the import was (the item being replayed,
    /// [`Progress::STARTING`] or [`Progress::FINISHING`]).
    pub fn first_low_memory(&self) -> Option<(String, i64)> {
        self.memory.lock().ok().and_then(|m| m.first_low.clone())
    }

    /// Cancelled once the process is low on memory, for the definitions
    /// evaluated from now on.
    fn memory_monitor(&self) -> Option<Arc<RecomputeMonitor>> {
        self.memory.lock().ok().map(|m| Arc::clone(&m.low))
    }

    /// The memory in use as last measured (e.g. "7.1 GiB of 11.4 GiB
    /// (address-space limit)"), for traces.
    pub fn set_memory(&self, memory: &str) {
        if let Ok(mut m) = self.memory.lock() {
            memory.clone_into(&mut m.last);
        }
    }

    /// The share of the tightest memory limit the process uses as last
    /// measured (`mitcad-ffi`'s memory guard): above half of it no more
    /// definitions are evaluated ahead (mitcad#95).
    /// From [`ahead::AHEAD_STOP`] of it on, the definitions being
    /// evaluated ahead are cut short (and evaluated again on the import's
    /// thread when it comes to them).
    pub fn set_memory_share(&self, share: f64) {
        self.memory_share.store(share.to_bits(), Ordering::Relaxed);
        if let Ok(mut ahead) = self.ahead.lock() {
            if share >= ahead::AHEAD_STOP {
                ahead.cancel();
            } else if share < ahead::AHEAD_MEMORY && ahead.is_cancelled() {
                *ahead = Arc::new(RecomputeMonitor::new());
            }
        }
    }

    /// Cancelled while the memory is too short for definitions evaluated
    /// ahead ([`Progress::set_memory_share`]).
    fn ahead_monitor(&self) -> Option<Arc<RecomputeMonitor>> {
        self.ahead.lock().ok().map(|a| Arc::clone(&a))
    }

    /// The share last measured ([`Progress::set_memory_share`]); 0 before.
    pub fn memory_share(&self) -> f64 {
        f64::from_bits(self.memory_share.load(Ordering::Relaxed))
    }

    /// The memory last measured ([`Progress::set_memory`]); empty before.
    pub fn memory(&self) -> String {
        self.memory
            .lock()
            .map(|m| m.last.clone())
            .unwrap_or_default()
    }

    /// The watchdog gave this try up (mitcad#71): the import stops at its
    /// next check, before the next item or definition, and the kernel's
    /// long operations of a definition being tried stop inside (P7e). What
    /// it made is not used: it reports nothing more and leaves its
    /// document as it is.
    pub fn abandon(&self) {
        self.abandoned.cancel();
    }

    /// Whether the watchdog gave this try up ([`Progress::abandon`]).
    pub fn abandoned(&self) -> bool {
        self.abandoned.is_cancelled()
    }

    /// The item the import stopped at, once it did.
    pub fn stopped_at(&self) -> Option<i64> {
        let at = self.stopped.load(Ordering::Relaxed);
        (at != Self::NOT_STOPPED).then_some(at)
    }

    /// The modelling items so far whose definitions were tried and gave no
    /// state, with why (not those that took the file's bodies without
    /// trying: hung, stopped, out of time, or failed in an earlier try).
    pub fn failed_items(&self) -> Vec<(i64, String)> {
        self.failed.lock().map(|f| f.clone()).unwrap_or_default()
    }

    fn failed(&self, item: i64, reason: &str) {
        if let Ok(mut f) = self.failed.lock() {
            f.push((item, reason.to_owned()));
        }
    }

    /// The import moved on: a kernel call returned. Long loops of the
    /// import's own (building and measuring a history state's bodies,
    /// finding faces and edges by their fingerprints, comparing bodies)
    /// tick after each kernel call, so that the watchdog takes only a
    /// kernel call that does not return for a hang (mitcad#82); see
    /// [`tick`].
    pub fn tick(&self) {
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

thread_local! {
    /// The progress of the import running on this thread
    /// ([`import_design`]), for [`tick`].
    static TICKING: std::cell::RefCell<Option<Arc<Progress>>> =
        const { std::cell::RefCell::new(None) };
}

/// Ticks the progress of the import running on this thread, if any
/// ([`Progress::tick`]): for loops deep in the import, and in the stored
/// geometry that builds a state's bodies (`mitcad-ffi`), that run many
/// kernel calls in a row (mitcad#82: building and measuring the first
/// history state of a large design took 110 s, which the watchdog took for
/// a hang). Call it between kernel calls only: a call that does not return
/// must still look like one.
pub fn tick() {
    TICKING.with(|t| {
        if let Some(p) = t.borrow().as_ref() {
            p.tick();
        }
    });
}

/// The progress of the import running on this thread, if any: for the
/// geometry kernel's long operations, whose progress (reported from their
/// worker threads too) is the import's (`mitcad-ffi`, mitcad#82).
pub fn current_progress() -> Option<Arc<Progress>> {
    TICKING.with(|t| t.borrow().clone())
}

/// Whether the import running on this thread was given up by the
/// watchdog ([`Progress::abandon`]): long loops may stop early, as what
/// the try makes is dropped.
pub fn abandoned() -> bool {
    TICKING.with(|t| t.borrow().as_ref().is_some_and(|p| p.abandoned()))
}

/// While it lives, [`tick`] ticks `progress` on this thread; the import
/// before it (none, usually) afterwards. (`mitcad-ffi`'s threads that
/// build the history's bodies ahead tick their import's, mitcad#103.)
pub struct Ticking(Option<Arc<Progress>>);

impl Ticking {
    pub fn new(progress: Option<Arc<Progress>>) -> Self {
        Self(TICKING.with(|t| t.replace(progress)))
    }
}

impl Drop for Ticking {
    fn drop(&mut self) {
        let before = self.0.take();
        TICKING.with(|t| *t.borrow_mut() = before);
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
    /// What a rule the learning dump proved reads from the item (the
    /// decoded profile regions, axis and operation of an extrusion or
    /// revolution, mitcad#96): tried before the candidates ranked by
    /// their volume, in its own order.
    pub first: bool,
}

impl Candidate {
    pub fn new(def: Value) -> Self {
        Self {
            defs: vec![def],
            note: None,
            guess: false,
            predicted: None,
            first: false,
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
    /// sketch index and component, with the transform that moved them.
    sketch_copies: HashMap<(i64, ComponentUid), (Transform, SketchInfo)>,
    /// The modelling item the import stopped at ([`Options::stop`]).
    stopped_at: Cell<Option<i64>>,
    /// A modelling item's definitions are being tried: the stop request
    /// is the document's monitor ([`Importer::try_candidate`]).
    trying: bool,
    /// Other definitions of extrusions that gave their states exactly, by
    /// feature, for later patterns and mirrors of them.
    alternatives: HashMap<FeatureUid, Alternatives>,
    /// Set while [`Importer::try_state`] translates a fillet or chamfer:
    /// its edge guesses come one history state at a time.
    guesses: Option<Guesses>,
    /// When the time of the item being replayed (or of the part of it
    /// being tried) is over: the definitions being tried stop there, also
    /// inside the kernel's long operations ([`Importer::interruptible`],
    /// mitcad#69).
    deadline: Option<std::time::Instant>,
    /// The file's captured positions, and where they leave the
    /// occurrences (mitcad#75).
    captures: captures::Captures,
    /// The times the process had run low on memory when the import last
    /// released what it could ([`Importer::release_memory`]).
    memory_released: u64,
    /// The circular edges and faces of each body that can give a joint's
    /// side its frame, with the body's version (mitcad#87).
    frame_entities: std::cell::RefCell<joints::FrameEntities>,
    /// The workers of this import that still run ([`ahead`], mitcad#95).
    workers: Arc<std::sync::atomic::AtomicUsize>,
    /// What the learning dump keeps ([`Options::learn`]).
    learn: Option<learn::Store>,
    /// Timeline index → the fillets and chamfers made for it, which
    /// patterns and mirrors of the item repeat on their copies (mitcad#105).
    dressups: HashMap<i64, Vec<FeatureUid>>,
}

/// Where the edge guesses of a fillet or chamfer go on from, when they come
/// one history state at a time ([`Importer::try_state`]): finding the edges
/// a state lost takes seconds on large bodies, and the item's own state (or
/// the edges found by their names) usually gives it, so the next states'
/// are found only when the definitions before gave no state. The
/// definitions come in the same order as all at once.
#[derive(Debug, Default)]
pub(crate) struct Guesses {
    /// The edges found by their names were tried (they come first).
    pub named: bool,
    /// How many of the next history states were looked at.
    pub states: usize,
    /// No more states to look at.
    pub done: bool,
    /// Definitions given already (so that none comes twice).
    pub seen: BTreeSet<Vec<String>>,
    /// Definitions were given already.
    pub given: bool,
}

/// Imports a design into the document (normally a new one). The report
/// tells how each timeline item came in.
pub fn import_design<K: ImportKernel>(
    doc: &mut Document<K>,
    dump: &Dump,
    geometry: &mut dyn StoredGeometry<K::Shape>,
    options: &Options,
) -> DesignReport {
    let depth = doc.undo_depth();
    // Loops deep in the import tick the watchdog (mitcad#82).
    let _ticking = Ticking::new(options.progress.clone());
    // Planar faces found once per body shape (mitcad#103).
    let _planes = refs::KeptPlanes::new();
    // The threads beside the import's own share `threads` (mitcad#103).
    if let Some(p) = &options.progress {
        p.set_threads(options.threads);
    }
    // Running low on memory limits the result cache for the rest of the
    // import (Importer::release_memory); the document's own budget after.
    let budget = doc.memory_budget();
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
        alternatives: HashMap::new(),
        guesses: None,
        deadline: None,
        captures: captures::Captures::default(),
        memory_released: 0,
        frame_entities: Default::default(),
        workers: Default::default(),
        learn: learn::store(options),
        dressups: HashMap::new(),
    };
    if !options.verify {
        importer.oracle.enabled = false;
    }
    importer.trace_states();
    importer.run();
    importer.light_bulbs();
    importer.learn_write();
    let Importer { doc, report, .. } = importer;
    if let Some(label) = &options.undo_label {
        doc.merge_undo(depth, label);
    }
    doc.set_memory_budget(budget);
    report
}

/// The note of an item the geometry kernel hung on.
const HUNG: &str =
    "the geometry kernel did not finish a definition of it (given up by the import's watchdog)";

/// The note of a modelling item not replayed because the import was
/// stopped ([`Options::stop`]); also why a try the watchdog gave up stops
/// trying definitions ([`Progress::abandon`]: its report is not used).
const STOPPED: &str = "the import was stopped";

/// The note of a modelling item not replayed because the process ran low on
/// memory ([`Progress::low_memory`], mitcad#80).
const LOW_MEMORY: &str = "the import ran low on memory";

/// The note of a modelling item whose result the file does not keep
/// (mitcad#96).
const NO_RESULT: &str = "the file keeps no result for it (suppressed or failed in the file)";

/// The note of an item whose definitions built, none to the file's result.
const NOT_IN_HISTORY: &str = "its result is not in the file's history";

/// The most memory (estimated) the document's cached results may take for
/// the rest of an import once the process ran low on memory (mitcad#80).
const CACHE_AFTER_LOW_MEMORY: u64 = 256 << 20;

/// How long the import waits, once each time the process ran low on memory,
/// for the memory guard to see it recover before the next item
/// ([`Importer::release_memory`]).
const MEMORY_SETTLE: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether two placements are the same within rounding.
fn same_placement(a: &Transform, b: &Transform) -> bool {
    (0..3).all(|r| {
        (0..3).all(|c| (a.linear[r][c] - b.linear[r][c]).abs() <= 1e-12)
            && (a.translation[r] - b.translation[r]).abs() <= 1e-9
    })
}

/// Whether a definition's error ends the item's tries: the import was
/// stopped (or given up), or ran low on memory.
fn halts(error: &str) -> bool {
    error == STOPPED || error == LOW_MEMORY
}

/// Why a definition failed that was cut short when the item's time was
/// over ([`Importer::deadline`]).
const OUT_OF_TIME: &str = "the item's time was over before its definition was evaluated";

/// Why a definition failed that was cut short at the end of the time it
/// is given ([`candidate_seconds`]).
const CANDIDATE_OUT_OF_TIME: &str = "its definition was not evaluated in the time it is given";

/// Why a definition failed that was cut short at the end of its share of
/// the item's time ([`Turns`]): it gives way to the next ones.
const GAVE_WAY: &str = "its definition was not evaluated in its share of the item's time";

/// A time in seconds (negative as none, huge as a day).
fn seconds(s: f64) -> std::time::Duration {
    std::time::Duration::from_secs_f64(s.clamp(0.0, 86_400.0))
}

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

// An item's definitions evaluated ahead, in parallel (mitcad#95).
mod ahead;
use ahead::Adding;
pub use ahead::{
    Helper, ImportKernel, Spawner, WORKER_STACK, WorkerJob, in_order, running_workers,
};

// The geometric check of an item's change (mitcad#138).
mod geometric;

fn item_name(item: &TimelineItem) -> String {
    item.name().unwrap_or("?").to_owned()
}

/// Replaces the strings `"$k"` in a definition by the uid of the k-th
/// feature added before it ([`Importer::add_all`]), also as the prefix of
/// a face or body name (`"$0:hole0.wall"`, `"$1.b0"`).
fn substitute(def: &mut Value, uids: &[FeatureUid]) {
    match def {
        Value::String(s) => {
            if let Some(rest) = s.strip_prefix('$') {
                let digits = rest
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(rest.len());
                let (k, tail) = rest.split_at(digits);
                let fits = tail.is_empty() || tail.starts_with([':', '.']);
                if fits && let Some(uid) = k.parse::<usize>().ok().and_then(|k| uids.get(k)) {
                    *s = format!("{uid}{tail}");
                }
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

/// How many more definitions are tried for an extrusion whose result
/// matched its state only approximately (see [`Importer::verified_at`]).
const LOOSE_RETRIES: usize = 8;

/// Whether a candidate's feature (its last definition) is an extrusion.
fn is_extrusion(c: &Candidate) -> bool {
    c.defs.last().is_some_and(|d| d["type"] == "extrude")
}

/// Whether a candidate whose result matched its state only approximately
/// looks for a closer definition: an extrusion (a set of regions next to
/// the right one), a fillet or chamfer (which face takes which distance
/// or offset is not decoded, and the history's edge guesses differ: the
/// wrong reading can come within the tolerance too).
fn seeks_closer(c: &Candidate) -> bool {
    is_extrusion(c) || is_dressup(c)
}

/// How long a thread on faces sized by offsets may take (`ops.rs`,
/// threads), s: offsetting a face of a large or stored body can take long
/// or not build, where the tube tried next takes a second (mitcad#68).
const SIZING_OFFSET_SECONDS: f64 = 2.0;

/// The seconds a candidate is given of its item's time, when fewer.
fn candidate_seconds(c: &Candidate) -> Option<f64> {
    let sizing = c.defs.last().is_some_and(|d| d["type"] == "thread")
        && c.defs.iter().any(|d| d["type"] == "offset_face");
    sizing.then_some(SIZING_OFFSET_SECONDS)
}

/// The part of an item's time a definition is given in the first round
/// ([`Turns`], mitcad#78). Definitions that give a state take a second or
/// two (up to about 15 s on bodies of thousands of faces); one still
/// running after a third of the item's time is mostly a wrong guess whose
/// boolean is slow (a minute or more), which would keep the definitions
/// ranked after it from being tried. Each one cut short at its share and
/// tried again costs that share, so it is not smaller.
const SHARE: f64 = 1.0 / 3.0;

/// The order in which an item's definitions are tried against the
/// history, so that one slow definition does not use up the item's time
/// (mitcad#78): in the first round each is given a share of the time
/// ([`SHARE`]; the last the rest), and one cut short at its share gives
/// way to those after it (the same definition limited to participants
/// waits behind it untried). Those that gave way are tried again, by rank,
/// with the rest of the item's time, each only while more time is left
/// than it ran (it cannot finish in less). A definition that would have
/// given the state within the item's time still does when those after it
/// are quick.
#[derive(Debug, Default)]
struct Turns {
    /// Definitions that gave way, by rank, with the seconds they ran.
    waiting: std::collections::VecDeque<(usize, f64)>,
    /// The definitions cut short (at their share or at the end of the
    /// item's time), with the seconds they ran.
    cut: Vec<(usize, f64)>,
}

impl Turns {
    /// The seconds the next definition is given: a share of `budget` in
    /// the first round, unless it is the last one (`last`) and none waits;
    /// None in the second round (`again`).
    fn share(&self, budget: f64, last: bool, again: bool) -> Option<f64> {
        let rest = again || (last && self.waiting.is_empty());
        (!rest).then_some(SHARE * budget)
    }

    /// Definition `k` was cut short after `seconds`; it waits for a second
    /// round when it gave way at its share.
    fn cut_short(&mut self, k: usize, seconds: f64, gave_way: bool) {
        if gave_way {
            self.waiting.push_back((k, seconds));
        }
        self.cut.push((k, seconds));
    }

    /// Whether definition `k` of `candidates` waits without being tried:
    /// the same definition without participants gave way (the boolean on
    /// fewer bodies takes as long), or a join or intersection of the same
    /// bodies with another of them as the target (the same boolean), so it
    /// waits behind that one, as if it had run as long.
    fn waits_behind(&mut self, candidates: &[Candidate], k: usize) -> bool {
        let same: Box<dyn Fn(&Candidate) -> bool> =
            if let Some((free, _)) = participants_of(&candidates[k]) {
                Box::new(move |c| matches!(c.defs.as_slice(), [d] if *d == free))
            } else if let Some(bodies) = symmetric_combine(&candidates[k]) {
                Box::new(move |c| symmetric_combine(c).as_ref() == Some(&bodies))
            } else {
                return false;
            };
        let ran = self
            .waiting
            .iter()
            .find(|(j, _)| same(&candidates[*j]))
            .map(|(_, ran)| *ran);
        if let Some(ran) = ran {
            self.waiting.push_back((k, ran));
        }
        ran.is_some()
    }

    /// The next definition to try again with `remaining` seconds: those
    /// that ran as long already are passed over.
    fn again(&mut self, remaining: f64) -> Option<usize> {
        while let Some((k, ran)) = self.waiting.pop_front() {
            if remaining > ran {
                return Some(k);
            }
        }
        None
    }

    /// What an item that gave no state says about the definitions cut
    /// short: the one that ran longest, of `candidates` (with its seconds).
    fn note(&self, candidates: &[Candidate]) -> Option<(f64, String)> {
        let (k, seconds) = self
            .cut
            .iter()
            .copied()
            .max_by(|a, b| a.1.total_cmp(&b.1))?;
        let others = match self.cut.iter().filter(|(j, _)| *j != k).count() {
            0 => String::new(),
            1 => " (and 1 other)".to_owned(),
            n => format!(" (and {n} others)"),
        };
        let note = format!(
            "cut short after {seconds:.1} s: definition {} of {} ({}){others}",
            k + 1,
            candidates.len(),
            brief(&candidates[k])
        );
        Some((seconds, note))
    }
}

/// A candidate's feature in a few words, for the report: its type,
/// operation and how many profiles.
fn brief(c: &Candidate) -> String {
    let Some(def) = c.defs.last() else {
        return String::new();
    };
    let mut words = vec![def["type"].as_str().unwrap_or("?").to_owned()];
    if let Some(operation) = def["operation"].as_str() {
        words.push(operation.to_owned());
    }
    if let Some(profiles) = def["profiles"].as_array() {
        let n = profiles.len();
        words.push(format!("{n} profile{}", if n == 1 { "" } else { "s" }));
    }
    words.join(", ")
}

/// Whether a candidate's feature (its last definition) is a fillet or
/// chamfer.
fn is_dressup(c: &Candidate) -> bool {
    c.defs
        .last()
        .is_some_and(|d| d["type"] == "fillet" || d["type"] == "chamfer")
}

/// Whether a result `d` from its state is closer than an approximate match
/// `loose` before it, so that `c` takes its place: for a fillet or chamfer
/// by a quarter at least. The right edges' rounding can differ from a
/// stored one (a spline) by about as much as another edge set's, which then
/// comes a little closer by chance (4.0e-4 against 4.4e-4) and whose edges
/// keep later items from building; a closer definition that is right comes
/// closer by half or more (the other reading of a chamfer, the edges a
/// rounding really took).
fn closer(c: &Candidate, d: f64, loose: f64) -> bool {
    if is_dressup(c) {
        d < 0.75 * loose
    } else {
        d < loose
    }
}

/// Whether an approximate match of a candidate is suspect: an extrusion
/// (a set of regions next to the right one can come within the
/// tolerance), or copies the history guessed (copies of another feature,
/// or about another plane, than the file's). Other approximations are the
/// file's (fillets stored as splines) and stay.
fn suspect(c: &Candidate) -> bool {
    is_extrusion(c)
        || (c.guess
            && c.defs.last().is_some_and(|d| {
                matches!(
                    d["type"].as_str(),
                    Some("circular_pattern" | "rectangular_pattern" | "mirror")
                )
            }))
}

/// Whether a result `d` from its state (relative, [`Sig::distance`]) is an
/// approximation the item made itself, not one the bodies before it
/// carried (`carried`, from the state before it): an item on bodies that
/// differ from the file's by `carried` comes about as close and no closer.
fn introduced(d: f64, carried: f64) -> bool {
    d > history::EXACT && d > 1.5 * carried
}

/// Why a result (the bodies `after`) that came within the tolerance of the
/// file's state is not taken: the volume the item adds or removes itself is
/// not the file's change ([`history::Change`]).
fn changes_otherwise(own: &history::Change, after: &[Sig]) -> String {
    let (ours, theirs) = own.volumes(after);
    format!(
        "its result comes close to the file's, but the volume it adds or removes is {:.1} % off \
         the file's ({ours:.4} mm³ against {theirs:.4})",
        own.difference(after) * 100.0
    )
}

/// Whether a candidate's feature cannot turn the bodies `before` into
/// `after` (its result, or a history state it matched): a join (or new
/// body) that leaves less solid volume on as many bodies, or a cut that
/// leaves more. For an item without a state of its own, which may match
/// any of the next states within the tolerance, a join came that close
/// to a later fillet's state by chance; the geometry kernel's booleans
/// also give such results on some inputs.
fn implausible(c: &Candidate, before: &[Sig], after: &[Sig]) -> bool {
    if before.len() != after.len() || before.is_empty() {
        return false;
    }
    let volume = |sigs: &[Sig]| sigs.iter().map(|s| s.volume).sum::<f64>();
    let (b, a) = (volume(before), volume(after));
    let slack = 1e-7 * b.abs().max(a.abs());
    match c.defs.last().and_then(|d| d["operation"].as_str()) {
        Some("join" | "new_body") => a < b - slack,
        Some("cut") => a > b + slack,
        _ => false,
    }
}

/// How many other definitions of an extrusion are kept for a later
/// pattern or mirror of it ([`Alternatives`]).
const ALTERNATIVES: usize = 4;

/// The most extrusions given another definition for one pattern or mirror
/// ([`Importer::with_other_profiles`]).
const ALTERNATIVE_EDITS: usize = 8;

/// Definitions of an extrusion ranked after the one that gave its state
/// exactly, not tried: other sets of regions can give the same bodies
/// (regions in material a cut already removed, or a join adds to), but
/// not the same copies, so a later pattern or mirror of the extrusion
/// that gives no state tries them ([`Importer::with_other_profiles`]).
#[derive(Debug, Clone)]
struct Alternatives {
    /// The extrusion's entry in the report.
    report: usize,
    /// Single definitions, with their notes.
    defs: Vec<(Value, Option<String>)>,
}

/// The extrusion definitions after `accepted` in `later` that differ from
/// it and from each other by more than their participants, without them
/// (a copy of the definition limited to some bodies acts on those only:
/// copies of one without them act on the bodies the original changed),
/// at most [`ALTERNATIVES`].
fn other_definitions(accepted: &Candidate, later: &[Candidate]) -> Vec<(Value, Option<String>)> {
    let free = |d: &Value| {
        let mut d = d.clone();
        if let Some(m) = d.as_object_mut() {
            m.remove("participants");
        }
        d
    };
    let [def] = accepted.defs.as_slice() else {
        return Vec::new();
    };
    if def["type"] != "extrude" {
        return Vec::new();
    }
    let mut seen = vec![free(def)];
    let mut out = Vec::new();
    for c in later {
        let [d] = c.defs.as_slice() else {
            continue;
        };
        if d["type"] != "extrude" || seen.contains(&free(d)) {
            continue;
        }
        seen.push(free(d));
        out.push((free(d), c.note.clone()));
        if out.len() >= ALTERNATIVES {
            break;
        }
    }
    out
}

/// The features a pattern or mirror candidate copies (`objects` of type
/// `features`).
fn copied_features(c: &Candidate) -> Vec<FeatureUid> {
    let Some(def) = c.defs.last() else {
        return Vec::new();
    };
    if def["objects"]["type"] != "features" {
        return Vec::new();
    }
    def["objects"]["features"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f.as_str()?.parse().ok())
        .collect()
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

/// A combine whose boolean does not depend on which body is the target (a
/// join or an intersection of bodies of its own component): its operation
/// and bodies.
fn symmetric_combine(c: &Candidate) -> Option<(String, BTreeSet<String>)> {
    let [def] = c.defs.as_slice() else {
        return None;
    };
    let operation = def["operation"].as_str()?;
    if def["type"] != "combine"
        || !matches!(operation, "join" | "intersect")
        || def.get("tool_links").is_some()
    {
        return None;
    }
    let mut bodies: BTreeSet<String> = def["tools"]
        .as_array()?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    bodies.insert(def["target"].as_str()?.to_owned());
    Some((operation.to_owned(), bodies))
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
    /// Changes no body in its component (occurrences, relationships, ...).
    NoGeometry,
    /// A joint, an as-built joint, a joint origin or a ground item
    /// (`joints.rs`).
    Assembly,
    /// A modelling feature (or an unknown item, which may be one).
    Geometry,
}

fn classify(object_type: Option<&str>) -> Class {
    match object_type {
        Some("Sketch") => Class::Sketch,
        Some("ConstructionPlane" | "ConstructionAxis" | "ConstructionPoint") => Class::Datum,
        Some(
            "Joint" | "AsBuiltJoint" | "JointOrigin" | "GroundOccurrence" | "Snapshot"
            | "RigidGroup",
        ) => Class::Assembly,
        Some(t) if no_geometry(t).is_some() => Class::NoGeometry,
        _ => Class::Geometry,
    }
}

/// Why an item of this type changes no body in its component, for the
/// report; `None` for the types that may.
fn no_geometry(object_type: &str) -> Option<&'static str> {
    Some(match object_type {
        "MotionLink" | "ContactSet" | "GeometricRelationship" | "AssemblyRelationship" => {
            "no geometry: an assembly relationship (occurrences are placed by the occurrence tree)"
        }
        "Occurrence"
        | "ComponentInsert"
        | "Fastener"
        | "CopyPasteOccurrence"
        | "DerivedInstance"
        | "RectangularOccurrencePattern"
        | "CircularOccurrencePattern" => {
            "no geometry: places occurrences of a component (they come from the occurrence tree, \
             the component's bodies with the component)"
        }
        "Context" => "no geometry: the assembly context of a component of another design",
        "Canvas" | "Decal" => "no geometry: a display item",
        "Group" => "no geometry: a timeline group (its items follow it)",
        "MeshFeature" => "no solid geometry: a mesh body, which the import leaves out",
        _ => return None,
    })
}

impl<'a, K: ImportKernel> Importer<'a, K> {
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
        self.captures = captures::read(self.dump, &self.components, self.doc.assembly());
        let captures = &self.captures;
        components::assign_items(
            self.dump,
            self.doc.assembly(),
            &|o, index| captures.placement_at(o.uid, index).unwrap_or(o.transform),
            &mut self.oracle,
            &mut self.components,
            &mut self.report.components,
        );
        // The occurrences start where the file places them at the end of
        // its timeline: their last captured positions (mitcad#75).
        for (occurrence, placement) in self.captures.last() {
            if let Err(e) = self
                .doc
                .set_occurrence_transform(occurrence, placement, false)
            {
                let name = self.doc.assembly().occurrence_name(occurrence);
                self.report
                    .warnings
                    .push(format!("{name}: its captured position: {e}"));
            }
        }
        self.report.history.states = self.oracle.count();
        if self.oracle.enabled {
            // The replay starts with no bodies: the first state without
            // solids, if the history starts that way.
            self.oracle.cursor = self.oracle.find(self.doc.kernel(), 0, &[]);
        }
        let dump = self.dump;
        for (position, item) in dump.timeline_items().iter().enumerate() {
            // A try the watchdog gave up does nothing more (mitcad#71).
            if self.abandoned() {
                trace!("given up by the watchdog before item {}", position + 1);
                return;
            }
            self.item(position, item);
        }
        if self.abandoned() {
            trace!("given up by the watchdog before the final bodies");
            return;
        }
        self.finish();
    }

    /// Whether the watchdog gave this try up ([`Progress::abandon`]).
    fn abandoned(&self) -> bool {
        self.options
            .progress
            .as_ref()
            .is_some_and(|p| p.abandoned())
    }

    /// Whether the process is low on memory ([`Progress::low_memory`]): no
    /// definitions are tried until it has recovered.
    fn memory_low(&self) -> bool {
        self.options
            .progress
            .as_ref()
            .is_some_and(|p| p.is_memory_low())
    }

    /// How many times the process ran low on memory.
    fn memory_lows(&self) -> u64 {
        self.options
            .progress
            .as_ref()
            .map_or(0, |p| p.memory_lows())
    }

    /// Drops the cached results of the definitions tried and the bodies of
    /// the history states behind the replay (mitcad#80): when memory gets
    /// tight ([`Progress::tight_on_memory`]), and after each time it ran
    /// low ([`Progress::low_memory`]; from the first on, the cached results
    /// take at most [`CACHE_AFTER_LOW_MEMORY`]).
    fn release_memory(&mut self) {
        let Some(p) = self.options.progress.clone() else {
            return;
        };
        // Once each time it ran low (also when it has recovered since).
        let lows = p.memory_lows();
        let low = lows != self.memory_released;
        if !p.take_tight() && !low {
            return;
        }
        if low {
            if self.memory_released == 0 {
                self.doc.set_memory_budget(Some(CACHE_AFTER_LOW_MEMORY));
            }
            self.memory_released = lows;
        }
        let (results, bytes) = self.doc.clear_memory_cache();
        let states = self.oracle.release_behind();
        trace!(
            "memory {} ({}): {results} cached results ({} MiB) and {states} history states dropped",
            if low { "low" } else { "tight" },
            p.memory(),
            bytes >> 20
        );
        if low {
            // The definition cut short has freed its memory, and so has
            // this; the memory guard measures the process every so often
            // (`mitcad-ffi`, 100 ms): the next items wait a little for it to
            // see the memory recover rather than take the file's bodies at
            // once (all the items after a combine cut short came in so,
            // within the same millisecond).
            let until = std::time::Instant::now() + MEMORY_SETTLE;
            while p.is_memory_low() && !p.abandoned() && std::time::Instant::now() < until {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            trace!(
                "memory {} after the wait ({})",
                if p.is_memory_low() {
                    "still low"
                } else {
                    "recovered"
                },
                p.memory()
            );
        }
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
            change_difference: None,
            geometric_check: None,
            note: None,
            component: None,
        };
        self.report.items.push(report);
        let at = self.report.items.len() - 1;
        // (The application's progress dialog shows it.)
        trace!("item {}: {name}", position + 1);
        self.release_memory();
        if let Some(memory) = self.options.progress.as_ref().map(|p| p.memory())
            && !memory.is_empty()
        {
            trace!(
                "memory: {memory}; cached results {} MiB, history states kept {}",
                self.doc.memory_cache_bytes() >> 20,
                self.oracle.kept()
            );
        }
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
        let lows = self.memory_lows();
        // The item's time; the parts of it that are given their own set
        // theirs (`Importer::within`).
        self.deadline = Some(started + seconds(self.options.item_seconds));
        let class = classify(object_type.as_deref());
        // A modelling item the file keeps no result for (mitcad#96): its
        // result number is -1 and a state flag is set (suppressed or
        // failed in the file). It changed nothing there, and no state
        // checks a definition of it. So with a state the history names but
        // does not hold (an `.ipt` part's suppressed feature).
        let flagged = item.f3d.as_ref().is_some_and(|f3d| {
            f3d.result_no == Some(-1)
                && f3d
                    .flags
                    .as_deref()
                    .is_some_and(|f| f.chars().any(|c| c != '0'))
        });
        if class == Class::Geometry && (flagged || self.oracle.item_without_result(index)) {
            self.note(at, Outcome::Skipped, NO_RESULT);
            self.deadline = None;
            return;
        }
        if class != Class::Geometry && self.options.hung_items.contains(&index) {
            self.note(at, Outcome::Skipped, HUNG);
            return;
        }
        match class {
            Class::Sketch => self.sketch_item(at, index, item),
            Class::Datum => self.datum_item(at, index, item),
            Class::Assembly => self.assembly_item(at, index, item),
            Class::NoGeometry => self.note(
                at,
                Outcome::Skipped,
                object_type
                    .as_deref()
                    .and_then(no_geometry)
                    .unwrap_or("no geometry"),
            ),
            Class::Geometry => {
                let originals = self.sketches_into_component(at, index, item);
                self.geometry_item(at, index, item);
                self.sketches.extend(originals);
            }
        }
        self.deadline = None;
        // A modelling item that took the file's bodies while the process
        // ran low on memory says so, also when it gave up for another
        // reason after (mitcad#80).
        let ran_low = self.memory_lows() != lows;
        let report = &mut self.report.items[at];
        if class == Class::Geometry
            && ran_low
            && matches!(report.outcome, Outcome::Fallback | Outcome::Skipped)
            && !report
                .note
                .as_deref()
                .is_some_and(|n| n.contains(LOW_MEMORY))
        {
            report.note = Some(match report.note.take() {
                Some(n) if !n.is_empty() => format!("{n}; {LOW_MEMORY}"),
                _ => LOW_MEMORY.to_owned(),
            });
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
        let options = self.options;
        let adding = Adding {
            component: self.component,
            monitor: self.try_monitor(),
            progress: options.progress.as_deref(),
            memory: true,
        };
        add(self.doc, def, name, &adding)
    }

    /// Adds a feature from its JSON definition to `target` (see
    /// [`Importer::add`]): a joint names geometry of other components than
    /// its own.
    pub(crate) fn add_in(
        &mut self,
        def: &Value,
        name: Option<&str>,
        target: ComponentUid,
    ) -> Result<FeatureUid, String> {
        let options = self.options;
        let adding = Adding {
            component: self.component,
            monitor: self.try_monitor(),
            progress: options.progress.as_deref(),
            memory: true,
        };
        add_in(self.doc, def, name, target, &adding)
    }

    /// The component a definition goes into ([`def_component`]).
    fn def_component(&self, def: &Value) -> ComponentUid {
        def_component(self.doc, def, self.component)
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
        let options = self.options;
        let adding = Adding {
            component: self.component,
            monitor: self.try_monitor(),
            progress: options.progress.as_deref(),
            memory: true,
        };
        add_all(self.doc, candidate, name, &adding)
    }

    /// What cancels a modelling item's definition while it is tried: a
    /// stop request, the watchdog giving the try up
    /// ([`Progress::abandon`]), the process running low on memory
    /// ([`Progress::low_memory`], mitcad#80).
    fn try_parents(&self) -> Vec<Arc<RecomputeMonitor>> {
        let mut parents = Vec::new();
        if let Some(m) = self.options.stop.as_ref().filter(|m| !m.is_cancelled()) {
            parents.push(Arc::clone(m));
        }
        if let Some(p) = &self.options.progress {
            parents.push(Arc::clone(&p.abandoned));
            parents.extend(p.memory_monitor());
        }
        parents
    }

    /// The monitor the document evaluates under while a modelling item's
    /// definitions are tried ([`Importer::interruptible`]); None otherwise
    /// (the document's own).
    fn try_monitor(&self) -> Option<Arc<RecomputeMonitor>> {
        if !self.trying {
            return None;
        }
        let parents = self.try_parents();
        if parents.is_empty() && self.deadline.is_none() {
            return None;
        }
        Some(Arc::new(RecomputeMonitor::within(parents, self.deadline)))
    }

    /// Runs `f` on the document with a monitor while a modelling item's
    /// definitions are tried, so that the evaluation stops (P7e: inside
    /// the kernel's long operations too) on a stop request, when the
    /// watchdog gives the try up ([`Progress::abandon`]), when the process
    /// runs low on memory ([`Progress::low_memory`], mitcad#80), or when the
    /// item's time is over ([`Importer::deadline`], mitcad#69): the
    /// command then fails and changes nothing. Undo never runs so: a
    /// cancelled monitor would refuse it.
    fn interruptible<T>(&mut self, f: impl FnOnce(&mut Document<K>) -> T) -> T {
        interruptible(self.doc, self.try_monitor().as_ref(), f)
    }

    /// Runs `f` with the item's time ([`Importer::deadline`]) set to
    /// `budget` seconds from now, for a part of an item that has a time of
    /// its own.
    fn within<T>(&mut self, budget: f64, f: impl FnOnce(&mut Self) -> T) -> T {
        let outer = self
            .deadline
            .replace(std::time::Instant::now() + seconds(budget));
        let result = f(self);
        self.deadline = outer;
        result
    }

    /// Whether the item's time is over ([`Importer::deadline`]).
    fn past_deadline(&self) -> bool {
        self.deadline
            .is_some_and(|d| std::time::Instant::now() >= d)
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
        self.try_candidate_for(index, candidate, name, None)
    }

    /// [`Importer::try_candidate`] for at most `share` seconds of the
    /// item's time ([`Turns`]): cut short then, it fails with [`GAVE_WAY`].
    fn try_candidate_for(
        &mut self,
        index: i64,
        candidate: &Candidate,
        name: Option<&str>,
        share: Option<f64>,
    ) -> Result<Vec<FeatureUid>, String> {
        if self.abandoned() || self.stopped(index) {
            return Err(STOPPED.to_owned());
        }
        if self.memory_low() {
            return Err(LOW_MEMORY.to_owned());
        }
        let lows = self.memory_lows();
        self.trying = true;
        let clock = std::time::Instant::now();
        // A candidate given fewer seconds than its item has left: its own
        // (a sizing offset's), or its share.
        let item_deadline = self.deadline;
        let own = candidate_seconds(candidate).map(|s| clock + seconds(s));
        let shared = share.map(|s| clock + seconds(s));
        self.deadline = [item_deadline, own, shared].into_iter().flatten().min();
        let added = self.add_all(candidate, name);
        self.deadline = item_deadline;
        self.trying = false;
        trace!(
            "{}: evaluated in {:.2} s",
            name.unwrap_or("?"),
            clock.elapsed().as_secs_f64()
        );
        let past =
            |d: Option<std::time::Instant>| d.is_some_and(|d| std::time::Instant::now() >= d);
        match added {
            Err(_) if self.abandoned() || self.stopped(index) => Err(STOPPED.to_owned()),
            // Cut short (or failed) as the memory ran low: also when it has
            // recovered since, as the definition's own memory was freed.
            Err(_) if self.memory_low() || self.memory_lows() != lows => Err(LOW_MEMORY.to_owned()),
            Err(e) if e == mitcad_model::Cancelled::MESSAGE && self.past_deadline() => {
                Err(OUT_OF_TIME.to_owned())
            }
            Err(e) if e == mitcad_model::Cancelled::MESSAGE && past(own) => {
                Err(CANDIDATE_OUT_OF_TIME.to_owned())
            }
            Err(e) if e == mitcad_model::Cancelled::MESSAGE && past(shared) => {
                Err(GAVE_WAY.to_owned())
            }
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

    /// The report's warning when the process ran low on memory (mitcad#80).
    fn report_low_memory(&mut self) {
        let Some((memory, at)) = self
            .options
            .progress
            .as_ref()
            .and_then(|p| p.first_low_memory())
        else {
            return;
        };
        let item = (at >= 0).then_some(at);
        let name = item
            .and_then(|i| self.report.items.iter().find(|r| r.index == i))
            .map(|r| r.name.clone());
        let given_up = self
            .report
            .items
            .iter()
            .filter(|i| i.note.as_deref().is_some_and(|n| n.contains(LOW_MEMORY)))
            .count();
        let place = match (&name, at) {
            (Some(name), _) => format!("at {name}"),
            (None, Progress::STARTING) => "before its first item".to_owned(),
            (None, _) => "after its last item".to_owned(),
        };
        let mut warning = format!(
            "the import ran low on memory {place} ({memory}): definitions were cut short or not \
             tried and {given_up} modelling items took the file's bodies"
        );
        if self.memory_low() {
            warning.push_str("; the final bodies were compared by volume only");
        }
        self.report.warnings.push(warning);
        self.report.low_memory = Some(LowMemoryReport {
            item,
            name,
            memory,
            items: given_up,
        });
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
        self.current_bodies()
            .into_iter()
            .map(|(_, sig)| sig)
            .collect()
    }

    fn current_bodies(&self) -> Vec<(BodyUid, Sig)> {
        bodies_of(self.doc)
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
            // Moved by the placements at this item (mitcad#86): a copy made
            // for an item before a captured position does not do after it.
            let moved = self.doc.feature(info.uid).and_then(|f| {
                let from = self.placement_at(f.component, index)?;
                Some(inverse(&self.placement_at(self.component, index)?).after(&from))
            });
            let Some(moved) = moved else {
                continue;
            };
            let made = self
                .sketch_copies
                .get(&(i, self.component))
                .filter(|(t, _)| same_placement(t, &moved))
                .map(|(_, copy)| copy.clone());
            let copy = made.or_else(|| self.copy_sketch(i, &info, &moved));
            if let Some(copy) = copy {
                self.report.items[at].features.push(copy.uid.to_string());
                self.sketches.insert(i, copy);
                originals.push((i, info));
            }
        }
        originals
    }

    /// Imports sketch `i` again into the current component, moved from the
    /// coordinates of its own component into this one's by `moved` (the
    /// file's assembly context: a feature of one component may use
    /// another's sketch); None when it does not import or lands elsewhere.
    fn copy_sketch(
        &mut self,
        i: i64,
        original: &SketchInfo,
        moved: &Transform,
    ) -> Option<SketchInfo> {
        let dump = self.dump;
        let item = dump
            .timeline_items()
            .iter()
            .enumerate()
            .find(|(p, it)| it.index.unwrap_or(*p as i64) == i)?
            .1;
        let depth = self.doc.undo_depth();
        let result = self.import_sketch(i, item, Some(moved));
        let copy = self.sketches.insert(i, original.clone());
        match (result, copy) {
            (Ok(_), Some(copy))
                if self.doc.feature(copy.uid).map(|f| f.component) == Some(self.component) =>
            {
                trace!(
                    "sketch {i} imported again into {}, moved by {:?}",
                    self.component, moved.translation
                );
                self.sketch_copies
                    .insert((i, self.component), (*moved, copy.clone()));
                Some(copy)
            }
            _ => {
                self.undo_to(depth);
                None
            }
        }
    }

    /// Where a component is placed in the design at the item `index` of
    /// the timeline, when it is placed once (the root: as it is): captured
    /// occurrences where the file has them there, not where they start
    /// (`captures.rs`, mitcad#86).
    pub(crate) fn placement_at(&self, component: ComponentUid, index: i64) -> Option<Transform> {
        components::placement(self.doc.assembly(), component, |o| {
            self.captures
                .placement_at(o.uid, index)
                .unwrap_or(o.transform)
        })
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
                // A mid plane, a plane at an angle or one through points
                // and lines (tried in several ways) lies where the dump has
                // it.
                let place = match &item.detail {
                    Some(Detail::ConstructionPlane(d)) => d
                        .geometry
                        .as_ref()
                        .and_then(|g| Some((geom::mm3(g.origin?), geom::unit(g.normal?)?))),
                    _ => None,
                };
                for c in candidates {
                    let depth = self.doc.undo_depth();
                    match self.add_all(&c, item.name()) {
                        Ok(uids)
                            if place.is_some_and(|(o, n)| {
                                matches!(
                                    c.defs[0]["definition"]["type"].as_str(),
                                    Some(
                                        "midplane"
                                            | "angle"
                                            | "three_points"
                                            | "two_edges"
                                            | "edge_and_point"
                                            | "normal_at_point"
                                    )
                                ) && uids
                                    .last()
                                    .and_then(|u| self.doc.datum(*u))
                                    .is_some_and(|d| match d {
                                        mitcad_model::datum::Datum::Plane(p) => {
                                            geom::norm(geom::cross(p.normal(), n)) > 1e-6
                                                || p.distance_to(o).abs() > 1e-3
                                        }
                                        _ => false,
                                    })
                            }) =>
                        {
                            self.undo_to(depth);
                            last = "it does not lie where the file has it".to_owned();
                        }
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
            if let Some(reason) = self.halted(index) {
                self.note(
                    at,
                    Outcome::Skipped,
                    format!("{reason}; the stored bodies stand in for it"),
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
            self.retire_consumed_tools(index, item);
            self.verified(at, index, item);
            return;
        }
        if self.options.hung_items.contains(&index) {
            self.failed(at, HUNG.to_owned());
            return;
        }
        if let Some(reason) = self.halted(index) {
            self.failed(at, reason.to_owned());
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

    /// Why no (more) definitions are tried for the modelling item `index`
    /// whatever its state: the import was stopped, or ran low on memory.
    fn halted(&self, index: i64) -> Option<&'static str> {
        if self.stopped(index) {
            Some(STOPPED)
        } else if self.memory_low() {
            Some(LOW_MEMORY)
        } else {
            None
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
                    let halt = halts(&e);
                    last = e;
                    if halt {
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
        // All of a fillet's features (one per tangent chain), for patterns
        // and mirrors of it (mitcad#105).
        let dressups: Vec<FeatureUid> = uids
            .iter()
            .copied()
            .filter(|u| {
                matches!(
                    self.doc.feature(*u).map(|f| &f.def),
                    Some(FeatureDef::Fillet(_) | FeatureDef::Chamfer(_))
                )
            })
            .collect();
        if !dressups.is_empty() {
            self.dressups.insert(index, dressups);
        }
        self.learn_accepted(index, c, verified);
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

    /// How far the replay's bodies are from history state `q`'s
    /// ([`history::bodies_distance`]; 0 when they do not pair within the
    /// tolerance or the state does not rebuild).
    fn carried(&mut self, q: usize) -> f64 {
        let kernel = self.doc.kernel();
        let state = self.oracle.sigs(kernel, q);
        history::bodies_distance(&state, &self.current_sigs()).map_or(0.0, |d| d.0)
    }

    /// The warning for a base feature that brought the bodies to the state
    /// before item `name`.
    fn exact_before(&mut self, uid: FeatureUid, name: &str) {
        let base = self.doc.feature(uid).map(|f| f.name.clone());
        self.report.warnings.push(format!(
            "{} brings the bodies to the file's state before {name}",
            base.unwrap_or_default()
        ));
    }

    /// After candidate `c` of the item gave history state `state` within
    /// `d` (the bodies before it `carried` from theirs; `later`: the
    /// candidates ranked after it, not tried): an extrusion that gave it
    /// exactly keeps its other definitions for a later pattern or mirror
    /// ([`Alternatives`]); a suspect approximation it made itself
    /// ([`suspect`]) does not reach later items: a base feature brings
    /// the bodies to the state exactly (the item stays, checked within
    /// the tolerance). Left alone, the difference grows in later items or
    /// keeps a later state (a split, a mirror, the stored design) from
    /// matching.
    #[allow(clippy::too_many_arguments)]
    fn after_match(
        &mut self,
        at: usize,
        index: i64,
        item: &TimelineItem,
        c: &Candidate,
        later: &[Candidate],
        state: usize,
        d: f64,
        carried: f64,
    ) {
        if d <= history::EXACT {
            let defs = other_definitions(c, later);
            if let (false, Some(uid)) = (defs.is_empty(), self.features.get(&index)) {
                trace!("{}: {} other definitions kept", item_name(item), defs.len());
                self.alternatives
                    .insert(*uid, Alternatives { report: at, defs });
            }
            return;
        }
        if !suspect(c) || !introduced(d, carried) {
            return;
        }
        let name = item_name(item);
        if tracing() {
            let kernel = self.doc.kernel();
            let target = self.oracle.sigs(kernel, state);
            trace!(
                "{name}: {:?} for state {state}: {:?}",
                short(&self.current_sigs()),
                short(&target)
            );
        }
        let depth = self.doc.undo_depth();
        match self.fallback_to(state, None) {
            Ok(Some(uid)) => {
                let base = self.doc.feature(uid).map(|f| f.name.clone());
                trace!("{name}: {uid} brings the bodies to state {state} exactly");
                self.report.warnings.push(format!(
                    "{} brings the bodies to the file's state after {name}, whose result \
                     differs from it by {:.1e}",
                    base.unwrap_or_default(),
                    d
                ));
            }
            Ok(None) => {}
            Err(e) => {
                trace!("{name}: its state did not come in exactly: {e}");
                self.undo_to(depth);
            }
        }
    }

    /// A pattern or mirror of features that gave no state (`candidates`
    /// tried against `target`, the item's state `state`): the extrusions
    /// its candidates copy are given their other definitions
    /// ([`Alternatives`]), each one kept only when the bodies stay as they
    /// are, and the candidates that copy it are tried again. True when one
    /// gave the state; the extrusion then keeps that definition.
    /// (`own`: the file's change for the item, [`history::Change`];
    /// `states`: the state before it and `state`, for its faces.)
    #[allow(clippy::too_many_arguments)]
    fn with_other_profiles(
        &mut self,
        at: usize,
        index: i64,
        item: &TimelineItem,
        candidates: &[Candidate],
        target: &[Sig],
        own: Option<history::Change>,
        states: geometric::States,
    ) -> bool {
        let budget = self.options.item_seconds;
        self.within(budget, |s| {
            s.with_other_profiles_in(at, index, item, candidates, target, own, states)
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn with_other_profiles_in(
        &mut self,
        at: usize,
        index: i64,
        item: &TimelineItem,
        candidates: &[Candidate],
        target: &[Sig],
        own: Option<history::Change>,
        states: geometric::States,
    ) -> bool {
        let state = states.after;
        let mut copied: Vec<FeatureUid> = Vec::new();
        for c in candidates {
            for f in copied_features(c) {
                if self.alternatives.contains_key(&f) && !copied.contains(&f) {
                    copied.push(f);
                }
            }
        }
        let name = item_name(item);
        trace!(
            "{name}: other definitions of {copied:?} for {} candidates",
            candidates.len()
        );
        if copied.is_empty() {
            return false;
        }
        let started = self.clock();
        let budget = self.options.item_seconds;
        let before = self.current_sigs();
        let mut edits = 0;
        for f in copied {
            let Some(alternatives) = self.alternatives.get(&f).cloned() else {
                continue;
            };
            for (def, note) in &alternatives.defs {
                if edits >= ALTERNATIVE_EDITS
                    || started.elapsed().as_secs_f64() > budget
                    || self.give_up(index).is_some()
                {
                    return false;
                }
                edits += 1;
                let depth = self.doc.undo_depth();
                let Ok(parsed) = serde_json::from_value::<FeatureDef<ValueInput>>(def.clone())
                else {
                    continue;
                };
                self.tick();
                self.trying = true;
                let edited = self.interruptible(|doc| doc.edit_feature(f, &parsed));
                self.trying = false;
                // The bodies must stay as they are: the extrusion gives its
                // state, and the features after it theirs.
                let unchanged = edited.is_ok()
                    && self.doc.status(f).and_then(|s| s.error()).is_none()
                    && history::bodies_distance(&before, &self.current_sigs())
                        .is_some_and(|(_, exact)| exact);
                if !unchanged {
                    trace!(
                        "{name}: {f} as {} changes the bodies",
                        summary(&Candidate::new(def.clone()))
                    );
                    self.undo_to(depth);
                    continue;
                }
                let shapes = self.body_shapes();
                for c in candidates
                    .iter()
                    .filter(|c| copied_features(c).contains(&f))
                {
                    let inner = self.doc.undo_depth();
                    let uids = match self.try_candidate(index, c, item.name()) {
                        Ok(uids) => uids,
                        Err(e) if halts(&e) => {
                            self.undo_to(depth);
                            return false;
                        }
                        Err(_) => continue,
                    };
                    let current = self.current_sigs();
                    let matched = history::bodies_distance_within(target, &current, c.tolerance());
                    let taken = match matched {
                        Some(_) => self
                            .change_check(&name, own, target, &current, &shapes, Some(states))
                            .map_err(|e| trace!("{name}: {} -> its state {state}: {e}", summary(c)))
                            .ok(),
                        None => None,
                    };
                    if let (Some((d, _)), Some(taken)) = (matched, taken) {
                        trace!(
                            "{name}: {} -> its state {state} ({d:.1e}) with {f} as {}",
                            summary(c),
                            summary(&Candidate::new(def.clone()))
                        );
                        self.oracle.cursor = Some(state);
                        self.report.history.matched += 1;
                        self.accepted(at, index, &uids, c, Some(true));
                        self.record_taken(at, taken);
                        self.loosely_matched(&name, d);
                        let entry = &mut self.report.items[alternatives.report];
                        let chosen = format!(
                            "chosen for {name}: the first definition gave its state too, but \
                             not the copies"
                        );
                        entry.note = Some(match note {
                            Some(n) if !n.is_empty() => format!("{n}; {chosen}"),
                            _ => chosen,
                        });
                        self.alternatives.remove(&f);
                        self.after_match(at, index, item, c, &[], state, d, 0.0);
                        return true;
                    }
                    trace!(
                        "{name}: {} -> {:?} with {f} as {}",
                        summary(c),
                        short(&current),
                        summary(&Candidate::new(def.clone()))
                    );
                    self.undo_to(inner);
                }
                self.undo_to(depth);
            }
        }
        false
    }

    /// With `MITCAD_IMPORT_TRACE_STATES` too, every history state's solids
    /// and the items' states, for traces.
    fn trace_states(&mut self) {
        if !tracing() || std::env::var_os("MITCAD_IMPORT_TRACE_STATES").is_none() {
            return;
        }
        let kernel = self.doc.kernel();
        for i in 0..self.oracle.count() {
            match self.oracle.try_sigs(kernel, i) {
                Some(s) => trace!("history state {i}: {:?}", short(&s)),
                None => trace!("history state {i}: not rebuilt"),
            }
        }
        let items = self.dump.timeline_items();
        for (p, it) in items.iter().enumerate() {
            if let Some(s) = self.oracle.state_of_item(it.index.unwrap_or(p as i64)) {
                trace!("{}: state {s}", item_name(it));
            }
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
        // The state of the next item that has one is that item's: this one
        // matches (and its fallbacks take) only states before it.
        let limit = self.next_named_state(index);
        self.oracle.limit = limit;
        self.verified_without_state(at, index, item, limit);
        self.oracle.limit = None;
    }

    /// The state the next item after `index` in the timeline that has a
    /// state of its own made, when it lies ahead of the replay.
    fn next_named_state(&mut self, index: i64) -> Option<usize> {
        let dump = self.dump;
        let from = self.oracle.next_index();
        let items = dump.timeline_items();
        let position = items
            .iter()
            .enumerate()
            .position(|(p, it)| it.index.unwrap_or(p as i64) == index)?;
        items
            .iter()
            .enumerate()
            .skip(position + 1)
            .filter_map(|(p, it)| self.oracle.state_of_item(it.index.unwrap_or(p as i64)))
            .filter(|&s| s >= from)
            .min()
    }

    /// [`Importer::verified`] of an item without a state of its own, with
    /// the history's states before `limit`.
    fn verified_without_state(
        &mut self,
        at: usize,
        index: i64,
        item: &TimelineItem,
        limit: Option<usize>,
    ) {
        let budget = self.options.item_seconds;
        self.within(budget, |s| {
            s.verified_without_state_in(at, index, item, limit);
        });
    }

    fn verified_without_state_in(
        &mut self,
        at: usize,
        index: i64,
        item: &TimelineItem,
        limit: Option<usize>,
    ) {
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
        let pending = !self.pending.is_empty();
        if pending {
            let next = self.oracle.next_index();
            let last = self.oracle.count();
            let last = last.min(limit.unwrap_or(usize::MAX));
            befores.extend(
                (next..last)
                    .take(self.options.fallback_states)
                    .map(Before::State),
            );
        } else if let Some(q) = self.oracle.cursor
            && self.carried(q) > history::EXACT
        {
            // The bodies carry an earlier item's approximation, which may
            // keep it from matching: also tried on the state's bodies.
            befores.push(Before::State(q));
        }
        let mut last_error = String::from("no definition gave the file's result");
        // The definition cut short after the longest time ([`Turns::note`]).
        let mut slowest: Option<(f64, String)> = None;
        let mut attempts = 0;
        let started = self.clock();
        let budget = self.options.item_seconds;
        let out_of_time = || started.elapsed().as_secs_f64() > budget;
        for before in befores {
            if let Some(reason) = self.halted(index) {
                last_error = reason.to_owned();
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
            // How far the bodies are from the state they stand for.
            let carried = match base {
                Some(q) if before == Before::Keep => self.carried(q),
                _ => 0.0,
            };
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
            let start = self.current_sigs();
            let start_shapes = self.body_shapes();
            // The state the bodies stand for, for the volume the item
            // changes itself ([`history::Change`]).
            let reference = match base {
                Some(q) => self.oracle.try_sigs(self.doc.kernel(), q),
                None => Some(Vec::new()),
            };
            // Definitions cut short at their share of the time are tried
            // again after the others ([`Turns`]).
            let mut turns = Turns::default();
            let mut position = 0;
            // The definitions after the one evaluated, evaluated ahead by
            // workers on these bodies (mitcad#95).
            let mut ahead = self.ahead(budget);
            loop {
                let again = if position < limit {
                    None
                } else {
                    let Some(k) = turns.again(budget - started.elapsed().as_secs_f64()) else {
                        break;
                    };
                    Some(k)
                };
                let k = again.unwrap_or_else(|| {
                    position += 1;
                    position - 1
                });
                let c = &candidates[k];
                if again.is_none() && turns.waits_behind(&candidates, k) {
                    trace!(
                        "{}: {before:?} {} waits behind the same boolean, which gave way",
                        item_name(item),
                        summary(c)
                    );
                    if let Some(a) = &mut ahead {
                        a.forget(k);
                    }
                    continue;
                }
                if attempts >= self.options.max_attempts || out_of_time() {
                    last_error = format!("{last_error} (gave up after {attempts} attempts)");
                    break;
                }
                attempts += 1;
                let inner = self.doc.undo_depth();
                let share = turns.share(budget, position == limit, again.is_some());
                let tried = std::time::Instant::now();
                let until = limit.min(k + 1 + self.options.max_attempts.saturating_sub(attempts));
                let first_round = ahead.as_mut().filter(|_| again.is_none());
                let evaluated = self.evaluate(
                    first_round,
                    &candidates,
                    k,
                    until,
                    index,
                    item.name(),
                    share,
                );
                let ran = evaluated
                    .ran
                    .unwrap_or_else(|| tried.elapsed().as_secs_f64());
                match evaluated.result.clone() {
                    Ok(uids) => {
                        let current = match &evaluated.bodies {
                            Some(bodies) => bodies.iter().map(|(_, s)| *s).collect(),
                            None => self.current_sigs(),
                        };
                        if implausible(c, &start, &current) {
                            trace!(
                                "{}: {before:?} {} -> {:?} from {:?}: not what it makes",
                                item_name(item),
                                summary(c),
                                short(&current),
                                short(&start)
                            );
                            self.undo_to(inner);
                            last_error = NOT_IN_HISTORY.to_owned();
                            continue;
                        }
                        let kernel = self.doc.kernel();
                        // With items pending and nothing in their place, an
                        // approximate match could hide what they changed.
                        let exact = before == Before::Keep && !self.pending.is_empty();
                        let found =
                            self.oracle
                                .find_within(kernel, from, &current, exact, c.tolerance());
                        // (Nor can the feature make a state that has less
                        // volume than the bodies it started from, for a join,
                        // or more, for a cut: it came close by chance.)
                        let found = found.filter(|(r, _)| {
                            let kernel = self.doc.kernel();
                            !implausible(c, &start, &self.oracle.sigs(kernel, *r))
                        });
                        // (Evaluated ahead: now into the document.)
                        let uids = if found.is_some() || exact {
                            match self.take_over(index, c, item.name(), &evaluated) {
                                Ok(uids) => uids,
                                Err(e) => {
                                    last_error = e;
                                    if halts(&last_error) {
                                        break;
                                    }
                                    continue;
                                }
                            }
                        } else {
                            uids
                        };
                        // (Nor one whose own change is not the file's: where
                        // the volumes do not settle it, its faces decide.)
                        let mut taken = geometric::Taken::default();
                        if let Some((r, _)) = found {
                            let kernel = self.doc.kernel();
                            let target = self.oracle.sigs(kernel, r);
                            let own = reference
                                .as_deref()
                                .and_then(|re| history::Change::new(re, &start, &target));
                            let states = geometric::States {
                                before: base,
                                after: r,
                            };
                            let name = item_name(item);
                            match self.change_check(
                                &name,
                                own,
                                &target,
                                &current,
                                &start_shapes,
                                Some(states),
                            ) {
                                Ok(t) => taken = t,
                                Err(e) => {
                                    last_error = e;
                                    trace!(
                                        "{name}: {before:?} {} -> state {r}: {last_error}: {:?} for {:?}",
                                        summary(c),
                                        short(&current),
                                        short(&target)
                                    );
                                    self.undo_to(inner);
                                    continue;
                                }
                            }
                        }
                        if let Some((r, d)) = found {
                            trace!(
                                "{}: {before:?} {} -> state {r}",
                                item_name(item),
                                summary(c)
                            );
                            self.commit(before, base, fallback);
                            if !pending && let Some(uid) = fallback {
                                self.exact_before(uid, &item_name(item));
                            }
                            self.oracle.cursor = Some(r);
                            self.report.history.matched += 1;
                            self.accepted(at, index, &uids, c, Some(true));
                            self.record_taken(at, taken);
                            self.loosely_matched(&item_name(item), d);
                            self.after_match(
                                at,
                                index,
                                item,
                                c,
                                &candidates[k + 1..],
                                r,
                                d,
                                carried,
                            );
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
                        last_error = NOT_IN_HISTORY.to_owned();
                    }
                    Err(e) if e == GAVE_WAY => {
                        trace!(
                            "{}: {before:?} {} gives way after {ran:.2} s, its share",
                            item_name(item),
                            summary(c)
                        );
                        turns.cut_short(k, ran, true);
                    }
                    Err(e) => {
                        trace!("{}: {before:?} {} failed: {e}", item_name(item), summary(c));
                        if e == OUT_OF_TIME {
                            turns.cut_short(k, ran, false);
                        }
                        let halt = halts(&e);
                        last_error = e;
                        if halt {
                            break;
                        }
                    }
                }
            }
            drop(ahead);
            if let Some(note) = turns.note(&candidates[..limit])
                && slowest.as_ref().is_none_or(|(s, _)| note.0 > *s)
            {
                slowest = Some(note);
            }
            self.undo_to(depth);
        }
        // Which definition the time went to.
        if !halts(&last_error)
            && let Some((_, note)) = slowest
        {
            last_error = format!("{last_error}; {note}");
        }
        self.tried_in_vain(index, &last_error);
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
        // How far the bodies are from the state before it (approximations
        // of earlier items they carry).
        let mut carried = 0.0;
        let within = expected
            .as_ref()
            .and_then(|e| history::bodies_distance(e, &current));
        // Bodies the state keeps in another component than the replay (an
        // item that moved bodies into a new component, not replayed) are
        // moved there by the state's base feature, so that the item finds
        // them in its component.
        let elsewhere = within.is_some() && start.is_some_and(|s| self.bodies_elsewhere(s));
        if expected.is_none() {
            // Not rebuilt: the replay goes on as it is.
            trace!("{name}: the state before it could not be rebuilt");
        } else if let Some((d, _)) = within.filter(|_| !elsewhere) {
            // The pending items changed nothing (or are in the state).
            carried = d;
            self.commit(Before::Keep, start, None);
        } else if let Some(s) = start {
            let depth = self.doc.undo_depth();
            let pending = !self.pending.is_empty();
            match self.fallback_to(s, None) {
                Ok(fallback) => {
                    if !pending && let Some(uid) = fallback {
                        self.exact_before(uid, &name);
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
        let budget = self.options.item_seconds;
        let fresh = self.give_up(index).is_none();
        // The state before it, for the volume it changes itself (and its
        // faces, mitcad#138).
        let reference = expected.as_deref();
        let states = geometric::States {
            before: start,
            after: state,
        };
        let mut tried =
            self.try_state(at, index, item, states, &target, reference, carried, budget);
        // The bodies before it carry an earlier item's approximation, which
        // may keep it from matching: brought to the state before it exactly,
        // it is tried again (in half the time). So is a fillet or chamfer
        // the geometry kernel failed on the replay's bodies: the stored
        // state's edges are split otherwise, and a rounding that fails (or
        // crashes) on one builds on the other.
        let kernel_failed = matches!(&item.detail, Some(Detail::Fillet(_) | Detail::Chamfer(_)))
            && matches!(&tried, Err(e) if !e.starts_with(NOT_IN_HISTORY) && e != "nothing to try");
        let again = matches!(&tried, Err(e) if !halts(e))
            && (carried > history::EXACT || kernel_failed)
            && self.give_up(index).is_none();
        if let (true, Some(s)) = (again, start) {
            let depth = self.doc.undo_depth();
            // (Bodies within the tolerance of the state's are replaced too.)
            match self.fallback_to_as(s, None, carried <= history::EXACT) {
                Ok(Some(uid)) => {
                    trace!("{name}: tried again on state {s} exactly");
                    let retried = self.try_state(
                        at,
                        index,
                        item,
                        states,
                        &target,
                        reference,
                        0.0,
                        budget / 2.0,
                    );
                    if retried.is_ok() {
                        self.exact_before(uid, &name);
                        tried = retried;
                    } else {
                        self.undo_to(depth);
                    }
                }
                _ => self.undo_to(depth),
            }
        }
        let last_error = match tried {
            Ok(()) => return,
            Err(e) => e,
        };
        if fresh {
            self.tried_in_vain(index, &last_error);
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

    /// The candidates of an item with a known state (`states.after`, whose
    /// bodies are `target`) on the bodies as they are (`reference`: the
    /// state's before it, `states.before`, when rebuilt; `carried`: how far
    /// they are from it), for at most `budget` seconds: Ok when one gave the
    /// state (and was accepted), else why none did.
    #[allow(clippy::too_many_arguments)]
    fn try_state(
        &mut self,
        at: usize,
        index: i64,
        item: &TimelineItem,
        states: geometric::States,
        target: &[Sig],
        reference: Option<&[Sig]>,
        carried: f64,
        budget: f64,
    ) -> Result<(), String> {
        self.within(budget, |s| {
            s.try_state_in(at, index, item, states, target, reference, carried, budget)
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn try_state_in(
        &mut self,
        at: usize,
        index: i64,
        item: &TimelineItem,
        states: geometric::States,
        target: &[Sig],
        reference: Option<&[Sig]>,
        carried: f64,
        budget: f64,
    ) -> Result<(), String> {
        let state = states.after;
        let name = item_name(item);
        let mut last_error = String::from("no definition gave the file's result");
        // The volume the item adds or removes must be the file's too, not
        // only the bodies' measures (mitcad#121).
        let own = reference.and_then(|r| history::Change::new(r, &self.current_sigs(), target));
        let started = self.clock();
        // A fillet's or chamfer's edge guesses one history state at a time.
        let mut guesses = matches!(&item.detail, Some(Detail::Fillet(_) | Detail::Chamfer(_)))
            .then(Guesses::default);
        let clock = std::time::Instant::now();
        let translated = match self.give_up(index) {
            Some(reason) => Err(reason),
            None => self.translate_part(index, item, &mut guesses),
        };
        trace!(
            "{name}: translated in {:.2} s",
            clock.elapsed().as_secs_f64()
        );
        // Extrusions whose prism holds less than the state adds or
        // removes (or that change the volume the other way) are not tried;
        // they are left out before the likeliest are chosen.
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
        let translated = translated.map(|c| self.rank(c, state));
        match translated {
            Ok(c) if c.is_empty() => last_error = "nothing to try".to_owned(),
            Ok(mut candidates) => {
                // The bodies a definition without participants changed:
                // limited to participants that include them it gives the
                // same result, which is not built again (on large bodies
                // each try takes seconds).
                let before: HashMap<BodyUid, Sig> = self.current_bodies().into_iter().collect();
                // (Their shapes, for the faces of a change the volumes do not
                // settle.)
                let before_shapes = self.body_shapes();
                let mut changed_by: Vec<(Value, BTreeSet<String>)> = Vec::new();
                // The geometry kernel fails a fillet or chamfer of more
                // edges too (contour by contour, unless at a vertex): such
                // guesses are left out.
                let mut failed_dressups: Vec<(Value, BTreeSet<String>)> = Vec::new();
                let mut attempt = 0;
                // An extrusion that gives the state only approximately (a
                // set of regions next to the right one can come within the
                // tolerance, and the difference grows in later items): a
                // few more definitions are tried for a closer one first.
                let mut loose: Option<(usize, f64)> = None;
                let mut closer_tries = 0;
                // Definitions cut short at their share of the time wait for
                // the second round.
                let mut turns = Turns::default();
                let mut second_round = false;
                let mut next = 0;
                // The definitions after the one evaluated, evaluated ahead
                // by workers (mitcad#95).
                let mut ahead = self.ahead(budget);
                loop {
                    let mut again = None;
                    if next == candidates.len() {
                        // A fillet's or chamfer's next edge guesses (the
                        // next history state's), as long as definitions are
                        // still tried.
                        let room = self.options.max_candidates.saturating_sub(candidates.len());
                        let more = guesses.as_ref().is_some_and(|g| !g.done)
                            && room > 0
                            && (loose.is_none() || closer_tries < LOOSE_RETRIES)
                            && !second_round;
                        if more {
                            if attempt >= self.options.max_attempts
                                || started.elapsed().as_secs_f64() > budget
                            {
                                last_error =
                                    format!("{last_error} (gave up after {attempt} attempts)");
                                break;
                            }
                            let mut part = self
                                .translate_part(index, item, &mut guesses)
                                .unwrap_or_default();
                            part.truncate(room);
                            candidates.extend(part);
                        }
                        if next == candidates.len() {
                            // The first round is over: those that gave way.
                            second_round = true;
                            let remaining = budget - started.elapsed().as_secs_f64();
                            let Some(k) = turns.again(remaining) else {
                                break;
                            };
                            again = Some(k);
                        }
                    }
                    let k = again.unwrap_or_else(|| {
                        next += 1;
                        next - 1
                    });
                    let c = &candidates[k];
                    if loose.is_some() {
                        if closer_tries >= LOOSE_RETRIES {
                            break;
                        }
                        closer_tries += 1;
                    }
                    // (Participants in another component take the feature
                    // there: not the same.)
                    if let Some((free, limited)) = participants_of(c)
                        && self.def_component(&free) == self.def_component(&c.defs[0])
                        && changed_by
                            .iter()
                            .any(|(d, changed)| *d == free && changed.is_subset(&limited))
                    {
                        trace!(
                            "{name}: {} gives what it gave without participants",
                            summary(c)
                        );
                        if let Some(a) = &mut ahead {
                            a.forget(k);
                        }
                        continue;
                    }
                    if again.is_none() && turns.waits_behind(&candidates, k) {
                        trace!(
                            "{name}: {} waits behind the same boolean, which gave way",
                            summary(c)
                        );
                        if let Some(a) = &mut ahead {
                            a.forget(k);
                        }
                        continue;
                    }
                    if let Some((shape, edges)) = dressup_edges(c)
                        && failed_dressups
                            .iter()
                            .any(|(s, failed)| *s == shape && failed.is_subset(&edges))
                    {
                        trace!("{name}: {} has the edges of one that failed", summary(c));
                        if let Some(a) = &mut ahead {
                            a.forget(k);
                        }
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
                    let last = next == candidates.len() && guesses.as_ref().is_none_or(|g| g.done);
                    let share = turns.share(budget, last, again.is_some());
                    let tried = std::time::Instant::now();
                    // Those a later one may still need: within the
                    // attempts, and the closer definitions after a loose
                    // match.
                    let mut until = k + 1 + self.options.max_attempts.saturating_sub(attempt);
                    if loose.is_some() {
                        until = until.min(k + 1 + LOOSE_RETRIES.saturating_sub(closer_tries));
                    }
                    let first_round = ahead.as_mut().filter(|_| again.is_none());
                    let mut evaluated = self.evaluate(
                        first_round,
                        &candidates,
                        k,
                        until,
                        index,
                        item.name(),
                        share,
                    );
                    let ran = evaluated
                        .ran
                        .unwrap_or_else(|| tried.elapsed().as_secs_f64());
                    match evaluated.result.clone() {
                        Ok(_) => {
                            let clock = std::time::Instant::now();
                            let bodies = match &evaluated.bodies {
                                Some(bodies) => bodies.clone(),
                                None => self.current_bodies(),
                            };
                            trace!("{name}: measured in {:.2} s", clock.elapsed().as_secs_f64());
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
                                changed_by.push((def.clone(), changed));
                            }
                            let current: Vec<Sig> = bodies.iter().map(|(_, s)| *s).collect();
                            let matched =
                                history::bodies_distance_within(target, &current, c.tolerance());
                            // Its own change must be the file's; where the
                            // volumes do not settle that, its faces do, with
                            // the result in the document (mitcad#138).
                            let mut taken = geometric::Taken::default();
                            if matched.is_some() {
                                let off = history::own_difference(own, target, &current);
                                if evaluated.ahead() && off.is_some_and(|o| o > history::UNSETTLED)
                                {
                                    match self.take_over(index, c, item.name(), &evaluated) {
                                        Ok(uids) => {
                                            evaluated = ahead::Evaluated {
                                                result: Ok(uids),
                                                bodies: None,
                                                ran: evaluated.ran,
                                            };
                                        }
                                        Err(e) => {
                                            trace!("{name}: {} not taken over: {e}", summary(c));
                                            last_error = e;
                                            if halts(&last_error) {
                                                break;
                                            }
                                            continue;
                                        }
                                    }
                                }
                                let checked = self.change_check(
                                    &name,
                                    own,
                                    target,
                                    &current,
                                    &before_shapes,
                                    Some(states),
                                );
                                match checked {
                                    Ok(t) => taken = t,
                                    Err(e) => {
                                        last_error = e;
                                        trace!(
                                            "{name}: {} -> its state {state}: {last_error}: {:?} \
                                             for {:?}",
                                            summary(c),
                                            short(&current),
                                            short(target)
                                        );
                                        self.undo_to(inner);
                                        continue;
                                    }
                                }
                            }
                            // (Not for a difference the bodies before it
                            // carried: no definition comes closer.)
                            if let Some((d, _)) = matched
                                && seeks_closer(c)
                                && introduced(d, carried)
                                && loose.is_none_or(|(_, l)| closer(c, d, l))
                                && closer_tries < LOOSE_RETRIES
                            {
                                trace!(
                                    "{name}: {} -> its state {state} only within {d:.1e}; \
                                     looking for a closer definition",
                                    summary(c)
                                );
                                loose = Some((k, d));
                                self.undo_to(inner);
                                continue;
                            }
                            if let Some((d, _)) = matched
                                && loose.is_none_or(|(_, l)| closer(c, d, l))
                            {
                                // (Evaluated ahead: now into the document.)
                                let uids = match self.take_over(index, c, item.name(), &evaluated) {
                                    Ok(uids) => uids,
                                    Err(e) => {
                                        trace!("{name}: {} not taken over: {e}", summary(c));
                                        last_error = e;
                                        if halts(&last_error) {
                                            break;
                                        }
                                        continue;
                                    }
                                };
                                if let Some(body) = self.invalid_change(&before, &bodies) {
                                    trace!("{name}: {} makes {body} invalid", summary(c));
                                    self.undo_to(inner);
                                    last_error = format!(
                                        "its result is the file's, but the geometry kernel's \
                                         checker finds {body} invalid"
                                    );
                                    continue;
                                }
                                trace!("{name}: {} -> its state {state} ({d:.1e})", summary(c));
                                self.oracle.cursor = Some(state);
                                self.report.history.matched += 1;
                                self.accepted(at, index, &uids, c, Some(true));
                                self.record_taken(at, taken);
                                self.loosely_matched(&name, d);
                                let later = &candidates[k + 1..];
                                self.after_match(at, index, item, c, later, state, d, carried);
                                return Ok(());
                            }
                            trace!(
                                "{name}: {} -> {:?}, its state {state}: {:?}",
                                summary(c),
                                short(&current),
                                short(target)
                            );
                            self.undo_to(inner);
                            last_error = NOT_IN_HISTORY.to_owned();
                        }
                        Err(e) if halts(&e) => {
                            last_error = e;
                            break;
                        }
                        Err(e) if e == GAVE_WAY => {
                            trace!(
                                "{name}: {} gives way after {ran:.2} s, its share",
                                summary(c)
                            );
                            turns.cut_short(k, ran, true);
                        }
                        Err(e) => {
                            trace!("{name}: {} failed: {e}", summary(c));
                            if e == OUT_OF_TIME {
                                turns.cut_short(k, ran, false);
                            }
                            // More edges can end a rounding at a vertex, or
                            // take the rest of a tangent chain.
                            if !e.contains("vertex") && !e.contains("continues tangentially") {
                                failed_dressups.extend(dressup_edges(c));
                            }
                            last_error = e;
                        }
                    }
                }
                // The workers still evaluating are not needed.
                drop(ahead);
                // None closer: the approximate one.
                if let Some((k, _)) = loose
                    && !halts(&last_error)
                {
                    let c = &candidates[k];
                    let inner = self.doc.undo_depth();
                    if let Ok(uids) = self.try_candidate(index, c, item.name()) {
                        let current = self.current_sigs();
                        let matched =
                            history::bodies_distance_within(target, &current, c.tolerance());
                        let taken = match matched {
                            Some(_) => self
                                .change_check(
                                    &name,
                                    own,
                                    target,
                                    &current,
                                    &before_shapes,
                                    Some(states),
                                )
                                .map_err(|e| {
                                    trace!("{name}: {} -> its state {state}: {e}", summary(c))
                                })
                                .ok(),
                            None => None,
                        };
                        let valid = || {
                            let before: HashMap<BodyUid, Sig> = HashMap::new();
                            self.invalid_change(&before, &self.current_bodies())
                                .is_none()
                        };
                        if let (Some((d, _)), Some(taken)) = (matched, taken)
                            && valid()
                        {
                            trace!("{name}: {} -> its state {state} ({d:.1e})", summary(c));
                            self.oracle.cursor = Some(state);
                            self.report.history.matched += 1;
                            self.accepted(at, index, &uids, c, Some(true));
                            self.record_taken(at, taken);
                            self.loosely_matched(&name, d);
                            self.after_match(at, index, item, c, &[], state, d, carried);
                            return Ok(());
                        }
                        self.undo_to(inner);
                    }
                }
                // A pattern or mirror of extrusions that gave other
                // definitions' bodies too: with those definitions.
                if !halts(&last_error)
                    && self.with_other_profiles(at, index, item, &candidates, target, own, states)
                {
                    return Ok(());
                }
                // Which definition the time went to.
                if !halts(&last_error)
                    && let Some((_, note)) = turns.note(&candidates)
                {
                    last_error = format!("{last_error}; {note}");
                }
            }
            Err(e) => {
                trace!("{name}: {e}");
                last_error = e;
            }
        }
        Err(last_error)
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
        let started = self.clock();
        let budget = self.options.item_seconds;
        let usable = candidates.iter().filter(|c| guesses || !c.guess);
        for c in usable.take(self.options.max_candidates) {
            if started.elapsed().as_secs_f64() > budget {
                break;
            }
            let depth = self.doc.undo_depth();
            let uids = match self.try_candidate(index, c, item.name()) {
                Ok(uids) => uids,
                Err(e) if halts(&e) => break,
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

    /// With [`Options::validate`], a body of `bodies` that is new or
    /// changed since `before` and that the geometry kernel's checker finds
    /// invalid (a kernel that cannot check finds none).
    fn invalid_change(
        &self,
        before: &HashMap<BodyUid, Sig>,
        bodies: &[(BodyUid, Sig)],
    ) -> Option<BodyUid> {
        if !self.options.validate {
            return None;
        }
        bodies
            .iter()
            .filter(|(uid, sig)| !before.get(uid).is_some_and(|s| s.same(sig)))
            .find(|(uid, _)| {
                self.doc
                    .body_shape(*uid)
                    .is_some_and(|s| matches!(self.doc.kernel().is_valid(s), Ok(false)))
            })
            .map(|(uid, _)| *uid)
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
        let had_bodies = !self.doc.bodies().is_empty();
        let body_count = self.doc.bodies().len();
        let mut last = match candidates {
            Ok(candidates) => {
                let ordered = candidates
                    .iter()
                    .filter(|c| !c.guess)
                    .chain(candidates.iter().filter(|c| c.guess));
                let mut last = "nothing to try".to_owned();
                for c in ordered.take(self.options.max_candidates) {
                    let depth = self.doc.undo_depth();
                    match self.try_candidate(index, c, item.name()) {
                        // Unchecked, a definition that takes every body
                        // away is not kept: the file's part has bodies
                        // after it.
                        Ok(_) if had_bodies && self.doc.bodies().is_empty() => {
                            self.undo_to(depth);
                            last = "it would leave no bodies".to_owned();
                        }
                        // Nor a helix cut that splits a body: its hand and
                        // direction are guesses (the `.ipt` import's coils).
                        Ok(_)
                            if c.defs.last().is_some_and(|d| {
                                d["type"] == "helix" && d["operation"] == "cut"
                            }) && self.doc.bodies().len() > body_count =>
                        {
                            self.undo_to(depth);
                            last = "its cut would split a body".to_owned();
                        }
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
                            let halt = halts(&e);
                            last = e;
                            if halt {
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
    /// on it in an earlier try, the import was stopped, its definitions
    /// gave no state in an earlier try, the process ran low on memory, or
    /// the time limit is over.
    fn give_up(&self, index: i64) -> Option<String> {
        let failed = || {
            self.options
                .failed_items
                .iter()
                .find(|(i, _)| *i == index)
                .map(|(_, reason)| reason.clone())
        };
        if self.options.hung_items.contains(&index) {
            Some(HUNG.to_owned())
        } else if self.abandoned() || self.stopped(index) {
            Some(STOPPED.to_owned())
        } else if let Some(reason) = failed() {
            Some(reason)
        } else if self.memory_low() {
            Some(LOW_MEMORY.to_owned())
        } else if self.out_of_time() {
            Some("the import's time limit was reached".to_owned())
        } else {
            None
        }
    }

    /// [`Importer::translate`]; with `guesses` set, the next part of a
    /// fillet's or chamfer's candidates ([`Guesses`]).
    fn translate_part(
        &mut self,
        index: i64,
        item: &TimelineItem,
        guesses: &mut Option<Guesses>,
    ) -> Result<Vec<Candidate>, String> {
        self.guesses = guesses.take();
        let translated = self.translate(index, item);
        *guesses = self.guesses.take();
        translated
    }

    /// The item's definitions were tried and gave no state, for `reason`
    /// (for a try run again, [`Options::failed_items`]).
    fn tried_in_vain(&self, index: i64, reason: &str) {
        if !halts(reason)
            && let Some(p) = &self.options.progress
        {
            p.failed(index, reason);
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
    /// states, those without an estimate in their own order; those a
    /// proven rule decoded ([`Candidate::first`]) before all. At most
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
            candidates.sort_by(|a, b| {
                b.first
                    .cmp(&a.first)
                    .then_with(|| score(a).total_cmp(&score(b)))
            });
        }
        candidates.truncate(self.options.max_candidates);
        self.learn_ranked(&candidates);
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
        self.fallback_to_as(q, name, false)
    }

    /// [`Importer::fallback_to`]; `all`: every solid comes from the state,
    /// also one the replay has within the tolerance (whose edges are split
    /// otherwise).
    fn fallback_to_as(
        &mut self,
        q: usize,
        name: Option<&str>,
        all: bool,
    ) -> Result<Option<FeatureUid>, String> {
        if self.abandoned() {
            return Err(STOPPED.to_owned());
        }
        let kernel = self.doc.kernel();
        let state: Vec<(StoredBody<K::Shape>, Sig)> = self.oracle.state(kernel, q)?.to_vec();
        let sheets = self.oracle.sheets(kernel, q)?.to_vec();
        let name = name.map(str::to_owned).or_else(|| {
            self.pending
                .last()
                .map(|p| self.report.items[p.report].name.clone())
        });
        self.replace_bodies_as(
            &state,
            &sheets,
            name.as_deref(),
            &format!("ASM history state {q}"),
            all,
        )
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
        // Without a history the file's sheets are not told from the tools
        // and sketch faces its blobs also keep: solids only.
        self.replace_bodies(&finals, &[], Some(name), "stored design")?
            .ok_or_else(|| "the bodies are the stored ones already".to_owned())
    }

    /// The replay's sheets (surface bodies) with their signatures and
    /// components.
    fn current_sheets(&self) -> Vec<(BodyUid, SheetSig, ComponentUid)> {
        let kernel = self.doc.kernel();
        self.doc
            .bodies()
            .iter()
            .filter_map(|b| {
                let sig = SheetSig::of(kernel, b.shape)?;
                let c = self.doc.body_component(b.uid).unwrap_or(ComponentUid::ROOT);
                Some((b.uid, sig, c))
            })
            .collect()
    }

    /// Base features, one per component that changes: bodies of `target`
    /// not in the replay replace the replay's bodies not in `target`, in
    /// their components (a body in another component than the file's moves
    /// there). Returns the first. The state's `sheets` (surface bodies,
    /// which the history's matching leaves out) come in alike: those the
    /// replay lacks are added, the replay's the state lacks removed
    /// (mitcad#35).
    fn replace_bodies(
        &mut self,
        target: &[(StoredBody<K::Shape>, Sig)],
        sheets: &[(StoredBody<K::Shape>, SheetSig)],
        name: Option<&str>,
        source: &str,
    ) -> Result<Option<FeatureUid>, String> {
        self.replace_bodies_as(target, sheets, name, source, false)
    }

    /// [`Importer::replace_bodies`]; `all`: every solid of the replay is
    /// replaced, also one that measures as a body of `target`.
    fn replace_bodies_as(
        &mut self,
        target: &[(StoredBody<K::Shape>, Sig)],
        sheets: &[(StoredBody<K::Shape>, SheetSig)],
        name: Option<&str>,
        source: &str,
        all: bool,
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
                .find(|&i| !all && !used[i] && current[i].2 == component && current[i].1.same(sig))
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
        // Sheets: added and removed, not paired with solids.
        let current_sheets = self.current_sheets();
        let mut kept = vec![false; current_sheets.len()];
        let mut new_sheets: BTreeMap<ComponentUid, Vec<&StoredBody<K::Shape>>> = BTreeMap::new();
        for (body, sig) in sheets {
            let component = self.components.of_body(body.component);
            match (0..current_sheets.len()).find(|&i| {
                !kept[i] && current_sheets[i].2 == component && current_sheets[i].1.same(sig)
            }) {
                Some(i) => kept[i] = true,
                None => new_sheets.entry(component).or_default().push(body),
            }
        }
        let mut old_sheets: BTreeMap<ComponentUid, Vec<BodyUid>> = BTreeMap::new();
        for ((uid, _, component), kept) in current_sheets.iter().zip(&kept) {
            if !kept {
                old_sheets.entry(*component).or_default().push(*uid);
            }
        }
        if new.is_empty() && old.is_empty() && new_sheets.is_empty() && old_sheets.is_empty() {
            return Ok(None);
        }
        let components: BTreeSet<ComponentUid> = new
            .keys()
            .chain(old.keys())
            .chain(new_sheets.keys())
            .chain(old_sheets.keys())
            .copied()
            .collect();
        let mut inputs = Vec::new();
        for component in components {
            let new = new.remove(&component).unwrap_or_default();
            let mut old = old.remove(&component).unwrap_or_default();
            let added_sheets = new_sheets.remove(&component).unwrap_or_default();
            let mut removed_sheets = old_sheets.remove(&component).unwrap_or_default();
            // Pair each new body with the nearest old one, so bodies keep
            // their identity where they can; sheets in order with sheets.
            let mut replaces = Vec::new();
            let mut paired = Vec::new();
            let mut unpaired = Vec::new();
            for (body, sig) in &new {
                let nearest = old
                    .iter()
                    .enumerate()
                    .min_by(|a, b| {
                        let d = |s: &Sig| geom::distance(s.center, sig.center);
                        d(&a.1.1).total_cmp(&d(&b.1.1))
                    })
                    .map(|(i, _)| i);
                match nearest {
                    Some(i) => {
                        replaces.push(old.remove(i).0);
                        paired.push(*body);
                    }
                    None => unpaired.push(*body),
                }
            }
            for body in added_sheets {
                if removed_sheets.is_empty() {
                    unpaired.push(body);
                } else {
                    replaces.push(removed_sheets.remove(0));
                    paired.push(body);
                }
            }
            // Bodies are listed replacing ones first.
            let ordered: Vec<ImportBody<K::Shape>> = paired
                .iter()
                .chain(&unpaired)
                .map(|body| ImportBody {
                    name: body.name.clone(),
                    color: None,
                    shape: body.shape.clone(),
                })
                .collect();
            // Removed bodies come after the replacing ones, when there are
            // no others (a base feature without bodies removes the bodies it
            // replaces); else by a base feature of their own.
            let removed: Vec<BodyUid> = old
                .iter()
                .map(|(uid, _)| *uid)
                .chain(removed_sheets)
                .collect();
            let alone = !unpaired.is_empty() && !removed.is_empty();
            if !alone {
                replaces.extend(removed.iter().copied());
            }
            let input_name = |inputs: &Vec<BaseInput<K::Shape>>| {
                if inputs.is_empty() {
                    name.filter(|n| !n.trim().is_empty()).map(str::to_owned)
                } else {
                    None
                }
            };
            inputs.push(BaseInput {
                name: input_name(&inputs),
                source: Some(format!("{} ({source})", self.report.label)),
                replaces,
                component: Some(component),
                ..BaseInput::new(ordered)
            });
            if alone {
                inputs.push(BaseInput {
                    name: None,
                    source: Some(format!("{} ({source})", self.report.label)),
                    replaces: removed,
                    component: Some(component),
                    ..BaseInput::new(Vec::new())
                });
            }
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
        self.release_memory();
        self.report_low_memory();
        // A kernel call that never returns, to test the watchdog with
        // (`mitcad-ffi` has another, `busy`).
        if self.options.compare
            && std::env::var_os("MITCAD_IMPORT_STALL").is_some_and(|v| v == "compare")
        {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        }
        self.compare_final();
        // Low on memory first while the file's bodies were built for the
        // comparison (an allocation failed in their healing, mitcad#132).
        if self.report.low_memory.is_none() {
            self.report_low_memory();
        }
    }

    /// Whether history state `q` keeps a body in another component than
    /// the replay does: the replay has the same body, but not in the
    /// component the state's blob belongs to (bodies of blobs whose
    /// component is not known are left out).
    fn bodies_elsewhere(&mut self, q: usize) -> bool {
        let kernel = self.doc.kernel();
        let Ok(state) = self.oracle.state(kernel, q).map(<[_]>::to_vec) else {
            return false;
        };
        let current: Vec<(Sig, ComponentUid)> = self
            .current_bodies()
            .into_iter()
            .map(|(uid, sig)| {
                let c = self.doc.body_component(uid).unwrap_or(ComponentUid::ROOT);
                (sig, c)
            })
            .collect();
        state.iter().any(|(body, sig)| {
            let Some(component) = self.components.known_body(body.component) else {
                return false;
            };
            let same: Vec<ComponentUid> = current
                .iter()
                .filter(|(s, _)| s.same(sig))
                .map(|(_, c)| *c)
                .collect();
            !same.is_empty() && !same.contains(&component)
        })
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
        // Given up by the watchdog while they were built (mitcad#71).
        if self.abandoned() {
            return;
        }
        let kernel = self.doc.kernel();
        let finals: Vec<(StoredBody<K::Shape>, Sig)> = finals
            .into_iter()
            .filter_map(|b| {
                let s = Sig::of(kernel, &b.shape)?;
                Some((b, s))
            })
            .collect();
        // The last history state's sheets (without a history the file's
        // sheets are not told from the tools its blobs keep).
        let sheets: Vec<(StoredBody<K::Shape>, SheetSig)> = match self.oracle.last() {
            Some(q) => self
                .oracle
                .sheets(kernel, q)
                .map(<[_]>::to_vec)
                .unwrap_or_default(),
            None => Vec::new(),
        };
        if finals.is_empty() && sheets.is_empty() {
            return;
        }
        let solids_differ = !history::same_bodies(
            &finals.iter().map(|(_, s)| *s).collect::<Vec<_>>(),
            &self.current_sigs(),
        ) || self.components_differ(&finals);
        let sheets_differ = self.sheets_differ(&sheets);
        if (solids_differ || sheets_differ) && self.options.fallback && !self.at_final {
            let replayed = self.doc.bodies().len();
            match self.replace_bodies(&finals, &sheets, Some("Stored bodies"), "stored design") {
                Ok(Some(uid)) if replayed == 0 => self.report.warnings.push(format!(
                    "nothing replayed: the file's bodies come in as they are ({uid})"
                )),
                Ok(Some(uid)) if !solids_differ => self.report.warnings.push(format!(
                    "{uid} brings in the file's surface bodies as they are stored"
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
        // Over the time limit, stopped or low on memory: volumes only.
        let compare = self.options.compare
            && !self.out_of_time()
            && self.report.stopped.is_none()
            && !self.memory_low();
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
        // Sheets are not compared: they are the file's stored ones.
        let kernel = self.doc.kernel();
        self.report.extra_bodies = self
            .doc
            .bodies()
            .iter()
            .filter(|b| !matched.iter().any(|m| m.starts_with(&b.uid.to_string())))
            .filter(|b| SheetSig::of(kernel, b.shape).is_none())
            .map(|b| format!("{} {}", b.uid, b.name))
            .collect();
    }

    /// Whether the replay's sheets are not the state's `sheets` (each the
    /// same in the same component).
    fn sheets_differ(&self, sheets: &[(StoredBody<K::Shape>, SheetSig)]) -> bool {
        let current = self.current_sheets();
        if current.len() != sheets.len() {
            return true;
        }
        let mut used = vec![false; current.len()];
        !sheets.iter().all(|(body, sig)| {
            let component = self.components.of_body(body.component);
            match (0..current.len())
                .find(|&i| !used[i] && current[i].2 == component && current[i].1.same(sig))
            {
                Some(i) => {
                    used[i] = true;
                    true
                }
                None => false,
            }
        })
    }
}

/// Runs `f` on the document under `monitor` (None: its own).
fn interruptible<K: Kernel, T>(
    doc: &mut Document<K>,
    monitor: Option<&Arc<RecomputeMonitor>>,
    f: impl FnOnce(&mut Document<K>) -> T,
) -> T {
    let Some(monitor) = monitor else {
        return f(doc);
    };
    let before = doc.monitor().cloned();
    doc.set_monitor(Some(Arc::clone(monitor)));
    let result = f(doc);
    doc.set_monitor(before);
    result
}

/// A kernel operation that ran out of memory (`error`, mitcad#80): the
/// process is low on memory.
fn out_of_memory(progress: Option<&Progress>, error: &str) {
    if KernelError::is_out_of_memory(error)
        && let Some(p) = progress
    {
        p.low_memory(&format!(
            "a geometry kernel operation ran out of memory ({error})"
        ));
    }
}

/// Adds a feature from its JSON definition to the document (see
/// [`Importer::add`]), in the component its references give.
fn add<K: Kernel>(
    doc: &mut Document<K>,
    def: &Value,
    name: Option<&str>,
    adding: &Adding<'_>,
) -> Result<FeatureUid, String> {
    let target = def_component(doc, def, adding.component);
    let mut def = def.clone();
    solid_participants(doc, &mut def, target);
    add_in(doc, &def, name, target, adding)
}

/// Adds a feature from its JSON definition to `target` (see
/// [`Importer::add_in`]).
fn add_in<K: Kernel>(
    doc: &mut Document<K>,
    def: &Value,
    name: Option<&str>,
    target: ComponentUid,
    adding: &Adding<'_>,
) -> Result<FeatureUid, String> {
    if let Some(p) = adding.progress {
        p.tick();
    }
    // Items may have no name (the decoder's unknown ones).
    let name = name.filter(|n| !n.trim().is_empty());
    let component = Some(target);
    let def = def.clone();
    let def: FeatureDef<ValueInput> =
        serde_json::from_value(def).map_err(|e| format!("definition: {e}"))?;
    let monitor = adding.monitor.as_ref();
    // (A worker's definition that ran out of memory is evaluated again on
    // the import's thread: the import is told by that one.)
    let progress = adding.progress.filter(|_| adding.memory);
    let added = match interruptible(doc, monitor, |doc| {
        doc.add_feature_to(&def, name, component)
    }) {
        Ok(added) => added,
        // A name Mitcad already uses: take the default one.
        Err(e) if name.is_some() && e.to_string().contains("already named") => {
            interruptible(doc, monitor, |doc| {
                doc.add_feature_to(&def, None, component)
            })
            .map_err(|e| e.to_string())
            .inspect_err(|e| out_of_memory(progress, e))?
        }
        Err(e) => {
            let e = e.to_string();
            out_of_memory(progress, &e);
            return Err(e);
        }
    };
    match doc.status(added.uid).and_then(|s| s.error()) {
        None => Ok(added.uid),
        Some(error) => {
            let error = error.to_owned();
            doc.undo();
            out_of_memory(progress, &error);
            Err(error)
        }
    }
}

/// A join, cut or intersection without participants works on every
/// body of its component, the file's sheets brought in as stored bodies
/// too ([`Importer::replace_bodies`]), while the file's solid operations
/// leave sheets alone: where the component holds sheets, its solids
/// become the participants (mitcad#35).
fn solid_participants<K: Kernel>(doc: &Document<K>, def: &mut Value, component: ComponentUid) {
    let kind = def["type"].as_str().unwrap_or_default();
    let combines = match kind {
        "hole" | "rib" | "web" => true,
        "extrude" | "revolve" | "sweep" | "loft" | "pipe" | "helix" | "coil" | "box"
        | "cylinder" | "sphere" | "torus" => {
            matches!(
                def["operation"].as_str(),
                Some("join" | "cut" | "intersect")
            )
        }
        _ => false,
    };
    if !combines || def.get("participants").is_some() {
        return;
    }
    let kernel = doc.kernel();
    let mut solids = Vec::new();
    let mut sheets = false;
    for b in doc.bodies() {
        if doc.body_component(b.uid).unwrap_or(ComponentUid::ROOT) != component {
            continue;
        }
        if SheetSig::of(kernel, b.shape).is_some() {
            sheets = true;
        } else {
            solids.push(b.uid.to_string());
        }
    }
    if sheets && !solids.is_empty() {
        def["participants"] = serde_json::json!(solids);
    }
}

/// The component a definition goes into: that of the bodies, sketches
/// and construction geometry it names when they agree, else the item's
/// (`component`: a feature works on its component's bodies, and the
/// history may tell another component than the one the item was given).
fn def_component<K: Kernel>(
    doc: &Document<K>,
    def: &Value,
    component: ComponentUid,
) -> ComponentUid {
    // A combine with tools of other components (mitcad#104) goes where its
    // target is.
    if def["type"] == "combine"
        && def
            .get("tool_links")
            .is_some_and(|l| l.as_object().is_some_and(|m| !m.is_empty()))
        && let Some(c) = def["target"]
            .as_str()
            .and_then(|t| t.parse::<BodyUid>().ok())
            .and_then(|b| doc.body_component(b))
    {
        return c;
    }
    let mut found = BTreeSet::new();
    let mut texts = Vec::new();
    strings(def, &mut texts);
    for text in texts {
        if let Ok(body) = text.parse::<BodyUid>() {
            found.extend(doc.body_component(body));
        } else if let Ok(uid) = text.parse::<FeatureUid>()
            && let Some(f) = doc.feature(uid)
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
        _ => component,
    }
}

/// Adds a candidate's features (all or none; see [`Importer::add_all`]).
fn add_all<K: Kernel>(
    doc: &mut Document<K>,
    candidate: &Candidate,
    name: Option<&str>,
    adding: &Adding<'_>,
) -> Result<Vec<FeatureUid>, String> {
    let depth = doc.undo_depth();
    let mut uids: Vec<FeatureUid> = Vec::new();
    let mut named = 0;
    for def in &candidate.defs {
        let mut def = def.clone();
        if !uids.is_empty() {
            substitute(&mut def, &uids);
        }
        // A thread's faces sized before it (`ops.rs`, threads) leave
        // the item's name to the thread.
        let sizing = matches!(
            def["type"].as_str(),
            Some("offset_face" | "cylinder" | "combine")
        ) && candidate.defs.last().is_some_and(|d| d["type"] == "thread");
        let own = match name {
            _ if def["type"] == "sketch" && candidate.defs.len() > 1 => None,
            _ if sizing => None,
            Some(n) if named == 0 => Some(n.to_owned()),
            Some(n) => Some(format!("{n}.{}", named + 1)),
            None => None,
        };
        if own.is_some() {
            named += 1;
        }
        match add(doc, &def, own.as_deref(), adding) {
            Ok(uid) => uids.push(uid),
            Err(e) => {
                while doc.undo_depth() > depth {
                    if doc.undo().is_none() {
                        break;
                    }
                }
                return Err(e);
            }
        }
    }
    Ok(uids)
}

/// The solids of the document at the marker, with their signatures.
fn bodies_of<K: Kernel>(doc: &Document<K>) -> Vec<(BodyUid, Sig)> {
    let kernel = doc.kernel();
    doc.bodies()
        .iter()
        .filter_map(|b| Some((b.uid, Sig::of(kernel, b.shape)?)))
        .collect()
}

#[cfg(test)]
mod tests;
