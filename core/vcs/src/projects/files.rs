// SPDX-License-Identifier: MIT
//! Other files of a project (mitcad#2204): a photo, a datasheet, a
//! spreadsheet or an archive kept with the designs, versioned and synced.
//! MitCad never reads their contents; it lists them (`project_tree`'s
//! `files`, `listing.rs`), copies them in (`add_files`), and records the
//! changes another program made to them (`record`), each one version with
//! the rules of the other folder commands (`operations.rs`). Moving,
//! renaming and deleting them are `move` and `delete`, as for designs.
//!
//! - **Listed** are the files that are no design, have no name starting
//!   with a dot on their path (so nothing under `.mitcad/`), are not files
//!   the operating system leaves in folders (`Thumbs.db`), and that the
//!   project's `.gitignore` files (and git's own exclude files) do not
//!   leave out: an ignored file would otherwise show as new forever.
//! - **Changed**: a file differs from the latest version when its size
//!   does, or, unless the index's file times show it untouched since it
//!   was recorded, when its content does (hashed as a stream, so a large
//!   file is not read into memory).
//! - **Large files**: hosting services refuse a file over 100 MiB on push
//!   (GitHub), and the history keeps every copy of a file for good, so
//!   adding or recording one is refused (`too_large`) unless the command's
//!   `allow_large` is true (the application asks first).

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use gix::ObjectId;
use gix::bstr::ByteSlice;
use mitcad_model::file::rename_retrying;
use serde_json::{Value, json};

use super::is_system_file;
use super::listing::walk_from;
use super::operations::{is_under, refuse, refused_answer};
use crate::remote::{Control, ErrorClass, RemoteError};
use crate::{ProjectRepo, VcsError, io_error, is_design_file};

/// The size above which hosting services refuse a file on push (GitHub:
/// 100 MiB) and the application asks before recording it.
pub const LARGE_FILE: u64 = 100 * 1024 * 1024;

/// How many names a version's default message lists.
const NAMES_IN_MESSAGE: usize = 3;

/// The project's ignore rules: its `.gitignore` files, the repository's
/// `info/exclude` and git's global exclude file. Without them (an
/// unreadable configuration) nothing is ignored.
pub(crate) struct Ignores<'r> {
    stack: Option<gix::AttributeStack<'r>>,
}

impl<'r> Ignores<'r> {
    pub(crate) fn of(repo: &'r gix::Repository) -> Self {
        // No index entries: the `.gitignore` files are read from the
        // folder, as the files listed are.
        let state = gix::index::State::new(repo.object_hash());
        let stack = repo
            .excludes(
                &state,
                None,
                gix::worktree::stack::state::ignore::Source::WorktreeThenIdMappingIfNotSkipped,
            )
            .ok();
        Self { stack }
    }

    /// Whether `path` (relative to the project, `/`) is ignored; `folder`:
    /// it is a folder. A file in an ignored folder is the caller's to
    /// leave out (git's rule: nothing below an ignored folder is seen).
    pub(crate) fn ignored(&mut self, path: &str, folder: bool) -> bool {
        let Some(stack) = self.stack.as_mut() else {
            return false;
        };
        let mode = if folder {
            gix::index::entry::Mode::DIR
        } else {
            gix::index::entry::Mode::FILE
        };
        stack
            .at_entry(path, Some(mode))
            .is_ok_and(|platform| platform.is_excluded())
    }

    /// Whether `path` or a folder above it is ignored.
    pub(crate) fn ignored_with_folders(&mut self, path: &str, folder: bool) -> bool {
        let mut above = String::new();
        let parts: Vec<&str> = path.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            if !above.is_empty() {
                above.push('/');
            }
            above.push_str(part);
            let last = i + 1 == parts.len();
            if self.ignored(&above, !last || folder) {
                return true;
            }
        }
        false
    }
}

/// Whether a file is listed as one of the project's other files by its
/// name alone: no design, not hidden, not a file of the operating system.
pub(crate) fn is_other_file(path: &str) -> bool {
    !is_design_file(path)
        && path.split('/').all(|part| !part.starts_with('.'))
        && !is_system_file(path.rsplit('/').next().unwrap_or(path))
}

/// A version's default message: `<verb> a, b and c`, at most
/// [`NAMES_IN_MESSAGE`] names and how many more.
fn message_of(verb: &str, paths: &[String]) -> String {
    match paths {
        [] => verb.to_owned(),
        [one] => format!("{verb} {one}"),
        _ if paths.len() <= NAMES_IN_MESSAGE => {
            let (last, first) = paths.split_last().expect("several");
            format!("{verb} {} and {last}", first.join(", "))
        }
        _ => format!(
            "{verb} {} and {} more files",
            paths[..NAMES_IN_MESSAGE].join(", "),
            paths.len() - NAMES_IN_MESSAGE
        ),
    }
}

/// The refusal of files over [`LARGE_FILE`] without `allow_large`.
fn too_large(names: &[String]) -> VcsError {
    refuse(
        ErrorClass::TooLarge,
        format!(
            "{} {} larger than 100 MiB: hosting services such as GitHub refuse files that large, \
             and the history keeps every copy of a file for good; record {} anyway only if this \
             project stays on this computer or on a server that takes them (allow_large)",
            names.join(", "),
            if names.len() == 1 { "is" } else { "are" },
            if names.len() == 1 { "it" } else { "them" },
        ),
    )
}

fn allows_large(command: &Value) -> bool {
    command
        .get("allow_large")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

impl ProjectRepo {
    /// The index, to tell untouched files without reading them; None
    /// without one.
    pub(crate) fn index_for_check(&self) -> Option<gix::index::File> {
        if !self.repo.index_path().is_file() {
            return None;
        }
        self.repo.open_index().ok()
    }

    /// The files of a tree other than designs, with their blobs, leaving
    /// out every name starting with a dot (`.mitcad`, `.gitignore`).
    pub(crate) fn other_files(
        &self,
        tree: ObjectId,
    ) -> Result<HashMap<String, ObjectId>, VcsError> {
        let mut out = HashMap::new();
        let mut folders = vec![(String::new(), tree)];
        while let Some((prefix, folder)) = folders.pop() {
            for (name, (mode, id)) in self.entries(folder)? {
                let name = name.to_str_lossy();
                if name.starts_with('.') {
                    continue;
                }
                let path = format!("{prefix}{name}");
                if mode.is_tree() {
                    folders.push((format!("{path}/"), id));
                } else if mode.is_blob() && is_other_file(&path) {
                    out.insert(path, id);
                }
            }
        }
        Ok(out)
    }

    /// Whether the file at `full` (`relative` in the project, `metadata`
    /// its own) differs from `recorded`, its blob in the latest version
    /// (None: not recorded, so it is).
    pub(crate) fn file_changed(
        &self,
        relative: &str,
        full: &Path,
        metadata: &fs::Metadata,
        recorded: Option<ObjectId>,
        index: Option<&gix::index::File>,
    ) -> bool {
        let Some(blob) = recorded else {
            return true;
        };
        if let Ok(Some(header)) = self.repo.try_find_header(blob)
            && header.size() != metadata.len()
        {
            return true;
        }
        // The index holds the file times the commit saw: equal ones, and
        // the file not touched in the second the index was written, mean
        // the file is the version's (git's own rule).
        if let Some(index) = index
            && let Some(entry) = index.entry_by_path(relative.as_bytes().as_bstr())
            && entry.id == blob
            && let Ok(now) = gix::index::fs::Metadata::from_path_no_follow(full)
            && let Ok(stat) = gix::index::entry::Stat::from_fs(&now)
        {
            let options = gix::index::entry::stat::Options::default();
            if entry.stat.matches(&stat, options) && !entry.stat.is_racy(index.timestamp(), options)
            {
                return false;
            }
        }
        let Ok(mut file) = fs::File::open(full) else {
            return true;
        };
        let hashed = gix::objs::compute_stream_hash(
            self.repo.object_hash(),
            gix::objs::Kind::Blob,
            &mut file,
            metadata.len(),
            &mut gix::progress::Discard,
            &AtomicBool::new(false),
        );
        hashed.ok() != Some(blob)
    }

    /// `add_files`: copies `files` (paths of files anywhere) into `folder`
    /// of the project ("" or left out: its top), recorded as one version,
    /// `Add <path>`. Nothing is replaced: a name that is there already is
    /// refused, as are folders, designs, hidden names, names the project's
    /// `.gitignore` leaves out and, without `allow_large`, files over
    /// [`LARGE_FILE`].
    pub(super) fn add_files(
        &self,
        command: &Value,
        control: Option<&Control>,
    ) -> Result<(Value, Option<RemoteError>), VcsError> {
        let root = self.root().to_path_buf();
        let folder = match command.get("folder").and_then(Value::as_str) {
            None | Some("") => String::new(),
            Some(_) => self.project_path(command, "folder")?,
        };
        let folder_full = if folder.is_empty() {
            root.clone()
        } else {
            root.join(&folder)
        };
        if !folder_full.is_dir() {
            return Err(refuse(
                ErrorClass::NotFound,
                format!("{folder} is not a folder of the project"),
            ));
        }
        let sources = command
            .get("files")
            .and_then(Value::as_array)
            .filter(|files| !files.is_empty())
            .ok_or_else(|| {
                VcsError::Command("add_files needs \"files\" (a list of files)".to_owned())
            })?
            .iter()
            .map(|file| file.as_str().map(PathBuf::from))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| VcsError::Command("\"files\" are paths (text)".to_owned()))?;
        let mut ignores = Ignores::of(&self.repo);
        let mut targets: Vec<(PathBuf, String)> = Vec::new();
        let mut names = BTreeSet::new();
        let mut large = Vec::new();
        for source in &sources {
            let shown = source.display().to_string();
            let metadata = fs::metadata(source).map_err(|_| {
                refuse(
                    ErrorClass::NotFound,
                    format!("{shown} is not there or cannot be read"),
                )
            })?;
            if !metadata.is_file() {
                return Err(refuse(
                    ErrorClass::InvalidPath,
                    format!("{shown}: only files are added, not folders"),
                ));
            }
            let Some(name) = source.file_name().and_then(|name| name.to_str()) else {
                return Err(refuse(
                    ErrorClass::InvalidPath,
                    format!("{shown}: a name every system can hold is needed"),
                ));
            };
            let wanted = if folder.is_empty() {
                name.to_owned()
            } else {
                format!("{folder}/{name}")
            };
            let path = self.project_path(&json!({"path": wanted}), "path")?;
            if is_design_file(&path) {
                return Err(refuse(
                    ErrorClass::InvalidPath,
                    format!(
                        "{path}: a design is not added as a file; open it and save it into the \
                         project, so its geometry comes along"
                    ),
                ));
            }
            if is_system_file(name) || ignores.ignored_with_folders(&path, false) {
                return Err(refuse(
                    ErrorClass::InvalidPath,
                    format!(
                        "{path}: the project's .gitignore leaves this name out of its versions"
                    ),
                ));
            }
            // Also another case of the same name on a system that does not
            // tell them apart.
            if fs::symlink_metadata(root.join(&path)).is_ok() || !names.insert(path.to_lowercase())
            {
                return Err(refuse(
                    ErrorClass::Exists,
                    format!("{path} is there already; nothing was replaced"),
                ));
            }
            if metadata.len() > LARGE_FILE {
                large.push(path.clone());
            }
            targets.push((source.clone(), path));
        }
        if !large.is_empty() && !allows_large(command) {
            return Err(too_large(&large));
        }
        let author = self.ready(command)?;
        // Copied under a hidden temporary name (`.*.tmp`, which the
        // project's .gitignore leaves out), then renamed: a file is either
        // whole or not there. On a failure the files copied so far go.
        let mut placed: Vec<PathBuf> = Vec::new();
        let copied = (|| -> Result<(), VcsError> {
            for (index, (source, path)) in targets.iter().enumerate() {
                let full = root.join(path);
                let name = path.rsplit('/').next().unwrap_or(path);
                let temporary =
                    folder_full.join(format!(".{name}.{}-{index}.tmp", std::process::id()));
                if let Err(e) = fs::copy(source, &temporary) {
                    let _ = fs::remove_file(&temporary);
                    return Err(VcsError::Io(format!(
                        "cannot copy {} to {}: {e}",
                        source.display(),
                        full.display()
                    )));
                }
                if let Err(e) = rename_retrying(&temporary, &full) {
                    let _ = fs::remove_file(&temporary);
                    return Err(io_error("write", &full, e));
                }
                placed.push(full);
            }
            Ok(())
        })();
        if let Err(error) = copied {
            for file in &placed {
                let _ = fs::remove_file(file);
            }
            return Err(error);
        }
        let added: Vec<String> = targets.iter().map(|(_, path)| path.clone()).collect();
        let (mut value, error) = self.record_version(
            &placed,
            &message_of("Add", &added),
            &author,
            command,
            control,
        )?;
        value["folder"] = json!(folder);
        value["added"] = json!(added);
        Ok((value, error))
    }

    /// `record`: the changes of the project's other files at `path` (a
    /// file, or a folder with everything in it; "" or left out: the whole
    /// project) recorded as one version, `Record <path>` (`Record changes
    /// in <folder>`, `Record changes`): new and changed files, and those
    /// the latest version holds that are gone from the folder. Designs
    /// are recorded by Save and stay as they are.
    pub(super) fn record_files(
        &self,
        command: &Value,
        control: Option<&Control>,
    ) -> Result<(Value, Option<RemoteError>), VcsError> {
        let root = self.root().to_path_buf();
        let path = match command.get("path").and_then(Value::as_str) {
            None | Some("") => String::new(),
            Some(_) => self.project_path(command, "path")?,
        };
        if !path.is_empty() && is_design_file(&path) {
            return Err(refuse(
                ErrorClass::InvalidPath,
                format!("{path}: a design's changes are recorded when it is saved"),
            ));
        }
        let full = if path.is_empty() {
            root.clone()
        } else {
            root.join(&path)
        };
        let metadata = fs::symlink_metadata(&full).ok();
        let folder = path.is_empty() || metadata.as_ref().is_some_and(fs::Metadata::is_dir);
        let recorded: HashMap<String, ObjectId> = match self.head_tree()? {
            Some(tree) => self.other_files(tree)?,
            None => HashMap::new(),
        };
        let mut ignores = Ignores::of(&self.repo);
        // The files of the folder or the file itself, as the listing has
        // them.
        let present: Vec<(String, PathBuf)> = if folder {
            walk_from(&root, &path, &mut |p, f| ignores.ignored(p, f)).files
        } else if metadata.as_ref().is_some_and(fs::Metadata::is_file) {
            if !is_other_file(&path) || ignores.ignored_with_folders(&path, false) {
                return Err(refuse(
                    ErrorClass::InvalidPath,
                    format!(
                        "{path}: the project's .gitignore leaves this file out of its versions"
                    ),
                ));
            }
            vec![(path.clone(), full.clone())]
        } else if metadata.is_none() && recorded.contains_key(&path) {
            Vec::new()
        } else {
            return Err(refuse(
                ErrorClass::NotFound,
                format!("{path} is neither in the project's folder nor in its latest version"),
            ));
        };
        let index = self.index_for_check();
        let mut changes: Vec<String> = Vec::new();
        let mut large = Vec::new();
        for (relative, file) in &present {
            let Ok(metadata) = fs::metadata(file) else {
                continue;
            };
            if self.file_changed(
                relative,
                file,
                &metadata,
                recorded.get(relative).copied(),
                index.as_ref(),
            ) {
                if metadata.len() > LARGE_FILE {
                    large.push(relative.clone());
                }
                changes.push(relative.clone());
            }
        }
        // Gone from the folder: a deletion to record.
        let within = |file: &str| path.is_empty() || is_under(file, &path);
        for file in recorded.keys().filter(|file| within(file)) {
            if fs::symlink_metadata(root.join(file)).is_err() {
                changes.push(file.clone());
            }
        }
        changes.sort();
        changes.dedup();
        if !large.is_empty() && !allows_large(command) {
            return Err(too_large(&large));
        }
        if changes.is_empty() {
            let mut value = refused_answer();
            value["path"] = json!(path);
            value["recorded"] = json!(changes);
            return Ok((value, None));
        }
        let author = self.ready(command)?;
        let files: Vec<PathBuf> = changes.iter().map(|file| root.join(file)).collect();
        let message = if !folder {
            format!("Record {path}")
        } else if path.is_empty() {
            "Record changes".to_owned()
        } else {
            format!("Record changes in {path}")
        };
        let (mut value, error) =
            self.record_version(&files, &message, &author, command, control)?;
        value["path"] = json!(path);
        value["recorded"] = json!(changes);
        Ok((value, error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_name_at_most_three_files() {
        let names = |n: usize| (1..=n).map(|i| format!("f{i}.pdf")).collect::<Vec<_>>();
        assert_eq!(message_of("Add", &names(1)), "Add f1.pdf");
        assert_eq!(message_of("Add", &names(2)), "Add f1.pdf and f2.pdf");
        assert_eq!(
            message_of("Add", &names(3)),
            "Add f1.pdf, f2.pdf and f3.pdf"
        );
        assert_eq!(
            message_of("Add", &names(5)),
            "Add f1.pdf, f2.pdf, f3.pdf and 2 more files"
        );
    }

    #[test]
    fn other_files_are_no_designs_hidden_or_system_files() {
        assert!(is_other_file("photo.png"));
        assert!(is_other_file("docs/datasheet.pdf"));
        assert!(!is_other_file("part.mitcad"));
        assert!(!is_other_file("docs/Part.MITCAD"));
        assert!(!is_other_file(".gitignore"));
        assert!(!is_other_file(".mitcad/brep/aa/x"));
        assert!(!is_other_file("docs/.hidden/a.txt"));
        assert!(!is_other_file("docs/Thumbs.db"));
        assert!(!is_other_file("desktop.ini"));
    }
}
