// SPDX-License-Identifier: MIT
//! The bodies stored in the file: the stored design and the states of its ASM
//! history (the `.smbh` blobs rolled back operation by operation, A4b).
//!
//! The importer checks its replay against these states: after a feature,
//! Mitcad's bodies should equal one of the next states (volume, area and
//! centre of every solid). That picks profiles and edges the stream decoder
//! does not identify, and when a feature cannot be replayed, the state
//! after it is the body its fallback base feature holds. A state's sheets
//! (surface bodies) are kept apart: they take no part in the matching and
//! come in with the fallbacks ([`Oracle::sheets`]).

use mitcad_model::{BodyKind, Kernel};

/// A body stored in the file, as a kernel shape (millimetres, in its
/// component's coordinates).
#[derive(Debug, Clone)]
pub struct StoredBody<S> {
    pub shape: S,
    /// The body's name in the file, when known.
    pub name: Option<String>,
    /// Where it comes from (blob and record), for the report.
    pub source: String,
    /// The component's object id (`components[]._f3d.object_id`), when
    /// known.
    pub component: Option<u64>,
    /// The same for the same body in different states, so its
    /// measurements are taken once; None when unknown.
    pub id: Option<u64>,
}

/// The bodies of an .f3d file for the importer. The OCCT implementation
/// reads the `.f3d` (`mitcad-ffi`); tests give their own.
pub trait StoredGeometry<S> {
    /// The number of body states of the ASM history, oldest first; the last
    /// is the stored design. 0 without a history.
    fn state_count(&mut self) -> usize;
    /// The bodies of a state (built on demand).
    fn state(&mut self, index: usize) -> Result<Vec<StoredBody<S>>, String>;
    /// The state the timeline item with this index made, when the history
    /// names it (the item's `_f3d.result_no`).
    fn item_state(&mut self, index: i64) -> Option<usize> {
        let _ = index;
        None
    }
    /// The bodies of the stored design, of every component.
    fn final_bodies(&mut self) -> Result<Vec<StoredBody<S>>, String>;
    /// The components (object ids) whose bodies the timeline item's
    /// operation changed, when the history names it (F6).
    fn item_components(&mut self, index: i64) -> Option<Vec<u64>> {
        let _ = index;
        None
    }
    /// The names of the file's components by object id, for dumps that
    /// name components only (external dumps).
    fn component_names(&mut self) -> std::collections::HashMap<u64, String> {
        std::collections::HashMap::new()
    }
    /// The components (object ids) whose bodies a history blob of their
    /// own holds; None when every component's are (or it is not known).
    fn history_components(&mut self) -> Option<std::collections::HashSet<u64>> {
        None
    }
    /// Drops what was built for the states before `state` alone, to be
    /// built again if they are asked for (mitcad#80: the process runs low
    /// on memory, and the import has gone past them).
    fn release_before(&mut self, state: usize) {
        let _ = state;
    }
    /// Whether the file names a state for the timeline item that its
    /// history does not hold, between states it does: the operation's
    /// states were taken out of the history (a feature suppressed or
    /// failed in the file), so the file keeps no result for it.
    fn item_without_result(&mut self, index: i64) -> bool {
        let _ = index;
        false
    }
}

/// The solids of a stored body of several lumps (disjoint pieces of one
/// body), each to come in as a body of its own, as the replay makes them
/// (Mitcad keeps disjoint solids apart: a join of a piece that touches
/// nothing makes a body of it). Empty for one solid, and when the solids
/// do not hold all of its faces (sheet faces with them).
pub fn lumps<K: Kernel>(kernel: &K, shape: &K::Shape) -> Vec<K::Shape> {
    let solids = kernel.solids(shape).unwrap_or_default();
    let faces = |s: &K::Shape| kernel.face_count(s).unwrap_or(0);
    if solids.len() > 1 && solids.iter().map(faces).sum::<usize>() == faces(shape) {
        solids
    } else {
        Vec::new()
    }
}

/// No bodies: an external dump without its `.f3d`.
pub struct NoGeometry;

impl<S> StoredGeometry<S> for NoGeometry {
    fn state_count(&mut self) -> usize {
        0
    }

    fn state(&mut self, index: usize) -> Result<Vec<StoredBody<S>>, String> {
        Err(format!("there is no history state {index}"))
    }

    fn final_bodies(&mut self) -> Result<Vec<StoredBody<S>>, String> {
        Ok(Vec::new())
    }
}

/// What identifies a solid for matching: volume (mm³), area (mm²), centre
/// of mass and the number of faces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sig {
    pub volume: f64,
    pub area: f64,
    pub center: [f64; 3],
    /// 0 when the kernel does not list faces.
    pub faces: usize,
}

/// Relative tolerance of volumes and areas.
pub(crate) const RELATIVE: f64 = 1e-3;

/// The looser tolerance of features whose shape follows Mitcad's own
/// conventions where the file does not settle them, or Mitcad's own rules
/// that do not give every stored surface exactly (lofts with end conditions or
/// rails): a match within it is kept with a warning.
pub(crate) const CONVENTIONS: f64 = 5e-3;

impl Sig {
    /// The signature of a solid; None for sheets, meshes and empty shapes.
    /// (Kernel calls: the import's loops over many bodies measure them, a
    /// minute and more for large states, so it ticks the watchdog,
    /// mitcad#82.)
    pub fn of<K: Kernel>(kernel: &K, shape: &K::Shape) -> Option<Sig> {
        crate::tick();
        match kernel.body_kind(shape) {
            Ok(BodyKind::Solid) | Err(_) => {}
            Ok(_) => return None,
        }
        let m = kernel.mass_properties(shape).ok()?;
        (m.volume.is_finite() && m.volume > 1e-9).then(|| Sig {
            volume: m.volume,
            area: m.area,
            center: m.center,
            faces: kernel.face_count(shape).unwrap_or(0),
        })
    }

    /// The same solid: the same faces and measures within [`EXACT`].
    pub fn same(&self, other: &Sig) -> bool {
        self.faces == other.faces && self.distance(other) <= EXACT
    }

    /// How far apart two solids are: the largest of the relative volume and
    /// area differences and the centre distance relative to the size.
    pub fn distance(&self, other: &Sig) -> f64 {
        let rel = |a: f64, b: f64| (a - b).abs() / a.abs().max(b.abs()).max(1e-9);
        let size = self.volume.abs().cbrt().max(1e-3);
        let d = (0..3)
            .map(|i| (self.center[i] - other.center[i]).powi(2))
            .sum::<f64>()
            .sqrt();
        rel(self.volume, other.volume)
            .max(rel(self.area, other.area))
            .max(d / size)
    }

    pub fn matches(&self, other: &Sig) -> bool {
        self.distance(other) <= RELATIVE
    }
}

/// What identifies a sheet (a surface body, `isSolid` false): its area
/// (mm²) and centre. Sheets take no part in matching the replay with the
/// history; they come in as stored bodies (see `Importer::replace_bodies`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SheetSig {
    pub area: f64,
    pub center: [f64; 3],
}

impl SheetSig {
    /// The signature of a sheet; None for solids, meshes and empty shapes.
    /// (Ticks the watchdog, as [`Sig::of`] does.)
    pub fn of<K: Kernel>(kernel: &K, shape: &K::Shape) -> Option<SheetSig> {
        crate::tick();
        if kernel.body_kind(shape).ok()? != BodyKind::Sheet {
            return None;
        }
        let m = kernel.mass_properties(shape).ok()?;
        (m.area.is_finite() && m.area > 1e-9).then_some(SheetSig {
            area: m.area,
            center: m.center,
        })
    }

    /// The same sheet: area and centre within [`EXACT`].
    pub fn same(&self, other: &SheetSig) -> bool {
        let size = self.area.max(other.area).sqrt().max(1e-3);
        let d = (0..3)
            .map(|i| (self.center[i] - other.center[i]).powi(2))
            .sum::<f64>()
            .sqrt();
        (self.area - other.area).abs() <= EXACT * self.area.max(other.area) && d <= EXACT * size
    }
}

/// How well two sets of solids agree: the largest distance of a pairing
/// (each body with its nearest unpaired one) and whether every pair is the
/// same solid ([`Sig::same`]); None when they cannot be paired within the
/// tolerance.
pub fn bodies_distance(a: &[Sig], b: &[Sig]) -> Option<(f64, bool)> {
    bodies_distance_within(a, b, RELATIVE)
}

/// [`bodies_distance`] with a relative tolerance of its own.
pub fn bodies_distance_within(a: &[Sig], b: &[Sig], tolerance: f64) -> Option<(f64, bool)> {
    if a.len() != b.len() {
        return None;
    }
    let mut used = vec![false; b.len()];
    let mut worst: f64 = 0.0;
    let mut exact = true;
    for x in a {
        // Prefer a pair with the same faces among equally near ones.
        let (j, d) = (0..b.len())
            .filter(|&j| !used[j])
            .map(|j| (j, x.distance(&b[j])))
            .min_by(|p, q| {
                let key = |(j, d): (usize, f64)| (d > EXACT || x.faces != b[j].faces, d);
                let (a, b) = (key(*p), key(*q));
                a.0.cmp(&b.0).then(a.1.total_cmp(&b.1))
            })?;
        if d > tolerance {
            return None;
        }
        used[j] = true;
        worst = worst.max(d);
        exact &= x.same(&b[j]);
    }
    Some((worst, exact))
}

/// Whether every solid of `part` is in `all` exactly, each once.
pub fn contains_all(all: &[Sig], part: &[Sig]) -> bool {
    let mut used = vec![false; all.len()];
    part.iter().all(
        |s| match (0..all.len()).find(|&j| !used[j] && all[j].same(s)) {
            Some(j) => {
                used[j] = true;
                true
            }
            None => false,
        },
    )
}

/// Whether two sets of solids are the same (a pairing within the
/// tolerance).
pub fn same_bodies(a: &[Sig], b: &[Sig]) -> bool {
    bodies_distance(a, b).is_some()
}

/// Matches this close (with the same faces) are exact: the same geometry
/// built twice. Looser ones (up to [`RELATIVE`]) are approximations, such
/// as fillets that ASM stores as splines; a small fillet changes a body's
/// measures less than that, but not its faces.
pub(crate) const EXACT: f64 = 1e-5;

/// Relative tolerance of the volume an item itself adds or removes against
/// the file's change for it ([`Change`]).
pub(crate) const CHANGE: f64 = 1e-2;

/// Changes further than this from the file's ([`Change::difference`]) are
/// near or beyond [`CHANGE`]: the volumes do not settle them, the faces do
/// (`crate::geometric`, mitcad#138).
pub(crate) const UNSETTLED: f64 = CHANGE / 2.0;

/// Changes further than this from the file's are another result whatever
/// its faces say: the differences the faces let pass (at the corners of a
/// rounding, the file's approximations) came to 2–4 % of the change.
pub(crate) const DIFFERENT: f64 = 10.0 * CHANGE;

/// Volume differences below this part of the bodies' volume are the
/// measures' noise: [`Change`] does not compare changes that small.
pub(crate) const CHANGE_NOISE: f64 = 1e-6;

/// The volume an item adds or removes against the change of the file's
/// history for it (mitcad#121). The bodies' measures are compared with a
/// tolerance relative to the bodies ([`RELATIVE`]), within which a wrong
/// rounding of a large body still fits (one that removes a few per cent
/// more or less than the file's changes the body by less than 1e-5): the
/// change is compared relative to itself as well.
///
/// The bodies before the item may carry a difference from the file's
/// state of earlier approximations, which the item can take away (a
/// mirror or a rounding that replaces the approximated part): its result
/// is then the state's although its change is not the file's. So the
/// item's own difference is the smaller of its change's from the file's
/// and its result's from the state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Change {
    /// The solid volume of the replay's bodies before the item.
    start: f64,
    /// The file's change: the state after the item minus the state the
    /// bodies before it stand for.
    theirs: f64,
    /// The solid volume of the state after the item.
    target: f64,
    /// The largest volume of the four, for the noise floor.
    scale: f64,
}

impl Change {
    /// The change from the state the bodies before the item stand for
    /// (`reference`) to the state after it (`target`); `start` the replay's
    /// bodies before the item. None when `start` does not pair with
    /// `reference` within the tolerance (the bodies stand for no state).
    pub fn new(reference: &[Sig], start: &[Sig], target: &[Sig]) -> Option<Change> {
        bodies_distance(reference, start)?;
        let (r, s, t) = (volume(reference), volume(start), volume(target));
        Some(Change {
            start: s,
            theirs: t - r,
            target: t,
            scale: r.abs().max(s.abs()).max(t.abs()),
        })
    }

    /// How far the item's change to the bodies `after` is from the file's,
    /// relative to the larger of the two changes: the smaller of the
    /// changes' difference and the result's from the state; 0 within the
    /// noise.
    pub fn difference(&self, after: &[Sig]) -> f64 {
        let a = volume(after);
        let ours = a - self.start;
        let off = self.difference_volume(after);
        if off <= CHANGE_NOISE * self.scale.max(a.abs()) {
            return 0.0;
        }
        off / ours.abs().max(self.theirs.abs())
    }

    /// [`Change::difference`] in mm³: the smaller of the changes'
    /// difference and the result's from the state.
    pub fn difference_volume(&self, after: &[Sig]) -> f64 {
        let a = volume(after);
        ((a - self.start) - self.theirs)
            .abs()
            .min((a - self.target).abs())
    }

    /// The replay's change to the bodies `after` and the file's (mm³).
    pub fn volumes(&self, after: &[Sig]) -> (f64, f64) {
        (volume(after) - self.start, self.theirs)
    }

    /// Whether the change to the bodies `after` is the file's within
    /// [`CHANGE`].
    pub fn agrees(&self, after: &[Sig]) -> bool {
        self.difference(after) <= CHANGE
    }
}

/// The item's own difference ([`Change::difference`]) for its result
/// `after` against its state `target`; None without a change to compare
/// or when the result is the state exactly ([`Sig::same`]: the same faces
/// and measures within [`EXACT`], which the measures' noise reaches for
/// small changes of large bodies).
pub fn own_difference(own: Option<Change>, target: &[Sig], after: &[Sig]) -> Option<f64> {
    let own = own?;
    if bodies_distance(target, after).is_some_and(|(_, exact)| exact) {
        return None;
    }
    Some(own.difference(after))
}

/// The solid volume of a set of bodies.
fn volume(sigs: &[Sig]) -> f64 {
    sigs.iter().map(|s| s.volume).sum()
}

/// The solids of a state with their signatures.
type Measured<S> = Vec<(StoredBody<S>, Sig)>;

/// The sheets of a state with their signatures.
pub type Sheets<S> = Vec<(StoredBody<S>, SheetSig)>;

/// The history states with their signatures, built as they are needed.
pub struct Oracle<'g, S> {
    geometry: &'g mut dyn StoredGeometry<S>,
    states: Vec<Option<Measured<S>>>,
    /// The sheets of the states built (mitcad#35).
    sheets: Vec<Sheets<S>>,
    /// Signatures by body id (None: not a solid).
    measured: std::collections::HashMap<u64, Option<Sig>>,
    /// Sheet signatures by body id (None: not a sheet).
    measured_sheets: std::collections::HashMap<u64, Option<SheetSig>>,
    /// The state the replay matches now; None before the first match.
    pub cursor: Option<usize>,
    /// States built so far.
    pub built: usize,
    /// How many states ahead of the cursor a search looks.
    pub window: usize,
    /// The first state a search does not look at (exclusive): set while an
    /// item without a state of its own is replayed, to the state of the
    /// next item that has one, which that item made (mitcad#37).
    pub limit: Option<usize>,
    /// Off when there is no history or it turned out not to fit the
    /// timeline.
    pub enabled: bool,
    /// States that could not be built, with the reason.
    broken: std::collections::HashMap<usize, String>,
    /// States whose bodies were dropped ([`Oracle::release_behind`]): built
    /// again, they do not count as built once more.
    released: std::collections::HashSet<usize>,
    /// Bodies the design no longer has from a state on although the ASM
    /// history keeps them, by body id ([`Oracle::retire`]).
    retired: Vec<(usize, u64)>,
}

impl<'g, S: Clone> Oracle<'g, S> {
    pub fn new(geometry: &'g mut dyn StoredGeometry<S>) -> Self {
        let count = geometry.state_count();
        Self {
            geometry,
            states: (0..count).map(|_| None).collect(),
            sheets: (0..count).map(|_| Vec::new()).collect(),
            measured: std::collections::HashMap::new(),
            measured_sheets: std::collections::HashMap::new(),
            cursor: None,
            built: 0,
            window: 32,
            limit: None,
            enabled: count > 0,
            broken: std::collections::HashMap::new(),
            released: std::collections::HashSet::new(),
            retired: Vec::new(),
        }
    }

    /// Leaves bodies (by [`StoredBody::id`]) out of the states from `from`
    /// on and out of the stored design: a combine's tools that the file
    /// says it consumed stay in the ASM history as they were, but are no
    /// bodies of the design (mitcad#96).
    pub fn retire(&mut self, from: usize, ids: &[u64]) {
        let new: Vec<u64> = ids
            .iter()
            .copied()
            .filter(|id| !self.retired.iter().any(|(f, r)| r == id && *f <= from))
            .collect();
        if new.is_empty() {
            return;
        }
        self.retired.extend(new.iter().map(|&id| (from, id)));
        for state in self.states.iter_mut().skip(from).flatten() {
            state.retain(|(b, _)| b.id.is_none_or(|id| !new.contains(&id)));
        }
    }

    /// Whether a body is retired in a state ([`Oracle::retire`]).
    fn is_retired(&self, state: usize, id: Option<u64>) -> bool {
        id.is_some_and(|id| self.retired.iter().any(|&(f, r)| r == id && f <= state))
    }

    pub fn count(&self) -> usize {
        self.states.len()
    }

    /// The state the timeline item `index` made, if the history names it.
    pub fn state_of_item(&mut self, index: i64) -> Option<usize> {
        self.geometry
            .item_state(index)
            .filter(|&s| s < self.states.len())
    }

    /// How many states asked for could not be built.
    pub fn unbuilt(&self) -> usize {
        self.broken.len()
    }

    /// How many states' bodies are kept (built, not released).
    pub fn kept(&self) -> usize {
        self.states.iter().filter(|s| s.is_some()).count()
    }

    /// Drops the bodies of the states before the replay's (the cursor's),
    /// and what the stored geometry built for them alone; their signatures
    /// stay, and their bodies are built again if they are asked for
    /// (mitcad#80: the process runs low on memory). The number of states
    /// whose bodies were dropped.
    pub fn release_behind(&mut self) -> usize {
        let Some(cursor) = self.cursor else {
            return 0;
        };
        let mut dropped = 0;
        for i in 0..cursor.min(self.states.len()) {
            if self.states[i].take().is_some() {
                self.released.insert(i);
                dropped += 1;
            }
            self.sheets[i] = Vec::new();
        }
        self.geometry.release_before(cursor);
        dropped
    }

    /// The state the timeline item `index` made, if known and not behind
    /// the cursor.
    pub fn item_state(&mut self, index: i64) -> Option<usize> {
        let state = self.geometry.item_state(index)?;
        (state < self.states.len() && self.cursor.is_none_or(|c| state >= c)).then_some(state)
    }

    /// The bodies of a state with their signatures (solids only).
    pub fn state<K: Kernel<Shape = S>>(
        &mut self,
        kernel: &K,
        index: usize,
    ) -> Result<&[(StoredBody<S>, Sig)], String> {
        if index >= self.states.len() {
            return Err(format!("there is no history state {index}"));
        }
        if let Some(e) = self.broken.get(&index) {
            return Err(e.clone());
        }
        if self.states[index].is_none() {
            let bodies = match self.geometry.state(index) {
                Ok(b) => b,
                Err(e) => {
                    self.broken.insert(index, e.clone());
                    return Err(e);
                }
            };
            if !self.released.remove(&index) {
                self.built += 1;
            }
            let measured = &mut self.measured;
            let measured_sheets = &mut self.measured_sheets;
            let mut solids = Vec::new();
            let mut sheets = Vec::new();
            for b in bodies {
                if self
                    .retired
                    .iter()
                    .any(|&(f, r)| b.id == Some(r) && f <= index)
                {
                    continue;
                }
                let sig = match b.id {
                    Some(id) => *measured
                        .entry(id)
                        .or_insert_with(|| Sig::of(kernel, &b.shape)),
                    None => Sig::of(kernel, &b.shape),
                };
                if let Some(s) = sig {
                    solids.push((b, s));
                    continue;
                }
                let sheet = match b.id {
                    Some(id) => *measured_sheets
                        .entry(id)
                        .or_insert_with(|| SheetSig::of(kernel, &b.shape)),
                    None => SheetSig::of(kernel, &b.shape),
                };
                if let Some(s) = sheet {
                    sheets.push((b, s));
                }
            }
            self.states[index] = Some(solids);
            self.sheets[index] = sheets;
        }
        Ok(self.states[index].as_deref().expect("built"))
    }

    /// The sheets of a state (built as [`Oracle::state`] builds it).
    pub fn sheets<K: Kernel<Shape = S>>(
        &mut self,
        kernel: &K,
        index: usize,
    ) -> Result<&[(StoredBody<S>, SheetSig)], String> {
        self.state(kernel, index)?;
        Ok(&self.sheets[index])
    }

    /// The signatures of a state's solids; none when it cannot be built.
    pub fn sigs<K: Kernel<Shape = S>>(&mut self, kernel: &K, index: usize) -> Vec<Sig> {
        self.try_sigs(kernel, index).unwrap_or_default()
    }

    /// The signatures of a state's solids; None when the state cannot be
    /// rebuilt (bodies the conversion of the rolled-back ASM data loses).
    pub fn try_sigs<K: Kernel<Shape = S>>(&mut self, kernel: &K, index: usize) -> Option<Vec<Sig>> {
        self.state(kernel, index)
            .ok()
            .map(|s| s.iter().map(|(_, sig)| *sig).collect())
    }

    /// The state in `from..` (within the window) whose solids are
    /// `current`: the first exact match, else the closest approximate one
    /// (the earliest of equals).
    pub fn find<K: Kernel<Shape = S>>(
        &mut self,
        kernel: &K,
        from: usize,
        current: &[Sig],
    ) -> Option<usize> {
        self.find_with(kernel, from, current, false)
    }

    /// [`Oracle::find`], with only exact matches when `exact` (when an item
    /// before is still unresolved, an approximate match may hide what it
    /// changed, such as a small fillet).
    pub fn find_with<K: Kernel<Shape = S>>(
        &mut self,
        kernel: &K,
        from: usize,
        current: &[Sig],
        exact: bool,
    ) -> Option<usize> {
        self.find_within(kernel, from, current, exact, RELATIVE)
            .map(|(i, _)| i)
    }

    /// [`Oracle::find_with`] within a relative tolerance of its own; the
    /// state and its distance.
    pub fn find_within<K: Kernel<Shape = S>>(
        &mut self,
        kernel: &K,
        from: usize,
        current: &[Sig],
        exact: bool,
        tolerance: f64,
    ) -> Option<(usize, f64)> {
        let end = self
            .states
            .len()
            .min(from.saturating_add(self.window))
            .min(self.limit.unwrap_or(usize::MAX));
        let mut best: Option<(f64, usize)> = None;
        for i in from..end {
            crate::tick();
            let Some(state) = self.try_sigs(kernel, i) else {
                continue;
            };
            let Some((d, same)) = bodies_distance_within(&state, current, tolerance) else {
                continue;
            };
            if same {
                return Some((i, d));
            }
            if !exact && best.is_none_or(|(b, _)| d < b) {
                best = Some((d, i));
            }
        }
        best.map(|(d, i)| (i, d))
    }

    /// The first state in `from..from + span` that holds every solid of
    /// `current` exactly ([`Sig::same`]) and more: what is left came from
    /// items still unresolved (such as bodies an undecoded item added in
    /// the same step).
    pub fn find_superset<K: Kernel<Shape = S>>(
        &mut self,
        kernel: &K,
        from: usize,
        span: usize,
        current: &[Sig],
    ) -> Option<usize> {
        let end = self
            .states
            .len()
            .min(from.saturating_add(span))
            .min(self.limit.unwrap_or(usize::MAX));
        for i in from..end {
            let state = self.sigs(kernel, i);
            if state.len() > current.len() && contains_all(&state, current) {
                return Some(i);
            }
        }
        None
    }

    /// The first state the replay can be at next: after the cursor, or
    /// from the start before the first match.
    pub fn next_index(&self) -> usize {
        self.cursor.map_or(0, |c| c + 1)
    }

    pub fn last(&self) -> Option<usize> {
        self.states.len().checked_sub(1)
    }

    pub fn final_bodies(&mut self) -> Result<Vec<StoredBody<S>>, String> {
        let mut bodies = self.geometry.final_bodies()?;
        bodies.retain(|b| !self.is_retired(usize::MAX, b.id));
        Ok(bodies)
    }

    /// See [`StoredGeometry::item_without_result`].
    pub fn item_without_result(&mut self, index: i64) -> bool {
        self.geometry.item_without_result(index)
    }

    /// See [`StoredGeometry::item_components`].
    pub fn item_components(&mut self, index: i64) -> Option<Vec<u64>> {
        self.geometry.item_components(index)
    }

    /// See [`StoredGeometry::history_components`].
    pub fn history_components(&mut self) -> Option<std::collections::HashSet<u64>> {
        self.geometry.history_components()
    }

    /// See [`StoredGeometry::component_names`].
    pub fn component_names(&mut self) -> std::collections::HashMap<u64, String> {
        self.geometry.component_names()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(volume: f64) -> Sig {
        Sig {
            volume,
            area: volume / 2.0,
            center: [1.0, 2.0, 3.0],
            faces: 6,
        }
    }

    #[test]
    fn a_body_of_disjoint_solids_comes_in_as_its_lumps() {
        use mitcad_model::testing::{MockKernel, MockShape};
        let kernel = MockKernel::default();
        let (a, b) = (MockShape::imported("a", 6), MockShape::imported("b", 4));
        let two = kernel.compound(&[a.clone(), b.clone()]).unwrap();
        assert_eq!(lumps(&kernel, &two), [a.clone(), b.clone()]);
        // One solid: no lumps.
        assert!(lumps(&kernel, &a).is_empty());
        // Solids that leave faces out (sheet faces with them): kept whole.
        let mut mixed = two.clone();
        mixed.faces.extend(MockShape::imported("c", 1).faces);
        assert!(lumps(&kernel, &mixed).is_empty());
    }

    #[test]
    fn body_sets_match_in_any_order_within_tolerance() {
        assert!(same_bodies(
            &[sig(10.0), sig(20.0)],
            &[sig(20.0), sig(10.0001)]
        ));
        assert!(!same_bodies(&[sig(10.0), sig(20.0)], &[sig(20.0)]));
        assert!(!same_bodies(
            &[sig(10.0), sig(10.0)],
            &[sig(10.0), sig(20.0)]
        ));
        assert!(!same_bodies(&[sig(10.0)], &[sig(10.5)]));
        let mut moved = sig(10.0);
        moved.center[0] += 1.0;
        assert!(!same_bodies(&[sig(10.0)], &[moved]));
        assert!(same_bodies(&[], &[]));
    }

    #[test]
    fn an_item_s_change_is_compared_relative_to_itself() {
        // A rounding of a large body: the file's removes 1.0, the replay's
        // 1.06; the bodies agree within 6e-6 of their volume.
        let (before, file) = (sig(1e4), sig(1e4 - 1.0));
        let change = Change::new(&[before], &[before], &[file]).expect("paired");
        let wrong = sig(1e4 - 1.06);
        assert!(same_bodies(&[file], &[wrong]));
        assert!((change.difference(&[wrong]) - 0.06 / 1.06).abs() < 1e-9);
        assert!(!change.agrees(&[wrong]));
        assert!(change.agrees(&[sig(1e4 - 1.005)]));
        // Bodies before the item that carry an earlier approximation: what
        // the item adds to it counts, not the bodies' difference; or the
        // item takes it away and its result is the state's.
        let carried = sig(1e4 + 0.5);
        let change = Change::new(&[before], &[carried], &[file]).expect("paired");
        assert!(change.agrees(&[sig(1e4 - 0.5)]));
        assert!(change.agrees(&[sig(1e4 - 1.0)]));
        assert!(!change.agrees(&[sig(1e4 - 0.8)]));
        assert!(!change.agrees(&[sig(1e4 - 1.2)]));
        // Changes within the measures' noise are not compared.
        let change = Change::new(&[before], &[before], &[before]).expect("paired");
        assert_eq!(change.difference(&[sig(1e4 + 0.005)]), 0.0);
        assert!(!change.agrees(&[sig(1e4 + 0.02)]));
        // Bodies that stand for another state: no comparison.
        assert!(Change::new(&[before], &[sig(2e5)], &[file]).is_none());
        // From no bodies: the whole new body is the change.
        let change = Change::new(&[], &[], &[file]).expect("paired");
        assert!(change.agrees(&[sig(1e4 - 1.0001)]));
        // A result that is the state exactly is not compared; one with other
        // faces is.
        let change = Change::new(&[before], &[before], &[file]);
        assert_eq!(own_difference(change, &[file], &[sig(1e4 - 1.05)]), None);
        let mut faces = sig(1e4 - 1.05);
        faces.faces += 2;
        assert!(own_difference(change, &[file], &[faces]).is_some_and(|d| d > CHANGE));
        assert_eq!(own_difference(None, &[file], &[faces]), None);
    }
}
