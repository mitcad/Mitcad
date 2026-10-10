// SPDX-License-Identifier: MIT
//! What a remote holds without a project (mitcad#89, `check_remote`): what
//! New Project and Open from Cloud show while the user types an address.
//! `git ls-remote` tells whether it is reachable and empty; a remote with
//! branches is cloned without file contents (`--filter=blob:none`, where
//! the server allows it) into a temporary bare repository, read with gix
//! and removed, so nothing is left behind.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use super::Context;
use crate::remote::{
    Control, RemoteCheck, check_url, parse_ls_remote, redact, remove_clone, run_logged,
};
use crate::{ProjectRepo, VcsError, io_error};

/// A new folder in the system's temporary folder, removed when dropped.
pub(crate) struct Temporary(PathBuf);

impl Temporary {
    pub(crate) fn new(what: &str) -> Result<Self, VcsError> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mitcad-{what}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        if dir.exists() {
            remove_clone(&dir, false);
        }
        std::fs::create_dir_all(&dir).map_err(|e| io_error("make the folder", &dir, e))?;
        Ok(Self(dir))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        remove_clone(&self.0, false);
    }
}

/// What the remote at `url` holds: reachable, empty, its default branch
/// and commit, a Mitcad project at its root, the names there, its latest
/// version's author and time, and its versions (counted up to 10 000).
/// The git commands go to `log`.
pub fn check_remote(
    url: &str,
    context: &Context,
    control: Option<&Control>,
    log: &mut Vec<String>,
) -> Result<RemoteCheck, VcsError> {
    let url = check_url(url)?;
    let shown = redact(&url);
    let git = context.git()?;
    let scratch = Temporary::new("check")?;
    let listed = run_logged(
        &git,
        scratch.path(),
        &["ls-remote", "--symref", &url],
        control,
        &format!("Reaching {shown}"),
        log,
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
        files: Vec::new(),
        latest: None,
        versions: None,
    };
    let Some(branch) = default_branch.filter(|_| tip.is_some()) else {
        return Ok(check);
    };
    let target = scratch.path().join("remote.git");
    let target_text = target.to_string_lossy().into_owned();
    run_logged(
        &git,
        scratch.path(),
        &[
            "clone",
            "--bare",
            "--filter=blob:none",
            "--no-tags",
            "--single-branch",
            "--progress",
            "--branch",
            &branch,
            "--",
            &url,
            &target_text,
        ],
        control,
        &format!("Reading {shown}"),
        log,
    )?;
    let repo = gix::open(&target)?;
    // The branch as cloned (it may have moved since the listing).
    let tip = repo
        .find_reference(format!("refs/heads/{branch}").as_str())
        .ok()
        .and_then(|reference| reference.try_id().map(|id| id.detach()))
        .or(tip)
        .ok_or_else(|| VcsError::Git(format!("{shown}: the branch {branch} was not cloned")))?;
    let reader = ProjectRepo::new(repo.clone(), target.clone());
    let summary = reader.summary(&repo, tip)?;
    check.head = Some(tip.to_string());
    check.has_project = Some(summary.has_project);
    check.files = summary.files;
    check.latest = summary.latest;
    check.versions = Some(summary.versions);
    Ok(check)
}
