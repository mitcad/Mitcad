// SPDX-License-Identifier: MIT
//! The commands of the user parameters' groups (mitcad#2180,
//! `crate::parameter_groups`): a group made, renamed and deleted, and
//! parameters moved into a group or out of their groups. Each is one undo
//! step that recomputes nothing (a group is organization only); nothing
//! changes is no undo step. A group can be made before it holds a
//! parameter; deleting one moves its parameters to its parent group, it
//! never deletes them.

use super::{Document, ModelError, invalid};
use crate::expr::ParamKind;
use crate::kernel::Kernel;
use crate::parameter_groups::{ParameterGroups, group_path};
use crate::parameters::{ParamId, ParameterError};

/// A group path as given, written plainly; an empty one is refused.
fn named_group(text: &str) -> Result<String, ModelError> {
    let path = group_path(text).map_err(invalid)?;
    if path.is_empty() {
        return Err(invalid("the group name is empty"));
    }
    Ok(path)
}

impl<K: Kernel> Document<K> {
    /// The groups of the user parameters.
    pub fn parameter_groups(&self) -> &ParameterGroups {
        &self.state.parameter_groups
    }

    /// A parameter's group by its name; None when it is in none (or there
    /// is no such parameter).
    pub fn parameter_group(&self, name: &str) -> Option<&str> {
        let id = self.state.parameters.find(name)?;
        self.state.parameter_groups.group_of(id)
    }

    /// Makes a parameter group, with the parents it lacks
    /// (`Cabinet/Doors` makes `Cabinet` too): `Add Parameter Group
    /// Cabinet/Doors`. Refused: an empty name, and a group that exists.
    /// Returns its path as written plainly.
    pub fn add_parameter_group(&mut self, group: &str) -> Result<String, ModelError> {
        let path = named_group(group)?;
        if self.state.parameter_groups.contains(&path) {
            return Err(invalid(format!("a parameter group {path} already exists")));
        }
        self.apply_with(|state| {
            state.parameter_groups.add(&path);
            Ok((format!("Add Parameter Group {path}"), path.clone(), false))
        })
    }

    /// Renames a parameter group (`new_group` is the whole new path, so a
    /// group can also move under another): its subgroups and every
    /// parameter in them follow, `Rename Parameter Group Cabinet to
    /// Table`. The same path is no undo step. Refused: a group that does
    /// not exist, a path another group has, and a path within the group.
    /// Returns the new path.
    pub fn rename_parameter_group(
        &mut self,
        group: &str,
        new_group: &str,
    ) -> Result<String, ModelError> {
        let from = named_group(group)?;
        let to = named_group(new_group)?;
        if !self.state.parameter_groups.contains(&from) {
            return Err(invalid(format!("there is no parameter group {from}")));
        }
        if from == to {
            return Ok(to);
        }
        self.apply_with(|state| {
            state.parameter_groups.rename(&from, &to).map_err(invalid)?;
            Ok((
                format!("Rename Parameter Group {from} to {to}"),
                to.clone(),
                false,
            ))
        })
    }

    /// Deletes a parameter group, `Delete Parameter Group Cabinet`: its
    /// parameters move to its parent group (out of any group when it is at
    /// the top), its subgroups up a level; no parameter is deleted.
    /// Returns the names of the parameters that were in the group itself.
    pub fn delete_parameter_group(&mut self, group: &str) -> Result<Vec<String>, ModelError> {
        let path = named_group(group)?;
        if !self.state.parameter_groups.contains(&path) {
            return Err(invalid(format!("there is no parameter group {path}")));
        }
        self.apply_with(|state| {
            let moved = state.parameter_groups.delete(&path).map_err(invalid)?;
            let names = moved
                .iter()
                .map(|id| state.parameters.name(*id))
                .collect::<Vec<_>>();
            Ok((format!("Delete Parameter Group {path}"), names, false))
        })
    }

    /// Puts user parameters into a group, made with its parents when it
    /// does not exist, or out of their groups with `""`: one undo step
    /// (`Move width to Group Cabinet`, `Move 2 Parameters to Group
    /// Cabinet`, `Take width out of its Group`), none when every one is
    /// there already. Refused: an unknown name, and a parameter that is not
    /// a user parameter. Returns the group's path as written plainly.
    pub fn set_parameters_group(
        &mut self,
        names: &[String],
        group: &str,
    ) -> Result<String, ModelError> {
        let path = group_path(group).map_err(invalid)?;
        let mut ids: Vec<ParamId> = Vec::new();
        for name in names {
            let id = self
                .state
                .parameters
                .find(name)
                .ok_or_else(|| ParameterError::Unknown(name.clone()))?;
            let user = self
                .state
                .parameters
                .get(id)
                .is_some_and(|p| p.kind() == ParamKind::User);
            if !user {
                return Err(invalid(format!(
                    "{name} is not a user parameter: only user parameters go in groups"
                )));
            }
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        if ids.is_empty() {
            return Err(invalid("no parameter to move"));
        }
        let groups = &self.state.parameter_groups;
        if ids
            .iter()
            .all(|id| groups.group_of(*id).unwrap_or_default() == path)
        {
            return Ok(path);
        }
        let label = match (ids.as_slice(), path.is_empty()) {
            ([id], false) => format!("Move {} to Group {path}", self.state.parameters.name(*id)),
            ([id], true) => format!("Take {} out of its Group", self.state.parameters.name(*id)),
            (many, false) => format!("Move {} Parameters to Group {path}", many.len()),
            (many, true) => format!("Take {} Parameters out of their Groups", many.len()),
        };
        self.apply_with(|state| {
            for id in &ids {
                state.parameter_groups.set(*id, &path);
            }
            Ok((label, path.clone(), false))
        })
    }
}
