// SPDX-License-Identifier: MIT
//! Component libraries for C++ (mitcad#64, mitcad#63, `mitcad_vcs::library`):
//! where fetched libraries are kept, the model's resolver of library parts
//! over them, and the library commands with a [`SyncControl`] for a
//! fetch's progress and cancellation.

use std::path::PathBuf;
use std::sync::RwLock;

use mitcad_vcs::VcsError;
use mitcad_vcs::library::{LibraryCache, LibraryResolver, api};
use serde_json::{Value, json};

use crate::remote::SyncControl;

/// The cache of fetched libraries; [`LibraryCache::default_root`] until
/// configured.
static CACHE: RwLock<Option<LibraryCache>> = RwLock::new(None);

fn cache() -> LibraryCache {
    CACHE
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(|| LibraryCache::new(LibraryCache::default_root()))
}

/// Sets where fetched libraries are kept (`root`; "" or left out:
/// `MITCAD_LIBRARIES_DIR`, else the user's data folder) and folders of
/// more (`extra`), and installs the resolver documents read library parts
/// with. Returns {"root", "extra"}.
pub fn configure_libraries(json: &str) -> Result<String, VcsError> {
    let value: Value = if json.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(json).map_err(|e| VcsError::Command(format!("not valid JSON: {e}")))?
    };
    let root = value
        .get("root")
        .and_then(Value::as_str)
        .filter(|r| !r.trim().is_empty())
        .map_or_else(LibraryCache::default_root, PathBuf::from);
    let mut cache = LibraryCache::new(root);
    cache.extra = value
        .get("extra")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default();
    LibraryResolver::install(cache.clone());
    let answer = json!({"root": cache.root.to_string_lossy(),
                        "extra": cache.extra.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>()});
    *CACHE.write().unwrap_or_else(|e| e.into_inner()) = Some(cache);
    Ok(answer.to_string())
}

/// Runs a library command (commands.md, "Component libraries"); a fetch
/// reports its progress to `control` and stops when it is cancelled.
pub fn library_command(json: &str, control: &SyncControl) -> Result<String, VcsError> {
    api::command(json, &cache(), None, Some(control.control()))
}

/// The JSON answer of a library command as text for people.
pub fn describe_library(command: &str, answer: &str) -> Result<String, VcsError> {
    api::describe(command, answer)
}

/// The `parts_list` query's answer as text: a line per part with its
/// quantity, and a library part's library, version and licence.
pub fn describe_parts(answer: &str) -> Result<String, VcsError> {
    use std::fmt::Write;
    let value: Value = serde_json::from_str(answer)
        .map_err(|e| VcsError::Command(format!("not valid JSON: {e}")))?;
    let rows = value["rows"].as_array().cloned().unwrap_or_default();
    let mut out = String::new();
    if rows.is_empty() {
        out.push_str("No parts: the design has no components.\n");
        return Ok(out);
    }
    let _ = writeln!(out, "Qty  Part");
    for row in rows {
        let s = |key: &str| row[key].as_str().unwrap_or("").to_owned();
        let mut line = format!("{:>3}  {}", row["quantity"], s("designation"));
        if !row["library"].is_null() {
            let license = if s("license").is_empty() {
                "no licence".to_owned()
            } else {
                s("license")
            };
            let how = if row["linked"] == true {
                "linked"
            } else {
                "copy"
            };
            line.push_str(&format!(
                "  ({} {}, {license}, {how})",
                s("library"),
                s("version")
            ));
        }
        let _ = writeln!(out, "{line}");
    }
    Ok(out)
}
