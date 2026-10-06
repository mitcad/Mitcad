// SPDX-License-Identifier: MIT
//! Sync: the project's versions and the remote's brought together, then
//! shared.
//!
//! A sync fetches, then compares the branch (L) with the remote-tracking
//! reference (R) and their last common version (B):
//!
//! - L = R: up to date. R = B: the project's versions are pushed. L = B: the
//!   remote's are taken (`git merge --ff-only`).
//! - Both have versions the other lacks: the project's unpublished versions
//!   are replayed after R, one by one, file by file, so that the history
//!   stays one line and the shared history is never rewritten. A path that
//!   the project's versions change, that the remote's change too and whose
//!   contents in L and R differ is a conflict, and each needs a choice for
//!   the whole file: keep mine, take theirs, or save mine as a copy next to
//!   it. Files are never merged line by line, and designs never feature by
//!   feature. The B-rep files of the store are never conflicts (the same
//!   name is the same content): each replayed version gets those its project
//!   files refer to and drops the others, as a recorded version does.
//! - The old local history stays in `refs/mitcad/sync-backup/<time>`; the
//!   branch and the folder then move together with `git reset --keep`,
//!   which leaves other changes in the folder as they are and refuses to
//!   overwrite changes no version holds. Then a push (never forced); the
//!   remote moving meanwhile starts the round again.
//!
//! Nothing changes before the backup reference is written: a sync cancelled,
//! or stopped by a conflict or by changes in the folder, leaves the project
//! as it was (the objects a replay wrote are left to git's gc).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use gix::ObjectId;
use gix::objs::tree::EntryKind;
use gix::refs::Target;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use mitcad_model::file::brep_path;
use serde::{Deserialize, Serialize};

use super::{
    Attempted, COUNT_LIMIT, Control, ErrorClass, FetchOutcome, PushOutcome, RemoteError, Upstream,
    ahead_behind, failure, head_of, reference_id,
};
use crate::commit::user_message;
use crate::tree::brep_sha256;
use crate::{Identity, ProjectRepo, VcsError};

/// Where a sync keeps the project's history from before it.
pub const BACKUP_PREFIX: &str = "refs/mitcad/sync-backup/";
/// Backups are kept this long ...
const BACKUP_DAYS: i64 = 30;
/// ... and the newest of them always.
const BACKUPS_KEPT: usize = 5;
/// How often a sync starts again when the remote moved meanwhile.
const RETRIES: u32 = 3;
/// The versions a plan lists on each side (the counts are complete).
const LIST_LIMIT: usize = 100;

/// What a sync does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncCase {
    /// The project and the remote have the same versions.
    #[default]
    UpToDate,
    /// Only the project has new versions: they are pushed.
    Push,
    /// Only the remote has new versions: the project takes them.
    FastForward,
    /// Both have: the project's are replayed after the remote's.
    Replay,
    /// A state the sync does not handle (`reason`).
    Unsupported,
}

/// The choice for a file changed both in the project and on the remote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    /// Keep mine: the project's versions of the file come after the
    /// remote's.
    Mine,
    /// Take theirs: the remote's file; the project's versions of it stay
    /// only in the backup reference.
    Theirs,
    /// Save mine as a copy: the remote's file, and the project's versions
    /// of it under a new name next to it.
    Copy,
}

impl Resolution {
    /// `mine`, `theirs` or `copy`.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "mine" => Some(Self::Mine),
            "theirs" => Some(Self::Theirs),
            "copy" => Some(Self::Copy),
            _ => None,
        }
    }
}

/// How a file changed on both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    /// Both changed it.
    Modified,
    /// The remote deleted it; the project changed it.
    DeletedTheirs,
    /// The project deleted it; the remote changed it.
    DeletedMine,
    /// Both added it, with different contents.
    AddedBoth,
}

/// A version in a sync: a commit on one side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyncVersion {
    pub id: String,
    pub short_id: String,
    /// The author's time: seconds since 1970 (UTC), the offset from UTC and
    /// both as text.
    pub time: i64,
    pub offset: i32,
    pub date: String,
    pub author: Identity,
    /// The message's first line.
    pub summary: String,
}

/// A side of a conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConflictSide {
    /// The file's blob on this side; None when deleted.
    pub blob: Option<String>,
    /// The latest version of this side that changed the file.
    pub version: Option<SyncVersion>,
    /// How many versions of this side changed it.
    pub versions: usize,
}

/// A file changed both in the project and on the remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    pub path: String,
    pub kind: ConflictKind,
    pub mine: ConflictSide,
    pub theirs: ConflictSide,
    /// The choices it has: no copy for the files of `.mitcad/`.
    pub choices: Vec<Resolution>,
    /// The copy's path when mine is saved as a copy.
    pub copy: Option<String>,
}

/// What a sync would do, read after a fetch from the local references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyncPlan {
    pub case: SyncCase,
    pub branch: String,
    /// `origin/main`.
    pub upstream: String,
    /// HEAD, the remote-tracking reference and their last common version.
    pub head: Option<String>,
    pub remote_head: Option<String>,
    pub base: Option<String>,
    /// The versions the remote lacks and those the project lacks (their
    /// counts), and the newest of them.
    pub ahead: usize,
    pub behind: usize,
    pub local: Vec<SyncVersion>,
    pub remote: Vec<SyncVersion>,
    pub conflicts: Vec<Conflict>,
    /// Paths in the folder with changes no version holds (saved but not
    /// recorded, staged, new), the B-rep store left out.
    pub uncommitted: Vec<String>,
    /// Why the sync cannot run (`unsupported`).
    pub reason: Option<String>,
    pub fetch: Option<FetchOutcome>,
}

/// How a sync is run.
#[derive(Debug, Clone)]
pub struct SyncOptions {
    /// The choices by path (as the plan's conflicts name them, or a file's
    /// path, absolute or relative to the current folder).
    pub resolutions: BTreeMap<String, Resolution>,
    /// The choice for the conflicts not in `resolutions` (where it is one).
    pub resolve_all: Option<Resolution>,
    /// Fetch first (else the remote-tracking reference as it is).
    pub fetch: bool,
    pub push: bool,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            resolutions: BTreeMap::new(),
            resolve_all: None,
            fetch: true,
            push: true,
        }
    }
}

/// A version of the project replayed after the remote's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Replayed {
    /// Its commit before the sync (in the backup reference).
    pub from: String,
    /// Its new commit; None when it was left out because its changes were
    /// all taken from the remote.
    pub to: Option<String>,
    pub summary: String,
}

/// A file saved as a copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConflictCopy {
    pub path: String,
    pub copy: String,
}

/// What a sync did, and why it stopped when it did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SyncOutcome {
    /// The case of the first round.
    pub case: SyncCase,
    pub branch: String,
    /// `origin/main`.
    pub upstream: String,
    /// The last fetch.
    pub fetch: Option<FetchOutcome>,
    /// HEAD after the sync.
    pub head: Option<String>,
    pub replayed: Vec<Replayed>,
    /// The choices applied, by path.
    pub resolved: BTreeMap<String, Resolution>,
    pub copies: Vec<ConflictCopy>,
    /// The reference that keeps the history from before the replay.
    pub backup: Option<String>,
    pub push: Option<PushOutcome>,
    pub pushed: bool,
    /// The rounds started again because the remote moved meanwhile.
    pub retries: u32,
    /// The paths the sync changed in the folder.
    pub changed_paths: Vec<String>,
    /// The conflicts without a choice (the error `conflict`).
    pub conflicts: Vec<Conflict>,
    /// Paths with changes no version holds, after the sync.
    pub uncommitted: Vec<String>,
    pub reason: Option<String>,
    pub warnings: Vec<String>,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    /// Why the sync stopped; what it did before stays done.
    #[serde(skip)]
    pub error: Option<RemoteError>,
}

/// The remote's versions the project lacks, as the last fetch left them:
/// what the application's status bar and its notice of a newer version of
/// the open file show (no network).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Incoming {
    /// `origin/main`.
    pub upstream: String,
    /// The versions the remote lacks and those the project lacks.
    pub ahead: usize,
    pub behind: usize,
    /// The newest of the remote's versions the project lacks.
    pub versions: Vec<SyncVersion>,
    /// A file asked about, relative to the project ...
    pub path: Option<String>,
    /// ... the remote's versions above that changed it, newest first ...
    pub file_versions: Vec<SyncVersion>,
    /// ... whether the remote's file differs from the project's latest
    /// version of it ...
    pub differs: bool,
    /// ... and whether the project's versions the remote lacks changed it
    /// too: a sync then asks what to keep.
    pub changed_here: bool,
}

/// The two sides of a sync, read from the local references.
struct Analysis {
    case: SyncCase,
    reason: Option<String>,
    head: Option<ObjectId>,
    remote: Option<ObjectId>,
    base: Option<ObjectId>,
    /// The project's versions the remote lacks, newest first: for a replay
    /// all of them, a line of commits with one parent each.
    local: Vec<ObjectId>,
    /// The remote's versions the project lacks, newest first.
    remote_versions: Vec<ObjectId>,
    ahead: usize,
    behind: usize,
    conflicts: Vec<Conflict>,
}

impl Analysis {
    fn new(head: Option<ObjectId>, remote: Option<ObjectId>) -> Self {
        Self {
            case: SyncCase::UpToDate,
            reason: None,
            head,
            remote,
            base: None,
            local: Vec::new(),
            remote_versions: Vec::new(),
            ahead: 0,
            behind: 0,
            conflicts: Vec::new(),
        }
    }

    fn unsupported(&mut self, reason: String) {
        self.case = SyncCase::Unsupported;
        self.reason = Some(reason);
    }
}

/// The replayed history.
struct Replay {
    tip: ObjectId,
    replayed: Vec<Replayed>,
}

/// Whether a round of a sync is done or starts again.
enum Round {
    Done,
    Again,
}

/// Whether a path is a file of the B-rep store.
fn in_store(path: &str) -> bool {
    brep_sha256(path).is_some()
}

/// Whether a path may be saved as a copy: not the project's own files in
/// `.mitcad/`.
fn may_copy(path: &str) -> bool {
    !path.starts_with(".mitcad/")
}

fn cancelled(control: Option<&Control>) -> Result<(), VcsError> {
    if control.is_some_and(Control::is_cancelled) {
        Err(RemoteError::new(ErrorClass::Cancelled, "cancelled").into())
    } else {
        Ok(())
    }
}

/// A list of paths for messages: `a`, `a and b`, `a, b and c`.
fn listed(paths: &[&str]) -> String {
    match paths {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The commits from `tip` back, without those reachable from `hidden`
/// (`git rev-list hidden..tip`), newest first, up to `limit`.
fn walk(
    repo: &gix::Repository,
    tip: ObjectId,
    hidden: Option<ObjectId>,
    limit: usize,
) -> Result<Vec<ObjectId>, VcsError> {
    let mut out = Vec::new();
    for info in repo.rev_walk([tip]).with_hidden(hidden).all()? {
        out.push(info?.id);
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

/// `yyyymmdd-HHMMSS` in UTC, as backup references are named.
pub(crate) fn stamp(seconds: i64) -> String {
    let text = gix::date::Time::new(seconds, 0).format_or_unix(gix::date::time::format::ISO8601);
    let digits: String = text.chars().take(19).filter(char::is_ascii_digit).collect();
    if digits.len() == 14 {
        format!("{}-{}", &digits[..8], &digits[8..])
    } else {
        seconds.to_string()
    }
}

/// The path of a copy of `path` saved by `who` from a version of `date`
/// (`yyyy-mm-dd HH.mm`): `part (conflict copy Name 2026-10-05 14.03).mitcad`
/// in the same folder; a number is added while `taken` says it is in use.
pub(crate) fn copy_path(path: &str, who: &str, date: &str, taken: impl Fn(&str) -> bool) -> String {
    let (folder, name) = match path.rsplit_once('/') {
        Some((folder, name)) => (format!("{folder}/"), name),
        None => (String::new(), path),
    };
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    // A name every file system takes.
    let who: String = who
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let who = who.trim();
    let mut n = 1;
    loop {
        let number = if n == 1 {
            String::new()
        } else {
            format!(" {n}")
        };
        let candidate = format!("{folder}{stem} (conflict copy {who} {date}{number}){extension}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

impl ProjectRepo {
    /// Runs the test hook of a sync's stage.
    #[cfg(test)]
    fn hook(&self, stage: &str) {
        let taken = self.sync_hook.borrow_mut().take();
        if let Some(mut hook) = taken {
            hook(stage);
            *self.sync_hook.borrow_mut() = Some(hook);
        }
    }

    #[cfg(not(test))]
    fn hook(&self, _stage: &str) {}

    fn tree_of(&self, commit: ObjectId) -> Result<ObjectId, VcsError> {
        Ok(self.repo.find_commit(commit)?.tree_id()?.detach())
    }

    fn sync_version(&self, id: ObjectId) -> Result<SyncVersion, VcsError> {
        let commit = self.repo.find_commit(id)?;
        let author = commit.author()?;
        let time = author.time()?;
        let message = commit.message_raw()?.to_string();
        Ok(SyncVersion {
            id: id.to_string(),
            short_id: id.to_hex_with_len(7).to_string(),
            time: time.seconds,
            offset: time.offset,
            date: time.format_or_unix(gix::date::time::format::ISO8601),
            author: Identity {
                name: author.name.to_string().trim().to_owned(),
                email: author.email.to_string().trim().to_owned(),
            },
            summary: user_message(&message)
                .lines()
                .next()
                .unwrap_or("")
                .to_owned(),
        })
    }

    /// The paths with changes no version holds: `git status` of the folder
    /// (changed, deleted, staged and new files; ignored ones left out).
    fn local_changes(&self, control: Option<&Control>) -> Result<Vec<String>, VcsError> {
        let output = self.git_ok(
            "Reading the changes in the project's folder",
            &[
                "--no-optional-locks",
                "status",
                "--porcelain",
                "-z",
                "--untracked-files=all",
            ],
            control,
        )?;
        let mut out = BTreeSet::new();
        let mut fields = output.stdout.split('\0');
        while let Some(field) = fields.next() {
            if field.len() < 4 {
                continue;
            }
            let (status, path) = field.split_at(3);
            out.insert(path.to_owned());
            // A rename or copy names its source next.
            if status.contains(['R', 'C'])
                && let Some(from) = fields.next()
            {
                out.insert(from.to_owned());
            }
        }
        Ok(out.into_iter().collect())
    }

    /// The paths that differ between two commits.
    fn changed_between(
        &self,
        from: Option<ObjectId>,
        to: Option<ObjectId>,
    ) -> Result<BTreeSet<String>, VcsError> {
        let old = from.map(|id| self.tree_of(id)).transpose()?;
        let new = to.map(|id| self.tree_of(id)).transpose()?;
        Ok(self
            .differences(old, new)?
            .into_iter()
            .map(|difference| difference.path)
            .collect())
    }

    /// Fails when a path the sync changes has changes no version holds.
    fn refuse_local_changes(
        &self,
        changed: &BTreeSet<String>,
        control: Option<&Control>,
    ) -> Result<(), VcsError> {
        let local = self.local_changes(control)?;
        let blocked: Vec<&str> = local
            .iter()
            .filter(|path| changed.contains(*path))
            .map(String::as_str)
            .collect();
        if blocked.is_empty() {
            return Ok(());
        }
        Err(failure(
            ErrorClass::LocalChanges,
            format!(
                "the sync would change {} in the folder, which {} changes no version holds: \
                 record {} as versions (or undo the changes), then sync again",
                listed(&blocked),
                if blocked.len() == 1 { "has" } else { "have" },
                if blocked.len() == 1 { "it" } else { "them" },
            ),
        ))
    }

    /// What a sync would do now: fetched first when `fetch`, then read from
    /// the local references (and `git status` for the folder's changes).
    /// Nothing else changes.
    pub fn sync_plan(&self, fetch: bool, control: Option<&Control>) -> Result<SyncPlan, VcsError> {
        let (branch, upstream) = self.current_upstream(&self.fresh()?)?;
        let fetched = if fetch {
            Some(self.fetch(control)?)
        } else {
            None
        };
        let analysis = self.analyse(&upstream)?;
        let versions = |list: &[ObjectId]| -> Result<Vec<SyncVersion>, VcsError> {
            list.iter()
                .take(LIST_LIMIT)
                .map(|id| self.sync_version(*id))
                .collect()
        };
        let uncommitted = self
            .local_changes(control)?
            .into_iter()
            .filter(|path| !in_store(path))
            .collect();
        Ok(SyncPlan {
            case: analysis.case,
            branch,
            upstream: upstream.short(),
            head: analysis.head.map(|id| id.to_string()),
            remote_head: analysis.remote.map(|id| id.to_string()),
            base: analysis.base.map(|id| id.to_string()),
            ahead: analysis.ahead,
            behind: analysis.behind,
            local: versions(&analysis.local)?,
            remote: versions(&analysis.remote_versions)?,
            conflicts: analysis.conflicts,
            uncommitted,
            reason: analysis.reason,
            fetch: fetched,
        })
    }

    /// The two sides of a sync and what the sync does with them.
    fn analyse(&self, upstream: &Upstream) -> Result<Analysis, VcsError> {
        let repo = self.fresh()?;
        let head = head_of(&repo)?;
        let remote = reference_id(&repo, &upstream.tracking())?;
        let mut analysis = Analysis::new(head, remote);
        if let Some(operation) = self.operation_in_progress() {
            analysis.unsupported(format!(
                "a {operation} is in progress in the repository: finish or abort it with git, \
                 then sync"
            ));
            return Ok(analysis);
        }
        let (local, remote) = match (head, remote) {
            (None, None) => return Ok(analysis),
            (Some(local), None) => {
                analysis.local = walk(&repo, local, None, COUNT_LIMIT)?;
                analysis.ahead = analysis.local.len();
                analysis.case = SyncCase::Push;
                return Ok(analysis);
            }
            (None, Some(remote)) => {
                analysis.remote_versions = walk(&repo, remote, None, COUNT_LIMIT)?;
                analysis.behind = analysis.remote_versions.len();
                analysis.case = SyncCase::FastForward;
                return Ok(analysis);
            }
            (Some(local), Some(remote)) if local == remote => {
                analysis.base = Some(local);
                return Ok(analysis);
            }
            (Some(local), Some(remote)) => (local, remote),
        };
        // All of the project's own versions: a replay needs every one.
        analysis.local = walk(&repo, local, Some(remote), usize::MAX)?;
        analysis.remote_versions = walk(&repo, remote, Some(local), COUNT_LIMIT)?;
        analysis.ahead = analysis.local.len();
        analysis.behind = analysis.remote_versions.len();
        if analysis.local.is_empty() {
            analysis.base = Some(local);
            analysis.case = SyncCase::FastForward;
            return Ok(analysis);
        }
        if analysis.remote_versions.is_empty() {
            analysis.base = Some(remote);
            analysis.case = SyncCase::Push;
            return Ok(analysis);
        }
        // The project's versions must be a line back to a version the
        // remote has: a merge made with git is not replayed.
        let own: HashSet<ObjectId> = analysis.local.iter().copied().collect();
        let mut line = Vec::new();
        let mut at = local;
        let base = loop {
            if !own.contains(&at) {
                break Some(at);
            }
            let parents: Vec<ObjectId> = repo
                .find_commit(at)?
                .parent_ids()
                .map(|id| id.detach())
                .collect();
            line.push(at);
            match parents.as_slice() {
                [parent] => at = *parent,
                [] => break None,
                _ => {
                    analysis.unsupported(format!(
                        "the project's versions that the remote lacks include a merge made with \
                         git ({}), which Mitcad does not replay: sync with git (git pull) instead",
                        at.to_hex_with_len(7)
                    ));
                    return Ok(analysis);
                }
            }
        };
        let Some(base) = base.filter(|_| line.len() == own.len()) else {
            analysis.unsupported(
                "the remote's branch holds another history than the project's: open the remote \
                 into a new folder (mitcad-cli clone) instead"
                    .to_owned(),
            );
            return Ok(analysis);
        };
        analysis.base = Some(base);
        analysis.local = line;
        analysis.case = SyncCase::Replay;
        analysis.conflicts = self.conflicts(&analysis, base, local, remote)?;
        Ok(analysis)
    }

    /// The files changed on both sides: paths the project's versions change
    /// and the remote's change too, whose contents in L and R differ; the
    /// B-rep store left out.
    fn conflicts(
        &self,
        analysis: &Analysis,
        base: ObjectId,
        local: ObjectId,
        remote: ObjectId,
    ) -> Result<Vec<Conflict>, VcsError> {
        let (base_tree, local_tree, remote_tree) = (
            self.tree_of(base)?,
            self.tree_of(local)?,
            self.tree_of(remote)?,
        );
        let theirs: BTreeSet<String> = self
            .differences(Some(base_tree), Some(remote_tree))?
            .into_iter()
            .map(|difference| difference.path)
            .filter(|path| !in_store(path))
            .collect();
        // The paths the project's versions change, with those versions,
        // oldest first.
        let mut mine: BTreeMap<String, Vec<ObjectId>> = BTreeMap::new();
        for &commit in analysis.local.iter().rev() {
            let parent = self
                .repo
                .find_commit(commit)?
                .parent_ids()
                .next()
                .map(|id| id.detach());
            let parent_tree = parent.map(|id| self.tree_of(id)).transpose()?;
            for difference in self.differences(parent_tree, Some(self.tree_of(commit)?))? {
                if theirs.contains(&difference.path) {
                    mine.entry(difference.path).or_default().push(commit);
                }
            }
        }
        let mut out = Vec::new();
        let mut copies: Vec<String> = Vec::new();
        for (path, versions) in mine {
            let in_local = self.blob_entry(local_tree, &path)?;
            let in_remote = self.blob_entry(remote_tree, &path)?;
            if in_local == in_remote {
                continue;
            }
            let kind = if in_local.is_none() {
                ConflictKind::DeletedMine
            } else if in_remote.is_none() {
                ConflictKind::DeletedTheirs
            } else if self.blob_entry(base_tree, &path)?.is_none() {
                ConflictKind::AddedBoth
            } else {
                ConflictKind::Modified
            };
            // The remote's versions that changed it, newest first.
            let mut theirs_versions = Vec::new();
            for &commit in &analysis.remote_versions {
                let parent = self
                    .repo
                    .find_commit(commit)?
                    .parent_ids()
                    .next()
                    .map(|id| id.detach());
                let before = match parent {
                    Some(parent) => self.entry_at(parent, &path)?,
                    None => None,
                };
                if self.entry_at(commit, &path)? != before {
                    theirs_versions.push(commit);
                }
            }
            let latest_mine = *versions.last().expect("a version changed it");
            let mine_version = self.sync_version(latest_mine)?;
            let copy = may_copy(&path).then(|| {
                let date = mine_version.date.get(..16).unwrap_or(&mine_version.date);
                copy_path(
                    &path,
                    &mine_version.author.name,
                    &date.replace(':', "."),
                    |p| {
                        copies.iter().any(|c| c == p)
                            || self.blob_entry(local_tree, p).ok().flatten().is_some()
                            || self.blob_entry(remote_tree, p).ok().flatten().is_some()
                            || self.root.join(p).exists()
                    },
                )
            });
            if let Some(copy) = &copy {
                copies.push(copy.clone());
            }
            out.push(Conflict {
                kind,
                mine: ConflictSide {
                    blob: in_local.map(|id| id.to_string()),
                    version: Some(mine_version),
                    versions: versions.len(),
                },
                theirs: ConflictSide {
                    blob: in_remote.map(|id| id.to_string()),
                    version: theirs_versions
                        .first()
                        .map(|id| self.sync_version(*id))
                        .transpose()?,
                    versions: theirs_versions.len(),
                },
                choices: if copy.is_some() {
                    vec![Resolution::Mine, Resolution::Theirs, Resolution::Copy]
                } else {
                    vec![Resolution::Mine, Resolution::Theirs]
                },
                copy,
                path,
            });
        }
        Ok(out)
    }

    /// Fetches (unless `options.fetch` is false), then brings the project
    /// and the remote together ([`SyncCase`]): pushes the project's new
    /// versions, takes the remote's, or replays the project's after the
    /// remote's with `options`' choices for the files changed on both sides,
    /// keeping the old history in a backup reference and moving the branch
    /// and the folder with `git reset --keep`; then pushes (unless
    /// `options.push` is false), starting again up to 3 times when the
    /// remote moved meanwhile. `committer` (needed for a replay) records the
    /// replayed versions, which keep their authors, times and messages.
    ///
    /// A failure of the remote, a conflict without a choice, changes in the
    /// folder in the way and a cancellation are [`SyncOutcome::error`], with
    /// what was done before; nothing changes locally before the backup
    /// reference is written.
    pub fn sync(
        &self,
        options: &SyncOptions,
        committer: Option<&Identity>,
        control: Option<&Control>,
    ) -> Result<SyncOutcome, VcsError> {
        let (branch, upstream) = self.current_upstream(&self.fresh()?)?;
        let mut outcome = SyncOutcome {
            upstream: upstream.short(),
            branch,
            ..SyncOutcome::default()
        };
        match self.sync_rounds(&upstream, options, committer, control, &mut outcome) {
            Ok(()) => {}
            Err(VcsError::Remote(error)) => outcome.error = Some(error),
            Err(error) => {
                self.record(Attempted::Sync, Some(&error));
                return Err(error);
            }
        }
        let changed = outcome.backup.is_some() || !outcome.changed_paths.is_empty();
        if changed || outcome.pushed {
            self.gc_auto();
        }
        let repo = self.fresh()?;
        outcome.head = head_of(&repo)?.map(|id| id.to_string());
        (outcome.ahead, outcome.behind) = ahead_behind(&repo, &upstream.tracking())?;
        if let Ok(local) = self.local_changes(None) {
            outcome.uncommitted = local.into_iter().filter(|p| !in_store(p)).collect();
        }
        outcome.changed_paths.sort();
        outcome.changed_paths.dedup();
        self.record(
            Attempted::Sync,
            outcome.error.clone().map(VcsError::Remote).as_ref(),
        );
        Ok(outcome)
    }

    fn sync_rounds(
        &self,
        upstream: &Upstream,
        options: &SyncOptions,
        committer: Option<&Identity>,
        control: Option<&Control>,
        outcome: &mut SyncOutcome,
    ) -> Result<(), VcsError> {
        // A round after the remote moved asks again for the choices: the
        // remote's newer versions were not seen when they were made.
        let again = SyncOptions {
            resolutions: BTreeMap::new(),
            resolve_all: None,
            ..options.clone()
        };
        for round in 0..=RETRIES {
            outcome.retries = round;
            if options.fetch || round > 0 {
                outcome.fetch = Some(self.fetch(control)?);
            }
            let analysis = self.analyse(upstream)?;
            if round == 0 {
                outcome.case = analysis.case;
            }
            let options = if round == 0 { options } else { &again };
            if let Round::Done = self.sync_round(&analysis, options, committer, control, outcome)? {
                return Ok(());
            }
        }
        Err(failure(
            ErrorClass::Rejected,
            format!(
                "the remote got newer versions during each of {} tries; sync again",
                RETRIES + 1
            ),
        ))
    }

    fn sync_round(
        &self,
        analysis: &Analysis,
        options: &SyncOptions,
        committer: Option<&Identity>,
        control: Option<&Control>,
        outcome: &mut SyncOutcome,
    ) -> Result<Round, VcsError> {
        match analysis.case {
            SyncCase::Unsupported => {
                outcome.reason.clone_from(&analysis.reason);
                Err(failure(
                    ErrorClass::Unsupported,
                    analysis.reason.clone().unwrap_or_default(),
                ))
            }
            SyncCase::UpToDate => Ok(Round::Done),
            SyncCase::Push => self.push_round(options, control, outcome),
            SyncCase::FastForward => {
                let changed = self.changed_between(analysis.head, analysis.remote)?;
                self.refuse_local_changes(&changed, control)?;
                // git updating the folder is not stopped halfway.
                cancelled(control)?;
                if let Some(forward) = self.fast_forward(None)? {
                    outcome.changed_paths.extend(forward.changed_paths);
                }
                Ok(Round::Done)
            }
            SyncCase::Replay => self.replay_round(analysis, options, committer, control, outcome),
        }
    }

    fn push_round(
        &self,
        options: &SyncOptions,
        control: Option<&Control>,
        outcome: &mut SyncOutcome,
    ) -> Result<Round, VcsError> {
        if !options.push {
            return Ok(Round::Done);
        }
        self.hook("pushing");
        match self.push(control) {
            Ok(push) => {
                outcome.pushed |= push.pushed;
                outcome.push = Some(push);
                Ok(Round::Done)
            }
            // Another push came first: fetch and replay again.
            Err(VcsError::Remote(error)) if error.class == ErrorClass::Rejected => Ok(Round::Again),
            Err(error) => Err(error),
        }
    }

    /// The choices for the conflicts, with the copies' paths; the
    /// conflicts without one are the error `conflict`.
    fn choices(
        &self,
        analysis: &Analysis,
        options: &SyncOptions,
        outcome: &mut SyncOutcome,
    ) -> Result<BTreeMap<String, (Resolution, Option<String>)>, VcsError> {
        let conflict_at = |path: &str| analysis.conflicts.iter().any(|c| c.path == path);
        let mut given = BTreeMap::new();
        for (key, choice) in &options.resolutions {
            let path = if conflict_at(key) {
                Some(key.clone())
            } else {
                self.relative(Path::new(key))
                    .ok()
                    .filter(|path| conflict_at(path))
            };
            match path {
                Some(path) => {
                    given.insert(path, *choice);
                }
                None => outcome.warnings.push(format!(
                    "{key} is not a file changed both here and on the remote, so its choice is \
                     not used"
                )),
            }
        }
        let mut chosen = BTreeMap::new();
        let mut open = Vec::new();
        for conflict in &analysis.conflicts {
            let choice = given.get(&conflict.path).copied().or_else(|| {
                options
                    .resolve_all
                    .filter(|choice| conflict.choices.contains(choice))
            });
            match choice {
                Some(choice) if conflict.choices.contains(&choice) => {
                    chosen.insert(conflict.path.clone(), (choice, conflict.copy.clone()));
                }
                Some(_) => {
                    return Err(VcsError::Command(format!(
                        "{} cannot be saved as a copy: keep mine or take theirs",
                        conflict.path
                    )));
                }
                None => open.push(conflict.clone()),
            }
        }
        if open.is_empty() {
            return Ok(chosen);
        }
        let paths: Vec<&str> = open.iter().map(|c| c.path.as_str()).collect();
        outcome.conflicts = open.clone();
        Err(failure(
            ErrorClass::Conflict,
            format!(
                "{} changed both here and on the remote: {}. Choose for each: keep mine, take \
                 theirs, or save mine as a copy (the other version stays in the history)",
                if open.len() == 1 {
                    "a file".to_owned()
                } else {
                    format!("{} files", open.len())
                },
                listed(&paths)
            ),
        ))
    }

    fn replay_round(
        &self,
        analysis: &Analysis,
        options: &SyncOptions,
        committer: Option<&Identity>,
        control: Option<&Control>,
        outcome: &mut SyncOutcome,
    ) -> Result<Round, VcsError> {
        let chosen = self.choices(analysis, options, outcome)?;
        let committer = committer.ok_or(VcsError::NoIdentity)?;
        let (Some(local), Some(remote)) = (analysis.head, analysis.remote) else {
            unreachable!("a replay has both sides");
        };
        let replay = self.replay(analysis, remote, &chosen, committer, control, outcome)?;
        self.hook("replayed");
        cancelled(control)?;
        let changed = self.changed_between(Some(local), Some(replay.tip))?;
        self.refuse_local_changes(&changed, control)?;
        // A version recorded meanwhile: the round again.
        if head_of(&self.fresh()?)? != Some(local) {
            return Ok(Round::Again);
        }
        cancelled(control)?;
        // From here on the project changes: the old history is kept first.
        let backup = self.backup(local, committer)?;
        if let Err(error) = self.reset_keep(replay.tip) {
            self.delete_reference(&backup, local, committer);
            return Err(error);
        }
        // A version recorded between the check and git's reset is where git
        // moved from: put the branch back there and replay again.
        let moved_from = reference_id(&self.fresh()?, "ORIG_HEAD")?;
        if let Some(other) = moved_from.filter(|id| *id != local) {
            self.reset_keep(other)?;
            self.delete_reference(&backup, local, committer);
            return Ok(Round::Again);
        }
        let tree = self.tree_of(replay.tip)?;
        let project = mitcad_model::Project::open(&self.root);
        for (path, (choice, copy)) in &chosen {
            outcome.resolved.insert(path.clone(), *choice);
            match (choice, copy) {
                (Resolution::Copy, Some(copy)) if self.blob_entry(tree, copy)?.is_some() => {
                    // The display state goes with the design it was saved with.
                    if let Some(project) = &project {
                        let _ =
                            project.move_local_state(&self.root.join(path), &self.root.join(copy));
                    }
                    outcome.copies.push(ConflictCopy {
                        path: path.clone(),
                        copy: copy.clone(),
                    });
                }
                (Resolution::Theirs, _) => outcome.warnings.push(format!(
                    "{path} is the remote's now; this project's versions of it are kept in {backup}"
                )),
                _ => {}
            }
        }
        outcome.replayed.extend(replay.replayed);
        outcome.changed_paths.extend(changed);
        outcome.backup = Some(backup);
        self.push_round(options, control, outcome)
    }

    /// The project's versions replayed after the remote's (`remote`), oldest
    /// first: each one's changes applied to the tree so far as `chosen`
    /// says (a path of a conflict: mine as it is, theirs left out, copy under
    /// the copy's path), the B-rep store reconciled, and a commit with the
    /// version's author, time and message by `committer`. A version left
    /// with no change is left out.
    fn replay(
        &self,
        analysis: &Analysis,
        remote: ObjectId,
        chosen: &BTreeMap<String, (Resolution, Option<String>)>,
        committer: &Identity,
        control: Option<&Control>,
        outcome: &mut SyncOutcome,
    ) -> Result<Replay, VcsError> {
        let local = analysis.head.expect("a replay has the project's versions");
        let (local_tree, remote_tree) = (self.tree_of(local)?, self.tree_of(remote)?);
        let committer = gix::actor::Signature {
            name: committer.name.as_str().into(),
            email: committer.email.as_str().into(),
            time: gix::date::Time::now_local_or_utc(),
        };
        let mut replay = Replay {
            tip: remote,
            replayed: Vec::new(),
        };
        let mut tree = remote_tree;
        for &id in analysis.local.iter().rev() {
            cancelled(control)?;
            let commit = self.repo.find_commit(id)?;
            let parent = commit
                .parent_ids()
                .next()
                .expect("a replayed version has one parent")
                .detach();
            let own_tree = commit.tree_id()?.detach();
            let mut editor = self.repo.edit_tree(tree)?;
            let mut touched = false;
            for difference in self.differences(Some(self.tree_of(parent)?), Some(own_tree))? {
                // The store follows the project files below.
                if in_store(&difference.path) {
                    continue;
                }
                let target = match chosen.get(&difference.path) {
                    None | Some((Resolution::Mine, _)) => difference.path.as_str(),
                    Some((Resolution::Theirs, _)) | Some((Resolution::Copy, None)) => continue,
                    Some((Resolution::Copy, Some(copy))) => copy.as_str(),
                };
                match (difference.new, difference.mode) {
                    (Some(blob), Some(mode)) => editor.upsert(target, mode.kind(), blob)?,
                    _ => editor.remove(target)?,
                };
                touched = true;
            }
            let applied = editor.write()?.detach();
            let message = commit.message_raw()?.to_string();
            let summary = user_message(&message)
                .lines()
                .next()
                .unwrap_or("")
                .to_owned();
            let next = if touched && applied != tree {
                self.reconcile_tree(
                    applied,
                    &[own_tree, remote_tree, local_tree],
                    &mut outcome.warnings,
                )?
            } else {
                tree
            };
            if next == tree {
                replay.replayed.push(Replayed {
                    from: id.to_string(),
                    to: None,
                    summary,
                });
                continue;
            }
            let decoded = commit.decode()?;
            let author = commit.author()?;
            let new = gix::objs::Commit {
                tree: next,
                parents: std::iter::once(replay.tip).collect(),
                author: gix::actor::Signature {
                    name: author.name.to_owned(),
                    email: author.email.to_owned(),
                    time: author.time()?,
                },
                committer: committer.clone(),
                encoding: decoded.encoding.map(ToOwned::to_owned),
                message: decoded.message.to_owned(),
                extra_headers: Vec::new(),
            };
            let new_id = self.repo.write_object(&new)?.detach();
            replay.replayed.push(Replayed {
                from: id.to_string(),
                to: Some(new_id.to_string()),
                summary,
            });
            replay.tip = new_id;
            tree = next;
        }
        Ok(replay)
    }

    /// A tree whose B-rep store holds what its project files refer to, as a
    /// recorded version's does: the missing files taken from the first of
    /// `sources` (trees) that has them, the files none refers to removed
    /// (unless a project file cannot be read).
    fn reconcile_tree(
        &self,
        tree: ObjectId,
        sources: &[ObjectId],
        warnings: &mut Vec<String>,
    ) -> Result<ObjectId, VcsError> {
        let mut referenced = BTreeSet::new();
        let mut unreadable = Vec::new();
        for (path, id) in self.project_files(tree)? {
            match self.references_with(id, || Ok(self.repo.find_blob(id)?.take_data())) {
                Some(references) => referenced.extend(references),
                None => unreadable.push(path),
            }
        }
        let present: BTreeSet<String> = self.brep_files(tree)?.into_iter().collect();
        let mut editor = self.repo.edit_tree(tree)?;
        let mut warn = |warning: String| {
            if !warnings.contains(&warning) {
                warnings.push(warning);
            }
        };
        for sha256 in &referenced {
            let path = brep_path(sha256);
            if present.contains(&path) {
                continue;
            }
            let mut found = None;
            for source in sources {
                found = self.blob_entry(*source, &path)?;
                if found.is_some() {
                    break;
                }
            }
            match found {
                Some(blob) => {
                    editor.upsert(path.as_str(), EntryKind::Blob, blob)?;
                }
                None => warn(format!(
                    "the B-rep file {path} is in neither version, so the replayed version lacks it"
                )),
            }
        }
        if unreadable.is_empty() {
            for path in present {
                if brep_sha256(&path).is_some_and(|sha256| !referenced.contains(&sha256)) {
                    editor.remove(path.as_str())?;
                }
            }
        } else {
            warn(format!(
                "{} could not be read as a project file, so no B-rep file is removed",
                unreadable.join(", ")
            ));
        }
        Ok(editor.write()?.detach())
    }

    /// `git reset --keep <target>`: the branch and the paths that differ
    /// between HEAD and `target` move to it; git refuses (changing nothing)
    /// when such a path has changes no version holds.
    fn reset_keep(&self, target: ObjectId) -> Result<(), VcsError> {
        // git compares the index's file times, not contents: a file written
        // again with the content it had would count as changed. (It fails
        // when files are changed, which the reset then tells.)
        let _ = self.git_ok(
            "Refreshing the index",
            &["update-index", "-q", "--refresh"],
            None,
        );
        let id = target.to_string();
        match self.git_ok(
            "Updating the project's files",
            &["reset", "--keep", "--quiet", &id],
            None,
        ) {
            Ok(_) => Ok(()),
            Err(VcsError::Remote(mut error)) => {
                // `error: Entry 'part.mitcad' not uptodate. Cannot merge.`,
                // `Untracked working tree file 'x' would be overwritten`.
                let blocked: Vec<&str> = error
                    .detail
                    .lines()
                    .filter(|line| {
                        line.contains("not uptodate") || line.contains("would be overwritten")
                    })
                    .filter_map(|line| line.split('\'').nth(1))
                    .collect();
                if !blocked.is_empty() {
                    error.class = ErrorClass::LocalChanges;
                    error.message = format!(
                        "the sync would change {} in the folder, which has changes no version \
                         holds: record them as versions (or undo the changes), then sync again",
                        listed(&blocked)
                    );
                }
                Err(error.into())
            }
            Err(error) => Err(error),
        }
    }

    /// Edits references, written in their logs as by `who`.
    fn edit_references(&self, edits: Vec<RefEdit>, who: &Identity) -> Result<(), VcsError> {
        let signature = gix::actor::Signature {
            name: who.name.as_str().into(),
            email: who.email.as_str().into(),
            time: gix::date::Time::now_local_or_utc(),
        };
        let mut time = Default::default();
        self.repo
            .edit_references_as(edits, Some(signature.to_ref(&mut time)))?;
        Ok(())
    }

    fn reference_edit(name: &str, change: Change) -> Result<RefEdit, VcsError> {
        Ok(RefEdit {
            change,
            name: name
                .try_into()
                .map_err(|e| VcsError::Git(format!("{name}: {e}")))?,
            deref: false,
        })
    }

    /// Keeps `target` (the history before a sync) in a new reference
    /// `refs/mitcad/sync-backup/<yyyymmdd-HHMMSS>` (UTC), and removes the
    /// backups older than 30 days except the newest 5.
    fn backup(&self, target: ObjectId, who: &Identity) -> Result<String, VcsError> {
        let now = gix::date::Time::now_utc().seconds;
        let repo = self.fresh()?;
        let stamped = format!("{BACKUP_PREFIX}{}", stamp(now));
        let mut name = stamped.clone();
        let mut n = 1;
        while reference_id(&repo, &name)?.is_some() {
            n += 1;
            name = format!("{stamped}-{n}");
        }
        let edit = Self::reference_edit(
            &name,
            Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: "sync: the versions before the sync".into(),
                },
                expected: PreviousValue::MustNotExist,
                new: Target::Object(target),
            },
        )?;
        self.edit_references(vec![edit], who)?;
        let _ = self.prune_backups(&stamp(now - BACKUP_DAYS * 24 * 3600), who);
        Ok(name)
    }

    /// Removes the backups named before `cutoff` (a stamp), except the
    /// newest [`BACKUPS_KEPT`].
    pub(crate) fn prune_backups(&self, cutoff: &str, who: &Identity) -> Result<(), VcsError> {
        let repo = self.fresh()?;
        let mut backups = Vec::new();
        for reference in repo.references()?.all()? {
            let reference = reference?;
            let name = reference.name().as_bstr().to_string();
            if name.starts_with(BACKUP_PREFIX)
                && let Some(id) = reference.try_id()
            {
                backups.push((name, id.detach()));
            }
        }
        backups.sort_by(|a, b| b.0.cmp(&a.0));
        let mut edits = Vec::new();
        for (name, id) in backups.into_iter().skip(BACKUPS_KEPT) {
            if name[BACKUP_PREFIX.len()..] < *cutoff {
                edits.push(Self::reference_edit(
                    &name,
                    Change::Delete {
                        expected: PreviousValue::MustExistAndMatch(Target::Object(id)),
                        log: RefLog::AndReference,
                    },
                )?);
            }
        }
        if !edits.is_empty() {
            self.edit_references(edits, who)?;
        }
        Ok(())
    }

    /// Removes a backup that is not needed after all (the branch did not
    /// move); a failure leaves it.
    fn delete_reference(&self, name: &str, target: ObjectId, who: &Identity) {
        if let Ok(edit) = Self::reference_edit(
            name,
            Change::Delete {
                expected: PreviousValue::MustExistAndMatch(Target::Object(target)),
                log: RefLog::AndReference,
            },
        ) {
            let _ = self.edit_references(vec![edit], who);
        }
    }
}

// The remote's newer versions (the application's notice).

impl ProjectRepo {
    /// The remote's versions the project lacks, from the local references
    /// as the last fetch left them, and with `file` those that changed it:
    /// whether the remote has a newer version of the file than the project.
    pub fn incoming(&self, file: Option<&Path>) -> Result<Incoming, VcsError> {
        let repo = self.fresh()?;
        let (_, upstream) = self.current_upstream(&repo)?;
        let head = head_of(&repo)?;
        let remote = reference_id(&repo, &upstream.tracking())?;
        let mut incoming = Incoming {
            upstream: upstream.short(),
            path: file.map(|file| self.relative(file)).transpose()?,
            ..Incoming::default()
        };
        let Some(remote) = remote else {
            // Never fetched or pushed: nothing known of the remote.
            return Ok(incoming);
        };
        let theirs = walk(&repo, remote, head, COUNT_LIMIT)?;
        let mine = match head {
            Some(head) => walk(&repo, head, Some(remote), COUNT_LIMIT)?,
            None => Vec::new(),
        };
        incoming.ahead = mine.len();
        incoming.behind = theirs.len();
        for id in theirs.iter().take(LIST_LIMIT) {
            incoming.versions.push(self.sync_version(*id)?);
        }
        let Some(path) = incoming.path.clone() else {
            return Ok(incoming);
        };
        let at = |commit: Option<ObjectId>| -> Result<Option<ObjectId>, VcsError> {
            commit.map_or(Ok(None), |commit| self.entry_at(commit, &path))
        };
        // Whether a version changed the file against its first parent.
        let changed = |id: ObjectId| -> Result<bool, VcsError> {
            let parent = repo
                .find_commit(id)?
                .parent_ids()
                .next()
                .map(|id| id.detach());
            Ok(at(Some(id))? != at(parent)?)
        };
        incoming.differs = at(Some(remote))? != at(head)?;
        for (id, version) in theirs.iter().zip(&incoming.versions) {
            if changed(*id)? {
                incoming.file_versions.push(version.clone());
            }
        }
        for id in &mine {
            if changed(*id)? {
                incoming.changed_here = true;
                break;
            }
        }
        Ok(incoming)
    }
}
