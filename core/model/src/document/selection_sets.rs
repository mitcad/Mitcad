// SPDX-License-Identifier: MIT
//! Selection sets (mitcad#2192): named lists of links to bodies,
//! occurrences and features (sketches, construction geometry and joints
//! are features), kept with the design.
//!
//! A set changes no geometry, timeline, bill of materials or export: its
//! commands are undo steps that recompute nothing. Its members are kept by
//! reference, as the selection names them (a body's uid, an occurrence's
//! path from the root, a feature's uid; a body or feature placed by an
//! occurrence with that path too), never by name, so a renamed item keeps
//! its place and shows its new name. A deleted item leaves every set in the
//! undo step that deletes it ([`DocState::prune_selection_sets`], called
//! after every edit): a feature deleted (with it the bodies it made), an
//! occurrence deleted (with it what it places). A moved item takes its
//! members along in the undo step that moves it (mitcad#2219): a feature or
//! body moved into another component ([`DocState::follow_set_members`]), an
//! occurrence reparented ([`DocState::follow_set_paths`]), the references
//! Save Component as Design with Replace renames (`DocState::follow`).

use std::collections::BTreeSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{DocState, Document, FeatureEntry, ModelError, invalid};
use crate::assembly::Assembly;
use crate::features::{FeatureDef, OccurrencePath};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::links::OccurrenceLink;
use crate::recompute::Recomputed;

/// An item a selection set links to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "MemberFile", into = "MemberFile")]
pub enum SetMember {
    /// A body, placed by the occurrence path (empty in the root).
    Body {
        body: BodyUid,
        occurrence: OccurrencePath,
    },
    /// An occurrence (a component placed in the design), by its path from
    /// the root; never empty.
    Occurrence { occurrence: OccurrencePath },
    /// A feature (a sketch, construction geometry, a joint, any feature of
    /// the timeline), placed by the occurrence path (empty in the root).
    Feature {
        feature: FeatureUid,
        occurrence: OccurrencePath,
    },
}

/// A member as the design file and the commands write it:
/// `{"body": "F2.b0"}`, `{"feature": "F5", "occurrence": "O1"}`,
/// `{"occurrence": "O1/O4"}`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemberFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body: Option<BodyUid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    feature: Option<FeatureUid>,
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    occurrence: OccurrencePath,
}

impl TryFrom<MemberFile> for SetMember {
    type Error = String;

    fn try_from(file: MemberFile) -> Result<Self, String> {
        match (file.body, file.feature) {
            (Some(body), None) => Ok(Self::Body {
                body,
                occurrence: file.occurrence,
            }),
            (None, Some(feature)) => Ok(Self::Feature {
                feature,
                occurrence: file.occurrence,
            }),
            (None, None) if !file.occurrence.is_empty() => Ok(Self::Occurrence {
                occurrence: file.occurrence,
            }),
            (None, None) => Err("a member needs a body, a feature or an occurrence".to_owned()),
            (Some(_), Some(_)) => Err("a member is a body or a feature, not both".to_owned()),
        }
    }
}

impl From<SetMember> for MemberFile {
    fn from(member: SetMember) -> Self {
        match member {
            SetMember::Body { body, occurrence } => Self {
                body: Some(body),
                feature: None,
                occurrence,
            },
            SetMember::Feature {
                feature,
                occurrence,
            } => Self {
                body: None,
                feature: Some(feature),
                occurrence,
            },
            SetMember::Occurrence { occurrence } => Self {
                body: None,
                feature: None,
                occurrence,
            },
        }
    }
}

impl SetMember {
    /// The occurrence path that places the item (empty in the root).
    pub fn occurrence(&self) -> &OccurrencePath {
        match self {
            Self::Body { occurrence, .. }
            | Self::Occurrence { occurrence }
            | Self::Feature { occurrence, .. } => occurrence,
        }
    }
}

/// A named selection set: its members in the order they were added, each
/// once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionSet {
    pub name: String,
    #[serde(default)]
    pub members: Vec<SetMember>,
}

/// A set's name as given: trimmed; an empty one is refused.
fn set_name(name: &str) -> Result<String, ModelError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(invalid("the selection set needs a name"));
    }
    Ok(name.to_owned())
}

/// Each member once, in the order first given.
fn distinct(members: Vec<SetMember>) -> Vec<SetMember> {
    let mut out: Vec<SetMember> = Vec::with_capacity(members.len());
    for member in members {
        if !out.contains(&member) {
            out.push(member);
        }
    }
    out
}

/// Move a path through a transfer's paired occurrences. Inside another
/// placement of their shared parent, keep that placement's prefix.
fn transfer_path(
    assembly: &Assembly,
    path: &OccurrencePath,
    from: &OccurrencePath,
    to: &OccurrencePath,
) -> Option<OccurrencePath> {
    let component =
        |p: &[OccurrenceUid]| crate::joints::path_component(assembly, ComponentUid::ROOT, p).ok();
    let source = component(&from.0)?;
    if component(&path.0) != Some(source) {
        return None;
    }
    let common = from.0.iter().zip(&to.0).take_while(|(a, b)| a == b).count();
    let from_suffix = &from.0[common..];
    if path.0.ends_with(from_suffix) {
        let prefix = &path.0[..path.0.len() - from_suffix.len()];
        if component(prefix) == component(&from.0[..common]) {
            let mut changed = prefix.to_vec();
            changed.extend_from_slice(&to.0[common..]);
            if component(&changed) == component(&to.0) {
                return Some(OccurrencePath(changed));
            }
        }
    }
    Some(to.clone())
}

fn transfer_moves<'a, S>(
    features: &'a [Arc<FeatureEntry>],
    result: &'a Recomputed<S>,
) -> impl DoubleEndedIterator<Item = (&'a [BodyUid], &'a OccurrenceLink, bool)> {
    features
        .iter()
        .zip(&result.results)
        .filter_map(|(entry, result)| {
            if let FeatureDef::BodyTransfer(transfer) = &entry.def {
                Some((
                    transfer.bodies.as_slice(),
                    &transfer.link,
                    result.status.is_ok(),
                ))
            } else {
                None
            }
        })
}

impl DocState {
    /// Whether a member's item is still in the design: its feature (a
    /// body's, the feature that made it) in the timeline and its occurrence
    /// path placed.
    fn member_kept(&self, member: &SetMember) -> bool {
        let placed = crate::joints::path_component(
            &self.assembly,
            ComponentUid::ROOT,
            &member.occurrence().0,
        )
        .is_ok();
        placed
            && match member {
                SetMember::Body { body, .. } => self.entry(body.feature).is_some(),
                SetMember::Feature { feature, .. } => self.entry(*feature).is_some(),
                SetMember::Occurrence { occurrence } => !occurrence.is_empty(),
            }
    }

    /// The members of moved items follow them (mitcad#2219), in the undo
    /// step of the move: a feature of `features`, a body of `bodies` or made
    /// by one of `features`, placed by a path `to` maps, is placed by the
    /// path it gives. Move Features into a Component, Move History into
    /// Component and Move to Component call it; a member that
    /// becomes one the set has already goes.
    pub(crate) fn follow_set_members(
        &mut self,
        features: &BTreeSet<FeatureUid>,
        bodies: &[BodyUid],
        to: &dyn Fn(&OccurrencePath) -> Option<OccurrencePath>,
    ) {
        for set in &mut self.selection_sets {
            let mut changed = false;
            for member in &mut set.members {
                let (moved, path) = match member {
                    SetMember::Feature {
                        feature,
                        occurrence,
                    } => (features.contains(&*feature), occurrence),
                    SetMember::Body { body, occurrence } => (
                        bodies.contains(&*body) || features.contains(&body.feature),
                        occurrence,
                    ),
                    SetMember::Occurrence { .. } => continue,
                };
                if let Some(new) = moved.then(|| to(path)).flatten() {
                    changed |= *path != new;
                    *path = new;
                }
            }
            if changed {
                set.members = distinct(std::mem::take(&mut set.members));
            }
        }
    }

    /// The members' occurrence paths that lead through `old` lead through
    /// `new` (Reparent Occurrence, mitcad#2219): a member occurrence moved,
    /// or what one places.
    pub(crate) fn follow_set_paths(&mut self, old: &[OccurrenceUid], new: &[OccurrenceUid]) {
        for set in &mut self.selection_sets {
            let mut changed = false;
            for member in &mut set.members {
                let path = match member {
                    SetMember::Body { occurrence, .. }
                    | SetMember::Occurrence { occurrence }
                    | SetMember::Feature { occurrence, .. } => &mut occurrence.0,
                };
                if !old.is_empty() && path.starts_with(old) {
                    let rest = path.split_off(old.len());
                    path.clear();
                    path.extend_from_slice(new);
                    path.extend(rest);
                    changed = true;
                }
            }
            if changed {
                set.members = distinct(std::mem::take(&mut set.members));
            }
        }
    }

    /// A transfer moves the body without changing the feature that made it
    /// (mitcad#2256). When the owner changes, walk back through the former
    /// transfers and forward through the ones that work at the new marker.
    /// The paths live with the edit so Undo restores the selected placement.
    pub(crate) fn follow_set_body_transfers<S>(
        &mut self,
        previous: Option<(&Self, &Recomputed<S>)>,
        result: &Recomputed<S>,
    ) {
        if self.selection_sets.is_empty() {
            return;
        }
        let former: Vec<_> = previous
            .into_iter()
            .flat_map(|(state, result)| transfer_moves(&state.features, result))
            .filter(|(_, _, worked)| *worked)
            .collect();
        let current: Vec<_> = transfer_moves(&self.features, result).collect();
        if former.is_empty() && current.is_empty() {
            return;
        }
        let mut sets = std::mem::take(&mut self.selection_sets);
        for set in &mut sets {
            for member in &mut set.members {
                let SetMember::Body { body, occurrence } = member else {
                    continue;
                };
                let Some(owner) = result.owners.get(body).copied() else {
                    continue;
                };
                let placed_in = |path: &OccurrencePath| {
                    crate::joints::path_component(&self.assembly, ComponentUid::ROOT, &path.0).ok()
                };
                if placed_in(occurrence) == Some(owner) {
                    continue;
                }
                let mut path = occurrence.clone();
                for &(bodies, link, _) in former.iter().rev().chain(current.iter().rev()) {
                    if bodies.contains(body)
                        && let Some(source) =
                            transfer_path(&self.assembly, &path, &link.target, &link.source)
                    {
                        path = source;
                    }
                }
                for &(bodies, link, worked) in &current {
                    if worked
                        && bodies.contains(body)
                        && let Some(target) =
                            transfer_path(&self.assembly, &path, &link.source, &link.target)
                    {
                        path = target;
                    }
                }
                if placed_in(&path) == Some(owner) {
                    *occurrence = path;
                }
            }
            set.members = distinct(std::mem::take(&mut set.members));
        }
        self.selection_sets = sets;
    }

    /// The selection sets lose the members whose items are gone (mitcad#2192):
    /// a deleted item leaves every set in the step that deletes it.
    pub(crate) fn prune_selection_sets(&mut self) {
        if self.selection_sets.is_empty() {
            return;
        }
        let mut sets = std::mem::take(&mut self.selection_sets);
        for set in &mut sets {
            set.members.retain(|m| self.member_kept(m));
        }
        self.selection_sets = sets;
    }
}

impl<K: Kernel> Document<K> {
    /// The selection sets in the order they were added.
    pub fn selection_sets(&self) -> &[SelectionSet] {
        &self.state.selection_sets
    }

    /// Whether a member's item is in the design now (see
    /// [`DocState::prune_selection_sets`]).
    pub fn selection_member_exists(&self, member: &SetMember) -> bool {
        self.state.member_kept(member)
    }

    fn selection_set_index(&self, name: &str) -> Result<usize, ModelError> {
        let name = name.trim();
        self.state
            .selection_sets
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(|| invalid(format!("there is no selection set {name}")))
    }

    /// Why an item cannot be a member, if it cannot: it must be in the
    /// design (a body at the marker) and placed where its path says.
    fn check_set_member(&self, member: &SetMember) -> Result<(), ModelError> {
        let a = &self.state.assembly;
        let path = &member.occurrence().0;
        let end = crate::joints::path_component(a, ComponentUid::ROOT, path).map_err(invalid)?;
        let place = || {
            if path.is_empty() {
                a.root_name.clone()
            } else {
                a.path_name(path)
            }
        };
        match member {
            SetMember::Occurrence { occurrence } => {
                if occurrence.is_empty() {
                    return Err(invalid(
                        "the root component is the whole design, not a member of a selection set",
                    ));
                }
            }
            SetMember::Feature { feature, .. } => {
                let entry = self
                    .state
                    .entry(*feature)
                    .ok_or(ModelError::NoSuchFeature(*feature))?;
                if entry.component != end {
                    return Err(invalid(format!("{} is not in {}", entry.name, place())));
                }
            }
            SetMember::Body { body, .. } => {
                let Some(owner) = self.body_component(*body) else {
                    return Err(invalid(format!(
                        "there is no body {body} at the timeline marker"
                    )));
                };
                if owner != end {
                    return Err(invalid(format!(
                        "{} is not in {}",
                        self.body_name(*body),
                        place()
                    )));
                }
            }
        }
        Ok(())
    }

    /// Adds a selection set: one undo step (`Add Selection Set Favourites`)
    /// that recomputes nothing. Without a name it is `SelectionSet1`,
    /// `SelectionSet2`, ... (the next free number). Refused: a name another
    /// set has, a member not in the design. Returns the name.
    pub fn add_selection_set(
        &mut self,
        name: Option<&str>,
        members: Vec<SetMember>,
    ) -> Result<String, ModelError> {
        let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) => name.to_owned(),
            None => (1..)
                .map(|n| format!("SelectionSet{n}"))
                .find(|n| !self.state.selection_sets.iter().any(|s| &s.name == n))
                .expect("a free name"),
        };
        if self.state.selection_sets.iter().any(|s| s.name == name) {
            return Err(invalid(format!("a selection set {name} exists")));
        }
        for member in &members {
            self.check_set_member(member)?;
        }
        let set = SelectionSet {
            name: name.clone(),
            members: distinct(members),
        };
        let label = format!("Add Selection Set {name}");
        self.apply_with(|state| {
            state.selection_sets.push(set);
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    /// Changes a set's members: `members` replaces them all (when given),
    /// then `add` adds those not in it yet and `remove` takes those out.
    /// One undo step (`Change Selection Set Favourites`) that recomputes
    /// nothing, none when the members stay. A member removed need not be
    /// in the design any more. Returns the number of members.
    pub fn set_selection_set_members(
        &mut self,
        name: &str,
        members: Option<Vec<SetMember>>,
        add: Vec<SetMember>,
        remove: &[SetMember],
    ) -> Result<usize, ModelError> {
        let i = self.selection_set_index(name)?;
        for member in members.iter().flatten().chain(&add) {
            self.check_set_member(member)?;
        }
        let current = &self.state.selection_sets[i];
        let mut list = members.unwrap_or_else(|| current.members.clone());
        list.extend(add);
        list.retain(|m| !remove.contains(m));
        let list = distinct(list);
        let count = list.len();
        if list == current.members {
            return Ok(count);
        }
        let label = format!("Change Selection Set {}", current.name);
        self.apply_with(|state| {
            state.selection_sets[i].members = list;
            Ok((label, (), false))
        })?;
        Ok(count)
    }

    /// Renames a set: one undo step (`Rename Selection Set Favourites`)
    /// that recomputes nothing, none when the name is the same. Refused: an
    /// empty name, a name another set has.
    pub fn rename_selection_set(&mut self, name: &str, new_name: &str) -> Result<(), ModelError> {
        let i = self.selection_set_index(name)?;
        let new_name = set_name(new_name)?;
        let old = self.state.selection_sets[i].name.clone();
        if new_name == old {
            return Ok(());
        }
        if self.state.selection_sets.iter().any(|s| s.name == new_name) {
            return Err(invalid(format!("a selection set {new_name} exists")));
        }
        let label = format!("Rename Selection Set {old}");
        self.apply_with(|state| {
            state.selection_sets[i].name = new_name;
            Ok((label, (), false))
        })
    }

    /// Deletes a set, never its items: one undo step (`Delete Selection Set
    /// Favourites`) that recomputes nothing.
    pub fn delete_selection_set(&mut self, name: &str) -> Result<(), ModelError> {
        let i = self.selection_set_index(name)?;
        let label = format!("Delete Selection Set {}", self.state.selection_sets[i].name);
        self.apply_with(|state| {
            state.selection_sets.remove(i);
            Ok((label, (), false))
        })
    }
}
