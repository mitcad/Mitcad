// SPDX-License-Identifier: MIT
//! The machine library (mitcad#1013, owner 12.11.2026: a tool library per
//! machine in the user's settings, with a copy in the project): one JSON
//! file per machine with its tool library, `cam/machines/<id>.json`, in
//!
//! - the **user's** folder, beside MitCad's other libraries
//!   ([`set_user_cam_library`], else `MITCAD_CAM_LIBRARY_DIR`);
//! - a **project's** folder `cam-library`, versioned with the project.
//!
//! A design keeps its own copies of the machines its setups use
//! (`CamState::machines`, with the library, id and version they came
//! from), so that a library change never changes a design by itself.
//!
//! A file: `{"format": "mitcad-cam-machine", "version": 3, "machine":
//! {...}, "tools": {"tools": [...]}}`; its id is its file name. The
//! version counts the saves of that file.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use super::{CamMachine, CamToolLibrary, MachineOrigin, valid_id};
use mitcad_cam::Machine;

/// The `format` of a machine file.
pub const MACHINE_FORMAT: &str = "mitcad-cam-machine";
/// The machines' folder in a library.
pub const MACHINES_DIR: &str = "cam/machines";
/// A project's library folder.
pub const PROJECT_LIBRARY_DIR: &str = "cam-library";
/// The largest machine file read.
pub const MAX_FILE: u64 = 4 << 20;

/// Which library a machine file is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CamLibrary {
    User,
    Project,
}

impl CamLibrary {
    pub const ALL: [CamLibrary; 2] = [CamLibrary::User, CamLibrary::Project];

    pub fn id(self) -> &'static str {
        match self {
            CamLibrary::User => "user",
            CamLibrary::Project => "project",
        }
    }

    pub fn parse(text: &str) -> Option<CamLibrary> {
        CamLibrary::ALL.into_iter().find(|l| l.id() == text)
    }
}

static USER_LIBRARY: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Sets the user's CAM library folder (the application sets it beside its
/// other libraries).
pub fn set_user_cam_library(folder: Option<PathBuf>) {
    *USER_LIBRARY.write().unwrap_or_else(|e| e.into_inner()) = folder;
}

/// The user's CAM library folder: the one set, else
/// `MITCAD_CAM_LIBRARY_DIR`.
pub fn user_cam_library() -> Option<PathBuf> {
    USER_LIBRARY
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .or_else(|| {
            std::env::var_os("MITCAD_CAM_LIBRARY_DIR")
                .filter(|d| !d.is_empty())
                .map(PathBuf::from)
        })
}

/// A library's folder: the user's, or the project's (`project` the
/// project's folder).
pub fn library_folder(library: CamLibrary, project: Option<&Path>) -> Result<PathBuf, String> {
    match library {
        CamLibrary::User => {
            user_cam_library().ok_or_else(|| "There is no user library folder.".to_owned())
        }
        CamLibrary::Project => project
            .map(|p| p.join(PROJECT_LIBRARY_DIR))
            .ok_or_else(|| "The design is in no project.".to_owned()),
    }
}

/// A machine file as it is written.
#[derive(Serialize, Deserialize)]
struct MachineFile {
    format: String,
    #[serde(default)]
    version: u32,
    machine: Machine,
    #[serde(default)]
    tools: CamToolLibrary,
}

/// A machine of a library.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryMachine {
    pub library: CamLibrary,
    pub id: String,
    pub version: u32,
    pub path: PathBuf,
    pub machine: Machine,
    pub tools: CamToolLibrary,
}

impl LibraryMachine {
    /// The design's copy of it.
    pub fn copy(&self) -> CamMachine {
        CamMachine {
            id: self.id.clone(),
            machine: self.machine.clone(),
            tools: self.tools.clone(),
            origin: Some(MachineOrigin {
                library: self.library.id().to_owned(),
                id: self.id.clone(),
                version: self.version,
            }),
        }
    }
}

/// Reads a machine file's text.
pub fn parse_machine(id: &str, text: &str) -> Result<(u32, CamMachine), String> {
    let file: MachineFile =
        serde_json::from_str(text).map_err(|e| format!("not a machine file: {e}"))?;
    if file.format != MACHINE_FORMAT {
        return Err(format!("not a machine file: its format is {}", file.format));
    }
    let machine = CamMachine {
        id: id.to_owned(),
        machine: file.machine,
        tools: file.tools,
        origin: None,
    };
    machine.check()?;
    Ok((file.version, machine))
}

/// A machine file's text: the design's machine at a version.
pub fn machine_text(machine: &CamMachine, version: u32) -> String {
    let file = MachineFile {
        format: MACHINE_FORMAT.to_owned(),
        version,
        machine: machine.machine.clone(),
        tools: machine.tools.clone(),
    };
    let mut text = serde_json::to_string_pretty(&file).expect("a machine file always serializes");
    text.push('\n');
    text
}

/// The machines of a library folder, and what could not be read. Links
/// are not followed; files larger than [`MAX_FILE`] are left out.
pub fn read_library(library: CamLibrary, folder: &Path) -> (Vec<LibraryMachine>, Vec<String>) {
    let mut machines = Vec::new();
    let mut problems = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder.join(MACHINES_DIR)) else {
        return (machines, problems);
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        if !valid_id(id) || !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        if !entry.metadata().is_ok_and(|m| m.len() <= MAX_FILE) {
            problems.push(format!("{}: {name} is too large", library.id()));
            continue;
        }
        let text = match std::fs::read_to_string(entry.path()) {
            Ok(text) => text,
            Err(e) => {
                problems.push(format!("{}: {name}: {e}", library.id()));
                continue;
            }
        };
        match parse_machine(id, &text) {
            Ok((version, machine)) => machines.push(LibraryMachine {
                library,
                id: id.to_owned(),
                version,
                path: entry.path(),
                machine: machine.machine,
                tools: machine.tools,
            }),
            Err(e) => problems.push(format!("{}: {name}: {e}", library.id())),
        }
    }
    (machines, problems)
}

/// Writes the design's machine into a library folder as `id` (the next
/// version of the file there, else version 1); returns the file and the
/// version.
pub fn write_machine(
    folder: &Path,
    id: &str,
    machine: &CamMachine,
) -> Result<(PathBuf, u32), String> {
    if !valid_id(id) {
        return Err(
            "A machine's id is 1 to 64 letters, digits, underscores or hyphens.".to_owned(),
        );
    }
    let dir = folder.join(MACHINES_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("The library folder cannot be made: {e}"))?;
    let path = dir.join(format!("{id}.json"));
    let version = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| parse_machine(id, &text).ok())
        .map_or(0, |(version, _)| version)
        + 1;
    crate::file::write_atomically(&path, machine_text(machine, version).as_bytes())
        .map_err(|e| format!("The machine file cannot be written: {e}"))?;
    Ok((path, version))
}
