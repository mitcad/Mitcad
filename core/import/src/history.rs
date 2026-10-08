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
        }
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
        self.geometry.final_bodies()
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
}
