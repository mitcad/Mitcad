// SPDX-License-Identifier: MIT
//! Reorient Design (mitcad#502): the whole design turned by a multiple of
//! 90 degrees about an origin axis in one undo step. How each part of the
//! definition turns is in [`crate::reorient`]; this is the command: the
//! next state, the sketch frames that only an evaluation can tell, and the
//! checks that the turned design is the design turned.
//!
//! The command computes the turned design before taking it and refuses it
//! when a body would not be its old self turned (its bounding box) or a
//! feature that built would fail, naming them; the document is then as it
//! was.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::{DocState, Document, ModelError, invalid};
use crate::features::sketch::FrameDef;
use crate::features::{FeatureDef, FeatureEntry};
use crate::ids::{BodyUid, ComponentUid, FeatureUid};
use crate::kernel::{BoundingBox, Kernel};
use crate::profile::SketchFrame;
use crate::recompute::Recomputed;
use crate::reorient::{PieceRenames, Turn, Turned};
use crate::topo::{EdgeName, FaceName, TopoName};
use mitcad_vec3::{cross, dot};

/// What Reorient Design changed, for the command's reply.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReorientReport {
    /// The features whose definitions changed, by name, in timeline order.
    pub features: Vec<String>,
    /// The Moves added after base features (imported bodies), by name.
    pub moves: Vec<String>,
    /// The occurrences whose placements changed, by name.
    pub occurrences: Vec<String>,
    /// Named views, section analyses and lights that turned.
    pub views: usize,
    pub analyses: usize,
    pub lights: usize,
}

/// A body as the checks compare it.
struct BodyFacts {
    bounds: Option<BoundingBox>,
    center: Option<[f64; 3]>,
}

/// How far a body is from where the turn takes it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Deviation {
    /// The distance of its centre of mass from the turned one, mm.
    center: Option<f64>,
    /// The largest distance of a corner of its box from the turned box's.
    corners: Option<f64>,
    /// The body is not there.
    gone: bool,
}

impl Deviation {
    fn describe(&self) -> String {
        if self.gone {
            return "the body is not made there".to_owned();
        }
        let mut parts = Vec::new();
        if let Some(c) = self.center {
            parts.push(format!("its center is {c:.3} mm"));
        }
        if let Some(c) = self.corners {
            parts.push(format!("its box up to {c:.3} mm"));
        }
        if parts.is_empty() {
            return "its box differs".to_owned();
        }
        format!("{} from where the turn takes it", parts.join(" and "))
    }
}

/// A turned body compared with its old self turned: None when it is where
/// the turn takes it (its box within 1e-6 of its size, at least 1e-6 mm,
/// its centre likewise).
fn deviation(turn: Turn, old: &BodyFacts, now: Option<&BodyFacts>) -> Option<Deviation> {
    let Some(now) = now else {
        return Some(Deviation {
            center: None,
            corners: None,
            gone: true,
        });
    };
    let expected = old.bounds.map(|b| turn_box(turn, &b));
    let size = expected.as_ref().map_or(1.0, box_size);
    let center = match (old.center, now.center) {
        (Some(was), Some(is)) => {
            let was = turn.vector(was);
            let off = (0..3).map(|i| (was[i] - is[i]).powi(2)).sum::<f64>().sqrt();
            ((0..3).any(|i| (was[i] - is[i]).abs() > 1e-6 * size)).then_some(off)
        }
        _ => None,
    };
    let boxed = same_box(expected.as_ref(), now.bounds.as_ref());
    let corners = match (&expected, &now.bounds) {
        (Some(a), Some(b)) if !boxed => Some((0..3).fold(0.0_f64, |m, i| {
            m.max((a.min[i] - b.min[i]).abs())
                .max((a.max[i] - b.max[i]).abs())
        })),
        _ => None,
    };
    (center.is_some() || !boxed).then_some(Deviation {
        center,
        corners,
        gone: false,
    })
}

/// A sketch frame compares within this (unit vectors; positions scaled by
/// their size).
const FRAME_TOLERANCE: f64 = 1e-9;

impl<K: Kernel> Document<K> {
    /// Turns the whole design (see the module documentation); one undo
    /// step named `Reorient Design`.
    pub fn reorient_design(&mut self, turn: Turn) -> Result<ReorientReport, ModelError> {
        if turn.is_identity() {
            return Err(invalid(
                "Reorient Design: the turn leaves the design as it is",
            ));
        }
        let all = self.state.features.len();
        // The bodies at the marker as they are, and the whole timeline (the
        // marker at the end, so that sketches and pieces after it are known
        // too).
        let old_boxes = self.boxes_of(&self.result)?;
        let old_ok = ok_features(&self.result);
        let old_whole = {
            let mut whole = self.state.clone();
            whole.marker = all;
            self.compute(&whole)?
        };
        let old_frames = sketch_frames(&old_whole);

        let mut next = self.state.clone();
        let turned = turned_components(&next);
        let mut report = ReorientReport::default();
        let reversed = self.reversed_wraps(&old_whole, turn, &turned);
        turn_state(&mut next, turn, &turned, &reversed, &mut report)?;

        // Sketches on faces, on construction planes and on other
        // components' planes: the frame they get from the turned design,
        // corrected where a rule of the model axes gives the plane another
        // in-plane direction. Then the pieces of split faces and the edges
        // numbered between the same faces, which the kernel numbers in a
        // geometric order that the turn changes: the references to them
        // renamed after the turned design's numbers, one feature at a time
        // (mitcad#537). A corrected sketch or a renamed reference can move
        // what is built on it, so the next round looks again.
        let sketches = next
            .features
            .iter()
            .filter(|f| matches!(f.def, FeatureDef::Sketch(_)))
            .count();
        let mut renamed: Vec<PieceRenames> = Vec::new();
        let mut resume = 0;
        for _ in 0..=sketches + next.features.len() {
            let mut whole = next.clone();
            whole.marker = all.max(next.features.len());
            let result = self.compute(&whole)?;
            let fixes = frame_fixes(&next, &result, &old_frames, turn, &turned);
            if !fixes.is_empty() {
                for (uid, frame) in fixes {
                    set_sketch_frame(&mut next, uid, frame);
                    let name = next.entry(uid).map(|f| f.name.clone()).unwrap_or_default();
                    if !report.features.contains(&name) {
                        report.features.push(name);
                    }
                }
                continue;
            }
            let Some((index, renames)) =
                self.piece_renames(&next, &old_whole, &result, turn, &turned, &renamed, resume)
            else {
                break;
            };
            for i in index..next.features.len() {
                let entry = &next.features[i];
                let def = crate::reorient::rename_pieces(&entry.def, &renames).map_err(|e| {
                    invalid(format!("Reorient Design cannot turn {}: {e}", entry.name))
                })?;
                if let Some(def) = def {
                    let mut changed = (**entry).clone();
                    changed.def = def;
                    if !report.features.contains(&changed.name) {
                        report.features.push(changed.name.clone());
                    }
                    next.features[i] = Arc::new(changed);
                }
            }
            renamed.push(renames);
            resume = index;
        }
        sort_by_timeline(&next, &mut report.features);

        // The turned design at the marker: the same bodies turned, and
        // every feature that built still builds (the Moves added after base
        // features too).
        let result = self.compute(&next)?;
        let old_uids: BTreeSet<FeatureUid> = self.state.features.iter().map(|f| f.uid).collect();
        let mut failed = Vec::new();
        for (i, f) in next.features.iter().enumerate() {
            let added = !old_uids.contains(&f.uid);
            if !added && !old_ok.contains(&f.uid) {
                continue;
            }
            let Some(error) = result.result(f.uid).and_then(|r| r.status.error()) else {
                continue;
            };
            let mut place = Vec::new();
            if added && i > 0 {
                place.push(format!("after {}", next.features[i - 1].name));
            }
            if !f.component.is_root() {
                place.push(format!("in {}", next.assembly.name(f.component)));
            }
            let place = if place.is_empty() {
                String::new()
            } else {
                format!(" ({})", place.join(", "))
            };
            failed.push(format!("{}{place} would fail: {error}", f.name));
        }
        if !failed.is_empty() {
            return Err(invalid(format!(
                "Reorient Design cannot turn the design: {}",
                failed.join("; ")
            )));
        }
        let new_boxes = self.boxes_of(&result)?;
        let mut moved = Vec::new();
        for (body, (component, old)) in &old_boxes {
            let q = if turned.contains(component) {
                turn
            } else {
                Turn::IDENTITY
            };
            let Some(off) = deviation(q, old, new_boxes.get(body).map(|(_, n)| n)) else {
                continue;
            };
            let label = self.body_label(&next, *body, *component);
            moved.push(match self.departure(&next, &result, *body, q) {
                Some((feature, off)) => format!(
                    "{label} departs from the turn after {feature}: {}",
                    off.describe()
                ),
                None => format!(
                    "{label} would not be where the turn takes it: {}",
                    off.describe()
                ),
            });
        }
        // Components where their placements take them (joints solve them
        // anew), in the turned coordinates of both sides.
        let context = crate::reorient::Turned {
            turn,
            turned: &turned,
            assembly: &next.assembly,
        };
        for (occurrence, was) in &self.result.placements {
            let Some(o) = next.assembly.occurrence(*occurrence) else {
                continue;
            };
            let expected = context.placed_in(o.parent, o.component, was);
            let same = result.placements.get(occurrence).is_some_and(|now| {
                let size = was.translation.iter().fold(1.0_f64, |m, v| m.max(v.abs()));
                (0..3).all(|r| {
                    (expected.translation[r] - now.translation[r]).abs() <= 1e-6 * size
                        && (0..3).all(|c| (expected.linear[r][c] - now.linear[r][c]).abs() <= 1e-9)
                })
            });
            if !same {
                moved.push(format!(
                    "{} would not be where the turn takes it",
                    next.assembly.occurrence_name(*occurrence)
                ));
            }
        }
        if !moved.is_empty() {
            return Err(invalid(format!(
                "Reorient Design cannot turn the design exactly: {}",
                moved.join("; ")
            )));
        }
        self.apply(move |state| {
            *state = next;
            Ok(("Reorient Design".to_owned(), report))
        })
    }

    /// Each body of a recompute with its component, bounding box and
    /// centre of mass (when the kernel measures it).
    fn boxes_of(
        &self,
        result: &Recomputed<K::Shape>,
    ) -> Result<BTreeMap<BodyUid, (ComponentUid, BodyFacts)>, ModelError> {
        result
            .bodies
            .iter()
            .map(|(uid, shape)| {
                let component = result
                    .owners
                    .get(uid)
                    .copied()
                    .unwrap_or(ComponentUid::ROOT);
                let bounds = self
                    .kernel
                    .bounding_box(&shape.value)
                    .map_err(|e| invalid(format!("Reorient Design: {uid}: {e}")))?;
                Ok((*uid, (component, self.facts(&shape.value, bounds))))
            })
            .collect()
    }

    fn facts(&self, shape: &K::Shape, bounds: Option<BoundingBox>) -> BodyFacts {
        let center = self
            .kernel
            .mass_properties(shape)
            .ok()
            .map(|m| m.center)
            .filter(|c| c.iter().all(|v| v.is_finite()));
        BodyFacts { bounds, center }
    }

    /// The Embosses of turned components whose wrap the turn makes count
    /// along its face's axis the other way (mitcad#1315,
    /// [`crate::reorient::wrap_count_reversed`]), from the axis of the face
    /// as each found it in the whole timeline before the turn: the face's
    /// body as the last feature before it set it. Those without Along Axis
    /// or Around, projected ones and those on a face that is no cylinder or
    /// cone are left out.
    fn reversed_wraps(
        &self,
        whole: &Recomputed<K::Shape>,
        turn: Turn,
        turned: &BTreeSet<ComponentUid>,
    ) -> BTreeSet<FeatureUid> {
        let mut reversed = BTreeSet::new();
        for entry in &self.state.features {
            let FeatureDef::Emboss(emboss) = &entry.def else {
                continue;
            };
            let Some(face) = &emboss.face else {
                continue;
            };
            let moved = emboss.along_axis.is_some() || emboss.around_angle.is_some();
            let projected = emboss.projection || emboss.direction.is_some();
            if !moved || projected || !turned.contains(&entry.component) {
                continue;
            }
            let Some(shape) = whole
                .results
                .iter()
                .take_while(|r| r.uid != entry.uid)
                .filter_map(|r| r.shape_set(face.body))
                .last()
            else {
                continue;
            };
            let name = match crate::features::renamed::find_face(&self.kernel, shape, &face.face) {
                Ok(None) => face.face.clone(),
                Ok(Some(found)) => found,
                Err(_) => continue,
            };
            let axis = match self.kernel.face_cylinder(shape, &name) {
                Ok(cylinder) => cylinder.axis.direction,
                Err(_) => match self.kernel.face_cone(shape, &name) {
                    Ok(cone) => cone.axis,
                    Err(_) => continue,
                },
            };
            if crate::reorient::wrap_count_reversed(turn, axis) {
                reversed.insert(entry.uid);
            }
        }
        reversed
    }

    /// `Body17 of Part (made by Base1)`; the root's bodies without their
    /// component.
    fn body_label(&self, state: &DocState, body: BodyUid, component: ComponentUid) -> String {
        let mut label = state
            .body_names
            .get(&body)
            .cloned()
            .unwrap_or_else(|| body.to_string());
        if !component.is_root() {
            label = format!("{label} of {}", state.assembly.name(component));
        }
        match state.entry(body.feature) {
            Some(feature) => format!("{label} (made by {})", feature.name),
            None => label,
        }
    }

    /// The first feature after which a body of the turned design `new` (at
    /// the marker) is not its old self (the document's result) turned,
    /// with how far it is then: the features that made or changed it in
    /// timeline order, a Move added after a base feature counting with it.
    fn departure(
        &self,
        next: &DocState,
        new: &Recomputed<K::Shape>,
        body: BodyUid,
        turn: Turn,
    ) -> Option<(String, Deviation)> {
        let old = &self.result;
        let old_uids: BTreeSet<FeatureUid> = old.results.iter().map(|r| r.uid).collect();
        // The body after each old feature in the turned design (None: not
        // made, or removed), and the name of the feature that made it so.
        let mut after: BTreeMap<FeatureUid, (String, Option<&K::Shape>)> = BTreeMap::new();
        let mut last_old = None;
        let new_results: BTreeMap<FeatureUid, _> = new.results.iter().map(|r| (r.uid, r)).collect();
        for entry in &next.features {
            if old_uids.contains(&entry.uid) {
                last_old = Some(entry.uid);
            }
            let (Some(at), Some(r)) = (last_old, new_results.get(&entry.uid)) else {
                continue;
            };
            let Some(output) = &r.output else { continue };
            for change in &output.changes {
                match change {
                    crate::recompute::Change::Set(uid, shape) if *uid == body => {
                        after.insert(at, (entry.name.clone(), Some(&shape.value)));
                    }
                    crate::recompute::Change::Remove(uid) if *uid == body => {
                        after.insert(at, (entry.name.clone(), None));
                    }
                    _ => {}
                }
            }
        }
        for r in &old.results {
            let was = r.shape_set(body);
            let now = after.get(&r.uid);
            if was.is_none() && now.is_none() {
                continue;
            }
            let name = now.map(|(n, _)| n.clone()).unwrap_or_else(|| {
                next.entry(r.uid)
                    .map(|f| f.name.clone())
                    .unwrap_or_default()
            });
            let Some(was) = was else {
                // Made or changed only in the turned design.
                continue;
            };
            let measure = |shape: &K::Shape| {
                let bounds = self.kernel.bounding_box(shape).ok().flatten();
                self.facts(shape, bounds)
            };
            let old_facts = measure(was);
            let now_facts = now.and_then(|(_, s)| s.map(measure));
            if let Some(off) = deviation(turn, &old_facts, now_facts.as_ref()) {
                return Some((name, off));
            }
        }
        None
    }

    /// The renames of the pieces and numbered edges the first feature from
    /// `resume` on refers to whose numbers the turned design `new` gives
    /// to other pieces than the old design `old` (both the whole timeline),
    /// with that feature's index: each piece the one of its family whose
    /// box (an edge: its middle) is where the turn takes the old piece's.
    /// `renamed` are the renames so far, which the old design's names are
    /// read through.
    #[allow(clippy::too_many_arguments)]
    fn piece_renames(
        &self,
        state: &DocState,
        old: &Recomputed<K::Shape>,
        new: &Recomputed<K::Shape>,
        turn: Turn,
        turned: &BTreeSet<ComponentUid>,
        renamed: &[PieceRenames],
        resume: usize,
    ) -> Option<(usize, PieceRenames)> {
        let old_results: BTreeMap<FeatureUid, _> = old.results.iter().map(|r| (r.uid, r)).collect();
        let new_results: BTreeMap<FeatureUid, _> = new.results.iter().map(|r| (r.uid, r)).collect();
        // The bodies before each feature, in both designs.
        let mut old_in: BTreeMap<BodyUid, &K::Shape> = BTreeMap::new();
        let mut new_in: BTreeMap<BodyUid, &K::Shape> = BTreeMap::new();
        for (index, entry) in state.features.iter().enumerate() {
            if index >= resume && turned.contains(&entry.component) {
                let (names, bodies) = crate::reorient::numbered_names(&entry.def);
                if !names.is_empty() {
                    let input = Input {
                        old: &old_in,
                        new: &new_in,
                        bodies: &bodies,
                        renamed,
                    };
                    let renames = self.match_pieces(state, &names, &input, turn, turned);
                    if !renames.is_empty() {
                        return Some((index, renames));
                    }
                }
            }
            take_changes(&mut old_in, old_results.get(&entry.uid).copied());
            take_changes(&mut new_in, new_results.get(&entry.uid).copied());
        }
        None
    }

    /// The renames that make the numbered names a feature refers to name
    /// the pieces the turn takes their old pieces onto (see
    /// [`Document::piece_renames`]). A family whose pieces do not pair up
    /// one to one is left as it is.
    fn match_pieces(
        &self,
        state: &DocState,
        names: &[TopoName],
        input: &Input<'_, K::Shape>,
        turn: Turn,
        turned: &BTreeSet<ComponentUid>,
    ) -> PieceRenames {
        let mut renames = PieceRenames::default();
        let turn_of = |body: &BodyUid| {
            let component = state
                .entry(body.feature)
                .map_or(ComponentUid::ROOT, |f| f.component);
            if turned.contains(&component) {
                turn
            } else {
                Turn::IDENTITY
            }
        };
        // Candidate bodies: those the definition names, else all.
        let candidates: Vec<BodyUid> = if input.bodies.iter().any(|b| input.old.contains_key(b)) {
            input.bodies.to_vec()
        } else {
            input.old.keys().copied().collect()
        };
        let mut old_faces = BTreeMap::new();
        let mut new_faces = BTreeMap::new();
        // Faces that are pieces, on their own or in edges and vertices.
        let mut pieces: Vec<&FaceName> = Vec::new();
        for name in names {
            match name {
                TopoName::Face(face) => pieces.push(face),
                TopoName::Edge(edge) => pieces.extend(edge.faces()),
                TopoName::Vertex(vertex) => pieces.extend(vertex.faces()),
            }
        }
        pieces.retain(|f| !f.split.is_empty());
        let mut families: Vec<&FaceName> = Vec::new();
        for face in pieces {
            if families.iter().any(|f| same_family(f, face)) {
                continue;
            }
            families.push(face);
            for body in &candidates {
                let (Some(old_shape), Some(new_shape)) = (input.old.get(body), input.new.get(body))
                else {
                    continue;
                };
                let Some(olds) = old_faces
                    .entry(*body)
                    .or_insert_with(|| self.named_face_boxes(old_shape, input.renamed))
                    .as_ref()
                else {
                    continue;
                };
                if !olds.iter().any(|(n, _)| n.contains(face)) {
                    continue;
                }
                let Some(news) = new_faces
                    .entry(*body)
                    .or_insert_with(|| self.named_face_boxes(new_shape, &[]))
                    .as_ref()
                else {
                    break;
                };
                let q = turn_of(body);
                let family = |faces: &[(Vec<FaceName>, BoundingBox)], turn: Turn| {
                    faces
                        .iter()
                        .filter_map(|(names, b)| {
                            let name = names.iter().find(|n| same_family(n, face))?;
                            Some((name.clone(), turn_box(turn, b)))
                        })
                        .collect::<Vec<_>>()
                };
                let pairs =
                    pair_nearest(&family(olds, q), &family(news, Turn::IDENTITY), |a, b| {
                        (0..3).fold(0.0_f64, |m, i| {
                            m.max((a.min[i] - b.min[i]).abs())
                                .max((a.max[i] - b.max[i]).abs())
                        })
                    });
                for (was, now) in pairs.into_iter().flatten() {
                    if was != now {
                        renames.faces.insert(was.to_string(), now);
                    }
                }
                break;
            }
        }
        // Edges numbered between the same two faces: by their middles.
        for name in names {
            let TopoName::Edge(edge) = name else {
                continue;
            };
            if edge.index.is_none() {
                continue;
            }
            let target = renames.edge(edge);
            for body in &candidates {
                let (Some(old_shape), Some(new_shape)) = (input.old.get(body), input.new.get(body))
                else {
                    continue;
                };
                let Ok(old_middles) = self.kernel.edge_middles(old_shape) else {
                    continue;
                };
                let q = turn_of(body);
                let read = |text: &str, renamed: &[PieceRenames]| {
                    let parsed = text.parse::<EdgeName>().ok()?;
                    Some(renamed.iter().fold(parsed, |e, r| r.edge(&e)))
                };
                let olds: Vec<(EdgeName, [f64; 3])> = old_middles
                    .iter()
                    .filter_map(|m| Some((read(&m.name, input.renamed)?, q.vector(m.point))))
                    .filter(|(e, _)| e.faces() == edge.faces())
                    .collect();
                if !olds.iter().any(|(e, _)| e == edge) {
                    continue;
                }
                let Ok(new_middles) = self.kernel.edge_middles(new_shape) else {
                    break;
                };
                let news: Vec<(EdgeName, [f64; 3])> = new_middles
                    .iter()
                    .filter_map(|m| Some((read(&m.name, &[])?, m.point)))
                    .filter(|(e, _)| e.faces() == target.faces())
                    .collect();
                let pairs = pair_nearest(&olds, &news, |a, b| {
                    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
                });
                // The edges keyed by their names with this round's faces.
                let faces_only = PieceRenames {
                    faces: renames.faces.clone(),
                    edges: BTreeMap::new(),
                };
                for (was, now) in pairs.into_iter().flatten() {
                    if faces_only.edge(&was) != now {
                        renames.edges.insert(was.to_string(), now);
                    }
                }
                break;
            }
        }
        renames
    }

    /// The faces of a shape with their names (read through `renamed`) and
    /// boxes; None where the kernel does not list them.
    fn named_face_boxes(
        &self,
        shape: &K::Shape,
        renamed: &[PieceRenames],
    ) -> Option<Vec<(Vec<FaceName>, BoundingBox)>> {
        let faces = self.kernel.faces(shape).ok()?;
        let boxes = self.kernel.face_boxes(shape).ok()?;
        if faces.len() != boxes.len() {
            return None;
        }
        Some(
            faces
                .into_iter()
                .zip(boxes)
                .map(|(face, [min, max])| {
                    let names = face
                        .names
                        .iter()
                        .filter_map(|n| n.parse::<FaceName>().ok())
                        .map(|n| renamed.iter().fold(n, |f, r| r.face(&f)))
                        .collect();
                    (names, BoundingBox { min, max })
                })
                .collect(),
        )
    }
}

/// The bodies before a feature in the old and the turned design, the
/// bodies its definition names and the renames so far.
struct Input<'a, S> {
    old: &'a BTreeMap<BodyUid, &'a S>,
    new: &'a BTreeMap<BodyUid, &'a S>,
    bodies: &'a [BodyUid],
    renamed: &'a [PieceRenames],
}

/// The bodies after a feature: its result's changes taken.
fn take_changes<'a, S>(
    bodies: &mut BTreeMap<BodyUid, &'a S>,
    result: Option<&'a crate::recompute::FeatureResult<S>>,
) {
    let Some(output) = result.and_then(|r| r.output.as_ref()) else {
        return;
    };
    for change in &output.changes {
        match change {
            crate::recompute::Change::Set(uid, shape) => {
                bodies.insert(*uid, &shape.value);
            }
            crate::recompute::Change::Remove(uid) => {
                bodies.remove(uid);
            }
        }
    }
}

/// Pieces of one split face: the same face, the same number of splits.
fn same_family(a: &FaceName, b: &FaceName) -> bool {
    a.feature == b.feature && a.role == b.role && a.key == b.key && a.split.len() == b.split.len()
}

/// Each old item with the new one nearest to it, when that pairs them one
/// to one; None otherwise.
fn pair_nearest<N: Clone, P>(
    old: &[(N, P)],
    new: &[(N, P)],
    distance: impl Fn(&P, &P) -> f64,
) -> Option<Vec<(N, N)>> {
    if old.is_empty() || old.len() != new.len() {
        return None;
    }
    let mut used = vec![false; new.len()];
    let mut pairs = Vec::with_capacity(old.len());
    for (name, at) in old {
        let (best, _) = new
            .iter()
            .enumerate()
            .map(|(i, (_, p))| (i, distance(at, p)))
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        if used[best] {
            return None;
        }
        used[best] = true;
        pairs.push((name.clone(), new[best].0.clone()));
    }
    Some(pairs)
}

/// The components that turn: all but those linked from other files.
fn turned_components(state: &DocState) -> BTreeSet<ComponentUid> {
    let mut turned = BTreeSet::from([ComponentUid::ROOT]);
    turned.extend(
        state
            .assembly
            .components
            .iter()
            .filter(|c| c.link.is_none())
            .map(|c| c.uid),
    );
    turned
}

/// Rewrites the state (see [`crate::reorient`]): definitions, the Moves of
/// base features, occurrences, named views, section analyses and lights.
fn turn_state(
    state: &mut DocState,
    turn: Turn,
    turned: &BTreeSet<ComponentUid>,
    reversed: &BTreeSet<FeatureUid>,
    report: &mut ReorientReport,
) -> Result<(), ModelError> {
    let assembly = state.assembly.clone();
    let context = Turned {
        turn,
        turned,
        assembly: &assembly,
    };
    // Definitions.
    let mut bases = Vec::new();
    let mut negated = Vec::new();
    for i in 0..state.features.len() {
        let entry = &state.features[i];
        if !turned.contains(&entry.component) {
            continue;
        }
        let (def, negate) = crate::reorient::turn_definition(&entry.def, entry.component, &context)
            .map_err(|e| invalid(format!("Reorient Design cannot turn {}: {e}", entry.name)))?;
        negated.extend(negate.into_iter().map(|id| (entry.uid, id)));
        // An Emboss whose wrap counts along its face's axis the other way
        // now (mitcad#1315): its Along Axis and Around are negated.
        if let FeatureDef::Emboss(emboss) = &entry.def
            && reversed.contains(&entry.uid)
        {
            negated.extend(
                [emboss.along_axis, emboss.around_angle]
                    .into_iter()
                    .flatten()
                    .map(|id| (entry.uid, id)),
            );
        }
        if let FeatureDef::Base(base) = &entry.def {
            bases.push((entry.uid, base.clone()));
        }
        if def != entry.def {
            report.features.push(entry.name.clone());
            let mut changed = (**entry).clone();
            changed.def = def;
            state.features[i] = Arc::new(changed);
        }
    }
    // Values along the model axes (a Move's x, y, z) went with their axes;
    // those of an axis that now runs the other way are negated.
    negate_parameters(state, &negated)?;
    // Imported bodies: a Move right after each base feature turns them.
    for (uid, base) in bases {
        let name = add_base_move(state, uid, &base, turn)?;
        report.moves.push(name);
    }
    // Occurrences: their placements, in both coordinates they join.
    for o in &mut state.assembly.occurrences {
        if !turned.contains(&o.parent) {
            continue;
        }
        let placed = context.placed_in(o.parent, o.component, &o.transform);
        if placed != o.transform {
            o.transform = placed;
            report
                .occurrences
                .push(assembly.occurrence_name(o.uid).to_string());
        }
    }
    // Named views, section analyses and the lights in the design.
    for view in &mut state.named_views {
        view.eye = turn.vector(view.eye);
        view.target = turn.vector(view.target);
        view.up = turn.vector(view.up);
        report.views += 1;
    }
    for analysis in &mut state.analyses {
        if crate::reorient::turn_analysis(&mut analysis.def, &context) {
            report.analyses += 1;
        }
    }
    report.lights += crate::reorient::turn_lights(&mut state.render, turn);
    // Image references' planes and comments' points and places
    // (mitcad#1315).
    for image in &mut state.image_references {
        crate::reorient::turn_image_reference(image, &context);
    }
    for comment in &mut state.comments {
        crate::reorient::turn_comment(comment, &context);
    }
    // What split features remember of their pieces (centres of mass).
    crate::reorient::turn_split_pieces(state, turn, turned);
    Ok(())
}

/// Negates the dimension parameters of features' values along the model
/// axes whose axis the turn reverses. Only a feature's own parameter that
/// nothing else uses can change so; otherwise the command is refused, as
/// the other users would change with it.
fn negate_parameters(
    state: &mut DocState,
    negated: &[(FeatureUid, crate::parameters::ParamId)],
) -> Result<(), ModelError> {
    let mut expressions: Vec<(crate::parameters::ParamId, String)> = Vec::new();
    for (feature, id) in negated {
        let Some(parameter) = state.parameters.get(*id) else {
            continue;
        };
        if parameter.value() == 0.0 {
            continue;
        }
        let name = parameter.name().to_owned();
        let feature_name = state
            .entry(*feature)
            .map(|f| f.name.clone())
            .unwrap_or_default();
        let uses = state
            .features
            .iter()
            .flat_map(|f| f.def.params())
            .filter(|p| p == id)
            .count();
        let shared = state.parameters.owner(*id) != Some(*feature)
            || uses != 1
            || !state.parameters.table().dependents(*id).is_empty()
            || state.configurations.parameters.contains(&name);
        if shared {
            return Err(invalid(format!(
                "Reorient Design cannot turn {feature_name}: its value {name} runs along an axis \
                 the turn reverses, and other values or features use it"
            )));
        }
        let expression = parameter.expression().trim();
        let negative = if parameter.dependencies().is_empty() {
            match expression.strip_prefix('-') {
                Some(rest) => rest.trim().to_owned(),
                None => format!("-{expression}"),
            }
        } else {
            format!("-({expression})")
        };
        expressions.push((*id, negative));
    }
    if expressions.is_empty() {
        return Ok(());
    }
    let pairs: Vec<(crate::parameters::ParamId, &str)> = expressions
        .iter()
        .map(|(id, text)| (*id, text.as_str()))
        .collect();
    state
        .parameters
        .set_expressions(&pairs)
        .map_err(ModelError::Parameter)?;
    Ok(())
}

/// Adds a Move of the base feature's new bodies right after it, turning
/// them about the origin. Returns its name.
fn add_base_move(
    state: &mut DocState,
    base: FeatureUid,
    def: &crate::features::BaseDef,
    turn: Turn,
) -> Result<String, ModelError> {
    let index = state
        .features
        .iter()
        .position(|f| f.uid == base)
        .expect("the base feature is in the timeline");
    let entry = state.features[index].clone();
    if def.operation != crate::features::Operation::NewBody {
        return Err(invalid(format!(
            "Reorient Design cannot turn {}: its bodies {} other bodies, so a Move cannot turn \
             them alone",
            entry.name,
            match def.operation {
                crate::features::Operation::Join => "join",
                crate::features::Operation::Cut => "cut",
                _ => "intersect",
            }
        )));
    }
    // By the body's own id: a body kept after Find Repeated Parts took
    // copies before it out keeps its number, not its place (mitcad#1306).
    let bodies: Vec<BodyUid> = (0u32..)
        .take(def.bodies.len())
        .map(|i| def.body_uid(base, i))
        .collect();
    let matrix = crate::assembly::matrix_rows(&turn.transform());
    let mv: FeatureDef<crate::features::ValueInput> = FeatureDef::Move(crate::features::MoveDef {
        bodies,
        transform: crate::features::moves::MoveSpec::Free { matrix },
        copy: false,
    });
    // In at the base feature's place, the marker kept after the features
    // it was after.
    let marker = state.marker;
    state.marker = index + 1;
    let added = state.add_feature_in(&mv, None, entry.component)?;
    state.marker = if index < marker { marker + 1 } else { marker };
    // A suppressed base feature's Move is suppressed with it.
    if entry.suppressed
        && let Some(i) = state.position(added.uid)
    {
        let mut moved: FeatureEntry = (*state.features[i]).clone();
        moved.suppressed = true;
        state.features[i] = Arc::new(moved);
    }
    Ok(added.name)
}

/// The features that built at the marker.
fn ok_features<S>(result: &Recomputed<S>) -> BTreeSet<FeatureUid> {
    result
        .results
        .iter()
        .filter(|r| r.status.is_ok())
        .map(|r| r.uid)
        .collect()
}

/// The sketches' base frames, before their placements on their planes
/// (mitcad#787): the own frame the fixes give is what moves them, and the
/// placement turns with it.
fn sketch_frames<S>(result: &Recomputed<S>) -> BTreeMap<FeatureUid, SketchFrame> {
    result
        .sketches
        .iter()
        .map(|(uid, output)| (*uid, output.value.base))
        .collect()
}

/// The sketches whose turned frame is not their old one turned, with the
/// own frame ([`FrameDef`]) that makes it so.
fn frame_fixes<S>(
    state: &DocState,
    result: &Recomputed<S>,
    old: &BTreeMap<FeatureUid, SketchFrame>,
    turn: Turn,
    turned: &BTreeSet<ComponentUid>,
) -> Vec<(FeatureUid, Option<FrameDef>)> {
    let mut fixes = Vec::new();
    for entry in &state.features {
        let FeatureDef::Sketch(sketch) = &entry.def else {
            continue;
        };
        if !turned.contains(&entry.component) {
            continue;
        }
        let (Some(was), Some(now)) = (old.get(&entry.uid), result.sketches.get(&entry.uid)) else {
            continue;
        };
        let target = turn.frame(was);
        let now = now.value.base;
        if same_frame(&now, &target) {
            continue;
        }
        // The target in the current frame's coordinates, then in the
        // plane's through the own frame the sketch has.
        let correction = relative(&now, &target);
        let own = match &sketch.frame {
            Some(own) => compose(own, &correction),
            None => correction,
        };
        // A sketch that stands on one corrected in this round is looked
        // at again in the next.
        fixes.push((entry.uid, (!is_identity(&own)).then_some(own)));
    }
    fixes
}

fn set_sketch_frame(state: &mut DocState, uid: FeatureUid, frame: Option<FrameDef>) {
    if let Some(i) = state.features.iter().position(|f| f.uid == uid) {
        let mut entry: FeatureEntry = (*state.features[i]).clone();
        if let FeatureDef::Sketch(sketch) = &mut entry.def {
            sketch.frame = frame.map(Box::new);
        }
        state.features[i] = Arc::new(entry);
    }
}

fn sort_by_timeline(state: &DocState, names: &mut [String]) {
    let order: BTreeMap<&str, usize> = state
        .features
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();
    names.sort_by_key(|name| order.get(name.as_str()).copied().unwrap_or(usize::MAX));
}

fn same_frame(a: &SketchFrame, b: &SketchFrame) -> bool {
    let near = |u: [f64; 3], v: [f64; 3], scale: f64| {
        (0..3).all(|i| (u[i] - v[i]).abs() <= FRAME_TOLERANCE * scale)
    };
    let size = a
        .origin
        .iter()
        .chain(&b.origin)
        .fold(1.0_f64, |m, v| m.max(v.abs()));
    near(a.origin, b.origin, size) && near(a.x_axis, b.x_axis, 1.0) && near(a.y_axis, b.y_axis, 1.0)
}

/// `target` in the coordinates of `frame` (x, y, normal).
fn relative(frame: &SketchFrame, target: &SketchFrame) -> FrameDef {
    let n = frame.normal();
    let local = |v: [f64; 3]| [dot(v, frame.x_axis), dot(v, frame.y_axis), dot(v, n)];
    let offset: [f64; 3] = std::array::from_fn(|i| target.origin[i] - frame.origin[i]);
    FrameDef {
        origin: local(offset),
        x_axis: local(target.x_axis),
        y_axis: local(target.y_axis),
    }
}

/// The own frame `inner` (in the sketch frame `outer` makes) as one in
/// the plane's coordinates.
fn compose(outer: &FrameDef, inner: &FrameDef) -> FrameDef {
    let n = cross(outer.x_axis, outer.y_axis);
    let place = |v: [f64; 3]| -> [f64; 3] {
        std::array::from_fn(|i| v[0] * outer.x_axis[i] + v[1] * outer.y_axis[i] + v[2] * n[i])
    };
    let o = place(inner.origin);
    FrameDef {
        origin: std::array::from_fn(|i| outer.origin[i] + o[i]),
        x_axis: place(inner.x_axis),
        y_axis: place(inner.y_axis),
    }
}

fn is_identity(frame: &FrameDef) -> bool {
    let near = |u: [f64; 3], v: [f64; 3]| (0..3).all(|i| (u[i] - v[i]).abs() <= 1e-12);
    near(frame.origin, [0.0; 3])
        && near(frame.x_axis, [1.0, 0.0, 0.0])
        && near(frame.y_axis, [0.0, 1.0, 0.0])
}

fn turn_box(turn: Turn, b: &BoundingBox) -> BoundingBox {
    let (p, q) = (turn.vector(b.min), turn.vector(b.max));
    BoundingBox {
        min: std::array::from_fn(|i| p[i].min(q[i])),
        max: std::array::from_fn(|i| p[i].max(q[i])),
    }
}

/// The largest side of a box, at least 1 mm.
fn box_size(b: &BoundingBox) -> f64 {
    (0..3).fold(1.0_f64, |m, i| m.max(b.max[i] - b.min[i]))
}

/// Boxes within 1e-6 of their size (at least 1e-6 mm).
fn same_box(a: Option<&BoundingBox>, b: Option<&BoundingBox>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let tolerance = 1e-6 * box_size(a);
            (0..3).all(|i| {
                (a.min[i] - b.min[i]).abs() <= tolerance && (a.max[i] - b.max[i]).abs() <= tolerance
            })
        }
        _ => false,
    }
}
