// SPDX-License-Identifier: MIT
//! In-app tutorials in git repositories (mitcad#2012, docs/tutorials.md),
//! fetched as component libraries are ([`super::LibraryCache::fetch_with`]):
//! a tutorials repository has `mitcad-tutorials.json` at its root and one
//! folder per tutorial in `tutorials/`, each with its manifest
//! `tutorial.json`, its pages and their assets and its starting designs.
//! A tutorial is installed by writing its folder's files at a version into
//! the user's data folder (`tutorial_install`); nothing in it is run by
//! MitCad itself (its pages run in the web browser component, with the
//! bridge `window.mitcad` only).

use std::fs;
use std::path::Path;

use gix::ObjectId;
use gix::bstr::ByteSlice;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::manifest::read_json;
use super::{LibraryCache, LibraryRepo, RepoKind};
use crate::VcsError;
use crate::api::text;

/// The root manifest of a tutorials repository.
pub const TUTORIALS_MANIFEST: &str = "mitcad-tutorials.json";
/// Its format name.
pub const TUTORIALS_FORMAT: &str = "mitcad-tutorials";
/// The folder of the tutorials, one folder each.
pub const TUTORIALS_DIR: &str = "tutorials";
/// A tutorial's manifest in its folder.
pub const TUTORIAL_MANIFEST: &str = "tutorial.json";
/// What an installed tutorial's folder records of where it came from.
pub const INSTALLED_MARK: &str = ".mitcad-tutorial-source.json";
/// The most files and bytes one tutorial may have.
const MAX_FILES: usize = 2000;
const MAX_BYTES: u64 = 100 << 20;

/// `mitcad-tutorials.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TutorialsManifest {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub description: String,
}

/// A tutorial's `tutorial.json` (docs/tutorials.md "The package").
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TutorialManifest {
    /// The folder's name: lower case letters, digits and hyphens.
    pub id: String,
    pub title: String,
    /// The language of its pages ("en", "fi").
    pub language: String,
    /// Its own version ("1.2").
    pub version: String,
    /// The oldest MitCad it works with ("0.68.0").
    pub min_mitcad: String,
    /// An SPDX licence id.
    pub license: String,
    pub description: String,
    /// The page shown first, a path in its folder.
    pub start: String,
    /// The starting designs, paths in its folder (opened by its pages).
    pub designs: Vec<String>,
}

impl TutorialManifest {
    /// What is wrong with it in the folder `folder`; empty when nothing is.
    pub fn problems(&self, folder: &str) -> Vec<String> {
        let mut problems = Vec::new();
        if !valid_id(&self.id) {
            problems.push(format!(
                "id \"{}\" is not lower case letters, digits and hyphens",
                self.id
            ));
        } else if self.id != folder {
            problems.push(format!(
                "id \"{}\" is not its folder's name \"{folder}\"",
                self.id
            ));
        }
        for (field, value) in [
            ("title", &self.title),
            ("language", &self.language),
            ("version", &self.version),
            ("min_mitcad", &self.min_mitcad),
            ("license", &self.license),
        ] {
            if value.trim().is_empty() {
                problems.push(format!("{field} is missing"));
            }
        }
        for path in std::iter::once(&self.start).chain(&self.designs) {
            if !path.is_empty() && !super::manifest::valid_path(path) {
                problems.push(format!("{path} is not a path inside the tutorial"));
            }
        }
        problems
    }

    /// The page shown first: `start`, else `index.html`.
    pub fn start_page(&self) -> &str {
        if self.start.is_empty() {
            "index.html"
        } else {
            &self.start
        }
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl LibraryRepo {
    /// The tutorials repository's manifest and its tutorials at `commit`;
    /// tutorials that cannot be read are reported apart.
    pub fn tutorials(
        &self,
        commit: ObjectId,
    ) -> Result<(TutorialsManifest, Vec<TutorialManifest>, Vec<String>), VcsError> {
        let data = self.read(commit, TUTORIALS_MANIFEST)?.ok_or_else(|| {
            VcsError::NotFound(format!(
                "there is no {TUTORIALS_MANIFEST}: not a tutorials repository"
            ))
        })?;
        let manifest: TutorialsManifest =
            read_json(&data, TUTORIALS_MANIFEST).map_err(VcsError::File)?;
        if manifest.format != TUTORIALS_FORMAT || manifest.version != 1 {
            return Err(VcsError::File(format!(
                "{TUTORIALS_MANIFEST} is not of a format this MitCad reads ({TUTORIALS_FORMAT}, version 1)"
            )));
        }
        let mut tutorials = Vec::new();
        let mut problems = Vec::new();
        let tree = self.repo.find_commit(commit)?.tree()?;
        if let Some(folder) = tree.lookup_entry([TUTORIALS_DIR])?
            && folder.mode().is_tree()
        {
            let folder = self.repo.find_tree(folder.object_id())?;
            for entry in folder.iter() {
                let entry = entry.map_err(|e| VcsError::Git(e.to_string()))?;
                if !entry.mode().is_tree() {
                    continue;
                }
                let name = entry.filename().to_str_lossy().into_owned();
                let path = format!("{TUTORIALS_DIR}/{name}/{TUTORIAL_MANIFEST}");
                let Some(data) = self.read(commit, &path)? else {
                    problems.push(format!("{TUTORIALS_DIR}/{name}: no {TUTORIAL_MANIFEST}"));
                    continue;
                };
                match read_json::<TutorialManifest>(&data, &path) {
                    Ok(tutorial) => {
                        let wrong = tutorial.problems(&name);
                        if wrong.is_empty() {
                            tutorials.push(tutorial);
                        } else {
                            problems.push(format!("{path}: {}", wrong.join("; ")));
                        }
                    }
                    Err(e) => problems.push(e),
                }
            }
        }
        tutorials.sort_by_key(|t| t.title.to_lowercase());
        Ok((manifest, tutorials, problems))
    }

    /// Writes the files of the folder `prefix` at `commit` into `target`
    /// (made new: an earlier one is replaced once all is written). Symbolic
    /// links and submodules are refused; at most [`MAX_FILES`] files and
    /// [`MAX_BYTES`] bytes.
    pub fn extract(
        &self,
        commit: ObjectId,
        prefix: &str,
        target: &Path,
    ) -> Result<usize, VcsError> {
        let tree = self.repo.find_commit(commit)?.tree()?;
        let Some(folder) = tree.lookup_entry(prefix.split('/'))? else {
            return Err(VcsError::NotFound(format!(
                "{prefix} is not in the repository"
            )));
        };
        if !folder.mode().is_tree() {
            return Err(VcsError::NotFound(format!("{prefix} is not a folder")));
        }
        let parent = target.parent().unwrap_or(target);
        fs::create_dir_all(parent).map_err(|e| crate::io_error("make the folder", parent, e))?;
        let temporary = parent.join(format!(
            ".{}.{}.tmp",
            target
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temporary);
        let mut written = (0usize, 0u64);
        let result = self.write_tree(folder.object_id(), &temporary, &mut written);
        if let Err(e) = result {
            let _ = fs::remove_dir_all(&temporary);
            return Err(e);
        }
        if target.exists() {
            fs::remove_dir_all(target).map_err(|e| crate::io_error("remove", target, e))?;
        }
        fs::rename(&temporary, target).map_err(|e| crate::io_error("rename", &temporary, e))?;
        Ok(written.0)
    }

    fn write_tree(
        &self,
        id: ObjectId,
        dir: &Path,
        written: &mut (usize, u64),
    ) -> Result<(), VcsError> {
        fs::create_dir_all(dir).map_err(|e| crate::io_error("make the folder", dir, e))?;
        let tree = self.repo.find_tree(id)?;
        for entry in tree.iter() {
            let entry = entry.map_err(|e| VcsError::Git(e.to_string()))?;
            let name = entry.filename().to_str_lossy().into_owned();
            if name.is_empty()
                || name == "."
                || name == ".."
                || name.contains(['/', '\\', ':'])
                || name.eq_ignore_ascii_case(".git")
            {
                return Err(VcsError::InvalidPath(format!("{name} is not a file name")));
            }
            let mode = entry.mode();
            let path = dir.join(&name);
            if mode.is_tree() {
                self.write_tree(entry.object_id(), &path, written)?;
            } else if mode.is_blob() {
                let data = self.repo.find_blob(entry.object_id())?.take_data();
                written.0 += 1;
                written.1 += data.len() as u64;
                if written.0 > MAX_FILES || written.1 > MAX_BYTES {
                    return Err(VcsError::File(format!(
                        "a tutorial has at most {MAX_FILES} files and {} MB",
                        MAX_BYTES >> 20
                    )));
                }
                fs::write(&path, data).map_err(|e| crate::io_error("write", &path, e))?;
            } else {
                return Err(VcsError::InvalidPath(format!(
                    "{name} is a symbolic link or a submodule; tutorials hold files"
                )));
            }
        }
        Ok(())
    }
}

/// `tutorial_list`: the tutorials of the fetched tutorials repositories
/// (all, or those of `urls`), newest version of each repository's default
/// branch.
pub fn list(cache: &LibraryCache, command: &Value) -> Value {
    let wanted: Option<Vec<String>> = command.get("urls").and_then(Value::as_array).map(|urls| {
        urls.iter()
            .filter_map(Value::as_str)
            .map(super::repository_key)
            .collect()
    });
    let mut sources = Vec::new();
    for dir in cache.repos() {
        let Ok(repo) = LibraryRepo::open(&dir) else {
            continue;
        };
        let Ok(Some(head)) = repo.head() else {
            continue;
        };
        if repo.kind(head) != RepoKind::Tutorials {
            continue;
        }
        let url = repo.url().unwrap_or_default();
        if let Some(wanted) = &wanted
            && !wanted.contains(&super::repository_key(&url))
        {
            continue;
        }
        let mut value = json!({"url": url, "rev": head.to_string(), "dir": dir.to_string_lossy()});
        match repo.tutorials(head) {
            Ok((manifest, tutorials, problems)) => {
                value["name"] = json!(manifest.name);
                value["description"] = json!(manifest.description);
                value["tutorials"] = json!(tutorials);
                value["problems"] = json!(problems);
            }
            Err(e) => {
                value["tutorials"] = json!([]);
                value["problems"] = json!([e.to_string()]);
            }
        }
        sources.push(value);
    }
    json!({"sources": sources})
}

/// `tutorial_install`: the tutorial `id` of the fetched repository `url`
/// written into `folder`/`id` at the repository's newest version, with what
/// it came from beside it ([`INSTALLED_MARK`]).
pub fn install(cache: &LibraryCache, command: &Value) -> Result<Value, VcsError> {
    let url = text(command, "url")?;
    let id = text(command, "id")?;
    let folder = text(command, "folder")?;
    if !valid_id(id) {
        return Err(VcsError::Command(format!(
            "\"{id}\" is not a tutorial's id"
        )));
    }
    let repo = cache
        .open_url(url)
        .ok_or_else(|| VcsError::NotFound(format!("{url} has not been fetched")))?;
    let head = repo
        .head()?
        .ok_or_else(|| VcsError::NotFound(format!("{url} has no version")))?;
    let (_, tutorials, _) = repo.tutorials(head)?;
    let tutorial = tutorials
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| VcsError::NotFound(format!("{url} has no tutorial {id}")))?;
    let target = Path::new(folder).join(id);
    let files = repo.extract(head, &format!("{TUTORIALS_DIR}/{id}"), &target)?;
    let mark = target.join(INSTALLED_MARK);
    let source = json!({"url": url, "rev": head.to_string(), "version": tutorial.version});
    fs::write(&mark, source.to_string()).map_err(|e| crate::io_error("write", &mark, e))?;
    Ok(json!({
        "id": id,
        "folder": target.to_string_lossy(),
        "start": target.join(tutorial.start_page()).to_string_lossy(),
        "files": files,
        "tutorial": tutorial,
        "rev": head.to_string(),
    }))
}
