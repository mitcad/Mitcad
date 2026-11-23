// SPDX-License-Identifier: MIT
//! Carriages that move together (mitcad#2274; `commands.md`, "BUILD:
//! carriages that move together"). Two carriages BUILD placed (on one rail
//! or on two) and joined by a Rigid joint, or by a Rigid Group, move as one:
//! the carriage of side `b` (of a Rigid Group, every member after the
//! first) **follows** the other. From the Rigid joint's or Rigid Group's
//! point of the timeline its slide is no longer driven by its own stored
//! offset (`joints::apply` releases it), so the new joint moves it; the
//! stored offset (the joint's `position`, BUILD's Placement Offset) is
//! rewritten from where the follower is after each command
//! ([`DocState::take_followed_offsets`]). An edit of one carriage's offset
//! (Placement, a drag, `drive_joint`) moves the others with it, each within
//! its travel limits ([`DocState::lead_followed_edits`]).

use std::collections::{BTreeMap, BTreeSet};

use super::held::slides_on_mate;
use crate::document::{DocState, ModelError};
use crate::features::FeatureDef;
use crate::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use crate::joints::{JointKind, JointsState, Motion};
use crate::parameters::ParamId;
use crate::recompute::Recomputed;

/// A carriage that follows another.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Follow {
    /// The Rigid joint or Rigid Group that joins them.
    pub by: FeatureUid,
    /// Its component, the carriages' parent.
    pub component: ComponentUid,
    /// The follower and the joint BUILD placed it with (its slide).
    pub follower: OccurrenceUid,
    pub joint: FeatureUid,
    /// The carriage it follows and that one's joint.
    pub leader: OccurrenceUid,
    pub lead_joint: FeatureUid,
}

/// The joint a top-level occurrence of `component` slides on as a BUILD
/// carriage (`slides_on_mate`), when that joint is one of `component`'s
/// and comes before `before` in the timeline.
fn carriage_joint(
    state: &DocState,
    occurrence: OccurrenceUid,
    component: ComponentUid,
    before: FeatureUid,
) -> Option<FeatureUid> {
    let placed = state.assembly.occurrence(occurrence)?;
    if placed.parent != component || !slides_on_mate(state, placed.component) {
        return None;
    }
    let joint = state
        .assembly
        .component(placed.component)?
        .build
        .as_ref()?
        .joint?;
    let entry = state.entry(joint)?;
    if entry.component != component || entry.suppressed {
        return None;
    }
    (state.position(joint)? < state.position(before)?).then_some(joint)
}

/// The carriages that follow another, in timeline order of the Rigid
/// joints and Rigid Groups that join them (suppressed ones join nothing).
/// A carriage follows one other (the first that joins it); a join that
/// would make carriages follow each other round a loop is left out, so
/// one carriage of each set keeps its own offset.
pub(crate) fn follows(state: &DocState) -> Vec<Follow> {
    let mut out: Vec<Follow> = Vec::new();
    for entry in state.features.iter().filter(|e| !e.suppressed) {
        for (leader, follower) in pairs(&entry.def) {
            if leader == follower || out.iter().any(|f| f.follower == follower) {
                continue;
            }
            let Some(lead_joint) = carriage_joint(state, leader, entry.component, entry.uid) else {
                continue;
            };
            let Some(joint) = carriage_joint(state, follower, entry.component, entry.uid) else {
                continue;
            };
            // Not round a loop: the leader does not follow the follower.
            let mut at = leader;
            let mut looped = false;
            while let Some(f) = out.iter().find(|f| f.follower == at) {
                at = f.leader;
                if at == follower {
                    looped = true;
                    break;
                }
            }
            if looped {
                continue;
            }
            out.push(Follow {
                by: entry.uid,
                component: entry.component,
                follower,
                joint,
                leader,
                lead_joint,
            });
        }
    }
    out
}

/// The (leader, follower) pairs a Rigid joint (a, b) or a Rigid Group
/// (the first member and each other one) would make, carriages or not.
fn pairs(def: &FeatureDef) -> Vec<(OccurrenceUid, OccurrenceUid)> {
    match def {
        FeatureDef::Joint(j) if j.kind == JointKind::Rigid => {
            match (j.a.occurrence.0.first(), j.b.occurrence.0.first()) {
                (Some(a), Some(b)) => vec![(*a, *b)],
                _ => Vec::new(),
            }
        }
        FeatureDef::RigidGroup(g) => match g.occurrences.split_first() {
            Some((first, rest)) => rest.iter().map(|o| (*first, *o)).collect(),
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// The carriages the Rigid joint or Rigid Group `by` makes follow another
/// ([`follows`]); looks no further when it joins no carriage.
pub(crate) fn follows_by(state: &DocState, by: FeatureUid) -> Vec<Follow> {
    let Some(entry) = state.entry(by) else {
        return Vec::new();
    };
    let any = pairs(&entry.def)
        .iter()
        .any(|(_, follower)| carriage_joint(state, *follower, entry.component, by).is_some());
    if !any {
        return Vec::new();
    }
    follows(state).into_iter().filter(|f| f.by == by).collect()
}

/// The follows whose Rigid joint or Rigid Group is in effect in `joints`
/// (applied before the marker; a joint that failed is not).
pub(crate) fn in_effect(state: &DocState, joints: &JointsState) -> Vec<Follow> {
    follows(state)
        .into_iter()
        .filter(|f| {
            joints.joints.contains_key(&f.by) || joints.groups.iter().any(|(g, _, _)| *g == f.by)
        })
        .collect()
}

/// The joints whose slides follow another carriage in `joints`
/// ([`in_effect`]): a drag leaves their stored offsets to
/// [`DocState::take_followed_offsets`].
pub(crate) fn released_joints(state: &DocState, joints: &JointsState) -> BTreeSet<FeatureUid> {
    in_effect(state, joints)
        .into_iter()
        .map(|f| f.joint)
        .collect()
}

/// The sets of carriages that move together: each carriage with its
/// joint, the leader of the set first.
fn coupled_sets(follows: &[Follow]) -> Vec<Vec<(OccurrenceUid, FeatureUid)>> {
    let mut sets: Vec<Vec<(OccurrenceUid, FeatureUid)>> = Vec::new();
    for f in follows {
        let lead = sets
            .iter()
            .position(|s| s.iter().any(|(o, _)| *o == f.leader));
        let follow = sets
            .iter()
            .position(|s| s.iter().any(|(o, _)| *o == f.follower));
        match (lead, follow) {
            (Some(l), Some(r)) if l != r => {
                let moved = sets.remove(r);
                let l = if r < l { l - 1 } else { l };
                sets[l].extend(moved);
            }
            (Some(_), Some(_)) => {}
            (Some(l), None) => sets[l].push((f.follower, f.joint)),
            (None, Some(r)) => sets[r].insert(0, (f.leader, f.lead_joint)),
            (None, None) => sets.push(vec![(f.leader, f.lead_joint), (f.follower, f.joint)]),
        }
    }
    sets
}

/// A joint's driven motions: each motion with its position parameter.
fn positions(state: &DocState, joint: FeatureUid) -> Vec<(Motion, ParamId)> {
    match state.entry(joint).map(|e| &e.def) {
        Some(FeatureDef::Joint(j)) => j.position.iter().map(|(m, id)| (*m, *id)).collect(),
        _ => Vec::new(),
    }
}

fn changed(v: f64, was: f64) -> bool {
    (v - was).abs() > 1e-9 * (1.0 + was.abs())
}

impl DocState {
    /// Rewrites the stored offsets of the carriages that follow another
    /// (mitcad#2274) from where the recompute `result` left them (their
    /// joints' values at the marker); true when one changed. Only the
    /// follower's own placement at its joint's point of the timeline
    /// depends on it: the Rigid joint or Rigid Group moves it on from there.
    pub(crate) fn take_followed_offsets<S>(&mut self, result: &Recomputed<S>) -> bool {
        let mut any = false;
        for f in in_effect(self, &result.joints) {
            let Some(values) = result
                .joints
                .joints
                .get(&f.joint)
                .and_then(|r| r.values.as_ref())
            else {
                continue;
            };
            for (motion, id) in positions(self, f.joint) {
                let (Some(v), Some(was)) = (values.get(&motion), self.parameters.value(id)) else {
                    continue;
                };
                if changed(*v, was) && self.parameters.set_value(id, *v).is_ok() {
                    any = true;
                }
            }
        }
        any
    }

    /// An edit that changed the stored offset of one carriage of a set that
    /// moves together (mitcad#2274; a Placement edit, a drag, `drive_joint`,
    /// a parameter) moves the others with it: in `self` (the state after
    /// the edit) their offsets become where the joints of `before` put
    /// them with that carriage at its new offset (solved from where the
    /// occurrences are in `result`, `before`'s recompute). Refused in words
    /// when one of them would go beyond its travel limits. A set in which
    /// the edit changed more than one offset is taken as it is.
    pub(crate) fn lead_followed_edits<S>(
        &mut self,
        before: &DocState,
        result: &Recomputed<S>,
    ) -> Result<(), ModelError> {
        let follows = in_effect(before, &result.joints);
        if follows.is_empty() {
            return Ok(());
        }
        for set in coupled_sets(&follows) {
            // What the edit changed: each member's driven motions whose
            // values differ, with the new value.
            let mut edited: Vec<(usize, Motion, f64)> = Vec::new();
            let mut gone = false;
            for (k, (_, joint)) in set.iter().enumerate() {
                let now = positions(self, *joint);
                if self.entry(*joint).is_none() {
                    gone = true;
                    break;
                }
                for (motion, id) in positions(before, *joint) {
                    let was = before.parameters.value(id);
                    let new = now
                        .iter()
                        .find(|(m, _)| *m == motion)
                        .and_then(|(_, id)| self.parameters.value(*id));
                    if let (Some(was), Some(new)) = (was, new)
                        && changed(new, was)
                    {
                        edited.push((k, motion, new));
                        break;
                    }
                }
            }
            if gone {
                continue;
            }
            let [(k, motion, value)] = edited.as_slice() else {
                continue;
            };
            let (moved, joint) = set[*k];
            let freed: BTreeSet<FeatureUid> = set
                .iter()
                .filter(|(o, _)| *o != moved)
                .map(|(_, j)| *j)
                .collect();
            let name = |o: OccurrenceUid| before.assembly.occurrence_name(o);
            let others = || {
                let names: Vec<String> = set
                    .iter()
                    .filter(|(o, _)| *o != moved)
                    .map(|(o, _)| name(*o))
                    .collect();
                names.join(", ")
            };
            let values = crate::joints::coupled_values(
                &before.assembly,
                &result.joints,
                &result.placements,
                joint,
                *motion,
                *value,
                &freed,
            )
            .ok_or_else(|| {
                ModelError::Invalid(format!(
                    "{} cannot follow {} there: the joints do not hold",
                    others(),
                    name(moved)
                ))
            })?;
            let mut writes: BTreeMap<ParamId, f64> = BTreeMap::new();
            for (o, j) in set.iter().filter(|(o, _)| *o != moved) {
                let Some(solved) = values.get(j) else {
                    continue;
                };
                let spec = |m: Motion| {
                    result
                        .joints
                        .active
                        .iter()
                        .find(|a| a.uid == *j && a.row.is_none() && a.under.is_empty())
                        .and_then(|a| a.motions.iter().find(|s| s.motion == m).copied())
                };
                for (m, id) in positions(self, *j) {
                    let Some(v) = solved.get(&m).copied() else {
                        continue;
                    };
                    let beyond = spec(m).is_some_and(|s| {
                        s.min.is_some_and(|lo| v < lo - 1e-9 * (1.0 + lo.abs()))
                            || s.max.is_some_and(|hi| v > hi + 1e-9 * (1.0 + hi.abs()))
                    });
                    if beyond {
                        return Err(ModelError::Invalid(format!(
                            "the offset puts {} beyond its travel: it moves with {}",
                            name(*o),
                            name(moved)
                        )));
                    }
                    if self.parameters.value(id).is_none_or(|was| changed(v, was)) {
                        writes.insert(id, v);
                    }
                }
            }
            for (id, v) in writes {
                self.parameters
                    .set_value(id, v)
                    .map_err(|e| ModelError::Invalid(e.to_string()))?;
            }
        }
        Ok(())
    }
}
