// SPDX-License-Identifier: MIT
//! Folders of a component's occurrences in the browser (mitcad#2103), and
//! the rename of a folder of its Bodies folder (mitcad#1802, mitcad#2103).
//!
//! A folder is organization only, like a name: a name kept on each
//! occurrence (or body) in it. Occurrences of the same parent with the same
//! folder share it; a folder no occurrence names any more is gone. Folders
//! do not nest. A folder command is one undo step that recomputes nothing:
//! no geometry, timeline, bill of materials, joint, drawing or export
//! changes with it. An occurrence moved to another parent (Move to
//! Component) leaves its folder behind (`reparent.rs`).

use super::{Document, ModelError, invalid};
use crate::ids::{ComponentUid, OccurrenceUid};
use crate::kernel::Kernel;

/// A folder's name as given: trimmed; an empty one is refused.
fn folder_name(name: &str) -> Result<String, ModelError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(invalid("the folder name is empty"));
    }
    Ok(name.to_owned())
}

impl<K: Kernel> Document<K> {
    /// Puts an occurrence into a folder of its parent component's
    /// occurrences in the browser, or back among them in no folder with
    /// None: one undo step (`Move Arm:1 to Frame`, `Take Arm:1 out of its
    /// Folder`) that recomputes nothing, none when nothing changes. The
    /// parent must not be linked from another file (its occurrences come
    /// from that file).
    pub fn set_occurrence_folder(
        &mut self,
        uid: OccurrenceUid,
        folder: Option<&str>,
    ) -> Result<(), ModelError> {
        let folder = folder.map(folder_name).transpose()?;
        let occurrence = self.state.require_occurrence(uid)?;
        self.state.editable(occurrence.parent)?;
        if occurrence.folder == folder {
            return Ok(());
        }
        self.apply_with(|state| {
            let name = state.assembly.occurrence_name(uid);
            let label = match &folder {
                Some(folder) => format!("Move {name} to {folder}"),
                None => format!("Take {name} out of its Folder"),
            };
            state.assembly.occurrence_mut(uid).expect("exists").folder = folder;
            Ok((label, (), false))
        })
    }

    /// The folders of a component's occurrences, in the order of their
    /// first occurrences (the browser's order, mitcad#2103).
    pub fn occurrence_folders(&self, component: ComponentUid) -> Vec<String> {
        let mut folders: Vec<String> = Vec::new();
        for o in self.state.assembly.children(component) {
            if let Some(folder) = &o.folder
                && !folders.contains(folder)
            {
                folders.push(folder.clone());
            }
        }
        folders
    }

    /// Renames a folder of a component's occurrences: every occurrence in
    /// it takes the new name, one undo step (`Rename Frame to Profiles`)
    /// that recomputes nothing, none when the name is the same. Refused: a
    /// folder the component's occurrences do not have, an empty name, and
    /// a name another of their folders has.
    pub fn rename_occurrence_folder(
        &mut self,
        component: ComponentUid,
        folder: &str,
        name: &str,
    ) -> Result<(), ModelError> {
        let name = folder_name(name)?;
        self.state.editable(component)?;
        let folders = self.occurrence_folders(component);
        let old = folder.trim();
        if !folders.iter().any(|f| f == old) {
            return Err(invalid(format!(
                "{} has no folder {old} among its components",
                self.state.assembly.name(component)
            )));
        }
        if name == old {
            return Ok(());
        }
        if folders.contains(&name) {
            return Err(invalid(format!(
                "{} already has a folder named {name}",
                self.state.assembly.name(component)
            )));
        }
        let old = old.to_owned();
        self.apply_with(|state| {
            for o in state.assembly.occurrences.iter_mut() {
                if o.parent == component && o.folder.as_deref() == Some(old.as_str()) {
                    o.folder = Some(name.clone());
                }
            }
            Ok((format!("Rename {old} to {name}"), (), false))
        })
    }

    /// The folders of a component's Bodies folder, in the order of their
    /// first bodies at the marker (mitcad#1802).
    pub fn body_folders(&self, component: ComponentUid) -> Vec<String> {
        let mut bodies = self.component_bodies(component);
        bodies.sort();
        let mut folders: Vec<String> = Vec::new();
        for uid in bodies {
            if let Some(folder) = self
                .state
                .body_attributes
                .get(&uid)
                .and_then(|a| a.folder.clone())
                && !folders.contains(&folder)
            {
                folders.push(folder);
            }
        }
        folders
    }

    /// Renames a folder of a component's Bodies folder (mitcad#2103): every
    /// body of the component in it takes the new name, one undo step
    /// (`Rename Glass to Panes`) that recomputes nothing, none when the
    /// name is the same. Refused: a folder the component's bodies at the
    /// marker do not have, an empty name, and a name another of its folders
    /// has.
    pub fn rename_body_folder(
        &mut self,
        component: ComponentUid,
        folder: &str,
        name: &str,
    ) -> Result<(), ModelError> {
        let name = folder_name(name)?;
        let folders = self.body_folders(component);
        let old = folder.trim();
        if !folders.iter().any(|f| f == old) {
            return Err(invalid(format!(
                "{} has no folder {old} among its bodies",
                self.state.assembly.name(component)
            )));
        }
        if name == old {
            return Ok(());
        }
        if folders.contains(&name) {
            return Err(invalid(format!(
                "{} already has a folder named {name} among its bodies",
                self.state.assembly.name(component)
            )));
        }
        let bodies = self.component_bodies(component);
        let old = old.to_owned();
        self.apply_with(|state| {
            for uid in &bodies {
                if let Some(attributes) = state.body_attributes.get_mut(uid)
                    && attributes.folder.as_deref() == Some(old.as_str())
                {
                    attributes.folder = Some(name.clone());
                }
            }
            Ok((format!("Rename {old} to {name}"), (), false))
        })
    }
}
