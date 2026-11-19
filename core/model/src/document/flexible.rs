// SPDX-License-Identifier: MIT
//! Flexible subassemblies (mitcad#1793): an occurrence of a component with
//! joints of its own made Flexible takes its component's joints into its
//! parent's solve (`crate::joints`, `flex_into_parents`), for this
//! occurrence alone, with its own values of them (`Occurrence::flexible`);
//! the component's design and its other occurrences stay as they are.
//! Since mitcad#2016 any occurrence of a component with components in it
//! can be flexible: the parts its component's joints, rigid groups and
//! grounding do not place are free in the parent, placed there by the
//! parent's joints alone (`crate::joints::flex_unit`), where drags leave
//! them kept per occurrence (`Flex::placements`).

use std::collections::{BTreeMap, BTreeSet};

use super::{DocState, Document, ModelError, invalid};
use crate::assembly::Flex;
use crate::contact::{ContactHit, ContactPairs, sweep};
use crate::features::{FeatureDef, ValueInput};
use crate::ids::{FeatureUid, OccurrenceUid};
use crate::joints::Motion;
use crate::kernel::Kernel;

impl<K: Kernel> Document<K> {
    /// Whether an occurrence is flexible.
    pub fn is_flexible(&self, uid: OccurrenceUid) -> bool {
        self.state
            .assembly
            .occurrence(uid)
            .is_some_and(|o| o.flexible.is_some())
    }

    /// The joints of a component that a flexible occurrence of it takes
    /// into its parent's solve: its joints and as-built joints.
    fn own_joints(&self, uid: OccurrenceUid) -> Vec<FeatureUid> {
        let Some(o) = self.state.assembly.occurrence(uid) else {
            return Vec::new();
        };
        self.state
            .features
            .iter()
            .filter(|f| f.component == o.component)
            .filter(|f| {
                matches!(
                    f.def,
                    FeatureDef::Joint(_)
                        | FeatureDef::AsBuiltJoint(_)
                        | FeatureDef::ConstraintSet(_)
                )
            })
            .map(|f| f.uid)
            .collect()
    }

    /// Makes an occurrence flexible in its parent, or rigid there again
    /// (mitcad#1793): one undo step (`Make Hinge:1 Flexible`, `Make
    /// Hinge:1 Rigid`), none when nothing changes. Rigid again, it drops
    /// its own joint values and kept placements. Any occurrence of a
    /// component with components in it can be flexible, joints of its own
    /// or not (mitcad#2016): its free parts are free in the parent
    /// ([`Flex::free_parts`]). Refused for one whose component has
    /// neither components nor joints of its own.
    pub fn set_occurrence_flexible(
        &mut self,
        uid: OccurrenceUid,
        flexible: bool,
    ) -> Result<(), ModelError> {
        let occurrence = self.state.require_occurrence(uid)?;
        if occurrence.flexible.is_some() == flexible {
            return Ok(());
        }
        let name = self.state.assembly.occurrence_name(uid);
        let has_parts = self
            .state
            .assembly
            .children(occurrence.component)
            .next()
            .is_some();
        if flexible && !has_parts && self.own_joints(uid).is_empty() {
            return Err(invalid(format!(
                "{name} cannot be flexible: {} has no components in it to move",
                self.state.assembly.name(occurrence.component)
            )));
        }
        self.apply(|state| {
            state.assembly.occurrence_mut(uid).expect("exists").flexible = flexible.then(Flex::new);
            let how = if flexible { "Flexible" } else { "Rigid" };
            Ok((format!("Make {name} {how}"), ()))
        })
    }

    /// Drives a joint of a flexible occurrence's component for that
    /// occurrence alone (mitcad#1793): each value (mm, angles; expressions
    /// read at once) is kept as the occurrence's own position of the
    /// motion, None takes it away (the component's position, else free).
    /// A joint of a flexible occurrence nested in it (mitcad#1805) is
    /// driven alike, for this occurrence. The component's design and its
    /// other occurrences do not change. With contact sets (mitcad#454,
    /// mitcad#1805) the joint moves from where it is in the occurrence
    /// toward the values and stops at the first contact, as `drive_joint`
    /// does: the positions are then where it stopped, and the answer the
    /// contact and how far it went (0 to 1). One undo step (`Drive Joint1
    /// in Hinge:1`).
    pub fn drive_flexible_joint(
        &mut self,
        occurrence: OccurrenceUid,
        joint: FeatureUid,
        values: &BTreeMap<Motion, Option<ValueInput>>,
    ) -> Result<Option<(ContactHit, f64)>, ModelError> {
        let o = self.state.require_occurrence(occurrence)?;
        let name = self.state.assembly.occurrence_name(occurrence);
        let entry = self
            .state
            .entry(joint)
            .ok_or_else(|| invalid(format!("feature {joint} does not exist")))?;
        let joint_name = entry.name.clone();
        let (kind, slide) = match &entry.def {
            FeatureDef::Joint(j) => (j.kind, j.slide_axis),
            FeatureDef::AsBuiltJoint(j) => (j.kind, j.slide_axis),
            _ => return Err(invalid(format!("{joint_name} is not a joint"))),
        };
        // Its component's joint, or one of a flexible occurrence nested in
        // it that moves inside it (mitcad#1805).
        if entry.component != o.component && self.flexible_chain(occurrence, joint).is_none() {
            return Err(invalid(format!(
                "{joint_name} is not a joint of {}, which {name} places",
                self.state.assembly.name(o.component)
            )));
        }
        if o.flexible.is_none() {
            return Err(invalid(format!(
                "{name} is not flexible: make it Flexible to drive {joint_name} in it alone"
            )));
        }
        let motions = kind.motions(slide);
        let mut set: BTreeMap<Motion, Option<f64>> = BTreeMap::new();
        for (motion, value) in values {
            if !motions.contains(motion) {
                return Err(invalid(format!("the joint has no free motion {motion}")));
            }
            let value = value
                .as_ref()
                .map(|v| self.motion_value(v, *motion))
                .transpose()?;
            set.insert(*motion, value);
        }
        // Contact sets (mitcad#454): from where the joint is in the
        // occurrence toward the values, stopping where bodies of a set
        // touch, as a drag and `drive_joint` do (mitcad#1805).
        let mut contact = None;
        let to: BTreeMap<Motion, f64> = set.iter().filter_map(|(m, v)| Some((*m, (*v)?))).collect();
        if !to.is_empty() && self.has_contact_pairs() {
            let now = self
                .flexible_joint_values(occurrence)
                .remove(&joint)
                .unwrap_or_default();
            let from: BTreeMap<Motion, f64> = to
                .iter()
                .map(|(m, v)| (*m, now.get(m).copied().unwrap_or(*v)))
                .collect();
            let (fraction, hit) = self.sweep_flexible(occurrence, joint, &from, &to)?;
            if let Some(hit) = hit {
                for (m, v) in &to {
                    let a = from.get(m).copied().unwrap_or(*v);
                    set.insert(*m, Some(a + (v - a) * fraction));
                }
                contact = Some((hit, fraction));
            }
        }
        self.apply(|state| {
            let flex = state
                .assembly
                .occurrence_mut(occurrence)
                .expect("exists")
                .flexible
                .as_mut()
                .expect("flexible");
            for (motion, value) in &set {
                let position = flex.position.entry(joint).or_default();
                match value {
                    Some(v) => {
                        position.insert(*motion, *v);
                    }
                    None => {
                        position.remove(motion);
                    }
                }
                // A free start a drag kept gives way to the position.
                if let Some(start) = flex.start.get_mut(&joint) {
                    start.remove(motion);
                }
            }
            flex.position.retain(|_, m| !m.is_empty());
            flex.start.retain(|_, m| !m.is_empty());
            Ok((format!("Drive {joint_name} in {name}"), ()))
        })?;
        Ok(contact)
    }

    /// How far (0 to 1) `joint` driven in the flexible `occurrence` from
    /// `from` toward `to` goes before bodies of the contact sets touch, and
    /// the contact that stops it (mitcad#1805); the pairs that pass through
    /// each other at the start are left out. Changes nothing.
    fn sweep_flexible(
        &mut self,
        occurrence: OccurrenceUid,
        joint: FeatureUid,
        from: &BTreeMap<Motion, f64>,
        to: &BTreeMap<Motion, f64>,
    ) -> Result<(f64, Option<ContactHit>), ModelError> {
        let base = self.state.clone();
        let pairs = ContactPairs::new(&self.kernel, &base, &self.result);
        if pairs.is_empty() {
            return Ok((1.0, None));
        }
        // The design with the occurrence's own position of the motions
        // at `t` along the way.
        let at = |t: f64| -> DocState {
            let mut state = base.clone();
            if let Some(flex) = state
                .assembly
                .occurrence_mut(occurrence)
                .and_then(|o| o.flexible.as_mut())
            {
                let position = flex.position.entry(joint).or_default();
                for (m, v) in to {
                    let a = from.get(m).copied().unwrap_or(*v);
                    position.insert(*m, a + (v - a) * t);
                }
                if let Some(start) = flex.start.get_mut(&joint) {
                    for m in to.keys() {
                        start.remove(m);
                    }
                }
            }
            state
        };
        let failed = |e: crate::kernel::KernelError| invalid(format!("contact check: {e}"));
        let at_start = self.compute(&at(0.0))?;
        let skip: BTreeSet<usize> = pairs
            .overlapping(
                &self.kernel,
                &at_start.placements,
                &at_start.joints.flexed,
                &BTreeSet::new(),
            )
            .map_err(failed)?
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        let samples = super::motion::samples_for(
            &BTreeMap::from([(joint, from.clone())]),
            &BTreeMap::from([(joint, to.clone())]),
        );
        let stop = sweep(
            samples,
            |t| -> Result<((), Option<ContactHit>), ModelError> {
                let result = self.compute(&at(t))?;
                let deepest = pairs
                    .overlapping(
                        &self.kernel,
                        &result.placements,
                        &result.joints.flexed,
                        &skip,
                    )
                    .map_err(failed)?
                    .into_iter()
                    .max_by(|a, b| a.1.total_cmp(&b.1));
                Ok(((), deepest.map(|(p, v)| pairs.hit(p, v))))
            },
        )?;
        Ok((stop.fraction, stop.hit))
    }

    /// The values of a flexible occurrence's joints at the marker: per
    /// joint of its component, its motions' values for this occurrence,
    /// where they hold (mitcad#1793).
    pub fn flexible_joint_values(
        &self,
        occurrence: OccurrenceUid,
    ) -> BTreeMap<FeatureUid, BTreeMap<Motion, f64>> {
        self.result
            .joints
            .flex_values
            .get(&occurrence)
            .cloned()
            .unwrap_or_default()
    }

    /// The placement of `uid` in its parent after `prefix` in a path: where
    /// the flexible occurrences at its end have it (mitcad#1793; nested
    /// ones, mitcad#1805), else its own.
    pub fn placement_after(
        &self,
        prefix: &[OccurrenceUid],
        uid: OccurrenceUid,
    ) -> Option<crate::transform::Transform> {
        crate::joints::flexed_step(&self.result.joints.flexed, prefix, uid)
            .or_else(|| self.placement(uid))
    }

    /// The chain of flexible occurrences, from `occurrence` on, through
    /// which a joint moves inside it at the marker (mitcad#1805): `[o]`
    /// for a joint of its component, longer for one of a flexible
    /// occurrence nested in it (the first such chain); None when the
    /// joint does not move inside it.
    pub fn flexible_chain(
        &self,
        occurrence: OccurrenceUid,
        joint: FeatureUid,
    ) -> Option<Vec<OccurrenceUid>> {
        self.result
            .joints
            .active
            .iter()
            .filter(|j| j.uid == joint && j.row.is_none())
            .filter(|j| j.under.first() == Some(&occurrence))
            .min_by_key(|j| j.under.len())
            .map(|j| j.under.clone())
    }

    /// Where the free parts of flexible occurrences that `paths` (joints'
    /// paths from their own components) reach stay when those joints go
    /// (mitcad#2017), as [`Document::held_placements`] keeps a placed
    /// occurrence: per free part a path reaches (with what a rigid group of
    /// its component ties to it), where it is now at the marker, to keep as
    /// the outermost flexible occurrence's own placement of it
    /// ([`crate::assembly::Flex::placements`]). A part the component's
    /// joints move keeps their values instead and is left out, as is one
    /// that would be there anyway (its kept placement, else where its
    /// component places it).
    pub fn held_free_parts<'a>(
        &self,
        paths: impl IntoIterator<Item = &'a [OccurrenceUid]>,
    ) -> Vec<HeldFreePart> {
        let assembly = &self.state.assembly;
        let joints = &self.result.joints;
        let mut held: Vec<HeldFreePart> = Vec::new();
        for path in paths {
            // The deepest unit the path moves, as the solve takes it
            // (`crate::joints::unit_of`).
            let Some((within, unit)) = (1..path.len()).rev().find_map(|k| {
                crate::joints::flex_unit(assembly, joints, &path[..k], path[k])
                    .map(|unit| (&path[..k], unit))
            }) else {
                continue;
            };
            let Some(last) = within.last().and_then(|o| assembly.occurrence(*o)) else {
                continue;
            };
            if !last.flexible.as_ref().is_some_and(|f| f.free_parts)
                || crate::joints::joined_in(joints, last.component, &unit.members)
            {
                continue;
            }
            let outer = within[0];
            let flex = assembly.occurrence(outer).and_then(|o| o.flexible.as_ref());
            for part in &unit.members {
                let mut kept = within[1..].to_vec();
                kept.push(*part);
                if !assembly.flex_path_holds(outer, &kept)
                    || held.iter().any(|h| h.flexible == outer && h.path == kept)
                {
                    continue;
                }
                let Some(now) = self.placement_after(within, *part) else {
                    continue;
                };
                let anyway = flex
                    .and_then(|f| f.placement(&kept))
                    .map(|base| self.free_part_after_moves(*part, &base))
                    .or_else(|| self.placement(*part));
                if anyway.is_some_and(|t| super::same_placement(&t, &now)) {
                    continue;
                }
                held.push(HeldFreePart {
                    flexible: outer,
                    path: kept,
                    transform: self.free_part_base(*part, &now),
                });
            }
        }
        held
    }

    /// Where a free part kept at `base` by a flexible occurrence is at the
    /// marker: moved from there by the Move features before the marker
    /// that move `part` (mitcad#2025, `crate::joints::move_free_parts`).
    pub(crate) fn free_part_after_moves(
        &self,
        part: OccurrenceUid,
        base: &crate::transform::Transform,
    ) -> crate::transform::Transform {
        match self.result.joints.free_moves.get(&part) {
            Some(moved) => moved.after(base),
            None => *base,
        }
    }

    /// What a flexible occurrence keeps of a free part that is at `now` at
    /// the marker (mitcad#2025): where it is before the Move features
    /// before the marker that move `part`, so that they move it from there
    /// to `now`, and it follows them when they are edited and goes back
    /// there when they are suppressed or deleted; a rigid placement to
    /// rounding (mitcad#1664).
    pub(crate) fn free_part_base(
        &self,
        part: OccurrenceUid,
        now: &crate::transform::Transform,
    ) -> crate::transform::Transform {
        let base = match self.result.joints.free_moves.get(&part) {
            Some(moved) => crate::assembly::inverse(moved).after(now),
            None => *now,
        };
        crate::assembly::solved_placement(&base).unwrap_or(base)
    }
}

/// Where a free part of a flexible occurrence stays when the joints that
/// placed it go (mitcad#2017, [`Document::held_free_parts`]): `path` from
/// the flexible occurrence `flexible` (through the flexible occurrences
/// nested in it) to the part, and its placement there, as
/// [`crate::assembly::FlexPlacement`] keeps it.
#[derive(Debug, Clone, PartialEq)]
pub struct HeldFreePart {
    pub flexible: OccurrenceUid,
    pub path: Vec<OccurrenceUid>,
    pub transform: crate::transform::Transform,
}

impl DocState {
    /// Keeps the free parts `held` names where it has them, as their
    /// flexible occurrences' own placements (mitcad#2017): the joints that
    /// placed them went. Those whose part or flexible occurrence went too
    /// are left out. Returns how many it kept.
    pub fn hold_free_parts(&mut self, held: &[HeldFreePart]) -> usize {
        let mut kept = 0;
        for h in held {
            if !self.assembly.flex_path_holds(h.flexible, &h.path) {
                continue;
            }
            if let Some(flex) = self
                .assembly
                .occurrence_mut(h.flexible)
                .and_then(|o| o.flexible.as_mut())
            {
                flex.keep_placement(h.path.clone(), h.transform);
                kept += 1;
            }
        }
        kept
    }
}
