// SPDX-License-Identifier: MIT
//! Reading the history: the versions of a file, versions named by id,
//! the paths that differ between two versions, what differs in a design
//! between them (P12c), and a file against the latest version (with a
//! rename outside Mitcad, P12d).

use std::collections::HashMap;
use std::path::Path;

use gix::ObjectId;
use gix::bstr::ByteSlice;
use mitcad_model::DocState;
use mitcad_model::diff::{DesignDiff, diff_states, read_design};
use serde::Serialize;

use crate::commit::user_message;
use crate::{Identity, ProjectRepo, VcsError, absolute};

/// A version of a file: a commit that changed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Version {
    /// The commit's id (40 hex digits) and its first 7.
    pub id: String,
    pub short_id: String,
    /// When it was made: seconds since 1970 (UTC), the author's offset
    /// from UTC in seconds, and both as text (`2026-10-05 14:03:12 +0300`).
    pub time: i64,
    pub offset: i32,
    pub date: String,
    pub author: Identity,
    /// The message's first line, and the message without Mitcad's
    /// trailers.
    pub summary: String,
    pub message: String,
    /// The file's path in this version (another one before a rename) and
    /// its blob id; None when this version deleted it.
    pub path: String,
    pub blob: Option<String>,
    /// The path it had before, when this version renamed it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renamed_from: Option<String>,
}

/// How a path differs between two versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Deleted,
    Modified,
    /// The same content under another path.
    Renamed,
}

/// A path that differs between two versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    pub kind: ChangeKind,
    pub path: String,
    /// The path before a rename.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

/// A file in the folder against the latest version: blob ids (as git
/// names content), None where the file is not there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileStatus {
    pub path: String,
    /// In the latest version (HEAD).
    pub head: Option<String>,
    /// In the folder.
    pub file: Option<String>,
    /// Whether they differ.
    pub modified: bool,
    /// A project file not in the latest version whose content is that of
    /// another one there which is gone from the folder: renamed or moved
    /// outside Mitcad from that path (P12d).
    pub renamed_from: Option<String>,
}

impl ProjectRepo {
    /// The versions of `file`, newest first: the commits on HEAD's
    /// first-parent chain that changed it (`git log --first-parent --
    /// <path>`), following it back through renames that kept its content.
    pub fn history(&self, file: &Path) -> Result<Vec<Version>, VcsError> {
        let mut path = self.relative(file)?;
        let Some(head) = self.head_commit()? else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        // The file in the commit about to be visited, found as the parent
        // of the one before.
        let mut next: Option<Option<ObjectId>> = None;
        for info in self.repo.rev_walk([head]).first_parent_only().all()? {
            let info = info?;
            let id = info.id;
            let parent = info.parent_ids().next().map(|p| p.detach());
            let here = match next.take() {
                Some(here) => here,
                None => self.entry_at(id, &path)?,
            };
            let mut before = match parent {
                Some(parent) => self.entry_at(parent, &path)?,
                None => None,
            };
            let mut renamed_from = None;
            if let (Some(blob), None, Some(parent)) = (here, before, parent)
                && let Some(old) = self.renamed_from(parent, id, blob)?
            {
                before = here;
                renamed_from = Some(old);
            }
            if here != before || renamed_from.is_some() {
                let commit = self.repo.find_commit(id)?;
                out.push(self.version(&commit, &path, here, renamed_from.clone())?);
            }
            if let Some(old) = renamed_from {
                path = old;
            }
            next = Some(before);
        }
        Ok(out)
    }

    /// The path of a file that `commit` renamed to one with content `blob`:
    /// a file of its parent with the same content that it deleted.
    fn renamed_from(
        &self,
        parent: ObjectId,
        commit: ObjectId,
        blob: ObjectId,
    ) -> Result<Option<String>, VcsError> {
        let old = self.repo.find_commit(parent)?.tree_id()?.detach();
        let new = self.repo.find_commit(commit)?.tree_id()?.detach();
        Ok(self
            .differences(Some(old), Some(new))?
            .into_iter()
            .find(|d| d.old == Some(blob) && d.new.is_none())
            .map(|d| d.path))
    }

    fn version(
        &self,
        commit: &gix::Commit<'_>,
        path: &str,
        blob: Option<ObjectId>,
        renamed_from: Option<String>,
    ) -> Result<Version, VcsError> {
        let author = commit.author()?;
        let time = author.time()?;
        let message = commit.message_raw()?.to_str_lossy().into_owned();
        let message = user_message(&message).to_owned();
        let id = commit.id;
        Ok(Version {
            id: id.to_string(),
            short_id: id.to_hex_with_len(7).to_string(),
            time: time.seconds,
            offset: time.offset,
            date: time.format_or_unix(gix::date::time::format::ISO8601),
            author: Identity {
                name: author.name.to_str_lossy().trim().to_owned(),
                email: author.email.to_str_lossy().trim().to_owned(),
            },
            summary: message.lines().next().unwrap_or("").to_owned(),
            message,
            path: path.to_owned(),
            blob: blob.map(|b| b.to_string()),
            renamed_from,
        })
    }

    /// The full id of version `rev`: an id or a unique prefix of one (at
    /// least 4 hex digits) of a commit reachable from HEAD, `HEAD`, or
    /// `HEAD~n` (n first parents back); also the full id of any other
    /// commit the repository has, such as a remote's version that a fetch
    /// brought (a sync's conflict compares it with the project's).
    pub fn resolve(&self, rev: &str) -> Result<String, VcsError> {
        Ok(self.resolve_id(rev)?.to_string())
    }

    pub(crate) fn resolve_id(&self, rev: &str) -> Result<ObjectId, VcsError> {
        let rev = rev.trim();
        let not_found = || VcsError::NotFound(format!("there is no version '{rev}'"));
        let head = self
            .head_commit()?
            .ok_or_else(|| VcsError::NotFound("the project has no versions yet".to_owned()))?;
        let upper = rev.to_ascii_uppercase();
        if let Some(back) = upper.strip_prefix("HEAD") {
            let steps: usize = match back {
                "" => 0,
                "^" => 1,
                _ => back
                    .strip_prefix('~')
                    .and_then(|n| {
                        if n.is_empty() {
                            Some(1)
                        } else {
                            n.parse().ok()
                        }
                    })
                    .ok_or_else(not_found)?,
            };
            let mut id = head;
            for _ in 0..steps {
                id = self
                    .repo
                    .find_commit(id)?
                    .parent_ids()
                    .next()
                    .ok_or_else(not_found)?
                    .detach();
            }
            return Ok(id);
        }
        let hex = rev.to_ascii_lowercase();
        if hex.len() < 4 || hex.len() > 40 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(not_found());
        }
        if hex.len() == self.repo.object_hash().len_in_hex()
            && let Ok(id) = ObjectId::from_hex(hex.as_bytes())
            && self.repo.find_commit(id).is_ok()
        {
            return Ok(id);
        }
        let mut found = None;
        for info in self.repo.rev_walk([head]).all()? {
            let id = info?.id;
            if id.to_string().starts_with(&hex) {
                if found.is_some_and(|f| f != id) {
                    return Err(VcsError::NotFound(format!(
                        "'{rev}' names more than one version; give more digits"
                    )));
                }
                found = Some(id);
            }
        }
        found.ok_or_else(not_found)
    }

    /// The paths that differ between versions `from` and `to` (default:
    /// the latest), sorted; a file deleted and another added with the
    /// same content is a rename.
    pub fn changes(&self, from: &str, to: Option<&str>) -> Result<Vec<Change>, VcsError> {
        let tree = |rev: &str| -> Result<ObjectId, VcsError> {
            let commit = self.resolve_id(rev)?;
            Ok(self.repo.find_commit(commit)?.tree_id()?.detach())
        };
        let old = tree(from)?;
        let new = tree(to.unwrap_or("HEAD"))?;
        let differences = self.differences(Some(old), Some(new))?;
        let mut renamed = vec![false; differences.len()];
        let mut out = Vec::new();
        for (i, d) in differences.iter().enumerate() {
            let kind = match (d.old, d.new) {
                (Some(_), Some(_)) => ChangeKind::Modified,
                (None, _) => ChangeKind::Added,
                (Some(_), None) => ChangeKind::Deleted,
            };
            if kind == ChangeKind::Added {
                // Paired with a deletion of the same content.
                let source = differences
                    .iter()
                    .enumerate()
                    .find(|(j, other)| !renamed[*j] && other.new.is_none() && other.old == d.new);
                if let Some((j, source)) = source {
                    renamed[j] = true;
                    out.push((
                        i,
                        Change {
                            kind: ChangeKind::Renamed,
                            path: d.path.clone(),
                            from: Some(source.path.clone()),
                        },
                    ));
                    continue;
                }
            }
            out.push((
                i,
                Change {
                    kind,
                    path: d.path.clone(),
                    from: None,
                },
            ));
        }
        Ok(out
            .into_iter()
            .filter(|(i, change)| !(renamed[*i] && change.kind == ChangeKind::Deleted))
            .map(|(_, change)| change)
            .collect())
    }

    /// What differs in the design of project file `file` between version
    /// `from` and version `to`, or the file as saved in the folder when
    /// `to` is None (P12c, `mitcad_model::diff`): parameters, timeline,
    /// sketches, components and bodies, read from the versions' trees
    /// (under the path the file had there, through the renames its history
    /// follows) without computing anything.
    pub fn diff(&self, file: &Path, from: &str, to: Option<&str>) -> Result<DesignDiff, VcsError> {
        let path = self.relative(file)?;
        let design = |text: Vec<u8>, at: &str| {
            let text = String::from_utf8(text)
                .map_err(|_| VcsError::File(format!("{path} {at} is not text")))?;
            read_design(&text).map_err(|e| VcsError::File(format!("{path} {at}: {e}")))
        };
        let version = |rev: &str| -> Result<_, VcsError> {
            let commit = self.resolve_id(rev)?;
            let at = format!("in version {}", commit.to_hex_with_len(7));
            let there = self.path_at(commit, file, &path)?;
            design(self.read_at(commit, &there)?, &at)
        };
        let old = version(from)?;
        let new = match to {
            Some(rev) => version(rev)?,
            None => {
                let full = self.root.join(&path);
                let text = std::fs::read(&full).map_err(|e| crate::io_error("read", &full, e))?;
                design(text, "in the folder")?
            }
        };
        Ok(diff_states(&old, &new))
    }

    /// What each of `versions` (a file's history, newest first) changed in
    /// the design against the version after it in the list, the one before
    /// it (P12e, the Version History window): the summary of their
    /// comparison (P12c, [`DesignDiff::summary`]; `no changes` when the
    /// design is the same, as after a rename). None for the oldest version
    /// and next to a version that deleted the file; an error for a version
    /// whose project file cannot be read (another program's, a newer
    /// Mitcad's). Each version is read once, from its blob, oldest first,
    /// and only two are kept at a time.
    pub fn summaries(&self, versions: &[Version]) -> Vec<Option<Result<String, VcsError>>> {
        let mut out = vec![None; versions.len()];
        let mut before: Option<Result<DocState, VcsError>> = None;
        for (i, version) in versions.iter().enumerate().rev() {
            let Some(blob) = version.blob.as_deref() else {
                before = None;
                continue;
            };
            let design = self.design_of(blob, &version.path);
            if let Some(old) = &before {
                out[i] = Some(match (old, &design) {
                    (Ok(old), Ok(new)) => Ok(diff_states(old, new).summary),
                    (Err(e), _) | (_, Err(e)) => Err(e.clone()),
                });
            }
            before = Some(design);
        }
        out
    }

    /// The design of a project file's blob (`path` names it in errors).
    fn design_of(&self, blob: &str, path: &str) -> Result<DocState, VcsError> {
        let id = ObjectId::from_hex(blob.as_bytes())
            .map_err(|e| VcsError::Git(format!("blob id {blob} of {path}: {e}")))?;
        let data = self.repo.find_blob(id)?.take_data();
        let text =
            String::from_utf8(data).map_err(|_| VcsError::File(format!("{path} is not text")))?;
        read_design(&text).map_err(|e| VcsError::File(format!("{path}: {e}")))
    }

    /// The path that `file` (at `path` now) had in version `commit`: the
    /// same, or the one before the renames its history follows, when
    /// `commit` is on HEAD's first-parent chain.
    pub(crate) fn path_at(
        &self,
        commit: ObjectId,
        file: &Path,
        path: &str,
    ) -> Result<String, VcsError> {
        if self.entry_at(commit, path)?.is_some() {
            return Ok(path.to_owned());
        }
        let renames: HashMap<String, String> = self
            .history(file)?
            .into_iter()
            .filter_map(|v| v.renamed_from.map(|from| (v.id, from)))
            .collect();
        let Some(head) = self.head_commit()?.filter(|_| !renames.is_empty()) else {
            return Ok(path.to_owned());
        };
        let mut current = path.to_owned();
        for info in self.repo.rev_walk([head]).first_parent_only().all()? {
            let id = info?.id;
            if id == commit {
                return Ok(current);
            }
            if let Some(from) = renames.get(&id.to_string()) {
                current.clone_from(from);
            }
        }
        Ok(path.to_owned())
    }

    /// `file` in the folder against the latest version.
    pub fn status(&self, file: &Path) -> Result<FileStatus, VcsError> {
        let path = self.relative(file)?;
        let commit = self.head_commit()?;
        let head = match commit {
            Some(commit) => self.entry_at(commit, &path)?,
            None => None,
        };
        let full = self.root.join(&path);
        let file = match std::fs::read(&full) {
            Ok(data) => Some(gix::objs::compute_hash(
                self.repo.object_hash(),
                gix::objs::Kind::Blob,
                &data,
            )?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(crate::io_error("read", &full, e)),
        };
        // Renamed outside Mitcad: the same content under a path the folder
        // no longer has.
        let renamed_from = match (commit, head, file) {
            (Some(commit), None, Some(id)) if crate::is_project_file(&path) => {
                let tree = self.repo.find_commit(commit)?.tree_id()?.detach();
                self.project_files(tree)?
                    .into_iter()
                    .find(|(other, blob)| *blob == id && !self.root.join(other).exists())
                    .map(|(other, _)| other)
            }
            _ => None,
        };
        Ok(FileStatus {
            path,
            head: head.map(|id| id.to_string()),
            file: file.map(|id| id.to_string()),
            modified: head != file,
            renamed_from,
        })
    }

    /// A project file renamed or moved outside Mitcad
    /// ([`FileStatus::renamed_from`]) takes its display state along from
    /// the path it had, which is per user and not versioned. Returns that
    /// path, if it was renamed, and whether a display state moved.
    pub fn follow_rename(&self, file: &Path) -> Result<(Option<String>, bool), VcsError> {
        let Some(from) = self.status(file)?.renamed_from else {
            return Ok((None, false));
        };
        let moved = match mitcad_model::Project::open(&self.root) {
            Some(project) => project
                .move_local_state(&self.root.join(&from), &absolute(file)?)
                .map_err(|e| crate::io_error("move the display state of", file, e))?,
            None => false,
        };
        Ok((Some(from), moved))
    }
}
