// SPDX-License-Identifier: MIT
//! Remote repositories (P12 remote): a project's version history shared
//! through a git server (Forgejo, GitHub, GitLab, any git host) or a
//! repository in a folder.
//!
//! The network goes through the system's git program ([`GitCli`]): `git
//! ls-remote`, `fetch`, `push` and `clone`, and `git merge --ff-only` to
//! bring the work tree up to the remote's versions. Objects, trees, commits
//! and the counts of versions ahead and behind are read with gix from the
//! local references, without the network. Signing in is git's: SSH keys and
//! the SSH agent, or a credential helper for HTTPS. Mitcad never asks for,
//! keeps or passes passwords or tokens, and refuses URLs that hold them
//! ([`check_url`]); what git prints is shown and logged with credentials
//! hidden ([`redact`]).
//!
//! - [`ProjectRepo::remote_check`]: whether a remote can be reached, is
//!   empty, holds a Mitcad project and shares the project's history.
//! - [`ProjectRepo::remote_set`] and [`ProjectRepo::remote_remove`]: the
//!   remote (`origin`), the branch's upstream, and `.gitattributes` with
//!   `merge=binary` for project files recorded as a version.
//! - [`ProjectRepo::connect`]: check, set, then push to an empty remote or
//!   fetch from one that shares the history; another history is refused
//!   without changing anything.
//! - [`ProjectRepo::fetch`], [`ProjectRepo::push`] (only versions the remote
//!   lacks; never forced; files too large for a git host held back),
//!   [`ProjectRepo::fast_forward`] (the remote's newer versions when the
//!   project has none of its own).
//! - [`ProjectRepo::sync_plan`] and [`ProjectRepo::sync`] (`sync.rs`): fetch,
//!   then the project's unpublished versions replayed file by file after the
//!   remote's newer ones, with a choice for each file changed on both sides,
//!   the folder brought along (`git reset --keep`), and a push.
//! - [`clone_project`]: a project opened from a remote into a new folder.
//! - [`ProjectRepo::remote_info`] and [`git_info`]: what the status bar and
//!   the settings show.
//!
//! The JSON commands are in `commands.rs` (`core/model/src/api/commands.md`,
//! "Remote repositories").

pub(crate) mod cli;
pub(crate) mod commands;
pub(crate) mod errors;
// Sync and conflicts.
pub(crate) mod sync;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use gix::ObjectId;
use gix::bstr::ByteSlice;
use mitcad_model::file::PROJECT_MARKER;
use serde::{Deserialize, Serialize};

use crate::{Identity, ProjectRepo, VcsError, absolute, io_error};
use cli::GitOutput;
pub use cli::{Control, GitCli, GitVersion, MIN_VERSION, Progress};
pub use errors::{ErrorClass, RemoteError, check_url, classify, redact};
pub use sync::{
    BACKUP_PREFIX, Conflict, ConflictCopy, ConflictKind, ConflictSide, Incoming, Replayed,
    Resolution, SyncCase, SyncOptions, SyncOutcome, SyncPlan, SyncVersion,
};

/// The remote a project is connected to.
pub const DEFAULT_REMOTE: &str = "origin";
/// The message of the version that configures a project for sync.
pub const CONFIGURE_MESSAGE: &str = "Configure project for sync";
/// Versions ahead and behind are counted up to this many.
const COUNT_LIMIT: usize = 10_000;
/// How far the histories are followed to find a common version.
const HISTORY_LIMIT: usize = 100_000;
/// The last fetch, push and sync, per user and not versioned.
const REMOTE_STATE: &str = ".mitcad/local/remote.json";
/// A push holds back a file larger than this (GitHub refuses files over
/// 100 MiB) ...
const MAX_FILE_SIZE: u64 = 100 << 20;
/// ... and warns about one larger than this (GitHub warns over 50 MiB).
const LARGE_FILE_SIZE: u64 = 50 << 20;

impl From<RemoteError> for VcsError {
    fn from(error: RemoteError) -> Self {
        VcsError::Remote(error)
    }
}

fn failure(class: ErrorClass, message: impl Into<String>) -> VcsError {
    RemoteError::new(class, message).into()
}

fn mebibytes(bytes: u64) -> f64 {
    bytes as f64 / f64::from(1 << 20)
}

/// The git program, its version and git-lfs: what Preferences shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitInfo {
    /// The program (None: not found).
    pub path: Option<String>,
    pub version: Option<String>,
    /// git-lfs's version, or None without it.
    pub lfs: Option<String>,
    /// Whether the version is [`MIN_VERSION`] or newer.
    pub supported: bool,
    pub minimum: String,
    /// Why there is no usable git.
    pub error: Option<RemoteError>,
}

/// The git program `git` (else the one [`GitCli::find`] finds), its version
/// and git-lfs's.
pub fn git_info(git: Option<&GitCli>) -> GitInfo {
    let minimum = format!("{}.{}", MIN_VERSION.0, MIN_VERSION.1);
    let missing = |path: Option<String>, error: RemoteError| GitInfo {
        path,
        version: None,
        lfs: None,
        supported: false,
        minimum: minimum.clone(),
        error: Some(error),
    };
    let git = match git.cloned().map_or_else(GitCli::find, Ok) {
        Ok(git) => git,
        Err(error) => return missing(None, error),
    };
    let path = Some(git.program().to_string_lossy().into_owned());
    match git.version() {
        Ok(version) => GitInfo {
            path,
            supported: version.supported(),
            version: Some(version.text),
            lfs: git.lfs_version(),
            minimum: minimum.clone(),
            error: None,
        },
        Err(error) => missing(path, error),
    }
}

/// A remote's branch that the current branch follows.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Upstream {
    remote: String,
    /// The branch on the remote.
    branch: String,
    /// Whether the branch's configuration names it (else it is `origin` and
    /// the branch's own name).
    configured: bool,
}

impl Upstream {
    /// The remote-tracking reference (the default fetch refspec).
    fn tracking(&self) -> String {
        format!("refs/remotes/{}/{}", self.remote, self.branch)
    }

    fn short(&self) -> String {
        format!("{}/{}", self.remote, self.branch)
    }
}

/// When a fetch or a push was last tried, and how it failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    /// Seconds since 1970 (UTC) and as text in local time.
    pub time: i64,
    pub date: String,
    pub error: Option<RemoteError>,
}

/// `.mitcad/local/remote.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RemoteState {
    #[serde(default)]
    fetch: Option<Attempt>,
    #[serde(default)]
    push: Option<Attempt>,
    #[serde(default)]
    sync: Option<Attempt>,
}

/// What [`ProjectRepo::record`] notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Attempted {
    Fetch,
    Push,
    Sync,
}

/// A linked component whose file is outside the project: clones of the
/// project do not have it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutsideLink {
    /// The project file that links it, relative to the project.
    pub file: String,
    /// The component's name.
    pub component: String,
    /// The link's path as saved.
    pub path: String,
}

/// A project's remote as the local references know it (no network).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteInfo {
    /// The remote the branch follows, else `origin` when there is one.
    pub name: Option<String>,
    /// Its URL, credentials hidden.
    pub url: Option<String>,
    pub branch: Option<String>,
    /// `origin/main`.
    pub upstream: Option<String>,
    /// Versions the remote lacks and versions the project lacks, by the
    /// remote-tracking reference of the last fetch or push; None before
    /// one.
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    pub last_fetch: Option<Attempt>,
    pub last_push: Option<Attempt>,
    pub last_sync: Option<Attempt>,
    /// Linked components outside the project (asked for: it reads every
    /// project file of the latest version).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_links: Option<Vec<OutsideLink>>,
}

/// What a remote holds, before connecting to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteCheck {
    /// The URL as used, credentials hidden.
    pub url: String,
    pub reachable: bool,
    /// No branches.
    pub empty: bool,
    /// The branch its HEAD names (else `main`, `master`, the only one).
    pub default_branch: Option<String>,
    /// That branch's commit.
    pub head: Option<String>,
    /// Whether that commit has a Mitcad project at its root; None when
    /// empty.
    pub has_project: Option<bool>,
    /// Whether it shares a version with the project's history; None when
    /// empty.
    pub related: Option<bool>,
}

/// A remote set for the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteSet {
    pub name: String,
    /// Credentials hidden (there are none: such a URL is refused).
    pub url: String,
    pub branch: String,
    /// `origin/main`.
    pub upstream: String,
    /// The version `Configure project for sync` (`.gitattributes`), when
    /// the project needed one.
    pub commit: Option<String>,
    pub written: Vec<String>,
}

/// A remote-tracking reference a fetch changed (None: not there).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefUpdate {
    pub name: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FetchOutcome {
    pub remote: String,
    pub updated: Vec<RefUpdate>,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PushOutcome {
    pub remote: String,
    /// The branch on the remote.
    pub branch: String,
    /// False when the remote had every version already.
    pub pushed: bool,
    /// The versions pushed, when the remote's branch was known before.
    pub versions: Option<usize>,
    pub head: String,
    /// Files of the versions pushed that some git hosts warn about (over
    /// 50 MiB).
    pub warnings: Vec<String>,
}

/// The project brought up to the remote's newer versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FastForward {
    pub from: Option<String>,
    pub to: String,
    /// The paths that changed in the folder.
    pub changed_paths: Vec<String>,
}

/// A project connected to a remote ([`ProjectRepo::connect`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Connected {
    pub check: RemoteCheck,
    pub set: RemoteSet,
    /// The fetch from a remote that had versions.
    pub fetch: Option<FetchOutcome>,
    pub push: Option<PushOutcome>,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    /// A step after setting the remote that failed (the remote stays set).
    #[serde(skip)]
    pub error: Option<RemoteError>,
}

/// A project opened from a remote ([`clone_project`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloneOutcome {
    pub root: String,
    pub url: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    /// The project files of its latest version, to choose from.
    pub files: Vec<String>,
}

/// Checks a remote's name: what git allows, leaving out what could be read
/// as an option or a path.
fn check_name(name: &str) -> Result<(), VcsError> {
    let valid = !name.is_empty()
        && !name.starts_with(['-', '.'])
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    if valid {
        Ok(())
    } else {
        Err(VcsError::Command(format!(
            "'{name}' is not a remote's name (letters, digits, '.', '_' and '-')"
        )))
    }
}

/// The branch HEAD is on.
fn branch_of(repo: &gix::Repository) -> Result<Option<String>, VcsError> {
    Ok(repo
        .head_name()?
        .map(|name| name.shorten().to_str_lossy().into_owned()))
}

/// The commit HEAD points to.
fn head_of(repo: &gix::Repository) -> Result<Option<ObjectId>, VcsError> {
    Ok(repo.head()?.id().map(|id| id.detach()))
}

/// A configuration value, None when not set or empty.
fn config(repo: &gix::Repository, key: &str) -> Option<String> {
    repo.config_snapshot()
        .string(key)
        .map(|value| value.to_str_lossy().into_owned())
        .filter(|value| !value.is_empty())
}

fn remote_url(repo: &gix::Repository, name: &str) -> Option<String> {
    config(repo, &format!("remote.{name}.url"))
}

/// The commit a reference points to.
fn reference_id(repo: &gix::Repository, name: &str) -> Result<Option<ObjectId>, VcsError> {
    Ok(repo
        .try_find_reference(name)?
        .and_then(|reference| reference.try_id())
        .map(|id| id.detach()))
}

/// The commits reachable from `tip` but not from `hidden` (`git rev-list
/// --count hidden..tip`), up to [`COUNT_LIMIT`].
fn count_only(repo: &gix::Repository, tip: ObjectId, hidden: ObjectId) -> Result<usize, VcsError> {
    let mut count = 0;
    for info in repo.rev_walk([tip]).with_hidden([hidden]).all()? {
        info?;
        count += 1;
        if count >= COUNT_LIMIT {
            break;
        }
    }
    Ok(count)
}

/// Versions ahead of and behind the remote-tracking reference `tracking`;
/// None when either side is missing.
fn ahead_behind(
    repo: &gix::Repository,
    tracking: &str,
) -> Result<(Option<usize>, Option<usize>), VcsError> {
    match (head_of(repo)?, reference_id(repo, tracking)?) {
        (Some(local), Some(remote)) => Ok((
            Some(count_only(repo, local, remote)?),
            Some(count_only(repo, remote, local)?),
        )),
        _ => Ok((None, None)),
    }
}

/// The remote-tracking references of `remote` and their commits.
fn remote_refs(
    repo: &gix::Repository,
    remote: &str,
) -> Result<BTreeMap<String, ObjectId>, VcsError> {
    let prefix = format!("refs/remotes/{remote}/");
    let mut out = BTreeMap::new();
    let platform = repo.references()?;
    for reference in platform.remote_branches()? {
        let reference = reference?;
        let name = reference.name().as_bstr().to_str_lossy().into_owned();
        if name.starts_with(&prefix)
            && let Some(id) = reference.try_id()
        {
            out.insert(name, id.detach());
        }
    }
    Ok(out)
}

/// What `git ls-remote --symref` lists: the default branch, its commit,
/// and whether there are no branches.
fn parse_ls_remote(listing: &str) -> (Option<String>, Option<ObjectId>, bool) {
    let mut symref = None;
    let mut head = None;
    let mut branches: Vec<(String, ObjectId)> = Vec::new();
    for line in listing.lines() {
        let Some((left, name)) = line.split_once('\t') else {
            continue;
        };
        if let Some(target) = left.strip_prefix("ref: ") {
            if name == "HEAD" {
                symref = target.strip_prefix("refs/heads/").map(str::to_owned);
            }
            continue;
        }
        let Ok(id) = ObjectId::from_hex(left.trim().as_bytes()) else {
            continue;
        };
        if name == "HEAD" {
            head = Some(id);
        } else if let Some(branch) = name.strip_prefix("refs/heads/") {
            branches.push((branch.to_owned(), id));
        }
    }
    let find = |name: &str| branches.iter().find(|(b, _)| b == name).cloned();
    let chosen = symref
        .as_deref()
        .and_then(find)
        .or_else(|| head.and_then(|h| branches.iter().find(|(_, id)| *id == h).cloned()))
        .or_else(|| find("main"))
        .or_else(|| find("master"))
        .or_else(|| (branches.len() == 1).then(|| branches[0].clone()));
    let empty = branches.is_empty();
    match chosen {
        Some((branch, id)) => (Some(branch), Some(id), empty),
        None => (None, None, empty),
    }
}

/// A line of the remote log: the git command (credentials hidden) and how
/// it ended.
fn log_line(args: &[&str], result: &Result<GitOutput, RemoteError>) -> String {
    let outcome = match result {
        Ok(output) if output.success => "ok".to_owned(),
        Ok(output) => {
            let class = classify(&format!("{}\n{}", output.stdout, output.stderr));
            let class = serde_json::to_value(class).expect("serializable");
            match output.code {
                Some(code) => format!("exit {code} ({})", class.as_str().unwrap_or("")),
                None => format!("ended ({})", class.as_str().unwrap_or("")),
            }
        }
        Err(error) => error.message.clone(),
    };
    redact(&format!("git {}: {outcome}", args.join(" ")))
}

/// Runs git, logging the command, and fails with `what` when git fails.
fn run_logged(
    git: &GitCli,
    dir: &Path,
    args: &[&str],
    control: Option<&Control>,
    what: &str,
    log: &mut Vec<String>,
) -> Result<GitOutput, VcsError> {
    let result = git.run(Some(dir), args, control);
    log.push(log_line(args, &result));
    let output = result?;
    if output.success {
        Ok(output)
    } else {
        Err(RemoteError::from_git(what, &format!("{}\n{}", output.stderr, output.stdout)).into())
    }
}

impl ProjectRepo {
    /// The git program for this project's remote work (tests: with a fake
    /// SSH command), instead of the one [`GitCli::find`] finds.
    pub fn set_git(&self, git: GitCli) {
        *self.git.borrow_mut() = Some(git);
    }

    /// The git commands run since the last call, credentials hidden, with
    /// how they ended: the remote commands' `log`.
    pub fn take_remote_log(&self) -> Vec<String> {
        std::mem::take(&mut *self.remote_log.borrow_mut())
    }

    fn git_cli(&self) -> Result<GitCli, RemoteError> {
        match &*self.git.borrow() {
            Some(git) => Ok(git.clone()),
            None => GitCli::find(),
        }
    }

    /// Runs git in the project's folder; fails with `what` when git fails.
    fn git_ok(
        &self,
        what: &str,
        args: &[&str],
        control: Option<&Control>,
    ) -> Result<GitOutput, VcsError> {
        let git = self.git_cli()?;
        let mut log = self.remote_log.borrow_mut();
        run_logged(&git, &self.root, args, control, what, &mut log)
    }

    /// The repository opened again: git changes its configuration and
    /// references, which an open one may have read before.
    pub(crate) fn fresh(&self) -> Result<gix::Repository, VcsError> {
        Ok(gix::open(self.repo.git_dir())?)
    }

    /// The branch HEAD is on, or why the operation cannot run.
    fn current_branch(&self) -> Result<String, VcsError> {
        self.branch()?.ok_or_else(|| {
            failure(
                ErrorClass::Unsupported,
                "HEAD is detached (not on a branch); put it on a branch with git first",
            )
        })
    }

    /// The remote's branch that `branch` follows: its configuration, else
    /// `origin`'s branch of the same name when there is an `origin`.
    fn upstream_of(&self, repo: &gix::Repository, branch: &str) -> Option<Upstream> {
        let configured =
            config(repo, &format!("branch.{branch}.remote")).filter(|remote| remote != ".");
        let remote = match &configured {
            Some(remote) => remote.clone(),
            None if remote_url(repo, DEFAULT_REMOTE).is_some() => DEFAULT_REMOTE.to_owned(),
            None => return None,
        };
        let merge = configured
            .as_ref()
            .and_then(|_| config(repo, &format!("branch.{branch}.merge")));
        let remote_branch = merge
            .as_deref()
            .and_then(|merge| merge.strip_prefix("refs/heads/"))
            .unwrap_or(branch)
            .to_owned();
        Some(Upstream {
            remote,
            branch: remote_branch,
            configured: configured.is_some(),
        })
    }

    /// The upstream of the current branch, or why there is none.
    fn current_upstream(&self, repo: &gix::Repository) -> Result<(String, Upstream), VcsError> {
        let branch = self.current_branch()?;
        let upstream = self.upstream_of(repo, &branch).ok_or_else(|| {
            failure(
                ErrorClass::NoRemote,
                "the project has no remote repository; connect it to one first",
            )
        })?;
        Ok((branch, upstream))
    }

    fn remote_state(&self) -> RemoteState {
        fs::read_to_string(self.root.join(REMOTE_STATE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Notes a fetch, push or sync and how it ended
    /// (`.mitcad/local/remote.json`).
    fn record(&self, what: Attempted, error: Option<&VcsError>) {
        let now = gix::date::Time::now_local_or_utc();
        let attempt = Attempt {
            time: now.seconds,
            date: now.format_or_unix(gix::date::time::format::ISO8601),
            error: error.map(|error| match error {
                VcsError::Remote(error) => error.clone(),
                other => RemoteError::new(ErrorClass::Other, other.to_string()),
            }),
        };
        let mut state = self.remote_state();
        match what {
            Attempted::Fetch => state.fetch = Some(attempt),
            Attempted::Push => state.push = Some(attempt),
            Attempted::Sync => state.sync = Some(attempt),
        }
        let text = serde_json::json!({
            "format": "mitcad-local",
            "version": 1,
            "fetch": state.fetch,
            "push": state.push,
            "sync": state.sync,
        });
        let _ = mitcad_model::file::write_atomically(
            &self.root.join(REMOTE_STATE),
            format!("{text:#}\n").as_bytes(),
        );
    }

    /// The project's remote as the local references know it; with `links`
    /// also the linked components outside the project.
    pub fn remote_info(&self, links: bool) -> Result<RemoteInfo, VcsError> {
        let repo = self.fresh()?;
        let branch = branch_of(&repo)?;
        let upstream = branch
            .as_deref()
            .and_then(|branch| self.upstream_of(&repo, branch));
        let (ahead, behind) = match &upstream {
            Some(upstream) => ahead_behind(&repo, &upstream.tracking())?,
            None => (None, None),
        };
        let name = match &upstream {
            Some(upstream) => Some(upstream.remote.clone()),
            None => remote_url(&repo, DEFAULT_REMOTE).map(|_| DEFAULT_REMOTE.to_owned()),
        };
        let url = name
            .as_deref()
            .and_then(|name| remote_url(&repo, name))
            .map(|url| redact(&url));
        let state = self.remote_state();
        Ok(RemoteInfo {
            name,
            url,
            branch,
            upstream: upstream.as_ref().map(Upstream::short),
            ahead,
            behind,
            last_fetch: state.fetch,
            last_push: state.push,
            last_sync: state.sync,
            external_links: if links {
                Some(self.outside_links()?)
            } else {
                None
            },
        })
    }

    /// The linked components of the latest version's project files whose
    /// files are outside the project.
    fn outside_links(&self) -> Result<Vec<OutsideLink>, VcsError> {
        let Some(head) = self.head_commit()? else {
            return Ok(Vec::new());
        };
        let tree = self.repo.find_commit(head)?.tree_id()?.detach();
        let mut out = Vec::new();
        for (file, blob) in self.project_files(tree)? {
            let data = self.repo.find_blob(blob)?.take_data();
            let Ok(design) = serde_json::from_slice::<serde_json::Value>(&data) else {
                continue;
            };
            let folder = self
                .root
                .join(&file)
                .parent()
                .map_or_else(|| self.root.clone(), Path::to_path_buf);
            for component in design["components"].as_array().into_iter().flatten() {
                let Some(path) = component["link"]["path"].as_str() else {
                    continue;
                };
                let target = absolute(&folder.join(path))?;
                if !target.starts_with(&self.root) {
                    out.push(OutsideLink {
                        file: file.clone(),
                        component: component["name"].as_str().unwrap_or("").to_owned(),
                        path: path.to_owned(),
                    });
                }
            }
        }
        Ok(out)
    }

    /// Whether two commits share a version (one is the other's ancestor,
    /// or they have one in common).
    fn related(&self, repo: &gix::Repository, a: ObjectId, b: ObjectId) -> Result<bool, VcsError> {
        if a == b {
            return Ok(true);
        }
        let mut seen = HashSet::new();
        for info in repo.rev_walk([b]).all()?.take(HISTORY_LIMIT) {
            seen.insert(info?.id);
        }
        for info in repo.rev_walk([a]).all()?.take(HISTORY_LIMIT) {
            if seen.contains(&info?.id) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// What the remote at `url` holds: reachable, empty, its default branch,
    /// whether that is a Mitcad project sharing this project's history. A
    /// branch whose commit the project lacks is fetched without a reference
    /// (its objects stay until git's gc, which `git gc --auto` runs when
    /// they add up), so that its history can be read.
    pub fn remote_check(
        &self,
        url: &str,
        control: Option<&Control>,
    ) -> Result<RemoteCheck, VcsError> {
        let url = check_url(url)?;
        let shown = redact(&url);
        let listed = self.git_ok(
            &format!("Reaching {shown}"),
            &["ls-remote", "--symref", &url],
            control,
        )?;
        let (default_branch, tip, empty) = parse_ls_remote(&listed.stdout);
        let mut check = RemoteCheck {
            url: shown.clone(),
            reachable: true,
            empty,
            default_branch: default_branch.clone(),
            head: tip.map(|id| id.to_string()),
            has_project: None,
            related: None,
        };
        let (Some(tip), Some(branch)) = (tip, default_branch) else {
            return Ok(check);
        };
        if !self.fresh()?.has_object(tip) {
            let refspec = format!("refs/heads/{branch}");
            self.git_ok(
                &format!("Reading {shown}"),
                &[
                    "fetch",
                    "--no-tags",
                    "--no-write-fetch-head",
                    "--progress",
                    &url,
                    &refspec,
                ],
                control,
            )?;
            self.gc_auto();
        }
        let repo = self.fresh()?;
        let tree = repo.find_commit(tip)?.tree_id()?.detach();
        check.has_project = Some(self.blob_entry(tree, PROJECT_MARKER)?.is_some());
        check.related = Some(match head_of(&repo)? {
            Some(local) => self.related(&repo, local, tip)?,
            None => false,
        });
        Ok(check)
    }

    /// Sets the remote `name` (added, or its URL changed), makes the current
    /// branch follow its branch of the same name (unless it follows one of
    /// that remote already), and records the recommended `.gitattributes`
    /// (project files merged only as whole files) as the version `Configure
    /// project for sync` by `author` when the project lacks it. Nothing is
    /// sent or fetched.
    pub fn remote_set(
        &self,
        url: &str,
        name: &str,
        author: &Identity,
    ) -> Result<RemoteSet, VcsError> {
        check_name(name)?;
        let url = check_url(url)?;
        let branch = self.current_branch()?;
        match remote_url(&self.fresh()?, name) {
            Some(old) if old == url => {}
            Some(_) => {
                self.git_ok(
                    &format!("Changing the remote {name}"),
                    &["remote", "set-url", name, &url],
                    None,
                )?;
            }
            None => {
                self.git_ok(
                    &format!("Adding the remote {name}"),
                    &["remote", "add", name, &url],
                    None,
                )?;
            }
        }
        let repo = self.fresh()?;
        let remote_branch = match self.upstream_of(&repo, &branch) {
            Some(upstream) if upstream.configured && upstream.remote == name => upstream.branch,
            _ => {
                let what = format!("Setting the upstream of {branch}");
                self.git_ok(
                    &what,
                    &["config", &format!("branch.{branch}.remote"), name],
                    None,
                )?;
                self.git_ok(
                    &what,
                    &[
                        "config",
                        &format!("branch.{branch}.merge"),
                        &format!("refs/heads/{branch}"),
                    ],
                    None,
                )?;
                branch.clone()
            }
        };
        let (_, written) = mitcad_model::Project::init(&self.root)
            .map_err(|e| io_error("configure the project", &self.root, e))?;
        let commit = if written.is_empty() {
            None
        } else {
            let files: Vec<PathBuf> = written.iter().map(|path| self.root.join(path)).collect();
            self.commit(&files, CONFIGURE_MESSAGE, author)?.commit
        };
        Ok(RemoteSet {
            name: name.to_owned(),
            url: redact(&url),
            branch,
            upstream: format!("{name}/{remote_branch}"),
            commit,
            written,
        })
    }

    /// Removes the remote `name` (git also drops its remote-tracking
    /// references and the upstream of branches that follow it). False when
    /// there is none.
    pub fn remote_remove(&self, name: &str) -> Result<bool, VcsError> {
        check_name(name)?;
        if remote_url(&self.fresh()?, name).is_none() {
            return Ok(false);
        }
        self.git_ok(
            &format!("Removing the remote {name}"),
            &["remote", "remove", name],
            None,
        )?;
        let _ = fs::remove_file(self.root.join(REMOTE_STATE));
        Ok(true)
    }

    /// Connects the project to the remote at `url` as `name`: checks it
    /// ([`ProjectRepo::remote_check`]), refuses one that holds another
    /// history (nothing is changed), sets it ([`ProjectRepo::remote_set`]),
    /// then pushes to an empty remote, or fetches from one that shares the
    /// history and pushes when the project only has versions to add. With
    /// `push` false nothing is pushed. A failure after the remote was set is
    /// [`Connected::error`].
    pub fn connect(
        &self,
        url: &str,
        name: &str,
        author: &Identity,
        push: bool,
        control: Option<&Control>,
    ) -> Result<Connected, VcsError> {
        let check = self.remote_check(url, control)?;
        if !check.empty && check.related != Some(true) {
            let message = if check.has_project == Some(true) {
                format!(
                    "{} holds another project's history: open it with Open Project from \
                     Remote (mitcad-cli clone) into a new folder",
                    check.url
                )
            } else {
                format!(
                    "{} is not empty and does not share this project's history: connect the \
                     project to an empty repository",
                    check.url
                )
            };
            return Err(failure(ErrorClass::Unrelated, message));
        }
        let set = self.remote_set(url, name, author)?;
        let mut connected = Connected {
            check,
            set,
            fetch: None,
            push: None,
            ahead: None,
            behind: None,
            error: None,
        };
        let result = (|| -> Result<(), VcsError> {
            if !connected.check.empty {
                connected.fetch = Some(self.fetch(control)?);
            }
            let repo = self.fresh()?;
            let (_, upstream) = self.current_upstream(&repo)?;
            let (ahead, behind) = ahead_behind(&repo, &upstream.tracking())?;
            let only_ahead = ahead.is_some_and(|n| n > 0) && behind == Some(0);
            if push && (connected.check.empty || only_ahead) {
                connected.push = Some(self.push(control)?);
            }
            Ok(())
        })();
        match result {
            Ok(()) => {}
            Err(VcsError::Remote(error)) => connected.error = Some(error),
            Err(other) => return Err(other),
        }
        let (_, upstream) = self.current_upstream(&self.fresh()?)?;
        (connected.ahead, connected.behind) = ahead_behind(&self.fresh()?, &upstream.tracking())?;
        Ok(connected)
    }

    /// Fetches the remote the current branch follows (`git fetch --prune`):
    /// its remote-tracking references move to the remote's branches, and
    /// nothing else changes. The time and the outcome are noted for
    /// [`ProjectRepo::remote_info`].
    pub fn fetch(&self, control: Option<&Control>) -> Result<FetchOutcome, VcsError> {
        let repo = self.fresh()?;
        let (_, upstream) = self.current_upstream(&repo)?;
        let remote = upstream.remote.clone();
        let before = remote_refs(&repo, &remote)?;
        let result = self.git_ok(
            &format!("Fetching from {remote}"),
            &["fetch", "--prune", "--progress", &remote],
            control,
        );
        self.record(Attempted::Fetch, result.as_ref().err());
        result?;
        let repo = self.fresh()?;
        let after = remote_refs(&repo, &remote)?;
        let names: std::collections::BTreeSet<&String> =
            before.keys().chain(after.keys()).collect();
        let updated = names
            .into_iter()
            .filter(|name| before.get(*name) != after.get(*name))
            .map(|name| RefUpdate {
                name: name.clone(),
                old: before.get(name).map(ObjectId::to_string),
                new: after.get(name).map(ObjectId::to_string),
            })
            .collect();
        let (ahead, behind) = ahead_behind(&repo, &upstream.tracking())?;
        Ok(FetchOutcome {
            remote,
            updated,
            ahead,
            behind,
        })
    }

    /// Pushes the current branch's versions that the remote lacks to the
    /// branch it follows (`git push`, never forced; the upstream is set when
    /// it was not). Nothing to push (the remote-tracking reference has
    /// HEAD): `pushed` false, no network. A file of those versions larger
    /// than 100 MiB holds the push back (`too_large`: GitHub refuses it), one
    /// larger than 50 MiB is a warning. The remote refusing (it has versions
    /// the project lacks) is the error class `rejected`. The time and the
    /// outcome are noted for [`ProjectRepo::remote_info`].
    pub fn push(&self, control: Option<&Control>) -> Result<PushOutcome, VcsError> {
        self.push_within(control, (MAX_FILE_SIZE, LARGE_FILE_SIZE))
    }

    /// [`ProjectRepo::push`] with the size limits `(hold back, warn)`.
    pub(crate) fn push_within(
        &self,
        control: Option<&Control>,
        limits: (u64, u64),
    ) -> Result<PushOutcome, VcsError> {
        let repo = self.fresh()?;
        let (branch, upstream) = self.current_upstream(&repo)?;
        let head = head_of(&repo)?
            .ok_or_else(|| failure(ErrorClass::Unsupported, "the project has no versions yet"))?;
        let (ahead, _) = ahead_behind(&repo, &upstream.tracking())?;
        let mut outcome = PushOutcome {
            remote: upstream.remote.clone(),
            branch: upstream.branch.clone(),
            pushed: false,
            versions: ahead,
            head: head.to_string(),
            warnings: Vec::new(),
        };
        if ahead == Some(0) {
            return Ok(outcome);
        }
        let hidden = reference_id(&repo, &upstream.tracking())?;
        match self.large_files(&repo, head, hidden, limits) {
            Ok(warnings) => outcome.warnings = warnings,
            Err(error) => {
                self.record(Attempted::Push, Some(&error));
                return Err(error);
            }
        }
        let refspec = format!("refs/heads/{branch}:refs/heads/{}", upstream.branch);
        let mut args = vec!["push", "--porcelain", "--progress", "--follow-tags"];
        if !upstream.configured {
            args.push("--set-upstream");
        }
        args.push(&upstream.remote);
        args.push(&refspec);
        let result = self.git_ok(&format!("Pushing to {}", upstream.short()), &args, control);
        self.record(Attempted::Push, result.as_ref().err());
        result?;
        outcome.pushed = true;
        Ok(outcome)
    }

    /// The files of the versions from `head` back to `hidden` (all, when
    /// None) larger than `limits.1`, as warnings; one larger than `limits.0`
    /// is the error `too_large`.
    fn large_files(
        &self,
        repo: &gix::Repository,
        head: ObjectId,
        hidden: Option<ObjectId>,
        (limit, warn): (u64, u64),
    ) -> Result<Vec<String>, VcsError> {
        let walk = repo.rev_walk([head]).with_hidden(hidden);
        let mut seen = HashSet::new();
        let (mut too_large, mut large) = (Vec::new(), Vec::new());
        for info in walk.all()? {
            let info = info?;
            let tree = repo.find_commit(info.id)?.tree_id()?.detach();
            let parent = match info.parent_ids().next() {
                Some(parent) => Some(repo.find_commit(parent)?.tree_id()?.detach()),
                None => None,
            };
            for difference in self.differences(parent, Some(tree))? {
                let Some(id) = difference.new.filter(|id| seen.insert(*id)) else {
                    continue;
                };
                let size = repo.find_header(id)?.size();
                let text = format!("{} ({:.1} MiB)", difference.path, mebibytes(size));
                if size > limit {
                    too_large.push(text);
                } else if size > warn {
                    large.push(text);
                }
            }
        }
        if !too_large.is_empty() {
            return Err(failure(
                ErrorClass::TooLarge,
                format!(
                    "Not pushed: {} larger than {:.0} MiB, which git hosts refuse (GitHub allows \
                     100 MiB per file): {}. B-rep data that large needs Git LFS.",
                    if too_large.len() == 1 {
                        "a file is"
                    } else {
                        "files are"
                    },
                    mebibytes(limit),
                    too_large.join(", ")
                ),
            ));
        }
        Ok(large
            .into_iter()
            .map(|file| {
                format!(
                    "{file} is larger than {:.0} MiB: git hosts may warn about it, and Git LFS \
                     suits data that large",
                    mebibytes(warn)
                )
            })
            .collect())
    }

    /// `git gc --auto`: git packs loose objects and drops unreachable old
    /// ones when they add up (gix writes loose objects, and a remote's
    /// branch read by `remote_check` has no reference). A failure is only
    /// logged.
    pub(crate) fn gc_auto(&self) {
        let _ = self.git_ok("Tidying the repository", &["gc", "--auto", "--quiet"], None);
    }

    /// Brings the project up to the remote's newer versions when it has
    /// none of its own (`git merge --ff-only`): the branch moves to the
    /// remote-tracking reference and the folder's files follow; git refuses
    /// when a file it would change has changes no version holds. None when
    /// there is nothing newer. Versions on both sides need a sync.
    pub fn fast_forward(&self, control: Option<&Control>) -> Result<Option<FastForward>, VcsError> {
        if let Some(reason) = self.why_not_commit()? {
            return Err(failure(ErrorClass::Unsupported, reason));
        }
        let repo = self.fresh()?;
        let (_, upstream) = self.current_upstream(&repo)?;
        let tracking = upstream.tracking();
        let Some(target) = reference_id(&repo, &tracking)? else {
            return Ok(None);
        };
        let from = head_of(&repo)?;
        if from == Some(target) {
            return Ok(None);
        }
        if let Some(from) = from
            && count_only(&repo, from, target)? > 0
        {
            return Err(failure(
                ErrorClass::Unsupported,
                "both the project and the remote have versions the other lacks: sync to \
                 combine them",
            ));
        }
        let tree = |commit: ObjectId| -> Result<ObjectId, VcsError> {
            Ok(repo.find_commit(commit)?.tree_id()?.detach())
        };
        let old = from.map(tree).transpose()?;
        let changed_paths = self
            .differences(old, Some(tree(target)?))?
            .into_iter()
            .map(|difference| difference.path)
            .collect();
        self.git_ok(
            &format!("Updating the project to {}", upstream.short()),
            &["merge", "--ff-only", "--no-stat", &tracking],
            control,
        )?;
        Ok(Some(FastForward {
            from: from.map(|id| id.to_string()),
            to: target.to_string(),
            changed_paths,
        }))
    }
}

/// Removes what a failed clone left: the folder, or what it holds when it
/// was there before. On Windows git's pack files are read-only, and a git
/// process that is ending may still hold a file for a moment.
fn remove_clone(dir: &Path, existed: bool) {
    for attempt in 0..10 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            #[cfg(windows)]
            make_writable(dir);
        }
        let removed = if existed {
            fs::read_dir(dir)
                .map(|entries| {
                    entries.flatten().all(|entry| {
                        let path = entry.path();
                        if path.is_dir() {
                            fs::remove_dir_all(&path).is_ok()
                        } else {
                            fs::remove_file(&path).is_ok()
                        }
                    })
                })
                .unwrap_or(true)
        } else {
            fs::remove_dir_all(dir).is_ok() || !dir.exists()
        };
        if removed {
            return;
        }
    }
}

/// Clears the read-only flag of the files below `dir`.
#[cfg(windows)]
fn make_writable(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            make_writable(&path);
        } else if let Ok(metadata) = fs::metadata(&path) {
            let mut permissions = metadata.permissions();
            if permissions.readonly() {
                // Windows has no world-writable bit: this only clears the
                // read-only attribute.
                #[allow(clippy::permissions_set_readonly_false)]
                permissions.set_readonly(false);
                let _ = fs::set_permissions(&path, permissions);
            }
        }
    }
}

/// Opens a project from the remote at `url` into folder `dir` (made; it may
/// exist when empty) with `git clone`: the remote's default branch, the
/// remote named `origin` and the branch following it. A remote that is
/// empty or whose root holds no project (`.mitcad/project.json`) is refused,
/// and a failed or cancelled clone leaves no folder behind. `git`: the
/// program (else [`GitCli::find`]); the git commands go to `log`.
pub fn clone_project(
    url: &str,
    dir: &Path,
    git: Option<&GitCli>,
    control: Option<&Control>,
    log: &mut Vec<String>,
) -> Result<CloneOutcome, VcsError> {
    let url = check_url(url)?;
    let shown = redact(&url);
    let dir = absolute(dir)?;
    let target = dir
        .to_str()
        .ok_or_else(|| VcsError::InvalidPath(format!("{} is not valid Unicode", dir.display())))?;
    let existed = dir.exists();
    if existed {
        let empty = dir.is_dir()
            && fs::read_dir(&dir)
                .map_err(|e| io_error("read", &dir, e))?
                .next()
                .is_none();
        if !empty {
            return Err(VcsError::InvalidPath(format!(
                "{} is not an empty folder; a project is opened from a remote into a new one",
                dir.display()
            )));
        }
    }
    let parent = dir
        .parent()
        .ok_or_else(|| VcsError::InvalidPath(format!("{} has no parent folder", dir.display())))?;
    fs::create_dir_all(parent).map_err(|e| io_error("make the folder", parent, e))?;
    let git = git.cloned().map_or_else(GitCli::find, Ok)?;
    // The default branch first: a remote whose HEAD names no branch still
    // gives its project, and an empty one is told before a folder is made.
    let listed = run_logged(
        &git,
        parent,
        &["ls-remote", "--symref", &url],
        control,
        &format!("Reaching {shown}"),
        log,
    )?;
    let (branch, tip, _) = parse_ls_remote(&listed.stdout);
    let (Some(branch), Some(_)) = (branch, tip) else {
        return Err(failure(
            ErrorClass::NotAProject,
            format!("{shown} is empty: it holds no project to open"),
        ));
    };
    let cloned = run_logged(
        &git,
        parent,
        &[
            "clone",
            "--origin",
            DEFAULT_REMOTE,
            "--progress",
            "--branch",
            &branch,
            "--",
            &url,
            target,
        ],
        control,
        &format!("Cloning {shown}"),
        log,
    );
    if let Err(error) = cloned {
        remove_clone(&dir, existed);
        return Err(error);
    }
    if !dir.join(PROJECT_MARKER).is_file() {
        remove_clone(&dir, existed);
        return Err(failure(
            ErrorClass::NotAProject,
            format!("{shown} holds no Mitcad project ({PROJECT_MARKER} at its root)"),
        ));
    }
    let repo = ProjectRepo::open(&dir)?;
    let head = repo.head_commit()?;
    let files = match head {
        Some(head) => {
            let tree = repo.repo.find_commit(head)?.tree_id()?.detach();
            repo.project_files(tree)?
                .into_iter()
                .map(|(path, _)| path)
                .collect()
        }
        None => Vec::new(),
    };
    Ok(CloneOutcome {
        root: dir.to_string_lossy().into_owned(),
        url: shown,
        branch: repo.branch()?,
        head: head.map(|id| id.to_string()),
        files,
    })
}
