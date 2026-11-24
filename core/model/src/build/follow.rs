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
//! its travel limits ([`DocState::lead_followed_edits`]). An offset that
//! is an expression keeps it (mitcad#2317): a plain user parameter that
//! only it uses takes the new value, any other expression (a formula, a
//! parameter used elsewhere) refuses the move in words.

use std::collections::{BTreeMap, BTreeSet};

use super::held::slides_on_mate;
use crate::document::{DocState, ModelError};
use crate::expr::ParamKind;
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

/// Whether a carriage's stored offset, the parameter `id` of its joint
/// `joint`'s position, can take where the carriage moves to (mitcad#2317):
/// the joint's own parameter when it is a number, or a user parameter the
/// position names when it is a number and nothing else uses it (its value
/// follows, the joint keeps naming it). Otherwise, in words, why not: an
/// offset by formula is calculated from its expression, and a parameter
/// that something else uses would move that too.
fn writable_offset(state: &DocState, joint: FeatureUid, id: ParamId) -> Result<(), String> {
    let params = &state.parameters;
    let Some(param) = params.get(id) else {
        return Ok(());
    };
    if params.owner(id) == Some(joint) {
        if param.expr().is_plain_number() || build_form(state, id) {
            return Ok(());
        }
        return Err(format!(
            "its offset is by formula ({}), calculated from its expression",
            param.expression()
        ));
    }
    let name = param.name();
    if !param.expr().is_plain_number() {
        return Err(format!(
            "its offset is the parameter {name}, which is by formula ({})",
            param.expression()
        ));
    }
    if param.kind() != ParamKind::User || params.owner(id).is_some() || params.is_reference(id) {
        return Err(format!(
            "its offset is the parameter {name}, which is not a user parameter"
        ));
    }
    let Some(entry) = state.entry(joint) else {
        return Ok(());
    };
    // Its joint uses it once (for this offset), and nothing else does.
    let mut users = state.parameter_users(name);
    if let Some(i) = users.iter().position(|u| *u == entry.name) {
        users.remove(i);
    }
    let slots = entry.def.params().iter().filter(|p| **p == id).count();
    if !users.is_empty() {
        return Err(format!(
            "its offset is the parameter {name}, which is used by {} as well",
            and_list(&users)
        ));
    }
    if slots > 1 {
        return Err(format!(
            "its offset is the parameter {name}, which {} also uses for another value",
            entry.name
        ));
    }
    Ok(())
}

/// Whether the expression of a joint's own parameter `id` is BUILD's form
/// of an Offset typed as a number (from End B `d12 - 140`, from the
/// middle `d12 / 2 + 15`): it names only parameters of BUILD parts (the
/// target's length), none of the user's.
fn build_form(state: &DocState, id: ParamId) -> bool {
    let params = &state.parameters;
    let Some(param) = params.get(id) else {
        return false;
    };
    let names = param.expr().references();
    param.expr().table_references().is_empty()
        && !names.is_empty()
        && names.iter().all(|name| {
            params
                .find(name)
                .and_then(|p| params.owner(p))
                .and_then(|owner| state.entry(owner))
                .is_some_and(|e| matches!(e.def, FeatureDef::DerivedPart(_)))
        })
}

/// BUILD's form of an Offset ([`build_form`]) moved by `delta`: its
/// trailing constant (`- 140` of `d12 - 140`) changed, the rest kept.
fn rebased(text: &str, delta: f64) -> String {
    let text = text.trim();
    let cut = [text.rfind(" + "), text.rfind(" - ")]
        .into_iter()
        .flatten()
        .max();
    let (base, k) = match cut {
        Some(i) => match text[i + 3..].trim().parse::<f64>() {
            Ok(n) if text[i..].starts_with(" + ") => (&text[..i], n),
            Ok(n) => (&text[..i], -n),
            Err(_) => (text, 0.0),
        },
        None => (text, 0.0),
    };
    let k = k + delta;
    let number = |v: f64| {
        let s = format!("{v:.9}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    };
    if k.abs() < 1e-9 {
        base.to_owned()
    } else if k > 0.0 {
        format!("{base} + {}", number(k))
    } else {
        format!("{base} - {}", number(-k))
    }
}

/// `a`, `a and b`, `a, b and c`.
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The refusal of a move that would take carriage `who` with `with` when
/// its offset cannot follow (`reason`, [`writable_offset`]).
fn cannot_follow(
    state: &DocState,
    who: OccurrenceUid,
    with: OccurrenceUid,
    reason: &str,
) -> ModelError {
    ModelError::Invalid(format!(
        "{} cannot move with {}: {reason}; make its offset a number, or a user parameter only \
         it uses",
        state.assembly.occurrence_name(who),
        state.assembly.occurrence_name(with)
    ))
}

impl DocState {
    /// Writes a carriage's new offset `v` into the parameter `id` of its
    /// joint `joint` ([`writable_offset`]); refused in words (the carriage
    /// `who` moving with `with`) when that cannot be, or when the
    /// parameter's limits keep it from `v`.
    fn write_offset(
        &mut self,
        joint: FeatureUid,
        id: ParamId,
        v: f64,
        who: OccurrenceUid,
        with: OccurrenceUid,
    ) -> Result<(), ModelError> {
        writable_offset(self, joint, id)
            .map_err(|reason| cannot_follow(self, who, with, &reason))?;
        // BUILD's form of an Offset from End B or the middle keeps its form
        // (it follows the target's length); a number where that does not
        // give the value (another length unit).
        let was = self.parameters.value(id);
        let kept = match (self.parameters.get(id), was) {
            (Some(p), Some(was)) if !p.expr().is_plain_number() && build_form(self, id) => {
                let text = rebased(p.expression(), v - was);
                self.parameters.set_expression(id, &text).is_ok()
                    && self
                        .parameters
                        .value(id)
                        .is_some_and(|now| (now - v).abs() < 1e-6)
            }
            _ => false,
        };
        if !kept {
            self.parameters
                .set_value(id, v)
                .map_err(|e| ModelError::Invalid(e.to_string()))?;
        }
        if self.parameters.value(id).is_some_and(|now| changed(v, now)) {
            let reason = format!(
                "the limits of its offset's parameter {} keep it from there",
                self.parameters.name(id)
            );
            return Err(cannot_follow(self, who, with, &reason));
        }
        Ok(())
    }
}

impl DocState {
    /// Rewrites the stored offsets of the carriages that follow another
    /// (mitcad#2274) from where the recompute `result` left them (their
    /// joints' values at the marker); true when one changed. Only the
    /// follower's own placement at its joint's point of the timeline
    /// depends on it: the Rigid joint or Rigid Group moves it on from there.
    ///
    /// An offset that is an expression (mitcad#2317) keeps it: a user
    /// parameter it names takes the value where nothing else uses it
    /// ([`writable_offset`]); otherwise the command that moved the follower
    /// (from where it was in `before`, the joints of the recompute before
    /// the command) is refused in words. One the command did not move stays
    /// as it is.
    pub(crate) fn take_followed_offsets<S>(
        &mut self,
        result: &Recomputed<S>,
        before: &JointsState,
    ) -> Result<bool, ModelError> {
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
                if !changed(*v, was) {
                    continue;
                }
                if let Err(reason) = writable_offset(self, f.joint, id) {
                    let moved = before
                        .joints
                        .get(&f.joint)
                        .and_then(|r| r.values.as_ref())
                        .and_then(|b| b.get(&motion))
                        .is_none_or(|b| changed(*v, *b));
                    if moved {
                        return Err(cannot_follow(self, f.follower, f.leader, &reason));
                    }
                    continue;
                }
                self.write_offset(f.joint, id, *v, f.follower, f.leader)?;
                any = true;
            }
        }
        Ok(any)
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
            let mut writes: BTreeMap<ParamId, (f64, FeatureUid, OccurrenceUid)> = BTreeMap::new();
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
                        writes.insert(id, (v, *j, *o));
                    }
                }
            }
            // An offset that is an expression keeps it, or the edit is
            // refused in words (mitcad#2317).
            for (id, (v, j, o)) in writes {
                self.write_offset(j, id, v, o, moved)?;
            }
        }
        Ok(())
    }
}
