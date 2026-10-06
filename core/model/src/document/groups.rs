// SPDX-License-Identifier: MIT
//! Timeline groups (P9): runs of features shown as one folder in the
//! timeline. A group is its members in timeline order; it
//! stays a run: features that are deleted leave it, a feature added or
//! moved between its members joins it, a member moved away leaves it, and
//! a group without members goes. Grouping changes no geometry: each
//! change is an undo step that recomputes nothing.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{DocState, Document, ModelError, invalid};
use crate::ids::FeatureUid;
use crate::kernel::Kernel;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineGroup {
    pub name: String,
    /// In timeline order, one run.
    pub features: Vec<FeatureUid>,
}

impl DocState {
    /// The index of the group a feature is in.
    pub(crate) fn group_of(&self, uid: FeatureUid) -> Option<usize> {
        self.groups.iter().position(|g| g.features.contains(&uid))
    }

    /// Keeps every group one run of the features there are: members that
    /// are gone leave, features between the first and the last member
    /// join, a feature is in one group only, and empty groups go.
    pub(crate) fn normalize_groups(&mut self) {
        if self.groups.is_empty() {
            return;
        }
        let positions: HashMap<FeatureUid, usize> = self
            .features
            .iter()
            .enumerate()
            .map(|(i, f)| (f.uid, i))
            .collect();
        let mut claimed = HashSet::new();
        let mut groups = std::mem::take(&mut self.groups);
        for group in &mut groups {
            let at: Vec<usize> = group
                .features
                .iter()
                .filter_map(|uid| positions.get(uid).copied())
                .collect();
            group.features = match (at.iter().min(), at.iter().max()) {
                (Some(&first), Some(&last)) => self.features[first..=last]
                    .iter()
                    .map(|f| f.uid)
                    .filter(|uid| claimed.insert(*uid))
                    .collect(),
                _ => Vec::new(),
            };
        }
        groups.retain(|g| !g.features.is_empty());
        self.groups = groups;
    }

    /// A feature moved by `moved`: a member leaves its group, unless it is
    /// the group's only one (the group moves with it); between members it
    /// joins again (`normalize_groups`).
    pub(crate) fn leave_group_for_move(&mut self, uid: FeatureUid) {
        if let Some(index) = self.group_of(uid)
            && self.groups[index].features.len() > 1
        {
            self.groups[index].features.retain(|f| *f != uid);
        }
    }
}

impl<K: Kernel> Document<K> {
    pub fn timeline_groups(&self) -> &[TimelineGroup] {
        &self.state.groups
    }

    /// Groups a run of features (in any order) as `name`, or the next
    /// free `Group<n>`; returns the name.
    pub fn group_features(
        &mut self,
        features: &[FeatureUid],
        name: Option<&str>,
    ) -> Result<String, ModelError> {
        if features.is_empty() {
            return Err(invalid("group at least one feature"));
        }
        let mut positions = Vec::with_capacity(features.len());
        for uid in features {
            let position = self.state.require(*uid)?;
            if positions.contains(&position) {
                return Err(invalid(format!("{uid} is listed twice")));
            }
            if let Some(group) = self.state.group_of(*uid) {
                return Err(invalid(format!(
                    "{} is in {} already",
                    self.state.features[position].name, self.state.groups[group].name
                )));
            }
            positions.push(position);
        }
        positions.sort_unstable();
        if positions.windows(2).any(|w| w[1] != w[0] + 1) {
            return Err(invalid(
                "a group is a run of features next to each other in the timeline",
            ));
        }
        let taken = |n: &str| self.state.groups.iter().any(|g| g.name == n);
        let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) if taken(name) => {
                return Err(invalid(format!("a group named {name} exists")));
            }
            Some(name) => name.to_owned(),
            None => (1..)
                .map(|n| format!("Group{n}"))
                .find(|n| !taken(n))
                .expect("a free name"),
        };
        let members: Vec<FeatureUid> = positions
            .iter()
            .map(|p| self.state.features[*p].uid)
            .collect();
        let label = format!("Create Group {name}");
        let made = name.clone();
        self.apply_with(|state| {
            state.groups.push(TimelineGroup {
                name: made,
                features: members,
            });
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    /// Takes a group away; its features stay.
    pub fn ungroup(&mut self, name: &str) -> Result<(), ModelError> {
        let index = self.group_index(name)?;
        self.apply_with(|state| {
            state.groups.remove(index);
            Ok((format!("Ungroup {name}"), (), false))
        })
    }

    pub fn rename_group(&mut self, name: &str, new_name: &str) -> Result<(), ModelError> {
        let index = self.group_index(name)?;
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(invalid("the group needs a name"));
        }
        if new_name == name {
            return Ok(());
        }
        if self.state.groups.iter().any(|g| g.name == new_name) {
            return Err(invalid(format!("a group named {new_name} exists")));
        }
        let renamed = new_name.to_owned();
        self.apply_with(|state| {
            state.groups[index].name = renamed;
            Ok((format!("Rename {name} to {new_name}"), (), false))
        })
    }

    fn group_index(&self, name: &str) -> Result<usize, ModelError> {
        self.state
            .groups
            .iter()
            .position(|g| g.name == name)
            .ok_or_else(|| invalid(format!("there is no group {name}")))
    }
}
