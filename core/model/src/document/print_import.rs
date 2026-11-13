// SPDX-License-Identifier: MIT
//! Importing a slicer's print settings (mitcad#640, part 5,
//! [`crate::print_files`]): a 3MF project's, a preset's or a bundle's
//! printer, process and filaments into the design (as profiles and
//! materials whose origin is the file; the design's root chooses the
//! printer and the process, as values of the source `3mf`) or into the
//! user's or the project's library.

use std::path::Path;

use serde_json::{Value, json};

use super::print_settings::hex_color;
use super::{Document, ModelError, invalid};
use crate::ids::ComponentUid;
use crate::kernel::Kernel;
use crate::material::MaterialOrigin;
use crate::print_files::installed::{self, PresetEnv};
use crate::print_files::{self, Imported};
use crate::print_keys::Dialect;
use crate::print_library::{ItemData, ItemKind, LibraryKind, item_path};
use crate::print_profile::ProfileKind;
use crate::properties::{PropertyOwner, Source};

/// The library an import's copies name as their origin.
pub const IMPORT_ORIGIN: &str = "3mf";

/// Reads a slicer's settings file. A panic of the reader (of a 3MF
/// package, a preset or a bundle) is its error (mitcad#922): it runs in
/// the application's process. A Bambu-family preset that inherits from
/// another (`inherits`) is read with every value of its chain filled in
/// (mitcad#1053): from the slicer data folder it lies in, else from the
/// installed slicers' system presets ([`installed::resolve_preset_file`]).
pub fn read_print_file(path: &str, dialect: Option<Dialect>) -> Result<Imported, ModelError> {
    let data = std::fs::read(path).map_err(|e| invalid(format!("cannot read {path}: {e}")))?;
    let name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);
    crate::reader_guard::catch_panic(
        path,
        || {
            let resolved = if inherits_another(name, &data) {
                let folders = installed::slicer_folders(&PresetEnv::current());
                installed::resolve_preset_file(Path::new(path), &folders, dialect)
                    .map_err(|e| invalid(format!("{name}: {e}")))?
            } else {
                None
            };
            let Some(resolved) = resolved else {
                return print_files::read_file(name, &data, dialect)
                    .map_err(|e| invalid(format!("{name}: {e}")));
            };
            let text = Value::Object(resolved.map).to_string();
            let mut imported = print_files::read_file(name, text.as_bytes(), dialect)
                .map_err(|e| invalid(format!("{name}: {e}")))?;
            for note in &mut imported.report.filaments {
                note.inherits.clone_from(&resolved.chain);
                note.missing.clone_from(&resolved.missing);
            }
            Ok(imported)
        },
        ModelError::Invalid,
    )
}

/// Whether a file is a Bambu-family preset (`.json`) that names a preset
/// it inherits from.
fn inherits_another(name: &str, data: &[u8]) -> bool {
    if !name.to_ascii_lowercase().ends_with(".json") {
        return false;
    }
    serde_json::from_slice::<serde_json::Map<String, Value>>(data).is_ok_and(|map| {
        map.get("inherits")
            .and_then(Value::as_str)
            .is_some_and(|base| !base.trim().is_empty())
    })
}

impl<K: Kernel> Document<K> {
    /// Imports a slicer's print settings from `path`: into the design (one
    /// undo step; `choose`: the root chooses the printer and the process
    /// read), or into the user's or the project's library (`into`; the
    /// design does not change). The answer lists what was made and the
    /// import's report.
    pub fn import_print_settings(
        &mut self,
        path: &str,
        dialect: Option<Dialect>,
        into: Option<LibraryKind>,
        project: Option<&Path>,
        choose: bool,
    ) -> Result<Value, ModelError> {
        self.import_print_settings_read(
            path,
            read_print_file(path, dialect)?,
            into,
            project,
            choose,
        )
    }

    /// Imports already read settings. The caller controls all source reads;
    /// passing no library target changes only the document.
    pub fn import_print_settings_read(
        &mut self,
        path: &str,
        mut imported: Imported,
        into: Option<LibraryKind>,
        project: Option<&Path>,
        choose: bool,
    ) -> Result<Value, ModelError> {
        // One filament's presets (a bundle's) as one material with a
        // variant per printer and nozzle (mitcad#1054).
        imported.merge_filaments(None);
        if imported.printers.is_empty()
            && imported.processes.is_empty()
            && imported.materials.is_empty()
        {
            return Err(invalid(format!("{path} has no print settings")));
        }
        let file = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
            .to_owned();
        let report = imported.report.to_json();
        let dialect_id = imported.dialect.map(|d| d.id());
        match into {
            Some(LibraryKind::Base) => Err(invalid(
                "the base library is MitCad's component library: import into the user's or the project's",
            )),
            Some(library) => {
                let libraries = self.print_libraries(project);
                let folder = libraries
                    .folder(library)
                    .ok_or_else(|| {
                        invalid(match library {
                            LibraryKind::Project => "the design is in no project",
                            _ => "there is no user library folder",
                        })
                    })?
                    .to_path_buf();
                let mut items: Vec<ItemData> = Vec::new();
                items.extend(imported.printers.into_iter().map(ItemData::Profile));
                items.extend(imported.processes.into_iter().map(ItemData::Profile));
                items.extend(
                    imported
                        .materials
                        .into_iter()
                        .map(|(material, appearance)| ItemData::Material {
                            material,
                            appearances: appearance.into_iter().collect(),
                        }),
                );
                let mut written = Vec::new();
                for mut item in items {
                    let kind = item.kind();
                    let version = libraries
                        .find(Some(library), kind, item.id())
                        .map_or(0, |i| i.data.version())
                        + 1;
                    match &mut item {
                        ItemData::Material { material, .. } => material.version = Some(version),
                        ItemData::Profile(profile) => profile.version = Some(version),
                    }
                    let target = folder.join(item_path(kind, item.id()));
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| {
                            invalid(format!("cannot make {}: {e}", parent.display()))
                        })?;
                    }
                    std::fs::write(&target, item.text())
                        .map_err(|e| invalid(format!("cannot write {}: {e}", target.display())))?;
                    written.push(json!({
                        "kind": kind.id(),
                        "id": item.id(),
                        "name": item.name(),
                        "version": version,
                        "path": target.to_string_lossy(),
                    }));
                }
                Ok(json!({
                    "dialect": dialect_id,
                    "into": library.id(),
                    "items": written,
                    "report": report,
                }))
            }
            None => {
                let origin = MaterialOrigin {
                    library: IMPORT_ORIGIN.to_owned(),
                    id: file.clone(),
                    version: None,
                    declined: None,
                };
                let label = format!("Import Print Settings {file}");
                let made = self.apply_with(move |state| {
                    let mut printers = Vec::new();
                    for mut profile in imported.printers {
                        profile.origin = Some(origin.clone());
                        printers.push(state.add_print_profile(profile));
                    }
                    let mut processes = Vec::new();
                    for mut profile in imported.processes {
                        profile.origin = Some(origin.clone());
                        processes.push(state.add_print_profile(profile));
                    }
                    let mut materials = Vec::new();
                    for (mut material, appearance) in imported.materials {
                        material.origin = Some(origin.clone());
                        materials.push(state.put_material(material, appearance.as_slice(), false));
                    }
                    // The project's purge table: its materials' pairs set by
                    // hand (mitcad#705).
                    for (from, to, volume) in imported.purge {
                        if let (Some(from), Some(to)) = (materials.get(from), materials.get(to))
                            && from != to
                        {
                            state.set_purge_override(from, to, Some(volume));
                        }
                    }
                    let root = PropertyOwner::Component(ComponentUid::ROOT);
                    if choose {
                        if let Some(id) = printers.first() {
                            state.choose_print_profile(
                                root,
                                ProfileKind::Printer,
                                id,
                                Source::ThreeMf,
                            );
                        }
                        if let Some(id) = processes.first() {
                            state.choose_print_profile(
                                root,
                                ProfileKind::Process,
                                id,
                                Source::ThreeMf,
                            );
                        }
                    }
                    Ok((label, (printers, processes, materials), false))
                })?;
                let (printers, processes, materials) = made;
                let named = |ids: &[String], kind: ItemKind| -> Vec<Value> {
                    ids.iter()
                        .map(|id| {
                            let name = match kind {
                                ItemKind::Material => self.material(id).map(|m| m.name.clone()),
                                _ => self.print_profile(id).map(|p| p.name.clone()),
                            };
                            json!({"id": id, "name": name})
                        })
                        .collect()
                };
                // Each material with its colour (its default appearance's,
                // `#RRGGBB`) and the file it came from (its origin), so
                // that two filaments of one name are told apart
                // (mitcad#1728).
                let materials: Vec<Value> = materials
                    .iter()
                    .map(|id| {
                        let material = self.material(id);
                        let colour = material
                            .and_then(|m| m.appearance_list().into_iter().next())
                            .and_then(|look| self.appearance(&look))
                            .map(|look| hex_color(look.display_color()));
                        let source = material
                            .and_then(|m| m.origin.as_ref())
                            .map_or_else(|| file.clone(), |origin| origin.id.clone());
                        json!({
                            "id": id,
                            "name": material.map(|m| m.name.clone()),
                            "colour": colour,
                            "source": source,
                        })
                    })
                    .collect();
                Ok(json!({
                    "dialect": dialect_id,
                    "into": "design",
                    "printers": named(&printers, ItemKind::Printer),
                    "processes": named(&processes, ItemKind::Process),
                    "materials": materials,
                    "chosen": choose,
                    "report": report,
                }))
            }
        }
    }
}
