// SPDX-License-Identifier: MIT
//! Find Repeated Parts (mitcad#1277): the bodies of a base feature (an
//! imported file's) that are copies of one another moved by a rigid motion
//! become occurrences of one component. The first body of each kind moves
//! into a new component named after it, where it is, by a
//! `component_from_bodies` feature (it keeps its id, name, colour and
//! appearance); each copy becomes an occurrence of that component placed
//! where the copy was, and its B-rep data leaves the base feature, which
//! keeps the ids and face names of the bodies it still holds
//! (`BaseBody::number`, `first_face`). Bodies that copy none stay in the
//! base feature. The components' features go right after the base
//! feature; a later combine's tools among the bodies become the first body
//! where each occurrence shows it, and the bodies other later features
//! name stay as they are, as do the bodies a print plate item or a comment
//! names. A copy has the colour, attributes and own properties (print
//! values among them) of the first body of its kind, which its occurrence
//! shows, so nothing of it is lost (mitcad#1312). One undo step. Matching
//! is the kernel's ([`Kernel::rigid_copies`]): a mirror image is not a
//! copy.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::{Document, ModelError, invalid};
use crate::assembly::{inverse, is_rigid};
use crate::features::combine::CombineTool;
use crate::features::{
    BaseBody, ComponentFromBodiesDef, FeatureDef, OccurrencePath, Operation, ValueInput,
};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::links::OccurrenceLink;
use crate::recompute::Change;
use crate::transform::Transform;

/// The tolerance (mm) of the comparison when neither the command nor the
/// design (an importer's join tolerance) gives one.
pub const REPEATED_PARTS_TOLERANCE: f64 = 0.01;

/// What [`Document::find_repeated_parts`] made.
#[derive(Debug, Clone, PartialEq)]
pub struct RepeatedParts {
    /// A component per kind of body found more than once.
    pub parts: Vec<RepeatedPart>,
    /// The base feature's bodies that copy none, or that a later feature
    /// other than a combine's tools, a print plate item or a comment uses,
    /// left in it.
    pub kept: Vec<BodyUid>,
    /// The combines whose tools are now the components' bodies where the
    /// occurrences show them.
    pub combines: Vec<FeatureUid>,
}

/// A component of [`RepeatedParts`].
#[derive(Debug, Clone, PartialEq)]
pub struct RepeatedPart {
    pub component: ComponentUid,
    pub name: String,
    /// The `component_from_bodies` feature that moved the first body into it.
    pub feature: FeatureUid,
    /// The first body, now the component's.
    pub body: BodyUid,
    /// Its occurrences: the first body's own (where it was), then one per
    /// copy, in the order of `copies`.
    pub occurrences: Vec<OccurrenceUid>,
    /// The bodies the occurrences after the first stand for, taken out of
    /// the base feature.
    pub copies: Vec<BodyUid>,
}

/// A body of the base feature that may be a copy: its place in the
/// feature, its id and what it looks like (colour, attributes and its own
/// properties, print values among them, which a copy must share with the
/// first body of its kind: the occurrence that takes its place shows the
/// first body, mitcad#1312).
struct Candidate {
    index: usize,
    uid: BodyUid,
    look: Look,
}

/// A body's colour (as bits), attributes and own property sets.
type Look = (
    Option<[u64; 3]>,
    super::BodyAttributes,
    Vec<crate::properties::PropertyGroup>,
);

/// The bodies of one component: the first, and the others with the
/// placement from the first onto each.
struct Group {
    first: usize,
    /// The kernel's first body of the kind, and the placement from it onto
    /// `first`.
    root: usize,
    from_root: Transform,
    copies: Vec<(usize, Transform)>,
}

impl<K: Kernel> Document<K> {
    /// Find Repeated Parts on a base feature (see the module): `tolerance`
    /// in mm, else the design's join tolerance, else
    /// [`REPEATED_PARTS_TOLERANCE`]. Refused for another feature, a base
    /// feature that joins, cuts, intersects or replaces bodies, and one
    /// rolled back. Later combines of the feature's component take their
    /// tools among the bodies where the occurrences show them; the bodies
    /// other later uses name stay. Nothing found changes nothing (no undo
    /// step).
    pub fn find_repeated_parts(
        &mut self,
        feature: FeatureUid,
        tolerance: Option<f64>,
    ) -> Result<RepeatedParts, ModelError> {
        let tolerance = tolerance
            .or(self.state.join_tolerance)
            .unwrap_or(REPEATED_PARTS_TOLERANCE);
        if !(tolerance > 0.0 && tolerance.is_finite()) {
            return Err(invalid(format!(
                "the tolerance must be greater than zero, got {tolerance} mm"
            )));
        }
        let position = self.state.require(feature)?;
        let entry = self.state.features[position].clone();
        let FeatureDef::Base(base) = &entry.def else {
            return Err(invalid(format!(
                "{} is not a base feature: Find Repeated Parts works on imported bodies",
                entry.name
            )));
        };
        if base.operation != Operation::NewBody || !base.replaces.is_empty() {
            return Err(invalid(format!(
                "{} does not make new bodies of its own",
                entry.name
            )));
        }
        if position >= self.state.marker {
            return Err(invalid(format!("{} is rolled back", entry.name)));
        }
        let owner = entry.component;
        self.state.editable(owner)?;
        let uids: Vec<BodyUid> = (0u32..)
            .zip(&base.bodies)
            .map(|(i, _)| base.body_uid(feature, i))
            .collect();
        // Each body's first face and face count, as the feature numbers its
        // faces (`features/base.rs`).
        // The bodies as the feature made them (a later combine may have
        // consumed them by the marker).
        let made: BTreeMap<BodyUid, &K::Shape> = self
            .feature_result(feature)
            .and_then(|r| r.output.as_ref())
            .map(|output| {
                output
                    .changes
                    .iter()
                    .filter_map(|change| match change {
                        Change::Set(uid, shape) => Some((*uid, &shape.value)),
                        Change::Remove(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let too_many = || invalid("Find Repeated Parts: too many faces");
        let mut next_face = 0u32;
        let mut faces_of = Vec::with_capacity(base.bodies.len());
        for (body, uid) in base.bodies.iter().zip(&uids) {
            let shape = made.get(uid).copied().ok_or_else(|| {
                invalid(format!(
                    "{} has no result to compare: {} did not build",
                    self.body_name(*uid),
                    entry.name
                ))
            })?;
            let count = self
                .kernel()
                .topology_counts(shape)
                .map_err(|e| invalid(format!("Find Repeated Parts: {e}")))?
                .faces;
            let count = u32::try_from(count).map_err(|_| too_many())?;
            let first = body.first_face.unwrap_or(next_face);
            next_face = first.checked_add(count).ok_or_else(too_many)?;
            faces_of.push((first, count));
        }
        // The later features that use the bodies: a combine of the feature's
        // component takes its tools where the occurrences show them; the
        // bodies any other use names (a combine's target too) stay as they
        // are.
        let users = self.state.dependents(&BTreeSet::from([feature]));
        let mut combines = Vec::new();
        let mut pinned: BTreeSet<BodyUid> = BTreeSet::new();
        for user in &users {
            let Some(used) = self.state.entry(*user) else {
                continue;
            };
            match &used.def {
                FeatureDef::Combine(combine) if used.component == owner => {
                    pinned.insert(combine.target);
                    pinned.extend(combine.tool_links.keys().copied());
                    for tool in &combine.tools {
                        if tool.link.is_some() {
                            pinned.insert(tool.body);
                        }
                    }
                    combines.push(*user);
                }
                def => {
                    let params = &self.state.parameters;
                    let text = def
                        .map_params(&mut |_, id| Ok::<_, ()>(params.name(*id)))
                        .ok()
                        .and_then(|named| serde_json::to_string(&named).ok())
                        .unwrap_or_default();
                    pinned.extend(named_bodies(&text, feature, &uids, &faces_of));
                }
            }
        }
        // A body that a print plate item or a comment names stays as it is
        // (mitcad#1312): as a copy it would be gone, and as the first body
        // it would print, or be found, where every occurrence shows it.
        for plate in &self.state.print_plates {
            for item in &plate.items {
                if let crate::print_plates::PlateTarget::Body(uid) = item.target {
                    pinned.insert(uid);
                }
            }
        }
        pinned.extend(self.state.comments.iter().filter_map(|c| c.target.body));
        // The bodies that can be copies: solids and sheets of the feature's
        // component without face-level looks, which a copy could not keep,
        // and that nothing pins.
        let mut candidates = Vec::new();
        let mut shapes = Vec::new();
        for (index, (body, uid)) in base.bodies.iter().zip(&uids).enumerate() {
            if body.mesh || pinned.contains(uid) {
                continue;
            }
            let Some(shape) = made.get(uid).copied() else {
                continue;
            };
            let attributes = self.body_attributes(*uid);
            if !attributes.face_appearances.is_empty()
                || !attributes.face_appearance_exprs.is_empty()
                || attributes.print_bed_face.is_some()
            {
                continue;
            }
            let properties = self
                .state
                .properties
                .sets(crate::properties::PropertyOwner::Body(*uid))
                .to_vec();
            candidates.push(Candidate {
                index,
                uid: *uid,
                look: (
                    body.color.map(|c| c.map(f64::to_bits)),
                    attributes,
                    properties,
                ),
            });
            shapes.push(shape);
        }
        let groups = if candidates.len() < 2 {
            Vec::new()
        } else {
            let copies = self
                .kernel()
                .rigid_copies(&shapes, tolerance)
                .map_err(|e| invalid(format!("Find Repeated Parts: {e}")))?;
            if copies.len() != candidates.len() {
                return Err(invalid(
                    "Find Repeated Parts: the kernel answered for another number of bodies",
                ));
            }
            group(&candidates, &copies)
        };
        let unmatched = |uid: &BodyUid| {
            !groups.iter().any(|g| {
                candidates[g.first].uid == *uid
                    || g.copies.iter().any(|(k, _)| candidates[*k].uid == *uid)
            })
        };
        let kept: Vec<BodyUid> = uids.iter().copied().filter(unmatched).collect();
        if groups.is_empty() {
            return Ok(RepeatedParts {
                parts: Vec::new(),
                kept,
                combines: Vec::new(),
            });
        }
        // The bodies that stay, with the numbers and first faces they had.
        let taken: BTreeSet<usize> = groups
            .iter()
            .flat_map(|g| g.copies.iter().map(|(k, _)| candidates[*k].index))
            .collect();
        let mut bodies: Vec<BaseBody> = Vec::with_capacity(base.bodies.len() - taken.len());
        let mut expected_face = 0u32;
        for (i, body) in base.bodies.iter().enumerate() {
            if taken.contains(&i) {
                continue;
            }
            let mut body = body.clone();
            let number = u32::try_from(i).map_err(|_| invalid("too many bodies"))?;
            let place = u32::try_from(bodies.len()).map_err(|_| invalid("too many bodies"))?;
            let number = body.number.unwrap_or(number);
            if number != place {
                body.number = Some(number);
            }
            let (first, count) = faces_of[i];
            if first != expected_face {
                body.first_face = Some(first);
            }
            expected_face = first + count;
            bodies.push(body);
        }
        let names: Vec<String> = groups
            .iter()
            .map(|g| self.body_name(candidates[g.first].uid))
            .collect();
        let removed: Vec<BodyUid> = taken.iter().map(|i| uids[*i]).collect();
        let found = self.apply(|state| {
            let mut changed = (*state.features[position]).clone();
            if let FeatureDef::Base(def) = &mut changed.def {
                def.bodies = bodies;
            }
            state.features[position] = Arc::new(changed);
            for uid in &removed {
                state.body_names.remove(uid);
                state.body_attributes.remove(uid);
                state.flat_patterns.remove(uid);
                // The same as the first body's, which the occurrence shows.
                state
                    .properties
                    .owners
                    .remove(&crate::properties::PropertyOwner::Body(*uid));
            }
            // The components' features go right after the base feature, so
            // that the features after it find the bodies in them.
            let marker = state.marker;
            state.marker = position + 1;
            let mut parts = Vec::with_capacity(groups.len());
            for (group, name) in groups.iter().zip(&names) {
                let body = candidates[group.first].uid;
                let def = FeatureDef::<ValueInput>::ComponentFromBodies(
                    ComponentFromBodiesDef::new(vec![body]),
                );
                let added = state.add_feature_in(&def, None, owner)?;
                let component = state
                    .assembly
                    .created_by(added.uid)
                    .ok_or_else(|| invalid("Find Repeated Parts: the component was not made"))?;
                let free = state.free_component_name(name);
                if let Some(c) = state.assembly.component_mut(component) {
                    c.name = free.clone();
                }
                let mut occurrences: Vec<OccurrenceUid> = state
                    .assembly
                    .occurrences_of(component)
                    .map(|o| o.uid)
                    .collect();
                let mut copies = Vec::with_capacity(group.copies.len());
                for (k, placement) in &group.copies {
                    occurrences.push(state.assembly.place(component, owner, *placement));
                    copies.push(candidates[*k].uid);
                }
                parts.push(RepeatedPart {
                    component,
                    name: free,
                    feature: added.uid,
                    body,
                    occurrences,
                    copies,
                });
            }
            state.marker = marker + parts.len();
            // A combine's tools among the bodies: the first body where each
            // occurrence shows it (mitcad#544's tools in occurrences).
            let mut seen: BTreeMap<BodyUid, (BodyUid, OccurrenceUid)> = BTreeMap::new();
            for part in &parts {
                for (body, occurrence) in std::iter::once(&part.body)
                    .chain(&part.copies)
                    .zip(&part.occurrences)
                {
                    seen.insert(*body, (part.body, *occurrence));
                }
            }
            let context =
                crate::links::first_path(&state.assembly, owner, None).map_err(invalid)?;
            let mut remapped = Vec::new();
            for uid in &combines {
                let at = state.require(*uid)?;
                let mut changed = (*state.features[at]).clone();
                let FeatureDef::Combine(combine) = &mut changed.def else {
                    continue;
                };
                let mut any = false;
                for tool in &mut combine.tools {
                    if tool.link.is_some() {
                        continue;
                    }
                    if let Some((body, occurrence)) = seen.get(&tool.body) {
                        let mut source = context.clone();
                        source.push(*occurrence);
                        *tool = CombineTool::linked(
                            *body,
                            OccurrenceLink {
                                source: OccurrencePath(source),
                                target: OccurrencePath(context.clone()),
                            },
                        );
                        any = true;
                    }
                }
                if any {
                    state.features[at] = Arc::new(changed);
                    remapped.push(*uid);
                }
            }
            let found = RepeatedParts {
                parts,
                kept,
                combines: remapped,
            };
            Ok((format!("Find Repeated Parts in {}", entry.name), found))
        })?;
        Ok(found)
    }
}

/// The bodies of `feature` (`uids`, with their first faces and face
/// counts) that a definition's text names: by id (`F1.b3`) or by a face of
/// theirs (`F1:import(12)`, also in edge and vertex names).
fn named_bodies(
    text: &str,
    feature: FeatureUid,
    uids: &[BodyUid],
    faces: &[(u32, u32)],
) -> Vec<BodyUid> {
    let number = |s: &str| -> Option<u32> {
        let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    };
    let prefix = feature.to_string();
    let bytes = text.as_bytes();
    let mut named = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find(&prefix) {
        let start = from + at;
        from = start + prefix.len();
        if start > 0 && bytes[start - 1].is_ascii_alphanumeric() {
            continue;
        }
        let rest = &text[from..];
        if let Some(n) = rest.strip_prefix(".b").and_then(number) {
            named.extend(uids.iter().filter(|u| u.index == n));
        } else if let Some(j) = rest.strip_prefix(":import(").and_then(number) {
            let body = faces
                .iter()
                .position(|(first, count)| j >= *first && j - *first < *count);
            named.extend(body.map(|k| uids[k]));
        }
    }
    named
}

/// The components the kernel's copies make: each kind of body split by
/// look, the first body of each look the component's, the others placed
/// from it (`copies[k]` names the first body of body k's kind and the
/// placement from it onto body k).
fn group(candidates: &[Candidate], copies: &[crate::kernel::RigidCopy]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (k, copy) in copies.iter().enumerate() {
        // A copy of a body after it, or a placement that is not rigid, is
        // a body of its own.
        let (root, from_root) = if copy.of < k && is_rigid(&copy.transform) {
            (copy.of, copy.transform)
        } else {
            (k, Transform::IDENTITY)
        };
        let same = groups
            .iter_mut()
            .find(|g| g.root == root && candidates[g.first].look == candidates[k].look);
        match same {
            Some(g) => {
                let placement = from_root.after(&inverse(&g.from_root));
                g.copies.push((k, placement));
            }
            None => groups.push(Group {
                first: k,
                root,
                from_root,
                copies: Vec::new(),
            }),
        }
    }
    groups.retain(|g| !g.copies.is_empty());
    groups
}
