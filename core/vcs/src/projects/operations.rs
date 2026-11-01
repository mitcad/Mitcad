// SPDX-License-Identifier: MIT
//! Folder and design operations of a project (mitcad#144), for the
//! Projects panel and `mitcad-cli project`: a new folder, a new design, a
//! design or a folder moved, and one deleted, each recorded as one version
//! with Save's rules ([`ProjectRepo::commit`]: the index locked, only the
//! version's paths change, nothing recorded while HEAD is detached or a
//! merge is in progress) and, in a Cloud project, pushed as a Save is.
//!
//! - **Paths** are relative to the project's folder with `/` (or absolute
//!   inside it); a name starting with a dot (hidden: the commit's walk
//!   and the listing skip it), a link on the way, and names some systems
//!   cannot hold are refused, so a project opens everywhere.
//! - **Folders** are kept in the versions by an empty placeholder file
//!   ([`FOLDER_PLACEHOLDER`]), which the listing never shows; a move or a
//!   delete that would leave its folder with nothing recorded adds one.
//! - **Refusals** change nothing and answer `error` (`exists`, `locked`,
//!   `invalid_path`, `not_found`, `local_changes`, `unsupported`).
//! - A moved design's per-user display state and the project's design
//!   opened last follow it; the B-rep store stays consistent because the
//!   old and the new paths go into the same version (the commit keeps the
//!   B-rep files the new paths refer to).

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use gix::ObjectId;
use mitcad_model::Project;
use mitcad_model::file::settings::ProjectSettings;
use mitcad_model::file::{
    brep_references, empty_design, empty_design_in, parse_length_unit, rename_retrying,
    write_atomically,
};
use serde_json::{Value, json};

use super::inspect::{last_design, remember, remotes_of, valid_relative};
use super::is_system_file;
use super::listing::FOLDER_PLACEHOLDER;
use crate::api::{optional, text};
use crate::remote::locks::lock_id;
use crate::remote::{Control, ErrorClass, RemoteError};
use crate::transaction::FileTransaction;
use crate::{CommitOutcome, Identity, ProjectRepo, VcsError, io_error, is_design_file};

/// Characters a name may not hold on some system (Windows), so that a
/// project opens everywhere.
const FORBIDDEN: [char; 8] = ['<', '>', ':', '"', '|', '?', '*', '\\'];

/// A refusal: an answer's `error`, nothing changed.
fn refuse(class: ErrorClass, message: impl Into<String>) -> VcsError {
    VcsError::Remote(RemoteError::new(class, message))
}

/// A name Windows keeps for devices (`CON`, `NUL`, `COM1`, ...), also with
/// an extension.
fn reserved(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix)
            .is_some_and(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit() && n != "0")
    };
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered("COM") || numbered("LPT")
}

/// Whether no part of a relative path is hidden (starts with a dot).
fn visible(path: &str) -> bool {
    path.split('/').all(|part| !part.starts_with('.'))
}

/// The folder a relative path is in ("" for the project's root).
fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// Whether `path` is `folder` or below it.
fn is_under(path: &str, folder: &str) -> bool {
    path == folder
        || path
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// The files under folder `full` (relative to the project, below `path`),
/// hidden ones too; links are files here, never followed.
fn files_on_disk(full: &Path, path: &str) -> Result<Vec<String>, VcsError> {
    let mut out = Vec::new();
    let mut folders = vec![(full.to_path_buf(), path.to_owned())];
    while let Some((folder, prefix)) = folders.pop() {
        let entries = fs::read_dir(&folder).map_err(|e| io_error("read", &folder, e))?;
        for entry in entries {
            let entry = entry.map_err(|e| io_error("read", &folder, e))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = format!("{prefix}/{name}");
            let kind = entry
                .file_type()
                .map_err(|e| io_error("read", &entry.path(), e))?;
            if kind.is_dir() {
                folders.push((entry.path(), relative));
            } else {
                out.push(relative);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// What an operation's answer starts with when it is refused.
fn refused_answer() -> Value {
    json!({"commit": null, "skipped": null, "written": [], "removed": [], "deleted": [],
           "warnings": [], "push": null})
}

impl ProjectRepo {
    /// Runs a command of a project's folders and designs (commands.md,
    /// "Folders and designs"): its answer and the failure it reports. None
    /// when `name` is not one.
    pub(crate) fn folder_command(
        &self,
        name: &str,
        command: &Value,
        control: Option<&Control>,
    ) -> Result<Option<(Value, Option<RemoteError>)>, VcsError> {
        let result = match name {
            "project_tree" => return Ok(Some((self.project_tree(command)?, None))),
            "create_folder" => self.create_folder(command, control),
            "new_design" => self.new_design(command, control),
            "move" => self.move_entry(command, control),
            "delete" => self.delete_entry(command, control),
            _ => return Ok(None),
        };
        let (mut value, error) = match result {
            Ok(done) => done,
            Err(VcsError::Remote(error)) => (refused_answer(), Some(error)),
            Err(other) => return Err(other),
        };
        value["error"] = json!(error);
        value["log"] = json!(self.take_remote_log());
        Ok(Some((value, error)))
    }

    /// A command's path field `key` as a path of the project: relative to
    /// its folder with `/` (an absolute path inside it also does); hidden
    /// names, links on the way and names not every system can hold are
    /// refused.
    fn project_path(&self, command: &Value, key: &str) -> Result<String, VcsError> {
        let given = text(command, key)?;
        let invalid = |why: &str| refuse(ErrorClass::InvalidPath, format!("{given}: {why}"));
        let unholdable = |part: &str| {
            invalid(&format!(
                "{part} is not a name every system can hold (no <>:\"|?*\\, no dot or \
                 space at the end, not a device name such as CON or NUL)"
            ))
        };
        // Windows drops a dot or a space at the end of a name when it
        // resolves a path, so those ends are checked on the names as given.
        if let Some(part) = given
            .split(['/', '\\'])
            .find(|p| *p != "." && *p != ".." && (p.ends_with('.') || p.ends_with(' ')))
        {
            return Err(unholdable(part));
        }
        let relative = self
            .relative(&self.root().join(given))
            .map_err(|_| invalid("not a path inside the project"))?;
        if !valid_relative(&relative) {
            return Err(invalid("not a path inside the project"));
        }
        let mut full = self.root().to_path_buf();
        for part in relative.split('/') {
            if part.starts_with('.') {
                return Err(invalid(
                    "a name starting with a dot is hidden, and the project's versions leave it out",
                ));
            }
            if part.ends_with('.')
                || part.ends_with(' ')
                || part
                    .chars()
                    .any(|c| FORBIDDEN.contains(&c) || c.is_control())
                || reserved(part)
            {
                return Err(unholdable(part));
            }
            full.push(part);
            if fs::symlink_metadata(&full).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(invalid(
                    "a link is on the way; MitCad does not follow links",
                ));
            }
        }
        Ok(relative)
    }

    /// The author of the version and whether one can be recorded now
    /// (Save's rule: not while HEAD is detached or a merge is in
    /// progress), before anything is written.
    fn ready(&self, command: &Value) -> Result<Identity, VcsError> {
        let author = self.author(command)?;
        if let Some(reason) = self.why_not_commit()? {
            return Err(refuse(
                ErrorClass::Unsupported,
                format!("{reason}; nothing was changed"),
            ));
        }
        Ok(author)
    }

    /// The tree of the latest version.
    fn head_tree(&self) -> Result<Option<ObjectId>, VcsError> {
        Ok(match self.head_commit()? {
            Some(head) => Some(self.repo.find_commit(head)?.tree_id()?.detach()),
            None => None,
        })
    }

    /// The files of the latest version at `path` (a file) or below it (a
    /// folder), with their blobs.
    fn recorded_under(&self, path: &str) -> Result<Vec<(String, ObjectId)>, VcsError> {
        let Some(tree) = self.head_tree()? else {
            return Ok(Vec::new());
        };
        if let Some(blob) = self.blob_entry(tree, path)? {
            return Ok(vec![(path.to_owned(), blob)]);
        }
        let Some(folder) = self.tree_entry(tree, path)? else {
            return Ok(Vec::new());
        };
        let is_tree = self
            .repo
            .try_find_header(folder)
            .ok()
            .flatten()
            .is_some_and(|header| header.kind() == gix::object::Kind::Tree);
        if !is_tree {
            return Ok(Vec::new());
        }
        Ok(self
            .differences(None, Some(folder))?
            .into_iter()
            .filter_map(|d| d.new.map(|blob| (format!("{path}/{}", d.path), blob)))
            .collect())
    }

    /// Refuses when one of `designs` is locked by someone else: a lock of
    /// another session (the command's `session`) and another person (the
    /// author's email), as the last lock poll saw it. Only with the
    /// project's edit locks on.
    fn check_locks(&self, designs: &[String], command: &Value) -> Result<(), VcsError> {
        if designs.is_empty() || !self.lock_settings().enabled {
            return Ok(());
        }
        let locks = self.known_locks()?;
        if locks.is_empty() {
            return Ok(());
        }
        let session = optional(command, "session");
        let me = self.author(command).ok();
        let held: Vec<String> = designs
            .iter()
            .filter_map(|path| {
                let lock = locks.get(&lock_id(path))?;
                let mine = session.is_some_and(|session| session == lock.session)
                    || me
                        .as_ref()
                        .is_some_and(|me| me.email.eq_ignore_ascii_case(&lock.owner.email));
                (!mine).then(|| format!("{path} by {} <{}>", lock.owner.name, lock.owner.email))
            })
            .collect();
        if held.is_empty() {
            return Ok(());
        }
        Err(refuse(
            ErrorClass::Locked,
            format!(
                "locked by someone else (edit locks): {}; ask for the lock first",
                held.join(", ")
            ),
        ))
    }

    /// When the version would leave `folder` (not the project's root)
    /// with nothing recorded while it stays in the project's folder, its
    /// placeholder is written and added to `files`. `gone`: the paths the
    /// version removes; `added`: those it adds.
    fn keep_folder(
        &self,
        folder: &str,
        gone: &[String],
        added: &[String],
        files: &mut Vec<PathBuf>,
    ) -> Result<(), VcsError> {
        let full = self.root().join(folder);
        if folder.is_empty() || !full.is_dir() {
            return Ok(());
        }
        let stays = self
            .recorded_under(folder)?
            .into_iter()
            .any(|(path, _)| !gone.contains(&path))
            || added.iter().any(|path| is_under(path, folder));
        if stays {
            return Ok(());
        }
        let placeholder = full.join(FOLDER_PLACEHOLDER);
        if !placeholder.is_file() {
            write_atomically(&placeholder, b"").map_err(|e| io_error("write", &placeholder, e))?;
        }
        files.push(placeholder);
        Ok(())
    }

    /// Whether a recorded version goes out at once: the command's `push`,
    /// else the project's "send each version at once" (default yes), in a
    /// Cloud project.
    fn wants_push(&self, command: &Value) -> Result<bool, VcsError> {
        let wanted = command
            .get("push")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| {
                ProjectSettings::read(self.root())
                    .0
                    .local
                    .sync
                    .send_at_once
                    .unwrap_or(true)
            });
        Ok(wanted && !remotes_of(&self.fresh()?).1.is_empty())
    }

    /// Records `files` as one version (the command's `message`, else
    /// `message`) and pushes it as a Save is pushed; a failed push is the
    /// answer's error, the version recorded.
    fn record_version(
        &self,
        files: &[PathBuf],
        message: &str,
        author: &Identity,
        command: &Value,
        control: Option<&Control>,
    ) -> Result<(Value, Option<RemoteError>), VcsError> {
        let message = optional(command, "message").unwrap_or(message);
        let outcome = self.commit(files, message, author)?;
        self.send_version(outcome, command, control)
    }

    /// Filesystem transactions are finalized before a network push. A
    /// failure to send an already recorded version must never undo it.
    fn send_version(
        &self,
        outcome: CommitOutcome,
        command: &Value,
        control: Option<&Control>,
    ) -> Result<(Value, Option<RemoteError>), VcsError> {
        let mut value = serde_json::to_value(&outcome).expect("serializable");
        let mut push = None;
        let mut error = None;
        if outcome.commit.is_some() && self.wants_push(command)? {
            let not_sent =
                |message: &str| format!("The version is recorded but was not sent: {message}");
            match self.push(control) {
                Ok(pushed) => push = Some(pushed),
                Err(VcsError::Remote(mut failed)) => {
                    failed.message = not_sent(&failed.message);
                    error = Some(failed);
                }
                Err(other) => {
                    error = Some(RemoteError::new(
                        ErrorClass::Other,
                        not_sent(&other.to_string()),
                    ));
                }
            }
        }
        value["push"] = json!(push);
        Ok((value, error))
    }

    /// `create_folder`: the folder (and those above it) with its
    /// placeholder, recorded as `Add folder <path>`.
    fn create_folder(
        &self,
        command: &Value,
        control: Option<&Control>,
    ) -> Result<(Value, Option<RemoteError>), VcsError> {
        let path = self.project_path(command, "path")?;
        let full = self.root().join(&path);
        if fs::symlink_metadata(&full).is_ok() {
            return Err(refuse(
                ErrorClass::Exists,
                format!("{path} is there already"),
            ));
        }
        let author = self.ready(command)?;
        fs::create_dir_all(&full).map_err(|e| io_error("make", &full, e))?;
        let placeholder = full.join(FOLDER_PLACEHOLDER);
        write_atomically(&placeholder, b"").map_err(|e| io_error("write", &placeholder, e))?;
        let (mut value, error) = self.record_version(
            &[placeholder],
            &format!("Add folder {path}"),
            &author,
            command,
            control,
        )?;
        value["path"] = json!(path);
        Ok((value, error))
    }

    /// `new_design`: an empty design (or `text`) at the path, recorded as
    /// `Add <path>`.
    fn new_design(
        &self,
        command: &Value,
        control: Option<&Control>,
    ) -> Result<(Value, Option<RemoteError>), VcsError> {
        let path = self.project_path(command, "path")?;
        if !is_design_file(&path) {
            return Err(refuse(
                ErrorClass::InvalidPath,
                format!("{path}: a design's name ends with .mitcad"),
            ));
        }
        let full = self.root().join(&path);
        if fs::symlink_metadata(&full).is_ok() {
            return Err(refuse(
                ErrorClass::Exists,
                format!("{path} is there already"),
            ));
        }
        // `units`: the empty design's length unit (mitcad#1036, the
        // application's default units for new designs); not with `text`,
        // which is a design with units of its own.
        let units = match command.get("units") {
            None | Some(Value::Null) => None,
            Some(Value::String(text)) => Some(
                parse_length_unit(text)
                    .map_err(|e| VcsError::Command(format!("\"units\": {e}")))?,
            ),
            Some(_) => {
                return Err(VcsError::Command(
                    "\"units\" is a length unit's name".to_owned(),
                ));
            }
        };
        let design = match command.get("text").and_then(Value::as_str) {
            Some(_) if units.is_some() => {
                return Err(VcsError::Command(
                    "\"units\" is for an empty design, not with \"text\"".to_owned(),
                ));
            }
            Some(design) => {
                brep_references(design)
                    .map_err(|e| VcsError::Command(format!("\"text\" is not a design: {e}")))?;
                design.to_owned()
            }
            None => units.map_or_else(empty_design, empty_design_in),
        };
        let author = self.ready(command)?;
        write_atomically(&full, design.as_bytes()).map_err(|e| io_error("write", &full, e))?;
        let (mut value, error) =
            self.record_version(&[full], &format!("Add {path}"), &author, command, control)?;
        value["path"] = json!(path);
        Ok((value, error))
    }

    /// `move`: a design, or a folder with everything in it, to a new path
    /// in the project, recorded as `Move <from> to <to>`.
    fn move_entry(
        &self,
        command: &Value,
        control: Option<&Control>,
    ) -> Result<(Value, Option<RemoteError>), VcsError> {
        let from = self.project_path(command, "from")?;
        let to = self.project_path(command, "to")?;
        let root = self.root().to_path_buf();
        let (from_full, to_full) = (root.join(&from), root.join(&to));
        let metadata = fs::symlink_metadata(&from_full).map_err(|_| {
            refuse(
                ErrorClass::NotFound,
                format!("{from} is not in the project's folder"),
            )
        })?;
        let folder = metadata.is_dir();
        let invalid = |why: String| refuse(ErrorClass::InvalidPath, why);
        if !folder && !is_design_file(&from) {
            return Err(invalid(format!(
                "{from}: only designs and folders are moved here"
            )));
        }
        if !folder && !is_design_file(&to) {
            return Err(invalid(format!("{to}: a design's name ends with .mitcad")));
        }
        if is_under(&to, &from) {
            return Err(invalid(format!("{from} cannot be moved into itself")));
        }
        // Also another case of the same name on a system that does not
        // tell them apart.
        if fs::symlink_metadata(&to_full).is_ok() {
            return Err(refuse(ErrorClass::Exists, format!("{to} is there already")));
        }
        let recorded: Vec<String> = self
            .recorded_under(&from)?
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        let on_disk = if folder {
            files_on_disk(&from_full, &from)?
        } else {
            vec![from.clone()]
        };
        let designs: Vec<String> = on_disk
            .iter()
            .filter(|path| is_design_file(path) && visible(path))
            .cloned()
            .collect();
        let rewrites = self.move_link_rewrites(&from, &to, command)?;
        let affected: Vec<String> = designs
            .iter()
            .chain(recorded.iter().filter(|path| is_design_file(path)))
            .chain(rewrites.iter().map(|rewrite| &rewrite.before))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        self.check_locks(&affected, command)?;
        // Another window owned by the same person can still overwrite a
        // retargeted link from its old in-memory state.
        let locks = self.known_locks()?;
        for rewrite in &rewrites {
            if let Some(lock) = locks.get(&lock_id(&rewrite.before))
                && optional(command, "session") != Some(lock.session.as_str())
            {
                return Err(refuse(
                    ErrorClass::Locked,
                    format!(
                        "{} must update its component links but is open in another edit session; close that design first",
                        rewrite.before
                    ),
                ));
            }
        }
        let author = self.ready(command)?;
        let mapped = |path: &str| format!("{to}{}", &path[from.len()..]);
        let renames: Vec<(String, String)> = designs
            .iter()
            .map(|path| (path.clone(), mapped(path)))
            .collect();
        let last = last_design(&root);
        // Stage link changes at the original paths, then rename. On a
        // refusal the rename is reversed before these bytes are restored.
        let mut writes: Vec<(PathBuf, Vec<u8>)> = rewrites
            .iter()
            .map(|rewrite| (root.join(&rewrite.before), rewrite.bytes.clone()))
            .collect();
        let old: Vec<String> = recorded
            .iter()
            .chain(designs.iter())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut new: Vec<String> = old.iter().map(|path| mapped(path)).collect();
        new.extend(rewrites.iter().map(|rewrite| rewrite.after.clone()));
        let mut files: Vec<PathBuf> = old
            .iter()
            .chain(new.iter())
            .map(|path| root.join(path))
            .collect();
        let parent = parent_of(&from);
        if !parent.is_empty()
            && !self
                .recorded_under(parent)?
                .iter()
                .any(|(path, _)| !old.contains(path))
            && !new.iter().any(|path| is_under(path, parent))
        {
            let placeholder = root.join(parent).join(FOLDER_PLACEHOLDER);
            if !placeholder.is_file() {
                writes.push((placeholder.clone(), Vec::new()));
            }
            files.push(placeholder);
        }
        let mut transaction = FileTransaction::prepare(&root, &writes)?;
        // Remember only absent ancestors, so a failed move can remove its
        // new empty folders without touching pre-existing folders.
        let mut made = Vec::new();
        if let Some(folder) = to_full.parent() {
            let mut next = folder;
            while !next.exists() {
                made.push(next.to_path_buf());
                let Some(parent) = next.parent() else {
                    break;
                };
                next = parent;
            }
        }
        let mut renamed = false;
        let result = (|| -> Result<CommitOutcome, VcsError> {
            if let Some(folder) = to_full.parent() {
                fs::create_dir_all(folder).map_err(|e| io_error("make", folder, e))?;
            }
            transaction.apply()?;
            rename_retrying(&from_full, &to_full).map_err(|e| {
                VcsError::Io(format!(
                    "cannot move {} to {}: {e}",
                    from_full.display(),
                    to_full.display()
                ))
            })?;
            renamed = true;
            let message = format!("Move {from} to {to}");
            let outcome = self.commit_with_renames(
                &files,
                optional(command, "message").unwrap_or(&message),
                &author,
                &renames,
            )?;
            if let Some(reason) = &outcome.skipped {
                return Err(refuse(ErrorClass::Unsupported, reason.clone()));
            }
            Ok(outcome)
        })();
        let mut outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                if renamed && let Err(back) = rename_retrying(&to_full, &from_full) {
                    let backup = transaction.retain_backups();
                    return Err(VcsError::Io(format!(
                        "{error}; could not move {} back to {}: {back}; original files retained in {}",
                        to_full.display(),
                        from_full.display(),
                        backup.display()
                    )));
                }
                let restored = transaction.rollback();
                for folder in made {
                    let _ = fs::remove_dir(folder);
                }
                return match restored {
                    Ok(()) => Err(error),
                    Err(restore) => Err(VcsError::Io(format!("{error}; {restore}"))),
                };
            }
        };
        outcome.warnings.extend(transaction.commit());
        // The per-user display state and last-opened design follow only
        // after the move and every link update have been recorded.
        let mut display_moved = 0;
        if let Some(project) = Project::open(&root) {
            for design in &designs {
                if project
                    .move_local_state(&root.join(design), &root.join(mapped(design)))
                    .unwrap_or(false)
                {
                    display_moved += 1;
                }
            }
        }
        if let Some(last) = last.filter(|last| is_under(last, &from)) {
            let _ = remember(&root, &mapped(&last));
        }
        let (mut value, error) = self.send_version(outcome, command, control)?;
        value["links_updated"] = json!(
            rewrites
                .iter()
                .map(|rewrite| &rewrite.after)
                .collect::<Vec<_>>()
        );
        let moved: Vec<Value> = designs
            .iter()
            .map(|design| json!({"from": design, "to": mapped(design)}))
            .collect();
        value["from"] = json!(from);
        value["to"] = json!(to);
        value["moved"] = json!(moved);
        value["display_moved"] = json!(display_moved);
        Ok((value, error))
    }

    /// `delete`: a design, or a folder with everything in it, removed from
    /// the folder and recorded as `Delete <path>`; the history keeps what
    /// its versions hold, so files the latest version lacks (or holds
    /// otherwise) are refused unless `force`.
    fn delete_entry(
        &self,
        command: &Value,
        control: Option<&Control>,
    ) -> Result<(Value, Option<RemoteError>), VcsError> {
        let path = self.project_path(command, "path")?;
        let root = self.root().to_path_buf();
        let full = root.join(&path);
        let recorded = self.recorded_under(&path)?;
        let metadata = fs::symlink_metadata(&full).ok();
        if metadata.is_none() && recorded.is_empty() {
            return Err(refuse(
                ErrorClass::NotFound,
                format!("{path} is neither in the project's folder nor in its latest version"),
            ));
        }
        let folder = match &metadata {
            Some(metadata) => metadata.is_dir(),
            None => recorded.iter().any(|(recorded, _)| *recorded != path),
        };
        if !folder && !is_design_file(&path) {
            return Err(refuse(
                ErrorClass::InvalidPath,
                format!("{path}: only designs and folders are deleted here"),
            ));
        }
        let on_disk = match &metadata {
            Some(_) if folder => files_on_disk(&full, &path)?,
            Some(_) => vec![path.clone()],
            None => Vec::new(),
        };
        let designs: Vec<String> = on_disk
            .iter()
            .filter(|file| is_design_file(file) && visible(file))
            .cloned()
            .collect();
        let affected: Vec<String> = designs
            .iter()
            .chain(
                recorded
                    .iter()
                    .map(|(file, _)| file)
                    .filter(|file| is_design_file(file)),
            )
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        self.check_locks(&affected, command)?;
        let force = command
            .get("force")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !force {
            let blobs: HashMap<&str, ObjectId> = recorded
                .iter()
                .map(|(file, blob)| (file.as_str(), *blob))
                .collect();
            let unrecorded: Vec<&String> = on_disk
                .iter()
                .filter(|file| !is_system_file(file.rsplit('/').next().unwrap_or("")))
                .filter(|file| {
                    let Some(blob) = blobs.get(file.as_str()) else {
                        return true;
                    };
                    let here = fs::read(root.join(file)).ok().and_then(|data| {
                        gix::objs::compute_hash(
                            self.repo.object_hash(),
                            gix::objs::Kind::Blob,
                            &data,
                        )
                        .ok()
                    });
                    here != Some(*blob)
                })
                .collect();
            if !unrecorded.is_empty() {
                let shown: Vec<&str> = unrecorded.iter().take(10).map(|s| s.as_str()).collect();
                let more = if unrecorded.len() > 10 { ", ..." } else { "" };
                return Err(refuse(
                    ErrorClass::LocalChanges,
                    format!(
                        "the latest version lacks {}{more} as it is in the folder, so deleting \
                         would lose it: save it as a version first, or delete with force",
                        shown.join(", ")
                    ),
                ));
            }
        }
        let author = self.ready(command)?;
        if let Some(project) = Project::open(&root) {
            for design in &designs {
                let _ = project.remove_local_state(&root.join(design));
            }
        }
        if metadata.is_some() {
            let removed = if folder {
                fs::remove_dir_all(&full)
            } else {
                fs::remove_file(&full)
            };
            removed.map_err(|e| io_error("delete", &full, e))?;
        }
        let gone: Vec<String> = recorded.iter().map(|(file, _)| file.clone()).collect();
        let mut files: Vec<PathBuf> = gone.iter().map(|file| root.join(file)).collect();
        self.keep_folder(parent_of(&path), &gone, &[], &mut files)?;
        let (mut value, error) =
            self.record_version(&files, &format!("Delete {path}"), &author, command, control)?;
        value["path"] = json!(path);
        value["deleted_designs"] = json!(designs);
        Ok((value, error))
    }
}

/// The answer of a folder or design command as text; None for another
/// command.
pub(crate) fn describe(name: &str, answer: &Value) -> Option<String> {
    use std::fmt::Write;
    let mut out = String::new();
    let field = |key: &str| answer[key].as_str().unwrap_or("");
    let _ = match name {
        "project_tree" => {
            super::listing::describe(&mut out, answer);
            return Some(out);
        }
        "create_folder" => writeln!(out, "Made the folder {}", field("path")),
        "new_design" => writeln!(out, "Made the design {}", field("path")),
        "move" => writeln!(out, "Moved {} to {}", field("from"), field("to")),
        "delete" => writeln!(out, "Deleted {}", field("path")),
        _ => return None,
    };
    if let Some(commit) = answer["commit"].as_str() {
        let _ = writeln!(out, "Recorded version {}", &commit[..7.min(commit.len())]);
    } else if let Some(reason) = answer["skipped"].as_str() {
        let _ = writeln!(out, "No version recorded: {reason}");
    } else {
        let _ = writeln!(
            out,
            "No version recorded: nothing the versions hold changed"
        );
    }
    if answer["push"]["pushed"] == true {
        let _ = writeln!(
            out,
            "Sent to {}",
            answer["push"]["remote"].as_str().unwrap_or("the remote")
        );
    }
    for warning in answer["warnings"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "warning: {}", warning.as_str().unwrap_or(""));
    }
    if let Some(message) = answer["error"]["message"].as_str() {
        let _ = writeln!(out, "warning: {message}");
    }
    Some(out)
}
