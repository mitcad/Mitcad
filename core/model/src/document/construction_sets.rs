// SPDX-License-Identifier: MIT
//! Construction sets (mitcad#2514, GitHub mitcad/Mitcad#656): named groups
//! of a component's Browser that collect the inputs and results of a
//! surfacing session as they are made.
//!
//! A set is organisation only, like a folder of the Bodies folder: its
//! members are a component's sketches (planar and 3D), construction
//! geometry (planes, axes, points, coordinate systems, intersection and
//! helix curves) and bodies, by uid. They keep their component, their place
//! in the timeline and their references; the set changes no geometry,
//! timeline, bill of materials or export, and its commands recompute
//! nothing.
//!
//! At most one set is active, and only in the active component: while it
//! is, every edit that makes a sketch or construction feature in that
//! component, or a surface body (a sheet) by a feature it adds or changes,
//! puts it into the set in the same undo step
//! ([`DocState::gather_into_active_set`], called after every edit), so undo
//! and redo take the item and its membership together. Making another
//! component active leaves the set inactive in that step. A deleted item,
//! or one moved into another component, leaves its set
//! ([`DocState::prune_construction_sets`]); a deleted component takes its
//! sets along.

use serde::{Deserialize, Serialize};

use super::{DocState, Document, ModelError, invalid};
use crate::exchange::BodyKind;
use crate::features::FeatureEntry;
use crate::ids::{BodyUid, ComponentUid, FeatureUid};
use crate::kernel::Kernel;
use crate::recompute::Recomputed;

/// An item of a construction set: `{"feature": "F5"}` (a sketch or
/// construction geometry) or `{"body": "F7.b0"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstructionMember {
    Feature(FeatureUid),
    Body(BodyUid),
}

/// A named construction set of a component: its members in the order they
/// joined, each once and in no other set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructionSet {
    /// Unique among the design's sets (`ConstructionSet1`).
    pub name: String,
    /// The component whose Browser rows it groups; the root's (`C0`) is
    /// left out.
    #[serde(default, skip_serializing_if = "is_root")]
    pub component: ComponentUid,
    /// Whether new items join it; at most one set is.
    #[serde(default, skip_serializing_if = "is_false")]
    pub active: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<ConstructionMember>,
}

fn is_root(component: &ComponentUid) -> bool {
    component.is_root()
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Whether a feature is one a construction set holds: a sketch (planar or
/// 3D) or construction geometry, the features of the Browser's Sketches
/// and Construction folders.
pub fn construction_kind(entry: &FeatureEntry) -> bool {
    let kind = entry.def.type_name();
    matches!(
        kind,
        "sketch" | "spatial_sketch" | "coordinate_system" | "intersection_curve" | "helix_curve"
    ) || kind.starts_with("construction_")
}

/// A set's name as given: trimmed; an empty one is refused.
fn set_name(name: &str) -> Result<String, ModelError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(invalid("the construction set needs a name"));
    }
    Ok(name.to_owned())
}

/// Checks a design file's sets: names given and once each, at most one
/// active.
pub(crate) fn check_construction_sets(sets: &[ConstructionSet]) -> Result<(), String> {
    for (i, set) in sets.iter().enumerate() {
        if set.name.trim().is_empty() {
            return Err(format!("construction_sets[{i}]: the name is empty"));
        }
        if sets[..i].iter().any(|s| s.name == set.name) {
            return Err(format!(
                "construction_sets[{i}]: construction set {} is listed more than once",
                set.name
            ));
        }
    }
    if sets.iter().filter(|s| s.active).count() > 1 {
        return Err("construction_sets: more than one construction set is active".to_owned());
    }
    Ok(())
}

impl DocState {
    /// Whether a member still belongs to a set of `component`: its feature
    /// (a body's, the feature that made it) in the timeline and in that
    /// component.
    fn construction_member_kept(
        &self,
        component: ComponentUid,
        member: &ConstructionMember,
    ) -> bool {
        match member {
            ConstructionMember::Feature(feature) => self
                .entry(*feature)
                .is_some_and(|e| e.component == component && construction_kind(e)),
            ConstructionMember::Body(body) => self
                .entry(body.feature)
                .is_some_and(|e| e.component == component),
        }
    }

    /// The sets lose what is gone (mitcad#2514): the sets of deleted
    /// components, members whose items are deleted or moved into another
    /// component. The active set stays active only while its component is
    /// the active one.
    pub(crate) fn prune_construction_sets(&mut self) {
        if self.construction_sets.is_empty() {
            return;
        }
        let mut sets = std::mem::take(&mut self.construction_sets);
        sets.retain(|s| self.assembly.exists(s.component));
        for set in &mut sets {
            let component = set.component;
            set.members
                .retain(|m| self.construction_member_kept(component, m));
            if set.active && component != self.assembly.active {
                set.active = false;
            }
        }
        self.construction_sets = sets;
    }

    /// New items join the active set, in the edit that makes them
    /// (mitcad#2514): the sketches and construction features of its
    /// component that `before` did not have, and the surface bodies of its
    /// component at the marker in the edit's result `now` (None: it
    /// recomputed nothing) that the previous result `old` did not have,
    /// made by a feature the edit added or changed (a marker moved or an
    /// earlier feature edited brings no body in). An item in a set already
    /// stays where it is.
    pub(crate) fn gather_into_active_set<K: Kernel>(
        &mut self,
        before: &DocState,
        old: &Recomputed<K::Shape>,
        now: Option<&Recomputed<K::Shape>>,
        kernel: &K,
    ) {
        let Some(i) = self.construction_sets.iter().position(|s| s.active) else {
            return;
        };
        let component = self.construction_sets[i].component;
        let mut new: Vec<ConstructionMember> = self
            .features
            .iter()
            .filter(|e| {
                e.component == component && construction_kind(e) && before.entry(e.uid).is_none()
            })
            .map(|e| ConstructionMember::Feature(e.uid))
            .collect();
        if let Some(now) = now {
            for (uid, shape) in now.bodies.iter() {
                if old.bodies.contains_key(uid) || now.owners.get(uid) != Some(&component) {
                    continue;
                }
                let made_now = match (before.entry(uid.feature), self.entry(uid.feature)) {
                    (None, Some(_)) => true,
                    // An entry the edit did not touch is shared with the
                    // state before it.
                    (Some(was), Some(is)) => !std::ptr::eq(was, is),
                    _ => false,
                };
                if made_now && matches!(kernel.body_kind(&shape.value), Ok(BodyKind::Sheet)) {
                    new.push(ConstructionMember::Body(*uid));
                }
            }
        }
        new.retain(|m| !self.construction_sets.iter().any(|s| s.members.contains(m)));
        self.construction_sets[i].members.extend(new);
    }
}

impl<K: Kernel> Document<K> {
    /// The construction sets in the order they were added.
    pub fn construction_sets(&self) -> &[ConstructionSet] {
        &self.state.construction_sets
    }

    /// The active construction set, if one is.
    pub fn active_construction_set(&self) -> Option<&ConstructionSet> {
        self.state.construction_sets.iter().find(|s| s.active)
    }

    fn construction_set_index(&self, name: &str) -> Result<usize, ModelError> {
        let name = name.trim();
        self.state
            .construction_sets
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(|| invalid(format!("there is no construction set {name}")))
    }

    /// Why an item cannot be a member of a set of `component`, if it
    /// cannot: a sketch or construction feature of that component, or a
    /// body of it at the marker.
    fn check_construction_member(
        &self,
        component: ComponentUid,
        member: &ConstructionMember,
    ) -> Result<(), ModelError> {
        let place = self.state.assembly.name(component);
        match member {
            ConstructionMember::Feature(feature) => {
                let entry = self
                    .state
                    .entry(*feature)
                    .ok_or(ModelError::NoSuchFeature(*feature))?;
                if !construction_kind(entry) {
                    return Err(invalid(format!(
                        "{} is not a sketch or construction geometry",
                        entry.name
                    )));
                }
                if entry.component != component {
                    return Err(invalid(format!("{} is not in {place}", entry.name)));
                }
            }
            ConstructionMember::Body(body) => match self.body_component(*body) {
                None => {
                    return Err(invalid(format!(
                        "there is no body {body} at the timeline marker"
                    )));
                }
                Some(owner)
                    if owner != component
                        || !self.state.construction_member_kept(component, member) =>
                {
                    return Err(invalid(format!(
                        "{} is not in {place}",
                        self.body_name(*body)
                    )));
                }
                Some(_) => {}
            },
        }
        Ok(())
    }

    /// Adds a construction set to `component` (the active one when None):
    /// one undo step (`Add Construction Set Guides`) that recomputes
    /// nothing. Without a name it is `ConstructionSet1`,
    /// `ConstructionSet2`, ... (the next free number). With `activate` it
    /// becomes the active set and its component the active component.
    /// Refused: a name another set has, a component that cannot be changed
    /// (linked, or around a design edited in place). Returns the name.
    pub fn add_construction_set(
        &mut self,
        name: Option<&str>,
        component: Option<ComponentUid>,
        activate: bool,
    ) -> Result<String, ModelError> {
        let component = component.unwrap_or(self.state.assembly.active);
        self.state.editable(component)?;
        let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) => name.to_owned(),
            None => (1..)
                .map(|n| format!("ConstructionSet{n}"))
                .find(|n| !self.state.construction_sets.iter().any(|s| &s.name == n))
                .expect("a free name"),
        };
        if self.state.construction_sets.iter().any(|s| s.name == name) {
            return Err(invalid(format!("a construction set {name} exists")));
        }
        let set = ConstructionSet {
            name: name.clone(),
            component,
            active: activate,
            members: Vec::new(),
        };
        let label = format!("Add Construction Set {name}");
        self.apply_with(|state| {
            if activate {
                for other in &mut state.construction_sets {
                    other.active = false;
                }
                state.assembly.active = component;
            }
            state.construction_sets.push(set);
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    /// Makes a set the active one, and its component the active component:
    /// one undo step (`Activate Construction Set Guides`) that recomputes
    /// nothing, none when it is active already. The set active before is
    /// not any more.
    pub fn activate_construction_set(&mut self, name: &str) -> Result<(), ModelError> {
        let i = self.construction_set_index(name)?;
        let set = &self.state.construction_sets[i];
        let component = set.component;
        self.state.editable(component)?;
        if set.active && self.state.assembly.active == component {
            return Ok(());
        }
        let label = format!("Activate Construction Set {}", set.name);
        self.apply_with(|state| {
            for (k, other) in state.construction_sets.iter_mut().enumerate() {
                other.active = k == i;
            }
            state.assembly.active = component;
            Ok((label, (), false))
        })
    }

    /// No set is active any more: new items go to their usual places. One
    /// undo step (`Deactivate Construction Set Guides`) that recomputes
    /// nothing, none when no set is active. Returns the set that was.
    pub fn deactivate_construction_set(&mut self) -> Result<Option<String>, ModelError> {
        let Some(name) = self.active_construction_set().map(|s| s.name.clone()) else {
            return Ok(None);
        };
        let label = format!("Deactivate Construction Set {name}");
        self.apply_with(|state| {
            for set in &mut state.construction_sets {
                set.active = false;
            }
            Ok((label, (), false))
        })?;
        Ok(Some(name))
    }

    /// Adds items to a set and takes others out: one undo step (`Change
    /// Construction Set Guides`) that recomputes nothing, none when nothing
    /// changes. An item added leaves the set it was in. A member removed
    /// need not be in the design any more. Returns the number of members.
    pub fn set_construction_set_members(
        &mut self,
        name: &str,
        add: &[ConstructionMember],
        remove: &[ConstructionMember],
    ) -> Result<usize, ModelError> {
        let i = self.construction_set_index(name)?;
        let component = self.state.construction_sets[i].component;
        for member in add {
            self.check_construction_member(component, member)?;
        }
        let current = &self.state.construction_sets[i];
        let mut list = current.members.clone();
        for member in add {
            if !list.contains(member) {
                list.push(*member);
            }
        }
        list.retain(|m| !remove.contains(m));
        let count = list.len();
        let others_hold = self
            .state
            .construction_sets
            .iter()
            .enumerate()
            .any(|(k, s)| k != i && s.members.iter().any(|m| add.contains(m)));
        if list == current.members && !others_hold {
            return Ok(count);
        }
        let label = format!("Change Construction Set {}", current.name);
        self.apply_with(|state| {
            state.construction_sets[i].members = list;
            for (k, set) in state.construction_sets.iter_mut().enumerate() {
                if k != i {
                    set.members.retain(|m| !add.contains(m));
                }
            }
            Ok((label, (), false))
        })?;
        Ok(count)
    }

    /// Renames a set: one undo step (`Rename Construction Set Guides`) that
    /// recomputes nothing, none when the name is the same. Refused: an
    /// empty name, a name another set has.
    pub fn rename_construction_set(
        &mut self,
        name: &str,
        new_name: &str,
    ) -> Result<(), ModelError> {
        let i = self.construction_set_index(name)?;
        let new_name = set_name(new_name)?;
        let old = self.state.construction_sets[i].name.clone();
        if new_name == old {
            return Ok(());
        }
        if self
            .state
            .construction_sets
            .iter()
            .any(|s| s.name == new_name)
        {
            return Err(invalid(format!("a construction set {new_name} exists")));
        }
        let label = format!("Rename Construction Set {old}");
        self.apply_with(|state| {
            state.construction_sets[i].name = new_name;
            Ok((label, (), false))
        })
    }

    /// Deletes a set, never its items: they go back to their usual places
    /// in the Browser. One undo step (`Delete Construction Set Guides`)
    /// that recomputes nothing.
    pub fn delete_construction_set(&mut self, name: &str) -> Result<(), ModelError> {
        let i = self.construction_set_index(name)?;
        let label = format!(
            "Delete Construction Set {}",
            self.state.construction_sets[i].name
        );
        self.apply_with(|state| {
            state.construction_sets.remove(i);
            Ok((label, (), false))
        })
    }
}
