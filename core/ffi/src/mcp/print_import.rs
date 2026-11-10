// SPDX-License-Identifier: MIT
//! Headless print imports: snapshot every source through Workspace, then run
//! the ordinary design import with no filesystem or installed-library reader.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mitcad_model::print_files::installed::{self, ResolvedPreset};
use mitcad_model::print_files::{self, Imported};
use mitcad_model::print_keys::Dialect;
use mitcad_model::{ModelError, RecomputeMonitor};
use mitcad_model_api::{PrintImportReader, validate_print_import_command};
use serde_json::{Map, Value};

use super::{MAX_DOCUMENT_BYTES, Workspace, path_text, read_bounded, string};

const MAX_PRESET_FILES: usize = 4096;

pub(super) fn is_print_import(command: &Value) -> bool {
    matches!(
        command["cmd"].as_str(),
        Some("import_print_profile" | "import_slicer_presets")
    )
}

pub(super) struct PreparedPrintImport {
    source: Option<(String, Vec<u8>, Option<ResolvedPreset>)>,
    presets: BTreeMap<PathBuf, ResolvedPreset>,
}

fn invalid(message: impl Into<String>) -> ModelError {
    ModelError::Invalid(message.into())
}

impl PrintImportReader for PreparedPrintImport {
    fn read_print_file(
        &self,
        path: &str,
        dialect: Option<Dialect>,
    ) -> Result<Imported, ModelError> {
        let Some((source, bytes, resolved)) = &self.source else {
            return Err(invalid("print import source was not prepared"));
        };
        if source != path {
            return Err(invalid(
                "print import source differs from the prepared source",
            ));
        }
        let name = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);
        mitcad_model::reader_guard::catch_panic(
            path,
            || {
                let resolved_bytes = resolved
                    .as_ref()
                    .map(|preset| Value::Object(preset.map.clone()).to_string());
                let data = resolved_bytes
                    .as_ref()
                    .map_or(bytes.as_slice(), |text| text.as_bytes());
                let mut imported = print_files::read_file(name, data, dialect).map_err(invalid)?;
                if let Some(resolved) = resolved {
                    for note in &mut imported.report.filaments {
                        note.inherits.clone_from(&resolved.chain);
                        note.missing.clone_from(&resolved.missing);
                    }
                }
                Ok(imported)
            },
            ModelError::Invalid,
        )
    }

    fn resolve_preset(&self, _folder: &Path, path: &Path) -> Result<ResolvedPreset, ModelError> {
        self.presets
            .get(path)
            .cloned()
            .ok_or_else(|| invalid("preset source was not prepared"))
    }
}

/// A shared bound for a whole model_command call, not one array entry.
pub(super) struct PreparationBudget {
    bytes: u64,
    files: usize,
    entries: usize,
    monitor: Option<Arc<RecomputeMonitor>>,
}

impl PreparationBudget {
    pub(super) fn new(monitor: Option<Arc<RecomputeMonitor>>) -> Self {
        Self {
            bytes: 0,
            files: 0,
            entries: 0,
            monitor,
        }
    }

    pub(super) fn check(&self) -> Result<(), String> {
        match self
            .monitor
            .as_ref()
            .and_then(|monitor| monitor.cancel_reason())
        {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }

    fn read(&mut self, workspace: &Workspace, path: &Path) -> Result<Vec<u8>, String> {
        self.check()?;
        self.files += 1;
        if self.files > MAX_PRESET_FILES {
            return Err("print sources exceed the MCP file count limit".to_owned());
        }
        let confined = workspace.read_path(path_text(path)?)?;
        let bytes = read_bounded(&confined, MAX_DOCUMENT_BYTES.saturating_sub(self.bytes))
            .map_err(|e| e.to_string())?;
        self.bytes += bytes.len() as u64;
        self.check()?;
        Ok(bytes)
    }

    fn resolved(&mut self, resolved: &ResolvedPreset) -> Result<(), String> {
        // Count expanded maps too: many children can share one large base.
        // Counting serialization avoids allocating another copy to measure it.
        struct Counter {
            remaining: u64,
            written: u64,
        }
        impl Write for Counter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if bytes.len() as u64 > self.remaining {
                    return Err(std::io::Error::other(
                        "prepared print sources exceed the MCP file size limit",
                    ));
                }
                self.remaining -= bytes.len() as u64;
                self.written += bytes.len() as u64;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        self.check()?;
        let mut counter = Counter {
            remaining: MAX_DOCUMENT_BYTES.saturating_sub(self.bytes),
            written: 0,
        };
        serde_json::to_writer(&mut counter, &resolved.map).map_err(|e| e.to_string())?;
        self.bytes += counter.written;
        self.check()
    }

    fn visit(&mut self) -> Result<(), String> {
        self.check()?;
        self.entries += 1;
        if self.entries > MAX_PRESET_FILES {
            return Err("preset directory entries exceed the MCP file count limit".to_owned());
        }
        Ok(())
    }
}

/// An in-memory input set; reads are charged to the call's shared budget.
#[derive(Default)]
struct PresetFiles {
    files: BTreeMap<PathBuf, Map<String, Value>>,
}

impl PresetFiles {
    fn read(
        &mut self,
        workspace: &Workspace,
        path: &Path,
        budget: &mut PreparationBudget,
    ) -> Result<(), String> {
        if self.files.contains_key(path) {
            return Ok(());
        }
        let bytes = budget.read(workspace, path)?;
        if let Ok(map) = serde_json::from_slice(&bytes) {
            self.files.insert(path.to_path_buf(), map);
        }
        Ok(())
    }

    fn tree(
        &mut self,
        workspace: &Workspace,
        path: &Path,
        depth: usize,
        seen: &mut BTreeSet<PathBuf>,
        budget: &mut PreparationBudget,
    ) -> Result<(), String> {
        let canonical = match std::fs::canonicalize(path) {
            Ok(path) => workspace.confined(path)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("cannot open preset folder: {error}")),
        };
        if !canonical.is_dir() {
            return Err("preset folder must be a directory".to_owned());
        }
        if !seen.insert(canonical) {
            return Err("preset folders contain a directory cycle".to_owned());
        }
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
            budget.visit()?;
            entries.push(entry.map_err(|e| e.to_string())?);
        }
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() || kind.is_symlink() {
                // Check a link before following it, even if it names a folder.
                let target =
                    workspace.confined(std::fs::canonicalize(&path).map_err(|e| e.to_string())?)?;
                if target.is_dir() {
                    if depth > 0 {
                        self.tree(workspace, &path, depth - 1, seen, budget)?;
                    }
                    continue;
                }
            }
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json"))
            {
                self.read(workspace, &path, budget)?;
            }
        }
        Ok(())
    }

    fn resolve(&self, folder: &Path, path: &Path) -> Result<ResolvedPreset, String> {
        let resolved = installed::resolve_preset_from_files(folder, path, &self.files)?;
        if let Some(missing) = &resolved.missing {
            return Err(format!(
                "inherited preset {missing} is missing from the confined workspace sources"
            ));
        }
        Ok(resolved)
    }
}

impl Workspace {
    fn preset_folder(&self, path: &str) -> Result<PathBuf, String> {
        let candidate = self.candidate(path)?;
        let path = self.confined(std::fs::canonicalize(candidate).map_err(|e| e.to_string())?)?;
        if !path.is_dir() {
            return Err("preset folder must be a directory".to_owned());
        }
        Ok(path)
    }

    fn preset_sources(
        &self,
        folder: &Path,
        paths: &[PathBuf],
        budget: &mut PreparationBudget,
    ) -> Result<PresetFiles, String> {
        let mut files = PresetFiles::default();
        for path in paths {
            files.read(self, path, budget)?;
        }
        let mut accounts = BTreeSet::new();
        for path in paths {
            if let Ok(relative) = path.strip_prefix(folder.join("user"))
                && let Some(account) = relative.components().next()
            {
                accounts.insert(folder.join("user").join(account.as_os_str()));
            }
        }
        for account in accounts {
            files.tree(self, &account, 2, &mut BTreeSet::new(), budget)?;
        }
        if !paths.is_empty() {
            files.tree(
                self,
                &folder.join("system"),
                4,
                &mut BTreeSet::new(),
                budget,
            )?;
        }
        Ok(files)
    }

    pub(super) fn prepare_print_import(
        &self,
        command: &Value,
        budget: &mut PreparationBudget,
    ) -> Result<(Value, PreparedPrintImport), String> {
        validate_print_import_command(command).map_err(|e| e.to_string())?;
        let mut command = command.clone();
        let mut prepared = PreparedPrintImport {
            source: None,
            presets: BTreeMap::new(),
        };
        if command["cmd"] == "import_print_profile" {
            let path = self.read_path(string(&command, "path")?)?;
            let source = path_text(&path)?.to_owned();
            let bytes = budget.read(self, &path)?;
            let inherits = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json"))
                && serde_json::from_slice::<Map<String, Value>>(&bytes).is_ok_and(|map| {
                    map.get("inherits")
                        .and_then(Value::as_str)
                        .is_some_and(|v| !v.trim().is_empty())
                });
            let resolved = if inherits {
                // Infer only a local slicer layout. Never discover installed
                // folders, or search ancestors beyond the workspace root.
                let folder = path.ancestors().skip(1).take_while(|p| p.starts_with(&self.root))
                    .find(|p| matches!(p.file_name().and_then(|n| n.to_str()), Some("user" | "system"))
                        && path.strip_prefix(p).is_ok_and(|r| r.components().count() >= 3))
                    .and_then(Path::parent)
                    .filter(|p| p.starts_with(&self.root))
                    .ok_or_else(|| "inherited JSON requires a confined slicer folder; use import_slicer_presets with folder".to_owned())?;
                let files = self.preset_sources(folder, std::slice::from_ref(&path), budget)?;
                let resolved = files.resolve(folder, &path)?;
                budget.resolved(&resolved)?;
                Some(resolved)
            } else {
                None
            };
            command["path"] = Value::String(source.clone());
            prepared.source = Some((source, bytes, resolved));
        } else {
            let folder = self.preset_folder(string(&command, "folder")?)?;
            let presets = command["presets"]
                .as_array_mut()
                .ok_or_else(|| "presets must be an array".to_owned())?;
            let mut paths = Vec::new();
            for preset in presets {
                let path = self.read_path(string(preset, "path")?)?;
                preset["path"] = Value::String(path_text(&path)?.to_owned());
                paths.push(path);
            }
            let files = self.preset_sources(&folder, &paths, budget)?;
            for path in paths {
                let resolved = files.resolve(&folder, &path)?;
                budget.resolved(&resolved)?;
                prepared.presets.insert(path, resolved);
            }
            command["folder"] = Value::String(path_text(&folder)?.to_owned());
        }
        Ok((command, prepared))
    }
}
