// SPDX-License-Identifier: MIT
//! Faces, edges and vertices found again after a sketch change renamed
//! them (mitcad#272).
//!
//! A side face is named by its sketch segment: the curve and the curves it
//! ends on (`F3:side(c1[c4,c2])`). When a sketch edit gives the segment
//! another neighbour, the face gets a new name although it is the same
//! face, and every reference to it would fail. Profile regions
//! (`sketch/regions.rs` `best_match`) and face appearances
//! (`document/bodies.rs`) already find such a face again; this module is
//! the one rule for features, used wherever a feature resolves a face,
//! edge or vertex name on a body (`docs/architecture.md`, "Topological
//! naming"):
//!
//! 1. A name that finds what it names is used as it is (a face name
//!    without `#k` finds every piece).
//! 2. A face name that finds no face finds the faces of the same feature
//!    and role whose key has most in common with it ([`likeness`]): a side
//!    face the same own curve and most of its end curves, a region most of
//!    its curves. A reference takes the best match only when it is the
//!    only one; a tie is an error that lists the candidates.
//! 3. An edge name that finds no edge finds its two faces each by rule 2
//!    and takes the edges between them: one with `#k`, all without. Of
//!    tied faces only those that meet the other face count.
//! 4. A vertex finds its faces each by rule 2.
//! 5. The stored name stays as it is; the feature warns that it found the
//!    face, edge or vertex under another name.
//!
//! An edge name that finds nothing by these rules still fails; its error
//! lists a few existing edges that share one of its faces as suggestions
//! (mitcad#1187, [`with_edge_suggestions`]), never taken in its place.

use std::str::FromStr;
use std::sync::Arc;

use super::face_refs::missing_face;
use super::{EvalContext, FeatureDef, FeatureEntry};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{Kernel, KernelError};
use crate::topo::{CurveId, EdgeName, FaceName, RoleKey, SegmentKey, TopoName, VertexName};

/// How much a face's name has in common with a reference that names no
/// face, best last: the curves the keys share and, as a tie-break, the
/// curves at the same end of a segment. None unless the feature and the
/// role are the same and the keys share a curve (a side face: its own
/// curve). Keys that hold names (a fillet's edge, a shell's face) compare
/// those names by the same rule.
pub(crate) fn likeness(reference: &FaceName, name: &FaceName) -> Option<(usize, usize)> {
    if reference.feature != name.feature || reference.role != name.role {
        return None;
    }
    key_likeness(&reference.key, &name.key)
}

fn key_likeness(reference: &Option<RoleKey>, name: &Option<RoleKey>) -> Option<(usize, usize)> {
    match (reference, name) {
        (None, None) => Some((1, 0)),
        (Some(RoleKey::Segment(a)), Some(RoleKey::Segment(b))) => {
            if a.curve != b.curve {
                return None;
            }
            let a_curves: Vec<CurveId> = a.curves().collect();
            let b_curves: Vec<CurveId> = b.curves().collect();
            Some((shared(&a_curves, &b_curves), ends_in_place(a, b)))
        }
        (Some(RoleKey::Region(a)), Some(RoleKey::Region(b))) => {
            let a: Vec<CurveId> = a.segments().flat_map(|s| s.curves()).collect();
            let b: Vec<CurveId> = b.segments().flat_map(|s| s.curves()).collect();
            let shared = shared(&a, &b);
            (shared > 0).then_some((shared, 0))
        }
        (Some(RoleKey::Face(a)), Some(RoleKey::Face(b))) => nested_likeness(a, b),
        (Some(RoleKey::Edge(a)), Some(RoleKey::Edge(b))) => edge_likeness(a, b),
        (Some(RoleKey::Vertex(a)), Some(RoleKey::Vertex(b))) => vertex_likeness(a, b),
        (Some(RoleKey::Index(a)), Some(RoleKey::Index(b))) if a == b => Some((1, 0)),
        _ => None,
    }
}

/// The curves of `a` that `b` has too, counted as `a` lists them.
fn shared(a: &[CurveId], b: &[CurveId]) -> usize {
    a.iter().filter(|c| b.contains(c)).count()
}

/// The curves two segments of one curve have at the same end.
fn ends_in_place(a: &SegmentKey, b: &SegmentKey) -> usize {
    match (&a.ends, &b.ends) {
        (Some(a), Some(b)) => shared(&a.0, &b.0) + shared(&a.1, &b.1),
        _ => 0,
    }
}

/// A name inside a key: the same piece of a face like it.
fn nested_likeness(a: &FaceName, b: &FaceName) -> Option<(usize, usize)> {
    if a.split != b.split {
        return None;
    }
    likeness(a, b)
}

fn add(a: (usize, usize), b: (usize, usize)) -> (usize, usize) {
    (a.0 + b.0, a.1 + b.1)
}

fn edge_likeness(a: &EdgeName, b: &EdgeName) -> Option<(usize, usize)> {
    if a.index != b.index {
        return None;
    }
    let [a0, a1] = a.faces();
    let [b0, b1] = b.faces();
    let pair = |x: &FaceName, y: &FaceName, z: &FaceName, w: &FaceName| {
        Some(add(nested_likeness(x, y)?, nested_likeness(z, w)?))
    };
    // Renaming may change the faces' order in the name.
    pair(a0, b0, a1, b1).max(pair(a0, b1, a1, b0))
}

fn vertex_likeness(a: &VertexName, b: &VertexName) -> Option<(usize, usize)> {
    if a.index != b.index || a.faces().len() != b.faces().len() {
        return None;
    }
    let mut total = (0, 0);
    for face in a.faces() {
        let best = b
            .faces()
            .iter()
            .filter_map(|other| nested_likeness(face, other))
            .max()?;
        total = add(total, best);
    }
    Some(total)
}

/// The faces `reference` finds among the faces (each face's names), as
/// face appearances take them: those it names (all pieces without `#k`),
/// else those of the same feature and role most like it, every one of
/// them (each face's index and the name of it that it finds).
pub(crate) fn faces_found<'a>(
    faces: &'a [Vec<FaceName>],
    reference: &FaceName,
) -> Vec<(usize, &'a FaceName)> {
    let exact: Vec<(usize, &FaceName)> = faces
        .iter()
        .enumerate()
        .filter_map(|(i, names)| names.iter().find(|n| n.matches(reference)).map(|n| (i, n)))
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    let scored: Vec<((usize, usize), usize, &FaceName)> = faces
        .iter()
        .enumerate()
        .filter_map(|(i, names)| {
            names
                .iter()
                .filter_map(|n| likeness(reference, n).map(|score| (score, i, n)))
                .max_by_key(|(score, _, _)| *score)
        })
        .collect();
    let Some(best) = scored.iter().map(|(score, _, _)| *score).max() else {
        return Vec::new();
    };
    scored
        .into_iter()
        .filter(|(score, _, _)| *score == best)
        .map(|(_, i, n)| (i, n))
        .collect()
}

/// The names a reference that names no face may stand for (rule 2): the
/// names, in text order, of the faces most like it, cut to the reference's
/// piece numbers (so that the pieces of one renamed face count once).
fn renamed_candidates(faces: &[Vec<FaceName>], reference: &FaceName) -> Vec<FaceName> {
    let mut best: Option<(usize, usize)> = None;
    let mut found: Vec<FaceName> = Vec::new();
    for name in faces.iter().flatten() {
        if !name.split.starts_with(&reference.split) {
            continue;
        }
        let Some(score) = likeness(reference, name) else {
            continue;
        };
        let mut base = name.clone();
        base.split.truncate(reference.split.len());
        match best {
            Some(b) if score < b => {}
            Some(b) if score == b => {
                if !found.contains(&base) {
                    found.push(base);
                }
            }
            _ => {
                best = Some(score);
                found = vec![base];
            }
        }
    }
    found.sort();
    found
}

/// The names of the shape's faces, each face's together; none when the
/// kernel cannot list them.
fn face_names<K: Kernel>(kernel: &K, shape: &K::Shape) -> Vec<Vec<FaceName>> {
    kernel
        .faces(shape)
        .unwrap_or_default()
        .into_iter()
        .map(|face| {
            face.names
                .iter()
                .filter_map(|n| FaceName::from_str(n).ok())
                .collect()
        })
        .collect()
}

/// Why a name finds nothing a feature can use.
#[derive(Debug, Clone, PartialEq)]
pub enum Lost {
    /// Nothing by the name and nothing like it: the caller explains it, as
    /// before (`missing_face`, `missing_edge`).
    Gone,
    /// Several equally like it after a sketch change: the message lists
    /// them.
    Ambiguous(String),
    /// The kernel failed to look.
    Failed(String),
}

impl Lost {
    /// The message, with `gone` for a name that is gone.
    pub(crate) fn message(self, gone: impl FnOnce() -> String) -> String {
        match self {
            Self::Gone => gone(),
            Self::Ambiguous(message) | Self::Failed(message) => message,
        }
    }
}

fn ambiguous(name: &dyn std::fmt::Display, candidates: &[String]) -> Lost {
    Lost::Ambiguous(format!(
        "{name} is ambiguous after a sketch change: {}",
        candidates.join(", ")
    ))
}

/// The faces a face name of a feature reference can stand for on the
/// shape: itself when it finds a face (or the kernel cannot tell), else
/// the faces most like it.
struct Candidates {
    exact: bool,
    faces: Vec<FaceName>,
}

/// Looks the shape's faces up once, on the first name that needs them.
struct Faces<'a, K: Kernel> {
    kernel: &'a K,
    shape: &'a K::Shape,
    listed: Option<Vec<Vec<FaceName>>>,
}

impl<'a, K: Kernel> Faces<'a, K> {
    fn new(kernel: &'a K, shape: &'a K::Shape) -> Self {
        Self {
            kernel,
            shape,
            listed: None,
        }
    }

    fn candidates(&mut self, face: &FaceName) -> Result<Candidates, Lost> {
        match self.kernel.count_faces(self.shape, face) {
            Ok(0) => {}
            Ok(_) | Err(KernelError::Unsupported(_)) => {
                return Ok(Candidates {
                    exact: true,
                    faces: vec![face.clone()],
                });
            }
            Err(error) => return Err(Lost::Failed(error.to_string())),
        }
        let (kernel, shape) = (self.kernel, self.shape);
        let listed = self.listed.get_or_insert_with(|| face_names(kernel, shape));
        Ok(Candidates {
            exact: false,
            faces: renamed_candidates(listed, face),
        })
    }

    fn has_edge(&self, a: &FaceName, b: &FaceName, index: Option<u32>) -> Option<EdgeName> {
        let mut edge = EdgeName::new(a.clone(), b.clone());
        edge.index = index;
        (self.kernel.count_edges(self.shape, &edge) > 0).then_some(edge)
    }
}

fn texts<T: ToString>(names: &[T]) -> Vec<String> {
    names.iter().map(ToString::to_string).collect()
}

/// Rule 2 for a face: None when the name finds its face (or the kernel
/// cannot tell), else the one face most like it.
pub fn find_face<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    face: &FaceName,
) -> Result<Option<FaceName>, Lost> {
    let found = Faces::new(kernel, shape).candidates(face)?;
    if found.exact {
        return Ok(None);
    }
    match found.faces.as_slice() {
        [] => Err(Lost::Gone),
        [one] => Ok(Some(one.clone())),
        several => Err(ambiguous(face, &texts(several))),
    }
}

/// Rule 3 for an edge: None when the name finds its edges, else the edge
/// name between the faces its faces find.
pub(crate) fn find_edge<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    edge: &EdgeName,
) -> Result<Option<EdgeName>, Lost> {
    if kernel.count_edges(shape, edge) > 0 {
        return Ok(None);
    }
    let mut faces = Faces::new(kernel, shape);
    let [a, b] = edge.faces();
    let a = faces.candidates(a)?;
    let b = faces.candidates(b)?;
    if a.exact && b.exact {
        return Err(Lost::Gone);
    }
    let mut found: Vec<EdgeName> = Vec::new();
    for x in &a.faces {
        for y in &b.faces {
            if let Some(between) = faces.has_edge(x, y, edge.index)
                && !found.contains(&between)
            {
                found.push(between);
            }
        }
    }
    found.sort();
    match found.as_slice() {
        [] => Err(Lost::Gone),
        [one] => Ok(Some(one.clone())),
        several => Err(ambiguous(edge, &texts(several))),
    }
}

/// At most this many suggestions follow an edge name that finds no edge
/// (mitcad#1187).
const EDGE_SUGGESTIONS: usize = 5;

/// An existing edge that an edge name finding none may have meant: how
/// plausible it is (0 best), its name, and the face it has in place of
/// one of the name's faces with that face of the name (none for an edge
/// between the same faces).
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Suggestion {
    rank: u8,
    edge: String,
    instead: Option<(String, String)>,
}

/// True when the key of `face` names `target` (a chamfer or fillet made
/// on an edge of it, a shell's offset of it), at any depth.
fn mentions(face: &FaceName, target: &FaceName) -> bool {
    let nested = |f: &FaceName| f.matches(target) || mentions(f, target);
    match &face.key {
        Some(RoleKey::Edge(edge)) => edge.faces().iter().any(nested),
        Some(RoleKey::Vertex(vertex)) => vertex.faces().iter().any(nested),
        Some(RoleKey::Face(inner)) => nested(inner.as_ref()),
        _ => false,
    }
}

/// The face a face of an edge name stands for on the shape (rules 1 and
/// 2): itself when it finds a face, else the one face most like it; None
/// when it finds none or several.
fn resolved<K: Kernel>(faces: &mut Faces<'_, K>, face: &FaceName) -> Option<FaceName> {
    let found = faces.candidates(face).ok()?;
    if found.exact {
        return Some(face.clone());
    }
    match found.faces.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
}

/// `candidate` as a suggestion for `edge`, whose faces were found as
/// `found` (None where not found): None unless it has one of them. Best
/// are edges between both faces found (another `#k`), then those whose
/// other face was made on the name's other face, then those whose other
/// face is like it (rule 2), then the rest.
fn suggestion(
    edge: &EdgeName,
    found: &[Option<FaceName>; 2],
    candidate: &EdgeName,
) -> Option<Suggestion> {
    let [p, q] = candidate.faces();
    for (i, face) in found.iter().enumerate() {
        let Some(face) = face else {
            continue;
        };
        let other = if p.matches(face) {
            q
        } else if q.matches(face) {
            p
        } else {
            continue;
        };
        let missing = &edge.faces()[1 - i];
        let other_found = found[1 - i].as_ref();
        let rank = if other_found.is_some_and(|f| other.matches(f)) {
            0
        } else if mentions(other, missing) || other_found.is_some_and(|f| mentions(other, f)) {
            1
        } else if likeness(missing, other).is_some() {
            2
        } else {
            3
        };
        return Some(Suggestion {
            rank,
            edge: candidate.to_string(),
            instead: (rank > 0).then(|| (other.to_string(), missing.to_string())),
        });
    }
    None
}

/// The existing edges of the shape that share a face with an edge name
/// that finds no edge, best first, at most [`EDGE_SUGGESTIONS`], and how
/// many more there are. Nothing when the name finds its edges, neither of
/// its faces is found (rules 1 and 2) or the kernel cannot list edges.
/// Bounded on large bodies: one listing of the edges, of which only the
/// names that hold a found face's text are read, and only the best kept.
fn edge_suggestions<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    edge: &EdgeName,
) -> (Vec<Suggestion>, usize) {
    let none = (Vec::new(), 0);
    if kernel.count_edges(shape, edge) > 0 {
        return none;
    }
    let mut faces = Faces::new(kernel, shape);
    let [a, b] = edge.faces();
    let found = [resolved(&mut faces, a), resolved(&mut faces, b)];
    let texts: Vec<String> = found.iter().flatten().map(ToString::to_string).collect();
    if texts.is_empty() {
        return none;
    }
    let Ok(listed) = kernel.edges(shape) else {
        return none;
    };
    let mut best: Vec<Suggestion> = Vec::with_capacity(EDGE_SUGGESTIONS + 1);
    let mut total = 0;
    for info in listed {
        let Some(name) = info.name else {
            continue;
        };
        if !texts.iter().any(|t| name.contains(t.as_str())) {
            continue;
        }
        let Ok(candidate) = EdgeName::from_str(&name) else {
            continue;
        };
        let Some(item) = suggestion(edge, &found, &candidate) else {
            continue;
        };
        total += 1;
        if best.len() < EDGE_SUGGESTIONS || best.last().is_some_and(|last| item < *last) {
            let at = best.partition_point(|s| *s <= item);
            best.insert(at, item);
            best.truncate(EDGE_SUGGESTIONS);
        }
    }
    let more = total - best.len();
    (best, more)
}

/// `message` about an edge name that finds no edge of the body, followed
/// by the existing edges of the body that share one of its faces
/// (mitcad#1187), each with its face in place of the name's other face:
/// suggestions only, never taken in the name's place. The message alone
/// when there are none.
pub fn with_edge_suggestions<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    body: BodyUid,
    edge: &EdgeName,
    message: String,
) -> String {
    let (best, more) = edge_suggestions(kernel, shape, edge);
    if best.is_empty() {
        return message;
    }
    let items: Vec<String> = best
        .iter()
        .map(|s| match &s.instead {
            Some((other, missing)) => format!("{} ({other} instead of {missing})", s.edge),
            None => s.edge.clone(),
        })
        .collect();
    let more = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    format!(
        "{message}; existing edges of body {body} that share a face with it \
         (suggestions only, not applied): {}{more}",
        items.join(", ")
    )
}

/// Rule 4 for a vertex: None when every face of its name finds its face,
/// else the vertex of the faces found. Of tied faces only the one that
/// meets another face of the vertex counts.
pub(crate) fn find_vertex<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    vertex: &VertexName,
) -> Result<Option<VertexName>, Lost> {
    let mut faces = Faces::new(kernel, shape);
    let mut found = Vec::with_capacity(vertex.faces().len());
    for face in vertex.faces() {
        found.push(faces.candidates(face)?);
    }
    if found.iter().all(|c| c.exact) {
        return Ok(None);
    }
    let settled: Vec<FaceName> = found
        .iter()
        .filter(|c| c.faces.len() == 1)
        .map(|c| c.faces[0].clone())
        .collect();
    let mut chosen = Vec::with_capacity(found.len());
    for (face, candidates) in vertex.faces().iter().zip(&found) {
        match candidates.faces.as_slice() {
            [] => return Err(Lost::Gone),
            [one] => chosen.push(one.clone()),
            several => {
                let near: Vec<&FaceName> = several
                    .iter()
                    .filter(|x| settled.iter().any(|s| faces.has_edge(x, s, None).is_some()))
                    .collect();
                match near.as_slice() {
                    [one] => chosen.push((*one).clone()),
                    _ => return Err(ambiguous(face, &texts(several))),
                }
            }
        }
    }
    let mut renamed = VertexName::new(chosen).ok_or(Lost::Gone)?;
    renamed.index = vertex.index;
    Ok(Some(renamed))
}

/// Rules 2 to 4 for any name: None when it finds what it names, else the
/// name it is found as, with what it is (`face`, `edge`, `vertex`) and
/// the feature that made the renamed face.
pub(crate) fn find_name<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    name: &TopoName,
) -> Result<Option<(TopoName, &'static str, FeatureUid)>, Lost> {
    Ok(match name {
        TopoName::Face(face) => find_face(kernel, shape, face)?
            .map(|found| (TopoName::Face(found), "face", face.feature)),
        TopoName::Edge(edge) => find_edge(kernel, shape, edge)?.map(|found| {
            let maker =
                renamed_maker(edge.faces(), found.faces()).unwrap_or(edge.faces()[0].feature);
            (TopoName::Edge(found), "edge", maker)
        }),
        TopoName::Vertex(vertex) => find_vertex(kernel, shape, vertex)?.map(|found| {
            let maker =
                renamed_maker(vertex.faces(), found.faces()).unwrap_or(vertex.faces()[0].feature);
            (TopoName::Vertex(found), "vertex", maker)
        }),
    })
}

/// The feature that made the first face of `stored` that `found` renamed
/// (the face whose sketch changed).
fn renamed_maker<'a>(
    stored: impl IntoIterator<Item = &'a FaceName>,
    found: &[FaceName],
) -> Option<FeatureUid> {
    stored
        .into_iter()
        .find(|face| !found.contains(face))
        .map(|face| face.feature)
}

/// The sketch whose change renamed faces of `maker`: the sketch of its
/// profiles, else a sketch it refers to; else `maker` itself (by name).
fn changed_sketch(features: &[Arc<FeatureEntry>], maker: FeatureUid) -> String {
    let name = |uid: FeatureUid| {
        features
            .iter()
            .find(|f| f.uid == uid)
            .map_or_else(|| uid.to_string(), |f| f.name.clone())
    };
    let Some(entry) = features.iter().find(|f| f.uid == maker) else {
        return name(maker);
    };
    let info = entry.def.info();
    let sketch = info.profiles().first().map(|p| p.sketch).or_else(|| {
        info.references().features.into_iter().find(|uid| {
            features
                .iter()
                .any(|f| f.uid == *uid && matches!(f.def, FeatureDef::Sketch(_)))
        })
    });
    name(sketch.unwrap_or(maker))
}

/// The warning of a feature that found `stored` as `found` (rule 5), e.g.
/// `edge E{…} found as E{…} after a change in Sketch10`; the timeline
/// shows it after the feature's name.
pub(crate) fn warning(
    features: &[Arc<FeatureEntry>],
    what: &str,
    stored: &dyn std::fmt::Display,
    found: &dyn std::fmt::Display,
    maker: FeatureUid,
) -> String {
    let sketch = changed_sketch(features, maker);
    format!("{what} {stored} found as {found} after a change in {sketch}")
}

/// Names of features as features resolve them: what a feature reads is
/// recorded, and a name found by the fallback leaves a warning.
impl<K: Kernel> EvalContext<'_, K> {
    /// Warns that `stored` was found as `found` (rule 5).
    pub(crate) fn found_renamed(
        &mut self,
        what: &str,
        stored: &dyn std::fmt::Display,
        found: &dyn std::fmt::Display,
        maker: FeatureUid,
    ) {
        let message = warning(self.env.features, what, stored, found, maker);
        self.warn(message);
    }

    /// The name the kernel gets for a face of the body (rules 1, 2 and 5);
    /// a face that is gone is explained as before.
    pub fn face_on(
        &mut self,
        body: BodyUid,
        shape: &K::Shape,
        face: &FaceName,
    ) -> Result<FaceName, String> {
        match find_face(self.kernel, shape, face) {
            Ok(None) => Ok(face.clone()),
            Ok(Some(found)) => {
                self.found_renamed("face", face, &found, face.feature);
                Ok(found)
            }
            Err(lost) => Err(lost.message(|| missing_face(&*self, body, face))),
        }
    }

    /// [`EvalContext::face_on`] with the caller's message for a face that
    /// is gone.
    pub(crate) fn face_or(
        &mut self,
        shape: &K::Shape,
        face: &FaceName,
        gone: impl FnOnce() -> String,
    ) -> Result<FaceName, String> {
        match find_face(self.kernel, shape, face) {
            Ok(None) => Ok(face.clone()),
            Ok(Some(found)) => {
                self.found_renamed("face", face, &found, face.feature);
                Ok(found)
            }
            Err(lost) => Err(lost.message(gone)),
        }
    }

    /// [`EvalContext::face_on`] for each face.
    pub fn faces_on(
        &mut self,
        body: BodyUid,
        shape: &K::Shape,
        faces: &[FaceName],
    ) -> Result<Vec<FaceName>, String> {
        faces
            .iter()
            .map(|face| self.face_on(body, shape, face))
            .collect()
    }

    /// The name the kernel gets for an edge of the body (rules 1, 3 and 5);
    /// an edge that is gone is explained as before, with the edges it may
    /// have meant as suggestions (mitcad#1187).
    pub fn edge_on(
        &mut self,
        body: BodyUid,
        shape: &K::Shape,
        edge: &EdgeName,
    ) -> Result<EdgeName, String> {
        match find_edge(self.kernel, shape, edge) {
            Ok(None) => Ok(edge.clone()),
            Ok(Some(found)) => {
                let maker =
                    renamed_maker(edge.faces(), found.faces()).unwrap_or(edge.faces()[0].feature);
                self.found_renamed("edge", edge, &found, maker);
                Ok(found)
            }
            Err(lost) => Err(lost.message(|| self.missing_edge_on(body, shape, edge))),
        }
    }

    /// [`EvalContext::missing_edge`] followed by the existing edges of the
    /// shape that share a face with the name, as suggestions only
    /// ([`with_edge_suggestions`], mitcad#1187).
    pub(crate) fn missing_edge_on(
        &self,
        body: BodyUid,
        shape: &K::Shape,
        edge: &EdgeName,
    ) -> String {
        let message = self.missing_edge(body, edge);
        with_edge_suggestions(self.kernel, shape, body, edge, message)
    }

    /// [`EvalContext::edge_on`] for each edge.
    pub fn edges_on(
        &mut self,
        body: BodyUid,
        shape: &K::Shape,
        edges: &[EdgeName],
    ) -> Result<Vec<EdgeName>, String> {
        edges
            .iter()
            .map(|edge| self.edge_on(body, shape, edge))
            .collect()
    }

    /// The name the kernel gets for a vertex of the body (rules 1, 4 and
    /// 5); a vertex whose faces are gone is left to the kernel, as before.
    pub fn vertex_on(
        &mut self,
        shape: &K::Shape,
        vertex: &VertexName,
    ) -> Result<VertexName, String> {
        match find_vertex(self.kernel, shape, vertex) {
            Ok(None) | Err(Lost::Gone) => Ok(vertex.clone()),
            Ok(Some(found)) => {
                let maker = renamed_maker(vertex.faces(), found.faces())
                    .unwrap_or(vertex.faces()[0].feature);
                self.found_renamed("vertex", vertex, &found, maker);
                Ok(found)
            }
            Err(lost) => Err(lost.message(String::new)),
        }
    }
}

/// The same rules where a reference resolves through
/// [`super::geom_ref::Resolver`] (planes, axes and points on faces, edges
/// and vertices): a name that is gone goes to the kernel as it is, whose
/// error says so.
pub(crate) fn face_for<R: super::geom_ref::Resolver>(
    r: &mut R,
    shape: &<R::Kernel as Kernel>::Shape,
    face: &FaceName,
) -> Result<FaceName, String> {
    match find_face(r.kernel(), shape, face) {
        Ok(None) | Err(Lost::Gone) => Ok(face.clone()),
        Ok(Some(found)) => {
            r.renamed("face", &face.to_string(), &found.to_string(), face.feature);
            Ok(found)
        }
        Err(lost) => Err(lost.message(String::new)),
    }
}

pub(crate) fn edge_for<R: super::geom_ref::Resolver>(
    r: &mut R,
    shape: &<R::Kernel as Kernel>::Shape,
    edge: &EdgeName,
) -> Result<EdgeName, String> {
    match find_edge(r.kernel(), shape, edge) {
        Ok(None) | Err(Lost::Gone) => Ok(edge.clone()),
        Ok(Some(found)) => {
            let maker =
                renamed_maker(edge.faces(), found.faces()).unwrap_or(edge.faces()[0].feature);
            r.renamed("edge", &edge.to_string(), &found.to_string(), maker);
            Ok(found)
        }
        Err(lost) => Err(lost.message(String::new)),
    }
}

pub(crate) fn vertex_for<R: super::geom_ref::Resolver>(
    r: &mut R,
    shape: &<R::Kernel as Kernel>::Shape,
    vertex: &VertexName,
) -> Result<VertexName, String> {
    match find_vertex(r.kernel(), shape, vertex) {
        Ok(None) | Err(Lost::Gone) => Ok(vertex.clone()),
        Ok(Some(found)) => {
            let maker =
                renamed_maker(vertex.faces(), found.faces()).unwrap_or(vertex.faces()[0].feature);
            r.renamed("vertex", &vertex.to_string(), &found.to_string(), maker);
            Ok(found)
        }
        Err(lost) => Err(lost.message(String::new)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MockKernel, MockShape};

    fn face(text: &str) -> FaceName {
        text.parse().unwrap()
    }

    #[test]
    fn a_side_face_needs_its_own_curve_and_prefers_ends_in_place() {
        let reference = face("F3:side(c71[c104,c72])");
        assert_eq!(likeness(&reference, &face("F3:side(c72[c71,c73])")), None);
        assert_eq!(likeness(&reference, &face("F4:side(c71[c104,c72])")), None);
        assert_eq!(likeness(&reference, &face("F3:end(r{c71})")), None);
        // c71 and c104 shared; c104 at the start in both.
        assert_eq!(
            likeness(&reference, &face("F3:side(c71[c104,c103])")),
            Some((2, 1))
        );
        // The same curves, c104 at the other end.
        assert_eq!(
            likeness(&reference, &face("F3:side(c71[c103,c104])")),
            Some((2, 0))
        );
    }

    #[test]
    fn names_in_keys_follow_the_same_rule() {
        let reference = face("F5:fillet(E{F3:side(c1[c4,c2])|F3:side(c2[c1,c3])})");
        let renamed = face("F5:fillet(E{F3:side(c1[c4,c9])|F3:side(c2[c9,c3])})");
        assert!(likeness(&reference, &renamed).is_some());
        let other = face("F5:fillet(E{F3:side(c3[c2,c4])|F3:side(c4[c3,c1])})");
        assert_eq!(likeness(&reference, &other), None);
        let shell = face("F6:offset(F3:side(c1[c4,c2]))");
        assert!(likeness(&shell, &face("F6:offset(F3:side(c1[c4,c9]))")).is_some());
        assert_eq!(
            likeness(&shell, &face("F6:offset(F3:side(c1[c4,c9])#1)")),
            None
        );
    }

    #[test]
    fn candidates_are_unique_faces_most_like_the_reference() {
        let faces = |names: &[&str]| -> Vec<Vec<FaceName>> {
            names.iter().map(|n| vec![face(n)]).collect()
        };
        let reference = face("F3:side(c1[c4,c2])");
        // The pieces of one renamed face count once.
        let pieces = faces(&[
            "F3:side(c1[c4,c9])#0",
            "F3:side(c1[c4,c9])#1",
            "F3:end(r{c1})",
        ]);
        assert_eq!(
            renamed_candidates(&pieces, &reference),
            vec![face("F3:side(c1[c4,c9])")]
        );
        // Two pieces of the line, each as like it: both listed.
        let split = faces(&["F3:side(c1[c5,c7])", "F3:side(c1[c7,c6])"]);
        assert_eq!(renamed_candidates(&split, &reference).len(), 2);
        // A piece number of the reference must be kept.
        let piece = face("F3:side(c1[c4,c2])#1");
        assert!(renamed_candidates(&pieces, &piece).len() == 1);
        assert!(renamed_candidates(&faces(&["F3:side(c1[c4,c9])#0"]), &piece).is_empty());
    }

    /// A mock body of the faces and the edges between them.
    fn body(faces: &[&str], edges: &[(&str, &str)]) -> MockShape {
        MockShape {
            history: String::new(),
            faces: faces.iter().copied().map(face).collect(),
            edges: edges.iter().map(|(a, b)| (face(a), face(b))).collect(),
            bounds: None,
            parts: Vec::new(),
        }
    }

    /// The message `lost` about `edge` on the body, with its suggestions.
    fn suggested(shape: &MockShape, edge: &str) -> String {
        let edge: EdgeName = edge.parse().unwrap();
        let body = BodyUid::new(FeatureUid(2), 0);
        let kernel = MockKernel::default();
        with_edge_suggestions(&kernel, shape, body, &edge, "lost".to_owned())
    }

    /// The text before the suggestions.
    const HEAD: &str = "lost; existing edges of body F2.b0 that share a face with it \
                        (suggestions only, not applied): ";

    /// The edge between two faces, by its text.
    fn between(a: &str, b: &str) -> String {
        EdgeName::new(face(a), face(b)).to_string()
    }

    /// The suggestion of the edge between `kept` and `other`, which has
    /// `other` in place of `missing`.
    fn instead(kept: &str, other: &str, missing: &str) -> String {
        let o = face(other).to_string();
        let m = face(missing).to_string();
        format!("{} ({o} instead of {m})", between(kept, other))
    }

    /// One face of the name found: its one edge is suggested, with the face
    /// it meets in place of the name's other face.
    #[test]
    fn one_face_found_suggests_its_edge() {
        let shape = body(
            &["F2:a", "F2:c", "F2:d"],
            &[("F2:a", "F2:c"), ("F2:c", "F2:d")],
        );
        // F2:b is not on the body and nothing is like it.
        assert_eq!(
            suggested(&shape, "E{F2:a|F2:b}"),
            format!("{HEAD}{}", instead("F2:a", "F2:c", "F2:b"))
        );
    }

    #[test]
    fn several_suggestions_come_best_first_and_are_bounded() {
        let side = "F2:side(c1[c4,c2])";
        // The name's other face is gone; two faces tie for it (rule 2).
        let missing = "F2:side(c2[c1,c3])";
        let like = "F2:side(c2[c1,c7])";
        let tie = "F2:side(c2[c7,c3])";
        let chamfer = format!("F3:chamfer({})", between(side, missing));
        let rest = ["F6:a", "F6:b", "F6:c", "F6:d", "F6:e"];
        let mut faces = vec![side, like, tie, chamfer.as_str()];
        faces.extend(rest);
        let mut edges = vec![(side, like), (like, tie), (chamfer.as_str(), side)];
        edges.extend(rest.iter().map(|r| (side, *r)));
        let shape = body(&faces, &edges);
        // The face made on the missing edge first, then the face like the
        // missing one, then the rest by name: five of seven.
        let expected = [
            instead(side, &chamfer, missing),
            instead(side, like, missing),
            instead(side, "F6:a", missing),
            instead(side, "F6:b", missing),
            instead(side, "F6:c", missing),
        ];
        assert_eq!(
            suggested(&shape, &between(side, missing)),
            format!("{HEAD}{} and 2 more", expected.join(", "))
        );
    }

    #[test]
    fn another_edge_between_the_same_faces_comes_first() {
        let shape = body(
            &["F2:a", "F2:c", "F2:d"],
            &[("F2:a", "F2:d"), ("F2:a", "F2:c")],
        );
        // One edge lies between F2:a and F2:c: #1 finds none.
        assert_eq!(
            suggested(&shape, "E{F2:a|F2:c}#1"),
            format!(
                "{HEAD}{}, {}",
                between("F2:a", "F2:c"),
                instead("F2:a", "F2:d", "F2:c")
            )
        );
    }

    #[test]
    fn no_face_found_or_no_edge_there_suggests_nothing() {
        let shape = body(&["F2:a", "F2:c"], &[("F2:a", "F2:c")]);
        assert_eq!(suggested(&shape, "E{F2:x|F2:y}"), "lost");
        let lone = body(&["F2:a", "F2:c"], &[]);
        assert_eq!(suggested(&lone, "E{F2:a|F2:b}"), "lost");
    }

    #[test]
    fn an_edge_that_exists_gets_no_suggestions() {
        let shape = body(&["F2:a", "F2:c"], &[("F2:a", "F2:c")]);
        assert_eq!(suggested(&shape, "E{F2:a|F2:c}"), "lost");
    }
}
