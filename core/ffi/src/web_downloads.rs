// SPDX-License-Identifier: MIT
//! The component browser's zip downloads (mitcad#1579, app/COMMANDS.md
//! "Component from Web"): the files a zip holds and one of them written
//! out, read with `mitcad-zip`. Which of them MitCad imports is the
//! application's choice (its format filter); this only reads the archive.

use std::path::{Component, Path};

use mitcad_model_api::ApiError;
use mitcad_zip::{Archive, LARGE_ENTRY_LIMIT};
use serde_json::{Value, json};

/// At most this many entries are listed: a model's zip holds a few files,
/// and a zip with more is not one to pick from.
const MAX_ENTRIES: usize = 10_000;

fn open(zip: &str) -> Result<Archive, ApiError> {
    Archive::open(Path::new(zip)).map_err(|e| ApiError(format!("{zip}: {e}")))
}

/// Whether an entry's name is a relative path inside the archive: no
/// absolute path, drive or `..`, so that writing it out stays in its
/// folder.
fn safe_name(name: &str) -> bool {
    let name = name.replace('\\', "/");
    !name.is_empty()
        && !name.starts_with('/')
        && Path::new(&name)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// The files of the zip at `zip` (not its folders), in the archive's
/// order, as JSON `[{"name", "size"}]`: `name` the entry's path inside the
/// archive with `/` between folders, `size` its uncompressed size in
/// bytes. Entries whose names would leave their folder are left out.
pub fn zip_list(zip: &str) -> Result<String, ApiError> {
    let archive = open(zip)?;
    let entries: Vec<Value> = archive
        .entries()
        .iter()
        .filter(|e| !e.is_dir() && safe_name(&e.name))
        .take(MAX_ENTRIES)
        .map(|e| json!({"name": e.name.replace('\\', "/"), "size": e.size}))
        .collect();
    Ok(Value::Array(entries).to_string())
}

/// Writes the entry `name` of the zip at `zip` to the file `target`
/// (its folders made), decompressed and its CRC checked; refused for a
/// name that would leave its folder, an encrypted entry or one that
/// declares more than 4 GiB or expands too far (mitcad-zip's limits).
pub fn zip_extract(zip: &str, name: &str, target: &str) -> Result<(), ApiError> {
    if !safe_name(name) {
        return Err(ApiError(format!("{name}: not a path inside the archive")));
    }
    let archive = open(zip)?;
    let entry = archive
        .entries()
        .iter()
        .find(|e| e.name.replace('\\', "/") == name && !e.is_dir())
        .ok_or_else(|| ApiError(format!("{zip}: no file named {name}")))?;
    let data = archive
        .read_with_limit(entry, LARGE_ENTRY_LIMIT)
        .map_err(|e| ApiError(format!("{name}: {e}")))?;
    let target = Path::new(target);
    if let Some(folder) = target.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|e| ApiError(format!("{}: {e}", folder.display())))?;
    }
    std::fs::write(target, data).map_err(|e| ApiError(format!("{}: {e}", target.display())))
}
