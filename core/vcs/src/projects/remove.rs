// SPDX-License-Identifier: MIT
//! Remove Project (mitcad#1562, `remove_project`): a folder stops being a
//! MitCad project and keeps its designs. What MitCad made goes: the
//! `.mitcad` folder (the marker, the B-rep store, this computer's
//! settings and caches), MitCad's lines of `.gitattributes` and
//! `.gitignore` (the file too when nothing else is left in it), and the
//! repository with the version history when MitCad made it. A repository
//! that was there before the project keeps its `.git` and history: a
//! version takes the project's files out of it. A Cloud project's remote
//! is never touched.
//!
//! Designs that refer to B-rep data in the project's store are written as
//! single files with that data inside first; the others stay byte for
//! byte. Everything is checked before anything changes: a design whose
//! data cannot be read, or a repository that cannot record the version,
//! refuses the removal with the folder as it was.

use std::fs;
use std::path::{Path, PathBuf};

use mitcad_model::Project;
use mitcad_model::file::{GITATTRIBUTES, GITIGNORE, PROJECT_MARKER, brep_references};
use serde::Serialize;

use super::inspect::{ProjectRemote, remotes_of};
use crate::commit::design_files_in;
use crate::remote::{COUNT_LIMIT, ErrorClass, failure, remove_clone};
use crate::{Identity, ProjectRepo, VcsError, absolute, io_error, same_folder};

/// The message of the version that takes a project's files out of a
/// repository that was there before the project.
pub const REMOVE_MESSAGE: &str = "Remove the MitCad project";

/// The key in a project repository's `.git/config` that says who made the
/// repository: `created` (MitCad, with the project) or `existing` (it was
/// there before).
const MARK_SECTION: &str = "mitcad";
const MARK_KEY: &str = "repository";

/// What happens to a project's repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryFate {
    /// MitCad made it: `.git` with the version history is deleted.
    Removed,
    /// It was there before the project: it keeps its history, and a
    /// version takes the project's files out.
    Kept,
    /// The project has no repository of its own.
    None,
}

/// What [`remove_project`] did, or with `dry_run` would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Removal {
    /// The folder (absolute), which stays.
    pub root: String,
    pub dry_run: bool,
    pub repository: RepositoryFate,
    /// The versions of the history that is deleted (up to 10 000).
    pub versions: Option<usize>,
    /// The paths removed, relative to the folder (`.git`, `.mitcad`, and
    /// `.gitattributes` or `.gitignore` when only MitCad's lines were in
    /// them).
    pub removed: Vec<String>,
    /// `.gitattributes` or `.gitignore` with MitCad's lines taken out, the
    /// others kept.
    pub edited: Vec<String>,
    /// Designs written as single files with their B-rep data inside.
    pub rewritten: Vec<String>,
    /// The designs in the folder, which all stay.
    pub designs: usize,
    /// A Cloud project's remote, which is not touched.
    pub remote: Option<ProjectRemote>,
    /// The version that took the project's files out of a kept repository.
    pub commit: Option<String>,
    pub warnings: Vec<String>,
}

impl ProjectRepo {
    /// Marks the repository as made by MitCad with the project (`created`)
    /// or there before it.
    pub(crate) fn mark_repository(&self, created: bool) -> Result<(), VcsError> {
        let path = self.repo.git_dir().join("config");
        let git_error = |e: &dyn std::fmt::Display| {
            VcsError::Git(format!("cannot change {}: {e}", path.display()))
        };
        let mut config =
            gix::config::File::from_path_no_includes(path.clone(), gix::config::Source::Local)
                .map_err(|e| git_error(&e))?;
        let value = if created { "created" } else { "existing" };
        config
            .set_raw_value_by(MARK_SECTION, None, MARK_KEY, value)
            .map_err(|e| git_error(&e))?;
        let mut text = Vec::new();
        config
            .write_to(&mut text)
            .map_err(|e| io_error("write", &path, e))?;
        mitcad_model::file::write_atomically(&path, &text).map_err(|e| io_error("write", &path, e))
    }

    /// Whether MitCad made the repository: its mark, else (a project made
    /// before the mark, or a clone of a Cloud project) whether its history
    /// began as a MitCad project, every first version holding the marker.
    fn made_by_mitcad(&self) -> Result<bool, VcsError> {
        let path = self.repo.git_dir().join("config");
        let mark = gix::config::File::from_path_no_includes(path, gix::config::Source::Local)
            .ok()
            .and_then(|config| {
                config
                    .string(format!("{MARK_SECTION}.{MARK_KEY}").as_str())
                    .map(|value| value.to_string())
            });
        match mark.as_deref() {
            Some("created") => return Ok(true),
            Some("existing") => return Ok(false),
            _ => {}
        }
        let Some(head) = self.head_commit()? else {
            return Ok(false);
        };
        let mut first = 0;
        for info in self.repo.rev_walk([head]).all()? {
            let info = info?;
            if info.parent_ids().next().is_some() {
                continue;
            }
            first += 1;
            let tree = self.repo.find_commit(info.id)?.tree_id()?.detach();
            if self.blob_entry(tree, PROJECT_MARKER)?.is_none() {
                return Ok(false);
            }
        }
        Ok(first > 0)
    }

    /// The versions on HEAD's history, up to [`COUNT_LIMIT`].
    fn version_count(&self) -> Result<usize, VcsError> {
        let Some(head) = self.head_commit()? else {
            return Ok(0);
        };
        let mut count = 0;
        for info in self.repo.rev_walk([head]).all()? {
            info?;
            count += 1;
            if count >= COUNT_LIMIT {
                break;
            }
        }
        Ok(count)
    }

    /// The files under `.mitcad` in HEAD's version, relative to the
    /// project.
    fn recorded_project_files(&self) -> Result<Vec<String>, VcsError> {
        let Some(head) = self.head_commit()? else {
            return Ok(Vec::new());
        };
        let tree = self.repo.find_commit(head)?.tree_id()?.detach();
        let Some(folder) = self.tree_entry(tree, ".mitcad")? else {
            return Ok(Vec::new());
        };
        Ok(self
            .differences(Some(folder), None)?
            .into_iter()
            .map(|difference| format!(".mitcad/{}", difference.path))
            .collect())
    }
}

/// A file with MitCad's lines (`template`) taken out: None when nothing
/// else is left, the same text when it has none of them.
fn without_lines(text: &str, template: &str) -> Option<String> {
    let ours: Vec<&str> = template
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| !ours.contains(&line.trim()))
        .collect();
    if kept.iter().all(|line| line.trim().is_empty()) {
        return None;
    }
    if kept.len() == text.lines().count() {
        return Some(text.to_owned());
    }
    let end = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = kept.join(end);
    out.push_str(end);
    Some(out)
}

/// What becomes of `.gitattributes` and `.gitignore`: (name, None to
/// remove it, or its new text).
fn plan_git_files(root: &Path) -> Result<Vec<(&'static str, Option<String>)>, VcsError> {
    let mut out = Vec::new();
    for (name, template) in [(".gitattributes", GITATTRIBUTES), (".gitignore", GITIGNORE)] {
        let path = root.join(name);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(io_error("read", &path, e)),
        };
        match without_lines(&text, template) {
            None => out.push((name, None)),
            Some(kept) if kept != text => out.push((name, Some(kept))),
            Some(_) => {}
        }
    }
    Ok(out)
}

/// Deletes folder `path` whole (read-only files of git too); an error
/// names what is left.
fn remove_folder(path: &Path) -> Result<(), VcsError> {
    if !path.exists() {
        return Ok(());
    }
    remove_clone(path, false);
    if path.exists() {
        return Err(failure(
            ErrorClass::Other,
            format!(
                "{} could not be deleted (a program may be using it): close it and delete the \
                 folder by hand",
                path.display()
            ),
        ));
    }
    Ok(())
}

/// Whether `path` is a link (never followed: only what MitCad made goes).
fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// Takes the project out of folder `dir` (a project's folder): see the
/// module. With `dry_run` nothing changes and the answer says what would
/// happen (Remove Project's confirmation). `author` records the version in
/// a kept repository (else the repository's author). Refused, nothing
/// changed: a folder that is no project (`not_a_project`), a design whose
/// B-rep data cannot be put inside it, a kept repository that cannot
/// record a version now (`unsupported`).
pub fn remove_project(
    dir: &Path,
    dry_run: bool,
    author: Option<&Identity>,
) -> Result<Removal, VcsError> {
    let dir = absolute(dir)?;
    let Some(project) = Project::open(&dir) else {
        return Err(failure(
            ErrorClass::NotAProject,
            format!(
                "{} is not a MitCad project (it has no {PROJECT_MARKER})",
                dir.display()
            ),
        ));
    };
    let mitcad = dir.join(".mitcad");
    if is_link(&mitcad) {
        return Err(failure(
            ErrorClass::Unsupported,
            format!(
                "{} is a link: MitCad removes only a folder it made",
                mitcad.display()
            ),
        ));
    }
    let git = dir.join(".git");
    // The project's own repository, if its folder is the root of one.
    let repo = gix::open(&dir)
        .ok()
        .filter(|repo| repo.workdir().is_some_and(|w| same_folder(w, &dir)))
        .map(|repo| ProjectRepo::new(repo, dir.clone()));
    let mut warnings = Vec::new();
    let (fate, versions, remote) = match &repo {
        Some(repo) => {
            let made = repo.made_by_mitcad()?
                && fs::symlink_metadata(&git).is_ok_and(|m| m.file_type().is_dir());
            let remote = remotes_of(&repo.repo).0;
            if made {
                (RepositoryFate::Removed, Some(repo.version_count()?), remote)
            } else {
                if let Some(reason) = repo.why_not_commit()? {
                    return Err(failure(
                        ErrorClass::Unsupported,
                        format!(
                            "{} is a git repository that was there before the project: MitCad \
                             takes the project's files out of it as a version, which cannot be \
                             recorded now ({reason}); finish that in git first",
                            dir.display()
                        ),
                    ));
                }
                (RepositoryFate::Kept, None, remote)
            }
        }
        None => {
            if git.exists() {
                warnings.push(format!(
                    "{} is no repository MitCad can read, so it is left as it is",
                    git.display()
                ));
            }
            (RepositoryFate::None, None, None)
        }
    };
    let author = match (&repo, fate) {
        (Some(repo), RepositoryFate::Kept) => Some(match author {
            Some(author) => author.clone(),
            None => repo.identity(None)?,
        }),
        _ => None,
    };
    // The designs, and those that need their B-rep data put inside.
    let designs = design_files_in(&dir);
    let store = project.store();
    let mut rewrites: Vec<(PathBuf, String)> = Vec::new();
    for design in &designs {
        let text = fs::read(design).map_err(|e| io_error("read", design, e))?;
        // A file this MitCad cannot read may keep data in the store too.
        let unreadable = |why: String| {
            failure(
                ErrorClass::Unsupported,
                format!(
                    "{} is no design file this MitCad reads ({why}), so whether it keeps data \
                     in the project is not known: move it out of the folder or delete it first",
                    design.display()
                ),
            )
        };
        let Ok(text) = String::from_utf8(text) else {
            return Err(unreadable("not UTF-8 text".to_owned()));
        };
        match brep_references(&text) {
            Ok(references) if references.is_empty() => continue,
            Ok(_) => {}
            Err(e) => return Err(unreadable(e.to_string())),
        }
        match mitcad_model::file::single_file_text(&text, &store) {
            Ok(Some(single)) => rewrites.push((design.clone(), single)),
            Ok(None) => {}
            Err(e) => {
                return Err(failure(
                    ErrorClass::Unsupported,
                    format!(
                        "{} keeps data in the project, which removing the project would lose: {e}",
                        design.display()
                    ),
                ));
            }
        }
    }
    let git_files = plan_git_files(&dir)?;
    let relative = |path: &Path| {
        path.strip_prefix(&dir)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path.to_string_lossy().into_owned())
    };
    let mut removal = Removal {
        root: dir.to_string_lossy().into_owned(),
        dry_run,
        repository: fate,
        versions,
        removed: Vec::new(),
        edited: Vec::new(),
        rewritten: rewrites.iter().map(|(path, _)| relative(path)).collect(),
        designs: designs.len(),
        remote,
        commit: None,
        warnings,
    };
    removal.rewritten.sort();
    if fate == RepositoryFate::Removed {
        removal.removed.push(".git".to_owned());
    }
    removal.removed.push(".mitcad".to_owned());
    for (name, kept) in &git_files {
        match kept {
            None => removal.removed.push((*name).to_owned()),
            Some(_) => removal.edited.push((*name).to_owned()),
        }
    }
    if dry_run {
        return Ok(removal);
    }

    // The designs first: whatever fails later, none has lost its data.
    for (path, text) in &rewrites {
        mitcad_model::file::write_atomically(path, text.as_bytes())
            .map_err(|e| io_error("write", path, e))?;
    }
    for (name, kept) in &git_files {
        let path = dir.join(name);
        match kept {
            None => fs::remove_file(&path).map_err(|e| io_error("delete", &path, e))?,
            Some(text) => mitcad_model::file::write_atomically(&path, text.as_bytes())
                .map_err(|e| io_error("write", &path, e))?,
        }
    }
    match (repo, fate) {
        (Some(repo), RepositoryFate::Kept) => {
            let mut files: Vec<PathBuf> = repo
                .recorded_project_files()?
                .iter()
                .map(|path| dir.join(path))
                .collect();
            // Only what the latest version holds: a file git does not
            // track stays untracked.
            let head = repo.head_commit()?;
            let others = rewrites
                .iter()
                .map(|(path, _)| path.clone())
                .chain(git_files.iter().map(|(name, _)| dir.join(name)));
            for path in others {
                let recorded = match head {
                    Some(head) => repo.entry_at(head, &repo.relative(&path)?)?.is_some(),
                    None => false,
                };
                if recorded {
                    files.push(path);
                }
            }
            remove_folder(&mitcad)?;
            let author = author.expect("a kept repository's author");
            let outcome = repo.commit(&files, REMOVE_MESSAGE, &author)?;
            if let Some(reason) = outcome.skipped {
                removal.warnings.push(format!(
                    "the project's files are gone from the folder, but no version records it: \
                     {reason}"
                ));
            }
            removal.commit = outcome.commit;
            removal.warnings.extend(outcome.warnings);
            drop(repo);
            // The commit may have written its cache there again.
            remove_folder(&mitcad)?;
        }
        (repo, _) => {
            // The repository is closed before its folder goes.
            drop(repo);
            if fate == RepositoryFate::Removed {
                remove_folder(&git)?;
            }
            remove_folder(&mitcad)?;
        }
    }
    Ok(removal)
}
