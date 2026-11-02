// SPDX-License-Identifier: MIT
//! One filament's separate materials merged into one with variants
//! (mitcad#1054, [`crate::print_files::filaments`]): materials kept from
//! before, one per slicer preset ("Shop PLA @Bench X", "Shop PLA @Bench
//! Y"), in the design or in the user's or the project's library.
//!
//! In the design the first material takes the merged values and the
//! filament's name (unless another material has it); the others go, and
//! the bodies and components that named them name it. One undo step. In a
//! library the first item's file takes the merged values (its next
//! version) and the others' files go; the design's copies of those that
//! bodies use stay as the design's own, as Delete leaves them. One undo
//! step too.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::library_items::library_item;
use super::{Document, ModelError, invalid};
use crate::appearance::Appearance;
use crate::kernel::Kernel;
use crate::material::{self, Material, MaterialChange};
use crate::print_files::filaments;
use crate::print_library::{ItemData, ItemKind, Libraries, LibraryKind};

/// Why materials cannot be merged.
const NOT_ONE_FILAMENT: &str =
    "these materials are not one filament's presets: two of them are for any printer";

impl<K: Kernel> Document<K> {
    /// The materials of the target (the design's own, `into` None, or a
    /// library's) that are one filament's presets kept apart: each group
    /// by the materials' ids and names.
    pub fn separate_filament_groups(
        &self,
        libraries: &Libraries,
        into: Option<LibraryKind>,
    ) -> Vec<Vec<(String, String)>> {
        let materials: Vec<&Material> = match into {
            None => self.state.materials.iter().collect(),
            Some(library) => libraries
                .items
                .iter()
                .filter(|i| i.library == library)
                .filter_map(|i| match &i.data {
                    ItemData::Material { material, .. } => Some(material),
                    ItemData::Profile(_) => None,
                })
                .collect(),
        };
        filaments::separate_filaments(&materials)
            .into_iter()
            .map(|group| {
                group
                    .into_iter()
                    .map(|i| (materials[i].id.clone(), materials[i].name.clone()))
                    .collect()
            })
            .collect()
    }

    /// [`Document::separate_filament_groups`] as the queries give it: each
    /// `{name, items: [{id, name}]}`, `name` the filament's.
    pub fn separate_filaments_json(
        &self,
        libraries: &Libraries,
        into: Option<LibraryKind>,
    ) -> Value {
        Value::Array(
            self.separate_filament_groups(libraries, into)
                .into_iter()
                .map(|group| {
                    let name = group
                        .first()
                        .map(|(_, name)| material::split_filament_name(name).0)
                        .unwrap_or_default();
                    let items: Vec<Value> = group
                        .iter()
                        .map(|(id, name)| json!({"id": id, "name": name}))
                        .collect();
                    json!({"name": name, "items": items})
                })
                .collect(),
        )
    }

    /// Merges the design's own materials `ids` (two or more, one
    /// filament's) into the first: see the module documentation. Returns
    /// its id.
    pub fn merge_materials(&mut self, ids: &[String]) -> Result<String, ModelError> {
        if ids.len() < 2 {
            return Err(invalid("merge_materials needs two materials or more"));
        }
        let mut indices = Vec::new();
        for id in ids {
            let i = self.custom_material_index(id)?;
            if indices.contains(&i) {
                return Err(invalid(format!("material {id} is named twice")));
            }
            indices.push(i);
        }
        let refs: Vec<&Material> = indices.iter().map(|&i| &self.state.materials[i]).collect();
        let mut merged = filaments::merge_kept(&refs).ok_or_else(|| invalid(NOT_ONE_FILAMENT))?;
        // It offers the appearances of them all (mitcad#1055): their
        // bodies keep theirs, now of the merged material's list.
        for m in &refs {
            for look in m.appearance_list() {
                merged.add_appearance(&look);
            }
        }
        let first = indices[0];
        // The filament's name, unless a material outside the merge shows
        // it.
        let state = &self.state;
        let shown_elsewhere = |name: &str| {
            state
                .materials
                .iter()
                .enumerate()
                .any(|(i, m)| !indices.contains(&i) && m.name == name)
                || material::built_in()
                    .iter()
                    .any(|b| b.name == name && !state.materials.iter().any(|m| m.id == b.id))
        };
        if shown_elsewhere(&merged.name) {
            merged.name = state.materials[first].name.clone();
        }
        merged.check().map_err(invalid)?;
        if let Some(a) = &merged.appearance
            && self.appearance(a).is_none()
        {
            return Err(invalid(format!(
                "material {}: there is no appearance {a}",
                merged.id
            )));
        }
        let id = merged.id.clone();
        let gone: Vec<String> = ids[1..].to_vec();
        let label = format!("Merge Materials {}", merged.name);
        let kept = id.clone();
        self.apply_with(move |state| {
            state.materials[first] = merged;
            state.materials.retain(|m| !gone.contains(&m.id));
            for attributes in state.body_attributes.values_mut() {
                if attributes
                    .material
                    .as_ref()
                    .is_some_and(|m| gone.contains(m))
                {
                    attributes.material = Some(kept.clone());
                }
            }
            for named in state.component_materials.values_mut() {
                if gone.contains(named) {
                    *named = kept.clone();
                }
            }
            for removed in &gone {
                state.drop_purge_overrides_of(removed);
            }
            Ok((label, (), false))
        })?;
        Ok(id)
    }

    /// Merges the materials `ids` of the user's or the project's library
    /// into the first's file: see the module documentation. Returns its
    /// id, its file and its version.
    pub fn merge_library_materials(
        &mut self,
        libraries: &Libraries,
        library: LibraryKind,
        ids: &[String],
    ) -> Result<(String, PathBuf, u32), ModelError> {
        if ids.len() < 2 {
            return Err(invalid("merge_materials needs two materials or more"));
        }
        let mut items: Vec<&Material> = Vec::new();
        // The appearances the items' files carry (mitcad#1067).
        let mut carried: Vec<Appearance> = Vec::new();
        for id in ids {
            let found = library_item(libraries, library, ItemKind::Material, id)?;
            let ItemData::Material {
                material,
                appearances,
            } = &found.data
            else {
                unreachable!("a material's folder")
            };
            if items.iter().any(|m| m.id == material.id) {
                return Err(invalid(format!("material {id} is named twice")));
            }
            items.push(material);
            for look in appearances {
                if !carried.iter().any(|a| a.id == look.id) {
                    carried.push(look.clone());
                }
            }
        }
        let merged = filaments::merge_kept(&items).ok_or_else(|| invalid(NOT_ONE_FILAMENT))?;
        let first = items[0];
        // It offers the appearances of them all, as the design's merge does
        // (mitcad#1055), and its file carries the others' too (mitcad#1150):
        // theirs go with their files.
        let mut listed = first.clone();
        for m in &items {
            for look in m.appearance_list() {
                listed.add_appearance(&look);
            }
        }
        // The first item's values become the merged ones: a key it has and
        // the merge has not goes.
        let mut filament: BTreeMap<String, Value> = first
            .filament
            .keys()
            .filter(|k| !merged.filament.contains_key(*k))
            .map(|k| (k.clone(), Value::Null))
            .collect();
        filament.extend(merged.filament.clone());
        let mut slicer: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
        for (dialect, keys) in &first.slicer {
            for key in keys.keys() {
                if merged
                    .slicer
                    .get(dialect)
                    .is_none_or(|own| !own.contains_key(key))
                {
                    slicer
                        .entry(dialect.clone())
                        .or_default()
                        .insert(key.clone(), Value::Null);
                }
            }
        }
        for (dialect, keys) in &merged.slicer {
            let own = slicer.entry(dialect.clone()).or_default();
            for (key, text) in keys {
                own.insert(key.clone(), Value::String(text.clone()));
            }
        }
        // The filament's name, unless another item of the library shows it.
        let shown_elsewhere = libraries.items.iter().any(|i| {
            i.library == library
                && i.data.kind() == ItemKind::Material
                && !ids.iter().any(|id| id == i.data.id())
                && i.data.name().eq_ignore_ascii_case(&merged.name)
        });
        let name = if shown_elsewhere {
            first.name.clone()
        } else {
            merged.name.clone()
        };
        let change = MaterialChange {
            name: Some(name),
            density: Some(merged.density),
            filament: Some(filament),
            slicer: Some(slicer),
            variants: Some(merged.variants.clone()),
            appearances: Some(listed.appearance_list()),
            ..MaterialChange::default()
        };
        let first_id = ids[0].clone();
        let others: Vec<String> = ids[1..].to_vec();
        self.as_one_step(|doc| {
            let (path, version) = doc.edit_library_material_carrying(
                libraries, library, &first_id, &change, None, &carried,
            )?;
            for id in &others {
                doc.delete_library_material(libraries, library, id)?;
            }
            Ok((first_id.clone(), path, version))
        })
    }

    /// Merges every group of one filament's separate materials the target
    /// has (an import's `merge_separate`): the design's own (`into` None)
    /// or a library's. The answer lists each merge: `{id, name, from}`
    /// (`from`: the ids merged).
    pub fn merge_separate_filaments(
        &mut self,
        into: Option<LibraryKind>,
        project: Option<&Path>,
    ) -> Result<Value, ModelError> {
        let libraries = self.print_libraries(project);
        let groups = self.separate_filament_groups(&libraries, into);
        let mut merged = Vec::new();
        for group in groups {
            let ids: Vec<String> = group.into_iter().map(|(id, _)| id).collect();
            let entry = match into {
                None => {
                    let id = self.merge_materials(&ids)?;
                    let name = self.material(&id).map(|m| m.name.clone());
                    json!({"id": id, "name": name, "from": ids})
                }
                Some(library) => {
                    // Read again: an earlier merge changed the files.
                    let libraries = self.print_libraries(project);
                    let (id, path, version) =
                        self.merge_library_materials(&libraries, library, &ids)?;
                    let name = self
                        .print_libraries(project)
                        .find(Some(library), ItemKind::Material, &id)
                        .map(|i| i.data.name().to_owned());
                    json!({"id": id, "name": name, "from": ids, "version": version,
                           "path": path.to_string_lossy()})
                }
            };
            merged.push(entry);
        }
        Ok(Value::Array(merged))
    }
}
