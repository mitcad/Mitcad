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

use super::build::{self, Resolved};
use super::manifest::{
    ENTRY_FORMAT, IndexComponent, IndexEntry, LIBRARY_FORMAT, LIBRARY_MANIFEST, MAX_COMPONENT,
    MAX_PREVIEW, Manifest, ManifestComponent, Problems, read_json, valid_id, valid_path,
};
use super::mounts;
use super::{LibraryCache, LibraryRepo, read_file};
use crate::api::{optional, text};
use crate::transaction::FileTransaction;
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

fn has_links(text: &str) -> bool {
    serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}'))
        .ok()
        .and_then(|v| v.get("components").cloned())
        .and_then(|c| c.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .any(|c| c.get("link").is_some_and(|l| !l.is_null()))
}

/// `library_check`: what is wrong with a library folder as it is now (its
/// manifest and the files it names), before it is recorded and pushed.
/// BUILD parts' references to parts of other libraries are checked
/// against the fetched ones in `cache`.
pub(super) fn check_command(
    command: &Value,
    cache: Option<&LibraryCache>,
) -> Result<Value, VcsError> {
    let dir = folder(command)?;
    let manifest = read_manifest(&dir)?;
    let problems = check_folder(&dir, &manifest, cache);
    Ok(json!({
        "dir": dir.to_string_lossy(),
        "id": manifest.id,
        "components": manifest.components.len(),
        "ok": problems.is_ok(),
        "errors": problems.errors,
        "warnings": problems.warnings,
    }))
}

/// The manifest's problems and those of the files it names: the designs,
/// the previews, and for BUILD parts (mitcad#375) the frames and
/// parameters of their designs and the library parts they refer to.
pub fn check_folder(dir: &Path, manifest: &Manifest, cache: Option<&LibraryCache>) -> Problems {
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
                        if let Err(e) = mitcad_model_diff::read_design(&text) {
                            out.errors.push(format!("{}: {}: {e}", c.id, c.path));
                        }
                        if let Ok(design) =
                            serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}'))
                        {
                            if c.build.is_some() {
                                build::check_design(c, &c.id, &design, &mut out);
                            }
                            // Mount frames of any part (mitcad#525).
                            if c.build.is_none() || c.mounts.is_some() {
                                mounts::check_design(c, &c.id, &design, &mut out);
                            }
                        }
                        if has_links(&text) {
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
        let image_problem = |path: &str| match fs::read(dir.join(path)) {
            Ok(data) if !data.starts_with(b"\x89PNG") => {
                Some(format!("{}: {path} is not a PNG image", c.id))
            }
            Ok(data) if data.len() > MAX_PREVIEW => Some(format!(
                "{}: {path} is larger than {} KB",
                c.id,
                MAX_PREVIEW >> 10
            )),
            Ok(_) => None,
            Err(_) => Some(format!("{}: {path} is not in the folder", c.id)),
        };
        if !c.preview.is_empty() && valid_path(&c.preview) {
            out.errors.extend(image_problem(&c.preview));
        } else if c.preview.is_empty() {
            out.warnings.push(format!("{}: no preview image", c.id));
        }
        // The rows' own previews (mitcad#484): rows of the design.
        if !c.row_previews.is_empty() {
            let rows = fs::read_to_string(dir.join(&c.path))
                .map(|text| row_names(&text))
                .unwrap_or_default();
            for (row, path) in &c.row_previews {
                if !rows.contains(row) {
                    out.errors.push(format!(
                        "{}: row_previews names {row}, which is no configuration row of {}",
                        c.id, c.path
                    ));
                }
                if valid_path(path) {
                    out.errors.extend(image_problem(path));
                }
            }
        }
        // The rows' standards and articles (mitcad#949): rows of the design.
        if !c.rows.is_empty() {
            let rows = fs::read_to_string(dir.join(&c.path))
                .map(|text| row_names(&text))
                .unwrap_or_default();
            for row in c.rows.keys() {
                if !rows.contains(row) {
                    out.errors.push(format!(
                        "{}: rows names {row}, which is no configuration row of {}",
                        c.id, c.path
                    ));
                }
            }
        }
    }
    let mut resolve =
        |library: &str, id: &str| resolve_reference(dir, manifest, cache, library, id);
    for c in &manifest.components {
        build::check_references(c, &c.id, &mut resolve, &mut out);
    }
    out
}

/// The row names of a design's text; none when it cannot be read.
fn row_names(text: &str) -> Vec<String> {
    serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}'))
        .map(|design| {
            build::rows(&design)
                .into_iter()
                .map(|row| row.name)
                .collect()
        })
        .unwrap_or_default()
}

/// What a BUILD part's reference to component `id` of `library` finds:
/// in this library's folder, else in the newest version of a fetched one.
fn resolve_reference(
    dir: &Path,
    manifest: &Manifest,
    cache: Option<&LibraryCache>,
    library: &str,
    id: &str,
) -> Resolved {
    if library == manifest.id {
        return match manifest.component(id) {
            None => Resolved::Missing,
            Some(other) if valid_path(&other.path) => Resolved::Found(
                fs::read_to_string(dir.join(&other.path))
                    .map(|text| row_names(&text))
                    .unwrap_or_default(),
            ),
            Some(_) => Resolved::Found(Vec::new()),
        };
    }
    let Some(repo) = cache.and_then(|cache| cache.open_id(library)) else {
        return Resolved::Unknown;
    };
    let Some(commit) = repo
        .newest()
        .ok()
        .flatten()
        .and_then(|version| repo.resolve(&version.rev).ok())
    else {
        return Resolved::Unknown;
    };
    let Ok(other) = repo.manifest(commit) else {
        return Resolved::Unknown;
    };
    match other.component(id) {
        None => Resolved::Missing,
        Some(item) => Resolved::Found(
            repo.read(commit, &item.path)
                .ok()
                .flatten()
                .and_then(|data| String::from_utf8(data).ok())
                .map(|text| row_names(&text))
                .unwrap_or_default(),
        ),
    }
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

/// `library_add`: a design (its text, a single-file design) added to a
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
    if design.len() > MAX_COMPONENT {
        return Err(invalid(format!(
            "the design is larger than the {} MB a library's component may hold",
            MAX_COMPONENT >> 20
        )));
    }
    let state = mitcad_model_diff::read_design(design)
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
    if has_links(design) {
        return Err(invalid(
            "the design links other files; a library's designs hold their parts \
             (insert them as copies)",
        ));
    }
    // One category or a list of them (mitcad#527), the first its home.
    let mut category_ids: Vec<String> = match optional(command, "category") {
        Some(one) if !one.trim().is_empty() => vec![one.trim().to_owned()],
        _ => strings(command, "category"),
    };
    for more in strings(command, "categories") {
        if !category_ids.contains(&more) {
            category_ids.push(more);
        }
    }
    let category = category_ids.first().cloned().unwrap_or_default();
    let folder_name = if category.is_empty() {
        "parts"
    } else {
        &category
    };
    let path = optional(command, "path")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{folder_name}/{id}.mitcad"));
    if !valid_path(&path) || !crate::is_design_file(&path) {
        return Err(invalid(format!(
            "'{path}' is no path of a design in the library"
        )));
    }
    let mut preview = String::new();
    let preview_data = if let Some(png) = optional(command, "preview") {
        let data = mitcad_model::library::decode_base64(png).map_err(invalid)?;
        if !data.starts_with(b"\x89PNG") || data.len() > MAX_PREVIEW {
            return Err(invalid(format!(
                "the preview must be a PNG image of at most {} KB",
                MAX_PREVIEW >> 10
            )));
        }
        preview = format!("previews/{id}.png");
        Some(data)
    } else {
        None
    };
    // A new category is listed: the home one with `category_name` (a path
    // of names, `Wood/Boards`) and `category_parent` (a category's id),
    // the others by their ids.
    for (i, id) in category_ids.iter().enumerate() {
        if manifest.categories.iter().any(|c| &c.id == id) {
            continue;
        }
        let (name, parent) = if i == 0 {
            (
                optional(command, "category_name").unwrap_or(id),
                optional(command, "category_parent").unwrap_or(""),
            )
        } else {
            (id.as_str(), "")
        };
        manifest.categories.push(super::manifest::Category {
            id: id.clone(),
            name: name.trim().to_owned(),
            parent: parent.trim().to_owned(),
        });
    }
    // The supplier and BUILD fields (mitcad#375): the command's, else those
    // of the component it replaces.
    let old = manifest.component(&id).cloned().unwrap_or_default();
    let kept = |key: &str, old: String| {
        optional(command, key)
            .map(|v| v.trim().to_owned())
            .unwrap_or(old)
    };
    let build = command
        .get("build")
        .filter(|b| !b.is_null())
        .cloned()
        .or(old.build);
    // The mount frames of a part that is no BUILD part (mitcad#525): the
    // command's, else the design's (a BUILD part lists its own in
    // build.frames).
    let mount_frames = match command.get("mounts").filter(|m| !m.is_null()) {
        Some(given) => Some(given.clone()),
        None if build.is_none() => {
            serde_json::from_str::<Value>(design.trim_start_matches('\u{feff}'))
                .ok()
                .and_then(|value| mounts::manifest_mounts(&value))
        }
        None => None,
    };
    let component = ManifestComponent {
        id: id.clone(),
        path,
        category: super::manifest::CategoryIds(category_ids),
        categories: Vec::new(),
        name: optional(command, "name").unwrap_or(&id).trim().to_owned(),
        standard: optional(command, "standard").unwrap_or("").to_owned(),
        description: optional(command, "description").unwrap_or("").to_owned(),
        keywords: strings(command, "keywords"),
        preview,
        license: optional(command, "license").map(str::to_owned),
        designation: optional(command, "designation").unwrap_or("").to_owned(),
        supplier: kept("supplier", old.supplier),
        part_number: kept("part_number", old.part_number),
        supplier_url: kept("supplier_url", old.supplier_url),
        // A new design: the rows' pictures of the old one may not match it.
        row_previews: Default::default(),
        // The rows' standards and suppliers' articles (mitcad#949) stay:
        // `library check` names a row the new design lacks.
        rows: old.rows,
        build,
        mounts: mount_frames,
    };
    match manifest.components.iter_mut().find(|c| c.id == id) {
        Some(old) => *old = component.clone(),
        None => manifest.components.push(component.clone()),
    }
    let problems = manifest.problems();
    if !problems.is_ok() {
        return Err(invalid(format!(
            "the updated library manifest is invalid: {}",
            problems.errors.join("; ")
        )));
    }
    // Validate everything before replacing even the first file. Keep the
    // originals until the whole design/preview/manifest update succeeds.
    let mut written = vec![component.path.clone()];
    let mut files = vec![(dir.join(&component.path), design.as_bytes().to_vec())];
    if let Some(data) = preview_data {
        files.push((dir.join(&component.preview), data));
        written.push(component.preview.clone());
    }
    files.push((
        dir.join(LIBRARY_MANIFEST),
        manifest_text(&manifest).into_bytes(),
    ));
    let mut transaction = FileTransaction::prepare(&dir, &files)?;
    transaction.apply()?;
    written.push(LIBRARY_MANIFEST.to_owned());
    let mut problems = check_folder(&dir, &manifest, None);
    problems.warnings.extend(transaction.commit());
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
