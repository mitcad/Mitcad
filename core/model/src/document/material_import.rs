// SPDX-License-Identifier: MIT
//! Materials imported from a JSON or CSV file (mitcad#1213,
//! `crate::material_exchange`): into the design or into the user's or the
//! project's library, as one undo step ("Import Materials").
//!
//! A material is matched by its name (any case): the design's own and the
//! built-in ones it shows, or the library's items. A name that is there
//! already is a conflict, decided once for the file: `keep` leaves the
//! material that is there and skips the file's; `replace` gives it the
//! file's values, the ones the file does not give kept (a built-in one
//! gets a material of the design in its place, with its values and the
//! file's); `keep_both` adds the file's as a new material, numbered
//! (`Steel (2)`). A material the file gives that is not valid is reported
//! with its place in the file and the reason, and the others are imported.
//!
//! Beside it, materials made from several entries of the bundled dataset
//! at once (mitcad#1706, `create_dataset_materials`): each as
//! `create_material` with `dataset` makes one, all as one undo step; an
//! entry whose name is there already is skipped, as `keep` skips it.

use std::path::Path;

use super::{Document, ModelError, invalid};
use crate::kernel::Kernel;
use crate::material::{self, MaterialChange};
use crate::material_dataset;
use crate::material_exchange::{self, FileFormat, MaterialRow, RowError};
use crate::print_library::{BasedOn, ItemKind, LibraryKind};

/// What an import does with a material whose name is there already.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NameConflict {
    /// The material that is there stays; the file's is skipped.
    #[default]
    Keep,
    /// The file's values replace its values; the others stay.
    Replace,
    /// The file's is added with a numbered name.
    KeepBoth,
    /// Nothing is imported when a name is there already: the names are
    /// given back for the user to choose (the application asks).
    Ask,
}

impl NameConflict {
    /// A choice by its id: `keep`, `replace` or `keep_both`.
    pub fn parse(text: &str) -> Result<NameConflict, ModelError> {
        match text {
            "keep" => Ok(NameConflict::Keep),
            "replace" => Ok(NameConflict::Replace),
            "keep_both" => Ok(NameConflict::KeepBoth),
            "ask" => Ok(NameConflict::Ask),
            _ => Err(invalid(format!(
                "on a name conflict: keep, replace, keep_both or ask, not {text}"
            ))),
        }
    }
}

/// What an import did: the materials added and replaced (their ids), the
/// names it kept, and the file's materials that were not valid or could not
/// be imported.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MaterialImport {
    pub added: Vec<String>,
    pub replaced: Vec<String>,
    pub kept: Vec<String>,
    /// With `ask`: the file's names that are there already, when nothing
    /// was imported for them.
    pub conflicts: Vec<String>,
    pub errors: Vec<RowError>,
}

impl MaterialImport {
    fn failed(&mut self, row: &MaterialRow, error: ModelError) {
        self.errors.push(RowError {
            row: row.row,
            name: row.name.clone(),
            message: error.to_string(),
        });
    }
}

/// The undo step of an import.
const LABEL: &str = "Import Materials";

/// What `create_dataset_materials` did (mitcad#1706): the materials made
/// (their ids, in the entries' order) and the entries skipped because a
/// material of their name is there already (their names).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DatasetMaterials {
    pub added: Vec<String>,
    pub skipped: Vec<String>,
}

/// The undo step of several materials made from the dataset at once.
const DATASET_LABEL: &str = "Create Materials";

impl<K: Kernel> Document<K> {
    /// Imports a materials file into the design (`library` none) or into
    /// the user's or the project's library (`project`: the project's
    /// folder), as one undo step; on a name conflict as `conflict` says.
    /// An error for a file it cannot read at all.
    pub fn import_materials(
        &mut self,
        text: &str,
        format: FileFormat,
        conflict: NameConflict,
        library: Option<LibraryKind>,
        project: Option<&Path>,
    ) -> Result<MaterialImport, ModelError> {
        if library == Some(LibraryKind::Base) {
            return Err(invalid("the base library cannot change"));
        }
        let read = material_exchange::read(text, format).map_err(invalid)?;
        let mut outcome = MaterialImport {
            errors: read.errors,
            ..MaterialImport::default()
        };
        if conflict == NameConflict::Ask {
            let libraries = library.map(|_| self.print_libraries(project));
            for row in &read.rows {
                let name = row.name.trim();
                let taken = match (library, &libraries) {
                    (Some(library), Some(libraries)) => libraries.items.iter().any(|i| {
                        i.library == library
                            && i.data.kind() == ItemKind::Material
                            && i.data.name().trim().eq_ignore_ascii_case(name)
                    }),
                    _ => self.design_material_named(name).is_some(),
                };
                if taken
                    && !outcome
                        .conflicts
                        .iter()
                        .any(|n| n.eq_ignore_ascii_case(name))
                {
                    outcome.conflicts.push(name.to_owned());
                }
            }
            if !outcome.conflicts.is_empty() {
                outcome.errors.sort_by_key(|e| e.row);
                return Ok(outcome);
            }
        }
        let depth = self.undo_depth();
        for row in &read.rows {
            let done = match library {
                None => self.import_into_design(row, conflict, &mut outcome),
                Some(library) => {
                    self.import_into_library(row, conflict, library, project, &mut outcome)
                }
            };
            if let Err(error) = done {
                outcome.failed(row, error);
            }
        }
        self.merge_undo(depth, LABEL);
        outcome.errors.sort_by_key(|e| e.row);
        Ok(outcome)
    }

    /// Makes a material of each entry of the bundled dataset (mitcad#1706),
    /// as `create_material` with `dataset` makes one: in the design
    /// (`library` none) or in the user's or the project's library
    /// (`project`: the project's folder), all as one undo step (`Create
    /// Materials`; one made keeps its own label). An entry whose name is
    /// there already (any case: the design's own and the built-in ones it
    /// shows, or the library's items and the built-in ones) is skipped, and
    /// so is a second of the same name. An entry the dataset lacks is
    /// refused, and nothing is made.
    pub fn create_dataset_materials(
        &mut self,
        entries: &[String],
        library: Option<LibraryKind>,
        project: Option<&Path>,
    ) -> Result<DatasetMaterials, ModelError> {
        if library == Some(LibraryKind::Base) {
            return Err(invalid("the base library cannot change"));
        }
        if entries.is_empty() {
            return Err(invalid("name at least one material of the dataset"));
        }
        let mut rows = Vec::new();
        for id in entries {
            let row = material_dataset::entry(id)
                .ok_or_else(|| invalid(format!("the material dataset has no material {id}")))?;
            rows.push((id, row));
        }
        let depth = self.undo_depth();
        let made = self.as_one_step(|doc| {
            let mut outcome = DatasetMaterials::default();
            for (id, row) in rows {
                let name = row.name.trim();
                if doc.dataset_name_taken(name, library, project) {
                    outcome.skipped.push(name.to_owned());
                    continue;
                }
                let change = MaterialChange {
                    dataset: Some(id.clone()),
                    ..MaterialChange::default()
                };
                let added = match library {
                    None => doc.create_material(&change)?,
                    Some(kind) => {
                        let libraries = doc.print_libraries(project);
                        let written = doc.create_library_material(&libraries, kind, &change)?;
                        written.id
                    }
                };
                outcome.added.push(added);
            }
            Ok(outcome)
        })?;
        if made.added.len() > 1 {
            self.merge_undo(depth, DATASET_LABEL);
        }
        Ok(made)
    }

    /// Whether a material of the name (any case) is where a material of
    /// the dataset would go: the design's own or a built-in one it shows;
    /// in a library, its items or a built-in one.
    fn dataset_name_taken(
        &self,
        name: &str,
        library: Option<LibraryKind>,
        project: Option<&Path>,
    ) -> bool {
        let Some(library) = library else {
            return self.design_material_named(name).is_some();
        };
        let same = |other: &str| other.trim().eq_ignore_ascii_case(name.trim());
        if material::built_in().iter().any(|m| same(&m.name)) {
            return true;
        }
        let libraries = self.print_libraries(project);
        libraries.items.iter().any(|i| {
            i.library == library && i.data.kind() == ItemKind::Material && same(i.data.name())
        })
    }

    /// One material into the design.
    fn import_into_design(
        &mut self,
        row: &MaterialRow,
        conflict: NameConflict,
        outcome: &mut MaterialImport,
    ) -> Result<(), ModelError> {
        let name = row.name.trim();
        // The design's own of the name, else a built-in it shows.
        let own = self
            .state
            .materials
            .iter()
            .find(|m| m.name.trim().eq_ignore_ascii_case(name))
            .map(|m| m.id.clone());
        let built_in = material::built_in()
            .iter()
            .filter(|m| self.state.materials.iter().all(|own| own.id != m.id))
            .find(|m| m.name.trim().eq_ignore_ascii_case(name))
            .map(|m| m.id.clone());
        let mut change = row.change();
        match (own, built_in, conflict) {
            (None, None, _) => {
                if row.density.is_none() {
                    return Err(invalid("a new material needs its density"));
                }
                outcome.added.push(self.create_material(&change)?);
            }
            (_, _, NameConflict::Keep | NameConflict::Ask) => {
                outcome.kept.push(name.to_owned());
            }
            (Some(id), _, NameConflict::Replace) => {
                // The name the design shows stays as it is written there.
                change.name = None;
                self.edit_material(&id, &change)?;
                outcome.replaced.push(id);
            }
            (None, Some(id), NameConflict::Replace) => {
                // The design's own in the built-in's place, its values and
                // the file's, named as the built-in (not as its copy).
                change.name = material::built_in_material(&id).map(|m| m.name.clone());
                change.id = Some(id.clone());
                change.based_on = Some(BasedOn::Design(id));
                outcome.replaced.push(self.create_material(&change)?);
            }
            (_, _, NameConflict::KeepBoth) => {
                if row.density.is_none() {
                    return Err(invalid("a new material needs its density"));
                }
                // Numbered past every name the design shows in any case,
                // as the clash above is found (mitcad#1315: `steel` beside
                // the built-in `Steel` is `steel (2)`).
                let state = &self.state;
                let shown = |candidate: &str| {
                    let candidate = candidate.trim();
                    state
                        .materials
                        .iter()
                        .any(|m| m.name.trim().eq_ignore_ascii_case(candidate))
                        || material::built_in()
                            .iter()
                            .filter(|m| state.materials.iter().all(|own| own.id != m.id))
                            .any(|m| m.name.trim().eq_ignore_ascii_case(candidate))
                };
                change.name = Some(material::numbered_name(name, shown));
                outcome.added.push(self.create_material(&change)?);
            }
        }
        Ok(())
    }

    /// The design's material of a name (any case): its own, else a built-in
    /// one it shows.
    fn design_material_named(&self, name: &str) -> Option<String> {
        let same = |other: &str| other.trim().eq_ignore_ascii_case(name.trim());
        self.state
            .materials
            .iter()
            .find(|m| same(&m.name))
            .or_else(|| {
                material::built_in()
                    .iter()
                    .filter(|m| self.state.materials.iter().all(|own| own.id != m.id))
                    .find(|m| same(&m.name))
            })
            .map(|m| m.id.clone())
    }

    /// One material into a library.
    fn import_into_library(
        &mut self,
        row: &MaterialRow,
        conflict: NameConflict,
        library: LibraryKind,
        project: Option<&Path>,
        outcome: &mut MaterialImport,
    ) -> Result<(), ModelError> {
        let name = row.name.trim();
        // Read again for each: the items imported before are there now.
        let libraries = self.print_libraries(project);
        let existing = libraries
            .items
            .iter()
            .find(|i| {
                i.library == library
                    && i.data.kind() == ItemKind::Material
                    && i.data.name().trim().eq_ignore_ascii_case(name)
            })
            .map(|i| i.data.id().to_owned());
        let mut change = row.change();
        match (existing, conflict) {
            (Some(_), NameConflict::Keep | NameConflict::Ask) => outcome.kept.push(name.to_owned()),
            (Some(id), NameConflict::Replace) => {
                change.name = None;
                self.edit_library_material(&libraries, library, &id, &change)?;
                outcome.replaced.push(id);
            }
            (existing, _) => {
                if row.density.is_none() {
                    return Err(invalid("a new material needs its density"));
                }
                if existing.is_some() {
                    let names: Vec<String> = libraries
                        .items
                        .iter()
                        .filter(|i| i.library == library && i.data.kind() == ItemKind::Material)
                        .map(|i| i.data.name().trim().to_lowercase())
                        .collect();
                    change.name = Some(material::numbered_name(name, |candidate| {
                        names.contains(&candidate.to_lowercase())
                    }));
                }
                let id = self
                    .create_library_material(&libraries, library, &change)?
                    .id;
                outcome.added.push(id);
            }
        }
        Ok(())
    }
}
