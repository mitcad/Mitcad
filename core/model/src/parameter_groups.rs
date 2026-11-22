// SPDX-License-Identifier: MIT
//! Groups of user parameters (mitcad#2180): organization only, like a
//! parameter's comment. A group is a path of names nested with `/`
//! (`Cabinet/Doors`); a user parameter is in at most one group, and an
//! ungrouped one is directly under User Parameters. Groups are kept also
//! while they hold no parameter, so that a group can be made before it is
//! filled. Every group's parent group exists ([`ParameterGroups::paths`]
//! lists it before its subgroups), so the groups are a tree.
//!
//! The commands are the document's (`document/parameter_groups.rs`); none
//! of them recomputes. Model parameters have no group: a parameter that
//! stops being a user parameter (a feature adopts it) or is deleted leaves
//! its group ([`ParameterGroups::retain_users`]).

use std::collections::BTreeMap;

use crate::expr::ParamKind;
use crate::parameters::{ParamId, Parameters};

/// The separator of a group path's names.
pub const SEPARATOR: char = '/';

/// A group path as given, written plainly: each name trimmed, joined with
/// `/`; the empty text (or only spaces) is no group (`""`). A path with an
/// empty name (`Cabinet//Doors`, `/Cabinet`) is refused.
pub fn group_path(text: &str) -> Result<String, String> {
    if text.trim().is_empty() {
        return Ok(String::new());
    }
    let mut names = Vec::new();
    for name in text.split(SEPARATOR) {
        let name = name.trim();
        if name.is_empty() {
            return Err(format!("“{text}” has an empty group name"));
        }
        names.push(name);
    }
    Ok(names.join("/"))
}

/// A path's parent group: `Cabinet` of `Cabinet/Doors`, `""` (no group) of
/// a group at the top.
pub fn parent_path(path: &str) -> &str {
    path.rfind(SEPARATOR).map_or("", |at| &path[..at])
}

/// A path's own name, its last: `Doors` of `Cabinet/Doors`.
pub fn group_name(path: &str) -> &str {
    path.rfind(SEPARATOR).map_or(path, |at| &path[at + 1..])
}

/// Whether `path` is `group` or one of its subgroups.
pub fn within(path: &str, group: &str) -> bool {
    path == group
        || path
            .strip_prefix(group)
            .is_some_and(|rest| rest.starts_with(SEPARATOR))
}

/// `path` (within `from`) with `from` replaced by `to`; `to` empty takes
/// `from` away (`Cabinet/Doors` with `Cabinet` to `""` is `Doors`).
fn moved(path: &str, from: &str, to: &str) -> String {
    let rest = &path[from.len()..];
    if to.is_empty() {
        rest.trim_start_matches(SEPARATOR).to_owned()
    } else {
        format!("{to}{rest}")
    }
}

/// The groups of the user parameters and which group each one is in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParameterGroups {
    /// Every group's path, in the order the groups were made; a parent
    /// before its subgroups, each path once.
    paths: Vec<String>,
    /// The group of each grouped user parameter.
    members: BTreeMap<ParamId, String>,
}

impl ParameterGroups {
    /// Groups with these paths (written plainly, see [`group_path`]) and no
    /// parameters, in this order; parents that are not listed are added
    /// before their first subgroup.
    pub fn from_paths<'a>(paths: impl IntoIterator<Item = &'a str>) -> Result<Self, String> {
        let mut groups = Self::default();
        for path in paths {
            let path = group_path(path)?;
            if path.is_empty() {
                return Err("a group path is empty".to_owned());
            }
            groups.paths.push(path);
        }
        groups.settle();
        Ok(groups)
    }

    /// Whether there is no group.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Every group's path, a parent before its subgroups, otherwise in the
    /// order the groups were made.
    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// Whether the group exists.
    pub fn contains(&self, path: &str) -> bool {
        self.paths.iter().any(|p| p == path)
    }

    /// A parameter's group; None when it is in none.
    pub fn group_of(&self, id: ParamId) -> Option<&str> {
        self.members.get(&id).map(String::as_str)
    }

    /// The parameters in a group itself (not in its subgroups), in the
    /// order they were made.
    pub fn members_of(&self, path: &str) -> Vec<ParamId> {
        self.members
            .iter()
            .filter(|(_, group)| *group == path)
            .map(|(id, _)| *id)
            .collect()
    }

    /// The parameters in a group and in its subgroups.
    pub fn members_within(&self, path: &str) -> Vec<ParamId> {
        self.members
            .iter()
            .filter(|(_, group)| within(group, path))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Makes a group and the parents it lacks; false when it exists.
    pub(crate) fn add(&mut self, path: &str) -> bool {
        if path.is_empty() || self.contains(path) {
            return false;
        }
        self.paths.push(path.to_owned());
        self.settle();
        true
    }

    /// Puts a parameter into a group, made with its parents when it does
    /// not exist; `""` takes it out of its group.
    pub(crate) fn set(&mut self, id: ParamId, path: &str) {
        if path.is_empty() {
            self.members.remove(&id);
        } else {
            self.add(path);
            self.members.insert(id, path.to_owned());
        }
    }

    /// Renames a group: its path and its subgroups' paths, and the group
    /// of every parameter in them, take `to` in place of `from`; the
    /// parents `to` lacks are made. Refused: a group that does not exist,
    /// a new path another group has, and a path within the group itself.
    pub(crate) fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        if !self.contains(from) {
            return Err(format!("there is no parameter group {from}"));
        }
        if to.is_empty() {
            return Err("the group name is empty".to_owned());
        }
        if within(to, from) && to != from {
            return Err(format!("the group {from} cannot go into itself"));
        }
        if self.contains(to) {
            return Err(format!("a parameter group {to} already exists"));
        }
        for path in &mut self.paths {
            if within(path, from) {
                *path = moved(path, from, to);
            }
        }
        for group in self.members.values_mut() {
            if within(group, from) {
                *group = moved(group, from, to);
            }
        }
        self.settle();
        Ok(())
    }

    /// Deletes a group: its parameters move to its parent group (out of
    /// any group at the top), and its subgroups with their parameters move
    /// up a level (merged with a group of the same path there). Returns the
    /// parameters that were in the group itself.
    pub(crate) fn delete(&mut self, path: &str) -> Result<Vec<ParamId>, String> {
        if !self.contains(path) {
            return Err(format!("there is no parameter group {path}"));
        }
        let parent = parent_path(path).to_owned();
        let own = self.members_of(path);
        self.paths.retain(|p| p != path);
        for p in &mut self.paths {
            if within(p, path) {
                *p = moved(p, path, &parent);
            }
        }
        for id in &own {
            self.set(*id, &parent);
        }
        for group in self.members.values_mut() {
            if within(group, path) {
                *group = moved(group, path, &parent);
            }
        }
        self.settle();
        Ok(own)
    }

    /// Keeps the groups of user parameters only: a deleted parameter, or
    /// one a feature adopted, leaves its group.
    pub(crate) fn retain_users(&mut self, parameters: &Parameters) {
        self.members.retain(|id, _| {
            parameters
                .get(*id)
                .is_some_and(|p| p.kind() == ParamKind::User)
        });
    }

    /// Every path once, each after its parents (which are made where
    /// missing), otherwise in the order they had.
    fn settle(&mut self) {
        let mut settled: Vec<String> = Vec::with_capacity(self.paths.len());
        for path in &self.paths {
            let mut at = 0;
            loop {
                let end = path[at..].find(SEPARATOR).map(|i| at + i);
                let prefix = &path[..end.unwrap_or(path.len())];
                if !settled.iter().any(|p| p == prefix) {
                    settled.push(prefix.to_owned());
                }
                match end {
                    Some(end) => at = end + 1,
                    None => break,
                }
            }
        }
        self.paths = settled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(raw: u32) -> ParamId {
        ParamId::from_raw(raw)
    }

    #[test]
    fn a_path_is_written_plainly() {
        assert_eq!(group_path(" Cabinet / Doors ").unwrap(), "Cabinet/Doors");
        assert_eq!(group_path("  ").unwrap(), "");
        assert!(group_path("Cabinet//Doors").is_err());
        assert!(group_path("/Cabinet").is_err());
        assert_eq!(parent_path("Cabinet/Doors/Left"), "Cabinet/Doors");
        assert_eq!(parent_path("Cabinet"), "");
        assert_eq!(group_name("Cabinet/Doors"), "Doors");
        assert!(within("Cabinet/Doors", "Cabinet"));
        assert!(within("Cabinet", "Cabinet"));
        assert!(!within("Cabinets", "Cabinet"));
    }

    #[test]
    fn a_nested_group_makes_its_parents() {
        let mut groups = ParameterGroups::default();
        assert!(groups.add("Material"));
        assert!(groups.add("Cabinet/Doors"));
        assert!(!groups.add("Cabinet"));
        assert_eq!(groups.paths(), ["Material", "Cabinet", "Cabinet/Doors"]);
        groups.set(id(3), "Table/Top");
        assert_eq!(
            groups.paths(),
            ["Material", "Cabinet", "Cabinet/Doors", "Table", "Table/Top"]
        );
        assert_eq!(groups.group_of(id(3)), Some("Table/Top"));
        groups.set(id(3), "");
        assert_eq!(groups.group_of(id(3)), None);
    }

    #[test]
    fn a_rename_moves_the_subgroups_and_the_parameters() {
        let mut groups = ParameterGroups::default();
        groups.set(id(1), "Cabinet");
        groups.set(id(2), "Cabinet/Doors");
        groups.set(id(3), "Material");
        groups.rename("Cabinet", "Furniture/Cupboard").unwrap();
        assert_eq!(
            groups.paths(),
            [
                "Furniture",
                "Furniture/Cupboard",
                "Furniture/Cupboard/Doors",
                "Material"
            ]
        );
        assert_eq!(groups.group_of(id(1)), Some("Furniture/Cupboard"));
        assert_eq!(groups.group_of(id(2)), Some("Furniture/Cupboard/Doors"));
        assert_eq!(groups.group_of(id(3)), Some("Material"));
        assert!(groups.rename("Material", "Furniture").is_err());
        assert!(groups.rename("Furniture", "Furniture/Inner").is_err());
        assert!(groups.rename("Nothing", "Else").is_err());
    }

    #[test]
    fn a_deleted_group_moves_everything_up_a_level() {
        let mut groups = ParameterGroups::default();
        groups.set(id(1), "Cabinet");
        groups.set(id(2), "Cabinet/Doors");
        groups.set(id(3), "Cabinet/Doors/Hinges");
        groups.set(id(4), "Doors");
        assert_eq!(groups.delete("Cabinet").unwrap(), vec![id(1)]);
        // Cabinet/Doors merges with the Doors at the top.
        assert_eq!(groups.paths(), ["Doors", "Doors/Hinges"]);
        assert_eq!(groups.group_of(id(1)), None);
        assert_eq!(groups.group_of(id(2)), Some("Doors"));
        assert_eq!(groups.group_of(id(3)), Some("Doors/Hinges"));
        assert_eq!(groups.delete("Doors/Hinges").unwrap(), vec![id(3)]);
        assert_eq!(groups.group_of(id(3)), Some("Doors"));
        assert!(groups.delete("Hinges").is_err());
    }

    #[test]
    fn paths_from_a_file_are_settled() {
        let groups = ParameterGroups::from_paths(["Cabinet/Doors", "Material", "Cabinet"]).unwrap();
        assert_eq!(groups.paths(), ["Cabinet", "Cabinet/Doors", "Material"]);
        assert!(ParameterGroups::from_paths(["a//b"]).is_err());
        assert!(ParameterGroups::from_paths([""]).is_err());
    }
}
