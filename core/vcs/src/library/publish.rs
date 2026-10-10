// SPDX-License-Identifier: MIT
//! Help for publishing designs and components in a library of one's own
//! (mitcad#63): a library folder made (its manifest, a licence note and a
//! git repository), designs added to it with a preview image, the folder
//! checked, and the entry a community index takes for it. Recording the
//! files as a version and pushing them are the version history's and the
//! remote's commands (the folder is a Mitcad project).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::manifest::{
    ENTRY_FORMAT, IndexComponent, IndexEntry, LIBRARY_FORMAT, LIBRARY_MANIFEST, MAX_PREVIEW,
    Manifest, ManifestComponent, Problems, read_json, valid_id, valid_path,
};
use super::{LibraryRepo, read_file};
use crate::api::{optional, text};
use crate::{ProjectRepo, VcsError};

fn invalid(message: impl Into<String>) -> VcsError {
    VcsError::Command(message.into())
}

fn folder(command: &Value) -> Result<PathBuf, VcsError> {
    crate::absolute(Path::new(text(command, "dir")?))
}

fn strings(command: &Value, key: &str) -> Vec<String> {
    command
        .get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn write(path: &Path, data: &[u8]) -> Result<(), VcsError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| crate::io_error("make the folder", parent, e))?;
    }
    mitcad_model::file::write_atomically(path, data).map_err(|e| crate::io_error("write", path, e))
}

fn manifest_text(manifest: &Manifest) -> String {
    let mut text = serde_json::to_string_pretty(manifest).expect("a manifest serializes");
    text.push('\n');
    text
}

fn read_manifest(dir: &Path) -> Result<Manifest, VcsError> {
    let data = read_file(&dir.join(LIBRARY_MANIFEST))?;
    read_json(&data, LIBRARY_MANIFEST).map_err(VcsError::File)
}

/// `library_check`: what is wrong with a library folder as it is now (its
/// manifest and the files it names), before it is recorded and pushed.
pub(super) fn check_command(command: &Value) -> Result<Value, VcsError> {
    let dir = folder(command)?;
    let manifest = read_manifest(&dir)?;
    let problems = check_folder(&dir, &manifest);
    Ok(json!({
        "dir": dir.to_string_lossy(),
        "id": manifest.id,
        "components": manifest.components.len(),
        "ok": problems.is_ok(),
        "errors": problems.errors,
        "warnings": problems.warnings,
    }))
}

/// The manifest's problems and those of the files it names.
pub fn check_folder(dir: &Path, manifest: &Manifest) -> Problems {
    let mut out = manifest.problems();
    if !dir.join("LICENSE").is_file() && !dir.join("LICENSE.md").is_file() {
        out.warnings
            .push("there is no LICENSE file with the licence's text or a note of it".to_owned());
    }
    for c in &manifest.components {
        if valid_path(&c.path) {
            let path = dir.join(&c.path);
            match fs::symlink_metadata(&path) {
                Ok(meta) if meta.file_type().is_symlink() => out
                    .errors
                    .push(format!("{}: {} is a symbolic link", c.id, c.path)),
                Ok(_) => match fs::read_to_string(&path) {
                    Ok(text) => {
                        if let Err(e) = mitcad_model::diff::read_design(&text) {
                            out.errors.push(format!("{}: {}: {e}", c.id, c.path));
                        }
                        let links = serde_json::from_str::<Value>(&text)
                            .ok()
                            .and_then(|v| v.get("components").cloned())
                            .and_then(|c| c.as_array().cloned())
                            .unwrap_or_default()
                            .iter()
                            .any(|c| c.get("link").is_some_and(|l| !l.is_null()));
                        if links {
                            out.errors.push(format!(
                                "{}: {} links other files; a library's designs hold their parts \
                                 (insert them as copies)",
                                c.id, c.path
                            ));
                        }
                    }
                    Err(e) => out
                        .errors
                        .push(format!("{}: cannot read {}: {e}", c.id, c.path)),
                },
                Err(_) => out
                    .errors
                    .push(format!("{}: {} is not in the folder", c.id, c.path)),
            }
        }
        if !c.preview.is_empty() && valid_path(&c.preview) {
            match fs::read(dir.join(&c.preview)) {
                Ok(data) if !data.starts_with(b"\x89PNG") => out
                    .errors
                    .push(format!("{}: {} is not a PNG image", c.id, c.preview)),
                Ok(data) if data.len() > MAX_PREVIEW => out.errors.push(format!(
                    "{}: {} is larger than {} KB",
                    c.id,
                    c.preview,
                    MAX_PREVIEW >> 10
                )),
                Ok(_) => {}
                Err(_) => out
                    .errors
                    .push(format!("{}: {} is not in the folder", c.id, c.preview)),
            }
        } else if c.preview.is_empty() {
            out.warnings.push(format!("{}: no preview image", c.id));
        }
    }
    out
}

/// `library_init`: makes `dir` a library: its manifest, a licence note, a
/// README, and a Mitcad project with a git repository (no version yet).
pub(super) fn init_command(command: &Value) -> Result<Value, VcsError> {
    let dir = folder(command)?;
    let id = text(command, "id")?.trim().to_owned();
    if !valid_id(&id) {
        return Err(invalid(format!(
            "'{id}' is no library id (2 to 64 of a-z, 0-9 and -)"
        )));
    }
    let name = text(command, "name")?.trim().to_owned();
    if dir.join(LIBRARY_MANIFEST).exists() {
        return Err(invalid(format!("{} is a library already", dir.display())));
    }
    let license = optional(command, "license").map(|l| l.trim().to_owned());
    let manifest = Manifest {
        format: LIBRARY_FORMAT.to_owned(),
        version: 1,
        id: id.clone(),
        name: name.clone(),
        description: optional(command, "description")
            .unwrap_or("")
            .trim()
            .to_owned(),
        library_version: "0.1.0".to_owned(),
        license: license.clone(),
        authors: strings(command, "authors"),
        homepage: optional(command, "homepage").unwrap_or("").to_owned(),
        units: "mm".to_owned(),
        ..Manifest::default()
    };
    ProjectRepo::create(&dir)?;
    let mut written = vec![LIBRARY_MANIFEST.to_owned()];
    write(
        &dir.join(LIBRARY_MANIFEST),
        manifest_text(&manifest).as_bytes(),
    )?;
    if !dir.join("LICENSE").exists()
        && let Some(license) = &license
    {
        let note = format!(
            "The designs in this library are licensed under {license}.\n\
             The licence's text: https://spdx.org/licenses/{license}.html\n"
        );
        write(&dir.join("LICENSE"), note.as_bytes())?;
        written.push("LICENSE".to_owned());
    }
    if !dir.join("README.md").exists() {
        let readme = format!(
            "# {name}\n\nA Mitcad component library ({id}). Fetch it in Mitcad with \
             Libraries > Add and insert its parts with Insert Component from Library.\n"
        );
        write(&dir.join("README.md"), readme.as_bytes())?;
        written.push("README.md".to_owned());
    }
    for name in [
        ".gitattributes",
        ".gitignore",
        mitcad_model::file::PROJECT_MARKER,
    ] {
        written.push(name.to_owned());
    }
    Ok(json!({"dir": dir.to_string_lossy(), "id": id, "written": written}))
}

/// `library_add`: a design (its text, a single project file) added to a
/// library folder as a component, with a preview image (base64 PNG) and
/// its manifest entry; a component of the same id is replaced.
pub(super) fn add_command(command: &Value) -> Result<Value, VcsError> {
    let dir = folder(command)?;
    let mut manifest = read_manifest(&dir)?;
    let id = text(command, "id")?.trim().to_owned();
    if !valid_id(&id) {
        return Err(invalid(format!(
            "'{id}' is no component id (2 to 64 of a-z, 0-9 and -)"
        )));
    }
    let design = text(command, "text")?;
    let state = mitcad_model::diff::read_design(design)
        .map_err(|e| invalid(format!("the design cannot be read: {e}")))?;
    let _ = state;
    if !mitcad_model::file::brep_references(design)
        .map_err(|e| invalid(e.to_string()))?
        .is_empty()
    {
        return Err(invalid(
            "the design refers to B-rep data in a project's store; give it as a single file",
        ));
    }
    let category = optional(command, "category")
        .unwrap_or("")
        .trim()
        .to_owned();
    let folder_name = if category.is_empty() {
        "parts"
    } else {
        &category
    };
    let path = optional(command, "path")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{folder_name}/{id}.mitcad"));
    if !valid_path(&path) || !crate::is_project_file(&path) {
        return Err(invalid(format!(
            "'{path}' is no path of a design in the library"
        )));
    }
    let mut written = vec![path.clone()];
    write(&dir.join(&path), design.as_bytes())?;
    let mut preview = String::new();
    if let Some(png) = optional(command, "preview") {
        let data = mitcad_model::library::decode_base64(png).map_err(invalid)?;
        if !data.starts_with(b"\x89PNG") || data.len() > MAX_PREVIEW {
            return Err(invalid(format!(
                "the preview must be a PNG image of at most {} KB",
                MAX_PREVIEW >> 10
            )));
        }
        preview = format!("previews/{id}.png");
        write(&dir.join(&preview), &data)?;
        written.push(preview.clone());
    }
    if !category.is_empty() && !manifest.categories.iter().any(|c| c.id == category) {
        manifest.categories.push(super::manifest::Category {
            id: category.clone(),
            name: optional(command, "category_name")
                .unwrap_or(&category)
                .to_owned(),
        });
    }
    let component = ManifestComponent {
        id: id.clone(),
        path,
        category,
        name: optional(command, "name").unwrap_or(&id).trim().to_owned(),
        standard: optional(command, "standard").unwrap_or("").to_owned(),
        description: optional(command, "description").unwrap_or("").to_owned(),
        keywords: strings(command, "keywords"),
        preview,
        license: optional(command, "license").map(str::to_owned),
        designation: optional(command, "designation").unwrap_or("").to_owned(),
    };
    match manifest.components.iter_mut().find(|c| c.id == id) {
        Some(old) => *old = component.clone(),
        None => manifest.components.push(component.clone()),
    }
    write(
        &dir.join(LIBRARY_MANIFEST),
        manifest_text(&manifest).as_bytes(),
    )?;
    written.push(LIBRARY_MANIFEST.to_owned());
    let problems = check_folder(&dir, &manifest);
    Ok(json!({
        "dir": dir.to_string_lossy(),
        "component": component,
        "written": written,
        "errors": problems.errors,
        "warnings": problems.warnings,
    }))
}

/// `index_entry`: the entry a community index takes for the library in
/// `dir` (`libraries/<id>.json`), from its recorded version `rev` (default
/// the latest), to be proposed to the index's maintainers.
pub(super) fn index_entry_command(command: &Value) -> Result<Value, VcsError> {
    let dir = folder(command)?;
    let url = text(command, "url")?;
    crate::remote::check_url(url)?;
    let repo = LibraryRepo::open(&dir)?;
    let commit = repo.resolve(optional(command, "rev").unwrap_or("HEAD"))?;
    let manifest = repo.manifest(commit)?;
    let entry = IndexEntry {
        format: ENTRY_FORMAT.to_owned(),
        version: 1,
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        description: manifest.description.clone(),
        url: crate::remote::redact(url),
        homepage: manifest.homepage.clone(),
        license: manifest.license.clone(),
        maintainers: manifest.authors.clone(),
        tags: manifest
            .categories
            .iter()
            .map(|c| {
                if c.name.is_empty() {
                    c.id.clone()
                } else {
                    c.name.clone()
                }
            })
            .collect(),
        components: manifest
            .components
            .iter()
            .map(|c| IndexComponent {
                id: c.id.clone(),
                name: c.name.clone(),
                standard: c.standard.clone(),
                tags: c.keywords.clone(),
            })
            .collect(),
        reviewed: Vec::new(),
    };
    let mut text = serde_json::to_string_pretty(&entry).expect("an entry serializes");
    text.push('\n');
    Ok(json!({
        "path": format!("{}/{}.json", super::manifest::INDEX_ENTRIES, entry.id),
        "rev": commit.to_string(),
        "labels": repo.labels(commit)?,
        "entry": entry,
        "text": text,
    }))
}
