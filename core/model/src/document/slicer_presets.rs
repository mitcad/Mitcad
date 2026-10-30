// SPDX-License-Identifier: MIT
//! The user presets of an installed Bambu-family slicer, and the slicer's
//! own presets the user enabled there (mitcad#871), listed and imported
//! (mitcad#830; [`crate::print_files::installed`] finds and reads them):
//! each preset with the item of the same kind and name its target
//! has (the design's own, or a library's), and the import of the ticked
//! ones in one go, each with the values it inherits from the slicer's
//! system presets. A ticked preset whose name the target has either
//! overrides that item (its next version in a library; the design's item
//! in the design) or comes in as a copy under the next free name
//! (`<name> 2`). Into the design it is one undo step.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::library_items::{FileWrite, free_name, library_folder};
use super::print_import::IMPORT_ORIGIN;
use super::{Document, ModelError, invalid};
use crate::kernel::Kernel;
use crate::material::{MaterialChange, MaterialOrigin};
use crate::print_files::installed::{self, SlicerFolder};
use crate::print_files::{self, ImportReport};
use crate::print_keys::{Dialect, PrintSet};
use crate::print_library::{ItemData, ItemKind, Libraries, LibraryKind};
use crate::print_profile::{ProfileChange, free_id, slug};
use crate::properties::same_name;

/// What a ticked preset does when its target has an item of its kind and
/// name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SameName {
    /// It comes in as a new item under the next free name: nothing is lost.
    #[default]
    Copy,
    /// The existing item takes its values.
    Override,
}

/// A preset ticked for the import.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetChoice {
    pub path: PathBuf,
    pub same: SameName,
}

/// The item a target has of a kind and name: its id, name and place
/// (`design`, `user`, `project`).
#[derive(Debug, Clone, PartialEq)]
struct Existing {
    id: String,
    name: String,
    place: &'static str,
}

fn item_kind(set: PrintSet) -> ItemKind {
    match set {
        PrintSet::Printer => ItemKind::Printer,
        PrintSet::Process => ItemKind::Process,
        PrintSet::Filament => ItemKind::Material,
    }
}

fn rename(item: &mut ItemData, name: String) {
    match item {
        ItemData::Material { material, .. } => material.name = name,
        ItemData::Profile(profile) => profile.name = name,
    }
}

fn set_id(item: &mut ItemData, id: String) {
    match item {
        ItemData::Material { material, .. } => material.id = id,
        ItemData::Profile(profile) => profile.id = id,
    }
}

/// A profile's or material's slicer text as a change's values.
fn slicer_change(
    slicer: &BTreeMap<String, BTreeMap<String, String>>,
) -> BTreeMap<String, BTreeMap<String, Value>> {
    slicer
        .iter()
        .map(|(dialect, keys)| {
            let keys = keys
                .iter()
                .map(|(key, text)| (key.clone(), json!(text)))
                .collect();
            (dialect.clone(), keys)
        })
        .collect()
}

/// An item read from a preset, with the preset's file name.
struct PresetItem {
    item: ItemData,
    file: String,
    same: SameName,
}

/// What an import does: the items that override existing ones, those
/// that come in (copies renamed already), and what its answer says.
struct Plan {
    label: String,
    dialect: Dialect,
    overrides: Vec<(Existing, PresetItem)>,
    added: Vec<PresetItem>,
    copied: Vec<Value>,
    report: Value,
}

impl<K: Kernel> Document<K> {
    /// The item of `kind` and `name` the target has: the design's own
    /// (`into` None) or the library's.
    fn same_named(
        &self,
        libraries: &Libraries,
        into: Option<LibraryKind>,
        kind: ItemKind,
        name: &str,
    ) -> Option<Existing> {
        match into {
            None => match kind {
                ItemKind::Material => self
                    .state
                    .materials
                    .iter()
                    .find(|m| same_name(&m.name, name))
                    .map(|m| Existing {
                        id: m.id.clone(),
                        name: m.name.clone(),
                        place: "design",
                    }),
                _ => self
                    .state
                    .print_profiles
                    .iter()
                    .find(|p| ItemKind::of_profile(p.kind) == kind && same_name(&p.name, name))
                    .map(|p| Existing {
                        id: p.id.clone(),
                        name: p.name.clone(),
                        place: "design",
                    }),
            },
            Some(library) => libraries
                .items
                .iter()
                .find(|i| {
                    i.library == library && i.data.kind() == kind && same_name(i.data.name(), name)
                })
                .map(|i| Existing {
                    id: i.data.id().to_owned(),
                    name: i.data.name().to_owned(),
                    place: library.id(),
                }),
        }
    }

    /// The `slicer_presets` query: the user presets of each slicer data
    /// folder in `folders`, then the system presets the user enabled there
    /// (`system`, with their `vendor`), each with the item of its kind and
    /// name the target has (`same`: `into` None is the design).
    pub fn slicer_presets_json(
        &self,
        folders: &[SlicerFolder],
        into: Option<LibraryKind>,
        project: Option<&Path>,
    ) -> Value {
        let libraries = self.print_libraries(project);
        let slicers: Vec<Value> = folders
            .iter()
            .map(|slicer| {
                // The user's, then the slicer's own the user enabled
                // (mitcad#871).
                let presets: Vec<Value> = installed::user_presets(&slicer.folder)
                    .into_iter()
                    .chain(installed::system_presets(&slicer.folder, slicer.dialect))
                    .map(|preset| {
                        let kind = item_kind(preset.set);
                        let same = self
                            .same_named(&libraries, into, kind, &preset.name)
                            .map(|e| json!({"id": e.id, "name": e.name, "place": e.place}));
                        let system = preset.vendor.is_some();
                        let account = (!system).then_some(preset.account);
                        json!({
                            "kind": kind.id(),
                            "name": preset.name,
                            "path": preset.path.to_string_lossy(),
                            "account": account,
                            "inherits": preset.inherits,
                            "system": system,
                            "vendor": preset.vendor,
                            "same": same,
                        })
                    })
                    .collect();
                json!({
                    "dialect": slicer.dialect.id(),
                    "name": slicer.dialect.name(),
                    "folder": slicer.folder.to_string_lossy(),
                    "flatpak": slicer.flatpak,
                    "presets": presets,
                })
            })
            .collect();
        json!({"slicers": slicers})
    }

    /// Imports the presets `presets` (user or system ones) of the slicer
    /// `dialect` whose data folder is `folder` (whose system presets fill
    /// in what they inherit) into the design (`into` None: one undo step) or the user's
    /// or the project's library. A preset whose name the target has
    /// follows its choice ([`SameName`]); two of one name in one import
    /// make the second a copy. The answer lists what came in and what was
    /// overridden, as `import_print_profile`'s, with `overridden` and
    /// `copied`.
    pub fn import_slicer_presets(
        &mut self,
        dialect: Dialect,
        folder: &Path,
        presets: &[PresetChoice],
        into: Option<LibraryKind>,
        project: Option<&Path>,
    ) -> Result<Value, ModelError> {
        if !dialect.bambu_family() {
            return Err(invalid(format!(
                "{} keeps no presets Mitcad reads from its folder",
                dialect.name()
            )));
        }
        if presets.is_empty() {
            return Err(invalid("no presets to import"));
        }
        let mut report = ImportReport::default();
        let mut read = Vec::new();
        for choice in presets {
            let file = choice
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            // The installed preset (and those it inherits from) read in the
            // application's process: a panic of the reader is its error
            // (mitcad#922), not an abort of the application.
            let imported = crate::reader_guard::catch_panic(
                &choice.path.to_string_lossy(),
                || {
                    let map = installed::resolved_preset(folder, &choice.path).map_err(invalid)?;
                    let text = Value::Object(map).to_string();
                    print_files::read_file(&file, text.as_bytes(), Some(dialect))
                        .map_err(|e| invalid(format!("{file}: {e}")))
                },
                ModelError::Invalid,
            )?;
            report.mapped.extend(imported.report.mapped);
            report.kept.extend(imported.report.kept);
            report.left_out.extend(imported.report.left_out);
            report.problems.extend(imported.report.problems);
            let items = imported
                .printers
                .into_iter()
                .chain(imported.processes)
                .map(ItemData::Profile)
                .chain(
                    imported
                        .materials
                        .into_iter()
                        .map(|(material, appearance)| ItemData::Material {
                            material,
                            appearance,
                        }),
                );
            for item in items {
                read.push(PresetItem {
                    item,
                    file: file.clone(),
                    same: choice.same,
                });
            }
        }
        let libraries = self.print_libraries(project);
        // Which override an existing item, which come in (copies renamed).
        let mut overrides: Vec<(Existing, PresetItem)> = Vec::new();
        let mut added: Vec<PresetItem> = Vec::new();
        let mut copied = Vec::new();
        let mut batch: Vec<(ItemKind, String)> = Vec::new();
        for mut entry in read {
            let kind = entry.item.kind();
            let name = entry.item.name().to_owned();
            let existing = self.same_named(&libraries, into, kind, &name);
            let overridden_already = existing.as_ref().is_some_and(|e| {
                overrides
                    .iter()
                    .any(|(o, r)| r.item.kind() == kind && o.id == e.id)
            });
            if let Some(existing) = existing.clone()
                && entry.same == SameName::Override
                && !overridden_already
            {
                batch.push((kind, name));
                overrides.push((existing, entry));
                continue;
            }
            let in_batch = |n: &str| batch.iter().any(|(k, b)| *k == kind && same_name(b, n));
            if existing.is_some() || in_batch(&name) {
                let taken =
                    |n: &str| in_batch(n) || self.same_named(&libraries, into, kind, n).is_some();
                let free = free_name(&name, taken);
                copied.push(json!({"kind": kind.id(), "name": free, "from": name}));
                rename(&mut entry.item, free);
            }
            batch.push((kind, entry.item.name().to_owned()));
            added.push(entry);
        }
        let plan = Plan {
            label: format!("Import Presets of {}", dialect.name()),
            dialect,
            overrides,
            added,
            copied,
            report: report.to_json(),
        };
        match into {
            None => self.import_presets_into_design(plan),
            Some(library) => self.import_presets_into_library(plan, &libraries, library),
        }
    }

    fn import_presets_into_design(&mut self, plan: Plan) -> Result<Value, ModelError> {
        let Plan {
            label,
            dialect,
            overrides,
            added,
            copied,
            report,
        } = plan;
        let made = self.as_one_step(move |doc| {
            let mut overridden = Vec::new();
            let mut listed: Vec<(ItemKind, String)> = Vec::new();
            for (existing, entry) in overrides {
                let kind = entry.item.kind();
                match entry.item {
                    ItemData::Profile(profile) => {
                        doc.edit_print_profile(
                            &existing.id,
                            &ProfileChange {
                                values: Some(profile.values),
                                slicer: Some(slicer_change(&profile.slicer)),
                                ..ProfileChange::default()
                            },
                        )?;
                    }
                    ItemData::Material { material, .. } => {
                        doc.edit_material(
                            &existing.id,
                            &MaterialChange {
                                density: Some(material.density),
                                filament: Some(material.filament),
                                ..MaterialChange::default()
                            },
                        )?;
                    }
                }
                overridden
                    .push(json!({"kind": kind.id(), "id": existing.id, "name": existing.name}));
                listed.push((kind, existing.id));
            }
            // The new ones last: the step is named as the import.
            let ids = doc.apply_with(move |state| {
                let mut ids = Vec::new();
                for entry in added {
                    let origin = MaterialOrigin {
                        library: IMPORT_ORIGIN.to_owned(),
                        id: entry.file,
                        version: None,
                        declined: None,
                    };
                    let kind = entry.item.kind();
                    let id = match entry.item {
                        ItemData::Profile(mut profile) => {
                            profile.origin = Some(origin);
                            state.add_print_profile(profile)
                        }
                        ItemData::Material {
                            mut material,
                            appearance,
                        } => {
                            material.origin = Some(origin);
                            state.put_material(material, appearance.as_ref(), false)
                        }
                    };
                    ids.push((kind, id));
                }
                Ok((label, ids, false))
            })?;
            listed.extend(ids);
            Ok((listed, overridden))
        })?;
        let (listed, overridden) = made;
        let named = |kind: ItemKind| -> Vec<Value> {
            listed
                .iter()
                .filter(|(k, _)| *k == kind)
                .map(|(_, id)| {
                    let name = match kind {
                        ItemKind::Material => self.material(id).map(|m| m.name.clone()),
                        _ => self.print_profile(id).map(|p| p.name.clone()),
                    };
                    json!({"id": id, "name": name})
                })
                .collect()
        };
        Ok(json!({
            "dialect": dialect.id(),
            "into": "design",
            "printers": named(ItemKind::Printer),
            "processes": named(ItemKind::Process),
            "materials": named(ItemKind::Material),
            "overridden": overridden,
            "copied": copied,
            "report": report,
        }))
    }

    fn import_presets_into_library(
        &mut self,
        plan: Plan,
        libraries: &Libraries,
        library: LibraryKind,
    ) -> Result<Value, ModelError> {
        let Plan {
            label,
            dialect,
            overrides,
            added,
            copied,
            report,
        } = plan;
        let folder = library_folder(libraries, library)?;
        // New items under ids no library has for their kind, version 1.
        let mut taken: Vec<(ItemKind, String)> = libraries
            .items
            .iter()
            .map(|i| (i.data.kind(), i.data.id().to_owned()))
            .collect();
        let mut files = Vec::new();
        let mut items = Vec::new();
        for entry in added {
            let mut item = entry.item;
            let kind = item.kind();
            let base = match &item {
                ItemData::Profile(profile) => slug(&profile.name),
                ItemData::Material { material, .. } => material.id.clone(),
            };
            let id = free_id(&base, |id| taken.iter().any(|(k, t)| *k == kind && t == id));
            taken.push((kind, id.clone()));
            set_id(&mut item, id);
            item.set_version(1);
            let file = FileWrite::item(&folder, &item);
            items.push(json!({
                "kind": kind.id(),
                "id": item.id(),
                "name": item.name(),
                "version": 1,
                "path": file.path().to_string_lossy(),
            }));
            files.push(file);
        }
        let overridden = self.as_one_step(|doc| {
            let mut overridden = Vec::new();
            for (existing, entry) in overrides {
                let kind = entry.item.kind();
                let (path, version) = match entry.item {
                    ItemData::Profile(profile) => doc.edit_library_profile(
                        libraries,
                        library,
                        &existing.id,
                        &ProfileChange {
                            kind: Some(profile.kind),
                            values: Some(profile.values),
                            slicer: Some(slicer_change(&profile.slicer)),
                            ..ProfileChange::default()
                        },
                    )?,
                    ItemData::Material { material, .. } => doc.edit_library_material(
                        libraries,
                        library,
                        &existing.id,
                        &MaterialChange {
                            density: Some(material.density),
                            filament: Some(material.filament),
                            ..MaterialChange::default()
                        },
                    )?,
                };
                overridden.push(json!({
                    "kind": kind.id(),
                    "id": existing.id,
                    "name": existing.name,
                    "version": version,
                    "path": path.to_string_lossy(),
                }));
            }
            // The new files last: the step is named as the import.
            doc.with_files(label, files, |_| Ok(()))?;
            Ok(overridden)
        })?;
        items.extend(overridden.iter().cloned());
        Ok(json!({
            "dialect": dialect.id(),
            "into": library.id(),
            "items": items,
            "overridden": overridden,
            "copied": copied,
            "report": report,
        }))
    }
}
