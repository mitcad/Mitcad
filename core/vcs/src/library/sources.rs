// SPDX-License-Identifier: MIT
//! Which clones of the cache the pickers and the search read (mitcad#1436,
//! GitHub mitcad/Mitcad#172).
//!
//! - **One repository, one clone**: the URL forms that name one repository
//!   (`https://github.com/mitcad/parts.git`,
//!   `https://www.github.com/mitcad/parts`, `HTTPS://GitHub.com/mitcad/parts/`)
//!   have one [`repository_key`]. A fetch of another form of a repository
//!   already cloned updates that clone instead of making a second one, and
//!   of two clones an older MitCad made for one repository the pickers read
//!   one ([`LibraryCache::clones_read`]).
//! - **Removed sources**: removing a source in Library Sources marks its
//!   clone (`library_remove`); the pickers and the search leave a marked
//!   clone out, while designs keep reading their parts' versions from it
//!   ([`LibraryCache::find`] reads every clone). Adding the source again,
//!   or fetching it, takes the mark off.

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

use super::{LibraryCache, LibraryRepo, normal_url, version_order};
use crate::VcsError;

/// The file in a clone that says its source was removed.
pub const REMOVED_MARK: &str = "mitcad-source-removed";

/// A repository's identity (mitcad#1436): one key for the URL forms that
/// name it. Without user information, the scheme of a network address
/// (`https://`, `http://`, `ssh://`, `user@host:path`) or `file://`, a host's
/// leading `www.`, trailing slashes and `.git`; `/` for a backslash; in
/// lower case. A folder is its path.
pub fn repository_key(url: &str) -> String {
    let shown = mitcad_model::library::shown_url(url.trim()).replace('\\', "/");
    let lower = shown.to_ascii_lowercase();
    let named = if let Some(path) = lower.strip_prefix("file://") {
        // file:///C:/library is the folder C:/library.
        match path.strip_prefix('/') {
            Some(rest) if has_drive(rest) => rest.to_owned(),
            _ => path.to_owned(),
        }
    } else if let Some((scheme, rest)) = lower.split_once("://") {
        match scheme {
            "https" | "http" | "ssh" | "git+ssh" | "ssh+git" => host_and_path(rest),
            _ => lower.clone(),
        }
    } else if let Some((user_host, path)) = scp_address(&lower) {
        let host = user_host
            .rsplit_once('@')
            .map_or(user_host, |(_, host)| host);
        host_and_path(&format!("{host}/{path}"))
    } else {
        lower.clone()
    };
    let mut key = named.trim_end_matches('/').to_owned();
    if let Some(stripped) = key.strip_suffix(".git") {
        key = stripped.trim_end_matches('/').to_owned();
    }
    key
}

/// Whether a path starts with a drive (`c:/`, `c:`).
fn has_drive(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// `user@host:path` (git's short SSH form) as `(user@host, path)`; never a
/// drive (`c:/folder`) or an address with a scheme.
fn scp_address(url: &str) -> Option<(&str, &str)> {
    let (before, after) = url.split_once(':')?;
    if before.len() < 2 || before.contains('/') || after.starts_with("//") {
        return None;
    }
    Some((before, after.trim_start_matches('/')))
}

/// `host[:port]/path` with the host's leading `www.` left out.
fn host_and_path(rest: &str) -> String {
    let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let host = host.strip_prefix("www.").unwrap_or(host);
    format!("{host}{path}")
}

/// Whether the clone in `dir` is marked as the clone of a removed source.
pub fn is_removed(dir: &Path) -> bool {
    dir.join(REMOVED_MARK).is_file()
}

/// A clone the pickers and the search leave out, and why.
#[derive(Debug, Clone)]
pub struct CloneLeftOut {
    pub dir: PathBuf,
    pub url: String,
    /// `its source was removed`, or `same repository as <url>`.
    pub reason: String,
}

/// The clones the pickers and the search read ([`LibraryCache::clones_read`]).
#[derive(Debug)]
pub struct ClonesRead {
    /// One per repository, in the cache's order.
    pub read: Vec<LibraryRepo>,
    pub left_out: Vec<CloneLeftOut>,
}

impl LibraryCache {
    /// The clones the pickers and the search read (mitcad#1436): every
    /// clone not marked removed, one per repository ([`repository_key`]).
    /// Of two clones of one repository (an older MitCad cloned each URL
    /// form apart) the one whose URL is one of `sources` as written, else
    /// the one with the newer newest version, else the first.
    pub fn clones_read(&self, sources: &[String]) -> ClonesRead {
        let wanted: Vec<String> = sources.iter().map(String::as_str).map(normal_url).collect();
        let mut groups: Vec<(String, Vec<LibraryRepo>)> = Vec::new();
        let mut left_out = Vec::new();
        for dir in self.repos() {
            let Ok(repo) = LibraryRepo::open(&dir) else {
                continue;
            };
            let url = repo.url().unwrap_or_default();
            if is_removed(&dir) {
                left_out.push(CloneLeftOut {
                    dir,
                    url,
                    reason: "its source was removed".to_owned(),
                });
                continue;
            }
            let key = if url.is_empty() {
                dir.to_string_lossy().into_owned()
            } else {
                repository_key(&url)
            };
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, clones)) => clones.push(repo),
                None => groups.push((key, vec![repo])),
            }
        }
        let mut read = Vec::new();
        for (_, mut clones) in groups {
            let mut best = 0;
            for (at, clone) in clones.iter().enumerate().skip(1) {
                if read_first(clone, &clones[best], &wanted) {
                    best = at;
                }
            }
            let chosen = clones.remove(best);
            let chosen_url = chosen.url().unwrap_or_default();
            for other in clones {
                left_out.push(CloneLeftOut {
                    dir: other.dir().to_path_buf(),
                    url: other.url().unwrap_or_default(),
                    reason: format!("same repository as {chosen_url}"),
                });
            }
            read.push(chosen);
        }
        ClonesRead { read, left_out }
    }

    /// The clone in the cache's own folder of the repository `url` names
    /// in any form ([`repository_key`]), if one was fetched.
    pub fn clone_of_repository(&self, url: &str) -> Option<PathBuf> {
        let key = repository_key(url);
        self.repos()
            .into_iter()
            .filter(|dir| dir.starts_with(&self.root))
            .find(|dir| {
                LibraryRepo::open(dir)
                    .ok()
                    .and_then(|repo| repo.url())
                    .is_some_and(|other| repository_key(&other) == key)
            })
    }

    /// Marks the clones of the repository `url` names (in any form, in
    /// every folder of the cache) as a removed source's, unless one of
    /// `sources` (the sources still listed) names that repository; with
    /// `removed` false takes the mark off instead (the source added again).
    /// The folders whose mark changed.
    pub fn mark_removed(
        &self,
        url: &str,
        sources: &[String],
        removed: bool,
    ) -> Result<Vec<PathBuf>, VcsError> {
        let key = repository_key(url);
        if removed && sources.iter().any(|other| repository_key(other) == key) {
            return Ok(Vec::new());
        }
        let mut changed = Vec::new();
        for dir in self.repos() {
            let same = LibraryRepo::open(&dir)
                .ok()
                .and_then(|repo| repo.url())
                .is_some_and(|other| repository_key(&other) == key);
            if !same || is_removed(&dir) == removed {
                continue;
            }
            let mark = dir.join(REMOVED_MARK);
            let result = if removed {
                fs::write(&mark, format!("{}\n", crate::remote::redact(url.trim())))
            } else {
                fs::remove_file(&mark)
            };
            result.map_err(|e| crate::io_error("mark the clone", &mark, e))?;
            changed.push(dir);
        }
        Ok(changed)
    }
}

/// Whether clone `a` is read rather than clone `b` of the same repository:
/// the one whose URL a source has as written, then the newer newest
/// version.
fn read_first(a: &LibraryRepo, b: &LibraryRepo, wanted: &[String]) -> bool {
    let named = |repo: &LibraryRepo| {
        repo.url()
            .is_some_and(|url| wanted.contains(&normal_url(&url)))
    };
    match (named(a), named(b)) {
        (true, false) => return true,
        (false, true) => return false,
        _ => {}
    }
    let newest = |repo: &LibraryRepo| repo.newest().ok().flatten();
    match (newest(a), newest(b)) {
        (Some(x), Some(y)) => {
            version_order(&x.label, &y.label).then(x.time.cmp(&y.time)) == Ordering::Greater
        }
        (Some(_), None) => true,
        _ => false,
    }
}
