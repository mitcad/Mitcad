// SPDX-License-Identifier: MIT
//! Feature inputs found in the ASM history.
//!
//! The streams name a feature's face, edge and body inputs by the names the
//! bodies carry ([`super::recipe`], [`mitcad_f3d_names`]). The entities exist in
//! the history state before the item: the `.smbh` blobs rolled back to just
//! before the item's own state (its `_f3d.result_no`; for an item
//! suppressed in the file, which has none, the state after the last item
//! of its component before it: [`starts`], mitcad#180). There they are found
//! by their names (some inputs name faces as they were a few states
//! earlier: those states are searched next, up to [`EARLIER`] back), and
//! their geometry goes into the fingerprints, as an
//! external dump gives it (cm): edges get `mid_point`, `length`,
//! `start_point` and `end_point` (and `_f3d.direction`, their own
//! direction at the middle); a thread's cylindrical faces their
//! `geometry` (`Cylinder`: origin, axis, radius) and `point_on_face`. A
//! fillet's or chamfer's face selections become the edges of the faces
//! (mitcad#96). `_f3d.found` says where the entity was found or why not.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use mitcad_asm::asm::AsmFile;
use mitcad_asm::asm::history::History;
use mitcad_f3d_file::container::F3dFile;
use mitcad_f3d_names::{EdgeGeometry, NamedState};

use serde_json::{Map, Value};

use super::ir::{Detail, Dump, EntityName, Fingerprint, Geometry, Mat4, Reference, TimelineItem};

/// A history blob of the document.
struct Blob {
    entry: String,
    file: AsmFile,
    history: History,
}

/// The history's blobs, read when an item first needs them (by one
/// thread; the others wait for them).
#[derive(Default)]
struct Blobs(std::sync::OnceLock<Vec<Blob>>);

impl Blobs {
    fn get(&self, doc: &F3dFile) -> &[Blob] {
        self.0.get_or_init(|| history_blobs(doc))
    }
}

// History states named once (mitcad#246).

/// The history's blobs with their named states, each (blob, state) named
/// once while it is kept and shared by every item (and thread) that looks
/// in it: the items before and after one look in the same states (an item's
/// earlier states are the states before the items before it), and an item
/// looks in its own state for its bodies, its faces and its edges. A state
/// is a function of the blob and the position alone, so what is found does
/// not depend on what is kept. The states last used are kept within
/// [`KEPT_STATES`] (naming every state at once ran a large design out of
/// memory, mitcad#132).
struct States<'a> {
    blobs: &'a Blobs,
    doc: &'a F3dFile,
    kept: Mutex<Kept<'a>>,
    /// [`KEPT_STATES`].
    budget: usize,
    /// The states whose whole circles [`circle_seams`] reads after the
    /// items ([`seam_states`]), and those circles, taken when the state is
    /// named (they are few; the state itself is not kept that long).
    circle_states: OnceLock<std::collections::HashSet<(usize, usize)>>,
    circles: Mutex<HashMap<(usize, usize), Circles>>,
}

/// A state's whole circles ([`closed_circles`]).
type Circles = Arc<Vec<ClosedCircle>>;

/// How large the named states [`States`] keeps may be in all
/// ([`NamedState::size`]; with the one just named): two or three states of
/// a design whose bodies have 38 000 faces (each counts about 160 000 to
/// 220 000, some 30 MB), for an item's own state and the next items'.
///
/// More is not worth its memory: an item that does not find an input looks
/// up to [`EARLIER`] states back, the states of the items before it, so
/// naming each state of that design once took keeping about twelve (155
/// namings of its 135 states instead of 184 with this budget, 234 before
/// mitcad#246), and with threads finding the inputs runs beside the
/// conversion of the bodies, where the import's memory peaks: twelve
/// states raised that peak from 3.6 to 4.1 GB. Keeping all took 5.7 GB.
const KEPT_STATES: usize = 500_000;

/// A named state, built once by the first who asks for it (the others
/// wait for it).
type Shared<'a> = Arc<OnceLock<Arc<NamedState<'a>>>>;

#[derive(Default)]
struct Kept<'a> {
    /// By (blob, `k` of `view(k)`): the state, when it was last asked for
    /// (by `clock`), and its size once it is named (0 before).
    states: HashMap<(usize, usize), (Shared<'a>, u64, usize)>,
    clock: u64,
    size: usize,
}

impl<'a> States<'a> {
    fn new(blobs: &'a Blobs, doc: &'a F3dFile) -> Self {
        States {
            blobs,
            doc,
            kept: Mutex::new(Kept::default()),
            budget: KEPT_STATES,
            circle_states: OnceLock::new(),
            circles: Mutex::new(HashMap::new()),
        }
    }

    /// The whole circles among the edges of blob `b` with `k` states rolled
    /// back ([`closed_circles`]).
    fn closed_circles(&self, b: usize, k: usize) -> Circles {
        let kept = || {
            self.circles
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&(b, k))
                .cloned()
        };
        if let Some(circles) = kept() {
            return circles;
        }
        let state = self.get(b, k);
        // (Kept when it was named just now.)
        kept().unwrap_or_else(|| Arc::new(closed_circles(&state)))
    }

    /// Keeps the whole circles of a state just named, when [`circle_seams`]
    /// will read them.
    fn keep_circles(&self, b: usize, k: usize, state: &NamedState) {
        if !self
            .circle_states
            .get()
            .is_some_and(|wanted| wanted.contains(&(b, k)))
        {
            return;
        }
        let circles = Arc::new(closed_circles(state));
        self.circles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert((b, k), circles);
    }

    fn blobs(&self) -> &'a [Blob] {
        self.blobs.get(self.doc)
    }

    fn kept(&self) -> std::sync::MutexGuard<'_, Kept<'a>> {
        self.kept
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Blob `b` with its newest `k` states rolled back (`view(k)`), named.
    fn get(&self, b: usize, k: usize) -> Arc<NamedState<'a>> {
        let shared = {
            let mut kept = self.kept();
            kept.clock += 1;
            let clock = kept.clock;
            let entry = kept
                .states
                .entry((b, k))
                .or_insert_with(|| (Shared::default(), 0, 0));
            entry.1 = clock;
            entry.0.clone()
        };
        let state = shared
            .get_or_init(|| {
                let blob = &self.blobs()[b];
                let state = NamedState::new(&blob.file, Some(blob.history.view(k)));
                self.keep_circles(b, k, &state);
                Arc::new(state)
            })
            .clone();
        let mut kept = self.kept();
        let size = state.size();
        let new = match kept.states.get_mut(&(b, k)) {
            Some(entry) if entry.2 == 0 && Arc::ptr_eq(&entry.0, &shared) => {
                entry.2 = size;
                true
            }
            _ => false,
        };
        if new {
            kept.size += size;
        }
        // The states used longest ago go first, until the rest fit.
        while kept.size > self.budget {
            let oldest = kept
                .states
                .iter()
                .filter(|(key, entry)| **key != (b, k) && entry.2 > 0)
                .min_by_key(|(_, entry)| entry.1)
                .map(|(key, _)| *key);
            let Some(key) = oldest else { break };
            if let Some((_, _, size)) = kept.states.remove(&key) {
                kept.size -= size;
            }
        }
        state
    }
}

// Inputs on other components' bodies (mitcad#161).

/// A timeline item's ASM state and owner, for finding inputs in other
/// components' histories.
#[derive(Clone, Copy, Debug, Default)]
struct Point {
    /// Its `_f3d.result_no`, when it has a state.
    result: Option<i64>,
    /// The component that owns it (`_f3d.component`).
    owner: Option<u64>,
}

/// The timeline's items as [`Point`]s, in order, and the history blobs'
/// components (worked out once a lookup needs them).
struct Timeline {
    points: Vec<Point>,
    links: std::sync::OnceLock<Vec<Option<u64>>>,
}

impl Timeline {
    fn new(items: &[TimelineItem]) -> Self {
        let points = items
            .iter()
            .map(|item| {
                let f = item.f3d.as_ref();
                Point {
                    result: f
                        .and_then(|f| f.result_no)
                        .filter(|&r| r > 0)
                        .map(i64::from),
                    owner: f.and_then(|f| f.component),
                }
            })
            .collect();
        Timeline {
            points,
            links: std::sync::OnceLock::new(),
        }
    }

    /// The component (object id) of each history blob: the one whose
    /// items name most of the blob's operations, where that blob is also
    /// the one whose operations its items name most (the numbers count
    /// per component, so components' items name other blobs' numbers too).
    fn links(&self, blobs: &[Blob]) -> &[Option<u64>] {
        self.links.get_or_init(|| {
            let counts: Vec<HashMap<u64, usize>> = blobs
                .iter()
                .map(|b| {
                    let ops: std::collections::HashSet<i64> =
                        b.history.states.iter().map(|s| s.id).collect();
                    let mut count: HashMap<u64, usize> = HashMap::new();
                    for p in &self.points {
                        if let (Some(r), Some(o)) = (p.result, p.owner)
                            && ops.contains(&r)
                        {
                            *count.entry(o).or_default() += 1;
                        }
                    }
                    count
                })
                .collect();
            let best = |count: &HashMap<u64, usize>| {
                let most = count.values().copied().max()?;
                let mut top = count.iter().filter(|(_, n)| **n == most).map(|(c, _)| *c);
                match (top.next(), top.next()) {
                    (Some(c), None) => Some((c, most)),
                    _ => None,
                }
            };
            counts
                .iter()
                .enumerate()
                .map(|(b, count)| {
                    let (c, n) = best(count)?;
                    // No other blob whose operations the component's items
                    // name as often.
                    let rival = counts
                        .iter()
                        .enumerate()
                        .any(|(o, other)| o != b && other.get(&c).is_some_and(|&m| m >= n));
                    (!rival).then_some(c)
                })
                .collect()
        })
    }

    /// The state of blob `b` at the item at `position` of the timeline:
    /// its position in the blob's states (newest first; the state after
    /// it is `view(k)`) of the last item before it whose operation the
    /// blob holds, of the blob's component. None when the blob has no
    /// component or none of its items comes before.
    fn state_at(&self, blobs: &[Blob], b: usize, position: usize) -> Option<usize> {
        let component = self.links(blobs).get(b).copied().flatten()?;
        let blob = &blobs[b];
        self.points[..position.min(self.points.len())]
            .iter()
            .rev()
            .filter(|p| p.owner == Some(component))
            .find_map(|p| {
                let r = p.result?;
                blob.history.states.iter().position(|s| s.id == r)
            })
    }
}

// Inputs of items suppressed in the file (mitcad#180).

/// Where an item's inputs are looked for in its own component's history:
/// per blob that holds its start, the blob's index and the position `k`
/// (states newest first) whose `view(k)` holds the bodies just before
/// the item. That is the state before the item's own (`result`); for an
/// item suppressed in the file, which the file keeps no result for, the
/// state after the last item of its component before it
/// ([`Timeline::state_at`]): the bodies it would change if unsuppressed.
fn starts(at: &At, blobs: &[Blob], result: Option<i32>) -> Vec<(usize, usize)> {
    match result.filter(|&r| r > 0) {
        Some(r) => blobs
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                let pos = b.history.states.iter().position(|s| s.id == i64::from(r))?;
                Some((i, pos + 1))
            })
            .collect(),
        None if at.suppressed => {
            let Some(owner) = at.timeline.points.get(at.position).and_then(|p| p.owner) else {
                return Vec::new();
            };
            let links = at.timeline.links(blobs);
            (0..blobs.len())
                .filter(|&b| links.get(b).copied().flatten() == Some(owner))
                .filter_map(|b| Some((b, at.timeline.state_at(blobs, b, at.position)?)))
                .collect()
        }
        None => Vec::new(),
    }
}

/// What the found inputs' notes say the state is ([`starts`]): `state r`,
/// or for an item suppressed in the file `item n`.
fn start_label(at: &At, result: Option<i32>) -> String {
    match result.filter(|&r| r > 0) {
        Some(r) => format!("state {r}"),
        None => format!("item {} (suppressed in the file)", at.position),
    }
}

/// Why an item's inputs are not looked for when [`starts`] gives no
/// blob.
fn no_start(at: &At, result: Option<i32>) -> String {
    match result.filter(|&r| r > 0) {
        Some(r) => format!("no ASM history holds state {r}"),
        None if at.suppressed => {
            "suppressed in the file: no ASM history of its component holds a state before it"
                .to_owned()
        }
        None => "the item has no ASM state".to_owned(),
    }
}

/// An item's own result number.
fn input_result(item: &TimelineItem) -> Option<i32> {
    item.f3d.as_ref().and_then(|f| f.result_no)
}

/// The file keeps no result for the item: it is suppressed, or after the
/// timeline marker (rolled back; its result number is -1 or one
/// the history no longer holds). Its inputs are looked for in the state
/// after the last item before it.
fn no_result_state(item: &TimelineItem) -> bool {
    item.file_state() == super::decode::ItemState::Suppressed || item.is_rolled_back == Some(true)
}

/// [`input_result`], or -1 for an item the file keeps no result for.
fn result_of(item: &TimelineItem, at: &At) -> Option<i32> {
    input_result(item).map(|r| if at.suppressed { -1 } else { r })
}

/// How many inputs were found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Resolved {
    pub found: usize,
    pub not_found: usize,
}

/// The edge and face fingerprints of an item's detail that carry names and
/// no geometry yet: fillet and chamfer edges, a thread's faces, the face an
/// extrusion goes up to.
fn named_entities(detail: &mut Detail) -> Vec<&mut Fingerprint> {
    // (A fillet's or chamfer's faces left after [`expand_faces`] were not
    // found.) A thread's faces and an extrusion's object are faces.
    let faces = matches!(
        detail,
        Detail::Thread(_)
            | Detail::Extrude(_)
            | Detail::Shell(_)
            | Detail::OffsetFaces(_)
            | Detail::Hole(_)
            | Detail::Revolve(_)
            | Detail::Primitive(_)
            | Detail::Coil(_)
            | Detail::Emboss(_)
            | Detail::MoveFace(_)
            | Detail::Draft(_)
    );
    let refs: Vec<&mut Reference> = match detail {
        Detail::Fillet(f) => f
            .edge_sets
            .iter_mut()
            .flatten()
            .flat_map(|s| s.edges.iter_mut().flatten())
            .collect(),
        Detail::Chamfer(c) => c
            .edge_sets
            .iter_mut()
            .flatten()
            .flat_map(|s| s.edges.iter_mut().flatten())
            .collect(),
        Detail::Thread(t) => t.input_cylindrical_faces.iter_mut().flatten().collect(),
        // The face an extrusion's extent goes up to (mitcad#96), the face
        // it starts on (mitcad#641), and the faces its profile selects
        // (mitcad#169).
        Detail::Extrude(x) => [&mut x.extent_one, &mut x.extent_two, &mut x.start_extent]
            .into_iter()
            .flatten()
            .filter_map(|d| d.entity.as_mut())
            .chain(x.profile.iter_mut().flatten())
            .collect(),
        // A shell's removed faces and offset faces (mitcad#154).
        Detail::Shell(s) => s.input_entities.iter_mut().flatten().collect(),
        Detail::OffsetFaces(o) => o.input_faces.iter_mut().flatten().collect(),
        // A revolution's axis face or edge.
        Detail::Revolve(r) => r.axis.iter_mut().collect(),
        // The face a primitive or a coil stands on (mitcad#25).
        Detail::Primitive(p) => p.plane.iter_mut().collect(),
        Detail::Coil(c) => c.plane.iter_mut().collect(),
        // The face an emboss lies on (mitcad#458).
        Detail::Emboss(e) => e.input_faces.iter_mut().flatten().collect(),
        // The faces a move face moves (mitcad#343).
        Detail::MoveFace(m) => m.faces.iter_mut().flatten().collect(),
        // A draft's faces, its neutral face and a parting tool's faces
        // (mitcad#1363).
        Detail::Draft(d) => {
            let d = &mut **d;
            d.input_faces
                .iter_mut()
                .flatten()
                .chain(d.plane.iter_mut())
                .chain(d.neutral_faces.iter_mut().flatten())
                .chain(d.parting_tool.iter_mut().flatten())
                .collect()
        }
        // A hole's position: its face and the two edges it is measured
        // from.
        Detail::Hole(h) => h
            .hole_position_definition
            .iter_mut()
            .flat_map(|p| [&mut p.planar_entity, &mut p.edge_one, &mut p.edge_two])
            .flatten()
            .collect(),
        _ => Vec::new(),
    };
    refs.into_iter()
        .filter_map(|r| match r {
            Reference::Edge(fp) if fp.mid_point.is_none() && fp.f3d.is_some() => Some(&mut **fp),
            Reference::Face(fp) if faces && fp.point_on_face.is_none() && fp.f3d.is_some() => {
                Some(&mut **fp)
            }
            _ => None,
        })
        .collect()
}

/// Writes the geometry of face `i` of a history state into a face
/// fingerprint (cm).
pub(crate) fn fill_face(state: &NamedState, i: usize, fp: &mut Fingerprint) -> Result<(), String> {
    let cm = |p: [f64; 3]| p.map(|x| x / 10.0);
    // A planar face (a mirror plane, a split tool; mitcad#67).
    if let (Err(_), Ok(p)) = (state.cylinder(i), state.plane(i)) {
        fp.geometry = Some(Geometry {
            geometry_type: Some("Plane".to_owned()),
            origin: Some(cm(p.point)),
            normal: Some(p.normal),
            ..Geometry::default()
        });
        fp.point_on_face = Some(cm(p.point));
        fp.normal_at_point = Some(p.normal);
        return Ok(());
    }
    let g = match state.cylinder(i) {
        Ok(g) => g,
        // Any other surface (mitcad#161: a thread's flank, a spline).
        Err(_) => {
            let s = state.surface_point(i)?;
            fp.geometry = Some(Geometry {
                geometry_type: Some(s.kind.to_owned()),
                origin: s.origin.map(cm),
                axis: s.axis,
                radius: s.radius.map(|r| r / 10.0),
                major_radius: s.minor.and(s.radius).map(|r| r / 10.0),
                minor_radius: s.minor.map(|r| r / 10.0),
                half_angle: s.half_angle,
                ..Geometry::default()
            });
            if s.minor.is_some() {
                // A torus has major and minor radii only.
                if let Some(g) = fp.geometry.as_mut() {
                    g.radius = None;
                }
            }
            fp.point_on_face = Some(cm(s.point));
            fp.normal_at_point = Some(s.normal);
            if !s.spread.is_empty()
                && let Some(f) = fp.f3d.as_mut()
            {
                f.face_points = Some(s.spread.into_iter().map(cm).collect());
            }
            return Ok(());
        }
    };
    fp.geometry = Some(Geometry {
        geometry_type: Some("Cylinder".to_owned()),
        origin: Some(cm(g.origin)),
        axis: Some(g.axis),
        radius: Some(g.radius / 10.0),
        ..Geometry::default()
    });
    fp.point_on_face = Some(cm(g.point));
    fp.normal_at_point = Some(g.normal);
    // Where a thread's helix starts round a whole cylinder.
    if g.full
        && let Some(f) = fp.f3d.as_mut()
    {
        f.seam = Some(g.ref_dir);
    }
    Ok(())
}

/// Finds a named edge or face in a history state and writes its geometry
/// into its fingerprint (cm).
///
/// `twin` (k, n): the edge input is the k-th of n of the item with the
/// same names; where exactly n edges fit them, it is the k-th of those
/// (mitcad#96: a set of n edges between the same two faces, such as
/// the edges where a patterned boss meets a plate, names each the same
/// way). `pieces`: a fillet's or chamfer's input may be several edges
/// ([`pick_edge`]). Returns how an edge was picked and the edges found (the
/// input's first; none for a face or vertex).
fn locate(
    state: &NamedState,
    fp: &mut Fingerprint,
    entities: &[Vec<EntityName>],
    twin: (usize, usize),
    surface: bool,
    pieces: bool,
) -> Result<(&'static str, Vec<usize>), String> {
    let cm = |p: [f64; 3]| p.map(|x| x / 10.0);
    if fp.object_type.as_deref() == Some("BRepFace") {
        let i = if surface {
            state.find_face_surface(entities)?
        } else {
            state.find_face(entities)?
        };
        fill_face(state, i, fp)?;
        return Ok(("", Vec::new()));
    }
    // A vertex (a plane or point through vertices): where the faces named
    // meet.
    if fp.object_type.as_deref() == Some("BRepVertex") {
        let (vertex, body) = state.find_vertex(entities)?;
        fp.point = Some(cm(state.vertex(vertex, body)?));
        return Ok(("", Vec::new()));
    }
    // The faces round the edge's ends in the order the recipe lists them.
    let tail = fp
        .f3d
        .as_ref()
        .and_then(|f| f.end_faces.as_ref())
        .map(|fans| super::recipe::EdgeTail {
            faces: [0, 1],
            fans: fans
                .iter()
                .map(|fan| {
                    fan.iter()
                        .filter_map(|&i| usize::try_from(i).ok())
                        .collect()
                })
                .collect(),
        });
    let curves = stored_curves(fp);
    let picked = pick_edge(state, entities, tail.as_ref(), twin, &curves, pieces)?;
    let g = state.edge(picked.edge)?;
    // The other edges the input is ([`by_stored_pieces`]), kept in the
    // fingerprint until [`split_pieces`] makes each an input of its own.
    let mut more = Vec::new();
    for &e in &picked.pieces {
        let g = state.edge(e)?;
        let mut piece = Fingerprint {
            object_type: fp.object_type.clone(),
            f3d: fp.f3d.clone(),
            ..Fingerprint::default()
        };
        edge_geometry(&mut piece, &g);
        note_curve_fit(&mut piece, &curves, &g);
        more.push(serde_json::to_value(&piece).map_err(|e| e.to_string())?);
    }
    edge_geometry(fp, &g);
    note_curve_fit(fp, &curves, &g);
    if !more.is_empty()
        && let Some(f) = fp.f3d.as_mut()
    {
        f.other.insert(PIECES.to_owned(), Value::Array(more));
    }
    let mut edges = vec![picked.edge];
    edges.extend(picked.pieces);
    Ok((picked.how, edges))
}

/// The key under which [`locate`] keeps the other edges an input is until
/// [`split_pieces`] takes them out (never in a dump written).
const PIECES: &str = "_pieces";

/// The edge inputs of a fillet or chamfer that are several edges
/// ([`by_stored_pieces`]) made one input per edge, each in the input's place
/// in its set and with its `found`: the import matches each edge on its own,
/// as it does the edges of a face selection ([`expand_faces`]).
fn split_pieces(detail: &mut Detail, out: &mut Resolved) {
    let sets = match detail {
        Detail::Fillet(f) => f.edge_sets.as_mut(),
        Detail::Chamfer(c) => c.edge_sets.as_mut(),
        _ => None,
    };
    // An edge by its place (the middle and length, cm).
    let key = |fp: &Fingerprint| {
        let r = |x: f64| (x * 1e6).round() as i64;
        Some((fp.mid_point?.map(r), r(fp.length?)))
    };
    for s in sets.into_iter().flatten() {
        let Some(refs) = s.edges.take() else { continue };
        // The edges the set has: a piece another input of the set found
        // (inputs whose stored curves overlap, the pieces of one ring) is
        // there once.
        let mut have: std::collections::HashSet<_> = refs
            .iter()
            .filter_map(|r| match r {
                Reference::Edge(fp) => key(fp),
                _ => None,
            })
            .collect();
        let mut edges = Vec::with_capacity(refs.len());
        for mut r in refs {
            let more = match &mut r {
                Reference::Edge(fp) => fp.f3d.as_mut().and_then(|f| f.other.remove(PIECES)),
                _ => None,
            };
            let found = match &r {
                Reference::Edge(fp) => fp.f3d.as_ref().and_then(|f| f.found.clone()),
                _ => None,
            };
            edges.push(r);
            let Some(Value::Array(more)) = more else {
                continue;
            };
            for v in more {
                let Value::Object(map) = v else { continue };
                let mut piece = Fingerprint::from_map(map);
                if key(&piece).is_some_and(|k| !have.insert(k)) {
                    continue;
                }
                if let Some(f) = piece.f3d.as_mut() {
                    f.found.clone_from(&found);
                }
                out.found += 1;
                edges.push(Reference::Edge(Box::new(piece)));
            }
        }
        s.edges = Some(edges);
    }
}

// The curves edge inputs store (mitcad#306).

/// How far (mm) an edge may lie from the curve its input stores and still
/// be on it: the import's default geometric tolerance (mitcad#224).
const CURVE_TOLERANCE: f64 = 0.01;

/// The curves an edge input stores ([`super::curve`]), in millimetres
/// (none when it stores none, or one that does not decode).
fn stored_curves(fp: &Fingerprint) -> Vec<super::curve::StoredCurve> {
    let Some(list) = fp.f3d.as_ref().and_then(|f| f.curves.as_ref()) else {
        return Vec::new();
    };
    list.iter()
        .map(|g| super::curve::StoredCurve::from_geometry(g).map(|c| c.scaled(10.0)))
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default()
}

/// How an edge lies to the best fitting of an input's stored curves: its
/// fit and the curve's index. `None` without curves, or when no fit can be
/// told.
fn best_fit(
    curves: &[super::curve::StoredCurve],
    g: &EdgeGeometry,
) -> Option<(super::curve::CurveFit, usize)> {
    curves
        .iter()
        .enumerate()
        .filter_map(|(k, c)| Some((c.fit(g.start, g.mid, g.end, CURVE_TOLERANCE)?, k)))
        .min_by(|(a, _), (b, _)| {
            (a.rank(CURVE_TOLERANCE), a.off)
                .partial_cmp(&(b.rank(CURVE_TOLERANCE), b.off))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// How an edge of a state lies to an input's stored curves
/// ([`super::curve::CurveFit::rank`] of the best; 3 when it cannot be told).
fn curve_rank(state: &NamedState, edge: usize, curves: &[super::curve::StoredCurve]) -> u8 {
    state
        .edge(edge)
        .ok()
        .and_then(|g| best_fit(curves, &g))
        .map_or(3, |(f, _)| f.rank(CURVE_TOLERANCE))
}

/// Notes in an edge input's fingerprint how the edge found lies to the
/// curves the input stores (`_f3d.curve_fit`, mm; which curve, where it
/// stores several).
fn note_curve_fit(fp: &mut Fingerprint, curves: &[super::curve::StoredCurve], g: &EdgeGeometry) {
    let Some((fit, k)) = best_fit(curves, g) else {
        return;
    };
    let mut text = fit.describe(CURVE_TOLERANCE);
    if curves.len() > 1 {
        text += &format!(" (curve {} of {})", k + 1, curves.len());
    }
    if let Some(f) = fp.f3d.as_mut() {
        f.curve_fit = Some(text);
    }
}

/// Of the edges an input's names fit, the one its stored curves give: the
/// only one that is a stored edge itself, else the only one that is a
/// piece of one (on its curve, within its ends). `None` when the curves do
/// not single one out.
fn by_stored_curve(
    state: &NamedState,
    fits: &[usize],
    curves: &[super::curve::StoredCurve],
) -> Option<usize> {
    let ranks: Vec<u8> = fits.iter().map(|&i| curve_rank(state, i, curves)).collect();
    for rank in [0, 1] {
        let at: Vec<usize> = (0..fits.len()).filter(|&k| ranks[k] == rank).collect();
        match at[..] {
            [k] => return Some(fits[k]),
            [] => continue,
            _ => return None,
        }
    }
    None
}

/// Of the edges an input's names fit, those its stored curves give where it
/// stands for several (mitcad#106): the input stores a curve per edge it saw
/// ([`super::curve`]: an edge in pieces, the pieces of a ring between the
/// same two faces), and the state has an edge on several of them, or the
/// edge it saw as one curve is in pieces in the state (a later change of an
/// earlier item cut it). The edges that fit the names and lie on a stored
/// curve within its ends are the input's when they are at least two, all
/// are on one body, and the edges on one curve are pieces of it whose
/// lengths add up to its length at most (two copies of one edge, as where a
/// body is in the state twice, are not two pieces). A stored curve no edge
/// of these names lies on is left alone (an edge the names no longer give).
/// `None` otherwise, and where one edge alone lies on the curves
/// ([`by_stored_curve`] decides those).
fn by_stored_pieces(
    state: &NamedState,
    fits: &[usize],
    curves: &[super::curve::StoredCurve],
) -> Option<Vec<usize>> {
    // The edges on a curve within its ends, with that curve.
    let on: Vec<(usize, usize, u8, f64)> = fits
        .iter()
        .filter_map(|&i| {
            let g = state.edge(i).ok()?;
            let (fit, k) = best_fit(curves, &g)?;
            let rank = fit.rank(CURVE_TOLERANCE);
            (rank <= 1).then_some((i, k, rank, g.length))
        })
        .collect();
    if on.len() < 2 {
        return None;
    }
    let body = state.edges.get(on[0].0)?.body;
    if on
        .iter()
        .any(|&(i, ..)| state.edges.get(i).is_none_or(|e| e.body != body))
    {
        return None;
    }
    for (k, curve) in curves.iter().enumerate() {
        let here: Vec<&(usize, usize, u8, f64)> = on.iter().filter(|e| e.1 == k).collect();
        match here[..] {
            [] | [_] => {}
            _ => {
                let fits: Vec<(u8, f64)> = here.iter().map(|e| (e.2, e.3)).collect();
                if !are_pieces(curve, &fits) {
                    return None;
                }
            }
        }
    }
    Some(on.into_iter().map(|e| e.0).collect())
}

/// Whether edges that lie on a stored curve within its ends (each its rank
/// of fit and its length, mm) can be pieces of it: none is the whole stored
/// edge, and together they are no longer than it.
fn are_pieces(curve: &super::curve::StoredCurve, fits: &[(u8, f64)]) -> bool {
    let Some(length) = curve.length() else {
        return false;
    };
    let sum: f64 = fits.iter().map(|f| f.1).sum();
    fits.iter().all(|f| f.0 == 1) && sum <= length + CURVE_TOLERANCE
}

/// The edge a recipe names among those that fit its names: the one that
/// fits when only one does; one of several that coincide (the same
/// geometry, as when a body is in the state twice); else, for an edge of
/// its own names (`twin` (0, 1)), the one whose faces round its ends are in
/// the order the recipe's tail lists them (mitcad#106: mirror-image edges
/// between the same two faces); else the k-th of n for `twin` (k, n) as
/// [`locate`] says. The tail is not used for the n inputs of the same
/// names of an item (n > 1): each would choose among the edges that fit
/// alone, and two could choose the same one.
///
/// Before all that, where the names fit several edges, the curve the input
/// stores (mitcad#306) decides when it singles one of them out
/// ([`by_stored_curve`]): it is the edge as the input saw it, read from the
/// file, so it tells apart the edges the names (and the tail's order) cannot.
/// With `pieces` (a fillet's or chamfer's input, whose set can take more
/// edges than it has inputs) the curves can give several edges first
/// ([`by_stored_pieces`]): the first is the input's, the others go into
/// [`Picked::pieces`].
fn pick_edge(
    state: &NamedState,
    entities: &[Vec<EntityName>],
    tail: Option<&super::recipe::EdgeTail>,
    twin: (usize, usize),
    curves: &[super::curve::StoredCurve],
    pieces: bool,
) -> Result<Picked, String> {
    let (k, n) = twin;
    let one = |edge: usize, how: &'static str| Picked {
        edge,
        pieces: Vec::new(),
        how,
    };
    if !curves.is_empty()
        && let Ok(all) = state.edges_fitting_ordered(entities, None)
        && all.len() > 1
    {
        if pieces && let Some(edges) = by_stored_pieces(state, &all, curves) {
            return Ok(Picked {
                edge: edges[0],
                pieces: edges[1..].to_vec(),
                how: " (one of the edges its stored curves give)",
            });
        }
        if let Some(i) = by_stored_curve(state, &all, curves) {
            return Ok(one(i, " (by its stored curve)"));
        }
    }
    let fits = state.edges_fitting_ordered(entities, tail.filter(|_| n == 1))?;
    if let [i] = fits[..] {
        // (By the order of the tail when the names alone leave several.)
        let by_tail = n == 1
            && tail.is_some()
            && state
                .edges_fitting_ordered(entities, None)
                .is_ok_and(|all| all.len() > 1);
        return Ok(one(
            i,
            if by_tail {
                " (by the recipe's tail)"
            } else {
                ""
            },
        ));
    }
    if let Some(first) = coincident(state, &fits) {
        return Ok(one(first, " (one of coincident copies)"));
    }
    if fits.len() == n {
        return Ok(one(fits[k], " (one of the edges of the same names)"));
    }
    Err(format!(
        "{} edges fit the names{}",
        fits.len(),
        describe_fits(state, &fits, curves)
    ))
}

/// The edge [`pick_edge`] found, how, and the other edges that are the
/// input's too ([`by_stored_pieces`]).
#[derive(Clone, Debug, PartialEq)]
struct Picked {
    edge: usize,
    pieces: Vec<usize>,
    how: &'static str,
}

/// How the edges an input's names fit lie to its stored curves, for the
/// reason it is not found: ` (to the stored curve: the stored edge, off the
/// stored curve by 3.2000, …)`, with the curve's number where it stores
/// several; empty without curves.
fn describe_fits(
    state: &NamedState,
    fits: &[usize],
    curves: &[super::curve::StoredCurve],
) -> String {
    if curves.is_empty() {
        return String::new();
    }
    let each: Vec<String> = fits
        .iter()
        .map(
            |&i| match state.edge(i).ok().and_then(|g| best_fit(curves, &g)) {
                Some((fit, k)) if curves.len() > 1 => {
                    format!("{} (curve {})", fit.describe(CURVE_TOLERANCE), k + 1)
                }
                Some((fit, _)) => fit.describe(CURVE_TOLERANCE),
                None => "not comparable".to_owned(),
            },
        )
        .collect();
    format!(" (to the stored curves: {})", each.join(", "))
}

/// The first of several edges with the same geometry (start, end and
/// middle points, length), if they have.
fn coincident(state: &NamedState, edges: &[usize]) -> Option<usize> {
    let (&first, rest) = edges.split_first()?;
    let a = state.edge(first).ok()?;
    let same = |x: [f64; 3], y: [f64; 3]| (0..3).all(|k| (x[k] - y[k]).abs() <= 1e-6);
    rest.iter()
        .all(|&i| {
            state.edge(i).is_ok_and(|b| {
                same(a.start, b.start)
                    && same(a.end, b.end)
                    && same(a.mid, b.mid)
                    && (a.length - b.length).abs() <= 1e-6
            })
        })
        .then_some(first)
}

/// An edge's geometry into its fingerprint (cm).
fn edge_geometry(fp: &mut Fingerprint, g: &EdgeGeometry) {
    let cm = |p: [f64; 3]| p.map(|x| x / 10.0);
    fp.mid_point = Some(cm(g.mid));
    fp.length = Some(g.length / 10.0);
    fp.start_point = Some(Some(cm(g.start)));
    fp.end_point = Some(Some(cm(g.end)));
    if let Some(f) = fp.f3d.as_mut() {
        f.direction = Some(g.direction);
    }
}

/// The edges of a face a fillet or chamfer selects (mitcad#96: a
/// `bounded_face` input: every edge between the face and another), found
/// as an edge input's are, each a fingerprint with the face's names.
fn face_edges(
    state: &NamedState,
    face: &Fingerprint,
    entities: &[Vec<EntityName>],
) -> Result<Vec<Fingerprint>, String> {
    let i = state.find_face(entities)?;
    let edges = state.edges_of_face(i);
    if edges.is_empty() {
        return Err("the face has no edges to other faces".to_owned());
    }
    edges
        .into_iter()
        .map(|e| {
            let g = state.edge(e)?;
            let mut fp = Fingerprint {
                object_type: Some("BRepEdge".to_owned()),
                f3d: face.f3d.clone(),
                ..Fingerprint::default()
            };
            edge_geometry(&mut fp, &g);
            Ok(fp)
        })
        .collect()
}

/// A face selection of a fillet or chamfer ([`expand_faces`]): its edge set
/// and place in it, the face and its names.
type FaceSelection<'f> = ((usize, usize), &'f Fingerprint, Vec<Vec<EntityName>>);

/// A fillet's or chamfer's face selections (face references in its edge
/// sets, mitcad#96) replaced by the edges of the faces, found in the
/// state before the item, else up to [`EARLIER`] states before it, as
/// [`find_named`] finds edges. A face not found stays a face reference
/// with the reason.
fn expand_faces(
    at: &At,
    blobs: &[Blob],
    result: Option<i32>,
    detail: &mut Detail,
    out: &mut Resolved,
) {
    let sets = match detail {
        Detail::Fillet(f) => f.edge_sets.as_mut(),
        Detail::Chamfer(c) => c.edge_sets.as_mut(),
        _ => None,
    };
    let Some(sets) = sets else { return };
    let is_face = |r: &Reference| matches!(r, Reference::Face(fp) if fp.f3d.is_some());
    // (set, position) of each face selection.
    let mut left: Vec<(usize, usize)> = Vec::new();
    for (k, s) in sets.iter().enumerate() {
        for (j, r) in s.edges.iter().flatten().enumerate() {
            if is_face(r) {
                left.push((k, j));
            }
        }
    }
    if left.is_empty() {
        return;
    }
    let mut found: HashMap<(usize, usize), Vec<Fingerprint>> = HashMap::new();
    let mut why: HashMap<(usize, usize), String> = HashMap::new();
    let own = starts(at, blobs, result);
    if own.is_empty() {
        let none = no_start(at, result);
        for &key in &left {
            why.insert(key, none.clone());
        }
        left.clear();
    }
    let label = start_label(at, result);
    for earlier in [0..1, 1..EARLIER + 1] {
        if left.is_empty() {
            break;
        }
        // The face selections, each with its names.
        let open: Vec<FaceSelection> = left
            .iter()
            .filter_map(|&(k, j)| {
                let Some(Reference::Face(face)) = sets[k].edges.as_ref().and_then(|e| e.get(j))
                else {
                    return None;
                };
                let entities = face
                    .f3d
                    .as_ref()
                    .and_then(|f| f.entities.clone())
                    .unwrap_or_default();
                Some(((k, j), &**face, entities))
            })
            .collect();
        // Each takes the first state (in this order) it is found in, and the
        // first reason it is not; the states are named one at a time, and
        // none after all are found.
        let mut hits: HashMap<(usize, usize), (Vec<Fingerprint>, &str, usize)> = HashMap::new();
        let views = earlier
            .flat_map(|e| own.iter().map(move |&(b, k)| (b, e, k + e)))
            .filter(|&(b, _, k)| k <= blobs[b].history.states.len());
        for (b, e, k) in views {
            if open.iter().all(|(key, _, _)| hits.contains_key(key)) {
                break;
            }
            let state = at.states.get(b, k);
            for (key, face, entities) in &open {
                if hits.contains_key(key) {
                    continue;
                }
                match face_edges(&state, face, entities) {
                    Ok(edges) => {
                        hits.insert(*key, (edges, blobs[b].entry.as_str(), e));
                    }
                    Err(err) => {
                        why.entry(*key).or_insert(err);
                    }
                }
            }
        }
        let keys: Vec<(usize, usize)> = open.iter().map(|(key, _, _)| *key).collect();
        drop(open);
        let mut still = Vec::new();
        for (k, j) in keys {
            match hits.remove(&(k, j)) {
                Some((mut edges, entry, e)) => {
                    let text = match e {
                        0 => format!("{entry} before {label} (an edge of a face)"),
                        e => {
                            format!("{entry} {e} states before {label} (an edge of a face)")
                        }
                    };
                    for fp in &mut edges {
                        if let Some(f) = fp.f3d.as_mut() {
                            f.found = Some(text.clone());
                        }
                    }
                    found.insert((k, j), edges);
                }
                None => still.push((k, j)),
            }
        }
        left = still;
    }
    for (k, s) in sets.iter_mut().enumerate() {
        let Some(refs) = s.edges.take() else { continue };
        let mut edges = Vec::new();
        for (j, r) in refs.into_iter().enumerate() {
            if let Some(list) = found.remove(&(k, j)) {
                out.found += list.len();
                edges.extend(list.into_iter().map(|fp| Reference::Edge(Box::new(fp))));
                continue;
            }
            match r {
                Reference::Face(mut fp) if fp.f3d.is_some() => {
                    out.not_found += 1;
                    if let Some(f) = fp.f3d.as_mut() {
                        f.found = why.remove(&(k, j));
                    }
                    edges.push(Reference::Face(fp));
                }
                r => edges.push(r),
            }
        }
        s.edges = Some(edges);
    }
}

/// Finds the named inputs of the dump's items in the document's ASM
/// history and writes their geometry into the dump.
pub fn resolve(dump: &mut Dump, doc: &F3dFile) -> Resolved {
    let mut out = Resolved::default();
    let Some(items) = dump.timeline.as_mut().and_then(|t| t.items.as_mut()) else {
        return out;
    };
    let blobs = Blobs::default();
    let states = States::new(&blobs, doc);
    let timeline = Timeline::new(items);
    let _ = states
        .circle_states
        .set(seam_states(items, &states, &timeline));
    for (position, item) in items.iter_mut().enumerate() {
        let at = At {
            blobs: &blobs,
            states: &states,
            doc,
            timeline: &timeline,
            position,
            suppressed: no_result_state(item),
        };
        resolve_item(item, &at, &mut out);
    }
    circle_seams(items, &states, &timeline, 1);
    out
}

/// Where an item's inputs are looked for: the history's blobs (read once
/// one needs them), the timeline and the item's position in it.
struct At<'a> {
    blobs: &'a Blobs,
    /// The blobs' states, named once ([`States`]).
    states: &'a States<'a>,
    doc: &'a F3dFile,
    timeline: &'a Timeline,
    position: usize,
    /// The item is suppressed in the file or rolled back ([`starts`],
    /// [`no_result_state`], mitcad#180).
    suppressed: bool,
}

/// [`resolve`] for one item.
fn resolve_item(item: &mut TimelineItem, at: &At, out: &mut Resolved) {
    let (blobs, doc) = (at.blobs, at.doc);
    let result = result_of(item, at);
    resolve_bodies(item, at, result, out);
    resolve_path_edges(item, at, out);
    let Some(detail) = item.detail.as_mut() else {
        return;
    };
    if let Detail::ConstructionPlane(p) = detail {
        resolve_plane_inputs(p.definition.as_mut(), at, out);
        // An angle about a face (a line is an edge or axis) is a tangent plane (mitcad#186:
        // the API lists those as `ConstructionPlaneTangentDefinition`, with
        // the face as `tangentFace`; 11 planes of the real designs).
        if let Some(def) = p.definition.as_mut()
            && def.definition_type.as_deref() == Some("ConstructionPlaneAtAngleDefinition")
            && let Some(Value::Object(face)) = def.other.get("linearEntity")
            && matches!(Reference::from_map(face.clone()), Reference::Face(_))
        {
            let face = def.other.remove("linearEntity").expect("present");
            def.other.insert("tangentFace".to_owned(), face);
            def.definition_type = Some("ConstructionPlaneTangentDefinition".to_owned());
        }
        return;
    }
    if let Detail::ConstructionPoint(p) = detail {
        resolve_plane_inputs(p.definition.as_mut(), at, out);
        return;
    }
    if let Detail::ConstructionAxis(a) = detail {
        resolve_plane_inputs(a.definition.as_mut(), at, out);
        // A face that is no cylinder, cone or torus alone is the face of
        // an axis perpendicular to it at a point (mitcad#186): the file
        // stores the face and the axis' own origin, which the API gives
        // as `point`.
        let origin = a.geometry.as_ref().and_then(|g| g.origin);
        if let Some(def) = a.definition.as_mut()
            && def.definition_type.as_deref() == Some("ConstructionAxisCircularFaceDefinition")
            && let Some(Value::Object(face)) = def.other.get("circularFace")
            && let Reference::Face(fp) = Reference::from_map(face.clone())
            && fp
                .geometry
                .as_ref()
                .and_then(|g| g.geometry_type.as_deref())
                == Some("Plane")
            && let Some(origin) = origin
        {
            let face = def.other.remove("circularFace").expect("present");
            def.other.insert("face".to_owned(), face);
            def.other
                .insert("point".to_owned(), serde_json::json!(origin));
            def.definition_type = Some("ConstructionAxisPerpendicularAtPointDefinition".to_owned());
        }
        return;
    }
    if matches!(detail, Detail::Fillet(_) | Detail::Chamfer(_)) {
        let blobs = blobs.get(doc);
        expand_faces(at, blobs, result, detail, out);
    }
    // Faces of other components only where an item may take them (an
    // extrusion's extent or profile): a fillet's, chamfer's, thread's or
    // shell's inputs are on the bodies it changes.
    let others = matches!(detail, Detail::Extrude(_));
    let sets = named_edge_sets(detail);
    let edges = named_entities(detail);
    if edges.is_empty() {
        return;
    }
    find_named(at, result, edges, Some(&sets), others, false, out);
    split_pieces(detail, out);
    // (An item suppressed in the file is looked for in the state after the
    // item before it, which the file never had: there pieces of one edge
    // its inputs saw can be one edge, and each input takes it.)
    if matches!(detail, Detail::Fillet(_) | Detail::Chamfer(_)) && !at.suppressed {
        drop_conflicting(detail, out);
    }
}

/// The edge inputs of a fillet's or chamfer's sets that were found.
fn found_dressup_edges(detail: &mut Detail) -> Vec<&mut Fingerprint> {
    let sets = match detail {
        Detail::Fillet(f) => f.edge_sets.as_mut(),
        Detail::Chamfer(c) => c.edge_sets.as_mut(),
        _ => None,
    };
    sets.into_iter()
        .flatten()
        .flat_map(|s| s.edges.iter_mut().flatten())
        .filter_map(|r| match r {
            Reference::Edge(fp) if fp.mid_point.is_some() && fp.f3d.is_some() => Some(&mut **fp),
            _ => None,
        })
        .collect()
}

/// An edge fingerprint's geometry in millimetres, as [`NamedState::edge`]
/// gives it.
fn fingerprint_edge(fp: &Fingerprint) -> Option<EdgeGeometry> {
    let mm = |p: [f64; 3]| p.map(|x| x * 10.0);
    Some(EdgeGeometry {
        mid: mm(fp.mid_point?),
        length: fp.length? * 10.0,
        start: mm(fp.start_point.flatten()?),
        end: mm(fp.end_point.flatten()?),
        direction: fp.f3d.as_ref()?.direction.unwrap_or([0.0; 3]),
    })
}

/// Inputs of one fillet or chamfer that found the same edge although their
/// names and their stored curves differ (mitcad#306): two stored curves are
/// two edges as the inputs saw them, so the names cannot have found both in
/// that edge. Those of them whose stored curves the edge does not lie
/// within (it runs beyond their ends, or off them: an edge their names fit
/// in a state that does not have theirs) are left not found, with why; the
/// others keep it (an input whose stored edge was longer than the edge now
/// is, mitcad#305, keeps it as one that is the stored edge does). Not for
/// an item suppressed in the file ([`resolve_item`]).
fn drop_conflicting(detail: &mut Detail, out: &mut Resolved) {
    let mut edges = found_dressup_edges(detail);
    // The inputs by the edge they found (its place, in its component).
    let key = |fp: &Fingerprint| {
        let r = |p: [f64; 3]| p.map(|x| (x * 1e6).round() as i64);
        (
            fp.mid_point.map(r),
            fp.length.map(|l| (l * 1e6).round() as i64),
            fp.f3d.as_ref().and_then(|f| f.component),
        )
    };
    let mut groups: HashMap<_, Vec<usize>> = HashMap::new();
    for (i, fp) in edges.iter().enumerate() {
        groups.entry(key(fp)).or_default().push(i);
    }
    for members in groups.values() {
        if members.len() < 2 {
            continue;
        }
        let f3d = |i: usize| edges[i].f3d.as_ref().expect("named");
        let distinct = |of: &dyn Fn(usize) -> String| {
            let mut v: Vec<String> = members.iter().map(|&i| of(i)).collect();
            v.sort();
            v.dedup();
            v.len()
        };
        let names = |i: usize| serde_json::to_string(&f3d(i).entities).unwrap_or_default();
        let curves = |i: usize| serde_json::to_string(&f3d(i).curves).unwrap_or_default();
        let with_curves = members.iter().filter(|&&i| f3d(i).curves.is_some()).count();
        if with_curves < 2 || distinct(&names) < 2 || distinct(&curves) < 2 {
            continue;
        }
        for &i in members {
            let curves = stored_curves(edges[i]);
            let Some((fit, _)) = fingerprint_edge(edges[i]).and_then(|g| best_fit(&curves, &g))
            else {
                continue;
            };
            let why = match fit.rank(CURVE_TOLERANCE) {
                2 => format!(
                    "the edge its names fit, which another input of the item with another \
                     stored curve found too, runs {:.4} mm beyond the ends of its stored curve",
                    fit.beyond
                ),
                3 => format!(
                    "the edge its names fit, which another input of the item with another \
                     stored curve found too, lies {:.4} mm off its stored curve",
                    fit.off
                ),
                _ => continue,
            };
            let fp = &mut *edges[i];
            fp.mid_point = None;
            fp.length = None;
            fp.start_point = None;
            fp.end_point = None;
            if let Some(f) = fp.f3d.as_mut() {
                f.direction = None;
                f.component = None;
                f.found = Some(why);
            }
            out.found = out.found.saturating_sub(1);
            out.not_found += 1;
        }
    }
}

/// The edge set of each input [`named_entities`] gives of a fillet or
/// chamfer (none for another item's).
fn named_edge_sets(detail: &Detail) -> Vec<usize> {
    let sets: Vec<&Option<Vec<Reference>>> = match detail {
        Detail::Fillet(d) => d.edge_sets.iter().flatten().map(|s| &s.edges).collect(),
        Detail::Chamfer(d) => d.edge_sets.iter().flatten().map(|s| &s.edges).collect(),
        _ => return Vec::new(),
    };
    sets.iter()
        .enumerate()
        .flat_map(|(i, edges)| {
            edges
                .iter()
                .flatten()
                .filter(|r| matches!(r, Reference::Edge(fp) if fp.mid_point.is_none() && fp.f3d.is_some()))
                .map(move |_| i)
        })
        .collect()
}

/// The keys of a construction plane's definition that hold the entities
/// it is measured from (mitcad#178; `planarEntity` is a field of its own).
const PLANE_INPUT_KEYS: [&str; 18] = [
    "linearEntity",
    "planarEntityOne",
    "planarEntityTwo",
    "linearEntityOne",
    "linearEntityTwo",
    "pointEntity",
    "pointEntityOne",
    "pointEntityTwo",
    "pointEntityThree",
    "tangentFace",
    "point",
    // Points and axes (mitcad#186).
    "edgeEntity",
    "circularFace",
    "face",
    "circularEntity",
    "planeOne",
    "planeTwo",
    "planeThree",
];

/// A construction plane's or point's named faces, edges and vertices
/// (mitcad#178). A datum has no ASM state of its own: they are looked for
/// in each history at its point of the timeline (the state after the last
/// item of the history's component before it), as other components'
/// inputs are.
fn resolve_plane_inputs(def: Option<&mut super::ir::Definition>, at: &At, out: &mut Resolved) {
    let Some(def) = def else {
        return;
    };
    let mut refs: Vec<(Option<&str>, Reference)> = Vec::new();
    if let Some(r) = def.planar_entity.take() {
        refs.push((None, r));
    }
    for k in PLANE_INPUT_KEYS {
        if let Some(Value::Object(m)) = def.other.remove(k) {
            refs.push((Some(k), Reference::from_map(m)));
        }
    }
    let mut paths: Vec<Reference> = match def.other.remove("pathEntities") {
        Some(Value::Array(v)) => v
            .into_iter()
            .filter_map(|v| match v {
                Value::Object(m) => Some(Reference::from_map(m)),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let named: Vec<&mut Fingerprint> = refs
        .iter_mut()
        .map(|(_, r)| r)
        .chain(paths.iter_mut())
        .filter_map(|r| match r {
            Reference::Edge(fp) if fp.mid_point.is_none() && fp.f3d.is_some() => Some(&mut **fp),
            Reference::Face(fp) if fp.point_on_face.is_none() && fp.f3d.is_some() => {
                Some(&mut **fp)
            }
            Reference::Vertex(fp) if fp.point.is_none() && fp.f3d.is_some() => Some(&mut **fp),
            _ => None,
        })
        .collect();
    if !named.is_empty() {
        find_named(at, None, named, None, true, true, out);
    }
    for (k, r) in refs {
        match k {
            None => def.planar_entity = Some(r),
            Some(k) => {
                def.other
                    .insert(k.to_owned(), serde_json::to_value(&r).expect("serializes"));
            }
        }
    }
    if !paths.is_empty() {
        def.other.insert(
            "pathEntities".to_owned(),
            serde_json::to_value(&paths).expect("serializes"),
        );
    }
}

// Inputs found on several threads (mitcad#103).

/// [`resolve`] on up to `threads` threads: the items are independent of
/// each other (each reads the history's blobs, read once for all), so the
/// dump comes out the same; on large designs finding the inputs took over
/// a minute on one thread.
pub fn resolve_on(dump: &mut Dump, doc: &F3dFile, threads: usize) -> Resolved {
    if threads <= 1 {
        return resolve(dump, doc);
    }
    let Some(items) = dump.timeline.as_mut().and_then(|t| t.items.as_mut()) else {
        return Resolved::default();
    };
    let blobs = Blobs::default();
    let states = States::new(&blobs, doc);
    let timeline = Timeline::new(items);
    let _ = states
        .circle_states
        .set(seam_states(items, &states, &timeline));
    let queue = std::sync::Mutex::new(items.iter_mut().enumerate());
    let found = std::sync::Mutex::new(Resolved::default());
    let work = || {
        let mut out = Resolved::default();
        loop {
            let next = queue
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .next();
            let Some((position, item)) = next else {
                break;
            };
            let at = At {
                blobs: &blobs,
                states: &states,
                doc,
                timeline: &timeline,
                position,
                suppressed: no_result_state(item),
            };
            resolve_item(item, &at, &mut out);
        }
        let mut found = found
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        found.found += out.found;
        found.not_found += out.not_found;
    };
    std::thread::scope(|scope| {
        for _ in 1..threads {
            // (One that does not start leaves its items to the others.)
            let _ = std::thread::Builder::new()
                .name("mitcad-import-inputs".to_owned())
                .stack_size(64 << 20)
                .spawn_scoped(scope, work);
        }
        work();
    });
    circle_seams(items, &states, &timeline, threads);
    found
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// An input [`find_named`] looks for: the first state it was found in
/// (blob index, states before the state the pass starts from), else why
/// not.
struct Input<'f> {
    fp: &'f mut Fingerprint,
    twin: (usize, usize),
    /// The edge set of the fillet or chamfer the input belongs to (none for the inputs of other items).
    set: Option<usize>,
    entities: Vec<Vec<EntityName>>,
    why: String,
    hit: Option<(usize, usize)>,
    /// The edge found, in the state of `hit`.
    edge: Option<usize>,
    /// The other edges the input is ([`by_stored_pieces`]).
    pieces: Vec<usize>,
    /// How an edge was picked where several fit ([`pick_edge`]).
    how: &'static str,
}

/// An input not found yet, with its place among the item's inputs of the
/// same names ([`locate`]) and its edge set.
type Left<'f> = Vec<(&'f mut Fingerprint, (usize, usize), Option<usize>)>;

/// Finds named edges and faces in the history state before the item
/// whose state is `result`, else up to [`EARLIER`] states before it; what
/// is in none of them, in the other components' histories at the item's
/// point of the timeline (mitcad#161: a sweep along an edge of another
/// component's body), else up to [`EARLIER`] states before that.
///
/// With `twins` (the edge set of each input; fillets and chamfers), inputs
/// of the same names take the edges that fit them one each ([`locate`]), and
/// inputs of one set that fit the same edges alike too ([`resolve_alike`]).
fn find_named(
    at: &At,
    result: Option<i32>,
    edges: Vec<&mut Fingerprint>,
    twins: Option<&[usize]>,
    others: bool,
    surface: bool,
    out: &mut Resolved,
) {
    let blobs = at.blobs.get(at.doc);
    // The blobs that hold the item's start, with the position of the
    // state just before it (newest first; [`starts`]).
    let own = starts(at, blobs, result);
    // Inputs with the same names: each the k-th of n.
    let keys: Vec<String> = edges
        .iter()
        .map(|fp| {
            let f = fp.f3d.as_ref();
            serde_json::to_string(&(
                fp.object_type.as_deref(),
                f.and_then(|f| f.entities.as_ref()),
            ))
            .unwrap_or_default()
        })
        .collect();
    let nth: Vec<(usize, usize)> = (0..keys.len())
        .map(|i| {
            if twins.is_none() {
                return (0, 1);
            }
            let k = keys[..i].iter().filter(|x| **x == keys[i]).count();
            (k, keys.iter().filter(|x| **x == keys[i]).count())
        })
        .collect();
    let mut left: Left = edges
        .into_iter()
        .zip(nth)
        .enumerate()
        .map(|(i, (fp, twin))| (fp, twin, twins.and_then(|s| s.get(i).copied())))
        .collect();
    if own.is_empty() {
        note_left(&mut left, &no_start(at, result));
    } else {
        // The state just before the item; for what is not there, the
        // earlier ones (some inputs name faces as an earlier state had
        // them).
        let label = start_label(at, result);
        for earlier in [0..1, 1..EARLIER + 1] {
            if left.is_empty() {
                break;
            }
            let views = earlier
                .flat_map(|e| own.iter().map(move |&(b, k)| (b, e, k + e)))
                .filter(|&(b, _, k)| k <= blobs[b].history.states.len());
            left = search(at.states, left, views, out, surface, &|b, e| {
                let entry = &blobs[b].entry;
                match e {
                    0 => format!("{entry} before {label}"),
                    e => format!("{entry} {e} states before {label}"),
                }
            });
        }
    }
    if left.is_empty() || !others {
        out.not_found += left.len();
        return;
    }
    // Other components' histories at the item's point of the timeline:
    // the state after the last item of the blob's component before it.
    let links = at.timeline.links(blobs);
    let other_blobs: Vec<(usize, usize)> = (0..blobs.len())
        .filter(|b| !own.iter().any(|(o, _)| o == b))
        .filter_map(|b| Some((b, at.timeline.state_at(blobs, b, at.position)?)))
        .collect();
    let views = (0..=EARLIER)
        .flat_map(|e| other_blobs.iter().map(move |&(b, k)| (b, e, k + e)))
        .filter(|&(b, _, k)| k <= blobs[b].history.states.len());
    let position = at.position;
    let mut found = Vec::new();
    left = search_marked(at.states, left, views, &mut found, surface, &|b, e| {
        let entry = &blobs[b].entry;
        match e {
            0 => format!("{entry} (another component's history) at item {position}"),
            e => format!("{entry} (another component's history) {e} states before item {position}"),
        }
    });
    for (fp, b) in found {
        if let Some(f) = fp.f3d.as_mut() {
            f.component = links[b];
        }
        out.found += 1;
    }
    out.not_found += left.len();
}

/// One pass of [`find_named`]: each input looks for its entity in the
/// states `views` give (blob, states before the pass's first state,
/// steps back from the blob's last state), in that order, and takes the
/// first it is in; `found` says where. Returns the inputs not found,
/// with the first reason.
fn search<'f>(
    states: &States,
    left: Left<'f>,
    views: impl Iterator<Item = (usize, usize, usize)>,
    out: &mut Resolved,
    surface: bool,
    found: &dyn Fn(usize, usize) -> String,
) -> Left<'f> {
    let mut hits = Vec::new();
    let left = search_marked(states, left, views, &mut hits, surface, found);
    out.found += hits.len();
    left
}

/// [`search`], giving each input found with its blob.
fn search_marked<'f>(
    states: &States,
    left: Left<'f>,
    views: impl Iterator<Item = (usize, usize, usize)>,
    hits: &mut Vec<(&'f mut Fingerprint, usize)>,
    surface: bool,
    found: &dyn Fn(usize, usize) -> String,
) -> Left<'f> {
    // Each input takes the first state (in this order) it is found in, and
    // the first reason it is not. The states are named one at a time: all
    // of them at once (up to EARLIER per blob, on each of the threads of
    // `resolve_on`) took gigabytes on large designs and ran the process out
    // of memory (mitcad#132).
    let mut inputs: Vec<Input> = left
        .into_iter()
        .filter_map(|(fp, twin, set)| {
            let f = fp.f3d.as_mut()?;
            let entities = f.entities.clone().unwrap_or_default();
            let why = f.found.take().unwrap_or_default();
            Some(Input {
                fp,
                twin,
                set,
                entities,
                why,
                hit: None,
                edge: None,
                pieces: Vec::new(),
                how: "",
            })
        })
        .collect();
    for (b, e, k) in views {
        if inputs.iter().all(|i| i.hit.is_some()) {
            break;
        }
        let state = states.get(b, k);
        for input in inputs.iter_mut().filter(|i| i.hit.is_none()) {
            match locate(
                &state,
                input.fp,
                &input.entities,
                input.twin,
                surface,
                input.set.is_some(),
            ) {
                Ok((how, edges)) => {
                    input.hit = Some((b, e));
                    input.edge = edges.first().copied();
                    input.pieces = edges.get(1..).unwrap_or_default().to_vec();
                    input.how = how;
                }
                Err(err) if input.why.is_empty() => input.why = err,
                Err(_) => {}
            }
        }
        resolve_alike(&state, &mut inputs, (b, e));
    }
    let mut still = Vec::new();
    for Input {
        fp,
        twin,
        set,
        why,
        hit,
        how,
        ..
    } in inputs
    {
        let f = fp.f3d.as_mut().expect("named");
        match hit {
            Some((b, e)) => {
                f.found = Some(format!("{}{how}", found(b, e)));
                hits.push((fp, b));
            }
            None => {
                f.found = Some(why);
                still.push((fp, twin, set));
            }
        }
    }
    still
}

/// The edge inputs of one edge set of a fillet or chamfer that the names fit
/// alike: several edges of the state fit the names of each, the same edges
/// for every one of them, and the inputs are as many as the edges (the
/// edges of a ring between the same two faces, whose recipes differ in the
/// faces at the ends only: [`locate`] cannot tell them apart), so the set
/// is those edges, and each input takes one. Edges other inputs of the set
/// found take are left out of the others' candidates first, so that the rest
/// may be as many as what is left (mitcad#186: on the API reference dumps
/// the edges agree).
fn resolve_alike(state: &NamedState, inputs: &mut [Input], view: (usize, usize)) {
    use std::collections::{BTreeMap, HashSet};
    // The edges taken, by edge set.
    let mut taken: HashSet<(usize, usize)> = inputs
        .iter()
        .filter(|i| i.hit == Some(view))
        .filter_map(|i| Some((i.set?, i.edge.into_iter().chain(i.pieces.iter().copied()))))
        .flat_map(|(set, edges)| edges.map(move |e| (set, e)))
        .collect();
    // The inputs still open, with the edges that fit their names.
    let mut open: Vec<(usize, usize, Vec<usize>)> = Vec::new();
    for (k, input) in inputs.iter().enumerate() {
        let Some(set) = input.set else { continue };
        if input.hit.is_some()
            || matches!(
                input.fp.object_type.as_deref(),
                Some("BRepFace" | "BRepVertex")
            )
        {
            continue;
        }
        if let Ok(fits) = state.edges_fitting_ordered(&input.entities, None)
            && fits.len() > 1
        {
            open.push((k, set, fits));
        }
    }
    if open.is_empty() {
        return;
    }
    loop {
        let mut groups: BTreeMap<(usize, Vec<usize>), Vec<usize>> = BTreeMap::new();
        for (k, set, fits) in &open {
            let mut left: Vec<usize> = fits
                .iter()
                .copied()
                .filter(|&e| !taken.contains(&(*set, e)))
                .collect();
            left.sort_unstable();
            groups.entry((*set, left)).or_default().push(*k);
        }
        let mut progress = false;
        for ((set, edges), members) in groups {
            if edges.is_empty() || members.len() != edges.len() {
                continue;
            }
            for (&k, &edge) in members.iter().zip(&edges) {
                let Ok(g) = state.edge(edge) else { continue };
                let input = &mut inputs[k];
                edge_geometry(input.fp, &g);
                let curves = stored_curves(input.fp);
                note_curve_fit(input.fp, &curves, &g);
                input.hit = Some(view);
                input.edge = Some(edge);
                input.how = " (one of the edges the inputs of the set name alike)";
                taken.insert((set, edge));
                progress = true;
            }
            open.retain(|(k, _, _)| inputs[*k].hit.is_none());
        }
        if !progress || open.is_empty() {
            break;
        }
    }
}

/// Notes why the inputs left were not found (the first reason each
/// keeps).
fn note_left(left: &mut Left, why: &str) {
    for (fp, _, _) in left.iter_mut() {
        if let Some(f) = fp.f3d.as_mut()
            && f.found.is_none()
        {
            f.found = Some(why.to_owned());
        }
    }
}

/// JSON pointers to the edge references with names and no geometry yet in
/// a value.
fn named_edges(v: &Value, at: &str, out: &mut Vec<String>) {
    named_refs(v, at, "edge", out);
}

/// JSON pointers to the references of a kind (`edge`, `face`) with names
/// and no geometry yet in a value.
fn named_refs(v: &Value, at: &str, kind: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            let named = m.get("kind").and_then(Value::as_str) == Some(kind)
                && m.contains_key("_f3d")
                && !m.contains_key("mid_point")
                && !m.contains_key("point_on_face");
            if named {
                out.push(at.to_owned());
                return;
            }
            for (k, x) in m {
                let k = k.replace('~', "~0").replace('/', "~1");
                named_refs(x, &format!("{at}/{k}"), kind, out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                named_refs(x, &format!("{at}/{i}"), kind, out);
            }
        }
        _ => {}
    }
}

/// The edges of sweeps' and pipes' paths and guide rails and of lofts'
/// sections, centre lines and rails (mitcad#34), and of patterns' axes and
/// directions (mitcad#67), found as fillet edges are.
fn resolve_path_edges(item: &mut TimelineItem, at: &At, out: &mut Resolved) {
    let object_type = item.object_type().map(str::to_owned);
    if !matches!(
        object_type.as_deref(),
        Some(
            "SweepFeature"
                | "PipeFeature"
                | "LoftFeature"
                | "CircularPatternFeature"
                | "RectangularPatternFeature"
                | "PathPatternFeature"
                | "MirrorFeature"
                | "SplitBodyFeature"
                | "DeleteFaceFeature"
                // A flange's edge (mitcad#211).
                | "FlangeFeature"
        )
    ) {
        return;
    }
    let Some(detail) = item.detail.as_ref() else {
        return;
    };
    let mut v = serde_json::to_value(detail).expect("serializes");
    let mut pointers = Vec::new();
    named_edges(&v, "", &mut pointers);
    // Faces of patterns, mirrors and splits: objects, axes, planes and
    // tools (mitcad#67).
    if !matches!(
        object_type.as_deref(),
        Some("SweepFeature" | "PipeFeature" | "LoftFeature")
    ) {
        named_refs(&v, "", "face", &mut pointers);
    }
    // A sweep's guide surfaces (mitcad#161).
    if object_type.as_deref() == Some("SweepFeature")
        && let Some(faces) = v.get("guideSurfaces")
    {
        named_refs(faces, "/guideSurfaces", "face", &mut pointers);
    }
    if pointers.is_empty() {
        return;
    }
    let mut fps: Vec<Fingerprint> = pointers
        .iter()
        .map(|p| {
            let map = v.pointer(p).and_then(Value::as_object).cloned();
            Fingerprint::from_map(map.unwrap_or_default())
        })
        .collect();
    let result = result_of(item, at);
    find_named(at, result, fps.iter_mut().collect(), None, true, false, out);
    for (p, fp) in pointers.iter().zip(&fps) {
        if let Some(slot) = v.pointer_mut(p) {
            *slot = serde_json::to_value(fp).expect("serializes");
        }
    }
    if let Value::Object(map) = v {
        item.detail = Some(Detail::typed(object_type.as_deref(), map));
    }
}

/// How many states before an item's own the names of its inputs are
/// looked for when the state just before it lacks them.
const EARLIER: usize = 24;

/// Calls `f` on every body reference with names and no points yet in a
/// JSON value (a body named only by the item that made it, whose recipe
/// does not decode, has no names: mitcad#96).
fn named_bodies(v: &mut Value, f: &mut dyn FnMut(&mut Map<String, Value>)) {
    match v {
        Value::Object(m) => {
            let named = m.get("kind").and_then(Value::as_str) == Some("body")
                && m.get("_f3d")
                    .is_some_and(|x| x.get("edge_points").is_none() && x.get("entities").is_some());
            if named {
                f(m);
            } else {
                for x in m.values_mut() {
                    named_bodies(x, f);
                }
            }
        }
        Value::Array(a) => {
            for x in a {
                named_bodies(x, f);
            }
        }
        _ => {}
    }
}

/// The bodies an item's detail names (moved, split, patterned, mirrored
/// bodies), found in the state just before the item: some of their edges'
/// middle points go into `_f3d.edge_points`.
fn resolve_bodies(item: &mut TimelineItem, at: &At, result: Option<i32>, out: &mut Resolved) {
    let Some(detail) = item.detail.as_ref() else {
        return;
    };
    let mut v = serde_json::to_value(detail).expect("serializes");
    let mut count = 0;
    named_bodies(&mut v, &mut |_| count += 1);
    if count == 0 {
        return;
    }
    let blobs = at.blobs.get(at.doc);
    let named: Vec<(&Blob, Arc<NamedState>)> = starts(at, blobs, result)
        .into_iter()
        .filter(|&(b, k)| k <= blobs[b].history.states.len())
        .map(|(b, k)| (&blobs[b], at.states.get(b, k)))
        .collect();
    let none = no_start(at, result);
    let label = start_label(at, result);
    named_bodies(&mut v, &mut |m| {
        let mut fp = Fingerprint::from_map(std::mem::take(m));
        let f = fp.f3d.get_or_insert_with(Default::default);
        let entities = f.entities.clone().unwrap_or_default();
        let mut why = if named.is_empty() {
            none.clone()
        } else {
            String::new()
        };
        let mut hit = None;
        for (b, state) in &named {
            match state.find_body(&entities) {
                Ok(i) => {
                    let points = state.body_points(i, 8);
                    if !points.is_empty() {
                        hit = Some((b.entry.as_str(), points));
                        break;
                    }
                    why = "no edge of the body has a middle point".to_owned();
                }
                Err(e) => why = e,
            }
        }
        match hit {
            Some((entry, points)) => {
                f.found = Some(format!("{entry} before {label}"));
                f.edge_points = Some(points.iter().map(|p| p.map(|x| x / 10.0)).collect());
                out.found += 1;
            }
            None => {
                f.found = Some(why);
                out.not_found += 1;
            }
        }
        if let Value::Object(map) = serde_json::to_value(&fp).expect("serializes") {
            *m = map;
        }
    });
    if let Value::Object(map) = v {
        let object_type = item.object_type().map(str::to_owned);
        item.detail = Some(Detail::typed(object_type.as_deref(), map));
    }
}

// Entities of other documents (mitcad#196).

/// A document the design refers to, opened: the GUID its inputs name their
/// entities with (lower case; several documents can share one), the
/// design's key for it and its file.
pub struct ExternalFile<'a> {
    pub guid: String,
    pub key: String,
    pub file: &'a F3dFile,
}

/// Where the design's own occurrences of other documents' components are:
/// each one's document key and its placement (cm).
fn external_placements(dump: &Dump) -> Vec<(Mat4, String)> {
    dump.occurrences
        .iter()
        .flatten()
        .filter(|o| o.is_referenced_component == Some(true))
        .filter_map(|o| Some((o.transform?, o.f3d.as_ref()?.external_key.clone()?)))
        .collect()
}

/// Whether two placements (cm) are the same within the files' rounding.
fn same_placement(a: &Mat4, b: &Mat4) -> bool {
    (0..3).all(|r| {
        (0..3).all(|c| (a[r][c] - b[r][c]).abs() <= 1e-5) && (a[r][3] - b[r][3]).abs() <= 5e-4
    })
}

/// The document an entity is in, among those with its GUID: where several
/// documents share one (copies of one document keep its GUID), the one whose
/// occurrence is at the placement the entity's slot carries (`_f3d.context`);
/// else the first.
fn pick_document<'a, 'b>(
    candidates: &'b [&'b ExternalFile<'a>],
    context: Option<&Mat4>,
    placements: &[(Mat4, String)],
) -> Option<&'b ExternalFile<'a>> {
    let keys: Vec<&str> = candidates.iter().map(|c| c.key.as_str()).collect();
    pick_by_placement(&keys, context, placements).and_then(|i| candidates.get(i).copied())
}

/// The index of the key whose occurrence is at `context` (the first key
/// without a placement that tells).
fn pick_by_placement(
    keys: &[&str],
    context: Option<&Mat4>,
    placements: &[(Mat4, String)],
) -> Option<usize> {
    if let (Some(context), [_, _, ..]) = (context, keys) {
        let key = placements
            .iter()
            .find(|(placed, _)| same_placement(placed, context))
            .map(|(_, key)| key);
        if let Some(found) = key.and_then(|k| keys.iter().position(|c| c == k)) {
            return Some(found);
        }
    }
    (!keys.is_empty()).then_some(0)
}

/// The entities of other documents the design's items name (`_f3d.
/// document`, [`super::recipe::owner_guid`]), found in those documents' own
/// histories: their names are those of the other document's operations, so
/// they are looked for in the last state of its history blobs (the version
/// the design refers to), by the GUID of the document (`documents`, the
/// files of the design's `_f3d.external_documents`). The geometry goes into
/// their fingerprints as for the design's own entities (cm, in the other
/// document's coordinates: the components that stand for it have them);
/// `_f3d.found` says where, or why not. Documents that share a GUID (copies
/// of one document) are told apart by the placement the entity's slot
/// carries (`_f3d.context`): the document whose occurrence is there.
pub fn resolve_external(dump: &mut Dump, documents: &[ExternalFile<'_>]) -> Resolved {
    let mut out = Resolved::default();
    if documents.is_empty() {
        return out;
    }
    let placements = external_placements(dump);
    let Some(items) = dump.timeline.as_mut().and_then(|t| t.items.as_mut()) else {
        return out;
    };
    let mut by_guid: HashMap<&str, Vec<&ExternalFile<'_>>> = HashMap::new();
    for d in documents {
        by_guid.entry(d.guid.as_str()).or_default().push(d);
    }
    let blobs: HashMap<&str, Blobs> = documents
        .iter()
        .map(|d| (d.key.as_str(), Blobs::default()))
        .collect();
    for item in items.iter_mut() {
        let Some(detail) = item.detail.as_ref() else {
            continue;
        };
        let mut v = serde_json::to_value(detail).expect("serializes");
        let mut changed = false;
        external_entities(&mut v, &by_guid, &mut |guid, m| {
            let context = m
                .get("_f3d")
                .and_then(|x| x.get("context"))
                .and_then(|c| serde_json::from_value::<Mat4>(c.clone()).ok());
            let Some(doc) = by_guid
                .get(guid)
                .and_then(|c| pick_document(c, context.as_ref(), &placements))
            else {
                return;
            };
            let Some(blobs) = blobs.get(doc.key.as_str()) else {
                return;
            };
            if find_external(m, doc.file, blobs, &mut out) {
                changed = true;
            }
        });
        if changed && let Value::Object(map) = v {
            let object_type = item.object_type().map(str::to_owned);
            item.detail = Some(Detail::typed(object_type.as_deref(), map));
        }
    }
    out
}

/// The references of a value that name an entity of one of `documents` and
/// are not found yet.
fn external_entities(
    v: &mut Value,
    documents: &HashMap<&str, Vec<&ExternalFile<'_>>>,
    f: &mut dyn FnMut(&str, &mut Map<String, Value>),
) {
    match v {
        Value::Object(m) => {
            let named = m.get("_f3d").and_then(|x| {
                let guid = x.get("document")?.as_str()?;
                (documents.contains_key(guid) && x.get("entities").is_some())
                    .then(|| guid.to_owned())
            });
            match named {
                Some(guid) => f(&guid, m),
                None => {
                    for x in m.values_mut() {
                        external_entities(x, documents, f);
                    }
                }
            }
        }
        Value::Array(a) => {
            for x in a {
                external_entities(x, documents, f);
            }
        }
        _ => {}
    }
}

/// Finds one entity in the last state of the document's history blobs and
/// writes what is found into the reference; whether it was found.
fn find_external(
    m: &mut Map<String, Value>,
    doc: &F3dFile,
    blobs: &Blobs,
    out: &mut Resolved,
) -> bool {
    let blobs = blobs.get(doc);
    let is_body = m.get("kind").and_then(Value::as_str) == Some("body");
    let mut reference = if is_body {
        Reference::Body(Box::new(Fingerprint::from_map(std::mem::take(m))))
    } else {
        Reference::from_map(std::mem::take(m))
    };
    let (Reference::Edge(fp) | Reference::Face(fp) | Reference::Vertex(fp) | Reference::Body(fp)) =
        &mut reference
    else {
        if let Ok(Value::Object(map)) = serde_json::to_value(&reference) {
            *m = map;
        }
        return false;
    };
    // What the design's own history gave for the names (they are the other
    // document's operations' numbers: a match there is a coincidence).
    fp.geometry = None;
    fp.point = None;
    fp.point_on_face = None;
    fp.normal_at_point = None;
    fp.start_point = None;
    fp.end_point = None;
    fp.mid_point = None;
    fp.length = None;
    if let Some(f) = fp.f3d.as_mut() {
        f.edge_points = None;
        f.direction = None;
        f.found = None;
        f.component = None;
        f.face_points = None;
        f.seam = None;
        f.curve_fit = None;
    }
    let entities = fp
        .f3d
        .as_ref()
        .and_then(|f| f.entities.clone())
        .unwrap_or_default();
    let mut why = String::new();
    let mut hit = None;
    'blobs: for blob in blobs {
        // The last state first, then earlier ones: the entity may be one the
        // document's later operations consumed.
        for back in 0..=blob.history.states.len() {
            let view = blob.history.view(blob.history.states.len() - back);
            let state = NamedState::new(&blob.file, Some(view));
            let back_note = if back == 0 {
                String::new()
            } else {
                format!(", {back} states before the last")
            };
            if is_body {
                match state.find_body(&entities) {
                    Ok(i) => {
                        // The body as the document's last state has it: the
                        // same entity (the later operations changed its
                        // names, not which body it is).
                        let record = state.bodies[i].record;
                        let last_view = blob.history.view(blob.history.states.len());
                        let last = NamedState::new(&blob.file, Some(last_view));
                        let points = match last.bodies.iter().position(|b| b.record == record) {
                            Some(j) => last.body_points(j, 8),
                            None => state.body_points(i, 8),
                        };
                        if points.is_empty() {
                            why = "no edge of the body has a middle point".to_owned();
                            continue;
                        }
                        if let Some(f) = fp.f3d.as_mut() {
                            f.edge_points =
                                Some(points.iter().map(|p| p.map(|x| x / 10.0)).collect());
                            f.stored = Some(format!("{}#{record}", blob.entry));
                        }
                        hit = Some((blob.entry.as_str(), back_note));
                        break 'blobs;
                    }
                    Err(e) if why.is_empty() => why = e,
                    Err(_) => {}
                }
                continue;
            }
            match locate(&state, fp, &entities, (0, 1), false, false)
                .or_else(|_| locate(&state, fp, &entities, (0, 1), true, false))
            {
                Ok((how, _)) => {
                    // A face as the last state has it, when it is still
                    // there (the same entity, its geometry changed).
                    if back > 0
                        && fp.object_type.as_deref() == Some("BRepFace")
                        && let Ok(i) = state
                            .find_face(&entities)
                            .or_else(|_| state.find_face_surface(&entities))
                    {
                        let entity = state.faces[i].entity;
                        let last_view = blob.history.view(blob.history.states.len());
                        let last = NamedState::new(&blob.file, Some(last_view));
                        if let Some(j) = last.faces.iter().position(|f| f.entity == entity) {
                            let mut later = fp.clone();
                            if fill_face(&last, j, &mut later).is_ok() {
                                *fp = later;
                            }
                        }
                    }
                    hit = Some((blob.entry.as_str(), format!("{how}{back_note}")));
                    break 'blobs;
                }
                Err(e) if why.is_empty() => why = e,
                Err(_) => {}
            }
        }
    }
    // A document without history (its bodies stored only): the entity in
    // the stored bodies, where they name it.
    let mut stored_note = None;
    if hit.is_none() && blobs.is_empty() && !is_body {
        match find_in_stored(doc, fp, &entities) {
            Ok(entry) => {
                stored_note = Some(format!("{entry} (another document's stored bodies)"));
            }
            Err(e) => why = e,
        }
    }
    let found = hit.is_some() || stored_note.is_some();
    if let Some(f) = fp.f3d.as_mut() {
        f.found = Some(match (hit, stored_note) {
            (Some((entry, how)), _) => format!("{entry} (another document's last state){how}"),
            (None, Some(note)) => note,
            (None, None) if blobs.is_empty() && why.is_empty() => {
                "the other document has no history".to_owned()
            }
            (None, None) => why,
        });
    }
    if found {
        out.found += 1;
    } else {
        out.not_found += 1;
    }
    if let Ok(Value::Object(map)) = serde_json::to_value(&reference) {
        *m = map;
    }
    found
}
/// Finds a face, edge or vertex of a document without history in its stored
/// bodies (`.smb` blobs) by its names and writes its geometry into the
/// fingerprint; the blob's entry name.
fn find_in_stored(
    doc: &F3dFile,
    fp: &mut Fingerprint,
    entities: &[Vec<EntityName>],
) -> Result<String, String> {
    let is_face = fp.object_type.as_deref() == Some("BRepFace");
    // (entry, the entity's geometry, for a face how many of the faces the
    // recipe lists around it are next to it there).
    let mut hits: Vec<(String, Fingerprint, usize)> = Vec::new();
    let mut why = String::new();
    for b in doc.body_blobs().into_iter().filter(|b| !b.history) {
        let Ok(data) = doc.read_large(&b.entry) else {
            continue;
        };
        let Ok(file) = AsmFile::parse(&data) else {
            continue;
        };
        let state = NamedState::new(&file, None);
        let mut found = fp.clone();
        match locate(&state, &mut found, entities, (0, 1), false, false) {
            Ok(_) => {
                let entry = b.entry.rsplit('/').next().unwrap_or(&b.entry).to_owned();
                let around = if is_face {
                    state
                        .find_face(entities)
                        .map(|i| state.faces_around(i, entities))
                        .unwrap_or(0)
                } else {
                    0
                };
                hits.push((entry, found, around));
            }
            Err(e) if why.is_empty() => why = e,
            Err(_) => {}
        }
    }
    // The stored bodies of one document name their entities each from its
    // own start (the operation that imported it): the same names fit faces
    // of several bodies. A face is the one with the most of the faces its
    // recipe lists around it.
    let most = hits.iter().map(|h| h.2).max().unwrap_or(0);
    hits.retain(|h| h.2 == most);
    match hits.len() {
        0 if why.is_empty() => Err("the other document has no history or stored body".to_owned()),
        0 => Err(why),
        1 => {
            let (entry, found, _) = hits.pop().expect("one");
            *fp = found;
            Ok(entry)
        }
        n => Err(format!(
            "the other document has no history, and {n} of its stored bodies have \
             an entity of these names"
        )),
    }
}

// Where the file's circles start.

/// A whole circle of a sketch, looked for among the stored bodies' closed
/// edges (mm, in the sketch's component space).
struct SeamCircle {
    curve: usize,
    center: [f64; 3],
    radius: f64,
}

/// A sketch's circles and its frame.
struct SeamSketch {
    circles: Vec<SeamCircle>,
    x: [f64; 3],
    y: [f64; 3],
}

/// Where each whole sketch circle starts in the file: the start vertex of
/// the closed edge the first extrusion that uses the circle made in the
/// stored bodies, as an angle about the sketch normal from the sketch x
/// axis (`_f3d_seam`). The file's sketch circle record always has the x
/// axis as its reference direction; the stored edge's start is elsewhere
/// (the file puts it at the x axis, the opposite side or at another angle,
/// by how the circle was drawn), and the replay needs it to make its
/// cylinders' seams, vertices and edges where the file has them.
fn circle_seams<'a>(
    items: &mut [TimelineItem],
    states: &'a States<'a>,
    timeline: &'a Timeline,
    threads: usize,
) {
    let (blobs, doc) = (states.blobs, states.doc);
    let sketches = seam_sketches(items);
    if sketches.is_empty() {
        return;
    }
    let jobs = seam_jobs(items, &sketches, states, timeline);
    let wanted = |used: &[i64], found: &HashMap<(i64, usize), f64>| {
        used.iter()
            .flat_map(|s| sketches[s].circles.iter().map(move |c| (*s, c.curve)))
            .any(|k| !found.contains_key(&k))
    };
    // What one extrusion's states settle for the circles not found yet:
    // the closed edges on each circle's axis and in its plane (the distance
    // along the axis from the sketch's plane and the angle), the one in the
    // sketch's plane, else the circles at the other end if they agree.
    let settle = |(position, result, used): &(usize, i32, Vec<i64>),
                  found: &HashMap<(i64, usize), f64>|
     -> Vec<((i64, usize), f64)> {
        let item = &items[*position];
        let at = At {
            blobs,
            states,
            doc,
            timeline,
            position: *position,
            suppressed: no_result_state(item),
        };
        let blobs = blobs.get(doc);
        let mut cands: HashMap<(i64, usize), Vec<(f64, f64)>> = HashMap::new();
        for (b, k) in starts(&at, blobs, Some(*result)) {
            if k == 0 || k > blobs[b].history.states.len() {
                continue;
            }
            // The bodies the extrusion made.
            let closed = states.closed_circles(b, k - 1);
            for s in used {
                for (curve, along, angle) in seam_candidates(&closed, &sketches[s]) {
                    if !found.contains_key(&(*s, curve)) {
                        cands.entry((*s, curve)).or_default().push((along, angle));
                    }
                }
            }
        }
        let mut settled = Vec::new();
        for (key, list) in cands {
            let same = |a: f64, b: f64| (a - b).abs() < 1e-6;
            if let Some(&(_, angle)) = list.iter().find(|(t, _)| *t < 0.01) {
                settled.push((key, angle));
            } else if list.iter().all(|&(_, a)| same(a, list[0].1)) {
                settled.push((key, list[0].1));
            }
        }
        settled
    };
    // In the timeline's order, a circle taking what the first extrusion
    // that settles it gives. With threads, `threads` extrusions at a time
    // (mitcad#119: naming each extrusion's states one after another took
    // seconds of a large design's reading): one may then also work out a
    // circle an earlier one of its group settles, which is not taken.
    let mut found: HashMap<(i64, usize), f64> = HashMap::new();
    for group in jobs.chunks(threads.max(1)) {
        let group: Vec<&(usize, i32, Vec<i64>)> =
            group.iter().filter(|j| wanted(&j.2, &found)).collect();
        let settled: Vec<Vec<((i64, usize), f64)>> = if group.len() > 1 {
            std::thread::scope(|scope| {
                let found = &found;
                let running: Vec<_> = group
                    .iter()
                    .map(|job| {
                        std::thread::Builder::new()
                            .name("mitcad-import-inputs".to_owned())
                            .stack_size(64 << 20)
                            .spawn_scoped(scope, move || settle(job, found))
                    })
                    .collect();
                running
                    .into_iter()
                    .zip(&group)
                    .map(|(thread, job)| match thread {
                        Ok(t) => t
                            .join()
                            .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
                        // (Not started: here.)
                        Err(_) => settle(job, found),
                    })
                    .collect()
            })
        } else {
            group.iter().map(|job| settle(job, &found)).collect()
        };
        for (key, angle) in settled.into_iter().flatten() {
            found.entry(key).or_insert(angle);
        }
    }
    for item in items.iter_mut() {
        let index = item.index.unwrap_or(-1);
        if let Some(Detail::Sketch(sk)) = &mut item.detail
            && let Some(curves) = sk.curves.as_mut()
        {
            for (i, c) in curves.iter_mut().enumerate() {
                if let Some(&angle) = found.get(&(index, i)) {
                    c.f3d_seam = Some(angle);
                }
            }
        }
    }
}

/// The states [`circle_seams`] looks in (each extrusion's of a sketch's
/// whole circles: the state after it), so that their closed circles are
/// taken while the items' inputs name them ([`States::closed_circles`]).
fn seam_states<'a>(
    items: &[TimelineItem],
    states: &'a States<'a>,
    timeline: &'a Timeline,
) -> std::collections::HashSet<(usize, usize)> {
    let mut out = std::collections::HashSet::new();
    let sketches = seam_sketches(items);
    if sketches.is_empty() {
        return out;
    }
    let blobs = states.blobs();
    for (position, result, _) in seam_jobs(items, &sketches, states, timeline) {
        let at = At {
            blobs: states.blobs,
            states,
            doc: states.doc,
            timeline,
            position,
            suppressed: no_result_state(&items[position]),
        };
        for (b, k) in starts(&at, blobs, Some(result)) {
            if k > 0 && k <= blobs[b].history.states.len() {
                out.insert((b, k - 1));
            }
        }
    }
    out
}

/// The sketches with whole circles, by their timeline index.
fn seam_sketches(items: &[TimelineItem]) -> HashMap<i64, SeamSketch> {
    let cm = |p: [f64; 3]| p.map(|v| v * 10.0);
    let mut sketches: HashMap<i64, SeamSketch> = HashMap::new();
    for item in items.iter() {
        let Some(Detail::Sketch(sk)) = &item.detail else {
            continue;
        };
        let (Some(frame), Some(curves)) = (sk.model_frame.as_ref(), sk.curves.as_ref()) else {
            continue;
        };
        let (Some(origin), Some(x), Some(y)) = (frame.origin, frame.x_axis, frame.y_axis) else {
            continue;
        };
        let circles: Vec<SeamCircle> = curves
            .iter()
            .enumerate()
            .filter(|(_, c)| c.curve_type.as_deref() == Some("SketchCircle"))
            .filter_map(|(i, c)| {
                let g = c.geometry.as_ref()?;
                let (centre, radius) = (g.center?, g.radius?);
                let at = [
                    origin[0] + centre[0] * x[0] + centre[1] * y[0],
                    origin[1] + centre[0] * x[1] + centre[1] * y[1],
                    origin[2] + centre[0] * x[2] + centre[1] * y[2],
                ];
                Some(SeamCircle {
                    curve: i,
                    center: cm(at),
                    radius: radius * 10.0,
                })
            })
            .collect();
        if !circles.is_empty() {
            sketches.insert(item.index.unwrap_or(-1), SeamSketch { circles, x, y });
        }
    }
    sketches
}

/// The extrusions of the sketches' profiles: (position, result, the
/// sketches).
fn seam_jobs<'a>(
    items: &[TimelineItem],
    sketches: &HashMap<i64, SeamSketch>,
    states: &'a States<'a>,
    timeline: &'a Timeline,
) -> Vec<(usize, i32, Vec<i64>)> {
    let (blobs, doc) = (states.blobs, states.doc);
    let mut jobs: Vec<(usize, i32, Vec<i64>)> = Vec::new();
    for (position, item) in items.iter().enumerate() {
        let Some(Detail::Extrude(d)) = &item.detail else {
            continue;
        };
        let at = At {
            blobs,
            states,
            doc,
            timeline,
            position,
            suppressed: no_result_state(item),
        };
        let Some(result) = result_of(item, &at).filter(|&r| r > 0) else {
            continue;
        };
        // The sketches the extrusion's profiles lie in.
        let mut used: Vec<i64> = Vec::new();
        for r in d.profile.iter().flatten() {
            if let Reference::Profile(p) = r
                && let Some(Some(s)) = p.sketch_timeline_index
                && sketches.contains_key(&s)
                && !used.contains(&s)
            {
                used.push(s);
            }
        }
        if !used.is_empty() {
            jobs.push((position, result, used));
        }
    }
    jobs
}

/// A closed edge that is a whole circle (mm): its start, its centre, its
/// radius and its direction at the middle.
struct ClosedCircle {
    start: [f64; 3],
    centre: [f64; 3],
    radius: f64,
    direction: [f64; 3],
}

/// The whole circles among a state's edges: those that start and end at
/// one vertex with the length of a circle.
fn closed_circles(state: &NamedState) -> Vec<ClosedCircle> {
    let mut out = Vec::new();
    for (i, e) in state.edges.iter().enumerate() {
        if e.vertices[0].is_none() || e.vertices[0] != e.vertices[1] {
            continue;
        }
        let Ok(g) = state.edge(i) else {
            continue;
        };
        // The middle is half way along the parameter: opposite the start.
        let radius = mitcad_asm::brep::dist(g.start, g.mid) / 2.0;
        if radius <= 0.0
            || mitcad_asm::brep::dist(g.start, g.end) > 1e-6
            || (g.length - std::f64::consts::TAU * radius).abs() > 1e-3 * radius
        {
            continue;
        }
        out.push(ClosedCircle {
            start: g.start,
            centre: mitcad_asm::brep::scale(mitcad_asm::brep::add(g.start, g.mid), 0.5),
            radius,
            direction: g.direction,
        });
    }
    out
}

/// The circles of a sketch (by their curve index) that whole circles of
/// a state lie on the axis of and in a plane across the sketch's normal
/// with: the distance of each along the axis from the sketch's plane and
/// its start as an angle ([`seam_angle`]).
fn seam_candidates(closed: &[ClosedCircle], sk: &SeamSketch) -> Vec<(usize, f64, f64)> {
    let normal = mitcad_asm::brep::normalize(mitcad_asm::brep::cross(sk.x, sk.y));
    let mut out = Vec::new();
    for g in closed {
        let d = mitcad_asm::brep::sub(g.start, g.centre);
        // In a plane across the sketch's normal.
        if mitcad_asm::brep::dot(d, normal).abs() > 0.01
            || mitcad_asm::brep::dot(g.direction, normal).abs() > 1e-3
        {
            continue;
        }
        for c in &sk.circles {
            if (c.radius - g.radius).abs() > 0.01 {
                continue;
            }
            // On the circle's axis (the extrusion's other end too).
            let apart = mitcad_asm::brep::sub(g.centre, c.center);
            let along = mitcad_asm::brep::dot(apart, normal);
            let off = mitcad_asm::brep::sub(apart, mitcad_asm::brep::scale(normal, along));
            if mitcad_asm::brep::norm(off) > 0.01 {
                continue;
            }
            out.push((c.curve, along.abs(), seam_angle(d, sk.x, sk.y)));
        }
    }
    out
}

/// The angle of `d` about the normal of a sketch with axes `x` and `y`,
/// counter-clockwise from `x`, in (-pi, pi]: a start on the x axis is 0
/// (not a rounding error from it), one on the opposite side pi.
fn seam_angle(d: [f64; 3], x: [f64; 3], y: [f64; 3]) -> f64 {
    let angle = mitcad_asm::brep::dot(d, y).atan2(mitcad_asm::brep::dot(d, x));
    if angle.abs() < 1e-9 {
        0.0
    } else if angle.abs() > std::f64::consts::PI - 1e-9 {
        std::f64::consts::PI
    } else {
        angle
    }
}

/// The `.smbh` blobs of a document that parse and have a history.
fn history_blobs(doc: &F3dFile) -> Vec<Blob> {
    doc.body_blobs()
        .into_iter()
        .filter(|b| b.history)
        .filter_map(|b| {
            let data = doc.read_large(&b.entry).ok()?;
            let file = AsmFile::parse(&data).ok()?;
            let history = History::parse(&file).ok().flatten()?;
            let entry = b.entry.rsplit('/').next().unwrap_or(&b.entry).to_owned();
            Some(Blob {
                entry,
                file,
                history,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mitcad_asm::brep;
    use mitcad_asm::testdata;

    /// The entities of a named state, to compare namings.
    fn entities(state: &NamedState) -> (Vec<usize>, Vec<usize>, Vec<usize>) {
        (
            state.bodies.iter().map(|b| b.record).collect(),
            state.faces.iter().map(|f| f.entity).collect(),
            state.edges.iter().map(|e| e.entity).collect(),
        )
    }

    #[test]
    fn a_state_is_named_once_while_it_is_kept() {
        let blob = testdata::cube_with_history_blob();
        let zip = mitcad_zip::stored_zip(&[(
            "FusionAssetName[Active]/Breps.BlobParts/BREP.1.smbh",
            &blob,
        )]);
        let doc = F3dFile::from_bytes(zip).unwrap();
        let blobs = Blobs::default();
        let mut states = States::new(&blobs, &doc);
        let file = &states.blobs()[0];
        let alone = |k: usize| NamedState::new(&file.file, Some(file.history.view(k)));
        // Asked for twice: one naming, the same as the state named alone.
        let one = states.get(0, 1);
        assert!(Arc::ptr_eq(&one, &states.get(0, 1)));
        assert_eq!(entities(&one), entities(&alone(1)));
        assert_eq!(one.faces.len(), 6);
        // Three states back there is no body.
        assert!(states.get(0, 3).bodies.is_empty());
        assert!(Arc::ptr_eq(&one, &states.get(0, 1)));
        // Room for one state: the one used longest ago is named again,
        // the same.
        states = States {
            budget: one.size(),
            ..States::new(&blobs, &doc)
        };
        let first = states.get(0, 1);
        let other = states.get(0, 2);
        assert!(Arc::ptr_eq(&other, &states.get(0, 2)));
        let again = states.get(0, 1);
        assert!(!Arc::ptr_eq(&first, &again));
        assert_eq!(entities(&first), entities(&again));
    }

    fn n(tag: &str) -> EntityName {
        EntityName {
            tag: tag.to_owned(),
            kind: 0,
            ops: vec![301],
        }
    }

    /// A face of a document without history (mitcad#332): found in its
    /// stored bodies by its names, the one body that has it; where several
    /// bodies have it just as well, not found, and the note says so.
    #[test]
    fn finds_a_face_of_a_document_without_history_in_its_stored_bodies() {
        let cube = testdata::named_cube_blob();
        let entry = |i: usize| format!("FusionAssetName[Active]/Breps.BlobParts/BREP.{i}.smb");
        let face = || Fingerprint {
            object_type: Some("BRepFace".to_owned()),
            ..Fingerprint::default()
        };
        // The top face, bounded by the front face.
        let entities = vec![vec![n("2")], vec![n("3")]];
        let one =
            F3dFile::from_bytes(mitcad_zip::stored_zip(&[(entry(1).as_str(), &cube)])).unwrap();
        assert!(history_blobs(&one).is_empty());
        let mut fp = face();
        assert_eq!(
            find_in_stored(&one, &mut fp, &entities).as_deref(),
            Ok("BREP.1.smb")
        );
        let point = fp.point_on_face.unwrap();
        assert!((point[2] - 1.0).abs() < 1e-9, "{point:?}");
        assert_eq!(fp.normal_at_point, Some([0.0, 0.0, 1.0]));
        assert_eq!(
            fp.geometry.and_then(|g| g.geometry_type).as_deref(),
            Some("Plane")
        );
        // Two bodies whose faces have the same names and neighbours: either.
        let two = F3dFile::from_bytes(mitcad_zip::stored_zip(&[
            (entry(1).as_str(), &cube),
            (entry(2).as_str(), &cube),
        ]))
        .unwrap();
        let mut fp = face();
        assert_eq!(
            find_in_stored(&two, &mut fp, &entities),
            Err(
                "the other document has no history, and 2 of its stored bodies have an \
                 entity of these names"
                    .to_owned()
            )
        );
        assert!(fp.point_on_face.is_none());
        // Names no stored body has.
        let mut fp = face();
        assert!(find_in_stored(&one, &mut fp, &[vec![n("9")]]).is_err());
    }

    /// The cube's edges along x at the bottom front and at the top front
    /// fit the names of "1" or "2" and "3" equally; their ends tell which
    /// (mitcad#106). Edges at the same place are one edge for a
    /// fingerprint.
    #[test]
    fn picks_the_edge_the_recipe_names() {
        let data = testdata::named_cube_blob();
        let file = AsmFile::parse(&data).unwrap();
        let state = NamedState::new(&file, None);
        // The bottom or top face, the front face, the left and right ones.
        let entities = vec![
            vec![n("1"), n("2")],
            vec![n("3")],
            vec![n("6")],
            vec![n("4")],
        ];
        let tail = |fans: [[usize; 3]; 2]| super::super::recipe::EdgeTail {
            faces: [0, 1],
            fans: fans.iter().map(|f| f.to_vec()).collect(),
        };
        let mut found = Vec::new();
        for fans in [[[0, 1, 2], [1, 0, 3]], [[1, 0, 2], [0, 1, 3]]] {
            let p = pick_edge(&state, &entities, Some(&tail(fans)), (0, 1), &[], true).unwrap();
            let (i, how) = (p.edge, p.how);
            assert_eq!(how, " (by the recipe's tail)");
            found.push(state.edge(i).unwrap().mid[2]);
        }
        // The bottom front edge and the top front edge.
        assert!(
            found[0].abs() < 1e-9 && (found[1] - 10.0).abs() < 1e-9,
            "{found:?}"
        );
        // Without the tail, two fit: one of two inputs of the item each.
        assert_eq!(
            pick_edge(&state, &entities, None, (0, 1), &[], true),
            Err("2 edges fit the names".to_owned())
        );
        assert!(pick_edge(&state, &entities, None, (1, 2), &[], true).is_ok());
        // Edges at the same place are the same for a fingerprint; others
        // are not.
        let all = state.edges_fitting(&entities).unwrap();
        assert_eq!(coincident(&state, &[all[0], all[0]]), Some(all[0]));
        assert_eq!(coincident(&state, &all), None);
        assert_eq!(coincident(&state, &[]), None);
    }

    /// Inputs of other names and other stored curves that found one edge
    /// (mitcad#306): those whose curve the edge does not lie within are
    /// left not found; one whose stored edge is longer keeps it, and inputs
    /// that store the same curve (two names of one edge) keep theirs.
    #[test]
    fn leaves_out_inputs_whose_stored_curve_another_edge_has() {
        use super::super::curve::StoredCurve;
        use super::super::ir::{EdgeSet, FilletDetail, FingerprintF3d};
        // The edge found: from (0,0,0) to (1,0,0) cm.
        let input = |id: u64, tag: &str, curve: Option<StoredCurve>| {
            Reference::Edge(Box::new(Fingerprint {
                object_type: Some("BRepEdge".to_owned()),
                mid_point: Some([0.5, 0.0, 0.0]),
                length: Some(1.0),
                start_point: Some(Some([0.0, 0.0, 0.0])),
                end_point: Some(Some([1.0, 0.0, 0.0])),
                f3d: Some(FingerprintF3d {
                    object_id: Some(id),
                    entities: Some(vec![vec![n(tag)]]),
                    curves: curve.map(|c| vec![c.to_geometry()]),
                    found: Some("state 3".to_owned()),
                    ..FingerprintF3d::default()
                }),
                ..Fingerprint::default()
            }))
        };
        let line = |x0: f64, x1: f64, y: f64| StoredCurve::Line {
            start: [x0, y, 0.0],
            vector: [x1 - x0, 0.0, 0.0],
        };
        let set = |edges: Vec<Reference>| EdgeSet {
            edges: Some(edges),
            ..EdgeSet::default()
        };
        let mut detail = Detail::Fillet(Box::new(FilletDetail {
            edge_sets: Some(vec![
                set(vec![
                    // The edge itself, a longer stored edge (mitcad#305),
                    // a piece of the edge (the edge runs beyond it) and an
                    // edge elsewhere.
                    input(1, "1", Some(line(0.0, 1.0, 0.0))),
                    input(2, "2", Some(line(-1.0, 1.0, 0.0))),
                    input(3, "3", Some(line(0.0, 0.5, 0.0))),
                ]),
                set(vec![
                    input(4, "4", Some(line(0.0, 1.0, 2.0))),
                    // Without a curve (a face's edge): not judged.
                    input(5, "5", None),
                ]),
            ]),
            ..FilletDetail::default()
        }));
        let mut out = Resolved {
            found: 5,
            not_found: 0,
        };
        drop_conflicting(&mut detail, &mut out);
        assert_eq!(
            out,
            Resolved {
                found: 3,
                not_found: 2
            }
        );
        let Detail::Fillet(f) = &detail else {
            unreachable!()
        };
        let found: Vec<(u64, bool)> = f
            .edge_sets
            .iter()
            .flatten()
            .flat_map(|s| s.edges.iter().flatten())
            .map(|r| match r {
                Reference::Edge(fp) => (
                    fp.f3d.as_ref().unwrap().object_id.unwrap(),
                    fp.mid_point.is_some(),
                ),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            found,
            vec![(1, true), (2, true), (3, false), (4, false), (5, true)]
        );
        // Two names of one stored edge: both keep it.
        let mut detail = Detail::Fillet(Box::new(FilletDetail {
            edge_sets: Some(vec![set(vec![
                input(1, "1", Some(line(0.0, 0.5, 2.0))),
                input(2, "2", Some(line(0.0, 0.5, 2.0))),
            ])]),
            ..FilletDetail::default()
        }));
        let mut out = Resolved {
            found: 2,
            not_found: 0,
        };
        drop_conflicting(&mut detail, &mut out);
        assert_eq!(out.found, 2);
    }

    /// Where the names fit two edges, the curve the input stores decides
    /// (mitcad#306): the stored edge itself, or the only piece of it; a
    /// curve that fits none of them or both leaves the choice as before.
    #[test]
    fn picks_the_edge_its_stored_curve_gives() {
        use super::super::curve::StoredCurve;
        let data = testdata::named_cube_blob();
        let file = AsmFile::parse(&data).unwrap();
        let state = NamedState::new(&file, None);
        let entities = vec![
            vec![n("1"), n("2")],
            vec![n("3")],
            vec![n("6")],
            vec![n("4")],
        ];
        let all = state.edges_fitting(&entities).unwrap();
        assert_eq!(all.len(), 2);
        for &e in &all {
            let g = state.edge(e).unwrap();
            // The whole edge, either way round.
            for (a, b) in [(g.start, g.end), (g.end, g.start)] {
                let line = StoredCurve::Line {
                    start: a,
                    vector: brep::sub(b, a),
                };
                let picked = pick_edge(
                    &state,
                    &entities,
                    None,
                    (0, 1),
                    std::slice::from_ref(&line),
                    true,
                );
                assert_eq!(
                    picked.map(|p| (p.edge, p.pieces, p.how)),
                    Ok((e, vec![], " (by its stored curve)"))
                );
            }
            // A longer edge on the same line (a later item shortened it).
            let d = brep::sub(g.end, g.start);
            let longer = StoredCurve::Line {
                start: brep::sub(g.start, d),
                vector: brep::scale(d, 3.0),
            };
            let picked = pick_edge(
                &state,
                &entities,
                None,
                (0, 1),
                std::slice::from_ref(&longer),
                true,
            );
            assert_eq!(picked.map(|p| p.edge), Ok(e));
            // Its fit is noted in the fingerprint.
            let mut fp = Fingerprint {
                f3d: Some(super::super::ir::FingerprintF3d::default()),
                ..Fingerprint::default()
            };
            note_curve_fit(&mut fp, std::slice::from_ref(&longer), &g);
            assert_eq!(
                fp.f3d.unwrap().curve_fit.as_deref(),
                Some("a piece of the stored edge")
            );
        }
        // A curve neither edge lies on: the names alone, as before.
        let elsewhere = StoredCurve::Line {
            start: [100.0, 100.0, 100.0],
            vector: [1.0, 0.0, 0.0],
        };
        let why = pick_edge(
            &state,
            &entities,
            None,
            (0, 1),
            std::slice::from_ref(&elsewhere),
            true,
        )
        .unwrap_err();
        assert!(
            why.starts_with(
                "2 edges fit the names (to the stored curves: off the stored curve by "
            ),
            "{why}"
        );
        assert_eq!(why.matches("off the stored curve").count(), 2, "{why}");
    }

    /// An input that stores a curve per piece of the edge it saw is every
    /// edge on them (mitcad#106): the two edges its names fit, each the
    /// stored edge of one of its two curves, are both a fillet's input; the
    /// first is the input's own. A curve no edge lies on, an input of
    /// another item, or edges on one curve longer together than it leave
    /// the choice as before.
    #[test]
    fn an_input_stored_in_pieces_is_every_edge_on_them() {
        use super::super::curve::StoredCurve;
        let data = testdata::named_cube_blob();
        let file = AsmFile::parse(&data).unwrap();
        let state = NamedState::new(&file, None);
        let entities = vec![
            vec![n("1"), n("2")],
            vec![n("3")],
            vec![n("6")],
            vec![n("4")],
        ];
        let all = state.edges_fitting(&entities).unwrap();
        assert_eq!(all.len(), 2);
        let line = |e: usize| {
            let g = state.edge(e).unwrap();
            StoredCurve::Line {
                start: g.start,
                vector: brep::sub(g.end, g.start),
            }
        };
        let curves = [line(all[1]), line(all[0])];
        let picked = pick_edge(&state, &entities, None, (0, 1), &curves, true).unwrap();
        assert_eq!(picked.edge, all[0]);
        assert_eq!(picked.pieces, vec![all[1]]);
        assert_eq!(picked.how, " (one of the edges its stored curves give)");
        // Not for another item's input: two edges are each a stored edge,
        // which the curves alone cannot choose between.
        let why = pick_edge(&state, &entities, None, (0, 1), &curves, false).unwrap_err();
        assert!(
            why.contains("the stored edge (curve 2), the stored edge (curve 1)"),
            "{why}"
        );
        // A stored curve no edge lies on: not the input's pieces; the one
        // edge on a curve is the input's.
        let elsewhere = StoredCurve::Line {
            start: [100.0, 100.0, 100.0],
            vector: [1.0, 0.0, 0.0],
        };
        let curves = [line(all[1]), elsewhere];
        assert_eq!(by_stored_pieces(&state, &all, &curves), None);
        let picked = pick_edge(&state, &entities, None, (0, 1), &curves, true).unwrap();
        assert_eq!((picked.edge, picked.pieces.len()), (all[1], 0));
        // One stored edge and only one edge on it: not pieces.
        let one = line(all[0]);
        assert_eq!(
            by_stored_pieces(&state, &all, std::slice::from_ref(&one)),
            None
        );
        // Pieces of one curve (10 mm): none the whole edge, together no
        // longer than it (two copies of one edge are).
        let g = state.edge(all[0]).unwrap();
        assert!((one.length().unwrap() - g.length).abs() < 1e-9);
        let ten = StoredCurve::Line {
            start: [0.0; 3],
            vector: [10.0, 0.0, 0.0],
        };
        assert!(are_pieces(&ten, &[(1, 4.0), (1, 6.0)]));
        assert!(are_pieces(&ten, &[(1, 4.0), (1, 3.0)]));
        assert!(!are_pieces(&ten, &[(1, 6.0), (1, 6.0)]));
        assert!(!are_pieces(&ten, &[(0, 10.0), (0, 10.0)]));
        // An arc of a quarter turn of radius 2: π mm.
        let arc = StoredCurve::Arc {
            center: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            ref_dir: [1.0, 0.0, 0.0],
            radius: 2.0,
            range: [0.0, std::f64::consts::FRAC_PI_2],
        };
        assert!((arc.length().unwrap() - std::f64::consts::PI).abs() < 1e-12);
        // A whole circle as an ellipse of equal radii 3: 6π mm, the
        // polyline within 1e-4.
        let circle = StoredCurve::Ellipse {
            center: [0.0; 3],
            major_dir: [1.0, 0.0, 0.0],
            minor_dir: [0.0, 1.0, 0.0],
            major: 3.0,
            minor: 3.0,
            range: [0.0, std::f64::consts::TAU],
        };
        let l = circle.length().unwrap();
        assert!((l - 6.0 * std::f64::consts::PI).abs() < 1e-4, "{l}");
    }

    /// The edges a fillet's input is besides its own ([`locate`] keeps them
    /// in the fingerprint) become inputs of their own after it in its set,
    /// with its `found`; one the set has already is not added again.
    #[test]
    fn the_pieces_of_an_input_become_inputs_of_their_own() {
        use super::super::ir::{EdgeSet, FilletDetail, FingerprintF3d};
        let edge = |x: f64| Fingerprint {
            object_type: Some("BRepEdge".to_owned()),
            mid_point: Some([x, 0.0, 0.0]),
            length: Some(1.0),
            f3d: Some(FingerprintF3d {
                object_id: Some(7),
                ..FingerprintF3d::default()
            }),
            ..Fingerprint::default()
        };
        let mut own = edge(0.5);
        let f = own.f3d.as_mut().unwrap();
        f.found = Some("state 3 (one of the edges its stored curves give)".to_owned());
        f.other.insert(
            PIECES.to_owned(),
            Value::Array(vec![
                serde_json::to_value(edge(2.5)).unwrap(),
                serde_json::to_value(edge(4.5)).unwrap(),
                // An edge another input of the set has: there once.
                serde_json::to_value(edge(9.0)).unwrap(),
            ]),
        );
        let mut detail = Detail::Fillet(Box::new(FilletDetail {
            edge_sets: Some(vec![EdgeSet {
                edges: Some(vec![
                    Reference::Edge(Box::new(own)),
                    Reference::Edge(Box::new(edge(9.0))),
                ]),
                ..EdgeSet::default()
            }]),
            ..FilletDetail::default()
        }));
        let mut out = Resolved {
            found: 2,
            not_found: 0,
        };
        split_pieces(&mut detail, &mut out);
        assert_eq!(out.found, 4);
        let Detail::Fillet(f) = &detail else {
            unreachable!()
        };
        let edges: Vec<(f64, Option<String>, bool)> = f.edge_sets.as_ref().unwrap()[0]
            .edges
            .iter()
            .flatten()
            .map(|r| match r {
                Reference::Edge(fp) => {
                    let f = fp.f3d.as_ref().unwrap();
                    (
                        fp.mid_point.unwrap()[0],
                        f.found.clone(),
                        f.other.contains_key(PIECES),
                    )
                }
                _ => unreachable!(),
            })
            .collect();
        let found = Some("state 3 (one of the edges its stored curves give)".to_owned());
        assert_eq!(
            edges,
            vec![
                (0.5, found.clone(), false),
                (2.5, found.clone(), false),
                (4.5, found, false),
                (9.0, None, false),
            ]
        );
    }

    fn open_input<'f>(
        fp: &'f mut Fingerprint,
        entities: Vec<Vec<EntityName>>,
        set: usize,
    ) -> Input<'f> {
        Input {
            fp,
            twin: (0, 1),
            set: Some(set),
            entities,
            why: String::new(),
            hit: None,
            edge: None,
            pieces: Vec::new(),
            how: "",
        }
    }

    /// Inputs whose names differ (in the faces at the ends, which tell
    /// nothing here) but fit the same two edges, as many as the edges, take
    /// one each; edges other inputs took are left out first; inputs of
    /// other edge sets share nothing (mitcad#186).
    #[test]
    fn inputs_that_fit_the_same_edges_take_one_each() {
        let data = testdata::named_cube_blob();
        let file = AsmFile::parse(&data).unwrap();
        let state = NamedState::new(&file, None);
        // The bottom front and the top front edge fit; an end face that
        // is no face of the state stands for any.
        let front = |end: &str| {
            vec![
                vec![n("1"), n("2")],
                vec![n("3")],
                vec![n(end)],
                vec![n(end)],
            ]
        };
        let view = (0, 0);
        let (mut a, mut b) = (Fingerprint::default(), Fingerprint::default());
        let mut inputs = [
            open_input(&mut a, front("98"), 0),
            open_input(&mut b, front("99"), 0),
        ];
        resolve_alike(&state, &mut inputs, view);
        assert!(inputs.iter().all(|i| i.hit == Some(view)));
        let (x, y) = (inputs[0].edge.unwrap(), inputs[1].edge.unwrap());
        assert_ne!(x, y);
        let heights: Vec<f64> = [x, y]
            .iter()
            .map(|&e| state.edge(e).unwrap().mid[2])
            .collect();
        assert!(heights.iter().any(|z| z.abs() < 1e-9));
        assert!(heights.iter().any(|z| (z - 10.0).abs() < 1e-9));
        assert!(a.mid_point.is_some());
        // More inputs than the edges that fit, or inputs of two sets:
        // nothing is decided.
        let (mut a, mut b, mut c) = (
            Fingerprint::default(),
            Fingerprint::default(),
            Fingerprint::default(),
        );
        let mut inputs = [
            open_input(&mut a, front("97"), 0),
            open_input(&mut b, front("98"), 0),
            open_input(&mut c, front("99"), 0),
        ];
        resolve_alike(&state, &mut inputs, view);
        assert!(inputs.iter().all(|i| i.hit.is_none()));
        let (mut a, mut b) = (Fingerprint::default(), Fingerprint::default());
        let mut inputs = [
            open_input(&mut a, front("98"), 0),
            open_input(&mut b, front("99"), 1),
        ];
        resolve_alike(&state, &mut inputs, view);
        assert!(inputs.iter().all(|i| i.hit.is_none()));
        // An input that found its edge already leaves the other no choice.
        let (mut a, mut b) = (Fingerprint::default(), Fingerprint::default());
        let all = state.edges_fitting(&front("98")).unwrap();
        let mut inputs = [
            open_input(&mut a, front("98"), 0),
            open_input(&mut b, front("99"), 0),
        ];
        inputs[0].hit = Some(view);
        inputs[0].edge = Some(all[0]);
        resolve_alike(&state, &mut inputs, view);
        assert_eq!(inputs[1].edge, Some(all[1]));
    }

    /// A circle's start is read from the closed edge round the cylinder
    /// (radius 10 mm, from z = 0 to 20), as an angle in the sketch's own
    /// axes, at either end of the extrusion.
    #[test]
    fn a_circle_starts_where_the_closed_edge_does() {
        let data = testdata::cylinder_blob();
        let file = AsmFile::parse(&data).unwrap();
        let state = NamedState::new(&file, None);
        let closed = closed_circles(&state);
        assert_eq!(closed.len(), 2, "the bottom and the top circle");
        let sketch = |x: [f64; 3], y: [f64; 3]| SeamSketch {
            circles: vec![SeamCircle {
                curve: 7,
                center: [0.0, 0.0, 0.0],
                radius: 10.0,
            }],
            x,
            y,
        };
        let angle_of = |sk: &SeamSketch| {
            let hits = seam_candidates(&closed, sk);
            assert_eq!(hits.len(), 2, "{hits:?}");
            // The one in the sketch's plane (z = 0) and the other end.
            let near = hits.iter().find(|h| h.1 < 1e-9).unwrap();
            let far = hits.iter().find(|h| (h.1 - 20.0).abs() < 1e-9).unwrap();
            assert_eq!((near.0, far.0), (7, 7));
            assert!((near.2 - far.2).abs() < 1e-9, "one seam for both ends");
            near.2
        };
        let start = state.edge(0).unwrap().start;
        // The start vertex's direction from the axis, in the world axes.
        let (sx, sy) = (start[0], start[1]);
        assert!((sx.hypot(sy) - 10.0).abs() < 1e-9);
        let expected = sy.atan2(sx);
        let a = angle_of(&sketch([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
        assert!((a - expected).abs() < 1e-9 || (a.abs() - std::f64::consts::PI).abs() < 1e-9);
        // The sketch's axes turned a quarter turn: the angle follows.
        let turned = angle_of(&sketch([0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]));
        let wrapped = (expected - std::f64::consts::FRAC_PI_2 + std::f64::consts::PI)
            .rem_euclid(std::f64::consts::TAU)
            - std::f64::consts::PI;
        assert!(
            (turned - wrapped).abs() < 1e-9 || (turned.abs() - std::f64::consts::PI).abs() < 1e-9
        );
        // A sketch in another plane (across x), or a circle of another
        // radius, finds none.
        assert!(seam_candidates(&closed, &sketch([0.0, 1.0, 0.0], [0.0, 0.0, 1.0])).is_empty());
        let mut other = sketch([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        other.circles[0].radius = 5.0;
        assert!(seam_candidates(&closed, &other).is_empty());
    }

    #[test]
    fn a_seam_angle_is_zero_on_the_axis_and_pi_opposite() {
        let (x, y) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert_eq!(seam_angle([5.0, 1e-12, 0.0], x, y), 0.0);
        assert_eq!(seam_angle([-5.0, -1e-12, 0.0], x, y), std::f64::consts::PI);
        assert_eq!(seam_angle([-5.0, 1e-12, 0.0], x, y), std::f64::consts::PI);
        assert!((seam_angle([0.0, 5.0, 0.0], x, y) - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!((seam_angle([0.0, -5.0, 0.0], x, y) + std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    }

    /// Documents that share a GUID are told apart by the placement the
    /// entity's slot carries: the document whose occurrence is there.
    #[test]
    fn documents_sharing_a_guid_are_told_apart_by_their_placement() {
        let at = |x: f64| -> Mat4 {
            [
                [1.0, 0.0, 0.0, x],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ]
        };
        let placements = vec![(at(0.0), "aa".to_owned()), (at(10.0), "bb".to_owned())];
        let keys = ["aa", "bb"];
        // Within the files' rounding.
        let near = at(10.0 + 1e-4);
        assert_eq!(pick_by_placement(&keys, Some(&near), &placements), Some(1));
        assert_eq!(
            pick_by_placement(&keys, Some(&at(0.0)), &placements),
            Some(0)
        );
        // Nothing is there, or no placement is given: the first.
        assert_eq!(
            pick_by_placement(&keys, Some(&at(5.0)), &placements),
            Some(0)
        );
        assert_eq!(pick_by_placement(&keys, None, &placements), Some(0));
        // A document alone, or none.
        assert_eq!(
            pick_by_placement(&["bb"], Some(&at(0.0)), &placements),
            Some(0)
        );
        assert_eq!(pick_by_placement(&[], None, &placements), None);
    }
}
