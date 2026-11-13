// SPDX-License-Identifier: MIT
//! The design's copies of library items (mitcad#640, part 4,
//! [`crate::print_library`]): a material (with its list of appearances,
//! mitcad#1067) or a printer or process profile copied into the design
//! with its origin, a newer version in its library offered and taken on
//! request, and a design's item saved into the user's or the project's
//! library.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{DocState, Document, ModelError, invalid, library_items::FileWrite};
use crate::appearance::{Appearance, library_appearance};
use crate::kernel::Kernel;
use crate::material::{Material, MaterialOrigin, built_in_material, numbered_name};
use crate::print_library::{
    ItemData, ItemKind, Libraries, LibraryItem, LibraryKind, carried, item_path,
};
use crate::print_profile::free_id;

/// A design's copy whose library has a newer version.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryUpdate {
    pub kind: ItemKind,
    /// The copy's id and name in the design.
    pub id: String,
    pub name: String,
    /// Where it came from: the library, the item's id, the version copied
    /// and the library's version now.
    pub library: LibraryKind,
    pub library_id: String,
    pub version: u32,
    pub newer: u32,
}

/// Whether an origin is an item's.
fn from_item(origin: Option<&MaterialOrigin>, library: &str, id: &str) -> bool {
    origin.is_some_and(|o| o.library == library && o.id == id)
}

impl DocState {
    /// The appearances a material's library item carries (mitcad#1067):
    /// those of its list that are the design's (not the library's), in
    /// the list's order, the default's first.
    pub(crate) fn carried_appearances(&self, material: &Material) -> Vec<Appearance> {
        material
            .appearance_list()
            .into_iter()
            .filter(|id| library_appearance(id).is_none())
            .filter_map(|id| self.appearances.iter().find(|a| a.id == id))
            .cloned()
            .collect()
    }

    /// Adds an appearance a library material carries: one of the library's
    /// appearances is used as it is, one the design has as it is too;
    /// another of the same id gets a free id. The id it has here.
    pub(crate) fn add_item_appearance(&mut self, appearance: &Appearance) -> String {
        if library_appearance(&appearance.id).is_some() {
            return appearance.id.clone();
        }
        match self.appearances.iter().find(|a| a.id == appearance.id) {
            Some(mine) if mine == appearance => appearance.id.clone(),
            Some(_) => {
                let base = appearance.id.clone();
                let id = (1..)
                    .map(|n| format!("{base}_{n}"))
                    .find(|candidate| {
                        library_appearance(candidate).is_none()
                            && self.appearances.iter().all(|a| &a.id != candidate)
                    })
                    .expect("a free id");
                let mut copy = appearance.clone();
                copy.id = id.clone();
                self.appearances.push(copy);
                id
            }
            None => {
                self.appearances.push(appearance.clone());
                appearance.id.clone()
            }
        }
    }

    /// Puts a material from elsewhere (a library, an import) into the
    /// design with the appearances of its list it carries (mitcad#1067):
    /// in the place of the design's of the same id when `replace` (or when
    /// that one is the same), else under a free id and a free name. The id
    /// it has here.
    pub(crate) fn put_material(
        &mut self,
        mut material: Material,
        appearances: &[Appearance],
        replace: bool,
    ) -> String {
        // Each carried appearance under the id it has here (another of its
        // id in the design gives it a free one), its place in the list kept.
        for appearance in appearances {
            let here = self.add_item_appearance(appearance);
            material.rename_appearance(&appearance.id, &here);
        }
        // The appearances it offers here, and its own filament colour as one
        // of them (mitcad#1055).
        self.known_appearances_only(&mut material);
        self.adopt_filament_colour(&mut material, false);
        if let Some(i) = self.materials.iter().position(|m| m.id == material.id) {
            if replace || self.materials[i] == material {
                let id = material.id.clone();
                self.materials[i] = material;
                return id;
            }
            let base = material.id.clone();
            material.id = (1..)
                .map(|n| format!("{base}_{n}"))
                .find(|candidate| {
                    self.materials.iter().all(|m| &m.id != candidate)
                        && built_in_material(candidate).is_none()
                })
                .expect("a free id");
        }
        // A name the design holds is numbered as elsewhere: "X (2)",
        // "X (3)", ... (mitcad#1728).
        if self.material_name_taken(&material.name, &material.id, None) {
            material.name = numbered_name(&material.name, |name| {
                self.material_name_taken(name, &material.id, None)
            });
        }
        let id = material.id.clone();
        self.materials.push(material);
        id
    }
}

impl<K: Kernel> Document<K> {
    /// The libraries for this design: the base library through the library
    /// resolver, the user's folder ([`crate::print_library::user_library`])
    /// and the project's (`project`: the project's folder, when the design
    /// is in one).
    pub fn print_libraries(&self, project: Option<&Path>) -> Libraries {
        let mut problems = Vec::new();
        let base = match self.link_resolver() {
            Some(resolver) => resolver.print_library().unwrap_or_else(|e| {
                problems.push(format!("base: {e}"));
                Vec::new()
            }),
            None => Vec::new(),
        };
        let user = crate::print_library::user_library();
        let mut libraries = Libraries::load(base, user.as_deref(), project);
        libraries.problems.extend(problems);
        libraries
    }

    /// The design's copies whose library item was changed elsewhere: a
    /// newer version than the one copied, not declined, with other fields
    /// (mitcad#655).
    pub fn print_library_updates(&self, libraries: &Libraries) -> Vec<LibraryUpdate> {
        let mut out = Vec::new();
        let mut check = |kind: ItemKind, id: &str, name: &str, origin: Option<&MaterialOrigin>| {
            let Some(origin) = origin else {
                return;
            };
            let Some(library) = LibraryKind::parse(&origin.library) else {
                return;
            };
            let Some(item) = libraries.find(Some(library), kind, &origin.id) else {
                return;
            };
            let version = origin.version.unwrap_or(0);
            if self.library_changed(id, item) {
                out.push(LibraryUpdate {
                    kind,
                    id: id.to_owned(),
                    name: name.to_owned(),
                    library,
                    library_id: origin.id.clone(),
                    version,
                    newer: item.data.version(),
                });
            }
        };
        for m in &self.state.materials {
            check(ItemKind::Material, &m.id, &m.name, m.origin.as_ref());
        }
        for p in &self.state.print_profiles {
            check(
                ItemKind::of_profile(p.kind),
                &p.id,
                &p.name,
                p.origin.as_ref(),
            );
        }
        out
    }

    /// Why a library material cannot come into the design: its default
    /// appearance is neither the library's nor carried with it nor the
    /// design's. (Another of its list that is none of these is left out.)
    fn check_item_appearance(&self, data: &ItemData) -> Result<(), ModelError> {
        if let ItemData::Material {
            material,
            appearances,
        } = data
            && let Some(id) = material.appearance.as_deref()
            && carried(appearances, Some(id)).is_none()
            && self.appearance(id).is_none()
        {
            return Err(invalid(format!(
                "the material {} names the appearance {id}, which neither it nor the design has",
                material.id
            )));
        }
        Ok(())
    }

    /// Copies a library item into the design with its origin: a material
    /// with its appearance, a printer or process profile. A copy the
    /// design has of the same item is replaced (an update); another item
    /// of the same id keeps its id and the copy gets a free one. Returns
    /// the id in the design. One undo step.
    pub fn copy_from_print_library(&mut self, item: &LibraryItem) -> Result<String, ModelError> {
        self.check_item_appearance(&item.data)?;
        let origin = MaterialOrigin {
            library: item.library.id().to_owned(),
            id: item.data.id().to_owned(),
            version: Some(item.data.version()),
            declined: None,
        };
        let label = format!("Copy {} from Library", item.data.name());
        let data = item.data.clone();
        self.apply_with(move |state| {
            let id = match data {
                ItemData::Material {
                    mut material,
                    appearances,
                } => {
                    let replace = state.materials.iter().any(|m| {
                        m.id == material.id
                            && from_item(m.origin.as_ref(), &origin.library, &origin.id)
                    });
                    material.origin = Some(origin);
                    state.put_material(material, &appearances, replace)
                }
                ItemData::Profile(mut profile) => {
                    let existing = state.print_profiles.iter().position(|p| {
                        p.id == profile.id
                            && from_item(p.origin.as_ref(), &origin.library, &origin.id)
                    });
                    profile.origin = Some(origin);
                    match existing {
                        Some(i) => {
                            let id = profile.id.clone();
                            state.print_profiles[i] = profile;
                            state.sync_profile_types();
                            id
                        }
                        None => state.add_print_profile(profile),
                    }
                }
            };
            Ok((label, id, false))
        })
    }

    /// The library item a design's copy came from.
    fn copy_origin_item<'a>(
        &self,
        libraries: &'a Libraries,
        kind: ItemKind,
        id: &str,
    ) -> Result<(MaterialOrigin, &'a LibraryItem), ModelError> {
        let origin = match kind {
            ItemKind::Material => self
                .state
                .materials
                .iter()
                .find(|m| m.id == id)
                .map(|m| m.origin.clone()),
            _ => self
                .print_profile(id)
                .filter(|p| ItemKind::of_profile(p.kind) == kind)
                .map(|p| p.origin.clone()),
        }
        .ok_or_else(|| invalid(format!("the design has no {} {id}", kind.id())))?;
        let origin = origin
            .ok_or_else(|| invalid(format!("the {} {id} came from no library", kind.id())))?;
        let library = LibraryKind::parse(&origin.library).ok_or_else(|| {
            invalid(format!(
                "the {} {id} came from {}, not a library",
                kind.id(),
                origin.library
            ))
        })?;
        let item = libraries
            .find(Some(library), kind, &origin.id)
            .ok_or_else(|| {
                invalid(format!(
                    "the {} library has no {} {}",
                    library.id(),
                    kind.id(),
                    origin.id
                ))
            })?;
        Ok((origin, item))
    }

    /// Takes the newer version of a design's copy from its library: its
    /// fields become the library item's, its id stays. Returns the version
    /// taken. One undo step.
    pub fn update_from_print_library(
        &mut self,
        libraries: &Libraries,
        kind: ItemKind,
        id: &str,
    ) -> Result<u32, ModelError> {
        let (origin, item) = self.copy_origin_item(libraries, kind, id)?;
        self.check_item_appearance(&item.data)?;
        let version = item.data.version();
        let new_origin = MaterialOrigin {
            library: origin.library,
            id: origin.id,
            version: Some(version),
            declined: None,
        };
        let label = format!("Update {} from Library", item.data.name());
        let data = item.data.clone();
        let id = id.to_owned();
        self.apply_with(move |state| {
            match data {
                ItemData::Material {
                    mut material,
                    appearances,
                } => {
                    material.id = id;
                    material.origin = Some(new_origin);
                    state.put_material(material, &appearances, true);
                }
                ItemData::Profile(mut profile) => {
                    profile.id = id.clone();
                    profile.origin = Some(new_origin);
                    if let Some(slot) = state.print_profiles.iter_mut().find(|p| p.id == id) {
                        *slot = profile;
                    }
                    state.sync_profile_types();
                }
            }
            Ok((label, (), false))
        })?;
        Ok(version)
    }

    /// Keeps a design copy's own values over the library's newer version
    /// ("Keep these", mitcad#655): the copy remembers that version as
    /// declined, so it is not offered again until the library changes
    /// again. Returns the version declined. One undo step.
    pub fn decline_print_library_update(
        &mut self,
        libraries: &Libraries,
        kind: ItemKind,
        id: &str,
    ) -> Result<u32, ModelError> {
        let (_, item) = self.copy_origin_item(libraries, kind, id)?;
        let version = item.data.version();
        let label = format!("Keep {}", item.data.name());
        let id = id.to_owned();
        self.apply_with(move |state| {
            let origin = match kind {
                ItemKind::Material => state
                    .materials
                    .iter_mut()
                    .find(|m| m.id == id)
                    .and_then(|m| m.origin.as_mut()),
                _ => state
                    .print_profiles
                    .iter_mut()
                    .find(|p| p.id == id)
                    .and_then(|p| p.origin.as_mut()),
            };
            if let Some(origin) = origin {
                origin.declined = Some(version);
            }
            Ok((label, (), false))
        })?;
        Ok(version)
    }

    /// Saves a design's material (also a built-in one) or profile into the
    /// user's or the project's library: as the next version of the item it
    /// was saved as before, else as a new item (under a free id when the
    /// library has another item of its id, mitcad#655). The design's copy
    /// then names it as its origin; the file and the copy are one undo
    /// step. Returns the file written and the version.
    pub fn save_to_print_library(
        &mut self,
        libraries: &Libraries,
        library: LibraryKind,
        kind: ItemKind,
        id: &str,
    ) -> Result<(PathBuf, u32), ModelError> {
        if library == LibraryKind::Base {
            return Err(invalid(
                "the base library is MitCad's component library: save to the user's or the project's",
            ));
        }
        let folder = libraries
            .folder(library)
            .ok_or_else(|| {
                invalid(match library {
                    LibraryKind::Project => "the design is in no project",
                    _ => "there is no user library folder",
                })
            })?
            .to_path_buf();
        // The design's item saved before goes on as the library's next
        // version; another item there of the same id stays, and this one
        // gets a free id (mitcad#655: designs' own ids repeat).
        let origin_now = match kind {
            ItemKind::Material => self
                .state
                .materials
                .iter()
                .find(|m| m.id == id)
                .and_then(|m| m.origin.clone()),
            _ => self.print_profile(id).and_then(|p| p.origin.clone()),
        };
        let saved_as = origin_now
            .filter(|o| o.library == library.id())
            .map(|o| o.id)
            .filter(|saved| libraries.find(Some(library), kind, saved).is_some());
        let library_id = saved_as.clone().unwrap_or_else(|| {
            free_id(id, |candidate| {
                libraries.find(Some(library), kind, candidate).is_some()
            })
        });
        let version = saved_as
            .and_then(|saved| libraries.find(Some(library), kind, &saved))
            .map_or(0, |i| i.data.version())
            + 1;
        let mut data = match kind {
            ItemKind::Material => {
                let mut material = self
                    .state
                    .materials
                    .iter()
                    .find(|m| m.id == id)
                    .cloned()
                    .or_else(|| built_in_material(id).cloned())
                    .ok_or_else(|| invalid(format!("there is no material {id}")))?;
                material.id = library_id.clone();
                material.origin = None;
                material.version = Some(version);
                // Every appearance of its list that is the design's, the
                // default's first (mitcad#1067).
                let appearances = self.state.carried_appearances(&material);
                ItemData::Material {
                    material,
                    appearances,
                }
            }
            _ => {
                let mut profile = self
                    .print_profile(id)
                    .filter(|p| ItemKind::of_profile(p.kind) == kind)
                    .cloned()
                    .ok_or_else(|| invalid(format!("there is no {} profile {id}", kind.id())))?;
                profile.id = library_id.clone();
                profile.origin = None;
                profile.version = Some(version);
                ItemData::Profile(profile)
            }
        };
        // A name another item there has gets a counter, `Fine 2` (mitcad#655:
        // the lists never show two alike); the design's copy takes it too.
        let clashes = |name: &str| {
            libraries.items.iter().any(|i| {
                i.library == library
                    && i.data.kind() == kind
                    && i.data.id() != library_id
                    && i.data.name().eq_ignore_ascii_case(name)
            })
        };
        let in_design = |name: &str| match kind {
            ItemKind::Material => self.state.material_name_taken(name, id, None),
            _ => self.state.print_profiles.iter().any(|p| {
                ItemKind::of_profile(p.kind) == kind
                    && p.id != id
                    && p.name.eq_ignore_ascii_case(name)
            }),
        };
        let current = data.name().to_owned();
        let name = if clashes(&current) {
            (2..)
                .map(|n| format!("{current} {n}"))
                .find(|candidate| !clashes(candidate) && !in_design(candidate))
                .expect("a free name")
        } else {
            current
        };
        match &mut data {
            ItemData::Material { material, .. } => material.name = name.clone(),
            ItemData::Profile(profile) => profile.name = name.clone(),
        }
        let path = folder.join(item_path(kind, &library_id));
        let file = FileWrite::new(path.clone(), Some(data.text().into_bytes()));
        let origin = MaterialOrigin {
            library: library.id().to_owned(),
            id: library_id,
            version: Some(version),
            declined: None,
        };
        // The file and the design's copy naming it are one undo step:
        // undo takes the file back too.
        let label = format!("Save {} to Library", data.name());
        let id = id.to_owned();
        self.with_files(label, vec![file], move |state| {
            match kind {
                ItemKind::Material => {
                    if let Some(m) = state.materials.iter_mut().find(|m| m.id == id) {
                        m.origin = Some(origin);
                        m.version = Some(version);
                        m.name = name;
                    }
                }
                _ => {
                    if let Some(p) = state.print_profiles.iter_mut().find(|p| p.id == id) {
                        p.origin = Some(origin);
                        p.version = Some(version);
                        p.name = name;
                    }
                    state.sync_profile_types();
                }
            }
            Ok(())
        })?;
        Ok((path, version))
    }

    /// The `print_library` query: every library item (`library`, `kind`,
    /// `id`, `name`, `version`, `path`, `shadowed`, `in_design`: the ids of
    /// the design's copies of it, `item`: its fields), the design's copies
    /// whose library item was changed elsewhere (`updates`), the folders and
    /// what could not be read.
    pub fn print_library_json(&self, libraries: &Libraries) -> Value {
        let copies = |item: &LibraryItem| -> Vec<String> {
            let library = item.library.id();
            let id = item.data.id();
            match &item.data {
                ItemData::Material { .. } => self
                    .state
                    .materials
                    .iter()
                    .filter(|m| from_item(m.origin.as_ref(), library, id))
                    .map(|m| m.id.clone())
                    .collect(),
                ItemData::Profile(_) => self
                    .state
                    .print_profiles
                    .iter()
                    .filter(|p| {
                        ItemKind::of_profile(p.kind) == item.data.kind()
                            && from_item(p.origin.as_ref(), library, id)
                    })
                    .map(|p| p.id.clone())
                    .collect(),
            }
        };
        let items: Vec<Value> = libraries
            .items
            .iter()
            .map(|item| {
                let fields = match &item.data {
                    ItemData::Material {
                        material,
                        appearances,
                    } => {
                        let mut value =
                            serde_json::to_value(material).expect("materials serialize");
                        if let Some(color) = carried(appearances, material.appearance.as_deref())
                            .map(|a| a.display_color())
                            .or_else(|| {
                                material
                                    .appearance
                                    .as_deref()
                                    .and_then(|a| self.appearance(a))
                                    .map(|a| a.display_color())
                            })
                        {
                            value["display_color"] = json!(color);
                        }
                        value
                    }
                    ItemData::Profile(profile) => {
                        let mut value = serde_json::to_value(profile).expect("profiles serialize");
                        if let Value::Object(fields) = &mut value {
                            fields.remove("raw");
                        }
                        value
                    }
                };
                json!({
                    "library": item.library.id(),
                    "kind": item.data.kind().id(),
                    "id": item.data.id(),
                    "name": item.data.name(),
                    "version": item.data.version(),
                    "path": item.path,
                    "shadowed": item.shadowed,
                    "in_design": copies(item),
                    "item": fields,
                })
            })
            .collect();
        let updates: Vec<Value> = self
            .print_library_updates(libraries)
            .into_iter()
            .map(|u| {
                json!({
                    "kind": u.kind.id(),
                    "id": u.id,
                    "name": u.name,
                    "library": u.library.id(),
                    "library_id": u.library_id,
                    "version": u.version,
                    "newer": u.newer,
                })
            })
            .collect();
        let folders: serde_json::Map<String, Value> = libraries
            .folders
            .iter()
            .map(|(library, folder)| (library.id().to_owned(), json!(folder.to_string_lossy())))
            .collect();
        json!({
            "items": items,
            "updates": updates,
            "folders": folders,
            "problems": libraries.problems,
        })
    }
}
