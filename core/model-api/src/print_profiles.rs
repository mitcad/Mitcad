// SPDX-License-Identifier: MIT
//! Commands and queries of print profiles and print settings (mitcad#640,
//! `document/print_settings.rs`): the design's printer and process
//! profiles, what a body, occurrence or component prints with, and the
//! design printer's build volume. See `commands.md`, "Print profiles".

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::ids::ComponentUid;
use mitcad_model::internal::document::writable_library;
use mitcad_model::kernel::Kernel;
use mitcad_model::print_files::Imported;
use mitcad_model::print_files::filaments::PrinterChoice;
use mitcad_model::print_files::installed::{self, PresetEnv, SlicerFolder};
use mitcad_model::print_keys::Dialect;
use mitcad_model::print_library::{ItemKind, LibraryKind};
use mitcad_model::print_profile::{ProfileChange, ProfileKind};
use mitcad_model::properties::PropertyOwner;
use mitcad_model::{Document, ModelError, PresetChoice, PrintTarget, SameName};

/// Supplies all files consumed by a print import, including inherited presets.
/// External clients can provide a confined reader instead of installed discovery.
pub trait PrintImportReader {
    fn read_print_file(&self, path: &str, dialect: Option<Dialect>)
    -> Result<Imported, ModelError>;
    fn resolve_preset(
        &self,
        folder: &Path,
        path: &Path,
    ) -> Result<installed::ResolvedPreset, ModelError>;
}

/// Validates the confined import subset without changing a document.
pub fn validate_print_import_command(value: &Value) -> Result<(), ApiError> {
    let command: PrintProfileCommand = super::parse(value.clone(), "command")?;
    let (into, project) = match &command {
        PrintProfileCommand::ImportPrintProfile { into, project, .. }
        | PrintProfileCommand::ImportSlicerPresets { into, project, .. } => (into, project),
        _ => return Err(ApiError("not a print import command".to_owned())),
    };
    if !matches!(into.as_deref(), None | Some("design")) || project.is_some() {
        return Err(ApiError("MCP print imports target the design only; library targets and project paths are refused".to_owned()));
    }
    Ok(())
}

/// The ordinary print-import commands with a caller-controlled source reader.
pub trait PrintImportApi {
    fn command_with_print_reader(
        &mut self,
        json: &str,
        reader: &dyn PrintImportReader,
    ) -> Result<String, ApiError>;
}

impl<K: Kernel> PrintImportApi for Document<K> {
    fn command_with_print_reader(
        &mut self,
        text: &str,
        reader: &dyn PrintImportReader,
    ) -> Result<String, ApiError> {
        use super::DocumentLocal;
        let value = super::read_json(text)?;
        validate_print_import_command(&value)?;
        let result = self.with_policy_checks(|doc| {
            doc.run_checked_action(value, |doc, value| {
                doc.run_print_profile_command_with_reader(
                    super::parse(value, "command")?,
                    Some(reader),
                )
            })
        })?;
        self.settle_comments_now();
        Ok(result.to_string())
    }
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum PrintProfileCommand {
        "create_print_profile" => CreatePrintProfile(ProfileChange),
        "edit_print_profile" => EditPrintProfile(ProfileChange),
        "delete_print_profile" => DeletePrintProfile {
            id: String,
            /// `user` or `project`: the item is deleted from that library
            /// (mitcad#655).
            #[serde(default)]
            library: Option<String>,
            /// `printer` or `process`, when the library has both of the id.
            #[serde(default)]
            kind: Option<String>,
            #[serde(default)]
            project: Option<String>,
        },
        // The libraries (part 4).
        "copy_from_print_library" => CopyFromPrintLibrary {
            /// `base`, `user` or `project`; the nearest when left out.
            #[serde(default)]
            library: Option<String>,
            /// `material`, `printer` or `process`.
            kind: String,
            id: String,
            /// The project's folder, when the design is in one.
            #[serde(default)]
            project: Option<String>,
        },
        "update_from_print_library" => UpdateFromPrintLibrary {
            kind: String,
            /// The design's copy.
            id: String,
            #[serde(default)]
            project: Option<String>,
        },
        /// Keeps the design's copy over its library's newer version
        /// (mitcad#655).
        "decline_print_library_update" => DeclinePrintLibraryUpdate {
            kind: String,
            /// The design's copy.
            id: String,
            #[serde(default)]
            project: Option<String>,
        },
        "save_to_print_library" => SaveToPrintLibrary {
            /// `user` or `project`.
            library: String,
            kind: String,
            id: String,
            #[serde(default)]
            project: Option<String>,
        },
        // Slicers' files (part 5).
        "import_print_profile" => ImportPrintProfile {
            /// A 3MF project, a preset or bundle, a PrusaSlicer .ini.
            path: String,
            /// The Bambu family's member when the file does not say.
            #[serde(default)]
            dialect: Option<String>,
            /// `design` (the default), `user` or `project`.
            #[serde(default)]
            into: Option<String>,
            #[serde(default)]
            project: Option<String>,
            /// Into the design: the root chooses the printer and process
            /// read (the default).
            #[serde(default)]
            choose: Option<bool>,
        },
        // An installed slicer's user presets (mitcad#830).
        "import_slicer_presets" => ImportSlicerPresets {
            /// The Bambu family's member: `bambu_studio`, `orcaslicer`,
            /// `qidi_studio`.
            dialect: String,
            /// The slicer's data folder (`slicer_presets`' `folder`): its
            /// system presets fill in what the presets inherit.
            folder: String,
            /// The presets ticked.
            presets: Vec<PresetArg>,
            /// `design` (the default), `user` or `project`.
            #[serde(default)]
            into: Option<String>,
            #[serde(default)]
            project: Option<String>,
            /// The printers and nozzles in use (mitcad#1054): only the
            /// filament presets and variants of these come in; all when
            /// left out.
            #[serde(default)]
            printers: Option<Vec<PrinterArg>>,
            /// Merges the target's separate materials of one filament
            /// first (mitcad#1054, `slicer_presets`' `merge`).
            #[serde(default)]
            merge_separate: Option<bool>,
        },
    }
}

/// A printer and nozzle of `import_slicer_presets`' `printers`
/// (mitcad#1054).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrinterArg {
    printer: String,
    /// The nozzle diameter in mm; null or left out: any.
    #[serde(default)]
    nozzle: Option<f64>,
}

/// A preset of `import_slicer_presets` (mitcad#830).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PresetArg {
    /// The preset's file (`slicer_presets`' `path`).
    path: String,
    /// When the target has an item of its kind and name: `copy` (the
    /// default: a new item, `<name> 2`) or `override` (that item takes its
    /// values).
    #[serde(default)]
    same_name: Option<String>,
}

/// A slicer data folder given to `slicer_presets` in place of the
/// installed ones (mitcad#830).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SlicerFolderArg {
    /// `bambu_studio`, `orcaslicer` or `qidi_studio`.
    dialect: String,
    path: String,
}

/// The `slicer_presets` query (mitcad#830).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SlicerPresetsQuery {
    /// `design` (the default), `user` or `project`: the target whose items
    /// of a preset's kind and name are named (`same`).
    #[serde(default)]
    into: Option<String>,
    /// The project's folder, when the design is in one.
    #[serde(default)]
    project: Option<String>,
    /// The slicers' data folders to read instead of those of the slicers
    /// installed (tests, another computer's folders).
    #[serde(default)]
    folders: Option<Vec<SlicerFolderArg>>,
}

/// A Bambu-family slicer by its dialect id.
fn bambu_dialect(id: &str) -> Result<Dialect, ApiError> {
    Dialect::from_id(id)
        .filter(|d| d.bambu_family())
        .ok_or_else(|| {
            ApiError(format!(
                "{id} is no slicer whose presets MitCad reads: bambu_studio, orcaslicer or qidi_studio"
            ))
        })
}

/// An import's target: None for the design.
fn import_target(into: Option<&str>) -> Result<Option<LibraryKind>, ApiError> {
    match into {
        None | Some("design") => Ok(None),
        Some(text) => library_kind(text).map(Some),
    }
}

/// The `print_library` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrintLibraryQuery {
    /// The project's folder, when the design is in one.
    #[serde(default)]
    project: Option<String>,
}

/// The `library_items` query (mitcad#655).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LibraryItemsQuery {
    /// `material`, `printer` or `process`.
    kind: String,
    /// The project's folder, when the design is in one.
    #[serde(default)]
    project: Option<String>,
    /// Each item where it lives, a design's copy and its library item as
    /// two rows (mitcad#1135: the print managers' list model).
    #[serde(default)]
    by_place: bool,
}

fn item_kind(text: &str) -> Result<ItemKind, ApiError> {
    ItemKind::parse(text).ok_or_else(|| {
        ApiError(format!(
            "“{text}” is no kind of library item: material, printer or process"
        ))
    })
}

fn library_kind(text: &str) -> Result<LibraryKind, ApiError> {
    LibraryKind::parse(text)
        .ok_or_else(|| ApiError(format!("“{text}” is no library: base, user or project")))
}

/// The `print_settings` query: a body (in an occurrence path), an
/// occurrence, a component, or nothing (the design's own).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrintSettingsQuery {
    /// A body by uid or name.
    #[serde(default)]
    body: Option<String>,
    /// The occurrence path (`O1/O4`, or names): of the body, or the
    /// occurrence asked for.
    #[serde(default)]
    occurrence: Option<String>,
    /// A component by uid (`C0` the root) or name.
    #[serde(default)]
    component: Option<String>,
    /// A print plate by id or name (mitcad#1050): the settings on it, the
    /// plate a level between the components and the design; left out, a
    /// body's or an occurrence's own plate.
    #[serde(default)]
    plate: Option<String>,
}

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait PrintProfilesCrateOps<K: Kernel> {
    fn run_print_profile_command(
        &mut self,
        command: PrintProfileCommand,
    ) -> Result<Value, ApiError>;

    fn run_print_profile_command_with_reader(
        &mut self,
        command: PrintProfileCommand,
        reader: Option<&dyn PrintImportReader>,
    ) -> Result<Value, ApiError>;

    /// What a `print_settings` query asks for.
    fn print_target(&self, query: &PrintSettingsQuery) -> Result<PrintTarget, ApiError>;
}

impl<K: Kernel> PrintProfilesCrateOps<K> for Document<K> {
    fn run_print_profile_command(
        &mut self,
        command: PrintProfileCommand,
    ) -> Result<Value, ApiError> {
        self.run_print_profile_command_with_reader(command, None)
    }

    fn run_print_profile_command_with_reader(
        &mut self,
        command: PrintProfileCommand,
        reader: Option<&dyn PrintImportReader>,
    ) -> Result<Value, ApiError> {
        Ok(match command {
            PrintProfileCommand::CreatePrintProfile(change) => match change.library.as_deref() {
                Some(library) => {
                    let library = writable_library(library)?;
                    let libraries = self.print_libraries(change.project.as_deref().map(Path::new));
                    let (id, path, version) =
                        self.create_library_profile(&libraries, library, &change)?;
                    json!({"id": id, "library": library.id(), "version": version,
                           "path": path.to_string_lossy()})
                }
                None => json!({"id": self.create_print_profile(&change)?}),
            },
            PrintProfileCommand::EditPrintProfile(change) => {
                let id = change.id.clone().ok_or_else(|| {
                    ApiError("edit_print_profile needs the profile's id".to_owned())
                })?;
                match change.library.as_deref() {
                    Some(library) => {
                        let library = writable_library(library)?;
                        let libraries =
                            self.print_libraries(change.project.as_deref().map(Path::new));
                        let (path, version) =
                            self.edit_library_profile(&libraries, library, &id, &change)?;
                        json!({"version": version, "path": path.to_string_lossy()})
                    }
                    None => {
                        self.edit_print_profile(&id, &change)?;
                        json!({})
                    }
                }
            }
            PrintProfileCommand::DeletePrintProfile {
                id,
                library,
                kind,
                project,
            } => match library.as_deref() {
                Some(library) => {
                    let library = writable_library(library)?;
                    let kind = kind
                        .as_deref()
                        .map(|text| {
                            ProfileKind::parse(text).ok_or_else(|| {
                                ApiError(format!(
                                    "“{text}” is no kind of profile: printer or process"
                                ))
                            })
                        })
                        .transpose()?;
                    let libraries = self.print_libraries(project.as_deref().map(Path::new));
                    let (path, kept, removed) =
                        self.delete_library_profile(&libraries, library, kind, &id)?;
                    json!({"path": path.to_string_lossy(), "kept": kept, "removed": removed})
                }
                None => json!({"owners": self.delete_print_profile(&id)?}),
            },
            PrintProfileCommand::CopyFromPrintLibrary {
                library,
                kind,
                id,
                project,
            } => {
                let library = library.as_deref().map(library_kind).transpose()?;
                let kind = item_kind(&kind)?;
                let libraries = self.print_libraries(project.as_deref().map(Path::new));
                let item = libraries.find(library, kind, &id).ok_or_else(|| {
                    ApiError(format!(
                        "{} has no {} {id}",
                        library.map_or("no library".to_owned(), |l| format!(
                            "the {} library",
                            l.id()
                        )),
                        kind.id()
                    ))
                })?;
                let copied = self.copy_from_print_library(item)?;
                json!({"id": copied, "library": item.library.id(), "version": item.data.version()})
            }
            PrintProfileCommand::UpdateFromPrintLibrary { kind, id, project } => {
                let kind = item_kind(&kind)?;
                let libraries = self.print_libraries(project.as_deref().map(Path::new));
                json!({"version": self.update_from_print_library(&libraries, kind, &id)?})
            }
            PrintProfileCommand::DeclinePrintLibraryUpdate { kind, id, project } => {
                let kind = item_kind(&kind)?;
                let libraries = self.print_libraries(project.as_deref().map(Path::new));
                json!({"version": self.decline_print_library_update(&libraries, kind, &id)?})
            }
            PrintProfileCommand::SaveToPrintLibrary {
                library,
                kind,
                id,
                project,
            } => {
                let library = library_kind(&library)?;
                let kind = item_kind(&kind)?;
                let libraries = self.print_libraries(project.as_deref().map(Path::new));
                let (path, version) = self.save_to_print_library(&libraries, library, kind, &id)?;
                json!({"path": path.to_string_lossy(), "version": version})
            }
            PrintProfileCommand::ImportPrintProfile {
                path,
                dialect,
                into,
                project,
                choose,
            } => {
                let dialect = dialect
                    .as_deref()
                    .map(|id| {
                        Dialect::from_id(id).ok_or_else(|| {
                            ApiError(format!(
                                "{id} is no slicer: bambu_studio, orcaslicer, qidi_studio, prusaslicer or cura"
                            ))
                        })
                    })
                    .transpose()?;
                let into = match into.as_deref() {
                    None | Some("design") => None,
                    Some(text) => Some(library_kind(text)?),
                };
                match reader {
                    Some(reader) => self.import_print_settings_read(
                        &path,
                        reader.read_print_file(&path, dialect)?,
                        None,
                        None,
                        choose.unwrap_or(true),
                    )?,
                    None => self.import_print_settings(
                        &path,
                        dialect,
                        into,
                        project.as_deref().map(Path::new),
                        choose.unwrap_or(true),
                    )?,
                }
            }
            PrintProfileCommand::ImportSlicerPresets {
                dialect,
                folder,
                presets,
                into,
                project,
                printers,
                merge_separate,
            } => {
                let printers: Option<Vec<PrinterChoice>> = printers.map(|chosen| {
                    chosen
                        .into_iter()
                        .map(|p| PrinterChoice {
                            printer: p.printer,
                            nozzle: p.nozzle,
                        })
                        .collect()
                });
                let dialect = bambu_dialect(&dialect)?;
                let into = import_target(into.as_deref())?;
                let choices = presets
                    .into_iter()
                    .map(|preset| {
                        let same = match preset.same_name.as_deref() {
                            None | Some("copy") => SameName::Copy,
                            Some("override") => SameName::Override,
                            Some(other) => {
                                return Err(ApiError(format!(
                                    "“{other}” is no choice for a preset of a name the target has: copy or override"
                                )));
                            }
                        };
                        Ok(PresetChoice {
                            path: PathBuf::from(preset.path),
                            same,
                        })
                    })
                    .collect::<Result<Vec<_>, ApiError>>()?;
                match reader {
                    Some(reader) => self.import_slicer_presets_with_reader(
                        dialect,
                        &choices,
                        None,
                        None,
                        printers.as_deref(),
                        merge_separate.unwrap_or(false),
                        |path| reader.resolve_preset(Path::new(&folder), path),
                    )?,
                    None => self.import_slicer_presets(
                        dialect,
                        Path::new(&folder),
                        &choices,
                        into,
                        project.as_deref().map(Path::new),
                        printers.as_deref(),
                        merge_separate.unwrap_or(false),
                    )?,
                }
            }
        })
    }

    fn print_target(&self, query: &PrintSettingsQuery) -> Result<PrintTarget, ApiError> {
        let path = match &query.occurrence {
            Some(text) => Some(
                self.assembly()
                    .find_path(text)
                    .ok_or_else(|| ApiError(format!("no occurrence path {text}")))?,
            ),
            None => None,
        };
        if let Some(text) = &query.body {
            let body = match self.property_owner_ref(text)? {
                PropertyOwner::Body(b) => b,
                _ => return Err(ApiError(format!("“{text}” is no body"))),
            };
            return Ok(PrintTarget::Body(body, path));
        }
        if let Some(path) = path {
            return Ok(PrintTarget::Occurrence(path));
        }
        Ok(PrintTarget::Component(match &query.component {
            Some(text) => self.component_ref(text)?,
            None => ComponentUid::ROOT,
        }))
    }
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait PrintProfilesOps<K: Kernel> {
    fn print_library_query(&self, query: &PrintLibraryQuery) -> Value;

    fn library_items_query(&self, query: &LibraryItemsQuery) -> Result<Value, ApiError>;

    fn print_settings_query(&self, query: &PrintSettingsQuery) -> Result<Value, ApiError>;

    fn slicer_presets_query(&self, query: &SlicerPresetsQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> PrintProfilesOps<K> for Document<K> {
    fn print_library_query(&self, query: &PrintLibraryQuery) -> Value {
        let libraries = self.print_libraries(query.project.as_deref().map(Path::new));
        self.print_library_json(&libraries)
    }

    fn library_items_query(&self, query: &LibraryItemsQuery) -> Result<Value, ApiError> {
        let kind = item_kind(&query.kind)?;
        let libraries = self.print_libraries(query.project.as_deref().map(Path::new));
        if query.by_place {
            return Ok(self.library_items_by_place_json(&libraries, kind));
        }
        Ok(self.library_items_json(&libraries, kind))
    }

    fn print_settings_query(&self, query: &PrintSettingsQuery) -> Result<Value, ApiError> {
        let target = self.print_target(query)?;
        // On a print plate given (mitcad#1050), else the target's own.
        if let Some(plate) = &query.plate {
            let index = self.plate_index(plate)?;
            let plate = &self.print_plates()[index];
            return Ok(self.print_settings_json_on(&target, Some(plate)));
        }
        Ok(self.print_settings_json(&target))
    }

    fn slicer_presets_query(&self, query: &SlicerPresetsQuery) -> Result<Value, ApiError> {
        let into = import_target(query.into.as_deref())?;
        // The installed slicers' folders, unless the query names some.
        let folders = match &query.folders {
            Some(given) => given
                .iter()
                .map(|folder| {
                    Ok(SlicerFolder {
                        dialect: bambu_dialect(&folder.dialect)?,
                        folder: PathBuf::from(&folder.path),
                        flatpak: false,
                    })
                })
                .collect::<Result<Vec<_>, ApiError>>()?,
            None => installed::slicer_folders(&PresetEnv::current()),
        };
        Ok(self.slicer_presets_json(&folders, into, query.project.as_deref().map(Path::new)))
    }
}
