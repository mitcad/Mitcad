// SPDX-License-Identifier: MIT
//! Replace Component (mitcad#2640): a component, in all its places or in
//! one, gives way to another design file, a library part or another
//! component of the design, and what held it stays on the replacement.
//!
//! The occurrences stay: each keeps its uid, its placement, grounding,
//! visibility and folder, and places the replacement instead. Joints,
//! assembly constraints, motion links, rigid groups, patterns and the
//! other features that name the occurrences by their paths therefore keep
//! naming them. What they name on the old component's geometry (the
//! origins of joints, as-built joints, constraint rows, bound lengths and
//! motion links) is re-pointed to the replacement's geometry:
//!
//! - by name: the replacement's face, edge or vertex of the same name,
//!   the old component's features read as the replacement's (a linked
//!   design's base feature as the other's, other features by name and
//!   type, then by their order among those of their type), when it is the
//!   same kind of geometry and no other reference is at its place;
//! - by place: the replacement's face, edge or vertex of the same kind at
//!   the old one's place (a plane in the same plane, a hole's round face
//!   on the same axis with the same radius, then on the same axis), in
//!   the component's coordinates, within 0.01 mm;
//! - else it stays unmatched: its geometry becomes a face of the
//!   replacement that no face is ([`FaceName::replaced`]), so the feature
//!   stays, fails, and is marked on the timeline until it is given other
//!   geometry. Never deleted.
//!
//! A file or library part is inserted first (as `insert_component` does)
//! and its features go before the old component's first feature, so that
//! what joins to it comes after it; in all occurrences the old component
//! goes with its features, its drawings go to the replacement, and the
//! stores beside the timeline (drawings' recorded picks, comments, print
//! plate items, decals) follow the bodies and faces. The whole is one undo
//! step, `Replace Bracket with Bracket_v2`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use super::components::InsertOptions;
use super::follow_paths::Follow;
use super::library_parts::LibraryPart;
use super::{DocState, Document, ModelError, invalid};
use crate::assembly::{Assembly, Flex};
use crate::datum::{CurveGeometry, SurfaceGeometry, Vec3};
use crate::features::constraint_set::row_label;
use crate::features::{FeatureDef, GeomRef, OccurrencePath};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::topo::{EdgeName, FaceName, RoleKey, TopoName, VertexName};
use crate::transform::Transform;
use mitcad_vec3::{cross, distance, dot, norm, sub, unit};

/// How far apart two places may be and still be the same (mm): the
/// geometric tolerance of the design's imports (mitcad#224).
const TOLERANCE: f64 = 0.01;

/// How far from parallel two directions may be (the sine of the angle).
const PARALLEL: f64 = 1e-6;

/// What takes the replaced component's place.
#[derive(Debug, Clone)]
pub enum ReplaceWith {
    /// A design file, linked (as `insert_component` links one) or copied
    /// with its history.
    File { path: String, link: bool },
    /// A library part, linked or copied.
    Library { part: LibraryPart, link: bool },
    /// Another component of this design.
    Component(ComponentUid),
}

/// What [`Document::replace_component`] replaces and with what.
#[derive(Debug, Clone)]
pub struct ReplaceOptions {
    /// The occurrence picked; its component is replaced.
    pub occurrence: OccurrenceUid,
    /// Every occurrence of its component (All occurrences); false: this
    /// one alone (This one).
    pub all: bool,
    pub with: ReplaceWith,
    /// The folder relative paths are relative to (the design file's).
    pub base: Option<PathBuf>,
    /// The replacement's name when it is inserted; left out: the file's
    /// or the part's, and in all occurrences the old component's name
    /// when the replacement is of the same kind.
    pub name: Option<String>,
}

/// How a reference found its geometry on the replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceMatch {
    /// The replacement's geometry of the same name.
    Name,
    /// The replacement's geometry at the same place.
    Geometry,
}

impl ReplaceMatch {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Geometry => "geometry",
        }
    }
}

/// One reference to the replaced component's geometry and what became of
/// it.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplacedReference {
    pub feature: FeatureUid,
    pub feature_name: String,
    /// Which of the feature's origins: `a`, `b`, `origin`, `from`, `to`,
    /// `row 2 (insert): a`, `origin 1` (a motion link's).
    pub slot: String,
    /// None: unmatched, the feature stays and is marked broken.
    pub matched: Option<ReplaceMatch>,
    /// In words, for the user.
    pub message: String,
}

/// What [`Document::replace_component`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplacedComponent {
    /// The replacement.
    pub component: ComponentUid,
    /// Its name now.
    pub name: String,
    /// The component replaced.
    pub replaced: ComponentUid,
    /// The component replaced went: no occurrence of it was left.
    pub removed: bool,
    /// The occurrences that place the replacement instead.
    pub occurrences: Vec<OccurrenceUid>,
    /// The references to the replaced component's geometry, in timeline
    /// order.
    pub references: Vec<ReplacedReference>,
}

impl ReplacedComponent {
    /// The references that found no match.
    pub fn unmatched(&self) -> usize {
        self.references
            .iter()
            .filter(|r| r.matched.is_none())
            .count()
    }
}

/// A component's kind, for keeping its name: a replacement of the same
/// kind takes the old one's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    LinkedFile,
    LibraryPart,
    Own,
}

fn kind_of(assembly: &Assembly, component: ComponentUid) -> Kind {
    match assembly.component(component) {
        Some(c) if c.library.is_some() => Kind::LibraryPart,
        Some(c) if c.link.is_some() => Kind::LinkedFile,
        _ => Kind::Own,
    }
}

impl<K: Kernel> Document<K> {
    /// Replaces the component of `options.occurrence` (in every place, or
    /// in that one) by a design file, a library part or another component
    /// (see the module documentation), as one undo step `Replace <old>
    /// with <new>`. Refused, with nothing changed, for an occurrence of a
    /// linked design, a copy a Pattern at Points makes (This One), a
    /// replacement that would hold itself, and while a feature that is not
    /// a joint, constraint, bound length or motion link uses the old
    /// component's geometry when it goes.
    pub fn replace_component(
        &mut self,
        options: &ReplaceOptions,
    ) -> Result<ReplacedComponent, ModelError> {
        let a = &self.state.assembly;
        let picked = a
            .occurrence(options.occurrence)
            .ok_or_else(|| invalid(format!("occurrence {} does not exist", options.occurrence)))?
            .clone();
        let old = picked.component;
        if picked.pattern.is_some() && !options.all {
            return Err(invalid(format!(
                "{} is a copy a Pattern at Points makes; replace its source, or all its occurrences",
                a.occurrence_name(picked.uid)
            )));
        }
        let targets: Vec<OccurrenceUid> = if options.all {
            a.occurrences_of(old).map(|o| o.uid).collect()
        } else {
            vec![picked.uid]
        };
        for uid in &targets {
            let parent = a.occurrence(*uid).map_or(ComponentUid::ROOT, |o| o.parent);
            self.state.editable(parent)?;
        }
        if let ReplaceWith::Component(with) = &options.with {
            check_replacement(a, old, *with, &targets, options.all)?;
        }
        let old_name = a.name(old);
        let old_kind = kind_of(a, old);
        let before: BTreeSet<FeatureUid> = self.state.features.iter().map(|f| f.uid).collect();
        self.as_one_step(|doc| {
            let (new, inserted) = doc.replacement(options)?;
            check_replacement(&doc.state.assembly, old, new, &targets, options.all)?;
            let new_name = doc.state.assembly.name(new);
            let keep_name = options.all
                && options.name.is_none()
                && !matches!(options.with, ReplaceWith::Component(_))
                && kind_of(&doc.state.assembly, new) == old_kind;
            let plan = doc.replace_plan(old, new, &targets)?;
            let step = Step {
                old,
                new,
                inserted,
                before: &before,
                targets: &targets,
                all: options.all,
                keep_name: keep_name.then_some(old_name.as_str()),
                label: format!("Replace {old_name} with {new_name}"),
            };
            doc.apply_replacement(step, plan)
        })
    }

    /// The replacement: a file or library part inserted (its component and
    /// the occurrence the insert made), or the component given.
    fn replacement(
        &mut self,
        options: &ReplaceOptions,
    ) -> Result<(ComponentUid, Option<OccurrenceUid>), ModelError> {
        // Relative paths from the design file's folder when no other is
        // given.
        let base = options.base.clone().or_else(|| self.source_directory());
        let insert = |link: bool| InsertOptions {
            link,
            transform: Transform::IDENTITY,
            name: options.name.clone(),
            base: base.clone(),
            ..InsertOptions::default()
        };
        match &options.with {
            ReplaceWith::File { path, link } => {
                let inserted = self.insert_file(path, &insert(*link))?;
                Ok((inserted.component, Some(inserted.occurrence)))
            }
            ReplaceWith::Library { part, link } => {
                let inserted = self.insert_library(part, &insert(*link))?;
                Ok((inserted.component, Some(inserted.occurrence)))
            }
            ReplaceWith::Component(component) => Ok((*component, None)),
        }
    }

    /// Where each reference to the old component's geometry through the
    /// replaced occurrences goes on the replacement (see the module
    /// documentation), read from the bodies at the marker.
    fn replace_plan(
        &self,
        old: ComponentUid,
        new: ComponentUid,
        targets: &[OccurrenceUid],
    ) -> Result<Plan, ModelError> {
        let replaced: BTreeSet<OccurrenceUid> = targets.iter().copied().collect();
        let features = feature_map(&self.state, old, new);
        let mut new_bodies = self.component_bodies(new);
        new_bodies.sort();
        // The origins that name geometry through a replaced occurrence.
        let mut found = Vec::new();
        for entry in &self.state.features {
            let mut def = entry.def.clone();
            for (slot, path, geometry) in origins_mut(&mut def) {
                let Some(at) = path.0.iter().position(|o| replaced.contains(o)) else {
                    continue;
                };
                let deeper = at + 1 < path.0.len();
                // Origin planes and fixed geometry are the occurrence's
                // own: they stay as they are on the replacement.
                let own = matches!(
                    geometry,
                    GeomRef::Origin(_)
                        | GeomRef::FixedPlane { .. }
                        | GeomRef::FixedAxis { .. }
                        | GeomRef::FixedPoint(_)
                );
                if own && !deeper {
                    continue;
                }
                found.push(Found {
                    feature: entry.uid,
                    name: entry.name.clone(),
                    slot,
                    path: path.0[..=at].to_vec(),
                    deeper,
                    geometry: geometry.clone(),
                });
            }
        }
        // The faces, edges and vertices they name, each once, matched.
        let mut wanted: Vec<Wanted> = Vec::new();
        let mut index: HashMap<(BodyUid, String), usize> = HashMap::new();
        for f in found.iter().filter(|f| !f.deeper) {
            let Some((body, element)) = Element::of(&f.geometry) else {
                continue;
            };
            let key = (body, element.text());
            if index.contains_key(&key) {
                continue;
            }
            let place = self
                .body_shape(body)
                .and_then(|shape| element.place(self.kernel(), shape));
            let by_name = map_body(body, &features)
                .filter(|b| new_bodies.contains(b))
                .zip(element.mapped(&features));
            index.insert(key, wanted.len());
            wanted.push(Wanted {
                element,
                place,
                by_name,
            });
        }
        let matches = self.match_elements(&wanted, &new_bodies);
        // The renames of the texts beside the timeline (all occurrences):
        // bodies by the feature map, faces, edges and vertices as matched.
        let mut renames: HashMap<String, String> = HashMap::new();
        for c in self.component_bodies(old) {
            if let Some(b) = map_body(c, &features).filter(|b| new_bodies.contains(b)) {
                renames.insert(c.to_string(), b.to_string());
            }
        }
        let marker_body = |body: Option<BodyUid>| -> Result<BodyUid, ModelError> {
            body.and_then(|b| map_body(b, &features))
                .filter(|b| new_bodies.contains(b))
                .or_else(|| new_bodies.first().copied())
                .ok_or_else(|| {
                    invalid(format!(
                        "{} has no bodies of its own for the joints on {} to stay on, marked, \
                         where it has nothing to match them",
                        self.state.assembly.name(new),
                        self.state.assembly.name(old)
                    ))
                })
        };
        let mut edits = Vec::new();
        for f in found {
            let word = kind_word(&f.geometry);
            let element = Element::of(&f.geometry);
            let matched: Option<(GeomRef, ReplaceMatch)> = if f.deeper {
                None
            } else if let Some((body, el)) = &element {
                index
                    .get(&(*body, el.text()))
                    .and_then(|i| matches[*i].clone())
                    .map(|(b, e, how)| (e.geom_ref(b), how))
            } else {
                mapped_reference(&f.geometry, &features).map(|g| (g, ReplaceMatch::Name))
            };
            let (geometry, how, message) = match matched {
                Some((geometry, how)) => {
                    let message = match how {
                        ReplaceMatch::Name => {
                            format!("its {word} is the replacement's {word} of the same name")
                        }
                        ReplaceMatch::Geometry => {
                            format!("its {word} is the replacement's {word} at the same place")
                        }
                    };
                    (geometry, Some(how), message)
                }
                None => {
                    let body = marker_body(element.as_ref().map(|(b, _)| *b))?;
                    let message = if f.deeper {
                        format!(
                            "its {word} was on a component inside the replaced one; it stays, \
                             marked as broken, until it is given new geometry"
                        )
                    } else {
                        format!(
                            "the replacement has no {word} that matches it by name or by place; \
                             it stays, marked as broken, until it is given new geometry"
                        )
                    };
                    (marked(&f.geometry, body), None, message)
                }
            };
            if let Some((_, el)) = &element
                && let Some(new_text) = geometry_text(&geometry)
            {
                renames.insert(el.text(), new_text);
            }
            edits.push(Edit {
                feature: f.feature,
                name: f.name,
                slot: f.slot,
                path: f.path,
                geometry,
                matched: how,
                message,
            });
        }
        Ok(Plan { edits, renames })
    }

    /// Matches each wanted face, edge and vertex to the replacement's
    /// bodies `bodies`: first by name where the geometry agrees, else by
    /// place; then by name where the place changed, else by axis. A face,
    /// edge or vertex of the replacement stands for one old one only.
    fn match_elements(
        &self,
        wanted: &[Wanted],
        bodies: &[BodyUid],
    ) -> Vec<Option<(BodyUid, Element, ReplaceMatch)>> {
        let kernel = self.kernel();
        let any = |f: fn(&Element) -> bool| wanted.iter().any(|w| f(&w.element));
        let mut candidates: Vec<Candidate> = Vec::new();
        for body in bodies {
            let Some(shape) = self.body_shape(*body) else {
                continue;
            };
            if any(|e| matches!(e, Element::Face(_))) {
                candidates.extend(face_candidates(kernel, *body, shape));
            }
            if any(|e| matches!(e, Element::Edge(_))) {
                candidates.extend(edge_candidates(kernel, *body, shape));
            }
        }
        // The candidate a name stands for (a vertex's found by its name).
        let named: Vec<Option<Candidate>> = wanted
            .iter()
            .map(|w| {
                let (body, element) = w.by_name.as_ref()?;
                if let Element::Vertex(vertex) = element {
                    let shape = self.body_shape(*body)?;
                    let point = kernel.vertex_point(shape, vertex).ok()?;
                    return Some(Candidate {
                        body: *body,
                        element: element.clone(),
                        names: vec![element.text()],
                        place: Some(Place::Point(point)),
                    });
                }
                let text = element.text();
                candidates
                    .iter()
                    .find(|c| c.body == *body && c.names.contains(&text))
                    .map(|c| Candidate {
                        element: element.clone(),
                        ..c.clone()
                    })
            })
            .collect();
        let mut claimed: BTreeSet<(BodyUid, String)> = BTreeSet::new();
        let mut out: Vec<Option<(BodyUid, Element, ReplaceMatch)>> = vec![None; wanted.len()];
        let claim =
            |c: &Candidate, how: ReplaceMatch, claimed: &mut BTreeSet<(BodyUid, String)>| {
                claimed.insert(c.key());
                Some((c.body, c.element.clone(), how))
            };
        // By name with the same geometry, else at the same place.
        for (i, w) in wanted.iter().enumerate() {
            if let Some(c) = &named[i]
                && !claimed.contains(&c.key())
                && compatible(w.place.as_ref(), c.place.as_ref())
                && same_or_unknown(w.place.as_ref(), c.place.as_ref())
            {
                out[i] = claim(c, ReplaceMatch::Name, &mut claimed);
                continue;
            }
            if let Some(place) = &w.place
                && let Some(c) = candidates.iter().find(|c| {
                    w.element.same_kind(&c.element)
                        && !claimed.contains(&c.key())
                        && c.place
                            .as_ref()
                            .is_some_and(|p| same_place(place, p, false))
                })
            {
                out[i] = claim(c, ReplaceMatch::Geometry, &mut claimed);
            }
        }
        // By name where the place changed (an edited part), else on the
        // same axis or line.
        for (i, w) in wanted.iter().enumerate() {
            if out[i].is_some() {
                continue;
            }
            if let Some(c) = &named[i]
                && !claimed.contains(&c.key())
                && compatible(w.place.as_ref(), c.place.as_ref())
            {
                out[i] = claim(c, ReplaceMatch::Name, &mut claimed);
                continue;
            }
            if let Some(place) = &w.place
                && let Some(c) = candidates.iter().find(|c| {
                    w.element.same_kind(&c.element)
                        && !claimed.contains(&c.key())
                        && c.place.as_ref().is_some_and(|p| same_place(place, p, true))
                })
            {
                out[i] = claim(c, ReplaceMatch::Geometry, &mut claimed);
            }
        }
        out
    }

    /// The replacement's step (see [`Document::replace_component`]).
    fn apply_replacement(
        &mut self,
        step: Step<'_>,
        plan: Plan,
    ) -> Result<ReplacedComponent, ModelError> {
        let Step {
            old,
            new,
            inserted,
            before,
            targets,
            all,
            keep_name,
            label,
        } = step;
        self.apply(|state| {
            let old_name = state.assembly.name(old);
            // The occurrences place the replacement.
            let mut number = state
                .assembly
                .occurrences_of(new)
                .filter(|o| Some(o.uid) != inserted)
                .map(|o| o.number)
                .max()
                .unwrap_or(0);
            for uid in targets {
                let o = state
                    .assembly
                    .occurrence_mut(*uid)
                    .ok_or_else(|| invalid(format!("occurrence {uid} does not exist")))?;
                o.component = new;
                // Its own values were of the old component's joints.
                if let Some(flex) = &mut o.flexible {
                    *flex = Flex {
                        free_parts: flex.free_parts,
                        ..Flex::default()
                    };
                }
                if !all {
                    number += 1;
                    o.number = number;
                }
            }
            if let Some(made) = inserted {
                state.assembly.occurrences.retain(|o| o.uid != made);
            }
            // The inserted features before what joins to them.
            if inserted.is_some() {
                move_inserted(state, before, old, &plan.edits);
            }
            // The references re-pointed, or marked.
            let mut edited: BTreeMap<FeatureUid, Vec<&Edit>> = BTreeMap::new();
            for edit in &plan.edits {
                edited.entry(edit.feature).or_default().push(edit);
            }
            for (uid, edits) in &edited {
                let position = state.require(*uid)?;
                let mut entry = (*state.features[position]).clone();
                for (slot, path, geometry) in origins_mut(&mut entry.def) {
                    if let Some(edit) = edits.iter().find(|e| e.slot == slot) {
                        path.0.clone_from(&edit.path);
                        *geometry = edit.geometry.clone();
                    }
                }
                state.features[position] = Arc::new(entry);
            }
            let mut removed = false;
            if all {
                follow_renamed(state, &plan.renames)?;
                // Its drawings are the replacement's now.
                state.move_drawings(old, new);
                remove_old(state, old, &old_name)?;
                removed = true;
                if let Some(name) = keep_name
                    && state.check_component_name(name, new).is_ok()
                    && let Some(c) = state.assembly.component_mut(new)
                {
                    c.name = name.to_owned();
                }
            }
            // What was re-pointed still checks: a replacement already in
            // the design must be made before what joins to it.
            let new_name = state.assembly.name(new);
            for uid in edited.keys() {
                let position = state.require(*uid)?;
                let entry = state.features[position].clone();
                let later = entry
                    .def
                    .references()
                    .features
                    .iter()
                    .any(|f| state.position(*f).is_some_and(|p| p > position));
                if later {
                    return Err(invalid(format!(
                        "{new_name} is made after {} on the timeline; move it before {} first",
                        entry.name, entry.name
                    )));
                }
                state.check_feature(position, &entry).map_err(|e| {
                    invalid(format!(
                        "{} would no longer work with {new_name}: {e}",
                        entry.name
                    ))
                })?;
            }
            let references = plan
                .edits
                .iter()
                .map(|e| ReplacedReference {
                    feature: e.feature,
                    feature_name: e.name.clone(),
                    slot: e.slot.clone(),
                    matched: e.matched,
                    message: e.message.clone(),
                })
                .collect();
            let done = ReplacedComponent {
                component: new,
                name: state.assembly.name(new),
                replaced: old,
                removed,
                occurrences: targets.to_vec(),
                references,
            };
            Ok((label, done))
        })
    }
}

/// Refuses a replacement that cannot take the old component's places.
fn check_replacement(
    a: &Assembly,
    old: ComponentUid,
    new: ComponentUid,
    targets: &[OccurrenceUid],
    all: bool,
) -> Result<(), ModelError> {
    if !a.exists(new) {
        return Err(invalid(format!("component {new} does not exist")));
    }
    if new == ComponentUid::ROOT {
        return Err(invalid("the design's root cannot replace a component"));
    }
    if new == old {
        return Err(invalid(format!("{} cannot replace itself", a.name(old))));
    }
    if a.linked_in(new).is_some() {
        return Err(invalid(format!(
            "{} is part of a linked design and is placed only by it",
            a.name(new)
        )));
    }
    if all && a.contains(old, new) {
        return Err(invalid(format!(
            "{} is inside {}, which goes when it is replaced in all its places",
            a.name(new),
            a.name(old)
        )));
    }
    for uid in targets {
        let Some(parent) = a.occurrence(*uid).map(|o| o.parent) else {
            continue;
        };
        if a.contains(new, parent) {
            return Err(invalid(format!(
                "{} holds {}, so it cannot be placed in it",
                a.name(new),
                a.name(parent)
            )));
        }
    }
    Ok(())
}

/// The step's inputs (see [`Document::apply_replacement`]).
struct Step<'a> {
    old: ComponentUid,
    new: ComponentUid,
    /// The occurrence an insert made, which goes.
    inserted: Option<OccurrenceUid>,
    /// The features before the insert.
    before: &'a BTreeSet<FeatureUid>,
    targets: &'a [OccurrenceUid],
    all: bool,
    /// The old component's name, for the replacement to take.
    keep_name: Option<&'a str>,
    label: String,
}

/// An origin that names geometry through a replaced occurrence.
struct Found {
    feature: FeatureUid,
    name: String,
    slot: String,
    /// Its path up to the replaced occurrence.
    path: Vec<OccurrenceUid>,
    /// Its geometry is in a component inside the replaced one.
    deeper: bool,
    geometry: GeomRef,
}

/// The plan of a replacement: each origin's new path and geometry, and
/// the texts the stores beside the timeline rename.
struct Plan {
    edits: Vec<Edit>,
    renames: HashMap<String, String>,
}

struct Edit {
    feature: FeatureUid,
    name: String,
    slot: String,
    path: Vec<OccurrenceUid>,
    geometry: GeomRef,
    matched: Option<ReplaceMatch>,
    message: String,
}

/// A face, edge or vertex of a body.
#[derive(Debug, Clone, PartialEq)]
enum Element {
    Face(FaceName),
    Edge(EdgeName),
    Vertex(VertexName),
}

impl Element {
    fn of(geometry: &GeomRef) -> Option<(BodyUid, Element)> {
        match geometry {
            GeomRef::Face { body, face } => Some((*body, Self::Face(face.clone()))),
            GeomRef::Edge { body, edge } => Some((*body, Self::Edge(edge.clone()))),
            GeomRef::Vertex { body, vertex } => Some((*body, Self::Vertex(vertex.clone()))),
            _ => None,
        }
    }

    fn text(&self) -> String {
        match self {
            Self::Face(face) => face.to_string(),
            Self::Edge(edge) => edge.to_string(),
            Self::Vertex(vertex) => vertex.to_string(),
        }
    }

    fn same_kind(&self, other: &Element) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    fn geom_ref(&self, body: BodyUid) -> GeomRef {
        match self {
            Self::Face(face) => GeomRef::Face {
                body,
                face: face.clone(),
            },
            Self::Edge(edge) => GeomRef::Edge {
                body,
                edge: edge.clone(),
            },
            Self::Vertex(vertex) => GeomRef::Vertex {
                body,
                vertex: vertex.clone(),
            },
        }
    }

    /// The same name in the replacement's features, when each feature it
    /// names has one there.
    fn mapped(&self, features: &BTreeMap<FeatureUid, FeatureUid>) -> Option<Element> {
        Some(match self {
            Self::Face(face) => Self::Face(map_face(face, features)?),
            Self::Edge(edge) => Self::Edge(map_edge(edge, features)?),
            Self::Vertex(vertex) => Self::Vertex(map_vertex(vertex, features)?),
        })
    }

    /// Where it is on `shape`, when the kernel can tell.
    fn place<K: Kernel>(&self, kernel: &K, shape: &K::Shape) -> Option<Place> {
        match self {
            Self::Face(face) => kernel.face_geometry(shape, face).ok().map(Place::Surface),
            Self::Edge(edge) => kernel.edge_geometry(shape, edge).ok().map(Place::Curve),
            Self::Vertex(vertex) => kernel.vertex_point(shape, vertex).ok().map(Place::Point),
        }
    }
}

/// A face, edge or vertex the old component referred to, to match.
struct Wanted {
    element: Element,
    place: Option<Place>,
    /// The replacement's body and element of the same name.
    by_name: Option<(BodyUid, Element)>,
}

/// A face or edge of the replacement (a vertex only as a name's).
#[derive(Debug, Clone)]
struct Candidate {
    body: BodyUid,
    element: Element,
    /// Its names (a face may have several).
    names: Vec<String>,
    place: Option<Place>,
}

impl Candidate {
    fn key(&self) -> (BodyUid, String) {
        (self.body, self.element.text())
    }
}

/// The geometry of a face, edge or vertex.
#[derive(Debug, Clone, PartialEq)]
enum Place {
    Surface(SurfaceGeometry),
    Curve(CurveGeometry),
    Point(Vec3),
}

/// The faces of a body of the replacement with their surfaces.
fn face_candidates<K: Kernel>(kernel: &K, body: BodyUid, shape: &K::Shape) -> Vec<Candidate> {
    let candidate = |names: Vec<String>, place: Option<Place>| {
        let face = names.first()?.parse::<FaceName>().ok()?;
        Some(Candidate {
            body,
            element: Element::Face(face),
            names,
            place,
        })
    };
    if let Ok(faces) = kernel.face_geometries(shape) {
        return faces
            .into_iter()
            .filter_map(|(names, surface)| candidate(names, Some(Place::Surface(surface))))
            .collect();
    }
    kernel
        .faces(shape)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|info| {
            let face = info.names.first()?.parse::<FaceName>().ok()?;
            let place = kernel.face_geometry(shape, &face).ok().map(Place::Surface);
            candidate(info.names, place)
        })
        .collect()
}

/// The named edges of a body of the replacement with their curves.
fn edge_candidates<K: Kernel>(kernel: &K, body: BodyUid, shape: &K::Shape) -> Vec<Candidate> {
    let candidate = |name: String, place: Option<Place>| {
        let edge = name.parse::<EdgeName>().ok()?;
        Some(Candidate {
            body,
            element: Element::Edge(edge),
            names: vec![name],
            place,
        })
    };
    if let Ok(edges) = kernel.edge_geometries(shape) {
        return edges
            .into_iter()
            .filter_map(|(name, curve)| candidate(name?, Some(Place::Curve(curve))))
            .collect();
    }
    kernel
        .edges(shape)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|info| {
            let name = info.name?;
            let edge = name.parse::<EdgeName>().ok()?;
            let place = kernel.edge_geometry(shape, &edge).ok().map(Place::Curve);
            candidate(name, place)
        })
        .collect()
}

/// The same kind of geometry, or one of them unknown.
fn compatible(a: Option<&Place>, b: Option<&Place>) -> bool {
    match (a, b) {
        (Some(Place::Surface(a)), Some(Place::Surface(b))) => a.kind() == b.kind(),
        (Some(Place::Curve(a)), Some(Place::Curve(b))) => a.kind() == b.kind(),
        (Some(Place::Point(_)), Some(Place::Point(_))) => true,
        (Some(_), Some(_)) => false,
        _ => true,
    }
}

/// At the same place, or one of them unknown.
fn same_or_unknown(a: Option<&Place>, b: Option<&Place>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => same_place(a, b, false),
        _ => true,
    }
}

fn parallel(a: Vec3, b: Vec3) -> bool {
    match (unit(a), unit(b)) {
        (Some(a), Some(b)) => norm(cross(a, b)) <= PARALLEL,
        _ => false,
    }
}

/// The distance of `point` from the line through `origin` along `axis`.
fn off_line(point: Vec3, origin: Vec3, axis: Vec3) -> f64 {
    unit(axis).map_or(f64::INFINITY, |a| norm(cross(sub(point, origin), a)))
}

fn same_axis(o1: Vec3, a1: Vec3, o2: Vec3, a2: Vec3) -> bool {
    parallel(a1, a2) && off_line(o2, o1, a1) <= TOLERANCE
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOLERANCE
}

/// Two faces, edges or vertices at the same place (see the module
/// documentation); `loose`: a round face or circle on the same axis, a
/// line on the same line, whatever its size.
fn same_place(a: &Place, b: &Place, loose: bool) -> bool {
    match (a, b) {
        (Place::Surface(a), Place::Surface(b)) => same_surface(a, b, loose),
        (Place::Curve(a), Place::Curve(b)) => same_curve(a, b, loose),
        (Place::Point(a), Place::Point(b)) => distance(*a, *b) <= TOLERANCE,
        _ => false,
    }
}

fn same_surface(a: &SurfaceGeometry, b: &SurfaceGeometry, loose: bool) -> bool {
    use SurfaceGeometry as S;
    match (a, b) {
        (
            S::Plane {
                origin: o1,
                normal: n1,
            },
            S::Plane {
                origin: o2,
                normal: n2,
            },
        ) => {
            parallel(*n1, *n2)
                && dot(*n1, *n2) > 0.0
                && unit(*n1).is_some_and(|n| dot(sub(*o2, *o1), n).abs() <= TOLERANCE)
        }
        (
            S::Cylinder {
                origin: o1,
                axis: a1,
                radius: r1,
            },
            S::Cylinder {
                origin: o2,
                axis: a2,
                radius: r2,
            },
        ) => same_axis(*o1, *a1, *o2, *a2) && (loose || near(*r1, *r2)),
        (
            S::Cone {
                origin: o1,
                axis: a1,
                half_angle: h1,
                ..
            },
            S::Cone {
                origin: o2,
                axis: a2,
                half_angle: h2,
                ..
            },
        ) => same_axis(*o1, *a1, *o2, *a2) && (loose || (h1 - h2).abs() <= PARALLEL),
        (
            S::Sphere {
                center: c1,
                radius: r1,
            },
            S::Sphere {
                center: c2,
                radius: r2,
            },
        ) => distance(*c1, *c2) <= TOLERANCE && (loose || near(*r1, *r2)),
        (
            S::Torus {
                center: c1,
                axis: a1,
                major_radius: m1,
                minor_radius: n1,
            },
            S::Torus {
                center: c2,
                axis: a2,
                major_radius: m2,
                minor_radius: n2,
            },
        ) => {
            distance(*c1, *c2) <= TOLERANCE
                && parallel(*a1, *a2)
                && (loose || (near(*m1, *m2) && near(*n1, *n2)))
        }
        _ => false,
    }
}

fn same_curve(a: &CurveGeometry, b: &CurveGeometry, loose: bool) -> bool {
    use CurveGeometry as C;
    let ends = |s1: Vec3, e1: Vec3, s2: Vec3, e2: Vec3| {
        (distance(s1, s2) <= TOLERANCE && distance(e1, e2) <= TOLERANCE)
            || (distance(s1, e2) <= TOLERANCE && distance(e1, s2) <= TOLERANCE)
    };
    match (a, b) {
        (C::Line { start: s1, end: e1 }, C::Line { start: s2, end: e2 }) => {
            if loose {
                let axis = sub(*e1, *s1);
                off_line(*s2, *s1, axis) <= TOLERANCE && off_line(*e2, *s1, axis) <= TOLERANCE
            } else {
                ends(*s1, *e1, *s2, *e2)
            }
        }
        (
            C::Circle {
                center: c1,
                normal: n1,
                radius: r1,
                ..
            },
            C::Circle {
                center: c2,
                normal: n2,
                radius: r2,
                ..
            },
        ) => {
            if loose {
                same_axis(*c1, *n1, *c2, *n2)
            } else {
                distance(*c1, *c2) <= TOLERANCE && parallel(*n1, *n2) && near(*r1, *r2)
            }
        }
        (
            C::Other {
                kind: k1,
                start: s1,
                end: e1,
            },
            C::Other {
                kind: k2,
                start: s2,
                end: e2,
            },
        ) => !loose && k1 == k2 && ends(*s1, *e1, *s2, *e2),
        _ => false,
    }
}

/// The old component's features as the replacement's (see the module
/// documentation): a linked base feature as the other's, then by name and
/// type, then by order among the features of a type.
fn feature_map(
    state: &DocState,
    old: ComponentUid,
    new: ComponentUid,
) -> BTreeMap<FeatureUid, FeatureUid> {
    let a = &state.assembly;
    let link = |c: ComponentUid| {
        a.component(c)
            .and_then(|c| c.link.as_ref())
            .map(|l| l.feature)
    };
    let mut map = BTreeMap::new();
    if let (Some(from), Some(to)) = (link(old), link(new)) {
        map.insert(from, to);
    }
    let of = |c: ComponentUid| -> Vec<(FeatureUid, String, &'static str)> {
        state
            .features
            .iter()
            .filter(|f| f.component == c)
            .map(|f| (f.uid, f.name.clone(), f.def.type_name()))
            .collect()
    };
    let olds = of(old);
    let news = of(new);
    let mut taken: BTreeSet<FeatureUid> = map.values().copied().collect();
    for (uid, name, kind) in &olds {
        if map.contains_key(uid) {
            continue;
        }
        if let Some((to, _, _)) = news
            .iter()
            .find(|(to, n, k)| n == name && k == kind && !taken.contains(to))
        {
            map.insert(*uid, *to);
            taken.insert(*to);
        }
    }
    let mut left_new: BTreeMap<&str, Vec<FeatureUid>> = BTreeMap::new();
    for (uid, _, kind) in news.iter().filter(|(u, _, _)| !taken.contains(u)) {
        left_new.entry(*kind).or_default().push(*uid);
    }
    let left_old: Vec<(FeatureUid, &str)> = olds
        .iter()
        .filter(|(u, _, _)| !map.contains_key(u))
        .map(|(u, _, kind)| (*u, *kind))
        .collect();
    let mut used: BTreeMap<&str, usize> = BTreeMap::new();
    for (uid, kind) in left_old {
        let next = used.entry(kind).or_default();
        if let Some(to) = left_new.get(kind).and_then(|list| list.get(*next)) {
            map.insert(uid, *to);
        }
        *next += 1;
    }
    map
}

fn map_body(body: BodyUid, features: &BTreeMap<FeatureUid, FeatureUid>) -> Option<BodyUid> {
    features
        .get(&body.feature)
        .map(|f| BodyUid::new(*f, body.index))
}

fn map_face(face: &FaceName, features: &BTreeMap<FeatureUid, FeatureUid>) -> Option<FaceName> {
    let feature = *features.get(&face.feature)?;
    let key = match &face.key {
        // A derived part's face names its source's ids (mitcad#375).
        Some(RoleKey::Face(_)) if face.role == "part" => face.key.clone(),
        Some(RoleKey::Face(inner)) => Some(RoleKey::Face(Box::new(map_face(inner, features)?))),
        Some(RoleKey::Edge(edge)) => Some(RoleKey::Edge(Box::new(map_edge(edge, features)?))),
        Some(RoleKey::Vertex(vertex)) => {
            Some(RoleKey::Vertex(Box::new(map_vertex(vertex, features)?)))
        }
        other => other.clone(),
    };
    Some(FaceName {
        feature,
        role: face.role.clone(),
        key,
        split: face.split.clone(),
    })
}

fn map_edge(edge: &EdgeName, features: &BTreeMap<FeatureUid, FeatureUid>) -> Option<EdgeName> {
    let [a, b] = edge.faces();
    let mapped = EdgeName::new(map_face(a, features)?, map_face(b, features)?);
    Some(match edge.index {
        Some(k) => mapped.with_index(k),
        None => mapped,
    })
}

fn map_vertex(
    vertex: &VertexName,
    features: &BTreeMap<FeatureUid, FeatureUid>,
) -> Option<VertexName> {
    let faces: Option<Vec<FaceName>> = vertex
        .faces()
        .iter()
        .map(|f| map_face(f, features))
        .collect();
    let mapped = VertexName::new(faces?)?;
    Some(match vertex.index {
        Some(k) => mapped.with_index(k),
        None => mapped,
    })
}

/// A reference that is no face, edge or vertex (construction geometry,
/// sketch geometry, a mount frame, a body) on the replacement's feature of
/// the same name; None when it has none. Origin planes and fixed geometry
/// are the occurrence's own and stay.
fn mapped_reference(
    geometry: &GeomRef,
    features: &BTreeMap<FeatureUid, FeatureUid>,
) -> Option<GeomRef> {
    let feature = |uid: &FeatureUid| features.get(uid).copied();
    Some(match geometry {
        GeomRef::Datum(uid) => GeomRef::Datum(feature(uid)?),
        GeomRef::SketchCurves { sketch, curves } => GeomRef::SketchCurves {
            sketch: feature(sketch)?,
            curves: curves.clone(),
        },
        GeomRef::SketchPoint { sketch, point } => GeomRef::SketchPoint {
            sketch: feature(sketch)?,
            point: *point,
        },
        GeomRef::Mount { feature: uid, name } => GeomRef::Mount {
            feature: feature(uid)?,
            name: name.clone(),
        },
        GeomRef::Body(body) => GeomRef::Body(map_body(*body, features)?),
        other => other.clone(),
    })
}

/// The geometry an unmatched reference takes: on `body` of the
/// replacement, faces no face is ([`FaceName::replaced`]).
fn marked(geometry: &GeomRef, body: BodyUid) -> GeomRef {
    let face = |old: &FaceName| FaceName::replaced(body.feature, old.clone());
    match geometry {
        GeomRef::Face { face: old, .. } => GeomRef::Face {
            body,
            face: face(old),
        },
        GeomRef::Edge { edge, .. } => {
            let [a, b] = edge.faces();
            let mut marked = EdgeName::new(face(a), face(b));
            if let Some(k) = edge.index {
                marked = marked.with_index(k);
            }
            GeomRef::Edge { body, edge: marked }
        }
        GeomRef::Vertex { vertex, .. } => match VertexName::new(vertex.faces().iter().map(face)) {
            Some(marked) => GeomRef::Vertex {
                body,
                vertex: marked,
            },
            None => GeomRef::Face {
                body,
                face: FaceName::new(body.feature, crate::topo::REPLACED_ROLE, None),
            },
        },
        _ => GeomRef::Face {
            body,
            face: FaceName::new(body.feature, crate::topo::REPLACED_ROLE, None),
        },
    }
}

/// The text of a face, edge or vertex reference.
fn geometry_text(geometry: &GeomRef) -> Option<String> {
    Element::of(geometry).map(|(_, element)| element.text())
}

/// The word for a reference in messages.
fn kind_word(geometry: &GeomRef) -> &'static str {
    match geometry {
        GeomRef::Face { .. } => "face",
        GeomRef::Edge { .. } => "edge",
        GeomRef::Vertex { .. } => "point",
        GeomRef::Datum(_) => "construction geometry",
        GeomRef::SketchCurves { .. } => "sketch curve",
        GeomRef::SketchPoint { .. } => "sketch point",
        GeomRef::Mount { .. } => "mount frame",
        GeomRef::Body(_) => "body",
        _ => "geometry",
    }
}

/// The origins of a definition that name geometry of an occurrence: each
/// with its slot, its path and its geometry. Joints, as-built joints,
/// constraint rows, bound lengths and motion links.
fn origins_mut(def: &mut FeatureDef) -> Vec<(String, &mut OccurrencePath, &mut GeomRef)> {
    match def {
        FeatureDef::Joint(joint) => vec![
            (
                "a".to_owned(),
                &mut joint.a.occurrence,
                &mut joint.a.geometry,
            ),
            (
                "b".to_owned(),
                &mut joint.b.occurrence,
                &mut joint.b.geometry,
            ),
        ],
        FeatureDef::AsBuiltJoint(joint) => joint
            .origin
            .iter_mut()
            .map(|o| ("origin".to_owned(), &mut o.occurrence, &mut o.geometry))
            .collect(),
        FeatureDef::ConstraintSet(set) => {
            let mut out = Vec::new();
            for (i, row) in set.rows.iter_mut().enumerate() {
                let label = row_label(i, row.kind);
                let slots = [("a", Some(&mut row.a)), ("b", Some(&mut row.b))]
                    .into_iter()
                    .chain([("a2", row.a2.as_mut()), ("b2", row.b2.as_mut())]);
                for (key, origin) in slots {
                    if let Some(origin) = origin {
                        out.push((
                            format!("{label}: {key}"),
                            &mut origin.occurrence,
                            &mut origin.geometry,
                        ));
                    }
                }
            }
            out
        }
        FeatureDef::BoundLength(bound) => vec![
            (
                "from".to_owned(),
                &mut bound.from.occurrence,
                &mut bound.from.geometry,
            ),
            (
                "to".to_owned(),
                &mut bound.to.occurrence,
                &mut bound.to.geometry,
            ),
        ],
        FeatureDef::MotionLink(link) => link
            .coupling
            .origins_mut()
            .into_iter()
            .enumerate()
            .map(|(i, o)| {
                (
                    format!("origin {}", i + 1),
                    &mut o.occurrence,
                    &mut o.geometry,
                )
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Moves the features an insert made (those not in `before`) to before
/// the first feature of the old component, its maker, or the first feature
/// re-pointed to them, whichever comes first, so that everything that
/// refers to the replacement comes after it. The marker stays after the
/// same features.
fn move_inserted(
    state: &mut DocState,
    before: &BTreeSet<FeatureUid>,
    old: ComponentUid,
    edits: &[Edit],
) {
    let start = state.features.iter().position(|f| !before.contains(&f.uid));
    let Some(start) = start else {
        return;
    };
    let inside = subtree(&state.assembly, old);
    let maker = state.assembly.component(old).and_then(|c| c.created_by);
    let at = state.features.iter().position(|f| {
        inside.contains(&f.component)
            || Some(f.uid) == maker
            || edits.iter().any(|e| e.feature == f.uid)
    });
    let Some(at) = at.filter(|at| *at < start) else {
        return;
    };
    // The features before `at` are not the insert's (`at` < `start`): they
    // keep their places, and the block, inserted at the marker, stays
    // before it.
    let (moved, mut features): (Vec<_>, Vec<_>) = std::mem::take(&mut state.features)
        .into_iter()
        .partition(|f| !before.contains(&f.uid));
    let rest = features.split_off(at);
    features.extend(moved);
    features.extend(rest);
    state.features = features;
}

/// The component and every component placed in it, through any depth.
fn subtree(a: &Assembly, component: ComponentUid) -> BTreeSet<ComponentUid> {
    let mut inside = BTreeSet::from([component]);
    let mut todo = vec![component];
    while let Some(c) = todo.pop() {
        for o in a.children(c) {
            if inside.insert(o.component) {
                todo.push(o.component);
            }
        }
    }
    inside
}

/// The stores beside the timeline (drawings' recorded bodies and picks,
/// comments, print plate items, decals) and the annotations' picks follow
/// the old component's bodies and faces to the replacement's.
fn follow_renamed(
    state: &mut DocState,
    renames: &HashMap<String, String>,
) -> Result<(), ModelError> {
    if renames.is_empty() {
        return Ok(());
    }
    let rename = |text: &str| -> Option<String> {
        if let Some(new) = renames.get(text) {
            return Some(new.clone());
        }
        // A body named with a face: `F3.b0` beside `F3:import(2)` are
        // separate texts; a name parsed renames what it names.
        let name = text.parse::<TopoName>().ok()?;
        renames.get(&name.to_string()).cloned()
    };
    for annotation in &mut state.annotations {
        super::save_component::rename_picks(annotation, &rename)?;
    }
    state.follow(&Follow::Renamed(&|value: &mut serde_json::Value| {
        super::save_component::rename_value(value, &rename)
    }));
    Ok(())
}

/// Removes the old component, which no occurrence places any more, with
/// its features, the components only it held and the feature that made
/// it. Refused while a feature that stays uses its geometry: it would go
/// with it.
fn remove_old(state: &mut DocState, old: ComponentUid, name: &str) -> Result<(), ModelError> {
    let owners: HashMap<FeatureUid, ComponentUid> = state
        .features
        .iter()
        .map(|f| (f.uid, f.component))
        .collect();
    let names: HashMap<FeatureUid, String> = state
        .features
        .iter()
        .map(|f| (f.uid, f.name.clone()))
        .collect();
    let makers: BTreeSet<FeatureUid> = subtree(&state.assembly, old)
        .into_iter()
        .filter_map(|c| state.assembly.component(c).and_then(|c| c.created_by))
        .collect();
    let removed = state.delete_components(BTreeSet::from([old]));
    let users: Vec<String> = removed
        .iter()
        .filter(|f| !makers.contains(f))
        .filter(|f| owners.get(f).is_some_and(|c| state.assembly.exists(*c)))
        .filter_map(|f| names.get(f).cloned())
        .collect();
    if !users.is_empty() {
        return Err(invalid(format!(
            "{name} cannot be replaced in all its places while {} use its geometry; edit them \
             to use other geometry first",
            users.join(", ")
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(origin: Vec3, normal: Vec3) -> Place {
        Place::Surface(SurfaceGeometry::Plane { origin, normal })
    }

    fn cylinder(origin: Vec3, radius: f64) -> Place {
        Place::Surface(SurfaceGeometry::Cylinder {
            origin,
            axis: [0.0, 0.0, 1.0],
            radius,
        })
    }

    #[test]
    fn planes_are_at_the_same_place_in_the_same_plane_facing_the_same_way() {
        let a = plane([0.0, 0.0, 5.0], [0.0, 0.0, 1.0]);
        assert!(same_place(
            &a,
            &plane([3.0, 4.0, 5.005], [0.0, 0.0, 2.0]),
            false
        ));
        assert!(!same_place(
            &a,
            &plane([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]),
            false
        ));
        assert!(!same_place(
            &a,
            &plane([0.0, 0.0, 5.1], [0.0, 0.0, 1.0]),
            false
        ));
        assert!(!same_place(&a, &cylinder([0.0; 3], 2.0), true));
    }

    #[test]
    fn a_hole_is_at_the_same_place_on_its_axis_then_whatever_its_radius() {
        let hole = cylinder([5.0, 5.0, 0.0], 2.0);
        assert!(same_place(&hole, &cylinder([5.0, 5.0, 7.0], 2.0), false));
        assert!(!same_place(&hole, &cylinder([5.0, 5.0, 7.0], 3.0), false));
        assert!(same_place(&hole, &cylinder([5.0, 5.0, 7.0], 3.0), true));
        assert!(!same_place(&hole, &cylinder([5.5, 5.0, 0.0], 2.0), true));
    }

    #[test]
    fn circles_and_lines_match_by_their_place() {
        let circle = |center: Vec3, radius: f64| {
            Place::Curve(CurveGeometry::Circle {
                center,
                normal: [0.0, 0.0, 1.0],
                radius,
                start: center,
                end: center,
            })
        };
        let line = |start: Vec3, end: Vec3| Place::Curve(CurveGeometry::Line { start, end });
        assert!(same_place(
            &circle([1.0, 2.0, 3.0], 4.0),
            &circle([1.0, 2.0, 3.0], 4.0),
            false
        ));
        assert!(!same_place(
            &circle([1.0, 2.0, 3.0], 4.0),
            &circle([1.0, 2.0, 3.0], 5.0),
            false
        ));
        assert!(same_place(
            &circle([1.0, 2.0, 3.0], 4.0),
            &circle([1.0, 2.0, 9.0], 5.0),
            true
        ));
        let edge = line([0.0; 3], [10.0, 0.0, 0.0]);
        assert!(same_place(&edge, &line([10.0, 0.0, 0.0], [0.0; 3]), false));
        assert!(!same_place(&edge, &line([0.0; 3], [12.0, 0.0, 0.0]), false));
        assert!(same_place(
            &edge,
            &line([2.0, 0.0, 0.0], [12.0, 0.0, 0.0]),
            true
        ));
    }

    #[test]
    fn names_follow_the_features_and_unmatched_ones_are_marked() {
        let features = BTreeMap::from([(FeatureUid(3), FeatureUid(9))]);
        let face: FaceName = "F3:import(4)".parse().unwrap();
        assert_eq!(
            map_face(&face, &features).unwrap().to_string(),
            "F9:import(4)"
        );
        let edge: EdgeName = "E{F3:import(1)|F3:import(2)}".parse().unwrap();
        assert_eq!(
            map_edge(&edge, &features).unwrap().to_string(),
            "E{F9:import(1)|F9:import(2)}"
        );
        // A feature the replacement has nothing for: no name to match.
        let other: FaceName = "F4:end(r{c1})".parse().unwrap();
        assert!(map_face(&other, &features).is_none());
        let body = BodyUid::new(FeatureUid(9), 0);
        let geometry = GeomRef::Face {
            body: BodyUid::new(FeatureUid(3), 1),
            face,
        };
        assert_eq!(
            geometry_text(&marked(&geometry, body)).as_deref(),
            Some("F9:replaced(F3:import(4))")
        );
        assert_eq!(
            mapped_reference(&GeomRef::Datum(FeatureUid(3)), &features),
            Some(GeomRef::Datum(FeatureUid(9)))
        );
        assert_eq!(
            mapped_reference(&GeomRef::Datum(FeatureUid(5)), &features),
            None
        );
    }
}
